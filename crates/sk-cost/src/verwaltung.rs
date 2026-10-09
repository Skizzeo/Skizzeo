//! Verwaltung am Einzelplatz (KA-3a1, paket-ka3a §2): Protokoll des
//! Firmenkatalogs nach Ständen und „Diese Änderung zurücknehmen“.
//!
//! Ein Firmen-Rückgängig gibt es nicht (Entscheid VK-05): Eine Änderung
//! nimmt man zurück, indem man die alten Werte als neuen Stand schreibt.
//! [`umkehr`] liefert dafür die benannten Operationen; geschrieben wird
//! über `firma_anwenden` wie jede andere Firmenänderung.

use crate::befund::{self, Befund, Ort};
use crate::geld::Dez;
use crate::katalog::Leistung;
use crate::katalog::{Katalog, RATEN};
use crate::op::{Herkunft, HerkunftArt, Rolle};
use crate::op::{Op, SatzId, Stoff};
use crate::preis::Aufbau;
use sk_model::{Guid, Library, Model};

/// Eine Zeile des Protokolls (`[log]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eintrag {
    pub key: u32,
    /// Operationsname („preis_setzen“).
    pub op: String,
    /// Abschnitt und Kennung des Satzes.
    pub rec: String,
    pub of: String,
    /// Kurzform alt und neu (nur die geänderten Felder).
    pub alt: String,
    pub neu: String,
}

/// Ein Stand mit seinen Protokollzeilen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stand {
    pub stand: u32,
    /// `2026-10-08T07:00`.
    pub zeit: String,
    pub rolle: String,
    pub eintraege: Vec<Eintrag>,
}

/// Abschnitte, deren Änderung sich zurücknehmen lässt; Kopf, Herkunft und
/// Protokoll selbst nicht (die Herkunft schreibt die Rücknahme neu).
const UMKEHRBAR: [&str; 6] = ["article", "service", "svcpart", "svcfollow", "rate", "lot"];

/// Protokoll des Firmenkatalogs nach Ständen, der neueste zuerst.
pub fn protokoll(k: &Katalog) -> Vec<Stand> {
    let mut out: Vec<Stand> = Vec::new();
    let mut zeilen: Vec<_> = k.protokoll.iter().collect();
    zeilen.sort_by_key(|p| (std::cmp::Reverse(p.stand), p.key));
    for p in zeilen {
        let s = &p.satz;
        let text = |f: &str| s.text(f).unwrap_or_default().to_string();
        let e = Eintrag {
            key: p.key,
            op: text("op"),
            rec: text("rec"),
            of: text("of"),
            alt: text("old"),
            neu: text("new"),
        };
        match out.last_mut().filter(|x| x.stand == p.stand) {
            Some(x) => x.eintraege.push(e),
            None => out.push(Stand {
                stand: p.stand,
                zeit: text("time"),
                rolle: text("role"),
                eintraege: vec![e],
            }),
        }
    }
    out
}

/// Firmenwert `schluessel` im Katalog (mit dem Werkswert als Rückfall).
pub(crate) fn firmenwert(k: &Katalog, schluessel: &str) -> Option<Dez> {
    let w = &k.werte;
    match schluessel {
        "wage" => Some(w.lohn),
        "surcharge" => Some(w.zuschlag),
        "vat" => Some(w.mwst),
        s => s.strip_prefix("steel.").and_then(|art| {
            w.stahl
                .iter()
                .find(|(a, _)| a == art)
                .map(|(_, v)| *v)
                .or_else(|| RATEN.iter().find(|r| r.0 == s).map(|r| Dez::ganz(r.3)))
        }),
    }
}

/// Name eines Satzes für Menschen: Artikelname, Kurztext, Firmenwert.
pub fn satz_name(k: &Katalog, rec: &str, of: &str) -> String {
    let g = Guid::from_ifc(of);
    let name = match rec {
        "article" => g.and_then(|g| k.artikel(g)).map(|a| a.name.clone()),
        "service" => g.and_then(|g| k.leistung(g)).map(|l| l.kurz.clone()),
        "svcpart" => g
            .and_then(|g| k.anteile.iter().find(|a| a.guid == g))
            .and_then(|a| k.leistung(a.leistung))
            .map(|l| format!("Stoffanteil von {}", l.kurz)),
        "svcfollow" => g
            .and_then(|g| k.folgen.iter().find(|f| f.guid == g))
            .and_then(|f| k.leistung(f.leistung))
            .map(|l| format!("Folgeposition von {}", l.kurz)),
        "lot" => g.and_then(|g| k.los(g)).map(|l| l.name.clone()),
        "rate" => Some(crate::wort::firmenwert(of)),
        _ => None,
    };
    name.unwrap_or_else(|| format!("{} {of}", crate::wort::abschnitt(rec)))
}

fn abgelehnt(stand: u32, grund: String) -> Vec<Befund> {
    vec![Befund::fehler(
        93,
        befund::r93(&format!("Stand {stand} zurücknehmen"), &grund),
        Ort::Datei,
    )]
}

/// Ausmustern bzw. Wiederherstellen, wenn `retired` vorher anders war.
fn ruhestand(ops: &mut Vec<Op>, rec: &'static str, of: &str, vorher: bool, jetzt: bool) {
    if vorher != jetzt {
        let satz = SatzId::neu(rec, of);
        ops.push(if vorher {
            Op::Ausmustern { satz }
        } else {
            Op::Wiederherstellen { satz }
        });
    }
}

/// Runden für PBKDF2 ab Werk (rund 0,1 s einmal beim Prüfen) und erlaubter
/// Bereich beim Lesen (BIM §3.1 `pw`).
pub const RUNDEN: u32 = 200_000;
const RUNDEN_MIN: u32 = 100_000;
const RUNDEN_MAX: u32 = 10_000_000;

/// Prüfwert des Verwaltungskennworts für `[catalog] pw` (KA-3b1, Review
/// 3at): `pbkdf2-sha256$runden$salz-hex$hash-hex`, PBKDF2-HMAC-SHA256 mit
/// 16 Byte Zufallssalz je Kennwort. Leer: kein Kennwort (zurück zum
/// Einzelplatz). Das Kennwort selbst steht nirgends; `Debug` zeigt auch den
/// Prüfwert nicht.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Pruefwert(String);

impl std::fmt::Debug for Pruefwert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_empty() {
            "Pruefwert(leer)"
        } else {
            "Pruefwert(…)"
        })
    }
}

impl Pruefwert {
    /// Prüfwert von `kennwort` mit `salz`; leeres Kennwort: leer.
    pub fn neu(kennwort: &str, salz: [u8; 16]) -> Pruefwert {
        Pruefwert::mit_runden(kennwort, salz, RUNDEN)
    }

    pub fn mit_runden(kennwort: &str, salz: [u8; 16], runden: u32) -> Pruefwert {
        if kennwort.is_empty() {
            return Pruefwert::default();
        }
        let mut h = [0u8; 32];
        crate::sha256::pbkdf2(kennwort.as_bytes(), &salz, runden, &mut h);
        Pruefwert(format!(
            "pbkdf2-sha256${runden}${}${}",
            crate::sha256::hex(&salz),
            crate::sha256::hex(&h)
        ))
    }

    /// Ein Prüfwert in der Form aus BIM §3.1; sonst `None`.
    pub fn lesen(text: &str) -> Option<Pruefwert> {
        Pruefwert::teile(text)?;
        Some(Pruefwert(text.to_string()))
    }

    /// Runden, Salz (16 Byte) und Hash (32 Byte).
    fn teile(text: &str) -> Option<(u32, Vec<u8>, Vec<u8>)> {
        let mut t = text.split('$');
        if t.next()? != "pbkdf2-sha256" {
            return None;
        }
        let runden = t.next()?;
        if runden.is_empty() || !runden.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let runden: u32 = runden.parse().ok()?;
        let salz = crate::sha256::aus_hex(t.next()?)?;
        let hash = crate::sha256::aus_hex(t.next()?)?;
        let ok = (RUNDEN_MIN..=RUNDEN_MAX).contains(&runden)
            && salz.len() == 16
            && hash.len() == 32
            && t.next().is_none();
        ok.then_some((runden, salz, hash))
    }

    pub fn ist_leer(&self) -> bool {
        self.0.is_empty()
    }

    /// Text für `[catalog] pw`.
    pub fn text(&self) -> &str {
        &self.0
    }

    /// Stimmt `kennwort`? Vergleich in konstanter Zeit.
    fn stimmt(&self, kennwort: &str) -> bool {
        let Some((runden, salz, hash)) = Pruefwert::teile(&self.0) else {
            return false;
        };
        let mut h = [0u8; 32];
        crate::sha256::pbkdf2(kennwort.as_bytes(), &salz, runden, &mut h);
        crate::sha256::gleich(&h, &hash)
    }
}

/// `[catalog] pw` eines Firmenkatalogs: `Some(Ok)` lesbar, `Some(Err(()))`
/// in anderer Form (sperrt), `None` ohne Kennwort.
fn pw(lib: &sk_model::Library) -> Option<Result<Pruefwert, ()>> {
    let r = lib.ext("catalog").next()?;
    let z = crate::zeile::zerlegen(&r.line)?;
    let s = crate::satz::Satz::lesen(&crate::satz::CATALOG, &z).ok()?;
    match s.wert("pw")? {
        crate::satz::Wert::Text(t) => Some(Pruefwert::lesen(t).ok_or(())),
        _ => Some(Err(())),
    }
}

/// Hat der Firmenkatalog ein Verwaltungskennwort (mehrere Plätze)? Auch
/// eines in anderer Form zählt: Es sperrt die Verwaltung.
pub fn hat_kennwort(lib: &sk_model::Library) -> bool {
    pw(lib).is_some()
}

/// Ist das Kennwort lesbar (oder keins gesetzt)? Sonst Befund 72 mit
/// [`crate::befund::r72_pw`].
pub fn kennwort_lesbar(lib: &sk_model::Library) -> bool {
    !matches!(pw(lib), Some(Err(())))
}

/// Stimmt `kennwort` mit `[catalog] pw`? Ohne Kennwort stimmt jedes, mit
/// einem in anderer Form keins.
pub fn kennwort_stimmt(lib: &sk_model::Library, kennwort: &str) -> bool {
    match pw(lib) {
        None => true,
        Some(Ok(p)) => p.stimmt(kennwort),
        Some(Err(())) => false,
    }
}

/// „Diese Änderung zurücknehmen“ für Stand `stand` (paket-ka3a §2 KA-3a1):
/// die Operationen, die jeden in diesem Stand geänderten Satz auf seinen
/// Wert davor setzen. `jetzt` ist der Firmenkatalog heute, `vorher` der
/// Stand davor (aus dem Archiv; ein Stand ohne eigene Kostensätze liest
/// sich als Werksbestand). Neu angelegte Sätze werden ausgemustert, nie
/// gelöscht (Regel 87). Hat ein späterer Stand denselben Satz wieder
/// geändert, gibt es nur den Befund mit dem Satz.
pub fn umkehr(jetzt: &Katalog, vorher: &Katalog, stand: u32) -> Result<Vec<Op>, Vec<Befund>> {
    let st = protokoll(jetzt);
    let Some(dieser) = st.iter().find(|s| s.stand == stand) else {
        return Err(abgelehnt(stand, "diesen Stand gibt es nicht".into()));
    };
    // Ablage aus einer anderen Fassung (Datei zurückgespielt, die
    // Standnummern wiederholen sich): Sie ist nicht der Stand davor
    let firma = matches!(vorher.quelle, crate::katalog::Quelle::Firma { .. });
    if let Some(k) = vorher.kopf.as_ref().filter(|_| firma) {
        let fremd = k.stand + 1 != stand
            || jetzt.kopf.as_ref().is_some_and(|j| j.guid != k.guid)
            || vorher
                .protokoll
                .iter()
                .any(|p| !jetzt.protokoll.contains(p));
        if fremd {
            return Err(abgelehnt(
                stand,
                format!(
                    "der abgelegte Stand {} gehört zu einer anderen Fassung des Firmenkatalogs",
                    stand - 1
                ),
            ));
        }
    }
    let mut saetze: Vec<(&str, &str)> = Vec::new();
    for e in &dieser.eintraege {
        let x = (e.rec.as_str(), e.of.as_str());
        if UMKEHRBAR.contains(&x.0) && !saetze.contains(&x) {
            saetze.push(x);
        }
    }
    if saetze.is_empty() {
        return Err(abgelehnt(
            stand,
            "er enthält keine Änderung an Kostensätzen".into(),
        ));
    }
    // Später wieder geändert: nichts geschieht
    for s in st.iter().filter(|s| s.stand > stand) {
        if let Some(e) = s
            .eintraege
            .iter()
            .find(|e| saetze.contains(&(e.rec.as_str(), e.of.as_str())))
        {
            let name = satz_name(jetzt, &e.rec, &e.of);
            return Err(abgelehnt(
                stand,
                format!("{name} wurde in Stand {} wieder geändert", s.stand),
            ));
        }
    }
    let mut ops = Vec::new();
    for (rec, of) in saetze {
        let g = Guid::from_ifc(of);
        match rec {
            "rate" => {
                let (Some(a), Some(n)) = (firmenwert(vorher, of), firmenwert(jetzt, of)) else {
                    continue;
                };
                if a != n {
                    ops.push(Op::FirmenwertSetzen {
                        schluessel: of.to_string(),
                        wert: a,
                    });
                }
            }
            "article" => {
                let Some(n) = g.and_then(|g| jetzt.artikel(g)) else {
                    continue;
                };
                match g.and_then(|g| vorher.artikel(g)) {
                    None => ruhestand(&mut ops, "article", of, true, n.retired),
                    Some(a) => {
                        if a.preis != n.preis {
                            let text = |f: &str| a.satz.text(f).unwrap_or_default().to_string();
                            ops.push(Op::PreisSetzen {
                                artikel: n.guid,
                                preis: a.preis,
                                stand: text("date"),
                                quelle: text("source"),
                                eingabe: String::new(),
                            });
                        }
                        ruhestand(&mut ops, "article", of, a.retired, n.retired);
                    }
                }
            }
            "service" => {
                let Some(n) = g.and_then(|g| jetzt.leistung(g)) else {
                    continue;
                };
                match g.and_then(|g| vorher.leistung(g)) {
                    None => ruhestand(&mut ops, "service", of, true, n.retired),
                    Some(a) => {
                        let daten = crate::preis::bauleistung(a);
                        if daten != crate::preis::bauleistung(n) {
                            ops.push(Op::BauleistungAendern {
                                bauleistung: n.guid,
                                daten,
                            });
                        }
                        ruhestand(&mut ops, "service", of, a.retired, n.retired);
                    }
                }
            }
            "svcpart" => {
                let finde = |k: &Katalog| -> Option<crate::katalog::Anteil> {
                    g.and_then(|g| k.anteile.iter().find(|x| x.guid == g).cloned())
                };
                let (a, n) = (finde(vorher), finde(jetzt));
                let (a, n) = (a.as_ref(), n.as_ref());
                let Some((leistung, nr)) = a.or(n).map(|x| (x.leistung, x.nr)) else {
                    continue;
                };
                let stoff = |x: &crate::katalog::Anteil| match x.artikel {
                    Some(artikel) => Stoff::Artikel {
                        artikel,
                        menge: x.menge,
                    },
                    None => Stoff::Schicht { faktor: x.menge },
                };
                if a.map(stoff) != n.map(stoff) {
                    ops.push(Op::StoffanteilSetzen {
                        bauleistung: leistung,
                        nr,
                        anteil: a.map(stoff),
                    });
                }
            }
            "svcfollow" => {
                let finde = |k: &Katalog| -> Option<crate::katalog::Folge> {
                    g.and_then(|g| k.folgen.iter().find(|x| x.guid == g).cloned())
                };
                let (a, n) = (finde(vorher), finde(jetzt));
                let (a, n) = (a.as_ref(), n.as_ref());
                let Some((leistung, nr)) = a.or(n).map(|x| (x.leistung, x.nr)) else {
                    continue;
                };
                let folge = |x: &crate::katalog::Folge| (x.folge, x.faktor);
                if a.map(folge) != n.map(folge) {
                    ops.push(Op::FolgeSetzen {
                        bauleistung: leistung,
                        nr,
                        folge: a.map(folge),
                    });
                }
            }
            "lot" => {
                let Some(n) = g.and_then(|g| jetzt.los(g)) else {
                    continue;
                };
                let vorher_aus = g.and_then(|g| vorher.los(g)).is_none_or(|a| a.retired);
                ruhestand(&mut ops, "lot", of, vorher_aus, n.retired);
            }
            _ => {}
        }
    }
    if ops.is_empty() {
        return Err(abgelehnt(
            stand,
            "die Werte stehen schon wie vor diesem Stand".into(),
        ));
    }
    Ok(ops)
}

/// Werks-Referenzhaus „Standardhaus“ (paket-ka3a §2 KA-3a4: dieselbe Datei
/// wie Prüfstand und Test).
pub const STANDARDHAUS: &str = include_str!("../referenz/rh1-standardhaus.szo");

/// Höchstlänge eines Kurztexts in Zeichen (Regel 79 Nachtrag, GAEB).
pub const KURZ_MAX: usize = 70;

/// Stammabschnitte des Firmenkatalogs: Hat eine Datei keinen davon, rechnet
/// sie mit dem Werksbestand.
const STAMM: [&str; 6] = ["article", "service", "svcpart", "svcfollow", "rate", "lot"];

/// Die Kostenzeilen, die `lib` gelten lässt, je (Abschnitt, Kennung): die
/// eigenen, ohne eigene die des Werks. `[catalog]`, `[log]` und
/// `[proposal]` zählen nicht.
fn geltende_zeilen(lib: &Library) -> std::collections::BTreeMap<(String, String), String> {
    let eigen = STAMM.iter().any(|x| lib.ext(x).next().is_some());
    let mut z = std::collections::BTreeMap::new();
    let mut dazu = |sec: &str, line: &str| {
        if matches!(sec, "catalog" | "log" | "proposal") {
            return;
        }
        if let Some(id) = sk_model::ext::rec_id(line) {
            z.insert((sec.to_string(), id), line.to_string());
        }
    };
    if eigen {
        for r in lib.ext.recs() {
            dazu(&r.section, &r.line);
        }
    } else {
        for (a, l) in crate::werk_zeilen() {
            dazu(a, l);
        }
    }
    z
}

/// Die Stammsätze, in denen der Entwurf vom freigegebenen Stand abweicht
/// (Pille „Entwurf · n Änderungen“, Punkt im Baum, Vorschau; KA-3b2).
/// Herkunftszeilen zählen beim Satz, den sie beschreiben.
pub fn entwurf_saetze(freigegeben: &Library, entwurf: &Library) -> Vec<SatzId> {
    let (a, b) = (geltende_zeilen(freigegeben), geltende_zeilen(entwurf));
    let mut v: Vec<SatzId> = Vec::new();
    // Stoffanteile und Folgepositionen zählen bei ihrer Bauleistung
    let feld = |l: &String, name: &str| {
        crate::zeile::zerlegen(l).and_then(|z| {
            z.paare
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, w)| w.clone())
        })
    };
    let mut dazu = |sec: &str, id: &str| {
        let leistung = matches!(sec, "svcpart" | "svcfollow")
            .then(|| {
                let k = (sec.to_string(), id.to_string());
                b.get(&k).or(a.get(&k)).and_then(|l| feld(l, "service"))
            })
            .flatten();
        let (sec, id) = match &leistung {
            Some(g) => ("service", g.as_str()),
            None => (sec, id),
        };
        let Some(a) = crate::satz::abschnitt(sec) else {
            return;
        };
        let s = SatzId::neu(a.name, id);
        if !v.contains(&s) {
            v.push(s);
        }
    };
    for k in a.keys().chain(b.keys()) {
        if a.get(k) == b.get(k) {
            continue;
        }
        if k.0 == "origin" {
            // `key` der Herkunft ist die Kennung des beschriebenen Satzes
            let rec = b.get(k).or(a.get(k)).and_then(|l| feld(l, "rec"));
            if let Some(rec) = rec {
                dazu(&rec, &k.1);
            }
        } else {
            dazu(&k.0, &k.1);
        }
    }
    // Das Verwaltungskennwort zählt mit (es gilt erst mit der Freigabe)
    let (ka, kb) = (kopf(freigegeben), kopf(entwurf));
    let pw =
        |k: &Option<crate::satz::Satz>| k.as_ref().and_then(|k| k.text("pw").map(str::to_string));
    if pw(&ka) != pw(&kb) {
        if let Some(id) = kb.or(ka).and_then(|k| k.kennung()) {
            v.push(SatzId::neu("catalog", &id));
        }
    }
    v
}

/// Katalog aus den geltenden Kostenzeilen von `lib`, auch aus einem
/// Entwurf (Verweise gegen die Baustoffe von `lib`).
pub fn katalog_von(lib: &Library, stand: u32) -> Katalog {
    let eigen = STAMM.iter().any(|x| lib.ext(x).next().is_some());
    let mut z: Vec<(&str, &str)> = Vec::new();
    for sec in crate::satz::ABSCHNITTE_SZK {
        if eigen || sec == "catalog" || sec == "log" {
            z.extend(lib.ext(sec).map(|r| (sec, r.line.as_str())));
        }
    }
    if !eigen {
        z.extend(
            crate::werk_zeilen()
                .into_iter()
                .filter(|(a, _)| *a != "catalog" && *a != "log"),
        );
    }
    let name = kopf(lib)
        .and_then(|k| k.text("name").map(str::to_string))
        .unwrap_or_else(|| "Firmenkatalog".into());
    crate::katalog::lesen(
        z,
        &crate::katalog::Umfeld::aus_bibliothek(lib),
        crate::katalog::Quelle::Firma { name, stand },
    )
}

/// Der Entwurf `lib`, gelesen wie ein freigegebener Firmenkatalog: nur für
/// die Verwaltung, die ihn bearbeitet, und ihre Vorschau (KA-3b2). Sonst
/// wird ein Entwurf nirgends als Firmenkatalog gelesen (Regel 91).
pub fn wie_freigegeben(lib: &Library) -> Library {
    let mut l = lib.clone();
    // Vorschläge gibt es nur im Entwurf (Regel 105); hier stören sie nicht
    let vorschlaege: Vec<String> = l.ext("proposal").filter_map(|r| r.id.clone()).collect();
    for id in vorschlaege {
        l.ext_remove("proposal", &id);
    }
    if let Some(mut k) = kopf(lib).filter(|k| k.text("status") == Some("draft")) {
        k.setzen("status", Some(crate::satz::Wert::Wort("released".into())));
        if let Some(id) = k.kennung() {
            l.ext_put("catalog", &id, k.zeile(), None);
        }
    }
    l
}

/// Die offenen Vorschläge des Entwurfs `lib` (KA-3b4); in einer
/// freigegebenen Datei keine (Regel 105).
pub fn vorschlaege(lib: &Library) -> Vec<crate::katalog::Vorschlag> {
    if lib.ext("proposal").next().is_none() {
        return Vec::new();
    }
    katalog_von(lib, 0).vorschlaege
}

/// Ist `lib` ein Entwurf (`[catalog] status=draft`)?
pub fn ist_entwurf(lib: &Library) -> bool {
    kopf(lib).is_some_and(|k| k.text("status") == Some("draft"))
}

fn kopf(lib: &Library) -> Option<crate::satz::Satz> {
    let r = lib.ext("catalog").next()?;
    let z = crate::zeile::zerlegen(&r.line)?;
    crate::satz::Satz::lesen(&crate::satz::CATALOG, &z).ok()
}

/// „Freigeben als Stand n+1“ (KA-3b3, verwaltung.md §4/§5): `firma` ist die
/// freigegebene Datei unter Sperre, frisch gelesen, `entwurf` der Entwurf.
/// Übernommen werden nur die Kostenabschnitte (BIM §3.1–§3.8, §3.11) und
/// das Kennwort aus dem Kopf; Bauteiltypen, Stifte und Schraffuren bleiben
/// aus `firma` (S3). Der Kopf bekommt Stand + 1, Datum und
/// `status=released`. Beruht der Entwurf auf einem anderen Stand oder
/// bringt er neue Fehler (Regeln 71–92), gibt es nur die Befunde.
pub fn freigeben(
    firma: &str,
    entwurf: &str,
    herkunft: &Herkunft,
) -> Result<crate::op::FirmaNeu, Vec<Befund>> {
    let fehler = |g: String| vec![Befund::fehler(93, befund::r93("Freigeben", &g), Ort::Datei)];
    let lies = |t: &str| sk_model::read_szk_with(t, &crate::satz::ABSCHNITTE_SZK);
    let mut lib = lies(firma).map_err(|_| fehler("der Firmenkatalog ist nicht lesbar".into()))?;
    let e = lies(entwurf).map_err(|_| fehler("der Entwurf ist nicht lesbar".into()))?;
    let stand_f = kopf(&lib).and_then(|k| k.ganz("stand")).unwrap_or(0) as u32;
    let mut kopf_e = kopf(&e).ok_or_else(|| fehler("der Entwurf hat keinen Kopf".into()))?;
    let stand_e = kopf_e.ganz("stand").unwrap_or(0) as u32;
    if stand_e != stand_f {
        return Err(fehler(format!(
            "der Entwurf beruht auf Stand {stand_e}, freigegeben ist inzwischen Stand {stand_f}"
        )));
    }
    let k_f = katalog_von(&lib, stand_f);
    let k_e = katalog_von(&e, stand_e);
    // Offene neue Sätze aus Erweiterungen stehen schon als Zeilen im
    // Entwurf und gingen sonst unbestätigt mit (Regel 105, §8a)
    if k_e.vorschlaege.iter().any(|v| v.ganzer_satz()) {
        return Err(vec![neue_saetze_offen()]);
    }
    let neu: Vec<Befund> = k_e
        .befunde
        .iter()
        .filter(|b| b.schwere == befund::Schwere::Fehler && !k_f.befunde.contains(b))
        .cloned()
        .collect();
    if !neu.is_empty() {
        return Err(neu);
    }
    let saetze = entwurf_saetze(&lib, &e);
    let stand = stand_f + 1;
    // Protokollzeilen, die der Entwurf dazugeschrieben hat, bekommen den
    // neuen Stand (Regel 91)
    let bis = lib
        .ext("log")
        .filter_map(|r| r.id.as_deref().and_then(|k| k.parse::<u32>().ok()))
        .max()
        .unwrap_or(0);
    kopf_e.setzen("stand", Some(crate::satz::Wert::Ganz(i64::from(stand))));
    kopf_e.setzen(
        "date",
        Some(crate::satz::Wert::Text(herkunft.datum.clone())),
    );
    kopf_e.setzen("status", Some(crate::satz::Wert::Wort("released".into())));
    // Kostenabschnitte ganz aus dem Entwurf, Zeile für Zeile: auch doppelte
    // Kennungen (Regel 74, die erste gilt weiter), fremde und kaputte Zeilen
    lib.ext_declare(&crate::satz::ABSCHNITTE_SZK);
    for sec in crate::satz::ABSCHNITTE_SZK {
        // Vorschläge bleiben im Entwurf (Regel 105, [`rest_entwurf`])
        if sec == "proposal" {
            continue;
        }
        lib.ext_clear(sec);
        if sec == "catalog" {
            // der Kopf vorn, weitere Zeilen des Abschnitts wie im Entwurf
            lib.ext_append(sec, kopf_e.zeile());
            for r in e.ext(sec).skip(1) {
                lib.ext_append(sec, r.line.clone());
            }
            continue;
        }
        for r in e.ext(sec) {
            let neu = sec == "log"
                && r.id
                    .as_deref()
                    .and_then(|k| k.parse::<u32>().ok())
                    .is_some_and(|k| k > bis);
            let zeile = crate::zeile::zerlegen(&r.line)
                .filter(|_| neu)
                .and_then(|z| crate::satz::Satz::lesen(&crate::satz::LOG, &z).ok())
                .map_or_else(
                    || r.line.clone(),
                    |mut s| {
                        s.setzen("stand", Some(crate::satz::Wert::Ganz(i64::from(stand))));
                        s.zeile()
                    },
                );
            lib.ext_append(sec, zeile);
        }
    }
    Ok(crate::op::FirmaNeu {
        text: sk_model::write_szk(&lib),
        stand_vorher: stand_f,
        stand,
        saetze,
    })
}

/// Befund, der „Freigeben“ sperrt, solange ein `field=*`-Vorschlag offen
/// ist (Regel 105, verwaltung.md §5.4).
pub fn neue_saetze_offen() -> Befund {
    Befund::fehler(
        105,
        "Neue Sätze aus Erweiterungen sind noch nicht übernommen oder abgelehnt.",
        Ort::Datei,
    )
}

/// Was nach „Freigeben“ oder „Entwurf verwerfen“ vom Entwurf bleibt
/// (Regel 105): nur `[catalog]` mit `status=draft` (Kopf aus `firma`, der
/// nun freigegebenen Datei) und die offenen `[proposal]`-Zeilen; ohne
/// Vorschläge nichts. Neue Sätze aus Erweiterungen (`field=*`) gehen mit
/// dem Entwurf weg (§8a).
pub fn rest_entwurf(firma: &str, entwurf: &str) -> Option<String> {
    let lies = |t: &str| sk_model::read_szk_with(t, &crate::satz::ABSCHNITTE_SZK).ok();
    let e = lies(entwurf)?;
    let ganzer_satz = |l: &str| {
        crate::zeile::zerlegen(l)
            .is_some_and(|z| z.paare.iter().any(|(k, v)| k == "field" && v == "*"))
    };
    let vorschlaege: Vec<&str> = e
        .ext("proposal")
        .map(|r| r.line.as_str())
        .filter(|l| !ganzer_satz(l))
        .collect();
    if vorschlaege.is_empty() {
        return None;
    }
    let mut k = kopf(&lies(firma)?)?;
    k.setzen("status", Some(crate::satz::Wert::Wort("draft".into())));
    let kopfzeile = firma.lines().next().unwrap_or("SZK 1");
    let mut t = format!("{kopfzeile}\n{}\n", k.zeile());
    for v in vorschlaege {
        t.push_str(v);
        t.push('\n');
    }
    Some(t)
}

/// Der Entwurf zum Bearbeiten: Ein Rest-Entwurf ([`rest_entwurf`]) ohne
/// eigene Kosten- und Protokollzeilen heißt „wie freigegeben“; dann die
/// freigegebene Datei `firma` mit `status=draft` und seinen Vorschlägen.
/// Sonst `entwurf` selbst.
pub fn entwurf_voll(firma: &str, entwurf: &str) -> String {
    let lies = |t: &str| sk_model::read_szk_with(t, &crate::satz::ABSCHNITTE_SZK).ok();
    let Some(e) = lies(entwurf) else {
        return entwurf.to_string();
    };
    let rest = !STAMM
        .iter()
        .chain(["origin", "log"].iter())
        .any(|x| e.ext(x).next().is_some());
    if !rest {
        return entwurf.to_string();
    }
    let Some(mut lib) = lies(firma) else {
        return entwurf.to_string();
    };
    let Some(mut k) = kopf(&lib) else {
        return entwurf.to_string();
    };
    k.setzen("status", Some(crate::satz::Wert::Wort("draft".into())));
    lib.ext_declare(&crate::satz::ABSCHNITTE_SZK);
    lib.ext_put("catalog", &k.kennung().unwrap_or_default(), k.zeile(), None);
    for r in e.ext("proposal") {
        if let Some(id) = &r.id {
            lib.ext_put("proposal", id, r.line.clone(), None);
        }
    }
    sk_model::write_szk(&lib)
}

/// Wie `satz` im Entwurf vom freigegebenen Stand abweicht (Vorschau,
/// KA-3b3): je geändertem Feld (Schlüssel, alt, neu), Zahlen wie in der
/// Datei. Ein neuer Satz ist („+“, –, –), ein entfernter („-“, –, –);
/// geänderte Stoffanteile oder Folgepositionen einer Bauleistung stehen als
/// („svcpart“, –, –) bzw. („svcfollow“, –, –). Vom Kennwort steht nur, ob
/// es gesetzt ist, nie der Prüfwert.
pub fn entwurf_felder(
    freigegeben: &Library,
    entwurf: &Library,
    satz: &SatzId,
) -> Vec<(String, Option<String>, Option<String>)> {
    if satz.abschnitt == "catalog" {
        let pw = |l: &Library| kopf(l).is_some_and(|k| k.wert("pw").is_some());
        let wort =
            |b: bool| Some(if b { "gesetzt" } else { "" }.to_string()).filter(|w| !w.is_empty());
        return vec![("pw".into(), wort(pw(freigegeben)), wort(pw(entwurf)))];
    }
    let (a, b) = (geltende_zeilen(freigegeben), geltende_zeilen(entwurf));
    let k = (satz.abschnitt.to_string(), satz.kennung.clone());
    let paare = |l: Option<&String>| -> Vec<(String, String)> {
        l.and_then(|l| crate::zeile::zerlegen(l))
            .map_or(Vec::new(), |z| z.paare)
    };
    let mut out = match (a.get(&k), b.get(&k)) {
        (None, Some(_)) => vec![("+".to_string(), None, None)],
        (Some(_), None) => vec![("-".to_string(), None, None)],
        (la, lb) => {
            let (pa, pb) = (paare(la), paare(lb));
            let mut keys: Vec<&String> = pa.iter().map(|p| &p.0).collect();
            for (x, _) in &pb {
                if !keys.contains(&x) {
                    keys.push(x);
                }
            }
            keys.into_iter()
                .filter(|x| x.as_str() != "guid")
                .filter_map(|x| {
                    let va = pa.iter().find(|p| &p.0 == x).map(|p| p.1.clone());
                    let vb = pb.iter().find(|p| &p.0 == x).map(|p| p.1.clone());
                    (va != vb).then(|| (x.clone(), va, vb))
                })
                .collect()
        }
    };
    if satz.abschnitt == "service" {
        for sec in ["svcpart", "svcfollow"] {
            let teile = |m: &std::collections::BTreeMap<(String, String), String>| {
                m.iter()
                    .filter(|(k, l)| {
                        k.0 == sec
                            && paare(Some(l))
                                .iter()
                                .any(|(n, v)| n == "service" && *v == satz.kennung)
                    })
                    .map(|(k, l)| (k.1.clone(), l.clone()))
                    .collect::<Vec<_>>()
            };
            if teile(&a) != teile(&b) {
                out.push((sec.to_string(), None, None));
            }
        }
    }
    out
}

/// „Änderung verwerfen“ in der Vorschau (KA-3b3, paket-ka3b §3): Der Satz
/// `satz` im Entwurf `entwurf` bekommt wieder seine Zeilen aus dem
/// freigegebenen Stand `firma` (eine Bauleistung mit Stoffanteilen,
/// Folgepositionen und Herkunft); was es dort nicht gibt, fällt weg. Die
/// `[log]`-Zeilen, die der Entwurf dazu geschrieben hat, fallen mit weg.
pub fn entwurf_ohne(firma: &str, entwurf: &str, satz: &SatzId) -> Result<String, Vec<Befund>> {
    let fehler = |g: &str| {
        vec![Befund::fehler(
            93,
            befund::r93("Änderung verwerfen", g),
            Ort::Datei,
        )]
    };
    let lies = |t: &str| sk_model::read_szk_with(t, &crate::satz::ABSCHNITTE_SZK);
    let f = lies(firma).map_err(|_| fehler("der Firmenkatalog ist nicht lesbar"))?;
    let mut e = lies(entwurf).map_err(|_| fehler("der Entwurf ist nicht lesbar"))?;
    let alt = geltende_zeilen(&f);
    let jetzt = geltende_zeilen(&e);
    let feld = |l: &String, name: &str| {
        crate::zeile::zerlegen(l).and_then(|z| {
            z.paare
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, w)| w.clone())
        })
    };
    // Die Zeilen des Satzes, in beiden Ständen
    let mut zeilen: Vec<(String, String)> = Vec::new();
    if satz.abschnitt != "catalog" {
        zeilen.push((satz.abschnitt.to_string(), satz.kennung.clone()));
    }
    if satz.abschnitt == "service" {
        for (k, l) in alt.iter().chain(jetzt.iter()) {
            if matches!(k.0.as_str(), "svcpart" | "svcfollow")
                && feld(l, "service").as_deref() == Some(satz.kennung.as_str())
                && !zeilen.contains(k)
            {
                zeilen.push(k.clone());
            }
        }
    }
    let ids: Vec<String> = zeilen.iter().map(|k| k.1.clone()).collect();
    for k in &zeilen {
        let herkunft = ("origin".to_string(), k.1.clone());
        for k in [k, &herkunft] {
            match alt.get(k) {
                Some(l) => {
                    e.ext_declare(&crate::satz::ABSCHNITTE_SZK);
                    e.ext_put(&k.0, &k.1, l.clone(), None);
                }
                None => {
                    e.ext_remove(&k.0, &k.1);
                }
            }
        }
    }
    // Verwaltungskennwort
    let mut kopf_id = None;
    if satz.abschnitt == "catalog" {
        let mut k = kopf(&e).ok_or_else(|| fehler("der Entwurf hat keinen Kopf"))?;
        k.setzen("pw", kopf(&f).and_then(|k| k.wert("pw").cloned()));
        let id = k.kennung().unwrap_or_default();
        e.ext_put("catalog", &id, k.zeile(), None);
        kopf_id = Some(id);
    }
    // Protokollzeilen des Entwurfs zu diesem Satz
    let bis = f
        .ext("log")
        .filter_map(|r| r.id.as_deref().and_then(|k| k.parse::<u32>().ok()))
        .max()
        .unwrap_or(0);
    let weg: Vec<String> = e
        .ext("log")
        .filter(|r| {
            r.id.as_deref()
                .and_then(|k| k.parse::<u32>().ok())
                .is_some_and(|k| k > bis)
        })
        .filter(|r| {
            let of = feld(&r.line, "of");
            of.is_some_and(|of| ids.contains(&of) || kopf_id.as_ref() == Some(&of))
        })
        .filter_map(|r| r.id.clone())
        .collect();
    for id in weg {
        e.ext_remove("log", &id);
    }
    Ok(sk_model::write_szk(&e))
}

/// Der Firmenkatalog `text` mit den gesammelten Operationen der Verwaltung,
/// rein im Speicher (paket-ka3a §3): derselbe Weg wie beim OK
/// (`firma_anwenden`), nur wird nichts geschrieben. Ein Fehler sind die
/// Befunde, die OK sperren.
pub fn mit_ops(text: &str, ops: &[Op]) -> Result<Library, Vec<Befund>> {
    mit_ops_saetze(text, ops).map(|x| x.0)
}

/// Wie [`mit_ops`], dazu die geänderten Stammsätze (für die Vorschau des
/// offenen Hauses: `StandUebernehmen` beim OK, Regel 89).
pub fn mit_ops_saetze(text: &str, ops: &[Op]) -> Result<(Library, Vec<SatzId>), Vec<Befund>> {
    mit_ops_in(text, ops, false)
}

/// Wie [`mit_ops_saetze`]; mit `entwurf` derselbe Weg wie beim Schreiben in
/// den Entwurf (`entwurf_anwenden`, KA-3b2).
pub fn mit_ops_in(
    text: &str,
    ops: &[Op],
    entwurf: bool,
) -> Result<(Library, Vec<SatzId>), Vec<Befund>> {
    // Regel 79 Nachtrag: ein Kurztext über 70 Zeichen gilt beim Lesen,
    // sperrt aber in der Verwaltung
    let lang: Vec<Befund> = ops
        .iter()
        .filter_map(|op| match op {
            Op::BauleistungAnlegen(d) | Op::BauleistungAendern { daten: d, .. } => Some(d),
            _ => None,
        })
        .filter(|d| d.kurz.chars().count() > KURZ_MAX)
        .map(|d| {
            let n = d.kurz.chars().count();
            Befund::fehler(
                79,
                befund::r79(
                    &d.kurz,
                    "short",
                    &format!("{n} Zeichen, höchstens {KURZ_MAX}"),
                ),
                Ort::Datei,
            )
        })
        .collect();
    if !lang.is_empty() {
        return Err(lang);
    }
    // Mit dem Datum von heute, damit die Herkunft im Fenster stimmt
    let h = Herkunft::jetzt(HerkunftArt::Manual);
    let neu = if entwurf {
        crate::entwurf_anwenden(text, text, Rolle::Admin, &h, ops)?
    } else {
        crate::firma_anwenden(text, text, Rolle::Admin, &h, ops)?
    };
    let lib = sk_model::read_szk_with(&neu.text, &crate::lesen::ABSCHNITTE_SZK).map_err(|_| {
        vec![Befund::fehler(
            93,
            befund::r93("Vorschau", "der geänderte Firmenkatalog ist nicht lesbar"),
            Ort::Datei,
        )]
    })?;
    Ok((lib, neu.saetze))
}

/// Regel 97 als Sperre der Verwaltung (paket-ka3a §3, BIM 10:40): die
/// Werksschichten der Werkstypen und die typlosen Werksbauteile des Hauses
/// `haus` (Decke, Sohlplatte, Frostschürze, Randdämmstreifen, je Decke die
/// Untersichtdämmung mit Startdicke), die mit `k` nur geschätzt, mit
/// Richtpreis oder gar nicht gerechnet würden. Nur Stufe 1 oder 2 zählt.
/// Die Befunde sind Fehler; die Verwaltung sperrt mit denen, die beim
/// Öffnen noch nicht da waren.
pub fn luecken(k: &Katalog, haus: &Model, sched: &sk_model::qto::Schedule) -> Vec<Befund> {
    use crate::rechnung::{baustoff_name, mm_text};
    use crate::zuordnung::{dicke, zuordnen, Grund};
    use sk_model::element::{Category, ElementKind};
    use sk_model::library::{MatCategory, TypeCategory};
    use sk_model::MaterialLayer;
    let fehlt = |m: &Model, kat: Category, s: &MaterialLayer| {
        let mat = m.material(s.material);
        if mat.is_none_or(|x| x.category == MatCategory::Air) {
            return false;
        }
        matches!(
            zuordnen(k, kat, s, mat).grund,
            Grund::Geschaetzt { .. } | Grund::Richtpreis | Grund::Ohne
        )
    };
    let mut out = Vec::new();
    let werk = Model::new();
    for (_, t) in werk.layer_sets().iter() {
        if t.category == TypeCategory::RoofTerrace {
            continue;
        }
        let Some(kat) = Category::ALL
            .into_iter()
            .find(|c| TypeCategory::of(*c) == Some(t.category))
        else {
            continue;
        };
        for (i, s) in t.layers.iter().enumerate() {
            if fehlt(&werk, kat, s) {
                out.push(Befund::fehler(
                    97,
                    format!(
                        "Werksschicht {} {} in {} hat keine genaue Bauleistung.",
                        baustoff_name(&werk, s),
                        mm_text(dicke(s.thickness)),
                        t.name
                    ),
                    Ort::Schicht {
                        typ: t.guid,
                        schicht: i,
                    },
                ));
            }
        }
    }
    // Typlose Bauteile, je Bauteilart, Baustoff und Dicke einmal
    let mut gesehen: Vec<(Category, sk_model::MaterialId, i64)> = Vec::new();
    let mut pruefen = |kat: Category, s: &MaterialLayer, out: &mut Vec<Befund>| {
        let key = (kat, s.material, (s.thickness * 1000.0).round() as i64);
        if gesehen.contains(&key) {
            return;
        }
        gesehen.push(key);
        if fehlt(haus, kat, s) {
            out.push(Befund::fehler(
                97,
                format!(
                    "Werksbauteil {} ({} {}) hat keine genaue Bauleistung.",
                    match kat {
                        Category::Floor => "Decke",
                        k => sk_model::kinds::spec(k).name,
                    },
                    baustoff_name(haus, s),
                    mm_text(dicke(s.thickness))
                ),
                Ort::Datei,
            ));
        }
    };
    let mut decken = Vec::new();
    for (_, r) in sched.layer_rows(haus) {
        let Some(e) = haus.element(r.element) else {
            continue;
        };
        let typlos = [
            Category::Floor,
            Category::GroundSlab,
            Category::StripFooting,
            Category::EdgeInsulation,
            Category::SoffitInsulation,
        ];
        if e.layer_set.is_some() || !typlos.contains(&r.category) {
            continue;
        }
        if let ElementKind::Floor(_) = e.kind {
            if !decken.contains(&r.element) {
                decken.push(r.element);
            }
        }
        if let Some(s) = haus.element_layers(r.element).get(r.layer) {
            pruefen(r.category, s, &mut out);
        }
    }
    for d in decken {
        if let Some(mat) = haus.soffit_material_of(d) {
            let s = MaterialLayer::new(
                mat,
                sk_model::SOFFIT_THICKNESS,
                sk_model::LayerFunction::Insulation,
            );
            pruefen(Category::SoffitInsulation, &s, &mut out);
        }
    }
    out
}

/// Typische Dicke einer Bauleistung für die Verwaltung: Mitte des Bands der
/// Regel, sonst die kleinste Artikeldicke des Baustoffs, sonst 0.
fn typische_dicke(k: &Katalog, l: &Leistung) -> Dez {
    match (l.tmin, l.tmax) {
        (Some(a), Some(b)) => Dez((a.0 + b.0) / 2),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => l
            .mat
            .and_then(|g| {
                k.artikel
                    .iter()
                    .filter(|a| a.mat == Some(g) && !a.retired)
                    .filter_map(|a| a.t)
                    .min()
            })
            .unwrap_or(Dez::NULL),
    }
}

/// EP einer Bauleistung in der Verwaltung (paket-ka3a §1 „EP-Balken“): an
/// einer typischen Schicht aus dem Baustoff der Regel (Baustoff aus `m`),
/// sonst wie das Preisblatt.
pub fn aufbau(m: &Model, k: &Katalog, l: &Leistung) -> Aufbau {
    let mat = l.mat.and_then(|g| {
        m.materials()
            .iter()
            .find(|(_, x)| x.guid == g)
            .map(|(_, x)| x)
    });
    crate::preis::aufbau_an(k, l, Some((typische_dicke(k, l), mat)), false)
}

/// Wort einer Schichtfunktion der Regel.
fn funktion_wort(f: &str) -> &str {
    match f {
        "loadbearing" => "tragend",
        "insulation" => "Dämmung",
        "finish" => "Bekleidung",
        "membrane" => "Abdichtung",
        x => x,
    }
}

/// Name einer Bauteilart (`kinds.rs`-Wort): „Außenwand“.
pub fn kategorie_name(wort: &str) -> String {
    sk_model::element::Category::ALL
        .into_iter()
        .map(sk_model::kinds::spec)
        .find(|s| s.szo == wort)
        .map_or_else(|| wort.to_string(), |s| s.name.to_string())
}

/// „Passt auf“ einer Bauleistung (soll-ka-3): „Porenbeton 230–250 mm ·
/// Außenwand“. `baustoff` ist der Name des Baustoffs der Regel.
pub fn passt_auf(l: &Leistung, baustoff: Option<&str>) -> String {
    let mm = |d: Dez| d.text().replace('.', ",");
    let dicke = match (l.tmin, l.tmax) {
        (Some(a), Some(b)) if a == b => format!("{} mm", mm(a)),
        (Some(a), Some(b)) => format!("{}–{} mm", mm(a), mm(b)),
        (Some(a), None) => format!("ab {} mm", mm(a)),
        (None, Some(b)) => format!("bis {} mm", mm(b)),
        (None, None) => String::new(),
    };
    let stoff = [baustoff.unwrap_or_default(), dicke.as_str()]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let arten = l
        .kategorien
        .iter()
        .map(|w| kategorie_name(w))
        .collect::<Vec<_>>()
        .join(", ");
    if l.kategorien.is_empty() {
        return "nur als Folgeposition".into();
    }
    let teile: Vec<String> = [
        stoff,
        arten,
        l.funktion
            .as_deref()
            .map(funktion_wort)
            .unwrap_or_default()
            .to_string(),
    ]
    .into_iter()
    .filter(|x| !x.is_empty())
    .collect();
    teile.join(" · ")
}

/// Bauteiltypen der Firma, deren Schichten nach Regel 81 die Bauleistung
/// `g` bekommen (paket-ka3a §3 „Verwendet in“): Guid des Typs und Name,
/// mit „(Standard)“ am Standardtyp; nach Namen.
pub fn verwendet_in(lib: &Library, k: &Katalog, g: Guid) -> Vec<(Guid, String)> {
    use sk_model::element::Category;
    use sk_model::library::TypeCategory;
    let mut out = Vec::new();
    for (id, t) in lib.types.iter() {
        let Some(kat) = Category::ALL
            .into_iter()
            .find(|c| TypeCategory::of(*c) == Some(t.category))
        else {
            continue;
        };
        let nutzt = t.layers.iter().any(|l| {
            let mat = lib.materials.get(l.material);
            crate::zuordnung::zuordnen(k, kat, l, mat).leistung == Some(g)
        });
        if nutzt {
            let std = lib.default_exterior == Some(id) || lib.default_interior == Some(id);
            let name = if std {
                format!("{} (Standard)", t.name)
            } else {
                t.name.clone()
            };
            out.push((t.guid, name));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::{firma_anwenden, Herkunft, HerkunftArt, Rolle};
    use sk_model::{Library, Model};

    fn hand() -> Herkunft {
        Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "15:30")
    }

    fn katalog(text: &str) -> Katalog {
        let lib = sk_model::read_szk_with(text, &crate::satz::ABSCHNITTE_SZK).unwrap();
        crate::lesen::firma_oder_werk(&Model::new(), Some(&lib))
    }

    fn schreiben(text: &str, ops: &[Op]) -> String {
        firma_anwenden(text, text, Rolle::Admin, &hand(), ops)
            .unwrap()
            .text
    }

    /// KA-3b2/3b3 (paket-ka3b Abnahme 3 und 5): Mit Kennwort schreibt der
    /// Administrator in den Entwurf (`status=draft`, Stand bleibt, `[log]`
    /// mit dem kommenden Stand); der freigegebene Stand bleibt bytegleich.
    /// Freigeben macht Stand 2 aus den Kostenabschnitten des Entwurfs, ein
    /// inzwischen an einem anderen Platz gespeicherter Bauteiltyp bleibt (S3).
    #[test]
    fn entwurf_und_freigabe() {
        let t0 = sk_model::write_szk(&Library::standard());
        let lies = |t: &str| sk_model::read_szk_with(t, &crate::satz::ABSCHNITTE_SZK).unwrap();
        let pw = Op::KennwortSetzen {
            pw: Pruefwert::neu("Mauer", [3; 16]),
        };
        let t1 = schreiben(&t0, &[pw]);
        let k1 = katalog(&t1);
        let lohn = Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(62),
        };
        let a = k1.artikel.iter().find(|a| a.preis.is_some()).unwrap();
        let preis = Op::PreisSetzen {
            artikel: a.guid,
            preis: Some(Dez::ganz(31)),
            stand: "10/2026".into(),
            quelle: "Händler".into(),
            eingabe: String::new(),
        };
        let e1 = crate::entwurf_anwenden(&t1, &t1, Rolle::Admin, &hand(), &[lohn]).unwrap();
        assert_eq!((e1.stand_vorher, e1.stand), (1, 1));
        let e2 = crate::entwurf_anwenden(&e1.text, &e1.text, Rolle::Admin, &hand(), &[preis])
            .unwrap()
            .text;
        let kopf = e2.lines().find(|l| l.starts_with("[catalog]")).unwrap();
        assert!(
            kopf.contains("status=draft") && kopf.contains("stand=1"),
            "{kopf}"
        );
        assert!(ist_entwurf(&lies(&e2)) && !ist_entwurf(&lies(&t1)));
        let neu = |t: &str, st: &str| {
            t.lines()
                .filter(|l| l.starts_with("[log]") && l.contains(st))
                .count()
        };
        assert_eq!(neu(&e2, "stand=1"), neu(&t1, "stand=1") + 2);
        assert!(katalog_von(&lies(&e2), 1)
            .befunde
            .iter()
            .all(|b| b.regel == 91));
        let s = entwurf_saetze(&lies(&t1), &lies(&e2));
        assert_eq!(s.len(), 2, "{s:?}");
        assert!(s.contains(&SatzId::neu("rate", "wage")));
        assert!(s.contains(&SatzId::neu("article", a.guid.to_ifc())));
        // Ein anderer Platz speichert inzwischen einen Bauteiltyp
        let mut lib = lies(&t1);
        let id = lib.types.iter().next().unwrap().0;
        lib.types.get_mut(id).unwrap().name = "Typ von Platz B".into();
        let t1b = sk_model::write_szk(&lib);
        let f = freigeben(&t1b, &e2, &hand()).unwrap();
        assert_eq!((f.stand_vorher, f.stand), (1, 2));
        assert_eq!(f.saetze.len(), 2);
        let k2 = katalog(&f.text);
        assert_eq!(k2.werte.lohn, Dez::ganz(62));
        assert_eq!(k2.artikel(a.guid).unwrap().preis, Some(Dez::ganz(31)));
        let kopf = f.text.lines().find(|l| l.starts_with("[catalog]")).unwrap();
        assert!(
            kopf.contains("status=released") && kopf.contains("stand=2"),
            "{kopf}"
        );
        assert!(kopf.contains("pw=pbkdf2-sha256"), "Kennwort bleibt: {kopf}");
        assert!(f.text.contains("Typ von Platz B"), "S3: Bauteiltyp bleibt");
        assert!(kennwort_stimmt(&lies(&f.text), "Mauer"));
        assert_eq!(neu(&f.text, "stand=2"), 2);
        assert_eq!(protokoll(&k2).iter().filter(|st| st.stand == 2).count(), 1);
        // Inzwischen freigegeben: der Entwurf passt nicht mehr
        let e = freigeben(&f.text, &e2, &hand()).unwrap_err();
        assert!(e[0].satz.contains("beruht auf Stand 1"), "{}", e[0].satz);
        // Der Entwurf allein gilt nie als Firmenkatalog (Regel 91)
        let k = crate::lesen::firma_oder_werk(&Model::new(), Some(&lies(&e2)));
        assert!(k.befunde.iter().any(|b| b.regel == 91));
    }

    /// KA-3b4 (paket-ka3b Abnahme 5, 8, 8a; BIM §3.16, Regel 105): Ein
    /// Nutzer schlägt Preis und Lohn vor; die Vorschläge stehen nur im
    /// Entwurf und zählen nicht als Änderung. Derselbe Wert noch einmal
    /// ersetzt den Vorschlag. Übernehmen schreibt den Wert mit Herkunft
    /// „Vorschlag aus …“ und einer `[log]`-Zeile, Ablehnen streicht ihn mit
    /// einer `[log]`-Zeile. Nach „Freigeben“ bleiben offene Vorschläge als
    /// Rest-Entwurf, der wie der freigegebene Stand gilt.
    #[test]
    fn vorschlaege() {
        let t0 = sk_model::write_szk(&Library::standard());
        let lies = |t: &str| sk_model::read_szk_with(t, &crate::satz::ABSCHNITTE_SZK).unwrap();
        let t1 = schreiben(
            &t0,
            &[Op::KennwortSetzen {
                pw: Pruefwert::neu("Mauer", [3; 16]),
            }],
        );
        let k1 = katalog(&t1);
        let a = k1.artikel.iter().find(|a| a.preis.is_some()).unwrap();
        let projekt = sk_model::Guid(77);
        let vorschlag = |preis: i64, lohn: Option<i64>| {
            let mut ops = vec![Op::PreisSetzen {
                artikel: a.guid,
                preis: Some(Dez::ganz(preis)),
                stand: String::new(),
                quelle: String::new(),
                eingabe: String::new(),
            }];
            if let Some(l) = lohn {
                ops.push(Op::FirmenwertSetzen {
                    schluessel: "wage".into(),
                    wert: Dez::ganz(l),
                });
            }
            Op::VorschlagFuerFirma {
                projekt,
                name: "Haus A".into(),
                werte: crate::op::vorschlag_werte(&ops, &k1),
            }
        };
        // Nur im Entwurf, nicht in die freigegebene Datei
        for rolle in [Rolle::Nutzer, Rolle::Admin] {
            let e = firma_anwenden(&t1, &t1, rolle, &hand(), &[vorschlag(40, None)]);
            assert!(e.unwrap_err()[0].satz.contains("nur im Entwurf"));
        }
        let e1 =
            crate::entwurf_anwenden(&t1, &t1, Rolle::Nutzer, &hand(), &[vorschlag(40, Some(65))])
                .unwrap()
                .text;
        let e2 = crate::entwurf_anwenden(&e1, &e1, Rolle::Nutzer, &hand(), &[vorschlag(41, None)])
            .unwrap()
            .text;
        assert!(ist_entwurf(&lies(&e2)));
        assert!(entwurf_saetze(&lies(&t1), &lies(&e2)).is_empty());
        assert_eq!(
            e2.lines().filter(|l| l.starts_with("[log]")).count(),
            t1.lines().filter(|l| l.starts_with("[log]")).count(),
            "Vorschlagen schreibt kein Protokoll"
        );
        let kv = katalog_von(&lies(&e2), 1);
        assert_eq!(kv.vorschlaege.len(), 2, "{:?}", kv.vorschlaege);
        let p = kv.vorschlaege.iter().find(|v| v.rec == "article").unwrap();
        assert_eq!((p.neu.as_str(), p.feld.as_str()), ("41", "price"));
        assert_eq!(p.alt, a.preis.map(Dez::text));
        assert_eq!(p.name, "Haus A");
        let l = kv.vorschlaege.iter().find(|v| v.rec == "rate").unwrap();
        assert_eq!((l.of.as_str(), l.neu.as_str()), ("wage", "65"));
        // Nutzer übernimmt nicht
        let ueber = Op::VorschlagUebernehmen { key: p.key };
        let e = crate::entwurf_anwenden(
            &e2,
            &e2,
            Rolle::Nutzer,
            &hand(),
            std::slice::from_ref(&ueber),
        );
        assert!(e.is_err());
        let log = |t: &str| t.lines().filter(|l| l.starts_with("[log]")).count();
        let e3 = crate::entwurf_anwenden(&e2, &e2, Rolle::Admin, &hand(), &[ueber])
            .unwrap()
            .text;
        assert_eq!(log(&e3), log(&e2) + 1);
        assert!(e3.contains("op=vorschlag_uebernehmen"));
        let k3 = katalog_von(&lies(&e3), 1);
        assert_eq!(k3.artikel(a.guid).unwrap().preis, Some(Dez::ganz(41)));
        assert_eq!(k3.vorschlaege.len(), 1);
        let h = k3.herkunft_von("article", &a.guid.to_ifc()).unwrap();
        assert_eq!(h.satz.text("source"), Some("Vorschlag aus Haus A"));
        let e4 = crate::entwurf_anwenden(
            &e3,
            &e3,
            Rolle::Admin,
            &hand(),
            &[Op::VorschlagAblehnen { key: l.key }],
        )
        .unwrap()
        .text;
        assert_eq!(log(&e4), log(&e3) + 1);
        assert!(e4.contains("op=vorschlag_ablehnen"));
        assert!(katalog_von(&lies(&e4), 1).vorschlaege.is_empty());
        assert_eq!(katalog_von(&lies(&e4), 1).werte.lohn, k1.werte.lohn);
        // Stunden einer Bauleistung (Preisblatt)
        let l0 = k1.leistungen.iter().find(|l| !l.retired).unwrap();
        let mut daten = crate::preis::bauleistung(l0);
        daten.stunden = Dez(l0.stunden.0 + 1_000);
        let ops = [Op::BauleistungAendern {
            bauleistung: l0.guid,
            daten,
        }];
        let werte = crate::op::vorschlag_werte(&ops, &k1);
        assert_eq!(werte.len(), 1);
        assert_eq!((werte[0].rec, werte[0].feld), ("service", "hours"));
        let st = crate::entwurf_anwenden(
            &e4,
            &e4,
            Rolle::Nutzer,
            &hand(),
            &[Op::VorschlagFuerFirma {
                projekt,
                name: "Haus A".into(),
                werte,
            }],
        )
        .unwrap()
        .text;
        let key = katalog_von(&lies(&st), 1).vorschlaege[0].key;
        let st = crate::entwurf_anwenden(
            &st,
            &st,
            Rolle::Admin,
            &hand(),
            &[Op::VorschlagUebernehmen { key }],
        )
        .unwrap()
        .text;
        let ks = katalog_von(&lies(&st), 1);
        assert_eq!(
            ks.leistung(l0.guid).unwrap().stunden,
            Dez(l0.stunden.0 + 1_000)
        );
        assert_eq!(ks.leistung(l0.guid).unwrap().kurz, l0.kurz);
        // Freigeben: kein Vorschlag in der Firma, offene bleiben als Rest
        let e5 = crate::entwurf_anwenden(&e4, &e4, Rolle::Nutzer, &hand(), &[vorschlag(50, None)])
            .unwrap()
            .text;
        let f = freigeben(&t1, &e5, &hand()).unwrap();
        assert!(!f.text.contains("[proposal]"));
        assert_eq!(
            katalog(&f.text).artikel(a.guid).unwrap().preis,
            Some(Dez::ganz(41))
        );
        let rest = rest_entwurf(&f.text, &e5).expect("ein Vorschlag offen");
        assert_eq!(
            rest.lines().filter(|l| l.starts_with("[proposal]")).count(),
            1
        );
        assert_eq!(rest.lines().count(), 3, "{rest}");
        assert!(rest.contains("status=draft") && rest.contains("stand=2"));
        assert_eq!(rest_entwurf(&f.text, &e4), None);
        let voll = entwurf_voll(&f.text, &rest);
        assert!(entwurf_saetze(&lies(&f.text), &lies(&voll)).is_empty());
        assert_eq!(katalog_von(&lies(&voll), 2).vorschlaege.len(), 1);
        assert_eq!(entwurf_voll(&f.text, &e5), e5, "ein ganzer Entwurf bleibt");
        // Regel 105: im freigegebenen Stand übergangen, mit Befund
        let falsch = format!("{}{}", f.text, rest.lines().last().unwrap());
        let k = katalog(&falsch);
        assert!(k.vorschlaege.is_empty());
        assert!(k.befunde.iter().any(|b| b.regel == 105));
    }

    /// KA-3a Abnahme 15: zwei Stände, „Diese Änderung zurücknehmen“ am
    /// ersten setzt den alten Preis als dritten Stand; der Lohn aus Stand 2
    /// bleibt. Ändert ein späterer Stand denselben Satz, gibt es nur den
    /// Befund mit dem Satz.
    #[test]
    fn stand_zuruecknehmen() {
        let t0 = sk_model::write_szk(&Library::standard());
        let k0 = katalog(&t0);
        let a = k0.artikel.iter().find(|a| a.preis.is_some()).unwrap();
        let alt = a.preis.unwrap();
        let preis = |p: i64| Op::PreisSetzen {
            artikel: a.guid,
            preis: Some(Dez(alt.0 + p * 10_000)),
            stand: "10/2026".into(),
            quelle: "Händler".into(),
            eingabe: String::new(),
        };
        let t1 = schreiben(&t0, &[preis(100)]);
        let lohn = Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(65),
        };
        let t2 = schreiben(&t1, &[lohn]);
        let k2 = katalog(&t2);
        let p = protokoll(&k2);
        assert_eq!(p.iter().map(|s| s.stand).collect::<Vec<_>>(), [2, 1]);
        assert!(p[1].eintraege.iter().any(|e| e.op == "preis_setzen"));
        assert_eq!(p[0].eintraege[0].op, "firmenwert_setzen");
        assert_eq!(
            (
                p[0].eintraege[0].alt.as_str(),
                p[0].eintraege[0].neu.as_str()
            ),
            ("60", "65")
        );
        // Stand 2 zurück: Lohn wieder 60
        let ops = umkehr(&k2, &katalog(&t1), 2).unwrap();
        assert_eq!(
            ops,
            [Op::FirmenwertSetzen {
                schluessel: "wage".into(),
                wert: Dez::ganz(60)
            }]
        );
        // Stand 1 zurück (davor Werksbestand): alter Preis als Stand 3
        let ops = umkehr(&k2, &k0, 1).unwrap();
        assert_eq!(ops.len(), 1, "{ops:?}");
        assert!(matches!(&ops[0], Op::PreisSetzen { preis, .. } if *preis == Some(alt)));
        let t3 = schreiben(&t2, &ops);
        let k3 = katalog(&t3);
        assert_eq!(k3.artikel(a.guid).unwrap().preis, Some(alt));
        assert_eq!(k3.werte.lohn, Dez::ganz(65), "Lohn aus Stand 2 bleibt");
        assert_eq!(protokoll(&k3)[0].stand, 3);
        // Stand 3 hat denselben Preis wieder geändert: Stand 1 geht nicht mehr
        let e = umkehr(&k3, &k0, 1).unwrap_err();
        assert_eq!(e[0].regel, 93);
        assert!(
            e[0].satz.contains("in Stand 3 wieder geändert"),
            "{}",
            e[0].satz
        );
        assert!(e[0].satz.contains(&a.name), "{}", e[0].satz);
        // Unbekannter Stand
        assert!(umkehr(&k3, &k0, 9).is_err());
        // Die Ablage muss der Stand davor sein
        assert!(umkehr(&k3, &katalog(&t1), 3).is_err());
        // Zurückgespielt (Review 3as): Stand 1 wieder eingesetzt, danach
        // Stand 2 und 3 einer anderen Fassung. Die Ablage stand-0002 der
        // alten Fassung wird nicht als Stand davor gelesen.
        let lohn = |w| Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(w),
        };
        let t2b = schreiben(&t1, &[lohn(70)]);
        let t3b = schreiben(&t2b, &[lohn(72)]);
        let e = umkehr(&katalog(&t3b), &katalog(&t2), 3).unwrap_err();
        assert!(e[0].satz.contains("anderen Fassung"), "{}", e[0].satz);
        assert_eq!(
            umkehr(&katalog(&t3b), &katalog(&t2b), 3).unwrap(),
            [lohn(70)]
        );
    }

    /// KA-3b1: Kennwort setzen schreibt nur die Prüfsumme (64 Hex, Salz
    /// Katalog-Guid) und eine `[log]`-Zeile ohne Prüfsumme; leer entfernt
    /// es. Nur Admin, nur Firma.
    #[test]
    fn kennwort_setzen() {
        let t0 = sk_model::write_szk(&Library::standard());
        let lies = |t: &str| sk_model::read_szk_with(t, &crate::satz::ABSCHNITTE_SZK).unwrap();
        assert!(!hat_kennwort(&lies(&t0)));
        assert!(kennwort_stimmt(&lies(&t0), "egal"));
        let setzen = |k: &str, salz: u8| Op::KennwortSetzen {
            pw: Pruefwert::neu(k, [salz; 16]),
        };
        let t1 = schreiben(&t0, &[setzen("Mauer 7", 1)]);
        let lib = lies(&t1);
        assert!(hat_kennwort(&lib) && kennwort_lesbar(&lib));
        assert!(kennwort_stimmt(&lib, "Mauer 7"));
        assert!(!kennwort_stimmt(&lib, "mauer 7"));
        assert!(!kennwort_stimmt(&lib, ""));
        let kopf = t1.lines().find(|l| l.starts_with("[catalog]")).unwrap();
        let pw = kopf.split(' ').find_map(|w| w.strip_prefix("pw=")).unwrap();
        let teile: Vec<&str> = pw.split('$').collect();
        assert_eq!(teile[..2], ["pbkdf2-sha256", "200000"], "{pw}");
        assert_eq!((teile[2], teile[3].len()), ("01".repeat(16).as_str(), 64));
        // Python: hashlib.pbkdf2_hmac('sha256', b'Mauer 7', b'\x01'*16, 200000)
        assert_eq!(
            teile[3],
            "ceaf65f4bae3f720542f0e86cb9a7ca05ed1b2c11b283d21730656118c13aa3d"
        );
        assert!(!t1.contains("Mauer 7"), "das Kennwort steht nirgends");
        // Dasselbe Kennwort mit anderem Salz: anderer Prüfwert
        assert_ne!(Pruefwert::neu("Mauer 7", [2; 16]).text(), pw);
        assert_eq!(
            format!("{:?}", setzen("Mauer 7", 1)),
            "KennwortSetzen { pw: Pruefwert(…) }"
        );
        let log: Vec<&str> = t1.lines().filter(|l| l.starts_with("[log]")).collect();
        let l = log
            .iter()
            .find(|l| l.contains("op=kennwort_setzen"))
            .unwrap();
        assert!(
            l.contains(r#"new="gesetzt""#) && !l.contains(teile[3]),
            "{l}"
        );
        assert!(l.contains("rec=catalog"), "{l}");
        // Zurück zum Einzelplatz
        let t2 = schreiben(&t1, &[setzen("", 1)]);
        assert!(!hat_kennwort(&lies(&t2)));
        let l = t2
            .lines()
            .rfind(|l| l.contains("op=kennwort_setzen"))
            .unwrap();
        assert!(l.contains(r#"old="gesetzt""#) && !l.contains("new="), "{l}");
        // Andere Form: der Katalog gilt, pw bleibt bytegleich, gesperrt
        for alt in [
            "pw=5c1f0e2d",
            "pw=pbkdf2-sha256$99999$0101010101010101010101010101010101$00",
            &format!(
                "pw=pbkdf2-sha256$50000${}${}",
                "01".repeat(16),
                "00".repeat(32)
            ),
        ] {
            let t3 = t1.replace(&format!("pw={pw}"), alt);
            let lib = lies(&t3);
            assert!(hat_kennwort(&lib) && !kennwort_lesbar(&lib), "{alt}");
            assert!(!kennwort_stimmt(&lib, "Mauer 7"));
            let lohn = Op::FirmenwertSetzen {
                schluessel: "wage".into(),
                wert: Dez::ganz(66),
            };
            let t4 = schreiben(&t3, &[lohn]);
            assert!(t4.contains(alt), "bytegleich: {alt}");
        }
        // Nur Admin, nur Firma, nur lesbare Prüfwerte
        let e = firma_anwenden(&t0, &t0, Rolle::Nutzer, &hand(), &[setzen("x", 1)]).unwrap_err();
        assert_eq!(e[0].regel, 93);
        let lohn = Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(65),
        };
        let e = firma_anwenden(&t0, &t0, Rolle::Nutzer, &hand(), &[lohn]).unwrap_err();
        assert!(
            e[0].satz.ends_with("nur in der Verwaltung."),
            "{}",
            e[0].satz
        );
        let m = Model::new();
        assert!(crate::vorschau(&m, None, Rolle::Admin, &[setzen("x", 1)]).is_err());
    }

    /// Bauleistung geändert und Artikel neu angelegt: zurück heißt alte
    /// Felder bzw. ausgemustert, nie gelöscht (Regel 87).
    #[test]
    fn bauleistung_und_neuer_artikel() {
        let t0 = sk_model::write_szk(&Library::standard());
        let k0 = katalog(&t0);
        let l = k0.leistungen.iter().find(|l| !l.retired).unwrap();
        let mut d = crate::preis::bauleistung(l);
        d.stunden = Dez(d.stunden.0 + 50_000);
        let t1 = schreiben(
            &t0,
            &[
                Op::BauleistungAendern {
                    bauleistung: l.guid,
                    daten: d,
                },
                Op::ArtikelAnlegen {
                    baustoff: None,
                    name: "Probeartikel".into(),
                    dicke: None,
                    guete: String::new(),
                    format: String::new(),
                    einheit: crate::katalog::Einheit::M2,
                    preis: Some(Dez::ganz(3)),
                    stand: "10/2026".into(),
                    quelle: String::new(),
                    lieferant: String::new(),
                    standard: false,
                },
            ],
        );
        let k1 = katalog(&t1);
        let neu = k1
            .artikel
            .iter()
            .find(|a| a.name == "Probeartikel")
            .unwrap();
        let ops = umkehr(&k1, &k0, 1).unwrap();
        assert!(ops.contains(&Op::BauleistungAendern {
            bauleistung: l.guid,
            daten: crate::preis::bauleistung(l)
        }));
        assert!(ops.contains(&Op::Ausmustern {
            satz: SatzId::neu("article", neu.guid.to_ifc())
        }));
        let t2 = schreiben(&t1, &ops);
        let k2 = katalog(&t2);
        assert_eq!(k2.leistung(l.guid).unwrap().stunden, l.stunden);
        assert!(
            k2.artikel(neu.guid).unwrap().retired,
            "ausgemustert, nicht gelöscht"
        );
    }

    /// KA-3a2: EP-Balken an der typischen Schicht (Mitte 230–250 mm, also
    /// der Planstein d=24), Vorschau mit Aufwandswert 0,50 rein im Speicher,
    /// „Passt auf“ und „Verwendet in“.
    #[test]
    fn luecken_regel_97() {
        let leer = sk_model::write_szk(&Library::standard());
        let haus = sk_model::szo::read_with(
            STANDARDHAUS,
            sk_model::GuidGen::with_seed(1),
            &crate::lesen::ABSCHNITTE_SZO,
        )
        .unwrap()
        .model;
        let sched = sk_model::qto::schedule(&haus);
        let k = katalog(&leer);
        assert_eq!(
            luecken(&k, &haus, &sched),
            vec![],
            "Werksbestand ohne Lücke"
        );
        // 15b: Stahlbetondecke in den Papierkorb ohne Ersatz
        let mit = |kurz: &str| {
            let l = k
                .leistungen
                .iter()
                .find(|l| l.kurz.starts_with(kurz))
                .unwrap();
            let lib = mit_ops(
                &leer,
                &[Op::Ausmustern {
                    satz: SatzId::neu("service", l.guid.to_ifc()),
                }],
            )
            .unwrap();
            let k2 = crate::lesen::firma_oder_werk(&Model::new(), Some(&lib));
            luecken(&k2, &haus, &sched)
                .into_iter()
                .map(|b| b.satz)
                .collect::<Vec<_>>()
        };
        let b = mit("Stb-Decke Ortbeton");
        assert!(
            b.iter().any(
                |s| s == "Werksbauteil Decke (Stahlbeton 220 mm) hat keine genaue Bauleistung."
            ),
            "{b:#?}"
        );
        let b = mit("AW Porenbeton-Planstein PP2-0,35 d=24cm");
        assert!(b.iter().any(|s| s.starts_with("Werksschicht ")), "{b:#?}");
    }

    #[test]
    fn aufbau_vorschau_passt_auf() {
        let leer = sk_model::write_szk(&Library::standard());
        let m = Model::new();
        let k = katalog(&leer);
        let l = k
            .leistungen
            .iter()
            .find(|l| {
                l.kurz
                    .starts_with("AW Porenbeton-Planstein PP2-0,35 d=24cm")
            })
            .unwrap();
        let a = aufbau(&m, &k, l);
        assert!(a.stoffe.iter().any(|t| t.name.contains("d=24cm")), "{a:?}");
        assert_eq!(a.ep, a.lohn + a.stoff + a.geraet + a.sonst);
        assert_eq!(a.lohn, crate::Cent(3000), "0,5 h × 60 €/h");
        let mut d = crate::preis::bauleistung(l);
        d.stunden = Dez::lesen("0.45", 6).unwrap();
        let op = Op::BauleistungAendern {
            bauleistung: l.guid,
            daten: d,
        };
        let lib = mit_ops(&leer, std::slice::from_ref(&op)).unwrap();
        let k2 = crate::lesen::firma_oder_werk(&m, Some(&lib));
        let l2 = k2.leistung(l.guid).unwrap();
        assert_eq!(aufbau(&m, &k2, l2).lohn, crate::Cent(2700));
        assert_eq!(katalog(&leer), k, "nichts geschrieben");
        let pa = passt_auf(l, Some("Porenbeton"));
        assert_eq!(pa, "Porenbeton 230–250 mm · Außenwand · tragend");
        let ohne = k
            .leistungen
            .iter()
            .find(|l| l.kategorien.is_empty())
            .unwrap();
        assert_eq!(passt_auf(ohne, None), "nur als Folgeposition");
        // Ungültig: Kurztext über 70 Zeichen sperrt
        let mut d = crate::preis::bauleistung(l);
        d.kurz = "x".repeat(71);
        let b = mit_ops(
            &leer,
            &[Op::BauleistungAendern {
                bauleistung: l.guid,
                daten: d,
            }],
        )
        .unwrap_err();
        assert!(b[0].satz.contains("71 Zeichen, höchstens 70"), "{b:?}");
        let preis = mit_ops(
            &leer,
            &[Op::PreisSetzen {
                artikel: a.stoffe[0].artikel.unwrap(),
                preis: Some(Dez::ganz(-1)),
                stand: String::new(),
                quelle: String::new(),
                eingabe: String::new(),
            }],
        );
        assert!(preis.is_err(), "Preis −1");
        let _ = verwendet_in(&Library::standard(), &k, l.guid);
    }
}

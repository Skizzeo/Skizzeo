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
fn firmenwert(k: &Katalog, schluessel: &str) -> Option<Dez> {
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
    let neu = crate::firma_anwenden(text, text, Rolle::Admin, &h, ops)?;
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
            }],
        );
        assert!(preis.is_err(), "Preis −1");
        let _ = verwendet_in(&Library::standard(), &k, l.guid);
    }
}

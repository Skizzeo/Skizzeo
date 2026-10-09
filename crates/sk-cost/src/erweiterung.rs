//! Erweiterungsbauteile in den Stammdaten (E8b, Plan §2 Nr. 7): Die
//! `[artikel]` und `[leistung]` der Definitionen im Projekt werden beim
//! Lesen zu Katalogzeilen im Speicher, nie zu Zeilen der .szo. Sie laufen
//! durch denselben Leser wie jede Kostenzeile (Regeln 72–80, 86), mit einer
//! fest aus den keys abgeleiteten Kennung. Ein Katalogsatz mit derselben
//! Kennung geht vor; ein Erweiterungsartikel mit genau einem gleichnamigen
//! Katalogartikel derselben Einheit nimmt diesen (E8-6).

use crate::befund::{satz_ort, Befund};
use crate::katalog::{Katalog, Umfeld};
use crate::satz::{self, Abschnitt};
use crate::zeile::Zeile;
use sk_model::{Guid, Model};
use std::collections::{HashMap, HashSet};

/// Kennung eines Satzes aus einer Erweiterung: fest aus dem key der
/// Definition, dem Abschnitt (`artikel`, `leistung`, `baustoff`,
/// `stoff.<leistung>`) und dem key des Satzes.
pub fn kennung(def: &str, rec: &str, key: &str) -> Guid {
    let h = crate::sha256::sha256(format!("skizzeo.erweiterung\n{def}\n{rec}\n{key}").as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&h[..16]);
    Guid(u128::from_be_bytes(b))
}

/// Kennung der Bauleistung hinter `leistung=` einer `[menge]`: eine
/// Werks-Kennung wie sie steht, sonst die abgeleitete.
pub fn leistung_von(def: &str, leistung: &str) -> Guid {
    if sk_szb::pruefen::ist_kennung(leistung) {
        if let Some(g) = Guid::from_ifc(leistung) {
            return g;
        }
    }
    kennung(def, "leistung", leistung)
}

/// Hinweise beim Einlesen (Prüfung E8, Frage 2): Eine genutzte Bauleistung
/// hat Folgen, deren Bauleistung die Definition nicht nennt. Folgen gelten
/// für Erweiterungen nicht; ohne den Hinweis fehlten sie still.
pub fn fehlende_folgen(k: &Katalog, d: &sk_model::erweiterung::ExtDef) -> Vec<String> {
    let mut genutzt: Vec<Guid> = Vec::new();
    for l in d.def.menge.iter().filter_map(|r| r.get("leistung")) {
        let g = leistung_von(&d.key, l);
        if !l.is_empty() && !genutzt.contains(&g) {
            genutzt.push(g);
        }
    }
    let mut out = Vec::new();
    for g in &genutzt {
        let Some(l) = k.leistung(*g) else {
            continue;
        };
        for f in k.folgen_von(*g).filter(|f| !genutzt.contains(&f.folge)) {
            let folge = k
                .leistung(f.folge)
                .map_or_else(|| f.folge.to_ifc(), |x| x.kurz.clone());
            let t = format!(
                "{} nutzt „{}“; deren Folge „{folge}“ kommt in der Definition nicht vor",
                d.name(),
                l.kurz
            );
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

/// Ein Satz des wirksamen Katalogs, der aus einer Erweiterung stammt
/// (Anzeige „aus Erweiterung werk.stuetze v1 · grob“, B15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtSatz {
    /// `article` oder `service`.
    pub rec: &'static str,
    /// Kennung im Katalog: abgeleitet, oder die des Katalogartikels (E8-6).
    pub guid: Guid,
    pub def: String,
    pub version: u32,
    pub key: String,
    /// Name oder Kurztext in der Definition.
    pub name: String,
    /// `sicherheit=` der Definition („grob“), sonst leer.
    pub sicherheit: String,
    /// E8-6: Katalogartikel statt des Erweiterungsartikels.
    pub statt: bool,
}

impl ExtSatz {
    /// „aus Erweiterung werk.stuetze v1 · grob“.
    pub fn herkunft(&self) -> String {
        let mut s = format!("aus Erweiterung {} v{}", self.def, self.version);
        if !self.sicherheit.is_empty() {
            s.push_str(" · ");
            s.push_str(&self.sicherheit);
        }
        s
    }
}

/// `[artikel]` einer Definition, vorverdaut.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtArtikel {
    pub key: String,
    pub name: String,
    /// Einheit als Wort der .szk (`st` statt `stk`).
    pub einheit: String,
    pub preis: Option<String>,
    pub mat: Option<Guid>,
    pub stand: Option<String>,
    pub quelle: Option<String>,
    pub sicherheit: String,
}

/// `[leistung]` einer Definition, vorverdaut.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtLeistung {
    pub key: String,
    pub kurz: String,
    /// Gewerk: genau ein Gewerk des Projekts mit der ATV-Nummer (E8-5).
    pub gewerk: Option<Guid>,
    /// ATV-Nummer, wie sie in der Definition steht.
    pub gewerk_nr: String,
    pub einheit: String,
    pub bezug: String,
    pub stunden: Option<String>,
    pub geraet: Option<String>,
    pub sonst: Option<String>,
    pub kg: Option<String>,
    pub dmin: Option<String>,
    pub dmax: Option<String>,
    /// `stoffe=`: key eines `[artikel]` oder Werks-Kennung, Menge.
    pub stoffe: Vec<(String, String)>,
    pub sicherheit: String,
}

/// Eine Definition des Projekts mit dem, was der Leser braucht.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtQuelle {
    pub key: String,
    pub version: u32,
    pub artikel: Vec<ExtArtikel>,
    pub leistungen: Vec<ExtLeistung>,
}

/// Einheit der .szb als Wort der .szk.
pub fn einheit_wort(e: &str) -> &str {
    match e {
        "stk" => "st",
        e => e,
    }
}

/// Gewerk mit der ATV-Nummer `nr`, wenn es genau eines gibt (E8-5).
pub fn gewerk(m: &Model, nr: &str) -> Option<Guid> {
    let mut t = m.trades().iter().filter(|t| t.code == nr);
    match (t.next(), t.next()) {
        (Some(t), None) => Some(t.guid),
        _ => None,
    }
}

/// Baustoff des Projekts zum Schlüssel `key` der Definition `d`: ein
/// Werksbaustoff des Projekts, sonst ein eigener `[baustoff]` mit
/// abgeleiteter Kennung (E8-7).
pub fn baustoff(m: &Model, d: &sk_model::ExtDef, key: &str) -> Option<Guid> {
    if sk_szb::Bestand::werk().baustoff(key).is_some() {
        let id = m.ext_material(d, key)?;
        return m.material(id).map(|x| x.guid);
    }
    d.def
        .baustoff
        .iter()
        .any(|b| b.key() == key)
        .then(|| kennung(&d.key, "baustoff", key))
}

/// Eigene Baustoffe der Definitionen (E8-7): Kennung, Name, Art.
pub fn eigene_baustoffe(m: &Model) -> Vec<(Guid, String, Option<sk_model::library::MatCategory>)> {
    use sk_model::library::MatCategory;
    let best = sk_szb::Bestand::werk();
    let mut out = Vec::new();
    for d in m.ext_defs() {
        for b in &d.def.baustoff {
            if best.baustoff(b.key()).is_some() {
                continue;
            }
            let kat = match b.get("kategorie").unwrap_or("") {
                "masonry" => Some(MatCategory::Masonry),
                "concrete" => Some(MatCategory::Concrete),
                "insulation" => Some(MatCategory::Insulation),
                "plaster" => Some(MatCategory::Plaster),
                "timber" => Some(MatCategory::Timber),
                "metal" => Some(MatCategory::Metal),
                _ => None,
            };
            let name = b.get("name").unwrap_or(b.key()).to_string();
            out.push((kennung(&d.key, "baustoff", b.key()), name, kat));
        }
    }
    out
}

/// Die Definitionen des Projekts in der Reihenfolge des Einlesens.
pub fn quellen(m: &Model) -> Vec<ExtQuelle> {
    m.ext_defs()
        .iter()
        .map(|d| {
            let text =
                |s: &sk_szb::Satz, k: &str| s.get(k).filter(|v| !v.is_empty()).map(str::to_string);
            let sicher = |s: &sk_szb::Satz| s.get("sicherheit").unwrap_or("").to_string();
            let artikel = d
                .def
                .artikel
                .iter()
                .map(|a| ExtArtikel {
                    key: a.key().to_string(),
                    name: a.get("name").unwrap_or("").to_string(),
                    einheit: einheit_wort(a.get("einheit").unwrap_or("")).to_string(),
                    preis: text(a, "preis"),
                    mat: a.get("baustoff").and_then(|b| baustoff(m, d, b)),
                    stand: text(a, "stand"),
                    quelle: text(a, "quelle"),
                    sicherheit: sicher(a),
                })
                .collect();
            let leistungen = d
                .def
                .leistung
                .iter()
                .map(|l| {
                    let nr = l
                        .get("gewerk")
                        .filter(|v| !v.is_empty())
                        .or(d.def.bauteil_feld("gewerk"))
                        .unwrap_or("")
                        .to_string();
                    ExtLeistung {
                        key: l.key().to_string(),
                        kurz: l.get("kurztext").unwrap_or("").to_string(),
                        gewerk: gewerk(m, &nr),
                        gewerk_nr: nr,
                        einheit: einheit_wort(l.get("einheit").unwrap_or("")).to_string(),
                        bezug: l.get("bezug").unwrap_or("").to_string(),
                        stunden: text(l, "stunden"),
                        geraet: text(l, "geraet"),
                        sonst: text(l, "sonstiges"),
                        kg: text(l, "kg"),
                        dmin: text(l, "dmin"),
                        dmax: text(l, "dmax"),
                        stoffe: l
                            .get("stoffe")
                            .and_then(|s| sk_szb::pruefen::stoffe(s).ok())
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(a, q)| (a, zahl(q)))
                            .collect(),
                        sicherheit: sicher(l),
                    }
                })
                .collect();
            ExtQuelle {
                key: d.key.clone(),
                version: d.version,
                artikel,
                leistungen,
            }
        })
        .collect()
}

/// Zahl mit Punkt, höchstens 6 Stellen, ohne Nullen am Ende.
fn zahl(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// Was der Leser schon gelesen hat, für Titel, `pos` und den Abgleich.
pub(crate) struct Vorhanden<'a> {
    /// Kennungen der Artikel, Bauleistungen und Stoffanteile.
    pub guids: HashSet<Guid>,
    /// Gültige Artikel: Kennung, Name, Einheit.
    pub artikel: Vec<(Guid, &'a str, &'a str)>,
    /// Bauleistungen: Gewerk, Titel, `pos`.
    pub leistungen: Vec<(Guid, Guid, i64)>,
    /// Titel (Lose mit `parent`), gültig: Kennung, Los-Nr, Titel-Nr.
    pub titel: Vec<(Guid, String, String)>,
}

/// Erzeugte Zeilen: Abschnitt und Zeile; dazu, was woher stammt, und die
/// Bauleistungen ohne Titel (A8: stehen in keinem Los).
pub(crate) struct Erzeugt {
    pub zeilen: Vec<(&'static Abschnitt, Zeile)>,
    pub saetze: Vec<ExtSatz>,
    pub ohne_titel: HashSet<Guid>,
    pub befunde: Vec<Befund>,
}

/// Leerzeichen zusammengefasst, für den Namensabgleich (E8-6).
fn normal(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Platzhalter-Titel einer Bauleistung, deren Gewerk in keinem Titel
/// steht: keine OZ, in keinem LV (E8-2).
pub const OHNE_TITEL: Guid = Guid(0);

pub(crate) fn erzeugen(u: &Umfeld, v: &Vorhanden) -> Erzeugt {
    let mut out = Erzeugt {
        zeilen: Vec::new(),
        saetze: Vec::new(),
        ohne_titel: HashSet::new(),
        befunde: Vec::new(),
    };
    // Titel in Los- und Titelfolge
    let mut titel = v.titel.clone();
    titel.sort_by(|a, b| {
        (a.1.len(), &a.1, a.2.len(), &a.2).cmp(&(b.1.len(), &b.1, b.2.len(), &b.2))
    });
    let mut hoechste: HashMap<Guid, i64> = HashMap::new();
    for (_, t, pos) in &v.leistungen {
        let h = hoechste.entry(*t).or_insert(0);
        *h = (*h).max(*pos);
    }
    let mut erzeugt: HashSet<Guid> = HashSet::new();
    let zeile = |sec: &str, paare: Vec<(&str, String)>| Zeile {
        abschnitt: sec.to_string(),
        paare: paare.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    };
    for q in &u.erweiterungen {
        // Artikel: Katalog vor Erweiterung
        let mut artikel: HashMap<&str, Guid> = HashMap::new();
        for a in &q.artikel {
            let g = kennung(&q.key, "artikel", &a.key);
            let mut satz = ExtSatz {
                rec: "article",
                guid: g,
                def: q.key.clone(),
                version: q.version,
                key: a.key.clone(),
                name: a.name.clone(),
                sicherheit: a.sicherheit.clone(),
                statt: false,
            };
            if v.guids.contains(&g) {
                artikel.insert(&a.key, g);
                continue;
            }
            let n = normal(&a.name);
            let mut treffer = v
                .artikel
                .iter()
                .filter(|(_, name, e)| normal(name) == n && *e == a.einheit);
            if let (Some(t), None) = (treffer.next(), treffer.next()) {
                artikel.insert(&a.key, t.0);
                satz.guid = t.0;
                satz.statt = true;
                out.saetze.push(satz);
                continue;
            }
            artikel.insert(&a.key, g);
            if !erzeugt.insert(g) {
                continue;
            }
            let mut p = vec![
                ("guid", g.to_ifc()),
                ("name", a.name.clone()),
                ("unit", a.einheit.clone()),
            ];
            if let Some(m) = a.mat {
                p.push(("mat", m.to_ifc()));
            }
            if let Some(x) = &a.preis {
                p.push(("price", x.clone()));
            }
            if let Some(x) = &a.stand {
                p.push(("date", x.clone()));
            }
            if let Some(x) = &a.quelle {
                p.push(("source", x.clone()));
            }
            out.zeilen.push((&satz::ARTICLE, zeile("article", p)));
            out.saetze.push(satz);
        }
        // Bauleistungen mit Stoffanteilen
        for l in &q.leistungen {
            let g = kennung(&q.key, "leistung", &l.key);
            if v.guids.contains(&g) || !erzeugt.insert(g) {
                continue;
            }
            let Some(gewerk) = l.gewerk else {
                // ohne genau ein Gewerk gibt es die Bauleistung nicht
                // (Regel 73); ihre Mengen stehen „ohne Bauleistung“
                out.befunde.push(Befund::fehler(
                    73,
                    format!(
                        "Bauleistung {} aus Erweiterung {}: Das Gewerk {} gibt es im Projekt nicht oder mehrfach.",
                        l.kurz, q.key, l.gewerk_nr
                    ),
                    satz_ort("service", g.to_ifc()),
                ));
                continue;
            };
            // Titel: der erste mit einer Bauleistung desselben Gewerks
            let t = titel
                .iter()
                .map(|t| t.0)
                .find(|t| v.leistungen.iter().any(|x| x.0 == gewerk && x.1 == *t));
            let (titel_g, pos) = match t {
                Some(t) => {
                    let h = hoechste.entry(t).or_insert(0);
                    *h = (*h / 10 + 1) * 10;
                    (t, *h)
                }
                None => {
                    out.ohne_titel.insert(g);
                    let h = hoechste.entry(OHNE_TITEL).or_insert(0);
                    *h += 10;
                    (OHNE_TITEL, *h)
                }
            };
            let mut p = vec![
                ("guid", g.to_ifc()),
                ("short", l.kurz.clone()),
                ("trade", gewerk.to_ifc()),
                ("title", titel_g.to_ifc()),
                ("pos", pos.to_string()),
                ("unit", l.einheit.clone()),
                ("basis", l.bezug.clone()),
            ];
            for (k, x) in [
                ("hours", &l.stunden),
                ("equip", &l.geraet),
                ("other", &l.sonst),
                ("kg", &l.kg),
                ("tmin", &l.dmin),
                ("tmax", &l.dmax),
            ] {
                if let Some(x) = x {
                    p.push((k, x.clone()));
                }
            }
            out.zeilen.push((&satz::SERVICE, zeile("service", p)));
            out.saetze.push(ExtSatz {
                rec: "service",
                guid: g,
                def: q.key.clone(),
                version: q.version,
                key: l.key.clone(),
                name: l.kurz.clone(),
                sicherheit: l.sicherheit.clone(),
                statt: false,
            });
            for (nr, (a, menge)) in l.stoffe.iter().enumerate() {
                let art = match artikel.get(a.as_str()) {
                    Some(g) => *g,
                    None => match Guid::from_ifc(a) {
                        Some(g) if sk_szb::pruefen::ist_kennung(a) => g,
                        _ => kennung(&q.key, "artikel", a),
                    },
                };
                let p = vec![
                    (
                        "guid",
                        kennung(&q.key, &format!("stoff.{}", l.key), a).to_ifc(),
                    ),
                    ("service", g.to_ifc()),
                    ("nr", (nr + 1).to_string()),
                    ("art", art.to_ifc()),
                    ("qty", menge.clone()),
                ];
                out.zeilen.push((&satz::SVCPART, zeile("svcpart", p)));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kennung_fest_und_verschieden() {
        let a = kennung("werk.stuetze", "leistung", "stuetze_beton");
        assert_eq!(a, kennung("werk.stuetze", "leistung", "stuetze_beton"));
        assert_ne!(a, kennung("werk.treppe", "leistung", "stuetze_beton"));
        assert_ne!(a, kennung("werk.stuetze", "artikel", "stuetze_beton"));
        assert_eq!(Guid::from_ifc(&a.to_ifc()), Some(a));
        let w = leistung_von("werk.stuetze", "1S7bUW0010080200000006");
        assert_eq!(w.to_ifc(), "1S7bUW0010080200000006");
    }

    #[test]
    fn zahlen() {
        assert_eq!(zahl(1.03), "1.03");
        assert_eq!(zahl(22.0), "22");
        assert_eq!(zahl(0.0), "0");
    }
}

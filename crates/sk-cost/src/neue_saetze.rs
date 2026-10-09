//! Neue Sätze aus einer Erweiterung für den Firmenkatalog (E8c,
//! stammdaten/verwaltung.md §8a, BIM §3.16 „Zweiter Fall“).
//!
//! Die Sätze entstehen wie im Projekt (E8b), nur gegen den Firmenkatalog
//! gelesen: Titel und `pos` passen dann zur Firma. Jede Bauleistung ist ein
//! Satz mit ihren Stoffanteilen und den neuen Artikeln, auf die sie zeigt;
//! ein Artikel, auf den keine neue Bauleistung zeigt, ist ein Satz für sich.

use crate::erweiterung::{kennung, OHNE_TITEL};
use crate::katalog::{self, Katalog, Quelle, Umfeld};
use crate::satz::Satz;
use sk_model::{ExtDef, Guid, Library, Model};

/// Wie der Satz zum Firmenkatalog steht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stand {
    /// Neu; kann vorgeschlagen werden.
    Neu,
    /// Die Firma hat einen Satz mit dieser Kennung: Firmenpreis gilt.
    Vorhanden,
    /// Katalogartikel gleichen Namens und gleicher Einheit gilt (E8-6).
    Statt { name: String },
    /// Kein Titel mit einer Bauleistung desselben Gewerks (A8).
    OhneTitel,
    /// Das Gewerk gibt es im Firmenkatalog nicht.
    OhneGewerk,
}

impl Stand {
    /// Text in der Liste der Rückfrage.
    pub fn text(&self) -> String {
        match self {
            Stand::Neu => "neu".into(),
            Stand::Vorhanden => "vorhanden, Firmenpreis gilt".into(),
            Stand::Statt { name } => format!("vorhanden als {name}, Firmenpreis gilt"),
            Stand::OhneTitel => "neu, braucht einen Titel im LV".into(),
            Stand::OhneGewerk => "Gewerk fehlt im Firmenkatalog".into(),
        }
    }
}

/// Ein Satz der Definition mit den Zeilen, die ihn im Firmenkatalog anlegen.
#[derive(Clone, Debug, PartialEq)]
pub struct NeuerSatz {
    /// `service` oder `article`.
    pub rec: &'static str,
    pub guid: Guid,
    /// key in der Definition.
    pub key: String,
    /// Kurztext oder Name.
    pub name: String,
    pub stand: Stand,
    /// Abschnitt und Zeile; leer, wenn nicht [`Stand::Neu`].
    pub zeilen: Vec<(&'static str, String)>,
}

/// `source` der `[origin]`-Zeilen: „Erweiterung werk.stuetze v1“.
pub fn quelle_text(d: &ExtDef) -> String {
    format!("Erweiterung {} v{}", d.key, d.version)
}

/// Die Sätze der Definition `d` gegen den Firmenkatalog `lib` (ohne
/// Kostensätze: gegen den Werksbestand). `m` gibt Gewerke und Baustoffe.
pub fn neue_saetze(m: &Model, lib: &Library, d: &ExtDef) -> Vec<NeuerSatz> {
    let mut u = Umfeld::aus_modell(m);
    u.erweiterungen = vec![crate::erweiterung::quelle(m, d)];
    let k = firmenkatalog(lib, &u);
    let ziel = Umfeld::aus_bibliothek(lib);
    let mut out: Vec<NeuerSatz> = Vec::new();
    let mut verwiesen: Vec<Guid> = Vec::new();
    for l in &d.def.leistung {
        let g = kennung(&d.key, "leistung", l.key());
        let name = l.get("kurztext").unwrap_or("").to_string();
        let mut satz = NeuerSatz {
            rec: "service",
            guid: g,
            key: l.key().to_string(),
            name,
            stand: Stand::Neu,
            zeilen: Vec::new(),
        };
        let aus_ext = k.aus_erweiterung("service", g).is_some();
        match k.leistung(g) {
            Some(_) if !aus_ext => satz.stand = Stand::Vorhanden,
            None => satz.stand = Stand::OhneGewerk,
            Some(x) if x.titel == OHNE_TITEL => satz.stand = Stand::OhneTitel,
            Some(x) if !ziel.gewerke.contains(&x.gewerk) => satz.stand = Stand::OhneGewerk,
            Some(x) => {
                satz.zeilen.push(("service", x.satz.zeile()));
                for a in k.anteile_von(g) {
                    satz.zeilen.push(("svcpart", a.satz.zeile()));
                    let Some(art) = a.artikel else { continue };
                    let neu = k
                        .aus_erweiterung("article", art)
                        .is_some_and(|e| !e.statt && e.def == d.key);
                    if neu && !verwiesen.contains(&art) {
                        verwiesen.push(art);
                        if let Some(x) = k.artikel(art) {
                            satz.zeilen.push(("article", artikel_zeile(&x.satz, &ziel)));
                        }
                    }
                }
            }
        }
        out.push(satz);
    }
    // Artikel: vorhanden, statt eines Katalogartikels, oder allein neu
    for a in &d.def.artikel {
        let g = kennung(&d.key, "artikel", a.key());
        let name = a.get("name").unwrap_or("").to_string();
        let e = k
            .erweiterung
            .iter()
            .find(|e| e.rec == "article" && e.def == d.key && e.key == a.key());
        let stand = match e {
            None if k.artikel(g).is_some() => Stand::Vorhanden,
            None => continue,
            Some(e) if e.statt => Stand::Statt {
                name: k.artikel(e.guid).map_or(String::new(), |x| x.name.clone()),
            },
            Some(_) if verwiesen.contains(&g) => continue,
            Some(_) => Stand::Neu,
        };
        let zeilen = match (&stand, k.artikel(g)) {
            (Stand::Neu, Some(x)) => vec![("article", artikel_zeile(&x.satz, &ziel))],
            _ => Vec::new(),
        };
        out.push(NeuerSatz {
            rec: "article",
            guid: g,
            key: a.key().to_string(),
            name,
            stand,
            zeilen,
        });
    }
    out
}

/// Firmenkatalog mit den Sätzen der Definition im Speicher.
fn firmenkatalog(lib: &Library, u: &Umfeld) -> Katalog {
    let stamm = ["article", "service", "svcpart", "svcfollow", "rate", "lot"];
    let quelle = Quelle::Firma {
        name: String::new(),
        stand: 0,
    };
    if stamm.iter().any(|s| lib.ext(s).next().is_some()) {
        let z = crate::satz::ABSCHNITTE_SZK
            .iter()
            .flat_map(|s| lib.ext(s).map(move |r| (*s, r.line.as_str())));
        katalog::lesen(z, u, quelle)
    } else {
        katalog::lesen(crate::werk_zeilen(), u, quelle)
    }
}

/// Zeile eines Artikels; einen Baustoff, den die Firma nicht kennt (eigener
/// Baustoff der Definition, E8-7), lässt sie weg.
fn artikel_zeile(s: &Satz, ziel: &Umfeld) -> String {
    let mut s = s.clone();
    if s.guid("mat")
        .is_some_and(|g| !ziel.materialien.contains_key(&g))
    {
        s.setzen("mat", None);
    }
    s.zeile()
}

/// Gewerke der Bauleistungen von `d`, die im wirksamen Katalog des
/// Projekts (Firma `firma`) keinen Titel haben, je ATV-Nummer und Satz für
/// die Rückfrage beim Einlesen (§2 Nr. 7). Ein Titel entsteht dabei nicht.
pub fn gewerke_ohne_titel(m: &Model, firma: Option<&Library>, d: &ExtDef) -> Vec<(String, String)> {
    let mut mit = m.clone();
    if mit.put_ext_def(d.clone()).is_err() {
        return Vec::new();
    }
    let k = crate::lesen::katalog(&mit, firma);
    let mut out: Vec<(String, String)> = Vec::new();
    for l in &d.def.leistung {
        let nr = crate::erweiterung::gewerk_nr(d, l);
        if nr.is_empty() || out.iter().any(|(n, _)| n == nr) {
            continue;
        }
        let satz = match k.leistung(kennung(&d.key, "leistung", l.key())) {
            Some(x) if x.titel != OHNE_TITEL => continue,
            Some(_) => {
                let name = m.trades().iter().find(|t| t.code == nr).map(|t| &t.name);
                format!(
                    "{} {nr} hat noch keinen Titel. Positionen stehen bis dahin in keinem Los.",
                    name.map_or("Gewerk", |n| n.as_str())
                )
            }
            None => {
                format!("Gewerk {nr} gibt es im Projekt nicht. Die Mengen stehen ohne Bauleistung.")
            }
        };
        out.push((nr.to_string(), satz));
    }
    out
}

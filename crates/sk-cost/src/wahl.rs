//! „Bauleistung wählen …“ an einer grauen Zeile (paket-ka2 §4, Regel 80,
//! Bedienbarkeit 5.1): die Bauleistungen, deren Mengenbezug das Bauteil hat,
//! geordnet nach eigenem Gewerk, gleicher Stoffart und dem Rest, je mit EP
//! an dieser Schicht. Rein; geschrieben wird über `BauleistungZuordnen`.

use crate::befund::Ort;
use crate::geld::{Cent, Dez};
use crate::katalog::{Einheit, Katalog, Leistung};
use crate::op::Op;
use crate::rechnung::{feste_ep, stoff_ep, OhneZeile};
use sk_model::library::{MatCategory, Material};
use sk_model::trade::TradeId;
use sk_model::{Guid, Model};

/// Eine Bauleistung im Blatt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wahl {
    pub leistung: Guid,
    pub kurz: String,
    pub einheit: Einheit,
    /// EP an dieser Schicht; `None`, wenn ein Preis fehlt.
    pub ep: Option<Cent>,
    /// Anderes Gewerk als die Zeile: „WDV-Systeme (DIN 18345)“ für „kommt
    /// dann zu …“.
    pub fremd: Option<String>,
}

/// Die Liste des Blatts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auswahl {
    /// Aus dem Gewerk der Zeile.
    pub eigene: Vec<Wahl>,
    /// Aus anderen Gewerken mit derselben Stoffart.
    pub aehnlich: Vec<Wahl>,
    /// Alles Übrige („+ n weitere in m²“).
    pub weitere: Vec<Wahl>,
    /// Kurzname des Gewerks der Zeile („Dachdecker“).
    pub gewerk: String,
    /// Stoffart der Schicht („Dämmung“).
    pub stoffart: Option<&'static str>,
}

impl Auswahl {
    /// Erste Zeile, wenn das eigene Gewerk nichts hat: fett und leise.
    pub fn leer_text(&self) -> Option<(String, String)> {
        if !self.eigene.is_empty() {
            return None;
        }
        let fett = format!("Für {} gibt es noch keine Bauleistung.", self.gewerk);
        let leise = match self.stoffart {
            Some(s) if !self.aehnlich.is_empty() => {
                format!("Ähnliche aus anderen Gewerken, auch {s}:")
            }
            _ => "Ähnliche aus anderen Gewerken:".into(),
        };
        Some((fett, leise))
    }

    /// „+ n weitere in m²“ (Einheiten in der Reihenfolge der Liste).
    pub fn weitere_text(&self) -> Option<String> {
        if self.weitere.is_empty() {
            return None;
        }
        let mut e: Vec<&str> = Vec::new();
        for w in &self.weitere {
            if !e.contains(&w.einheit.zeichen()) {
                e.push(w.einheit.zeichen());
            }
        }
        Some(format!(
            "+ {} weitere in {}",
            self.weitere.len(),
            e.join(", ")
        ))
    }

    pub fn alle(&self) -> impl Iterator<Item = &Wahl> {
        self.eigene
            .iter()
            .chain(&self.aehnlich)
            .chain(&self.weitere)
    }
}

/// Operation „Bauleistung gewählt“ an der Schicht des Typs der Zeile.
pub fn zuordnen(z: &OhneZeile, leistung: Guid) -> Option<Op> {
    let s = z.schicht()?;
    Some(Op::BauleistungZuordnen {
        typ: s.typ?,
        schicht: s.schicht,
        bauleistung: Some(leistung),
    })
}

fn baustoff(m: &Model, g: Guid) -> Option<&Material> {
    m.materials().iter().map(|(_, x)| x).find(|x| x.guid == g)
}

fn gewerk_kurz(m: &Model, g: Option<Guid>) -> String {
    match g.and_then(|g| m.trade(TradeId(g))) {
        Some(t) => match t.short.as_deref() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => t.name.clone(),
        },
        None => "dieses Gewerk".into(),
    }
}

/// EP der Bauleistung an der Schicht (Regel 83): NU, sonst Lohn, Stoff,
/// Gerät und Sonstiges.
fn ep(k: &Katalog, l: &Leistung, z: &OhneZeile, mat: Option<&Material>) -> Option<Cent> {
    let (lohn, geraet, sonst, nu) = feste_ep(k, l);
    if nu.is_some() {
        return nu;
    }
    let (stoff, fehlt) = stoff_ep(
        k,
        l,
        Some((z.schicht().map_or(Dez::NULL, |s| s.dicke), mat)),
        false,
        &Ort::Datei,
        &mut Vec::new(),
    );
    (!fehlt).then_some(lohn + stoff + geraet + sonst)
}

fn passt(l: &Leistung, z: &OhneZeile) -> bool {
    !l.retired
        && !l.kategorien.is_empty()
        && z.schicht().is_some_and(|s| s.bezuege.contains(&l.bezug))
}

/// Gibt es für die graue Zeile etwas zu wählen (Verweis „Bauleistung
/// wählen …“ beim Überfahren)?
pub fn waehlbar(k: &Katalog, z: &OhneZeile) -> bool {
    z.schicht().is_some_and(|s| s.typ.is_some()) && k.leistungen.iter().any(|l| passt(l, z))
}

/// Die Liste für eine graue Zeile; `None`, wenn keine Bauleistung zu einem
/// Mengenbezug des Bauteils passt (die Zeile bleibt ohne Verweis) oder die
/// Schicht keinen Typ hat.
pub fn auswahl(m: &Model, k: &Katalog, z: &OhneZeile) -> Option<Auswahl> {
    let s = z.schicht()?;
    s.typ?;
    let mat = baustoff(m, s.baustoff);
    let art: Option<MatCategory> = mat.map(|x| x.category);
    let order = |g: Guid| m.trade(TradeId(g)).map_or(u16::MAX, |t| t.order);
    let mut treffer: Vec<&Leistung> = k.leistungen.iter().filter(|l| passt(l, z)).collect();
    if treffer.is_empty() {
        return None;
    }
    treffer.sort_by(|a, b| {
        (order(a.gewerk), k.oz_voll(a), a.guid).cmp(&(order(b.gewerk), k.oz_voll(b), b.guid))
    });
    let mut out = Auswahl {
        eigene: Vec::new(),
        aehnlich: Vec::new(),
        weitere: Vec::new(),
        gewerk: gewerk_kurz(m, z.gewerk),
        stoffart: art.map(MatCategory::name),
    };
    for l in treffer {
        let eigen = Some(l.gewerk) == z.gewerk;
        let w = Wahl {
            leistung: l.guid,
            kurz: l.kurz.clone(),
            einheit: l.einheit,
            ep: ep(k, l, z, mat),
            fremd: (!eigen).then(|| {
                m.trade(TradeId(l.gewerk))
                    .map_or_else(String::new, |t| format!("{} (DIN {})", t.name, t.code))
            }),
        };
        let gleich = art.is_some() && l.mat.and_then(|g| baustoff(m, g)).map(|x| x.category) == art;
        if eigen {
            out.eigene.push(w);
        } else if gleich {
            out.aehnlich.push(w);
        } else {
            out.weitere.push(w);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lesen;
    use sk_model::{qto, szo, GuidGen};

    /// RH-3 mit einer eigenen Dämmung der Dachterrasse (sonst greift die
    /// Werksleistung) im Gewerk Zimmerer, das im Werksbestand nichts hat:
    /// Die erste Zeile sagt das, dann Dämmungen aus anderen Gewerken mit
    /// „kommt dann zu“, der Rest dahinter. Wählen macht aus der grauen Zeile
    /// eine Position.
    #[test]
    fn dachterrasse_ehrlich_geordnet() {
        let text = include_str!("../referenz/rh3-versatz-dachterrasse.szo")
            .replace("3NKChAqkL3uwFZN5n$FOr1", "3NKChAqkL3uwFZN5n$FOr9")
            .replace(
                "t=80 fn=insulation core=0 trade=1S7Wf_00100800000004UY",
                "t=80 fn=insulation core=0 trade=1S7Wf_00100800000004UU",
            );
        let mut m = szo::read_with(&text, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
            .expect("lädt")
            .model;
        let k = lesen::katalog(&m, None);
        let b = lesen::kosten(&m, &qto::schedule(&m), &k, &crate::Umfang::projekt());
        let z = b
            .ohne
            .iter()
            .find(|z| {
                z.schicht()
                    .and_then(|s| baustoff(&m, s.baustoff))
                    .is_some_and(|x| x.category == MatCategory::Insulation)
            })
            .expect("graue Dämmschicht");
        let a = auswahl(&m, &k, z).expect("passende Einheit");
        let (fett, leise) = a.leer_text().expect("Zimmerer hat nichts");
        assert!(fett.starts_with("Für ") && fett.ends_with(" gibt es noch keine Bauleistung."));
        assert_eq!(leise, "Ähnliche aus anderen Gewerken, auch Dämmung:");
        assert!(!a.aehnlich.is_empty());
        assert!(a
            .aehnlich
            .iter()
            .all(|w| w.fremd.as_deref().is_some_and(|f| f.contains("(DIN "))));
        assert!(a.weitere_text().is_some_and(|t| t.starts_with("+ ")));
        // EP an dieser Schicht; ohne nur, wo der Stoff der Artikel der
        // Schicht ist (`layer=1`) und es für diesen Baustoff keinen gibt
        let schichtartikel = |g: Guid| k.anteile_von(g).any(|x| x.artikel.is_none());
        assert!(
            a.alle()
                .all(|w| w.ep.is_some_and(|c| c.0 > 0) || schichtartikel(w.leistung)),
            "{a:?}"
        );
        // wählen: eine Position mehr, die Zeile ist nicht mehr grau
        let w = &a.aehnlich[0];
        let op = zuordnen(z, w.leistung).unwrap();
        let (n, grau) = (b.positionen.len(), b.ohne.len());
        let h = crate::Herkunft::neu(crate::HerkunftArt::Manual, "2026-10-08", "12:00");
        m.begin("Bauleistung gewählt");
        crate::ausfuehren_folge(&mut m, None, crate::Rolle::Admin, &h, &[op]).unwrap();
        m.commit();
        let k = lesen::katalog(&m, None);
        let b2 = lesen::kosten(&m, &qto::schedule(&m), &k, &crate::Umfang::projekt());
        assert!(b2.ohne.len() < grau);
        assert!(
            b2.positionen.iter().any(|p| p.kurz.starts_with(&w.kurz)),
            "{n}"
        );
    }
}

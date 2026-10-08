//! Abgleich mit dem Firmenkatalog (Regel 92, paket-ka2 §5, Bedienbarkeit
//! 4.3): Welche Stammsätze der Projektkopie weichen von der Firma ab, ohne
//! eine eigene Projektabweichung zu sein? Daraus die Abgleichzeile „Für neue
//! Häuser gilt Lohn 65,00 €/h (hier 60,00) · übernehmen · so lassen“.
//! „übernehmen“ ist `StandUebernehmen` mit diesen Sätzen, „so lassen“
//! `AbgleichLassen` mit dem Firmenstand.

use crate::geld::Dez;
use crate::katalog::Katalog;
use crate::op::{firma_bezug, hat_kopie, projektherkunft, SatzId};
use sk_model::{ExtStore, Guid, Library, Model};
use std::collections::{BTreeMap, HashSet};

/// Stammsätze, die eine Projektkopie mit der Firma teilt (BIM §3).
const STAMM: [&str; 6] = ["article", "service", "svcpart", "svcfollow", "rate", "lot"];

/// Unterschiede zwischen Projekt und Firma.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Abgleich {
    /// Firmenstand (für `AbgleichLassen`).
    pub stand: u32,
    /// Abweichende Sätze ohne eigene Projektabweichung (für
    /// `StandUebernehmen`).
    pub saetze: Vec<SatzId>,
    /// Je Unterschied ein Satzteil mit Werten, Firma zuerst: „Lohn 65,00 €/h
    /// (hier 60,00)“. Gleichartige Sätze einer Bauleistung stehen einmal.
    pub texte: Vec<String>,
}

impl Abgleich {
    /// Text der Abgleichzeile ohne die Verweise: bis zwei Unterschiede im
    /// Satz, ab drei die Zahl (Liste im Tooltip).
    pub fn zeile(&self) -> String {
        match self.texte.as_slice() {
            [a] => format!("Für neue Häuser gilt {a}"),
            [a, b] => format!("Für neue Häuser gilt {a}, {b}"),
            v => format!("{} Werte für neue Häuser sind anders", v.len()),
        }
    }
}

/// Deutsch mit Komma und mindestens zwei Stellen („65,00“, „0,45“).
fn zahl(d: Dez) -> String {
    let t = d.text();
    let (g, b) = t.split_once('.').unwrap_or((&t, ""));
    let mut b = b.to_string();
    while b.len() < 2 {
        b.push('0');
    }
    format!("{g},{b}")
}

/// Unterschiede, wenn das Projekt eine Kopie hat, die Firma Kostensätze
/// trägt und der Stand nicht mit „so lassen“ zurückgestellt ist.
pub fn abgleich(m: &Model, firma: Option<&Library>) -> Option<Abgleich> {
    if !hat_kopie(m) {
        return None;
    }
    let (fk, bezug, _, stand) = firma_bezug(m, firma)?;
    let pk = crate::lesen::katalog(m, firma);
    if pk
        .kopie
        .as_ref()
        .and_then(|c| c.keep)
        .is_some_and(|keep| keep >= stand)
    {
        return None;
    }
    let projekt = m.ext_store();
    let markiert: HashSet<&str> = projekt
        .section("origin")
        .filter(|r| projektherkunft(&r.line))
        .filter_map(|r| r.id.as_deref())
        .collect();
    let zeilen = |s: &ExtStore, a: &str| -> BTreeMap<String, String> {
        s.section(a)
            .filter_map(|r| Some((r.id.clone()?, r.line.clone())))
            .collect()
    };
    let mut saetze = Vec::new();
    let mut texte: Vec<String> = Vec::new();
    for a in STAMM {
        let (p, f) = (zeilen(projekt, a), zeilen(&bezug, a));
        let mut ids: Vec<&String> = p.keys().chain(f.keys()).collect();
        ids.sort();
        ids.dedup();
        for id in ids {
            if markiert.contains(id.as_str()) || p.get(id) == f.get(id) {
                continue;
            }
            let Some(abschnitt) = crate::satz::abschnitt(a).map(|x| x.name) else {
                continue;
            };
            saetze.push(SatzId::neu(abschnitt, id.clone()));
            let t = text(a, id, &fk, &pk);
            if !texte.contains(&t) {
                texte.push(t);
            }
        }
    }
    (!saetze.is_empty()).then_some(Abgleich {
        stand,
        saetze,
        texte,
    })
}

/// Satzteil zu einem abweichenden Satz, Firma zuerst, „hier“ das Projekt.
fn text(abschnitt: &str, id: &str, f: &Katalog, p: &Katalog) -> String {
    let g = Guid::from_ifc(id);
    let leistung = |l: Option<Guid>| {
        l.and_then(|l| f.leistung(l).or_else(|| p.leistung(l)))
            .map_or_else(|| id.to_string(), |l| l.kurz.clone())
    };
    match abschnitt {
        "rate" => {
            let wert = |k: &Katalog| match id {
                "wage" => Some(k.werte.lohn),
                "surcharge" => Some(k.werte.zuschlag),
                "vat" => Some(k.werte.mwst),
                s => k
                    .werte
                    .stahl
                    .iter()
                    .find(|(w, _)| s.strip_prefix("steel.") == Some(w.as_str()))
                    .map(|x| x.1),
            };
            let (name, einheit) = match id {
                "wage" => ("Lohn".to_string(), " €/h"),
                "surcharge" => ("Zuschlag Stoff".to_string(), " %"),
                "vat" => ("MwSt.".to_string(), " %"),
                s => (
                    format!("Bewehrungsgrad {}", s.strip_prefix("steel.").unwrap_or(s)),
                    " kg/m³",
                ),
            };
            match (wert(f), wert(p)) {
                (Some(a), Some(b)) => format!("{name} {}{einheit} (hier {})", zahl(a), zahl(b)),
                (Some(a), None) => format!("{name} {}{einheit}", zahl(a)),
                _ => format!("{name} anders"),
            }
        }
        "article" => {
            let (a, b) = (g.and_then(|g| f.artikel(g)), g.and_then(|g| p.artikel(g)));
            let name = a.or(b).map_or_else(|| id.to_string(), |x| x.name.clone());
            match (a.and_then(|x| x.preis), b.and_then(|x| x.preis)) {
                (Some(x), Some(y)) if x != y => {
                    let e = a.map_or("", |a| a.einheit.zeichen());
                    format!("{name} {} €/{e} (hier {})", zahl(x), zahl(y))
                }
                _ => format!("{name} anders"),
            }
        }
        "service" => {
            let (a, b) = (g.and_then(|g| f.leistung(g)), g.and_then(|g| p.leistung(g)));
            let name = leistung(g);
            match (a, b) {
                (Some(x), Some(y)) if x.stunden != y.stunden => format!(
                    "Aufwandswert {name} {} h/{} (hier {})",
                    zahl(x.stunden),
                    x.einheit.zeichen(),
                    zahl(y.stunden)
                ),
                _ => format!("{name} anders"),
            }
        }
        "svcpart" => {
            let l = f
                .anteile
                .iter()
                .chain(&p.anteile)
                .find(|x| Some(x.guid) == g)
                .map(|x| x.leistung);
            format!("Stoffanteile von {}", leistung(l))
        }
        "svcfollow" => {
            let l = f
                .folgen
                .iter()
                .chain(&p.folgen)
                .find(|x| Some(x.guid) == g)
                .map(|x| x.leistung);
            format!("Folgepositionen von {}", leistung(l))
        }
        _ => {
            let name = g
                .and_then(|g| f.los(g).or_else(|| p.los(g)))
                .map_or_else(|| id.to_string(), |l| l.name.clone());
            format!("Los {name}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lesen, Herkunft, HerkunftArt, Op, Rolle};
    use sk_model::{szo, GuidGen};

    fn rh1() -> Model {
        szo::read_with(
            include_str!("../referenz/rh1-standardhaus.szo"),
            GuidGen::with_seed(1),
            &lesen::ABSCHNITTE_SZO,
        )
        .expect("lädt")
        .model
    }

    fn lohn(v: i64) -> Op {
        Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(v),
        }
    }

    /// Ohne Kopie kein Abgleich. Firma auf Lohn 65, Projektkopie 60: „Für
    /// neue Häuser gilt Lohn 65,00 €/h (hier 60,00)“; eine eigene
    /// Projektabweichung zählt nicht; „so lassen“ blendet bis zum nächsten
    /// Stand aus; „übernehmen“ macht gleich.
    #[test]
    fn lohn_fuer_neue_haeuser() {
        let h = Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "12:00");
        let leer = sk_model::write_szk(&Library::standard());
        let firma_text = crate::firma_anwenden(&leer, &leer, Rolle::Admin, &h, &[lohn(60)])
            .unwrap()
            .text;
        let firma = sk_model::read_szk_with(&firma_text, &lesen::ABSCHNITTE_SZK).unwrap();
        let mut m = rh1();
        assert_eq!(abgleich(&m, Some(&firma)), None, "ohne Kopie");
        // Kopie anlegen: ein Preis nur für dieses Haus (eigene Abweichung)
        let k = lesen::katalog(&m, Some(&firma));
        let art = k.artikel.iter().find(|a| a.preis.is_some()).unwrap();
        let preis = Op::PreisSetzen {
            artikel: art.guid,
            preis: Some(Dez::ganz(99)),
            stand: "10/2026".into(),
            quelle: "Preisblatt".into(),
        };
        m.begin("Preis");
        crate::ausfuehren_folge(&mut m, Some(&firma), Rolle::Admin, &h, &[preis]).unwrap();
        m.commit();
        assert_eq!(
            abgleich(&m, Some(&firma)),
            None,
            "eigene Abweichung zählt nicht"
        );
        // Firma: Lohn 65, neuer Stand
        let neu =
            crate::firma_anwenden(&firma_text, &firma_text, Rolle::Admin, &h, &[lohn(65)]).unwrap();
        let firma = sk_model::read_szk_with(&neu.text, &lesen::ABSCHNITTE_SZK).unwrap();
        let a = abgleich(&m, Some(&firma)).expect("Lohn anders");
        assert_eq!(
            a.zeile(),
            "Für neue Häuser gilt Lohn 65,00 €/h (hier 60,00)"
        );
        assert_eq!(a.stand, neu.stand);
        assert_eq!(a.saetze, [SatzId::neu("rate", "wage")]);
        // so lassen: weg bis zum nächsten Stand
        let mut m2 = m.clone();
        m2.begin("Lassen");
        let lassen = Op::AbgleichLassen { stand: a.stand };
        crate::ausfuehren_folge(&mut m2, Some(&firma), Rolle::Admin, &h, &[lassen]).unwrap();
        m2.commit();
        assert_eq!(abgleich(&m2, Some(&firma)), None);
        // übernehmen: gleich, der eigene Preis bleibt
        m.begin("Übernehmen");
        let op = Op::StandUebernehmen { saetze: a.saetze };
        crate::ausfuehren_folge(&mut m, Some(&firma), Rolle::Admin, &h, &[op]).unwrap();
        m.commit();
        assert_eq!(abgleich(&m, Some(&firma)), None);
        let k = lesen::katalog(&m, Some(&firma));
        assert_eq!(k.werte.lohn, Dez::ganz(65));
        assert_eq!(k.artikel(art.guid).unwrap().preis, Some(Dez::ganz(99)));
    }
}

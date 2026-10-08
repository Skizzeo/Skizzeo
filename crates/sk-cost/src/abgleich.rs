//! Abgleich mit dem Firmenkatalog (Regel 92, paket-ka2 §5, Bedienbarkeit
//! 4.3): Welche Stammsätze der Projektkopie weichen von der Firma ab, ohne
//! eine eigene Projektabweichung zu sein? Daraus die Abgleichzeile „Für neue
//! Häuser gilt Lohn 65,00 €/h (hier 60,00) · übernehmen · so lassen“, mit
//! einem Unterschied ohne Wert „Für neue Häuser geändert: …“ (Bedienbarkeit
//! 8.1).
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
    /// Ein Satzteil hat keinen Wert („Stoffanteile von …“, „Los …“).
    pub ohne_wert: bool,
    /// Abweichungen mit eigenem Projektwert (Regel 89): bleiben beim
    /// Übernehmen; Satzteile wie `texte` (KA-3a5).
    pub eigene: Vec<String>,
    /// Stammsätze, die gleich sind (KA-3a5 „Unterschiede ansehen“).
    pub gleich: usize,
}

impl Abgleich {
    /// Text der Abgleichzeile ohne die Verweise: bis zwei Unterschiede im
    /// Satz, ab drei die Zahl (Liste im Tooltip). Hat ein Teil keinen Wert,
    /// heißt es „geändert:“ statt „gilt“ (Bedienbarkeit 8.1).
    pub fn zeile(&self) -> String {
        match self.texte.as_slice() {
            v if v.len() > 2 => format!("{} Änderungen für neue Häuser", v.len()),
            v if self.ohne_wert => format!("Für neue Häuser geändert: {}", v.join(", ")),
            v => format!("Für neue Häuser gilt {}", v.join(", ")),
        }
    }

    /// Tooltip der Zeile: Kopf und je Unterschied eine Zeile. Er steht
    /// immer, denn die Zeile kann gekürzt sein (Bedienbarkeit 8.3).
    pub fn tooltip(&self) -> String {
        let kopf = if self.ohne_wert {
            "Für neue Häuser geändert:"
        } else {
            "Für neue Häuser gilt:"
        };
        format!("{kopf}\n{}", self.texte.join("\n"))
    }
}

/// Deutsch mit Komma, ganze Zahlen ohne Stellen („80“, „82,5“; kg/m³).
fn ganz_oder_komma(d: Dez) -> String {
    d.text().replace('.', ",")
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
    let mut ohne_wert = false;
    let mut eigene: Vec<String> = Vec::new();
    let mut gleich = 0;
    for a in STAMM {
        let (p, f) = (zeilen(projekt, a), zeilen(&bezug, a));
        let mut ids: Vec<&String> = p.keys().chain(f.keys()).collect();
        ids.sort();
        ids.dedup();
        for id in ids {
            if p.get(id) == f.get(id) {
                gleich += 1;
                continue;
            }
            let Some(abschnitt) = crate::satz::abschnitt(a).map(|x| x.name) else {
                continue;
            };
            if markiert.contains(id.as_str()) {
                let t = eigen_text(a, id, &fk, &pk);
                if !eigene.contains(&t) {
                    eigene.push(t);
                }
                continue;
            }
            saetze.push(SatzId::neu(abschnitt, id.clone()));
            let (t, wert) = text(a, id, &fk, &pk);
            if !texte.contains(&t) {
                ohne_wert |= !wert;
                texte.push(t);
            }
        }
    }
    (!saetze.is_empty()).then_some(Abgleich {
        stand,
        saetze,
        texte,
        ohne_wert,
        eigene,
        gleich,
    })
}

/// Satzteil zu einem eigenen Wert dieses Hauses: wie [`text`]; ein Preis,
/// der dem der Firma gleicht, steht mit „wie Firma“ statt nur dem Namen
/// (Bedienbarkeit 15). Bei einer Bauleistung kann das Eigene ein anderes
/// Feld als die Stunden sein, dort bleibt es beim Namen.
fn eigen_text(abschnitt: &str, id: &str, f: &Katalog, p: &Katalog) -> String {
    let (t, wert) = text(abschnitt, id, f, p);
    if wert {
        return t;
    }
    let g = Guid::from_ifc(id);
    match abschnitt {
        "article" => match g.and_then(|g| p.artikel(g)) {
            Some(a) if a.preis.is_some() => format!(
                "{t} {} €/{} (wie Firma)",
                zahl(a.preis.unwrap_or_default()),
                a.einheit.zeichen()
            ),
            _ => t,
        },
        _ => t,
    }
}

/// Satzteil zu einem abweichenden Satz, Firma zuerst, „hier“ das Projekt;
/// `true`, wenn er einen Wert nennt. Ein Name, der sich nicht auflösen
/// lässt, heißt „ein Eintrag“, nie nach seiner Kennung.
fn text(abschnitt: &str, id: &str, f: &Katalog, p: &Katalog) -> (String, bool) {
    use crate::wort::EIN_EINTRAG;
    let g = Guid::from_ifc(id);
    let leistung = |l: Option<Guid>| {
        l.and_then(|l| f.leistung(l).or_else(|| p.leistung(l)))
            .map_or_else(|| EIN_EINTRAG.to_string(), |l| l.kurz.clone())
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
            let (name, einheit, z): (String, &str, fn(Dez) -> String) = match id {
                "wage" => ("Lohn".into(), " €/h", zahl),
                "surcharge" => ("Zuschlag Stoff".into(), " %", zahl),
                "vat" => ("MwSt.".into(), " %", zahl),
                s => (crate::wort::bewehrungsgrad(s), " kg/m³", ganz_oder_komma),
            };
            match (wert(f), wert(p)) {
                (Some(a), Some(b)) => (format!("{name} {}{einheit} (hier {})", z(a), z(b)), true),
                (Some(a), None) => (format!("{name} {}{einheit}", z(a)), true),
                _ => (name, false),
            }
        }
        "article" => {
            let (a, b) = (g.and_then(|g| f.artikel(g)), g.and_then(|g| p.artikel(g)));
            let name = a
                .or(b)
                .map_or_else(|| EIN_EINTRAG.to_string(), |x| x.name.clone());
            match (a.and_then(|x| x.preis), b.and_then(|x| x.preis)) {
                (Some(x), Some(y)) if x != y => {
                    let e = a.map_or("", |a| a.einheit.zeichen());
                    (format!("{name} {} €/{e} (hier {})", zahl(x), zahl(y)), true)
                }
                _ => (name, false),
            }
        }
        "service" => {
            let (a, b) = (g.and_then(|g| f.leistung(g)), g.and_then(|g| p.leistung(g)));
            let name = leistung(g);
            match (a, b) {
                (Some(x), Some(y)) if x.stunden != y.stunden => (
                    format!(
                        "Aufwandswert {name} {} h/{} (hier {})",
                        zahl(x.stunden),
                        x.einheit.zeichen(),
                        zahl(y.stunden)
                    ),
                    true,
                ),
                _ => (name, false),
            }
        }
        "svcpart" => {
            let l = f
                .anteile
                .iter()
                .chain(&p.anteile)
                .find(|x| Some(x.guid) == g)
                .map(|x| x.leistung);
            (format!("Stoffanteile von {}", leistung(l)), false)
        }
        "svcfollow" => {
            let l = f
                .folgen
                .iter()
                .chain(&p.folgen)
                .find(|x| Some(x.guid) == g)
                .map(|x| x.leistung);
            (format!("Folgepositionen von {}", leistung(l)), false)
        }
        _ => {
            let name = g
                .and_then(|g| f.los(g).or_else(|| p.los(g)))
                .map_or_else(|| EIN_EINTRAG.to_string(), |l| l.name.clone());
            (format!("Los {name}"), false)
        }
    }
}

/// Die Sätze aus `saetze`, die dieses Haus als eigene Abweichung hält (Regel
/// 89), deren Zeile aber jetzt gleich der Firma ist, bei einer Bauleistung
/// samt Stoffanteilen und Folgepositionen. Nach „Freigeben“ (KA-3b3) ist so
/// ein eigener Wert der Firmenwert: `AbweichungZuruecknehmen` löst dann nur
/// den Vermerk, und das Haus folgt späteren Firmenänderungen wieder.
pub fn eigen_wie_firma(m: &Model, firma: Option<&Library>, saetze: &[SatzId]) -> Vec<SatzId> {
    let Some((_, bezug, _, _)) = firma_bezug(m, firma) else {
        return Vec::new();
    };
    let projekt = m.ext_store();
    let markiert: HashSet<&str> = projekt
        .section("origin")
        .filter(|r| projektherkunft(&r.line))
        .filter_map(|r| r.id.as_deref())
        .collect();
    let zeile = |s: &ExtStore, a: &str, id: &str| {
        s.section(a)
            .find(|r| r.id.as_deref() == Some(id))
            .map(|r| r.line.clone())
    };
    // Stoffanteile und Folgepositionen einer Bauleistung, je Seite
    let teile = |s: &ExtStore, g: &str| -> BTreeMap<(String, String), String> {
        ["svcpart", "svcfollow"]
            .iter()
            .flat_map(|a| s.section(a).map(move |r| (*a, r)))
            .filter(|(_, r)| {
                crate::zeile::zerlegen(&r.line)
                    .is_some_and(|z| z.paare.iter().any(|(k, v)| k == "service" && v == g))
            })
            .filter_map(|(a, r)| Some(((a.to_string(), r.id.clone()?), r.line.clone())))
            .collect()
    };
    saetze
        .iter()
        .filter(|s| markiert.contains(s.kennung.as_str()))
        .filter(|s| {
            let gleich = zeile(projekt, s.abschnitt, &s.kennung)
                .is_some_and(|p| Some(p) == zeile(&bezug, s.abschnitt, &s.kennung));
            gleich
                && (s.abschnitt != "service"
                    || teile(projekt, &s.kennung) == teile(&bezug, &s.kennung))
        })
        .cloned()
        .collect()
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
            eingabe: String::new(),
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
        // Unterschiede ansehen (KA-3a5): der eigene Preis bleibt, der Rest
        // ist gleich
        assert_eq!(a.eigene.len(), 1, "{:?}", a.eigene);
        assert!(a.eigene[0].ends_with("(hier 99,00)"), "{:?}", a.eigene);
        assert!(a.gleich > 50, "{}", a.gleich);
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

    /// Bedienbarkeit 15: ein eigener Preis, der dem der Firma gleicht, steht
    /// mit Wert und „wie Firma“, ein anderer mit „(hier …)“.
    #[test]
    fn eigener_preis_mit_wert() {
        let m = rh1();
        let k = lesen::katalog(&m, None);
        let art = k.artikel.iter().find(|a| a.preis.is_some()).unwrap();
        let id = art.guid.to_ifc();
        let t = eigen_text("article", &id, &k, &k);
        assert_eq!(
            t,
            format!(
                "{} {} €/{} (wie Firma)",
                art.name,
                zahl(art.preis.unwrap()),
                art.einheit.zeichen()
            )
        );
        let mut p = k.clone();
        p.artikel
            .iter_mut()
            .find(|a| a.guid == art.guid)
            .unwrap()
            .preis = Some(Dez::ganz(99));
        assert!(eigen_text("article", &id, &k, &p).ends_with("(hier 99,00)"));
    }

    /// Bedienbarkeit 8.1 und 8.2: Satzteile ohne Wert machen „geändert:“,
    /// ab drei steht die Zahl; Bewehrungsgrad nach der Bauteilart; ein
    /// Name ohne Auflösung ist „ein Eintrag“.
    #[test]
    fn saetze_ohne_wert_und_namen() {
        let leer = lesen::werk(&Model::new());
        let (t, w) = text("svcpart", "0000000000000000000XYZ", &leer, &leer);
        assert_eq!((t.as_str(), w), ("Stoffanteile von ein Eintrag", false));
        let (t, w) = text("article", "0000000000000000000XYZ", &leer, &leer);
        assert_eq!((t.as_str(), w), ("ein Eintrag", false));
        let mut hier = leer.clone();
        hier.werte.stahl = vec![("groundslab".into(), Dez::ganz(100))];
        let mut firma = leer.clone();
        firma.werte.stahl = vec![("groundslab".into(), Dez::ganz(80))];
        let (t, w) = text("rate", "steel.groundslab", &firma, &hier);
        assert_eq!(
            (t.as_str(), w),
            ("Bewehrungsgrad Sohlplatte 80 kg/m³ (hier 100)", true)
        );
        let a = Abgleich {
            stand: 2,
            saetze: Vec::new(),
            texte: vec!["Lohn 65,00 €/h (hier 60,00)".into()],
            ohne_wert: false,
            eigene: Vec::new(),
            gleich: 0,
        };
        assert_eq!(
            a.zeile(),
            "Für neue Häuser gilt Lohn 65,00 €/h (hier 60,00)"
        );
        assert_eq!(
            a.tooltip(),
            "Für neue Häuser gilt:\nLohn 65,00 €/h (hier 60,00)"
        );
        let a = Abgleich {
            texte: vec![
                "Lohn 65,00 €/h (hier 60,00)".into(),
                "Stoffanteile von AW Porenbeton 17,5".into(),
            ],
            ohne_wert: true,
            ..a
        };
        assert_eq!(
            a.zeile(),
            "Für neue Häuser geändert: Lohn 65,00 €/h (hier 60,00), Stoffanteile von AW Porenbeton 17,5"
        );
        assert!(a.tooltip().starts_with("Für neue Häuser geändert:\n"));
        let a = Abgleich {
            texte: vec!["a".into(), "b".into(), "c".into()],
            ..a
        };
        assert_eq!(a.zeile(), "3 Änderungen für neue Häuser");
    }
}

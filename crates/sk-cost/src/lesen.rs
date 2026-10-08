//! Lesefunktionen, rein und ohne `&mut` (Regel 94, Bausteingrenze §5).

use crate::befund::{self, satz_ort, Befund, Ort};
use crate::katalog::{self, Katalog, Quelle, Umfeld};
use sk_model::{Library, Model};

pub use crate::satz::{ABSCHNITTE_SZK, ABSCHNITTE_SZO};

/// Abschnitte mit Stammdatensätzen, die eine Quelle „hat“ (BIM §3.2–§3.7).
const STAMM: [&str; 6] = ["article", "service", "svcpart", "svcfollow", "rate", "lot"];

fn hat_kosten<'a>(mut ext: impl FnMut(&'a str) -> usize) -> bool {
    STAMM.iter().any(|s| ext(s) > 0)
}

/// Kopfzeile `[catalog]` eines Firmenkatalogs: (Name, Stand, Entwurf).
fn kopf(lib: &Library) -> Option<(String, u32, bool)> {
    let r = lib.ext("catalog").next()?;
    let k = katalog::lesen(
        [("catalog", r.line.as_str())],
        &Umfeld::aus_bibliothek(lib),
        Quelle::Werk {
            stand: String::new(),
        },
    );
    k.kopf.map(|c| (c.name, c.stand, c.entwurf))
}

/// „2026-10-08“ → „10/2026“.
fn monat(tag: &str) -> String {
    let mut t = tag.split('-');
    match (t.next(), t.next()) {
        (Some(j), Some(m)) => format!("{m}/{j}"),
        _ => tag.to_string(),
    }
}

/// Werksbestand gegen die Baustoffe von `m`.
pub fn werk(m: &Model) -> Katalog {
    let z = crate::werk_zeilen();
    let stand = z
        .iter()
        .find(|(a, _)| *a == "catalog")
        .and_then(|(_, l)| crate::zeile::zerlegen(l))
        .and_then(|z| z.paare.into_iter().find(|(k, _)| k == "date"))
        .map(|(_, d)| monat(&d))
        .unwrap_or_default();
    katalog::lesen(z, &Umfeld::aus_modell(m), Quelle::Werk { stand })
}

/// Wirksame Stammdaten nach Bausteingrenze §6: Projektkopie, sonst der
/// freigegebene Firmenkatalog mit Kostensätzen, sonst der Werksbestand.
/// Ein Projekt gilt als Kopie, wenn es `[costproject]` oder einen
/// Stammdatensatz enthält (nie still übergangene Projektzeilen).
pub fn katalog(m: &Model, firma: Option<&Library>) -> Katalog {
    let firma_kopf = firma.and_then(kopf);
    let firma_stand = firma_kopf.as_ref().filter(|k| !k.2).map(|k| k.1);
    let projekt = m.ext("costproject").next().is_some() || hat_kosten(|s| m.ext(s).count());
    let mut k = if projekt {
        let z = ABSCHNITTE_SZO
            .iter()
            .flat_map(|s| m.ext(s).map(move |r| (*s, r.line.as_str())));
        let mut k = katalog::lesen(
            z,
            &Umfeld::aus_modell(m),
            Quelle::Projekt {
                katalog: None,
                stand: None,
            },
        );
        if let Some(c) = &k.kopie {
            k.quelle = Quelle::Projekt {
                katalog: c.katalog,
                stand: c.stand,
            };
        }
        k
    } else {
        match firma.filter(|f| {
            hat_kosten(|s| f.ext(s).count()) && !firma_kopf.as_ref().is_some_and(|k| k.2)
        }) {
            Some(f) => {
                let z = ABSCHNITTE_SZK
                    .iter()
                    .flat_map(|s| f.ext(s).map(move |r| (*s, r.line.as_str())));
                let (name, stand) = firma_kopf
                    .clone()
                    .map_or(("Firmenkatalog".to_string(), 0), |k| (k.0, k.1));
                // Verweise auf Baustoffe gelten gegen das Projekt, in dem
                // gerechnet wird
                katalog::lesen(z, &Umfeld::aus_modell(m), Quelle::Firma { name, stand })
            }
            None => werk(m),
        }
    };
    k.firma_stand = firma_stand;
    k
}

/// Alle Befunde zu Stammdaten und Zuordnung an den Typen (Regeln 71–92,
/// 99). Die Zuordnungs- und Rechenregeln (81–85, 95–97) kommen mit der
/// Rechnung (KA-0e) dazu.
pub fn befunde(m: &Model, k: &Katalog) -> Vec<Befund> {
    let mut out = k.befunde.clone();
    // 99: svc= an einer Schicht zeigt ins Leere oder auf Ausgemustertes
    for (_, t) in m.layer_sets().iter() {
        for (i, l) in t.layers.iter().enumerate() {
            let Some(g) = l.svc else { continue };
            if k.leistung(g).is_some_and(|s| !s.retired) {
                continue;
            }
            let b = m.material(l.material).map_or("?", |x| x.name.as_str());
            out.push(Befund::warnung(
                99,
                befund::r99(&t.name, b),
                Ort::Schicht {
                    typ: t.guid,
                    schicht: i,
                },
            ));
        }
    }
    if let Quelle::Projekt { stand, .. } = &k.quelle {
        // 92: neuerer Firmenstand, solange nicht „so lassen“
        let eigen = stand.unwrap_or(0);
        let keep = k.kopie.as_ref().and_then(|c| c.keep).unwrap_or(0);
        if let Some(f) = k.firma_stand.filter(|f| *f > eigen && *f > keep) {
            let e = stand.map_or("ohne Stand".to_string(), |s| s.to_string());
            out.push(Befund::hinweis(92, befund::r92(f, &e), Ort::Datei));
        }
        // 89: Projektabweichung ist markiert
        let st = stand.map_or("?".to_string(), |s| s.to_string());
        for u in k.herkunft.iter().filter(|u| u.kind != "factory") {
            let name = match u.rec.as_str() {
                "article" => sk_model::Guid::from_ifc(&u.key)
                    .and_then(|g| k.artikel(g))
                    .map(|a| a.name.clone()),
                "service" => sk_model::Guid::from_ifc(&u.key)
                    .and_then(|g| k.leistung(g))
                    .map(|l| l.kurz.clone()),
                _ => None,
            }
            .unwrap_or_else(|| u.key.clone());
            out.push(Befund::hinweis(
                89,
                befund::r89(&name, &st),
                satz_ort(
                    crate::satz::abschnitt(&u.rec).map_or("origin", |a| a.name),
                    u.key.clone(),
                ),
            ));
        }
    }
    // 75: Werkswert unter anderer Kennung (gleicher Name, andere Guid)
    if !matches!(k.quelle, Quelle::Werk { .. }) {
        let w = werk(m);
        for a in &k.artikel {
            if w.artikel
                .iter()
                .any(|x| x.name == a.name && x.guid != a.guid)
            {
                out.push(Befund::warnung(
                    75,
                    befund::r75(&a.name),
                    satz_ort("article", a.guid.to_ifc()),
                ));
            }
        }
        for l in &k.leistungen {
            if w.leistungen
                .iter()
                .any(|x| x.kurz == l.kurz && x.guid != l.guid)
            {
                out.push(Befund::warnung(
                    75,
                    befund::r75(&l.kurz),
                    satz_ort("service", l.guid.to_ifc()),
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geld::Dez;
    use sk_model::Guid;

    const FIRMA: &str = "0000000000000000000F01";

    fn firma(stand: u32, status: &str, lohn: u32) -> Library {
        let mut lib = Library::standard();
        lib.ext_put(
            "catalog",
            FIRMA,
            format!("[catalog] guid={FIRMA} name=\"Muster Bau\" stand={stand} status={status}"),
            None,
        );
        lib.ext_put("rate", "wage", format!("[rate] key=wage num={lohn}"), None);
        lib
    }

    fn projekt(stand: u32, lohn: u32) -> Model {
        let mut m = Model::new();
        m.begin("Kopie");
        m.ext_put(
            "costproject",
            "project",
            format!("[costproject] key=project catalog={FIRMA} stand={stand}"),
            None,
        );
        m.ext_put("rate", "wage", format!("[rate] key=wage num={lohn}"), None);
        m.commit().unwrap();
        m
    }

    /// Abnahme 9: Quelle der Stammdaten (Bausteingrenze §6).
    #[test]
    fn quelle_kopie_firma_werk() {
        let neu = Model::new();
        // ohne Firma: Werk
        let k = katalog(&neu, None);
        assert_eq!(k.quelle.text(), "Werkspreise 10/2026");
        assert_eq!(k.werte.lohn, Dez::ganz(60));
        // Firma vor Werk; nur Firmenwerte ohne Artikel: der Rest fehlt nicht
        // still, sondern die Quelle ist die Firma
        let f = firma(3, "released", 70);
        let k = katalog(&neu, Some(&f));
        assert_eq!(
            k.quelle,
            Quelle::Firma {
                name: "Muster Bau".into(),
                stand: 3,
            }
        );
        assert_eq!(k.werte.lohn, Dez::ganz(70));
        // Firmenentwurf zählt nie
        let k = katalog(&neu, Some(&firma(3, "draft", 70)));
        assert!(matches!(k.quelle, Quelle::Werk { .. }));
        // Projekt mit Kopie rechnet nur mit der Kopie, auch bei neuerem
        // Firmenstand (Befund 92)
        let m = projekt(2, 65);
        let k = katalog(&m, Some(&f));
        assert_eq!(
            k.quelle,
            Quelle::Projekt {
                katalog: Guid::from_ifc(FIRMA),
                stand: Some(2),
            }
        );
        assert_eq!(k.werte.lohn, Dez::ganz(65));
        assert!(k.leistungen.is_empty());
        let b = befunde(&m, &k);
        assert!(
            b.iter().any(|b| b.regel == 92
                && b.satz
                    == "Firmenkatalog Stand 3 ist verfügbar; das Projekt rechnet mit Stand 2."),
            "{b:#?}"
        );
        // gleicher Stand: kein Befund 92
        let k = katalog(&projekt(3, 65), Some(&f));
        assert!(befunde(&m, &k).iter().all(|b| b.regel != 92));
    }

    /// Abnahme 15: Lesen ohne Nebenwirkung (Regel 94).
    #[test]
    fn lesen_ohne_nebenwirkung() {
        let mut m = projekt(2, 65);
        let id = m.layer_sets().ids().next().unwrap();
        let mut t = m.layer_set(id).unwrap().clone();
        t.layers[0].svc = Some(Guid::from_ifc("0000000000000000000S99").unwrap());
        m.begin("svc");
        m.set_layer_set(id, t);
        m.commit().unwrap();
        let vorher = (m.revision(), m.ext_revision(), sk_model::szo::write(&m));
        let f = firma(3, "released", 70);
        let k = katalog(&m, Some(&f));
        let b = befunde(&m, &k);
        let _ = werk(&m);
        assert_eq!(
            vorher,
            (m.revision(), m.ext_revision(), sk_model::szo::write(&m))
        );
        // 99: gewählte Bauleistung gibt es nicht
        assert!(b.iter().any(|b| b.regel == 99), "{b:#?}");
    }

    /// Projektzeilen ohne `[costproject]` werden nicht still übergangen.
    #[test]
    fn projektzeilen_ohne_kopfzeile_gelten() {
        let mut m = Model::new();
        m.begin("Lohn");
        m.ext_put("rate", "wage", "[rate] key=wage num=61".into(), None);
        m.commit().unwrap();
        let k = katalog(&m, None);
        assert_eq!(k.quelle.text(), "Projektstand");
        assert_eq!(k.werte.lohn, Dez::ganz(61));
    }
}

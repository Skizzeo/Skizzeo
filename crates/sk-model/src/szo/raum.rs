//! Räume in der `.szo` (Grundriss G3, schnittstelle-raum.md §2): je Raum
//! ein `[space]` mit Anker und Name, `stamp=`, `use=` und `factor=` nur wenn
//! gesetzt. Die Form steht nie in der Datei; Dateien ohne Räume bleiben
//! bytegleich.

use super::{ausserhalb, err, LoadError, Record};
use crate::element::StoreyId;
use crate::element::{Category, Element, ElementKind};
use crate::model::{Model, Space};
use crate::Guid;
use sk_math::vec3;
use std::collections::HashMap;

pub(super) fn write_spaces(m: &Model, els: &[&Element], out: &mut String) {
    for e in els {
        let ElementKind::Space(s) = &e.kind else {
            continue;
        };
        let mut l = super::Line::new("space")
            .guid("guid", Some(e.guid))
            .text("number", &e.number)
            .word("at", &format!("{},{}", s.anker.x, s.anker.y))
            .text("name", &s.name);
        if let Some(p) = s.stempel {
            l = l.word("stamp", &format!("{},{}", p.x, p.y));
        }
        if s.nutzung.is_some() {
            l = l.guid("use", s.nutzung);
        }
        if let Some(f) = s.faktor {
            l = l.num("factor", f);
        }
        l.num("seq", e.seq)
            .guid("storey", m.storey(e.storey).map(|s| s.guid))
            .finish(out);
    }
}

/// Liest einen `[space]` als Bauteil. Eine unlesbare Stempellage oder ein
/// Faktor außerhalb 0 … 1 fällt mit Hinweis weg (Regel 114). Ist sonst
/// etwas unlesbar, das Geschoss unbekannt oder Guid bzw. Nummer doppelt,
/// entfällt die Zeile mit Hinweis, das Projekt öffnet (Regel 72, Review #87
/// R3); der Raum entsteht beim nächsten Abgleich seiner Fläche neu.
pub(super) fn read(
    r: &Record,
    number: &mut impl FnMut(&Record) -> Result<String, LoadError>,
    storeys: &HashMap<Guid, StoreyId>,
    seen: &HashMap<Guid, usize>,
    hints: &mut Vec<String>,
) -> Option<Element> {
    let mut eigene = Vec::new();
    let mut lesen = || -> Result<Element, LoadError> {
        let guid = r.guid("guid")?;
        if let Some(first) = seen.get(&guid) {
            let g = guid.to_ifc();
            return Err(err(
                r.line,
                format!("Guid {g} doppelt (schon in Zeile {first})"),
            ));
        }
        let storey = r.link("storey", storeys)?;
        let seq = r
            .opt("seq")
            .map_or(Ok(crate::SPACE_SEQ), |_| r.int("seq"))?;
        let kind = ElementKind::Space(Box::new(space(r, &mut eigene)?));
        Ok(Element {
            guid,
            number: number(r)?,
            category: Category::Space,
            storey,
            layer_set: None,
            seq,
            kind,
            props: Default::default(),
            locked: false,
        })
    };
    match lesen() {
        Ok(e) => {
            hints.append(&mut eigene);
            Some(e)
        }
        Err(e) => {
            r.all_used();
            hints.push(format!("{e}; der Raum entfällt"));
            None
        }
    }
}

fn space(r: &Record, hints: &mut Vec<String>) -> Result<Space, LoadError> {
    let at = punkt(r.get("at")?).ok_or_else(|| r.bad("at", "x,y"))?;
    let mut s = Space::neu(at, r.get("name")?.to_string());
    if let Some(t) = r.opt("stamp") {
        match punkt(t) {
            Some(p) => s.stempel = Some(p),
            None => hints.push(format!("Zeile {}: Stempellage unlesbar, mittig", r.line)),
        }
    }
    if let Some(t) = r.opt("use") {
        match Guid::from_ifc(t) {
            Some(g) => s.nutzung = Some(g),
            None => hints.push(format!("Zeile {}: Nutzung unlesbar, Vorgabe", r.line)),
        }
    }
    if let Some(t) = r.opt("factor") {
        match t.parse::<f64>().ok().filter(|f| f.is_finite()) {
            Some(f) if (0.0..=1.0).contains(&f) => s.faktor = Some(f),
            Some(f) => hints.push(ausserhalb(r.line, "Faktor", f, 0.0, 1.0, "")),
            None => hints.push(format!("Zeile {}: Faktor unlesbar, entfällt", r.line)),
        }
    }
    Ok(s)
}

fn punkt(t: &str) -> Option<sk_math::Vec3> {
    let (x, y) = t.split_once(',')?;
    let (x, y) = (x.parse::<f64>().ok()?, y.parse::<f64>().ok()?);
    (x.is_finite() && y.is_finite()).then(|| vec3(x, y, 0.0))
}

#[cfg(test)]
mod tests {
    use super::super::tests::house;
    use super::super::{read, write};
    use super::*;
    use crate::guid::GuidGen;

    #[test]
    fn raeume_in_der_datei() {
        let mut m = house();
        let ohne = write(&m);
        assert!(!ohne.contains("[space]"), "ohne Räume kein Abschnitt");
        let st = m.defaults().storey;
        let a = m.add_space(
            st,
            Space::neu(vec3(2000.0, 3000.5, 0.0), "Küche \"neu\"".into()),
        );
        let b = m.add_space(st, Space::neu(vec3(7000.0, 3000.0, 0.0), "Raum 2".into()));
        m.set_space(b, |s| {
            s.stempel = Some(vec3(7100.0, 2900.0, 0.0));
            s.faktor = Some(0.5);
            s.nutzung = Some(crate::guid::Guid::from_ifc("0123456789abcdefghijkl").unwrap());
        });
        let text = write(&m);
        let zeile = |n: &str| text.lines().find(|l| l.contains(n)).unwrap();
        let k = zeile("R-001");
        assert!(
            k.contains(" at=2000,3000.5 name=\"Küche \\\"neu\\\"\" seq=0"),
            "{k}"
        );
        let r = zeile("R-002");
        assert!(
            r.contains(" stamp=7100,2900 use=0123456789abcdefghijkl factor=0.5 "),
            "{r}"
        );
        let l = read(&text, GuidGen::with_seed(99)).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(write(&l.model), text, "bytegleich");
        let nach = |id| {
            let g = m.element(id).unwrap().guid;
            let (i, _) = l
                .model
                .elements()
                .iter()
                .find(|(_, e)| e.guid == g)
                .unwrap();
            l.model.space(i).cloned()
        };
        assert_eq!(nach(a).as_ref(), m.space(a));
        assert_eq!(nach(b).as_ref(), m.space(b));
        assert!(l.model.check().is_empty(), "{:?}", l.model.check());
        // Unsinn fällt mit Hinweis weg, das Projekt öffnet
        let kaputt = text
            .replace("factor=0.5", "factor=7")
            .replace("stamp=7100,2900", "stamp=x");
        let l = read(&kaputt, GuidGen::with_seed(99)).unwrap();
        assert_eq!(l.hints.len(), 2, "{:?}", l.hints);
        let s = l.model.elements().iter().find_map(|(i, e)| {
            (e.number == "R-002")
                .then(|| l.model.space(i).cloned())
                .flatten()
        });
        assert_eq!(s.map(|s| (s.faktor, s.stempel)), Some((None, None)));
        let r2 = |l: &super::super::Loaded| {
            l.model.elements().iter().find_map(|(i, e)| {
                (e.number == "R-002")
                    .then(|| l.model.space(i).cloned())
                    .flatten()
            })
        };
        // Unlesbare Nutzung und Faktor: nur der Wert entfällt
        let kaputt = text
            .replace("factor=0.5", "factor=abc")
            .replace("use=0123456789abcdefghijkl", "use=xyz");
        let l = read(&kaputt, GuidGen::with_seed(99)).unwrap();
        assert_eq!(l.hints.len(), 2, "{:?}", l.hints);
        let s = r2(&l).unwrap();
        assert_eq!(
            (s.nutzung, s.faktor, s.name.as_str()),
            (None, None, "Raum 2")
        );
        // Sonst Unlesbares, unbekanntes Geschoss oder Doppeltes: die Zeile
        // entfällt mit Hinweis, das Projekt öffnet (Review #87 R3)
        let zeile = r.to_string();
        let g = m.element(b).unwrap().guid.to_ifc();
        let st_guid = m.storey(st).unwrap().guid.to_ifc();
        let andere = m.element(a).unwrap().guid.to_ifc();
        for (was, neu) in [
            (" at=7000,3000".to_string(), " at=7000;3000".to_string()),
            (" at=7000,3000".to_string(), String::new()),
            (" name=\"Raum 2\"".to_string(), String::new()),
            (" seq=0".to_string(), " seq=x".to_string()),
            (
                format!(" storey={st_guid}"),
                " storey=3vHRQ8oT0Hsm00051Mm008".to_string(),
            ),
            (format!(" guid={g}"), format!(" guid={andere}")),
            (
                " number=\"R-002\"".to_string(),
                " number=\"R-001\"".to_string(),
            ),
        ] {
            assert!(zeile.contains(&was), "{was} in {zeile}");
            let kaputt = text.replace(&zeile, &zeile.replace(&was, &neu));
            let l = read(&kaputt, GuidGen::with_seed(99)).expect(&neu);
            assert_eq!(l.hints.len(), 1, "{neu}: {:?}", l.hints);
            assert!(l.hints[0].ends_with("der Raum entfällt"), "{:?}", l.hints);
            let raeume = l.model.elements().iter();
            let raeume = raeume.filter(|(i, _)| l.model.space(*i).is_some()).count();
            assert_eq!(raeume, 1, "{neu}");
            assert!(l.model.check().is_empty(), "{neu}: {:?}", l.model.check());
        }
    }
}

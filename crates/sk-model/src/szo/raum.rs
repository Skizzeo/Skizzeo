//! Räume in der `.szo` (Grundriss G3, schnittstelle-raum.md §2): je Raum
//! ein `[space]` mit Anker und Name, `stamp=`, `use=` und `factor=` nur wenn
//! gesetzt. Die Form steht nie in der Datei; Dateien ohne Räume bleiben
//! bytegleich.

use super::{ausserhalb, LoadError, Record};
use crate::element::StoreyId;
use crate::element::{Category, Element, ElementKind};
use crate::model::{Model, Space};
use sk_math::vec3;

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

/// Liest einen `[space]` als Bauteil; eine unlesbare Stempellage oder ein
/// Faktor außerhalb 0 … 1 fällt mit Hinweis weg (Regel 114).
pub(super) fn read(
    r: &Record,
    number: String,
    storey: StoreyId,
    hints: &mut Vec<String>,
) -> Result<Element, LoadError> {
    Ok(Element {
        guid: r.guid("guid")?,
        number,
        category: Category::Space,
        storey,
        layer_set: None,
        seq: r
            .opt("seq")
            .map_or(Ok(crate::SPACE_SEQ), |_| r.int("seq"))?,
        kind: ElementKind::Space(space(r, hints)?),
        props: Default::default(),
        locked: false,
    })
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
    if r.opt("use").is_some() {
        s.nutzung = Some(r.guid("use")?);
    }
    if r.opt("factor").is_some() {
        let f = r.f64("factor")?;
        if (0.0..=1.0).contains(&f) {
            s.faktor = Some(f);
        } else {
            hints.push(ausserhalb(r.line, "Faktor", f, 0.0, 1.0, ""));
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
    }
}

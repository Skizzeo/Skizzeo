//! Räume in der `.szo` (Grundriss G3, schnittstelle-raum.md §2): je Raum
//! ein `[space]` mit Anker und Name, `stamp=`, `use=` und `factor=` nur wenn
//! gesetzt. Die Form steht nie in der Datei; Dateien ohne Räume bleiben
//! bytegleich.

use super::{ausserhalb, err, LoadError, Record};
use crate::element::StoreyId;
use crate::element::{Category, Element, ElementKind};
use crate::model::{Model, RawSpace, Space};
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
    for r in m.raw_spaces() {
        for l in std::iter::once(&r.line).chain(&r.props) {
            out.push_str(l);
            out.push('\n');
        }
    }
}

/// Was aus einer `[space]`-Zeile wird.
pub(super) enum Lesung {
    Raum(Element),
    /// Unlesbar: Die Zeile bleibt roh in der Datei ([`raw`]).
    Roh,
    /// Guid oder Nummer doppelt, Geschoss unbekannt: Die Zeile entfällt.
    Weg,
}

/// Liest einen `[space]` als Bauteil. Eine unlesbare Stempellage oder ein
/// Faktor außerhalb 0 … 1 fällt mit Hinweis weg (Regel 114). Ist sonst
/// etwas unlesbar, bleibt die Zeile roh in der Datei (Regel 72), ihr Raum
/// zählt nicht. Ein unbekanntes Geschoss oder eine doppelte Guid bzw.
/// Nummer lässt die Zeile entfallen; das Projekt öffnet in jedem Fall
/// (Review #87 R3).
pub(super) fn read(
    r: &Record,
    number: &mut impl FnMut(&Record) -> Result<String, LoadError>,
    storeys: &HashMap<Guid, StoreyId>,
    seen: &HashMap<Guid, usize>,
    hints: &mut Vec<String>,
) -> Lesung {
    let weg = |e: LoadError, hints: &mut Vec<String>| {
        r.all_used();
        hints.push(format!("{e}; der Raum entfällt"));
        Lesung::Weg
    };
    if let Some(g) = r.opt("guid").and_then(Guid::from_ifc) {
        if let Some(first) = seen.get(&g) {
            let e = format!("Guid {} doppelt (schon in Zeile {first})", g.to_ifc());
            return weg(err(r.line, e), hints);
        }
    }
    let storey = match r.link("storey", storeys) {
        Ok(s) => s,
        Err(e) => return weg(e, hints),
    };
    let mut eigene = Vec::new();
    let mut lesen = || -> Result<(Guid, u16, Space), LoadError> {
        let guid = r.guid("guid")?;
        r.get("number")?;
        let seq = r
            .opt("seq")
            .map_or(Ok(crate::SPACE_SEQ), |_| r.int("seq"))?;
        Ok((guid, seq, space(r, &mut eigene)?))
    };
    let (guid, seq, s) = match lesen() {
        Ok(t) => t,
        Err(e) => {
            r.all_used();
            hints.push(format!("{e}; die Zeile bleibt unverändert in der Datei"));
            return Lesung::Roh;
        }
    };
    match number(r) {
        Ok(number) => {
            hints.append(&mut eigene);
            Lesung::Raum(Element {
                guid,
                number,
                category: Category::Space,
                storey,
                layer_set: None,
                seq,
                kind: ElementKind::Space(Box::new(s)),
                props: Default::default(),
                locked: false,
            })
        }
        Err(e) => weg(e, hints),
    }
}

/// Rohe `[space]`-Zeile nach den gültigen Bauteilen (L12): Guid und
/// Nummer bleiben belegt; sind sie inzwischen doppelt, entfällt die Zeile.
pub(super) fn raw(
    r: &Record,
    line: &str,
    number: &mut impl FnMut(&Record) -> Result<String, LoadError>,
    seen: &mut HashMap<Guid, usize>,
    hints: &mut Vec<String>,
) -> Option<RawSpace> {
    let guid = r.opt("guid").and_then(Guid::from_ifc);
    let doppelt = |was: String| {
        format!(
            "Zeile {}: [space] {was} doppelt, die Zeile entfällt",
            r.line
        )
    };
    if let Some(g) = guid.filter(|g| seen.contains_key(g)) {
        hints.push(doppelt(format!("Guid {}", g.to_ifc())));
        return None;
    }
    let number = match r.opt("number") {
        Some(n) => match number(r) {
            Ok(n) => Some(n),
            Err(_) => {
                hints.push(doppelt(format!("Nummer {n}")));
                return None;
            }
        },
        None => None,
    };
    if let Some(g) = guid {
        seen.insert(g, r.line);
    }
    Some(RawSpace {
        line: line.to_string(),
        props: Vec::new(),
        guid,
        storey: r.guid("storey").ok()?,
        anker: r.opt("at").and_then(punkt),
        number,
    })
}

/// Eine `[prop]`-Zeile zu einer rohen `[space]`-Zeile bleibt mit ihr roh.
pub(super) fn raw_prop(r: &Record, line: &str, raw: &mut [RawSpace]) -> bool {
    let g = r.opt("elem").and_then(Guid::from_ifc);
    let Some(s) = raw.iter_mut().find(|s| g.is_some() && s.guid == g) else {
        return false;
    };
    r.all_used();
    s.props.push(line.to_string());
    true
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
        // Sonst Unlesbares bleibt roh (Regel 72), unbekanntes Geschoss oder
        // Doppeltes entfällt; das Projekt öffnet (Review #87 R3)
        let zeile = r.to_string();
        let g = m.element(b).unwrap().guid.to_ifc();
        let st_guid = m.storey(st).unwrap().guid.to_ifc();
        let andere = m.element(a).unwrap().guid.to_ifc();
        for (was, neu, roh) in [
            (
                " at=7000,3000".to_string(),
                " at=7000;3000".to_string(),
                true,
            ),
            (" at=7000,3000".to_string(), String::new(), true),
            (" name=\"Raum 2\"".to_string(), String::new(), true),
            (" seq=0".to_string(), " seq=x".to_string(), true),
            (format!(" guid={g}"), " guid=x".to_string(), true),
            (
                format!(" storey={st_guid}"),
                " storey=3vHRQ8oT0Hsm00051Mm008".to_string(),
                false,
            ),
            (format!(" guid={g}"), format!(" guid={andere}"), false),
            (
                " number=\"R-002\"".to_string(),
                " number=\"R-001\"".to_string(),
                false,
            ),
        ] {
            assert!(zeile.contains(&was), "{was} in {zeile}");
            let kaputte = zeile.replace(&was, &neu);
            let kaputt = text.replace(&zeile, &kaputte);
            let l = read(&kaputt, GuidGen::with_seed(99)).expect(&neu);
            assert_eq!(l.hints.len(), 1, "{neu}: {:?}", l.hints);
            let folge = match roh {
                true => "die Zeile bleibt unverändert in der Datei",
                false => "der Raum entfällt",
            };
            assert!(l.hints[0].ends_with(folge), "{:?}", l.hints);
            let raeume = l.model.elements().iter();
            let raeume = raeume.filter(|(i, _)| l.model.space(*i).is_some()).count();
            assert_eq!(raeume, 1, "{neu}");
            assert!(l.model.check().is_empty(), "{neu}: {:?}", l.model.check());
            let neu_text = write(&l.model);
            if roh || neu.contains("storey") {
                assert_eq!(neu_text.contains(&kaputte), roh, "{neu}: {neu_text}");
            }
            let l2 = read(&neu_text, GuidGen::with_seed(99)).unwrap();
            assert_eq!(write(&l2.model), neu_text, "{neu}: stabil");
        }
    }

    /// Regel 72 (BIM-Datenmodell zu #87): Eine rohe `[space]`-Zeile hält
    /// Nummer und Guid, ihre Eigenschaften bleiben mit ihr roh; mit ihrem
    /// Geschoss entfällt sie.
    #[test]
    fn rohe_raumzeile() {
        let mut m = house();
        let st = m.defaults().storey;
        m.add_space(st, Space::neu(vec3(2000.0, 3000.0, 0.0), "Küche".into()));
        let b = m.add_space(st, Space::neu(vec3(7000.0, 3000.0, 0.0), "Bad".into()));
        assert!(m.set_prop(
            b,
            "Bemerkung",
            Some(crate::PropValue::Text("Fliesen".into()))
        ));
        let text = write(&m);
        let zeile = text.lines().find(|l| l.contains("R-002")).unwrap();
        let prop = text.lines().find(|l| l.contains("Fliesen")).unwrap();
        let ohne_name = zeile.replace(" name=\"Bad\"", "");
        let kaputt = text.replace(zeile, &ohne_name);
        let mut l = read(&kaputt, GuidGen::with_seed(7)).unwrap().model;
        let g = m.storey(st).unwrap().guid;
        let st = l.storeys().iter().find(|(_, s)| s.guid == g).unwrap().0;
        assert_eq!(l.spaces(st).len(), 1);
        assert_eq!(l.raw_space_anchors(st), vec![vec3(7000.0, 3000.0, 0.0)]);
        let roh = write(&l);
        assert!(roh.contains(&format!("{ohne_name}\n{prop}\n")), "{roh}");
        assert_eq!(roh.matches(prop).count(), 1, "{roh}");
        // Nummer belegt: der nächste Raum wird R-003, R-002 ist reserviert
        l.begin("Raum");
        let c = l.add_space(st, Space::neu(vec3(9000.0, 3000.0, 0.0), "Diele".into()));
        assert_eq!(l.element(c).unwrap().number, "R-003");
        assert_eq!(l.set_number(c, "R-002"), Err(crate::NumberError::Reserved));
        l.commit();
        // Dieselbe Guid in einer zweiten rohen Zeile: die zweite entfällt
        let zweite = zeile
            .replace(" number=\"R-002\"", " number=\"R-009\"")
            .replace(" at=7000,3000", " at=x");
        let doppelt = format!("{kaputt}{zweite}\n");
        let d = read(&doppelt, GuidGen::with_seed(7)).unwrap();
        assert!(
            d.hints
                .iter()
                .any(|h| h.ends_with("doppelt, die Zeile entfällt")),
            "{:?}",
            d.hints
        );
        assert!(!write(&d.model).contains(&zweite));
        // Ohne ihr Geschoss entfällt sie mit ihren Eigenschaften
        l.space_raw[0].storey = Guid::from_ifc("0000000000000000000000").unwrap();
        assert!(l.raw_space_anchors(st).is_empty());
        let ohne = write(&l);
        assert!(
            !ohne.contains(&ohne_name) && !ohne.contains("Fliesen"),
            "{ohne}"
        );
    }
}

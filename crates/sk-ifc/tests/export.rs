//! IFC-Export der Referenzhäuser (Plan ifc/plan.md §5, I1): Form der Datei,
//! Kennungen, Gliederung, Klassen, Schichten und Rauminhalt der Körper.

use sk_ifc::geometrie;
use sk_ifc::step::{self, File, V};
use sk_ifc::{export, Optionen};
use sk_model::{run_qto, szo, Category, ElementKind, GuidGen, Model};
use std::collections::{BTreeMap, HashMap, HashSet};

const HAEUSER: [(&str, &str); 6] = [
    (
        "rh1",
        include_str!("../../sk-cost/referenz/rh1-standardhaus.szo"),
    ),
    (
        "rh2",
        include_str!("../../sk-cost/referenz/rh2-mehrschalig.szo"),
    ),
    (
        "rh3",
        include_str!("../../sk-cost/referenz/rh3-versatz-dachterrasse.szo"),
    ),
    (
        "rh4",
        include_str!("../../sk-cost/referenz/rh4-erweiterungen.szo"),
    ),
    (
        "rh5",
        include_str!("../../sk-cost/referenz/rh5-flachdach.szo"),
    ),
    (
        "r7",
        include_str!("../../sk-cost/referenz/r7-alle-abschnitte.szo"),
    ),
];

fn laden(text: &str) -> Model {
    szo::read(text, GuidGen::with_seed(1)).expect("lädt").model
}

fn opt() -> Optionen {
    Optionen {
        zeit: 1_791_819_528,
        datei: "test.ifc".into(),
        version: "0.2.0".into(),
    }
}

fn verweise(v: &V, out: &mut Vec<u32>) {
    match v {
        V::Ref(i) => out.push(*i),
        V::List(xs) => xs.iter().for_each(|x| verweise(x, out)),
        V::Typed(_, x) => verweise(x, out),
        _ => {}
    }
}

fn text(v: &V) -> &str {
    match v {
        V::Str(t) => t,
        _ => "",
    }
}

fn liste(v: &V) -> Vec<u32> {
    let mut out = Vec::new();
    verweise(v, &mut out);
    out
}

/// Bauteile der Datei (Klassen, die Skizzeo für Bauteile schreibt) nach
/// Kennung: (Nummer, Klasse).
fn bauteile(f: &File) -> HashMap<String, (u32, String)> {
    let klassen: HashSet<String> = sk_ifc::klassen::KLASSEN
        .iter()
        .map(|k| k.name.to_ascii_uppercase())
        .collect();
    f.data
        .iter()
        .filter(|(_, e)| klassen.contains(&e.name))
        .map(|(i, e)| (text(&e.args[0]).to_string(), (*i, e.name.clone())))
        .collect()
}

#[test]
fn datei_ist_vollstaendig_und_eindeutig() {
    for (name, t) in HAEUSER {
        let m = laden(t);
        let (ifc, b) = export(&m, &opt());
        let f = step::read(&ifc).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(f.schema(), Some("IFC4"), "{name}");
        assert!(f.twice.is_empty(), "{name}: doppelte Nummern {:?}", f.twice);
        // Jede Verweisung zeigt auf einen Datensatz
        for (i, e) in &f.data {
            let mut r = Vec::new();
            e.args.iter().for_each(|a| verweise(a, &mut r));
            for x in r {
                assert!(f.data.contains_key(&x), "{name}: #{i} {} → #{x}", e.name);
            }
        }
        // GlobalId: 22 Zeichen, lesbar, eindeutig
        let mut seen = HashSet::new();
        for e in f.data.values() {
            if let Some(V::Str(g)) = e.args.first() {
                if e.name.starts_with("IFCREL")
                    || e.name.ends_with("TYPE")
                    || matches!(
                        e.name.as_str(),
                        "IFCPROJECT"
                            | "IFCSITE"
                            | "IFCBUILDING"
                            | "IFCBUILDINGSTOREY"
                            | "IFCPROPERTYSET"
                    )
                {
                    assert!(sk_model::Guid::from_ifc(g).is_some(), "{name}: {g}");
                    assert!(seen.insert(g.clone()), "{name}: {g} doppelt");
                }
            }
        }
        // Jedes Bauteil mit Körper genau einmal, mit seiner Kennung
        let im = bauteile(&f);
        assert_eq!(im.len(), b.bauteile, "{name}");
        assert!(b.ausgelassen.is_empty(), "{name}: {:?}", b.ausgelassen);
        let solids = m.element_solids();
        assert_eq!(solids.len(), b.bauteile, "{name}");
        for (id, _) in &solids {
            let el = m.element(*id).unwrap();
            assert!(im.contains_key(&el.guid.to_ifc()), "{name}: {}", el.number);
        }
        // Jedes Bauteil in genau einem Geschoss
        let mut drin: HashMap<u32, usize> = HashMap::new();
        for (_, e) in f.all("IFCRELCONTAINEDINSPATIALSTRUCTURE") {
            assert_eq!(
                f.get(liste(&e.args[5])[0]).unwrap().name,
                "IFCBUILDINGSTOREY"
            );
            for x in liste(&e.args[4]) {
                *drin.entry(x).or_default() += 1;
            }
        }
        for (g, (i, _)) in &im {
            assert_eq!(drin.get(i), Some(&1), "{name}: {g}");
        }
        assert_eq!(f.all("IFCBUILDINGSTOREY").count(), b.geschosse, "{name}");
        assert_eq!(f.all("IFCPROJECT").count(), 1);
        assert_eq!(f.all("IFCSITE").count(), 1);
    }
}

#[test]
fn zwei_exporte_bytegleich_kennungen_bleiben() {
    for (name, t) in HAEUSER {
        let m = laden(t);
        let (a, _) = export(&m, &opt());
        let (b, _) = export(&m, &opt());
        assert!(a == b, "{name}: zwei Exporte unterscheiden sich");
        // Nach Speichern und Laden dieselben Kennungen (Rundlauf .szo)
        let m2 = laden(&szo::write(&m));
        let (c, _) = export(&m2, &opt());
        let ka: Vec<String> = bauteile(&step::read(&a).unwrap()).into_keys().collect();
        let kc: HashSet<String> = bauteile(&step::read(&c).unwrap()).into_keys().collect();
        assert!(ka.iter().all(|k| kc.contains(k)), "{name}");
    }
}

#[test]
fn klassen_wie_in_der_bauteilliste() {
    let m = laden(HAEUSER[5].1);
    let (ifc, _) = export(&m, &opt());
    let f = step::read(&ifc).unwrap();
    let im = bauteile(&f);
    let mut gesehen: BTreeMap<Category, String> = BTreeMap::new();
    for (id, _) in m.element_solids() {
        let el = m.element(id).unwrap();
        let soll = match &el.kind {
            ElementKind::Ext(_) => m.ext_def_of(id).unwrap().ifc().to_string(),
            _ => el.category.ifc_class().to_string(),
        };
        let z = sk_ifc::klassen::zuordnen(&soll);
        let (i, klasse) = &im[&el.guid.to_ifc()];
        assert_eq!(*klasse, z.klasse.name.to_ascii_uppercase(), "{}", el.number);
        let e = f.get(*i).unwrap();
        assert_eq!(text(&e.args[2]), el.number);
        assert_eq!(e.args[8], V::Enum(z.predefined.into()), "{}", el.number);
        gesehen.insert(el.category, klasse.clone());
    }
    // Das Haus mit allen Abschnitten zeigt die wichtigsten Arten
    for c in [
        Category::ExteriorWall,
        Category::GroundSlab,
        Category::StripFooting,
        Category::Floor,
        Category::Roof,
        Category::Parapet,
        Category::Coping,
        Category::Downpipe,
    ] {
        assert!(gesehen.contains_key(&c), "{c:?} fehlt im Prüfhaus");
    }
    // Attikablech: IFC4 kennt COPING noch nicht
    assert!(ifc.contains(",'COPING',"));
    // Umlaute nach ISO 10303-21
    assert!(ifc.contains("'Au\\X2\\00DF\\X0\\enwand'"));
}

#[test]
fn schichten_wie_im_bauteiltyp() {
    for (name, t) in HAEUSER {
        let m = laden(t);
        let (ifc, _) = export(&m, &opt());
        let f = step::read(&ifc).unwrap();
        let typen: HashMap<String, u32> = f
            .all("IFCWALLTYPE")
            .map(|(i, e)| (text(&e.args[2]).to_string(), i))
            .collect();
        // Typ → Aufbau über die Baustoffzuordnung des Typs
        let mut aufbau_von: HashMap<u32, u32> = HashMap::new();
        for (_, e) in f.all("IFCRELASSOCIATESMATERIAL") {
            for x in liste(&e.args[4]) {
                aufbau_von.insert(x, liste(&e.args[5])[0]);
            }
        }
        for (id, set) in m.layer_sets().iter() {
            let Some(&t) = typen.get(&set.name) else {
                continue;
            };
            let _ = id;
            let ls = f.get(aufbau_von[&t]).unwrap();
            assert_eq!(ls.name, "IFCMATERIALLAYERSET", "{name}");
            let schichten = liste(&ls.args[0]);
            assert_eq!(schichten.len(), set.layers.len(), "{name}: {}", set.name);
            for (l, &si) in set.layers.iter().zip(&schichten) {
                let e = f.get(si).unwrap();
                let V::Real(d) = e.args[1] else {
                    panic!("Dicke")
                };
                assert!((d - l.thickness).abs() < 1e-3, "{name}: {}", set.name);
                let mat = m.material(l.material).unwrap();
                let mi = liste(&e.args[0])[0];
                assert_eq!(text(&f.get(mi).unwrap().args[0]), mat.name);
            }
        }
    }
}

/// Geschlossene Wandkörper haben den Rauminhalt aus den Mengen (Schichten
/// ohne Luft), im Haus mit zweischaligem Mauerwerk und im Haus mit allen
/// Abschnitten. In den Häusern mit Rücksprung zählen Ansicht und Mengen ein
/// Stück Außenschale über der Decke zum Teil verschiedenen Wänden zu
/// (Befund an Geometrie, ifc/plan.md §8); dort wird nicht verglichen.
#[test]
fn rauminhalt_wie_mengen() {
    let mut geprueft = 0;
    for (name, t) in HAEUSER {
        if !matches!(name, "rh2" | "r7") {
            continue;
        }
        let m = laden(t);
        for (id, s) in m.element_solids() {
            let Some((run, seg)) = m.segment_of(id) else {
                continue;
            };
            let netze = geometrie::netze(&s, Default::default());
            assert!(
                netze.iter().all(geometrie::geschlossen),
                "{name}: {} offen",
                m.element(id).unwrap().number
            );
            let v: f64 = netze.iter().map(geometrie::volumen).sum();
            let q = run_qto(&m, run)[seg].volume;
            assert!(
                (v - q).abs() <= q.abs() * 1e-3,
                "{name}: {} {v} statt {q}",
                m.element(id).unwrap().number
            );
            geprueft += 1;
        }
    }
    assert!(geprueft >= 20, "nur {geprueft} geprüft");
}

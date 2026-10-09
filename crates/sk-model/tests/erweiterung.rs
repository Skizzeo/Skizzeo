//! Erweiterungsbauteile im Modell und in der Projektdatei (Schrittplan E3):
//! Definition und Exemplare speichern und öffnen, Nummern je Präfix,
//! Rückgängig, Löschen, Sperren, Höhe im Obergeschoss, Robustheit beim
//! Öffnen fremder Dateien.

use sk_model::erweiterung::{ExtDef, ExtPart};
use sk_model::txn::Direction;
use sk_model::{szo, Category, ElementId, ElementKind, ExtError, GuidGen, Model, StoreyId};

const BEISPIELE: [(&str, &str); 5] = [
    (
        "werk.bodenplatte",
        include_str!("../../sk-szb/beispiele/werk.bodenplatte.szb"),
    ),
    (
        "werk.stabgelaender",
        include_str!("../../sk-szb/beispiele/werk.stabgelaender.szb"),
    ),
    (
        "werk.streifenfundament",
        include_str!("../../sk-szb/beispiele/werk.streifenfundament.szb"),
    ),
    (
        "werk.stuetze",
        include_str!("../../sk-szb/beispiele/werk.stuetze.szb"),
    ),
    (
        "werk.treppe",
        include_str!("../../sk-szb/beispiele/werk.treppe.szb"),
    ),
];

fn def(text: &str) -> ExtDef {
    ExtDef::lesen(text).unwrap_or_else(|e| panic!("{e}"))
}

fn geschoss(m: &Model, kurz: &str) -> StoreyId {
    m.storeys()
        .iter()
        .find(|(_, s)| s.short == kurz && s.building.is_some())
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Geschoss {kurz}"))
}

/// Projekt mit einem Gebäude (GR, EG, OG) und allen fünf Beispielen.
fn projekt() -> Model {
    let mut m = Model::with_seed(1);
    m.add_building(2);
    for (_, t) in BEISPIELE {
        m.put_ext_def(def(t)).unwrap();
    }
    m
}

fn setzen(m: &mut Model, key: &str, kurz: &str, at: [f64; 2]) -> ElementId {
    let s = geschoss(m, kurz);
    let p = ExtPart::new(m.ext_def(key).unwrap(), at);
    m.add_ext(s, p).unwrap()
}

fn lesen(text: &str) -> szo::Loaded {
    szo::read(text, GuidGen::with_seed(7)).unwrap_or_else(|e| panic!("{e:?}"))
}

#[test]
fn speichern_und_oeffnen_behaelt_alles() {
    let mut m = projekt();
    let a = setzen(&mut m, "werk.stuetze", "EG", [1000.0, 2000.0]);
    let b = setzen(&mut m, "werk.stuetze", "OG", [-500.5, 0.25]);
    let mut p = match &m.element(b).unwrap().kind {
        ElementKind::Ext(p) => p.clone(),
        _ => unreachable!(),
    };
    p.rot = 33.5;
    p.typ = None;
    p.set("b", 365.0);
    p.set("h", 2500.0);
    assert!(m.set_ext(b, p.clone()));
    for (key, _) in BEISPIELE {
        setzen(&mut m, key, "EG", [5000.0, 5000.0]);
    }
    let text = szo::write(&m);
    assert!(text.contains("[extdef] key=werk.stuetze version=1 text=\"SZB 0\\n"));
    assert!(text.contains("[extpart] guid="));
    let l = lesen(&text);
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert_eq!(szo::write(&l.model), text, "bytegleich");
    assert_eq!(l.model.ext_defs(), m.ext_defs());
    let nummern = |m: &Model| -> Vec<(String, Category)> {
        let mut v: Vec<_> = m
            .elements()
            .iter()
            .filter(|(_, e)| e.category == Category::Extension)
            .map(|(_, e)| (e.number.clone(), e.category))
            .collect();
        v.sort();
        v
    };
    assert_eq!(nummern(&l.model), nummern(&m));
    let g = m.element(b).unwrap().guid;
    let (nb, eb) = l
        .model
        .elements()
        .iter()
        .find(|(_, e)| e.guid == g)
        .unwrap();
    assert_eq!(eb.kind, ElementKind::Ext(p));
    assert_eq!(
        l.model.ext_ergebnis(nb).unwrap(),
        m.ext_ergebnis(b).unwrap()
    );
    assert_eq!(m.element(a).unwrap().number, "ST-001");
    assert_eq!(m.element(b).unwrap().number, "ST-002");
    assert!(m.check().is_empty(), "{:?}", m.check());
    assert!(l.model.check().is_empty(), "{:?}", l.model.check());
}

/// Texte der Definition mit `\"`, `\\` und `#` (Test-Thread, Plan E3).
#[test]
fn texte_mit_sonderzeichen() {
    let t = BEISPIELE[3].1.replace(
        "name=\"Stahlbetonstütze\"",
        "name=\"Stütze \\\"rund\\\" \\\\ # nicht Kommentar\"",
    );
    assert_ne!(t, BEISPIELE[3].1, "Ersetzung muss greifen");
    let d = def(&t);
    assert_eq!(d.name(), "Stütze \"rund\" \\ # nicht Kommentar");
    let mut m = Model::new();
    m.add_building(1);
    m.put_ext_def(d).unwrap();
    let id = setzen(&mut m, "werk.stuetze", "EG", [0.0, 0.0]);
    let text = szo::write(&m);
    let l = lesen(&text);
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert_eq!(l.model.ext_defs()[0].text, t);
    assert_eq!(l.model.ext_defs()[0].name(), m.ext_defs()[0].name());
    assert_eq!(szo::write(&l.model), text);
    assert!(m.ext_ergebnis(id).is_some());
}

/// Höhe des Einfügepunkts im OG wie die Werkbank (Sollwerte z0).
#[test]
fn hoehe_im_obergeschoss() {
    let mut m = projekt();
    for (key, eg, og) in [
        ("werk.bodenplatte", -200.0, 2655.0),
        ("werk.streifenfundament", -800.0, 2055.0),
        ("werk.stuetze", 0.0, 2855.0),
    ] {
        for (kurz, soll) in [("EG", eg), ("OG", og)] {
            let s = geschoss(&m, kurz);
            m.set_storey_height(s, 2855.0);
            let id = setzen(&mut m, key, kurz, [0.0, 0.0]);
            let g = m.ext_geschoss(s);
            assert_eq!((g.gh, g.decke), (2855.0, 220.0), "{kurz}");
            let uk = m.storey(s).unwrap().elevation;
            let e = m.ext_ergebnis(id).unwrap();
            assert_eq!(uk + e.z0, soll, "{key} {kurz}");
        }
    }
}

#[test]
fn nummern_je_praefix_nie_wieder() {
    let mut m = projekt();
    let a = setzen(&mut m, "werk.stuetze", "EG", [0.0, 0.0]);
    let b = setzen(&mut m, "werk.stuetze", "EG", [0.0, 0.0]);
    let c = setzen(&mut m, "werk.treppe", "EG", [0.0, 0.0]);
    let nr = |m: &Model, id| m.element(id).unwrap().number.clone();
    assert_eq!(nr(&m, a), "ST-001");
    assert_eq!(nr(&m, b), "ST-002");
    let tr = m.ext_def("werk.treppe").unwrap().prefix().to_string();
    assert_eq!(nr(&m, c), format!("{tr}-001"));
    m.begin("Löschen");
    let d = m.delete_elements(&[b]);
    assert_eq!(d.removed, [b]);
    assert_eq!(d.kinds, [Category::Extension]);
    m.commit().unwrap();
    // Nach dem Speichern und Öffnen bleibt ST-002 vergeben
    let text = szo::write(&m);
    assert!(
        text.contains("next=ST:2"),
        "{}",
        text.lines().find(|l| l.starts_with("[project]")).unwrap()
    );
    let mut l = lesen(&text).model;
    let e = setzen(&mut l, "werk.stuetze", "EG", [0.0, 0.0]);
    assert_eq!(nr(&l, e), "ST-003");
}

#[test]
fn rueckgaengig_sperren_loeschen() {
    let mut m = Model::new();
    m.add_building(1);
    let eg = geschoss(&m, "EG");
    m.begin("Einlesen");
    m.put_ext_def(def(BEISPIELE[3].1)).unwrap();
    let t1 = m.commit().unwrap();
    m.begin("Setzen");
    let id = m
        .add_ext(
            eg,
            ExtPart::new(m.ext_def("werk.stuetze").unwrap(), [0.0, 0.0]),
        )
        .unwrap();
    let t2 = m.commit().unwrap();
    m.apply(&t2, Direction::Undo);
    assert!(m.element(id).is_none());
    m.apply(&t1, Direction::Undo);
    assert!(m.ext_defs().is_empty());
    m.apply(&t1, Direction::Redo);
    m.apply(&t2, Direction::Redo);
    assert_eq!(m.element(id).unwrap().number, "ST-001");
    // Gesperrt: Ändern und Löschen werden abgewiesen
    m.begin("Sperren");
    m.set_locked(&[id], true);
    m.commit().unwrap();
    m.begin("Ändern");
    let mut p = match &m.element(id).unwrap().kind {
        ElementKind::Ext(p) => p.clone(),
        _ => unreachable!(),
    };
    p.at = [10.0, 0.0];
    m.set_ext(id, p.clone());
    assert!(m.try_commit().is_err());
    assert!(m.can_delete(id).is_err());
    m.begin("Entsperren");
    m.set_locked(&[id], false);
    m.commit().unwrap();
    m.begin("Ändern");
    m.set_ext(id, p.clone());
    m.commit().unwrap();
    assert_eq!(m.element(id).unwrap().kind, ElementKind::Ext(p));
    // Eine Definition mit Exemplaren bleibt
    assert!(!m.remove_ext_def("werk.stuetze"));
}

#[test]
fn praefix_doppelt_abgewiesen() {
    let mut m = projekt();
    let fremd = BEISPIELE[3]
        .1
        .replace("key=werk.stuetze", "key=test.stuetze");
    assert_eq!(
        m.put_ext_def(def(&fremd)),
        Err(ExtError::PrefixTaken {
            prefix: "ST".into(),
            by: "werk.stuetze".into()
        })
    );
    let s = geschoss(&m, "EG");
    let p = ExtPart {
        key: "fehlt.ganz".into(),
        at: [0.0, 0.0],
        rot: 0.0,
        typ: None,
        werte: Vec::new(),
    };
    assert_eq!(m.add_ext(s, p), Err(ExtError::NoDef("fehlt.ganz".into())));
}

/// Eine Projektdatei, deren Definition fehlt oder unlesbar ist, öffnet mit
/// Hinweis; Definition, Exemplare und ihre Eigenschaften bleiben roh und
/// werden unverändert geschrieben.
#[test]
fn oeffnen_mit_unlesbarer_definition() {
    let mut m = projekt();
    let id = setzen(&mut m, "werk.stuetze", "EG", [0.0, 0.0]);
    setzen(&mut m, "werk.treppe", "EG", [0.0, 0.0]);
    m.set_prop(
        id,
        "Hinweis",
        Some(sk_model::PropValue::Text("innen".into())),
    );
    let gut = szo::write(&m);
    let kaputt: String = gut
        .lines()
        .map(|l| match l.starts_with("[extdef] key=werk.stuetze ") {
            true => l.replace("[koerper] form=", "[koerper] form=kugel"),
            false => l.to_string(),
        })
        .map(|l| format!("{l}\n"))
        .collect();
    assert_ne!(kaputt, gut);
    let l = lesen(&kaputt);
    assert!(
        l.hints
            .iter()
            .any(|h| h.contains("Erweiterung „werk.stuetze“ nicht lesbar")),
        "{:?}",
        l.hints
    );
    assert!(l.model.ext_def("werk.stuetze").is_none());
    assert!(l.model.ext_def("werk.treppe").is_some());
    assert_eq!(
        l.model.ext_raw().len(),
        3,
        "Definition, Exemplar, Eigenschaft"
    );
    let neu = szo::write(&l.model);
    let zeilen = |t: &str| {
        let mut v: Vec<String> = t.lines().map(str::to_string).collect();
        v.sort();
        v
    };
    assert_eq!(zeilen(&neu), zeilen(&kaputt), "alles bleibt");
    assert_eq!(szo::write(&lesen(&neu).model), neu, "stabil");
    // Exemplar ohne Definition in der Datei
    let ohne: String = gut
        .lines()
        .filter(|l| !l.starts_with("[extdef] key=werk.treppe"))
        .map(|l| format!("{l}\n"))
        .collect();
    let l = lesen(&ohne);
    assert!(
        l.hints
            .iter()
            .any(|h| h.contains("Erweiterung „werk.treppe“ fehlt")),
        "{:?}",
        l.hints
    );
    assert_eq!(zeilen(&szo::write(&l.model)), zeilen(&ohne));
}

struct Lcg(u64);

impl Lcg {
    fn bis(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) % n.max(1) as u64) as usize
    }
}

/// Review Nr. 15: beschädigte Definitionen und Exemplare über den
/// .szo-Weg. Öffnen gelingt mit Hinweis oder weist sauber ab; nie ein
/// Absturz, und was öffnet, schreibt sich stabil.
#[test]
fn beschaedigte_projektdateien() {
    let mut m = projekt();
    for (key, _) in BEISPIELE {
        setzen(&mut m, key, "EG", [0.0, 0.0]);
        setzen(&mut m, key, "OG", [100.0, 0.0]);
    }
    let gut = szo::write(&m);
    let zeilen: Vec<&str> = gut.lines().collect();
    let ext: Vec<usize> = (0..zeilen.len())
        .filter(|&i| zeilen[i].starts_with("[ext"))
        .collect();
    let ersatz = [
        "0", "-1", "1e308", "NaN", "inf", "", "\\\"", "ä", "((((", "1/0", "500", "501", "x=1",
    ];
    let mut r = Lcg(31);
    let mut geoeffnet = 0;
    for _ in 0..300 {
        let mut z: Vec<String> = zeilen.iter().map(|s| s.to_string()).collect();
        for _ in 0..1 + r.bis(3) {
            let i = ext[r.bis(ext.len())];
            let l = &z[i];
            let ks: Vec<usize> = l.char_indices().map(|(k, _)| k).collect();
            let k = ks[r.bis(ks.len())];
            let neu = match r.bis(4) {
                0 => format!("{}{}{}", &l[..k], ersatz[r.bis(ersatz.len())], &l[k..]),
                1 => l[..k].to_string(),
                2 => {
                    let ende = ks.get(r.bis(ks.len())).copied().unwrap_or(l.len()).max(k);
                    format!("{}{}", &l[..k], &l[ende..])
                }
                _ => l.replacen(
                    ["=1", "=0", "\\n", "b=", "h="][r.bis(5)],
                    ersatz[r.bis(ersatz.len())],
                    1,
                ),
            };
            z[i] = neu;
        }
        let text = z.join("\n") + "\n";
        if let Ok(l) = szo::read(&text, GuidGen::with_seed(3)) {
            geoeffnet += 1;
            let a = szo::write(&l.model);
            let b = szo::write(&szo::read(&a, GuidGen::with_seed(3)).unwrap().model);
            assert_eq!(a, b);
            for (id, e) in l.model.elements().iter() {
                if e.category == Category::Extension {
                    let e = l.model.ext_ergebnis(id).unwrap();
                    assert!(e.vol.values().all(|v| v.is_finite()));
                }
            }
        }
    }
    assert!(geoeffnet > 100, "{geoeffnet}");
}

/// Test-Thread (Abnahme E3): Sperre und Ausblenden eines Bauteils, dessen
/// Erweiterung nicht lesbar ist, bleiben wie das Bauteil in der Datei.
#[test]
fn sperre_und_ausblenden_ohne_lesbare_definition() {
    let mut m = projekt();
    let id = setzen(&mut m, "werk.stuetze", "EG", [0.0, 0.0]);
    let g = m.element(id).unwrap().guid;
    m.set_locked(&[id], true);
    let mut v = m.visibility().clone();
    v.hidden.insert(g);
    m.set_visibility(v);
    let gut = szo::write(&m);
    assert!(gut.contains("[lock] elem=") && gut.contains("[hide] elem="));
    let kaputt: String = gut
        .lines()
        .map(|l| match l.starts_with("[extdef] key=werk.stuetze ") {
            true => l.replace("[koerper] form=", "[koerper] form=kugel"),
            false => l.to_string(),
        })
        .map(|l| format!("{l}\n"))
        .collect();
    let l = lesen(&kaputt);
    assert!(
        !l.hints.iter().any(|h| h.contains("Sperre auf unbekanntes")),
        "{:?}",
        l.hints
    );
    let neu = szo::write(&l.model);
    let zeilen = |t: &str| {
        let mut v: Vec<String> = t.lines().map(str::to_string).collect();
        v.sort();
        v
    };
    assert_eq!(zeilen(&neu), zeilen(&kaputt), "alles bleibt");
    let wieder: String = neu
        .lines()
        .map(|z| match z.starts_with("[extdef] key=werk.stuetze ") {
            true => z.replace("form=kugelquader", "form=quader"),
            false => z.to_string(),
        })
        .map(|z| format!("{z}\n"))
        .collect();
    let l = lesen(&wieder);
    let (id, _) = l
        .model
        .elements()
        .iter()
        .find(|(_, e)| e.guid == g)
        .expect("Bauteil wieder da");
    assert!(l.model.element(id).unwrap().locked, "gesperrt");
    assert!(l.model.visibility().hidden.contains(&g), "ausgeblendet");
}

/// Eigener Baustoff einer Definition wird ein Baustoff des Projekts mit
/// abgeleiteter Kennung (E8c): einmal, auch beim erneuten Einlesen, weg
/// mit Rückgängig, und nach Speichern und Öffnen derselbe.
#[test]
fn eigener_baustoff_im_projekt() {
    let mut m = Model::new();
    m.add_building(1);
    let vorher = m.materials().len();
    let g = sk_model::erweiterung::kennung("werk.stabgelaender", "baustoff", "stahl_s235");
    m.begin("Einlesen");
    m.put_ext_def(def(BEISPIELE[1].1)).unwrap();
    let t = m.commit().unwrap();
    assert_eq!(m.materials().len(), vorher + 1);
    let (id, b) = m.materials().iter().find(|(_, b)| b.guid == g).unwrap();
    assert_eq!(b.name, "Baustahl S235, verzinkt");
    assert_eq!(b.category, sk_model::MatCategory::Metal);
    assert_eq!(b.density, 7850.0);
    let d = m.ext_def("werk.stabgelaender").unwrap().clone();
    assert_eq!(m.ext_material(&d, "stahl_s235"), Some(id));
    // Erneut einlesen: kein zweiter Baustoff
    m.begin("Einlesen");
    m.put_ext_def(def(BEISPIELE[1].1)).unwrap();
    m.commit();
    assert_eq!(m.materials().len(), vorher + 1);
    // Speichern und Öffnen
    let l = lesen(&szo::write(&m));
    let (id2, _) = l
        .model
        .materials()
        .iter()
        .find(|(_, b)| b.guid == g)
        .unwrap();
    assert_eq!(l.model.ext_material(&d, "stahl_s235"), Some(id2));
    // Rückgängig
    m.apply(&t, Direction::Undo);
    assert_eq!(m.materials().len(), vorher);
    assert!(m.ext_defs().is_empty());
}

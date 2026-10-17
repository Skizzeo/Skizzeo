//! Tests des Formats .szo (aus szo.rs, Paket T3).

use super::*;
use crate::element::PropSet;
use crate::model::Location;
use crate::qto::run_qto;

/// Haus 10 × 8 m, eine offene Wand, umbenannte Nummer, Eigenschaften,
/// eigener Stift, Schraffur mit zwei Linienscharen, eigene Oberfläche.
pub(super) fn house() -> Model {
    let mut m = Model::with_seed(1);
    let set = m.defaults().exterior_wall;
    let rect = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.eg_at(2750.0);
    let r = m
        .add_wall_run(&rect, true, RefSide::Left, eg, set, Category::ExteriorWall)
        .unwrap();
    let line = [vec3(12000.0, 0.0, 0.0), vec3(12000.0, 4500.25, 0.0)];
    let eg = m.eg_at(2750.0);
    m.add_wall_run(
        &line,
        false,
        RefSide::Center,
        eg,
        set,
        Category::ExteriorWall,
    )
    .unwrap();
    let w = m.run(r).unwrap().segments[1];
    m.set_number(w, "AW-Nord \"alt\"").unwrap();
    m.set_prop(w, "Brandschutz", Some(PropValue::Text("F90".into())));
    m.set_prop(w, "U-Wert", Some(PropValue::Number(0.21)));
    m.set_prop(w, "Tragend", Some(PropValue::Bool(true)));
    let (pid, pen) = m
        .attr()
        .pens()
        .iter()
        .next()
        .map(|(i, p)| (i, p.clone()))
        .unwrap();
    m.set_pen(
        pid,
        Pen {
            color: [12, 34, 200],
            width_mm: 0.35,
            ..pen
        },
    );
    let guid = m.new_guid();
    m.add_fill(Fill {
        guid,
        name: "Kreuz".into(),
        kind: FillKind::Lines(vec![
            HatchLine::solid(45.0, 2.5, 0.0),
            HatchLine {
                dash_mm: 1.0,
                gap_mm: 0.5,
                ..HatchLine::solid(-45.0, 2.5, 1.25)
            },
        ]),
        space: FillSpace::Paper,
    });
    let guid = m.new_guid();
    m.add_surface(Surface {
        guid,
        name: "Klinker rot".into(),
        color: [160, 60, 40],
        cut_color: [200, 90, 70],
        pattern: None,
    });
    m
}

fn load(text: &str) -> Result<Loaded, LoadError> {
    read(text, GuidGen::with_seed(99))
}

/// A310 (Vorankündigung KA-0, Umbenennung Gasbeton → Porenbeton): Eine
/// Altdatei mit „Gasbeton“ unter zeitbasierter Guid und ohne λ bekommt
/// weiter das λ des Werksbaustoffs; der Name in der Datei bleibt.
#[test]
fn a310_altdatei_gasbeton_bekommt_lambda() {
    let m = Model::with_seed(1);
    let werk = m
        .materials()
        .iter()
        .find(|(_, x)| x.name == "Gasbeton" || x.name == "Porenbeton")
        .map(|(_, x)| x.clone())
        .expect("Werksbaustoff Gas- oder Porenbeton");
    let lambda = werk.lambda.expect("Werks-λ");
    let neu = write(&m);
    let g = werk.guid.to_ifc();
    let other = if g.ends_with('A') { 'B' } else { 'A' };
    let g2 = format!("{}{other}", &g[..g.len() - 1]);
    let mut hit = 0;
    let alt: String = neu
        .replace(&g, &g2)
        .lines()
        .map(|l| {
            if !(l.starts_with("[material] ") && l.contains(&format!("guid={g2}"))) {
                return format!("{l}\n");
            }
            hit += 1;
            let l = l.replacen(&format!("name=\"{}\"", werk.name), "name=\"Gasbeton\"", 1);
            let i = l.find("lambda=").unwrap();
            let j = l[i..].find(' ').map_or(l.len(), |j| i + j);
            format!("{}lambda=-{}\n", &l[..i], &l[j..])
        })
        .collect();
    assert_eq!(hit, 1, "{neu}");
    assert!(alt.contains("name=\"Gasbeton\""), "{alt}");
    let got = load(&alt).unwrap().model;
    let (_, x) = got
        .materials()
        .iter()
        .find(|(_, x)| x.name == "Gasbeton" && x.guid != werk.guid)
        .expect("Altbaustoff bleibt „Gasbeton“");
    assert_eq!(x.lambda, Some(lambda));
}

/// E15/E3b: Schraffuren vor E15 (Winkel im Uhrzeigersinn, Stahlbeton
/// gekreuzt) werden umgerechnet bzw. ersetzt; Guid und Verweise bleiben.
#[test]
fn alte_schraffuren_werden_umgestellt() {
    let m = Model::with_seed(15);
    let neu = write(&m);
    assert!(
        neu.contains("lines=135:2.54:0:0:0;135:2.54:1.27:1.5:0.75 ccw=1"),
        "{neu}"
    );
    let alt: String = neu
        .lines()
        .map(|l| {
            let l = l.replace(" ccw=1", "");
            let l = l.replace("lines=135:1.27:0:0:0", "lines=45:1.27:0");
            let l = l.replace(
                "lines=135:2.54:0:0:0;135:2.54:1.27:1.5:0.75",
                "lines=45:1.27:0;135:1.27:0",
            );
            format!("{l}\n")
        })
        .collect();
    assert!(!alt.contains("ccw") && alt.contains("lines=45:1.27:0;135:1.27:0"));
    let l = load(&alt).unwrap();
    assert_eq!(
        l.hints,
        ["Stahlbeton-Schraffur auf gestrichelte Diagonale umgestellt"]
    );
    assert_eq!(write(&l.model), neu, "gleich einer neuen Datei");
    // Eine selbst geänderte Kreuzschraffur wird nur umgerechnet
    let eigen = alt.replace("lines=45:1.27:0;135:1.27:0", "lines=30:2:0;120:2:0.5");
    let l = load(&eigen).unwrap();
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    let (_, f) = l
        .model
        .attr()
        .fills()
        .iter()
        .find(|(_, f)| f.name == "Stahlbeton")
        .unwrap();
    assert_eq!(
        f.kind,
        FillKind::Lines(vec![
            HatchLine::solid(150.0, 2.0, 0.0),
            HatchLine::solid(60.0, 2.0, 0.5)
        ])
    );
}

/// Ab zwei Obergeschossen heißen sie „1. OG“, „2. OG“: das Kürzel hat ein
/// Leerzeichen und muss in Anführungszeichen stehen.
#[test]
fn gebaeude_mit_drei_obergeschossen_laedt_wieder() {
    let mut m = Model::with_seed(12);
    m.add_building(4);
    let a = write(&m);
    assert!(a.contains("short=\"2. OG\""), "{a}");
    let l = load(&a).unwrap();
    assert_eq!(write(&l.model), a);
}

/// Schnitte A und B: Lage und Blickrichtung stehen in der Datei, nie
/// gezeigte fehlen (Datei bleibt bytegleich), Unbekanntes wird gemeldet.
#[test]
fn schnitte_in_der_datei() {
    let mut m = Model::with_seed(16);
    let leer = write(&m);
    assert!(!leer.contains("[cut]"));
    m.set_cut(
        0,
        crate::model::Cut {
            pos: Some(4000.0),
            flip: false,
        },
    );
    m.set_cut(
        1,
        crate::model::Cut {
            pos: Some(5000.5),
            flip: true,
        },
    );
    let a = write(&m);
    assert!(a.contains("[cut] name=A pos=4000 flip=0"), "{a}");
    assert!(a.contains("[cut] name=B pos=5000.5 flip=1"), "{a}");
    let l = load(&a).unwrap();
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert_eq!(l.model.cuts(), m.cuts());
    assert_eq!(l.model.active_cut(), 0);
    assert_eq!(write(&l.model), a);
    // Zuletzt gezeigt: B
    m.set_active_cut(1);
    let b = write(&m);
    assert!(b.contains("[cut] name=B pos=5000.5 flip=1 active=1"), "{b}");
    let l = load(&b).unwrap();
    assert_eq!(l.model.active_cut(), 1);
    assert_eq!(write(&l.model), b);
    let l = load(&a.replace("name=B", "name=C")).unwrap();
    assert!(l.hints.iter().any(|h| h.contains("„C“")), "{:?}", l.hints);
    assert_eq!(l.model.cuts()[1], crate::model::Cut::default());
}

#[test]
fn speichern_oeffnen_speichern_gibt_dieselben_bytes() {
    let m = house();
    let a = write(&m);
    let l = load(&a).unwrap();
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert_eq!(write(&l.model), a);
    assert!(a.starts_with("SZO 4\n"));
    assert!(a.contains("number=\"AW-Nord \\\"alt\\\"\""), "{a}");
    assert!(l.model.check().is_empty());
    assert_eq!(l.model.project(), m.project());
}

#[test]
fn geladenes_modell_gleicht_dem_gespeicherten() {
    let m = house();
    let l = load(&write(&m)).unwrap().model;
    let g = |m: &Model| -> Vec<(Guid, String, PropSet)> {
        let mut v: Vec<_> = m
            .elements()
            .iter()
            .map(|(_, e)| (e.guid, e.number.clone(), e.props.clone()))
            .collect();
        v.sort_by_key(|x| x.0);
        v
    };
    assert_eq!(g(&l), g(&m));
    let pen = |m: &Model| -> Vec<Pen> {
        let mut v: Vec<Pen> = m.attr().pens().iter().map(|(_, p)| p.clone()).collect();
        v.sort_by_key(|p| p.guid);
        v
    };
    assert_eq!(pen(&l), pen(&m));
    assert!(l.attr().fills().iter().any(|(_, f)| f.name == "Kreuz"
        && matches!(&f.kind, FillKind::Lines(ls) if ls.len() == 2 && ls[1].offset_mm == 1.25)));
    assert!(l
        .attr()
        .surfaces()
        .iter()
        .any(|(_, s)| s.name == "Klinker rot" && s.cut_color == [200, 90, 70]));
    // Rechteck: dieselben Mengen wie vor dem Speichern
    let r = l
        .runs()
        .iter()
        .find(|(id, r)| r.closed && l.run_below(*id).is_none())
        .map(|(id, _)| id)
        .unwrap();
    let vol: f64 = run_qto(&l, r).iter().map(|w| w.volume).sum();
    assert!((vol / 1e9 - 28.7776).abs() < 1e-4, "{}", vol / 1e9);
    let open = l.runs().iter().find(|(_, r)| !r.closed).unwrap().1;
    assert_eq!(open.points[1].y, 4500.25);
}

/// Baustoffe sehen nach dem Laden gleich aus (Schraffur, Stifte, Oberfläche).
#[test]
fn baustoffe_behalten_ihr_aussehen() {
    let m = house();
    let l = load(&write(&m)).unwrap().model;
    type Look = (
        Guid,
        Option<Fill>,
        Option<Pen>,
        Option<Pen>,
        Option<Surface>,
    );
    let look = |m: &Model| -> Vec<Look> {
        let a = m.attr();
        let mut v: Vec<_> = m
            .materials()
            .iter()
            .map(|(_, x)| {
                (
                    x.guid,
                    a.fill(x.cut_fill).cloned(),
                    a.pen(x.cut_fg).cloned(),
                    a.pen(x.cut_bg).cloned(),
                    a.surface(x.surface).cloned(),
                )
            })
            .collect();
        v.sort_by_key(|x| x.0);
        v
    };
    assert_eq!(look(&l), look(&m));
    let shown = |m: &Model| -> Vec<(Guid, Guid)> {
        let a = m.attr();
        let d = a.display();
        d.drawing
            .iter()
            .chain(&d.model3d)
            .chain([&d.ground, &d.section_line, &d.section_ends, &d.background])
            .map(|s| {
                (
                    a.pen(s.pen).unwrap().guid,
                    a.line_type(s.line_type).unwrap().guid,
                )
            })
            .collect()
    };
    assert_eq!(shown(&l), shown(&m));
    assert_eq!(l.attr().display().paper, m.attr().display().paper);
}

#[test]
fn naechste_nummer_nach_dem_laden() {
    let m = house();
    let mut l = load(&write(&m)).unwrap().model;
    let set = l.defaults().exterior_wall;
    let pts = [vec3(0.0, -5000.0, 0.0), vec3(3000.0, -5000.0, 0.0)];
    let eg = l.eg_at(2750.0);
    let r = l
        .add_wall_run(&pts, false, RefSide::Left, eg, set, Category::ExteriorWall)
        .unwrap();
    let w = l.run(r).unwrap().segments[0];
    // AW-001..AW-009 vergeben (EG, OG, gerade Wand; AW-002 umbenannt),
    // weiter mit AW-010
    assert_eq!(l.element(w).unwrap().number, "AW-010");
    let mut taken: Vec<Guid> = l.elements().iter().map(|(_, e)| e.guid).collect();
    taken.sort();
    taken.dedup();
    assert_eq!(taken.len(), l.elements().len());
}

/// F-17b: Ein unbekannter Abschnitt bleibt ohne Hinweis roh erhalten
/// (am Ende), ein unbekannter Schlüssel mit Hinweis an seiner Zeile;
/// ohne beides schreibt das Modell wie vorher.
#[test]
fn unbekannter_abschnitt_und_schluessel_bleiben() {
    let a = write(&house());
    let b = a.replacen("[storey]", "[zukunft] x=1\n[storey] neu=\"ja\"", 1);
    let l = load(&b).unwrap();
    assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
    assert!(l.hints[0].contains("„neu“"));
    let c = write(&l.model);
    let storey = b.lines().find(|x| x.starts_with("[storey]")).unwrap();
    assert!(c.lines().any(|x| x == storey), "{c}");
    assert!(c.ends_with("[zukunft] x=1\n"), "{c}");
    assert_eq!(write_known(&l.model), a);
    assert_eq!(write(&load(&c).unwrap().model), c, "zweiter Rundlauf");
}

/// Review 3n: Fremde Schlüssel an [matprop]-Zeilen bleiben beim
/// richtigen Baustoff, auch wenn eine neuere Fassung die Zeilen anders
/// ordnet (Satzkennung mit `mat=`).
#[test]
fn fremder_kennwert_bleibt_beim_baustoff() {
    let mut m = house();
    let ids: Vec<_> = m
        .materials()
        .iter()
        .filter(|(_, x)| crate::matprop::price_unit(x.category).is_some())
        .map(|(id, _)| id)
        .take(2)
        .collect();
    assert_eq!(ids.len(), 2);
    for (i, id) in ids.iter().enumerate() {
        let mut x = m.material(*id).unwrap().clone();
        x.props
            .insert("Richtpreis".into(), PropValue::Number(10.0 + i as f64));
        assert!(m.set_material(*id, x));
    }
    let a = write(&m);
    let rows: Vec<&str> = a.lines().filter(|l| l.starts_with("[matprop]")).collect();
    assert_eq!(rows.len(), 2, "{a}");
    // Getauscht, die zweite Zeile mit fremdem Schlüssel
    let marked = format!("{} zukunft=1", rows[1]);
    let b = a
        .replacen(rows[0], "@@", 1)
        .replacen(rows[1], rows[0], 1)
        .replacen("@@", &marked, 1);
    let c = write(&load(&b).unwrap().model);
    assert!(c.lines().any(|l| l == marked), "{c}");
    assert!(c.lines().any(|l| l == rows[0]), "{c}");
}

#[test]
fn kaputter_verweis_nennt_die_zeile() {
    let a = write(&house());
    let (i, line) = a
        .lines()
        .enumerate()
        .find(|(_, l)| l.starts_with("[wall]"))
        .unwrap();
    let run = line
        .split(" run=")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap();
    let b = a.replacen(&format!(" run={run}"), " run=0000000000000000000000", 1);
    let e = load(&b).err().unwrap();
    assert_eq!(e.line, i + 1, "{e}");
    assert!(e.message.contains("unbekannte Guid"), "{e}");
}

#[test]
fn neuere_version_wird_nicht_geoeffnet() {
    let a = write(&house()).replacen("SZO 4", "SZO 5", 1);
    let e = load(&a).err().unwrap();
    assert_eq!(e.line, 1);
    assert!(e.to_string().contains("neuerer Skizzeo-Version"), "{e}");
    assert!(load("Hallo").is_err());
    assert!(load("").is_err());
}

#[test]
fn fehlender_slot_bekommt_den_startwert() {
    let m = house();
    let a = write(&m);
    let b: String = a
        .lines()
        .filter(|l| !l.starts_with("[display] slot=section_line "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_ne!(a, b);
    let l = load(&b).unwrap();
    assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
    assert!(l.hints[0].contains("section_line"));
    assert_eq!(write(&l.model), a);
}

#[test]
fn doppelte_guid_und_falsche_segmente_sind_fehler() {
    let a = write(&house());
    let first = a.lines().find(|l| l.starts_with("[storey]")).unwrap();
    let b = format!("{a}{first}\n");
    let e = load(&b).err().unwrap();
    assert!(e.message.contains("doppelt"), "{e}");
    let c: String = {
        let mut skipped = false;
        a.lines()
            .filter(|l| {
                // eine OG-Wand: auf sie verweist keine Kopplung
                let drop = !skipped && l.starts_with("[wall]") && l.contains(" below=");
                skipped |= drop;
                !drop
            })
            .map(|l| format!("{l}\n"))
            .collect()
    };
    let e = load(&c).err().unwrap();
    assert!(e.message.contains("Segmente"), "{e}");
}

/// Doppelte Punkte und Schichtdicke 0 laden, melden sich aber.
#[test]
fn unsinnige_masse_ergeben_hinweise() {
    let a = write(&house());
    let b = a
        .replacen("pts=\"0 0;0 8000;", "pts=\"0 0;0 0;", 1)
        .replacen(" t=140 ", " t=0 ", 1);
    assert_ne!(a, b);
    let l = load(&b).unwrap();
    // dazu je OG-Wand, dass sie nicht mehr auf ihrer EG-Wand steht
    assert_eq!(l.hints.len(), 6, "{:?}", l.hints);
    assert!(l.hints.iter().any(|h| h.contains("ohne Länge")));
    assert!(l.hints.iter().any(|h| h.contains("Schichtdicke")));
    assert_eq!(
        l.hints
            .iter()
            .filter(|h| h.contains("nicht parallel"))
            .count(),
        4
    );
}

#[test]
fn texte_mit_sonderzeichen() {
    let mut out = String::new();
    Line::new("x")
        .text("t", "a \"b\" \\ c\nd # e")
        .finish(&mut out);
    let r = Record::parse(1, out.trim_end()).unwrap().unwrap();
    assert_eq!(r.get("t").unwrap(), "a \"b\" \\ c\nd # e");
    assert!(Record::parse(1, "[x] t=\"offen").is_err());
    assert!(Record::parse(1, "  # nur Kommentar").unwrap().is_none());
}

// --- R4: Aufbau für Decke, Sohlplatte und Frostschürze ---------------

/// Haus mit einem Deckentyp „Estrich 50 + Kern“ (noch nicht gesetzt);
/// liefert die erste Decke und den Typ.
fn haus_mit_deckentyp() -> (Model, crate::ElementId, LayerSetId) {
    let mut m = house();
    let de = m
        .elements()
        .iter()
        .find(|(_, e)| e.category == Category::Floor)
        .map(|(id, _)| id)
        .expect("Decke");
    let beton = m.element(de).unwrap().kind.material().unwrap();
    let putz = m
        .materials()
        .iter()
        .find(|(_, x)| x.name == "Putz")
        .map(|(id, _)| id)
        .unwrap();
    let t = m
        .add_layer_set(LayerSet {
            guid: Guid(0x4e00),
            name: "Decke mit Estrich".into(),
            code: "DE-E".into(),
            category: TypeCategory::Floor,
            layers: vec![
                MaterialLayer::new(putz, 50.0, LayerFunction::Finish),
                MaterialLayer::new(beton, 100.0, LayerFunction::Structure).core(),
            ],
            props: PropSet::new(),
            note: String::new(),
            changed: 1,
            bearing: Bearing::Core,
            height: None,
        })
        .expect("Typ");
    (m, de, t)
}

fn nach_guid(m: &Model, g: Guid) -> crate::ElementId {
    m.elements()
        .iter()
        .find(|(_, e)| e.guid == g)
        .map(|(id, _)| id)
        .unwrap()
}

/// Ohne Typ hat jedes Bauteil mit Baustoff einen Einschicht-Aufbau aus
/// Baustoff und Dicke (Breite); Wände haben die Schichten ihres Typs.
#[test]
fn r4_gedachter_einschicht_aufbau() {
    let m = house();
    let mut seen = Vec::new();
    for (id, e) in m.elements().iter() {
        let a = m.element_layers(id);
        match e.kind {
            ElementKind::Wall(_) => {
                let t = m.layer_set(e.layer_set.unwrap()).unwrap();
                assert_eq!(a, t.layers);
            }
            ElementKind::Floor(_) | ElementKind::GroundSlab(_) | ElementKind::StripFooting(_) => {
                let one = MaterialLayer::new(
                    e.kind.material().unwrap(),
                    e.kind.core_thickness().unwrap(),
                    LayerFunction::Structure,
                )
                .core();
                assert_eq!(a, vec![one], "{}", e.number);
                seen.push(e.category);
            }
            _ => assert!(a.len() <= 1, "{}", e.number),
        }
    }
    for c in [
        Category::Floor,
        Category::GroundSlab,
        Category::StripFooting,
    ] {
        assert!(seen.contains(&c), "{c:?} im Prüfhaus");
    }
}

/// Decke mit Typ: Schichten von oben nach unten, der Kern mit Dicke und
/// Baustoff der Decke (variable Kernschicht, Regel 38). Ein Typ fremder
/// Art passt nicht (Regel 37).
#[test]
fn r4_decke_mit_typ() {
    let (mut m, de, t) = haus_mit_deckentyp();
    let dicke = m.element(de).unwrap().kind.core_thickness().unwrap();
    let mat = m.element(de).unwrap().kind.material().unwrap();
    assert!(m.set_slab_type(de, Some(t)));
    assert_eq!(m.element(de).unwrap().layer_set, Some(t));
    let a = m.element_layers(de);
    assert_eq!(a.len(), 2);
    assert_eq!(a.iter().map(|l| l.thickness).sum::<f64>(), 50.0 + dicke);
    assert_eq!((a[1].core, a[1].material), (true, mat));
    assert!(m.check().is_empty(), "{:?}", m.check());
    assert!(m.set_floor_thickness(de, 250.0));
    assert_eq!(m.element_layers(de)[1].thickness, 250.0);
    // Regel 37: Wandtyp an der Decke, Deckentyp an der Sohlplatte
    let aw = m.defaults().exterior_wall;
    assert!(!m.set_slab_type(de, Some(aw)));
    let sp = m
        .elements()
        .iter()
        .find(|(_, e)| e.category == Category::GroundSlab)
        .map(|(id, _)| id)
        .unwrap();
    assert!(!m.set_slab_type(sp, Some(t)));
    // Regel 38: Kernbaustoff = Baustoff am Bauteil (Datei von Hand
    // geändert)
    let putz = m.layer_set(t).unwrap().layers[0].material;
    let g = |id| m.material(id).unwrap().guid.to_ifc();
    let text = write(&m);
    let zeile = text.lines().find(|l| l.starts_with("[floor]")).unwrap();
    let falsch = zeile.replace(&format!(" mat={} ", g(mat)), &format!(" mat={} ", g(putz)));
    assert_ne!(zeile, falsch);
    let back = read(&text.replace(zeile, &falsch), GuidGen::with_seed(1)).unwrap();
    assert!(
        back.hints.iter().any(|p| p.contains("Kern des Typs")),
        "{:?}",
        back.hints
    );
    assert!(m.set_slab_type(de, None));
    assert_eq!(m.element_layers(de).len(), 1);
    // Regel 38 am Typ: waagerecht genau ein Kern
    let mut zwei = m.layer_set(t).unwrap().clone();
    zwei.layers[0].core = true;
    assert!(zwei
        .problems()
        .iter()
        .any(|p| p.contains("genau eine Kernschicht")));
}

/// Ohne Typ an Decke, Sohlplatte oder Frostschürze steht kein `set=` in
/// der Datei; mit Typ steht es, und der Rundlauf ist bytegleich.
#[test]
fn r4_typ_in_der_datei() {
    let (mut m, de, t) = haus_mit_deckentyp();
    let ohne = write(&m);
    for l in ohne.lines().filter(|l| {
        l.starts_with("[floor]") || l.starts_with("[slab]") || l.starts_with("[footing]")
    }) {
        assert!(!l.contains(" set="), "{l}");
    }
    assert!(ohne.contains(" cat=floor "), "Deckentyp in der Datei");
    assert!(m.set_slab_type(de, Some(t)));
    let mit = write(&m);
    let g = m.layer_set(t).unwrap().guid.to_ifc();
    assert!(mit
        .lines()
        .any(|l| l.starts_with("[floor]") && l.contains(&format!(" set={g} "))));
    let back = read(&mit, GuidGen::with_seed(1)).unwrap();
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(write(&back.model), mit, "Rundlauf bytegleich");
    let de2 = nach_guid(&back.model, m.element(de).unwrap().guid);
    let form = |m: &Model, id| {
        m.element_layers(id)
            .iter()
            .map(|l| (m.material(l.material).unwrap().guid, l.thickness, l.core))
            .collect::<Vec<_>>()
    };
    assert_eq!(form(&back.model, de2), form(&m, de));
}

/// F-17: Ein Typ unbekannter Art (`cat=zukunft`) wird mit Hinweis
/// übersprungen; die Decke mit `set=` darauf öffnet mit Einschicht-Aufbau
/// aus `mat=` (Regel 40). Eine unbekannte Schichtaufgabe gilt als
/// Bekleidung.
#[test]
fn r4_unbekannte_art_und_aufgabe() {
    let (mut m, de, t) = haus_mit_deckentyp();
    assert!(m.set_slab_type(de, Some(t)));
    let text = write(&m)
        .lines()
        .map(|l| match l.starts_with("[layerset]") {
            true => l.replace(" cat=floor ", " cat=zukunft "),
            false => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let back = read(&text, GuidGen::with_seed(1)).expect("öffnet");
    assert!(
        back.hints
            .iter()
            .any(|h| h.contains("unbekannter Art „zukunft“")),
        "{:?}",
        back.hints
    );
    assert!(
        back.hints
            .iter()
            .any(|h| h.contains("Typ von DE-001 fehlt")),
        "{:?}",
        back.hints
    );
    let de2 = nach_guid(&back.model, m.element(de).unwrap().guid);
    assert_eq!(back.model.element(de2).unwrap().layer_set, None);
    assert_eq!(back.model.element_layers(de2).len(), 1);
    assert!(back
        .model
        .layer_sets()
        .iter()
        .all(|(_, s)| s.code != "DE-E"));

    let text = write(&house());
    let text = text.replacen(" fn=insulation ", " fn=zukunft ", 1);
    let back = read(&text, GuidGen::with_seed(1)).expect("öffnet");
    assert!(
        back.hints
            .iter()
            .any(|h| h.contains("unbekannte Schichtaufgabe „zukunft“")),
        "{:?}",
        back.hints
    );
}

use crate::txn::Direction;

/// Abschnitte von `sk-cost` in `.szo` (Bausteingrenze §4.1).
const KA0: [&str; 8] = [
    "article",
    "service",
    "svcpart",
    "svcfollow",
    "rate",
    "lot",
    "origin",
    "costproject",
];

/// Alle acht Abschnitte, mit ungültigem Wert, doppelter Kennung, Zeile
/// ohne Kennung und fremdem Schlüssel; dahinter ein fremder Abschnitt.
fn mit_erweiterung(m: &Model) -> String {
    let mut t = write(m);
    t.push_str(
        "[article] guid=0Art1 name=\"Ziegel\" unit=Banane price=12.5\n\
         [article] guid=0Art1 name=\"doppelt\" price=1\n\
         [article] name=\"ohne Kennung\" price=2\n\
         [service] guid=0Svc1 name=\"Mauern\" zukunft=\"ja\"\n\
         [svcpart] key=0Svc1/0Art1 svc=0Svc1 art=0Art1 qty=0.12\n\
         [svcfollow] key=0Svc1/0Svc2 from=0Svc1 to=0Svc2\n\
         [rate] key=wage num=60.00\n\
         [lot] key=300 name=\"Mauerarbeiten\"\n\
         [origin] key=0Art1 rec=article from=werk\n\
         [costproject] key=vat num=19\n\
         [zukunft] guid=0Zuk1 wert=\"neu\"\n",
    );
    t
}

/// KA-0b (paket-ka0.md §6 Nr. 4): Rundlauf aller Abschnitte bytegleich,
/// mit und ohne Erweiterungsliste.
#[test]
fn ka0b_erweiterung_rundlauf_bytegleich() {
    let text = mit_erweiterung(&house());
    let mit = read_with(&text, GuidGen::with_seed(99), &KA0).unwrap();
    assert_eq!(write(&mit.model), text);
    assert_eq!(mit.model.ext("article").count(), 3);
    assert_eq!(mit.model.ext("costproject").count(), 1);
    assert_eq!(mit.model.ext("zukunft").count(), 0);
    let ohne = load(&text).unwrap();
    assert_eq!(write(&ohne.model), text);
    assert_eq!(ohne.model.ext("article").count(), 0);
}

/// KA-0b (§6 Nr. 5): Stand vor KA-0 → KA-0 → vorher ohne Verlust; und
/// eine Datei mit Erweiterungszeilen mitten zwischen den bekannten
/// Sätzen kommt hinter sie, ohne Zeile zu verlieren.
#[test]
fn ka0b_alt_neu_alt_ohne_verlust() {
    let text = mit_erweiterung(&house());
    let neu = read_with(&text, GuidGen::with_seed(99), &KA0).unwrap();
    let mut m = neu.model;
    m.begin("Satz");
    m.ext_put("rate", "wage", "[rate] key=wage num=61.00".into(), None);
    m.commit().unwrap();
    let geschrieben = write(&m);
    let alt = load(&geschrieben).unwrap();
    assert_eq!(write(&alt.model), geschrieben);
    let wieder = read_with(&geschrieben, GuidGen::with_seed(99), &KA0).unwrap();
    assert_eq!(write(&wieder.model), geschrieben);
    assert_eq!(
        wieder.model.ext("rate").next().unwrap().line,
        "[rate] key=wage num=61.00"
    );
    // Erweiterungszeile vor den Stiften: gleicher Inhalt, neue Stelle
    let plain = write(&house());
    let (kopf, rest) = plain.split_once('\n').unwrap();
    let vorn = format!("{kopf}\n[lot] key=300 name=\"Mauerarbeiten\"\n{rest}");
    let back = read_with(&vorn, GuidGen::with_seed(99), &KA0).unwrap();
    assert_eq!(
        write(&back.model),
        format!("{plain}[lot] key=300 name=\"Mauerarbeiten\"\n")
    );
}

/// KA-0b (§6 Nr. 6): Rückgängig und Wiederholen eines Erweiterungssatzes
/// an seine Stelle; Modell- und Erweiterungsänderung sind ein Schritt.
#[test]
fn ka0b_ein_schritt_rueckgaengig_an_seine_stelle() {
    let text = mit_erweiterung(&house());
    let mut m = read_with(&text, GuidGen::with_seed(99), &KA0)
        .unwrap()
        .model;
    m.require_steps();
    let rev = m.revision();
    let ext_rev = m.ext_revision();
    m.begin("Kosten und Projekt");
    m.ext_put(
        "article",
        "0Art2",
        "[article] guid=0Art2 name=\"Mörtel\"".into(),
        Some("0Art1"),
    );
    m.ext_put(
        "article",
        "0Art1",
        "[article] guid=0Art1 name=\"Ziegel\" price=13".into(),
        None,
    );
    m.ext_remove("lot", "300");
    m.ext_remove("lot", "999");
    assert!(m.set_project(Project {
        site: "Hofweg 3".into(),
        client: "Familie Muster".into(),
        author: "J. Architekt".into(),
        ..m.project().clone()
    }));
    let w = m.runs().iter().next().map(|(_, r)| r.segments[0]).unwrap();
    m.set_number(w, "AW-Süd").unwrap();
    let t = m.commit().expect("ein Schritt");
    assert!(m.revision() > rev);
    assert!(m.ext_revision() > ext_rev);
    let nachher = write(&m);
    assert!(nachher.contains(
        "[article] guid=0Art2 name=\"Mörtel\"\n[article] guid=0Art1 name=\"Ziegel\" price=13\n\
         [article] guid=0Art1 name=\"doppelt\" price=1\n"
    ));
    assert!(!nachher.contains("[lot]"));
    assert!(nachher.contains(" site=\"Hofweg 3\""));
    m.apply(&t, Direction::Undo);
    assert_eq!(write(&m), text);
    m.apply(&t, Direction::Redo);
    assert_eq!(write(&m), nachher);
    m.apply(&t, Direction::Undo);
    assert_eq!(write(&m), text);
    // Abbrechen rollt die Erweiterung mit zurück
    m.begin("verworfen");
    m.ext_put("rate", "wage", "[rate] key=wage num=1".into(), None);
    m.rollback();
    assert_eq!(write(&m), text);
}

fn projektdaten() -> Project {
    Project {
        kind: "Neubau Einfamilienhaus".into(),
        site: "Haus Mustermann".into(),
        place: "Musterweg 1\n27777 Ganderkesee".into(),
        number: "01/26".into(),
        client: "Max Mustermann".into(),
        client_addr: "Phantasiestraße 7\n12345 Irgendwo".into(),
        author: "Dipl.-Ing. (FH) Jörn Horstmann".into(),
        author_addr: "Denkmalsweg 18b\n27777 Ganderkesee".into(),
        ..Project::new(Guid(0), "")
    }
}

/// Paket PD Abnahme 1 und 2: „Neu“ mit Daten schreibt eine
/// `[projectinfo]`-Zeile mit acht Feldern, `[project]` bleibt; ohne
/// Daten bleibt die Datei bytegleich. Zeilenumbrüche überstehen den
/// Rundlauf.
#[test]
fn projektdaten_neu_mit_und_ohne() {
    let m = Model::with_seed(1);
    let leer = write(&m);
    let mut ohne = Model::with_seed(1);
    ohne.init_project(Project::new(Guid(0), ""));
    assert_eq!(write(&ohne), leer);
    let mut m = Model::with_seed(1);
    m.init_project(projektdaten());
    let text = write(&m);
    let projekt = |t: &str| {
        t.lines()
            .find(|l| l.starts_with("[project] "))
            .unwrap()
            .to_string()
    };
    assert_eq!(projekt(&text), projekt(&leer));
    let info: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("[projectinfo]"))
        .collect();
    assert_eq!(
        info,
        [
            "[projectinfo] key=project kind=\"Neubau Einfamilienhaus\" projno=\"01/26\" \
          site=\"Haus Mustermann\" place=\"Musterweg 1\\n27777 Ganderkesee\" \
          client=\"Max Mustermann\" clientaddr=\"Phantasiestraße 7\\n12345 Irgendwo\" \
          author=\"Dipl.-Ing. (FH) Jörn Horstmann\" \
          authoraddr=\"Denkmalsweg 18b\\n27777 Ganderkesee\""
        ]
    );
    let back = load(&text).unwrap();
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(back.model.project().fields(), projektdaten().fields());
    assert_eq!(back.model.project().place, "Musterweg 1\n27777 Ganderkesee");
    assert_eq!(write(&back.model), text);
}

/// Paket PD Abnahme 3 und 5 (Regel 110): Eine alte Datei mit `site` und
/// `client` an `[project]` liest sich wie bisher und bleibt bytegleich.
/// Die erste Änderung zieht alles nach `[projectinfo]` um, ein Schritt;
/// Rückgängig stellt die alte Datei bytegleich her. Unverändert
/// übernehmen ist kein Schritt.
#[test]
fn projektdaten_alte_datei_zieht_um() {
    // vor Regel 110 geschrieben: Schlüssel an [project]
    let heute = write(&house());
    let zeile = heute
        .lines()
        .find(|l| l.starts_with("[project] "))
        .unwrap()
        .to_string();
    assert!(!zeile.contains(" next="), "{zeile}");
    let alt = heute.replace(
        &zeile,
        &format!("{zeile} site=\"Haus Meier\" client=\"Meier\""),
    );
    let mut m = load(&alt).unwrap().model;
    assert_eq!(
        (m.project().site.as_str(), m.project().client.as_str()),
        ("Haus Meier", "Meier")
    );
    assert!(!m.project().info);
    assert_eq!(write(&m), alt);
    m.begin("Projektdaten geändert");
    assert!(!m.set_project(m.project().clone()), "unverändert");
    assert!(m.set_project(Project {
        number: "01/26".into(),
        ..m.project().clone()
    }));
    let t = m.commit().expect("ein Schritt");
    let neu = write(&m);
    assert!(neu.contains(&format!("{zeile}\n")), "{neu}");
    assert!(neu.contains(
        "\n[projectinfo] key=project projno=\"01/26\" site=\"Haus Meier\" client=\"Meier\"\n"
    ));
    assert!(!neu.contains("[project] guid") || !projekt_hat_site(&neu));
    m.apply(&t, crate::txn::Direction::Undo);
    assert_eq!(write(&m), alt);
    m.apply(&t, crate::txn::Direction::Redo);
    assert_eq!(write(&m), neu);
    // Beides in einer Datei: [projectinfo] gilt, [project] bleibt bytegleich
    let beides = alt.replace(
        &format!("{zeile} site=\"Haus Meier\" client=\"Meier\"\n"),
        &format!(
            "{zeile} site=\"Haus Meier\" client=\"Meier\"\n[projectinfo] key=project site=\"Haus Neu\"\n"
        ),
    );
    let b = load(&beides).unwrap();
    assert!(b.hints.is_empty(), "{:?}", b.hints);
    assert_eq!(b.model.project().site, "Haus Neu");
    assert_eq!(b.model.project().client, "");
    assert_eq!(write(&b.model), beides);
    // Ein zu langer Wert und ein Umbruch im einzeiligen Feld bleiben
    let lang = "x".repeat(300);
    let odd = alt.replace(
        &format!("{zeile} site=\"Haus Meier\" client=\"Meier\"\n"),
        &format!("{zeile}\n[projectinfo] key=project projno=\"{lang}\" client=\"A\\nB\"\n"),
    );
    let o = load(&odd).unwrap();
    assert_eq!(o.model.project().number, lang);
    assert_eq!(o.model.project().client, "A\nB");
    assert_eq!(write(&o.model), odd);
    // Leere [projectinfo] neben alten Werten an [project]: die Anzeige
    // bleibt über Speichern und Laden leer, die Datei bytegleich
    let leer = beides.replace(
        "[projectinfo] key=project site=\"Haus Neu\"\n",
        "[projectinfo] key=project\n",
    );
    let l = load(&leer).unwrap();
    assert!(l.model.project().is_blank(), "{:?}", l.model.project());
    assert_eq!(write(&l.model), leer);
    let l = load(&write(&l.model)).unwrap();
    assert!(l.model.project().is_blank());
    // doppelt: Fehler mit der Zeile
    let doppelt = odd.replace("[projectinfo]", "[projectinfo] key=project\n[projectinfo]");
    assert!(load(&doppelt).is_err());
}

fn projekt_hat_site(text: &str) -> bool {
    text.lines()
        .any(|l| l.starts_with("[project] ") && l.contains(" site="))
}

/// KA-0b (§6 Nr. 7): `svc=` und `site/client/author` stehen nur, wo
/// gesetzt; heutige Dateien bleiben bytegleich.
#[test]
fn ka0b_bauleistung_und_projektangaben() {
    let m = house();
    let plain = write(&m);
    assert!(!plain.contains(" svc="));
    assert!(!plain.contains(" site="));
    assert!(!plain.contains(" client="));
    assert!(!plain.contains(" author="));
    let mut m = m;
    let id = m.layer_sets().ids().next().unwrap();
    let mut s = m.layer_set(id).unwrap().clone();
    let svc = m.new_guid();
    s.layers[0].svc = Some(svc);
    assert!(m.set_layer_set(id, s));
    m.set_project(Project {
        client: "Familie \"Muster\"".into(),
        ..m.project().clone()
    });
    let text = write(&m);
    assert_eq!(text.matches(" svc=").count(), 1);
    assert!(text.contains(" client="));
    assert!(!text.contains(" site="));
    let back = load(&text).unwrap();
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(write(&back.model), text);
    assert_eq!(back.model.project().client, "Familie \"Muster\"");
    let set = m.layer_set(id).unwrap().guid;
    assert_eq!(
        back.model
            .layer_sets()
            .iter()
            .find(|(_, s)| s.guid == set)
            .map(|(_, s)| s.layers[0].svc),
        Some(Some(svc))
    );
}

/// Review 3ac: Zwei gleiche Schichten, nur die zweite mit ungültigem
/// `svc=`. Der rohe Wert bleibt an der zweiten; vorher wanderte er an
/// die erste gleiche Zeile.
#[test]
fn a311_gleiche_schichten_behalten_ihre_zeile() {
    let mut m = house();
    let id = m.layer_sets().ids().next().unwrap();
    let mut s = m.layer_set(id).unwrap().clone();
    let l0 = s.layers[0];
    s.layers.insert(0, l0);
    let svc = m.new_guid();
    s.layers[1].svc = Some(svc);
    assert!(m.set_layer_set(id, s));
    let text = write(&m);
    let kaputt = text.replace(&format!(" svc={}", svc.to_ifc()), " svc=nix");
    assert_ne!(kaputt, text);
    let back = load(&kaputt).unwrap();
    assert_eq!(write(&back.model), kaputt, "bytegleich");
}

/// KA-0d, Befund 99: `Model::raw_svc` nennt Typ, Schicht und rohen Wert,
/// genau solange die Datei ihn zurückschreibt; nach Zuordnen weg, nach
/// Rückgängig wieder da. Gleiche Schichten: die richtige.
#[test]
fn a311_raw_svc_nennt_typ_und_schicht() {
    use crate::txn::Direction;
    let mut m = house();
    assert!(m.raw_svc().is_empty());
    let id = m.layer_sets().ids().next().unwrap();
    let mut s = m.layer_set(id).unwrap().clone();
    let l0 = s.layers[0];
    s.layers.insert(0, l0);
    let svc = m.new_guid();
    s.layers[1].svc = Some(svc);
    assert!(m.set_layer_set(id, s));
    let set = m.layer_set(id).unwrap().guid;
    let text = write(&m);
    assert!(load(&text).unwrap().model.raw_svc().is_empty(), "gültig");
    let kaputt = text.replace(&format!(" svc={}", svc.to_ifc()), " svc=nix");
    let mut b = load(&kaputt).unwrap().model;
    let id = b
        .layer_sets()
        .iter()
        .find(|(_, s)| s.guid == set)
        .map(|(i, _)| i)
        .unwrap();
    let roh = vec![crate::RawSvc {
        set,
        layer: 1,
        value: "nix".into(),
    }];
    assert_eq!(b.raw_svc(), roh);
    // Zuordnen an der Schicht ersetzt den rohen Wert, Rückgängig holt ihn
    let mut s = b.layer_set(id).unwrap().clone();
    s.layers[1].svc = Some(svc);
    b.begin("Bauleistung zuordnen");
    assert!(b.set_layer_set(id, s));
    let t = b.commit().unwrap();
    assert!(b.raw_svc().is_empty());
    assert!(!write(&b).contains(" svc=nix"));
    b.apply(&t, Direction::Undo);
    assert_eq!(b.raw_svc(), roh);
    assert_eq!(write(&b), kaputt, "bytegleich nach Rückgängig");
}

/// Wie oben für den Firmenkatalog.
#[test]
fn a311_raw_svc_im_firmenkatalog() {
    let mut lib = crate::catalog::Library::from_model(&house());
    let id = lib.types.ids().next().unwrap();
    let t = lib.types.get_mut(id).unwrap();
    let set = t.guid;
    let svc = Guid::from_ifc("0Svc000000000000000001").unwrap();
    t.layers[0].svc = Some(svc);
    let text = crate::write_szk(&lib);
    let kaputt = text.replace(&format!(" svc={}", svc.to_ifc()), " svc=-x-");
    assert_ne!(kaputt, text);
    let back = crate::read_szk(&kaputt).unwrap();
    assert_eq!(crate::write_szk(&back), kaputt);
    assert_eq!(
        back.raw_svc(),
        vec![crate::RawSvc {
            set,
            layer: 0,
            value: "-x-".into(),
        }]
    );
}

/// A311 (Nachtrag KA-0c/d zu §6 Nr. 4/7, R7): Ein ungültiges `svc=` an
/// einer Schicht bleibt roh erhalten. Es wird nicht benutzt (Schicht ohne
/// Bauleistung), mit Befund „Bauleistung“ gemeldet und beim Speichern
/// unverändert zurückgeschrieben; die Datei bleibt bytegleich, auch nach
/// einer Änderung an anderer Stelle.
#[test]
fn a311_ungueltige_bauleistung_bleibt_erhalten() {
    let mut m = house();
    let id = m.layer_sets().ids().next().unwrap();
    let mut s = m.layer_set(id).unwrap().clone();
    let svc = m.new_guid();
    s.layers[0].svc = Some(svc);
    assert!(m.set_layer_set(id, s));
    let set = m.layer_set(id).unwrap().guid;
    let text = write(&m);
    for roh in ["nix", "0Svc1", "-x-", "\"\""] {
        let kaputt = text.replace(&format!(" svc={}", svc.to_ifc()), &format!(" svc={roh}"));
        assert_ne!(kaputt, text);
        let back = load(&kaputt).unwrap();
        assert!(
            back.hints.iter().any(|h| h.contains("Bauleistung")),
            "{roh}: {:?}",
            back.hints
        );
        let layer0 = back
            .model
            .layer_sets()
            .iter()
            .find(|(_, s)| s.guid == set)
            .map(|(_, s)| s.layers[0].svc);
        assert_eq!(layer0, Some(None), "{roh}: nicht benutzt");
        assert_eq!(write(&back.model), kaputt, "{roh}: bytegleich");
        // Änderung an anderer Stelle: der rohe Wert bleibt stehen
        let mut b = back.model;
        let w = b.runs().iter().next().map(|(_, r)| r.segments[0]).unwrap();
        b.set_number(w, "AW-Süd").unwrap();
        let neu = write(&b);
        assert!(neu.contains(&format!(" svc={roh}")), "{roh}: nach Änderung");
    }
}

/// Sonnenstand S4 (§8 09:25): Ohne eingeschaltetes System kein
/// `[sun]`, alte Dateien bleiben bytegleich; danach `date` und `time`,
/// `on=1` nur, solange es an ist. Ansichtszustand: kein Schritt, keine
/// neue Revision. Unlesbares bleibt roh mit Hinweis, bis ein Stand es
/// ersetzt.
#[test]
fn s4_sonnenstand_in_der_datei() {
    use sk_math::sonne::Datum;
    let mut m = house();
    let alt = write(&m);
    assert!(!alt.contains("[sun]"));
    let rev = m.revision();
    let mut s = crate::Sun {
        date: Datum::new(2026, 6, 21).unwrap(),
        minutes: 12 * 60,
        on: true,
    };
    m.set_sun(s);
    assert_eq!(
        m.revision(),
        rev,
        "Ansichtszustand ändert die Revision nicht"
    );
    let an = write(&m);
    let zeilen: Vec<&str> = an.lines().filter(|z| z.starts_with("[sun]")).collect();
    assert_eq!(zeilen, ["[sun] date=2026-06-21 time=12:00 on=1"]);
    let back = load(&an).unwrap();
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(back.model.sun(), Some(s));
    assert_eq!(write(&back.model), an);
    s.on = false;
    s.minutes = 9 * 60 + 5;
    m.set_sun(s);
    let aus = write(&m);
    assert!(
        aus.contains("\n[sun] date=2026-06-21 time=09:05\n"),
        "{aus}"
    );
    assert_eq!(load(&aus).unwrap().model.sun(), Some(s));
    assert_eq!(aus.replace("[sun] date=2026-06-21 time=09:05\n", ""), alt);

    // Unlesbar: Hinweis, die Zeile bleibt bytegleich, bis ein Stand sie ersetzt
    for roh in [
        "[sun] date=2026-02-30 time=12:00",
        "[sun] date=21.06.2026 time=12:00 on=1",
        "[sun] date=2026-06-21 time=24:00",
        "[sun] date=2026-06-21",
        "[sun] date=2026-06-21 time=12:00 on=ja",
    ] {
        let text = alt.replacen("[storey]", &format!("{roh}\n[storey]"), 1);
        let l = load(&text).unwrap();
        assert_eq!(l.model.sun(), None, "{roh}");
        assert_eq!(l.hints.len(), 1, "{roh}: {:?}", l.hints);
        assert!(l.hints[0].contains("[sun]"), "{:?}", l.hints);
        let w = write(&l.model);
        assert_eq!(
            w.lines()
                .filter(|z| z.starts_with("[sun]"))
                .collect::<Vec<_>>(),
            [roh]
        );
        let mut b = l.model;
        b.set_sun(s);
        let neu = write(&b);
        assert!(!neu.lines().any(|z| z == roh), "{roh}: ersetzt");
        assert_eq!(neu.matches("[sun]").count(), 1);
    }
    let doppelt = an.replacen("[sun]", "[sun] date=2026-01-01 time=08:00\n[sun]", 1);
    assert!(load(&doppelt).is_err());
}

/// S11 (Jörn 09.10. 14:10): Unter Gelände gestrichelt je Ansicht, nur
/// gewählt eine Zeile; alte Dateien bytegleich, keine neue Revision;
/// Unlesbares bleibt roh mit Hinweis.
#[test]
fn s11_unter_gelaende_in_der_datei() {
    let mut m = house();
    let alt = write(&m);
    assert!(!alt.contains("[viewbelow]"));
    assert!(!m.view_below(1));
    let rev = m.revision();
    m.set_view_below(1, true);
    assert_eq!(m.revision(), rev, "Ansichtszustand");
    let neu = write(&m);
    assert!(
        neu.lines().any(|z| z == "[viewbelow] view=back dashed=1"),
        "{neu}"
    );
    let back = load(&neu).unwrap();
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert!(back.model.view_below(1) && !back.model.view_below(0));
    assert_eq!(write(&back.model), neu);
    let mut m2 = back.model;
    m2.set_view_below(1, false);
    assert_eq!(write(&m2), alt);
    for roh in [
        "[viewbelow] view=oben dashed=1",
        "[viewbelow] view=back dashed=ja",
    ] {
        let text = alt.replacen("[storey]", &format!("{roh}\n[storey]"), 1);
        let l = load(&text).unwrap();
        assert_eq!(l.hints.len(), 1, "{roh}: {:?}", l.hints);
        assert!(!l.model.view_below(1));
        assert!(write(&l.model).lines().any(|z| z == roh), "{roh}: roh");
    }
}

/// Sonnenstand S7: Ohne eigene Wahl kein `[viewshade]`, alte Dateien
/// bleiben bytegleich; je gewählter Ansicht eine Zeile, Rundlauf
/// bytegleich, keine neue Revision. Unlesbares bleibt roh mit Hinweis,
/// bis eine Wahl für diese Ansicht es ersetzt.
#[test]
fn s7_schatten_der_ansichten_in_der_datei() {
    use crate::{ShadeLight, ViewShade};
    let mut m = house();
    let alt = write(&m);
    assert!(!alt.contains("[viewshade]"));
    assert_eq!(m.view_shade(2, ViewShade::WERK), ViewShade::WERK);
    let rev = m.revision();
    let a = ViewShade {
        on: true,
        hatch: true,
        light: ShadeLight::Sun,
    };
    let b = ViewShade {
        on: false,
        hatch: false,
        light: ShadeLight::FrontRight,
    };
    m.set_view_shade(3, b, ViewShade::WERK);
    m.set_view_shade(0, a, ViewShade::WERK);
    assert_eq!(m.revision(), rev, "Ansichtszustand");
    let neu = write(&m);
    let zeilen: Vec<&str> = neu
        .lines()
        .filter(|z| z.starts_with("[viewshade]"))
        .collect();
    assert_eq!(
        zeilen,
        [
            "[viewshade] view=front on=1 fill=hatch light=sun",
            "[viewshade] view=right on=0 fill=area light=front-right",
        ]
    );
    let back = load(&neu).unwrap();
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(back.model.view_shade_own(0), Some(a));
    assert_eq!(back.model.view_shade_own(1), None);
    assert_eq!(back.model.view_shade(3, ViewShade::WERK), b);
    assert_eq!(write(&back.model), neu);
    let ohne: String = neu
        .lines()
        .filter(|z| !z.starts_with("[viewshade]"))
        .map(|z| format!("{z}\n"))
        .collect();
    assert_eq!(ohne, alt);

    // Unlesbar: Hinweis, die Zeile bleibt, bis die Ansicht gewählt wird
    for roh in [
        "[viewshade] view=oben on=1 fill=area light=sun",
        "[viewshade] view=back on=ja fill=area light=sun",
        "[viewshade] view=back on=1 fill=grau light=sun",
        "[viewshade] view=back on=1 fill=area light=hinten",
        "[viewshade] view=back on=1 fill=area",
    ] {
        let text = alt.replacen("[storey]", &format!("{roh}\n[storey]"), 1);
        let l = load(&text).unwrap();
        assert_eq!(l.hints.len(), 1, "{roh}: {:?}", l.hints);
        assert!(l.hints[0].contains("[viewshade]"), "{:?}", l.hints);
        assert_eq!(l.model.view_shade_own(1), None, "{roh}");
        let w = write(&l.model);
        assert!(w.lines().any(|z| z == roh), "{roh}: roh");
        let mut m2 = l.model;
        m2.set_view_shade(1, a, ViewShade::WERK);
        let w2 = write(&m2);
        let ersetzt = roh.contains("view=back");
        assert_eq!(!w2.lines().any(|z| z == roh), ersetzt, "{roh}");
    }
    // Dieselbe Ansicht doppelt: die zweite Zeile zählt nicht
    let doppelt = neu.replacen(
        "[viewshade] view=front",
        "[viewshade] view=front on=0 fill=area light=front-left\n[viewshade] view=front",
        1,
    );
    let l = load(&doppelt).unwrap();
    assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
    assert!(!l.model.view_shade_own(0).unwrap().on);

    // „vorne oben“ (§8 11:55, Rückfrage a) im Rundlauf
    let oben = ViewShade {
        light: ShadeLight::Top,
        ..ViewShade::WERK
    };
    let mut m3 = back.model;
    m3.set_view_shade(1, oben, ViewShade::WERK);
    let w3 = write(&m3);
    assert!(w3
        .lines()
        .any(|z| z == "[viewshade] view=back on=1 fill=area light=top"));
    assert_eq!(load(&w3).unwrap().model.view_shade_own(1), Some(oben));
    // Gleicht eine Ansicht wieder der Vorgabe, entfällt ihre Zeile
    for i in 0..4 {
        m3.set_view_shade(i, ViewShade::WERK, ViewShade::WERK);
    }
    assert_eq!(m3.view_shade_own(0), None);
    assert_eq!(write(&m3), alt);
    // Eine andere Vorgabe (S8): dieselbe Wahl ist dann eigene Wahl
    let vorgabe = ViewShade {
        on: false,
        ..ViewShade::WERK
    };
    m3.set_view_shade(2, ViewShade::WERK, vorgabe);
    assert_eq!(m3.view_shade_own(2), Some(ViewShade::WERK));
}

/// Sonnenstand S1: Ohne Lage keine Zeile, die alte Datei bleibt
/// bytegleich; mit Lage eine Zeile `[location]` hinter den
/// Projektdaten, Rundlauf bytegleich; ein Schritt „Nordrichtung
/// geändert“ und Strg+Z stellt die alte Datei her.
#[test]
fn s1_lage_und_nordrichtung() {
    let mut m = house();
    let alt = write(&m);
    assert!(!alt.contains("[location]"));
    assert_eq!(write(&load(&alt).unwrap().model), alt);
    assert!(load(&alt).unwrap().model.location().is_unset());

    m.begin("Nordrichtung geändert");
    let l = Location {
        lat: Some(53.0589),
        lon: Some(8.591),
        north: Some(-12.5),
        ..Default::default()
    };
    assert!(m.set_location(l));
    assert!(!m.set_location(l), "gleiche Werte ändern nichts");
    let t = m.commit().expect("ein Schritt");
    assert_eq!(t.label, "Nordrichtung geändert");
    assert_eq!(t.changes.len(), 1);
    let neu = write(&m);
    let zeilen: Vec<&str> = neu
        .lines()
        .filter(|z| z.starts_with("[location]"))
        .collect();
    assert_eq!(zeilen, ["[location] lat=53.0589 lon=8.591 north=347.5"]);
    let vor = neu.find("[location]").unwrap();
    assert!(neu.find("[project]").unwrap() < vor);
    assert!(vor < neu.find("[building]").unwrap_or(neu.len()));
    assert!(vor < neu.find("[storey]").unwrap());

    let back = load(&neu).unwrap();
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(back.model.location().north, Some(347.5));
    assert_eq!(back.model.revision(), 0, "Laden ist keine Änderung");
    assert_eq!(write(&back.model), neu);

    m.apply(&t, Direction::Undo);
    assert_eq!(write(&m), alt);
    m.apply(&t, Direction::Redo);
    assert_eq!(write(&m), neu);

    // Nur die Nordrichtung: Breite und Länge bleiben ungeschrieben
    let nur = alt.replacen("[storey]", "[location] north=90\n[storey]", 1);
    let l = load(&nur).unwrap();
    assert_eq!(l.model.location().lat, None);
    assert_eq!(l.model.location().north, Some(90.0));
    assert!(write(&l.model).contains("\n[location] north=90\n"));
}

/// Sonnenstand S1: Unbekanntes an `[location]` (neuere Fassung) bleibt;
/// eine unlesbare Zeile bleibt roh (Befund A); eine zweite Zeile ist ein
/// Fehler mit der Zeile.
#[test]
fn s1_lage_fremd_und_kaputt() {
    let alt = write(&house());
    // an der Stelle, an die der Schreiber [location] setzt
    let mit = |z: &str| alt.replacen("[building]", &format!("{z}\n[building]"), 1);
    let fremd = mit("[location] lat=52.5 lon=13.4 north=0 elev=34");
    let l = load(&fremd).unwrap();
    assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
    assert!(l.hints[0].contains("„elev“"));
    assert_eq!(l.model.location().north, Some(0.0));
    let text = write(&l.model);
    assert!(text.contains("\n[location] lat=52.5 lon=13.4 north=0 elev=34\n"));

    // Befund A: Unlesbares öffnet das Projekt, die Zeile zählt nicht und
    // bleibt bytegleich; eine gesetzte Lage ersetzt sie (genau eine
    // Zeile), Rückgängig holt sie zurück
    for z in [
        "[location] lat=95 lon=13.4",
        "[location] lat=abc",
        "[location] lat=53°03' lon=8.591 north=12",
        "[location] north=inf elev=3",
    ] {
        let kaputt = mit(z);
        let l = load(&kaputt).unwrap_or_else(|e| panic!("{z}: {e:?}"));
        assert_eq!(l.hints.len(), 1, "{z}: {:?}", l.hints);
        assert!(
            l.hints[0].contains("keine Zahl im Bereich"),
            "{:?}",
            l.hints
        );
        assert!(l.model.location().is_unset(), "{z}");
        assert_eq!(write(&l.model), kaputt, "{z}: bytegleich");
        let mut m = l.model;
        m.begin("Nordrichtung geändert");
        assert!(m.set_location(Location {
            north: Some(30.0),
            ..Default::default()
        }));
        let t = m.commit().expect("ein Schritt");
        let neu = write(&m);
        let zeilen: Vec<&str> = neu
            .lines()
            .filter(|z| z.starts_with("[location]"))
            .collect();
        assert_eq!(zeilen, ["[location] north=30"], "{z}");
        m.apply(&t, crate::txn::Direction::Undo);
        assert_eq!(write(&m), kaputt, "{z}: Rückgängig");
        m.apply(&t, crate::txn::Direction::Redo);
        assert_eq!(write(&m), neu, "{z}: Wiederholen");
        // Leere Lage übernommen (Maske ohne Breite und Länge): die Zeile bleibt
        let mut m = load(&kaputt).unwrap().model;
        m.begin("Projektdaten geändert");
        assert!(!m.set_location(Location::default()));
        assert!(m.commit().is_none());
        assert_eq!(write(&m), kaputt);
    }
    // Nur Fremdes oder leer (Review 3br): zählt nicht als Lage, bleibt
    // beim Speichern ohne Änderung im Wortlaut; eine gesetzte Lage
    // ersetzt die Zeile, Rückgängig holt sie zurück
    for z in [
        "[location] elev=34",
        "[location] lat=95 elev=34",
        "[location]",
    ] {
        let f = mit(z);
        let l = load(&f).unwrap();
        assert!(l.model.location().is_unset(), "{z}");
        let text = write(&l.model);
        assert_eq!(text, f, "{z}");
        assert_eq!(write(&load(&text).unwrap().model), text, "{z}");
        let mut m = l.model;
        m.begin("Nordrichtung geändert");
        assert!(m.set_location(Location {
            north: Some(5.0),
            ..Default::default()
        }));
        let t = m.commit().unwrap();
        let neu = write(&m);
        let zeilen: Vec<&str> = neu
            .lines()
            .filter(|z| z.starts_with("[location]"))
            .collect();
        assert_eq!(zeilen, ["[location] north=5"], "{z}");
        m.apply(&t, crate::txn::Direction::Undo);
        assert_eq!(write(&m), f, "{z}: Rückgängig");
    }
    let l = load(&mit("[location] elev=34")).unwrap();
    assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
    assert!(l.hints[0].contains("„elev“"), "{:?}", l.hints);

    // Gesetzt und im selben Schritt wieder leer: kein leerer Schritt,
    // der die Zeile stillschweigend verliert
    let kaputt = mit("[location] lat=abc");
    let mut m = load(&kaputt).unwrap().model;
    m.begin("Projektdaten geändert");
    assert!(m.set_location(Location {
        lat: Some(1.0),
        ..Default::default()
    }));
    assert!(m.set_location(Location::default()));
    let t = m.commit().expect("Schritt mit der Zeile");
    m.apply(&t, crate::txn::Direction::Undo);
    assert_eq!(write(&m), kaputt);

    assert!(load(&mit("[location] north=1\n[location] north=2")).is_err());
}

/// Sonnenstand S2: Der Fußpunkt des Nordpfeils steht als `x=`/`y=` an
/// `[location]`, nur zusammen und nur mit `north`; ein Schritt mit der
/// Richtung, Rundlauf und Rückgängig bytegleich. Eine Lücke oder
/// Unbrauchbares lässt die Zeile roh stehen (Befund A).
#[test]
fn s2_fusspunkt() {
    let mut m = house();
    let alt = write(&m);
    m.begin("Nordrichtung geändert");
    assert!(m.set_north_arrow(372.0, Some([12500.0, -3000.5])));
    assert!(!m.set_north_arrow(12.0, Some([12500.0, -3000.5])));
    let t = m.commit().unwrap();
    assert_eq!(t.changes.len(), 1);
    let neu = write(&m);
    let zeilen: Vec<&str> = neu
        .lines()
        .filter(|z| z.starts_with("[location]"))
        .collect();
    assert_eq!(zeilen, ["[location] north=12 x=12500 y=-3000.5"]);
    let back = load(&neu).unwrap();
    assert!(back.hints.is_empty(), "{:?}", back.hints);
    assert_eq!(back.model.north_foot(), Some([12500.0, -3000.5]));
    assert_eq!(write(&back.model), neu);
    // Verschieben: ein eigener Schritt, Richtung bleibt
    m.begin("Nordpfeil verschoben");
    assert!(m.set_north_arrow(12.0, Some([0.0, 0.0])));
    let t2 = m.commit().unwrap();
    assert!(write(&m).contains("\n[location] north=12 x=0 y=0\n"));
    // Breite über die Maske: der Fußpunkt bleibt
    m.begin("Projektdaten geändert");
    let l = Location {
        lat: Some(52.5),
        ..*m.location()
    };
    assert!(m.set_location(l));
    let t3 = m.commit().unwrap();
    assert!(write(&m).contains("\n[location] lat=52.5 north=12 x=0 y=0\n"));
    for t in [&t3, &t2] {
        m.apply(t, crate::txn::Direction::Undo);
    }
    assert_eq!(write(&m), neu);
    m.apply(&t, crate::txn::Direction::Undo);
    assert_eq!(write(&m), alt);

    let mit = |z: &str| alt.replacen("[building]", &format!("{z}\n[building]"), 1);
    for (z, k) in [
        ("[location] north=12 x=1", "„y“ fehlt"),
        ("[location] north=12 y=1", "„x“ fehlt"),
        ("[location] x=1 y=2", "„north“ fehlt"),
        ("[location] north=12 x=1 y=abc", "„y“ ist keine Zahl"),
        ("[location] north=12 x=1e12 y=0", "„x“ ist keine Zahl"),
    ] {
        let f = mit(z);
        let l = load(&f).unwrap();
        assert_eq!(l.hints.len(), 1, "{z}: {:?}", l.hints);
        assert!(l.hints[0].contains(k), "{z}: {:?}", l.hints);
        assert_eq!(l.hints[0].matches("„").count(), 1, "{:?}", l.hints);
        assert!(l.model.location().is_unset(), "{z}");
        assert_eq!(l.model.north_foot(), None);
        assert_eq!(write(&l.model), f, "{z}");
    }
}

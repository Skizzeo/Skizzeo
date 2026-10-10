use super::*;

// Abnahmetests A200–A210 (ohne A204, das steht in
// a204-a212-fremdabschnitte.rs): Sichtbarkeit (Paket 3,
// projektstruktur/paket-3-sichtbarkeit.md §5, dazu Review 3a G1).
// Spezifikation: test/abnahme-baumpanel.md.
//
// Einbau: als `mod sichtbarkeit { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, decke, r4,
// SectionLine, fit_parallel, px, W, H.
//
// Angenommene Namen stehen nur in den Adaptern (Vorschlag paket-3 §3.4):
// `sk_model::view::{Visibility, Isolate, Shown, Look}`,
// `Model::visibility`, `Model::shown`, `Scene::set_visibility` (erhöht
// `vis_rev`, ohne Schritt und Revision), `Scene::ghost_mesh` (zweites
// MeshData für `Look::Ghost`), `Scene::run_solid` (zwischengespeicherter
// Körper eines Zugs), `Tri::layer`, `Edge::elem`, `Edge::layer`,
// `sk_model::NO_LAYER`, `crate::cli::visibility_args` (reine Funktion
// hinter --ausblenden, --isolieren, --gelaende-aus).

use sk_model::view::{Isolate, Look, Shown, Visibility};
use sk_model::{Category, ElementId, RefSide, RunId};

// ===== Adapter Paket 3 =====

/// Sichtbarkeit ändern wie das Baumpanel: kein Schritt, keine Revision.
fn sicht(s: &mut Scene, f: impl FnOnce(&mut Visibility)) {
    let mut v = s.model().visibility().clone();
    f(&mut v);
    s.set_visibility(v);
}

fn zeigt(s: &Scene, id: ElementId) -> (Shown, Look) {
    s.model().shown(id)
}

/// Schichtmaske als Liste der sichtbaren Schichtindizes.
fn maske(sh: &Shown) -> Option<Vec<usize>> {
    match sh {
        Shown::Layers(m) => Some((0..64).filter(|i| m & (1u64 << i) != 0).collect()),
        _ => None,
    }
}

/// Netz der blassen Teile (Isolieren), gleiche Schlüssel wie `Scene::mesh`.
fn geist(s: &mut Scene) -> sk_render::MeshData {
    s.ghost_mesh(ViewKind::Persp, None, &[])
}

/// Befehlszeile ohne Fenster: Ergebnis ist der Ansichtszustand.
fn befehlszeile(s: &Scene, args: &[&str]) -> Result<Visibility, String> {
    crate::cli::visibility_args(args, s.model())
}

/// Zwischengespeicherter Körper eines Zugs (mit Stempeln).
fn koerper(s: &mut Scene, run: RunId) -> sk_model::Solid {
    s.run_solid(run).expect("Körper").clone()
}

/// Schichtindex am Dreieck (`NO_LAYER` = keine Schicht, z. B. Stirn).
fn schicht(t: &sk_model::Tri) -> u8 {
    t.layer
}

const KEINE_SCHICHT: u8 = sk_model::NO_LAYER;

/// Teil (Segment) an der Kante, wie `Tri::elem`.
fn kante_teil(e: &sk_model::Edge) -> u32 {
    e.elem
}

// ===== Hilfen =====

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

fn guid(s: &Scene, nummer: &str) -> sk_model::Guid {
    s.model().element(nr(s, nummer)).unwrap().guid
}

fn gewerk(s: &Scene, code: &str) -> sk_model::TradeId {
    s.model().trade_by_code(code).unwrap()
}

fn typ(s: &Scene, code: &str) -> sk_model::LayerSetId {
    s.model()
        .layer_sets()
        .iter()
        .find(|(_, t)| t.code == code)
        .map(|(id, _)| id)
        .unwrap()
}

/// Prüfhaus 10 × 8 m, AW-31,5, eine Innenwand im EG bei x = 5 m, OG-Nord
/// AW-006 gelöst um `d` mm (0: gekoppelt, bündig).
fn pruefhaus(seed: u64, d: f64) -> (Scene, RunId, RunId) {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let (eg, og) = gebaeude(&mut s);
    assert!(s.edit_model("Innenwand", |m| {
        let st = m.run(eg).unwrap().storey;
        let t = m.default_type(sk_model::TypeCategory::InteriorWall);
        let iw = [vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)];
        m.add_wall_run(&iw, false, RefSide::Center, st, t, Category::InteriorWall)
            .is_some()
    }));
    if d != 0.0 {
        let w = nr(&s, "AW-006");
        assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
        assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, d).is_some()));
    }
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    (s, eg, og)
}

fn csv(s: &mut Scene) -> Vec<u8> {
    let l = s.schedule().clone();
    crate::schedule_view::csv(s.model(), &l)
}

fn lesen(text: &str) -> sk_model::szo::Loaded {
    sk_model::szo::read(text, sk_model::GuidGen::with_seed(1)).expect("öffnet")
}

fn flaechen(m: &sk_render::MeshData) -> usize {
    m.faces.len()
}

/// A200 (§3.4 Auflösung, §1 Regeln): Ohne Ausgeblendetes ist jedes
/// Bauteil `(All, Solid)`. Bauteil ausgeblendet → `None`, nur dieses.
/// Art ausgeblendet → alle Bauteile der Art `None`, die übrigen bleiben.
/// Gewerk schichtgenau: 18345 aus → AW-31,5 zeigt nur Schicht 1
/// (Porenbeton), Innenwand und Decke bleiben `All`. 18330 aus → AW-31,5
/// zeigt nur Schicht 0 (WDVS), die Innenwand (nur Porenbeton) ist `None`
/// (leere Maske). AW-49 mit 18330 aus: Verblender, Kerndämmung und Kern
/// gehören zum Maurer (Paket 1a), übrig bleibt nur die Luftschicht
/// (Schicht 1, ohne Gewerk). Mineralwolle im WDVS gehört dagegen zu 18345:
/// Schicht statt Baustoff. Gelände aus ändert kein Bauteil. Die Attika
/// gehört zur EG-Wand: DT und AB sind eigene Bauteile und bleiben, wenn
/// AW-002 ausgeblendet ist.
#[test]
fn a200_aufloesung_ausblenden() {
    let (mut s, _, _) = pruefhaus(200, -1500.0);
    let alle: Vec<ElementId> = s.model().elements().iter().map(|(id, _)| id).collect();
    for &id in &alle {
        assert_eq!(zeigt(&s, id), (Shown::All, Look::Solid));
    }
    let aw2 = nr(&s, "AW-002");
    let g2 = guid(&s, "AW-002");
    sicht(&mut s, |v| {
        v.hidden.insert(g2);
    });
    for &id in &alle {
        let soll = if id == aw2 { Shown::None } else { Shown::All };
        assert_eq!(
            zeigt(&s, id).0,
            soll,
            "{}",
            s.model().element(id).unwrap().number
        );
    }
    assert_eq!(
        zeigt(&s, nr(&s, "DT-001")).0,
        Shown::All,
        "DT eigenes Bauteil"
    );
    assert_eq!(
        zeigt(&s, nr(&s, "AB-001")).0,
        Shown::All,
        "AB eigenes Bauteil"
    );

    sicht(&mut s, |v| {
        v.hidden.clear();
        v.hidden_cat.insert(Category::ExteriorWall);
    });
    for &id in &alle {
        let aussen = s.model().element(id).unwrap().category == Category::ExteriorWall;
        let soll = if aussen { Shown::None } else { Shown::All };
        assert_eq!(zeigt(&s, id).0, soll);
    }

    let wdvs = gewerk(&s, "18345");
    let maurer = gewerk(&s, "18330");
    sicht(&mut s, |v| {
        v.hidden_cat.clear();
        v.hidden_trade.insert(wdvs);
    });
    assert_eq!(maske(&zeigt(&s, aw2).0), Some(vec![1]), "nur Porenbeton");
    assert_eq!(zeigt(&s, nr(&s, "IW-001")).0, Shown::All);
    assert_eq!(zeigt(&s, nr(&s, "DE-001")).0, Shown::All);

    sicht(&mut s, |v| {
        v.hidden_trade.clear();
        v.hidden_trade.insert(maurer);
    });
    assert_eq!(maske(&zeigt(&s, aw2).0), Some(vec![0]), "nur WDVS");
    assert_eq!(zeigt(&s, nr(&s, "IW-001")).0, Shown::None, "leere Maske");

    let (mut b, eg, _) = pruefhaus(2000, 0.0);
    let t = typ(&b, "AW-49");
    assert!(b.edit_model("Wandtyp", |m| m.set_run_type(eg, t)));
    let maurer = gewerk(&b, "18330");
    sicht(&mut b, |v| {
        v.hidden_trade.insert(maurer);
    });
    assert_eq!(
        maske(&zeigt(&b, nr(&b, "AW-001")).0),
        Some(vec![1]),
        "nur Luft"
    );

    sicht(&mut s, |v| {
        v.hidden_trade.clear();
        v.terrain_hidden = true;
    });
    for &id in &alle {
        assert_eq!(zeigt(&s, id), (Shown::All, Look::Solid), "Gelände");
    }
}

/// A201 (§1, §3.4): Isolieren über Bauteile, Art und Gewerk. Isoliertes
/// ist `Solid`, alles andere `Ghost` mit seinem normalen `Shown`.
/// Isolieren geht vor Ausblenden: ein ausgeblendetes, aber isoliertes
/// Bauteil ist `(All, Solid)`. Gewerk isolieren: AW zeigt nur die Schicht
/// des Gewerks deckend, Bauteile ohne dieses Gewerk sind `Ghost`.
/// Beenden stellt den Zustand davor genau wieder her.
#[test]
fn a201_isolieren() {
    let (mut s, _, _) = pruefhaus(201, 0.0);
    let de = nr(&s, "DE-001");
    let g_de = guid(&s, "DE-001");
    let g_aw = guid(&s, "AW-001");
    sicht(&mut s, |v| {
        v.hidden.insert(g_aw);
    });
    let vorher = s.model().visibility().clone();
    sicht(&mut s, |v| {
        v.isolate = Some(Isolate::Elements([g_de, g_aw].into_iter().collect()));
    });
    assert_eq!(zeigt(&s, de), (Shown::All, Look::Solid));
    assert_eq!(
        zeigt(&s, nr(&s, "AW-001")),
        (Shown::All, Look::Solid),
        "vor Ausblenden"
    );
    assert_eq!(zeigt(&s, nr(&s, "AW-002")), (Shown::All, Look::Ghost));
    assert_eq!(zeigt(&s, nr(&s, "IW-001")).1, Look::Ghost);

    sicht(&mut s, |v| {
        v.isolate = Some(Isolate::Category(Category::InteriorWall))
    });
    assert_eq!(zeigt(&s, nr(&s, "IW-001")), (Shown::All, Look::Solid));
    assert_eq!(zeigt(&s, de).1, Look::Ghost);
    assert_eq!(
        zeigt(&s, nr(&s, "AW-001")),
        (Shown::None, Look::Ghost),
        "bleibt aus"
    );

    let wdvs = gewerk(&s, "18345");
    sicht(&mut s, |v| v.isolate = Some(Isolate::Trade(wdvs)));
    let (sh, look) = zeigt(&s, nr(&s, "AW-002"));
    assert_eq!(
        (maske(&sh), look),
        (Some(vec![0]), Look::Solid),
        "nur WDVS deckend"
    );
    assert_eq!(zeigt(&s, de).1, Look::Ghost);
    assert_eq!(zeigt(&s, nr(&s, "IW-001")).1, Look::Ghost);

    sicht(&mut s, |v| v.isolate = None);
    assert_eq!(s.model().visibility(), &vorher, "Beenden stellt wieder her");
    assert_eq!(zeigt(&s, nr(&s, "AW-001")), (Shown::None, Look::Solid));
}

/// A202 (§1 Regeln, §5.2/5.3): Sichtbarkeit ändert nie Mengen, Guids,
/// Nummern oder `Model::revision` und ist kein Rückgängig-Schritt: Nach
/// Ausblenden nimmt Strg+Z den letzten Modellschritt zurück, das
/// Ausgeblendete bleibt. „Alles zeigen“ leert Ausgeblendetes und
/// Isolieren.
#[test]
fn a202_keine_mengen_kein_schritt() {
    let (mut s, _, _) = pruefhaus(202, -300.0);
    let mengen = csv(&mut s);
    let rev = s.model().revision();
    let ids: Vec<(sk_model::Guid, String)> = s
        .model()
        .elements()
        .iter()
        .map(|(_, e)| (e.guid, e.number.clone()))
        .collect();
    let schritt = s.undo_label();
    let g = guid(&s, "AW-002");
    let wdvs = gewerk(&s, "18345");
    let de = guid(&s, "DE-001");
    sicht(&mut s, |v| {
        v.hidden.insert(g);
        v.hidden_cat.insert(Category::InteriorWall);
        v.hidden_trade.insert(wdvs);
        v.terrain_hidden = true;
        v.isolate = Some(Isolate::Elements([de].into_iter().collect()));
    });
    assert_eq!(s.model().revision(), rev, "Revision");
    assert_eq!(csv(&mut s), mengen, "Mengen zählen alles");
    let nach: Vec<(sk_model::Guid, String)> = s
        .model()
        .elements()
        .iter()
        .map(|(_, e)| (e.guid, e.number.clone()))
        .collect();
    assert_eq!(nach, ids);
    assert_eq!(s.undo_label(), schritt, "kein Schritt");
    assert!(s.undo(), "letzter Modellschritt");
    assert!(
        s.model().visibility().hidden.contains(&g),
        "Ausgeblendetes bleibt"
    );
    sicht(&mut s, |v| *v = Visibility::default());
    assert!(s.model().visibility().hidden.is_empty());
    assert!(s.model().visibility().isolate.is_none());
}

/// A203 (§3.6, F-17): `.szo` bleibt 4. `[hide] elem=`, `cat=`, `trade=`
/// und `terrain=1` im Rundlauf bytegleich. Ohne Ausgeblendetes keine
/// `[hide]`-Zeile, die Datei ist bytegleich wie vorher. Isolieren steht
/// nie in der Datei. `[hide] elem=<unbekannt>`, `cat=zukunft`,
/// `trade=<unbekannt>` werden still verworfen, ohne Hinweis; das Modell
/// öffnet sauber.
#[test]
fn a203_datei_hide() {
    let (mut s, _, _) = pruefhaus(203, 0.0);
    let ohne = sk_model::szo::write(s.model());
    assert!(!ohne.contains("[hide]"));
    let g = guid(&s, "AW-002");
    let wdvs = gewerk(&s, "18345");
    let tg = s.model().trade(wdvs).unwrap().guid;
    let de = guid(&s, "DE-001");
    sicht(&mut s, |v| {
        v.isolate = Some(Isolate::Elements([de].into_iter().collect()))
    });
    assert_eq!(
        sk_model::szo::write(s.model()),
        ohne,
        "Isolieren nie in der Datei"
    );
    sicht(&mut s, |v| {
        v.isolate = None;
        v.hidden.insert(g);
        v.hidden_cat.insert(Category::InteriorWall);
        v.hidden_trade.insert(wdvs);
        v.terrain_hidden = true;
    });
    let text = sk_model::szo::write(s.model());
    assert!(text.starts_with("SZO 4\n"));
    let zeilen: Vec<&str> = text.lines().filter(|l| l.starts_with("[hide]")).collect();
    assert_eq!(zeilen.len(), 4, "{zeilen:?}");
    assert!(zeilen.iter().any(|l| l.contains(&format!("elem={g}"))));
    assert!(zeilen.iter().any(|l| l.contains("cat=interior")));
    assert!(zeilen.iter().any(|l| l.contains(&format!("trade={tg}"))));
    assert!(zeilen.iter().any(|l| l.contains("terrain=1")));
    let wieder = lesen(&text);
    assert!(wieder.hints.is_empty());
    assert_eq!(sk_model::szo::write(&wieder.model), text, "Rundlauf");
    assert_eq!(wieder.model.visibility(), s.model().visibility());

    let fremd = format!(
        "{text}[hide] elem=00000000-0000-0000-0000-00000000abcd\n[hide] cat=zukunft\n\
             [hide] trade=00000000-0000-0000-0000-00000000abce\n"
    );
    let l = lesen(&fremd);
    assert!(l.hints.is_empty(), "nur Ansicht: still");
    assert!(l.model.check().is_empty());
    assert_eq!(l.model.visibility(), s.model().visibility());
}

/// A205 (§3.5, §5.6/5.7): Ein Strahl durch die ausgeblendete AW-002
/// trifft dahinter die Innenwand bzw. nichts, nie AW-002. Bei Isolieren
/// trifft er nur Isoliertes. Im Schnitt trifft ein Klick auf die
/// ausgeblendete Frostschürze nichts. `Picking::validate` nimmt ein
/// ausgeblendetes Bauteil aus der Auswahl und aus dem Überfahren.
#[test]
fn a205_picking_ueberspringt() {
    let (mut s, _, _) = pruefhaus(205, 0.0);
    let aw2 = nr(&s, "AW-002");
    let iw = nr(&s, "IW-001");
    // von Norden waagerecht auf Brusthöhe durch AW-002 auf die Innenwand?
    // Die Innenwand steht bei x = 5 m quer: Strahl von Nord nach Süd bei
    // x = 5 m trifft erst AW-002, dann die Innenwand.
    let o = vec3(5000.0, 20000.0, 1000.0);
    let d = vec3(0.0, -1.0, 0.0);
    assert_eq!(s.raycast(o, d).map(|x| x.1), Some(aw2));
    let g = guid(&s, "AW-002");
    sicht(&mut s, |v| {
        v.hidden.insert(g);
    });
    assert_eq!(
        s.raycast(o, d).map(|x| x.1),
        Some(iw),
        "durch die Ausgeblendete"
    );
    let gi = guid(&s, "AW-004");
    sicht(&mut s, |v| {
        v.hidden.clear();
        v.isolate = Some(Isolate::Elements([gi].into_iter().collect()));
    });
    assert_eq!(
        s.raycast(o, d).map(|x| x.1),
        Some(nr(&s, "AW-004")),
        "nur Isoliertes"
    );

    let fs = nr(&s, "FS-001");
    let gfs = guid(&s, "FS-001");
    sicht(&mut s, |v| {
        v.isolate = None;
        v.hidden.insert(gfs);
    });
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let c = fit_parallel(ViewKind::Section, s.bounds(), W, H);
    for p in [vec3(175.0, 4000.0, -500.0), vec3(175.0, 4000.0, -600.0)] {
        let (x, y) = px(&c, p);
        let t = selection::pick_at(&mut s, &c, ViewKind::Section, sect.plane(), x, y, W, H);
        assert_ne!(t, Some(fs), "Frostschürze ausgeblendet");
    }

    let mut pk = crate::picking::Picking {
        selected: vec![aw2, fs],
        hover: Some(fs),
        ..Default::default()
    };
    assert!(pk.validate(&s));
    assert_eq!(pk.selected, vec![aw2]);
    assert_eq!(pk.hover, None);
}

/// A206 (§1, Entscheidung 13): Ein neu gezeichnetes Bauteil ist immer
/// sichtbar, auch nach `[hide] elem=…` eines anderen. Ist seine Art
/// ausgeblendet, erscheint es nicht.
#[test]
fn a206_neues_bauteil() {
    let (mut s, eg, _) = pruefhaus(206, 0.0);
    let g = guid(&s, "IW-001");
    sicht(&mut s, |v| {
        v.hidden.insert(g);
    });
    let neu = |s: &mut Scene, cat: Category, pts: [Vec3; 2]| -> ElementId {
        let vorher: Vec<ElementId> = s.model().elements().iter().map(|(id, _)| id).collect();
        assert!(s.edit_model("Wand zeichnen", |m| {
            let st = m.run(eg).unwrap().storey;
            let t = m.default_type(sk_model::TypeCategory::of(cat).unwrap());
            m.add_wall_run(&pts, false, RefSide::Center, st, t, cat)
                .is_some()
        }));
        s.model()
            .elements()
            .iter()
            .map(|(id, _)| id)
            .find(|id| !vorher.contains(id))
            .unwrap()
    };
    let iw2 = neu(
        &mut s,
        Category::InteriorWall,
        [vec3(0.0, 4000.0, 0.0), vec3(5000.0, 4000.0, 0.0)],
    );
    assert_eq!(zeigt(&s, iw2).0, Shown::All, "neu ist sichtbar");
    sicht(&mut s, |v| {
        v.hidden_cat.insert(Category::ExteriorWall);
    });
    let aw = neu(
        &mut s,
        Category::ExteriorWall,
        [vec3(12000.0, 0.0, 0.0), vec3(15000.0, 0.0, 0.0)],
    );
    assert_eq!(zeigt(&s, aw).0, Shown::None, "Art ausgeblendet");
}

/// A207 (§3.2, §3.5): Ausblenden filtert beim Zusammensetzen. Mit
/// ausgeblendeten Außenwänden hat das 3D-Netz weniger Flächen, mit allen
/// Arten ausgeblendet keine Bauteilfläche mehr; die Körper im
/// Zwischenspeicher bleiben (`Scene::mesh` danach wieder wie vorher).
/// Isolieren: das deckende Netz enthält nur DE-001, alles andere liegt im
/// Geist-Netz; Summe der Flächen wie ohne Isolieren.
#[test]
fn a207_netz_filter_und_geist() {
    let (mut s, _, _) = pruefhaus(207, 0.0);
    let voll = flaechen(&s.mesh(ViewKind::Persp, None, &[]));
    assert!(voll > 0);
    assert_eq!(flaechen(&geist(&mut s)), 0, "ohne Isolieren kein Geist");
    sicht(&mut s, |v| {
        v.hidden_cat.insert(Category::ExteriorWall);
    });
    let ohne_aw = flaechen(&s.mesh(ViewKind::Persp, None, &[]));
    assert!(ohne_aw < voll, "{ohne_aw} < {voll}");
    sicht(&mut s, |v| {
        for c in Category::ALL {
            v.hidden_cat.insert(c);
        }
    });
    assert_eq!(flaechen(&s.mesh(ViewKind::Persp, None, &[])), 0);
    let de = guid(&s, "DE-001");
    sicht(&mut s, |v| {
        *v = Visibility::default();
        v.isolate = Some(Isolate::Elements([de].into_iter().collect()));
    });
    let deckend = flaechen(&s.mesh(ViewKind::Persp, None, &[]));
    let blass = flaechen(&geist(&mut s));
    assert!(deckend > 0 && blass > 0);
    assert_eq!(deckend + blass, voll, "nichts verloren, nichts doppelt");
    sicht(&mut s, |v| *v = Visibility::default());
    assert_eq!(flaechen(&s.mesh(ViewKind::Persp, None, &[])), voll);
}

/// Kanten des 3D-Netzes mit beiden Enden auf Höhe `z` (±1 mm) und
/// `y` (±1 mm): Gesamtlänge in mm.
fn kantenlaenge(m: &sk_render::MeshData, y: f32, z: f32) -> f32 {
    m.edges
        .iter()
        .filter(|(e, _)| {
            e.iter()
                .all(|p| (p[1] - y).abs() < 1.0 && (p[2] - z).abs() < 1.0)
        })
        .map(|(e, _)| ((e[0][0] - e[1][0]).powi(2) + (e[0][1] - e[1][1]).powi(2)).sqrt())
        .sum()
}

/// A208 (Review 3a G1): Kanten am Stoß gestapelter Züge. Ist die
/// OG-Wand AW-006 ausgeblendet oder Geist (Isolieren von AW-002), behält
/// die EG-Wand AW-002 ihre obere Kontur: Außenkante y = 8000 auf +2,855
/// über die ganze Länge (mindestens 9,5 m). Im Schnitt mit ausgeblendeter
/// Frostschürze hat die Sohlplatte ihre Unterkante auf −0,22 am Rand.
#[test]
fn a208_kanten_am_stoss() {
    let (mut s, _, _) = pruefhaus(208, 0.0);
    let g6 = guid(&s, "AW-006");
    sicht(&mut s, |v| {
        v.hidden.insert(g6);
    });
    let m = s.mesh(ViewKind::Persp, None, &[]);
    let l = kantenlaenge(&m, 8000.0, 2855.0);
    assert!(
        l >= 9500.0,
        "obere Außenkante EG bei ausgeblendetem OG: {l}"
    );

    let g2 = guid(&s, "AW-002");
    sicht(&mut s, |v| {
        v.hidden.clear();
        v.isolate = Some(Isolate::Elements([g2].into_iter().collect()));
    });
    let m = s.mesh(ViewKind::Persp, None, &[]);
    let l = kantenlaenge(&m, 8000.0, 2855.0);
    assert!(l >= 9500.0, "obere Außenkante EG bei OG als Geist: {l}");

    let gfs = guid(&s, "FS-001");
    sicht(&mut s, |v| {
        v.isolate = None;
        v.hidden.insert(gfs);
    });
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let m = s.mesh(ViewKind::Section, sect.plane(), &[]);
    let unten: Vec<_> = m
        .edges
        .iter()
        .filter(|(e, _)| e.iter().all(|p| (p[2] + 220.0).abs() < 1.0))
        .collect();
    assert!(!unten.is_empty(), "Unterkante der Sohlplatte im Schnitt");
}

/// A209 (§3.7): Befehlszeile ohne Fenster. `--ausblenden AW-001,DE-001`
/// blendet beide Bauteile aus, `--ausblenden art=exterior` die Art,
/// `--ausblenden gewerk=18345` das Gewerk, `--gelaende-aus` das Gelände,
/// `--isolieren DE-001` / `art=interior` / `gewerk=18338` isoliert.
/// Unbekannte Nummer, Art oder Gewerk: Fehlertext, kein Absturz. Die
/// Datei bleibt unverändert, die Revision auch.
#[test]
fn a209_befehlszeile() {
    let (s, _, _) = pruefhaus(209, -1500.0);
    let rev = s.model().revision();
    let v = befehlszeile(&s, &["--ausblenden", "AW-001,DE-001"]).unwrap();
    assert_eq!(
        v.hidden,
        [guid(&s, "AW-001"), guid(&s, "DE-001")]
            .into_iter()
            .collect()
    );
    let v = befehlszeile(&s, &["--ausblenden", "art=exterior"]).unwrap();
    assert!(v.hidden_cat.contains(&Category::ExteriorWall));
    let v = befehlszeile(&s, &["--ausblenden", "gewerk=18345"]).unwrap();
    assert!(v.hidden_trade.contains(&gewerk(&s, "18345")));
    let v = befehlszeile(&s, &["--gelaende-aus"]).unwrap();
    assert!(v.terrain_hidden);
    let v = befehlszeile(&s, &["--isolieren", "DE-001"]).unwrap();
    assert_eq!(
        v.isolate,
        Some(Isolate::Elements(
            [guid(&s, "DE-001")].into_iter().collect()
        ))
    );
    let v = befehlszeile(&s, &["--isolieren", "art=interior"]).unwrap();
    assert_eq!(v.isolate, Some(Isolate::Category(Category::InteriorWall)));
    let v = befehlszeile(&s, &["--isolieren", "gewerk=18338"]).unwrap();
    assert_eq!(v.isolate, Some(Isolate::Trade(gewerk(&s, "18338"))));
    let v = befehlszeile(
        &s,
        &[
            "--ausblenden",
            "AW-002",
            "--gelaende-aus",
            "--isolieren",
            "DE-001",
        ],
    )
    .unwrap();
    assert!(v.terrain_hidden && v.hidden.len() == 1 && v.isolate.is_some());
    for falsch in ["AW-099", "art=zukunft", "gewerk=99999"] {
        assert!(
            befehlszeile(&s, &["--ausblenden", falsch]).is_err(),
            "{falsch}"
        );
    }
    assert_eq!(s.model().revision(), rev);
    assert!(befehlszeile(&s, &[]).unwrap() == Visibility::default());
}

/// A210 (§3.3, Geometriekern): Stempel an Dreieck und Kante. Am Haus
/// mit AW-49: jede Fläche und jede Kante des Wandzugs trägt ein gültiges
/// Bauteil (`part_of` ist ein Bauteil), jede Wandfläche einen
/// Schichtindex unter der Schichtzahl oder `NO_LAYER`; jede Schicht mit
/// Körper hat Flächen (die Luftschicht nicht). DT: Belag 0, Dämmung 1.
#[test]
fn a210_stempel_schicht_und_bauteil() {
    let (mut s, eg, _) = pruefhaus(210, -1500.0);
    let t = typ(&s, "AW-49");
    assert!(s.edit_model("Wandtyp", |m| m.set_run_type(eg, t)));
    let k = koerper(&mut s, eg);
    let m = s.model();
    assert!(!k.triangles.is_empty() && !k.edges.is_empty());
    let mut je_schicht = [0usize; 4];
    for tri in &k.triangles {
        let id = m.part_of(eg, tri.elem).expect("Fläche ohne Bauteil");
        if m.element(id).unwrap().category == Category::ExteriorWall {
            let l = schicht(tri);
            if l != KEINE_SCHICHT {
                assert!((l as usize) < 4, "Schicht {l}");
                je_schicht[l as usize] += 1;
            }
        }
    }
    for e in &k.edges {
        assert!(m.part_of(eg, kante_teil(e)).is_some(), "Kante ohne Bauteil");
    }
    assert!(
        je_schicht[0] > 0 && je_schicht[2] > 0 && je_schicht[3] > 0,
        "{je_schicht:?}"
    );
    assert_eq!(je_schicht[1], 0, "Luft ohne Körper");
    let dt = nr(&s, "DT-001");
    let schichten: std::collections::BTreeSet<u8> = k
        .triangles
        .iter()
        .filter(|t| m.part_of(eg, t.elem) == Some(dt))
        .map(schicht)
        .collect();
    assert!(schichten.is_empty() || schichten.is_subset(&[0, 1].into_iter().collect()));
}

/// Netz ohne Reihenfolge: Flächen und Kanten sortiert.
fn kanonisch(m: &sk_render::MeshData) -> (Vec<[u32; 9]>, Vec<[u32; 7]>) {
    let mut f: Vec<[u32; 9]> = m.faces.iter().map(|t| t.map(f32::to_bits)).collect();
    let mut e: Vec<[u32; 7]> = m
        .edges
        .iter()
        .map(|([a, b], k)| {
            [
                a[0].to_bits(),
                a[1].to_bits(),
                a[2].to_bits(),
                b[0].to_bits(),
                b[1].to_bits(),
                b[2].to_bits(),
                k.to_bits(),
            ]
        })
        .collect();
    f.sort_unstable();
    e.sort_unstable();
    (f, e)
}

/// Netz derselben Lage, ganz neu gerechnet (ohne gemerkte Teilergebnisse).
fn frisch(s: &Scene, view: ViewKind, except: &[RunId]) -> sk_render::MeshData {
    Scene::with_model(s.model().clone()).mesh(view, None, except)
}

/// A303 (nach Paket 9 Punkt 3, Review 3h Befund 2; Darstellung §2.4,
/// Koordinator 22:13): Ganze Art bzw. ganzes Gewerk ausblenden mit
/// gemerktem Teilergebnis je Zug. Das Netz gleicht in jedem Schritt dem
/// ganz neu gerechneten: nach dem Ausblenden, beim Ziehen (Vorschau ohne
/// den gezogenen Zug), nach dem Ziehen, nach Rückgängig, nach einem
/// Musterwechsel, nach geänderter lichter Höhe und nach dem Einblenden.
/// In 3D und in der Ansicht von vorne. Mengen bleiben gleich.
#[test]
fn a303_ganze_art_ausblenden_ohne_alte_geometrie() {
    type Aus = fn(&mut Visibility, sk_model::TradeId);
    let faelle: [(&str, Aus); 3] = [
        ("Decken", |v, _| {
            v.hidden_cat.insert(Category::Floor);
        }),
        ("Außenwände", |v, _| {
            v.hidden_cat.insert(Category::ExteriorWall);
        }),
        ("Mauerarbeiten", |v, t| {
            v.hidden_trade.insert(t);
        }),
    ];
    for (name, aus) in faelle {
        let mut s = Scene::with_model(Model::with_seed(303));
        let (eg, og) = gebaeude(&mut s);
        let maurer = gewerk(&s, "18330");
        let aw6 = nr(&s, "AW-006");
        let mengen = csv(&mut s);
        for view in [ViewKind::Persp, ViewKind::Front] {
            let pruefe = |s: &mut Scene, schritt: &str, except: &[RunId]| {
                let ist = s.mesh(view, None, except);
                assert_eq!(
                    kanonisch(&ist),
                    kanonisch(&frisch(s, view, except)),
                    "{name}, {view:?}: {schritt}"
                );
            };
            let voll = kanonisch(&s.mesh(view, None, &[]));
            sicht(&mut s, |v| aus(v, maurer));
            pruefe(&mut s, "ausgeblendet", &[]);
            assert_ne!(
                kanonisch(&s.mesh(view, None, &[])),
                voll,
                "{name}: Netz ändert sich"
            );

            // Ziehen: Vorschau ohne den gezogenen Zug, danach das Ergebnis
            pruefe(&mut s, "Vorschau beim Ziehen", &[og]);
            assert!(s.edit_model("Wand verschoben", |m| {
                m.set_linked(aw6, false) && m.set_offset(aw6, 300.0).is_some()
            }));
            pruefe(&mut s, "nach dem Ziehen", &[]);
            assert_eq!(s.undo_label(), Some("Wand verschoben"));
            assert!(s.undo());
            pruefe(&mut s, "nach Rückgängig", &[]);

            let vb = s
                .model()
                .materials()
                .iter()
                .find(|(_, x)| x.name == sk_model::proctex::FACING)
                .map(|(_, x)| x.surface)
                .expect("Verblender");
            let muster_vorher = s.model().attr().surface(vb).unwrap().pattern.clone();
            assert!(s.edit_model("Einstellungen geändert", |m| {
                m.set_surface_pattern(vb, Some(sk_model::proctex::masonry_default()))
            }));
            pruefe(&mut s, "nach Musterwechsel", &[]);

            let eg_geschoss = s.model().run(eg).map(|r| r.storey).expect("EG");
            let hoehe_vorher = s.model().clear_height(eg_geschoss);
            assert!(s.edit_model("lichte Höhe EG", |m| m
                .set_clear_height(eg_geschoss, 2750.0)));
            pruefe(&mut s, "nach lichter Höhe", &[]);
            assert!(s.edit_model("lichte Höhe EG", |m| m
                .set_clear_height(eg_geschoss, hoehe_vorher)));
            pruefe(&mut s, "Höhe zurück", &[]);
            assert!(s.edit_model("Einstellungen geändert", |m| m
                .set_surface_pattern(vb, muster_vorher.clone())));
            pruefe(&mut s, "Muster zurück", &[]);

            sicht(&mut s, |v| *v = Visibility::default());
            pruefe(&mut s, "eingeblendet", &[]);
            assert_eq!(
                kanonisch(&s.mesh(view, None, &[])),
                voll,
                "{name}: wie vorher"
            );
        }
        assert_eq!(csv(&mut s), mengen, "{name}: Mengen unverändert");
    }
}

/// A303b (Review 3y): wie A303, aber im Grundriss und im Schnitt (schräge
/// Ebene durch die Mitte des Hauses), dazu Wiederholen und Typwechsel.
/// Jeder Schritt gleicht dem ganz neu gerechneten Netz.
#[test]
fn a303b_ausblenden_grundriss_schnitt_typwechsel() {
    let mut s = Scene::with_model(Model::with_seed(303));
    let (eg, og) = gebaeude(&mut s);
    let aw6 = nr(&s, "AW-006");
    let m0 = s.mesh(ViewKind::Persp, None, &[]);
    let n = m0.faces.len() as f32;
    let mut c = [0.0f32; 3];
    for f in &m0.faces {
        for k in 0..3 {
            c[k] += f[k] / n;
        }
    }
    let mitte = sk_math::vec3(c[0] as f64, c[1] as f64, c[2] as f64);
    let ebene = (mitte, sk_math::vec3(1.0, 0.2, 0.0).normalized());
    let typen: Vec<_> = s.model().layer_sets().iter().map(|(id, _)| id).collect();
    let ansichten = [
        (ViewKind::Plan, None),
        (ViewKind::Section, Some(ebene)),
        (ViewKind::Persp, None),
    ];
    for (view, sec) in ansichten {
        for cat in [Category::Floor, Category::ExteriorWall] {
            let pruefe = |s: &mut Scene, schritt: &str, except: &[RunId]| {
                let ist = s.mesh(view, sec, except);
                let neu = Scene::with_model(s.model().clone()).mesh(view, sec, except);
                assert_eq!(
                    kanonisch(&ist),
                    kanonisch(&neu),
                    "{cat:?} {view:?}: {schritt}"
                );
            };
            sicht(&mut s, |v| {
                v.hidden_cat.insert(cat);
            });
            pruefe(&mut s, "ausgeblendet", &[]);
            pruefe(&mut s, "Vorschau", &[og]);
            assert!(s.edit_model("Wand verschoben", |m| {
                m.set_linked(aw6, false) && m.set_offset(aw6, 300.0).is_some()
            }));
            pruefe(&mut s, "gezogen", &[]);
            assert!(s.undo());
            pruefe(&mut s, "Rückgängig", &[]);
            assert!(s.redo());
            pruefe(&mut s, "Wiederholen", &[]);
            assert!(s.undo());
            for t in typen.iter().take(4) {
                // Derselbe Typ ändert nichts und landet nicht im Verlauf
                if !s.edit_model("Wandtyp", |m| m.set_run_type(eg, *t))
                    || s.undo_label() != Some("Wandtyp")
                {
                    continue;
                }
                pruefe(&mut s, "Typwechsel", &[]);
                assert!(s.undo());
                pruefe(&mut s, "Typ zurück", &[]);
            }
            sicht(&mut s, |v| *v = Visibility::default());
            pruefe(&mut s, "eingeblendet", &[]);
        }
    }
}

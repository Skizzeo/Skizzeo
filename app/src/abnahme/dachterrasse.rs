use super::*;

// Abnahmetests A189–A194: Dachterrasse DT, Fläche und Aufbau (Commit 2a;
// bim/paket-dachterrasse.md E1–E3, Regeln 41–44, endgültige Sollwerte
// 08:54; Steckbrief bauteile/dt-dachterrasse.md; geometrie/
// machbarkeit-dachterrasse.md D0a/D1). Spezifikation:
// test/abnahme-dachterrasse.md. Setzt R4a und 1a voraus
// (`element_layers`, `layer_trade`, `layer_kg`) und
// test/patches/element-bounds.patch.
//
// Einbau: als `mod dachterrasse { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, decke, r4.
//
// Prüfhaus wie paket-og-phase2 §8: Dialog, 10 × 8 m, AW-31,5 (WDVS 140 +
// Porenbeton 175), Standardhöhen (OK Rohdecke EG +2,855). OG-Wände AW-005
// West, AW-006 Nord, AW-007 Ost, AW-008 Süd über AW-001 … 004.
//
// Angenommene Namen stehen nur in den Adaptern: `Model::terrace_of`,
// `Model::terrace_outlines`, `Scene::terrace_qto` mit `area,
// insulation_volume, finish_volume`, Merkmal „begehbar“ über
// `Model::props_of`, `Model::set_floor_upstand`, `Category::RoofTerrace`,
// `TypeCategory::RoofTerrace`.

use sk_model::{Category, ElementId, LayerFunction, PropValue, RunId, TypeCategory};

// ===== Adapter 2a =====

/// Dachterrasse auf der Decke `de` (`None`: keine).
fn dt(s: &Scene, de: ElementId) -> Option<ElementId> {
    s.model().terrace_of(de)
}

/// Zahl der zusammenhängenden Terrassenstücke über dem EG-Zug (ein Ring
/// ringsum zählt als ein Stück).
fn stuecke(s: &Scene, eg: RunId) -> usize {
    s.model().terrace_outlines(eg).len()
}

/// (Fläche m², Dämmung m³, Belag m³), auf 4 Stellen.
fn dt_mengen(s: &Scene, id: ElementId) -> (f64, f64, f64) {
    let q = s.terrace_qto(id).expect("Mengen der Dachterrasse");
    (
        r4(q.area / 1e6),
        r4(q.insulation_volume / 1e9),
        r4(q.finish_volume / 1e9),
    )
}

fn begehbar(s: &Scene, id: ElementId) -> bool {
    matches!(
        s.model().props_of(id).get("begehbar"),
        Some(PropValue::Bool(true))
    )
}

/// Attikahöhe über OK Belag an der Decke (mm), ein Schritt.
fn attika(s: &mut Scene, de: ElementId, mm: f64) -> bool {
    s.edit_model("Attikahöhe", |m| m.set_floor_upstand(de, mm))
}

fn koerper_z(s: &mut Scene, id: ElementId) -> (f64, f64) {
    let (lo, hi) = s.element_bounds(id).expect("Körper");
    (lo.z.round(), hi.z.round())
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

fn hat(s: &Scene, nummer: &str) -> bool {
    s.model().elements().iter().any(|(_, e)| e.number == nummer)
}

fn zahl(s: &Scene, c: Category) -> usize {
    s.model()
        .elements()
        .iter()
        .filter(|(_, e)| e.category == c)
        .count()
}

fn guid(s: &Scene, id: ElementId) -> sk_model::Guid {
    s.model().element(id).unwrap().guid
}

fn pruefung(s: &Scene) {
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// OG-Wand lösen und um `d` mm versetzen (+ außen), je ein Schritt.
fn versetzen(s: &mut Scene, wand: &str, d: f64) {
    let w = nr(s, wand);
    if s.model().stack_offset(w).is_some_and(|(_, l)| l) {
        assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
    }
    assert!(
        s.edit_model("Wand verschoben", |m| m.move_segment(w, d).is_some()),
        "{wand} um {d}"
    );
}

/// Prüfhaus; `rueck`: OG-Wände, die um je 1,50 m zurückspringen.
fn pruefhaus(seed: u64, rueck: &[&str]) -> (Scene, RunId, RunId) {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let (eg, og) = gebaeude(&mut s);
    for w in rueck {
        versetzen(&mut s, w, -1500.0);
    }
    pruefung(&s);
    (s, eg, og)
}

fn typ(s: &Scene, code: &str) -> sk_model::LayerSetId {
    s.model()
        .layer_sets()
        .iter()
        .find(|(_, t)| t.code == code)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Typ {code}"))
}

/// Zahlenfeld im Paneel „Aufbau“: Dicke der Schicht `i` des Projekttyps
/// der Terrasse (0 Belag, 1 Dämmung), ein Schritt; `false`: abgelehnt.
fn aufbau_dicke(s: &mut Scene, i: usize, mm: f64) -> bool {
    let id = typ(s, "DT-14");
    let mut t = s.model().layer_set(id).unwrap().clone();
    t.layers[i].thickness = mm;
    s.edit_model("Aufbau geändert", |m| m.set_layer_set(id, t.clone()))
}

/// A189 (D0a, Regel 41): Die Terrassenfläche entsteht aus den
/// Rücksprüngen, zusammenhängend über Ecken. Nord −1,50: ein Stück,
/// 9,72 × 1,36 = 13,2192 m² (Deckenkante bis Außenfläche OG-WDVS, Enden
/// an den Kernaußenflächen der Nachbarn). Nord und Ost über Eck: ein
/// Stück, 75,0384 − 8,36 × 6,36 = 21,8688 m². Nord und Süd: zwei Stücke,
/// 26,4384 m². Ringsum: ein Ring, 75,0384 − 7,00 × 5,00 = 40,0384 m².
/// Ein DT je Decke (E3), egal wie viele Stücke.
#[test]
fn a189_terrassenflaeche_aus_den_ruecksprungen() {
    for (seed, rueck, flaeche, n) in [
        (189, &["AW-006"][..], 13.2192, 1),
        (1890, &["AW-006", "AW-007"][..], 21.8688, 1),
        (1891, &["AW-006", "AW-008"][..], 26.4384, 2),
        (
            1892,
            &["AW-005", "AW-006", "AW-007", "AW-008"][..],
            40.0384,
            1,
        ),
    ] {
        let (s, eg, _) = pruefhaus(seed, rueck);
        let de = decke(&s, eg).unwrap();
        let id = dt(&s, de).unwrap_or_else(|| panic!("{rueck:?}: DT"));
        assert_eq!(dt_mengen(&s, id).0, flaeche, "{rueck:?}");
        assert_eq!(stuecke(&s, eg), n, "{rueck:?}: Stücke");
        assert_eq!(
            zahl(&s, Category::RoofTerrace),
            1,
            "{rueck:?}: ein DT je Decke"
        );
        assert_eq!(s.model().element(id).unwrap().number, "DT-001");
        assert!(dt(&s, decke(&s, s.model().runs_above(eg)[0]).unwrap()).is_none());
    }
}

/// Haus 10 × 8 direkt im Modell, Punkte im Uhrzeigersinn oder dagegen.
fn haus_richtung(seed: u64, gegen: bool) -> (Scene, RunId, RunId) {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let mut pts = vec![
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    if gegen {
        pts.reverse();
    }
    let mut eg = None;
    assert!(s.edit_model("Gebäude erstellt", |m| {
        let b = m.add_building(2);
        eg = m.build_from_polygon(b, &pts);
        eg.is_some()
    }));
    let eg = eg.unwrap();
    let og = s.model().runs_above(eg)[0];
    (s, eg, og)
}

/// OG-Wand, deren Segment bei y = 8000 liegt (Nordwand).
fn nordwand(s: &Scene, og: RunId) -> ElementId {
    let r = s.model().run(og).unwrap();
    let n = r.points.len();
    (0..n)
        .find(|&k| {
            let (p, q) = (r.points[k], r.points[(k + 1) % n]);
            (p.y - 8000.0).abs() < 1.0 && (q.y - 8000.0).abs() < 1.0
        })
        .map(|k| r.segments[k])
        .unwrap()
}

/// A189b (D0a): Die Terrasse folgt der Umlaufrichtung. `build_from_polygon`
/// legt die Wände links der Punkte an (RefSide::Left): im Uhrzeigersinn
/// steht das Haus innerhalb der Punkte (10 × 8, Terrasse 9,72 × 1,36 =
/// 13,2192 m²), gegen den Uhrzeigersinn außerhalb (10,63 × 8,63,
/// Terrasse zwischen den Kernaußenflächen 10,35 × 1,36 = 14,0760 m²).
/// In beiden Fällen liegt sie auf der Decke zwischen OG-Außenfläche und
/// Deckenkante (Probe auf 952a6fa).
#[test]
fn a189b_beide_umlaufrichtungen() {
    for (gegen, soll) in [
        (false, (13.2192, 1.0575, 0.7932)),
        (true, (14.076, 1.1261, 0.8446)),
    ] {
        let (mut s, eg, og) = haus_richtung(1893, gegen);
        let w = nordwand(&s, og);
        assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
        assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, -1500.0).is_some()));
        pruefung(&s);
        let id = dt(&s, decke(&s, eg).unwrap()).expect("DT");
        assert_eq!(dt_mengen(&s, id), soll, "gegen = {gegen}");
    }
}

/// A190 (D1, Regeln 43/44, Sollwerte 08:54): Nord −1,50 ergibt DT-001
/// mit 13,2192 m², Dämmung 1,0575 m³, Belag 0,7932 m³, begehbar.
/// Aufbau aus dem Werkstyp „Dachterrasse 14“ (DT-14, Typart DT, ohne
/// Kern): Belag 60 über Dämmung hart 80, Gewerk 18338 und KG 363 je
/// Schicht. Körper von OK Rohdecke +2,855 bis OK Belag +2,995. DE-002
/// 60,4584 m², DE-001 75,0384 m², keine UD. Einordnung DT, IfcCovering
/// ROOFING, KG 363. Höhen nur relativ: EG 30 cm höher → +3,155 …
/// +3,295. Dämmung 100 im Paneel → 1,3219 m³, OK Belag +3,015;
/// Grenzen Dämmung 40–300, Belag 20–150.
#[test]
fn a190_aufbau_und_mengen() {
    let (mut s, eg, og) = pruefhaus(190, &["AW-006"]);
    let de = decke(&s, eg).unwrap();
    let id = dt(&s, de).expect("DT");
    let m = s.model();
    let e = m.element(id).unwrap();
    assert_eq!(
        (e.number.as_str(), e.category),
        ("DT-001", Category::RoofTerrace)
    );
    assert_eq!(dt_mengen(&s, id), (13.2192, 1.0575, 0.7932));
    assert!(begehbar(&s, id));
    let c = Category::RoofTerrace;
    assert_eq!(
        (c.prefix(), c.ifc_class(), c.din276()),
        ("DT", "IfcCovering.ROOFING", Some(363))
    );
    let t = s.model().layer_set(typ(&s, "DT-14")).unwrap();
    assert_eq!(
        (t.name.as_str(), t.category),
        ("Dachterrasse 14", TypeCategory::RoofTerrace)
    );
    let m = s.model();
    let aufbau: Vec<(String, f64, LayerFunction, bool)> = m
        .element_layers(id)
        .iter()
        .map(|l| {
            (
                m.material(l.material).unwrap().name.clone(),
                l.thickness,
                l.function,
                l.core,
            )
        })
        .collect();
    assert_eq!(
        aufbau,
        [
            (
                "Terrassenbelag".to_string(),
                60.0,
                LayerFunction::Finish,
                false
            ),
            (
                "Dämmung hart (Terrasse)".to_string(),
                80.0,
                LayerFunction::Insulation,
                false
            ),
        ]
    );
    for i in 0..2 {
        let gw = m
            .layer_trade(id, i)
            .and_then(|t| m.trade(t))
            .map(|t| t.code.clone());
        assert_eq!(gw.as_deref(), Some("18338"), "Schicht {i}");
        assert_eq!(m.layer_kg(id, i), Some(363), "Schicht {i}");
    }
    assert_eq!(r4(s.floor_qto(og).unwrap().area / 1e6), 60.4584, "DE-002");
    assert_eq!(r4(s.floor_qto(eg).unwrap().area / 1e6), 75.0384, "DE-001");
    assert_eq!(zahl(&s, Category::SoffitInsulation), 0);
    assert_eq!(koerper_z(&mut s, id), (2855.0, 2995.0));

    // Höhen nur relativ (Regel 43)
    let st = s.model().run(eg).unwrap().storey;
    let h = s.model().storey(st).unwrap().height;
    assert!(s.edit_model("Geschosshöhe", |m| m.set_storey_height(st, h + 300.0)));
    assert_eq!(koerper_z(&mut s, id), (3155.0, 3295.0));
    pruefung(&s);
    assert!(s.undo());

    // Paneel „Aufbau“: Dicken ändern den Projekttyp
    assert!(aufbau_dicke(&mut s, 1, 100.0));
    assert_eq!(dt_mengen(&s, id), (13.2192, 1.3219, 0.7932));
    assert_eq!(koerper_z(&mut s, id), (2855.0, 3015.0));
    pruefung(&s);
    assert!(s.undo());
    let schritt = s.undo_label();
    for (i, falsch) in [(1, 39.0), (1, 301.0), (0, 19.0), (0, 151.0)] {
        assert!(
            !aufbau_dicke(&mut s, i, falsch),
            "Schicht {i}: {falsch} abgelehnt"
        );
    }
    assert_eq!(s.undo_label(), schritt, "kein Schritt");
    assert_eq!(dt_mengen(&s, id), (13.2192, 1.0575, 0.7932));
}

/// A191 (E1, E3, Regel 41): Die Terrasse entsteht im selben Schritt wie
/// der Rücksprung und verschwindet mit ihm. Lichte Tiefe 10 mm (−150 bei
/// 140 WDVS): nichts; 20 mm (−160): DT mit 0,1944 m², nicht begehbar.
/// Rückgängig/Wiederherstellen: dieselbe Guid und Nummer. Wechsel auf
/// Vorsprung +0,30: DT-001 weg, eine UD entsteht; zurück auf −1,50:
/// DT-002 mit neuer Guid. Zweimal Rückgängig bringt DT-001 zurück.
/// Bündig setzen (Regel 36) lässt das DT verschwinden.
#[test]
fn a191_entsteht_und_verschwindet_mit_dem_ruecksprung() {
    let (mut s, eg, _) = pruefhaus(191, &[]);
    let de = decke(&s, eg).unwrap();
    assert!(dt(&s, de).is_none(), "bündig: keine Terrasse");
    versetzen(&mut s, "AW-006", -150.0);
    assert!(dt(&s, de).is_none(), "lichte Tiefe 10 mm");
    pruefung(&s);
    versetzen(&mut s, "AW-006", -10.0);
    let id = dt(&s, de).expect("lichte Tiefe 20 mm");
    assert_eq!(dt_mengen(&s, id).0, 0.1944);
    assert!(!begehbar(&s, id));
    pruefung(&s);
    assert!(s.undo());
    assert!(dt(&s, de).is_none());

    versetzen(&mut s, "AW-006", -1350.0);
    let id = dt(&s, de).expect("−1,50");
    assert_eq!(dt_mengen(&s, id).0, 13.2192);
    // die Nummer DT-001 ist schon verbraucht (Regel 27): neue Nummer
    let n1 = s.model().element(id).unwrap().number.clone();
    let g1 = guid(&s, id);
    assert!(s.undo());
    assert!(dt(&s, de).is_none(), "Rückgängig nimmt die Terrasse mit");
    assert!(s.redo());
    let id = dt(&s, de).unwrap();
    assert_eq!(
        (guid(&s, id), s.model().element(id).unwrap().number.clone()),
        (g1, n1.clone())
    );

    versetzen(&mut s, "AW-006", 1800.0);
    assert!(dt(&s, de).is_none(), "Vorsprung: kein DT");
    assert_eq!(zahl(&s, Category::SoffitInsulation), 1, "UD entsteht");
    pruefung(&s);
    versetzen(&mut s, "AW-006", -1800.0);
    let id2 = dt(&s, de).expect("wieder Rücksprung");
    assert_ne!(guid(&s, id2), g1, "neues Bauteil");
    assert_ne!(s.model().element(id2).unwrap().number, n1, "neue Nummer");
    assert_eq!(zahl(&s, Category::SoffitInsulation), 0);
    assert!(s.undo());
    assert!(s.undo());
    let id = dt(&s, de).expect("DT von vorher");
    assert_eq!(
        (guid(&s, id), s.model().element(id).unwrap().number.clone()),
        (g1, n1)
    );

    let w = nr(&s, "AW-006");
    assert!(s.edit_model("Bündig gesetzt", |m| m.set_flush(w)));
    assert!(dt(&s, de).is_none(), "bündig gesetzt");
    assert!(!hat(&s, "DT-001") && !hat(&s, "DT-002"));
    pruefung(&s);
}

/// A192 (E3, Regel 42): Mischfall an derselben Decke. Nord −1,50 und
/// Süd +0,30: DT-001 (13,2192 m²) und UD-001 (0,30 × 9,72 = 2,9160 m²)
/// nebeneinander, Prüfung ohne Befund.
#[test]
fn a192_mischfall_ud_und_dt() {
    let (mut s, eg, _) = pruefhaus(192, &["AW-006"]);
    versetzen(&mut s, "AW-008", 300.0);
    let de = decke(&s, eg).unwrap();
    let id = dt(&s, de).expect("DT");
    assert_eq!(dt_mengen(&s, id).0, 13.2192);
    let ud = s.model().soffit_of(de).expect("UD");
    assert_eq!(r4(s.soffit_qto(ud).unwrap().area / 1e6), 2.916);
    assert_eq!(
        (
            zahl(&s, Category::RoofTerrace),
            zahl(&s, Category::SoffitInsulation)
        ),
        (1, 1)
    );
    pruefung(&s);
}

fn lesen(text: &str) -> sk_model::szo::Loaded {
    sk_model::szo::read(text, sk_model::GuidGen::with_seed(1)).expect("öffnet")
}

/// A193 (paket-dachterrasse §3, F-17): `.szo` bleibt 4. Mit DT stehen
/// `[terrace] guid number="DT-001" floor=<Guid DE-001>` und der
/// Projekttyp `[layerset] cat=roofterrace` mit zwei `[layer]`; `[floor]`
/// ohne `terrace=`/`upstand=`, solange beides Standard ist. Attika 100 →
/// `upstand=100`. Rundlauf bytegleich. Fehlt die `[terrace]`-Zeile,
/// ergänzt der Leser das DT mit Hinweis. Ohne Rücksprung steht nichts
/// davon in der Datei.
#[test]
fn a193_datei() {
    let (mut s, eg, _) = pruefhaus(193, &[]);
    let ohne = sk_model::szo::write(s.model());
    assert!(!ohne.contains("[terrace]") && !ohne.contains("cat=roofterrace"));
    versetzen(&mut s, "AW-006", -1500.0);
    let de = decke(&s, eg).unwrap();
    let text = sk_model::szo::write(s.model());
    assert!(text.starts_with("SZO 4\n"));
    let gde = s.model().element(de).unwrap().guid.to_string();
    let zeilen: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("[terrace]"))
        .collect();
    assert_eq!(zeilen.len(), 1);
    assert!(zeilen[0].contains("number=\"DT-001\"") && zeilen[0].contains(&format!("floor={gde}")));
    let typ_zeile = text
        .lines()
        .find(|l| l.starts_with("[layerset]") && l.contains(" cat=roofterrace "))
        .expect("Projekttyp DT-14");
    assert!(typ_zeile.contains("code=\"DT-14\""));
    let tguid = s
        .model()
        .layer_set(typ(&s, "DT-14"))
        .unwrap()
        .guid
        .to_string();
    assert_eq!(
        text.lines()
            .filter(|l| l.starts_with("[layer]") && l.contains(&format!("set={tguid}")))
            .count(),
        2
    );
    let fz = text
        .lines()
        .find(|l| l.starts_with("[floor]") && l.contains("number=\"DE-001\""))
        .unwrap();
    assert!(!fz.contains("terrace=") && !fz.contains("upstand="), "{fz}");
    assert_eq!(sk_model::szo::write(&lesen(&text).model), text, "Rundlauf");

    assert!(attika(&mut s, de, 100.0));
    let t2 = sk_model::szo::write(s.model());
    let fz = t2
        .lines()
        .find(|l| l.starts_with("[floor]") && l.contains("number=\"DE-001\""))
        .unwrap();
    assert!(fz.contains(" upstand=100"), "{fz}");
    assert_eq!(sk_model::szo::write(&lesen(&t2).model), t2, "Rundlauf");

    let ohne_zeile: String = text
        .lines()
        .filter(|l| !l.starts_with("[terrace]"))
        .map(|l| format!("{l}\n"))
        .collect();
    let geladen = lesen(&ohne_zeile);
    assert!(!geladen.hints.is_empty(), "Hinweis");
    let m = &geladen.model;
    assert_eq!(
        m.elements()
            .iter()
            .filter(|(_, e)| e.category == Category::RoofTerrace)
            .count(),
        1,
        "ergänzt"
    );
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// A194 (Löschregel, Regel 27): DT lässt sich nicht löschen, mit dem
/// Satz nach Einstellungen §1 (Koordinator 09:25); „Gebäude löschen?“
/// nennt „1 Dachterrasse“.
#[test]
fn a194_loeschen_abgelehnt() {
    let (s, eg, _) = pruefhaus(194, &["AW-006"]);
    let id = dt(&s, decke(&s, eg).unwrap()).unwrap();
    let r = s.model().can_delete(id).expect_err("nicht löschbar");
    assert_eq!(
        sk_model::refusal_text(s.model(), id, &r),
        "Die Dachterrasse folgt dem Rücksprung des OG. Ihren Aufbau stellst du im Paneel ein."
    );
    let b = s.model().buildings().iter().next().unwrap().0;
    assert!(crate::delete::parts_text(s.model(), b).contains("1 Dachterrasse"));
}

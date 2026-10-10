use super::*;

// Abnahmetest A128: Deckenauflager bearbeitbar (Umschalter, Tiefe, Baustoff
// des Streifens), Koordinator 22:32. Spezifikation: test/abnahme-wandtypen.md
// (A128), Handtest H111.
//
// Einbau: als `mod auflager_bearbeiten { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, og_zug, decke_mengen,
// wall_m3, m3. Die Bedienung des Katalogs (OK → ein Schritt „Bauteilkatalog
// geändert“) steht nur im Adapter `katalog`; baut der Bau dafür eine eigene
// Funktion, wird nur der Adapter angepasst.
//
// Sollwerte auf 27213bd gerechnet und von Hand nachgeprüft (Haus B11,
// 10 × 8 m, Geschoss 2,635 + Decke 0,22):
// - Decke 9,63 × 7,63 = 73,4769 m², × 0,22 = 16,164918 m³
// - Streifen 18,5 cm: Achsen 9,815 / 7,815 m, je Geschoss
//   35,26 × 0,185 × 0,22 = 1,435082 m³; Decke + Streifen = 17,6
// - Porenbeton netto 34,30 × 0,425 × 2,635 = 38,4117 m³ (Achse × Dicke ×
//   lichte Höhe, Tasche voll abgezogen, Streifen nicht doppelt)
// - IW-17,5: 7,15 × 0,175 × 2,635 = 3,2970 m³
// - U = 1 / (0,13 + 0,425/0,09 + 0,04) = 0,2044 → 0,20

use sk_model::catalog::{export_type, import_type, read_szk, write_szk, Library};
use sk_model::{Bearing, Category, ElementId, GuidGen, LayerSet, LayerSetId, Model, RunId};

// ===== Adapter =====

/// Katalog-Dialog mit OK: eine Änderung an den Typen, ein Rückgängig-Schritt.
fn katalog(s: &mut Scene, f: impl FnOnce(&mut Model) -> bool) -> bool {
    s.edit_types(crate::scene::CATALOG_STEP, f)
}

// ===== Hilfen =====

fn typ(m: &Model, code: &str) -> LayerSetId {
    m.layer_sets()
        .iter()
        .find(|(_, t)| t.code == code)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Typ {code} fehlt"))
}

fn r4(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

fn zugtyp(s: &mut Scene, run: RunId, id: LayerSetId) -> bool {
    s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id))
}

/// Randdämmstreifen: (Nummer, Guid, Breite mm, Achslänge mm), nach Nummer.
fn streifen(s: &Scene) -> Vec<(String, sk_model::Guid, f64, f64)> {
    let mut v: Vec<_> = s
        .model()
        .elements()
        .iter()
        .filter(|(_, e)| e.category == Category::EdgeInsulation)
        .map(|(id, e)| {
            let q = s.edge_strip_qto(id).expect("Streifenmengen");
            let breite = q.volume / q.length / 220.0;
            (e.number.clone(), e.guid, breite, q.length)
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn streifen_m3(s: &Scene, ids: &[ElementId]) -> f64 {
    ids.iter()
        .map(|&el| s.edge_strip_qto(el).unwrap().volume)
        .sum::<f64>()
        / 1e9
}

fn gasbeton_m3(s: &Scene, run: RunId) -> f64 {
    let m = s.model();
    let mut v = 0.0;
    for i in 0..4 {
        for l in &s.wall_qto(m.wall_at(run, i).unwrap()).unwrap().layers {
            if m.material(l.material).unwrap().name == "Porenbeton" {
                v += l.volume;
            }
        }
    }
    r4(v / 1e9)
}

fn auflager_mit(m: &Model, t: &LayerSet) -> Option<(f64, String)> {
    match t.bearing {
        Bearing::Core => None,
        Bearing::Depth { depth, strip } => Some((depth, m.material(strip).unwrap().name.clone())),
    }
}

/// Neuer Typ AW-42,5 (Porenbeton 42,5, Auflager 24 mit Randdämmung) über den
/// Katalog: Duplizieren von AW-36,5, Dicke, Kürzel, Name, Tiefe, OK.
fn neuer_typ(s: &mut Scene) -> LayerSetId {
    let mono = typ(s.model(), "AW-36,5");
    let mut neu = None;
    assert!(katalog(s, |m| {
        let Some(id) = m.duplicate_type(mono) else {
            return false;
        };
        let mut t = m.layer_set(id).unwrap().clone();
        t.layers[0].thickness = 425.0;
        t.code = "AW-42,5".into();
        t.name = "AW 42,5 Porenbeton monolithisch".into();
        let Bearing::Depth { strip, .. } = t.bearing else {
            return false;
        };
        t.bearing = Bearing::Depth {
            depth: 240.0,
            strip,
        };
        neu = Some(id);
        m.set_layer_set(id, t)
    }));
    neu.unwrap()
}

/// Ändert das Auflager eines Typs über den Katalog (ein Schritt).
fn setze_auflager(s: &mut Scene, id: LayerSetId, tiefe: Option<f64>) -> bool {
    katalog(s, |m| {
        let mut t = m.layer_set(id).unwrap().clone();
        let strip = match t.bearing {
            Bearing::Depth { strip, .. } => strip,
            Bearing::Core => typ_strip(m),
        };
        t.bearing = match tiefe {
            None => Bearing::Core,
            Some(depth) => Bearing::Depth { depth, strip },
        };
        m.set_layer_set(id, t)
    })
}

/// Baustoff „Randdämmung“ (Streifen von AW-36,5).
fn typ_strip(m: &Model) -> sk_model::MaterialId {
    match m.layer_set(typ(m, "AW-36,5")).unwrap().bearing {
        Bearing::Depth { strip, .. } => strip,
        Bearing::Core => panic!("AW-36,5 ohne Streifen"),
    }
}

/// A128: AW-42,5 (Porenbeton 42,5, Auflager 24, Streifen 18,5 aus
/// Randdämmung) entsteht im Katalog in einem Rückgängig-Schritt, U 0,20.
/// Am Haus B11: 8 Streifen 18,5 cm breit, Achsen 9,815/7,815, Decke
/// 73,4769 m² / 16,1649 m³, Decke + Streifen = 17,6, Porenbeton 38,4117,
/// IW 3,2970, SP/FS unverändert, `check()` leer, A-09. Tiefe 24 → 30 in einem
/// Schritt (Streifen 12,5, gleiche Guids), Rückgängig zurück auf 18,5.
/// Umschalter aus (ganzer Kern): keine Streifen, Decke 80 m² / 17,6;
/// Rückgängig bringt dieselben Streifen. Rundlauf .szo und .szk behalten
/// Tiefe und Baustoff des Streifens.
#[test]
fn a128_deckenauflager_bearbeiten() {
    let mut s = Scene::with_model(Model::with_seed(128));
    let (aw, iw) = haus_b11(&mut s);
    let i17 = typ(s.model(), "IW-17,5");
    assert!(zugtyp(&mut s, iw, i17));

    // Neuer Typ im Katalog: ein Schritt
    let typen = s.model().layer_sets().len();
    let id = neuer_typ(&mut s);
    assert_eq!(s.undo_label(), Some(crate::scene::CATALOG_STEP));
    let t = s.model().layer_set(id).unwrap().clone();
    assert_eq!((t.code.as_str(), t.thickness()), ("AW-42,5", 425.0));
    assert_eq!(
        auflager_mit(s.model(), &t),
        Some((240.0, "Randdämmung".to_string()))
    );
    assert_eq!(
        s.model().u_value(id).map(|u| (u * 100.0).round() / 100.0),
        Some(0.2)
    );
    assert!(s.undo());
    assert_eq!(s.model().layer_sets().len(), typen, "Rückgängig: Typ weg");
    assert!(s.redo());
    let id = typ(s.model(), "AW-42,5");

    // Am Haus
    assert!(zugtyp(&mut s, aw, id));
    let st = streifen(&s);
    assert_eq!(st.len(), 8);
    for (n, _, b, _) in &st {
        assert!((b - 185.0).abs() < 0.01, "{n} breit {b}");
    }
    let mut achsen: Vec<f64> = st.iter().map(|x| (x.3).round() / 1000.0).collect();
    achsen.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(
        achsen,
        [7.815, 7.815, 7.815, 7.815, 9.815, 9.815, 9.815, 9.815]
    );
    let og = og_zug(&s, aw);
    let m = s.model();
    assert_eq!(m.edge_strip_pairs().len(), 8);
    for run in [aw, og] {
        let (f, v, _) = decke_mengen(&s, run);
        assert_eq!(f, 73.4769);
        assert!((v - 16.164918).abs() < 1e-3, "Decke {v}");
        let walls: Vec<ElementId> = (0..4).map(|i| m.wall_at(run, i).unwrap()).collect();
        let eigene: Vec<ElementId> = m
            .elements()
            .iter()
            .filter(|(_, e)| e.category == Category::EdgeInsulation)
            .filter(|(el, _)| streifen_wand(m, *el).is_some_and(|w| walls.contains(&w)))
            .map(|(el, _)| el)
            .collect();
        assert_eq!(eigene.len(), 4);
        for el in &eigene {
            let w = streifen_wand(m, *el).unwrap();
            assert_eq!(
                m.element(*el).unwrap().storey,
                m.element(w).unwrap().storey,
                "A-09"
            );
        }
        let vs = streifen_m3(&s, &eigene);
        assert!((vs - 1.435082).abs() < 1e-4, "Streifen {vs}");
        assert!(
            (v + vs - 17.6).abs() < 1e-3,
            "Decke + Streifen = volle Tasche"
        );
        assert_eq!(gasbeton_m3(&s, run), 38.4117);
    }
    assert_eq!(r4(wall_m3(&s, iw, 0)), 3.297);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());

    // Tiefe 24 → 30: ein Schritt, gleiche Streifen (Guid), 12,5 breit
    let guids: Vec<_> = st.iter().map(|x| (x.0.clone(), x.1)).collect();
    assert!(setze_auflager(&mut s, id, Some(300.0)));
    assert_eq!(s.undo_label(), Some(crate::scene::CATALOG_STEP));
    let st2 = streifen(&s);
    assert_eq!(
        st2.iter().map(|x| (x.0.clone(), x.1)).collect::<Vec<_>>(),
        guids
    );
    assert!(st2.iter().all(|x| (x.2 - 125.0).abs() < 0.01), "12,5 breit");
    assert!(s.undo());
    assert!(
        streifen(&s).iter().all(|x| (x.2 - 185.0).abs() < 0.01),
        "zurück auf 18,5"
    );

    // Umschalter aus: ganzer Kern, keine Streifen
    assert!(setze_auflager(&mut s, id, None));
    assert!(streifen(&s).is_empty());
    let (f, v, _) = decke_mengen(&s, aw);
    assert_eq!(f, 80.0);
    assert!((v - 17.6).abs() < 1e-3);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert!(s.undo());
    assert_eq!(
        streifen(&s)
            .iter()
            .map(|x| (x.0.clone(), x.1))
            .collect::<Vec<_>>(),
        guids,
        "dieselben Streifen zurück"
    );

    // Rundlauf .szo
    let text = sk_model::szo::write(s.model());
    let l = sk_model::szo::read(&text, GuidGen::with_seed(1)).unwrap();
    assert!(l.hints.is_empty() && l.model.check().is_empty());
    assert_eq!(sk_model::szo::write(&l.model), text);
    let t2 = l.model.layer_set(typ(&l.model, "AW-42,5")).unwrap();
    assert_eq!(
        auflager_mit(&l.model, t2),
        Some((240.0, "Randdämmung".to_string()))
    );

    // Rundlauf .szk und Übernahme in ein anderes Projekt
    let g = s.model().layer_set(id).unwrap().guid;
    let mut lib = Library::default();
    assert!(export_type(s.model(), &mut lib, g));
    let szk = write_szk(&lib);
    let lib2 = read_szk(&szk).unwrap();
    assert_eq!(lib2, lib);
    assert_eq!(write_szk(&lib2), szk);
    let mut m2 = Model::with_seed(2);
    m2.allow_unstepped();
    let id2 = import_type(&mut m2, &lib2, g).expect("übernommen");
    let t3 = m2.layer_set(id2).unwrap();
    assert_eq!((t3.code.as_str(), t3.thickness()), ("AW-42,5", 425.0));
    assert_eq!(
        auflager_mit(&m2, t3),
        Some((240.0, "Randdämmung".to_string()))
    );
}

/// Wand eines Randdämmstreifens.
fn streifen_wand(m: &Model, el: ElementId) -> Option<ElementId> {
    match m.element(el)?.kind {
        sk_model::ElementKind::EdgeStrip { wall, .. } => Some(wall),
        _ => None,
    }
}

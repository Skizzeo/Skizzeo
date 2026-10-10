use super::*;

// Abnahmetest A129: Grenzen des bearbeitbaren Deckenauflagers, Prüfregel 21
// neu (BIM, bim/paket-k4-k5-wandtypen.md, „Nachtrag: Deckenauflager im
// Katalog bearbeitbar“, „Fertig, wenn“). Spezifikation:
// test/abnahme-wandtypen.md (A129), Handtest H111.
//
// Einbau: als `mod auflager_regel_21 { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, decke_mengen.
// Angenommener Name nur im Adapter: `Model::remove_material` (Baustoff
// löschen, `false` wenn benutzt).
//
// Prüfmuster wie A118: Ein unzulässiger Wert wird entweder von
// `set_layer_set` abgelehnt oder von `check()` gemeldet.

use sk_model::{Bearing, Category, GuidGen, LayerSet, LayerSetId, MaterialId, Model, RunId};

// ===== Adapter =====

/// Baustoff löschen; `false`, wenn er benutzt wird (Regel 15).
fn baustoff_loeschen(m: &mut Model, id: MaterialId) -> bool {
    m.remove_material(id)
}

// ===== Hilfen =====

fn typ(m: &Model, code: &str) -> LayerSetId {
    m.layer_sets()
        .iter()
        .find(|(_, t)| t.code == code)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Typ {code} fehlt"))
}

fn baustoff(m: &Model, name: &str) -> MaterialId {
    m.materials()
        .iter()
        .find(|(_, x)| x.name == name)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Baustoff {name} fehlt"))
}

/// Modell mit AW-42,5 (Duplikat von AW-36,5, Tiefe 24), ohne Rückgängig.
fn mit_42_5() -> (Model, LayerSetId) {
    let mut m = Model::with_seed(129);
    m.allow_unstepped();
    let id = m.duplicate_type(typ(&m, "AW-36,5")).unwrap();
    let mut t = m.layer_set(id).unwrap().clone();
    t.layers[0].thickness = 425.0;
    t.code = "AW-42,5".into();
    assert!(m.set_layer_set(id, t));
    (m, id)
}

/// Wendet `t` auf eine Kopie an: angenommen und ohne Befund?
fn sauber(m: &Model, id: LayerSetId, t: LayerSet) -> bool {
    let mut k = m.clone();
    k.allow_unstepped();
    k.set_layer_set(id, t) && k.check().is_empty()
}

fn mit_tiefe(m: &Model, id: LayerSetId, depth: f64, strip: MaterialId) -> LayerSet {
    let mut t = m.layer_set(id).unwrap().clone();
    t.bearing = Bearing::Depth { depth, strip };
    t
}

fn zugtyp(s: &mut Scene, run: RunId, id: LayerSetId) -> bool {
    s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id))
}

/// A129: Regel 21 neu. AW-42,5: Tiefe 9,5 und 41 abgelehnt oder gemeldet,
/// 10 und 40,5 sauber (Bereich innen + 10 cm … Dicke − 2 cm). „Fest“ an AW-36
/// (WDVS vor dem Kern) und an einer Innenwand (IW-24) nicht sauber.
/// Streifenbaustoff Porenbeton (Mauerwerk) nicht sauber, Randdämmung sauber.
/// Eine Datei mit Tiefe 40 an AW-36,5 lädt, der Wert bleibt, `check()`
/// meldet, gezeichnet wie ganze tragende Schicht: keine Streifen, Decke
/// 80 m². Randdämmung, nur als Streifenbaustoff benutzt, lässt sich nicht
/// löschen.
#[test]
fn a129_auflager_grenzen_regel_21() {
    let (m, id) = mit_42_5();
    let rd = baustoff(&m, "Randdämmung");
    for d in [95.0, 410.0] {
        assert!(
            !sauber(&m, id, mit_tiefe(&m, id, d, rd)),
            "Tiefe {d} muss scheitern"
        );
    }
    for d in [100.0, 240.0, 405.0] {
        assert!(sauber(&m, id, mit_tiefe(&m, id, d, rd)), "Tiefe {d} geht");
    }
    // Streifenbaustoff nur Dämmung
    let gas = baustoff(&m, "Porenbeton");
    assert!(
        !sauber(&m, id, mit_tiefe(&m, id, 240.0, gas)),
        "Porenbeton als Streifen"
    );
    // „Fest“ nur ohne Schicht vor dem Kern und nur an Außenwänden
    let aw36 = typ(&m, "AW-36");
    assert!(
        !sauber(&m, aw36, mit_tiefe(&m, aw36, 240.0, rd)),
        "AW-36 mit WDVS außen"
    );
    let iw24 = typ(&m, "IW-24");
    assert!(
        !sauber(&m, iw24, mit_tiefe(&m, iw24, 140.0, rd)),
        "Innenwand"
    );

    // Randdämmung dient nur als Streifenbaustoff (AW-36,5): nicht löschbar
    let mut k = m.clone();
    k.allow_unstepped();
    assert!(
        !baustoff_loeschen(&mut k, rd),
        "Streifenbaustoff ist benutzt"
    );
    assert!(k.materials().get(rd).is_some());

    // Datei mit ungültiger Tiefe 40 an AW-36,5
    let mut s = Scene::with_model(Model::with_seed(129));
    let (aw, _) = haus_b11(&mut s);
    let mono = typ(s.model(), "AW-36,5");
    assert!(zugtyp(&mut s, aw, mono));
    let text = sk_model::szo::write(s.model());
    let g = s.model().layer_set(mono).unwrap().guid.to_string();
    let mut n = 0;
    let kaputt: String = text
        .lines()
        .map(|l| {
            if l.starts_with("[layerset]") && l.contains(&format!("guid={g}")) {
                n += 1;
                l.replacen("bearing=240", "bearing=400", 1)
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(n, 1);
    assert!(kaputt.contains("bearing=400"));
    let l = sk_model::szo::read(&kaputt, GuidGen::with_seed(1)).expect("lädt");
    assert!(!l.model.check().is_empty(), "Regel 21 gemeldet");
    assert!(
        sk_model::szo::write(&l.model).contains("bearing=400"),
        "Wert bleibt gespeichert"
    );
    let t = Scene::with_model(l.model);
    assert!(
        !t.model()
            .elements()
            .iter()
            .any(|(_, e)| e.category == Category::EdgeInsulation),
        "keine Streifen"
    );
    let (f, v, _) = decke_mengen(&t, aw);
    assert_eq!(f, 80.0, "Decke bis zur Kernaußenseite");
    assert!((v - 17.6).abs() < 1e-3);
}

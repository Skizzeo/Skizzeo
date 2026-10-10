use super::*;
// Abnahmetest A130: Firmentyp mit ungültigem Deckenauflager (Review 1p,
// 3bd97c0). Ein Typ aus dem Firmenkatalog, dessen Auflager gegen Regel 21
// verstößt (Tiefe 40 an 36,5 bzw. Porenbeton als Streifen), kommt beim
// Übernehmen ins Projekt mit und wird gebaut wie „ganze tragende Schicht“,
// genau wie dieselbe Angabe aus einer .szo (A129).
// Spezifikation: test/abnahme-wandtypen.md (A130).
//
// Einbau: als `mod firmentyp_auflager { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, decke_mengen.
// Keine angenommenen Namen: Library::standard, write_szk/read_szk,
// import_type, Scene::edit_types(CATALOG_STEP), Model::set_run_type.

use sk_model::catalog::{import_type, read_szk, write_szk, Library};
use sk_model::{Category, Guid, GuidGen, LayerSetId, Model, RunId};

fn zugtyp(s: &mut Scene, run: RunId, id: LayerSetId) -> bool {
    s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id))
}

/// Firmenkatalog mit Duplikat von AW-36,5 („AW-F“, Guid 0x130), dessen
/// [layerset]-Zeile das Wort `ersetze` durch `durch` ersetzt bekommt; über den Text gelesen wie
/// eine Datei vom Datenträger.
fn firmenkatalog(ersetze: &str, durch: &str) -> (Library, Guid) {
    let mut lib = Library::standard();
    let mono = lib.type_by_guid(sk_model::MONO_TYPE_GUID).unwrap();
    let mut t = lib.types.get(mono).unwrap().clone();
    t.guid = Guid(0x130);
    t.code = "AW-F".into();
    t.name = "Firmentyp".into();
    lib.types.insert(t);
    let text = write_szk(&lib);
    let mut n = 0;
    let neu: String = text
        .lines()
        .map(|l| {
            if l.starts_with("[layerset]") && l.contains("code=\"AW-F\"") {
                n += 1;
                // ganzes Wort ersetzen; endet `ersetze` auf „=“, gilt jeder Wert
                let mut k = 0;
                let l = l
                    .split(' ')
                    .map(|w| {
                        if w == ersetze || (ersetze.ends_with('=') && w.starts_with(ersetze)) {
                            k += 1;
                            durch.to_string()
                        } else {
                            w.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                assert_eq!(k, 1, "{ersetze} genau einmal: {l}");
                l
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(n, 1);
    (read_szk(&neu).expect("liest"), Guid(0x130))
}

/// A130: Firmentyp „AW-F“ mit Auflagertiefe 40 (Bereich 10–34,5) bzw. mit
/// Porenbeton als Streifen: Übernehmen gelingt in einem Rückgängig-Schritt,
/// Tiefe bleibt 40, `check()` meldet, an den Wänden gibt es keine Streifen,
/// die Decke reicht bis zur Kernaußenseite (80 m², 17,6 m³), beim Speichern
/// steht bearing=400 in der Datei. Strg+Z nimmt den Typ wieder heraus.
#[test]
fn a130_firmentyp_mit_ungueltigem_auflager() {
    let gasbeton = Model::new()
        .materials()
        .iter()
        .find(|(_, x)| x.name == "Porenbeton")
        .map(|(_, x)| x.guid.to_string())
        .unwrap();
    for (fall, ersetze, durch) in [
        (
            "Tiefe 40",
            "bearing=240".to_string(),
            "bearing=400".to_string(),
        ),
        (
            "Porenbeton als Streifen",
            "strip=".to_string(),
            format!("strip={gasbeton}"),
        ),
    ] {
        let mut s = Scene::with_model(Model::with_seed(130));
        let (aw, _) = haus_b11(&mut s);
        let (lib, g) = firmenkatalog(&ersetze, &durch);
        let typen = s.model().layer_sets().len();
        assert!(
            s.edit_types(crate::scene::CATALOG_STEP, |m| import_type(m, &lib, g)
                .is_some()),
            "{fall}: Firmentyp kommt mit"
        );
        assert_eq!(s.model().layer_sets().len(), typen + 1, "{fall}");
        let id = s.model().type_by_guid(g).unwrap();
        assert!(
            s.model()
                .bearing_problem(s.model().layer_set(id).unwrap())
                .is_some(),
            "{fall}"
        );
        assert!(!s.model().check().is_empty(), "{fall}: Regel 21 gemeldet");
        assert!(zugtyp(&mut s, aw, id), "{fall}: Wände bekommen AW-F");
        assert!(
            !s.model()
                .elements()
                .iter()
                .any(|(_, e)| e.category == Category::EdgeInsulation),
            "{fall}: keine Streifen"
        );
        let (f, v, _) = decke_mengen(&s, aw);
        assert_eq!(f, 80.0, "{fall}: Decke bis zur Kernaußenseite");
        assert!((v - 17.6).abs() < 1e-3, "{fall}: {v}");
        // Speichern und Öffnen: Angabe bleibt, Modell gleich
        let text = sk_model::szo::write(s.model());
        assert!(text.contains(&durch), "{fall}: Wert bleibt gespeichert");
        let l = sk_model::szo::read(&text, GuidGen::with_seed(1)).expect("lädt");
        assert_eq!(sk_model::szo::write(&l.model), text, "{fall}: Rundlauf");
        // Rückgängig: erst die Wände, dann der Typ
        assert!(s.undo());
        assert!(s.undo());
        assert!(
            s.model().type_by_guid(g).is_none(),
            "{fall}: Strg+Z nimmt den Typ heraus"
        );
    }
}

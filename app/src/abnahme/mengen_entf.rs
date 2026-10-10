use super::*;

// Abnahmetest A138: Entf im Mengenfenster (H119 neu, Sollbild
// soll-loeschen-5, Jörns Freigabe 07.10. 03:23; Abschnitt „Vorschlag H119“ am
// Ende von einstellungen/paket-loeschen-gestaltung.md).
// Spezifikation: test/abnahme-loeschen.md (A138).
//
// Einbau: als `mod mengen_entf { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, cam3d, tool, click,
// key, W, H. Zeilen wählen über die vorhandene ListView (click_row,
// click_group, B7).
//
// Angenommener Name nur im Adapter: `ListView::delete_key` (Entf im
// Mengenfenster, wirkt auf die gemeinsame Auswahl, liefert die Zeilen des
// Hinweises). Ein Rechtsklick-Menü auf eine Zeile („Im Modell zeigen“ oben)
// und die Übergänge prüft Handtest H119.

use crate::picking::Picking;
use crate::schedule_view::ListView;
use sk_model::{Category, ElementId, Model, RunId};

// ===== Adapter =====

/// Entf, während das Mengenfenster vorne ist: löscht nach denselben Regeln
/// wie im Hauptfenster (ein Rückgängig-Schritt) und gibt den Hinweis unter
/// der Zeile Zeile für Zeile zurück (leer: kein Hinweis).
fn mengen_entf(v: &mut ListView, s: &mut Scene, p: &mut Picking) -> Vec<String> {
    v.delete_key(s, p)
}

// ===== Hilfen =====

fn hat(s: &Scene, n: &str) -> bool {
    s.model().elements().iter().any(|(_, e)| e.number == n)
}

fn iw_zug(s: &mut Scene, a: (f64, f64), b: (f64, f64)) -> RunId {
    let c = cam3d();
    let set = s.model().defaults().interior_wall;
    let mut t = tool(s);
    t.set_category(Category::InteriorWall, s.model().wall_layers(set));
    click(&mut t, &c, vec3(a.0, a.1, 0.0));
    click(&mut t, &c, vec3(b.0, b.1, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    s.add_wall_as(&w, Category::InteriorWall).unwrap()
}

/// Prüfhaus mit drei Innenwänden im EG (IW-001 bei x = 5 m, IW-002 bei
/// 2,5 m, IW-003 bei 7,5 m).
fn haus() -> Scene {
    let mut s = Scene::with_model(Model::with_seed(138));
    haus_b11(&mut s);
    iw_zug(&mut s, (2500.0, 0.0), (2500.0, 8000.0));
    iw_zug(&mut s, (7500.0, 0.0), (7500.0, 8000.0));
    assert!(hat(&s, "IW-003"));
    s
}

fn csv(s: &mut Scene) -> String {
    let l = s.schedule().clone();
    String::from_utf8_lossy(&crate::schedule_view::csv(s.model(), &l)).to_string()
}

fn zeilen(v: &[&str]) -> Vec<String> {
    v.iter().map(|z| z.to_string()).collect()
}

fn sauber(s: &Scene, ctx: &str) {
    assert!(
        s.model().check().is_empty(),
        "{ctx}: {:?}",
        s.model().check()
    );
}

// ===== Tests =====

/// A138 (H119 neu): Entf im Mengenfenster wirkt auf die gemeinsame Auswahl
/// mit denselben Regeln und Sätzen wie im Hauptfenster.
/// - Zeile IW-001 gewählt: IW-001 weg, kein Hinweis, Schritt „Bauteil
///   gelöscht“, Zeile fehlt in der Liste, Auswahl bereinigt; Rückgängig
///   bringt sie zurück.
/// - Gruppenzeile „Innenwände“ im EG gewählt: alle drei weg in einem
///   Schritt „3 Bauteile gelöscht“, Hinweis „3 Wände gelöscht.“ (mit
///   „Rückgängig“).
/// - Zeile AW-003 gewählt: nichts weg, Hinweis mit dem Außenwand-Satz, kein
///   Schritt, Revision gleich.
/// - IW-001 und AW-001 mit Strg gewählt: „1 Wand gelöscht.“ / „Die
///   Außenwand bleibt, sie gehört zum Gebäudeumriss.“
/// - Nichts gewählt (Geschoss- oder Summenzeile): nichts weg, Hinweis
///   „Hier ist kein Bauteil gewählt.“, kein Schritt.
#[test]
fn a138_entf_im_mengenfenster() {
    let aw_satz = [
        "Außenwände gehören zum Gebäudeumriss.",
        "Zum Entfernen den Umriss ändern oder das ganze Gebäude löschen.",
    ];

    // Eine Zeile
    let mut s = haus();
    let mut p = Picking::default();
    let mut v = ListView::new(&mut s);
    v.click_row(&mut s, &mut p, "IW-001", false, false);
    assert_eq!(p.selected.len(), 1);
    let id: ElementId = p.selected[0];
    let text = sk_model::szo::write(s.model());
    let hinweis = mengen_entf(&mut v, &mut s, &mut p);
    assert!(hinweis.is_empty(), "{hinweis:?}");
    assert!(!hat(&s, "IW-001"));
    assert!(s.model().element(id).is_none());
    assert_eq!(s.undo_label(), Some("Bauteil gelöscht"));
    assert!(!csv(&mut s).contains("IW-001"), "Zeile weg");
    assert!(p.selected.is_empty(), "Auswahl bereinigt");
    v.follow(&mut s, &p);
    sauber(&s, "eine Zeile");
    assert!(s.undo());
    assert_eq!(sk_model::szo::write(s.model()), text, "Rückgängig");
    assert!(csv(&mut s).contains("IW-001"));

    // Gruppenzeile
    let mut s = haus();
    let mut p = Picking::default();
    let mut v = ListView::new(&mut s);
    v.click_group(&mut s, &mut p, "EG", Category::InteriorWall);
    assert_eq!(p.selected.len(), 3, "Gruppe wählt alle drei");
    let hinweis = mengen_entf(&mut v, &mut s, &mut p);
    assert_eq!(
        hinweis.first().map(String::as_str),
        Some("3 Wände gelöscht.")
    );
    assert_eq!(hinweis.len(), 1, "nichts abgelehnt: {hinweis:?}");
    assert!(!hat(&s, "IW-001") && !hat(&s, "IW-002") && !hat(&s, "IW-003"));
    assert_eq!(s.undo_label(), Some("3 Bauteile gelöscht"), "ein Schritt");
    sauber(&s, "Gruppe");
    assert!(s.undo());
    assert!(hat(&s, "IW-001") && hat(&s, "IW-002") && hat(&s, "IW-003"));

    // Außenwand-Zeile
    let mut s = haus();
    let mut p = Picking::default();
    let mut v = ListView::new(&mut s);
    v.click_row(&mut s, &mut p, "AW-003", false, false);
    let rev = s.model().revision();
    let label = s.undo_label();
    let hinweis = mengen_entf(&mut v, &mut s, &mut p);
    assert_eq!(hinweis, zeilen(&aw_satz));
    assert!(hat(&s, "AW-003"));
    assert_eq!(s.model().revision(), rev, "Datei unverändert");
    assert_eq!(s.undo_label(), label, "kein Schritt");

    // Gemischt mit Strg
    v.click_row(&mut s, &mut p, "IW-001", false, false);
    v.click_row(&mut s, &mut p, "AW-001", true, false);
    assert_eq!(p.selected.len(), 2);
    let hinweis = mengen_entf(&mut v, &mut s, &mut p);
    assert_eq!(
        hinweis,
        zeilen(&[
            "1 Wand gelöscht.",
            "Die Außenwand bleibt, sie gehört zum Gebäudeumriss."
        ])
    );
    assert!(!hat(&s, "IW-001") && hat(&s, "AW-001"));
    assert_eq!(s.undo_label(), Some("Bauteil gelöscht"));

    // Nichts gewählt
    p.clear();
    let rev = s.model().revision();
    let label = s.undo_label();
    let hinweis = mengen_entf(&mut v, &mut s, &mut p);
    assert_eq!(hinweis, zeilen(&["Hier ist kein Bauteil gewählt."]));
    assert_eq!(s.model().revision(), rev);
    assert_eq!(s.undo_label(), label);
}

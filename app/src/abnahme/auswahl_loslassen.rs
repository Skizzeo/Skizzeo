use super::*;

// Abnahmetest A139: Auswahl beim Loslassen, Strg+Klick auf das violette
// Wandband erweitert die Auswahl (Fix aus e06cada, Handtest H121).
// Die Entscheidung steckt in `selection::release_pick` (Bauthread, Name und
// Signatur vom Koordinator 07.10. 03:25); main.rs und das Mengenfenster
// nutzen sie beide.
// Spezifikation: test/abnahme-loeschen.md (A139). Ergänzt H121, ersetzt ihn
// nicht: ob main.rs die Funktion wirklich aufruft, sieht nur der Handtest.
//
// Einbau: als `mod auswahl_loslassen { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11.
// Name nur im Adapter: `selection::release_pick` und `selection::PickChange`
// (Keep, Replace(Option<ElementId>), Add(id), Remove(id); braucht Debug und
// PartialEq).

use crate::selection::PickChange;
use sk_model::{Category, ElementId, Model};

// ===== Adapter =====

/// Maus losgelassen. `band`: Bauteil, dessen violettes Band angeklickt (nicht
/// gezogen) wurde, hat Vorrang. `treffer`: `None` = gezogen, kein Klick;
/// `Some(None)` = Klick ins Leere; `Some(Some(id))` = Bauteil getroffen.
/// `werkzeug`: Wandwerkzeug an, dann wirkt Strg nicht.
fn loslassen(
    band: Option<ElementId>,
    treffer: Option<Option<ElementId>>,
    strg: bool,
    werkzeug: bool,
    gewaehlt: &[ElementId],
) -> PickChange {
    crate::selection::release_pick(band, treffer, strg, werkzeug, gewaehlt)
}

// ===== Hilfen =====

fn wand(s: &Scene, c: Category, n: usize) -> ElementId {
    s.model()
        .elements()
        .iter()
        .filter(|(_, e)| e.category == c)
        .map(|(id, _)| id)
        .nth(n)
        .unwrap()
}

// ===== Tests =====

/// A139: Entscheidung beim Loslassen.
/// - Klick auf ein Bauteil ohne Strg ersetzt die Auswahl.
/// - Strg+Klick nimmt ein nicht gewähltes Bauteil dazu und nimmt ein
///   gewähltes heraus, auf das Bauteil wie auf sein violettes Band (der
///   Fehler vor e06cada: Band + Strg ersetzte die Auswahl).
/// - Das Band hat Vorrang vor dem Treffer darunter.
/// - Gezogen (kein Klick) ändert nichts, auch nicht mit Strg.
/// - Klick ins Leere hebt die Auswahl auf; mit Strg bleibt sie (Koordinator
///   07.10. 03:28: wer mit Strg sammelt und daneben klickt, verliert nichts).
/// - Bei eingeschaltetem Wandwerkzeug wirkt Strg nicht: ersetzen.
#[test]
fn a139_strg_klick_erweitert_die_auswahl() {
    let mut s = Scene::with_model(Model::with_seed(139));
    haus_b11(&mut s);
    let iw = wand(&s, Category::InteriorWall, 0);
    let a1 = wand(&s, Category::ExteriorWall, 0);
    let a2 = wand(&s, Category::ExteriorWall, 1);
    let sel = [iw];

    // Ohne Strg: ersetzen, egal ob Treffer oder Band
    assert_eq!(
        loslassen(None, Some(Some(a1)), false, false, &sel),
        PickChange::Replace(Some(a1))
    );
    assert_eq!(
        loslassen(Some(a1), Some(None), false, false, &sel),
        PickChange::Replace(Some(a1)),
        "Klick aufs Band ohne Strg"
    );

    // Strg: dazu bzw. heraus, Treffer wie Band
    for (band, treffer, wie) in [
        (None, Some(Some(a1)), "Treffer"),
        (Some(a1), Some(None), "Band"),
        (Some(a1), Some(Some(a1)), "Band über dem Bauteil"),
    ] {
        assert_eq!(
            loslassen(band, treffer, true, false, &sel),
            PickChange::Add(a1),
            "Strg+Klick ({wie}) nimmt dazu"
        );
        assert_eq!(
            loslassen(band, treffer, true, false, &[iw, a1]),
            PickChange::Remove(a1),
            "Strg+Klick ({wie}) auf Gewähltes nimmt heraus"
        );
    }

    // Band hat Vorrang vor dem Treffer darunter
    assert_eq!(
        loslassen(Some(a2), Some(Some(a1)), false, false, &sel),
        PickChange::Replace(Some(a2))
    );
    assert_eq!(
        loslassen(Some(a2), Some(Some(a1)), true, false, &sel),
        PickChange::Add(a2)
    );

    // Gezogen: nichts
    for strg in [false, true] {
        assert_eq!(
            loslassen(None, None, strg, false, &sel),
            PickChange::Keep,
            "gezogen, Strg {strg}"
        );
    }

    // Klick ins Leere: ohne Strg weg, mit Strg bleibt sie
    assert_eq!(
        loslassen(None, Some(None), false, false, &sel),
        PickChange::Replace(None)
    );
    assert_eq!(
        loslassen(None, Some(None), true, false, &sel),
        PickChange::Keep,
        "Strg+Klick ins Leere behält die Auswahl"
    );

    // Wandwerkzeug an: Strg wirkt nicht
    assert_eq!(
        loslassen(None, Some(Some(a1)), true, true, &sel),
        PickChange::Replace(Some(a1))
    );
    assert_eq!(
        loslassen(Some(a1), Some(None), true, true, &[iw, a1]),
        PickChange::Replace(Some(a1))
    );
}

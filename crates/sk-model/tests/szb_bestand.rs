//! Der Werksbestand der Bauteil-Werkbank (`sk_szb::bestand`) kennt die
//! Bauteilarten und Präfixe des Lieferumfangs (Gleichstand R8).

use sk_model::element::Category;
use sk_model::kinds::spec;
use sk_szb::bestand::{ARTEN, BELEGT};

#[test]
fn praefixe_des_lieferumfangs_sind_belegt() {
    for c in Category::ALL {
        let p = spec(c).prefix;
        assert!(BELEGT.contains(&p), "{c:?}: Präfix {p} fehlt in BELEGT");
    }
}

/// Wörter für `art=`: die Arten mit Schichten, ohne Öffnungen und Räume
/// (wie `sk_cost::satz::KATEGORIEN`; das Flachdach hat einen Aufbau).
#[test]
fn arten_wie_kinds() {
    let ohne = [
        Category::Window,
        Category::Door,
        Category::Opening,
        Category::Space,
    ];
    let mut soll: Vec<&str> = Category::ALL
        .iter()
        .filter(|c| !ohne.contains(c))
        .map(|c| spec(*c).szo)
        .collect();
    let mut ist = ARTEN.to_vec();
    ist.sort_unstable();
    soll.sort_unstable();
    assert_eq!(ist, soll);
}

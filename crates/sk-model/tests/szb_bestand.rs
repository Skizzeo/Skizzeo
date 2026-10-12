//! Der Werksbestand der Bauteil-Werkbank (`sk_szb::bestand`) kennt die
//! Bauteilarten und Präfixe des Lieferumfangs (Gleichstand R8).

use sk_model::element::Category;
use sk_model::kinds::spec;
use sk_model::library::MatCategory;
use sk_szb::bestand::{ARTEN, BELEGT};
use sk_szb::{pruefen, Bestand, Geschoss};

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

/// `kategorie=` am eigenen Baustoff: jede Kategorie des Modells außer der
/// Luftschicht, die keinen Körper hat (Raumbuch RB1: screed, ceramic,
/// flooring, coating).
#[test]
fn kategorien_wie_modell() {
    let basis = include_str!("../../sk-szb/beispiele/werk.stuetze.szb").trim_end();
    for k in MatCategory::ALL {
        let w = k.word();
        let text = format!("{basis}\n[baustoff] key=probe name=\"Probe\" kategorie={w}");
        let p = pruefen(&text, &Bestand::werk(), &Geschoss::PROBE);
        let abgelehnt = p.befunde.iter().any(|b| b.text.starts_with("kategorie:"));
        assert_eq!(abgelehnt, k == MatCategory::Air, "kategorie={w}");
    }
}

//! Zeilenumbruch im Paneel „Eigenschaften“ (Begrenzung eines Raums).

use super::*;

/// Begrenzung eines Raums mit acht Wänden (Einstellungen zu #112 B3):
/// bei 192 dip Innenbreite auf mehrere Zeilen, jede passt, nichts ist
/// gekürzt, das Paneel wird höher.
#[test]
fn begrenzung_bricht_um() {
    let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
    let mut fonts = Fonts::system();
    if fonts.regular.is_none() {
        fonts.regular = std::fs::read(lib.join("LiberationSans-Regular.ttf"))
            .ok()
            .and_then(sk_paint::font::Font::parse);
    }
    let text = (1..=8)
        .map(|i| format!("AW-{i:03}"))
        .collect::<Vec<_>>()
        .join(", ");
    let p = Props {
        set_label: "Begrenzung",
        layer_set: text.clone(),
        set_umbruch: true,
        ..Default::default()
    };
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.fonts = fonts;
    ui.set_props(Some(p.clone()));
    let Some(f) = ui.fonts.regular.as_ref() else {
        return; // ohne Schrift misst nichts
    };
    let breite = ui.width(Panel::Props) - 2.0 * ui.size.panel_pad;
    assert_eq!(breite, 192.0);
    let zeilen: Vec<String> = ui
        .rows(Panel::Props)
        .into_iter()
        .filter_map(|r| match r {
            Row::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    assert!(zeilen.len() >= 2, "{zeilen:?}");
    for z in &zeilen {
        assert!(f.width(z, ui.size.font_small) <= breite, "{z}");
    }
    assert_eq!(zeilen.join(" "), text, "nichts gekürzt");
    let hoch = ui.props_natural_height();
    ui.set_props(Some(Props {
        set_umbruch: false,
        ..p
    }));
    assert!(ui.props_natural_height() < hoch);
}

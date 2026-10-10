use super::*;

// Abnahmetest A229 (Review 3b, Koordinator 11:30; paket-5 §3a): Der
// Umzug der Auswahlfelder und Vorschauen der Darstellung aus
// prefs_attr.rs nach attr_pick.rs (samt Kachelspeicher M1–M3) ändert das
// Einstellungsfenster um kein Pixel. Spezifikation:
// test/abnahme-materialfenster.md.
//
// Einbau: als `mod bildgleich_attr_pick { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude. Kein Adapter.
//
// Die Sollwerte sind FNV-1a-64 über `Canvas::to_rgba8` des ganzen
// Fensters, gemessen auf main 818a6b0 und 0e8e08d (gleich), ohne Schrift
// (`Fonts` leer), damit das Ergebnis nicht von installierten Schriften
// abhängt. Gültig für einen reinen Umzug: Er sollte als eigener
// Vor-Commit „5-0“ vor dem Materialfenster kommen. Mit Paket 5 selbst
// entfällt der Reiter „Baustoffe“, und die Reiterleiste ändert sich;
// dann wird dieser Test durch die Bildvergleiche soll-p5-* abgelöst und
// gelöscht (Test liefert den Patch dazu).

use crate::prefs::{Prefs, Tab, Win};

fn fnv(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for v in b {
        h ^= *v as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn bild(s: &mut Scene, tab: Tab, scale: f32) -> (usize, usize, u64) {
    let th = Theme::dark();
    let f = sk_ui::widgets::Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let mut p = Prefs::open(s, &th);
    p.tab = tab;
    let w = Win {
        w: (1440.0 * scale) as u32,
        h: (900.0 * scale) as u32,
        top: (32.0 * scale) as u32,
        scale,
    };
    let c = p.paint(&th, &f, &w, s).0;
    let h = fnv(&c.to_rgba8());
    let mut th2 = th.clone();
    p.cancel(s, &mut th2);
    (c.width, c.height, h)
}

/// A229 (Review 3b, ab Paket 5): Reiter „Stifte“, „Linientypen“,
/// „Schraffuren“ und „Oberflächen“ am Standardhaus bei 100 % und 150 %
/// in der Fenstergröße wie vor Paket 5, zweimal gezeichnet bitgleich
/// (Kachelspeicher darf nichts verändern). Mit dem Materialfenster
/// entfällt der Reiter „Baustoffe“ und die Reiterleiste ändert sich;
/// die festen Bildwerte aus 5-0 entfallen darum, das Aussehen prüfen
/// die Bildvergleiche soll-p5-*. Test trägt neue feste Werte nach, wenn
/// Paket 5 auf main ist.
#[test]
fn a229_einstellungen_bitgleich() {
    let mut s = Scene::with_model(Model::with_seed(5));
    gebaeude(&mut s);
    for scale in [1.0, 1.5] {
        for tab in [Tab::Pens, Tab::LineTypes, Tab::Fills, Tab::Surfaces] {
            let ist = bild(&mut s, tab, scale);
            let groesse = if scale == 1.0 {
                (880, 640)
            } else {
                (1320, 960)
            };
            assert_eq!((ist.0, ist.1), groesse, "{tab:?} bei {scale}");
            assert_eq!(bild(&mut s, tab, scale), ist, "{tab:?} zweimal gleich");
        }
    }
}

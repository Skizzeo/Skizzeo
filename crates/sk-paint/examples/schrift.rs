//! Probedruck einer Schrift: `cargo run -p sk-paint --example schrift -- datei.ttf ausgabe.png`

use sk_paint::{font::Font, Canvas, Rgba};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let font = Font::parse(std::fs::read(&args[1]).expect("Datei")).expect("TrueType");
    let mut c = Canvas::new(420, 140);
    c.clear(Rgba::rgb(31, 37, 45));
    for (i, px) in [13.0f32, 16.0, 24.0, 40.0].into_iter().enumerate() {
        let y = 20.0 + [0.0, 20.0, 46.0, 90.0][i];
        font.draw(
            &mut c,
            "Gebäude Grundriss Schnitt ÄÖÜß 90°",
            px,
            10.0,
            y,
            Rgba::rgb(231, 229, 222),
        );
    }
    std::fs::write(&args[2], c.to_png()).unwrap();
}

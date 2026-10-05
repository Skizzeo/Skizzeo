//! Das SK-Logo als Vektorpfad, nachgezeichnet aus Jörns Vorlage (2048 px).
//! Koordinaten in Vorlagenpixeln relativ zur linken oberen Ecke des Logos.

use sk_paint::{Canvas, Path, Rgba};

pub const WIDTH: f32 = 1219.0;
pub const HEIGHT: f32 = 909.0;

pub fn path() -> Path {
    let mut p = Path::new();
    // Oberer S-Bogen mit dem K zu einer Fläche verbunden
    p.move_to(0.0, 237.5)
        .cubic_to((0.0, 96.5), (103.9, 0.0), (259.6, 0.0))
        .line_to(586.0, 0.0)
        .line_to(586.0, 434.7)
        .line_to(935.0, 0.0)
        .line_to(1190.0, 0.0)
        .line_to(840.0, 427.5)
        .line_to(1219.0, 909.0)
        .line_to(956.5, 909.0)
        .line_to(587.3, 452.0)
        .cubic_to((518.5, 366.9), (419.0, 287.0), (339.0, 287.0))
        .line_to(0.0, 287.0)
        .close();
    // Unterer S-Bogen
    p.move_to(0.0, 390.6)
        .line_to(0.0, 909.0)
        .line_to(445.5, 909.0)
        .cubic_to((532.1, 909.0), (602.2, 836.9), (602.2, 748.0))
        .cubic_to((602.2, 659.1), (532.1, 587.0), (445.5, 587.0))
        .line_to(281.5, 587.0)
        .cubic_to((157.1, 587.0), (38.5, 480.6), (0.0, 390.6))
        .close();
    p
}

/// Logo in ein Rechteck einpassen (Höhe `h`, linke obere Ecke `x`,`y`).
pub fn path_at(x: f32, y: f32, h: f32) -> Path {
    path().transformed(h / HEIGHT, x, y)
}

/// Programm-Logo: weißes SK auf schwarzem, leicht gerundetem Quadrat.
pub fn app_icon(size: usize) -> Canvas {
    let s = size as f32;
    let mut c = Canvas::new(size, size);
    let mut bg = Path::new();
    bg.rounded_rect(0.0, 0.0, s, s, s * 0.16);
    c.fill(&bg, Rgba::rgb(0, 0, 0));
    let w = s * 0.68;
    let h = w * HEIGHT / WIDTH;
    c.fill(&path_at((s - w) * 0.5, (s - h) * 0.5, h), Rgba::rgb(255, 255, 255));
    c
}

/// Titelleisten-Logo: schwarzes SK auf transparentem Grund.
pub fn titlebar_logo(height: usize) -> Canvas {
    let h = height as f32;
    let w = (h * WIDTH / HEIGHT).ceil();
    let mut c = Canvas::new(w as usize, height);
    c.fill(&path_at(0.0, 0.0, h), Rgba::rgb(0, 0, 0));
    c
}

/// SVG-Dateien der beiden Logos (Vektor, beliebig skalierbar).
pub fn svg_titlebar() -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {WIDTH} {HEIGHT}\">\
         <path fill=\"#000\" d=\"{}\"/></svg>\n",
        path().to_svg_d()
    )
}

pub fn svg_app() -> String {
    let s = 1024.0f32;
    let w = s * 0.68;
    let h = w * HEIGHT / WIDTH;
    let mut bg = Path::new();
    bg.rounded_rect(0.0, 0.0, s, s, s * 0.16);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {s} {s}\">\
         <path fill=\"#000\" d=\"{}\"/><path fill=\"#fff\" d=\"{}\"/></svg>\n",
        bg.to_svg_d(),
        path_at((s - w) * 0.5, (s - h) * 0.5, h).to_svg_d()
    )
}

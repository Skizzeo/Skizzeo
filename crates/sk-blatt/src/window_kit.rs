//! Gemeinsames der Fenster „Bauteilkatalog“ und „Baustoffe …“ (Review 3n,
//! Hygiene): kleine Malhilfen, Zahlen und Kennwerte als Text, Teilbilder.

use sk_model::PropValue;
use sk_paint::{Canvas, Path, Rgba};
use sk_ui::widgets::Rect;

pub use sk_ui::widgets::text as label;

/// Gefülltes Rechteck mit runden Ecken.
pub fn rounded(c: &mut Canvas, r: Rect, rad: f32, col: Rgba) {
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    c.fill(&p, col);
}

/// Rand der Breite `b` innen an einem Rechteck mit runden Ecken.
pub fn outline(c: &mut Canvas, r: Rect, rad: f32, b: f32, col: Rgba) {
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    p.rounded_rect_hole(
        r.x + b,
        r.y + b,
        r.w - 2.0 * b,
        r.h - 2.0 * b,
        (rad - b).max(0.0),
    );
    c.fill(&p, col);
}

/// Zahl in deutscher oder englischer Schreibweise („0,09“, „0.09“).
pub fn parse_num(t: &str) -> Option<f64> {
    t.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

/// Zahl, wie die Fenster sie zeigen: kürzeste exakte Form mit Komma.
pub fn num_text(v: f64) -> String {
    format!("{v}").replace('.', ",")
}

/// Kennwert als Text: Zahl mit Komma, Text, „ja“ bzw. „nein“.
pub fn prop_text(v: &PropValue) -> String {
    match v {
        PropValue::Text(t) => t.clone(),
        PropValue::Number(n) => num_text(*n),
        PropValue::Bool(b) => if *b { "ja" } else { "nein" }.into(),
    }
}

/// Schnitt zweier Rechtecke auf ganze Bildpunkte; `None`, wenn leer.
pub fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x0 = a.x.max(b.x).round();
    let y0 = a.y.max(b.y).round();
    let x1 = (a.x + a.w).min(b.x + b.w).round();
    let y1 = (a.y + a.h).min(b.y + b.h).round();
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

/// Fügt ein Teilbild (x0, y0, x1, y1) hinzu; überlappende werden zum
/// umschließenden Rechteck vereinigt.
pub fn merge_rect(
    parts: &mut Vec<(usize, usize, usize, usize)>,
    mut r: (usize, usize, usize, usize),
) {
    while let Some(i) = parts
        .iter()
        .position(|p| p.0 < r.2 && r.0 < p.2 && p.1 < r.3 && r.1 < p.3)
    {
        let p = parts.swap_remove(i);
        r = (r.0.min(p.0), r.1.min(p.1), r.2.max(p.2), r.3.max(p.3));
    }
    parts.push(r);
}

/// Fensterbereiche (Fensterkoordinaten) als Teilbilder in Bildpunkten eines
/// Bildes mit der linken oberen Ecke bei `(ox, oy)` und der Größe
/// `cw` × `ch`: um `pad` erweitert (Rand, Fokusring, Glättung), nach außen
/// auf ganze Bildpunkte gerundet, überlappende vereinigt (Darstellung n9
/// §2.2).
pub fn pixel_parts(
    rects: &[Rect],
    pad: f32,
    (ox, oy): (f32, f32),
    (cw, ch): (usize, usize),
) -> Vec<(usize, usize, usize, usize)> {
    let mut parts = Vec::new();
    for r in rects {
        let x0 = ((r.x - pad - ox).floor().max(0.0) as usize).min(cw);
        let y0 = ((r.y - pad - oy).floor().max(0.0) as usize).min(ch);
        let x1 = ((r.x + r.w + pad - ox).ceil().max(0.0) as usize).min(cw);
        let y1 = ((r.y + r.h + pad - oy).ceil().max(0.0) as usize).min(ch);
        if x1 > x0 && y1 > y0 {
            merge_rect(&mut parts, (x0, y0, x1, y1));
        }
    }
    parts
}

/// Wie [`pixel_parts`], aber als Streifen über die ganze Bildbreite: dann
/// beginnt jede Zeile wie im ganzen Bild am linken Rand, und das Teilbild
/// gleicht ihm bitgenau (Darstellung n9 §2.1, ohne Toleranz).
pub fn pixel_strips(
    rects: &[Rect],
    pad: f32,
    origin: (f32, f32),
    (cw, ch): (usize, usize),
) -> Vec<(usize, usize, usize, usize)> {
    let mut strips = Vec::new();
    for (_, y0, _, y1) in pixel_parts(rects, pad, origin, (cw, ch)) {
        merge_rect(&mut strips, (0, y0, cw, y1));
    }
    strips
}

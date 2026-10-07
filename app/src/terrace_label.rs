//! Platzregel der Terrassenangabe im Grundriss (Review 3f K1, Ist/Soll p′;
//! einstellungen/vorschlag-dachterrasse.md §15): „Dachterrasse 13,22 m²“
//! überdeckt nie Wände, Attika oder Blechkante und hält 6 dip Abstand zum
//! Kettensymbol. Der Textkasten trägt 4 dip Rand. Stufen, ohne Übergang:
//!
//! 1. voll, mittig in der Terrasse;
//! 2. voll, auf der Längsachse zur Seite mit mehr Platz verschoben;
//! 3. Kurzform „13,22 m²“ in der Terrasse, mittig oder verschoben;
//! 4. voll, außen 6 dip vor der Blechkante, mittig auf ihrer Länge, auf
//!    freiem Papier;
//! 5. sonst entfällt die Angabe.
//!
//! [`place`] rechnet rein in Bildschirmpixeln (y nach unten); [`for_plan`]
//! füttert sie aus Szene und Kamera, wie der Grundriss sie zeigt.

use crate::camera::Camera;
use crate::link_view;
use crate::scene::Scene;
use sk_math::Vec3;
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;

/// Rand um den Text (dip).
pub const PAD: f64 = 4.0;
/// Abstand zu Kettensymbol und Blechkante (dip).
pub const GAP: f64 = 6.0;

/// Rechteck in Pixeln: links, oben, Breite, Höhe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Box {
    fn around(c: [f64; 2], (w, h): (f64, f64)) -> Box {
        Box {
            x: c[0] - w * 0.5,
            y: c[1] - h * 0.5,
            w,
            h,
        }
    }

    /// Überdecken sich die Kästen (Berühren zählt nicht)?
    fn overlaps(&self, o: &Box) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }

    fn grown(&self, d: f64) -> Box {
        Box {
            x: self.x - d,
            y: self.y - d,
            w: self.w + 2.0 * d,
            h: self.h + 2.0 * d,
        }
    }

    fn corners(&self) -> [[f64; 2]; 4] {
        let (x1, y1) = (self.x + self.w, self.y + self.h);
        [[self.x, self.y], [x1, self.y], [x1, y1], [self.x, y1]]
    }
}

/// Eine Lage im Bildschirm.
pub struct Input<'a> {
    /// Umriss der Terrasse.
    pub outline: &'a [[f64; 2]],
    /// Wände, Attika und Blechkante (und was sonst nicht überdeckt wird).
    pub blocked: &'a [Box],
    /// Kettensymbole.
    pub chips: &'a [Box],
    /// Blechkante außen: Anfang, Ende, Richtung nach außen (Einheitsvektor).
    pub edge: Option<([f64; 2], [f64; 2], [f64; 2])>,
    /// Textmaße ohne Rand: voll und kurz.
    pub full: (f64, f64),
    pub short: (f64, f64),
    pub scale: f64,
}

/// Ergebnis: der Textkasten samt Rand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placement {
    Full(Box),
    Shifted(Box),
    Short(Box),
    Outside(Box),
    None,
}

impl Placement {
    /// Kasten und ob die Kurzform gilt.
    pub fn shown(&self) -> Option<(Box, bool)> {
        match *self {
            Placement::Full(b) | Placement::Shifted(b) | Placement::Outside(b) => Some((b, false)),
            Placement::Short(b) => Some((b, true)),
            Placement::None => None,
        }
    }
}

/// Die Platzregel (Stufen 1–5, siehe Modulkopf).
pub fn place(i: &Input) -> Placement {
    let n = i.outline.len();
    if n < 3 {
        return Placement::None;
    }
    let pad = PAD * i.scale;
    let gap = GAP * i.scale;
    let padded = |(w, h): (f64, f64)| (w + 2.0 * pad, h + 2.0 * pad);
    let free = |b: &Box| {
        !i.blocked.iter().any(|o| b.overlaps(o))
            && !i.chips.iter().any(|c| b.overlaps(&c.grown(gap)))
    };
    let center = centroid(i.outline);
    let (lo, hi) = bounds(i.outline);
    // Längsachse des Umrisses im Bild
    let axis = if hi[0] - lo[0] >= hi[1] - lo[1] { 0 } else { 1 };
    let reach = (hi[axis] - lo[axis]).ceil() as i64;
    // Mittig, sonst die kleinste Verschiebung (gleich weit: nach rechts
    // bzw. unten)
    let along = |size: (f64, f64)| -> Option<(Box, bool)> {
        for k in 0..=reach {
            for d in if k == 0 { vec![0] } else { vec![k, -k] } {
                let mut c = center;
                c[axis] += d as f64;
                let b = Box::around(c, size);
                if inside(i.outline, &b) && free(&b) {
                    return Some((b, d != 0));
                }
            }
        }
        None
    };
    match along(padded(i.full)) {
        Some((b, false)) => return Placement::Full(b),
        Some((b, true)) => return Placement::Shifted(b),
        None => {}
    }
    if let Some((b, _)) = along(padded(i.short)) {
        return Placement::Short(b);
    }
    if let Some((a, e, nv)) = i.edge {
        let (w, h) = padded(i.full);
        let depth = (nv[0].abs() * w + nv[1].abs() * h) * 0.5 + gap;
        let c = [
            (a[0] + e[0]) * 0.5 + nv[0] * depth,
            (a[1] + e[1]) * 0.5 + nv[1] * depth,
        ];
        let b = Box::around(c, (w, h));
        if free(&b) && !touches(i.outline, &b) {
            return Placement::Outside(b);
        }
    }
    Placement::None
}

/// Flächenschwerpunkt (bei entartetem Umriss das Mittel der Punkte).
fn centroid(p: &[[f64; 2]]) -> [f64; 2] {
    let n = p.len();
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for k in 0..n {
        let (q, r) = (p[k], p[(k + 1) % n]);
        let c = q[0] * r[1] - r[0] * q[1];
        a += c;
        cx += (q[0] + r[0]) * c;
        cy += (q[1] + r[1]) * c;
    }
    if a.abs() < 1e-9 {
        let s = p.iter().fold([0.0, 0.0], |s, q| [s[0] + q[0], s[1] + q[1]]);
        return [s[0] / n as f64, s[1] / n as f64];
    }
    [cx / (3.0 * a), cy / (3.0 * a)]
}

fn bounds(p: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    p.iter().fold(
        ([f64::MAX, f64::MAX], [f64::MIN, f64::MIN]),
        |(lo, hi), q| {
            (
                [lo[0].min(q[0]), lo[1].min(q[1])],
                [hi[0].max(q[0]), hi[1].max(q[1])],
            )
        },
    )
}

fn contains(p: &[[f64; 2]], q: [f64; 2]) -> bool {
    let n = p.len();
    let mut inside = false;
    for k in 0..n {
        let (a, b) = (p[k], p[(k + 1) % n]);
        if (a[1] > q[1]) != (b[1] > q[1]) {
            let x = a[0] + (q[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if q[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Schneidet die Strecke `a`–`b` das Innere von `r`?
fn crosses(a: [f64; 2], b: [f64; 2], r: &Box) -> bool {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-dx, a[0] - r.x),
        (dx, r.x + r.w - a[0]),
        (-dy, a[1] - r.y),
        (dy, r.y + r.h - a[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

/// Liegt der Kasten ganz im Umriss? (Randberührung erlaubt)
fn inside(p: &[[f64; 2]], b: &Box) -> bool {
    let core = b.grown(-0.01);
    let n = p.len();
    core.corners().iter().all(|c| contains(p, *c))
        && (0..n).all(|k| !crosses(p[k], p[(k + 1) % n], &core))
}

/// Überdeckt der Kasten den Umriss? (Randberührung zählt nicht)
fn touches(p: &[[f64; 2]], b: &Box) -> bool {
    let core = b.grown(-0.01);
    let n = p.len();
    core.corners().iter().any(|c| contains(p, *c))
        || (0..n).any(|k| crosses(p[k], p[(k + 1) % n], &core))
}

/// Text der Angabe: voll und kurz.
pub fn texts(area_mm2: f64) -> (String, String) {
    let a = format!("{} m²", crate::selection::de(area_mm2 / 1e6, 2));
    (format!("Dachterrasse {a}"), a)
}

/// Maße des gezeichneten Texts (Pixel), wie [`paint`] ihn zeichnet.
pub fn measure(fonts: &Fonts, text: &str, s: f64, t: &Theme) -> (f64, f64) {
    let px = t.size.font_small * s as f32;
    let f = fonts.regular.as_ref();
    let tw = f.map_or(text.len() as f32 * px * 0.5, |f| f.width(text, px));
    let cap = f.map_or(px * 0.7, |f| f.cap_height(px));
    ((tw.ceil() + 2.0) as f64, (cap * 2.0).ceil() as f64)
}

/// Die Angabe als Bild: Schrift in gedimmter Tinte auf durchsichtigem Grund,
/// so groß wie [`measure`] sagt.
pub fn paint(fonts: &Fonts, text: &str, s: f64, t: &Theme) -> sk_paint::Canvas {
    let px = t.size.font_small * s as f32;
    let f = fonts.regular.as_ref();
    let cap = f.map_or(px * 0.7, |f| f.cap_height(px));
    let (w, h) = measure(fonts, text, s, t);
    let mut c = sk_paint::Canvas::new(w as usize, h as usize);
    sk_ui::widgets::text(
        &mut c,
        f,
        text,
        px,
        1.0,
        ((h as f32 + cap) * 0.5).round(),
        t.ui.sheet_text_dim,
    );
    c
}

/// Angaben im Grundriss des aktiven Geschosses, je Terrasse eine, mit den
/// Kettensymbolen, die ohne Maus stehen (gelöste Wände).
#[cfg(test)]
pub fn for_plan(
    s: &Scene,
    c: &Camera,
    w: f64,
    h: f64,
    scale: f64,
    fonts: &Fonts,
    t: &Theme,
) -> Vec<Placement> {
    let m = s.model();
    let plan_z = m.storey(s.active_storey()).map(|st| st.elevation);
    let chips: Vec<Box> = link_view::chips(&link_view::Want {
        scene: s,
        cam: c,
        w,
        h,
        top: 0.0,
        scale,
        plan_z,
        band: None,
        hover: None,
        keep_linked: None,
    })
    .iter()
    .map(|k| chip_box(k, scale, 0.0))
    .collect();
    labels(s, c, w, h, scale, fonts, t, &chips, &[])
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

/// Kasten eines Kettensymbols in der Ansicht (`top`: Titelleiste).
pub fn chip_box(k: &link_view::Chip, scale: f64, top: f64) -> Box {
    let e = k.size(scale);
    Box::around([k.at.0, k.at.1 - top], (e, e))
}

/// Wie [`for_plan`] mit gegebenen Kettenkästen und weiteren Sperrflächen
/// (Paneele über dem Plan); dazu der zu zeichnende Text.
#[allow(clippy::too_many_arguments)]
pub fn labels(
    s: &Scene,
    c: &Camera,
    w: f64,
    h: f64,
    scale: f64,
    fonts: &Fonts,
    t: &Theme,
    chips: &[Box],
    covered: &[Box],
) -> Vec<(Placement, String)> {
    let (marks, walls) = s.terrace_marks();
    if marks.is_empty() {
        return Vec::new();
    }
    let pt = |p: Vec3| c.project(p, w, h).map(|(x, y)| [x, y]);
    // Wände, Attika und Blech; dazu alles außerhalb der Ansicht
    let far = 1e6;
    let mut blocked: Vec<Box> = vec![
        Box {
            x: -far,
            y: -far,
            w: far,
            h: 3.0 * far,
        },
        Box {
            x: w,
            y: -far,
            w: far,
            h: 3.0 * far,
        },
        Box {
            x: -far,
            y: -far,
            w: 3.0 * far,
            h: far,
        },
        Box {
            x: -far,
            y: h,
            w: 3.0 * far,
            h: far,
        },
    ];
    blocked.extend_from_slice(covered);
    for q in &walls {
        let p: Vec<[f64; 2]> = q.iter().filter_map(|p| pt(*p)).collect();
        if p.len() == 4 {
            let (lo, hi) = bounds(&p);
            blocked.push(Box {
                x: lo[0],
                y: lo[1],
                w: hi[0] - lo[0],
                h: hi[1] - lo[1],
            });
        }
    }
    marks
        .iter()
        .map(|mk| {
            let (full, short) = texts(mk.area);
            let outline: Vec<[f64; 2]> = mk.outline.iter().filter_map(|p| pt(*p)).collect();
            if outline.len() != mk.outline.len() {
                return (Placement::None, full);
            }
            let edge = mk.edge.and_then(|(a, b, n)| {
                let (pa, pb, pn) = (pt(a)?, pt(b)?, pt(a + n * 1000.0)?);
                let (dx, dy) = (pn[0] - pa[0], pn[1] - pa[1]);
                let l = (dx * dx + dy * dy).sqrt();
                (l > 1e-9).then(|| (pa, pb, [dx / l, dy / l]))
            });
            let p = place(&Input {
                outline: &outline,
                blocked: &blocked,
                chips,
                edge,
                full: measure(fonts, &full, scale, t),
                short: measure(fonts, &short, scale, t),
                scale,
            });
            let text = match p.shown() {
                Some((_, true)) => short,
                _ => full,
            };
            (p, text)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    #[test]
    fn senkrechte_terrasse_verschiebt_laengs() {
        // Hochkant: Längsachse senkrecht, Kette in der Mitte
        let t = rect(100.0, 100.0, 260.0, 700.0);
        let chip = Box {
            x: 170.0,
            y: 390.0,
            w: 20.0,
            h: 20.0,
        };
        let p = place(&Input {
            outline: &t,
            blocked: &[],
            chips: &[chip],
            edge: None,
            full: (124.0, 16.0),
            short: (44.0, 16.0),
            scale: 1.0,
        });
        let Placement::Shifted(b) = p else {
            panic!("{p:?}")
        };
        assert!((b.x + b.w * 0.5 - 180.0).abs() < 0.5, "{b:?}");
        assert!(b.y >= 416.0, "unterhalb mit 6 dip: {b:?}");
    }

    #[test]
    fn l_foermige_terrasse_bleibt_im_umriss() {
        // L: unten breit, links hoch; Schwerpunkt im Knick
        let t = vec![
            [0.0, 0.0],
            [60.0, 0.0],
            [60.0, 200.0],
            [400.0, 200.0],
            [400.0, 260.0],
            [0.0, 260.0],
        ];
        let p = place(&Input {
            outline: &t,
            blocked: &[],
            chips: &[],
            edge: None,
            full: (124.0, 16.0),
            short: (44.0, 16.0),
            scale: 1.0,
        });
        let (b, _) = p.shown().expect("Platz");
        assert!(inside(&t, &b), "{p:?}");
    }
}

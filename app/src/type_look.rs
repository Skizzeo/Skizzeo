//! Aussehen eines Bauteiltyps (K3): kleine Kachel für Listen und Chips und
//! das Schnittbild im Bauteilkatalog nach Jörns Skizze (außen links, Decke
//! rechts eingebunden, Maßkette mit hochgestellter 5, Gesamtmaß).
//!
//! Die Schraffuren kommen aus den Darstellungsverweisen der Baustoffe (E6)
//! und werden mit derselben Formel wie im Plan gezeichnet
//! ([`sk_render::fill_color`]).

use crate::draw_table::{fallback_look, look_rows, mat_look, MatLook};
use sk_model::{edge_kind, LayerFunction, LayerSet, MatCategory, Model, TypeCategory};
use sk_paint::{font::Font, Canvas, Path, Rgba};
use sk_ui::theme::Theme;
use sk_ui::widgets::Rect;

/// Eine Schicht: Dicke (mm), Schraffur, Luftschicht (leer), Kern.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerLook {
    pub t: f64,
    pub look: MatLook,
    pub air: bool,
    pub core: bool,
}

/// Was Kachel und Schnittbild von einem Typ brauchen; ändert sich nur mit
/// Typ, Baustoffen oder Farbschema.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeLook {
    /// Von außen nach innen.
    pub layers: Vec<LayerLook>,
    pub exterior: bool,
    /// Stift „Schnitt“ (Umrisse).
    pub ink: Rgba,
    pub paper: Rgba,
    /// Decke im Schnittbild: erster Baustoff der Kategorie Beton.
    pub slab: MatLook,
}

impl TypeLook {
    #[cfg(test)]
    pub fn thicknesses(&self) -> Vec<f64> {
        self.layers.iter().map(|l| l.t).collect()
    }
}

pub fn type_look(m: &Model, theme: &Theme, set: &LayerSet) -> TypeLook {
    let mut notes = Vec::new();
    let a = m.attr();
    let cut = a.pen(a.display().drawing[edge_kind::CUT as usize].pen);
    let ink = cut.map_or(theme.env.edge, |p| Rgba::from_rgb8(p.color));
    let fallback = fallback_look(theme);
    let layers = set
        .layers
        .iter()
        .map(|l| LayerLook {
            t: l.thickness,
            look: m.material(l.material).map_or(fallback, |mat| {
                mat_look(m, theme, &mat.display(), &mut notes)
            }),
            air: l.function == LayerFunction::AirGap,
            core: l.core,
        })
        .collect();
    let slab = m
        .materials()
        .iter()
        .find(|(_, mat)| mat.category == MatCategory::Concrete)
        .map_or(fallback, |(_, mat)| {
            mat_look(m, theme, &mat.display(), &mut notes)
        });
    TypeLook {
        layers,
        exterior: set.category == TypeCategory::ExteriorWall,
        ink,
        paper: theme.ui.sheet_bg,
        slab,
    }
}

/// Füllt ein senkrechtes Band `x0..x1` × `y0..y1` mit der Schraffur;
/// Luftschichten bleiben Papier.
fn shade(
    c: &mut Canvas,
    (x0, y0, x1, y1): (f32, f32, f32, f32),
    look: &MatLook,
    air: bool,
    paper: Rgba,
    s: f32,
) {
    if air {
        c.fill_rect(x0, y0, x1 - x0, y1 - y0, paper);
        return;
    }
    let rows = look_rows(look, s);
    let w = (x1 - x0).max(1.0);
    c.shade_rect(x0, y0, x1, y1, |x, y| {
        // Längs = senkrecht (Zickzack), quer 0..1 über die Schicht
        let k = sk_render::fill_color(&rows, x, -y, [-y / w, (x - x0) / w], [1.0 / w, 1.0 / w]);
        Rgba::from_f32([k[0], k[1], k[2], 1.0])
    });
}

/// Waagrechtes Band (Decke): längs = waagrecht.
fn shade_slab(c: &mut Canvas, x0: f32, y0: f32, x1: f32, y1: f32, look: &MatLook, s: f32) {
    let rows = look_rows(look, s);
    let h = (y1 - y0).max(1.0);
    c.shade_rect(x0, y0, x1, y1, |x, y| {
        let k = sk_render::fill_color(&rows, x, -y, [x / h, (y1 - y) / h], [1.0 / h, 1.0 / h]);
        Rgba::from_f32([k[0], k[1], k[2], 1.0])
    });
}

fn line(c: &mut Canvas, a: (f32, f32), b: (f32, f32), w: f32, col: Rgba) {
    let mut p = Path::new();
    p.segment(a, b, w);
    c.fill(&p, col);
}

/// Gestrichelte senkrechte Linie.
fn dashed_v(c: &mut Canvas, x: f32, y0: f32, y1: f32, w: f32, dash: f32, col: Rgba) {
    let mut y = y0;
    while y < y1 {
        let e = (y + dash).min(y1);
        c.fill_rect(x - w * 0.5, y, w, e - y, col);
        y += 2.0 * dash;
    }
}

/// Kachel: Schichten nebeneinander im Verhältnis ihrer Dicke, außen links,
/// mit feinem Rand.
pub fn paint_thumb(c: &mut Canvas, r: Rect, look: &TypeLook, s: f32) {
    let b = s.round().max(1.0);
    let (x, y, w, h) = (r.x.round(), r.y.round(), r.w.round(), r.h.round());
    c.fill_rect(x, y, w, h, look.ink);
    let (ix, iy, iw, ih) = (x + b, y + b, w - 2.0 * b, h - 2.0 * b);
    c.fill_rect(ix, iy, iw, ih, look.paper);
    let total: f64 = look.layers.iter().map(|l| l.t).sum();
    if total <= 0.0 || iw <= 0.0 {
        return;
    }
    let mut acc = 0.0;
    let n = look.layers.len();
    for (i, l) in look.layers.iter().enumerate() {
        let x0 = (ix + iw * (acc / total) as f32).round();
        acc += l.t;
        let x1 = if i + 1 == n {
            ix + iw
        } else {
            (ix + iw * (acc / total) as f32).round()
        };
        shade(c, (x0, iy, x1, iy + ih), &l.look, l.air, look.paper, s);
        if i > 0 {
            c.fill_rect(x0, iy, b, ih, look.ink);
        }
    }
}

/// Lage der Teile des Schnittbilds in Bildpunkten.
struct Geo {
    /// Grenzen der Schichten von außen nach innen (n + 1 Werte).
    xs: Vec<f32>,
    top: f32,
    bottom: f32,
    /// Decke: x0, y0, x1, y1; links bzw. rechts ein Bruchsymbol.
    slab: (f32, f32, f32, f32),
    break_left: bool,
}

/// Sehr dünne Typen bleiben sichtbar.
const MIN_TOTAL: f64 = 50.0;

fn geo(r: Rect, exterior: bool, t: &[f64], core_from: Option<usize>) -> Geo {
    let total = t.iter().sum::<f64>().max(MIN_TOTAL) as f32;
    let k = (r.h / 880.0).min(0.42 * r.w / total);
    let wall_w = t.iter().sum::<f64>() as f32 * k;
    let x_out = if exterior {
        r.x + 0.195 * r.w
    } else {
        r.x + 0.5 * (r.w - wall_w)
    };
    let mut xs = vec![x_out];
    let mut acc = 0.0;
    for v in t {
        acc += v;
        xs.push(x_out + acc as f32 * k);
    }
    let slab_h = 220.0 * k;
    if exterior {
        // Decke von der Außenseite des Kerns nach innen
        let x0 = core_from.map_or(*xs.last().unwrap_or(&x_out), |i| xs[i]);
        let y0 = r.y + 0.29 * r.h;
        Geo {
            xs,
            top: r.y + 0.115 * r.h,
            bottom: r.y + 0.75 * r.h,
            slab: (x0, y0, r.x + 0.805 * r.w, y0 + slab_h),
            break_left: false,
        }
    } else {
        let y0 = r.y + 0.14 * r.h;
        Geo {
            xs,
            top: y0 + slab_h,
            bottom: r.y + 0.75 * r.h,
            slab: (r.x + 0.12 * r.w, y0, r.x + 0.88 * r.w, y0 + slab_h),
            break_left: true,
        }
    }
}

/// Erste Kernschicht (sonst erste tragende), an ihr beginnt die Decke.
fn core_from(look: &TypeLook) -> Option<usize> {
    look.layers.iter().position(|l| l.core)
}

/// Schicht unter dem Punkt im Schnittbild (Hover-Kopplung mit den Zeilen).
pub fn section_layer_at(r: Rect, look: &TypeLook, t: &[f64], x: f32, y: f32) -> Option<usize> {
    let g = geo(r, look.exterior, t, core_from(look));
    if y < g.top || y > g.bottom {
        return None;
    }
    (0..t.len()).find(|&i| x >= g.xs[i] && x < g.xs[i + 1])
}

/// Was das Schnittbild zusätzlich zeigt.
#[derive(Clone, Debug, Default)]
pub struct SectionMarks {
    /// Hervorgehobene Schicht (Zeile überfahren).
    pub hover: Option<usize>,
    /// Alte Schichtgrenze gestrichelt: Abstand von außen (mm) und
    /// Beschriftung („+4“).
    pub ghost: Option<(f64, String)>,
}

/// Maß in cm, halbe Zentimeter als hochgestellte 5 („36⁵“).
fn dim_text(mm: f64) -> (String, bool) {
    let cm = (mm / 5.0).round() * 0.5;
    if cm.fract() != 0.0 {
        (format!("{}", cm.floor() as i64), true)
    } else {
        (format!("{}", cm as i64), false)
    }
}

fn dim_label(c: &mut Canvas, f: &Font, mm: f64, cx: f32, base: f32, px: f32, col: Rgba) {
    let (main, half) = dim_text(mm);
    let sup = px * 0.68;
    let w = f.width(&main, px) + if half { f.width("5", sup) } else { 0.0 };
    let x = (cx - w * 0.5).round();
    f.draw(c, &main, px, x, base.round(), col);
    if half {
        let xs = x + f.width(&main, px);
        f.draw(c, "5", sup, xs.round(), (base - px * 0.38).round(), col);
    }
}

/// Maßlinie mit Schrägstrichen an den Grenzen.
fn dim_chain(c: &mut Canvas, xs: &[f32], y: f32, ink: Rgba, s: f32) {
    let (Some(&a), Some(&b)) = (xs.first(), xs.last()) else {
        return;
    };
    let w = (0.8 * s).max(1.0);
    line(c, (a - 9.0 * s, y), (b + 9.0 * s, y), w, ink);
    let d = 3.5 * s;
    for &x in xs {
        line(c, (x - d, y + d), (x + d, y - d), 1.4 * w, ink);
        line(c, (x, y - 5.0 * s), (x, y + 5.0 * s), w, ink);
    }
}

/// Bruchsymbol (senkrechte Zickzacklinie) am Ende der Decke.
fn break_mark(c: &mut Canvas, x: f32, y0: f32, y1: f32, ink: Rgba, s: f32) {
    let w = (0.8 * s).max(1.0);
    let ym = 0.5 * (y0 + y1);
    let a = 4.0 * s;
    line(c, (x, y0 - a), (x, ym - a), w, ink);
    line(c, (x, ym - a), (x + a, ym - 0.5 * a), w, ink);
    line(c, (x + a, ym - 0.5 * a), (x - a, ym + 0.5 * a), w, ink);
    line(c, (x - a, ym + 0.5 * a), (x, ym + a), w, ink);
    line(c, (x, ym + a), (x, y1 + a), w, ink);
}

/// Schnittbild eines Typs in `r` (Papier). `t` sind die Dicken der Schichten,
/// während eines Übergangs Zwischenwerte.
#[allow(clippy::too_many_arguments)]
pub fn paint_section(
    c: &mut Canvas,
    font: Option<&Font>,
    r: Rect,
    look: &TypeLook,
    t: &[f64],
    marks: &SectionMarks,
    s: f32,
    theme: &Theme,
) {
    let rad = 4.0 * s;
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    c.fill(&p, look.paper);
    let n = t.len().min(look.layers.len());
    let t = &t[..n];
    let g = geo(r, look.exterior, t, core_from(look));
    let ink = look.ink;
    let paper = look.paper;
    let lw = s.round().max(1.0);
    let core_w = (1.6 * s).round().max(1.0);
    // Schichten
    for (i, l) in look.layers.iter().take(n).enumerate() {
        let (x0, x1) = (g.xs[i].round(), g.xs[i + 1].round());
        shade(
            c,
            (x0, g.top.round(), x1, g.bottom.round()),
            &l.look,
            l.air,
            paper,
            s,
        );
    }
    for (i, &x) in g.xs.iter().enumerate() {
        let core = (i < n && look.layers[i].core) || (i > 0 && look.layers[i - 1].core);
        let w = if core { core_w } else { lw };
        c.fill_rect(
            x.round() - (w * 0.5).floor(),
            g.top.round(),
            w,
            g.bottom.round() - g.top.round(),
            ink,
        );
    }
    // Decke
    let (sx0, sy0, sx1, sy1) = g.slab;
    let (sx0, sy0, sx1, sy1) = (sx0.round(), sy0.round(), sx1.round(), sy1.round());
    shade_slab(c, sx0, sy0, sx1, sy1, &look.slab, s);
    c.fill_rect(sx0, sy0 - (core_w * 0.5).floor(), sx1 - sx0, core_w, ink);
    c.fill_rect(sx0, sy1 - (core_w * 0.5).floor(), sx1 - sx0, core_w, ink);
    if look.exterior {
        // Kernaußenseite läuft an der Decke durch
        c.fill_rect(sx0 - (core_w * 0.5).floor(), sy0, core_w, sy1 - sy0, ink);
    }
    break_mark(c, sx1, sy0, sy1, ink, s);
    if g.break_left {
        break_mark(c, sx0, sy0, sy1, ink, s);
    }
    // Hervorgehobene Schicht
    let accent = theme.ui.accent;
    if let Some(i) = marks.hover.filter(|&i| i < n) {
        let (x0, x1) = (g.xs[i].round(), g.xs[i + 1].round());
        let (y0, y1) = (g.top.round(), g.bottom.round());
        let a = accent;
        c.fill_rect(x0, y0, x1 - x0, y1 - y0, Rgba(a.0, a.1, a.2, 56));
        let b = (2.0 * s).round().max(1.0);
        let mut p = Path::new();
        p.rounded_rect(x0 - b, y0 - b, x1 - x0 + 2.0 * b, y1 - y0 + 2.0 * b, 0.0);
        p.rounded_rect_hole(x0, y0, x1 - x0, y1 - y0, 0.0);
        c.fill(&p, accent);
    }
    let Some(f) = font else {
        return;
    };
    let px = theme.size.font_small * s;
    let dim = theme.ui.sheet_text_dim;
    let text = theme.ui.sheet_text;
    // Alte Grenze
    let k = if t.iter().sum::<f64>() > 0.0 {
        (g.xs[n] - g.xs[0]) / t.iter().sum::<f64>() as f32
    } else {
        0.0
    };
    if let Some((pos, label)) = &marks.ghost {
        let x = (g.xs[0] + *pos as f32 * k).round();
        dashed_v(c, x, g.top, g.bottom, lw, 4.0 * s, accent);
        // Auf Papier an der Linie unter der Decke; Maßkette und
        // Beschriftung bleiben frei
        let (w, ch, pad) = (f.width(label, px), f.cap_height(px), 3.0 * s);
        let y = 0.5 * (sy1 + g.bottom);
        let bx = (x + 4.0 * s).round();
        let mut p = Path::new();
        p.rounded_rect(
            bx - pad,
            y - 0.5 * ch - pad,
            w + 2.0 * pad,
            ch + 2.0 * pad,
            3.0 * s,
        );
        c.fill(&p, paper);
        f.draw(c, label, px, bx, (y + 0.5 * ch).round(), accent);
    }
    // Beschriftung
    if look.exterior {
        let y = (g.top - 6.0 * s).round();
        for (x, label) in [(g.xs[0], "außen"), (g.xs[n], "innen")] {
            let w = f.width(label, px);
            f.draw(c, label, px, (x - 0.5 * w).round(), y, dim);
        }
    }
    let label = "EG-Decke";
    let lw_ = f.width(label, px);
    let (lx, ly) = (0.5 * (sx0 + sx1), 0.5 * (sy0 + sy1));
    let pad = 4.0 * s;
    let ch = f.cap_height(px);
    let mut p = Path::new();
    p.rounded_rect(
        lx - 0.5 * lw_ - pad,
        ly - 0.5 * ch - pad,
        lw_ + 2.0 * pad,
        ch + 2.0 * pad,
        3.0 * s,
    );
    c.fill(&p, paper);
    f.draw(
        c,
        label,
        px,
        (lx - 0.5 * lw_).round(),
        (ly + 0.5 * ch).round(),
        text,
    );
    // Maßketten
    if n == 0 {
        return;
    }
    let chain_y = (r.y + 0.835 * r.h).round();
    let total_y = (r.y + 0.925 * r.h).round();
    if n > 1 {
        dim_chain(c, &g.xs, chain_y, ink, s);
        for (i, &ti) in t.iter().enumerate() {
            let cx = 0.5 * (g.xs[i] + g.xs[i + 1]);
            dim_label(c, f, ti, cx, chain_y - 4.0 * s, px, text);
        }
    }
    dim_chain(c, &[g.xs[0], g.xs[n]], total_y, ink, s);
    let cx = 0.5 * (g.xs[0] + g.xs[n]);
    dim_label(c, f, t.iter().sum(), cx, total_y - 4.0 * s, px, text);
}

/// Dicke in cm für Listen: „36 cm“, „36,5 cm“.
pub fn cm_text(mm: f64) -> String {
    let cm = (mm / 5.0).round() * 0.5;
    if cm.fract() == 0.0 {
        format!("{cm:.0} cm")
    } else {
        format!("{cm:.1} cm").replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masse_mit_hochgestellter_fuenf() {
        assert_eq!(dim_text(240.0), ("24".to_string(), false));
        assert_eq!(dim_text(365.0), ("36".to_string(), true));
        assert_eq!(cm_text(365.0), "36,5 cm");
        assert_eq!(cm_text(360.0), "36 cm");
    }

    #[test]
    fn schnittbild_trifft_schichten() {
        let m = Model::new();
        let theme = Theme::dark();
        let id = m.default_type(TypeCategory::ExteriorWall);
        let set = m.layer_set(id).unwrap();
        let look = type_look(&m, &theme, set);
        let r = Rect::new(0.0, 0.0, 430.0, 262.0);
        let t = look.thicknesses();
        let g = geo(r, true, &t, core_from(&look));
        // außen links bei 0,195 der Breite
        assert!((g.xs[0] - 83.85).abs() < 0.01);
        let mid = 0.5 * (g.xs[0] + g.xs[1]);
        assert_eq!(section_layer_at(r, &look, &t, mid, 150.0), Some(0));
        let mid = 0.5 * (g.xs[1] + g.xs[2]);
        assert_eq!(section_layer_at(r, &look, &t, mid, 150.0), Some(1));
        assert_eq!(section_layer_at(r, &look, &t, 5.0, 150.0), None);
        // Zeichnen ohne Schrift und mit Hover läuft durch
        let mut c = Canvas::new(430, 262);
        let marks = SectionMarks {
            hover: Some(0),
            ghost: Some((120.0, "+4".into())),
        };
        paint_section(&mut c, None, r, &look, &t, &marks, 1.0, &theme);
        let mut c = Canvas::new(30, 34);
        paint_thumb(&mut c, Rect::new(0.0, 0.0, 30.0, 34.0), &look, 1.0);
    }
}

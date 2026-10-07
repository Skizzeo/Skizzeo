//! Kettensymbol je gestapeltem Wandsegment (OG Phase 2, Sollbild
//! `soll-og2-1-kette.png`): ein dunkles Plättchen 12 px außen neben der
//! Mitte des Wandfußes. Gekoppelt erscheint es nur am Band unter der Maus,
//! gelöst bleibt es klein stehen. Klick schaltet die Kette um.
//!
//! Die Plättchen sind Oberflächenbilder auf eigenen Plätzen; je Bild ändern
//! sich nur ihre Lagen, gezeichnet wird ein Plättchen nur, wenn sich sein
//! Aussehen ändert.

use crate::camera::Camera;
use crate::scene::Scene;
use sk_math::Vec3;
use sk_model::ElementId;
use sk_paint::{Canvas, Path, Rgba};
use sk_render::Renderer;
use sk_ui::theme::Theme;

/// Höchstens so viele Plättchen gleichzeitig.
pub const SLOTS: usize = 12;
/// Plättchen (dip): Kante, Abstand zum Band, Radius; gelöst dauerhaft kleiner.
pub const CHIP: f64 = 22.0;
const GAP: f64 = 12.0;
const RADIUS: f32 = 5.0;
const SMALL: f64 = 0.85;

/// Ein Plättchen im Fenster.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chip {
    pub wall: ElementId,
    pub linked: bool,
    /// Gelöst und nicht unter der Maus: klein.
    pub small: bool,
    pub hover: bool,
    /// Mitte (Fenster-Pixel).
    pub at: (f64, f64),
}

impl Chip {
    /// Kantenlänge in Pixeln.
    pub fn size(&self, scale: f64) -> f64 {
        (CHIP * scale * if self.small { SMALL } else { 1.0 }).round()
    }

    pub fn contains(&self, x: f64, y: f64, scale: f64) -> bool {
        // Treffer immer auf der vollen Fläche
        let r = CHIP * scale * 0.5;
        (x - self.at.0).abs() <= r && (y - self.at.1).abs() <= r
    }
}

/// Was gezeigt wird.
pub struct Want<'a> {
    pub scene: &'a Scene,
    pub cam: &'a Camera,
    /// Größe der 3D-Ansicht und Höhe der Titelleiste (Pixel).
    pub w: f64,
    pub h: f64,
    pub top: f64,
    pub scale: f64,
    /// Grundriss: nur die Füße auf dieser Höhe (mm).
    pub plan_z: Option<f64>,
    /// Wand unter der Maus bzw. gezogen (ihr Band leuchtet).
    pub band: Option<ElementId>,
    /// Plättchen unter der Maus.
    pub hover: Option<ElementId>,
}

/// Die Plättchen der gestapelten Wände: gelöste immer, gekoppelte nur am
/// Band unter der Maus (oder solange die Maus auf dem Plättchen steht).
pub fn chips(wnt: &Want) -> Vec<Chip> {
    let m = wnt.scene.model();
    let mut out = Vec::new();
    for (run, _, foot) in wnt.scene.stacked_feet() {
        if let Some(z) = wnt.plan_z {
            if foot.first().is_some_and(|(a, _)| (a.z - z).abs() >= 1.0) {
                continue;
            }
        }
        let Some(chain) = wnt.scene.chain(run) else {
            continue;
        };
        for (k, &(a, b)) in foot.iter().enumerate() {
            let Some(wall) = m.wall_at(run, k) else {
                continue;
            };
            let Some((_, linked)) = m.stack_offset(wall) else {
                continue;
            };
            let near = wnt.band == Some(wall) || wnt.hover == Some(wall);
            if linked && !near {
                continue;
            }
            let Some(n) = chain.segment_normal(k) else {
                continue;
            };
            let out_n = n * chain.outward_sign();
            // In 3D nur, wenn der Wandfuß zu sehen ist
            if wnt.plan_z.is_none() && !visible(wnt, (a + b) * 0.5 + out_n * 60.0) {
                continue;
            }
            let Some(at) = place(wnt, (a + b) * 0.5, out_n) else {
                continue;
            };
            out.push(Chip {
                wall,
                linked,
                small: !near,
                hover: wnt.hover == Some(wall),
                at,
            });
        }
    }
    out
}

/// Liegt zwischen Auge und `p` (knapp vor der Fassade) kein Bauteil?
fn visible(wnt: &Want, p: Vec3) -> bool {
    let cam = wnt.cam;
    let (o, d) = if cam.ortho.is_some() {
        let f = cam.forward();
        (p - f * 1e7, f)
    } else {
        let v = p - cam.eye;
        (cam.eye, v * (1.0 / v.length()))
    };
    let dist = (p - o).dot(d);
    wnt.scene.raycast(o, d).is_none_or(|(t, _)| t >= dist - 1.0)
}

/// Mitte des Plättchens: außen neben der Mitte des Wandfußes `mid`.
fn place(wnt: &Want, mid: Vec3, out: Vec3) -> Option<(f64, f64)> {
    let p0 = wnt.cam.project(mid, wnt.w, wnt.h)?;
    let p1 = wnt.cam.project(mid + out * 200.0, wnt.w, wnt.h)?;
    let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
    let len = (dx * dx + dy * dy).sqrt();
    // Von der Seite gesehen: unter den Fuß
    let (ux, uy) = if len > 1e-3 {
        (dx / len, dy / len)
    } else {
        (0.0, 1.0)
    };
    let d = (GAP + CHIP * 0.5) * wnt.scale;
    let (x, y) = (p0.0 + ux * d, p0.1 + uy * d + wnt.top);
    let inside = x >= 0.0 && y >= wnt.top && x <= wnt.w && y <= wnt.h + wnt.top;
    inside.then_some((x, y))
}

/// Plättchen unter der Maus.
pub fn hit(chips: &[Chip], x: f64, y: f64, scale: f64) -> Option<ElementId> {
    chips
        .iter()
        .rev()
        .find(|c| c.contains(x, y, scale))
        .map(|c| c.wall)
}

/// Hinweis am Plättchen, zwei Zeilen wie im Sollbild.
pub fn tip(linked: bool) -> &'static str {
    if linked {
        "Gekoppelt: EG und OG bewegen sich gemeinsam.\nKlick löst. Strg beim Ziehen: nur diese Wand."
    } else {
        "Gelöst: EG und OG werden getrennt gezogen.\nKlick koppelt. Der Versatz bleibt."
    }
}

/// Aussehen eines Plättchens (für den Vergleich, ob neu zu zeichnen ist).
type Look = (bool, bool, bool, u32, u64);

/// Die Plättchen als Oberflächenbilder auf den Plätzen `base` …
pub struct LinkView {
    base: usize,
    looks: [Option<Look>; SLOTS],
    shown: usize,
}

impl LinkView {
    pub fn new(base: usize) -> LinkView {
        LinkView {
            base,
            looks: [None; SLOTS],
            shown: 0,
        }
    }

    /// Zeigt die Plättchen; die übrigen Plätze werden leer. `theme_key`
    /// ändert sich mit dem Farbschema.
    pub fn show(
        &mut self,
        r: &mut Renderer,
        chips: &[Chip],
        t: &Theme,
        theme_key: u64,
        scale: f64,
    ) {
        let n = chips.len().min(SLOTS);
        for (i, c) in chips.iter().take(SLOTS).enumerate() {
            let look = (
                c.linked,
                c.small,
                c.hover,
                (scale * 100.0) as u32,
                theme_key,
            );
            let size = c.size(scale);
            if self.looks[i] != Some(look) {
                let img = paint(t, c.linked, c.hover, size as f32, scale as f32);
                r.set_overlay(
                    self.base + i,
                    0,
                    0,
                    img.width as u32,
                    img.height as u32,
                    &img.to_premul_rgba8(),
                );
                self.looks[i] = Some(look);
            }
            let (x, y) = (
                (c.at.0 - size * 0.5).round() as i32,
                (c.at.1 - size * 0.5).round() as i32,
            );
            r.place_overlay(self.base + i, x, y, size as i32, size as i32, 1.0);
        }
        for i in n..self.shown.max(n) {
            r.place_overlay(self.base + i, 0, 0, 0, 0, 0.0);
        }
        self.shown = n;
    }
}

fn lift(c: Rgba, k: f32) -> Rgba {
    let m = |v: u8| (v as f32 + (255.0 - v as f32) * k).round() as u8;
    Rgba(m(c.0), m(c.1), m(c.2), c.3)
}

/// Plättchen mit zwei Gliedern: gekoppelt greifen sie ineinander (Akzent),
/// gelöst liegen sie gekippt auseinander (grau).
pub fn paint(t: &Theme, linked: bool, hover: bool, size: f32, scale: f32) -> Canvas {
    let n = size.max(1.0) as usize;
    let mut c = Canvas::new(n, n);
    let u = &t.ui;
    let k = size / (CHIP as f32 * scale);
    let fill = if hover {
        lift(u.link_chip, 0.08)
    } else {
        u.link_chip
    };
    let mut p = Path::new();
    p.rounded_rect(0.0, 0.0, size, size, RADIUS * scale * k);
    c.fill(&p, fill);
    let (cx, cy) = (size * 0.5, size * 0.5);
    let s = scale * k;
    let (w, h, stroke) = (9.0 * s, 5.0 * s, 1.5 * s);
    if linked {
        for dx in [-3.0, 3.0] {
            ring(&mut c, cx + dx * s, cy, w, h, 0.0, stroke, u.link_on);
        }
    } else {
        ring(
            &mut c,
            cx - 4.5 * s,
            cy + 0.5 * s,
            w,
            h,
            0.45,
            stroke,
            u.link_off,
        );
        ring(
            &mut c,
            cx + 4.5 * s,
            cy - 0.5 * s,
            w,
            h,
            0.45,
            stroke,
            u.link_off,
        );
    }
    c
}

/// Glied: Langloch `w` × `h` als Ring der Stärke `stroke`, um `angle`
/// (Bogenmaß) gedreht.
#[allow(clippy::too_many_arguments)]
fn ring(c: &mut Canvas, cx: f32, cy: f32, w: f32, h: f32, angle: f32, stroke: f32, col: Rgba) {
    let (sin, cos) = angle.sin_cos();
    let half = (w - h).max(0.0) * 0.5;
    let outline = |r: f32| -> Vec<(f32, f32)> {
        let mut v = Vec::new();
        for (ox, a0) in [(half, -90.0f32), (-half, 90.0)] {
            for i in 0..=12 {
                let a = (a0 + i as f32 * 15.0).to_radians();
                let (x, y) = (ox + r * a.cos(), r * a.sin());
                v.push((cx + x * cos - y * sin, cy + x * sin + y * cos));
            }
        }
        v
    };
    let r = h * 0.5;
    let mut p = Path::new();
    let outer = outline(r + stroke * 0.5);
    p.move_to(outer[0].0, outer[0].1);
    for q in &outer[1..] {
        p.line_to(q.0, q.1);
    }
    p.close();
    let inner = outline((r - stroke * 0.5).max(0.0));
    let last = inner.len() - 1;
    p.move_to(inner[last].0, inner[last].1);
    for q in inner[..last].iter().rev() {
        p.line_to(q.0, q.1);
    }
    p.close();
    c.fill(&p, col);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gekoppelt: Akzent in der Mitte; gelöst: grau; das Plättchen deckt.
    #[test]
    fn plaettchen_zeigt_den_zustand() {
        let t = Theme::dark();
        let on = paint(&t, true, false, 22.0, 1.0);
        let off = paint(&t, false, false, 22.0, 1.0);
        let px = |c: &Canvas, x: usize, y: usize| {
            let i = (y * c.width + x) * 4;
            let v = c.to_rgba8();
            (v[i], v[i + 1], v[i + 2], v[i + 3])
        };
        assert_eq!(px(&on, 1, 11).3, 255, "Plättchen deckt");
        let a = t.ui.link_on;
        let mut on_px = (0..22).flat_map(|x| (0..22).map(move |y| (x, y)));
        assert!(on_px
            .clone()
            .any(|(x, y)| px(&on, x, y) == (a.0, a.1, a.2, 255)));
        assert!(!on_px.any(|(x, y)| px(&off, x, y) == (a.0, a.1, a.2, 255)));
    }
}

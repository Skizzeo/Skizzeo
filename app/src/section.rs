//! Schnittlinie im Grundriss nach DIN 1356: dünne Strichpunktlinie mit kräftigen
//! Enden, Pfeile in Blickrichtung und Kennbuchstaben „A“. Im Grundriss lässt sie
//! sich greifen und quer verschieben; die Ansicht „Schnitt“ folgt ihr.

use crate::camera::Camera;
use crate::scene::Scene;
use sk_math::{vec3, Vec3};
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{Event, MouseButton};
use sk_render::Helper;
use sk_ui::widgets::Fonts;

/// Greifabstand in Pixeln (bei 96 dpi).
const PICK_PX: f64 = 8.0;
/// Überstand der Linie über das Gebäude (mm).
const OVERHANG: f64 = 1500.0;
/// Raster beim Verschieben (mm).
const STEP: f64 = 10.0;

const INK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const HOT: [f32; 4] = [0.56, 0.27, 0.86, 1.0];

#[derive(Default)]
pub struct SectionLine {
    /// Lage der senkrechten Schnittebene (y in mm); Blick in +y.
    pub y: Option<f64>,
    hover: bool,
    /// Beim Ziehen: Abstand zwischen Griffpunkt und Linie.
    drag: Option<f64>,
}

#[derive(Default)]
pub struct SectionOutcome {
    pub redraw: bool,
    /// Lage geändert: Schnitt neu berechnen.
    pub changed: bool,
    pub consumed: bool,
}

/// Lage eines Endsymbols im Bild (Pixel der 3D-Ansicht).
pub struct Mark {
    pub x: f64,
    pub y: f64,
    pub left: bool,
}

/// Bildgröße eines Endsymbols in Pixeln.
fn mark_size(scale: f32) -> (f32, f32) {
    ((44.0 * scale).round(), (52.0 * scale).round())
}

fn dist_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let len2 = vx * vx + vy * vy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((p.0 - a.0 - vx * t).powi(2) + (p.1 - a.1 - vy * t).powi(2)).sqrt()
}

impl SectionLine {
    /// Setzt die Schnittlinie beim ersten Gebrauch in die Mitte des Modells.
    pub fn ensure(&mut self, scene: &Scene) {
        if self.y.is_none() {
            self.y = scene.center().map(|c| (c.y / STEP).round() * STEP);
        }
    }

    /// Schnittebene: Punkt und Normale zum Betrachter (Blick in +y).
    pub fn plane(&self) -> Option<(Vec3, Vec3)> {
        self.y.map(|y| (vec3(0.0, y, 0.0), vec3(0.0, -1.0, 0.0)))
    }

    pub fn is_busy(&self) -> bool {
        self.hover || self.drag.is_some()
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Anfang und Ende der Linie (x-Bereich des Modells mit Überstand).
    fn ends(&self, scene: &Scene) -> Option<(Vec3, Vec3)> {
        let y = self.y?;
        let (x0, x1) = scene
            .bounds()
            .map_or((-2000.0, 12000.0), |(lo, hi)| (lo.x, hi.x));
        Some((vec3(x0 - OVERHANG, y, 0.0), vec3(x1 + OVERHANG, y, 0.0)))
    }

    fn near(&self, scene: &Scene, cam: &Camera, m: (f64, f64), w: f64, h: f64, scale: f64) -> bool {
        let Some((a, b)) = self.ends(scene) else {
            return false;
        };
        match (cam.project(a, w, h), cam.project(b, w, h)) {
            (Some(pa), Some(pb)) => dist_to_segment(m, pa, pb) < PICK_PX * scale,
            _ => false,
        }
    }

    /// Verarbeitet ein Ereignis (Koordinaten der 3D-Ansicht). `enabled` nur im Grundriss.
    #[allow(clippy::too_many_arguments)]
    pub fn handle(
        &mut self,
        e: &Event,
        scene: &Scene,
        cam: &Camera,
        w: f64,
        h: f64,
        scale: f64,
        enabled: bool,
    ) -> SectionOutcome {
        let mut out = SectionOutcome::default();
        if !enabled && self.drag.is_none() {
            out.redraw = std::mem::take(&mut self.hover);
            return out;
        }
        match *e {
            Event::MouseMove { x, y, .. } => {
                if let Some(off) = self.drag {
                    if let Some(g) = cam.ground_point(x, y, w, h) {
                        let ny = ((g.y - off) / STEP).round() * STEP;
                        if self.y != Some(ny) {
                            self.y = Some(ny);
                            out.changed = true;
                            out.redraw = true;
                        }
                    }
                } else {
                    let hover = self.near(scene, cam, (x, y), w, h, scale);
                    out.redraw = hover != self.hover;
                    self.hover = hover;
                }
            }
            Event::MouseLeave => {
                if self.drag.is_none() {
                    out.redraw = std::mem::take(&mut self.hover);
                }
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.hover = self.near(scene, cam, (x, y), w, h, scale);
                if self.hover {
                    if let (Some(g), Some(sy)) = (cam.ground_point(x, y, w, h), self.y) {
                        self.drag = Some(g.y - sy);
                        out.consumed = true;
                        out.redraw = true;
                    }
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } if self.drag.take().is_some() => {
                out.consumed = true;
                out.redraw = true;
            }
            _ => {}
        }
        out
    }

    /// Linie als Hilfslinien: Strichpunkt in der Mitte, kräftige Enden.
    pub fn helpers(&self, scene: &Scene, cam: &Camera, h: f64, scale: f32) -> Vec<Helper> {
        let Some((a, b)) = self.ends(scene) else {
            return Vec::new();
        };
        let color = if self.is_busy() { HOT } else { INK };
        let mm_per_px = cam.ortho.map_or(10.0, |half| 2.0 * half / h.max(1.0));
        let end = 16.0 * mm_per_px * scale as f64;
        let lift = |p: Vec3| [p.x as f32, p.y as f32, p.z as f32 + 2.0];
        let line = |p: Vec3, q: Vec3, width: f32, dash: f32| Helper {
            a: lift(p),
            b: lift(q),
            color,
            width: width * scale,
            dash: dash * scale,
            occlude: false,
        };
        let dx = vec3(end, 0.0, 0.0);
        vec![
            line(a + dx, b - dx, 1.2, -6.0),
            line(a, a + dx, 3.2, 0.0),
            line(b - dx, b, 3.2, 0.0),
        ]
    }

    /// Lage der beiden Endsymbole im Bild.
    pub fn marks(&self, scene: &Scene, cam: &Camera, w: f64, h: f64) -> Vec<Mark> {
        let Some((a, b)) = self.ends(scene) else {
            return Vec::new();
        };
        [(a, true), (b, false)]
            .into_iter()
            .filter_map(|(p, left)| cam.project(p, w, h).map(|(x, y)| Mark { x, y, left }))
            .collect()
    }

    /// Bezugspunkt (Linienende) im Bild eines Endsymbols.
    pub fn mark_anchor(&self, left: bool, scale: f32) -> (f32, f32) {
        let (cw, ch) = mark_size(scale);
        (
            if left {
                10.0 * scale
            } else {
                cw - 10.0 * scale
            },
            ch - 6.0 * scale,
        )
    }

    /// Bild eines Endsymbols (Pfeil in Blickrichtung und Buchstabe) und sein
    /// Bezugspunkt (Ende der Linie) im Bild.
    pub fn paint_mark(&self, fonts: &Fonts, left: bool, scale: f32) -> (Canvas, f32, f32) {
        let s = scale;
        let (cw, ch) = mark_size(scale);
        let mut c = Canvas::new(cw as usize, ch as usize);
        let color = if self.is_busy() {
            Rgba::rgb(143, 69, 219)
        } else {
            Rgba::rgb(0, 0, 0)
        };
        // Bezugspunkt: am Linienende; der Pfeil steht senkrecht darauf (Blick nach oben = +y)
        let (ax, ay) = self.mark_anchor(left, scale);
        let shaft = 2.4 * s;
        let mut p = Path::new();
        p.move_to(ax - shaft * 0.5, ay)
            .line_to(ax + shaft * 0.5, ay)
            .line_to(ax + shaft * 0.5, ay - 22.0 * s)
            .line_to(ax - shaft * 0.5, ay - 22.0 * s)
            .close();
        c.fill(&p, color);
        let mut p = Path::new();
        p.move_to(ax, ay - 36.0 * s)
            .line_to(ax + 6.5 * s, ay - 20.0 * s)
            .line_to(ax - 6.5 * s, ay - 20.0 * s)
            .close();
        c.fill(&p, color);
        if let Some(f) = fonts.bold.as_ref().or(fonts.regular.as_ref()) {
            let px = 17.0 * s;
            let tw = f.width("A", px);
            let tx = if left {
                ax + 7.0 * s
            } else {
                ax - 7.0 * s - tw
            };
            f.draw(&mut c, "A", px, tx.round(), (ay - 8.0 * s).round(), color);
        }
        (c, ax, ay)
    }
}

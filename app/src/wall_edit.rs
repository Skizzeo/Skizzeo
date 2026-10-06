//! Gummiband: violette Linie am äußeren Wandfuß, sichtbar beim Darüberfahren.
//!
//! Mit der linken Maustaste lässt sich ein Segment des Bandes quer zu seiner
//! Richtung ziehen. Die Wand geht live mit, die Nachbarsegmente behalten ihre
//! Richtung und werden länger oder kürzer. Esc bricht das Ziehen ab.

use crate::camera::Camera;
use crate::scene::Scene;
use sk_math::Vec3;
use sk_model::WallChain;
use sk_platform::{Event, Key, MouseButton};
use sk_render::Helper;

/// Greifabstand zum Band in Pixeln (bei 96 dpi).
const PICK_PX: f64 = 8.0;
/// Raster beim Ziehen in Millimetern.
const STEP: f64 = 10.0;

const BAND_HOT: [f32; 4] = [0.74, 0.50, 1.0, 1.0];
const BAND_GHOST: [f32; 4] = [0.56, 0.27, 0.86, 0.7];
const DARK: [f32; 4] = [0.0, 0.0, 0.0, 0.55];

struct Drag {
    wall: usize,
    seg: usize,
    start: Vec3,
    normal: Vec3,
    original: WallChain,
    before: Vec<WallChain>,
}

#[derive(Default)]
pub struct WallEdit {
    /// Segment unter der Maus: (Wand, Segment).
    hover: Option<(usize, usize)>,
    drag: Option<Drag>,
    mouse: Option<(f64, f64)>,
}

/// Ergebnis eines Ereignisses für die App.
#[derive(Default)]
pub struct EditOutcome {
    pub redraw: bool,
    /// Wände haben sich geändert, das Netz muss neu hochgeladen werden.
    pub changed: bool,
    /// Ereignis gehört dem Band, das Wandwerkzeug bekommt es nicht.
    pub consumed: bool,
}

/// Bandsegmente einer Wand: (Anfang, Ende) am äußeren Wandfuß.
fn band(w: &WallChain) -> Vec<(Vec3, Vec3)> {
    let c = w.face_corners(w.outer_offset());
    let n = c.len();
    (0..w.segment_count())
        .map(|k| (c[k], c[(k + 1) % n]))
        .collect()
}

fn dist_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let len2 = vx * vx + vy * vy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (qx, qy) = (a.0 + vx * t, a.1 + vy * t);
    ((p.0 - qx).powi(2) + (p.1 - qy).powi(2)).sqrt()
}

impl WallEdit {
    pub fn is_busy(&self) -> bool {
        self.hover.is_some() || self.drag.is_some()
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    fn pick(
        &self,
        scene: &Scene,
        cam: &Camera,
        w: f64,
        h: f64,
        scale: f64,
    ) -> Option<(usize, usize)> {
        let m = self.mouse?;
        let mut best: Option<(f64, (usize, usize))> = None;
        for (i, wall) in scene.walls.iter().enumerate() {
            for (k, (a, b)) in band(wall).into_iter().enumerate() {
                let (Some(pa), Some(pb)) = (cam.project(a, w, h), cam.project(b, w, h)) else {
                    continue;
                };
                let d = dist_to_segment(m, pa, pb);
                if d < PICK_PX * scale && best.is_none_or(|b| d < b.0) {
                    best = Some((d, (i, k)));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// Greifstelle neu bestimmen (nach Kamerawechsel oder Änderung der Wände).
    pub fn refresh(
        &mut self,
        scene: &Scene,
        cam: &Camera,
        w: f64,
        h: f64,
        scale: f64,
        enabled: bool,
    ) -> bool {
        if self.drag.is_some() {
            return false;
        }
        let hover = if enabled {
            self.pick(scene, cam, w, h, scale)
        } else {
            None
        };
        let changed = hover != self.hover;
        self.hover = hover;
        changed
    }

    fn update_drag(&mut self, scene: &mut Scene, cam: &Camera, w: f64, h: f64) -> bool {
        let (Some(d), Some((mx, my))) = (&self.drag, self.mouse) else {
            return false;
        };
        let Some(g) = cam.ground_point(mx, my, w, h) else {
            return false;
        };
        let off = ((g - d.start).dot(d.normal) / STEP).round() * STEP;
        match d.original.with_segment_moved(d.seg, off) {
            Some(moved) if scene.walls[d.wall] != moved => {
                scene.set_wall(d.wall, moved);
                true
            }
            _ => false,
        }
    }

    /// Verarbeitet ein Ereignis (Mauskoordinaten relativ zur 3D-Ansicht).
    /// `enabled` ist falsch, solange gerade ein Wandzug gezeichnet wird.
    #[allow(clippy::too_many_arguments)]
    pub fn handle(
        &mut self,
        e: &Event,
        scene: &mut Scene,
        cam: &Camera,
        w: f64,
        h: f64,
        scale: f64,
        enabled: bool,
    ) -> EditOutcome {
        let mut out = EditOutcome::default();
        match *e {
            Event::MouseMove { x, y, .. } => {
                self.mouse = Some((x, y));
                if self.drag.is_some() {
                    out.changed = self.update_drag(scene, cam, w, h);
                    out.redraw = out.changed;
                } else {
                    out.redraw = self.refresh(scene, cam, w, h, scale, enabled);
                }
            }
            Event::MouseLeave => {
                self.mouse = None;
                if self.drag.is_none() {
                    out.redraw = self.hover.take().is_some();
                }
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.mouse = Some((x, y));
                self.refresh(scene, cam, w, h, scale, enabled);
                let Some((wall, seg)) = self.hover else {
                    return out;
                };
                out.consumed = true;
                let original = scene.walls[wall].clone();
                let (Some(start), Some(normal)) =
                    (cam.ground_point(x, y, w, h), original.segment_normal(seg))
                else {
                    return out;
                };
                self.drag = Some(Drag {
                    wall,
                    seg,
                    start,
                    normal,
                    original,
                    before: scene.walls.clone(),
                });
                out.redraw = true;
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if let Some(d) = self.drag.take() {
                    scene.record(d.before);
                    out.consumed = true;
                    out.redraw = true;
                    self.refresh(scene, cam, w, h, scale, enabled);
                }
            }
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } => {
                if let Some(d) = self.drag.take() {
                    scene.set_walls(d.before);
                    out.consumed = true;
                    out.changed = true;
                    out.redraw = true;
                    self.refresh(scene, cam, w, h, scale, enabled);
                }
            }
            _ => {}
        }
        out
    }

    /// Gummibänder aller Wände, das gegriffene Segment hervorgehoben.
    /// `occlude`: hinter Wänden liegende Teile blass (3D); sonst immer voll sichtbar.
    pub fn helpers(&self, scene: &Scene, scale: f32, occlude: bool) -> Vec<Helper> {
        let mut out = Vec::new();
        let lift = |p: Vec3| [p.x as f32, p.y as f32, p.z as f32 + 2.0];
        let line = |a: Vec3, b: Vec3, color, width: f32, dash: f32| Helper {
            a: lift(a),
            b: lift(b),
            color,
            width: width * scale,
            dash: dash * scale,
            occlude,
        };
        let active = self.drag.as_ref().map(|d| (d.wall, d.seg)).or(self.hover);

        // Beim Ziehen: ursprüngliche Lage gestrichelt
        if let Some(d) = &self.drag {
            if let Some(&(a, b)) = band(&d.original).get(d.seg) {
                out.push(line(a, b, BAND_GHOST, 1.5, 6.0));
            }
        }
        // Sichtbar nur das Segment unter der Maus bzw. das gezogene
        if let Some((i, k)) = active {
            if let Some((a, b)) = scene.walls.get(i).and_then(|w| band(w).get(k).copied()) {
                out.push(line(a, b, DARK, 6.0, 0.0));
                out.push(line(a, b, BAND_HOT, 4.0, 0.0));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::RefSide;
    use sk_platform::Modifiers;

    const W: f64 = 1200.0;
    const H: f64 = 800.0;

    fn setup() -> (Scene, Camera) {
        let mut s = Scene::new();
        s.add_wall(WallChain {
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 4000.0, 0.0),
                vec3(5000.0, 4000.0, 0.0),
                vec3(5000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            layers: vec![sk_model::Layer::new(400.0, 0)],
            height: 3500.0,
        });
        let c = Camera::looking_at(
            vec3(2500.0, -9000.0, 14000.0),
            vec3(2500.0, 2000.0, 0.0),
            45.0,
        );
        (s, c)
    }

    fn at(c: &Camera, p: Vec3) -> (f64, f64) {
        c.project(p, W, H).unwrap()
    }

    #[test]
    fn oberes_segment_nach_aussen_ziehen() {
        let (mut s, c) = setup();
        let mut e = WallEdit::default();
        let m = Modifiers::default();
        let (x, y) = at(&c, vec3(2500.0, 4000.0, 0.0));
        e.handle(
            &Event::MouseMove { x, y, mods: m },
            &mut s,
            &c,
            W,
            H,
            1.0,
            true,
        );
        assert_eq!(e.hover, Some((0, 1)));
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: m,
        };
        assert!(e.handle(&down, &mut s, &c, W, H, 1.0, true).consumed);
        let (x, y) = at(&c, vec3(2600.0, 5003.0, 0.0));
        let out = e.handle(
            &Event::MouseMove { x, y, mods: m },
            &mut s,
            &c,
            W,
            H,
            1.0,
            true,
        );
        assert!(out.changed);
        let p = &s.walls[0].points;
        assert!(
            (p[1].y - 5000.0).abs() < 1e-6 && (p[2].y - 5000.0).abs() < 1e-6,
            "{p:?}"
        );
        assert!(p[1].x.abs() < 1e-6 && (p[2].x - 5000.0).abs() < 1e-6);
        let up = Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods: m,
        };
        e.handle(&up, &mut s, &c, W, H, 1.0, true);
        assert!(s.undo());
        assert!((s.walls[0].points[1].y - 4000.0).abs() < 1e-6);
    }

    #[test]
    fn ohne_freigabe_kein_greifen() {
        let (mut s, c) = setup();
        let mut e = WallEdit::default();
        let (x, y) = at(&c, vec3(2500.0, 4000.0, 0.0));
        let mv = Event::MouseMove {
            x,
            y,
            mods: Modifiers::default(),
        };
        e.handle(&mv, &mut s, &c, W, H, 1.0, false);
        assert!(!e.is_busy());
    }
}

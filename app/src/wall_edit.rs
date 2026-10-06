//! Gummiband: violette Linie am äußeren Wandfuß, sichtbar beim Darüberfahren.
//!
//! Mit der linken Maustaste lässt sich ein Segment des Bandes quer zu seiner
//! Richtung ziehen. Die Wand geht live mit, die Nachbarsegmente behalten ihre
//! Richtung und werden länger oder kürzer. Esc bricht das Ziehen ab.

use crate::camera::Camera;
use crate::scene::Scene;
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, Model, RunId, WallChain};
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
    /// Gezogene Wand und ihr Platz im Wandzug.
    wall: ElementId,
    run: RunId,
    seg: usize,
    start: Vec3,
    normal: Vec3,
    original: WallChain,
    before: Model,
}

#[derive(Default)]
pub struct WallEdit {
    /// Wand unter der Maus.
    hover: Option<ElementId>,
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

/// Bildrechteck (x0, y0, x1, y1) eines Quaders auf dem Boden. `None`, wenn eine
/// Ecke hinter der Kamera liegt (dann ist kein Vortest möglich).
fn screen_rect(
    cam: &Camera,
    (lo, hi): (Vec3, Vec3),
    w: f64,
    h: f64,
) -> Option<(f64, f64, f64, f64)> {
    let mut r = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in [(lo.x, lo.y), (hi.x, lo.y), (lo.x, hi.y), (hi.x, hi.y)] {
        let (px, py) = cam.project(vec3(x, y, lo.z), w, h)?;
        r = (r.0.min(px), r.1.min(py), r.2.max(px), r.3.max(py));
    }
    Some(r)
}

/// Bild einer Strecke. Ragt sie hinter die Kamera, wird sie an der nahen
/// Schnittebene gekürzt, damit lange Wände auch aus der Nähe greifbar bleiben.
fn project_segment(
    cam: &Camera,
    a: Vec3,
    b: Vec3,
    w: f64,
    h: f64,
) -> Option<((f64, f64), (f64, f64))> {
    let f = cam.forward();
    let near = cam.near() * 1.01;
    let (da, db) = ((a - cam.eye).dot(f) - near, (b - cam.eye).dot(f) - near);
    if da < 0.0 && db < 0.0 {
        return None;
    }
    let cut = |p: Vec3, q: Vec3, dp: f64, dq: f64| p + (q - p) * (dp / (dp - dq));
    let a2 = if da < 0.0 { cut(a, b, da, db) } else { a };
    let b2 = if db < 0.0 { cut(a, b, da, db) } else { b };
    Some((cam.project(a2, w, h)?, cam.project(b2, w, h)?))
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

    /// Wandzug, der gerade gezogen wird.
    pub fn dragging_run(&self) -> Option<RunId> {
        self.drag.as_ref().map(|d| d.run)
    }

    fn pick(&self, scene: &Scene, cam: &Camera, w: f64, h: f64, scale: f64) -> Option<ElementId> {
        let m = self.mouse?;
        let reach = PICK_PX * scale;
        let mut best: Option<(f64, (RunId, usize))> = None;
        for (run, bounds, foot) in scene.feet() {
            // Vortest: Liegt die Maus weit neben dem Bild des ganzen Wandfußes,
            // kann kein Segment dieses Zuges getroffen sein
            if let Some(r) = bounds.and_then(|b| screen_rect(cam, b, w, h)) {
                if m.0 < r.0 - reach || m.0 > r.2 + reach || m.1 < r.1 - reach || m.1 > r.3 + reach
                {
                    continue;
                }
            }
            for (k, &(a, b)) in foot.iter().enumerate() {
                let Some((pa, pb)) = project_segment(cam, a, b, w, h) else {
                    continue;
                };
                let d = dist_to_segment(m, pa, pb);
                if d < reach && best.is_none_or(|b| d < b.0) {
                    best = Some((d, (run, k)));
                }
            }
        }
        best.and_then(|(_, (run, k))| scene.model().wall_at(run, k))
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
            Some(moved) if scene.chain(d.run).is_some_and(|c| c.points != moved.points) => {
                scene.set_run_points(d.run, &moved.points);
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
                let Some(wall) = self.hover else {
                    return out;
                };
                out.consumed = true;
                let Some((run, seg)) = scene.model().segment_of(wall) else {
                    return out;
                };
                let Some(original) = scene.chain(run) else {
                    return out;
                };
                let (Some(start), Some(normal)) =
                    (cam.ground_point(x, y, w, h), original.segment_normal(seg))
                else {
                    return out;
                };
                self.drag = Some(Drag {
                    wall,
                    run,
                    seg,
                    start,
                    normal,
                    original: original.clone(),
                    before: scene.snapshot(),
                });
                out.redraw = true;
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if let Some(d) = self.drag.take() {
                    // Nur ein Verlaufsschritt, wenn die Wand wirklich woanders steht
                    if scene
                        .chain(d.run)
                        .is_some_and(|c| c.points != d.original.points)
                    {
                        scene.record(d.before);
                    }
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
                    scene.restore(d.before);
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
        let active = self.drag.as_ref().map(|d| d.wall).or(self.hover);

        // Beim Ziehen: ursprüngliche Lage gestrichelt
        if let Some(d) = &self.drag {
            if let Some(&(a, b)) = d.original.outer_foot().get(d.seg) {
                out.push(line(a, b, BAND_GHOST, 1.5, 6.0));
            }
        }
        // Sichtbar nur das Segment unter der Maus bzw. das gezogene
        let segment = active.and_then(|e| scene.model().segment_of(e));
        if let Some((run, k)) = segment {
            if let Some((a, b)) = scene.foot(run).and_then(|f| f.get(k).copied()) {
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
        s.add_wall(&WallChain {
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 4000.0, 0.0),
                vec3(5000.0, 4000.0, 0.0),
                vec3(5000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            layers: Vec::new(),
            height: 3500.0,
        })
        .unwrap();
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
        let run = s.model().runs().ids().next().unwrap();
        let wall = s.model().wall_at(run, 1).unwrap();
        assert_eq!(e.hover, Some(wall));
        assert_eq!(s.model().element(wall).unwrap().number, "AW-002");
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
        let p = &s.model().run(run).unwrap().points;
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
        // Die Wand behält beim Ziehen ihre Kennung
        assert_eq!(s.model().wall_at(run, 1), Some(wall));
        assert!(s.undo());
        assert!((s.model().run(run).unwrap().points[1].y - 4000.0).abs() < 1e-6);
    }

    #[test]
    fn lange_wand_hinter_der_kamera_bleibt_greifbar() {
        let mut s = Scene::new();
        let run = s
            .add_wall(&WallChain {
                points: vec![vec3(0.0, -20000.0, 0.0), vec3(0.0, 20000.0, 0.0)],
                closed: false,
                ref_side: RefSide::Left,
                layers: Vec::new(),
                height: 3500.0,
            })
            .unwrap();
        // Kamera steht neben der Wand und blickt an ihr entlang: Der Anfang liegt hinter ihr
        let c = Camera::looking_at(vec3(2000.0, 0.0, 1600.0), vec3(0.0, 6000.0, 0.0), 45.0);
        let (a, b) = s.foot(run).unwrap()[0];
        let p = a + (b - a) * 0.6;
        assert!(c.project(a, W, H).is_none());
        let (x, y) = at(&c, p);
        let mut e = WallEdit::default();
        let mv = Event::MouseMove {
            x,
            y,
            mods: Modifiers::default(),
        };
        e.handle(&mv, &mut s, &c, W, H, 1.0, true);
        assert_eq!(e.hover, s.model().wall_at(run, 0));
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

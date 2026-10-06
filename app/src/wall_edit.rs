//! Gummiband: violette Linie am äußeren Wandfuß, sichtbar beim Darüberfahren.
//!
//! Mit der linken Maustaste lässt sich ein Segment des Bandes quer zu seiner
//! Richtung ziehen. Die Wand geht live mit, die Nachbarsegmente behalten ihre
//! Richtung und werden länger oder kürzer. Esc bricht das Ziehen ab.
//!
//! Im Schnitt und in den Ansichten blickt man waagerecht auf das Modell. Dort
//! lassen sich nur die Wände ziehen, die vom Betrachter weg laufen: Ihr Fuß
//! erscheint als Punkt und wird beim Darüberfahren als Kugel gezeigt.

use crate::camera::Camera;
use crate::scene::Scene;
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, RunId, WallChain};
use sk_platform::{Event, Key, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Greifabstand zum Band in Pixeln (bei 96 dpi).
const PICK_PX: f64 = 8.0;
/// Raster beim Ziehen in Millimetern.
const STEP: f64 = 10.0;

struct Drag {
    /// Gezogene Wand und ihr Platz im Wandzug.
    wall: ElementId,
    run: RunId,
    seg: usize,
    start: Vec3,
    normal: Vec3,
    original: WallChain,
}

#[derive(Default)]
pub struct WallEdit {
    /// Wand unter der Maus.
    hover: Option<ElementId>,
    drag: Option<Drag>,
    mouse: Option<(f64, f64)>,
    /// Schnittebene der Ansicht „Schnitt“ (Punkt, Normale zum Betrachter):
    /// Was davor liegt, ist weggeschnitten und verdeckt nichts.
    pub section: Option<(Vec3, Vec3)>,
}

/// Ergebnis eines Ereignisses für die App.
#[derive(Default)]
pub struct EditOutcome {
    pub redraw: bool,
    /// Wände haben sich geändert, das Netz muss neu hochgeladen werden.
    pub changed: bool,
    /// Ereignis gehört dem Band, das Wandwerkzeug bekommt es nicht.
    pub consumed: bool,
    /// Band nur angeklickt, nicht verschoben: diese Wand auswählen.
    pub clicked: Option<ElementId>,
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

/// Parallelansicht von der Seite (Schnitt und Ansichten).
fn side_view(cam: &Camera) -> bool {
    cam.ortho.is_some() && cam.forward().z.abs() < 0.5
}

/// Punkt unter der Maus, an dem das Ziehen gemessen wird: in Parallelansichten
/// auf der Bildebene, perspektivisch auf dem Boden.
fn drag_point(cam: &Camera, x: f64, y: f64, w: f64, h: f64) -> Option<Vec3> {
    if cam.ortho.is_some() {
        Some(cam.ray(x, y, w, h).0)
    } else {
        cam.ground_point(x, y, w, h)
    }
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

    /// Ist der Fuß einer Wand, die von der Seite als Punkt erscheint, zu sehen?
    /// Geprüft wird knapp links und rechts daneben: Liegt dort auf beiden Seiten
    /// eine Wand davor, steckt die Wand hinter der Fassade.
    fn foot_visible(&self, scene: &Scene, cam: &Camera, p: Vec3) -> bool {
        let f = cam.forward();
        [-1.0, 1.0].into_iter().any(|side| {
            let q = p + cam.right() * (side * 20.0) + vec3(0.0, 0.0, 50.0);
            let mut o = q - f * 1e7;
            // Im Schnitt erst an der Schnittebene beginnen
            if let Some((p0, n)) = self.section {
                let fn_ = f.dot(n);
                if fn_.abs() > 1e-9 {
                    o = o + f * ((p0 - o).dot(n) / fn_);
                }
            }
            let tq = (q - o).dot(f);
            tq <= 0.0 || scene.raycast(o, f).is_none_or(|(t, _)| t >= tq - 1.0)
        })
    }

    fn pick(&self, scene: &Scene, cam: &Camera, w: f64, h: f64, scale: f64) -> Option<ElementId> {
        let m = self.mouse?;
        let reach = PICK_PX * scale;
        let (side, f) = (side_view(cam), cam.forward());
        // Abstand im Bild, Tiefe, Segment
        let mut best: Option<(f64, f64, (RunId, usize))> = None;
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
                // Von der Seite: nur Wände, die vom Betrachter weg laufen
                if side && (b - a).normalized().dot(f).abs() < 0.99 {
                    continue;
                }
                let Some((pa, pb)) = project_segment(cam, a, b, w, h) else {
                    continue;
                };
                let d = dist_to_segment(m, pa, pb);
                let depth = ((a + b) * 0.5 - cam.eye).dot(f);
                // Von der Seite liegen Wände hintereinander: die vordere gewinnt
                let better = match best {
                    None => true,
                    Some((bd, bz, _)) if side && (d - bd).abs() < 0.5 => depth < bz,
                    Some((bd, ..)) => d < bd,
                };
                if d < reach && better && (!side || self.foot_visible(scene, cam, (a + b) * 0.5)) {
                    best = Some((d, depth, (run, k)));
                }
            }
        }
        best.and_then(|(_, _, (run, k))| scene.model().wall_at(run, k))
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
        let Some(g) = drag_point(cam, mx, my, w, h) else {
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
                    (drag_point(cam, x, y, w, h), original.segment_normal(seg))
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
                });
                scene.begin("Wand verschieben");
                out.redraw = true;
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if let Some(d) = self.drag.take() {
                    // Ein Verlaufsschritt nur, wenn die Wand wirklich woanders steht
                    if scene
                        .chain(d.run)
                        .is_none_or(|c| c.points == d.original.points)
                    {
                        out.clicked = Some(d.wall);
                    }
                    scene.commit();
                    out.consumed = true;
                    out.redraw = true;
                    self.refresh(scene, cam, w, h, scale, enabled);
                }
            }
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } if self.drag.take().is_some() => {
                scene.rollback();
                out.consumed = true;
                out.changed = true;
                out.redraw = true;
                self.refresh(scene, cam, w, h, scale, enabled);
            }
            _ => {}
        }
        out
    }

    /// Gummiband des Segments unter der Maus bzw. des gezogenen, von der Seite als
    /// Kugel am Wandfuß. `occlude`: hinter Wänden liegende Teile blass (3D);
    /// sonst immer voll sichtbar.
    pub fn helpers(
        &self,
        scene: &Scene,
        cam: &Camera,
        scale: f32,
        occlude: bool,
        theme: &Theme,
    ) -> Vec<Helper> {
        let col = &theme.interact;
        let mut out = Vec::new();
        let lift = |p: Vec3| [p.x as f32, p.y as f32, p.z as f32 + 2.0];
        let line = |a: Vec3, b: Vec3, color, width: f32, dash: f32| Helper {
            a: lift(a),
            b: lift(b),
            color,
            width: width * scale,
            dash: dash * scale,
            occlude,
            round: false,
        };
        let dot = |p: Vec3, color, d: f32| Helper {
            a: lift(p),
            b: lift(p),
            color,
            width: d * scale,
            dash: 0.0,
            occlude,
            round: true,
        };
        let active = self.drag.as_ref().map(|d| d.wall).or(self.hover);

        if side_view(cam) {
            if let Some(d) = &self.drag {
                if let Some(&(a, b)) = d.original.outer_foot().get(d.seg) {
                    out.push(dot((a + b) * 0.5, col.drag_ghost, 8.0));
                }
            }
            let segment = active.and_then(|e| scene.model().segment_of(e));
            if let Some((run, k)) = segment {
                if let Some((a, b)) = scene.foot(run).and_then(|f| f.get(k).copied()) {
                    out.push(dot((a + b) * 0.5, col.shadow_band, 15.0));
                    out.push(dot((a + b) * 0.5, col.drag_hot, 12.0));
                }
            }
            return out;
        }

        // Beim Ziehen: ursprüngliche Lage gestrichelt
        if let Some(d) = &self.drag {
            if let Some(&(a, b)) = d.original.outer_foot().get(d.seg) {
                out.push(line(a, b, col.drag_ghost, 1.5, 6.0));
            }
        }
        // Sichtbar nur das Segment unter der Maus bzw. das gezogene
        let segment = active.and_then(|e| scene.model().segment_of(e));
        if let Some((run, k)) = segment {
            if let Some((a, b)) = scene.foot(run).and_then(|f| f.get(k).copied()) {
                out.push(line(a, b, col.shadow_band, 6.0, 0.0));
                out.push(line(a, b, col.drag_hot, 4.0, 0.0));
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
    use std::f64::consts::FRAC_PI_2;

    const W: f64 = 1200.0;
    const H: f64 = 800.0;

    fn setup() -> (Scene, Camera) {
        let mut s = Scene::new();
        s.add_wall(&WallChain {
            base: 0.0,
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
            joints: Default::default(),
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

    /// Geht das Loslassen verloren (Fenster verliert die Maus beim Ziehen, z. B.
    /// Alt+Tab), beginnt das nächste Drücken einen neuen Schritt. Der erste Zug
    /// bleibt rückgängig zu machen und nichts bricht ab.
    #[test]
    fn drücken_ohne_loslassen_behält_den_ersten_schritt() {
        let (mut s, c) = setup();
        let mut e = WallEdit::default();
        let m = Modifiers::default();
        let run = s.model().runs().ids().next().unwrap();
        let drag = |s: &mut Scene, e: &mut WallEdit, from: f64, to: f64| {
            let (x, y) = at(&c, vec3(2500.0, from, 0.0));
            e.handle(&Event::MouseMove { x, y, mods: m }, s, &c, W, H, 1.0, true);
            let down = Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods: m,
            };
            assert!(e.handle(&down, s, &c, W, H, 1.0, true).consumed);
            let (x, y) = at(&c, vec3(2500.0, to, 0.0));
            e.handle(&Event::MouseMove { x, y, mods: m }, s, &c, W, H, 1.0, true);
            (x, y)
        };
        let y1 = |s: &Scene| s.model().run(run).unwrap().points[1].y;
        drag(&mut s, &mut e, 4000.0, 5000.0);
        assert!((y1(&s) - 5000.0).abs() < 1e-6);
        // kein MouseUp; zweiter Zug
        let (x, y) = drag(&mut s, &mut e, 5000.0, 6000.0);
        let up = Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods: m,
        };
        e.handle(&up, &mut s, &c, W, H, 1.0, true);
        assert!((y1(&s) - 6000.0).abs() < 1e-6);
        assert!(s.undo());
        assert!((y1(&s) - 5000.0).abs() < 1e-6, "{}", y1(&s));
        assert!(s.undo());
        assert!((y1(&s) - 4000.0).abs() < 1e-6, "{}", y1(&s));
    }

    #[test]
    fn von_vorne_seitenwand_als_punkt_ziehen() {
        let (mut s, _) = setup();
        let run = s.model().runs().ids().next().unwrap();
        // Ansicht von vorne: Blick nach +y, Parallelprojektion
        let c = Camera::parallel(vec3(2500.0, 2000.0, 1750.0), FRAC_PI_2, 0.0, 6000.0);
        let m = Modifiers::default();
        let mv = |x, y| Event::MouseMove { x, y, mods: m };
        let mut e = WallEdit::default();
        // Die Vorderwand (Segment 3, quer zum Blick) ist nicht greifbar
        let (x, y) = at(&c, vec3(2500.0, 0.0, 0.0));
        e.handle(&mv(x, y), &mut s, &c, W, H, 1.0, true);
        assert_eq!(e.hover, None);
        // Linke Seitenwand (Segment 0): ihr Fuß ist ein Punkt im Bild
        let (a, b) = s.foot(run).unwrap()[0];
        let (xa, ya) = at(&c, a);
        let (xb, yb) = at(&c, b);
        assert!((xa - xb).abs() < 1e-6 && (ya - yb).abs() < 1e-6);
        e.handle(&mv(xa + 3.0, ya - 2.0), &mut s, &c, W, H, 1.0, true);
        assert_eq!(e.hover, s.model().wall_at(run, 0));
        let h = e.helpers(&s, &c, 1.0, false, &Theme::dark());
        assert!(h.iter().all(|h| h.round && h.a == h.b), "Kugel statt Linie");
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x: xa,
            y: ya,
            mods: m,
        };
        assert!(e.handle(&down, &mut s, &c, W, H, 1.0, true).consumed);
        // 1 m nach links: das Gebäude wird 1 m breiter
        let (x, y) = at(&c, a + vec3(-1000.0, 0.0, 0.0));
        assert!(e.handle(&mv(x, y), &mut s, &c, W, H, 1.0, true).changed);
        let up = Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods: m,
        };
        e.handle(&up, &mut s, &c, W, H, 1.0, true);
        let p = &s.model().run(run).unwrap().points;
        assert!(
            (p[0].x + 1000.0).abs() < 1e-6 && (p[1].x + 1000.0).abs() < 1e-6,
            "{p:?}"
        );
        assert!((p[2].x - 5000.0).abs() < 1e-6);
        assert!(s.model().check().is_empty());
    }

    #[test]
    fn verdeckte_seitenwand_ist_nicht_greifbar() {
        // L-förmiger Zug: Die Wand bei x = 3000 (Segment 2) liegt von vorne
        // gesehen hinter der langen Vorderwand
        let mut s = Scene::new();
        let run = s
            .add_wall(&WallChain {
                base: 0.0,
                points: vec![
                    vec3(0.0, 0.0, 0.0),
                    vec3(0.0, 6000.0, 0.0),
                    vec3(3000.0, 6000.0, 0.0),
                    vec3(3000.0, 3000.0, 0.0),
                    vec3(6000.0, 3000.0, 0.0),
                    vec3(6000.0, 0.0, 0.0),
                ],
                closed: true,
                ref_side: RefSide::Left,
                layers: Vec::new(),
                height: 3500.0,
                joints: Default::default(),
            })
            .unwrap();
        let c = Camera::parallel(vec3(3000.0, 3000.0, 1750.0), FRAC_PI_2, 0.0, 6000.0);
        let m = Modifiers::default();
        let mut e = WallEdit::default();
        let hover_at = |e: &mut WallEdit, s: &mut Scene, k: usize| {
            let (a, b) = s.foot(run).unwrap()[k];
            let (x, y) = at(&c, (a + b) * 0.5);
            e.handle(&Event::MouseMove { x, y, mods: m }, s, &c, W, H, 1.0, true);
            e.hover
        };
        assert_eq!(hover_at(&mut e, &mut s, 2), None);
        // Die linke Außenwand ist sichtbar
        assert_eq!(hover_at(&mut e, &mut s, 0), s.model().wall_at(run, 0));
        // Im Schnitt hinter dem Knick (y = 4000) liegt die Wand frei
        e.section = Some((vec3(0.0, 4000.0, 0.0), vec3(0.0, -1.0, 0.0)));
        assert_eq!(hover_at(&mut e, &mut s, 2), s.model().wall_at(run, 2));
    }

    #[test]
    fn lange_wand_hinter_der_kamera_bleibt_greifbar() {
        let mut s = Scene::new();
        let run = s
            .add_wall(&WallChain {
                base: 0.0,
                points: vec![vec3(0.0, -20000.0, 0.0), vec3(0.0, 20000.0, 0.0)],
                closed: false,
                ref_side: RefSide::Left,
                layers: Vec::new(),
                height: 3500.0,
                joints: Default::default(),
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

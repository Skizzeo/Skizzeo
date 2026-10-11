//! Werkzeug „Wand teilen“ (W-12, Jörn 11.10. 06:36): Eine durchgehende
//! Wand bekommt einen oder zwei Teilungspunkte; danach lässt sich das
//! Teilstück am Gummiband parallel rein- oder rausziehen (Eck- bzw.
//! Rechteckrück- oder -vorsprung).
//!
//! Bedienung wie beim Zeichnen und Ziehen: Die Maus zeigt den Punkt auf der
//! Wand (10-mm-Raster ab dem näheren Ende), ein Klick setzt ihn, eine Zahl
//! mit Enter setzt ihn genau ab dem Ende, dem die Maus näher ist. Der zweite
//! Punkt teilt ein Mittelstück ab und schließt; Enter ohne Zahl teilt nach
//! dem ersten. Esc schließt erst die Eingabe, dann das Werkzeug. Die
//! Entscheidungen stehen hier ohne Fenster, die Verdrahtung in
//! [`crate::wall_edit`] und [`crate::kette_app`].

use crate::camera::Camera;
use crate::measure_input::{opens, InputOutcome, MeasureInput};
use crate::scene::Scene;
use crate::ui::MeasureKind;
use sk_math::Vec3;
use sk_model::fuge::MIN_PIECE;
use sk_model::{ElementId, FugeError};
use sk_platform::{Event, Key, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Karte, solange das Werkzeug läuft.
pub const CARD: [&str; 3] = [
    "Teilungspunkt setzen",
    "Klick auf die Wand setzt ihn, Zahl + Enter genau ab dem nahen Ende.",
    "Ein zweiter Punkt teilt ein Mittelstück ab; Enter teilt am ersten, Esc bricht ab.",
];

/// Statuszeile nach Esc.
pub const CANCELLED: &str = "Teilen abgebrochen.";

/// Raster der Maus (mm).
const STEP: f64 = 10.0;

/// Was ein Ereignis im Werkzeug bewirkt hat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub redraw: bool,
    /// Geteilt (ein Schritt): das Teilstück, das gewählt wird.
    pub done: Option<ElementId>,
    /// Abgebrochen (Esc).
    pub cancelled: bool,
    /// Punkt abgelehnt, mit Grund.
    pub refused: Option<FugeError>,
    /// Gesperrt: das sperrende Bauteil und die Wand (Review 61-3).
    pub locked: Option<(ElementId, ElementId)>,
}

/// Laufendes Teilen an einer Wand.
#[derive(Clone, Debug)]
pub struct Teilen {
    pub wall: ElementId,
    /// Bezugslinie: Anfang, Richtung, Länge, Normale, Lage der Wand quer
    /// dazu (kleiner, größer) und Höhe des Fußes.
    p0: Vec3,
    dir: Vec3,
    len: f64,
    normal: Vec3,
    span: (f64, f64),
    z: f64,
    /// Gesetzte Punkte (Abstand ab `p0`).
    pub points: Vec<f64>,
    /// Punkt unter der Maus (Abstand ab `p0`).
    pub hover: Option<f64>,
    pub input: Option<MeasureInput>,
}

impl Teilen {
    /// Beginnt an der Wand `wall`; `None`, wenn sie kein Wandsegment ist.
    pub fn start(scene: &Scene, wall: ElementId) -> Option<Teilen> {
        let (run, seg) = scene.model().segment_of(wall)?;
        let c = scene.chain(run)?;
        let pts = c.clean_points();
        let n = pts.len();
        let (p0, p1) = (*pts.get(seg)?, *pts.get((seg + 1) % n)?);
        let len = (p1 - p0).length();
        if len < 2.0 * MIN_PIECE {
            return None;
        }
        let dir = (p1 - p0) * (1.0 / len);
        let t = c.thickness();
        let span = match c.ref_side {
            sk_model::RefSide::Left => (0.0, t),
            sk_model::RefSide::Right => (-t, 0.0),
            sk_model::RefSide::Center => (-t * 0.5, t * 0.5),
        };
        Some(Teilen {
            wall,
            p0,
            dir,
            len,
            normal: c.segment_normal(seg)?,
            span,
            z: c.base,
            points: Vec::new(),
            hover: None,
            input: None,
        })
    }

    /// Darf an `t` ein Punkt stehen (Stücke mindestens [`MIN_PIECE`])?
    fn fits(&self, t: f64) -> bool {
        let mut all: Vec<f64> = self.points.iter().copied().chain([t]).collect();
        all.sort_by(f64::total_cmp);
        let mut last = 0.0;
        for x in all.into_iter().chain([self.len]) {
            if x - last < MIN_PIECE - 1e-6 {
                return false;
            }
            last = x;
        }
        true
    }

    /// Steht die Maus näher am Ende als am Anfang?
    fn near_end(&self) -> bool {
        self.hover.is_some_and(|t| t > self.len * 0.5)
    }

    /// Punkt unter der Maus: auf die Bezugslinie gelotet, im Raster ab dem
    /// näheren Ende, innerhalb der Wand.
    fn aim(&mut self, cam: &Camera, x: f64, y: f64, w: f64, h: f64) -> bool {
        let g = if cam.ortho.is_some() {
            Some(cam.ray(x, y, w, h).0)
        } else {
            cam.plane_point(x, y, w, h, self.z)
        };
        let t = g.map(|g| {
            let t = (g - self.p0).dot(self.dir).clamp(0.0, self.len);
            if t <= self.len * 0.5 {
                (t / STEP).round() * STEP
            } else {
                self.len - ((self.len - t) / STEP).round() * STEP
            }
        });
        let t = t.map(|t| t.clamp(MIN_PIECE, self.len - MIN_PIECE));
        let changed = t != self.hover;
        self.hover = t;
        changed
    }

    /// Setzt einen Punkt; beim zweiten wird geteilt.
    fn place(&mut self, scene: &mut Scene, t: f64, out: &mut Outcome) {
        if !self.fits(t) {
            out.refused = Some(FugeError::TooShort);
            return;
        }
        self.points.push(t);
        if self.points.len() >= 2 {
            self.finish(scene, out);
        }
    }

    /// Teilt an den gesetzten Punkten (ein Schritt „Wand geteilt“).
    fn finish(&mut self, scene: &mut Scene, out: &mut Outcome) {
        if self.points.is_empty() {
            return;
        }
        let mut pts = self.points.clone();
        pts.sort_by(f64::total_cmp);
        // Gewählt wird das Teilstück: das mittlere bzw. das kürzere
        let mid = match pts[..] {
            [a, b, ..] => (a + b) * 0.5,
            [a] if a < self.len - a => a * 0.5,
            [a] => (a + self.len) * 0.5,
            [] => return,
        };
        let at = self.p0 + self.dir * mid;
        let wall = self.wall;
        // Geteilt wird der ganze Stapel: eine Sperre darin hält es auf
        let m = scene.model();
        if let Some(id) = sk_model::edit_blocked(m, &m.stack_column(wall)) {
            out.locked = Some((id, wall));
            self.points.clear();
            return;
        }
        let mut res = Ok(Vec::new());
        scene.edit_model("Wand geteilt", |m| {
            res = m.split_wall(wall, &pts);
            res.is_ok()
        });
        match res {
            Ok(_) => out.done = Some(piece_at(scene, wall, at).unwrap_or(wall)),
            Err(e) => {
                out.refused = Some(e);
                self.points.clear();
            }
        }
    }

    /// Verarbeitet ein Ereignis der Ansicht (Koordinaten relativ zur
    /// Ansicht). Das Werkzeug nimmt Linksklicks und Tasten.
    pub fn handle(
        &mut self,
        e: &Event,
        scene: &mut Scene,
        cam: &Camera,
        w: f64,
        h: f64,
    ) -> (Outcome, bool) {
        let mut out = Outcome::default();
        let consumed = match *e {
            Event::MouseMove { x, y, .. } => {
                out.redraw = self.aim(cam, x, y, w, h);
                false
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.aim(cam, x, y, w, h);
                if let Some(t) = self.hover {
                    self.input = None;
                    self.place(scene, t, &mut out);
                }
                out.redraw = true;
                true
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => true,
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } => {
                out.redraw = true;
                match self.input.as_mut() {
                    None => match key {
                        Key::Char(ch) if opens(ch, MeasureKind::Length, mods) => {
                            let mut i = MeasureInput::new(MeasureKind::Length, None);
                            i.push(ch);
                            self.input = Some(i);
                            true
                        }
                        Key::Enter => {
                            self.finish(scene, &mut out);
                            true
                        }
                        Key::Escape => {
                            out.cancelled = true;
                            true
                        }
                        // Zurücktaste nimmt den gesetzten Punkt zurück
                        Key::Backspace => {
                            self.points.pop();
                            true
                        }
                        _ => false,
                    },
                    Some(i) => match i.key(key, mods) {
                        InputOutcome::Ignored => false,
                        InputOutcome::Changed | InputOutcome::Refused => true,
                        InputOutcome::Emptied | InputOutcome::Escape | InputOutcome::EnterEmpty => {
                            self.input = None;
                            true
                        }
                        InputOutcome::Enter => {
                            if let Some(Ok(v)) = i.value(0) {
                                self.input = None;
                                let t = if self.near_end() { self.len - v } else { v };
                                if t > 0.0 && t < self.len {
                                    self.place(scene, t, &mut out);
                                } else {
                                    out.refused = Some(FugeError::TooShort);
                                }
                            }
                            true
                        }
                    },
                }
            }
            _ => false,
        };
        (out, consumed)
    }

    /// Pille an der Maus: der Abstand ab dem nahen Ende und der Rest; beim
    /// Tippen das Feld „Abstand“.
    pub fn pill(&self) -> Option<(String, bool)> {
        if let Some(i) = &self.input {
            return Some((i.text([LABEL, ""]), true));
        }
        let t = self.hover?;
        let (a, b) = if self.near_end() {
            (self.len - t, t)
        } else {
            (t, self.len - t)
        };
        Some((
            format!(
                "{LABEL} {} · Rest {}",
                crate::ui::m_text(a),
                crate::ui::m_text(b)
            ),
            false,
        ))
    }

    /// Hilfslinien: die Wand am Fuß, die gesetzten Punkte und der Punkt
    /// unter der Maus als Querstrich über die Wanddicke.
    pub fn helpers(&self, scale: f32, occlude: bool, theme: &Theme) -> Vec<Helper> {
        let col = &theme.interact;
        let lift = |p: Vec3| [p.x as f32, p.y as f32, (self.z + 2.0) as f32];
        let line = |a: Vec3, b: Vec3, color, width: f32| Helper {
            a: lift(a),
            b: lift(b),
            color,
            width: width * scale,
            dash: 0.0,
            pattern: sk_render::SOLID,
            occlude,
            round: false,
        };
        let face = |t: f64, off: f64| self.p0 + self.dir * t + self.normal * off;
        let (lo, hi) = self.span;
        let mut out = vec![
            line(face(0.0, lo), face(self.len, lo), col.shadow_band, 6.0),
            line(face(0.0, lo), face(self.len, lo), col.drag_hot, 4.0),
        ];
        let tick = |t: f64, w: f32| {
            let reach = 150.0;
            [
                line(
                    face(t, lo - reach),
                    face(t, hi + reach),
                    col.shadow_band,
                    w + 2.0,
                ),
                line(face(t, lo - reach), face(t, hi + reach), col.drag_hot, w),
            ]
        };
        for t in &self.points {
            out.extend(tick(*t, 3.0));
        }
        let typed = self
            .input
            .as_ref()
            .and_then(|i| i.value(0))
            .and_then(|v| v.ok())
            .map(|v| if self.near_end() { self.len - v } else { v });
        if let Some(t) = typed.or(self.hover).filter(|t| *t > 0.0 && *t < self.len) {
            out.extend(tick(t, 2.0));
        }
        out
    }
}

/// Feldname der Pille.
pub const LABEL: &str = "Abstand";

/// Wand im Zug von `wall`, deren Bezugslinie den Punkt `at` trägt (nach
/// dem Teilen).
fn piece_at(scene: &Scene, wall: ElementId, at: Vec3) -> Option<ElementId> {
    let m = scene.model();
    let (run, _) = m.segment_of(wall)?;
    let c = m.chain(run)?;
    let pts = c.clean_points();
    let n = pts.len();
    (0..c.segment_count())
        .min_by(|&a, &b| {
            let d = |k: usize| {
                let (a, b) = (pts[k], pts[(k + 1) % n]);
                sk_math::dist_to_segment((at.x, at.y), (a.x, a.y), (b.x, b.y))
            };
            d(a).total_cmp(&d(b))
        })
        .and_then(|k| m.wall_at(run, k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    fn haus() -> Scene {
        let mut s = Scene::new();
        s.edit_model("Gebäude erstellt", |m| {
            let b = m.add_building(2);
            m.build_from_polygon(
                b,
                &[
                    vec3(0.0, 0.0, 0.0),
                    vec3(0.0, 8000.0, 0.0),
                    vec3(10000.0, 8000.0, 0.0),
                    vec3(10000.0, 0.0, 0.0),
                ],
            )
            .is_some()
        });
        s
    }

    fn nordwand(s: &Scene) -> ElementId {
        let m = s.model();
        let b = m.buildings().ids().next().unwrap();
        let eg = m.ground_of(Some(b)).unwrap();
        let run = m
            .runs()
            .iter()
            .find(|(_, r)| r.storey == eg)
            .map(|(id, _)| id)
            .unwrap();
        m.wall_at(run, 1).unwrap()
    }

    fn taste(t: &mut Teilen, s: &mut Scene, cam: &Camera, key: Key) -> Outcome {
        let e = Event::Key {
            key,
            down: true,
            mods: Default::default(),
            repeat: false,
        };
        t.handle(&e, s, cam, 800.0, 600.0).0
    }

    /// Zwei Punkte getippt (2,00 m ab links, dann 3,00 m ab rechts, weil
    /// die Maus rechts steht): ein Schritt, das Mittelstück ist gewählt.
    #[test]
    fn zwei_getippte_punkte_teilen_ein_mittelstueck_ab() {
        let mut s = haus();
        let cam = Camera::looking_at(
            vec3(5000.0, 4000.0, 30000.0),
            vec3(5000.0, 4000.0, 0.0),
            45.0,
        );
        let w = nordwand(&s);
        let mut t = Teilen::start(&s, w).unwrap();
        t.hover = Some(1000.0);
        for k in [Key::Char('2'), Key::Enter] {
            taste(&mut t, &mut s, &cam, k);
        }
        assert_eq!(t.points, vec![2000.0]);
        t.hover = Some(9000.0);
        assert_eq!(t.pill().unwrap().0, "Abstand 1,00 · Rest 9,00");
        let mut out = Outcome::default();
        for k in [Key::Char('3'), Key::Enter] {
            out = taste(&mut t, &mut s, &cam, k);
        }
        let mitte = out.done.expect("geteilt");
        let m = s.model();
        let (run, k) = m.segment_of(mitte).unwrap();
        assert_eq!(m.run(run).unwrap().segments.len(), 6);
        assert_eq!(k, 2);
        let p = &m.run(run).unwrap().points;
        assert_eq!((p[2].x, p[3].x), (2000.0, 7000.0));
        assert_eq!(s.undo_label(), Some("Wand geteilt"));
    }

    /// Review 61-3: Ist die OG-Wand darüber gesperrt, teilt das EG nicht;
    /// die Sperre wird gemeldet, nichts ändert sich.
    #[test]
    fn gesperrt_im_stapel_teilt_nicht() {
        let mut s = haus();
        let cam = Camera::looking_at(
            vec3(5000.0, 4000.0, 30000.0),
            vec3(5000.0, 4000.0, 0.0),
            45.0,
        );
        let w = nordwand(&s);
        let og = s.model().wall_above(w).unwrap();
        s.edit_model("Gesperrt", |m| {
            m.set_locked(&[og], true);
            true
        });
        let (run, _) = s.model().segment_of(w).unwrap();
        let mut t = Teilen::start(&s, w).unwrap();
        t.hover = Some(1000.0);
        let mut out = Outcome::default();
        for k in [Key::Char('2'), Key::Enter, Key::Enter] {
            out = taste(&mut t, &mut s, &cam, k);
        }
        assert_eq!(out.done, None);
        assert_eq!(out.locked.map(|l| l.1), Some(w));
        assert_eq!(s.model().run(run).unwrap().segments.len(), 4);
    }

    /// Zu nah am Ende: abgelehnt, nichts gesetzt; Enter mit einem Punkt
    /// teilt, gewählt ist das kürzere Stück.
    #[test]
    fn ein_punkt_und_ablehnung() {
        let mut s = haus();
        let cam = Camera::looking_at(
            vec3(5000.0, 4000.0, 30000.0),
            vec3(5000.0, 4000.0, 0.0),
            45.0,
        );
        let w = nordwand(&s);
        let mut t = Teilen::start(&s, w).unwrap();
        t.hover = Some(9000.0);
        let mut out = Outcome::default();
        for k in [
            Key::Char('0'),
            Key::Char(','),
            Key::Char('0'),
            Key::Char('5'),
            Key::Enter,
        ] {
            out = taste(&mut t, &mut s, &cam, k);
        }
        assert_eq!(out.refused, Some(FugeError::TooShort));
        assert!(t.points.is_empty());
        for k in [Key::Char('2'), Key::Enter] {
            taste(&mut t, &mut s, &cam, k);
        }
        let out = taste(&mut t, &mut s, &cam, Key::Enter);
        let stueck = out.done.unwrap();
        let m = s.model();
        let (run, k) = m.segment_of(stueck).unwrap();
        assert_eq!(k, 2, "das 2-m-Stück an der Ecke rechts");
        assert_eq!(m.run(run).unwrap().points[2].x, 8000.0);
        let e = taste(&mut t, &mut s, &cam, Key::Escape);
        assert!(e.cancelled);
    }
}

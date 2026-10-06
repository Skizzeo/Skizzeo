//! Werkzeug „Gebäude“: Außenwand als Polygonzug auf dem Boden zeichnen, die Wand
//! wächst live mit. Eingeschaltet über den Knopf „Gebäude“.
//!
//! - Linksklick setzt Punkte. Klick auf den Startpunkt schließt den Zug.
//! - Klick auf den letzten Punkt (Doppelklick) oder Enter beendet einen offenen Zug.
//! - Tab wechselt die Bezugsseite: links (Standard, bei Uhrzeigersinn außen), rechts, Mitte.
//! - R schaltet den 90°-Sprung aus/ein (Standard: ein), Umschalt gedrückt halten kehrt ihn kurz um.
//! - Spurlinien durch den Startpunkt (parallel und senkrecht zur ersten Wand sowie
//!   entlang der Achsen) fangen den letzten Punkt rechtwinklig zum Anfang.
//! - Rücktaste nimmt den letzten Punkt zurück, Esc bricht ab.

use crate::camera::Camera;
use sk_math::{vec3, Vec3};
use sk_model::{Category, Layer, RefSide, WallChain};
use sk_platform::{Event, Key, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

pub const WALL_HEIGHT: f64 = 3500.0;

/// Fangradius in Pixeln (bei 96 dpi).
const SNAP_PX: f64 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum SnapKind {
    Free,
    /// Auf dem Startpunkt: schließt den Zug.
    Start,
    /// Auf einer Spurlinie oder Richtung.
    Line,
    /// Schnitt zweier Spurlinien.
    Crossing,
}

#[derive(Clone, Copy, Debug)]
struct Line {
    origin: Vec3,
    dir: Vec3,
    from_start: bool,
}

#[derive(Clone, Debug)]
struct Cursor {
    pos: Vec3,
    kind: SnapKind,
    guides: Vec<Line>,
}

pub struct WallTool {
    /// Werkzeug eingeschaltet (Knopf „Gebäude“).
    pub enabled: bool,
    points: Vec<Vec3>,
    cursor: Option<Cursor>,
    pub ref_side: RefSide,
    pub ortho: bool,
    shift: bool,
    mouse: Option<(f64, f64)>,
    /// Schichten des Aufbaus für die Vorschau (aus der Bibliothek des Modells).
    pub layers: Vec<Layer>,
    /// Außenwand (Knopf „Gebäude“) oder Innenwand (Knopf „Innenwand“).
    pub category: Category,
}

/// Ergebnis eines Ereignisses für die App.
#[derive(Default)]
pub struct Outcome {
    pub redraw: bool,
    pub commit: Option<WallChain>,
}

fn perp(d: Vec3) -> Vec3 {
    vec3(-d.y, d.x, 0.0)
}

fn cross2(a: Vec3, b: Vec3) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Richtungen ohne Dubletten (auch gegenläufige gelten als gleich).
fn push_dir(dirs: &mut Vec<Vec3>, d: Vec3) {
    let d = vec3(d.x, d.y, 0.0).normalized();
    if d.length() < 0.5 {
        return;
    }
    if dirs.iter().all(|e| cross2(*e, d).abs() > 1e-3) {
        dirs.push(d);
    }
}

impl WallTool {
    pub fn new() -> WallTool {
        WallTool {
            enabled: false,
            points: Vec::new(),
            cursor: None,
            ref_side: RefSide::Left,
            ortho: true,
            shift: false,
            mouse: None,
            layers: Vec::new(),
            category: Category::ExteriorWall,
        }
    }

    /// Wechselt zwischen Außen- und Innenwand. Eine angefangene Eingabe geht weg;
    /// die Bezugsseite springt auf den Standard der Art (Innenwand: Achse).
    pub fn set_category(&mut self, category: Category, layers: Vec<Layer>) {
        if category != self.category {
            self.points.clear();
            self.cursor = None;
            self.category = category;
            self.ref_side = match category {
                Category::InteriorWall => RefSide::Center,
                _ => RefSide::Left,
            };
        }
        self.layers = layers;
    }

    pub fn is_active(&self) -> bool {
        !self.points.is_empty()
    }

    fn chain(&self, points: Vec<Vec3>, closed: bool) -> WallChain {
        WallChain {
            points,
            closed,
            ref_side: self.ref_side,
            layers: self.layers.clone(),
            height: WALL_HEIGHT,
            joints: Default::default(),
        }
    }

    /// Wand, wie sie gerade am Cursor entsteht.
    pub fn preview(&self) -> Option<WallChain> {
        if !self.enabled {
            return None;
        }
        let c = self.cursor.as_ref()?;
        if self.points.is_empty() {
            return None;
        }
        let mut pts = self.points.clone();
        let closed = c.kind == SnapKind::Start;
        if !closed {
            pts.push(c.pos);
        }
        Some(self.chain(pts, closed))
    }

    /// Spurlinien vom Startpunkt aus.
    fn start_lines(&self) -> Vec<Line> {
        if self.points.len() < 2 {
            return Vec::new();
        }
        let s = self.points[0];
        let mut dirs = Vec::new();
        push_dir(&mut dirs, self.points[1] - s);
        push_dir(&mut dirs, perp(self.points[1] - s));
        push_dir(&mut dirs, Vec3::X);
        push_dir(&mut dirs, Vec3::Y);
        dirs.into_iter()
            .map(|dir| Line {
                origin: s,
                dir,
                from_start: true,
            })
            .collect()
    }

    /// Richtungen vom letzten Punkt aus: entlang und rechtwinklig zur letzten Wand,
    /// beim ersten Segment entlang der Achsen.
    fn last_lines(&self) -> Vec<Line> {
        let Some(&last) = self.points.last() else {
            return Vec::new();
        };
        let mut dirs = Vec::new();
        if self.points.len() >= 2 {
            let d = last - self.points[self.points.len() - 2];
            push_dir(&mut dirs, d);
            push_dir(&mut dirs, perp(d));
        } else {
            push_dir(&mut dirs, Vec3::X);
            push_dir(&mut dirs, Vec3::Y);
        }
        dirs.into_iter()
            .map(|dir| Line {
                origin: last,
                dir,
                from_start: false,
            })
            .collect()
    }

    /// Berechnet den gefangenen Cursorpunkt neu (nach Mausbewegung oder Kamerawechsel).
    pub fn refresh(&mut self, cam: &Camera, w: f64, h: f64, scale: f64) {
        self.cursor = if self.enabled {
            self.mouse
                .and_then(|(mx, my)| self.snap(cam, mx, my, w, h, scale))
        } else {
            None
        };
    }

    /// Ein- oder ausschalten; ein halb gezeichneter Zug wird verworfen.
    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        self.points.clear();
        self.cursor = None;
    }

    fn snap(&self, cam: &Camera, mx: f64, my: f64, w: f64, h: f64, scale: f64) -> Option<Cursor> {
        let raw = cam.ground_point(mx, my, w, h)?;
        let thr = SNAP_PX * scale;
        let px = |p: Vec3| -> f64 {
            cam.project(p, w, h).map_or(f64::MAX, |(x, y)| {
                ((x - mx).powi(2) + (y - my).powi(2)).sqrt()
            })
        };
        let free = |pos| Cursor {
            pos,
            kind: SnapKind::Free,
            guides: Vec::new(),
        };

        // 1. Startpunkt schließt den Zug
        if self.points.len() >= 3 && px(self.points[0]) < thr {
            return Some(Cursor {
                pos: self.points[0],
                kind: SnapKind::Start,
                guides: Vec::new(),
            });
        }

        let starts = self.start_lines();
        let ortho = self.ortho != self.shift;
        let mut lasts = self.last_lines();

        // 2. 90°-Sprung: Punkt auf die passendste Richtung vom letzten Punkt zwingen
        if ortho && !lasts.is_empty() {
            let last = lasts[0].origin;
            let v = raw - last;
            let best = *lasts
                .iter()
                .max_by(|a, b| a.dir.dot(v).abs().total_cmp(&b.dir.dot(v).abs()))
                .unwrap();
            lasts = vec![best];
        }

        // 3. Schnitt von Richtung am letzten Punkt mit Spurlinie vom Start
        let mut best: Option<(f64, Cursor)> = None;
        for l in &lasts {
            for s in &starts {
                let c = cross2(l.dir, s.dir);
                if c.abs() < 1e-9 {
                    continue;
                }
                let t = cross2(s.origin - l.origin, s.dir) / c;
                let x = l.origin + l.dir * t;
                let d = px(x);
                if d < thr && best.as_ref().is_none_or(|b| d < b.0) {
                    best = Some((
                        d,
                        Cursor {
                            pos: x,
                            kind: SnapKind::Crossing,
                            guides: vec![*l, *s],
                        },
                    ));
                }
            }
        }
        if let Some((_, c)) = best {
            return Some(c);
        }

        if ortho && !lasts.is_empty() {
            let l = lasts[0];
            let pos = l.origin + l.dir * (raw - l.origin).dot(l.dir);
            return Some(Cursor {
                pos,
                kind: SnapKind::Line,
                guides: vec![l],
            });
        }

        // 4. Einzelne Linie in Reichweite
        let mut best: Option<(f64, Cursor)> = None;
        for l in lasts.iter().chain(starts.iter()) {
            let p = l.origin + l.dir * (raw - l.origin).dot(l.dir);
            let d = px(p);
            if d < thr && best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((
                    d,
                    Cursor {
                        pos: p,
                        kind: SnapKind::Line,
                        guides: vec![*l],
                    },
                ));
            }
        }
        Some(best.map_or_else(|| free(raw), |b| b.1))
    }

    /// Verarbeitet ein Ereignis (Mauskoordinaten relativ zur 3D-Ansicht).
    pub fn handle(&mut self, e: &Event, cam: &Camera, w: f64, h: f64, scale: f64) -> Outcome {
        let mut out = Outcome::default();
        if !self.enabled {
            if let Event::MouseMove { x, y, .. } = *e {
                self.mouse = Some((x, y));
            }
            return out;
        }
        match *e {
            Event::MouseMove { x, y, mods } => {
                self.mouse = Some((x, y));
                self.shift = mods.shift;
                self.refresh(cam, w, h, scale);
                out.redraw = true;
            }
            Event::MouseLeave => {
                self.mouse = None;
                self.cursor = None;
                out.redraw = true;
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            } => {
                self.mouse = Some((x, y));
                self.shift = mods.shift;
                self.refresh(cam, w, h, scale);
                out.redraw = true;
                let Some(c) = self.cursor.clone() else {
                    return out;
                };
                if c.kind == SnapKind::Start {
                    let pts = std::mem::take(&mut self.points);
                    out.commit = Some(self.chain(pts, true));
                } else if let Some(&last) = self.points.last() {
                    let on_last = cam.project(last, w, h).is_some_and(|(lx, ly)| {
                        ((lx - x).powi(2) + (ly - y).powi(2)).sqrt() < 5.0 * scale
                    });
                    if on_last || (c.pos - last).length() < 1.0 {
                        out.commit = self.finish_open();
                    } else {
                        self.points.push(c.pos);
                    }
                } else {
                    self.points.push(c.pos);
                }
                self.refresh(cam, w, h, scale);
            }
            Event::Key {
                key, down, mods, ..
            } => {
                if key == Key::Shift {
                    self.shift = down;
                    self.refresh(cam, w, h, scale);
                    out.redraw = true;
                    return out;
                }
                if !down {
                    return out;
                }
                match key {
                    Key::Tab => {
                        self.ref_side = self.ref_side.next();
                        out.redraw = true;
                    }
                    Key::Char('R') if !mods.ctrl => {
                        self.ortho = !self.ortho;
                        self.refresh(cam, w, h, scale);
                        out.redraw = true;
                    }
                    Key::Escape => {
                        self.points.clear();
                        self.refresh(cam, w, h, scale);
                        out.redraw = true;
                    }
                    Key::Backspace => {
                        self.points.pop();
                        self.refresh(cam, w, h, scale);
                        out.redraw = true;
                    }
                    Key::Enter => {
                        out.commit = self.finish_open();
                        self.refresh(cam, w, h, scale);
                        out.redraw = true;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        out
    }

    fn finish_open(&mut self) -> Option<WallChain> {
        let pts = std::mem::take(&mut self.points);
        let chain = self.chain(pts, false);
        (chain.clean_points().len() >= 2).then_some(chain)
    }

    /// Bezugslinie, Spurlinien und Fangmarken.
    pub fn helpers(&self, cam: &Camera, scale: f32, theme: &Theme) -> Vec<Helper> {
        let col = &theme.interact;
        let mut out = Vec::new();
        if !self.enabled {
            return out;
        }
        let lift = |p: Vec3| [p.x as f32, p.y as f32, p.z as f32 + 1.0];
        let line = |a: Vec3, b: Vec3, color, width: f32, dash: f32| Helper {
            a: lift(a),
            b: lift(b),
            color,
            width: width * scale,
            dash: dash * scale,
            occlude: false,
            round: false,
        };
        let mark = |p: Vec3, color, size: f32| {
            [
                Helper {
                    a: lift(p),
                    b: lift(p),
                    color: col.shadow_tool,
                    width: (size + 3.0) * scale,
                    dash: 0.0,
                    occlude: false,
                    round: false,
                },
                Helper {
                    a: lift(p),
                    b: lift(p),
                    color,
                    width: size * scale,
                    dash: 0.0,
                    occlude: false,
                    round: false,
                },
            ]
        };

        // Bezugslinie des Zuges inklusive Gummiband zum Cursor
        let mut pts = self.points.clone();
        if let Some(c) = &self.cursor {
            if !pts.is_empty() {
                pts.push(c.pos);
            }
        }
        for s in pts.windows(2) {
            out.push(line(s[0], s[1], col.draw, 2.0, 0.0));
        }

        if let Some(c) = &self.cursor {
            // Spurlinien vom Ursprung bis über den Punkt hinaus
            for g in &c.guides {
                let t = (c.pos - g.origin).dot(g.dir);
                let reach = cam.focus.max(1000.0) * 0.15 * t.signum();
                let color = if g.from_start { col.track } else { col.guide };
                out.push(line(
                    g.origin,
                    g.origin + g.dir * (t + reach),
                    color,
                    1.5,
                    6.0,
                ));
            }
        }

        if let Some(&s) = self.points.first() {
            out.extend(mark(s, col.start, 8.0));
        }
        if let Some(c) = &self.cursor {
            let (color, size) = match c.kind {
                SnapKind::Start => (col.start, 12.0),
                SnapKind::Crossing => (col.track, 10.0),
                SnapKind::Line => (col.track, 7.0),
                SnapKind::Free => (col.draw, 6.0),
            };
            out.extend(mark(c.pos, color, size));
        }
        let _ = cam;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_platform::Modifiers;

    fn cam() -> Camera {
        // Senkrecht von oben ist für Tests unhandlich; leicht schräg von oben
        Camera::looking_at(
            vec3(2000.0, -12000.0, 15000.0),
            vec3(2000.0, 2000.0, 0.0),
            45.0,
        )
    }

    fn click_at(t: &mut WallTool, c: &Camera, p: Vec3) -> Outcome {
        let (x, y) = c.project(p, 1200.0, 800.0).unwrap();
        t.handle(
            &Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods: Modifiers::default(),
            },
            c,
            1200.0,
            800.0,
            1.0,
        )
    }

    #[test]
    fn rechteck_schliesst_am_startpunkt() {
        let c = cam();
        let mut t = WallTool::new();
        t.set_enabled(true);
        for p in [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 4000.0, 0.0),
            vec3(5000.0, 4000.0, 0.0),
            vec3(5000.0, 0.0, 0.0),
        ] {
            assert!(click_at(&mut t, &c, p).commit.is_none());
        }
        let out = click_at(&mut t, &c, vec3(0.0, 0.0, 0.0));
        let w = out.commit.expect("geschlossener Zug");
        assert!(w.closed);
        assert_eq!(w.points.len(), 4);
        assert_eq!(w.ref_side, RefSide::Left);
        assert!(!t.is_active());
    }

    #[test]
    fn ausgeschaltet_setzt_keine_punkte() {
        let c = cam();
        let mut t = WallTool::new();
        assert!(click_at(&mut t, &c, vec3(0.0, 0.0, 0.0)).commit.is_none());
        assert!(!t.is_active());
        assert!(t.preview().is_none());
    }

    #[test]
    fn spurlinien_fangen_rechtwinklig_zum_start() {
        let c = cam();
        let mut t = WallTool::new();
        t.set_enabled(true);
        click_at(&mut t, &c, vec3(0.0, 0.0, 0.0));
        click_at(&mut t, &c, vec3(0.0, 4000.0, 0.0));
        click_at(&mut t, &c, vec3(5000.0, 4000.0, 0.0));
        // Etwas neben der Ecke (5000, 0) klicken: Schnitt von „senkrecht zur letzten Wand“
        // und „waagerecht durch den Start“ muss fangen
        click_at(&mut t, &c, vec3(5030.0, 25.0, 0.0));
        let p = *t.points.last().unwrap();
        assert!((p.x - 5000.0).abs() < 1e-6 && p.y.abs() < 1e-6, "{p:?}");
    }

    #[test]
    fn doppelklick_beendet_offenen_zug() {
        let c = cam();
        let mut t = WallTool::new();
        t.set_enabled(true);
        t.ortho = false;
        click_at(&mut t, &c, vec3(0.0, 0.0, 0.0));
        click_at(&mut t, &c, vec3(3000.0, 1000.0, 0.0));
        let out = click_at(&mut t, &c, vec3(3000.0, 1000.0, 0.0));
        let w = out.commit.expect("offener Zug");
        assert!(!w.closed);
        assert_eq!(w.clean_points().len(), 2);
    }

    #[test]
    fn tab_wechselt_bezugsseite() {
        let c = cam();
        let mut t = WallTool::new();
        t.set_enabled(true);
        let tab = Event::Key {
            key: Key::Tab,
            down: true,
            repeat: false,
            mods: Modifiers::default(),
        };
        t.handle(&tab, &c, 1200.0, 800.0, 1.0);
        assert_eq!(t.ref_side, RefSide::Right);
        t.handle(&tab, &c, 1200.0, 800.0, 1.0);
        assert_eq!(t.ref_side, RefSide::Center);
        t.handle(&tab, &c, 1200.0, 800.0, 1.0);
        assert_eq!(t.ref_side, RefSide::Left);
    }
}

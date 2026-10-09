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
//! - Paket 8: Am Gummiband steht seine Länge (Bezugslinie). Eine Ziffer öffnet
//!   die Eingabe „Länge“, Tab das Feld „Winkel“ (gegen die vorige Wand, plus
//!   nach links); Enter setzt den Punkt genau, Esc schließt erst die Eingabe.
//! - E6: Mit [`WallTool::ext`] setzt dieselbe Eingabe Erweiterungsbauteile
//!   (Punkt, Linie, Rechteck, siehe [`crate::ext_werkzeug`]).

use crate::camera::Camera;
use crate::ext_werkzeug::{Art, ExtModus};
use crate::measure_input::{opens, InputOutcome, MeasureInput};
use crate::ui::MeasureKind;
use sk_math::{vec3, Vec3};
use sk_model::erweiterung::ExtPart;
use sk_model::{Category, Layer, RefSide, WallChain};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Vorschauhöhe ohne Geschoss (EG mit OK +2,855).
pub const WALL_HEIGHT: f64 = 2855.0;

/// Fangradius in Pixeln (bei 96 dpi).
const SNAP_PX: f64 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum SnapKind {
    Free,
    /// Auf dem Startpunkt: schließt den Zug.
    Start,
    /// Auf einer Spurlinie oder Richtung.
    Line,
    /// Schnitt zweier Spurlinien (oder einer Richtung mit dem Hintergrund).
    Crossing,
    /// Endpunkt einer Hintergrundkante (E16).
    Point,
    /// Auf einer Hintergrundkante.
    Edge,
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
    /// Arbeitsebene: UK des aktiven Geschosses (mm).
    pub z: f64,
    /// Wandhöhe der Vorschau: Geschosshöhe des aktiven Geschosses (mm).
    pub height: f64,
    /// Fangbare Kanten des Hintergrunds (Geschoss darunter, E16), auf der
    /// Arbeitsebene.
    pub snaps: Vec<(Vec3, Vec3)>,
    /// Getippte Länge und Winkel (Paket 8).
    input: Option<MeasureInput>,
    /// Länge des Gummibands auf dem Bildschirm (dip), für die Pille.
    rubber_dip: f64,
    /// Werkzeug „Erweiterungen“ statt Wand (E6).
    pub ext: Option<ExtModus>,
    /// Gesetzte Erweiterungsbauteile, die die App abholt.
    pub ext_fertig: Vec<ExtPart>,
}

/// Felder der Pille beim Zeichnen.
pub const LABELS: [&str; 2] = ["Länge", "Winkel"];

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
            z: 0.0,
            height: WALL_HEIGHT,
            snaps: Vec::new(),
            input: None,
            rubber_dip: 0.0,
            ext: None,
            ext_fertig: Vec::new(),
        }
    }

    /// Beginnt das Werkzeug „Erweiterungen“ mit `m` (statt Wand).
    pub fn start_ext(&mut self, m: ExtModus) {
        self.set_enabled(true);
        self.ext = Some(m);
    }

    /// Art des laufenden Werkzeugs „Erweiterungen“.
    fn ext_art(&self) -> Option<Art> {
        self.ext.as_ref().map(|m| m.art)
    }

    /// Felder der Pille.
    pub fn labels(&self) -> [&'static str; 2] {
        self.ext.as_ref().map_or(LABELS, |m| m.art.labels())
    }

    /// Wechselt zwischen Außen- und Innenwand. Eine angefangene Eingabe geht weg;
    /// die Bezugsseite springt auf den Standard der Art (Innenwand: Achse).
    pub fn set_category(&mut self, category: Category, layers: Vec<Layer>) {
        if category != self.category {
            self.points.clear();
            self.cursor = None;
            self.input = None;
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

    /// Gesetzte Punkte des Zugs (Bezugslinie), ohne den Cursorpunkt.
    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    /// Offene Maßeingabe.
    pub fn input(&self) -> Option<&MeasureInput> {
        self.input.as_ref()
    }

    /// Richtung der vorigen Wand, bei der ersten die Waagerechte.
    fn base_dir(&self) -> Vec3 {
        match self.points.as_slice() {
            [.., a, b] => {
                let d = vec3(b.x - a.x, b.y - a.y, 0.0);
                if d.length() > 1e-9 {
                    d.normalized()
                } else {
                    Vec3::X
                }
            }
            _ => Vec3::X,
        }
    }

    /// Punkt aus getippter Länge und Winkel; ohne Winkel in Richtung der
    /// (gefangenen) Maus. `None` ohne gültige Länge.
    fn typed_point(&self) -> Option<Vec3> {
        let i = self.input.as_ref()?;
        if i.error.is_some() {
            return None;
        }
        let len = i.value(0)?.ok()?;
        let last = match self.ext.as_ref() {
            Some(m) if m.art == Art::Punkt => m.zuletzt?,
            _ => *self.points.last()?,
        };
        if self.ext_art() == Some(Art::Rechteck) {
            // Breite in x, Tiefe in y, Vorzeichen wie die Maus
            let m = self.cursor.as_ref().map_or(Vec3::ZERO, |c| c.pos - last);
            let sx = if m.x < 0.0 { -1.0 } else { 1.0 };
            let sy = if m.y < 0.0 { -1.0 } else { 1.0 };
            let t = match i.value(1) {
                Some(Ok(t)) => t,
                _ => m.y.abs(),
            };
            return Some(last + vec3(sx * len, sy * t, 0.0));
        }
        let dir = match i.value(1) {
            Some(Ok(a)) => {
                let (s, c) = a.to_radians().sin_cos();
                let b = self.base_dir();
                vec3(b.x * c - b.y * s, b.x * s + b.y * c, 0.0)
            }
            _ => {
                let m = self.cursor.as_ref().map_or(Vec3::ZERO, |c| c.pos - last);
                let m = vec3(m.x, m.y, 0.0);
                if m.length() > 1e-6 {
                    m.normalized()
                } else {
                    self.base_dir()
                }
            }
        };
        Some(last + dir * len)
    }

    /// Schließt der Punkt `p` den Zug (unter 1 mm am Start, ab drei Punkten)?
    fn closes(&self, p: Vec3) -> bool {
        self.ext.is_none() && self.points.len() >= 3 && (p - self.points[0]).length() < 1.0
    }

    /// Ende des Gummibands: getippter Punkt oder Cursor.
    fn rubber_end(&self) -> Option<Vec3> {
        self.typed_point()
            .or_else(|| self.cursor.as_ref().map(|c| c.pos))
    }

    /// Pille am Gummiband: Anfang und Ende des Gummibands und der Text
    /// (Länge der Bezugslinie bzw. die Eingabe). Ohne gesetzten Punkt und
    /// bei einem Gummiband unter 1 dip ohne Eingabe keine Pille.
    pub fn label(&self) -> Option<([Vec3; 2], String)> {
        if !self.enabled {
            return None;
        }
        let last = match self.ext.as_ref() {
            Some(m) if m.art == Art::Punkt && self.input.is_some() => m.zuletzt?,
            _ => *self.points.last()?,
        };
        let end = self.rubber_end()?;
        if let Some(i) = &self.input {
            return Some(([last, end], i.text(self.labels())));
        }
        if self.rubber_dip < 1.0 {
            return None;
        }
        let text = if self.ext_art() == Some(Art::Rechteck) {
            let t = crate::ui::live_length_text;
            format!(
                "{} × {}",
                t((end.x - last.x).abs()),
                t((end.y - last.y).abs())
            )
        } else {
            crate::ui::live_length_text((end - last).length())
        };
        Some(([last, end], text))
    }

    fn chain(&self, points: Vec<Vec3>, closed: bool) -> WallChain {
        WallChain {
            base: self.z,
            points,
            closed,
            ref_side: self.ref_side,
            layers: self.layers.clone(),
            height: self.height,
            joints: Default::default(),
        }
    }

    /// Wand, wie sie gerade am Cursor entsteht.
    pub fn preview(&self) -> Option<WallChain> {
        if !self.enabled || self.ext.is_some() {
            return None;
        }
        let c = self.cursor.as_ref()?;
        if self.points.is_empty() {
            return None;
        }
        let mut pts = self.points.clone();
        let (end, closed) = match self.typed_point() {
            Some(p) => (p, self.closes(p)),
            None => (c.pos, c.kind == SnapKind::Start),
        };
        if !closed {
            pts.push(end);
        }
        Some(self.chain(pts, closed))
    }

    /// Erweiterungsbauteil, wie es gerade am Cursor entsteht (E6).
    pub fn ext_vorschau(&self) -> Option<ExtPart> {
        let m = self.ext.as_ref().filter(|_| self.enabled)?;
        let end = self.rubber_end()?;
        let mut p = self.points.clone();
        p.push(end);
        m.teil(&p)
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
        // Zwei Ecken eines Rechtecks liegen nie auf einer Achse
        if self.ext_art() == Some(Art::Rechteck) {
            return Vec::new();
        }
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
        let ends = self.points.last().copied().zip(self.rubber_end());
        self.rubber_dip = ends
            .and_then(|(a, b)| Some((cam.project(a, w, h)?, cam.project(b, w, h)?)))
            .map_or(0.0, |(a, b)| {
                ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() / scale.max(1e-6)
            });
    }

    /// Arbeitsebene und Vorschauhöhe aus dem aktiven Geschoss. Wechselt die
    /// Ebene, geht ein angefangener Zug weg. `true`, wenn sich etwas ändert.
    pub fn set_plane(&mut self, z: f64, height: f64) -> bool {
        if (z, height) == (self.z, self.height) {
            return false;
        }
        if z != self.z {
            self.points.clear();
            self.cursor = None;
            self.input = None;
        }
        (self.z, self.height) = (z, height);
        true
    }

    /// Ein- oder ausschalten; ein halb gezeichneter Zug wird verworfen.
    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        self.points.clear();
        self.cursor = None;
        self.input = None;
        if !on {
            self.ext = None;
        }
    }

    /// Setzt einen Punkt im Werkzeug „Erweiterungen“; mit dem letzten
    /// Punkt der Art ist das Bauteil fertig (in [`WallTool::ext_fertig`]).
    fn ext_punkt(&mut self, p: Vec3) {
        let Some(m) = self.ext.as_mut() else {
            return;
        };
        self.points.push(p);
        if self.points.len() < m.art.klicks() {
            return;
        }
        let pts = std::mem::take(&mut self.points);
        if let Some(t) = m.teil(&pts) {
            self.ext_fertig.push(t);
            m.zuletzt = Some(p);
        }
    }

    fn snap(&self, cam: &Camera, mx: f64, my: f64, w: f64, h: f64, scale: f64) -> Option<Cursor> {
        let raw = cam.plane_point(mx, my, w, h, self.z)?;
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

        // 2. Endpunkt einer Hintergrundkante
        let near = self
            .snaps
            .iter()
            .flat_map(|&(a, b)| [a, b])
            .map(|p| (px(p), p))
            .filter(|(d, _)| *d < thr)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p)) = near {
            return Some(Cursor {
                pos: p,
                kind: SnapKind::Point,
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
        // Richtung am letzten Punkt trifft eine Hintergrundkante
        for l in &lasts {
            for &(a, b) in &self.snaps {
                let e = b - a;
                let c = cross2(l.dir, e);
                if c.abs() < 1e-9 {
                    continue;
                }
                let u = cross2(a - l.origin, l.dir) / c;
                if !(0.0..=1.0).contains(&u) {
                    continue;
                }
                let x = a + e * u;
                let d = px(x);
                if d < thr && best.as_ref().is_none_or(|b| d < b.0) {
                    best = Some((
                        d,
                        Cursor {
                            pos: x,
                            kind: SnapKind::Crossing,
                            guides: vec![*l],
                        },
                    ));
                }
            }
        }
        if let Some((_, c)) = best {
            return Some(c);
        }

        // Frei (ohne erzwungene Richtung): auf eine Hintergrundkante
        if !ortho || lasts.is_empty() {
            let on_edge = self
                .snaps
                .iter()
                .filter_map(|&(a, b)| {
                    let e = b - a;
                    let len2 = e.dot(e);
                    (len2 > 1e-9).then(|| a + e * ((raw - a).dot(e) / len2).clamp(0.0, 1.0))
                })
                .map(|p| (px(p), p))
                .filter(|(d, _)| *d < thr)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, p)) = on_edge {
                return Some(Cursor {
                    pos: p,
                    kind: SnapKind::Edge,
                    guides: Vec::new(),
                });
            }
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
                // Ein Klick verwirft die getippte Zahl
                self.input = None;
                self.mouse = Some((x, y));
                self.shift = mods.shift;
                self.refresh(cam, w, h, scale);
                out.redraw = true;
                let Some(c) = self.cursor.clone() else {
                    return out;
                };
                if self.ext.is_some() {
                    self.ext_punkt(c.pos);
                    self.refresh(cam, w, h, scale);
                    return out;
                }
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
                if self.input_key(key, mods, &mut out) {
                    self.refresh(cam, w, h, scale);
                    return out;
                }
                match key {
                    Key::Tab if self.ext.is_some() => {
                        // Punkt mit drehen=ja: um 90° weiter
                        if let Some(m) = self
                            .ext
                            .as_mut()
                            .filter(|m| m.art == Art::Punkt && m.def.drehen())
                        {
                            m.rot = (m.rot + 90.0) % 360.0;
                            out.redraw = true;
                        }
                    }
                    Key::Enter if self.ext.is_some() => {}
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
                        self.input = None;
                        self.points.clear();
                        self.refresh(cam, w, h, scale);
                        out.redraw = true;
                    }
                    Key::Backspace => {
                        self.input = None;
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

    /// Taste für die Maßeingabe: öffnet sie mit einer Ziffer (ab dem
    /// ersten Punkt) bzw. gibt sie an die offene Eingabe. `true`, wenn die
    /// Taste verbraucht ist; Buchstaben, Strg und Alt gehen ans Werkzeug.
    fn input_key(&mut self, key: Key, mods: Modifiers, out: &mut Outcome) -> bool {
        let Some(i) = self.input.as_mut() else {
            let Key::Char(ch) = key else {
                return false;
            };
            let bezug = match self.ext.as_ref() {
                Some(m) if m.art == Art::Punkt => m.zuletzt.is_some(),
                _ => !self.points.is_empty(),
            };
            if !bezug || !opens(ch, MeasureKind::Length, mods) {
                return false;
            }
            let zweites = if self.ext_art() == Some(Art::Rechteck) {
                MeasureKind::Length
            } else {
                MeasureKind::Angle
            };
            let mut i = MeasureInput::new(MeasureKind::Length, Some(zweites));
            i.push(ch);
            self.input = Some(i);
            out.redraw = true;
            return true;
        };
        match i.key(key, mods) {
            InputOutcome::Ignored => return false,
            InputOutcome::Changed | InputOutcome::Refused => {}
            InputOutcome::Emptied | InputOutcome::Escape => self.input = None,
            InputOutcome::EnterEmpty => {
                self.input = None;
                if self.ext.is_none() {
                    out.commit = self.finish_open();
                }
            }
            InputOutcome::Enter => {
                if let Some(p) = self.typed_point() {
                    self.input = None;
                    if self.ext.is_some() {
                        self.ext_punkt(p);
                    } else if self.closes(p) {
                        let pts = std::mem::take(&mut self.points);
                        out.commit = Some(self.chain(pts, true));
                    } else {
                        self.points.push(p);
                    }
                }
            }
        }
        out.redraw = true;
        true
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
            pattern: sk_render::SOLID,
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
                    pattern: sk_render::SOLID,
                    occlude: false,
                    round: false,
                },
                Helper {
                    a: lift(p),
                    b: lift(p),
                    color,
                    width: size * scale,
                    dash: 0.0,
                    pattern: sk_render::SOLID,
                    occlude: false,
                    round: false,
                },
            ]
        };

        // Bezugslinie des Zuges inklusive Gummiband zum Cursor
        let mut pts = self.points.clone();
        if let Some(end) = self.rubber_end() {
            if !pts.is_empty() {
                pts.push(end);
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
                SnapKind::Crossing | SnapKind::Point => (col.track, 10.0),
                SnapKind::Edge => (col.track, 7.0),
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
    fn taste(t: &mut WallTool, c: &Camera, key: Key) -> Outcome {
        t.handle(
            &Event::Key {
                key,
                down: true,
                mods: Modifiers::default(),
                repeat: false,
            },
            c,
            1200.0,
            800.0,
            1.0,
        )
    }

    fn ext(i: usize) -> ExtModus {
        const B: [&str; 3] = [
            include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb"),
            include_str!("../../crates/sk-szb/beispiele/werk.stabgelaender.szb"),
            include_str!("../../crates/sk-szb/beispiele/werk.bodenplatte.szb"),
        ];
        ExtModus::new(sk_model::erweiterung::ExtDef::einlesen(B[i]).unwrap())
    }

    /// E6: Punkt, Linie und Rechteck setzen Erweiterungsbauteile; Wände
    /// entstehen dabei keine.
    #[test]
    fn erweiterungen_setzen() {
        let c = cam();
        let mut t = WallTool::new();
        // Punkt: Tab dreht, jeder Klick ein Bauteil, Zahl + Enter im Abstand
        t.start_ext(ext(0));
        taste(&mut t, &c, Key::Tab);
        let out = click_at(&mut t, &c, vec3(1000.0, 1000.0, 0.0));
        assert!(out.commit.is_none() && t.preview().is_none());
        let p = t.ext_fertig.pop().unwrap();
        assert!((p.at[0] - 1000.0).abs() < 1.0 && (p.at[1] - 1000.0).abs() < 1.0);
        assert_eq!(p.rot, 90.0);
        assert!(!t.is_active());
        t.mouse = c.project(vec3(3000.0, 1000.0, 0.0), 1200.0, 800.0);
        t.refresh(&c, 1200.0, 800.0, 1.0);
        for ch in "2,5".chars() {
            taste(&mut t, &c, Key::Char(ch));
        }
        assert_eq!(t.labels(), ["Abstand", "Winkel"]);
        taste(&mut t, &c, Key::Enter);
        let q = t.ext_fertig.pop().unwrap();
        assert!((q.at[0] - p.at[0] - 2500.0).abs() < 1e-6, "{:?}", q.at);
        // Linie: zwei Klicks, Länge aus der Eingabe
        t.start_ext(ext(1));
        click_at(&mut t, &c, vec3(0.0, 0.0, 0.0));
        assert!(t.is_active() && t.ext_fertig.is_empty());
        t.mouse = c.project(vec3(0.0, 1800.0, 0.0), 1200.0, 800.0);
        t.refresh(&c, 1200.0, 800.0, 1.0);
        assert!(t.ext_vorschau().is_some());
        for ch in "3".chars() {
            taste(&mut t, &c, Key::Char(ch));
        }
        taste(&mut t, &c, Key::Enter);
        let l = t.ext_fertig.pop().unwrap();
        assert!((l.rot - 90.0).abs() < 1e-6);
        assert_eq!(l.werte, [("l".to_string(), 3000.0)]);
        // Rechteck: zwei Ecken, Breite und Tiefe getippt
        t.start_ext(ext(2));
        click_at(&mut t, &c, vec3(0.0, 0.0, 0.0));
        t.mouse = c.project(vec3(1000.0, 1000.0, 0.0), 1200.0, 800.0);
        t.refresh(&c, 1200.0, 800.0, 1.0);
        for k in [Key::Char('6'), Key::Tab, Key::Char('4'), Key::Enter] {
            taste(&mut t, &c, k);
        }
        let r = t.ext_fertig.pop().unwrap();
        assert_eq!(
            r.werte,
            [("b".to_string(), 6000.0), ("t".to_string(), 4000.0)]
        );
        // Esc ohne Punkte bleibt im Werkzeug; Ausschalten beendet es
        taste(&mut t, &c, Key::Escape);
        assert!(t.ext.is_some());
        t.set_enabled(false);
        assert!(t.ext.is_none());
    }
}

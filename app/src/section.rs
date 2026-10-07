//! Schnittlinien im Grundriss nach DIN 1356: dünne Strichpunktlinie mit
//! kräftigen Enden, Pfeile in Blickrichtung und Kennbuchstaben. Schnitt A
//! (quer) liegt waagerecht im Grundriss und blickt nach +y, Schnitt B (längs)
//! ist um 90° gedreht und blickt nach +x. Im Grundriss lässt sich jede Linie
//! greifen und quer verschieben, ein Klick auf einen Pfeil spiegelt die
//! Blickrichtung; die Ansicht „Schnitt“ folgt dem aktiven Schnitt.
//!
//! Die Entscheidungen stehen als reine Funktionen oben (Richtung, Ebene,
//! Anfangslage, Spiegeln, Wechsel), damit sie sich ohne Fenster prüfen lassen.

use crate::camera::Camera;
use crate::scene::Scene;
use sk_math::{dist_to_segment, vec3, Vec3};
use sk_model::{Cut, CUT_NAMES};
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{Event, MouseButton};
use sk_render::{DashPattern, Helper, SOLID};
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;

/// Strichmuster der Schnittlinie A–A in Bildpunkten bei 96 dpi, aus dem
/// Linientyp der Darstellung (E4).
#[cfg(test)]
pub fn dash_pattern(model: &sk_model::Model, theme: &Theme) -> DashPattern {
    crate::draw_table::dash_px(
        model,
        model.attr().display().section_line.line_type,
        theme.px_per_mm,
    )
}

/// Greifabstand in Pixeln (bei 96 dpi).
const PICK_PX: f64 = 8.0;
/// Überstand der Linie über das Gebäude (mm).
const OVERHANG: f64 = 1500.0;
/// Raster beim Verschieben (mm).
const STEP: f64 = 10.0;
/// Länge des Pfeils vom Linienende bis zur Spitze (dip).
const ARROW: f32 = 36.0;
/// Halbe Kantenlänge des Bildes eines Endsymbols (dip); der Bezugspunkt
/// liegt in der Mitte.
const MARK_HALF: f32 = 52.0;

/// Kennung von Schnitt A (quer) und B (längs).
pub const CUT_A: usize = 0;
pub const CUT_B: usize = 1;
/// Zahl der Schnitte.
pub const CUTS: usize = CUT_NAMES.len();

// ===== Reine Funktionen =====

/// Blickrichtung des Schnitts `id` (waagerecht, Länge 1): A nach +y, B nach
/// +x, gespiegelt umgekehrt.
pub fn view_dir(id: usize, flip: bool) -> Vec3 {
    let d = if id == CUT_B {
        vec3(1.0, 0.0, 0.0)
    } else {
        vec3(0.0, 1.0, 0.0)
    };
    if flip {
        d * -1.0
    } else {
        d
    }
}

/// Richtung der Linie im Grundriss (A längs x, B längs y).
pub fn line_dir(id: usize) -> Vec3 {
    if id == CUT_B {
        vec3(0.0, 1.0, 0.0)
    } else {
        vec3(1.0, 0.0, 0.0)
    }
}

/// Schnittebene: Punkt und Normale zum Betrachter (gegen die Blickrichtung).
pub fn plane_of(id: usize, cut: Cut) -> Option<(Vec3, Vec3)> {
    let pos = cut.pos?;
    let p = if id == CUT_B {
        vec3(pos, 0.0, 0.0)
    } else {
        vec3(0.0, pos, 0.0)
    };
    Some((p, view_dir(id, cut.flip) * -1.0))
}

/// Anfangslage: mittig durch das Modell, quer zur Linie, im Raster.
pub fn default_pos(id: usize, center: Vec3) -> f64 {
    let c = if id == CUT_B { center.x } else { center.y };
    (c / STEP).round() * STEP
}

/// Blickrichtung umkehren; die Lage bleibt.
pub fn mirror(cut: Cut) -> Cut {
    Cut {
        flip: !cut.flip,
        ..cut
    }
}

/// Reihenfolge der Schnitte am Rad von unten nach oben: A oben, die Spitze
/// unten (Bild↓) geht im Alphabet weiter (A → B).
pub const ORDER: [usize; CUTS] = [CUT_B, CUT_A];

/// Art des Schnitts aus dem Gebäude (E19 §8): Läuft die Linie parallel zur
/// längeren Seite, ist es ein Längsschnitt, sonst ein Querschnitt; bei
/// gleich langen Seiten oder ohne Gebäude keine Angabe.
pub fn kind(id: usize, bounds: Option<(Vec3, Vec3)>) -> Option<&'static str> {
    let (lo, hi) = bounds?;
    let (dx, dy) = (hi.x - lo.x, hi.y - lo.y);
    if (dx - dy).abs() < 1.0 {
        return None;
    }
    // A läuft längs x, B längs y
    let along_longer = if id == CUT_B { dy > dx } else { dx > dy };
    Some(if along_longer {
        "Längsschnitt"
    } else {
        "Querschnitt"
    })
}

/// Unterzeile am Schnittrad: Art und, falls gespiegelt, der Hinweis.
pub fn subtitle(id: usize, flip: bool, bounds: Option<(Vec3, Vec3)>) -> String {
    match (kind(id, bounds), flip) {
        (Some(k), true) => format!("{k} · gespiegelt"),
        (Some(k), false) => k.into(),
        (None, true) => "gespiegelt".into(),
        (None, false) => String::new(),
    }
}

/// Hinweis an der Spitze des Rads, die zum Schnitt `id` führt: „Schnitt
/// B–B ↓“, darunter die Unterzeile wie am Rad.
pub fn arrow_hint(id: usize, up: bool, sub: String) -> (String, String) {
    let arrow = if up { "↑" } else { "↓" };
    (format!("Schnitt {} {arrow}", title(id)), sub)
}

/// Knopf unter dem Schnittrad und sein Hinweis (E19 §2).
pub const MIRROR_LABEL: &str = "⇄ Blickrichtung";
pub const MIRROR_HINT: &str = "Blick umdrehen (in die andere Richtung schauen)";

/// Name am Schnittrad: „A–A“.
pub fn title(id: usize) -> String {
    let n = CUT_NAMES.get(id).copied().unwrap_or("?");
    format!("{n}–{n}")
}

// ===== Linie im Grundriss =====

pub struct SectionLine {
    /// Schnitt A ([`CUT_A`]) oder B ([`CUT_B`]).
    pub id: usize,
    /// Lage der senkrechten Schnittebene quer zur Linie (A: y, B: x, mm).
    pub y: Option<f64>,
    /// Blick gespiegelt.
    pub flip: bool,
    hover: bool,
    /// Maus über einem Pfeil: ein Klick spiegelt.
    hover_mark: bool,
    /// Beim Ziehen: Abstand zwischen Griffpunkt und Linie.
    drag: Option<f64>,
}

impl Default for SectionLine {
    /// Schnitt A.
    fn default() -> SectionLine {
        SectionLine::new(CUT_A)
    }
}

#[derive(Default)]
pub struct SectionOutcome {
    pub redraw: bool,
    /// Lage oder Blickrichtung geändert: Schnitt neu berechnen.
    pub changed: bool,
    pub consumed: bool,
    /// Ein Klick auf den Pfeil hat die Blickrichtung umgekehrt.
    pub mirrored: bool,
    /// Welche Linie sich geändert hat ([`Sections`]).
    pub line: Option<usize>,
}

/// Lage eines Endsymbols im Bild (Pixel der 3D-Ansicht).
pub struct Mark {
    pub x: f64,
    pub y: f64,
    /// Anfang der Linie (A: links, B: unten).
    pub left: bool,
}

/// Bildgröße eines Endsymbols in Pixeln.
fn mark_size(scale: f32) -> f32 {
    (2.0 * MARK_HALF * scale).round()
}

/// Richtung im Bild (Pixel, y nach unten) einer waagerechten Richtung im
/// Grundriss (Blick von oben, +y nach oben).
fn screen(d: Vec3) -> (f32, f32) {
    (d.x as f32, -d.y as f32)
}

impl SectionLine {
    pub fn new(id: usize) -> SectionLine {
        SectionLine {
            id,
            y: None,
            flip: false,
            hover: false,
            hover_mark: false,
            drag: None,
        }
    }

    /// Stand für die Datei.
    pub fn cut(&self) -> Cut {
        Cut {
            pos: self.y,
            flip: self.flip,
        }
    }

    /// Stand aus der Datei übernehmen.
    pub fn set_cut(&mut self, c: Cut) {
        self.y = c.pos;
        self.flip = c.flip;
    }

    /// Setzt die Schnittlinie beim ersten Gebrauch in die Mitte des Modells.
    pub fn ensure(&mut self, scene: &Scene) {
        if self.y.is_none() {
            self.y = scene.center().map(|c| default_pos(self.id, c));
        }
    }

    /// Schnittebene: Punkt und Normale zum Betrachter.
    pub fn plane(&self) -> Option<(Vec3, Vec3)> {
        plane_of(self.id, self.cut())
    }

    /// Blickrichtung umkehren.
    pub fn mirror(&mut self) {
        self.set_cut(mirror(self.cut()));
    }

    pub fn is_busy(&self) -> bool {
        self.hover || self.hover_mark || self.drag.is_some()
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Maus über einem Pfeil (Zeiger „Hand“).
    pub fn over_mark(&self) -> bool {
        self.hover_mark
    }

    /// Anfang und Ende der Linie (Bereich des Modells mit Überstand).
    fn ends(&self, scene: &Scene) -> Option<(Vec3, Vec3)> {
        let y = self.y?;
        let b = self.id == CUT_B;
        let (lo, hi) = scene.bounds().map_or(
            if b {
                (-2000.0, 10000.0)
            } else {
                (-2000.0, 12000.0)
            },
            |(lo, hi)| if b { (lo.y, hi.y) } else { (lo.x, hi.x) },
        );
        Some(if b {
            (vec3(y, lo - OVERHANG, 0.0), vec3(y, hi + OVERHANG, 0.0))
        } else {
            (vec3(lo - OVERHANG, y, 0.0), vec3(hi + OVERHANG, y, 0.0))
        })
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

    /// Liegt die Maus auf einem der beiden Pfeile?
    fn near_mark(
        &self,
        scene: &Scene,
        cam: &Camera,
        m: (f64, f64),
        w: f64,
        h: f64,
        scale: f64,
    ) -> bool {
        let (dx, dy) = screen(view_dir(self.id, self.flip));
        let len = ARROW as f64 * scale;
        self.marks(scene, cam, w, h).iter().any(|k| {
            let tip = (k.x + dx as f64 * len, k.y + dy as f64 * len);
            dist_to_segment(m, (k.x, k.y), tip) < 10.0 * scale
        })
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
            out.redraw = std::mem::take(&mut self.hover) | std::mem::take(&mut self.hover_mark);
            return out;
        }
        let across = |g: Vec3| if self.id == CUT_B { g.x } else { g.y };
        match *e {
            Event::MouseMove { x, y, .. } => {
                if let Some(off) = self.drag {
                    if let Some(g) = cam.ground_point(x, y, w, h) {
                        let ny = ((across(g) - off) / STEP).round() * STEP;
                        if self.y != Some(ny) {
                            self.y = Some(ny);
                            out.changed = true;
                            out.redraw = true;
                        }
                    }
                } else {
                    let mark = self.near_mark(scene, cam, (x, y), w, h, scale);
                    let hover = !mark && self.near(scene, cam, (x, y), w, h, scale);
                    out.redraw = (hover, mark) != (self.hover, self.hover_mark);
                    (self.hover, self.hover_mark) = (hover, mark);
                }
            }
            Event::MouseLeave => {
                if self.drag.is_none() {
                    out.redraw =
                        std::mem::take(&mut self.hover) | std::mem::take(&mut self.hover_mark);
                }
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if self.near_mark(scene, cam, (x, y), w, h, scale) {
                    self.mirror();
                    self.hover_mark = true;
                    out.mirrored = true;
                    out.changed = true;
                    out.consumed = true;
                    out.redraw = true;
                    return out;
                }
                self.hover = self.near(scene, cam, (x, y), w, h, scale);
                if self.hover {
                    if let (Some(g), Some(sy)) = (cam.ground_point(x, y, w, h), self.y) {
                        self.drag = Some(across(g) - sy);
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

    /// Linie als Hilfslinien: Mitte im Linientyp der Schnittlinie (im
    /// Startsatz Strichpunkt), kräftige Enden.
    pub fn helpers(
        &self,
        scene: &Scene,
        cam: &Camera,
        h: f64,
        scale: f32,
        theme: &Theme,
    ) -> Vec<Helper> {
        let Some((a, b)) = self.ends(scene) else {
            return Vec::new();
        };
        let t = scene.table();
        let hot = theme.interact.drag;
        let color = |ink: [f32; 4]| if self.is_busy() { hot } else { ink };
        let mm_per_px = cam.ortho.map_or(10.0, |half| 2.0 * half / h.max(1.0));
        let end = 16.0 * mm_per_px * scale as f64;
        let lift = |p: Vec3| [p.x as f32, p.y as f32, p.z as f32 + 2.0];
        let line = |p: Vec3, q: Vec3, (width, ink): (f32, [f32; 4]), pattern: DashPattern| Helper {
            a: lift(p),
            b: lift(q),
            color: color(ink),
            width: width * scale,
            dash: 0.0,
            pattern: pattern.map(|[l, g, dot, _]| [l * scale, g * scale, dot, 0.0]),
            occlude: false,
            round: false,
        };
        let dx = line_dir(self.id) * end;
        vec![
            line(a + dx, b - dx, t.section_line, t.section_dash),
            line(a, a + dx, t.section_ends, SOLID),
            line(b - dx, b, t.section_ends, SOLID),
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

    /// Bezugspunkt (Linienende) im Bild eines Endsymbols: die Mitte.
    pub fn mark_anchor(&self, _left: bool, scale: f32) -> (f32, f32) {
        let c = mark_size(scale) * 0.5;
        (c, c)
    }

    /// Bild eines Endsymbols (Pfeil in Blickrichtung und Buchstabe) und sein
    /// Bezugspunkt (Ende der Linie) im Bild.
    pub fn paint_mark(
        &self,
        scene: &Scene,
        theme: &Theme,
        fonts: &Fonts,
        left: bool,
        scale: f32,
    ) -> (Canvas, f32, f32) {
        let s = scale;
        let size = mark_size(scale);
        let mut c = Canvas::new(size as usize, size as usize);
        let color = Rgba::from_f32(if self.is_busy() {
            theme.interact.drag
        } else {
            scene.table().section_ends.1
        });
        let (ax, ay) = self.mark_anchor(left, scale);
        // Pfeil senkrecht zur Linie in Blickrichtung, Buchstabe zur
        // Linienmitte hin
        let (dx, dy) = screen(view_dir(self.id, self.flip));
        let (nx, ny) = (-dy, dx);
        let (lx, ly) = screen(line_dir(self.id));
        let (ix, iy) = if left { (lx, ly) } else { (-lx, -ly) };
        let at = |along: f32, side: f32| (ax + dx * along + nx * side, ay + dy * along + ny * side);
        let shaft = 1.2 * s;
        let mut p = Path::new();
        let pts = [
            at(0.0, -shaft),
            at(0.0, shaft),
            at(22.0 * s, shaft),
            at(22.0 * s, -shaft),
        ];
        p.move_to(pts[0].0, pts[0].1);
        for q in &pts[1..] {
            p.line_to(q.0, q.1);
        }
        p.close();
        c.fill(&p, color);
        let mut p = Path::new();
        let (t, l, r) = (
            at(ARROW * s, 0.0),
            at(20.0 * s, 6.5 * s),
            at(20.0 * s, -6.5 * s),
        );
        p.move_to(t.0, t.1)
            .line_to(l.0, l.1)
            .line_to(r.0, r.1)
            .close();
        c.fill(&p, color);
        if let Some(f) = fonts.bold.as_ref().or(fonts.regular.as_ref()) {
            let px = theme.size.font_mark * s;
            let letter = CUT_NAMES.get(self.id).copied().unwrap_or("?");
            let (tw, cap) = (f.width(letter, px), f.cap_height(px));
            // Mitte des Buchstabens: vom Linienende in Blickrichtung und zur
            // Linienmitte, je um den halben Buchstaben mehr
            let half = |vx: f32, vy: f32| (vx.abs() * tw + vy.abs() * cap) * 0.5;
            let a = 8.0 * s + half(dx, dy);
            let b = 7.0 * s + half(ix, iy);
            let (cx, cy) = (ax + dx * a + ix * b, ay + dy * a + iy * b);
            f.draw(
                &mut c,
                letter,
                px,
                (cx - tw * 0.5).round(),
                (cy + cap * 0.5).round(),
                color,
            );
        }
        (c, ax, ay)
    }
}

/// Beide Schnittlinien im Grundriss: Ereignisse gehen an die gezogene,
/// sonst an alle; die erste, die einen Klick nimmt, behält ihn.
pub struct Sections {
    pub lines: [SectionLine; CUTS],
}

impl Default for Sections {
    fn default() -> Sections {
        Sections {
            lines: [SectionLine::new(CUT_A), SectionLine::new(CUT_B)],
        }
    }
}

impl Sections {
    /// Lage und Blickrichtung aus dem Modell (der Datei).
    pub fn load(&mut self, scene: &Scene) {
        for (l, c) in self.lines.iter_mut().zip(scene.model().cuts()) {
            l.set_cut(*c);
        }
    }

    pub fn ensure(&mut self, scene: &Scene) {
        for l in &mut self.lines {
            l.ensure(scene);
        }
    }

    /// Ebene des Schnitts `active`.
    pub fn plane(&self, active: usize) -> Option<(Vec3, Vec3)> {
        self.lines.get(active).and_then(|l| l.plane())
    }

    pub fn is_busy(&self) -> bool {
        self.lines.iter().any(|l| l.is_busy())
    }

    pub fn is_dragging(&self) -> bool {
        self.lines.iter().any(|l| l.is_dragging())
    }

    pub fn over_mark(&self) -> bool {
        self.lines.iter().any(|l| l.over_mark())
    }

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
        let dragging = self.lines.iter().position(|l| l.is_dragging());
        let mut taken = false;
        for (i, l) in self.lines.iter_mut().enumerate() {
            let ev = match (dragging, taken) {
                (Some(d), _) if d != i => continue,
                (None, true) => &Event::MouseLeave,
                _ => e,
            };
            let o = l.handle(ev, scene, cam, w, h, scale, enabled);
            out.redraw |= o.redraw;
            out.mirrored |= o.mirrored;
            if o.changed {
                out.changed = true;
                out.line = Some(i);
            }
            if o.consumed {
                out.consumed = true;
                taken = matches!(e, Event::MouseDown { .. });
            }
        }
        out
    }

    pub fn helpers(
        &self,
        scene: &Scene,
        cam: &Camera,
        h: f64,
        scale: f32,
        theme: &Theme,
    ) -> Vec<Helper> {
        self.lines
            .iter()
            .flat_map(|l| l.helpers(scene, cam, h, scale, theme))
            .collect()
    }

    /// Endsymbole beider Linien mit der Kennung ihrer Linie.
    pub fn marks(&self, scene: &Scene, cam: &Camera, w: f64, h: f64) -> Vec<(usize, Mark)> {
        self.lines
            .iter()
            .enumerate()
            .flat_map(|(i, l)| l.marks(scene, cam, w, h).into_iter().map(move |m| (i, m)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn richtung_ebene_und_spiegeln() {
        assert_eq!(view_dir(CUT_A, false), vec3(0.0, 1.0, 0.0));
        assert_eq!(view_dir(CUT_A, true), vec3(0.0, -1.0, 0.0));
        assert_eq!(view_dir(CUT_B, false), vec3(1.0, 0.0, 0.0));
        assert_eq!(view_dir(CUT_B, true), vec3(-1.0, 0.0, 0.0));
        let c = Cut {
            pos: Some(4000.0),
            flip: false,
        };
        assert_eq!(
            plane_of(CUT_B, c),
            Some((vec3(4000.0, 0.0, 0.0), vec3(-1.0, 0.0, 0.0)))
        );
        assert_eq!(
            plane_of(CUT_A, mirror(c)),
            Some((vec3(0.0, 4000.0, 0.0), vec3(0.0, 1.0, 0.0)))
        );
        assert_eq!(mirror(mirror(c)), c);
        assert_eq!(plane_of(CUT_A, Cut::default()), None);
        let m = vec3(5004.0, 3996.0, 1000.0);
        assert_eq!(
            (default_pos(CUT_A, m), default_pos(CUT_B, m)),
            (4000.0, 5000.0)
        );
        // A oben, B unten
        assert_eq!(ORDER, [CUT_B, CUT_A]);
        assert_eq!(title(CUT_B), "B–B");
        // 10 × 8 m: A läuft längs der langen Seite
        let b = Some((vec3(0.0, 0.0, 0.0), vec3(10000.0, 8000.0, 5000.0)));
        assert_eq!(subtitle(CUT_A, true, b), "Längsschnitt · gespiegelt");
        assert_eq!(subtitle(CUT_B, false, b), "Querschnitt");
        let q = Some((vec3(0.0, 0.0, 0.0), vec3(8000.0, 8000.0, 5000.0)));
        assert_eq!(subtitle(CUT_B, false, q), "");
        assert_eq!(subtitle(CUT_B, true, None), "gespiegelt");
        assert_eq!(arrow_hint(CUT_A, true, String::new()).0, "Schnitt A–A ↑");
    }

    fn ev(x: f64, y: f64, kind: u8) -> Event {
        let mods = sk_platform::Modifiers {
            shift: false,
            ctrl: false,
            alt: false,
        };
        let button = MouseButton::Left;
        match kind {
            0 => Event::MouseDown { button, x, y, mods },
            1 => Event::MouseMove { x, y, mods },
            _ => Event::MouseUp { button, x, y, mods },
        }
    }

    /// Beide Linien im Grundriss: B liegt mittig quer zu A, lässt sich für
    /// sich verschieben, und ein Klick auf ihren Pfeil spiegelt nur B.
    #[test]
    fn zwei_linien_ziehen_und_spiegeln() {
        let (w, h) = (1440.0, 778.0);
        let mut s = Scene::with_model(sk_model::Model::with_seed(5));
        s.add_wall(&sk_model::WallChain {
            base: 0.0,
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: sk_model::RefSide::Left,
            layers: Vec::new(),
            height: 2750.0,
            joints: Default::default(),
        })
        .unwrap();
        let c = crate::fit_parallel(crate::ui::ViewKind::Plan, s.bounds(), w, h);
        let mut sect = Sections::default();
        sect.ensure(&s);
        let (a, b) = (sect.lines[CUT_A].cut(), sect.lines[CUT_B].cut());
        assert_eq!((a.pos, b.pos), (Some(4000.0), Some(5000.0)));
        assert_eq!(sect.marks(&s, &c, w, h).len(), 4);

        // B greifen (abseits von A) und nach x = 6,00 m ziehen
        let px = |p: Vec3| c.project(p, w, h).unwrap();
        let (x, y) = px(vec3(5000.0, 2000.0, 0.0));
        assert!(sect.handle(&ev(x, y, 0), &s, &c, w, h, 1.0, true).consumed);
        let (x2, y2) = px(vec3(6003.0, 2000.0, 0.0));
        let o = sect.handle(&ev(x2, y2, 1), &s, &c, w, h, 1.0, true);
        assert!(o.changed && o.line == Some(CUT_B));
        sect.handle(&ev(x2, y2, 2), &s, &c, w, h, 1.0, true);
        assert_eq!(sect.lines[CUT_B].cut().pos, Some(6000.0));
        assert_eq!(sect.lines[CUT_A].cut(), a, "A bleibt liegen");
        let (p0, n) = sect.plane(CUT_B).unwrap();
        assert_eq!((p0.x, n), (6000.0, vec3(-1.0, 0.0, 0.0)));

        // Pfeil von B (zeigt nach +x, im Bild nach rechts) anklicken
        let (_, m) = sect
            .marks(&s, &c, w, h)
            .into_iter()
            .find(|(i, m)| *i == CUT_B && m.left)
            .unwrap();
        let o = sect.handle(&ev(m.x + 20.0, m.y, 0), &s, &c, w, h, 1.0, true);
        assert!(o.mirrored && o.consumed && o.line == Some(CUT_B));
        assert!(sect.lines[CUT_B].cut().flip && !sect.lines[CUT_A].cut().flip);
        assert_eq!(sect.plane(CUT_B).unwrap().1, vec3(1.0, 0.0, 0.0));
        // Der Pfeil zeigt jetzt nach links: derselbe Punkt trifft nichts mehr
        sect.handle(&ev(m.x + 20.0, m.y, 2), &s, &c, w, h, 1.0, true);
        let o = sect.handle(&ev(m.x + 20.0, m.y, 0), &s, &c, w, h, 1.0, true);
        assert!(!o.mirrored);
    }
}

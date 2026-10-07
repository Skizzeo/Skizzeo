//! „Bündig setzen“ mit freier Zielwand (Jörn 07.10. 07:53, E20, Regel 36):
//! Nach dem Klick auf „Bündig setzen“ leuchten beide Wände des Paars; die
//! angeklickte ist das Ziel, die andere rückt bündig an sie heran. Die
//! Entscheidungen stehen hier ohne Fenster, die Verdrahtung in `main.rs`.

use crate::camera::Camera;
use crate::scene::Scene;
use crate::selection;
use crate::ui::ViewKind;
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, FlushError, Model, WallChain};
use sk_platform::Key;
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Zielkarte, solange die Zielwahl läuft (E20 §3).
pub const CARD: [&str; 2] = [
    "Zielwand anklicken",
    "Die andere Wand rückt bündig an sie heran. Esc bricht ab.",
];

/// Karte nach dem Klick auf eine Zielwand, an die nicht gerückt werden kann.
pub const REFUSED: [&str; 2] = [
    "Die EG-Wand kann hier nicht nachrücken.",
    "Daneben bliebe ein zu kurzes Wandstück. Die OG-Wand an die EG-Wand setzen oder die Nachbarwand erst anpassen.",
];

/// Zeilen der Ablehnungskarte: rückt die EG-Wand, die aus E20 §3; rückt
/// die OG-Wand (selten, ihr Versatz war schon gültig), der Grund.
pub fn refused_lines(t: Target, e: FlushError) -> [String; 2] {
    match t {
        Target::Above => REFUSED.map(String::from),
        Target::Below => [
            "Die OG-Wand kann hier nicht nachrücken.".into(),
            e.message().into(),
        ],
    }
}

/// Statuszeile nach Esc, Klick ins Leere oder auf ein anderes Bauteil.
pub const CANCELLED: &str = "Bündig setzen abgebrochen.";

/// Welche Wand des Paars Ziel ist: die darunter (das OG rückt, wie bisher)
/// oder die darüber (das EG rückt).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Below,
    Above,
}

/// Statuszeile, wenn `wall` an `target` rückt (E20 §3); abgelehnt der
/// Grund aus [`FlushError::message`].
pub fn status_text(m: &Model, wall: ElementId, target: ElementId) -> String {
    match preview(m, wall, target) {
        Ok(p) => {
            let d = crate::selection::de(p.distance / 1000.0, 2);
            if m.wall_below(wall) == Some(target) {
                format!("Die OG-Wand rückt {d} m an die EG-Wand.")
            } else {
                format!("Die EG-Wand rückt {d} m an die OG-Wand.")
            }
        }
        Err(e) => e.message().to_string(),
    }
}

/// Statuszeile nach dem Einrasten, wenn `wall` an `target` gerückt ist.
pub fn done_text(m: &Model, wall: ElementId, target: ElementId) -> &'static str {
    if m.wall_below(wall) == Some(target) {
        "OG-Wand bündig gesetzt."
    } else {
        "EG-Wand bündig gesetzt."
    }
}

/// Vorschau: Die Wand `moving` rückt um `distance` (mm) und steht danach
/// in `chain` als Segment `seg`.
#[derive(Clone, Debug)]
pub struct Preview {
    pub moving: ElementId,
    pub distance: f64,
    pub chain: WallChain,
    pub seg: usize,
}

/// Rechnet [`Model::flush_to`] an einer Kopie (ändert nichts).
pub fn preview(m: &Model, wall: ElementId, target: ElementId) -> Result<Preview, FlushError> {
    m.can_flush_to(wall, target)?;
    let upper = if m.wall_below(wall) == Some(target) {
        wall
    } else {
        target
    };
    let (o, _) = m.stack_offset(upper).ok_or(FlushError::NotPartners)?;
    let mut t = m.clone();
    if !t.in_step() {
        t.begin("Vorschau");
    }
    t.flush_to(wall, target)?;
    let (run, seg) = t.segment_of(wall).ok_or(FlushError::Invalid)?;
    let chain = t.chain(run).ok_or(FlushError::Invalid)?;
    Ok(Preview {
        moving: wall,
        distance: o.abs(),
        chain,
        seg,
    })
}

/// Was ein Ereignis während der Zielwahl bewirkt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// Nichts (etwa eine andere Taste).
    None,
    /// Bündig setzen: die erste Wand rückt an die zweite.
    Flush(ElementId, ElementId),
    /// Dieses Ziel geht nicht: Ablehnungskarte, die Zielwahl bleibt offen.
    Refused(Target, FlushError),
    /// Abbrechen, nichts geändert.
    Cancel,
}

/// Laufende Zielwahl an einem Paar.
#[derive(Clone, Debug)]
pub struct FlushPick {
    /// Die gestapelte Wand (OG).
    pub wall: ElementId,
    /// Ihr Partner darunter (EG).
    pub below: ElementId,
    /// Kandidat unter der Maus.
    pub hover: Option<Target>,
    /// Ergebnis je Ziel: [unten, oben].
    checks: [Result<Preview, FlushError>; 2],
    /// Statuszeile je Ziel.
    texts: [String; 2],
    /// Stand des Modells, für den `checks` gilt.
    rev: u64,
}

fn slot(t: Target) -> usize {
    match t {
        Target::Below => 0,
        Target::Above => 1,
    }
}

impl FlushPick {
    /// Zielwahl an der gestapelten Wand `wall`. `None` ohne Partner oder
    /// ohne Versatz (dann gibt es nichts zu wählen: nur koppeln).
    pub fn start(m: &Model, wall: ElementId) -> Option<FlushPick> {
        let (o, _) = m.stack_offset(wall)?;
        let below = m.wall_below(wall)?;
        if o == 0.0 {
            return None;
        }
        Some(FlushPick {
            wall,
            below,
            hover: None,
            checks: [preview(m, wall, below), preview(m, below, wall)],
            texts: [status_text(m, wall, below), status_text(m, below, wall)],
            rev: m.revision(),
        })
    }

    /// Nach einer Änderung am Modell neu rechnen; `false`, wenn das Paar
    /// nicht mehr gilt (abbrechen).
    pub fn refresh(&mut self, m: &Model) -> bool {
        if m.revision() == self.rev {
            return true;
        }
        match FlushPick::start(m, self.wall) {
            Some(p) if p.below == self.below => {
                *self = FlushPick {
                    hover: self.hover,
                    ..p
                };
                true
            }
            _ => false,
        }
    }

    /// Die zwei Kandidaten: OG-Wand, EG-Wand.
    pub fn candidates(&self) -> [ElementId; 2] {
        [self.wall, self.below]
    }

    /// Ziel zum Bauteil `e`: die OG-Wand ist Ziel oben, die EG-Wand Ziel
    /// unten, alles andere keins.
    pub fn target_of(&self, e: Option<ElementId>) -> Option<Target> {
        match e {
            Some(e) if e == self.wall => Some(Target::Above),
            Some(e) if e == self.below => Some(Target::Below),
            _ => None,
        }
    }

    /// Die Wand, die rückt, und die Zielwand.
    pub fn pair(&self, t: Target) -> (ElementId, ElementId) {
        match t {
            Target::Below => (self.wall, self.below),
            Target::Above => (self.below, self.wall),
        }
    }

    /// Ergebnis für ein Ziel.
    pub fn check(&self, t: Target) -> Result<&Preview, FlushError> {
        self.checks[slot(t)].as_ref().map_err(|e| *e)
    }

    /// Maus über dem Bauteil `e`. `true`, wenn sich der Kandidat geändert hat.
    pub fn hover(&mut self, e: Option<ElementId>) -> bool {
        let t = self.target_of(e);
        let changed = t != self.hover;
        self.hover = t;
        changed
    }

    /// Linksklick auf das Bauteil `e` (oder ins Leere).
    pub fn click(&self, e: Option<ElementId>) -> Act {
        match self.target_of(e) {
            Some(t) => self.choose(t),
            None => Act::Cancel,
        }
    }

    /// Taste: Esc bricht ab, Enter wählt die EG-Wand (wie bisher).
    pub fn key(&self, key: Key) -> Act {
        match key {
            Key::Escape => Act::Cancel,
            Key::Enter => self.choose(Target::Below),
            _ => Act::None,
        }
    }

    fn choose(&self, t: Target) -> Act {
        match self.check(t) {
            Ok(_) => {
                let (w, to) = self.pair(t);
                Act::Flush(w, to)
            }
            Err(e) => Act::Refused(t, e),
        }
    }

    /// Statuszeile über einem Kandidaten (E20 §3).
    pub fn status(&self) -> Option<String> {
        self.hover.map(|t| self.texts[slot(t)].clone())
    }

    /// Weg der rückenden Wand unter der Maus: Mitte der Außenkante vorher
    /// und nachher (auf Höhe ihres Fußes) und die Maßzahl, z. B. „3,33 m“.
    pub fn path(&self, scene: &Scene) -> Option<(Vec3, Vec3, String)> {
        let p = self.check(self.hover?).ok()?;
        let (run, k) = scene.model().segment_of(p.moving)?;
        let now = scene.chain(run)?;
        let mid = |c: &WallChain, k: usize| {
            let (a, b) = *c.outer_foot().get(k)?;
            Some((a + b) * 0.5 + vec3(0.0, 0.0, c.base))
        };
        Some((
            mid(now, k)?,
            mid(&p.chain, p.seg)?,
            format!("{} m", crate::selection::de(p.distance / 1000.0, 2)),
        ))
    }

    /// Hilfslinien der Zielwahl (E20 §2): Kandidaten in `link_on`, der unter
    /// der Maus wie gewählt mit Schein, dazu der Geist der anderen Wand an
    /// ihrer künftigen Lage und der Weg; abgelehnt rot und ohne Geist.
    pub fn helpers(
        &self,
        scene: &Scene,
        view: ViewKind,
        section: Option<(Vec3, Vec3)>,
        scale: f32,
        theme: &Theme,
    ) -> Vec<Helper> {
        let sz = &theme.size;
        let line = |a: Vec3, b: Vec3, color: [f32; 4], width: f32, dash: f32| Helper {
            a: a.to_f32(),
            b: b.to_f32(),
            color,
            width: width * scale,
            dash: dash * scale,
            pattern: sk_render::SOLID,
            occlude: false,
            round: dash == 0.0,
        };
        let plan_cut = scene.plan_cut();
        let m = scene.model();
        let body = |e: ElementId| {
            let (run, k) = m.segment_of(e)?;
            Some((scene.chain(run)?.clone(), k))
        };
        let mut out = Vec::new();
        let hovered = self.hover.map(|t| (t, self.check(t)));
        for (e, t) in [(self.wall, Target::Above), (self.below, Target::Below)] {
            let Some((c, k)) = body(e) else {
                continue;
            };
            let edges = prism(&c, k, view, plan_cut);
            match hovered {
                Some((h, Ok(_))) if h == t => {
                    // Wie gewählt, mit dem Schein des Hovers aus der Mengenliste
                    out.extend(selection::hover_glow(scene, e, view, section, scale, theme));
                    out.extend(selection::helpers(scene, e, view, section, scale, theme));
                }
                Some((h, Err(_))) if h == t => {
                    for &(p, q) in &edges {
                        out.push(line(p, q, theme.ui.danger.to_f32(), sz.link_line, 0.0));
                    }
                }
                _ => {
                    for &(p, q) in &edges {
                        out.push(line(p, q, theme.ui.link_on.to_f32(), sz.link_line, 0.0));
                    }
                }
            }
        }
        // Geist der anderen Wand an ihrer künftigen Lage, und der Weg
        if let Some((_, Ok(p))) = hovered {
            let ghost = theme.interact.drag_ghost;
            for (a, b) in prism(&p.chain, p.seg, view, plan_cut) {
                out.push(line(a, b, ghost, sz.link_line, sz.link_dash));
            }
            if let Some((a, b, _)) = self.path(scene) {
                out.push(line(a, b, ghost, sz.link_line, sz.link_dash));
            }
        }
        out
    }

    /// Kandidat unter dem Bildpunkt im Grundriss, wo die OG-Wand nicht
    /// gezeichnet ist: der Fußumriss enthält den Punkt (6 px Spiel); liegen
    /// beide darunter, gewinnt die Wand, deren Mitte näher liegt.
    pub fn plan_hit(
        &self,
        scene: &Scene,
        cam: &Camera,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> Option<ElementId> {
        let m = scene.model();
        let mut best: Option<(f64, ElementId)> = None;
        for e in self.candidates() {
            let Some((run, k)) = m.segment_of(e) else {
                continue;
            };
            let Some(f) = scene.chain(run).and_then(|c| c.segment_footprint(k)) else {
                continue;
            };
            let pts: Vec<(f64, f64)> = f.iter().filter_map(|p| cam.project(*p, w, h)).collect();
            if pts.len() != 4 {
                continue;
            }
            let inside = contains(&pts, (x, y)) || edge_dist(&pts, (x, y)) <= 6.0;
            if !inside {
                continue;
            }
            let c = pts
                .iter()
                .fold((0.0, 0.0), |s, p| (s.0 + p.0 / 4.0, s.1 + p.1 / 4.0));
            let d = (c.0 - x).hypot(c.1 - y);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, e));
            }
        }
        best.map(|(_, e)| e)
    }
}

/// Maßzahl am Weg (E20 §2): `dim_text` auf `shadow_band`.
pub fn paint_label(
    fonts: &sk_ui::widgets::Fonts,
    text: &str,
    s: f32,
    t: &Theme,
) -> sk_paint::Canvas {
    let px = t.size.font_small * s;
    let f = fonts.regular.as_ref();
    let tw = f.map_or(text.len() as f32 * px * 0.5, |f| f.width(text, px));
    let (pad, h) = (
        (t.size.dim_label_pad * s).round(),
        (t.size.dim_label_h * s).round(),
    );
    let w = (tw + 2.0 * pad).ceil();
    let mut c = sk_paint::Canvas::new(w as usize, h as usize);
    let mut p = sk_paint::Path::new();
    p.rounded_rect(0.0, 0.0, w, h, t.size.dim_label_radius * s);
    c.fill(&p, sk_paint::Rgba::from_f32(t.interact.shadow_band));
    let cap = f.map_or(px * 0.7, |f| f.cap_height(px));
    let base = ((h + cap) * 0.5).round();
    sk_ui::widgets::text(&mut c, f, text, px, pad, base, t.ui.dim_text);
    c
}

/// Kanten des Wandkörpers von Segment `k` (im Grundriss nur der Umriss).
fn prism(c: &WallChain, k: usize, view: ViewKind, plan_cut: f64) -> Vec<(Vec3, Vec3)> {
    let Some(f) = c.segment_footprint(k) else {
        return Vec::new();
    };
    let at = |p: Vec3, z: f64| vec3(p.x, p.y, z);
    let (z0, z1) = if view == ViewKind::Plan {
        let z = c.base + plan_cut.min(c.height);
        (z, z)
    } else {
        (c.base, c.base + c.height)
    };
    let mut v = Vec::with_capacity(12);
    for i in 0..4 {
        let (a, b) = (f[i], f[(i + 1) % 4]);
        v.push((at(a, z1), at(b, z1)));
        if view != ViewKind::Plan {
            v.push((at(a, z0), at(b, z0)));
            v.push((at(a, z0), at(a, z1)));
        }
    }
    v
}

fn contains(poly: &[(f64, f64)], p: (f64, f64)) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
    }
    inside
}

fn edge_dist(poly: &[(f64, f64)], p: (f64, f64)) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let (vx, vy) = (b.0 - a.0, b.1 - a.1);
            let l2 = vx * vx + vy * vy;
            let t = if l2 > 0.0 {
                (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (a.0 + vx * t - p.0).hypot(a.1 + vy * t - p.1)
        })
        .fold(f64::MAX, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn haus() -> (Scene, ElementId, ElementId) {
        let mut m = Model::with_seed(74);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let og = m.runs_above(eg)[0];
        let w = m.wall_at(og, 1).unwrap();
        let p = m.wall_at(eg, 1).unwrap();
        assert!(m.set_linked(w, false));
        assert!(m.set_offset(w, 300.0).is_some());
        assert!(m.set_linked(w, true));
        (Scene::with_model(m), w, p)
    }

    #[test]
    fn zielwahl_entscheidet() {
        let (s, w, p) = haus();
        let mut pick = FlushPick::start(s.model(), w).unwrap();
        assert_eq!(pick.candidates(), [w, p]);
        assert!(pick.hover(Some(w)));
        assert_eq!(pick.hover, Some(Target::Above));
        assert_eq!(
            pick.status().as_deref(),
            Some("Die EG-Wand rückt 0,30 m an die OG-Wand.")
        );
        assert!(!pick.hover(Some(w)), "gleich: nichts neu");
        assert!(pick.hover(Some(p)));
        assert_eq!(
            pick.status().as_deref(),
            Some("Die OG-Wand rückt 0,30 m an die EG-Wand.")
        );
        let v = pick.check(Target::Above).unwrap();
        assert_eq!((v.moving, v.seg), (p, 1));
        assert_eq!(v.chain.points[1].y, 8300.0);
        let path = pick.path(&s).unwrap();
        assert_eq!(path.2, "0,30 m");
        assert!(((path.0 - path.1).length() - 300.0).abs() < 1e-6);
        assert!(!pick
            .helpers(&s, ViewKind::Persp, None, 1.0, &Theme::dark())
            .is_empty());
        assert_eq!(pick.click(Some(w)), Act::Flush(p, w));
        assert_eq!(pick.click(Some(p)), Act::Flush(w, p));
        assert_eq!(pick.click(None), Act::Cancel);
        let other = s.model().wall_at(s.model().segment_of(p).unwrap().0, 0);
        assert_eq!(pick.click(other), Act::Cancel, "anderes Bauteil");
        assert_eq!(pick.key(Key::Escape), Act::Cancel);
        assert_eq!(pick.key(Key::Enter), Act::Flush(w, p));
        assert_eq!(pick.key(Key::Tab), Act::None);
        assert_eq!(done_text(s.model(), p, w), "EG-Wand bündig gesetzt.");
        assert_eq!(done_text(s.model(), w, p), "OG-Wand bündig gesetzt.");
    }

    #[test]
    fn ohne_versatz_keine_zielwahl() {
        let (mut s, w, p) = haus();
        assert!(s.edit_model("Bündig gesetzt", |m| m.set_flush(w)));
        let m = s.model();
        assert!(FlushPick::start(m, w).is_none());
        assert!(FlushPick::start(m, p).is_none(), "EG hat keinen Partner");
    }
}

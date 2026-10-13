//! Gummiband: violette Linie am äußeren Wandfuß, sichtbar beim Darüberfahren.
//!
//! Mit der linken Maustaste lässt sich ein Segment des Bandes quer zu seiner
//! Richtung ziehen. Die Wand geht live mit, die Nachbarsegmente behalten ihre
//! Richtung und werden länger oder kürzer. Esc bricht das Ziehen ab.
//!
//! Im Schnitt und in den Ansichten blickt man waagerecht auf das Modell. Dort
//! lassen sich nur die Wände ziehen, die vom Betrachter weg laufen: Ihr Fuß
//! erscheint als Punkt und wird beim Darüberfahren als Kugel gezeigt.
//!
//! Paket 8b: Beim Ziehen öffnet eine Ziffer die Eingabe „Versatz“ (gelöste
//! bzw. mit Strg gezogene Wand) oder „Verschiebung“ (gekoppelt; die Seite
//! zeigt die Maus). Enter übernimmt den Wert auf den Millimeter. Direkt nach
//! dem Loslassen ersetzt ein getippter Wert mit Enter den letzten Schritt
//! (Nachkorrektur), solange kein anderer Schritt dazwischen kam.

use crate::camera::Camera;
use crate::measure_input::{opens, InputOutcome, MeasureInput};
use crate::scene::Scene;
use crate::teilen_edit::{self, Teilen};
use crate::ui::MeasureKind;
use breite_edit::BreiteZug;
use ecke_edit::EckeWahl;
use ende_edit::EndeZug;
use sk_math::{dist_to_segment, vec3, Vec3};
use sk_model::{ElementId, RunId, WallChain};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;
use tuer_edit::TuerZug;

#[path = "grundriss/breite_edit.rs"]
pub mod breite_edit;
#[path = "grundriss/ecke_edit.rs"]
pub mod ecke_edit;
#[path = "grundriss/ende_edit.rs"]
pub mod ende_edit;
#[path = "grundriss/tuer_edit.rs"]
pub mod tuer_edit;

/// Greifabstand zum Band in Pixeln (bei 96 dpi).
const PICK_PX: f64 = 8.0;
/// Raster beim Ziehen in Millimetern.
const STEP: f64 = 10.0;
/// So nah an der Flucht der Wand hinter dem Nachbarn rastet das gezogene
/// Stück bündig ein (mm, W-11): die Stufe fällt weg, eine Fuge bleibt.
const SNAP: f64 = 50.0;

struct Drag {
    /// Gegriffene Wand (ihr Band leuchtet) und der Zug, der sich bewegt, mit
    /// dem Segment darin: am gekoppelten OG-Fuß ohne Strg der Fuß der Kette.
    wall: ElementId,
    run: RunId,
    seg: usize,
    start: Vec3,
    normal: Vec3,
    /// Höhe der Ebene, auf der gemessen wird (Fuß der gegriffenen Wand).
    z: f64,
    original: WallChain,
    /// Die Wand, die wandert (gekoppelt der Fuß der Kette).
    moving: ElementId,
    /// Nur den Versatz dieser gestapelten Wand ändern (gelöst, oder
    /// gekoppelt mit Strg): Wand, Versatz beim Greifen, Vorzeichen nach außen.
    offset: Option<(ElementId, f64, f64)>,
    /// Zuletzt abgelehnt (der Stapel würde ungültig).
    blocked: bool,
    /// Seite, zu der die Maus zuletzt gezogen hat (±1, Paket 8b).
    side: f64,
    /// An einer Stufe (W-12): die Fuge darunter, die mitwandert.
    joint: Option<Joint>,
}

/// Fuge im Zug `run` vor Segment `seg`, die beim Ziehen an einer Stufe
/// längs der Flucht wandert; `sign` rechnet die Zugrichtung in die
/// Laufrichtung um.
#[derive(Clone)]
struct Joint {
    run: RunId,
    seg: usize,
    original: WallChain,
    sign: f64,
}

impl Joint {
    fn moved(&self, d: f64) -> Option<WallChain> {
        self.original.with_joint_moved(self.seg, d * self.sign)
    }
}

impl Drag {
    /// Was ein getippter Wert bewegt.
    fn target(&self) -> Target {
        Target {
            run: self.run,
            seg: self.seg,
            original: self.original.clone(),
            moving: self.moving,
            offset: self.offset.map(|o| o.0),
            side: self.side,
            joint: self.joint.clone(),
        }
    }
}

/// Ziel eines getippten Werts (beim Ziehen und in der Nachkorrektur).
#[derive(Clone)]
struct Target {
    run: RunId,
    seg: usize,
    original: WallChain,
    moving: ElementId,
    /// Versatz dieser gestapelten Wand; sonst wandert das Segment.
    offset: Option<ElementId>,
    side: f64,
    joint: Option<Joint>,
}

impl Target {
    fn kind(&self) -> MeasureKind {
        if self.offset.is_some() {
            MeasureKind::Offset
        } else {
            MeasureKind::Length
        }
    }

    fn label(&self) -> &'static str {
        if self.offset.is_some() {
            "Versatz"
        } else {
            "Verschiebung"
        }
    }

    /// Setzt den getippten Wert `v` (mm) genau, ohne das Raster.
    fn apply(&self, scene: &mut Scene, v: f64) -> bool {
        if let Some(wall) = self.offset {
            return scene.set_offset(wall, v);
        }
        if let Some(j) = &self.joint {
            return j.moved(v * self.side).is_some_and(|c| {
                put(scene, j.run, &c);
                true
            });
        }
        match self
            .original
            .with_segment_moved_snap(self.seg, v * self.side, 0.0)
        {
            Some(moved) => {
                put(scene, self.run, &moved);
                true
            }
            None => false,
        }
    }

    /// Erreichter Wert jetzt: Versatz bzw. Betrag der Verschiebung (mm),
    /// gemessen an der Wand in gleicher Richtung, die die alte Lage längs
    /// der Flucht überdeckt und ihr am nächsten liegt (Stufen verschieben
    /// die Nummern der Segmente).
    fn value(&self, scene: &Scene) -> f64 {
        if let Some(wall) = self.offset {
            return scene.model().stack_offset(wall).map_or(0.0, |o| o.0);
        }
        if let Some(j) = &self.joint {
            let now = scene.chain(j.run).map(|c| c.clean_points());
            let old = j.original.clean_points();
            return match (now.as_ref().and_then(|p| p.get(j.seg)), old.get(j.seg)) {
                (Some(a), Some(b)) => (*a - *b).length(),
                _ => 0.0,
            };
        }
        let old = self.original.clean_points();
        let (Some(&a), Some(&b), Some(c)) = (
            old.get(self.seg),
            old.get((self.seg + 1) % old.len().max(1)),
            scene.chain(self.run),
        ) else {
            return 0.0;
        };
        let len = (b - a).length();
        let (dir, n) = (
            (b - a) * (1.0 / len.max(1e-9)),
            self.original.segment_normal(self.seg),
        );
        let n = n.unwrap_or(Vec3::ZERO);
        let pts = c.clean_points();
        (0..c.segment_count())
            .filter_map(|k| {
                let (p, q) = (pts[k], pts[(k + 1) % pts.len()]);
                let along = (q - p).dot(dir);
                let overlap = (q - a).dot(dir).min(len) - (p - a).dot(dir).max(0.0);
                let parallel = (q - p).length() - along < 1e-6;
                (parallel && overlap > 1.0).then(|| (p - a).dot(n).abs())
            })
            .min_by(f64::total_cmp)
            .unwrap_or(0.0)
    }
}

/// Setzt den bewegten Zug. Entsteht oder fällt eine Stufe weg (andere Zahl
/// der Wände), beginnt der Schritt neu beim Stand vor dem Ziehen: So
/// behalten die Wände ihre Nummern bezogen auf den Anfang (Regel 11).
fn put(scene: &mut Scene, run: RunId, moved: &WallChain) -> bool {
    let Some(now) = scene.chain(run) else {
        return false;
    };
    if now.points == moved.points {
        return false;
    }
    if now.clean_points().len() != moved.points.len() {
        scene.rollback();
        scene.begin("Wand verschoben");
        if scene.chain(run).is_some_and(|c| c.points == moved.points) {
            return true;
        }
    }
    scene.set_run_points(run, &moved.points);
    true
}

/// Das letzte Ziehen als Schritt im Verlauf (Nachkorrektur, K3).
struct LastDrag {
    target: Target,
    serial: u64,
    value: f64,
}

/// Was das Ziehen an einer gestapelten Wand tut (Statuszeile, OG Phase 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackDrag {
    /// Gelöst: nur die OG-Wand.
    Free,
    /// Gekoppelt mit Strg: nur diese Wand, die Kette bleibt geschlossen.
    Ctrl,
}

#[derive(Default)]
pub struct WallEdit {
    /// Wand unter der Maus.
    hover: Option<ElementId>,
    drag: Option<Drag>,
    mouse: Option<(f64, f64)>,
    /// Grundriss: nur die Füße des aktiven Geschosses greifen (Höhe des
    /// Wandfußes in mm).
    pub plan_z: Option<f64>,
    /// Wand, über deren Kettensymbol die Maus steht: ihr Band bleibt
    /// sichtbar (E16).
    pub link_hover: Option<ElementId>,
    /// Schnittebene der Ansicht „Schnitt“ (Punkt, Normale zum Betrachter):
    /// Was davor liegt, ist weggeschnitten und verdeckt nichts.
    pub section: Option<(Vec3, Vec3)>,
    /// Getippter Wert beim Ziehen bzw. in der Nachkorrektur (Paket 8b).
    input: Option<MeasureInput>,
    /// Letztes Ziehen, solange die Nachkorrektur möglich ist.
    last: Option<LastDrag>,
    /// Laufendes „Wand teilen“: nimmt Klicks und Tasten, das Band ruht.
    pub teilen: Option<Teilen>,
    /// Laufendes „Ecke bilden“ oder „Verlängern“ (G5), wie beim Teilen.
    pub ecke: Option<EckeWahl>,
    /// Gewählte Wand: im Grundriss Griffe an ihren Enden (G5b).
    gewaehlt: Option<ElementId>,
    /// Maus über einem Griff.
    griff: bool,
    /// Laufendes Ziehen an einem Wandende (G5b).
    pub ende: Option<EndeZug>,
    /// Griffe der Türen bei der Maus (Grundriss) und der darunter.
    tueren: Vec<(ElementId, Vec3)>,
    tuer_griff: Option<ElementId>,
    /// Laufendes Ziehen am Griff einer Tür (Aufschlag, DIN).
    pub tuer: Option<TuerZug>,
    /// Griffe an den Enden der Öffnungen bei der Maus und der darunter.
    breiten: Vec<breite_edit::Griff>,
    breite_griff: Option<breite_edit::Griff>,
    /// Laufendes Ziehen an einem Ende einer Öffnung (Breite, O6).
    pub breite: Option<BreiteZug>,
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
    /// Greifen abgelehnt: dieses Bauteil ist gesperrt (Paket 4 §1.7),
    /// dazu die gegriffene Wand.
    pub locked: Option<(ElementId, ElementId)>,
    /// Ändern abgelehnt (Leertaste an der Tür): dieses Bauteil ist
    /// gesperrt, dazu die Tür.
    pub gesperrt: Option<(ElementId, ElementId)>,
    /// Bündig zusammengeschoben (W-11): diese Wand steht jetzt an einer
    /// oder zwei Fugen in einer Flucht mit ihren Nachbarn.
    pub joined: Option<ElementId>,
    /// Werkzeug „Wand teilen“ (W-12).
    pub teilen: Option<teilen_edit::Outcome>,
    /// Werkzeuge „Ecke bilden“ und „Verlängern“ (G5).
    pub ecke: Option<ecke_edit::Outcome>,
}

/// Steht `wall` nach dem Ziehen an einer Fuge? Dann ist sie gerade bündig
/// eingerastet (vorher hätte das Ziehen dort eine Stufe eingefügt).
fn joined(scene: &Scene, run: RunId, wall: ElementId) -> Option<ElementId> {
    let m = scene.model();
    m.straight_joints(run)
        .iter()
        .any(|j| j.0 == wall || j.1 == wall)
        .then_some(wall)
}

/// Wände, die das Ziehen am Fuß von `wall` ohne Strg ändern würde (Paket 4
/// §2.2): die gezogene Wand (gekoppelt der Fuß der Kette), ihre Nachbarn im
/// Zug (deren Ecken wandern) und alles, was gekoppelt darüber steht.
#[cfg(test)]
pub fn drag_set(m: &sk_model::Model, wall: ElementId) -> Vec<ElementId> {
    drag_set_mods(m, wall, false)
}

/// Wie [`drag_set`]; mit Strg zieht eine gestapelte Wand nur sich selbst.
pub fn drag_set_mods(m: &sk_model::Model, wall: ElementId, ctrl: bool) -> Vec<ElementId> {
    let moving = match m.stack_offset(wall) {
        Some((_, true)) if !ctrl => m.chain_foot(wall),
        _ => wall,
    };
    let mut seeds = vec![moving];
    // An einer Stufe wandert die Fuge darunter: beide Stücke dort
    if let Some((run, k)) = m.step_joint(wall) {
        let n = m.run(run).map_or(1, |r| r.segments.len());
        seeds.extend(m.wall_at(run, k));
        seeds.extend(m.wall_at(run, (k + n - 1) % n));
    }
    if let Some((run, k)) = m.segment_of(moving) {
        let r = m.run(run);
        let n = r.map_or(0, |r| r.segments.len());
        let closed = r.is_some_and(|r| r.closed);
        if k > 0 || closed {
            seeds.extend(m.wall_at(run, (k + n - 1) % n));
        }
        if k + 1 < n || closed {
            seeds.extend(m.wall_at(run, (k + 1) % n));
        }
    }
    let mut out: Vec<ElementId> = Vec::new();
    while let Some(w) = seeds.pop() {
        if out.contains(&w) {
            continue;
        }
        out.push(w);
        // Gekoppelt darüber: geht mit
        for (id, _) in m.elements().iter() {
            if m.wall_below(id) == Some(w) && m.stack_offset(id).is_some_and(|s| s.1) {
                seeds.push(id);
            }
        }
    }
    out
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
fn drag_point(cam: &Camera, x: f64, y: f64, w: f64, h: f64, z: f64) -> Option<Vec3> {
    if cam.ortho.is_some() {
        Some(cam.ray(x, y, w, h).0)
    } else {
        cam.plane_point(x, y, w, h, z)
    }
}

/// Texte der Zielwahl „Bündig setzen“ (E20 §3) unter den Namen, die die
/// Abnahme A174 erwartet; sie stehen in [`crate::flush_pick`].
#[cfg(test)]
pub use crate::flush_pick::{CARD as FLUSH_PICK, REFUSED as FLUSH_REFUSED};

/// Fußlinie des Partners darunter in 3D (OG Phase 2): durchgehend in
/// `link_on`, Breite `size.link_line`, über allem.
pub fn partner_line(a: Vec3, b: Vec3, scale: f32, theme: &Theme) -> Helper {
    let lift = |p: Vec3| [p.x as f32, p.y as f32, p.z as f32 + 2.0];
    Helper {
        a: lift(a),
        b: lift(b),
        color: theme.ui.link_on.to_f32(),
        width: theme.size.link_line * scale,
        dash: 0.0,
        pattern: sk_render::SOLID,
        occlude: false,
        round: false,
    }
}

impl WallEdit {
    /// Stand der Ansicht vor jedem Ereignis: Schnittebene, Höhe des
    /// Grundrisses und gewählte Wand.
    pub fn stand(
        &mut self,
        section: Option<(Vec3, Vec3)>,
        plan_z: Option<f64>,
        gewaehlt: Option<ElementId>,
    ) {
        self.section = section;
        self.plan_z = plan_z;
        self.gewaehlt = gewaehlt;
    }

    /// Offene Maßeingabe beim Ziehen bzw. in der Nachkorrektur (Paket 8b).
    pub fn input(&self) -> Option<&MeasureInput> {
        match (&self.teilen, &self.ende, &self.breite) {
            (Some(t), ..) => t.input.as_ref(),
            (None, Some(z), _) => z.input.as_ref(),
            (None, None, Some(b)) => b.input.as_ref(),
            (None, None, None) => self.input.as_ref(),
        }
    }

    /// Griff der gewählten Wand unter der Maus (nur im Grundriss).
    fn griff_bei(
        &self,
        scene: &Scene,
        cam: &Camera,
        w: f64,
        h: f64,
        scale: f64,
    ) -> Option<(usize, Vec3)> {
        let (wand, z, mouse) = (self.gewaehlt?, self.plan_z?, self.mouse?);
        ende_edit::griff_bei(scene, cam, wand, z, mouse, w, h, scale)
    }

    /// Läuft „Wand teilen“ oder ein anderes Werkzeug an einer gewählten
    /// Wand („Ecke bilden“, „Verlängern“)? Es nimmt Klicks und Tasten.
    pub fn splitting(&self) -> bool {
        self.teilen.is_some() || self.ecke.is_some()
    }

    /// Hat das laufende Tun an der Wand ein eigenes Hilfethema ([`Self::thema`])
    /// statt „Wand verschieben“? Teilen, Ecke, Breite und Aufschlag (39.2).
    pub fn eigenes_thema(&self) -> bool {
        self.splitting() || self.tuer.is_some() || self.breite.is_some()
    }

    /// Hilfethema des laufenden Werkzeugs an der Wand.
    pub fn thema(&self) -> crate::help::Topic {
        if self.tuer.is_some() || self.breite.is_some() {
            crate::help::Topic::Openings
        } else if self.ecke.is_some() {
            crate::help::Topic::InnerWall
        } else {
            crate::help::Topic::Split
        }
    }

    /// „Ecke bilden“ bzw. „Verlängern“ an `wall` beginnen (G5).
    pub fn start_ecke(
        &mut self,
        scene: &Scene,
        wall: ElementId,
        art: ecke_edit::Art,
    ) -> Result<(), sk_model::EckeFehler> {
        self.drag = None;
        self.last = None;
        self.input = None;
        self.hover = None;
        self.teilen = None;
        self.ecke = Some(EckeWahl::start(scene, wall, art)?);
        Ok(())
    }

    /// „Wand teilen“ an `wall` beginnen; `false`, wenn sie zu kurz ist.
    pub fn start_split(&mut self, scene: &Scene, wall: ElementId) -> bool {
        self.drag = None;
        self.last = None;
        self.input = None;
        self.hover = None;
        self.teilen = Teilen::start(scene, wall);
        self.teilen.is_some()
    }

    /// Ist die Nachkorrektur des letzten Ziehens noch möglich (kein anderer
    /// Schritt dazwischen, nichts wiederherzustellen)?
    fn last_valid(&self, scene: &Scene) -> bool {
        self.last
            .as_ref()
            .is_some_and(|l| l.serial == scene.step_serial() && scene.redo_label().is_none())
    }

    /// Pille statt des Hinweises am Band: Text und ob gerade getippt wird.
    /// Beim Ziehen nur mit Eingabe, nach dem Loslassen mit dem erreichten
    /// Wert (Nachkorrektur; die App blendet sie nach 1,5 s aus).
    pub fn pill(&self, scene: &Scene) -> Option<(String, bool)> {
        if let Some(t) = &self.teilen {
            return t.pill();
        }
        if let Some(t) = &self.ecke {
            return t.pill(scene);
        }
        if let Some(z) = &self.ende {
            return z.pill();
        }
        if let Some(b) = &self.breite {
            return b.pill(scene);
        }
        let label = match (&self.drag, &self.last) {
            (Some(d), _) => d.target().label(),
            (None, Some(l)) if self.last_valid(scene) => l.target.label(),
            _ => return None,
        };
        if let Some(i) = &self.input {
            return Some((i.text([label, ""]), true));
        }
        let l = self.last.as_ref().filter(|_| self.drag.is_none())?;
        let v = l.value;
        let text = if l.target.offset.is_some() && v == 0.0 {
            format!("{label} bündig")
        } else if l.target.offset.is_some() {
            let sign = if v > 0.0 {
                "+"
            } else if v < 0.0 {
                "\u{2212}"
            } else {
                ""
            };
            format!("{label} {sign}{}", crate::ui::m_text(v))
        } else {
            format!("{label} {}", crate::ui::m_text(v))
        };
        Some((text, false))
    }

    /// Feldname der Pille: „Versatz“ oder „Verschiebung“.
    pub fn input_label(&self) -> &'static str {
        if self.teilen.is_some() {
            return teilen_edit::LABEL;
        }
        if self.ende.is_some() {
            return ende_edit::LABEL;
        }
        if self.breite.is_some() {
            return breite_edit::LABEL;
        }
        match (&self.drag, &self.last) {
            (Some(d), _) => d.target().label(),
            (None, Some(l)) => l.target.label(),
            _ => "Verschiebung",
        }
    }

    /// Nachkorrektur beenden (Zeit um, Klick, andere Taste).
    pub fn end_post(&mut self) -> bool {
        let had = self.last.is_some() && self.drag.is_none();
        if self.drag.is_none() {
            self.last = None;
            self.input = None;
        }
        had
    }

    /// Getippten Wert beim Ziehen sofort zeigen (gültig: gesetzt).
    fn apply_typed(&mut self, scene: &mut Scene) -> bool {
        let (Some(d), Some(i)) = (&self.drag, &self.input) else {
            return false;
        };
        let Some(Ok(v)) = i.value(0) else {
            return false;
        };
        d.target().apply(scene, v)
    }

    /// Tasten beim Ziehen und in der Nachkorrektur; `None`: nicht für die
    /// Eingabe (das Ziehen behandelt Esc wie bisher).
    #[allow(clippy::too_many_arguments)]
    fn input_key(
        &mut self,
        key: Key,
        mods: Modifiers,
        scene: &mut Scene,
        cam: &Camera,
        w: f64,
        h: f64,
        scale: f64,
        enabled: bool,
    ) -> Option<EditOutcome> {
        let mut out = EditOutcome {
            consumed: true,
            redraw: true,
            ..Default::default()
        };
        let dragging = self.drag.is_some();
        let Some(i) = self.input.as_mut() else {
            let kind = if let Some(d) = &self.drag {
                d.target().kind()
            } else if self.last_valid(scene) {
                self.last.as_ref()?.target.kind()
            } else {
                // Eine andere Taste beendet die Nachkorrektur
                if !matches!(key, Key::Shift | Key::Control | Key::Alt) {
                    self.last = None;
                }
                return None;
            };
            let Key::Char(ch) = key else {
                if !dragging && !matches!(key, Key::Shift | Key::Control | Key::Alt) {
                    self.last = None;
                }
                return None;
            };
            if !opens(ch, kind, mods) {
                if !dragging {
                    self.last = None;
                }
                return None;
            }
            let mut i = MeasureInput::new(kind, None);
            i.push(ch);
            self.input = Some(i);
            out.changed = self.apply_typed(scene);
            return Some(out);
        };
        match i.key(key, mods) {
            InputOutcome::Ignored => {
                if !dragging {
                    self.input = None;
                    self.last = None;
                }
                return None;
            }
            InputOutcome::Changed | InputOutcome::Refused => {
                out.changed = self.apply_typed(scene);
            }
            // Leer getippt: in der Nachkorrektur darf neu getippt werden
            // (A306, ungültigen Wert berichtigen)
            InputOutcome::Emptied if !dragging => self.input = None,
            InputOutcome::Emptied | InputOutcome::Escape | InputOutcome::EnterEmpty => {
                self.input = None;
                if dragging {
                    // Wieder die Maus
                    out.changed = self.update_drag(scene, cam, w, h);
                } else {
                    self.last = None;
                }
            }
            InputOutcome::Enter => {
                let v = match i.value(0) {
                    Some(Ok(v)) => v,
                    _ => return Some(out),
                };
                self.input = None;
                let serial = scene.step_serial();
                let target = if let Some(d) = self.drag.take() {
                    let t = d.target();
                    t.apply(scene, v);
                    scene.commit();
                    t
                } else {
                    // Nachkorrektur: den letzten Schritt ersetzen (K3, K4)
                    let Some(l) = self.last.take() else {
                        return Some(out);
                    };
                    scene.undo();
                    scene.begin("Wand verschoben");
                    l.target.apply(scene, v);
                    scene.commit();
                    scene.clear_redo();
                    l.target
                };
                if target.offset.is_none() {
                    out.joined = joined(scene, target.run, target.moving);
                }
                // Die Pille bleibt wie nach dem Loslassen stehen (die App
                // blendet sie nach 1,5 s aus); eine neue Ziffer korrigiert
                // erneut
                self.remember(scene, serial, target);
                out.changed = true;
                self.refresh(scene, cam, w, h, scale, enabled);
            }
        }
        Some(out)
    }

    /// Merkt das Ziehen für die Nachkorrektur, wenn `commit` wirklich einen
    /// Schritt angelegt hat (K3).
    fn remember(&mut self, scene: &Scene, before: u64, target: Target) {
        self.last = (scene.step_serial() != before).then(|| LastDrag {
            value: target.value(scene),
            target,
            serial: scene.step_serial(),
        });
    }

    pub fn is_busy(&self) -> bool {
        self.hover.is_some() || self.griff || self.is_dragging() || self.splitting()
    }

    /// Griffe an den Enden der Öffnungen bei der Maus (Grundriss).
    pub fn oeffnungs_enden(&self) -> &[breite_edit::Griff] {
        &self.breiten
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some() || self.ende.is_some() || self.tuer.is_some() || self.breite.is_some()
    }

    /// Wandzug, der gerade gezogen wird.
    pub fn dragging_run(&self) -> Option<RunId> {
        self.drag
            .as_ref()
            .map(|d| d.run)
            .or(self.ende.as_ref().map(|z| z.run))
            .or(self.tuer.as_ref().map(|t| t.run()))
            .or(self.breite.as_ref().map(|b| b.run()))
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
        // Seit OG Phase 2 haben auch die gestapelten Füße ein Band
        let plan_z = self.plan_z;
        let on_level = move |foot: &[(Vec3, Vec3)]| {
            plan_z.is_none_or(|z| foot.first().is_none_or(|(a, _)| (a.z - z).abs() < 1.0))
        };
        let feet = scene
            .feet()
            .chain(scene.stacked_feet())
            .filter(move |(_, _, f)| on_level(f));
        self.pick_in(feet, scene, cam, w, h, scale)
    }

    /// Was das laufende Ziehen an einer gestapelten Wand tut.
    pub fn stack_drag(&self, scene: &Scene) -> Option<StackDrag> {
        let d = self.drag.as_ref()?;
        let (wall, ..) = d.offset?;
        Some(match scene.model().stack_offset(wall) {
            Some((_, true)) => StackDrag::Ctrl,
            _ => StackDrag::Free,
        })
    }

    /// Gezogene gestapelte Wand und ihr Versatz jetzt (Anzeige am Band).
    pub fn dragged_offset(&self, scene: &Scene) -> Option<(ElementId, f64)> {
        let (wall, ..) = self.drag.as_ref()?.offset?;
        Some((wall, scene.model().stack_offset(wall)?.0))
    }

    /// Wand unter der Maus bzw. die gezogene.
    pub fn active_wall(&self) -> Option<ElementId> {
        self.drag
            .as_ref()
            .map(|d| d.wall)
            .or(self.hover)
            .or(self.link_hover)
    }

    /// Nächster Wandfuß unter der Maus unter den gegebenen Zügen.
    #[allow(clippy::too_many_arguments)]
    fn pick_in<'a>(
        &self,
        feet: impl Iterator<Item = (RunId, Option<(Vec3, Vec3)>, &'a [(Vec3, Vec3)])>,
        scene: &Scene,
        cam: &Camera,
        w: f64,
        h: f64,
        scale: f64,
    ) -> Option<ElementId> {
        let m = self.mouse?;
        let reach = PICK_PX * scale;
        let (side, f) = (side_view(cam), cam.forward());
        // Abstand im Bild, Tiefe, Segment
        let mut best: Option<(f64, f64, (RunId, usize))> = None;
        for (run, bounds, foot) in feet {
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
        // Gesperrte Wände haben keine Griffe (Paket 4 §1.7)
        best.and_then(|(_, _, (run, k))| scene.model().wall_at(run, k))
            .filter(|&e| scene.pickable(e) && !scene.model().element(e).is_some_and(|x| x.locked))
    }

    /// Tür für die Leertaste: die mit dem Griff unter der Maus, sonst die
    /// mit dem nächsten Griff.
    fn tuer_bei_maus(&self, cam: &Camera, w: f64, h: f64) -> Option<ElementId> {
        if self.tuer_griff.is_some() {
            return self.tuer_griff;
        }
        let ((x, y), z) = (self.mouse?, self.plan_z?);
        self.tueren
            .iter()
            .filter_map(|&(id, p)| {
                let (px, py) = cam.project(vec3(p.x, p.y, z), w, h)?;
                Some(((px - x).hypot(py - y), id))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    }

    /// Gewählte Tür mit Aufschlag, im Grundriss (Grundriss G7).
    fn gewaehlte_tuer(&self, scene: &Scene) -> Option<ElementId> {
        let id = self.gewaehlt.filter(|_| self.plan_z.is_some())?;
        // Nur im aktiven Geschoss und nicht ausgeblendet, wie die Tür bei
        // der Maus (Review #132 H1)
        let e = scene.model().element(id)?;
        if e.storey != scene.active_storey() || !scene.pickable(id) {
            return None;
        }
        let o = scene.model().opening(id)?;
        o.swing.map(|_| id)
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
        if self.is_dragging() {
            return false;
        }
        let frei = enabled && !self.splitting();
        // Griffe der Öffnungen bei der Maus: Türblatt vor den Enden
        let punkt = match (self.plan_z, self.mouse) {
            (Some(z), Some((x, y))) if frei => drag_point(cam, x, y, w, h, z),
            _ => None,
        };
        let tueren = punkt.map_or_else(Vec::new, |p| tuer_edit::sichtbar(scene, p));
        // Bis 60 px um die Öffnung: so erreicht die Maus das Kettensymbol
        let r_min = match (punkt, self.mouse, self.plan_z) {
            (Some(p), Some((x, y)), Some(z)) => {
                drag_point(cam, x + 60.0 * scale, y, w, h, z).map_or(0.0, |q| (q - p).length())
            }
            _ => 0.0,
        };
        let breiten = punkt.map_or_else(Vec::new, |p| breite_edit::sichtbar(scene, p, r_min));
        let (tuer_griff, breite_griff) = match (self.plan_z, self.mouse) {
            (Some(z), Some(m)) => {
                let t = tuer_edit::griff_bei(&tueren, cam, z, m, w, h, scale).map(|g| g.0);
                let b = breite_edit::griff_bei(&breiten, cam, z, m, w, h, scale);
                (t, b.filter(|_| t.is_none()))
            }
            _ => (None, None),
        };
        let griff = tuer_griff.is_some()
            || breite_griff.is_some()
            || frei && self.griff_bei(scene, cam, w, h, scale).is_some();
        let changed_tuer = tueren != self.tueren
            || tuer_griff != self.tuer_griff
            || breiten != self.breiten
            || breite_griff != self.breite_griff;
        (self.tueren, self.tuer_griff) = (tueren, tuer_griff);
        (self.breiten, self.breite_griff) = (breiten, breite_griff);
        // In Loch und Aufschlag einer Öffnung gilt der Klick ihr, nicht
        // dem Band der Wand (Grundriss G7)
        let in_oeffnung = || {
            punkt
                .and_then(|p| tuer_edit::oeffnung_bei(scene, p))
                .is_some_and(|id| scene.pickable(id))
        };
        let hover = if frei && !griff && !in_oeffnung() {
            self.pick(scene, cam, w, h, scale)
        } else {
            None
        };
        let changed = changed_tuer || hover != self.hover || griff != self.griff;
        (self.hover, self.griff) = (hover, griff);
        changed
    }

    fn update_drag(&mut self, scene: &mut Scene, cam: &Camera, w: f64, h: f64) -> bool {
        let (Some(d), Some((mx, my))) = (&self.drag, self.mouse) else {
            return false;
        };
        let Some(g) = drag_point(cam, mx, my, w, h, d.z) else {
            return false;
        };
        let raw = (g - d.start).dot(d.normal);
        let off = (raw / STEP).round() * STEP;
        if self.input.is_some() {
            // K8: nach der ersten Ziffer gibt die Maus nur noch die Seite an
            if let Some(d) = self.drag.as_mut() {
                if d.offset.is_none() && off != 0.0 {
                    d.side = off.signum();
                }
            }
            return self.apply_typed(scene);
        }
        if let Some(d) = self.drag.as_mut() {
            if off != 0.0 {
                d.side = off.signum();
            }
        }
        let Some(d) = &self.drag else {
            return false;
        };
        if let Some((wall, o0, sign)) = d.offset {
            let want = o0 + off * sign;
            let now = scene.model().stack_offset(wall).map(|o| o.0);
            let count = |s: &Scene| {
                let m = s.model();
                m.segment_of(wall)
                    .and_then(|(r, _)| m.run(r))
                    .map(|r| r.segments.len())
            };
            let before = count(scene);
            let mut ok = now == Some(want) || scene.set_offset(wall, want);
            // Kommt oder geht eine Stufe, von vorn wie bei put(): sonst
            // wandern die Nummern der Stufen mit jedem Hin und Her (Review #69)
            if ok && count(scene) != before {
                scene.rollback();
                scene.begin("Wand verschoben");
                ok = scene.model().stack_offset(wall).map(|o| o.0) == Some(want)
                    || scene.set_offset(wall, want);
            }
            if let Some(d) = self.drag.as_mut() {
                d.blocked = !ok;
            }
            return ok && now != scene.model().stack_offset(wall).map(|o| o.0);
        }
        if let Some(j) = &d.joint {
            let Some(moved) = j.moved(off) else {
                return false;
            };
            return put(scene, j.run, &moved);
        }
        match d.original.with_segment_moved_snap(d.seg, off, SNAP) {
            Some(moved) => put(scene, d.run, &moved),
            None => false,
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
        if let Some(t) = self.teilen.as_mut() {
            let (o, consumed) = t.handle(e, scene, cam, w, h);
            if o.done.is_some() || o.cancelled {
                self.teilen = None;
            }
            out.redraw = o.redraw || o.done.is_some() || o.cancelled;
            out.changed = o.done.is_some();
            out.consumed = consumed;
            out.teilen = Some(o);
            return out;
        }
        if let Some(t) = self.ecke.as_mut() {
            let (o, consumed) = t.handle(e, scene, cam, w, h);
            if o.fertig {
                self.ecke = None;
            }
            out.redraw = o.redraw || o.schritt.is_some() || o.fertig;
            out.changed = o.schritt.is_some();
            out.consumed = consumed;
            out.ecke = Some(o);
            return out;
        }
        if let Some(z) = self.ende.as_mut() {
            let o = z.handle(e, scene, cam, w, h, scale);
            if o.fertig {
                self.ende = None;
                if let Event::MouseUp { x, y, .. } = *e {
                    self.mouse = Some((x, y));
                }
                self.refresh(scene, cam, w, h, scale, enabled);
            }
            out.redraw = o.redraw;
            out.changed = o.changed;
            out.consumed = true;
            return out;
        }
        if let Some(b) = self.breite.as_mut() {
            let o = b.handle(e, scene, cam, w, h);
            if o.fertig {
                self.breite = None;
                if let Event::MouseUp { x, y, .. } = *e {
                    self.mouse = Some((x, y));
                }
                self.refresh(scene, cam, w, h, scale, enabled);
            }
            out.redraw = o.redraw;
            out.changed = o.changed;
            out.consumed = true;
            return out;
        }
        if let Some(t) = self.tuer.as_mut() {
            let o = t.handle(e, scene, cam, w, h);
            if o.fertig {
                self.tuer = None;
                if let Event::MouseUp { x, y, .. } = *e {
                    self.mouse = Some((x, y));
                }
                self.refresh(scene, cam, w, h, scale, enabled);
            }
            out.redraw = o.redraw;
            out.changed = o.changed;
            out.consumed = true;
            return out;
        }
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
                (self.tuer_griff, self.breite_griff) = (None, None);
                out.redraw |= !std::mem::take(&mut self.tueren).is_empty();
                out.redraw |= !std::mem::take(&mut self.breiten).is_empty();
            }
            Event::Wheel { .. } => {
                out.redraw = self.end_post();
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            } => {
                out.redraw = self.end_post();
                self.mouse = Some((x, y));
                self.refresh(scene, cam, w, h, scale, enabled);
                // Griff am Blatt einer Tür: Aufschlag und DIN
                if let (Some(id), Some(z)) = (self.tuer_griff, self.plan_z) {
                    out.consumed = true;
                    if let Some(l) = sk_model::edit_blocked(scene.model(), &[id]) {
                        out.locked = Some((l, id));
                        return out;
                    }
                    self.tuer = TuerZug::greifen(scene, id, z);
                    out.redraw = true;
                    return out;
                }
                // Griff an einem Ende einer Öffnung: Breite
                if let (Some((id, k, _)), Some(z)) = (self.breite_griff, self.plan_z) {
                    out.consumed = true;
                    // Gekoppelte Enden gehen mit (Review #109 B5)
                    let mut ids = vec![id];
                    ids.extend(scene.model().locked_ends(id, k).iter().map(|e| e.0));
                    if let Some(l) = sk_model::edit_blocked(scene.model(), &ids) {
                        out.locked = Some((l, id));
                        return out;
                    }
                    self.breite = BreiteZug::greifen(scene, id, k, z);
                    out.redraw = true;
                    return out;
                }
                // Griff an einem Ende der gewählten Wand (G5b)
                let griff = self.griff_bei(scene, cam, w, h, scale).filter(|_| enabled);
                if let (Some((ende, _)), Some(wand), Some(z)) = (griff, self.gewaehlt, self.plan_z)
                {
                    out.consumed = true;
                    if let Some(id) = sk_model::edit_blocked(scene.model(), &[wand]) {
                        out.locked = Some((id, wand));
                        return out;
                    }
                    self.ende = EndeZug::greifen(scene, wand, ende, z);
                    out.redraw = true;
                    return out;
                }
                let Some(wall) = self.hover else {
                    return out;
                };
                out.consumed = true;
                // Gesperrtes: beim Greifen ablehnen, nie ziehen und
                // zurückspringen
                let set = drag_set_mods(scene.model(), wall, mods.ctrl);
                if let Some(id) = sk_model::edit_blocked(scene.model(), &set) {
                    out.locked = Some((id, wall));
                    return out;
                }
                let Some((grip_run, grip_seg)) = scene.model().segment_of(wall) else {
                    return out;
                };
                let Some(grip) = scene.chain(grip_run) else {
                    return out;
                };
                let (z, normal) = (grip.base, grip.segment_normal(grip_seg));
                // Gestapelt (OG Phase 2): gelöst oder mit Strg nur der eigene
                // Versatz; gekoppelt ohne Strg zieht der Fuß der Kette
                let ctrl = mods.ctrl;
                let target = match scene.model().stack_offset(wall) {
                    Some((o, linked)) if !linked || ctrl => {
                        Some((wall, Some((wall, o, grip.outward_sign()))))
                    }
                    Some(_) => {
                        let foot = scene.model().chain_foot(wall);
                        let off = scene.model().stack_offset(foot).filter(|s| !s.1);
                        Some((foot, off.map(|(o, _)| (foot, o, grip.outward_sign()))))
                    }
                    None => Some((wall, None)),
                };
                let Some((moving, offset)) = target else {
                    return out;
                };
                let Some((run, seg)) = scene.model().segment_of(moving) else {
                    return out;
                };
                let Some(original) = scene.chain(run) else {
                    return out;
                };
                let (Some(start), Some(normal)) = (drag_point(cam, x, y, w, h, z), normal) else {
                    return out;
                };
                // Stufe: die Fuge darunter wandert längs der Flucht
                let joint = scene.model().step_joint(wall).and_then(|(run, seg)| {
                    let c = scene.chain(run)?;
                    let p = c.clean_points();
                    let dir = p[(seg + 1) % p.len()] - p[seg];
                    Some(Joint {
                        run,
                        seg,
                        sign: normal.dot(dir).signum(),
                        original: c.clone(),
                    })
                });
                self.drag = Some(Drag {
                    wall,
                    run,
                    seg,
                    start,
                    normal,
                    z,
                    original: original.clone(),
                    moving,
                    offset,
                    blocked: false,
                    side: 1.0,
                    joint,
                });
                scene.begin("Wand verschoben");
                out.redraw = true;
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if self.drag.is_some() {
                    // Getippt und ohne Enter losgelassen (A306): ein gültiger
                    // Wert gilt wie mit Enter, die Eingabe schließt; ein
                    // ungültiger bleibt mit Fehlerpunkt offen für die
                    // Nachkorrektur, die Wand steht bei der Maus
                    match self.input.as_ref().and_then(|i| i.value(0)) {
                        Some(Ok(_)) => self.input = None,
                        Some(Err(_)) => {
                            let i = self.input.take();
                            self.update_drag(scene, cam, w, h);
                            self.input = i;
                        }
                        None => {}
                    }
                }
                if let Some(d) = self.drag.take() {
                    // Ein Verlaufsschritt nur, wenn die Wand wirklich woanders steht
                    if scene
                        .chain(d.run)
                        .is_none_or(|c| c.points == d.original.points)
                    {
                        out.clicked = Some(d.wall);
                    }
                    let serial = scene.step_serial();
                    scene.commit();
                    if d.offset.is_none() && d.joint.is_none() && scene.step_serial() != serial {
                        out.joined = joined(scene, d.run, d.moving);
                    }
                    self.remember(scene, serial, d.target());
                    // Ohne Verlaufsschritt gibt es keine Nachkorrektur: eine
                    // offene (ungültige) Eingabe schließt, sonst bliebe sie
                    // unsichtbar und nähme Tasten (Review 3z)
                    if self.last.is_none() {
                        self.input = None;
                    }
                    out.consumed = true;
                    out.redraw = true;
                    self.refresh(scene, cam, w, h, scale, enabled);
                }
            }
            // Leertaste an der Tür bei der Maus, sonst an der gewählten
            // (plan.md §4.3); mit Strg oder Alt nicht (Alt+Leertaste ist
            // unter Windows das Systemmenü), gehalten nur einmal (Review #127
            // H1)
            Event::Key {
                key: tuer_edit::LEERTASTE,
                down: true,
                mods,
                repeat,
            } if self.input.is_none()
                && self.drag.is_none()
                && (!self.tueren.is_empty() || enabled && self.gewaehlte_tuer(scene).is_some())
                && !mods.ctrl
                && !mods.alt =>
            {
                let tuer = self.tuer_bei_maus(cam, w, h);
                let gewaehlt = || self.gewaehlte_tuer(scene).filter(|_| enabled);
                if let Some(id) = tuer.or_else(gewaehlt) {
                    out.consumed = true;
                    if repeat {
                        return out;
                    }
                    if let Some(l) = sk_model::edit_blocked(scene.model(), &[id]) {
                        out.gesperrt = Some((l, id));
                        return out;
                    }
                    if tuer_edit::umschlagen(scene, id, mods.shift) {
                        out.changed = true;
                        out.redraw = true;
                        self.refresh(scene, cam, w, h, scale, enabled);
                    }
                }
            }
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } if key != Key::Escape || self.input.is_some() || self.drag.is_none() => {
                if let Some(o) = self.input_key(key, mods, scene, cam, w, h, scale, enabled) {
                    out = o;
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
        if let Some(t) = &self.teilen {
            return t.helpers(scale, occlude, theme);
        }
        if let Some(t) = &self.ecke {
            return t.helpers(scene, scale, occlude, theme);
        }
        if let Some(z) = &self.ende {
            return z.helpers(scale, occlude, theme);
        }
        if let Some(t) = &self.tuer {
            return t.helpers(scene, scale, occlude, theme);
        }
        if let Some(b) = &self.breite {
            return b.helpers(scene, scale, occlude, theme);
        }
        let col = &theme.interact;
        let mut out = Vec::new();
        let lift = |p: Vec3| [p.x as f32, p.y as f32, p.z as f32 + 2.0];
        let line = |a: Vec3, b: Vec3, color, width: f32, dash: f32| Helper {
            a: lift(a),
            b: lift(b),
            color,
            width: width * scale,
            dash: dash * scale,
            pattern: sk_render::SOLID,
            occlude,
            round: false,
        };
        let dot = |p: Vec3, color, d: f32| Helper {
            a: lift(p),
            b: lift(p),
            color,
            width: d * scale,
            dash: 0.0,
            pattern: sk_render::SOLID,
            occlude,
            round: true,
        };
        let active = self.active_wall();

        if side_view(cam) {
            if let Some(d) = &self.drag {
                if let Some(&(a, b)) = d.original.outer_foot().get(d.seg) {
                    let lift = vec3(0.0, 0.0, d.original.base);
                    out.push(dot(
                        (a + b) * 0.5 + lift,
                        col.drag_ghost,
                        theme.size.drag_dot,
                    ));
                }
            }
            let segment = active.and_then(|e| scene.model().segment_of(e));
            if let Some((run, k)) = segment {
                if let Some((a, b)) = scene.foot(run).and_then(|f| f.get(k).copied()) {
                    let sz = &theme.size;
                    out.push(dot((a + b) * 0.5, col.shadow_band, sz.drag_dot_shadow));
                    out.push(dot((a + b) * 0.5, col.drag_hot, sz.drag_dot_hot));
                }
            }
            return out;
        }

        // Beim Ziehen: ursprüngliche Lage gestrichelt; an einer gestapelten
        // Wand zusätzlich die Lage des Partners darunter auf ihrer Höhe
        if let Some(d) = &self.drag {
            if let Some(&(a, b)) = d.original.outer_foot().get(d.seg) {
                // Auf Höhe des Wandfußes wie das Band (OG-Wände)
                let lift = vec3(0.0, 0.0, d.original.base);
                let (w, dash) = (theme.size.link_line, theme.size.link_dash);
                out.push(line(a + lift, b + lift, col.drag_ghost, w, dash));
            }
            let m = scene.model();
            let partner = d
                .offset
                .and_then(|(w, ..)| m.wall_below(w))
                .and_then(|w| m.segment_of(w))
                .and_then(|(run, k)| scene.foot(run).and_then(|f| f.get(k).copied()));
            if let Some((a, b)) = partner {
                let up = |p: Vec3| vec3(p.x, p.y, d.z);
                if occlude {
                    // 3D: durchgehend und über allem (wie bei gewählter Wand)
                    out.push(partner_line(up(a), up(b), scale, theme));
                } else {
                    let (w, dash) = (theme.size.link_line, theme.size.link_dash);
                    out.push(line(up(a), up(b), theme.ui.link_on.to_f32(), w, dash));
                }
            }
        }
        // Griffe an den Enden der gewählten Innenwand (Grundriss, G5b)
        if let (Some(wand), Some(z)) = (self.gewaehlt, self.plan_z) {
            for (_, p) in ende_edit::griffe(scene, wand) {
                out.extend(ende_edit::griff(p, z, scale, occlude, theme));
            }
        }
        if let Some(z) = self.plan_z {
            let breiten = self.breiten.iter().map(|g| g.2);
            for p in self.tueren.iter().map(|g| g.1).chain(breiten) {
                out.extend(ende_edit::griff(p, z, scale, occlude, theme));
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

    /// Teilrücksprung im OG (W-12): Zieht man eine Stufe, wandert die Fuge
    /// im EG darunter längs der Flucht mit; der Rücksprung wird breiter,
    /// ein Schritt im Verlauf.
    #[test]
    fn stufe_ziehen_verschiebt_die_fuge() {
        let mut s = Scene::new();
        s.edit_model("Gebäude erstellt", |m| {
            let b = m.add_building(2);
            let eg = m
                .build_from_polygon(
                    b,
                    &[
                        vec3(0.0, 0.0, 0.0),
                        vec3(0.0, 8000.0, 0.0),
                        vec3(10000.0, 8000.0, 0.0),
                        vec3(10000.0, 0.0, 0.0),
                    ],
                )
                .unwrap();
            let nord = m.wall_at(eg, 1).unwrap();
            m.split_wall(nord, &[3000.0, 6000.0]).is_ok()
        });
        let m = s.model();
        let eg = m.runs().ids().find(|r| m.run_below(*r).is_none()).unwrap();
        let og = m.runs_above(eg)[0];
        let mitte = m.wall_at(og, 2).unwrap();
        s.edit_model("Versatz", |m| m.set_offset(mitte, -1500.0).is_some());
        let stufe = s.model().wall_at(og, 2).unwrap();
        assert_eq!(s.model().step_joint(stufe), Some((eg, 2)));
        let z = s.chain(og).unwrap().base;
        let c = Camera::looking_at(
            vec3(5000.0, 4000.0, z + 30000.0),
            vec3(5000.0, 4000.0, z),
            45.0,
        );
        let mut e = WallEdit {
            plan_z: Some(z),
            ..WallEdit::default()
        };
        let m = Modifiers::default();
        let ev = |p: Vec3, kind: u8| {
            let (x, y) = at(&c, vec3(p.x, p.y, z));
            match kind {
                0 => Event::MouseMove { x, y, mods: m },
                1 => Event::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                    mods: m,
                },
                _ => Event::MouseUp {
                    button: MouseButton::Left,
                    x,
                    y,
                    mods: m,
                },
            }
        };
        let go = |e: &mut WallEdit, s: &mut Scene, p: Vec3, k: u8| {
            e.handle(&ev(p, k), s, &c, W, H, 1.0, true)
        };
        let grip = vec3(3000.0, 7250.0, 0.0);
        go(&mut e, &mut s, grip, 0);
        assert_eq!(e.hover, Some(stufe));
        go(&mut e, &mut s, grip, 1);
        go(&mut e, &mut s, vec3(2000.0, 7250.0, 0.0), 0);
        go(&mut e, &mut s, vec3(2000.0, 7250.0, 0.0), 2);
        let mm = s.model();
        assert_eq!(mm.run(eg).unwrap().points[2].x, 2000.0);
        assert_eq!(mm.run(og).unwrap().segments.len(), 8);
        assert_eq!(mm.stack_offset(mitte), Some((-1500.0, true)));
        assert!(mm.check().is_empty(), "{:?}", mm.check());
        assert_eq!(e.pill(&s).unwrap().0, "Verschiebung 1,00");
        assert!(s.undo());
        assert_eq!(s.model().run(eg).unwrap().points[2].x, 3000.0);
    }

    /// Review #69: Mit Strg ein OG-Mittelstück hin und her ziehen, die
    /// Stufen kommen und gehen; sie behalten die Nummern vom ersten Mal,
    /// ein Schritt im Verlauf.
    #[test]
    fn strg_hin_und_her_behaelt_stufennummern() {
        let mut s = Scene::new();
        s.edit_model("Gebäude erstellt", |m| {
            let b = m.add_building(2);
            let eg = m
                .build_from_polygon(
                    b,
                    &[
                        vec3(0.0, 0.0, 0.0),
                        vec3(0.0, 8000.0, 0.0),
                        vec3(10000.0, 8000.0, 0.0),
                        vec3(10000.0, 0.0, 0.0),
                    ],
                )
                .unwrap();
            let nord = m.wall_at(eg, 1).unwrap();
            m.split_wall(nord, &[3000.0, 6000.0]).is_ok()
        });
        let m = s.model();
        let eg = m.runs().ids().find(|r| m.run_below(*r).is_none()).unwrap();
        let og = m.runs_above(eg)[0];
        let mitte = m.wall_at(og, 2).unwrap();
        let z = s.chain(og).unwrap().base;
        let c = Camera::looking_at(
            vec3(5000.0, 4000.0, z + 30000.0),
            vec3(5000.0, 4000.0, z),
            45.0,
        );
        let mut e = WallEdit {
            plan_z: Some(z),
            ..WallEdit::default()
        };
        let m = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        let ev = |y: f64, kind: u8| {
            let (x, y) = at(&c, vec3(4500.0, y, z));
            match kind {
                0 => Event::MouseMove { x, y, mods: m },
                1 => Event::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                    mods: m,
                },
                _ => Event::MouseUp {
                    button: MouseButton::Left,
                    x,
                    y,
                    mods: m,
                },
            }
        };
        let go = |e: &mut WallEdit, s: &mut Scene, y: f64, k: u8| {
            e.handle(&ev(y, k), s, &c, W, H, 1.0, true);
        };
        let y0 = 7850.0;
        go(&mut e, &mut s, y0, 0);
        assert_eq!(e.hover, Some(mitte));
        go(&mut e, &mut s, y0, 1);
        let stufen = |s: &Scene| -> Vec<String> {
            let m = s.model();
            let map = m.stack_map(og);
            let r = m.run(og).unwrap();
            (0..r.segments.len())
                .filter(|k| map[*k].is_none())
                .map(|k| m.element(r.segments[k]).unwrap().number.clone())
                .collect()
        };
        go(&mut e, &mut s, y0 - 1000.0, 0);
        let erst = stufen(&s);
        assert_eq!(erst.len(), 2, "{erst:?}");
        for y in [y0, y0 - 1000.0, y0, y0 - 1500.0] {
            go(&mut e, &mut s, y, 0);
        }
        go(&mut e, &mut s, y0 - 1500.0, 2);
        assert_eq!(s.model().stack_offset(mitte), Some((-1500.0, true)));
        assert_eq!(stufen(&s), erst);
        assert!(s.model().check().is_empty(), "{:?}", s.model().check());
        assert!(s.undo());
        assert_eq!(s.model().run(og).unwrap().segments.len(), 6);
    }

    /// L zusammenschieben (W-11): Knapp vor der Flucht rastet der rechte
    /// Teil ein, die Stufe fällt weg, die Wand behält ihre Kennung und die
    /// App bekommt die Rückfrage. Über die Flucht hinaus springt die Stufe
    /// auf die andere Seite; Esc stellt alles wieder her.
    #[test]
    fn l_zusammenschieben_rastet_ein() {
        let mut s = Scene::new();
        s.add_wall(&WallChain {
            base: 0.0,
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 4000.0, 0.0),
                vec3(2000.0, 4000.0, 0.0),
                vec3(2000.0, 3000.0, 0.0),
                vec3(5000.0, 3000.0, 0.0),
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
        let mut e = WallEdit::default();
        let m = Modifiers::default();
        let run = s.model().runs().ids().next().unwrap();
        let wall = s.model().wall_at(run, 3).unwrap();
        let ev = |p: Vec3, kind: u8| {
            let (x, y) = at(&c, p);
            match kind {
                0 => Event::MouseMove { x, y, mods: m },
                1 => Event::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                    mods: m,
                },
                _ => Event::MouseUp {
                    button: MouseButton::Left,
                    x,
                    y,
                    mods: m,
                },
            }
        };
        let segs = |s: &Scene| s.model().run(run).unwrap().segments.len();
        let go = |e: &mut WallEdit, s: &mut Scene, p: Vec3, k: u8| {
            e.handle(&ev(p, k), s, &c, W, H, 1.0, true)
        };
        go(&mut e, &mut s, vec3(3500.0, 3000.0, 0.0), 0);
        assert_eq!(e.hover, Some(wall));
        go(&mut e, &mut s, vec3(3500.0, 3000.0, 0.0), 1);
        go(&mut e, &mut s, vec3(3500.0, 3500.0, 0.0), 0);
        assert_eq!(segs(&s), 6);
        // 40 mm davor: rastet bündig ein
        go(&mut e, &mut s, vec3(3500.0, 3960.0, 0.0), 0);
        assert_eq!(segs(&s), 5);
        assert_eq!(s.model().wall_at(run, 2), Some(wall));
        // Darüber hinaus: die Stufe springt nach außen
        go(&mut e, &mut s, vec3(3500.0, 4500.0, 0.0), 0);
        assert_eq!(segs(&s), 6);
        assert_eq!(s.model().wall_at(run, 3), Some(wall));
        go(&mut e, &mut s, vec3(3500.0, 3980.0, 0.0), 0);
        let out = go(&mut e, &mut s, vec3(3500.0, 3980.0, 0.0), 2);
        assert_eq!(out.joined, Some(wall));
        assert_eq!(segs(&s), 5);
        let p = &s.model().run(run).unwrap().points;
        assert!(p.iter().all(|q| (q.y - 3000.0).abs() > 1.0), "{p:?}");
        assert_eq!(e.pill(&s).unwrap().0, "Verschiebung 1,00");
        assert!(s.undo());
        assert_eq!(segs(&s), 6);
        assert_eq!(s.model().wall_at(run, 3), Some(wall));
        // Esc beim Ziehen: alles wie vorher
        go(&mut e, &mut s, vec3(3500.0, 3000.0, 0.0), 0);
        go(&mut e, &mut s, vec3(3500.0, 3000.0, 0.0), 1);
        go(&mut e, &mut s, vec3(3500.0, 3990.0, 0.0), 0);
        assert_eq!(segs(&s), 5);
        let esc = Event::Key {
            key: Key::Escape,
            down: true,
            repeat: false,
            mods: m,
        };
        e.handle(&esc, &mut s, &c, W, H, 1.0, true);
        assert_eq!(segs(&s), 6);
        assert_eq!(s.model().wall_at(run, 3), Some(wall));
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

    /// A52: Der Fuß einer gekoppelten OG-Wand hat kein Band, nur den Hinweis
    /// „gekoppelt: am EG-Wandfuß ziehen“; am EG-Fuß greift das Band.
    #[test]
    fn og_wandfuss_hat_ein_band() {
        let mut s = Scene::new();
        s.open_building_dialog();
        let eg = s
            .add_wall_as(
                &WallChain {
                    base: 0.0,
                    points: vec![
                        vec3(0.0, 0.0, 0.0),
                        vec3(10000.0, 0.0, 0.0),
                        vec3(10000.0, 8000.0, 0.0),
                        vec3(0.0, 8000.0, 0.0),
                    ],
                    closed: true,
                    ref_side: RefSide::Left,
                    layers: Vec::new(),
                    height: 2855.0,
                    joints: Default::default(),
                },
                sk_model::Category::ExteriorWall,
            )
            .unwrap();
        let og = s.model().runs_above(eg)[0];
        let c = Camera::looking_at(
            vec3(5000.0, -20000.0, 4000.0),
            vec3(5000.0, 4000.0, 2500.0),
            45.0,
        );
        let m = Modifiers::default();
        let mut e = WallEdit::default();
        let hover_at = |e: &mut WallEdit, s: &mut Scene, run| {
            let (a, b) = s.foot(run).unwrap()[0];
            let (x, y) = at(&c, (a + b) * 0.5);
            e.handle(&Event::MouseMove { x, y, mods: m }, s, &c, W, H, 1.0, true);
            e.hover
        };
        // Seit OG Phase 2 hat auch der OG-Fuß ein Band
        let og_wall = s.model().wall_at(og, 0);
        assert_eq!(hover_at(&mut e, &mut s, og), og_wall);
        let eg_wall = s.model().wall_at(eg, 0);
        assert_eq!(hover_at(&mut e, &mut s, eg), eg_wall);
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

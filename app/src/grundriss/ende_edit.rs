//! Wandende ziehen (Grundriss G5b, Jörn 11.10. 16:47, J5): Eine gewählte
//! Innenwand zeigt im Grundriss an jedem Ende ihres offenen Zuges einen
//! Griff. Ziehen fängt wie beim Zeichnen: Die Richtung springt in 45°-Schritten
//! zur Wand davor (bzw. zur eigenen Richtung), Umschalt zieht frei; Flächen
//! und freie Enden anderer Wände ziehen das Ende an. Loslassen auf einer
//! Fläche ergibt T, auf einem freien Ende L (die Anschlüsse entstehen aus den
//! Punkten). Zurück auf die Wand davor geht es nicht (kein 180°-Schritt).
//! Eine Zahl setzt die Länge des Segments; Esc schließt erst die Eingabe,
//! dann bricht es ab. Teil von [`crate::wall_edit`]; die Rechnung steht in
//! [`sk_model::Model::ende_punkte`].

use super::drag_point;
use crate::camera::Camera;
use crate::measure_input::{opens, InputOutcome, MeasureInput};
use crate::scene::Scene;
use crate::ui::MeasureKind;
use sk_math::{vec3, Vec3};
use sk_model::{Category, EckeFehler, ElementId, Fangziele, RunId};
use sk_platform::{Event, Key, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Greifabstand zum Griff (px bei 96 dpi).
pub const GRIFF_PX: f64 = 10.0;
/// Fangabstand zu Flächen und Enden anderer Wände (px bei 96 dpi), wie beim
/// Zeichnen.
const FANG_PX: f64 = 12.0;
/// Raster der Länge ohne Fang (mm).
const STEP: f64 = 10.0;
/// Winkelsprung (45°).
const SPRUNG: f64 = std::f64::consts::FRAC_PI_4;

/// Bezeichnung des Schritts im Verlauf.
pub const SCHRITT: &str = "Wandende gezogen";
/// Feldname der Eingabe.
pub const LABEL: &str = "Länge";

/// Was ein Ereignis beim Ziehen bewirkt hat.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Outcome {
    pub redraw: bool,
    /// Die Wand hat sich (live) geändert.
    pub changed: bool,
    /// Das Ziehen ist zu Ende (losgelassen, Enter oder Esc).
    pub fertig: bool,
}

/// Griffe der Wand `wand`: Enden ihres offenen Zuges, nur bei Innenwänden
/// des aktiven Geschosses (eine im Baum gewählte Wand eines anderen
/// Geschosses hat keine, Review #82 V3).
pub fn griffe(scene: &Scene, wand: ElementId) -> Vec<(usize, Vec3)> {
    let m = scene.model();
    let hier = m
        .element(wand)
        .is_some_and(|e| e.category == Category::InteriorWall && e.storey == scene.active_storey());
    if !hier {
        return Vec::new();
    }
    m.wand_enden(wand)
}

/// Griff unter der Maus: Ende und Punkt (auf der Höhe `z`).
#[allow(clippy::too_many_arguments)]
pub fn griff_bei(
    scene: &Scene,
    cam: &Camera,
    wand: ElementId,
    z: f64,
    (x, y): (f64, f64),
    w: f64,
    h: f64,
    scale: f64,
) -> Option<(usize, Vec3)> {
    griffe(scene, wand)
        .into_iter()
        .filter_map(|(e, p)| {
            let (px, py) = cam.project(vec3(p.x, p.y, z), w, h)?;
            let d = (px - x).hypot(py - y);
            (d <= GRIFF_PX * scale).then_some((d, e, p))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, e, p)| (e, p))
}

/// Laufendes Ziehen an einem Wandende.
pub struct EndeZug {
    pub wand: ElementId,
    pub run: RunId,
    ende: usize,
    /// Anderes Ende des Segments (bleibt).
    fest: Vec3,
    /// Bezug des Winkelsprungs: Richtung der Wand davor im Zug, sonst die
    /// eigene Richtung beim Greifen.
    bezug: Vec3,
    /// Gibt es die Wand davor? Dann fehlt der 180°-Schritt.
    davor: bool,
    /// Höhe des Grundrisses.
    z: f64,
    /// Punkte beim Greifen (Geisterlinie).
    original: Vec3,
    /// Zuletzt gesetztes Ende: bleibt beim Loslassen an einer abgelehnten
    /// Stelle.
    gesetzt: Vec3,
    /// Flächen und freie Enden der anderen Wände im Geschoss.
    fang: Fangziele,
    /// Ende jetzt und das gefangene Ziel (Name, Strecke der Fläche).
    jetzt: Vec3,
    ziel: Option<(String, Vec3, Vec3)>,
    pub input: Option<MeasureInput>,
    fehler: Option<EckeFehler>,
    mouse: Option<(f64, f64, bool)>,
}

impl EndeZug {
    /// Greift Ende `ende` der Wand und öffnet den Schritt.
    pub fn greifen(scene: &mut Scene, wand: ElementId, ende: usize, z: f64) -> Option<EndeZug> {
        let m = scene.model();
        let (run, seg) = m.segment_of(wand)?;
        let r = m.run(run)?;
        let pts: Vec<Vec3> = r.points.iter().map(|p| vec3(p.x, p.y, 0.0)).collect();
        let (jetzt, fest) = (pts.get(seg + ende)?, pts.get(seg + 1 - ende)?);
        let eigen = (*jetzt - *fest).normalized();
        // Wand davor: das Segment, das am festen Punkt anschließt
        let davor = if ende == 1 && seg > 0 {
            Some((pts[seg] - pts[seg - 1]).normalized())
        } else if ende == 0 && seg + 2 < pts.len() {
            Some((pts[seg + 1] - pts[seg + 2]).normalized())
        } else {
            None
        };
        let mut fang = scene.fangziele(m.element(wand)?.storey);
        let eigene = &r.segments;
        fang.flaechen.retain(|f| !eigene.contains(&f.wand));
        fang.enden.retain(|e| !eigene.contains(&e.wand));
        let z = EndeZug {
            wand,
            run,
            ende,
            fest: *fest,
            bezug: davor.unwrap_or(eigen),
            davor: davor.is_some(),
            z,
            original: *jetzt,
            gesetzt: *jetzt,
            fang,
            jetzt: *jetzt,
            ziel: None,
            input: None,
            fehler: None,
            mouse: None,
        };
        scene.begin(SCHRITT);
        Some(z)
    }

    /// Richtung vom festen Punkt zu `p`: im Winkelsprung zum Bezug, mit
    /// `frei` (Umschalt) wie gezogen. Zurück auf die Wand davor (180°)
    /// springt es nicht, sondern auf 135° (Review #82 3).
    fn richtung(&self, p: Vec3, frei: bool) -> Vec3 {
        let v = vec3(p.x - self.fest.x, p.y - self.fest.y, 0.0);
        if v.length() < 1.0 {
            return (self.jetzt - self.fest).normalized();
        }
        if frei {
            return v.normalized();
        }
        let b = self.bezug;
        let w = (b.x * v.y - b.y * v.x).atan2(b.dot(v));
        let mut a = (w / SPRUNG).round() * SPRUNG;
        if self.davor && a.abs() > std::f64::consts::PI - 1e-6 {
            a = (std::f64::consts::PI - SPRUNG).copysign(w);
        }
        let (s, c) = a.sin_cos();
        vec3(b.x * c - b.y * s, b.x * s + b.y * c, 0.0)
    }

    /// Ziel zur Maus: freies Ende vor Fläche vor Raster.
    fn zielen(&mut self, cam: &Camera, (x, y): (f64, f64), w: f64, h: f64, scale: f64, frei: bool) {
        let Some(raw) = drag_point(cam, x, y, w, h, self.z) else {
            return;
        };
        let raw = vec3(raw.x, raw.y, 0.0);
        let z = self.z;
        let px = |p: Vec3| {
            cam.project(vec3(p.x, p.y, z), w, h)
                .map_or(f64::MAX, |(a, b)| (a - x).hypot(b - y))
        };
        let reichweite = FANG_PX * scale;
        let ende = self
            .fang
            .enden
            .iter()
            .map(|e| (px(e.p), e))
            .filter(|(d, _)| *d < reichweite)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, e)) = ende {
            self.jetzt = vec3(e.p.x, e.p.y, 0.0);
            self.ziel = Some((e.name.clone(), self.jetzt, self.jetzt));
            return;
        }
        let d = self.richtung(raw, frei);
        let flaeche = self
            .fang
            .flaechen
            .iter()
            .filter_map(|f| {
                let q = if frei {
                    f.naechster(raw)
                } else {
                    schnitt(self.fest, d, f.a, f.b)?
                };
                let q = vec3(q.x, q.y, 0.0);
                ((q - self.fest).length() > 1.0).then(|| (px(q), q, f))
            })
            .filter(|(d, ..)| *d < reichweite)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, q, f)) = flaeche {
            self.jetzt = q;
            self.ziel = Some((f.name.clone(), f.a, f.b));
            return;
        }
        let l = ((raw - self.fest).dot(d) / STEP).round() * STEP;
        self.jetzt = self.fest + d * l.max(0.0);
        self.ziel = None;
    }

    /// Setzt das Ende im offenen Schritt; ein abgelehnter Punkt lässt die
    /// Wand stehen und nennt den Grund in der Pille.
    fn setzen(&mut self, scene: &mut Scene) -> bool {
        let p = self.jetzt;
        match scene.model().ende_punkte(self.wand, self.ende, p) {
            Ok((run, pts)) => {
                let vorher = scene.model().run(run).map(|r| r.points.clone());
                if vorher.as_ref() == Some(&pts) {
                    self.fehler = None;
                    self.gesetzt = p;
                    return false;
                }
                scene.set_run_points(run, &pts);
                let ok = scene.model().run(run).is_some_and(|r| r.points == pts);
                self.fehler = (!ok).then_some(EckeFehler::Ungueltig);
                if ok {
                    self.gesetzt = p;
                }
                ok
            }
            Err(e) => {
                self.fehler = Some(e);
                false
            }
        }
    }

    /// Getippte Länge in der Richtung der Maus.
    fn getippt(&mut self, scene: &mut Scene) -> bool {
        let Some(Some(Ok(v))) = self.input.as_ref().map(|i| i.value(0)) else {
            return false;
        };
        let d = (self.jetzt - self.fest).normalized();
        let d = if d.length() < 0.5 { self.bezug } else { d };
        self.jetzt = self.fest + d * v;
        self.ziel = None;
        self.setzen(scene)
    }

    fn schliessen(&mut self, scene: &mut Scene, behalten: bool, out: &mut Outcome) {
        if behalten {
            scene.commit();
        } else {
            scene.rollback();
        }
        out.fertig = true;
        out.changed = true;
        out.redraw = true;
    }

    /// Ereignis beim Ziehen; alle gehören dem Ziehen.
    pub fn handle(
        &mut self,
        e: &Event,
        scene: &mut Scene,
        cam: &Camera,
        w: f64,
        h: f64,
        scale: f64,
    ) -> Outcome {
        let mut out = Outcome::default();
        match *e {
            Event::MouseMove { x, y, mods } => {
                self.mouse = Some((x, y, mods.shift));
                self.zielen(cam, (x, y), w, h, scale, mods.shift);
                out.changed = if self.input.is_some() {
                    self.getippt(scene)
                } else {
                    self.setzen(scene)
                };
                out.redraw = true;
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                let gilt = !matches!(self.input.as_ref().and_then(|i| i.value(0)), Some(Err(_)));
                self.schliessen(scene, gilt, &mut out);
            }
            // Mit offener Eingabe schließt Esc erst sie (unten)
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } if self.input.is_none() => self.schliessen(scene, false, &mut out),
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } => {
                out.redraw = true;
                let Some(i) = self.input.as_mut() else {
                    if let Key::Char(ch) = key {
                        if opens(ch, MeasureKind::Length, mods) {
                            let mut i = MeasureInput::new(MeasureKind::Length, None);
                            i.push(ch);
                            self.input = Some(i);
                            out.changed = self.getippt(scene);
                        }
                    }
                    return out;
                };
                match i.key(key, mods) {
                    InputOutcome::Enter => {
                        self.getippt(scene);
                        self.schliessen(scene, true, &mut out);
                    }
                    InputOutcome::Emptied | InputOutcome::Escape => {
                        self.input = None;
                        if let Some((x, y, frei)) = self.mouse {
                            self.zielen(cam, (x, y), w, h, scale, frei);
                        }
                        out.changed = self.setzen(scene);
                    }
                    InputOutcome::Changed => out.changed = self.getippt(scene),
                    _ => {}
                }
            }
            _ => {}
        }
        out
    }

    /// Pille: Eingabe, sonst Ziel und Länge oder der Grund der Ablehnung
    /// mit dem, was beim Loslassen bleibt (Review #82 6).
    pub fn pill(&self) -> Option<(String, bool)> {
        if let Some(i) = &self.input {
            return Some((i.text([LABEL, ""]), true));
        }
        if let Some(f) = self.fehler {
            let bleibt = if (self.gesetzt - self.original).length() < 0.5 {
                "wie vorher".to_string()
            } else {
                let l = crate::ui::m_text((self.gesetzt - self.fest).length());
                format!("{LABEL} {l} m")
            };
            return Some((format!("{} Loslassen: {bleibt}", f.message()), false));
        }
        let l = crate::ui::m_text((self.jetzt - self.fest).length());
        Some(match &self.ziel {
            Some((name, ..)) => (format!("bis {name} · {l} m"), false),
            None => (format!("{LABEL} {l} m"), false),
        })
    }

    /// Ursprüngliche Lage gestrichelt, gefangene Fläche, Griff am Ende.
    pub fn helpers(&self, scale: f32, occlude: bool, theme: &Theme) -> Vec<Helper> {
        let col = &theme.interact;
        let z = self.z as f32 + 2.0;
        let lift = |p: Vec3| [p.x as f32, p.y as f32, z];
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
        let (w, dash) = (theme.size.link_line, theme.size.link_dash);
        let mut out = vec![line(self.fest, self.original, col.drag_ghost, w, dash)];
        if let Some((_, a, b)) = &self.ziel {
            out.push(line(*a, *b, col.shadow_band, 6.0, 0.0));
            out.push(line(*a, *b, col.drag_hot, 4.0, 0.0));
        }
        out.extend(griff(self.jetzt, self.z, scale, occlude, theme));
        out
    }
}

/// Griff als Punkt mit Schatten.
pub fn griff(p: Vec3, z: f64, scale: f32, occlude: bool, theme: &Theme) -> [Helper; 2] {
    let col = &theme.interact;
    let dot = |color, d: f32| Helper {
        a: [p.x as f32, p.y as f32, z as f32 + 2.0],
        b: [p.x as f32, p.y as f32, z as f32 + 2.0],
        color,
        width: d * scale,
        dash: 0.0,
        pattern: sk_render::SOLID,
        occlude,
        round: true,
    };
    let sz = &theme.size;
    [
        dot(col.shadow_band, sz.drag_dot_shadow),
        dot(col.drag_hot, sz.drag_dot_hot),
    ]
}

/// Schnitt des Strahls ab `p` in Richtung `d` mit der Strecke `a`–`b`.
fn schnitt(p: Vec3, d: Vec3, a: Vec3, b: Vec3) -> Option<Vec3> {
    let e = b - a;
    let k = d.x * e.y - d.y * e.x;
    if k.abs() < 1e-9 * e.length() {
        return None;
    }
    let w = a - p;
    let t = (w.x * e.y - w.y * e.x) / k;
    let u = (w.x * d.y - w.y * d.x) / k;
    (t > 0.0 && (0.0..=1.0).contains(&u)).then(|| p + d * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::{JoinKind, Model, RefSide};
    use sk_platform::Modifiers;

    const W: f64 = 1200.0;
    const H: f64 = 800.0;

    fn cam() -> Camera {
        Camera::looking_at(
            vec3(5000.0, -6000.0, 18000.0),
            vec3(5000.0, 4000.0, 0.0),
            45.0,
        )
    }

    /// Haus 10 × 8 m; Innenwand a senkrecht bei x = 2 m von 1 bis 4 m,
    /// Innenwand b waagerecht bei y = 6 m von 5 bis 8 m.
    fn haus() -> (Scene, ElementId, ElementId) {
        let (m, a, b) = modell();
        (Scene::with_model(m), a, b)
    }

    fn modell() -> (Model, ElementId, ElementId) {
        let mut m = Model::new();
        let st = m.defaults().storey;
        let aw = m.defaults().exterior_wall;
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
            .unwrap();
        let iw = m.defaults().interior_wall;
        let mut innen = |a: Vec3, b: Vec3| {
            let r = m
                .add_wall_run(
                    &[a, b],
                    false,
                    RefSide::Center,
                    st,
                    iw,
                    Category::InteriorWall,
                )
                .unwrap();
            m.wall_at(r, 0).unwrap()
        };
        let a = innen(vec3(2000.0, 1000.0, 0.0), vec3(2000.0, 4000.0, 0.0));
        let b = innen(vec3(5000.0, 6000.0, 0.0), vec3(8000.0, 6000.0, 0.0));
        (m, a, b)
    }

    fn bewege(z: &mut EndeZug, s: &mut Scene, c: &Camera, p: Vec3, shift: bool) -> Outcome {
        let (x, y) = c.project(p, W, H).unwrap();
        let mods = Modifiers {
            shift,
            ..Default::default()
        };
        z.handle(&Event::MouseMove { x, y, mods }, s, c, W, H, 1.0)
    }

    fn los(z: &mut EndeZug, s: &mut Scene, c: &Camera) -> Outcome {
        let e = Event::MouseUp {
            button: MouseButton::Left,
            x: 0.0,
            y: 0.0,
            mods: Default::default(),
        };
        z.handle(&e, s, c, W, H, 1.0)
    }

    fn enden(s: &Scene, w: ElementId) -> (Vec3, Vec3) {
        let m = s.model();
        let (run, _) = m.segment_of(w).unwrap();
        let p = &m.run(run).unwrap().points;
        (p[0], p[p.len() - 1])
    }

    fn nah(a: Vec3, b: Vec3) -> bool {
        (a.x - b.x).abs() < 0.5 && (a.y - b.y).abs() < 0.5
    }

    #[test]
    fn griffe_nur_an_innenwaenden() {
        let (s, a, _) = haus();
        assert_eq!(griffe(&s, a).len(), 2);
        let aw = s
            .model()
            .elements()
            .iter()
            .find(|(_, e)| e.category == Category::ExteriorWall)
            .unwrap()
            .0;
        assert!(griffe(&s, aw).is_empty());
        let c = cam();
        let (x, y) = c.project(vec3(2000.0, 4000.0, 0.0), W, H).unwrap();
        let g = griff_bei(&s, &c, a, 0.0, (x + 3.0, y), W, H, 1.0);
        assert_eq!(g.map(|g| g.0), Some(1), "oberes Ende");
        assert!(griff_bei(&s, &c, a, 0.0, (x + 40.0, y), W, H, 1.0).is_none());
    }

    #[test]
    fn ziehen_im_raster_und_esc() {
        let c = cam();
        let (mut s, a, _) = haus();
        let mut z = EndeZug::greifen(&mut s, a, 1, 0.0).unwrap();
        // Leicht schräg gezogen: bleibt senkrecht (Winkelsprung), Raster 10 mm
        let o = bewege(&mut z, &mut s, &c, vec3(2100.0, 4993.0, 0.0), false);
        assert!(o.changed);
        assert!(
            nah(enden(&s, a).1, vec3(2000.0, 4990.0, 0.0)),
            "{:?}",
            enden(&s, a)
        );
        assert_eq!(z.pill(), Some(("Länge 3,99 m".into(), false)));
        // Esc: alles wie vorher, kein Schritt
        let e = Event::Key {
            key: Key::Escape,
            down: true,
            mods: Default::default(),
            repeat: false,
        };
        let o = z.handle(&e, &mut s, &c, W, H, 1.0);
        assert!(o.fertig);
        assert!(nah(enden(&s, a).1, vec3(2000.0, 4000.0, 0.0)));
        assert!(s.undo_label().is_none());
    }

    #[test]
    fn loslassen_an_der_flaeche_ergibt_t() {
        let c = cam();
        let (mut s, a, _) = haus();
        let mut z = EndeZug::greifen(&mut s, a, 1, 0.0).unwrap();
        // Nahe der Innenseite der oberen Außenwand: Schnitt der Richtung mit ihr
        bewege(&mut z, &mut s, &c, vec3(2000.0, 7550.0, 0.0), false);
        let p = z.pill().unwrap().0;
        assert!(p.starts_with("bis AW-"), "{p}");
        assert!(p.contains("innen"), "{p}");
        let o = los(&mut z, &mut s, &c);
        assert!(o.fertig && o.changed);
        assert_eq!(s.undo_label(), Some(SCHRITT));
        let top = enden(&s, a).1;
        assert!(top.y > 7000.0 && top.y < 8000.0, "{top:?}");
        // T an der Außenwand: das obere Ende ist nicht mehr frei
        let m = s.model();
        assert!(m.joins().iter().any(|j| j.a == a && j.kind == JoinKind::T));
        assert!(m
            .fangziele(m.defaults().storey)
            .enden
            .iter()
            .all(|e| !nah(e.p, top)));
    }

    #[test]
    fn freies_ende_ergibt_l_und_zahl_setzt_laenge() {
        let c = cam();
        let (mut s, a, b) = haus();
        // Oberes Ende von a an das linke Ende von b: L
        let mut z = EndeZug::greifen(&mut s, a, 1, 0.0).unwrap();
        bewege(&mut z, &mut s, &c, vec3(5030.0, 6020.0, 0.0), false);
        los(&mut z, &mut s, &c);
        assert!(
            nah(enden(&s, a).1, vec3(5000.0, 6000.0, 0.0)),
            "{:?}",
            enden(&s, a)
        );
        assert_eq!(enden(&s, b).0, vec3(5000.0, 6000.0, 0.0));
        // Unteres Ende von a: Zahl setzt die Länge in der Richtung der Maus
        let mut z = EndeZug::greifen(&mut s, a, 0, 0.0).unwrap();
        bewege(&mut z, &mut s, &c, vec3(2000.0, 900.0, 0.0), false);
        for ch in ['2', ',', '5'] {
            let e = Event::Key {
                key: Key::Char(ch),
                down: true,
                mods: Default::default(),
                repeat: false,
            };
            z.handle(&e, &mut s, &c, W, H, 1.0);
        }
        assert_eq!(z.pill(), Some(("Länge 2,5 m".into(), true)));
        let e = Event::Key {
            key: Key::Enter,
            down: true,
            mods: Default::default(),
            repeat: false,
        };
        let o = z.handle(&e, &mut s, &c, W, H, 1.0);
        assert!(o.fertig);
        let (u, o_) = enden(&s, a);
        assert!(((o_ - u).length() - 2500.0).abs() < 0.5, "{u:?} {o_:?}");
    }

    /// Im Band: Griff der gewählten Wand greifen, ziehen, loslassen ergibt
    /// einen Schritt; ohne Auswahl gibt es keinen Griff.
    #[test]
    fn griff_der_gewaehlten_wand() {
        use crate::wall_edit::WallEdit;
        let c = cam();
        let (mut s, a, _) = haus();
        let mut e = WallEdit::default();
        let mods = Modifiers::default();
        let at = |p: Vec3| c.project(p, W, H).unwrap();
        let ev = |e: &mut WallEdit, s: &mut Scene, ev: Event| e.handle(&ev, s, &c, W, H, 1.0, true);
        e.stand(None, Some(0.0), None);
        let (x, y) = at(vec3(2000.0, 4000.0, 0.0));
        ev(&mut e, &mut s, Event::MouseMove { x, y, mods });
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods,
        };
        ev(&mut e, &mut s, down);
        assert!(e.ende.is_none(), "ohne Auswahl kein Griff");
        let up = |x, y| Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods: Default::default(),
        };
        ev(&mut e, &mut s, up(x, y));
        s.rollback();
        e.stand(None, Some(0.0), Some(a));
        ev(&mut e, &mut s, Event::MouseMove { x, y, mods });
        assert!(e.is_busy(), "Maus über dem Griff");
        let o = ev(&mut e, &mut s, down);
        assert!(o.consumed && e.is_dragging());
        let (x2, y2) = at(vec3(2000.0, 5000.0, 0.0));
        let o = ev(&mut e, &mut s, Event::MouseMove { x: x2, y: y2, mods });
        assert!(o.changed);
        assert_eq!(e.pill(&s), Some(("Länge 4,00 m".into(), false)));
        ev(&mut e, &mut s, up(x2, y2));
        assert!(!e.is_dragging());
        assert_eq!(s.undo_label(), Some(SCHRITT));
        assert!(nah(enden(&s, a).1, vec3(2000.0, 5000.0, 0.0)));
    }

    fn taste(z: &mut EndeZug, s: &mut Scene, c: &Camera, key: Key) -> Outcome {
        let e = Event::Key {
            key,
            down: true,
            mods: Default::default(),
            repeat: false,
        };
        z.handle(&e, s, c, W, H, 1.0)
    }

    /// Zug mit Knick: Das Ende des zweiten Segments springt nicht auf das
    /// erste zurück (kein 180°-Schritt); frei gezogen lehnt es ab, die Pille
    /// nennt, was beim Loslassen bleibt (Review #82 3 und 6).
    #[test]
    fn faltet_nicht_zurueck() {
        let c = cam();
        let (mut m, ..) = modell();
        let iw = m.defaults().interior_wall;
        let st = m.defaults().storey;
        let r = m
            .add_wall_run(
                &[
                    vec3(7000.0, 1000.0, 0.0),
                    vec3(7000.0, 4000.0, 0.0),
                    vec3(9000.0, 4000.0, 0.0),
                ],
                false,
                RefSide::Center,
                st,
                iw,
                Category::InteriorWall,
            )
            .unwrap();
        let w = m.wall_at(r, 1).unwrap();
        let mut s = Scene::with_model(m);
        let mut z = EndeZug::greifen(&mut s, w, 1, 0.0).unwrap();
        bewege(&mut z, &mut s, &c, vec3(7000.0, 2500.0, 0.0), false);
        let p = s.model().run(r).unwrap().points.clone();
        let d = (p[2] - p[1]).normalized();
        assert!(
            (d.y + std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6 && d.x.abs() > 0.7,
            "135° statt 180°: {p:?}"
        );
        // Frei gezogen auf das erste Segment: abgelehnt, es bleibt der letzte Stand
        let o = bewege(&mut z, &mut s, &c, vec3(7000.0, 2500.0, 0.0), true);
        assert!(!o.changed);
        let pille = z.pill().unwrap().0;
        assert!(
            pille.starts_with(EckeFehler::Faltet.message()) && pille.contains("Loslassen: Länge"),
            "{pille}"
        );
        assert_eq!(s.model().run(r).unwrap().points, p);
        los(&mut z, &mut s, &c);
        assert_eq!(s.model().run(r).unwrap().points, p);
        assert_eq!(s.undo_label(), Some(SCHRITT));
    }

    /// Esc schließt erst die Eingabe, das zweite bricht das Ziehen ab.
    #[test]
    fn esc_schliesst_erst_die_eingabe() {
        let c = cam();
        let (mut s, a, _) = haus();
        let mut z = EndeZug::greifen(&mut s, a, 1, 0.0).unwrap();
        bewege(&mut z, &mut s, &c, vec3(2000.0, 5000.0, 0.0), false);
        taste(&mut z, &mut s, &c, Key::Char('2'));
        assert!(z.input.is_some());
        let o = taste(&mut z, &mut s, &c, Key::Escape);
        assert!(!o.fertig && z.input.is_none());
        assert!(
            nah(enden(&s, a).1, vec3(2000.0, 5000.0, 0.0)),
            "wieder an der Maus"
        );
        let o = taste(&mut z, &mut s, &c, Key::Escape);
        assert!(o.fertig);
        assert!(nah(enden(&s, a).1, vec3(2000.0, 4000.0, 0.0)));
        assert!(s.undo_label().is_none());
    }

    /// Eine Wand eines anderen Geschosses (etwa im Baum gewählt) zeigt im
    /// aktiven Geschoss keine Griffe (Review #82 V3).
    #[test]
    fn keine_griffe_aus_anderem_geschoss() {
        let (mut m, ..) = modell();
        let b = m.add_building(0);
        let st = m.ground_of(Some(b)).unwrap();
        let iw = m.defaults().interior_wall;
        let r = m
            .add_wall_run(
                &[vec3(20000.0, 0.0, 0.0), vec3(23000.0, 0.0, 0.0)],
                false,
                RefSide::Center,
                st,
                iw,
                Category::InteriorWall,
            )
            .unwrap();
        let w = m.wall_at(r, 0).unwrap();
        let s = Scene::with_model(m);
        assert_ne!(s.active_storey(), st);
        assert!(griffe(&s, w).is_empty());
    }
}

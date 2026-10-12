//! Aufschlag und DIN-Hand einer Innentür am Griff (Grundriss, Jörn 12.10.
//! 06:36–06:38): Steht die Maus bei einer Tür des aktiven Geschosses, zeigt
//! sie einen runden Griff am freien Ende ihres offenen Blatts. Beim Ziehen
//! bestimmt die Wandseite, auf der der Griff steht, den Raum, in den die Tür
//! aufschlägt, und die Hälfte der Öffnung darüber den Bandpfosten (und damit
//! DIN links oder rechts, DIN 107). Die Tür geht live mit, Loslassen ist ein
//! Schritt, Esc bricht ab. Ohne Ziehen spiegelt die Leertaste an der Tür bei
//! der Maus die Bandseite, Umschalt+Leertaste schlägt sie in den anderen
//! Raum auf (plan.md §4.3), je ein Schritt. Teil von [`crate::wall_edit`].

use super::drag_point;
use super::ende_edit::{self, Outcome, GRIFF_PX};
use crate::camera::Camera;
use crate::scene::Scene;
use sk_math::{vec3, Vec3};
use sk_model::oeffnung::{Din, FaceSide, Swing};
use sk_model::{ElementId, ElementKind, Model, RunId, WallChain};
use sk_platform::{Event, Key, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Bezeichnung des Schritts im Verlauf.
pub const SCHRITT: &str = "Türaufschlag geändert";
/// Leertaste (Code des Betriebssystems, wie [`Key::Other`]).
pub const LEERTASTE: Key = Key::Other(0x20);
/// Der Griff erscheint, wenn die Maus näher als so viele Türbreiten an der
/// Türmitte steht (das offene Blattende liegt bei gut einer Breite).
const NAH: f64 = 1.25;
/// Vorfilter der Griffe am Pfad der Tür (mm): Das Blatt liegt auf der
/// Rohbauflucht, höchstens eine Wanddicke neben dem Pfad (Review #99 B1).
const DICKE: f64 = 1000.0;

fn flat(p: Vec3) -> Vec3 {
    vec3(p.x, p.y, 0.0)
}

/// Wandkörper je Zug (aus dem Zwischenspeicher der Szene).
type Koerper<'a> = &'a dyn Fn(RunId) -> Option<WallChain>;

/// Griff der Tür `id` (z = 0) und ihre Mitte und Breite; `None` ohne
/// Aufschlag.
fn griff_der_tuer(m: &Model, id: ElementId, k: Koerper) -> Option<(Vec3, Vec3, f64)> {
    let l = m.door_leaf_in(id, k)?;
    let w = (l.u.1 - l.u.0).abs();
    let mitte = (l.line.0 + l.line.1) * 0.5;
    Some((flat(l.hinge + l.swing_into * w), flat(mitte), w))
}

/// Griffe der Türen des aktiven Geschosses, bei denen die Maus (Punkt `p`
/// im Grundriss) steht.
pub fn sichtbar(scene: &Scene, p: Vec3) -> Vec<(ElementId, Vec3)> {
    let m = scene.model();
    let st = scene.active_storey();
    let p = flat(p);
    m.elements()
        .iter()
        .filter_map(|(id, e)| {
            let ElementKind::Opening(o) = &e.kind else {
                return None;
            };
            if e.storey != st || o.swing.is_none() || o.path.len() != 2 {
                return None;
            }
            // Erst am Pfad filtern, das Blatt nur für Türen bei der Maus
            let (a, b) = (flat(o.path[0]), flat(o.path[1]));
            if (p - (a + b) * 0.5).length() > NAH * (b - a).length() + DICKE {
                return None;
            }
            let (g, mitte, w) = griff_der_tuer(m, id, &|r| scene.koerper(r))?;
            ((p - mitte).length() <= NAH * w).then_some((id, g))
        })
        .collect()
}

/// Griff unter der Maus (Ansichtspixel `x`, `y`) aus `griffe`, gezeichnet
/// in Höhe `z`.
#[allow(clippy::too_many_arguments)]
pub fn griff_bei(
    griffe: &[(ElementId, Vec3)],
    cam: &Camera,
    z: f64,
    (x, y): (f64, f64),
    w: f64,
    h: f64,
    scale: f64,
) -> Option<(ElementId, Vec3)> {
    griffe
        .iter()
        .filter_map(|&(id, p)| {
            let (px, py) = cam.project(vec3(p.x, p.y, z), w, h)?;
            let d = (px - x).hypot(py - y);
            (d <= GRIFF_PX * scale).then_some((d, id, p))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id, p)| (id, p))
}

/// Lage der Tür beim Greifen: Anfang und Richtung des Pfads, seine Länge,
/// ein Punkt der Wandmitte und der Zug. Beim Ziehen ändert sich nur der
/// Aufschlag, die Lage bleibt (Review #99 B1).
#[derive(Clone, Copy, Debug)]
struct Lage {
    a: Vec3,
    along: Vec3,
    len: f64,
    mitte: Vec3,
    run: RunId,
}

fn lage(m: &Model, id: ElementId, k: Koerper) -> Option<Lage> {
    let o = m.opening(id)?;
    let l = m.door_leaf_in(id, k)?;
    let (a, b) = (flat(*o.path.first()?), flat(*o.path.last()?));
    let len = (b - a).length();
    if len < 1e-9 {
        return None;
    }
    let c = k(l.run)?;
    let f = m.outer_face_in(l.run, l.seg, &c)?;
    // Der Pfad liegt auf der Bezugsfläche, die Wand dahinter um ihre Dicke
    Some(Lage {
        a,
        along: (b - a) / len,
        len,
        mitte: a - f.n * (c.thickness() / 2.0),
        run: l.run,
    })
}

/// Aufschlag der Tür `id`, wenn ihr Griff bei `p` (Grundriss) steht: die
/// Wandseite von `p` zur Wandmitte gibt den Raum, die Hälfte der Lichte den
/// Bandpfosten. Die Seite gilt zur Pfadrichtung wie in [`Swing`].
#[cfg(test)]
fn swing_bei(m: &Model, id: ElementId, p: Vec3) -> Option<Swing> {
    swing_aus(&lage(m, id, &|r| m.chain(r))?, p)
}

fn swing_aus(l: &Lage, p: Vec3) -> Option<Swing> {
    let Lage {
        a,
        along,
        len,
        mitte,
        ..
    } = *l;
    let p = flat(p);
    let rechts = FaceSide::Right.normal(along);
    let side = if (p - mitte).dot(rechts) > 0.0 {
        FaceSide::Right
    } else {
        FaceSide::Left
    };
    let band_am_anfang = (p - a).dot(along) < len / 2.0;
    let din = if (side == FaceSide::Right) == band_am_anfang {
        Din::Links
    } else {
        Din::Rechts
    };
    Some(Swing { side, din })
}

/// Leertaste an der Tür `id`: die Bänder an den anderen Pfosten, mit
/// `raum` der Aufschlag in den anderen Raum bei denselben Bändern
/// ([`Swing::other_hand`], [`Swing::flipped`]); ein Schritt. `false`, wenn
/// die Tür keinen Aufschlag hat oder das Modell ihn ablehnt.
pub fn umschlagen(scene: &mut Scene, id: ElementId, raum: bool) -> bool {
    let m = scene.model();
    let Some(alt) = m.opening(id).and_then(|o| o.swing) else {
        return false;
    };
    let Some(l) = m.door_leaf_in(id, &|r| scene.koerper(r)) else {
        return false;
    };
    let neu = if raum {
        alt.flipped()
    } else {
        alt.other_hand()
    };
    scene.begin(SCHRITT);
    if scene.set_swing(id, neu, l.run) {
        scene.commit();
        true
    } else {
        scene.rollback();
        false
    }
}

/// Laufendes Ziehen am Griff einer Tür.
pub struct TuerZug {
    pub id: ElementId,
    alt: Swing,
    jetzt: Swing,
    /// Höhe des Grundrisses.
    z: f64,
    lage: Lage,
    /// Griff am Blattende, neu nur, wenn der Aufschlag wechselt.
    griff: Option<Vec3>,
}

impl TuerZug {
    /// Greift den Griff der Tür `id` und öffnet den Schritt.
    pub fn greifen(scene: &mut Scene, id: ElementId, z: f64) -> Option<TuerZug> {
        let m = scene.model();
        let k = |r| scene.koerper(r);
        let alt = m.opening(id)?.swing?;
        let lage = lage(m, id, &k)?;
        let griff = griff_der_tuer(m, id, &k).map(|g| g.0);
        scene.begin(SCHRITT);
        Some(TuerZug {
            id,
            alt,
            jetzt: alt,
            z,
            lage,
            griff,
        })
    }

    /// Maus und Esc beim Ziehen.
    pub fn handle(
        &mut self,
        e: &Event,
        scene: &mut Scene,
        cam: &Camera,
        w: f64,
        h: f64,
    ) -> Outcome {
        let mut out = Outcome::default();
        match *e {
            Event::MouseMove { x, y, .. } => {
                let s = drag_point(cam, x, y, w, h, self.z).and_then(|p| swing_aus(&self.lage, p));
                if let Some(s) = s.filter(|s| *s != self.jetzt) {
                    if scene.set_swing(self.id, s, self.lage.run) {
                        self.jetzt = s;
                        let k = |r| scene.koerper(r);
                        self.griff = griff_der_tuer(scene.model(), self.id, &k).map(|g| g.0);
                        out.changed = true;
                        out.redraw = true;
                    }
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if self.jetzt == self.alt {
                    scene.rollback();
                } else {
                    scene.commit();
                }
                out.fertig = true;
                out.redraw = true;
            }
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } => {
                scene.rollback();
                out.changed = self.jetzt != self.alt;
                out.fertig = true;
                out.redraw = true;
            }
            _ => {}
        }
        out
    }

    /// Zug der Tür: Er wird beim Ziehen live gezeichnet.
    pub fn run(&self) -> RunId {
        self.lage.run
    }

    /// Der Griff am freien Blattende, wie die Tür gerade steht.
    pub fn helpers(&self, _scene: &Scene, scale: f32, occlude: bool, theme: &Theme) -> Vec<Helper> {
        self.griff
            .map(|g| ende_edit::griff(g, self.z, scale, occlude, theme).to_vec())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
    use sk_model::{Category, LevelRef, RefSide};

    /// Prüfhaus 10 × 8 m mit Innenwand bei x = 5 m (Zeichenrichtung +y)
    /// und einer Tür darin, 88,5 cm breit um y = 3 m.
    fn haus() -> (Model, ElementId) {
        let mut m = Model::with_seed(412);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let st = m.run(eg).unwrap().storey;
        let set = m.defaults().interior_wall;
        let iw = m
            .add_wall_run(
                &[vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)],
                false,
                RefSide::Center,
                st,
                set,
                Category::InteriorWall,
            )
            .unwrap();
        let f = m.outer_face(iw, 0).unwrap();
        let u = f.u_of(vec3(5000.0, 3000.0, 0.0));
        let o = Opening {
            kind: OpeningKind::Door,
            path: vec![f.at(u - 442.5, 0.0), f.at(u + 442.5, 0.0)],
            bottom: HeightRef::Level(LevelRef::bottom(st)),
            top: HeightRef::Line(st, HeightLine::Door),
            reveal: None,
            install: Default::default(),
            swing: Some(Swing {
                side: FaceSide::Right,
                din: Din::Links,
            }),
            lock: Default::default(),
        };
        let id = m.add_opening(o).unwrap();
        (m, id)
    }

    /// Der Griff steht am freien Ende des offenen Blatts: über dem
    /// Bandpfosten, eine Türbreite im Raum des Aufschlags.
    #[test]
    fn griff_am_blattende() {
        let (m, id) = haus();
        let l = m.door_leaf(id).unwrap();
        let (g, mitte, w) = griff_der_tuer(&m, id, &|r| m.chain(r)).unwrap();
        assert_eq!(w, 885.0);
        assert!((g - flat(l.hinge) - l.swing_into * w).length() < 1e-6);
        assert!(((mitte.y) - 3000.0).abs() < 1e-6, "{mitte:?}");
    }

    /// Alle vier Lagen: die Wandseite des Griffs gibt den Raum, die Hälfte
    /// der Lichte den Bandpfosten; das Modell zeichnet das Blatt dann dort.
    #[test]
    fn vier_lagen_am_griff() {
        let (mut m, id) = haus();
        for (p, x_into, y_band) in [
            (vec3(5800.0, 2700.0, 0.0), 1.0, 2557.5),
            (vec3(5800.0, 3300.0, 0.0), 1.0, 3442.5),
            (vec3(4200.0, 2700.0, 0.0), -1.0, 2557.5),
            (vec3(4200.0, 3300.0, 0.0), -1.0, 3442.5),
        ] {
            let s = swing_bei(&m, id, p).unwrap();
            let mut o = m.opening(id).unwrap().clone();
            o.swing = Some(s);
            assert!(m.set_opening(id, o));
            let l = m.door_leaf(id).unwrap();
            assert!((l.swing_into.x - x_into).abs() < 1e-9, "{p:?}: {l:?}");
            assert!((l.hinge.y - y_band).abs() < 1e-6, "{p:?}: {l:?}");
            let (g, ..) = griff_der_tuer(&m, id, &|r| m.chain(r)).unwrap();
            // Der Griff landet in der Hälfte und auf der Seite der Maus
            assert!((g.x - 5000.0) * (p.x - 5000.0) > 0.0);
            assert!((g.y - 3000.0) * (p.y - 3000.0) > 0.0);
        }
        // Aus dem Raum bei +x gesehen (Blick nach −x) liegt das Band bei
        // −y links: DIN links
        let s = swing_bei(&m, id, vec3(5800.0, 2700.0, 0.0)).unwrap();
        assert_eq!(s.din, Din::Links);
        let s = swing_bei(&m, id, vec3(5800.0, 3300.0, 0.0)).unwrap();
        assert_eq!(s.din, Din::Rechts);
    }

    /// Ziehen am Griff: die Tür geht live mit, Esc stellt sie zurück,
    /// Loslassen ist ein Schritt, den Rückgängig wieder nimmt.
    #[test]
    fn ziehen_esc_und_schritt() {
        const W: f64 = 1200.0;
        const H: f64 = 800.0;
        let cam = Camera::looking_at(
            vec3(5000.0, 3000.0, 20000.0),
            vec3(5000.0, 3001.0, 0.0),
            45.0,
        );
        let (m, id) = haus();
        let mut s = Scene::with_model(m);
        let alt = s.model().opening(id).unwrap().swing.unwrap();
        let ziel = vec3(4200.0, 3300.0, 0.0);
        let soll = swing_bei(s.model(), id, ziel).unwrap();
        assert_ne!(soll, alt);
        let (x, y) = cam.project(ziel, W, H).unwrap();
        let mods = Default::default();
        let zug = |s: &mut Scene| TuerZug::greifen(s, id, 0.0).unwrap();
        let swing = |s: &Scene| s.model().opening(id).unwrap().swing.unwrap();
        // Esc: zurück
        let mut t = zug(&mut s);
        // Die Zeichnung geht live mit: Der Zug ist neu gerechnet und das
        // Blatt im Körper steht auf der neuen Seite (Jörn 12.10. 14:44)
        let run = t.run();
        let blatt = |s: &Scene| {
            let c = s.chain(run).unwrap();
            let h = c.joints.holes.iter().find(|h| h.opening == Some(id));
            h.and_then(|h| h.leaf).map(|l| (l.side, l.hinge_lo))
        };
        let vorher = blatt(&s);
        assert!(vorher.is_some());
        let o = t.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        assert!(o.changed);
        assert_eq!(swing(&s), soll);
        assert_ne!(blatt(&s), vorher);
        let esc = Event::Key {
            key: Key::Escape,
            down: true,
            mods,
            repeat: false,
        };
        assert!(t.handle(&esc, &mut s, &cam, W, H).fertig);
        assert_eq!(swing(&s), alt);
        assert_eq!(blatt(&s), vorher, "Esc rechnet den Zug zurück");
        // Loslassen: ein Schritt
        let mut t = zug(&mut s);
        t.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        let los = Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods,
        };
        assert!(t.handle(&los, &mut s, &cam, W, H).fertig);
        assert_eq!(swing(&s), soll);
        assert_eq!(s.undo_label(), Some(SCHRITT));
        assert!(s.undo());
        assert_eq!(swing(&s), alt);
    }

    /// Leertaste an der Tür bei der Maus: die Bänder an den anderen
    /// Pfosten, mit Umschalt der Aufschlag in den anderen Raum an denselben
    /// Bändern; je ein Schritt, den Rückgängig wieder nimmt.
    #[test]
    fn leertaste_an_der_tuer() {
        const W: f64 = 1200.0;
        const H: f64 = 800.0;
        let cam = Camera::looking_at(
            vec3(5000.0, 3000.0, 20000.0),
            vec3(5000.0, 3001.0, 0.0),
            45.0,
        );
        let (m, id) = haus();
        let mut s = Scene::with_model(m);
        let mut e = crate::wall_edit::WallEdit {
            plan_z: Some(0.0),
            ..Default::default()
        };
        let mods = sk_platform::Modifiers::default();
        let (x, y) = cam.project(vec3(5300.0, 3000.0, 0.0), W, H).unwrap();
        e.handle(
            &Event::MouseMove { x, y, mods },
            &mut s,
            &cam,
            W,
            H,
            1.0,
            true,
        );
        let swing = |s: &Scene| s.model().opening(id).unwrap().swing.unwrap();
        let blatt = |s: &Scene| s.model().door_leaf(id).unwrap();
        let taste = |shift: bool| Event::Key {
            key: LEERTASTE,
            down: true,
            mods: sk_platform::Modifiers { shift, ..mods },
            repeat: false,
        };
        let (alt, l0) = (swing(&s), blatt(&s));
        let o = e.handle(&taste(false), &mut s, &cam, W, H, 1.0, true);
        assert!(o.consumed && o.changed);
        assert_eq!(swing(&s), alt.other_hand());
        let l1 = blatt(&s);
        assert!((l1.hinge.y - l0.hinge.y).abs() > 800.0, "anderer Pfosten");
        assert_eq!(l1.swing_into, l0.swing_into, "derselbe Raum");
        assert_eq!(s.undo_label(), Some(SCHRITT));
        let o = e.handle(&taste(true), &mut s, &cam, W, H, 1.0, true);
        assert!(o.changed);
        assert_eq!(swing(&s), alt.other_hand().flipped());
        let l2 = blatt(&s);
        assert!((l2.hinge.y - l1.hinge.y).abs() < 1e-6, "derselbe Pfosten");
        assert!(
            (l2.swing_into + l1.swing_into).length() < 1e-9,
            "anderer Raum"
        );
        assert!(s.undo() && s.undo());
        assert_eq!(swing(&s), alt);
        // Weit weg von der Tür tut die Taste nichts
        let (x, y) = cam.project(vec3(8000.0, 6000.0, 0.0), W, H).unwrap();
        e.handle(
            &Event::MouseMove { x, y, mods },
            &mut s,
            &cam,
            W,
            H,
            1.0,
            true,
        );
        assert!(
            !e.handle(&taste(false), &mut s, &cam, W, H, 1.0, true)
                .changed
        );
        assert_eq!(swing(&s), alt);
    }
}

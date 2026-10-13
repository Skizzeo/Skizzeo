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
/// im Grundriss) steht; ausgeblendete Türen haben keinen.
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
            if e.storey != st || o.swing.is_none() || o.path.len() != 2 || !scene.pickable(id) {
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

/// So weit reicht ein Aufschlag höchstens über den Wandkörper hinaus (mm):
/// Vortest beim Wählen.
const AUFSCHLAG: f64 = 3000.0;

/// Öffnung des aktiven Geschosses unter dem Punkt `p` des Grundrisses
/// (Grundriss G7): in ihrem Loch zwischen den Wandfluchten oder, bei einer
/// Tür, im Viertelkreis ihres Aufschlags. Nur Öffnungen, die die
/// Schnitthöhe trifft, also die gezeichneten; liegen Aufschläge
/// übereinander, gewinnt das nächste Band.
pub fn oeffnung_bei(scene: &Scene, p: Vec3) -> Option<ElementId> {
    let m = scene.model();
    let (st, cut, p) = (scene.active_storey(), scene.plan_cut(), flat(p));
    let weit = |(lo, hi): (Vec3, Vec3)| {
        p.x < lo.x - AUFSCHLAG
            || p.x > hi.x + AUFSCHLAG
            || p.y < lo.y - AUFSCHLAG
            || p.y > hi.y + AUFSCHLAG
    };
    let mut bogen: Option<(f64, ElementId)> = None;
    for (run, c, quader) in scene.zuege() {
        if quader.is_some_and(weit) || m.run(run).is_none_or(|r| r.storey != st) {
            continue;
        }
        let geschnitten = |z: (f64, f64)| z.0 < cut && cut < z.1;
        for (h, q) in c.hole_quads() {
            if let Some(id) = h.opening.filter(|_| geschnitten(h.z)) {
                if super::ecke_edit::innen(&q, p) {
                    return Some(id);
                }
            }
        }
        for (h, s) in c.door_swings() {
            let (Some(id), true) = (h.opening, geschnitten(h.z)) else {
                continue;
            };
            let (v, w) = (p - s.band, s.offen.length());
            let drin = v.dot(s.offen) >= 0.0 && v.dot(s.zu) >= 0.0 && v.length() <= w;
            if drin && bogen.is_none_or(|b| v.length() < b.0) {
                bogen = Some((v.length(), id));
            }
        }
    }
    bogen.map(|b| b.1)
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
        let id = haus_in(&mut m);
        (m, id)
    }

    /// Wie [`haus`], in ein Modell gebaut (in einem Schritt der Szene
    /// entstehen dabei die Räume).
    fn haus_in(m: &mut Model) -> ElementId {
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
        m.add_opening(o).unwrap()
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
        let (tx, ty) = cam.project(vec3(5300.0, 3000.0, 0.0), W, H).unwrap();
        e.handle(
            &Event::MouseMove { x: tx, y: ty, mods },
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
        // Zurück an der Tür (Review #127 H1): gehalten kippt sie nicht
        // weiter, mit Strg oder Alt gehört die Taste nicht der Tür
        e.handle(
            &Event::MouseMove { x: tx, y: ty, mods },
            &mut s,
            &cam,
            W,
            H,
            1.0,
            true,
        );
        let gehalten = Event::Key {
            key: LEERTASTE,
            down: true,
            mods,
            repeat: true,
        };
        let o = e.handle(&gehalten, &mut s, &cam, W, H, 1.0, true);
        assert!(o.consumed && !o.changed);
        for m in [
            sk_platform::Modifiers { ctrl: true, ..mods },
            sk_platform::Modifiers { alt: true, ..mods },
        ] {
            let k = Event::Key {
                key: LEERTASTE,
                down: true,
                mods: m,
                repeat: false,
            };
            assert!(!e.handle(&k, &mut s, &cam, W, H, 1.0, true).changed);
        }
        assert_eq!(swing(&s), alt);
        // Gesperrt (B1): keine Änderung, die Sperre geht an die App
        s.edit_model("Gesperrt", |m| {
            m.set_locked(&[id], true);
            true
        });
        let o = e.handle(&taste(false), &mut s, &cam, W, H, 1.0, true);
        assert!(o.consumed && !o.changed);
        assert_eq!(o.gesperrt, Some((id, id)));
        assert_eq!(o.locked, None);
        assert_eq!(swing(&s), alt);
        let karte =
            crate::delete::locked_card(s.model(), id, Some((id, crate::delete::Act::Field)));
        assert_eq!(karte.len(), 1);
        assert!(karte[0].ends_with(" ist gesperrt."), "{karte:?}");
    }

    /// Eine ausgeblendete Tür zeigt keinen Griff und kippt nicht, auch
    /// nicht gewählt mit der Maus weit weg (Review #132 H1).
    #[test]
    fn ausgeblendete_tuer_bleibt() {
        const W: f64 = 1200.0;
        const H: f64 = 800.0;
        let cam = Camera::looking_at(
            vec3(5000.0, 3000.0, 20000.0),
            vec3(5000.0, 3001.0, 0.0),
            45.0,
        );
        let (m, id) = haus();
        let mut s = Scene::with_model(m);
        let mut e = crate::wall_edit::WallEdit::default();
        e.stand(None, Some(0.0), Some(id));
        let mods = sk_platform::Modifiers::default();
        let geh = |e: &mut crate::wall_edit::WallEdit, s: &mut Scene, p: Vec3| {
            let (x, y) = cam.project(p, W, H).unwrap();
            e.handle(&Event::MouseMove { x, y, mods }, s, &cam, W, H, 1.0, true);
        };
        let taste = Event::Key {
            key: LEERTASTE,
            down: true,
            mods,
            repeat: false,
        };
        let swing = |s: &Scene| s.model().opening(id).unwrap().swing.unwrap();
        let alt = swing(&s);
        let mut v = s.model().visibility().clone();
        v.hidden.insert(s.model().element(id).unwrap().guid);
        assert!(s.set_visibility(v));
        for p in [vec3(5300.0, 3000.0, 0.0), vec3(9000.0, 7000.0, 0.0)] {
            geh(&mut e, &mut s, p);
            let o = e.handle(&taste, &mut s, &cam, W, H, 1.0, true);
            assert!(!o.changed, "bei {p:?}");
            assert_eq!(swing(&s), alt);
        }
        // Wieder eingeblendet kippt sie gewählt mit der Maus weit weg
        let mut v = s.model().visibility().clone();
        v.hidden.clear();
        assert!(s.set_visibility(v));
        geh(&mut e, &mut s, vec3(9000.0, 7000.0, 0.0));
        assert!(e.handle(&taste, &mut s, &cam, W, H, 1.0, true).changed);
        assert_eq!(swing(&s), alt.other_hand());
    }

    /// Grundriss G7: Ein Klick in Loch oder Aufschlag wählt die Tür, nicht
    /// Wand oder Decke, und das Band der Wand meldet sich dort nicht. Das
    /// Paneel zeigt Art, Breite, Höhe, Anschlag mit dem Raum und die Wand,
    /// die Auswahl ihren Umriss. Die Leertaste schlägt die gewählte Tür auch
    /// ohne Maus an ihr um, Löschen nimmt sie aus der Wand.
    #[test]
    fn tuer_waehlen() {
        use crate::selection;
        use crate::ui::ViewKind;
        const W: f64 = 1200.0;
        const H: f64 = 800.0;
        let cam = Camera::looking_at(
            vec3(5000.0, 3000.0, 20000.0),
            vec3(5000.0, 3001.0, 0.0),
            45.0,
        );
        // Im Schritt der Szene entstehen die Räume
        let mut s = Scene::with_model(Model::with_seed(412));
        let mut tuer = None;
        assert!(s.edit_model("Haus", |m| {
            tuer = Some(haus_in(m));
            true
        }));
        let id = tuer.unwrap();
        assert_eq!(s.model().spaces(s.active_storey()).len(), 2);
        let z = s.plan_cut();
        let bild = |p: Vec3| cam.project(vec3(p.x, p.y, z), W, H).unwrap();
        let wahl = |s: &mut Scene, p: Vec3| {
            let (x, y) = bild(p);
            selection::pick_at(s, &cam, ViewKind::Plan, None, x, y, W, H)
        };
        let l = s.model().door_leaf(id).unwrap();
        let gegen = if (l.line.0 - l.hinge).length() > 1.0 {
            l.line.0
        } else {
            l.line.1
        };
        let zu = (gegen - l.hinge).normalized();
        let band = flat(l.hinge);
        assert_eq!(wahl(&mut s, vec3(5000.0, 3000.0, 0.0)), Some(id), "im Loch");
        let im_bogen = band + l.swing_into * 400.0 + zu * 400.0;
        assert_eq!(wahl(&mut s, im_bogen), Some(id), "im Aufschlag");
        let wand = wahl(&mut s, vec3(5000.0, 5500.0, 0.0));
        assert!(wand.is_some() && wand != Some(id), "daneben die Wand");
        let drueben = band - l.swing_into * 400.0 + zu * 400.0;
        assert_ne!(wahl(&mut s, drueben), Some(id), "nicht im anderen Raum");
        let draussen = band + l.swing_into * 400.0 - zu * 400.0;
        assert_ne!(wahl(&mut s, draussen), Some(id), "nicht hinter dem Band");
        // Das Band der Wand (am Fuß, z = 0) nimmt den Klick im Loch nicht
        let mods = sk_platform::Modifiers::default();
        let mut e = crate::wall_edit::WallEdit::default();
        e.stand(None, Some(0.0), None);
        let zeige = |e: &mut crate::wall_edit::WallEdit, s: &mut Scene, p: Vec3| {
            let (x, y) = cam.project(p, W, H).unwrap();
            e.handle(&Event::MouseMove { x, y, mods }, s, &cam, W, H, 1.0, true);
            e.active_wall()
        };
        assert_eq!(zeige(&mut e, &mut s, vec3(5000.0, 3000.0, 0.0)), None);
        assert!(zeige(&mut e, &mut s, vec3(5000.0, 5500.0, 0.0)).is_some());
        // Paneel
        let wert = |s: &Scene, k: &str| {
            let p = selection::props(s, id).unwrap();
            p.values.iter().find(|v| v.0 == k).map(|v| v.1.clone())
        };
        assert_eq!(wert(&s, "Art").as_deref(), Some("Türöffnung"));
        assert_eq!(wert(&s, "Breite").as_deref(), Some("0,885 m"));
        assert!(wert(&s, "Höhe (Rohbau)").is_some_and(|h| h.ends_with(" m")));
        assert_eq!(wert(&s, "Brüstung (Rohbau)"), None, "Tür ab Rohdecke");
        // Der Raum steht als Detailzeile unter dem Anschlag (Einstellungen #132 B1)
        let anschlag = |s: &Scene| {
            let v = selection::props(s, id).unwrap().values;
            let i = v.iter().position(|v| v.0 == "Anschlag").unwrap();
            (v[i].1.clone(), v.get(i + 1).cloned())
        };
        let (din, vorher) = anschlag(&s);
        assert_eq!(din, "DIN links");
        let vorher = vorher.unwrap();
        assert!(
            vorher.0.is_empty() && vorher.1.starts_with("öffnet in Raum"),
            "{vorher:?}"
        );
        let p = selection::props(&s, id).unwrap();
        assert_eq!(p.set_label, "Wand");
        assert!(p.layer_set.starts_with("IW-"), "{}", p.layer_set);
        // Umriss: Loch und Aufschlag im Grundriss, das Loch in 3D
        let t = sk_ui::theme::Theme::dark();
        let plan = selection::helpers(&s, id, ViewKind::Plan, None, 1.0, &t);
        assert_eq!(plan.len(), 4 + 13, "Viereck, Blatt und Bogen");
        let persp = selection::helpers(&s, id, ViewKind::Persp, None, 1.0, &t);
        assert_eq!(persp.len(), 12, "Quader des Lochs");
        // Leertaste an der gewählten Tür, die Maus weit weg
        e.stand(None, Some(0.0), Some(id));
        zeige(&mut e, &mut s, vec3(8000.0, 6000.0, 0.0));
        let taste = Event::Key {
            key: LEERTASTE,
            down: true,
            mods: sk_platform::Modifiers {
                shift: true,
                ..mods
            },
            repeat: false,
        };
        assert!(e.handle(&taste, &mut s, &cam, W, H, 1.0, true).changed);
        let (_, nachher) = anschlag(&s);
        let nachher = nachher.unwrap();
        assert!(nachher.1.starts_with("öffnet in Raum"), "{nachher:?}");
        assert_ne!(nachher, vorher, "anderer Raum");
        // Löschen nimmt sie aus der Wand, auch im Kontextmenü
        let menu = crate::delete::ContextMenu::new(
            s.model(),
            id,
            &[id],
            0.0,
            0.0,
            (1200, 800, 0),
            &t,
            1.0,
        );
        assert!(menu.actions().iter().any(|a| a.0 == "Löschen" && a.1));
        let run = l.run;
        let d = s.delete_elements(&[id]);
        assert_eq!(d.removed, vec![id]);
        assert!(s.chain(run).unwrap().hole_quads().is_empty());
        assert_ne!(wahl(&mut s, vec3(5000.0, 3000.0, 0.0)), Some(id));
        assert!(s.undo());
        assert_eq!(wahl(&mut s, vec3(5000.0, 3000.0, 0.0)), Some(id));
    }

    /// Ist-Bilder G7c (Einstellungen #132 B1): Paneel einer Tür mit kurzem,
    /// mittlerem und langem Raumnamen bei 100 % und 150 %, links die alte
    /// Wertzeile „DIN links, öffnet in …“, rechts Anschlag und Raum als
    /// Detailzeile. Nur auf Wunsch:
    /// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder_g7c -- --ignored`
    #[test]
    #[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
    fn istbilder_g7c() {
        use crate::selection;
        use crate::ui::{Panel, Ui};
        use sk_paint::Canvas;
        use sk_ui::theme::Theme;
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
        let lade = |n: &str| {
            std::fs::read(lib.join(n))
                .ok()
                .and_then(sk_paint::font::Font::parse)
        };
        let fonts = || sk_ui::widgets::Fonts {
            regular: lade("LiberationSans-Regular.ttf"),
            bold: lade("LiberationSans-Bold.ttf"),
            italic: None,
        };
        if fonts().regular.is_none() {
            return;
        }
        std::fs::create_dir_all(&ziel).unwrap();
        let t = Theme::dark();
        let mut s = Scene::with_model(Model::with_seed(412));
        let mut tuer = None;
        assert!(s.edit_model("Haus", |m| {
            tuer = Some(haus_in(m));
            true
        }));
        let id = tuer.unwrap();
        let raeume = s.model().spaces(s.active_storey());
        let namen = [
            "Küche",
            "Hauswirtschaftsraum",
            "Hauswirtschafts- und Abstellraum mit Waschmaschine",
        ];
        for sk in [1.0f32, 1.5] {
            let (w, h) = ((1440.0 * sk) as u32, (900.0 * sk) as u32);
            let mut bilder: Vec<Canvas> = Vec::new();
            for name in namen {
                assert!(s.edit_model("Name", |m| raeume
                    .iter()
                    .all(|&r| m.set_space(r, |sp| sp.name = name.into()))));
                let neu = selection::props(&s, id).unwrap();
                let i = neu.values.iter().position(|v| v.0 == "Anschlag").unwrap();
                assert_eq!(neu.values[i + 1].1, format!("öffnet in {name}"));
                // vorher: eine Wertzeile
                let mut alt = neu.clone();
                let raum = alt.values.remove(i + 1).1;
                alt.values[i].1 = format!("{}, {raum}", alt.values[i].1);
                for p in [alt, neu] {
                    let mut ui = Ui::new(sk, &t);
                    ui.fit(sk, w, h);
                    ui.fonts = fonts();
                    ui.set_props(Some(p));
                    let (c, ..) = ui.paint(&t, Panel::Props, w, (32.0 * sk) as u32);
                    bilder.push(c.clone());
                }
            }
            // je Name eine Zeile: vorher, nachher
            let rand = (20.0 * sk) as usize;
            let bw = bilder.iter().map(|c| c.width).max().unwrap();
            let bh = bilder.iter().map(|c| c.height).max().unwrap();
            let (breite, hoehe) = (2 * bw + 3 * rand, 3 * bh + 4 * rand);
            let mut m = Canvas::new(breite, hoehe);
            m.fill_rect(0.0, 0.0, breite as f32, hoehe as f32, t.ui.bg);
            for (k, c) in bilder.iter().enumerate() {
                let (x, y) = (rand + (k % 2) * (bw + rand), rand + (k / 2) * (bh + rand));
                m.blit(c, x as i32, y as i32);
            }
            let datei = format!("ist-g7c-anschlag-{}.png", (sk * 100.0) as u32);
            std::fs::write(ziel.join(datei), m.to_png()).unwrap();
        }
    }

    /// Sturz im Grundriss (G7b): eigene Stelle „Über der Schnittebene“ der
    /// Darstellung. Im Startsatz sieht er aus wie das Einbauelement; ändert
    /// man die Stelle, ändert sich nur der Sturz.
    #[test]
    fn sturz_ueber_der_schnittebene() {
        use sk_model::edge_kind::{ABOVE, ELEMENT};
        let (m, _) = haus();
        let mut s = Scene::with_model(m);
        let cut = s.plan_cut() as f32;
        let mesh = s.mesh(crate::ui::ViewKind::Plan, None, &[]);
        let sturz: Vec<_> = mesh.edges.iter().filter(|e| e.1 == ABOVE as f32).collect();
        assert_eq!(sturz.len(), 2, "beide Wandfluchten");
        assert!(sturz.iter().all(|e| e.0[0][2] == cut && e.0[1][2] == cut));
        let art = |s: &Scene, k: u8| {
            let l = s.table().edge_looks(true, 1.0);
            let k = k as usize;
            (l.width[k], l.color[k], l.dash[2 * k], l.dash[2 * k + 1])
        };
        assert_eq!(art(&s, ABOVE), art(&s, ELEMENT));
        let element = art(&s, ELEMENT);
        assert!(s.edit_model("Darstellung", |m| {
            let a = m.attr();
            let mittel = a.pens().iter().find(|(_, p)| p.number == 2).unwrap().0;
            let spl = a
                .line_types()
                .iter()
                .find(|(_, l)| l.name == "Strichpunktlinie");
            let mut d = a.display().clone();
            d.above = sk_model::EdgeStyle {
                pen: mittel,
                line_type: spl.unwrap().0,
            };
            m.set_display(d);
            true
        }));
        let (w, _, [l, g, punkt, _], _) = art(&s, ABOVE);
        assert!(w > element.0, "Stift 2 statt Fein");
        assert!(punkt == 1.0 && (l / g - 6.0).abs() < 1e-4, "Strichpunkt");
        assert_eq!(art(&s, ELEMENT), element, "Einbauelement bleibt");
    }
}

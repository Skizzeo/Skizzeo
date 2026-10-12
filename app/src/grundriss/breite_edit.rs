//! Breite einer Öffnung am Griff ziehen (Paket O6, Grundriss): Steht die
//! Maus bei einer Öffnung des aktiven Geschosses, zeigt sie an beiden Enden
//! ihres Pfads einen Griff, an der Außenwand auf der Außenfläche, an der
//! Innenwand in der Wandmitte. Ziehen schiebt das Ende längs seiner Fläche
//! im Raster von 10 mm; eine Innentür rastet an den Normbreiten nach
//! DIN 18101. Die Öffnung bleibt mindestens 10 cm breit, das Ende kommt
//! nicht über das andere hinaus, und was das Modell ablehnt (Wandende,
//! Nachbaröffnung), bleibt beim letzten gültigen Stand stehen. Live,
//! Loslassen ist ein Schritt, Esc bricht ab. Teil von [`crate::wall_edit`].

use super::drag_point;
use super::ende_edit::{self, Outcome, GRIFF_PX};
use crate::camera::Camera;
use crate::scene::Scene;
use sk_math::{dist_to_segment, vec3, Vec3};
use sk_model::hoehen::{DOOR_CATCH, DOOR_WIDTHS};
use sk_model::oeffnung::{Opening, OpeningKind, OuterFace, MIN_OPENING};
use sk_model::{Category, ElementId, ElementKind, Model, StoreyId};
use sk_platform::{Event, Key, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Bezeichnung des Schritts im Verlauf.
pub const SCHRITT: &str = "Öffnungsbreite geändert";
/// Raster längs der Fläche (mm).
const STEP: f64 = 10.0;
/// Die Griffe erscheinen, wenn die Maus so nah am Pfad steht (mm), bei
/// breiten Öffnungen bis zur halben Breite.
const NAH: f64 = 500.0;

fn flat(p: Vec3) -> Vec3 {
    vec3(p.x, p.y, 0.0)
}

/// Ein Griff: Öffnung, Ende im Pfad (0 oder letzter Punkt), Zeichenpunkt.
pub type Griff = (ElementId, usize, Vec3);

/// Fläche einer öffnungsfähigen Wand des Geschosses `st`, auf der der Punkt
/// `p` liegt, und die Wanddicke; `innen` bei einer Innenwand.
fn flaeche_bei(m: &Model, st: StoreyId, p: Vec3) -> Option<(OuterFace, f64, bool)> {
    let p = flat(p);
    for (id, run) in m.runs().iter() {
        let cat = run
            .segments
            .first()
            .and_then(|e| m.element(*e))
            .map(|e| e.category);
        if run.storey != st || !cat.is_some_and(Category::takes_openings) {
            continue;
        }
        let Some(c) = m.chain(id) else {
            continue;
        };
        for seg in 0..c.segment_count() {
            let Some(f) = m.outer_face(id, seg) else {
                continue;
            };
            let u = f.u_of(p);
            let (lo, hi) = (f.u.0.min(f.u.1), f.u.0.max(f.u.1));
            if f.on_line(p) && u > lo - 1.0 && u < hi + 1.0 {
                let innen = cat == Some(Category::InteriorWall);
                return Some((f, c.thickness(), innen));
            }
        }
    }
    None
}

/// Zeichenpunkt eines Pfadendes: Außenfläche, an der Innenwand die Mitte.
fn zeichenpunkt(f: &OuterFace, p: Vec3, dicke: f64, innen: bool) -> Vec3 {
    let p = flat(p);
    if innen {
        p - f.n * (dicke / 2.0)
    } else {
        p
    }
}

/// Griffe der Öffnungen des aktiven Geschosses bei der Maus (Punkt `p` im
/// Grundriss).
pub fn sichtbar(scene: &Scene, p: Vec3) -> Vec<Griff> {
    let m = scene.model();
    let st = scene.active_storey();
    let p = flat(p);
    let mut out = Vec::new();
    for (id, e) in m.elements().iter() {
        let ElementKind::Opening(o) = &e.kind else {
            continue;
        };
        let n = o.path.len();
        if e.storey != st || !(2..=3).contains(&n) {
            continue;
        }
        let nah = o.path.windows(2).any(|w| {
            let r = NAH.max((w[1] - w[0]).length() / 2.0);
            let xy = |q: Vec3| (q.x, q.y);
            dist_to_segment(xy(p), xy(w[0]), xy(w[1])) <= r
        });
        if !nah {
            continue;
        }
        for k in [0, n - 1] {
            if let Some((f, t, innen)) = flaeche_bei(m, st, o.path[k]) {
                out.push((id, k, zeichenpunkt(&f, o.path[k], t, innen)));
            }
        }
    }
    out
}

/// Griff unter der Maus (Ansichtspixel `x`, `y`) aus `griffe`, gezeichnet
/// in Höhe `z`.
pub fn griff_bei(
    griffe: &[Griff],
    cam: &Camera,
    z: f64,
    (x, y): (f64, f64),
    w: f64,
    h: f64,
    scale: f64,
) -> Option<Griff> {
    griffe
        .iter()
        .filter_map(|&(id, k, p)| {
            let (px, py) = cam.project(vec3(p.x, p.y, z), w, h)?;
            let d = (px - x).hypot(py - y);
            (d <= GRIFF_PX * scale).then_some((d, (id, k, p)))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|x| x.1)
}

/// Lage längs der Fläche für das gezogene Ende bei `u` (roh): im Raster,
/// mindestens [`MIN_OPENING`] vom Nachbarpunkt `u_nb` und auf der Seite, auf
/// der es beim Greifen lag (`vorz`); bei `norm` rastet die Breite an den
/// Normbreiten der Innentüren.
pub fn lage(u: f64, u_nb: f64, vorz: f64, norm: bool) -> f64 {
    let mut b = ((u - u_nb) * vorz / STEP).round() * STEP;
    if norm {
        if let Some(w) = DOOR_WIDTHS.iter().find(|w| (b - **w).abs() <= DOOR_CATCH) {
            b = *w;
        }
    }
    u_nb + vorz * b.max(MIN_OPENING)
}

/// Laufendes Ziehen an einem Ende einer Öffnung.
pub struct BreiteZug {
    pub id: ElementId,
    k: usize,
    original: Opening,
    face: OuterFace,
    dicke: f64,
    innen: bool,
    /// Lage des Nachbarpunkts im Pfad und Seite des Endes dazu.
    u_nb: f64,
    vorz: f64,
    /// Höhe des Grundrisses.
    z: f64,
    geaendert: bool,
}

impl BreiteZug {
    /// Greift Ende `k` der Öffnung `id` und öffnet den Schritt.
    pub fn greifen(scene: &mut Scene, id: ElementId, k: usize, z: f64) -> Option<BreiteZug> {
        let m = scene.model();
        let original = m.opening(id)?.clone();
        let st = m.element(id)?.storey;
        let p = *original.path.get(k)?;
        let nb = if k == 0 { 1 } else { k.checked_sub(1)? };
        let (face, dicke, innen) = flaeche_bei(m, st, p)?;
        let u_nb = face.u_of(flat(*original.path.get(nb)?));
        let vorz = (face.u_of(flat(p)) - u_nb).signum();
        scene.begin(SCHRITT);
        Some(BreiteZug {
            id,
            k,
            original,
            face,
            dicke,
            innen,
            u_nb,
            vorz,
            z,
            geaendert: false,
        })
    }

    /// Die Öffnung mit dem gezogenen Ende bei `u` (roh).
    fn mit_ende(&self, u: f64) -> Opening {
        let norm = self.innen && self.original.kind == OpeningKind::Door;
        let u = lage(u, self.u_nb, self.vorz, norm);
        let mut o = self.original.clone();
        let alt = o.path[self.k];
        o.path[self.k] = self.face.at(u, alt.z);
        o
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
                let Some(p) = drag_point(cam, x, y, w, h, self.z) else {
                    return out;
                };
                let o = self.mit_ende(self.face.u_of(flat(p)));
                let jetzt = scene.model().opening(self.id).cloned();
                if jetzt.as_ref() != Some(&o) && scene.set_opening(self.id, o) {
                    self.geaendert = true;
                    out.changed = true;
                    out.redraw = true;
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                let gleich = scene.model().opening(self.id) == Some(&self.original);
                if gleich {
                    scene.rollback();
                } else {
                    scene.commit();
                }
                out.changed = self.geaendert && gleich;
                out.fertig = true;
                out.redraw = true;
            }
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } => {
                scene.rollback();
                out.changed = self.geaendert;
                out.fertig = true;
                out.redraw = true;
            }
            _ => {}
        }
        out
    }

    /// Der Griff am gezogenen Ende, wie die Öffnung gerade steht.
    pub fn helpers(&self, scene: &Scene, scale: f32, occlude: bool, theme: &Theme) -> Vec<Helper> {
        let Some(p) = scene
            .model()
            .opening(self.id)
            .and_then(|o| o.path.get(self.k))
        else {
            return Vec::new();
        };
        let g = zeichenpunkt(&self.face, *p, self.dicke, self.innen);
        ende_edit::griff(g, self.z, scale, occlude, theme).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::oeffnung::{Din, FaceSide, HeightLine, HeightRef, Swing};
    use sk_model::{LevelRef, RefSide};

    /// Prüfhaus 10 × 8 m mit Innenwand bei x = 5 m und einer Tür darin,
    /// 88,5 cm breit um y = 3 m, dazu ein Fenster in der Außenwand y = 0.
    fn haus() -> (Scene, ElementId, ElementId) {
        let mut m = Model::with_seed(413);
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
        let tuer = Opening {
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
        };
        let tuer = m.add_opening(tuer).unwrap();
        let seg = (0..4)
            .find(|&s| {
                m.outer_face(eg, s)
                    .is_some_and(|f| f.n.y < -0.5 && f.u_of(vec3(7000.0, 0.0, 0.0)).is_finite())
            })
            .unwrap();
        let fa = m.outer_face(eg, seg).unwrap();
        let ua = fa.u_of(vec3(7000.0, -300.0, 0.0));
        let fenster = Opening {
            kind: OpeningKind::Window,
            path: vec![fa.at(ua - 500.0, 0.0), fa.at(ua + 500.0, 0.0)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
        };
        let fenster = m.add_opening(fenster).unwrap();
        (Scene::with_model(m), tuer, fenster)
    }

    /// Raster, Mindestbreite, keine Kreuzung, Normbreiten.
    #[test]
    fn lage_im_raster() {
        assert_eq!(lage(1234.0, 0.0, 1.0, false), 1230.0);
        assert_eq!(lage(-50.0, 0.0, 1.0, false), MIN_OPENING);
        assert_eq!(lage(-1234.0, 0.0, -1.0, false), -1230.0);
        assert_eq!(lage(500.0, 0.0, -1.0, false), -MIN_OPENING);
        assert_eq!(lage(870.0, 0.0, 1.0, true), 885.0);
        assert_eq!(lage(1040.0, 0.0, 1.0, true), 1010.0);
        assert_eq!(lage(950.0, 0.0, 1.0, true), 950.0);
    }

    /// Griffe erscheinen nur bei der Maus: an der Innentür in der
    /// Wandmitte, am Fenster auf der Außenfläche.
    #[test]
    fn griffe_bei_der_maus() {
        let (s, tuer, fenster) = haus();
        let g = sichtbar(&s, vec3(5100.0, 3000.0, 0.0));
        assert_eq!(g.iter().filter(|x| x.0 == tuer).count(), 2);
        assert!(g.iter().all(|x| x.0 == tuer));
        let ys: Vec<f64> = g.iter().map(|x| x.2.y).collect();
        assert!(ys.iter().any(|y| (y - 2557.5).abs() < 1e-6), "{ys:?}");
        assert!(g.iter().all(|x| (x.2.x - 5000.0).abs() < 1e-6), "{g:?}");
        let g = sichtbar(&s, vec3(7000.0, 200.0, 0.0));
        assert_eq!(g.len(), 2);
        assert!(g.iter().all(|x| x.0 == fenster));
        assert!(sichtbar(&s, vec3(2500.0, 6000.0, 0.0)).is_empty());
    }

    /// Ziehen am Ende der Innentür: rastet an der Normbreite, Esc stellt
    /// zurück, Loslassen ist ein Schritt; am Fenster bleibt die Öffnung vor
    /// der Hausecke stehen, wenn das Modell weiter ablehnt.
    #[test]
    fn ende_ziehen() {
        const W: f64 = 1200.0;
        const H: f64 = 800.0;
        let cam = Camera::looking_at(
            vec3(5000.0, 3000.0, 20000.0),
            vec3(5000.0, 3001.0, 0.0),
            45.0,
        );
        let (mut s, tuer, fenster) = haus();
        let mods = Default::default();
        let breite = |s: &Scene, id| s.model().opening(id).unwrap().width();
        let k_bei = |s: &Scene, id, y: f64| {
            let o = s.model().opening(id).unwrap();
            usize::from((o.path[1].y - y).abs() < (o.path[0].y - y).abs())
        };
        // Oberes Ende der Tür (y = 3442,5) auf 3555: Breite 997,5, im
        // Raster 1000, gefangen an 1010
        let k = k_bei(&s, tuer, 3442.5);
        let mut z = BreiteZug::greifen(&mut s, tuer, k, 0.0).unwrap();
        let (x, y) = cam.project(vec3(5000.0, 3555.0, 0.0), W, H).unwrap();
        assert!(
            z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H)
                .changed
        );
        assert!(
            (breite(&s, tuer) - 1010.0).abs() < 1e-6,
            "{}",
            breite(&s, tuer)
        );
        let esc = Event::Key {
            key: Key::Escape,
            down: true,
            mods,
            repeat: false,
        };
        assert!(z.handle(&esc, &mut s, &cam, W, H).fertig);
        assert!((breite(&s, tuer) - 885.0).abs() < 1e-6);
        // Unteres Ende nach unten auf 760 schmaler (Fang), Loslassen
        let k = k_bei(&s, tuer, 2557.5);
        let mut z = BreiteZug::greifen(&mut s, tuer, k, 0.0).unwrap();
        let (x, y) = cam.project(vec3(5000.0, 2690.0, 0.0), W, H).unwrap();
        z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        let los = Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods,
        };
        assert!(z.handle(&los, &mut s, &cam, W, H).fertig);
        assert!(
            (breite(&s, tuer) - 760.0).abs() < 1e-6,
            "{}",
            breite(&s, tuer)
        );
        assert_eq!(s.undo_label(), Some(SCHRITT));
        // Fenster: das rechte Ende weit über die Hausecke hinaus gezogen
        // bleibt beim letzten gültigen Stand vor der Ecke
        let o = s.model().opening(fenster).unwrap().clone();
        let k = usize::from(o.path[1].x > o.path[0].x);
        let mut z = BreiteZug::greifen(&mut s, fenster, k, 0.0).unwrap();
        for x_mm in [8000.0, 9000.0, 9600.0, 12000.0] {
            let (x, y) = cam.project(vec3(x_mm, 0.0, 0.0), W, H).unwrap();
            z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        }
        let o = s.model().opening(fenster).unwrap();
        let xmax = o.path[0].x.max(o.path[1].x);
        assert!(xmax > 9000.0 && xmax < 10000.0, "{xmax}");
        assert_eq!(s.model().opening_fault(o, Some(fenster)), None);
    }
}

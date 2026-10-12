//! Türaufschläge für die Raumstempel (Grundriss G7, plan.md §6a): Der
//! Viertelkreis, den ein Türblatt überstreicht, bleibt frei, wenn der Stempel
//! anderswo im Raum Platz hat ([`super::Stempel`]). Gerechnet wird nur, was
//! die Auslegung braucht, je Modellstand einmal: die Türen des Geschosses,
//! ein Aufschlag erst für einen Raum im Bild, die Ausweichstelle erst, wenn
//! der Stempel an seiner Mitte einen Bogen überdeckt (Review #87 R2, #99 B1).

use crate::scene::Scene;
use sk_math::{vec3, Vec3};
use sk_model::oeffnung::DoorLeaf;
use sk_model::{ElementId, ElementKind, StoreyId};
use sk_raum::RaumForm;
use std::collections::HashMap;

/// Teile des Bogens im Umriss, wie das Blatt im Grundriss gezeichnet wird.
const BOGEN: usize = 12;
/// Vorfilter (mm): Das Blatt liegt auf der Rohbauflucht, höchstens eine
/// Wanddicke neben dem Pfad der Tür (wie beim Griff, `tuer_edit`).
const DICKE: f64 = 1000.0;

fn flat(p: Vec3) -> Vec3 {
    vec3(p.x, p.y, 0.0)
}

/// Überdecken sich die Kästen (mm: x, y von–bis)?
fn ueber(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// Viertelkreis, den ein Türblatt überstreicht (mm, Grundriss z = 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aufschlag {
    /// Bandpfosten, die Mitte des Kreises.
    pub band: Vec3,
    /// Richtung des offenen Blatts (in den Raum des Aufschlags) und des
    /// geschlossenen (zur Gegenzarge), je der Länge 1.
    pub offen: Vec3,
    pub zu: Vec3,
    /// Blattbreite, der Halbmesser.
    pub r: f64,
}

impl Aufschlag {
    /// Aufschlag des Türblatts `l`; `None` ohne Breite.
    pub fn aus(l: &DoorLeaf) -> Option<Aufschlag> {
        let band = flat(l.hinge);
        let (a, b) = (flat(l.line.0), flat(l.line.1));
        let gegen = if (a - band).length() > (b - band).length() {
            a
        } else {
            b
        };
        let r = (gegen - band).length();
        (r > 1e-6).then(|| Aufschlag {
            band,
            offen: flat(l.swing_into).normalized(),
            zu: (gegen - band) / r,
            r,
        })
    }

    /// Abstand von `p` zum Viertelkreis (0 darin). Er ändert sich je mm Weg
    /// um höchstens 1 mm, wie [`sk_math::flaeche::innenpunkt_mit`] verlangt.
    pub fn abstand(&self, p: Vec3) -> f64 {
        let v = flat(p) - self.band;
        if v.dot(self.offen) >= 0.0 && v.dot(self.zu) >= 0.0 {
            return (v.length() - self.r).max(0.0);
        }
        // Außerhalb des Winkels liegt der nächste Punkt auf einem Halbmesser
        let auf = |d: Vec3| (v - d * v.dot(d).clamp(0.0, self.r)).length();
        auf(self.offen).min(auf(self.zu))
    }

    /// Umriss: Band, offenes Blattende, der Bogen bis zur Gegenzarge.
    pub fn umriss(&self) -> Vec<Vec3> {
        std::iter::once(self.band)
            .chain((0..=BOGEN).map(|i| {
                let w = std::f64::consts::FRAC_PI_2 * i as f64 / BOGEN as f64;
                self.band + (self.offen * w.cos() + self.zu * w.sin()) * self.r
            }))
            .collect()
    }

    /// Kasten (mm: x, y von–bis) um das Quadrat aus beiden Blattlagen.
    fn kasten(&self) -> [f64; 4] {
        let (o, z) = (self.offen * self.r, self.zu * self.r);
        [self.band, self.band + o, self.band + z, self.band + o + z]
            .iter()
            .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |k, p| {
                [k[0].min(p.x), k[1].min(p.y), k[2].max(p.x), k[3].max(p.y)]
            })
    }
}

/// Aufschläge der Türen eines Geschosses zu einem Modellstand und die
/// Ausweichstellen der Stempel, je erst bei Bedarf gerechnet.
#[derive(Default)]
pub struct Tueren {
    /// Türen mit Aufschlag: Id und ein Kasten um ihren Pfad, der den
    /// Aufschlag sicher enthält.
    liste: Option<Vec<(ElementId, [f64; 4])>>,
    boegen: HashMap<ElementId, Option<Aufschlag>>,
    /// Je Fläche (Hausgruppe, Nummer).
    stellen: HashMap<(usize, usize), Vec3>,
}

impl Tueren {
    /// Aufschläge der Türen des Geschosses `st`, deren Kasten den Kasten
    /// `k` (mm) eines Raums überdeckt.
    pub fn bei(&mut self, s: &Scene, st: StoreyId, k: [f64; 4]) -> Vec<Aufschlag> {
        let m = s.model();
        let liste = self.liste.get_or_insert_with(|| {
            m.elements()
                .iter()
                .filter_map(|(id, e)| {
                    let ElementKind::Opening(o) = &e.kind else {
                        return None;
                    };
                    if e.storey != st || o.swing.is_none() || o.path.len() != 2 {
                        return None;
                    }
                    let (a, b) = (o.path[0], o.path[1]);
                    let r = (b - a).length() + DICKE;
                    let k = [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)];
                    Some((id, [k[0] - r, k[1] - r, k[2] + r, k[3] + r]))
                })
                .collect()
        });
        let mut v = Vec::new();
        for &(id, _) in liste.iter().filter(|(_, kt)| ueber(*kt, k)) {
            let a = *self.boegen.entry(id).or_insert_with(|| {
                let l = m.door_leaf_in(id, &|r| s.koerper(r))?;
                Aufschlag::aus(&l)
            });
            v.extend(a.filter(|a| ueber(a.kasten(), k)));
        }
        v
    }

    /// Stelle für den Stempel der Fläche `f` (Hausgruppe und Nummer
    /// `form`), die außer von den Wänden auch von den Aufschlägen `a`
    /// Abstand hält.
    pub fn stelle(&mut self, form: (usize, usize), f: &RaumForm, a: &[Aufschlag]) -> Vec3 {
        *self.stellen.entry(form).or_insert_with(|| {
            f.mitte_ohne(|p| a.iter().map(|a| a.abstand(p)).fold(f64::INFINITY, f64::min))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Bild, Stempel};
    use super::*;
    use crate::camera::Camera;
    use crate::terrace_label::touches;
    use crate::ui::ViewKind;
    use sk_model::oeffnung::{Din, FaceSide, HeightLine, HeightRef, Opening, OpeningKind, Swing};
    use sk_model::{Category, LevelRef, Model, RefSide, RunId};
    use sk_ui::theme::Theme;
    use sk_ui::widgets::Fonts;

    const W: f64 = 1000.0;
    const H: f64 = 700.0;

    #[test]
    fn abstand_zum_viertelkreis() {
        let a = Aufschlag {
            band: vec3(0.0, 0.0, 0.0),
            offen: vec3(1.0, 0.0, 0.0),
            zu: vec3(0.0, 1.0, 0.0),
            r: 1000.0,
        };
        let nah = |p: Vec3, d: f64| (a.abstand(p) - d).abs() < 1e-6;
        assert!(nah(vec3(300.0, 300.0, 0.0), 0.0), "darin");
        assert!(
            nah(vec3(1000.0, 1000.0, 0.0), 1000.0 * (2f64.sqrt() - 1.0)),
            "vor dem Bogen"
        );
        assert!(nah(vec3(1500.0, 0.0, 0.0), 500.0), "vor dem offenen Blatt");
        assert!(
            nah(vec3(500.0, -200.0, 0.0), 200.0),
            "neben dem offenen Blatt"
        );
        assert!(nah(vec3(-300.0, -400.0, 0.0), 500.0), "hinter dem Band");
        assert!(
            nah(vec3(-200.0, 1500.0, 0.0), 200f64.hypot(500.0)),
            "an der Gegenzarge"
        );
        let u = a.umriss();
        assert_eq!(u.len(), BOGEN + 2);
        assert_eq!(u[0], a.band);
        assert!(
            (u[1] - vec3(1000.0, 0.0, 0.0)).length() < 1e-9,
            "offenes Blattende"
        );
        assert!(
            (u[BOGEN + 1] - vec3(0.0, 1000.0, 0.0)).length() < 1e-9,
            "Gegenzarge"
        );
        assert!(u.iter().all(|p| a.abstand(*p) < 1e-9));
    }

    /// Haus 10 × 8 m mit Innenwand bei x = 2,40 m (Zeichenrichtung +y):
    /// links ein schmaler, langer Raum, noch ohne gespeicherte Räume.
    fn haus() -> (Scene, RunId) {
        let mut m = Model::with_seed(7);
        let st = m.defaults().storey;
        let (aw, iw) = (m.defaults().exterior_wall, m.defaults().interior_wall);
        let t: f64 = m.wall_layers(aw).iter().map(|l| l.thickness).sum();
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
            .unwrap();
        let pts = [vec3(2400.0, t, 0.0), vec3(2400.0, 8000.0 - t, 0.0)];
        let run = m
            .add_wall_run(&pts, false, RefSide::Center, st, iw, Category::InteriorWall)
            .unwrap();
        (Scene::with_model(m), run)
    }

    /// Tür 1,01 m in der Innenwand, Band bei `y`, schlägt nach links in den
    /// schmalen Raum auf und öffnet zur Gegenzarge darüber.
    fn tuer(s: &mut Scene, run: RunId, y: f64) -> ElementId {
        let st = s.active_storey();
        let f = s.model().outer_face(run, 0).unwrap();
        let u = f.u_of(vec3(2400.0, y + 505.0, 0.0));
        let o = Opening {
            kind: OpeningKind::Door,
            path: vec![f.at(u - 505.0, 0.0), f.at(u + 505.0, 0.0)],
            bottom: HeightRef::Level(LevelRef::bottom(st)),
            top: HeightRef::Line(st, HeightLine::Door),
            reveal: None,
            install: Default::default(),
            swing: Some(Swing {
                side: FaceSide::Left,
                din: Din::Rechts,
            }),
            lock: Default::default(),
        };
        s.add_opening(o).unwrap()
    }

    fn auslegen(st: &mut Stempel, s: &Scene, c: &Camera) {
        let fonts = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        st.auslegen(s, c, W, H, 1.0, &fonts, &Theme::dark(), &[]);
    }

    /// Der schmale Raum links (sein Stempel).
    fn links(st: &Stempel) -> Bild {
        st.bilder
            .iter()
            .find(|b| b.punkt.x < 2400.0)
            .unwrap()
            .clone()
    }

    /// Bogen der Tür `id` in Pixeln.
    fn bogen(s: &Scene, c: &Camera, id: ElementId) -> Vec<[f64; 2]> {
        let a = Aufschlag::aus(&s.model().door_leaf(id).unwrap()).unwrap();
        let z = s.plan_cut();
        a.umriss()
            .iter()
            .map(|p| {
                let (x, y) = c.project(vec3(p.x, p.y, z), W, H).unwrap();
                [x, y]
            })
            .collect()
    }

    /// Eine Tür, deren Bogen den Stempel an der Raummitte überdeckt, rückt
    /// ihn im Raum zur Seite, ganz und frei vom Bogen; der Stempel des
    /// anderen Raums bleibt. Ein verschobener Stempel bleibt, wo er ist,
    /// auch auf dem Bogen.
    #[test]
    fn stempel_meidet_den_bogen() {
        let (mut s, run) = haus();
        let c = crate::fit_parallel(ViewKind::Plan, s.bounds(), W, H);
        let mut st = Stempel::default();
        auslegen(&mut st, &s, &c);
        assert_eq!(st.bilder.len(), 2);
        let (vorher, rechts) = (
            links(&st),
            st.bilder.iter().find(|b| b.punkt.x > 2400.0).cloned(),
        );
        let id = tuer(&mut s, run, vorher.punkt.y);
        let l = s.model().door_leaf(id).unwrap();
        assert!(l.swing_into.x < -0.99, "in den schmalen Raum: {l:?}");
        assert!(
            (l.hinge.y - vorher.punkt.y).abs() < 1.0,
            "Band auf Höhe der Mitte: {l:?}"
        );
        let b = bogen(&s, &c, id);
        assert!(touches(&b, &vorher.kasten), "der Bogen träfe den Stempel");

        auslegen(&mut st, &s, &c);
        let jetzt = links(&st);
        assert!(!jetzt.kurz, "ganz");
        assert!(!touches(&b, &jetzt.kasten), "{jetzt:?}");
        assert!((jetzt.punkt - vorher.punkt).length() > 500.0, "{jetzt:?}");
        assert_eq!(
            st.bilder.iter().find(|b| b.punkt.x > 2400.0).cloned(),
            rechts
        );

        // Raum anlegen: Das Namensfeld öffnet, wo der Stempel steht
        let raum = s.raum_anlegen(s.active_storey(), jetzt.punkt);
        st.oeffnen(&s, raum);
        auslegen(&mut st, &s, &c);
        let feld = links(&st);
        assert_eq!(feld.punkt, jetzt.punkt);
        let k = (
            feld.kasten.x + feld.kasten.w / 2.0,
            feld.kasten.y + feld.kasten.h / 2.0,
        );
        let z = s.plan_cut();
        let q = c
            .project(vec3(jetzt.punkt.x, jetzt.punkt.y, z), W, H)
            .unwrap();
        assert!(
            (k.0 - q.0).abs() < 1.0 && (k.1 - q.1).abs() < 1.0,
            "{k:?} {q:?}"
        );
        st.namen = None;
        // Den Stempel auf den Bogen ziehen: Er bleibt dort
        let p = l.hinge + vec3(-700.0, 300.0, 0.0);
        s.begin("Raumstempel verschoben");
        assert!(s.raum_stempel(raum, p));
        s.commit();
        auslegen(&mut st, &s, &c);
        let da = links(&st);
        assert_eq!(da.raum, Some(raum));
        assert!((da.punkt - vec3(p.x, p.y, 0.0)).length() < 1e-6, "{da:?}");
        assert!(touches(&b, &da.kasten));
    }
}

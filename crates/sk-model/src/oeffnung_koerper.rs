//! Leerraum einer Öffnung je Wand (IFC I5, ifc/plan.md §5): dieselben
//! gestaffelten Löcher wie im Wandkörper ([`crate::wand_loch`]), hier als
//! Quader. Durch die Außenschale geht die sichtbare Öffnung, durch die
//! Schichten dahinter die um den Anschlag größere; monolithisch ein Quader.
//! Der Export schreibt sie als `IfcOpeningElement` an die Wand, deren Netz
//! schon gelocht ist.

use crate::solid::Solid;
use crate::wall::right_of;
use crate::{ElementId, Model};
use sk_math::{vec3, Vec3};

/// Leerraum einer Öffnung in einer Wand.
#[derive(Clone, Debug)]
pub struct OpeningVoid {
    /// Die gelochte Wand (ein Segment eines Außenwandzugs).
    pub wall: ElementId,
    /// Quader von außen nach innen (absolut, mm).
    pub boxes: Vec<Solid>,
    /// Sichtbare Breite und Höhe in dieser Wand und die Dicke der Wand
    /// quer durch das Loch (mm).
    pub width: f64,
    pub height: f64,
    pub depth: f64,
}

impl Model {
    /// Leerräume der Öffnung `id`, je gelochter Wand einer, nach Wand
    /// geordnet. Läuft die Öffnung um die Ecke, endet der Quader an der
    /// Außenecke; über zwei Geschosse teilt die Wandhöhe ihn auf.
    pub fn opening_voids(&self, id: ElementId) -> Vec<OpeningVoid> {
        let Some(op) = self.opening(id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        // nur Außenwandzüge ([`Model::opening_runs`])
        for run in self.opening_runs(op) {
            let Some(c) = self.chain(run) else {
                continue;
            };
            let Some((pts, _, dirs)) = c.layout() else {
                continue;
            };
            let lagen = c.layer_offsets();
            // Ecken der Außenfläche: dort endet ein zur Ecke offenes Loch
            let ecken = c.face_corners(c.outer_offset());
            let (zb, zt) = (c.base, c.base + c.height);
            for (_, h) in self.legs_on(run, &c, Some(id), op) {
                let Some(wall) = self.wall_at(run, h.seg) else {
                    continue;
                };
                let (p0, d) = (pts[h.seg], dirs[h.seg]);
                let ende = |k: usize| ecken.get(k % ecken.len().max(1)).map(|&e| (e - p0).dot(d));
                let (Some(e0), Some(e1)) = (ende(h.seg), ende(h.seg + 1)) else {
                    continue;
                };
                let nr = right_of(d);
                let quer = |a: usize, b: usize| {
                    let s = &lagen[a..b];
                    let lo = s.iter().map(|l| l.0).fold(f64::INFINITY, f64::min);
                    let hi = s.iter().map(|l| l.1).fold(f64::NEG_INFINITY, f64::max);
                    (lo, hi)
                };
                let n = lagen.len();
                let shell = h.shell.min(n);
                let teile = if shell >= n || shell == 0 {
                    vec![(quer(0, n), true)]
                } else {
                    vec![(quer(0, shell), true), (quer(shell, n), false)]
                };
                let mut boxes = Vec::new();
                let mut sichtbar = (0.0, 0.0);
                for (t, aussen) in teile {
                    let ((u0, u1), (z0, z1)) = h.kasten(aussen);
                    // zur Ecke hin offen: bis an die Außenecke
                    let u = (
                        if u0.is_finite() { u0 } else { e0 },
                        if u1.is_finite() { u1 } else { e1 },
                    );
                    let z = (z0.max(zb), z1.min(zt));
                    if u.1 - u.0 < 1e-6 || z.1 - z.0 < 1e-6 || t.1 - t.0 < 1e-6 {
                        continue;
                    }
                    if aussen {
                        sichtbar = (u.1 - u.0, z.1 - z.0);
                    }
                    boxes.push(quader(p0, d, nr, u, t, z));
                }
                if !boxes.is_empty() {
                    let (lo, hi) = quer(0, n);
                    out.push(OpeningVoid {
                        wall,
                        boxes,
                        width: sichtbar.0,
                        height: sichtbar.1,
                        depth: hi - lo,
                    });
                }
            }
        }
        out.sort_by_key(|v| v.wall.index());
        out
    }
}

/// Quader längs `d` (u), quer `nr` (t) und lotrecht (z) ab `p0`, mit
/// Normalen nach außen.
fn quader(p0: Vec3, d: Vec3, nr: Vec3, u: (f64, f64), t: (f64, f64), z: (f64, f64)) -> Solid {
    let at = |u: f64, t: f64, z: f64| {
        let p = p0 + d * u + nr * t;
        vec3(p.x, p.y, z)
    };
    let up = vec3(0.0, 0.0, 1.0);
    let mut s = Solid::default();
    for (uu, n) in [(u.0, d * -1.0), (u.1, d)] {
        s.quad(
            at(uu, t.0, z.0),
            at(uu, t.1, z.0),
            at(uu, t.1, z.1),
            at(uu, t.0, z.1),
            n,
        );
    }
    for (tt, n) in [(t.0, nr * -1.0), (t.1, nr)] {
        s.quad(
            at(u.0, tt, z.0),
            at(u.1, tt, z.0),
            at(u.1, tt, z.1),
            at(u.0, tt, z.1),
            n,
        );
    }
    for (zz, n) in [(z.0, up * -1.0), (z.1, up)] {
        s.quad(
            at(u.0, t.0, zz),
            at(u.1, t.0, zz),
            at(u.1, t.1, zz),
            at(u.0, t.1, zz),
            n,
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use crate::element::RunId;
    use crate::model::{Model, MONO_TYPE_GUID};
    use crate::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
    use crate::solid::Solid;
    use crate::wall::right_of;
    use crate::Guid;
    use sk_math::{vec3, Vec3};

    /// Prüfhaus 10 × 8 m wie in den Öffnungstests, Typ `typ` (`None`: WDVS).
    fn haus(typ: Option<Guid>) -> (Model, RunId, RunId) {
        let mut m = Model::with_seed(211);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let og = m.runs_above(eg)[0];
        if let Some(g) = typ {
            let t = m.type_by_guid(g).unwrap();
            m.begin("Typ");
            assert!(m.set_run_type(eg, t));
            m.commit();
        }
        (m, eg, og)
    }

    fn aussen(m: &Model, run: RunId, seg: usize, u: f64) -> Vec3 {
        let c = m.chain(run).unwrap();
        let (pts, _, dirs) = c.layout().unwrap();
        pts[seg] + dirs[seg] * u + right_of(dirs[seg]) * c.outer_offset()
    }

    fn fenster(m: &Model, run: RunId, u: (f64, f64)) -> Opening {
        let st = m.run(run).unwrap().storey;
        Opening {
            kind: OpeningKind::Window,
            path: vec![aussen(m, run, 0, u.0), aussen(m, run, 0, u.1)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
        }
    }

    /// Rauminhalt eines geschlossenen Netzes (mm³).
    fn volumen(s: &Solid) -> f64 {
        s.triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.p;
                let v = a.dot(b.cross(c)) / 6.0;
                // Umlauf nach der Normale
                if (b - a).cross(c - a).dot(t.n) < 0.0 {
                    -v
                } else {
                    v
                }
            })
            .sum()
    }

    /// WDVS: zwei Quader an der Wand von Segment 0, zusammen so groß wie
    /// die Löcher der Mengen je Schicht; monolithisch einer.
    #[test]
    fn zwei_quader_wie_die_loecher() {
        use crate::qto::run_qto;
        let (mut m, eg, _) = haus(None);
        let id = m.add_opening(fenster(&m, eg, (3000.0, 4250.0))).unwrap();
        let v = m.opening_voids(id);
        assert_eq!(v.len(), 1);
        assert_eq!(Some(v[0].wall), m.wall_at(eg, 0));
        assert_eq!(v[0].boxes.len(), 2);
        let c = m.chain(eg).unwrap();
        assert!((v[0].depth - c.thickness()).abs() < 1e-9);
        assert!((v[0].width - 1250.0).abs() < 1e-6 && (v[0].height - 1510.0).abs() < 1e-6);
        let loecher: f64 = run_qto(&m, eg)[0]
            .layers
            .iter()
            .flat_map(|l| &l.holes)
            .map(|h| h.volume)
            .sum();
        let quader: f64 = v[0].boxes.iter().map(volumen).sum();
        assert!((quader - loecher).abs() < 1.0, "{quader} {loecher}");
        // die äußere sichtbar 1,25 m breit, die innere um 2 × 3 cm breiter
        let breite = |s: &Solid| {
            let (a, b) = s.bounds().unwrap();
            b.y - a.y
        };
        assert!((breite(&v[0].boxes[0]) - 1250.0).abs() < 1e-6);
        assert!((breite(&v[0].boxes[1]) - 1310.0).abs() < 1e-6);

        let (mut m, eg, _) = haus(Some(MONO_TYPE_GUID));
        let id = m.add_opening(fenster(&m, eg, (3000.0, 4250.0))).unwrap();
        let v = m.opening_voids(id);
        assert_eq!((v.len(), v[0].boxes.len()), (1, 1));
        assert!(m.opening_voids(m.wall_at(eg, 0).unwrap()).is_empty());
    }

    /// Ecköffnung lochte beide Wände an der Ecke, geschossübergreifend
    /// EG- und OG-Wand, jede nur in ihrer Höhe.
    #[test]
    fn ecke_und_geschosse() {
        let (mut m, eg, og) = haus(None);
        let c = m.chain(eg).unwrap();
        let ecke = c.face_corners(c.outer_offset())[1];
        let id = m
            .add_opening(Opening {
                path: vec![aussen(&m, eg, 0, 7000.0), ecke, aussen(&m, eg, 1, 1500.0)],
                ..fenster(&m, eg, (0.0, 1.0))
            })
            .unwrap();
        let walls: Vec<_> = m.opening_voids(id).iter().map(|v| v.wall).collect();
        assert_eq!(
            walls,
            vec![m.wall_at(eg, 0).unwrap(), m.wall_at(eg, 1).unwrap()]
        );
        // die äußere endet an der Außenecke
        let v = m.opening_voids(id);
        let (_, hi) = v[0].boxes[0].bounds().unwrap();
        assert!((hi.y - ecke.y).abs() < 1e-6, "{} {}", hi.y, ecke.y);

        let s_og = m.run(og).unwrap().storey;
        let id = m
            .add_opening(Opening {
                top: HeightRef::Line(s_og, HeightLine::Lintel),
                ..fenster(&m, eg, (2000.0, 3000.0))
            })
            .unwrap();
        let v = m.opening_voids(id);
        let walls: Vec<_> = v.iter().map(|v| v.wall).collect();
        assert_eq!(
            walls,
            vec![m.wall_at(eg, 0).unwrap(), m.wall_at(og, 0).unwrap()]
        );
        let (ce, co) = (m.chain(eg).unwrap(), m.chain(og).unwrap());
        for (v, c) in v.iter().zip([ce, co]) {
            for b in &v.boxes {
                let (lo, hi) = b.bounds().unwrap();
                assert!(lo.z >= c.base - 1e-6 && hi.z <= c.base + c.height + 1e-6);
            }
        }
    }
}

//! Geschossdecke über einem geschlossenen Außenwandzug (Geometrie).
//!
//! Der Umriss ist die Außenseite der tragenden Schicht des Zuges: Die Decke
//! liegt in einer Auflagertasche über die ganze Kerndicke und stößt außen an
//! die Dämmung. Die Dicke geht von der Oberkante nach unten.
//!
//! Die Tasche entsteht nicht hier, sondern als Abzug aus den Wandkörpern: Das
//! Modell setzt bei allen Wandzügen unter der Decke [`Joints::slab_band`] auf
//! [`FloorSlab::band`], dann teilt die Wand ihre tragenden Schichten in z
//! (unter und über der Decke). So läuft die Kontur des Mauerwerks um die Decke
//! herum, und keine Linie steht quer durch sie.
//!
//! [`Joints::slab_band`]: crate::wall::Joints::slab_band

use crate::solid::{edge_kind, material, Solid, Tri};
use crate::wall::WallChain;
use sk_math::polygon;
use sk_math::{vec3, Vec3};

/// Maße und Baustoff der Decke (mm, Baustoff als Darstellungsschlüssel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloorParams {
    /// Oberkante über Wandfuß (OK Sohlplatte).
    pub top: f64,
    pub thickness: f64,
    pub mat: u16,
}

/// Warum keine Decke entsteht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloorError {
    /// Wandzug offen oder mit weniger als drei Ecken.
    NotClosed,
    /// Der Zug hat keine tragende Schicht (keine Tasche möglich).
    NoCore,
    /// Umriss überschneidet sich selbst.
    NotSimple,
    /// Dicke ≤ 0.
    BadThickness,
    /// Unterkante der Decke auf oder unter dem Wandfuß.
    BelowWallFoot,
    /// Oberkante der Decke über der Wandkrone.
    AboveWallTop,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FloorSlab {
    /// Umriss gegen den Uhrzeigersinn, auf z = 0.
    pub outline: Vec<Vec3>,
    pub params: FloorParams,
}

fn at_z(p: Vec3, z: f64) -> Vec3 {
    vec3(p.x, p.y, z)
}

fn right_of(d: Vec3) -> Vec3 {
    vec3(d.y, -d.x, 0.0)
}

impl FloorSlab {
    /// Decke über dem geschlossenen Außenwandzug `chain`.
    pub fn from_chain(chain: &WallChain, p: &FloorParams) -> Result<FloorSlab, FloorError> {
        if !chain.closed || chain.clean_points().len() < 3 {
            return Err(FloorError::NotClosed);
        }
        let core = chain
            .layers
            .iter()
            .position(|l| l.core)
            .ok_or(FloorError::NoCore)?;
        if p.thickness.is_nan() || p.thickness <= 0.0 {
            return Err(FloorError::BadThickness);
        }
        if p.top.is_nan() || p.top - p.thickness <= chain.base {
            return Err(FloorError::BelowWallFoot);
        }
        if p.top > chain.top() + 1e-6 {
            return Err(FloorError::AboveWallTop);
        }
        // Außenseite der tragenden Schicht: Außenfläche mit derselben
        // Eckberechnung wie die Wand, dann um die Dämmdicke mit dem robusten
        // Versatz (sonst verknoten kurze Vorsprünge den Umriss). Ohne
        // Sonderfälle deckungsgleich mit der Schichtgrenze der Wand.
        let depth: f64 = chain.layers[..core].iter().map(|l| l.thickness).sum();
        let face = chain.face_corners(chain.outer_offset());
        let outline = if depth > 0.0 {
            polygon::inset(&face, depth)
                .map_err(|_| FloorError::NotSimple)?
                .pts
        } else {
            face
        };
        FloorSlab::from_outline(&outline, p)
    }

    /// Decke über einem beliebigen einfachen Umriss (Richtung egal).
    pub fn from_outline(outline: &[Vec3], p: &FloorParams) -> Result<FloorSlab, FloorError> {
        if p.thickness.is_nan() || p.thickness <= 0.0 {
            return Err(FloorError::BadThickness);
        }
        let outline = polygon::simplified(&polygon::to_ccw(outline));
        if outline.len() < 3 {
            return Err(FloorError::NotClosed);
        }
        if !polygon::is_simple(&outline) {
            return Err(FloorError::NotSimple);
        }
        Ok(FloorSlab {
            outline,
            params: *p,
        })
    }

    /// Höhenband (Unterkante, Oberkante), in dem die Wände ausgespart werden.
    pub fn band(&self) -> (f64, f64) {
        (self.params.top - self.params.thickness, self.params.top)
    }

    fn cap(s: &mut Solid, pts: &[Vec3], z: f64, up: bool) {
        let nrm = vec3(0.0, 0.0, if up { 1.0 } else { -1.0 });
        for t in polygon::triangulate(pts) {
            let [a, b, c] = t.map(|k| at_z(pts[k], z));
            s.triangles.push(Tri {
                p: if up { [a, b, c] } else { [a, c, b] },
                n: nrm,
                mat: s.mat,
                uv: [[0.0; 2]; 3],
                elem: s.elem,
            });
        }
    }

    /// Senkrechte Ecke an Punkt `i` (nicht dort, wo der Umriss gerade weiterläuft)?
    fn corner(c: &[Vec3], i: usize) -> bool {
        let n = c.len();
        let (u, w) = (
            (c[i] - c[(i + n - 1) % n]).normalized(),
            (c[(i + 1) % n] - c[i]).normalized(),
        );
        !((u.x * w.y - u.y * w.x).abs() < 1e-9 && u.dot(w) > 0.0)
    }

    /// Deckenkörper zwischen `z0` und `z1`; ist `cut_top`, trägt die Deckfläche
    /// [`material::CUT`] und ist kräftig umrandet.
    fn prism(&self, z0: f64, z1: f64, cut_top: bool) -> Solid {
        let mut s = Solid {
            mat: self.params.mat,
            ..Solid::default()
        };
        let c = &self.outline;
        let n = c.len();
        FloorSlab::cap(&mut s, c, z0, false);
        s.mat = if cut_top {
            self.params.mat | material::CUT
        } else {
            self.params.mat
        };
        FloorSlab::cap(&mut s, c, z1, true);
        s.mat = self.params.mat;
        for i in 0..n {
            let (a, b) = (c[i], c[(i + 1) % n]);
            let nr = right_of((b - a).normalized());
            s.quad(at_z(a, z0), at_z(b, z0), at_z(b, z1), at_z(a, z1), nr);
            s.edge_kind = edge_kind::VIEW;
            s.edge(at_z(a, z0), at_z(b, z0));
            s.edge_kind = if cut_top {
                edge_kind::CUT
            } else {
                edge_kind::VIEW
            };
            s.edge(at_z(a, z1), at_z(b, z1));
            if FloorSlab::corner(c, i) {
                s.edge_kind = edge_kind::VIEW;
                s.edge(at_z(a, z0), at_z(a, z1));
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Körper der Decke für 3D und Ansichten.
    pub fn solid(&self) -> Solid {
        let (b, t) = self.band();
        self.prism(b, t, false)
    }

    /// Decke waagerecht geschnitten in Höhe `cut` (Grundriss): Liegt sie über
    /// dem Schnitt, ist der Körper leer (nichts zu zeichnen), liegt sie
    /// darunter, ganz, sonst bis zum Schnitt mit Schnittfläche oben.
    pub fn solid_cut_at(&self, cut: f64) -> Solid {
        let (b, t) = self.band();
        if cut <= b {
            Solid::default()
        } else if cut >= t {
            self.solid()
        } else {
            self.prism(b, cut, true)
        }
    }

    /// Schnittflächen mit der senkrechten Ebene durch `p0` mit Normale `n`
    /// (Flächen zeigen in Richtung `n`), ringsum kräftig umrandet.
    pub fn section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let n = vec3(n.x, n.y, 0.0).normalized();
        let along = vec3(-n.y, n.x, 0.0);
        let base = vec3(p0.x, p0.y, 0.0) - along * vec3(p0.x, p0.y, 0.0).dot(along);
        let pt = |u: f64, z: f64| base + along * u + vec3(0.0, 0.0, z);
        let (zb, zt) = self.band();
        let mut s = Solid {
            mat: self.params.mat | material::CUT,
            edge_kind: edge_kind::CUT,
            ..Solid::default()
        };
        for (a, b) in polygon::plane_intervals(&self.outline, p0, n, along) {
            s.quad(pt(a, zb), pt(b, zb), pt(b, zt), pt(a, zt), n);
            s.edge(pt(a, zb), pt(b, zb));
            s.edge(pt(a, zt), pt(b, zt));
            s.edge(pt(a, zb), pt(a, zt));
            s.edge(pt(b, zb), pt(b, zt));
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    // ---- Mengen (Grundlage für qto, Einheiten mm, mm², mm³) ----

    /// Fläche der Decke (Umriss bis Außenseite Kern).
    pub fn area(&self) -> f64 {
        polygon::area(&self.outline)
    }

    /// Umfang (Randschalung).
    pub fn perimeter(&self) -> f64 {
        polygon::perimeter(&self.outline)
    }

    pub fn volume(&self) -> f64 {
        self.area() * self.params.thickness
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wall::{Layer, RefSide};

    const CONCRETE: u16 = 7;
    const AAC: u16 = 1;
    const INSULATION: u16 = 2;

    /// Rechteck 10 × 8 m im Uhrzeigersinn, Außenseite auf der Bezugslinie,
    /// AW 31,5 (14 Dämmung, 17,5 Gasbeton), Wandhöhe 3,50 m.
    fn haus() -> WallChain {
        WallChain {
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            layers: vec![Layer::new(140.0, INSULATION), Layer::core(175.0, AAC)],
            base: 0.0,
            height: 3500.0,
            joints: Default::default(),
        }
    }

    fn params(t: f64) -> FloorParams {
        FloorParams {
            top: 2330.0,
            thickness: t,
            mat: CONCRETE,
        }
    }

    fn near(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    /// Volumen eines geschlossenen Körpers aus seinen Dreiecken (Divergenzsatz).
    /// Die Normalen zeigen nach außen, die Reihenfolge der Ecken kann abweichen.
    fn volume(s: &Solid) -> f64 {
        s.triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.p;
                let cr = (b - a).cross(c - a);
                let sign = if cr.dot(t.n) >= 0.0 { 1.0 } else { -1.0 };
                sign * a.dot(b.cross(c)) / 6.0
            })
            .sum()
    }

    #[test]
    fn sollwerte_b10() {
        let f = FloorSlab::from_chain(&haus(), &params(220.0)).unwrap();
        assert!(near(f.area(), 9720.0 * 7720.0, 1e-3));
        assert!(near(f.area() / 1e6, 75.0384, 1e-9));
        assert!(near(f.volume() / 1e9, 16.508448, 1e-9));
        assert!(near(f.perimeter(), 34880.0, 1e-6));
        assert_eq!(f.band(), (2110.0, 2330.0));
        let f = FloorSlab::from_chain(&haus(), &params(250.0)).unwrap();
        assert!(near(f.volume() / 1e9, 18.7596, 1e-4));
        // Gummiband: Wand y = 8 um +1 m
        let mut w = haus();
        w.points[1].y = 9000.0;
        w.points[2].y = 9000.0;
        let f = FloorSlab::from_chain(&w, &params(220.0)).unwrap();
        assert!(near(f.area() / 1e6, 84.7584, 1e-9));
    }

    #[test]
    fn gegen_uhrzeigersinn_gleich() {
        let mut w = haus();
        w.points.reverse();
        w.ref_side = RefSide::Right;
        let f = FloorSlab::from_chain(&w, &params(220.0)).unwrap();
        assert!(near(f.area(), 9720.0 * 7720.0, 1e-3));
    }

    #[test]
    fn ungueltige_lage() {
        let w = haus();
        let mut p = params(220.0);
        p.top = 3600.0;
        assert_eq!(FloorSlab::from_chain(&w, &p), Err(FloorError::AboveWallTop));
        p.top = 3500.0;
        assert!(FloorSlab::from_chain(&w, &p).is_ok());
        p.top = 200.0;
        assert_eq!(
            FloorSlab::from_chain(&w, &p),
            Err(FloorError::BelowWallFoot)
        );
        p.top = 2330.0;
        p.thickness = 0.0;
        assert_eq!(FloorSlab::from_chain(&w, &p), Err(FloorError::BadThickness));
        let mut w2 = haus();
        w2.layers = vec![Layer::new(300.0, AAC)];
        assert_eq!(
            FloorSlab::from_chain(&w2, &params(220.0)),
            Err(FloorError::NoCore)
        );
        let mut w3 = haus();
        w3.closed = false;
        assert_eq!(
            FloorSlab::from_chain(&w3, &params(220.0)),
            Err(FloorError::NotClosed)
        );
    }

    #[test]
    fn tasche_im_kern_daemmung_laeuft_durch() {
        let f = FloorSlab::from_chain(&haus(), &params(220.0)).unwrap();
        let mut w = haus();
        let brutto = volume(&w.solid());
        w.joints.slab_band = Some(f.band());
        let s = w.solid();
        // Volumen je Baustoff
        let vol_of = |mat: u16| {
            let sub = Solid {
                triangles: s
                    .triangles
                    .iter()
                    .filter(|t| t.mat & !material::CUT == mat)
                    .cloned()
                    .collect(),
                ..Solid::default()
            };
            volume(&sub)
        };
        assert!(
            near(vol_of(AAC) / 1e9, 19.61932, 1e-5),
            "{}",
            vol_of(AAC) / 1e9
        );
        assert!(near(vol_of(INSULATION) / 1e9, 17.3656, 1e-4));
        assert!(near((brutto - volume(&s)) / 1e9, 1.31593, 1e-5));
        // Decke + Gasbeton netto = Gasbeton brutto + Decke − Tasche
        let tasche = (brutto - volume(&s)) / 1e9;
        assert!(near(
            tasche,
            (f.area() - 9370.0 * 7370.0) * 220.0 / 1e9,
            1e-9
        ));
    }

    #[test]
    fn innenwand_unterbrochen() {
        let f = FloorSlab::from_chain(&haus(), &params(220.0)).unwrap();
        let mut iw = WallChain {
            points: vec![vec3(5000.0, 315.0, 0.0), vec3(5000.0, 7685.0, 0.0)],
            closed: false,
            ref_side: RefSide::Center,
            layers: vec![Layer::core(175.0, AAC)],
            base: 0.0,
            height: 3500.0,
            joints: Default::default(),
        };
        iw.joints.slab_band = Some(f.band());
        assert_eq!(iw.layer_spans(0), vec![(0.0, 2110.0), (2330.0, 3500.0)]);
        assert!(near(volume(&iw.solid()) / 1e9, 4.23038, 1e-5));
    }

    #[test]
    fn schnitt_ohne_linie_quer_durch_die_decke() {
        let f = FloorSlab::from_chain(&haus(), &params(220.0)).unwrap();
        let mut w = haus();
        w.joints.slab_band = Some(f.band());
        let (p0, n) = (vec3(0.0, 4000.0, 0.0), vec3(0.0, -1.0, 0.0));
        let caps = w.section_caps(p0, n);
        // Keine Gasbeton-Kante im Deckenband (2110 < z < 2330)
        let through = caps.edges.iter().any(|e| {
            let (lo, hi) = (e.a.z.min(e.b.z), e.a.z.max(e.b.z));
            lo < 2329.0 && hi > 2111.0 && e.kind == edge_kind::CUT
        });
        assert!(!through);
        // Tasche: Gasbeton-Kontur oben und unten waagerecht, kräftig
        for z in [2110.0, 2330.0] {
            assert!(caps
                .edges
                .iter()
                .any(|e| near(e.a.z, z, 1e-9) && near(e.b.z, z, 1e-9) && e.kind == edge_kind::CUT));
        }
        // Dämmung durchgehend: eine Kappe je Seite über die ganze Höhe
        let ins = caps
            .triangles
            .iter()
            .filter(|t| t.mat & !material::CUT == INSULATION)
            .count();
        assert_eq!(ins, 2 * 2);
        // Deckenkappe von Außenseite Kern zu Außenseite Kern
        let d = f.section_caps(p0, n);
        let (lo, hi) = d.bounds().unwrap();
        assert!(near(lo.x, 140.0, 1e-6) && near(hi.x, 9860.0, 1e-6));
        assert!(near(lo.z, 2110.0, 1e-9) && near(hi.z, 2330.0, 1e-9));
        assert!(d.edges.iter().all(|e| e.kind == edge_kind::CUT));
        assert_eq!(d.edges.len(), 4);
    }

    #[test]
    fn grundriss_unveraendert() {
        let f = FloorSlab::from_chain(&haus(), &params(220.0)).unwrap();
        assert!(f.solid_cut_at(1000.0).is_empty());
        let mut w = haus();
        let vorher = w.solid_cut_at(1000.0);
        w.joints.slab_band = Some(f.band());
        let nachher = w.solid_cut_at(1000.0);
        assert_eq!(vorher.triangles.len(), nachher.triangles.len());
        assert_eq!(vorher.edges.len(), nachher.edges.len());
        assert!(near(volume(&vorher), volume(&nachher), 1e-3));
    }

    #[test]
    fn kurze_vorspruenge_und_spitze_winkel() {
        for (deg, step) in [(10.0f64, 50.0), (20.0, 200.0), (25.0, 1.5)] {
            let a = deg.to_radians();
            let w = WallChain {
                points: vec![
                    vec3(0.0, 0.0, 0.0),
                    vec3(0.0, 8000.0, 0.0),
                    vec3(5000.0, 8000.0, 0.0),
                    vec3(5000.0, 8000.0 + step, 0.0),
                    vec3(5000.0 + step, 8000.0 + step, 0.0),
                    vec3(5000.0 + step, 8000.0, 0.0),
                    vec3(14000.0, 8000.0 - 14000.0 * a.tan(), 0.0),
                    vec3(10000.0, 0.0, 0.0),
                ],
                ..haus()
            };
            let f = FloorSlab::from_chain(&w, &params(220.0));
            let f = f.unwrap_or_else(|e| panic!("{deg}° {step}: {e:?}"));
            let s = f.solid();
            assert!(s
                .triangles
                .iter()
                .all(|t| t.p.iter().all(|p| p.x.is_finite())));
            let top: f64 = s
                .triangles
                .iter()
                .filter(|t| t.n.z > 0.5)
                .map(|t| polygon::area(&t.p))
                .sum();
            assert!(near(top, f.area(), 1.0), "{deg}°: {top} {}", f.area());
        }
    }

    #[test]
    fn decke_oben_buendig_mit_wandkrone() {
        // Geschossmanager: Wand reicht bis OK Rohdecke, die Tasche sitzt oben
        let mut w = haus();
        w.height = 2750.0;
        let p = FloorParams {
            top: 2750.0,
            thickness: 220.0,
            mat: CONCRETE,
        };
        let f = FloorSlab::from_chain(&w, &p).unwrap();
        w.joints.slab_band = Some(f.band());
        assert_eq!(w.layer_spans(1), vec![(0.0, 2530.0)]);
        assert_eq!(w.layer_spans(0), vec![(0.0, 2750.0)]);
        let (lo, hi) = w.solid().bounds().unwrap();
        assert!(near(lo.z, 0.0, 1e-9) && near(hi.z, 2750.0, 1e-9));
        // Band ganz über der Wand: Wand unverändert
        w.joints.slab_band = Some((2800.0, 3000.0));
        assert_eq!(w.layer_spans(1), vec![(0.0, 2750.0)]);
    }

    #[test]
    fn schnell_genug_fuer_gummiband() {
        let mut w = haus();
        let t = std::time::Instant::now();
        let runs = 200;
        for k in 0..runs {
            w.points[1].y = 8000.0 + k as f64;
            w.points[2].y = 8000.0 + k as f64;
            let f = FloorSlab::from_chain(&w, &params(220.0)).unwrap();
            let mut ww = w.clone();
            ww.joints.slab_band = Some(f.band());
            let _ = (f.solid(), ww.solid());
        }
        let per = t.elapsed().as_secs_f64() * 1000.0 / runs as f64;
        eprintln!("Decke und Wand mit Tasche: {per:.3} ms");
    }
}

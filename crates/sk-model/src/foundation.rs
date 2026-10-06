//! Gründung unter einem geschlossenen Außenwandzug: Sohlplatte und
//! umlaufende Frostschürze (Geometrie).
//!
//! Die Sohlplatte liegt mit der Oberkante auf z = 0 (Wandfuß), ihre Dicke geht
//! nach unten. Ihr Umriss ist die Außenfläche des Wandzugs, um den
//! Sockelrücksprung nach innen versetzt. Die Frostschürze liegt darunter,
//! außen bündig mit der Platte, als Ring der Breite `footing_width` und der
//! Tiefe `footing_depth` ab Unterkante Platte.
//!
//! Beide sind getrennte Körper (eigene Bauteile, eigener Treffer beim Klicken).
//! Bei gleichem Baustoff entsteht zwischen ihnen keine Fuge: Die Berührfläche
//! fehlt in beiden Körpern, und keine Kante zeichnet die Trennung, weder in 3D
//! noch im Schnitt.

use crate::solid::{edge_kind, material, Solid};
use crate::wall::WallChain;
use sk_math::polygon::{self, Inset, InsetError};
use sk_math::{vec3, Vec3};

/// Maße und Baustoffe der Gründung (mm, Baustoff als Darstellungsschlüssel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoundationParams {
    /// Sockelrücksprung der Platte nach innen, 0 = bündig mit der Wand außen.
    pub recess: f64,
    pub slab_thickness: f64,
    pub footing_width: f64,
    /// Tiefe der Frostschürze ab Unterkante Platte.
    pub footing_depth: f64,
    pub slab_mat: u16,
    pub footing_mat: u16,
}

impl Default for FoundationParams {
    fn default() -> FoundationParams {
        FoundationParams {
            recess: 0.0,
            slab_thickness: 200.0,
            footing_width: 350.0,
            footing_depth: 600.0,
            slab_mat: material::PLAIN,
            footing_mat: material::PLAIN,
        }
    }
}

/// Warum keine Gründung entsteht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoundationError {
    /// Wandzug offen oder mit weniger als drei Ecken.
    NotClosed,
    /// Umriss überschneidet sich selbst.
    NotSimple,
    /// Rücksprung negativ oder nicht kleiner als die Wanddicke
    /// (die Wand stünde nicht mehr auf der Platte).
    RecessTooLarge,
    /// Maße ≤ 0.
    BadSize,
}

/// Form der Frostschürze im Grundriss.
#[derive(Clone, Debug, PartialEq)]
pub enum FootingShape {
    /// Ring zwischen Plattenumriss und dem nach innen versetzten Umriss.
    Ring(Inset),
    /// Die Platte ist schmaler als zwei Schürzenbreiten (oder der Versatz
    /// überschneidet sich): Die Schürze füllt die ganze Fläche unter der Platte.
    Full(InsetError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Foundation {
    /// Plattenumriss gegen den Uhrzeigersinn, auf z = 0.
    pub outline: Vec<Vec3>,
    pub footing: FootingShape,
    pub params: FoundationParams,
}

/// Rechte Normale einer Richtung (bei Umriss gegen den Uhrzeigersinn: außen).
fn right_of(d: Vec3) -> Vec3 {
    vec3(d.y, -d.x, 0.0)
}

fn at_z(p: Vec3, z: f64) -> Vec3 {
    vec3(p.x, p.y, z)
}

/// Gerade weiter an Punkt `i` (keine Kante zeichnen)?
fn straight(c: &[Vec3], i: usize) -> bool {
    let n = c.len();
    let (a, b, d) = (c[(i + n - 1) % n], c[i], c[(i + 1) % n]);
    let (u, w) = ((b - a).normalized(), (d - b).normalized());
    (u.x * w.y - u.y * w.x).abs() < 1e-9 && u.dot(w) > 0.0
}

impl Foundation {
    /// Gründung unter dem geschlossenen Wandzug `chain`.
    pub fn from_chain(
        chain: &WallChain,
        p: &FoundationParams,
    ) -> Result<Foundation, FoundationError> {
        if !chain.closed || chain.clean_points().len() < 3 {
            return Err(FoundationError::NotClosed);
        }
        if !(p.recess >= 0.0 && p.recess < chain.thickness()) {
            return Err(FoundationError::RecessTooLarge);
        }
        let (outer, inner) = (chain.outer_offset(), chain.inner_offset());
        let inward = (inner - outer).signum();
        // Dieselbe Eckberechnung wie die Wand: bei Rücksprung 0 deckungsgleich
        let outline = chain.face_corners(outer + inward * p.recess);
        Foundation::from_outline(&outline, p)
    }

    /// Gründung unter einem beliebigen einfachen Umriss (Richtung egal).
    pub fn from_outline(
        outline: &[Vec3],
        p: &FoundationParams,
    ) -> Result<Foundation, FoundationError> {
        if !(p.slab_thickness > 0.0 && p.footing_width > 0.0 && p.footing_depth > 0.0) {
            return Err(FoundationError::BadSize);
        }
        let outline = polygon::simplified(&polygon::to_ccw(outline));
        if outline.len() < 3 {
            return Err(FoundationError::NotClosed);
        }
        if !polygon::is_simple(&outline) {
            return Err(FoundationError::NotSimple);
        }
        let footing = match polygon::inset(&outline, p.footing_width) {
            Ok(i) => FootingShape::Ring(i),
            Err(e) => FootingShape::Full(e),
        };
        Ok(Foundation {
            outline,
            footing,
            params: *p,
        })
    }

    fn seamless(&self) -> bool {
        self.params.slab_mat == self.params.footing_mat
    }

    /// Unterkante Platte und Unterkante Schürze.
    fn levels(&self) -> (f64, f64) {
        let t = self.params.slab_thickness;
        (-t, -t - self.params.footing_depth)
    }

    /// Ringzellen je Umrisskante (Vier- oder Dreieck), gegen den Uhrzeigersinn.
    fn ring_cells(&self, inset: &Inset) -> Vec<Vec<Vec3>> {
        let (c0, c1) = (&self.outline, &inset.pts);
        let n = c0.len();
        (0..n)
            .map(|i| {
                let j = (i + 1) % n;
                let (a, b) = (inset.map[i], inset.map[j]);
                let mut cell = vec![c0[i], c0[j], c1[b]];
                if a != b {
                    cell.push(c1[a]);
                }
                cell
            })
            .collect()
    }

    /// Waagerechte Fläche aus dem Polygon `pts` auf Höhe `z`.
    fn cap(s: &mut Solid, pts: &[Vec3], z: f64, up: bool) {
        let nrm = vec3(0.0, 0.0, if up { 1.0 } else { -1.0 });
        for t in polygon::triangulate(pts) {
            let [a, b, c] = t.map(|k| at_z(pts[k], z));
            let p = if up { [a, b, c] } else { [a, c, b] };
            s.triangles.push(crate::solid::Tri {
                p,
                n: nrm,
                mat: s.mat,
                uv: [[0.0; 2]; 3],
                elem: s.elem,
            });
        }
    }

    /// Seitenflächen entlang `c` zwischen `z0` (unten) und `z1`, nach außen
    /// (`outward`) oder nach innen gerichtet; senkrechte Kanten an den Ecken.
    fn walls(s: &mut Solid, c: &[Vec3], z0: f64, z1: f64, outward: bool) {
        let n = c.len();
        for i in 0..n {
            let (a, b) = (c[i], c[(i + 1) % n]);
            let d = (b - a).normalized();
            let nr = if outward { right_of(d) } else { -right_of(d) };
            s.quad(at_z(a, z0), at_z(b, z0), at_z(b, z1), at_z(a, z1), nr);
        }
        for i in 0..n {
            if !straight(c, i) {
                s.edge(at_z(c[i], z0), at_z(c[i], z1));
            }
        }
    }

    fn ring(s: &mut Solid, c: &[Vec3], z: f64) {
        let n = c.len();
        for i in 0..n {
            s.edge(at_z(c[i], z), at_z(c[(i + 1) % n], z));
        }
    }

    /// Körper der Sohlplatte.
    pub fn slab_solid(&self) -> Solid {
        let mut s = Solid {
            mat: self.params.slab_mat,
            ..Solid::default()
        };
        let (zb, _) = self.levels();
        let c0 = &self.outline;
        Foundation::cap(&mut s, c0, 0.0, true);
        Foundation::walls(&mut s, c0, zb, 0.0, true);
        Foundation::ring(&mut s, c0, 0.0);
        match (&self.footing, self.seamless()) {
            (FootingShape::Ring(inset), seamless) => {
                // Unterseite nur innerhalb der Schürze sichtbar
                Foundation::cap(&mut s, &inset.pts, zb, false);
                Foundation::ring(&mut s, &inset.pts, zb);
                if !seamless {
                    for cell in self.ring_cells(inset) {
                        Foundation::cap(&mut s, &cell, zb, false);
                    }
                    Foundation::ring(&mut s, c0, zb);
                }
            }
            (FootingShape::Full(_), false) => {
                Foundation::cap(&mut s, c0, zb, false);
                Foundation::ring(&mut s, c0, zb);
            }
            (FootingShape::Full(_), true) => {}
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Körper der Frostschürze.
    pub fn footing_solid(&self) -> Solid {
        let mut s = Solid {
            mat: self.params.footing_mat,
            ..Solid::default()
        };
        let (zt, zb) = self.levels();
        let c0 = &self.outline;
        Foundation::walls(&mut s, c0, zb, zt, true);
        Foundation::ring(&mut s, c0, zb);
        if !self.seamless() {
            Foundation::ring(&mut s, c0, zt);
        }
        match &self.footing {
            FootingShape::Ring(inset) => {
                Foundation::walls(&mut s, &inset.pts, zb, zt, false);
                Foundation::ring(&mut s, &inset.pts, zb);
                for cell in self.ring_cells(inset) {
                    Foundation::cap(&mut s, &cell, zb, false);
                    if !self.seamless() {
                        Foundation::cap(&mut s, &cell, zt, true);
                    }
                }
            }
            FootingShape::Full(_) => {
                Foundation::cap(&mut s, c0, zb, false);
                if !self.seamless() {
                    Foundation::cap(&mut s, c0, zt, true);
                }
            }
        }
        s
    }

    /// Schnittflächen mit der senkrechten Ebene durch `p0` mit Normale `n`
    /// (Flächen zeigen in Richtung `n`): (Platte, Schürze). Die Kontur der
    /// Vereinigung ist kräftig ([`edge_kind::CUT`]); bei gleichem Baustoff gibt
    /// es keine Linie zwischen Platte und Schürze.
    pub fn section_caps(&self, p0: Vec3, n: Vec3) -> (Solid, Solid) {
        let n = vec3(n.x, n.y, 0.0).normalized();
        let along = vec3(-n.y, n.x, 0.0);
        let base = vec3(p0.x, p0.y, 0.0) - along * vec3(p0.x, p0.y, 0.0).dot(along);
        let pt = |u: f64, z: f64| base + along * u + vec3(0.0, 0.0, z);
        let (zt, zb) = self.levels();
        let mut slab = Solid {
            mat: self.params.slab_mat | material::CUT,
            edge_kind: edge_kind::CUT,
            ..Solid::default()
        };
        let mut foot = Solid {
            mat: self.params.footing_mat | material::CUT,
            edge_kind: edge_kind::CUT,
            ..Solid::default()
        };
        let outer = polygon::plane_intervals(&self.outline, p0, n, along);
        let holes = match &self.footing {
            FootingShape::Ring(i) => polygon::plane_intervals(&i.pts, p0, n, along),
            FootingShape::Full(_) => Vec::new(),
        };
        let rect = |s: &mut Solid, u0: f64, u1: f64, z0: f64, z1: f64| {
            s.quad(pt(u0, z0), pt(u1, z0), pt(u1, z1), pt(u0, z1), n);
        };
        for &(a, b) in &outer {
            rect(&mut slab, a, b, zt, 0.0);
            slab.edge(pt(a, 0.0), pt(b, 0.0));
            // Abschnitte ohne Schürze innerhalb [a, b]
            let inner: Vec<(f64, f64)> = holes
                .iter()
                .map(|&(c, d)| (c.max(a), d.min(b)))
                .filter(|(c, d)| d > c)
                .collect();
            let mut u = a;
            let mut zones: Vec<(f64, f64, bool)> = Vec::new(); // (von, bis, Schürze)
            for &(c, d) in &inner {
                if c > u {
                    zones.push((u, c, true));
                }
                zones.push((c, d, false));
                u = d;
            }
            if b > u {
                zones.push((u, b, true));
            }
            for z in &zones {
                if z.2 {
                    rect(&mut foot, z.0, z.1, zb, zt);
                    foot.edge(pt(z.0, zb), pt(z.1, zb));
                    if !self.seamless() {
                        slab.edge(pt(z.0, zt), pt(z.1, zt));
                    }
                } else {
                    slab.edge(pt(z.0, zt), pt(z.1, zt));
                }
            }
            // Senkrechte Kontur: außen ganz hinunter, innen an den Übergängen
            if let (Some(f), Some(l)) = (zones.first(), zones.last()) {
                slab.edge(pt(a, 0.0), pt(a, zt));
                slab.edge(pt(b, 0.0), pt(b, zt));
                if f.2 {
                    foot.edge(pt(a, zt), pt(a, zb));
                }
                if l.2 {
                    foot.edge(pt(b, zt), pt(b, zb));
                }
            }
            for w in zones.windows(2) {
                if w[0].2 != w[1].2 {
                    foot.edge(pt(w[0].1, zt), pt(w[0].1, zb));
                }
            }
        }
        slab.edge_kind = edge_kind::VIEW;
        foot.edge_kind = edge_kind::VIEW;
        (slab, foot)
    }

    // ---- Mengen (Grundlage für qto, Einheiten mm, mm², mm³) ----

    /// Fläche der Sohlplatte (Umriss).
    pub fn slab_area(&self) -> f64 {
        polygon::area(&self.outline)
    }

    /// Umfang der Sohlplatte (Randschalung).
    pub fn slab_perimeter(&self) -> f64 {
        polygon::perimeter(&self.outline)
    }

    pub fn slab_volume(&self) -> f64 {
        self.slab_area() * self.params.slab_thickness
    }

    /// Grundfläche der Frostschürze (Ring, bei voller Füllung der ganze Umriss).
    pub fn footing_area(&self) -> f64 {
        match &self.footing {
            FootingShape::Ring(i) => self.slab_area() - polygon::area(&i.pts),
            FootingShape::Full(_) => self.slab_area(),
        }
    }

    /// Länge der Frostschürze auf ihrer Mittellinie (Versatz Breite/2).
    /// Ohne weggefallene Kanten gilt Länge × Breite = Ringfläche exakt.
    pub fn footing_axis_length(&self) -> f64 {
        match polygon::inset(&self.outline, self.params.footing_width * 0.5) {
            Ok(i) => polygon::perimeter(&i.pts),
            Err(_) => self.footing_area() / self.params.footing_width,
        }
    }

    /// Länge der Frostschürze an ihrer Außenkante.
    pub fn footing_outer_length(&self) -> f64 {
        self.slab_perimeter()
    }

    /// Volumen der Frostschürze aus der Ringfläche (auch bei weggefallenen Kanten exakt).
    pub fn footing_volume(&self) -> f64 {
        self.footing_area() * self.params.footing_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wall::{Layer, RefSide};

    const CONCRETE: u16 = 7;

    /// Rechteck 10 × 8 m im Uhrzeigersinn, Außenseite auf der Bezugslinie, AW 31,5.
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
            layers: vec![Layer::new(140.0, 2), Layer::core(175.0, 1)],
            height: 2750.0,
            joints: Default::default(),
        }
    }

    fn params(recess: f64) -> FoundationParams {
        FoundationParams {
            recess,
            slab_mat: CONCRETE,
            footing_mat: CONCRETE,
            ..FoundationParams::default()
        }
    }

    fn near(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn sollwerte_b9_buendig() {
        let f = Foundation::from_chain(&haus(), &params(0.0)).unwrap();
        assert!(near(f.slab_area(), 80e6, 1e-3));
        assert!(near(f.slab_volume(), 16e9, 1.0));
        assert!(near(f.slab_perimeter(), 36000.0, 1e-6));
        assert!(near(f.footing_axis_length(), 34600.0, 1e-6));
        assert!(near(f.footing_volume(), 34600.0 * 350.0 * 600.0, 1.0));
    }

    #[test]
    fn sollwerte_b9_ruecksprung() {
        let f = Foundation::from_chain(&haus(), &params(20.0)).unwrap();
        assert!(near(f.slab_area(), 9960.0 * 7960.0, 1e-3));
        assert!(near(f.footing_axis_length(), 34440.0, 1e-6));
        assert!(near(f.footing_volume(), 34440.0 * 350.0 * 600.0, 1.0));
    }

    #[test]
    fn gegen_uhrzeigersinn_gleich() {
        let mut w = haus();
        w.points.reverse();
        // Außenseite liegt jetzt rechts der Bezugslinie, die Wand wächst nach außen
        w.ref_side = RefSide::Right;
        let f = Foundation::from_chain(&w, &params(0.0)).unwrap();
        assert!(near(f.slab_area(), 80e6, 1e-3));
    }

    #[test]
    fn ruecksprung_groesser_als_wand_wird_abgelehnt() {
        assert_eq!(
            Foundation::from_chain(&haus(), &params(315.0)),
            Err(FoundationError::RecessTooLarge)
        );
        assert!(Foundation::from_chain(&haus(), &params(314.0)).is_ok());
        assert_eq!(
            Foundation::from_chain(&haus(), &params(-5.0)),
            Err(FoundationError::RecessTooLarge)
        );
    }

    #[test]
    fn offener_zug_hat_keine_platte() {
        let mut w = haus();
        w.closed = false;
        assert_eq!(
            Foundation::from_chain(&w, &params(0.0)),
            Err(FoundationError::NotClosed)
        );
    }

    #[test]
    fn hoehen() {
        let f = Foundation::from_chain(&haus(), &params(0.0)).unwrap();
        let (lo, hi) = f.slab_solid().bounds().unwrap();
        assert!(near(hi.z, 0.0, 1e-9) && near(lo.z, -200.0, 1e-9));
        let (lo, hi) = f.footing_solid().bounds().unwrap();
        assert!(near(hi.z, -200.0, 1e-9) && near(lo.z, -800.0, 1e-9));
        assert!(near(lo.x, 0.0, 1e-9) && near(hi.x, 10000.0, 1e-9));
    }

    #[test]
    fn fugenlos_keine_kante_auf_hoehe_uk_platte_aussen() {
        let f = Foundation::from_chain(&haus(), &params(0.0)).unwrap();
        let mut s = f.slab_solid();
        s.append(&f.footing_solid());
        // Keine Kante auf z = −200 entlang der Außenkante (x = 0 von y 0 bis 8000)
        let outer_seam = s.edges.iter().any(|e| {
            near(e.a.z, -200.0, 1e-9)
                && near(e.b.z, -200.0, 1e-9)
                && near(e.a.x, 0.0, 1e-9)
                && near(e.b.x, 0.0, 1e-9)
        });
        assert!(!outer_seam);
        // Keine waagerechte Fläche zwischen Platte und Schürze im Ring
        let between = s.triangles.iter().any(|t| {
            t.p.iter().all(|p| near(p.z, -200.0, 1e-9)) && t.p.iter().any(|p| p.x < 300.0)
        });
        assert!(!between);
        // Bei verschiedenen Baustoffen gibt es die Fuge
        let mut p = params(0.0);
        p.footing_mat = CONCRETE + 1;
        let f = Foundation::from_chain(&haus(), &p).unwrap();
        let mut s = f.slab_solid();
        s.append(&f.footing_solid());
        assert!(s.edges.iter().any(|e| near(e.a.z, -200.0, 1e-9)
            && near(e.a.x, 0.0, 1e-9)
            && near(e.b.x, 0.0, 1e-9)));
    }

    #[test]
    fn flaechen_decken_den_umriss() {
        let f = Foundation::from_chain(&haus(), &params(0.0)).unwrap();
        let tri_area = |t: &crate::solid::Tri| polygon::area(&t.p);
        let s = f.slab_solid();
        let top: f64 = s
            .triangles
            .iter()
            .filter(|t| t.n.z > 0.5)
            .map(tri_area)
            .sum();
        assert!(near(top, 80e6, 1.0));
        let b = f.footing_solid();
        let bottom: f64 = b
            .triangles
            .iter()
            .filter(|t| t.n.z < -0.5)
            .map(tri_area)
            .sum();
        assert!(near(bottom, f.footing_area(), 1.0));
    }

    #[test]
    fn schnitt_kontur_ohne_fuge() {
        let f = Foundation::from_chain(&haus(), &params(0.0)).unwrap();
        // Schnitt A–A quer bei y = 4000, Blick nach −y
        let (slab, foot) = f.section_caps(vec3(0.0, 4000.0, 0.0), vec3(0.0, -1.0, 0.0));
        assert_eq!(slab.triangles.len(), 2);
        assert_eq!(foot.triangles.len(), 4); // zwei Schürzenquerschnitte
        assert!(slab
            .triangles
            .iter()
            .all(|t| t.mat == CONCRETE | material::CUT));
        // Keine Kante auf z = −200 über den Schürzen (|x| < 350 oder x > 9650)
        let seam = slab.edges.iter().chain(&foot.edges).any(|e| {
            near(e.a.z, -200.0, 1e-9) && near(e.b.z, -200.0, 1e-9) && e.a.x.min(e.b.x) < 349.0
        });
        assert!(!seam);
        // Kontur: oben, unten Platte innen, zwei Schürzenunterkanten, außen je
        // 2 senkrechte Stücke, innen je eine Stufe
        let n = slab.edges.len() + foot.edges.len();
        assert_eq!(n, 1 + 1 + 2 + 2 * 2 + 2, "{n}");
        assert!(slab
            .edges
            .iter()
            .chain(&foot.edges)
            .all(|e| e.kind == edge_kind::CUT));
    }

    #[test]
    fn schmale_platte_volle_schuerze() {
        let outline = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(600.0, 0.0, 0.0),
            vec3(600.0, 5000.0, 0.0),
            vec3(0.0, 5000.0, 0.0),
        ];
        let f = Foundation::from_outline(&outline, &params(0.0)).unwrap();
        assert!(matches!(
            f.footing,
            FootingShape::Full(InsetError::Vanished)
        ));
        assert!(near(f.footing_volume(), 600.0 * 5000.0 * 600.0, 1.0));
        let (_, foot) = f.section_caps(vec3(0.0, 2500.0, 0.0), vec3(0.0, 1.0, 0.0));
        assert_eq!(foot.triangles.len(), 2);
    }

    #[test]
    fn kurze_segmente_und_spitze_winkel() {
        // Wandzug mit 20-cm-Vorsprung und 25°-Spitze
        let w = WallChain {
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(6000.0, 8000.0, 0.0),
                vec3(6000.0, 8200.0, 0.0),
                vec3(6200.0, 8200.0, 0.0),
                vec3(6200.0, 8000.0, 0.0),
                vec3(14000.0, 4000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ],
            ..haus()
        };
        let f = Foundation::from_chain(&w, &params(20.0)).unwrap();
        let FootingShape::Ring(i) = &f.footing else {
            panic!("{:?}", f.footing)
        };
        assert!(polygon::is_simple(&polygon::simplified(&i.pts)));
        let tri_area = |t: &crate::solid::Tri| polygon::area(&t.p);
        let bottom: f64 = f
            .footing_solid()
            .triangles
            .iter()
            .filter(|t| t.n.z < -0.5)
            .map(tri_area)
            .sum();
        assert!(
            near(bottom, f.footing_area(), 1.0),
            "{bottom} {}",
            f.footing_area()
        );
    }

    #[test]
    fn schnell_genug_fuer_gummiband() {
        // Kreis mit n Ecken: Umriss, Ring, beide Körper, Mengen
        for n in [12usize, 48, 200] {
            let pts: Vec<Vec3> = (0..n)
                .map(|i| {
                    let a = i as f64 / n as f64 * std::f64::consts::TAU;
                    vec3(8000.0 * a.cos(), 8000.0 * a.sin(), 0.0)
                })
                .collect();
            let runs = 50;
            let t = std::time::Instant::now();
            for _ in 0..runs {
                let f = Foundation::from_outline(&pts, &params(0.0)).unwrap();
                let _ = (f.slab_solid(), f.footing_solid(), f.footing_axis_length());
            }
            let per = t.elapsed().as_secs_f64() * 1000.0 / runs as f64;
            eprintln!("Gründung mit {n} Ecken: {per:.3} ms");
        }
    }
}

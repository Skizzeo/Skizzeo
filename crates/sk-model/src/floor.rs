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

use crate::solid::{
    at_z, edge_kind, material, right_of, straight_at, SectionFrame, Solid, NO_LAYER,
};
use crate::terrace::{attika_section_caps, attika_solid, coping_profile, TerracePlan};
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

/// Randdämmstreifen vor dem Deckenauflager (K5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripParams {
    /// Breite ab Wandaußenseite (Wanddicke minus Auflagertiefe), mm.
    pub width: f64,
    /// Baustoff des Streifens (Schnittschraffur).
    pub mat: u16,
    /// Baustoff der Wand: in 3D trägt der Streifen ihre Oberfläche, damit
    /// die Wand außen fugenlos aussieht.
    pub face: u16,
    /// Darüber steht eine Wand (OG): die Oberkante des Streifens ist keine
    /// Gebäudekante. In 3D verschmilzt sie mit dem Fuß der Wand, im Schnitt
    /// zeichnet er dort keine Linie.
    pub covered: bool,
}

/// Untersichtdämmung unter dem auskragenden Deckenstreifen, wo das
/// Geschoss darüber vorspringt (OG Phase 2, G7 K4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoffitParams {
    /// Dicke ab UK Decke nach unten, mm.
    pub thickness: f64,
    /// Baustoff (Darstellungsschlüssel).
    pub mat: u16,
}

/// Dachterrasse auf der Decke mit Attika und Blech (D1–D3): Aufbau und
/// Höhen, aus dem Modell aufgelöst.
#[derive(Clone, Debug, PartialEq)]
pub struct TerraceParams {
    /// Schichten von oben nach unten: Dicke (mm), Baustoff
    /// (Darstellungsschlüssel), Dämmschicht (Schraffur längs).
    pub layers: Vec<(f64, u16, bool)>,
    /// Unterkante der Attika: Krone der Wand darunter (z, absolut).
    pub attika_from: f64,
    /// Attika über OK Belag (mm).
    pub upstand: f64,
    /// Baustoff des Blechs (Darstellungsschlüssel).
    pub coping_mat: u16,
}

impl TerraceParams {
    /// Dicke des Aufbaus (mm).
    pub fn thickness(&self) -> f64 {
        self.layers.iter().map(|l| l.0).sum()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FloorSlab {
    /// Umriss gegen den Uhrzeigersinn, auf z = 0.
    pub outline: Vec<Vec3>,
    pub params: FloorParams,
    /// Randdämmstreifen, falls die Decke nur teilweise aufliegt.
    pub strip: Option<StripParams>,
    /// Grundriss der Streifen je Wandsegment (auf z = 0): außen Anfang,
    /// außen Ende, innen Ende, innen Anfang, mit Gehrung wie die Wand.
    pub strips: Vec<[Vec3; 4]>,
    /// Je Streifen: steht die Wand darüber bündig darauf (OG Phase 2, G7 K3)?
    /// Leer: es gilt [`StripParams::covered`] für alle.
    pub strip_covered: Vec<bool>,
    /// Untersichtdämmung (K4), falls ein Segment vorspringt.
    pub soffit: Option<SoffitParams>,
    /// Grundriss der Untersichtdämmung je vorspringendem Segment (Segment,
    /// auf z = 0): Kern EG Anfang, Ende, Kern darüber Ende, Anfang.
    pub soffits: Vec<(usize, [Vec3; 4])>,
    /// Dachterrasse, falls das Geschoss darüber zurückspringt (D1).
    pub terrace: Option<TerraceParams>,
    /// Umrisse, Attika-Stücke und Blechpfad dazu (leer ohne Terrasse).
    pub terraces: TerracePlan,
    /// Schicht der Rohdecke in ihrem Aufbau (Darstellung, Paket 3; das
    /// Modell setzt die Kernschicht des Typs).
    pub core_layer: u8,
}

impl FloorSlab {
    /// Decke über dem geschlossenen Außenwandzug `chain`.
    pub fn from_chain(chain: &WallChain, p: &FloorParams) -> Result<FloorSlab, FloorError> {
        FloorSlab::from_chain_with(chain, p, None)
    }

    /// Wie [`FloorSlab::from_chain`]; mit `strip` liegt die Decke nur
    /// teilweise auf: Umriss = Wandaußenseite um die Streifenbreite nach
    /// innen, davor je Wandsegment ein Randdämmstreifen (K5).
    pub fn from_chain_with(
        chain: &WallChain,
        p: &FloorParams,
        strip: Option<StripParams>,
    ) -> Result<FloorSlab, FloorError> {
        FloorSlab::from_chain_over(chain, p, strip, None)
    }

    /// Wie [`FloorSlab::from_chain_with`]; springt das Geschoss darüber vor
    /// (`over`: Lage der Wand darüber als [`WallChain::overhang_chain`], der
    /// Versatz je Segment und die Untersichtdämmung), folgt der Umriss dort
    /// der Außenseite ihres tragenden Kerns, und unter dem auskragenden
    /// Streifen liegt die Dämmung (G7 K4).
    pub fn from_chain_over(
        chain: &WallChain,
        p: &FloorParams,
        strip: Option<StripParams>,
        over: Option<(&WallChain, &[f64], SoffitParams)>,
    ) -> Result<FloorSlab, FloorError> {
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
        let depth: f64 = match strip {
            Some(sp) => sp.width,
            None => chain.layers[..core].iter().map(|l| l.thickness).sum(),
        };
        let own = chain;
        let chain = over.map_or(chain, |o| o.0);
        let face = chain.face_corners(chain.outer_offset());
        let outline = if depth > 0.0 {
            polygon::inset(&face, depth)
                .map_err(|_| FloorError::NotSimple)?
                .pts
        } else {
            face.clone()
        };
        let mut slab = FloorSlab::from_outline(&outline, p)?;
        if let Some(sp) = strip.filter(|sp| sp.width > 0.0) {
            // Gleiche Eckberechnung wie die Schichtgrenzen der Wand
            let (lo, _) = chain.ref_side.span(chain.thickness());
            let sign = if chain.outer_offset() == lo {
                1.0
            } else {
                -1.0
            };
            let inner = chain.face_corners(chain.outer_offset() + sign * sp.width);
            let n = face.len().min(inner.len());
            slab.strips = (0..chain.segment_count().min(n))
                .map(|k| {
                    let j = (k + 1) % n;
                    [face[k], face[j], inner[j], inner[k]]
                })
                .collect();
            slab.strip = Some(sp);
        }
        if let Some((ext, offsets, sp)) = over.filter(|o| o.2.thickness > 0.0) {
            // Zwischen den Außenseiten der tragenden Kerne unten und oben
            let core_face = |c: &WallChain| {
                let d: f64 = c.layers[..core].iter().map(|l| l.thickness).sum();
                c.face_corners(c.outer_offset() + c.outward_sign() * -d)
            };
            let (a, b) = (core_face(own), core_face(ext));
            let n = a.len().min(b.len());
            slab.soffits = (0..own.segment_count().min(n))
                .filter(|&k| offsets.get(k).is_some_and(|o| *o > 0.0))
                .map(|k| {
                    let j = (k + 1) % n;
                    (k, [a[k], a[j], b[j], b[k]])
                })
                .collect();
            if !slab.soffits.is_empty() {
                slab.soffit = Some(sp);
            }
        }
        Ok(slab)
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
            strip: None,
            strips: Vec::new(),
            strip_covered: Vec::new(),
            soffit: None,
            soffits: Vec::new(),
            terrace: None,
            terraces: TerracePlan::default(),
            core_layer: 0,
        })
    }

    /// Höhenband (Unterkante, Oberkante), in dem die Wände ausgespart werden.
    pub fn band(&self) -> (f64, f64) {
        (self.params.top - self.params.thickness, self.params.top)
    }

    /// Deckenkörper zwischen `z0` und `z1`; ist `cut_top`, trägt die Deckfläche
    /// [`material::CUT`] und ist kräftig umrandet.
    fn prism(&self, z0: f64, z1: f64, cut_top: bool) -> Solid {
        let mut s = Solid {
            mat: self.params.mat,
            layer: self.core_layer,
            ..Solid::default()
        };
        let c = &self.outline;
        let n = c.len();
        s.cap(c, z0, false);
        s.mat = if cut_top {
            self.params.mat | material::CUT
        } else {
            self.params.mat
        };
        s.cap(c, z1, true);
        s.mat = self.params.mat;
        s.sides(c, z0, z1, true);
        for i in 0..n {
            let (a, b) = (c[i], c[(i + 1) % n]);
            s.edge_kind = edge_kind::VIEW;
            s.edge(at_z(a, z0), at_z(b, z0));
            s.edge_kind = if cut_top {
                edge_kind::CUT
            } else {
                edge_kind::VIEW
            };
            s.edge(at_z(a, z1), at_z(b, z1));
            if !straight_at(c, i) {
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

    /// Körper der Untersichtdämmung (K4), leer ohne Vorsprung.
    pub fn soffit_solid(&self) -> Solid {
        match self.soffit_band() {
            Some((z0, z1)) => self.soffit_prisms(z0, z1, false),
            None => Solid::default(),
        }
    }

    /// Untersichtdämmung waagerecht geschnitten in Höhe `cut` (Grundriss).
    pub fn soffit_cut_at(&self, cut: f64) -> Solid {
        match self.soffit_band().filter(|z| cut > z.0) {
            Some((z0, z1)) => self.soffit_prisms(z0, z1.min(cut), cut < z1),
            None => Solid::default(),
        }
    }

    /// Höhenband der Untersichtdämmung (UK Dämmung, UK Decke), falls es sie gibt.
    pub fn soffit_band(&self) -> Option<(f64, f64)> {
        let sp = self.soffit?;
        let (b, _) = self.band();
        Some((b - sp.thickness, b))
    }

    /// Untersichtdämmung zwischen `z0` und `z1`. Sie liegt zwischen der
    /// Decke, der EG-Wand und den herabgezogenen Außenschichten; in 3D
    /// zeichnen diese die Kanten, als Schnitt oben ist sie umrandet.
    fn soffit_prisms(&self, z0: f64, z1: f64, cut_top: bool) -> Solid {
        let Some(sp) = self.soffit else {
            return Solid::default();
        };
        let mut s = Solid {
            mat: sp.mat,
            ..Solid::default()
        };
        for (_, q) in &self.soffits {
            let ring = polygon::to_ccw(q);
            s.cap(&ring, z0, false);
            s.mat = if cut_top {
                sp.mat | material::CUT
            } else {
                sp.mat
            };
            s.cap(&ring, z1, true);
            s.mat = sp.mat;
            s.sides(&ring, z0, z1, true);
            if cut_top {
                s.edge_kind = edge_kind::CUT_LAYER;
                let n = ring.len();
                for i in 0..n {
                    s.edge(at_z(ring[i], z1), at_z(ring[(i + 1) % n], z1));
                }
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
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
        let f = SectionFrame::new(p0, n);
        let (n, along) = (f.n, f.along);
        let pt = |u: f64, z: f64| f.pt(u, z);
        let (zb, zt) = self.band();
        let mut s = Solid {
            mat: self.params.mat | material::CUT,
            edge_kind: edge_kind::CUT,
            layer: self.core_layer,
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

    /// Schnittfläche der Untersichtdämmung (K4): Dämmschraffur, mitteldick
    /// umrandet; oben zeichnet die Decke die Linie.
    pub fn soffit_section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let (Some(sp), Some((z0, z1))) = (self.soffit, self.soffit_band()) else {
            return Solid::default();
        };
        let f = SectionFrame::new(p0, n);
        let (n, along) = (f.n, f.along);
        let pt = |u: f64, z: f64| f.pt(u, z);
        let mut s = Solid {
            mat: sp.mat | material::CUT,
            edge_kind: edge_kind::CUT_LAYER,
            ..Solid::default()
        };
        let t = sp.thickness.max(1.0);
        for (_, q) in &self.soffits {
            for (a, b) in polygon::plane_intervals(q, p0, n, along) {
                s.quad_uv(
                    [pt(a, z0), pt(b, z0), pt(b, z1), pt(a, z1)],
                    n,
                    // Zickzack längs der Schicht (waagerecht), Zacken über
                    // die Dicke: u längs in Dicken, v von unten (0) nach oben
                    [[a / t, 0.0], [b / t, 0.0], [b / t, 1.0], [a / t, 1.0]],
                );
                s.edge(pt(a, z0), pt(b, z0));
                s.edge(pt(a, z0), pt(a, z1));
                s.edge(pt(b, z0), pt(b, z1));
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Fläche der Untersichtdämmung (mm²).
    pub fn soffit_area(&self) -> f64 {
        self.soffits
            .iter()
            .fold(0.0, |a, (_, q)| a + polygon::area(q))
    }

    /// Volumen der Untersichtdämmung (mm³).
    pub fn soffit_volume(&self) -> f64 {
        self.soffit
            .map_or(0.0, |sp| self.soffit_area() * sp.thickness)
    }

    // ---- Dachterrasse, Attika, Attikablech (D1–D3) ----

    /// Höhenband des Terrassenaufbaus (OK Rohdecke, OK Belag).
    pub fn terrace_band(&self) -> Option<(f64, f64)> {
        let t = self
            .terrace
            .as_ref()
            .filter(|_| !self.terraces.is_empty())?;
        Some((self.params.top, self.params.top + t.thickness()))
    }

    /// Höhenband der Attika (Wandkrone darunter, OK Attika).
    pub fn attika_band(&self) -> Option<(f64, f64)> {
        let t = self.terrace.as_ref()?;
        let (_, top) = self.terrace_band()?;
        Some((t.attika_from, top + t.upstand))
    }

    /// Schichten des Aufbaus mit Höhen (UK, OK, Baustoff, Dämmung, Schicht
    /// von oben gezählt), von unten nach oben.
    fn terrace_layers(&self) -> Vec<(f64, f64, u16, bool, u8)> {
        let Some(t) = self.terrace.as_ref().filter(|_| !self.terraces.is_empty()) else {
            return Vec::new();
        };
        let mut z = self.params.top;
        t.layers
            .iter()
            .enumerate()
            .rev()
            .map(|(i, &(d, mat, ins))| {
                z += d;
                (z - d, z, mat, ins, i as u8)
            })
            .collect()
    }

    /// Fläche der Dachterrasse (mm²).
    pub fn terrace_area(&self) -> f64 {
        self.terraces.area()
    }

    /// Volumen je Schicht des Aufbaus von oben nach unten (mm³).
    pub fn terrace_volumes(&self) -> Vec<f64> {
        let a = self.terrace_area();
        self.terrace
            .as_ref()
            .map_or_else(Vec::new, |t| t.layers.iter().map(|l| l.0 * a).collect())
    }

    /// Körper der Dachterrasse für 3D und Ansichten. Ringsum stößt sie an
    /// Attika und Wand darüber: Oberfläche des Belags, Unterseite auf der
    /// Rohdecke und Seiten je Schicht, ohne Kanten der Schichtfugen.
    pub fn terrace_solid(&self) -> Solid {
        let mut s = self.terrace_below(f64::INFINITY);
        if let Some((z0, _)) = self.terrace_band() {
            let layers = self.terrace_layers();
            let top_mat = s.mat;
            s.layer = layers.first().map_or(NO_LAYER, |l| l.4);
            for t in &self.terraces.outlines {
                for p in &t.parts {
                    s.cap(&polygon::to_ccw(p), z0, false);
                }
            }
            // Seiten je Schicht, ohne Kanten: an Wand und Attika liegen sie
            // verdeckt; sind diese ausgeblendet, schließen sie den Körper,
            // und die Unterseite auf der Rohdecke bleibt unsichtbar (sonst
            // flimmerte sie dort mit der Deckfläche, Prüfung p3-1 x)
            for &(a, b, mat, _, li) in &layers {
                (s.mat, s.layer) = (mat, li);
                for t in &self.terraces.outlines {
                    for p in &t.parts {
                        s.sides(&polygon::to_ccw(p), a, b, true);
                    }
                }
            }
            s.mat = top_mat;
        }
        s
    }

    /// Dachterrasse waagerecht geschnitten in Höhe `cut` (Grundriss).
    pub fn terrace_cut_at(&self, cut: f64) -> Solid {
        self.terrace_below(cut)
    }

    fn terrace_below(&self, cut: f64) -> Solid {
        let mut s = Solid::default();
        let layers = self.terrace_layers();
        let Some(&(_, top, mat, _, li)) = layers.iter().rev().find(|l| l.0 < cut) else {
            return s;
        };
        let (z, mat, li) = if cut < top {
            // geschnittene Schicht: die oberste unter dem Schnitt
            let l = layers
                .iter()
                .find(|l| l.0 < cut && cut < l.1)
                .unwrap_or(&layers[0]);
            (cut, l.2 | material::CUT, l.4)
        } else {
            (top, mat, li)
        };
        s.mat = mat;
        s.layer = li;
        s.edge_kind = if cut < top {
            edge_kind::CUT_LAYER
        } else {
            edge_kind::VIEW
        };
        for t in &self.terraces.outlines {
            for p in &t.parts {
                let ring = polygon::to_ccw(p);
                s.cap(&ring, z, true);
                let n = ring.len();
                for i in 0..n {
                    s.edge(at_z(ring[i], z), at_z(ring[(i + 1) % n], z));
                }
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Schnittflächen der Dachterrasse: je Schicht umrandet, Dämmung mit
    /// Schraffur längs (waagerecht), Belag ohne.
    pub fn terrace_section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let f = SectionFrame::new(p0, n);
        let (n, along) = (f.n, f.along);
        let pt = |u: f64, z: f64| f.pt(u, z);
        let mut s = Solid {
            edge_kind: edge_kind::CUT_LAYER,
            ..Solid::default()
        };
        let layers = self.terrace_layers();
        for t in &self.terraces.outlines {
            for p in &t.parts {
                for (a, b) in polygon::plane_intervals(p, p0, n, along) {
                    for &(z0, z1, mat, ins, li) in &layers {
                        s.mat = mat | material::CUT;
                        s.layer = li;
                        let d = (z1 - z0).max(1.0);
                        let uv = if ins {
                            [[a / d, 0.0], [b / d, 0.0], [b / d, 1.0], [a / d, 1.0]]
                        } else {
                            [[0.0; 2]; 4]
                        };
                        s.quad_uv([pt(a, z0), pt(b, z0), pt(b, z1), pt(a, z1)], n, uv);
                        s.edge(pt(a, z1), pt(b, z1));
                        s.edge(pt(a, z0), pt(a, z1));
                        s.edge(pt(b, z0), pt(b, z1));
                    }
                }
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Körper der Attika ohne die Wand (Grundriss eines Geschosses darüber,
    /// in dem die Wand selbst nicht erscheint).
    pub fn attika_solid(&self) -> Solid {
        match self.attika_band() {
            Some(b) => attika_solid(&self.terraces.attika, b, f64::INFINITY),
            None => Solid::default(),
        }
    }

    /// Körper des Attikablechs (D3): Profil längs der Außenfläche der
    /// Attika auf OK Attika, Enden an der Außenfläche des Geschosses darüber.
    pub fn coping_solid(&self) -> Solid {
        let (Some(t), Some((_, z))) = (&self.terrace, self.attika_band()) else {
            return Solid::default();
        };
        let mut s = Solid {
            mat: t.coping_mat,
            ..Solid::default()
        };
        let profile = coping_profile(self.terraces.width);
        for c in &self.terraces.coping {
            let path: Vec<Vec3> = c.points.iter().map(|p| at_z(*p, z)).collect();
            s.sweep(&path, c.closed, &profile, c.ends);
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Schnittfläche des Attikablechs: das Profil in der Ebene, kräftig
    /// umrandet.
    pub fn coping_section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let (Some(t), Some((_, z))) = (&self.terrace, self.attika_band()) else {
            return Solid::default();
        };
        let nn = vec3(n.x, n.y, 0.0).normalized();
        let mut s = Solid {
            mat: t.coping_mat | material::CUT,
            edge_kind: edge_kind::CUT,
            ..Solid::default()
        };
        let profile = coping_profile(self.terraces.width);
        let flat2: Vec<Vec3> = profile.iter().map(|p| vec3(p.0, p.1, 0.0)).collect();
        let tris = polygon::triangulate(&flat2);
        for c in &self.terraces.coping {
            let m = c.points.len();
            let segs = if c.closed { m } else { m.saturating_sub(1) };
            for k in 0..segs {
                let (a, b) = (c.points[k], c.points[(k + 1) % m]);
                let (da, db) = ((a - p0).dot(nn), (b - p0).dot(nn));
                if (da < 0.0) == (db < 0.0) || (da - db).abs() < 1e-9 {
                    continue;
                }
                let x = a + (b - a) * (da / (da - db));
                let d = vec3(b.x - a.x, b.y - a.y, 0.0).normalized();
                let r = right_of(d);
                let dn = d.dot(nn);
                // Querrichtung in der Ebene (das Profil geschert)
                let q = r - d * (r.dot(nn) / dn);
                let at = |(u, h): (f64, f64)| vec3(x.x, x.y, z) + q * u + vec3(0.0, 0.0, h);
                for tri in &tris {
                    s.oriented_tri(tri.map(|i| at(profile[i])), nn);
                }
                let np = profile.len();
                for i in 0..np {
                    s.edge(at(profile[i]), at(profile[(i + 1) % np]));
                }
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Länge des Attikablechs an der Außenkante der Attika (mm).
    pub fn coping_length(&self) -> f64 {
        self.terraces.coping_length()
    }

    /// Schnittflächen der Attika ohne die Wand.
    pub fn attika_section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        match self.attika_band() {
            Some(b) => attika_section_caps(&self.terraces.attika, b, p0, n),
            None => Solid::default(),
        }
    }

    // ---- Randdämmstreifen (K5) ----

    /// Streifen `k` zwischen `z0` und `z1`. In 3D trägt er die Oberfläche der
    /// Wand und hat keine Kanten außer den Gebäudeecken außen (die Wand
    /// läuft fugenlos durch); als Schnitt oben trägt er seine Schraffur.
    fn strip_prism(&self, k: usize, z0: f64, z1: f64, cut_top: bool) -> Solid {
        let (Some(sp), Some(q)) = (self.strip, self.strips.get(k)) else {
            return Solid::default();
        };
        let mut s = Solid {
            mat: sp.face,
            ..Solid::default()
        };
        let [o0, o1, i1, i0] = *q;
        let up = vec3(0.0, 0.0, z1 - z0);
        let z = |p: Vec3| at_z(p, z0);
        let d = (o1 - o0).normalized();
        let mut out = right_of(d);
        if out.dot(o0 - i0) < 0.0 {
            out = -out;
        }
        // Umlauf außen → innen so, dass die Flächen nach außen zeigen
        let ring = if right_of(d).dot(out) > 0.0 {
            [o0, o1, i1, i0]
        } else {
            [o1, o0, i0, i1]
        };
        let [a, b, c, e] = ring.map(z);
        s.quad(a, e, c, b, vec3(0.0, 0.0, -1.0));
        s.mat = if cut_top {
            sp.mat | material::CUT
        } else {
            sp.face
        };
        s.quad(a + up, b + up, c + up, e + up, vec3(0.0, 0.0, 1.0));
        s.mat = sp.face;
        s.quad(z(o0), z(o1), z(o1) + up, z(o0) + up, out);
        s.quad(z(i1), z(i0), z(i0) + up, z(i1) + up, -out);
        s.edge_kind = edge_kind::VIEW;
        let n = self.strips.len();
        let prev = self.strips[(k + n - 1) % n];
        let pd = (prev[1] - prev[0]).normalized();
        let straight = (pd.x * d.y - pd.y * d.x).abs() < 1e-9 && pd.dot(d) > 0.0;
        if !straight {
            s.edge(z(o0), z(o0) + up);
        }
        if cut_top {
            s.edge_kind = edge_kind::CUT;
        }
        // Oben außen: Gebäudekante; steht die OG-Wand darauf, nimmt
        // merge_seam sie mit deren Fußkante weg (gleiche Oberfläche)
        s.edge(z(o0) + up, z(o1) + up);
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Körper des Streifens `k` für 3D und Ansichten.
    pub fn strip_solid(&self, k: usize) -> Solid {
        let (b, t) = self.band();
        self.strip_prism(k, b, t, false)
    }

    /// Streifen `k` waagerecht geschnitten in Höhe `cut` (Grundriss), wie
    /// [`FloorSlab::solid_cut_at`].
    pub fn strip_cut_at(&self, k: usize, cut: f64) -> Solid {
        let (b, t) = self.band();
        if cut <= b {
            Solid::default()
        } else if cut >= t {
            self.strip_solid(k)
        } else {
            self.strip_prism(k, b, cut, true)
        }
    }

    /// Schnittfläche des Streifens `k` mit der senkrechten Ebene durch `p0`
    /// (F4): Dämmschraffur, ohne Linie zur Wand darüber und darunter; nur
    /// die Außenkontur der Wand läuft durch.
    pub fn strip_section_caps(&self, k: usize, p0: Vec3, n: Vec3) -> Solid {
        let (Some(sp), Some(q)) = (self.strip, self.strips.get(k)) else {
            return Solid::default();
        };
        let f = SectionFrame::new(p0, n);
        let (n, along) = (f.n, f.along);
        let pt = |u: f64, z: f64| f.pt(u, z);
        let (zb, zt) = self.band();
        let mut s = Solid {
            mat: sp.mat | material::CUT,
            edge_kind: edge_kind::CUT,
            ..Solid::default()
        };
        let (o0, o1) = (q[0], q[1]);
        let on_outer = |p: Vec3| {
            let d = o1 - o0;
            let len = d.length();
            if len < 1e-9 {
                return false;
            }
            let r = vec3(p.x, p.y, 0.0) - vec3(o0.x, o0.y, 0.0);
            (r.x * d.y - r.y * d.x).abs() / len < 1e-3
        };
        // Musterkoordinaten wie in der Wand: u in Streifenbreiten nach oben,
        // v quer von außen (0) nach innen (1)
        let w = sp.width.max(1e-9);
        let v = |u: f64| {
            let d = o1 - o0;
            let r = pt(u, 0.0) - vec3(o0.x, o0.y, 0.0);
            ((r.x * d.y - r.y * d.x).abs() / d.length().max(1e-9) / w).min(1.0)
        };
        for (a, b) in polygon::plane_intervals(q, p0, n, along) {
            let (ub, ut) = (zb / w, zt / w);
            s.quad_uv(
                [pt(a, zb), pt(b, zb), pt(b, zt), pt(a, zt)],
                n,
                [[ub, v(a)], [ub, v(b)], [ut, v(b)], [ut, v(a)]],
            );
            for u in [a, b] {
                if on_outer(pt(u, 0.0)) {
                    s.edge(pt(u, zb), pt(u, zt));
                }
            }
            // Oberste Decke oder Wand darüber versetzt: der Streifen schließt
            // die Kontur oben
            if !self.strip_covered.get(k).copied().unwrap_or(sp.covered) {
                s.edge(pt(a, zt), pt(b, zt));
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Länge des Streifens `k` auf seiner Achse (Mittel aus außen und innen), mm.
    pub fn strip_length(&self, k: usize) -> f64 {
        self.strips.get(k).map_or(0.0, |q| {
            ((q[1] - q[0]).length() + (q[2] - q[3]).length()) * 0.5
        })
    }

    /// Volumen des Streifens `k` (Grundfläche × Deckendicke), mm³.
    pub fn strip_volume(&self, k: usize) -> f64 {
        self.strips
            .get(k)
            .map_or(0.0, |q| polygon::area(q) * self.params.thickness)
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
    /// AW 31,5 (14 Dämmung, 17,5 Porenbeton), Wandhöhe 3,50 m.
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
        // Decke + Porenbeton netto = Porenbeton brutto + Decke − Tasche
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
        // Keine Porenbeton-Kante im Deckenband (2110 < z < 2330)
        let through = caps.edges.iter().any(|e| {
            let (lo, hi) = (e.a.z.min(e.b.z), e.a.z.max(e.b.z));
            lo < 2329.0 && hi > 2111.0 && e.kind == edge_kind::CUT
        });
        assert!(!through);
        // Tasche: Porenbeton-Kontur oben und unten waagerecht, kräftig
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

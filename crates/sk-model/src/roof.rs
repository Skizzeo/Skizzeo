//! Flachdach: waagerechter Dachaufbau zwischen den Aufkantungen und
//! Attikablech auf ihrer Krone (Jörn 10.10., Plan Flachdach D3, D4).
//!
//! Der Aufbau liegt auf der obersten Rohdecke und füllt die Innenfläche der
//! Aufkantung; seine Schichten stehen von oben nach unten im Typ. Das Blech
//! läuft als geschlossener Ring auf der Außenfläche der Aufkantung über die
//! ganze Wanddicke und zählt nicht zur Höhe der Ebene.

use crate::gefaelle::SlopeField;
use crate::solid::{at_z, edge_kind, material, right_of, SectionFrame, Solid, SweepEnd, NO_LAYER};
use crate::terrace::{coping_girth_with, coping_profile_with, ROOF_COPING_INNER};
use sk_math::{polygon, vec3, Vec3};

/// Dachaufbau und Attikablech über einer Aufkantung.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatRoof {
    /// Innenfläche der Aufkantung, gegen den Uhrzeigersinn, auf z = 0.
    pub outline: Vec<Vec3>,
    /// OK Rohdecke (absolut).
    pub base: f64,
    /// Schichten von oben nach unten: Dicke (mm), Baustoff
    /// (Darstellungsschlüssel), Dämmschicht (Schraffur längs).
    pub layers: Vec<(f64, u16, bool)>,
    /// Außenfläche der Aufkantung, gegen den Uhrzeigersinn (außen liegt
    /// rechts), auf z = 0: Pfad des Blechs.
    pub ring: Vec<Vec3>,
    /// Dicke der Aufkantung, zugleich Breite des Blechs (mm).
    pub width: f64,
    /// OK Aufkantung (absolut), Unterseite des Blechs.
    pub crown: f64,
    /// Baustoff des Blechs (Darstellungsschlüssel).
    pub coping_mat: u16,
    /// Gefälleplan, wenn das Dach Gefälle hat: der Keil liegt in der
    /// Dämmschicht `tapered` und hebt die Schichten darüber an.
    pub slope: Option<SlopeField>,
    /// Gefälledämmschicht (von oben gezählt), nur mit `slope`.
    pub tapered: Option<usize>,
}

impl FlatRoof {
    /// Dicke des Aufbaus (mm).
    pub fn thickness(&self) -> f64 {
        self.layers.iter().map(|l| l.0).sum()
    }

    /// Höhenband des Aufbaus (OK Rohdecke, OK Dachhaut).
    pub fn band(&self) -> (f64, f64) {
        (self.base, self.base + self.thickness())
    }

    /// Anschlusshöhe: OK Aufkantung über OK Dachhaut (mm).
    pub fn upstand(&self) -> f64 {
        self.crown - self.band().1
    }

    /// Fläche des Aufbaus (mm²).
    pub fn area(&self) -> f64 {
        polygon::area(&self.outline)
    }

    /// Volumen je Schicht von oben nach unten (mm³).
    pub fn volumes(&self) -> Vec<f64> {
        let a = self.area();
        self.layers.iter().map(|l| l.0 * a).collect()
    }

    /// Anschlusslänge an der Innenfläche der Aufkantung (mm).
    pub fn edge_length(&self) -> f64 {
        polygon::perimeter(&self.outline)
    }

    /// Innenecken des Anschlusses an der Aufkantung (Stück): Knicke der
    /// Innenfläche ab 5°, an denen die Dachfläche konvex ist. Einspringende
    /// Ecken (Außenecken des Anschlusses) und gerade Zwischenpunkte zählen
    /// nicht (Review fe0e1ed).
    pub fn corners(&self) -> usize {
        let p = &self.outline;
        let n = p.len();
        if n < 3 {
            return 0;
        }
        // Umlaufsinn aus der vorzeichenbehafteten Fläche
        let sinn: f64 = (0..n)
            .map(|k| {
                let (a, b) = (p[k], p[(k + 1) % n]);
                a.x * b.y - b.x * a.y
            })
            .sum();
        let grenze = 5.0f64.to_radians().cos();
        (0..n)
            .filter(|&k| {
                let (a, b, c) = (p[(k + n - 1) % n], p[k], p[(k + 1) % n]);
                let (u, v) = ((b - a).normalized(), (c - b).normalized());
                let kreuz = u.x * v.y - u.y * v.x;
                u.dot(v) < grenze && kreuz * sinn > 0.0
            })
            .count()
    }

    /// Schichten mit Höhen (UK, OK, Baustoff, Dämmung, Schicht von oben
    /// gezählt), von unten nach oben.
    fn bands(&self) -> Vec<(f64, f64, u16, bool, u8)> {
        let mut z = self.base;
        self.layers
            .iter()
            .enumerate()
            .rev()
            .map(|(i, &(d, mat, ins))| {
                z += d;
                (z - d, z, mat, ins, i as u8)
            })
            .collect()
    }

    /// Körper für 3D und Ansichten: Oberfläche der Dachhaut, Unterseite auf
    /// der Rohdecke und Seiten je Schicht an der Aufkantung, ohne Kanten
    /// der Schichtfugen (wie die Dachterrasse).
    pub fn solid(&self) -> Solid {
        let mut s = self.below(f64::INFINITY);
        let bands = self.bands();
        if bands.is_empty() {
            return s;
        }
        let top_mat = s.mat;
        s.layer = bands.first().map_or(NO_LAYER, |l| l.4);
        s.cap(&self.outline, self.base, false);
        for &(a, b, mat, _, li) in &bands {
            (s.mat, s.layer) = (mat, li);
            s.sides(&self.outline, a, b, true);
        }
        s.mat = top_mat;
        s
    }

    /// Aufbau waagerecht geschnitten in Höhe `cut` (Grundriss).
    pub fn cut_at(&self, cut: f64) -> Solid {
        self.below(cut)
    }

    fn below(&self, cut: f64) -> Solid {
        let mut s = Solid::default();
        let bands = self.bands();
        let Some(&(_, top, mat, _, li)) = bands.iter().rev().find(|l| l.0 < cut) else {
            return s;
        };
        let (z, mat, li) = if cut < top {
            let l = bands
                .iter()
                .find(|l| l.0 < cut && cut < l.1)
                .unwrap_or(&bands[0]);
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
        let ring = &self.outline;
        s.cap(ring, z, true);
        let n = ring.len();
        for i in 0..n {
            s.edge(at_z(ring[i], z), at_z(ring[(i + 1) % n], z));
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Schnittflächen des Aufbaus: je Schicht umrandet, Dämmung mit
    /// Schraffur längs (waagerecht), Dampfsperre und Abdichtung ohne.
    pub fn section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let f = SectionFrame::new(p0, n);
        let (n, along) = (f.n, f.along);
        let pt = |u: f64, z: f64| f.pt(u, z);
        let mut s = Solid {
            edge_kind: edge_kind::CUT_LAYER,
            ..Solid::default()
        };
        let bands = self.bands();
        for (a, b) in polygon::plane_intervals(&self.outline, p0, n, along) {
            for &(z0, z1, mat, ins, li) in &bands {
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
        s.edge_kind = edge_kind::VIEW;
        s
    }

    // ---- Attikablech (D4) ----

    /// Querschnitt des Blechs (Innenschenkel 50 über dem Hochzug).
    fn profile(&self) -> Vec<(f64, f64)> {
        coping_profile_with(self.width, ROOF_COPING_INNER)
    }

    /// Abwicklung des Blechs (mm).
    pub fn coping_girth(&self) -> f64 {
        coping_girth_with(self.width, ROOF_COPING_INNER)
    }

    /// Länge des Blechs an der Außenkante der Aufkantung (mm).
    pub fn coping_length(&self) -> f64 {
        polygon::perimeter(&self.ring)
    }

    /// Körper des Blechs: Profil als geschlossener Ring auf OK Aufkantung.
    pub fn coping_solid(&self) -> Solid {
        let mut s = Solid {
            mat: self.coping_mat,
            ..Solid::default()
        };
        let path: Vec<Vec3> = self.ring.iter().map(|p| at_z(*p, self.crown)).collect();
        s.sweep(&path, true, &self.profile(), [SweepEnd::Square; 2]);
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Schnittfläche des Blechs: das Profil in der Ebene, kräftig umrandet.
    pub fn coping_section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let nn = vec3(n.x, n.y, 0.0).normalized();
        let mut s = Solid {
            mat: self.coping_mat | material::CUT,
            edge_kind: edge_kind::CUT,
            ..Solid::default()
        };
        let profile = self.profile();
        let flat: Vec<Vec3> = profile.iter().map(|p| vec3(p.0, p.1, 0.0)).collect();
        let tris = polygon::triangulate(&flat);
        let z = self.crown;
        let m = self.ring.len();
        for k in 0..m {
            let (a, b) = (self.ring[k], self.ring[(k + 1) % m]);
            let (da, db) = ((a - p0).dot(nn), (b - p0).dot(nn));
            if (da < 0.0) == (db < 0.0) || (da - db).abs() < 1e-9 {
                continue;
            }
            let x = a + (b - a) * (da / (da - db));
            let d = vec3(b.x - a.x, b.y - a.y, 0.0).normalized();
            let r = right_of(d);
            // Querrichtung in der Ebene (das Profil geschert)
            let q = r - d * (r.dot(nn) / d.dot(nn));
            let at = |(u, h): (f64, f64)| vec3(x.x, x.y, z) + q * u + vec3(0.0, 0.0, h);
            for tri in &tris {
                s.oriented_tri(tri.map(|i| at(profile[i])), nn);
            }
            let np = profile.len();
            for i in 0..np {
                s.edge(at(profile[i]), at(profile[(i + 1) % np]));
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }
}

// ---- Automatikmengen (`roof.*`, Plan Flachdach D3, für die Leistungen K3) ----

pub const EDGE: &str = "roof.edge";
pub const CORNERS: &str = "roof.corners";
pub const DRAINS: &str = "roof.drains";
pub const OVERFLOWS: &str = "roof.overflows";

/// Schlüssel des Flachdachs (`[service] auto=`), Einheit und Bedeutung.
pub const SCHLUESSEL: [(&str, &str, &str); 4] = [
    (
        EDGE,
        "m",
        "Anschluss der Abdichtung an die Aufkantung: Länge an ihrer Innenfläche",
    ),
    (
        CORNERS,
        "st",
        "Innenecken des Anschlusses an der Aufkantung (Knick ab 5°, ohne einspringende Ecken)",
    ),
    (
        DRAINS,
        "st",
        "Dachabläufe: 1 je angefangene 150 m² Dachfläche",
    ),
    (OVERFLOWS, "st", "Notüberläufe: so viele wie Dachabläufe"),
];

/// Dachfläche je Ablauf (mm², Schätzung des Plans Flachdach D3; die
/// Bemessung nach DIN 1986-100 macht der Fachplaner).
pub const DRAIN_AREA: f64 = 150.0e6;

/// Kostengruppe der Dachbeläge samt Entwässerung (DIN 276: 363).
pub const KG_ROOF: u16 = 363;

/// Automatikmengen eines Flachdachs: Anschlusslänge, Ecken, Abläufe und
/// Notüberläufe. `vorlage` trägt Bauteil, Nummer und Geschoss (den
/// Dachaufbau).
pub fn roof_mengen(vorlage: &crate::qto::AutoMenge, r: &FlatRoof) -> Vec<crate::qto::AutoMenge> {
    let m = |mm: f64| format!("{:.2} m", mm / 1000.0).replace('.', ",");
    let area = r.area();
    let drains = (area / DRAIN_AREA).ceil().max(1.0);
    let menge = |key, unit, value, formula| crate::qto::AutoMenge {
        key,
        unit,
        value,
        kg: Some(KG_ROOF),
        formula,
        ..vorlage.clone()
    };
    let corners = r.corners() as f64;
    let fl = format!("{:.2} m²", area / 1e6).replace('.', ",");
    vec![
        menge(
            EDGE,
            "m",
            r.edge_length(),
            format!("Innenfläche der Aufkantung {}", m(r.edge_length())),
        ),
        menge(CORNERS, "st", corners, format!("{corners} Innenecken")),
        menge(
            DRAINS,
            "st",
            drains,
            format!("{fl} ÷ 150 m², aufgerundet, mindestens 1"),
        ),
        menge(OVERFLOWS, "st", drains, "so viele wie Dachabläufe".into()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L-förmige Innenfläche mit einem geraden Zwischenpunkt: 6 Knicke,
    /// davon einer einspringend, also 5 Innenecken; in beiden Umlaufrichtungen.
    #[test]
    fn innenecken_ohne_einspringende() {
        let mut outline = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(5000.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 4000.0, 0.0),
            vec3(4000.0, 4000.0, 0.0),
            vec3(4000.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        let mut r = FlatRoof {
            outline: outline.clone(),
            base: 0.0,
            layers: Vec::new(),
            ring: Vec::new(),
            width: 0.0,
            crown: 0.0,
            coping_mat: 0,
            slope: None,
            tapered: None,
        };
        assert_eq!(r.corners(), 5);
        outline.reverse();
        r.outline = outline;
        assert_eq!(r.corners(), 5);
    }
}

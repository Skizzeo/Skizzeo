//! Lochmaße der Öffnungen je Wandschicht (Paket O2) für die Mengen und die
//! Abrechnung nach VOB/C (Vorgabe `vob/oeffnung-vorgabe.md` §5): je Öffnung
//! und getroffener Schicht eines Segments Breite, Höhe, Fläche und Volumen
//! des Lochs dieser Schicht (mit Anschlag), Laibungslängen und -tiefe, Art
//! und ob eine Brüstung da ist. Das Volumen der Schicht ist um jedes Loch
//! gemindert (geometrisch); über den Abzug von der Abrechnungsfläche
//! entscheidet die VOB-Regel, die Teilstücke einer Ecköffnung oder einer
//! geschossübergreifenden Öffnung über die Kennung zusammenzählt.

use crate::oeffnung::OpeningKind;
use crate::wall::WallChain;
use crate::{ElementId, Model};
use sk_math::Vec3;

/// Loch einer Öffnung in einer Wandschicht eines Segments. Maße in mm.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerHole {
    pub opening: ElementId,
    pub kind: OpeningKind,
    /// Breite längs der Wand im Segment (Teilstück an einer Ecke).
    pub width: f64,
    /// Höhe in der Schicht dieser Wand (Teilstück über Geschosse).
    pub height: f64,
    pub area: f64,
    /// Fläche × Schichtdicke.
    pub volume: f64,
    /// Unterkante über OK Rohdecke des Geschosses der Öffnung (Fenster);
    /// sonst Schwelle (Terrassentür, Haustür).
    pub sill: bool,
    /// Laibungstiefe: Dicke der Schicht.
    pub depth: f64,
    /// Laibungslängen links, rechts (je Höhe), oben, unten (je Breite), von
    /// außen gesehen; 0, wo die Schicht dort keine Laibung zeigt (Ecke,
    /// anderes Geschoss).
    pub reveal: [f64; 4],
}

/// Löcher in Schicht `li` von Segment `k`; `fa`, `fb`: Eckpunkte der beiden
/// Flächen der Schicht.
pub(crate) fn layer_holes(
    model: &Model,
    chain: &WallChain,
    k: usize,
    li: usize,
    (fa, fb): (&[Vec3], &[Vec3]),
    thickness: f64,
) -> Vec<LayerHole> {
    let mut out = Vec::new();
    let Some((pts, _, dirs)) = chain.layout() else {
        return out;
    };
    let (Some(&d), Some(&p0)) = (dirs.get(k), pts.get(k)) else {
        return out;
    };
    let j = (k + 1) % fa.len().max(1);
    let (Some(a0), Some(a1), Some(b0), Some(b1)) = (fa.get(k), fa.get(j), fb.get(k), fb.get(j))
    else {
        return out;
    };
    let along = |p: &Vec3| (*p - p0).dot(d);
    let (ua, ub) = (0.5 * (along(a0) + along(b0)), 0.5 * (along(a1) + along(b1)));
    let parts = chain.layer_parts(li);
    let eps = 1e-6;
    for h in chain.joints.holes.iter().filter(|h| h.seg == k) {
        let Some(id) = h.opening else {
            continue;
        };
        let (Some(o), Some(e)) = (model.opening(id), model.element(id)) else {
            continue;
        };
        let ((u0, u1), (z0, z1)) = chain.loch_in(h, li);
        let width = u1.min(ub) - u0.max(ua);
        let height: f64 = parts
            .iter()
            .map(|p| (z1.min(p.1) - z0.max(p.0)).max(0.0))
            .sum();
        if width <= eps || height <= eps {
            continue;
        }
        let inside = |z: f64| parts.iter().any(|p| z > p.0 + eps && z < p.1 - eps);
        let side = |u: f64| {
            if u.is_finite() && u > ua + eps && u < ub - eps {
                height
            } else {
                0.0
            }
        };
        let (at0, at1) = (side(u0), side(u1));
        let (left, right) = if h.u_right { (at0, at1) } else { (at1, at0) };
        let floor = model
            .storey(e.storey)
            .map_or(f64::NEG_INFINITY, |s| s.elevation);
        let area = width * height;
        out.push(LayerHole {
            opening: id,
            kind: o.kind,
            width,
            height,
            area,
            volume: area * thickness,
            sill: h.z.0 > floor + 1.0,
            depth: thickness,
            reveal: [
                left,
                right,
                if inside(z1) { width } else { 0.0 },
                if inside(z0) { width } else { 0.0 },
            ],
        });
    }
    out
}

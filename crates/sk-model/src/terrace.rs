//! Dachterrasse: Fläche über dem Rücksprung des Geschosses darüber
//! (Jörn 07.10. 08:31–08:39, BIM paket-dachterrasse.md, Review 2a R8).
//!
//! Das Spiegelbild der Untersichtdämmung (G7 K4): Springt eine Wand darüber
//! zurück, liegt zwischen ihrer Außenfläche und der Deckenkante (Außenfläche
//! des tragenden Kerns darunter) ein Streifen der Rohdecke frei. Benachbarte
//! zurückspringende Segmente bilden einen zusammenhängenden Umriss über die
//! Ecke; neben einem bündigen oder vorspringenden Segment endet er an dessen
//! Deckenkante.

use crate::model::MIN_OFFSET;
use crate::solid::right_of;
use crate::wall::WallChain;
use sk_math::{polygon, Vec3};

/// Eine Dachterrasse: die zurückspringenden Segmente (in Laufrichtung) und
/// ihr Umriss als einfache Polygone. Eine Folge ist ein Polygon; springt das
/// Geschoss ringsum zurück, ist die Fläche ein Ring und wird in zwei Teile
/// geteilt.
#[derive(Clone, Debug, PartialEq)]
pub struct TerraceOutline {
    pub segments: Vec<usize>,
    pub parts: Vec<Vec<Vec3>>,
}

impl TerraceOutline {
    /// Fläche (mm²).
    pub fn area(&self) -> f64 {
        self.parts.iter().map(|p| polygon::area(p)).sum()
    }
}

/// Schnittpunkt der Geraden durch `a` (Richtung `da`) und durch `b`
/// (Richtung `db`); parallel: Fußpunkt von `b` auf der ersten Geraden.
fn meet(a: Vec3, da: Vec3, b: Vec3, db: Vec3) -> Vec3 {
    let c = da.x * db.y - da.y * db.x;
    if c.abs() < 1e-9 * da.length() * db.length() {
        let u = da.normalized();
        return a + u * (b - a).dot(u);
    }
    let w = b - a;
    a + da * ((w.x * db.y - w.y * db.x) / c)
}

/// Terrassen über dem geschlossenen Zug `slab` (Deckenumriss: Zug darunter,
/// bei einem Vorsprung in OG-Lage) unter dem Zug `up` darüber. Ein Segment
/// trägt Terrasse, wenn zwischen der Außenfläche von `up` und der
/// Kernaußenfläche von `slab` lichte Tiefe von mindestens [`MIN_OFFSET`]
/// bleibt (BIM Regel 41). Leer, wenn die Züge nicht zusammenpassen.
pub fn terrace_outlines(slab: &WallChain, up: &WallChain) -> Vec<TerraceOutline> {
    let n = slab.segment_count();
    if !slab.closed || !up.closed || up.segment_count() != n || n < 3 {
        return Vec::new();
    }
    let core = slab.layers.iter().position(|l| l.core).unwrap_or(0);
    let ext: f64 = slab.layers[..core].iter().map(|l| l.thickness).sum();
    let lc = slab.face_corners(slab.outer_offset() - slab.outward_sign() * ext);
    let mc = up.face_corners(up.outer_offset());
    if lc.len() != n || mc.len() != n {
        return Vec::new();
    }
    // Außen = rechts der Laufrichtung bei Umlauf gegen den Uhrzeigersinn
    let sign = if polygon::signed_area(&lc) > 0.0 {
        1.0
    } else {
        -1.0
    };
    let dir = |c: &[Vec3], k: usize| c[(k + 1) % n] - c[k];
    let terrace: Vec<bool> = (0..n)
        .map(|k| {
            let d = dir(&lc, k);
            if d.length() < 1e-9 || dir(&mc, k).length() < 1e-9 {
                return false;
            }
            let out = right_of(d.normalized()) * sign;
            // M parallel zu L (Regel 29): Abstand an einem Punkt genügt
            (lc[k] - mc[k]).dot(out) >= MIN_OFFSET - 1e-6
        })
        .collect();
    // Umriss der Folge ab Segment `a` mit `len` Segmenten; `split`: an
    // beiden Enden liegt eine weitere Terrasse (Ring), dort die Gehrung
    let part = |a: usize, len: usize, split: bool| -> Option<Vec<Vec3>> {
        let b = (a + len - 1) % n;
        let (prev, next) = ((a + n - 1) % n, (b + 1) % n);
        // Anfang und Ende innen: an der Gehrung oder auf der Deckenkante des
        // Nachbarn
        let start = if split {
            mc[a]
        } else {
            meet(mc[a], dir(&mc, a), lc[prev], dir(&lc, prev))
        };
        let end = if split {
            mc[next]
        } else {
            meet(mc[b], dir(&mc, b), lc[next], dir(&lc, next))
        };
        let mut p = vec![start];
        p.extend((0..=len).map(|i| lc[(a + i) % n]));
        p.push(end);
        p.extend((1..len).rev().map(|i| mc[(a + i) % n]));
        let p = polygon::simplified(&p);
        (p.len() >= 3 && polygon::is_simple(&p) && polygon::area(&p) > 1.0).then_some(p)
    };
    if terrace.iter().all(|t| *t) {
        let h = n / 2;
        let parts: Option<Vec<_>> = [(0, h), (h, n - h)]
            .into_iter()
            .map(|(a, len)| part(a, len, true))
            .collect();
        return parts
            .map(|parts| TerraceOutline {
                segments: (0..n).collect(),
                parts,
            })
            .into_iter()
            .collect();
    }
    (0..n)
        .filter(|&k| terrace[k] && !terrace[(k + n - 1) % n])
        .filter_map(|a| {
            let segments: Vec<usize> = (0..n)
                .map(|i| (a + i) % n)
                .take_while(|k| terrace[*k])
                .collect();
            let p = part(a, segments.len(), false)?;
            Some(TerraceOutline {
                segments,
                parts: vec![p],
            })
        })
        .collect()
}

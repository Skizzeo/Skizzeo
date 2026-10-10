//! Dachterrasse: Fläche über dem Rücksprung des Geschosses darüber
//! (Jörn 07.10. 08:31–08:39, BIM paket-dachterrasse.md, Review 2a R8).
//!
//! Das Spiegelbild der Untersichtdämmung (G7 K4): Springt eine Wand darüber
//! zurück, liegt zwischen ihrer Außenfläche und der Deckenkante (Außenfläche
//! des tragenden Kerns darunter) ein Streifen der Rohdecke frei. Benachbarte
//! zurückspringende Segmente bilden einen zusammenhängenden Umriss über die
//! Ecke; neben einem bündigen oder vorspringenden Segment endet er an dessen
//! Deckenkante.
//!
//! Dazu der Plan für Attika und Attikablech (D2, D3): Über jedem
//! Terrassenrand laufen die Schichten der Wand darunter, die außerhalb der
//! Deckenkante liegen, bis OK Attika weiter (BIM E2), an den Nachbarn bis
//! zur Außenfläche des Geschosses darüber; das Blech folgt der Außenfläche
//! der Attika und endet dort an derselben Fläche.

use crate::model::MIN_OFFSET;
use crate::solid::{at_z, edge_kind, material, right_of, SectionFrame, Solid, SweepEnd, NO_LAYER};
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
    /// Größte lichte Tiefe der Segmente (mm), für „begehbar“.
    pub depth: f64,
}

impl TerraceOutline {
    /// Fläche (mm²).
    pub fn area(&self) -> f64 {
        self.parts.iter().map(|p| polygon::area(p)).sum()
    }
}

/// Ein Stück Attika: Schicht `layer` der Wand darunter über Segment `seg`,
/// Grundriss auf z = 0 (außen Anfang, außen Ende, innen Ende, innen
/// Anfang, in Laufrichtung des Segments). `layer` ist `None` für die Zone
/// vor der Deckenkante einer Wand mit Randdämmstreifen (K5).
#[derive(Clone, Debug, PartialEq)]
pub struct AttikaPiece {
    pub seg: usize,
    pub layer: Option<usize>,
    pub quad: [Vec3; 4],
    /// Darstellungsschlüssel des Baustoffs.
    pub mat: u16,
    /// Endet am Anfang bzw. Ende an der Außenfläche des Geschosses darüber
    /// (Stirnfläche) statt in einer Gehrung.
    pub free: [bool; 2],
    /// Liegt außen bzw. innen auf der Kontur der Attika (sonst Schichtfuge).
    pub contour: [bool; 2],
}

/// Pfad des Attikablechs auf der Außenfläche der Attika, so gerichtet,
/// dass außen rechts liegt (Profil quer nach außen positiv).
#[derive(Clone, Debug, PartialEq)]
pub struct CopingPath {
    pub points: Vec<Vec3>,
    pub closed: bool,
    pub ends: [SweepEnd; 2],
}

impl CopingPath {
    /// Länge an der Außenkante der Attika (mm, BIM §6).
    pub fn length(&self) -> f64 {
        let n = self.points.len();
        let segs = if self.closed { n } else { n.saturating_sub(1) };
        (0..segs)
            .map(|k| (self.points[(k + 1) % n] - self.points[k]).length())
            .sum()
    }
}

/// Terrassen, Attika und Blechpfad über einem Zug.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerracePlan {
    pub outlines: Vec<TerraceOutline>,
    pub attika: Vec<AttikaPiece>,
    pub coping: Vec<CopingPath>,
    /// Breite der Attika (Außenfläche bis Deckenkante), mm.
    pub width: f64,
}

impl TerracePlan {
    pub fn is_empty(&self) -> bool {
        self.outlines.is_empty()
    }

    /// Fläche aller Terrassen (mm²).
    pub fn area(&self) -> f64 {
        self.outlines.iter().map(|t| t.area()).sum()
    }

    /// Größte lichte Tiefe (mm).
    pub fn depth(&self) -> f64 {
        self.outlines.iter().fold(0.0, |a, t| a.max(t.depth))
    }

    /// Länge des Blechs an der Außenkante (mm).
    pub fn coping_length(&self) -> f64 {
        self.coping.iter().map(|c| c.length()).sum()
    }
}

/// Terrassen über dem geschlossenen Zug `slab` (Deckenumriss: Zug darunter,
/// bei einem Vorsprung in OG-Lage) unter dem Zug `up` darüber. Ein Segment
/// trägt Terrasse, wenn zwischen der Außenfläche von `up` und der
/// Kernaußenfläche von `slab` lichte Tiefe von mindestens [`MIN_OFFSET`]
/// bleibt (BIM Regel 41). Leer, wenn die Züge nicht zusammenpassen.
pub fn terrace_outlines(slab: &WallChain, up: &WallChain) -> Vec<TerraceOutline> {
    let core = slab.layers.iter().position(|l| l.core).unwrap_or(0);
    let ext: f64 = slab.layers[..core].iter().map(|l| l.thickness).sum();
    terrace_plan(slab, up, ext, None).outlines
}

/// Wie [`terrace_outlines`] mit der Deckenkante `depth` mm hinter der
/// Außenfläche von `slab` (Kernaußenfläche oder Innenseite des
/// Randdämmstreifens), dazu Attika und Blechpfad. `strip`: Baustoff der
/// Zone vor der Deckenkante, wenn dort ein Randdämmstreifen liegt.
pub fn terrace_plan(
    slab: &WallChain,
    up: &WallChain,
    depth: f64,
    strip: Option<u16>,
) -> TerracePlan {
    plan(slab, up, depth, strip).unwrap_or_default()
}

fn plan(slab: &WallChain, up: &WallChain, depth: f64, strip: Option<u16>) -> Option<TerracePlan> {
    let n = slab.segment_count();
    if !slab.closed || !up.closed || up.segment_count() != n || n < 3 {
        return None;
    }
    let face = |d: f64| slab.outer_offset() - slab.outward_sign() * d;
    let lc = slab.face_corners(face(depth));
    let mc = up.face_corners(up.outer_offset());
    let oc = slab.face_corners(slab.outer_offset());
    if lc.len() != n || mc.len() != n || oc.len() != n {
        return None;
    }
    // Außen = rechts der Laufrichtung bei Umlauf gegen den Uhrzeigersinn
    let sign = if polygon::signed_area(&lc) > 0.0 {
        1.0
    } else {
        -1.0
    };
    let dir = |c: &[Vec3], k: usize| c[(k + 1) % n] - c[k];
    let depths: Vec<f64> = (0..n)
        .map(|k| {
            let d = dir(&lc, k);
            if d.length() < 1e-9 || dir(&mc, k).length() < 1e-9 {
                return 0.0;
            }
            let out = right_of(d.normalized()) * sign;
            // M parallel zu L (Regel 29): Abstand an einem Punkt genügt
            (lc[k] - mc[k]).dot(out)
        })
        .collect();
    let terrace: Vec<bool> = depths.iter().map(|d| *d >= MIN_OFFSET - 1e-6).collect();
    if !terrace.iter().any(|t| *t) {
        return None;
    }
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
    let max_depth = |segs: &[usize]| segs.iter().fold(0.0, |a: f64, k| a.max(depths[*k]));
    // Folgen (Anfang, Länge); ringsum eine
    let runs: Vec<(usize, usize)> = if terrace.iter().all(|t| *t) {
        vec![(0, n)]
    } else {
        (0..n)
            .filter(|&k| terrace[k] && !terrace[(k + n - 1) % n])
            .map(|a| (a, (0..n).take_while(|i| terrace[(a + i) % n]).count()))
            .collect()
    };
    let ring = runs.len() == 1 && runs[0].1 == n;
    let mut out = TerracePlan {
        width: depth,
        ..TerracePlan::default()
    };
    for &(a, len) in &runs {
        let segments: Vec<usize> = (0..len).map(|i| (a + i) % n).collect();
        let parts: Option<Vec<Vec<Vec3>>> = if ring {
            let h = n / 2;
            [(0, h), (h, n - h)]
                .into_iter()
                .map(|(a, len)| part(a, len, true))
                .collect()
        } else {
            part(a, len, false).map(|p| vec![p])
        };
        let Some(parts) = parts else {
            continue;
        };
        out.outlines.push(TerraceOutline {
            depth: max_depth(&segments),
            segments,
            parts,
        });
    }
    if out.outlines.is_empty() {
        return None;
    }

    // Attika: Schichten (bzw. ihr Teil) außerhalb der Deckenkante
    let mut zones: Vec<(f64, f64, Option<usize>, u16)> = Vec::new();
    let mut at = 0.0;
    for (li, l) in slab.layers.iter().enumerate() {
        let (d0, d1) = (at, at + l.thickness);
        at = d1;
        if l.air || d0 >= depth - 1e-6 {
            continue;
        }
        if d1 <= depth + 1e-6 {
            zones.push((d0, d1, Some(li), l.material));
        } else {
            match strip {
                Some(m) => zones.push((d0, depth, None, m)),
                None => zones.push((d0, depth, Some(li), l.material)),
            }
        }
    }
    let line = |c: &[Vec3], k: usize| (c[k], dir(c, k));
    for (zi, &(d0, d1, layer, mat)) in zones.iter().enumerate() {
        let (c0, c1) = (slab.face_corners(face(d0)), slab.face_corners(face(d1)));
        if c0.len() != n || c1.len() != n {
            continue;
        }
        let contour = [zi == 0, (d1 - depth).abs() < 1e-6];
        let piece = |seg: usize, quad: [Vec3; 4], free: [bool; 2]| AttikaPiece {
            seg,
            layer,
            quad,
            mat,
            free,
            contour,
        };
        for &(a, len) in &runs {
            for i in 0..len {
                let k = (a + i) % n;
                let j = (k + 1) % n;
                out.attika
                    .push(piece(k, [c0[k], c0[j], c1[j], c1[k]], [false, false]));
            }
            if ring {
                continue;
            }
            // Nachbarn bis zur Außenfläche darüber
            let b = (a + len - 1) % n;
            let (prev, next) = ((a + n - 1) % n, (b + 1) % n);
            let (ma, mb) = (line(&mc, a), line(&mc, b));
            let cut = |c: &[Vec3], k: usize, m: (Vec3, Vec3)| {
                let (p, d) = line(c, k);
                meet(p, d, m.0, m.1)
            };
            out.attika.push(piece(
                prev,
                [cut(&c0, prev, ma), c0[a], c1[a], cut(&c1, prev, ma)],
                [true, false],
            ));
            out.attika.push(piece(
                next,
                [c0[next], cut(&c0, next, mb), cut(&c1, next, mb), c1[next]],
                [false, true],
            ));
        }
    }

    // Blech: auf der Außenfläche, bei Rücksprung bis zur Außenfläche darüber
    for &(a, len) in &runs {
        let (mut points, closed, mut ends) = if ring {
            (oc.clone(), true, [SweepEnd::Square; 2])
        } else {
            let b = (a + len - 1) % n;
            let (prev, next) = ((a + n - 1) % n, (b + 1) % n);
            let (ma, mb) = (line(&mc, a), line(&mc, b));
            let s = meet(oc[a], dir(&oc, prev), ma.0, ma.1);
            let e = meet(oc[next], dir(&oc, next), mb.0, mb.1);
            let mut p = vec![s];
            p.extend((0..=len).map(|i| oc[(a + i) % n]));
            p.push(e);
            let nrm = |m: (Vec3, Vec3)| right_of(m.1.normalized());
            (
                p,
                false,
                [SweepEnd::Plane(s, nrm(ma)), SweepEnd::Plane(e, nrm(mb))],
            )
        };
        if sign < 0.0 {
            points.reverse();
            ends.swap(0, 1);
        }
        out.coping.push(CopingPath {
            points,
            closed,
            ends,
        });
    }
    Some(out)
}

/// Querschnitt des Attikablechs (quer nach außen ab Außenfläche der Attika,
/// hoch über OK Attika; mm) für die Attikabreite `w` (BIM E4): 3° Gefälle
/// nach innen, Tropfkante 40 vor der Fassade (Jörn 10.10., vorher 20),
/// Außenschenkel 50, Innenschenkel 40, gezeichnet mit symbolischer Dicke 3.
pub fn coping_profile(w: f64) -> Vec<(f64, f64)> {
    coping_profile_with(w, COPING_INNER)
}

/// [`coping_profile`] mit dem Innenschenkel `inner` (Flachdach: 50, er
/// deckt den Hochzug der Abdichtung).
pub fn coping_profile_with(w: f64, inner: f64) -> Vec<(f64, f64)> {
    let (o, t, tan) = (COPING_DRIP, COPING_DRAWN, COPING_SLOPE.to_radians().tan());
    vec![
        (-w - t, -inner),
        (-w, -inner),
        (-w, 0.0),
        (o - t, (o - t + w) * tan),
        (o - t, -COPING_OUTER),
        (o, -COPING_OUTER),
        (o, (o + w) * tan + t),
        (-w - t, -t * tan + t),
    ]
}

/// Abwicklung des Blechs (Zuschnitt) bei Attikabreite `w`, mm (BIM §6).
pub fn coping_girth(w: f64) -> f64 {
    coping_girth_with(w, COPING_INNER)
}

/// [`coping_girth`] mit dem Innenschenkel `inner`.
pub fn coping_girth_with(w: f64, inner: f64) -> f64 {
    w + COPING_DRIP + COPING_OUTER + inner
}

/// Zuschnitt des Blechs: die Abwicklung `girth` aufgerundet auf die
/// nächste handelsübliche Breite (mm; Plan Flachdach D4). Breiter als die
/// größte: die Abwicklung.
pub fn coping_cut_width(girth: f64) -> f64 {
    COPING_WIDTHS
        .into_iter()
        .find(|w| girth <= *w + 1e-6)
        .unwrap_or(girth)
}

/// Handelsübliche Zuschnittbreiten (mm).
pub const COPING_WIDTHS: [f64; 6] = [333.0, 400.0, 500.0, 625.0, 667.0, 750.0];

/// Maße des Attikablechs (mm, Grad; BIM E4). Tropfkante 40 mm vor der
/// Fassade (Jörn 10.10., Flachdachrichtlinie mindestens 40; vorher 20).
pub const COPING_DRIP: f64 = 40.0;
pub const COPING_OUTER: f64 = 50.0;
pub const COPING_INNER: f64 = 40.0;
/// Innenschenkel am Flachdach: deckt den Hochzug der Abdichtung.
pub const ROOF_COPING_INNER: f64 = 50.0;
pub const COPING_SLOPE: f64 = 3.0;
/// Gezeichnete Dicke; die wahre (0,7 mm) ist Merkmal.
const COPING_DRAWN: f64 = 3.0;

/// Körper der Attika-Stücke von `z0` (Wandkrone) bis `z1` (OK Attika),
/// höchstens bis `cut`; endet ein Stück am Schnitt, ist die Deckfläche
/// Schnittfläche. Ohne Bodenfläche (die Wand darunter schließt), die
/// Bodenkanten stehen für [`crate::merge_seam`] im Körper.
pub fn attika_solid(pieces: &[AttikaPiece], (z0, z1): (f64, f64), cut: f64) -> Solid {
    let mut s = Solid::default();
    if cut <= z0 || z1 <= z0 {
        return s;
    }
    let top = z1.min(cut);
    let is_cut = cut < z1;
    for p in pieces {
        let [o0, o1, i1, i0] = p.quad;
        s.elem = p.seg as u32;
        s.layer = p.layer.map_or(NO_LAYER, |l| l as u8);
        s.mat = p.mat;
        let d = (o1 - o0).normalized();
        let r = right_of(d);
        let out = if (o0 - i0).dot(r) > 0.0 { r } else { -r };
        let q = |a: Vec3, b: Vec3, n: Vec3, s: &mut Solid| {
            s.oriented_quad([at_z(a, z0), at_z(b, z0), at_z(b, top), at_z(a, top)], n)
        };
        q(o0, o1, out, &mut s);
        q(i1, i0, -out, &mut s);
        // Stirn bzw. Gehrung: Normale aus der Lage, nach außen vom Stück
        for (a, b, away) in [(i0, o0, -d), (o1, i1, d)] {
            let m = right_of((b - a).normalized());
            q(a, b, if m.dot(away) < 0.0 { -m } else { m }, &mut s);
        }
        s.mat = if is_cut { p.mat | material::CUT } else { p.mat };
        let ring = polygon::to_ccw(&p.quad);
        s.cap(&ring, top, true);
        s.mat = p.mat;
        // Kanten: oben ringsum, unten für die Naht, senkrecht an den Ecken
        let kind = |c: bool| {
            if is_cut {
                edge_kind::CUT_LAYER
            } else if c {
                edge_kind::VIEW
            } else {
                edge_kind::FINE
            }
        };
        let sides = [
            (o0, o1, p.contour[0]),
            (i1, i0, p.contour[1]),
            (i0, o0, p.free[0]),
            (o1, i1, p.free[1]),
        ];
        for (a, b, c) in sides {
            s.edge_kind = kind(c);
            s.edge(at_z(a, top), at_z(b, top));
            s.edge_kind = edge_kind::VIEW;
            s.edge(at_z(a, z0), at_z(b, z0));
        }
        for (v, c) in [
            (o0, p.contour[0]),
            (o1, p.contour[0]),
            (i0, p.contour[1]),
            (i1, p.contour[1]),
        ] {
            s.edge_kind = if c { edge_kind::VIEW } else { edge_kind::FINE };
            s.edge(at_z(v, z0), at_z(v, top));
        }
    }
    s.edge_kind = edge_kind::VIEW;
    s
}

/// Schnittflächen der Attika-Stücke mit der senkrechten Ebene durch `p0`
/// mit Normale `n`, wie die Schichten der Wand umrandet; unten ohne Linie
/// (die Wand läuft durch).
pub fn attika_section_caps(
    pieces: &[AttikaPiece],
    (z0, z1): (f64, f64),
    p0: Vec3,
    n: Vec3,
) -> Solid {
    let mut s = Solid::default();
    if z1 <= z0 {
        return s;
    }
    let f = SectionFrame::new(p0, n);
    let (n, along) = (f.n, f.along);
    let pt = |u: f64, z: f64| f.pt(u, z);
    for p in pieces {
        s.elem = p.seg as u32;
        s.layer = p.layer.map_or(NO_LAYER, |l| l as u8);
        s.mat = p.mat | material::CUT;
        s.edge_kind = edge_kind::CUT_LAYER;
        let w = (p.quad[3] - p.quad[0]).length().max(1.0);
        for (a, b) in polygon::plane_intervals(&p.quad, p0, n, along) {
            s.quad_uv(
                [pt(a, z0), pt(b, z0), pt(b, z1), pt(a, z1)],
                n,
                [[z0 / w, 0.0], [z0 / w, 1.0], [z1 / w, 1.0], [z1 / w, 0.0]],
            );
            s.edge(pt(a, z1), pt(b, z1));
            s.edge(pt(a, z0), pt(a, z1));
            s.edge(pt(b, z0), pt(b, z1));
            s.edge_kind = edge_kind::VIEW;
            s.edge(pt(a, z0), pt(b, z0));
            s.edge_kind = edge_kind::CUT_LAYER;
        }
    }
    s.edge_kind = edge_kind::VIEW;
    s
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

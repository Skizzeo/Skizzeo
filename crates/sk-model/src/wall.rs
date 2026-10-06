//! Wandzug: Polygonzug auf dem Boden, aus dem Wände mit Dicke und Höhe entstehen.
//!
//! Die gezeichnete Linie ist die Bezugslinie der Wand. Die Bezugsseite sagt,
//! welche Wandfläche auf dieser Linie liegt, in Zeichenrichtung gesehen.
//! Wird im Uhrzeigersinn gezeichnet, liegt links außen: Bezugsseite `Left`
//! bedeutet dann „außen“, der Wandkörper wächst nach rechts ins Gebäude.

use crate::solid::{edge_kind, material, Solid};
use sk_math::{vec3, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefSide {
    /// Linke Wandfläche auf der Linie (bei Uhrzeigersinn: außen).
    Left,
    /// Rechte Wandfläche auf der Linie (bei Uhrzeigersinn: innen).
    Right,
    /// Wandachse auf der Linie.
    Center,
}

impl RefSide {
    /// Reihenfolge beim Umschalten mit Tab.
    pub fn next(self) -> RefSide {
        match self {
            RefSide::Left => RefSide::Right,
            RefSide::Right => RefSide::Center,
            RefSide::Center => RefSide::Left,
        }
    }

    /// Bereich des Wandkörpers quer zur Linie (positiv = rechts der Zeichenrichtung).
    fn span(self, t: f64) -> (f64, f64) {
        match self {
            RefSide::Left => (0.0, t),
            RefSide::Right => (-t, 0.0),
            RefSide::Center => (-t * 0.5, t * 0.5),
        }
    }
}

/// Eine Schicht des Wandaufbaus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    pub thickness: f64,
    pub material: u16,
    /// Tragender Kern: im Schnitt dick umrandet, übrige Schichten mitteldick.
    pub core: bool,
}

impl Layer {
    pub const fn new(thickness: f64, material: u16) -> Layer {
        Layer {
            thickness,
            material,
            core: false,
        }
    }

    /// Schicht des tragenden Kerns.
    pub const fn core(thickness: f64, material: u16) -> Layer {
        Layer {
            thickness,
            material,
            core: true,
        }
    }

    /// Kantenart der Schnittkontur dieser Schicht.
    fn cut_kind(&self) -> u8 {
        if self.core {
            edge_kind::CUT
        } else {
            edge_kind::CUT_LAYER
        }
    }
}

/// Gerade in der Grundrissebene: Punkt und Richtung.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line2 {
    pub p: Vec3,
    pub d: Vec3,
}

impl Line2 {
    /// Schnittpunkt mit der Geraden `p + d·t`; `None`, wenn (fast) parallel.
    pub(crate) fn meet(&self, p: Vec3, d: Vec3) -> Option<Vec3> {
        let c = cross2(d, self.d);
        if c.abs() < 1e-6 {
            return None;
        }
        Some(p + d * (cross2(self.p - p, self.d) / c))
    }
}

/// Abschluss eines freien Endes eines offenen Wandzugs.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum EndCut {
    /// Rechtwinklig am gezeichneten Endpunkt.
    #[default]
    Square,
    /// L-Anschluss an einen anderen Zug: alle Schichten enden an der
    /// Gehrungslinie, ohne Stirnfläche und Stirnkanten (wie eine Ecke im Zug).
    Miter(Line2),
    /// T-Anschluss: je Schicht die Linie, an der sie endet, und ob sie dort
    /// ohne Fuge in denselben Baustoff übergeht (dann ohne Stirnkanten).
    Layers(Vec<(Line2, bool)>),
}

/// Unterbrochene Konturkante: Auf der Fläche im Versatz `off` von Segment
/// `seg` fehlen die Kanten zwischen `from` und `to`, gemessen längs des
/// Segments ab seinem Anfangspunkt (T-Anschluss ohne Fuge).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gap {
    pub seg: usize,
    pub off: f64,
    pub from: f64,
    pub to: f64,
}

/// Anschlüsse an andere Wandzüge, aus dem Modell abgeleitet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Joints {
    /// Anfang und Ende eines offenen Zuges.
    pub ends: [EndCut; 2],
    pub gaps: Vec<Gap>,
    /// Höhenband einer Geschossdecke (Unterkante, Oberkante), in dem die
    /// tragenden Schichten unterbrochen sind (Auflagertasche bzw. Innenwand
    /// unter und über der Decke), siehe [`WallChain::band_layers`].
    pub slab_band: Option<(f64, f64)>,
}

impl Joints {
    pub fn is_empty(&self) -> bool {
        self.ends == [EndCut::Square, EndCut::Square]
            && self.gaps.is_empty()
            && self.slab_band.is_none()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WallChain {
    /// Eckpunkte der Bezugslinie (z wird ignoriert, siehe [`WallChain::base`]).
    pub points: Vec<Vec3>,
    pub closed: bool,
    pub ref_side: RefSide,
    /// Schichten von außen nach innen.
    pub layers: Vec<Layer>,
    /// Höhe des Wandfußes (z, absolut): EG ±0, OG auf OK EG-Decke.
    pub base: f64,
    /// Wandhöhe ab dem Fuß.
    pub height: f64,
    /// Anschlüsse an andere Wandzüge (Paket B5a).
    pub joints: Joints,
}

/// Größter Abstand einer Gehrungsecke vom Linienpunkt, in Wanddicken.
const MITER_LIMIT: f64 = 8.0;

pub(crate) fn flat(p: Vec3) -> Vec3 {
    vec3(p.x, p.y, 0.0)
}

/// Rechte Normale einer Richtung in der Ebene.
pub(crate) fn right_of(d: Vec3) -> Vec3 {
    vec3(d.y, -d.x, 0.0)
}

pub(crate) fn cross2(a: Vec3, b: Vec3) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Punkte auf dem Boden ohne doppelte Nachbarn (und bei geschlossenem Zug ohne
/// Schlusspunkt gleich Startpunkt).
pub fn clean_points(points: &[Vec3], closed: bool) -> Vec<Vec3> {
    let mut pts: Vec<Vec3> = Vec::new();
    for p in points {
        let p = flat(*p);
        if pts.last().is_none_or(|q| (p - *q).length() > 1.0) {
            pts.push(p);
        }
    }
    if closed && pts.len() > 2 && (pts[0] - pts[pts.len() - 1]).length() <= 1.0 {
        pts.pop();
    }
    pts
}

/// Anzahl der Segmente eines Zuges aus `n` bereinigten Punkten.
pub fn segment_count(n: usize, closed: bool) -> usize {
    match n {
        0 | 1 => 0,
        2 => 1,
        _ if closed => n,
        _ => n - 1,
    }
}

impl WallChain {
    /// Punkte ohne doppelte Nachbarn (und ohne Schlusspunkt gleich Startpunkt).
    pub fn clean_points(&self) -> Vec<Vec3> {
        clean_points(&self.points, self.closed)
    }

    /// Bereinigte Punkte, ob wirklich geschlossen, und Richtung jedes Segments.
    fn layout(&self) -> Option<(Vec<Vec3>, bool, Vec<Vec3>)> {
        let pts = self.clean_points();
        if pts.len() < 2 {
            return None;
        }
        let closed = self.closed && pts.len() >= 3;
        let n = pts.len();
        let m = if closed { n } else { n - 1 };
        let dirs = (0..m)
            .map(|i| (pts[(i + 1) % n] - pts[i]).normalized())
            .collect();
        Some((pts, closed, dirs))
    }

    /// Freies Ende `e` (0 Anfang, 1 Ende) eines offenen Zuges: Punkt auf der
    /// Bezugslinie und Zeichenrichtung des Endsegments.
    pub(crate) fn end_frame(&self, e: usize) -> Option<(Vec3, Vec3)> {
        let (pts, closed, dirs) = self.layout()?;
        if closed {
            return None;
        }
        Some(match e {
            0 => (pts[0], dirs[0]),
            _ => (pts[pts.len() - 1], dirs[dirs.len() - 1]),
        })
    }

    /// Anfangspunkt und Richtung von Segment `seg` (bereinigte Punkte).
    pub(crate) fn segment_frame(&self, seg: usize) -> Option<(Vec3, Vec3)> {
        let (pts, _, dirs) = self.layout()?;
        Some((pts[seg], *dirs.get(seg)?))
    }

    /// Bereich des Wandkörpers quer zur Bezugslinie (kleiner, größer).
    pub(crate) fn span(&self) -> (f64, f64) {
        self.ref_side.span(self.thickness())
    }

    /// Anzahl der Wandsegmente.
    pub fn segment_count(&self) -> usize {
        self.layout().map_or(0, |(_, _, dirs)| dirs.len())
    }

    /// Normale eines Segments in der Ebene (rechts der Zeichenrichtung).
    pub fn segment_normal(&self, i: usize) -> Option<Vec3> {
        let (_, _, dirs) = self.layout()?;
        dirs.get(i).map(|d| right_of(*d))
    }

    /// Lage der Außenfläche quer zur Bezugslinie (positiv = rechts der Zeichenrichtung).
    ///
    /// Geschlossen: die Seite, die vom umschlossenen Bereich weg zeigt.
    /// Offen: die Fläche auf der Bezugsseite (Mitte zählt als links).
    pub fn outer_offset(&self) -> f64 {
        let (lo, hi) = self.ref_side.span(self.thickness());
        match self.layout() {
            Some((pts, true, _)) => {
                let n = pts.len();
                let area: f64 = (0..n).map(|i| cross2(pts[i], pts[(i + 1) % n])).sum();
                // Fläche > 0: gegen den Uhrzeigersinn, außen liegt rechts
                if area > 0.0 {
                    hi
                } else {
                    lo
                }
            }
            _ => match self.ref_side {
                RefSide::Right => hi,
                RefSide::Left | RefSide::Center => lo,
            },
        }
    }

    /// Lage der Innenfläche quer zur Bezugslinie (gegenüber der Außenfläche).
    pub fn inner_offset(&self) -> f64 {
        let (lo, hi) = self.ref_side.span(self.thickness());
        if self.outer_offset() == lo {
            hi
        } else {
            lo
        }
    }

    /// Grundriss eines Segments über die ganze Dicke, mit Gehrung: außen Anfang,
    /// außen Ende, innen Ende, innen Anfang (auf z = 0).
    pub fn segment_footprint(&self, seg: usize) -> Option<[Vec3; 4]> {
        if seg >= self.segment_count() {
            return None;
        }
        let (o, i) = (
            self.face_corners(self.outer_offset()),
            self.face_corners(self.inner_offset()),
        );
        let j = (seg + 1) % o.len();
        Some([o[seg], o[j], i[j], i[seg]])
    }

    /// Eckpunkte der zur Bezugslinie parallelen Wandfläche im Abstand `off`
    /// (je Punkt der bereinigten Linie einer, mit Gehrung an den Ecken).
    /// An angeschlossenen Enden gilt der Abschluss der Schicht, zu der die
    /// Fläche gehört (bei einer Schichtfuge die äußere der beiden).
    pub fn face_corners(&self, off: f64) -> Vec<Vec3> {
        self.face_corners_in(off, self.layer_at(off))
    }

    /// Schicht, zu der die Fläche im Versatz `off` gehört.
    fn layer_at(&self, off: f64) -> Option<usize> {
        if self.joints.ends == [EndCut::Square, EndCut::Square] {
            return None;
        }
        self.layer_offsets()
            .iter()
            .position(|&(lo, hi, _)| off >= lo - 1e-6 && off <= hi + 1e-6)
    }

    /// Abschlusslinie an Ende `e` (0 Anfang, 1 Ende) für Schicht `layer`.
    fn end_line(&self, e: usize, layer: Option<usize>) -> Option<Line2> {
        match &self.joints.ends[e] {
            EndCut::Square => None,
            EndCut::Miter(l) => Some(*l),
            EndCut::Layers(v) => layer.and_then(|i| v.get(i)).map(|x| x.0),
        }
    }

    /// Wie [`WallChain::face_corners`] für eine Fläche von Schicht `layer`.
    pub fn face_corners_in(&self, off: f64, layer: Option<usize>) -> Vec<Vec3> {
        let Some((pts, closed, dirs)) = self.layout() else {
            return Vec::new();
        };
        let (n, m) = (pts.len(), dirs.len());
        let corner = |j: usize| -> Vec3 {
            let has_prev = closed || j > 0;
            let has_next = closed || j < n - 1;
            match (has_prev, has_next) {
                (true, true) => {
                    let (d0, d1) = (dirs[(j + m - 1) % m], dirs[j % m]);
                    let (n0, n1) = (right_of(d0), right_of(d1));
                    let c = cross2(d0, d1);
                    let base = pts[j];
                    if c.abs() < 1e-9 {
                        return base + n1 * off;
                    }
                    // Schnitt der Linien base + n0*off + d0*s und base + n1*off + d1*u
                    let p0 = base + n0 * off;
                    let p1 = base + n1 * off;
                    let s_par = cross2(p1 - p0, d1) / c;
                    let x = p0 + d0 * s_par;
                    if (x - base).length() > MITER_LIMIT * self.thickness().max(1.0) {
                        base + n1 * off
                    } else {
                        x
                    }
                }
                (false, _) => {
                    let p = pts[0] + right_of(dirs[0]) * off;
                    self.end_line(0, layer)
                        .and_then(|l| l.meet(p, dirs[0]))
                        .unwrap_or(p)
                }
                (_, false) => {
                    let p = pts[n - 1] + right_of(dirs[m - 1]) * off;
                    self.end_line(1, layer)
                        .and_then(|l| l.meet(p, dirs[m - 1]))
                        .unwrap_or(p)
                }
            }
        };
        (0..n).map(corner).collect()
    }

    /// Kanten der Flächen im Versatz `off` von Segment `seg` ohne angeschlossene Stellen.
    fn gaps_at(&self, seg: usize, off: f64) -> impl Iterator<Item = (f64, f64)> + '_ {
        self.joints
            .gaps
            .iter()
            .filter(move |g| g.seg == seg && (g.off - off).abs() < 1e-3)
            .map(|g| (g.from, g.to))
    }

    /// Kante von `p` nach `q` auf der Fläche `off` von Segment `seg` (Anfangspunkt
    /// und Richtung `frame`), ohne die Lücken.
    fn gapped_edge(
        &self,
        s: &mut Solid,
        seg: usize,
        off: f64,
        frame: (Vec3, Vec3),
        p: Vec3,
        q: Vec3,
    ) {
        let (base, dir) = frame;
        let mut gaps: Vec<(f64, f64)> = self.gaps_at(seg, off).collect();
        if gaps.is_empty() {
            s.edge(p, q);
            return;
        }
        gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (tp, tq) = ((flat(p) - base).dot(dir), (flat(q) - base).dot(dir));
        if (tq - tp).abs() < 1e-9 {
            return;
        }
        let at = |t: f64| p + (q - p) * ((t - tp) / (tq - tp));
        let (lo, hi) = (tp.min(tq), tp.max(tq));
        let mut from = lo;
        for (a, b) in gaps {
            let (a, b) = (a.max(lo), b.min(hi));
            if b <= a {
                continue;
            }
            if a - from > 1e-6 {
                s.edge(at(from), at(a));
            }
            from = from.max(b);
        }
        if hi - from > 1e-6 {
            s.edge(at(from), at(hi));
        }
    }

    /// Verschiebt Segment `i` um `d` entlang seiner Normalen. Die Nachbarsegmente
    /// behalten ihre Richtung: Die gemeinsamen Punkte gleiten auf deren Linien.
    /// `None`, wenn dabei ein Segment verschwinden oder sich umkehren würde.
    pub fn with_segment_moved(&self, i: usize, d: f64) -> Option<WallChain> {
        let (pts, closed, dirs) = self.layout()?;
        let (n, m) = (pts.len(), dirs.len());
        if i >= m {
            return None;
        }
        let shift = right_of(dirs[i]) * d;
        let (a, b) = (i, (i + 1) % n);
        // Schnitt der verschobenen Linie mit der Linie des Nachbarn durch `pts[k]`
        let slide = |k: usize, nd: Vec3| -> Vec3 {
            let c = cross2(nd, dirs[i]);
            if c.abs() < 1e-9 {
                return pts[k] + shift;
            }
            // pts[k] + nd*t liegt auf pts[k] + shift + dirs[i]*u
            let t = cross2(shift, dirs[i]) / c;
            pts[k] + nd * t
        };
        let mut out = pts.clone();
        out[a] = if closed || a > 0 {
            slide(a, dirs[(i + m - 1) % m])
        } else {
            pts[a] + shift
        };
        out[b] = if closed || b < n - 1 {
            slide(b, dirs[(i + 1) % m])
        } else {
            pts[b] + shift
        };
        // Kein Segment darf verschwinden oder die Richtung wechseln
        for k in 0..m {
            let v = out[(k + 1) % n] - out[k];
            if v.length() < 1.0 || v.dot(dirs[k]) <= 0.0 {
                return None;
            }
        }
        Some(WallChain {
            points: out,
            closed,
            ..self.clone()
        })
    }

    /// Richtung „nach außen“ je Segment: +1, wenn die Außenfläche rechts der
    /// Zeichenrichtung liegt, sonst −1.
    fn outward_sign(&self) -> f64 {
        let (_, hi) = self.span();
        if self.outer_offset() == hi {
            1.0
        } else {
            -1.0
        }
    }

    /// Zug mit jedem Segment `i` um `offsets[i]` nach außen verschoben
    /// (negativ: nach innen), z. B. die OG-Wand über der EG-Wand mit ihrem
    /// Versatz je Segment. Die Ecken sind die Schnittpunkte der verschobenen
    /// Nachbarlinien, Richtungen bleiben erhalten. `None`, wenn die Anzahl
    /// nicht passt, zwei fluchtende Nachbarn verschieden weit wandern sollen
    /// oder ein Segment dabei verschwinden oder sich umkehren würde.
    pub fn with_segment_offsets(&self, offsets: &[f64]) -> Option<WallChain> {
        let (pts, closed, dirs) = self.layout()?;
        let (n, m) = (pts.len(), dirs.len());
        if offsets.len() != m || offsets.iter().any(|d| !d.is_finite()) {
            return None;
        }
        if offsets.iter().all(|d| *d == 0.0) {
            return Some(WallChain {
                points: pts,
                closed,
                ..self.clone()
            });
        }
        let sign = self.outward_sign();
        let shift = |i: usize| right_of(dirs[i]) * (sign * offsets[i]);
        let mut out = Vec::with_capacity(n);
        for j in 0..n {
            let prev = if closed || j > 0 {
                Some((j + m - 1) % m)
            } else {
                None
            };
            let next = if closed || j < n - 1 {
                Some(j % m)
            } else {
                None
            };
            let p = match (prev, next) {
                (Some(a), Some(b)) => {
                    let c = cross2(dirs[a], dirs[b]);
                    if c.abs() < 1e-9 {
                        // Fluchtend: nur gemeinsam verschiebbar
                        if (offsets[a] - offsets[b]).abs() > 1e-6 || dirs[a].dot(dirs[b]) < 0.0 {
                            return None;
                        }
                        pts[j] + shift(b)
                    } else {
                        // Schnitt von pts[j] + shift(a) + dirs[a]·s mit pts[j] + shift(b) + dirs[b]·u
                        let (pa, pb) = (pts[j] + shift(a), pts[j] + shift(b));
                        pa + dirs[a] * (cross2(pb - pa, dirs[b]) / c)
                    }
                }
                (Some(a), None) => pts[j] + shift(a),
                (None, Some(b)) => pts[j] + shift(b),
                (None, None) => return None,
            };
            out.push(p);
        }
        for k in 0..m {
            let v = out[(k + 1) % n] - out[k];
            if v.length() < 1.0 || v.dot(dirs[k]) <= 0.0 {
                return None;
            }
        }
        Some(WallChain {
            points: out,
            closed,
            ..self.clone()
        })
    }

    /// Versatz nach außen je Segment von `self` gegenüber dem Zug `below`
    /// (gleiche Segmentzahl und Richtungen), Gegenstück zu
    /// [`WallChain::with_segment_offsets`]: Bezugslinie zu Bezugslinie.
    /// `None`, wenn die Züge nicht zusammenpassen.
    pub fn segment_offsets_from(&self, below: &WallChain) -> Option<Vec<f64>> {
        let (pa, ca, da) = below.layout()?;
        let (pb, cb, db) = self.layout()?;
        if ca != cb || da.len() != db.len() {
            return None;
        }
        let sign = below.outward_sign();
        da.iter()
            .zip(&db)
            .enumerate()
            .map(|(i, (a, b))| {
                (cross2(*a, *b).abs() < 1e-9 && a.dot(*b) > 0.0)
                    .then(|| (pb[i] - pa[i]).dot(right_of(*a)) * sign)
            })
            .collect()
    }

    /// Gesamtdicke aller Schichten.
    pub fn thickness(&self) -> f64 {
        self.layers.iter().map(|l| l.thickness).sum()
    }

    /// Lage jeder Schicht quer zur Bezugslinie: (kleiner, größer, Baustoff).
    pub fn layer_offsets(&self) -> Vec<(f64, f64, u16)> {
        let (lo, _) = self.ref_side.span(self.thickness());
        let outer = self.outer_offset();
        let sign = if outer == lo { 1.0 } else { -1.0 };
        let mut at = outer;
        self.layers
            .iter()
            .map(|l| {
                let next = at + sign * l.thickness;
                let r = (at.min(next), at.max(next), l.material);
                at = next;
                r
            })
            .collect()
    }

    /// Äußerer Wandfuß je Segment als (Anfang, Ende), auf z = 0.
    pub fn outer_foot(&self) -> Vec<(Vec3, Vec3)> {
        let c = self.face_corners(self.outer_offset());
        let n = c.len();
        (0..self.segment_count())
            .map(|k| (c[k], c[(k + 1) % n]))
            .collect()
    }

    /// Schichten, die eine Geschossdecke unterbricht: der tragende Kern und
    /// alles innen davon (die Decke reicht bis an die Dämmung außen). Ohne
    /// tragende Schicht alle.
    pub fn band_layers(&self) -> std::ops::Range<usize> {
        let first = self.layers.iter().position(|l| l.core).unwrap_or(0);
        first..self.layers.len()
    }

    /// Wandkrone (z, absolut).
    pub fn top(&self) -> f64 {
        self.base + self.height
    }

    /// Höhenabschnitte (von, bis; z absolut) der Schicht `layer`: ganze Höhe
    /// oder, wenn eine Decke sie unterbricht, unter und über der Decke.
    pub fn layer_spans(&self, layer: usize) -> Vec<(f64, f64)> {
        let (z0, h) = (self.base, self.top());
        match self.joints.slab_band {
            Some((b, t)) if self.band_layers().contains(&layer) && b < h && t > z0 => {
                let mut v = Vec::with_capacity(2);
                if b > z0 + 1e-6 {
                    v.push((z0, b.min(h)));
                }
                if t < h - 1e-6 {
                    v.push((t.max(z0), h));
                }
                v
            }
            _ => vec![(z0, h)],
        }
    }

    /// Wandkörper mit Gehrungen an den Ecken, eine Schale je Schicht.
    pub fn solid(&self) -> Solid {
        self.solid_below(f64::INFINITY)
    }

    /// Wand waagerecht geschnitten in Höhe `cut` (für den Grundriss).
    /// Die Schnittfläche oben trägt den Baustoff mit [`material::CUT`].
    pub fn solid_cut_at(&self, cut: f64) -> Solid {
        self.solid_below(cut)
    }

    /// Alle Schichten bis Höhe `cut`; endet ein Abschnitt am Schnitt, ist seine
    /// Deckfläche Schnittfläche.
    fn solid_below(&self, cut: f64) -> Solid {
        let mut s = Solid::default();
        for (i, ((lo, hi, mat), l)) in self
            .layer_offsets()
            .into_iter()
            .zip(&self.layers)
            .enumerate()
        {
            for (z0, z1) in self.layer_spans(i) {
                if z0 >= cut {
                    continue;
                }
                s.mat = mat;
                let (top, top_mat) = if z1 > cut {
                    (cut, mat | material::CUT)
                } else {
                    (z1, mat)
                };
                self.prism(&mut s, i, lo, hi, (z0, top), top_mat, l.cut_kind());
            }
        }
        s
    }

    /// Schnittflächen der Wand mit der senkrechten Ebene durch `p0` mit Normale `n`
    /// (Flächen zeigen in Richtung `n`). Jede Schicht ist umrandet: der tragende Kern
    /// dick, die übrigen Schichten mitteldick.
    pub fn section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let mut s = Solid::default();
        let Some((pts, _, dirs)) = self.layout() else {
            return s;
        };
        let n = vec3(n.x, n.y, 0.0).normalized();
        let along = vec3(-n.y, n.x, 0.0);
        let side = |p: Vec3| (p - p0).dot(n);
        for (li, ((lo, hi, mat), l)) in self
            .layer_offsets()
            .into_iter()
            .zip(&self.layers)
            .enumerate()
        {
            let kind = l.cut_kind();
            let spans = self.layer_spans(li);
            let (cl, ch) = (
                self.face_corners_in(lo, Some(li)),
                self.face_corners_in(hi, Some(li)),
            );
            let cnt = cl.len();
            let t = (hi - lo).max(1.0);
            for i in 0..self.segment_count() {
                let j = (i + 1) % cnt;
                let quad = [cl[i], cl[j], ch[j], ch[i]];
                // Schnitt der Grundfläche (konvex) mit der Ebene: Strecke
                let mut hits: Vec<Vec3> = Vec::new();
                for k in 0..4 {
                    let (a, b) = (quad[k], quad[(k + 1) % 4]);
                    let (da, db) = (side(a), side(b));
                    if (da < 0.0) != (db < 0.0) {
                        hits.push(a + (b - a) * (da / (da - db)));
                    }
                }
                if hits.len() < 2 {
                    continue;
                }
                hits.sort_by(|a, b| a.dot(along).total_cmp(&b.dot(along)));
                let (a, b) = (hits[0], hits[hits.len() - 1]);
                if (b - a).length() < 1e-6 {
                    continue;
                }
                // Lage quer zur Wand: auf welcher Wandfläche liegt ein Schnittpunkt?
                let nr = right_of(dirs[i]);
                let off = |p: Vec3| (flat(p) - pts[i]).dot(nr);
                let v = |p: Vec3| (off(p) - lo) / t;
                s.mat = mat | material::CUT;
                s.elem = i as u32;
                for &(z0, z1) in &spans {
                    let (a, b) = (flat(a) + vec3(0.0, 0.0, z0), flat(b) + vec3(0.0, 0.0, z0));
                    let up = vec3(0.0, 0.0, z1 - z0);
                    let (v0, v1) = (z0 / t, z1 / t);
                    s.quad_uv(
                        [a, b, b + up, a + up],
                        n,
                        [[v0, v(a)], [v0, v(b)], [v1, v(b)], [v1, v(a)]],
                    );
                    s.edge_kind = kind;
                    s.edge(a, b);
                    s.edge(a + up, b + up);
                    for p in [a, b] {
                        let o = off(p);
                        let face = if (o - lo).abs() < 1e-3 {
                            Some(lo)
                        } else if (o - hi).abs() < 1e-3 {
                            Some(hi)
                        } else {
                            None
                        };
                        let t = (flat(p) - pts[i]).dot(dirs[i]);
                        if let Some(f) = face {
                            if !self
                                .gaps_at(i, f)
                                .any(|(g0, g1)| t > g0 + 1e-6 && t < g1 - 1e-6)
                            {
                                s.edge(p, p + up);
                            }
                        }
                    }
                }
            }
        }
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Äußerste Wandflächen (kleinster und größter Versatz aller Schichten).
    fn contour(&self) -> (f64, f64) {
        let l = self.layer_offsets();
        let lo = l.iter().map(|x| x.0).fold(f64::MAX, f64::min);
        let hi = l.iter().map(|x| x.1).fold(f64::MIN, f64::max);
        (lo, hi)
    }

    /// Prisma zwischen den Wandflächen `lo` und `hi` (lo < hi) bis Höhe `h`.
    /// Flächen, die nicht auf der Wandkontur liegen (Schichtfugen), bekommen feine Kanten.
    /// Ist die Deckfläche ein Schnitt, wird sie ringsum mit `cut_kind` umrandet.
    #[allow(clippy::too_many_arguments)]
    fn prism(
        &self,
        s: &mut Solid,
        layer: usize,
        lo: f64,
        hi: f64,
        (z0, z1): (f64, f64),
        top_mat: u16,
        cut_kind: u8,
    ) {
        let Some((pts, closed, dirs)) = self.layout() else {
            return;
        };
        let (n, m) = (pts.len(), dirs.len());
        let (clo, chi) = self.contour();
        let cut = top_mat & material::CUT != 0;
        let kind_at = |off: f64, outer: u8| {
            if (off - clo).abs() < 1e-6 || (off - chi).abs() < 1e-6 {
                outer
            } else {
                edge_kind::FINE
            }
        };
        let top_kind = if cut { cut_kind } else { edge_kind::VIEW };
        let base = vec3(0.0, 0.0, z0);
        let ca: Vec<Vec3> = self
            .face_corners_in(lo, Some(layer))
            .into_iter()
            .map(|p| flat(p) + base)
            .collect();
        let cb: Vec<Vec3> = self
            .face_corners_in(hi, Some(layer))
            .into_iter()
            .map(|p| flat(p) + base)
            .collect();
        let side_mat = s.mat;
        let up = vec3(0.0, 0.0, z1 - z0);
        let t = (hi - lo).max(1.0);

        for i in 0..m {
            let j = (i + 1) % n;
            let (a0, a1, b0, b1) = (ca[i], ca[j], cb[i], cb[j]);
            let nr = right_of(dirs[i]);
            s.elem = i as u32;
            // Musterkoordinaten: u längs in Schichtdicken, v quer 0..1
            let uv = |p: Vec3| -> [f64; 2] {
                let r = flat(p) - pts[i];
                [r.dot(dirs[i]) / t, (r.dot(nr) - lo) / t]
            };
            s.quad(a0, a1, b1, b0, vec3(0.0, 0.0, -1.0));
            s.mat = top_mat;
            let top = [a0 + up, b0 + up, b1 + up, a1 + up];
            s.quad_uv(top, vec3(0.0, 0.0, 1.0), top.map(uv));
            s.mat = side_mat;
            s.quad(a0, a0 + up, a1 + up, a1, -nr);
            s.quad(b0, b1, b1 + up, b0 + up, nr);
            for (p, q, off) in [(a0, a1, lo), (b0, b1, hi)] {
                s.edge_kind = kind_at(off, edge_kind::VIEW);
                self.gapped_edge(s, i, off, (pts[i], dirs[i]), p, q);
                s.edge_kind = if cut {
                    cut_kind
                } else {
                    kind_at(off, top_kind)
                };
                self.gapped_edge(s, i, off, (pts[i], dirs[i]), p + up, q + up);
            }
        }
        if !closed {
            let (d0, d1) = (dirs[0], dirs[m - 1]);
            let (a0, b0, a1, b1) = (ca[0], cb[0], ca[n - 1], cb[n - 1]);
            // Stirnfläche und -kanten je Ende: bei L keine (Gehrung wie im Zug),
            // bei T ohne Fuge keine Kanten
            let face = |e: usize| -> (bool, bool) {
                match &self.joints.ends[e] {
                    EndCut::Square => (true, true),
                    EndCut::Miter(_) => (false, false),
                    EndCut::Layers(v) => (true, !v.get(layer).is_some_and(|x| x.1)),
                }
            };
            // Normale der Stirnfläche aus ihrer Lage (rechtwinklig: genau die Zugrichtung)
            let normal = |e: usize, p: Vec3, q: Vec3, out: Vec3| -> Vec3 {
                if self.joints.ends[e] == EndCut::Square {
                    return out;
                }
                let nn = right_of((q - p).normalized());
                if nn.dot(out) < 0.0 {
                    -nn
                } else {
                    nn
                }
            };
            let ends = [(a0, b0, -d0, 0), (a1, b1, d1, m - 1)];
            for (e, &(p, q, out, elem)) in ends.iter().enumerate() {
                if face(e).0 {
                    s.elem = elem as u32;
                    let nn = normal(e, p, q, out);
                    if e == 0 {
                        s.quad(p, q, q + up, p + up, nn);
                    } else {
                        s.quad(p, p + up, q + up, q, nn);
                    }
                }
            }
            for (e, &(p, q, _, _)) in ends.iter().enumerate() {
                if face(e).1 {
                    s.edge_kind = edge_kind::VIEW;
                    s.edge(p, q);
                    s.edge_kind = top_kind;
                    s.edge(p + up, q + up);
                }
            }
        }
        // Senkrechte Kanten an Ecken (nicht dort, wo die Wand gerade weiterläuft)
        for j in 0..n {
            let straight = if closed || (j > 0 && j < n - 1) {
                let (d0, d1) = (dirs[(j + m - 1) % m], dirs[j % m]);
                cross2(d0, d1).abs() < 1e-9 && d0.dot(d1) > 0.0
            } else {
                false
            };
            if !straight {
                s.edge_kind = kind_at(lo, edge_kind::VIEW);
                s.edge(ca[j], ca[j] + up);
                s.edge_kind = kind_at(hi, edge_kind::VIEW);
                s.edge(cb[j], cb[j] + up);
            }
        }
        s.edge_kind = edge_kind::VIEW;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_cw(side: f64) -> Vec<Vec3> {
        // Im Uhrzeigersinn von oben gesehen (Z nach oben)
        vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, side, 0.0),
            vec3(side, side, 0.0),
            vec3(side, 0.0, 0.0),
        ]
    }

    fn bbox(s: &Solid) -> (Vec3, Vec3) {
        let mut lo = vec3(f64::MAX, f64::MAX, f64::MAX);
        let mut hi = -lo;
        for t in &s.triangles {
            for p in &t.p {
                lo = vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            }
        }
        (lo, hi)
    }

    fn chain(points: Vec<Vec3>, closed: bool, ref_side: RefSide) -> WallChain {
        WallChain {
            points,
            closed,
            ref_side,
            layers: vec![Layer::new(400.0, material::PLAIN)],
            base: 0.0,
            height: 3500.0,
            joints: Default::default(),
        }
    }

    #[test]
    fn aussen_im_uhrzeigersinn_waechst_nach_innen() {
        let s = chain(square_cw(5000.0), true, RefSide::Left).solid();
        let (lo, hi) = bbox(&s);
        assert!(
            (lo.x - 0.0).abs() < 1e-6 && (hi.x - 5000.0).abs() < 1e-6,
            "{lo:?} {hi:?}"
        );
        assert!((hi.z - 3500.0).abs() < 1e-6);
        // Innenecke liegt 400 mm innen
        let inner = vec3(400.0, 400.0, 0.0);
        assert!(s
            .triangles
            .iter()
            .any(|t| t.p.iter().any(|p| (*p - inner).length() < 1e-6)));
    }

    #[test]
    fn innen_waechst_nach_aussen() {
        let s = chain(square_cw(5000.0), true, RefSide::Right).solid();
        let (lo, hi) = bbox(&s);
        assert!(
            (lo.x + 400.0).abs() < 1e-6 && (hi.x - 5400.0).abs() < 1e-6,
            "{lo:?} {hi:?}"
        );
    }

    #[test]
    fn mitte_waechst_beidseitig() {
        let s = chain(square_cw(5000.0), true, RefSide::Center).solid();
        let (lo, hi) = bbox(&s);
        assert!(
            (lo.y + 200.0).abs() < 1e-6 && (hi.y - 5200.0).abs() < 1e-6,
            "{lo:?} {hi:?}"
        );
    }

    #[test]
    fn offener_zug_hat_stirnflaechen() {
        let s = chain(
            vec![vec3(0.0, 0.0, 0.0), vec3(3000.0, 0.0, 0.0)],
            false,
            RefSide::Left,
        )
        .solid();
        // unten, oben, zwei Seiten, zwei Stirnseiten = 6 Vierecke
        assert_eq!(s.triangles.len(), 12);
        assert_eq!(s.edges.len(), 12);
    }

    #[test]
    fn geschlossenes_viereck_hat_keine_stirnflaechen() {
        let s = chain(square_cw(5000.0), true, RefSide::Left).solid();
        assert_eq!(s.triangles.len(), 4 * 8);
        // je Seite 4 waagerechte, je Ecke 2 senkrechte Kanten
        assert_eq!(s.edges.len(), 4 * 4 + 4 * 2);
    }
}

#[cfg(test)]
mod verschieben {
    use super::*;

    fn rechteck() -> WallChain {
        WallChain {
            // im Uhrzeigersinn: links unten, links oben, rechts oben, rechts unten
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 4000.0, 0.0),
                vec3(5000.0, 4000.0, 0.0),
                vec3(5000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            layers: vec![Layer::new(400.0, material::PLAIN)],
            base: 0.0,
            height: 3500.0,
            joints: Default::default(),
        }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-6
    }

    #[test]
    fn aussenseite_im_uhrzeigersinn_ist_links() {
        let w = rechteck();
        assert_eq!(w.outer_offset(), 0.0);
        let mut ccw = w.clone();
        ccw.points.reverse();
        assert_eq!(ccw.outer_offset(), 400.0);
        // Gegen den Uhrzeigersinn liegt die Bezugslinie innen, die Wand wächst nach außen
        let c = ccw.face_corners(ccw.outer_offset());
        assert!(
            c.iter().any(|p| near(*p, vec3(5400.0, 4400.0, 0.0))),
            "{c:?}"
        );
    }

    #[test]
    fn segment_wandert_nachbarn_bleiben_richtungstreu() {
        let w = rechteck();
        // Segment 1 (oben, von links nach rechts) nach außen (+y) schieben:
        // Normale rechts der Zeichenrichtung zeigt nach -y, also d negativ
        let n = w.segment_normal(1).unwrap();
        assert!(near(n, vec3(0.0, -1.0, 0.0)));
        let v = w.with_segment_moved(1, -1000.0).unwrap();
        assert!(near(v.points[1], vec3(0.0, 5000.0, 0.0)));
        assert!(near(v.points[2], vec3(5000.0, 5000.0, 0.0)));
        assert!(near(v.points[0], w.points[0]) && near(v.points[3], w.points[3]));
    }

    #[test]
    fn schraege_nachbarn_gleiten_auf_ihrer_linie() {
        let w = WallChain {
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(1000.0, 3000.0, 0.0),
                vec3(4000.0, 3000.0, 0.0),
                vec3(5000.0, 0.0, 0.0),
            ],
            ..rechteck()
        };
        let v = w.with_segment_moved(1, 300.0).unwrap();
        // Oberes Segment liegt jetzt bei y = 2700, Nachbarn behalten ihre Richtung
        for (k, prev) in [(1usize, 0usize), (2, 3)] {
            assert!((v.points[k].y - 2700.0).abs() < 1e-6);
            let a = (w.points[k] - w.points[prev]).normalized();
            let b = (v.points[k] - v.points[prev]).normalized();
            assert!(near(a, b), "{a:?} {b:?}");
        }
    }

    #[test]
    fn offenes_ende_wandert_parallel() {
        let w = WallChain {
            points: vec![vec3(0.0, 0.0, 0.0), vec3(3000.0, 0.0, 0.0)],
            closed: false,
            ..rechteck()
        };
        let v = w.with_segment_moved(0, 500.0).unwrap();
        assert!(near(v.points[0], vec3(0.0, -500.0, 0.0)));
        assert!(near(v.points[1], vec3(3000.0, -500.0, 0.0)));
    }

    #[test]
    fn zu_weit_verschoben_wird_abgelehnt() {
        let w = rechteck();
        // Oberes Segment über das untere hinaus nach innen schieben
        assert!(w.with_segment_moved(1, 4500.0).is_none());
    }
}

#[cfg(test)]
mod schichten {
    use super::*;

    const AERATED_CONCRETE: u16 = 1;
    const INSULATION: u16 = 2;

    /// Zweischalige Außenwand: 14 cm Dämmung außen, 17,5 cm Gasbeton innen.
    fn exterior_wall_layers() -> Vec<Layer> {
        vec![
            Layer::new(140.0, INSULATION),
            Layer::core(175.0, AERATED_CONCRETE),
        ]
    }

    fn haus(ref_side: RefSide) -> WallChain {
        WallChain {
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 4000.0, 0.0),
                vec3(5000.0, 4000.0, 0.0),
                vec3(5000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side,
            layers: exterior_wall_layers(),
            base: 0.0,
            height: 3500.0,
            joints: Default::default(),
        }
    }

    #[test]
    fn daemmung_aussen_gasbeton_innen() {
        let w = haus(RefSide::Left);
        assert!((w.thickness() - 315.0).abs() < 1e-9);
        let l = w.layer_offsets();
        assert_eq!(l[0], (0.0, 140.0, INSULATION));
        assert_eq!(l[1], (140.0, 315.0, AERATED_CONCRETE));
        // Gegen den Uhrzeigersinn gezeichnet bleibt die Dämmung außen
        let mut ccw = w.clone();
        ccw.points.reverse();
        let l = ccw.layer_offsets();
        assert_eq!(l[0], (175.0, 315.0, INSULATION));
        assert_eq!(l[1], (0.0, 175.0, AERATED_CONCRETE));
    }

    #[test]
    fn grundriss_schnitt_hat_schnittflaechen_oben() {
        let s = haus(RefSide::Left).solid_cut_at(1000.0);
        let tops: Vec<_> = s
            .triangles
            .iter()
            .filter(|t| t.p.iter().all(|p| (p.z - 1000.0).abs() < 1e-9))
            .collect();
        assert!(!tops.is_empty());
        assert!(tops.iter().all(|t| t.mat & material::CUT != 0));
    }

    #[test]
    fn senkrechter_schnitt_trifft_beide_waende_und_schichten() {
        let w = haus(RefSide::Left);
        let caps = w.section_caps(vec3(0.0, 2000.0, 0.0), vec3(0.0, -1.0, 0.0));
        // linke und rechte Wand, je zwei Schichten, je ein Viereck
        assert_eq!(caps.triangles.len(), 2 * 2 * 2);
        let xs: Vec<f64> = caps
            .triangles
            .iter()
            .flat_map(|t| t.p.map(|p| p.x))
            .collect();
        let lo = xs.iter().cloned().fold(f64::MAX, f64::min);
        let hi = xs.iter().cloned().fold(f64::MIN, f64::max);
        assert!(lo.abs() < 1e-9 && (hi - 5000.0).abs() < 1e-9, "{lo} {hi}");
    }
}

#[cfg(test)]
mod richtung {
    use super::*;

    #[test]
    fn links_bezug_waechst_nach_rechts() {
        let w = WallChain {
            points: vec![vec3(0.0, 0.0, 0.0), vec3(0.0, 4000.0, 0.0)],
            closed: false,
            ref_side: RefSide::Left,
            layers: vec![Layer::new(400.0, material::PLAIN)],
            base: 0.0,
            height: 3500.0,
            joints: Default::default(),
        };
        let s = w.solid();
        let xs: Vec<f64> = s
            .triangles
            .iter()
            .flat_map(|t| t.p.iter().map(|p| p.x))
            .collect();
        let (lo, hi) = (
            xs.iter().cloned().fold(f64::MAX, f64::min),
            xs.iter().cloned().fold(f64::MIN, f64::max),
        );
        assert!(lo.abs() < 1e-9 && (hi - 400.0).abs() < 1e-9, "{lo} {hi}");
    }
}

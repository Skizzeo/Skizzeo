//! Wandzug: Polygonzug auf dem Boden, aus dem Wände mit Dicke und Höhe entstehen.
//!
//! Die gezeichnete Linie ist die Bezugslinie der Wand. Die Bezugsseite sagt,
//! welche Wandfläche auf dieser Linie liegt, in Zeichenrichtung gesehen.
//! Wird im Uhrzeigersinn gezeichnet, liegt links außen: Bezugsseite `Left`
//! bedeutet dann „außen“, der Wandkörper wächst nach rechts ins Gebäude.

use crate::solid::{edge_kind, material, merge_seam, Solid};
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
    pub fn span(self, t: f64) -> (f64, f64) {
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
    /// Luftschicht (K4): zählt zur Dicke, hat aber keinen Körper.
    pub air: bool,
}

impl Layer {
    pub const fn new(thickness: f64, material: u16) -> Layer {
        Layer {
            thickness,
            material,
            core: false,
            air: false,
        }
    }

    /// Schicht des tragenden Kerns.
    pub const fn core(thickness: f64, material: u16) -> Layer {
        Layer {
            thickness,
            material,
            core: true,
            air: false,
        }
    }

    /// Luftschicht: verschiebt die Schichten dahinter, ohne Körper.
    pub const fn air(thickness: f64, material: u16) -> Layer {
        Layer {
            thickness,
            material,
            core: false,
            air: true,
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
    /// Vor der Decke liegt ein Randdämmstreifen (K5): Außenfläche und
    /// Schnittkontur laufen über das Deckenband ohne Kante durch; die
    /// Kanten dort zeichnet der Streifen.
    pub seamless: bool,
    /// Der Zug steht auf einem mit Randdämmstreifen (K5): im Schnitt keine
    /// Linie am Fuß; über dem Streifen läuft die Wand ohne Fuge weiter, über
    /// der Decke zeichnet die Decke ihre Kontur.
    pub strip_below: bool,
    /// Je Segment: steht es gelöst oder versetzt auf dem Streifen (OG
    /// Phase 2, G7 K3)? Dort zeichnet der Fuß im Schnitt seine Linie.
    pub strip_open: Vec<bool>,
    /// Das Geschoss darüber springt vor (OG Phase 2, G7 K4): Die nicht
    /// tragenden Außenschichten laufen dort bis UK Untersichtdämmung herab.
    pub overhang: Option<Overhang>,
}

/// Vorsprung des Geschosses darüber (G7 K4). Zwischen `from` (UK
/// Untersichtdämmung) und `to` (OK Decke) liegen die Schichten außen vor
/// dem tragenden Kern in der Lage der Wand darüber: Segment `i` um
/// `offsets[i]` (≥ 0) nach außen, so dass sie die Deckenstirn decken und
/// ohne Fuge in die Schichten darüber übergehen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overhang {
    pub offsets: Vec<f64>,
    pub from: f64,
    pub to: f64,
}

impl Overhang {
    /// Springt ein Segment vor?
    pub fn any(&self) -> bool {
        self.offsets.iter().any(|d| *d > 0.0)
    }
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
        Some((*pts.get(seg)?, *dirs.get(seg)?))
    }

    /// Anfangspunkt und Richtung aller Segmente, wie [`WallChain::segment_frame`]
    /// mit nur einer Bereinigung der Punkte.
    pub(crate) fn segment_frames(&self) -> Vec<(Vec3, Vec3)> {
        self.layout().map_or_else(Vec::new, |(pts, _, dirs)| {
            pts.into_iter().zip(dirs).collect()
        })
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
    pub fn outward_sign(&self) -> f64 {
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
        for (j, &pj) in pts.iter().enumerate() {
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
                        pj + shift(b)
                    } else {
                        // Schnitt von pj + shift(a) + dirs[a]·s mit pj + shift(b) + dirs[b]·u
                        let (pa, pb) = (pj + shift(a), pj + shift(b));
                        pa + dirs[a] * (cross2(pb - pa, dirs[b]) / c)
                    }
                }
                (Some(a), None) => pj + shift(a),
                (None, Some(b)) => pj + shift(b),
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
        // Nicht benachbarte Segmente dürfen sich auch nicht kreuzen (G7 K2)
        if closed && !sk_math::polygon::is_simple(&out) {
            return None;
        }
        Some(WallChain {
            points: out,
            closed,
            ..self.clone()
        })
    }

    /// Wandzug des Geschosses darüber (Gebäude aus einem Polygon, B12):
    /// gleicher Aufbau und gleiche Bezugsseite, Segment `i` um `offsets[i]`
    /// nach außen versetzt (Phase 1 überall 0), Fuß bei `base`, Krone bei
    /// `top`. Ohne Anschlüsse; die Deckentasche setzt das Modell wie im EG.
    pub fn stacked(&self, offsets: &[f64], base: f64, top: f64) -> Option<WallChain> {
        if top.is_nan() || base.is_nan() || top <= base {
            return None;
        }
        let mut c = self.with_segment_offsets(offsets)?;
        c.base = base;
        c.height = top - base;
        c.joints = Joints::default();
        Some(c)
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

    /// Abschnitte (von, bis, verlängert) der Schicht `layer`: wie
    /// [`WallChain::layer_spans`], bei einem Vorsprung darüber (K4) die
    /// Außenschichten zusätzlich geteilt; `true` liegt in der Lage von
    /// [`WallChain::overhang_chain`].
    pub fn layer_parts(&self, layer: usize) -> Vec<(f64, f64, bool)> {
        let spans = self.layer_spans(layer);
        let Some(o) = self
            .joints
            .overhang
            .as_ref()
            .filter(|o| o.any() && layer < self.band_layers().start)
        else {
            return spans.into_iter().map(|(a, b)| (a, b, false)).collect();
        };
        let mut out = Vec::with_capacity(spans.len() + 2);
        for (z0, z1) in spans {
            let cuts = [z0, o.from.clamp(z0, z1), o.to.clamp(z0, z1), z1];
            for k in 0..3 {
                if cuts[k + 1] - cuts[k] > 1e-6 {
                    out.push((cuts[k], cuts[k + 1], k == 1));
                }
            }
        }
        out
    }

    /// Lage der verlängerten Außenschichten (K4): der Zug mit den Segmenten
    /// um den Vorsprung des Geschosses darüber nach außen. `None` ohne
    /// Vorsprung.
    pub fn overhang_chain(&self) -> Option<WallChain> {
        let o = self.joints.overhang.as_ref().filter(|o| o.any())?;
        let mut c = self.with_segment_offsets(&o.offsets)?;
        c.joints.overhang = None;
        Some(c)
    }

    /// Gruppe eines Abschnitts für die Nähte bei einem Vorsprung (K4):
    /// 0 bis UK Untersichtdämmung, 1 verlängert, 2 darüber.
    fn part_group(&self, layer: usize, (z0, ext): (f64, bool)) -> usize {
        match &self.joints.overhang {
            _ if ext => 1,
            Some(o) if layer < self.band_layers().start && z0 >= o.to - 1e-6 => 2,
            _ => 0,
        }
    }

    /// Fügt die Gruppen aus [`WallChain::part_group`] zusammen; wo eine
    /// Schicht in derselben Flucht weiterläuft, ohne Naht.
    fn join_parts(&self, mut g: [Solid; 3]) -> Solid {
        if let Some(o) = &self.joints.overhang {
            let [a, b, c] = &mut g;
            merge_seam(a, b, o.from);
            merge_seam(b, c, o.to);
        }
        let [mut a, b, c] = g;
        a.append(&b);
        a.append(&c);
        a
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
        let ext = self.overhang_chain();
        let mut g: [Solid; 3] = Default::default();
        for (i, ((lo, hi, mat), l)) in self
            .layer_offsets()
            .into_iter()
            .zip(&self.layers)
            .enumerate()
        {
            if l.air {
                continue;
            }
            for (z0, z1, e) in self.layer_parts(i) {
                if z0 >= cut {
                    continue;
                }
                let s = &mut g[self.part_group(i, (z0, e))];
                let chain = if e {
                    ext.as_ref().unwrap_or(self)
                } else {
                    self
                };
                s.mat = mat;
                let (top, top_mat) = if z1 > cut {
                    (cut, mat | material::CUT)
                } else {
                    (z1, mat)
                };
                chain.prism(s, i, lo, hi, (z0, top), top_mat, l.cut_kind());
            }
        }
        self.join_parts(g)
    }

    /// Schnittflächen der Wand mit der senkrechten Ebene durch `p0` mit Normale `n`
    /// (Flächen zeigen in Richtung `n`). Jede Schicht ist umrandet: der tragende Kern
    /// dick, die übrigen Schichten mitteldick.
    pub fn section_caps(&self, p0: Vec3, n: Vec3) -> Solid {
        let ext = self.overhang_chain();
        let mut g: [Solid; 3] = Default::default();
        for (li, ((lo, hi, mat), l)) in self
            .layer_offsets()
            .into_iter()
            .zip(&self.layers)
            .enumerate()
        {
            if l.air {
                continue;
            }
            for (z0, z1, e) in self.layer_parts(li) {
                let chain = if e {
                    ext.as_ref().unwrap_or(self)
                } else {
                    self
                };
                let s = &mut g[self.part_group(li, (z0, e))];
                chain.layer_caps(s, (li, lo, hi, mat), l.cut_kind(), (z0, z1), p0, n);
            }
        }
        let mut s = self.join_parts(g);
        s.edge_kind = edge_kind::VIEW;
        s
    }

    /// Schnittflächen der Schicht `li` (Lage `lo`..`hi`, Baustoff `mat`)
    /// zwischen `z0` und `z1`, siehe [`WallChain::section_caps`].
    fn layer_caps(
        &self,
        s: &mut Solid,
        (li, lo, hi, mat): (usize, f64, f64, u16),
        kind: u8,
        (z0, z1): (f64, f64),
        p0: Vec3,
        n: Vec3,
    ) {
        let Some((pts, _, dirs)) = self.layout() else {
            return;
        };
        let n = vec3(n.x, n.y, 0.0).normalized();
        let along = vec3(-n.y, n.x, 0.0);
        let side = |p: Vec3| (p - p0).dot(n);
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
            let (a, b) = (flat(a) + vec3(0.0, 0.0, z0), flat(b) + vec3(0.0, 0.0, z0));
            let up = vec3(0.0, 0.0, z1 - z0);
            let (v0, v1) = (z0 / t, z1 / t);
            s.quad_uv(
                [a, b, b + up, a + up],
                n,
                [[v0, v(a)], [v0, v(b)], [v1, v(b)], [v1, v(a)]],
            );
            s.edge_kind = kind;
            // Am Deckenband mit Randdämmstreifen keine Querlinie:
            // die Decke zeichnet ihre eigene, der Streifen keine
            let (mut bottom, top) = match self.joints.slab_band {
                Some((zb, zt)) if self.joints.seamless => {
                    ((z0 - zt).abs() < 1e-6, (z1 - zb).abs() < 1e-6)
                }
                _ => (false, false),
            };
            bottom |= self.joints.strip_below
                && self.joints.strip_open.get(i) != Some(&true)
                && (z0 - self.base).abs() < 1e-6;
            if !bottom {
                s.edge(a, b);
            }
            if !top {
                s.edge(a + up, b + up);
            }
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

    /// Fällt die Unter- bzw. Oberkante eines Abschnitts `z` auf der Fläche
    /// `off` weg, weil dort ein Randdämmstreifen fugenlos anschließt?
    /// Gilt für die Außenfläche am Deckenband ([`Joints::seamless`]).
    fn seam_at(&self, off: f64, (z0, z1): (f64, f64)) -> (bool, bool) {
        match self.joints.slab_band {
            Some((b, t)) if self.joints.seamless && (off - self.outer_offset()).abs() < 1e-6 => {
                ((z0 - t).abs() < 1e-6, (z1 - b).abs() < 1e-6)
            }
            _ => (false, false),
        }
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
                let (bottom, top) = self.seam_at(off, (z0, z1));
                s.edge_kind = kind_at(off, edge_kind::VIEW);
                if !bottom {
                    self.gapped_edge(s, i, off, (pts[i], dirs[i]), p, q);
                }
                s.edge_kind = if cut {
                    cut_kind
                } else {
                    kind_at(off, top_kind)
                };
                if !top || cut {
                    self.gapped_edge(s, i, off, (pts[i], dirs[i]), p + up, q + up);
                }
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

#[cfg(test)]
mod obergeschoss {
    use super::*;
    use crate::floor::{FloorParams, FloorSlab};
    use crate::solid::{merge_seam, Edge};

    const AAC: u16 = 1;
    const INSULATION: u16 = 2;
    const CONCRETE: u16 = 7;
    const OK_EG: f64 = 2855.0;
    const OK_OG: f64 = 5835.0;

    /// B12: Rechteck 10 × 8 m im Uhrzeigersinn, AW 31,5, EG ±0 … +2,855.
    fn eg() -> WallChain {
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
            height: OK_EG,
            joints: Default::default(),
        }
    }

    /// Wand und Decke eines Geschosses mit Tasche (Decke 22 cm an der Krone).
    fn mit_decke(mut w: WallChain) -> (WallChain, FloorSlab) {
        let p = FloorParams {
            top: w.top(),
            thickness: 220.0,
            mat: CONCRETE,
        };
        let f = FloorSlab::from_chain(&w, &p).unwrap();
        w.joints.slab_band = Some(f.band());
        (w, f)
    }

    /// Volumen der Flächen mit Baustoff `mat` (Divergenzsatz), in m³.
    fn volume(s: &Solid, mat: u16) -> f64 {
        s.triangles
            .iter()
            .filter(|t| t.mat & !material::CUT == mat)
            .map(|t| {
                let [a, b, c] = t.p;
                let cr = (b - a).cross(c - a);
                let sign = if cr.dot(t.n) >= 0.0 { 1.0 } else { -1.0 };
                sign * a.dot(b.cross(c)) / 6.0
            })
            .sum::<f64>()
            * 1e-9
    }

    fn near(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    fn stapel(offsets: &[f64]) -> ((WallChain, FloorSlab), (WallChain, FloorSlab)) {
        let (w0, f0) = mit_decke(eg());
        let og = eg().stacked(offsets, OK_EG, OK_OG).unwrap();
        ((w0, f0), mit_decke(og))
    }

    /// Waagerechte Kanten in Höhe `z`, die auf der Außenfläche liegen
    /// (Abstand zur Außenkontur des EG < `tol`) bzw. alle in `z`.
    fn edges_at(s: &Solid, z: f64) -> Vec<Edge> {
        s.edges
            .iter()
            .filter(|e| near(e.a.z, z, 1e-6) && near(e.b.z, z, 1e-6))
            .copied()
            .collect()
    }

    #[test]
    fn sollwerte_b12_eg_und_og() {
        let ((w0, f0), (w1, f1)) = stapel(&[0.0; 4]);
        assert_eq!((w1.base, w1.top()), (OK_EG, OK_OG));
        assert_eq!(w1.points, w0.points);
        // Decken DE-001 und DE-002 gleich
        for f in [&f0, &f1] {
            assert!(near(f.area() * 1e-6, 75.0384, 1e-6));
            assert!(near(f.volume() * 1e-9, 16.508448, 1e-6));
        }
        assert_eq!(f1.band(), (5615.0, OK_OG));
        let (s0, s1) = (w0.solid(), w1.solid());
        // EG: Gasbeton netto 15,761252, Dämmung 14,1654
        assert!(
            near(volume(&s0, AAC), 15.761252, 1e-6),
            "{}",
            volume(&s0, AAC)
        );
        assert!(
            near(volume(&s0, INSULATION), 14.1654, 1e-4),
            "{}",
            volume(&s0, INSULATION)
        );
        // OG: Gasbeton netto 16,50894, Dämmung 14,7856
        assert!(
            near(volume(&s1, AAC), 16.50894, 1e-5),
            "{}",
            volume(&s1, AAC)
        );
        assert!(
            near(volume(&s1, INSULATION), 14.7856, 1e-4),
            "{}",
            volume(&s1, INSULATION)
        );
        // OG-Gasbeton steht auf OK EG-Decke und endet unter DE-002
        assert_eq!(w1.layer_spans(1), vec![(OK_EG, 5615.0)]);
        assert_eq!(w1.layer_spans(0), vec![(OK_EG, OK_OG)]);
    }

    #[test]
    fn schale_ohne_naht() {
        let ((w0, _), (w1, _)) = stapel(&[0.0; 4]);
        let (mut s0, mut s1) = (w0.solid(), w1.solid());
        let (n0, n1) = (s0.triangles.len(), s1.triangles.len());
        assert!(!edges_at(&s0, OK_EG).is_empty() && !edges_at(&s1, OK_EG).is_empty());
        merge_seam(&mut s0, &mut s1, OK_EG);
        // Die Dämmung läuft ohne Kante durch: In +2,855 bleibt im EG nichts,
        // im OG nur der Fuß des Gasbetons (auf der Decke, anderer Baustoff)
        assert!(
            edges_at(&s0, OK_EG).is_empty(),
            "{:?}",
            edges_at(&s0, OK_EG)
        );
        let rest = edges_at(&s1, OK_EG);
        for e in &rest {
            // nur Kanten an der Gasbeton-Innen- oder -Außenseite (≥ 140 mm innen)
            let inside = |p: Vec3| p.x.min(p.y).min(10000.0 - p.x).min(8000.0 - p.y);
            assert!(inside(e.a) > 139.0 && inside(e.b) > 139.0, "{e:?}");
        }
        // Deckfläche der EG-Dämmung und Bodenfläche der OG-Dämmung entfallen
        assert_eq!(n0 - s0.triangles.len(), 8);
        assert_eq!(n1 - s1.triangles.len(), 8);
        // Keine Kante auf der Außenfläche in +2,855
        let on_outer = |e: &Edge| e.a.x.abs() < 1e-6 && e.b.x.abs() < 1e-6;
        assert!(!edges_at(&s1, OK_EG).iter().any(on_outer));
    }

    #[test]
    fn schnitt_ohne_naht() {
        let ((w0, _), (w1, _)) = stapel(&[0.0; 4]);
        let (p0, n) = (vec3(5000.0, 4000.0, 0.0), vec3(1.0, 0.0, 0.0));
        let (mut c0, mut c1) = (w0.section_caps(p0, n), w1.section_caps(p0, n));
        merge_seam(&mut c0, &mut c1, OK_EG);
        // Dämmung: keine Linie in +2,855 (y in 0..140 und 7860..8000)
        let ins = |e: &Edge| {
            let y = 0.5 * (e.a.y + e.b.y);
            !(140.0..=7860.0).contains(&y)
        };
        assert!(!edges_at(&c0, OK_EG).iter().any(ins));
        assert!(!edges_at(&c1, OK_EG).iter().any(ins));
        // Gasbeton OG: Fuß auf der Decke bleibt als Kontur (beide Wände)
        assert_eq!(edges_at(&c1, OK_EG).len(), 2);
    }

    #[test]
    fn vorsprung_gibt_saubere_stufe() {
        // Phase 2 (B12): OG-Wand y = 8 um 0,30 m nach außen
        let ((w0, _), (w1, f1)) = stapel(&[0.0, 300.0, 0.0, 0.0]);
        assert!(near(w1.points[1].y, 8300.0, 1e-9) && near(w1.points[2].y, 8300.0, 1e-9));
        assert_eq!(
            w1.segment_offsets_from(&w0).unwrap(),
            vec![0.0, 300.0, 0.0, 0.0]
        );
        // Sollwerte B12 Phase 2
        assert!(near(f1.area() * 1e-6, 77.9544, 1e-6), "{}", f1.area());
        assert!(near(f1.volume() * 1e-9, 17.149968, 1e-5), "{}", f1.volume());
        let (mut s0, mut s1) = (w0.solid(), w1.solid());
        assert!(
            near(volume(&s1, AAC), 16.7987, 1e-4),
            "{}",
            volume(&s1, AAC)
        );
        assert!(
            near(volume(&s1, INSULATION), 15.0359, 1e-4),
            "{}",
            volume(&s1, INSULATION)
        );
        merge_seam(&mut s0, &mut s1, OK_EG);
        // An der Stufe (Außenfläche EG bei y = 8000 und OG bei y = 8300)
        // bleiben die Kanten, an den übrigen drei Seiten keine Naht außen
        let at_y = |s: &Solid, y: f64| {
            edges_at(s, OK_EG)
                .iter()
                .filter(|e| near(e.a.y, y, 1e-6) && near(e.b.y, y, 1e-6))
                .count()
        };
        assert!(at_y(&s0, 8000.0) > 0 && at_y(&s1, 8300.0) > 0);
        // Westseite nur, wo EG und OG decken (y < 8000); darüber kragt das OG aus
        let west =
            |e: &Edge| e.a.x.abs() < 1e-6 && e.b.x.abs() < 1e-6 && e.a.y.min(e.b.y) < 8000.0 - 1e-6;
        let south = |e: &Edge| e.a.y.abs() < 1e-6 && e.b.y.abs() < 1e-6;
        for s in [&s0, &s1] {
            for e in edges_at(s, OK_EG) {
                assert!(!west(&e) && !south(&e), "{e:?}");
            }
        }
        // Ostseite: die Unterkante der Auskragung (y 8000 … 8300) bleibt
        let ost: Vec<Edge> = edges_at(&s1, OK_EG)
            .into_iter()
            .filter(|e| near(e.a.x, 10000.0, 1e-6) && near(e.b.x, 10000.0, 1e-6))
            .collect();
        assert!(!ost.is_empty());
        for e in &ost {
            assert!(e.a.y.min(e.b.y) >= 8000.0 - 1e-6, "{e:?}");
        }
    }

    #[test]
    fn versatz_abgelehnt_wenn_segment_verschwindet() {
        assert!(eg()
            .stacked(&[0.0, -9000.0, 0.0, 0.0], OK_EG, OK_OG)
            .is_none());
        assert!(eg().stacked(&[0.0; 3], OK_EG, OK_OG).is_none());
        assert!(eg().stacked(&[0.0; 4], OK_OG, OK_EG).is_none());
    }

    #[test]
    fn schnell_genug_fuer_gummiband_zwei_geschosse() {
        let t = std::time::Instant::now();
        let runs = 100;
        for k in 0..runs {
            let mut g = eg();
            g.points[1].y += k as f64;
            g.points[2].y += k as f64;
            let og = g.stacked(&[0.0; 4], OK_EG, OK_OG).unwrap();
            let ((w0, f0), (w1, f1)) = (mit_decke(g), mit_decke(og));
            let (mut s0, mut s1) = (w0.solid(), w1.solid());
            merge_seam(&mut s0, &mut s1, OK_EG);
            let _ = (f0.solid(), f1.solid());
        }
        let per = t.elapsed().as_secs_f64() * 1000.0 / runs as f64;
        eprintln!("EG und OG mit Decken und Naht: {per:.3} ms");
        assert!(per < 5.0);
    }
}

#[cfg(test)]
mod pruefung_stapel {
    //! Robustheit des Stapelns (G6) an schwierigen Umrissen: spitze Winkel,
    //! winzige Vorsprünge, Zwischenpunkte auf geraden Kanten, beide Richtungen.
    use super::*;
    use crate::floor::{FloorParams, FloorSlab};
    use crate::solid::merge_seam;

    const OK_EG: f64 = 2855.0;
    const OK_OG: f64 = 5835.0;

    fn umrisse() -> Vec<(String, Vec<Vec3>)> {
        let mut v = Vec::new();
        for deg in [10.0f64, 20.0] {
            let t = deg.to_radians();
            v.push((
                format!("spitz {deg}°"),
                vec![
                    vec3(0.0, 0.0, 0.0),
                    vec3(12000.0 * t.cos(), 12000.0 * t.sin(), 0.0),
                    vec3(12000.0, 0.0, 0.0),
                ],
            ));
        }
        for bump in [1.0, 50.0, 200.0] {
            v.push((
                format!("Vorsprung {bump} mm"),
                vec![
                    vec3(0.0, 0.0, 0.0),
                    vec3(0.0, 8000.0, 0.0),
                    vec3(4000.0, 8000.0, 0.0),
                    vec3(4000.0, 8000.0 + bump, 0.0),
                    vec3(6000.0, 8000.0 + bump, 0.0),
                    vec3(6000.0, 8000.0, 0.0),
                    vec3(10000.0, 8000.0, 0.0),
                    vec3(10000.0, 0.0, 0.0),
                ],
            ));
        }
        v.push((
            "Zwischenpunkt".into(),
            vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 4000.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ],
        ));
        let mut both = Vec::new();
        for (name, p) in v {
            let mut r = p.clone();
            r.reverse();
            both.push((format!("{name} rechts"), p));
            both.push((format!("{name} links"), r));
        }
        both
    }

    fn mit_decke(mut w: WallChain) -> Option<(WallChain, FloorSlab)> {
        let p = FloorParams {
            top: w.top(),
            thickness: 220.0,
            mat: 7,
        };
        let f = FloorSlab::from_chain(&w, &p).ok()?;
        w.joints.slab_band = Some(f.band());
        Some((w, f))
    }

    fn finite(s: &Solid) -> bool {
        s.triangles.iter().all(|t| {
            t.p.iter()
                .all(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite())
        }) && s
            .edges
            .iter()
            .all(|e| e.a.x.is_finite() && e.b.y.is_finite())
    }

    #[test]
    fn stapel_ohne_naht_an_schwierigen_umrissen() {
        let mut worst: f64 = 0.0;
        for (name, pts) in umrisse() {
            for ref_side in [RefSide::Left, RefSide::Right, RefSide::Center] {
                let eg = WallChain {
                    points: pts.clone(),
                    closed: true,
                    ref_side,
                    layers: vec![Layer::new(140.0, 2), Layer::core(175.0, 1)],
                    base: 0.0,
                    height: OK_EG,
                    joints: Default::default(),
                };
                let t = std::time::Instant::now();
                let m = eg.segment_count();
                let og = eg.stacked(&vec![0.0; m], OK_EG, OK_OG).unwrap();
                assert_eq!(og.points.len(), eg.clean_points().len(), "{name}");
                // Decke kann bei spitzen Winkeln fehlen (eigene Meldung), die
                // Schale muss trotzdem ohne Naht bleiben
                let (w0, w1) = match (mit_decke(eg.clone()), mit_decke(og.clone())) {
                    (Some((a, _)), Some((b, _))) => (a, b),
                    _ => (eg, og),
                };
                let (mut s0, mut s1) = (w0.solid(), w1.solid());
                merge_seam(&mut s0, &mut s1, OK_EG);
                let p0 = vec3(3000.0, 1.0, 0.0);
                let (mut c0, mut c1) = (
                    w0.section_caps(p0, vec3(1.0, 0.0, 0.0)),
                    w1.section_caps(p0, vec3(1.0, 0.0, 0.0)),
                );
                merge_seam(&mut c0, &mut c1, OK_EG);
                worst = worst.max(t.elapsed().as_secs_f64() * 1000.0);
                for s in [&s0, &s1, &c0, &c1] {
                    assert!(finite(s), "{name} {ref_side:?}");
                }
                // EG-Kopf in +2,855: nur Dämmung, die im OG weiterläuft
                let rest: Vec<_> = s0
                    .edges
                    .iter()
                    .filter(|e| (e.a.z - OK_EG).abs() < 1e-6 && (e.b.z - OK_EG).abs() < 1e-6)
                    .collect();
                assert!(rest.is_empty(), "{name} {ref_side:?}: {rest:?}");
            }
        }
        eprintln!("Stapel je Umriss höchstens {worst:.3} ms (Debug)");
    }

    #[test]
    fn versatz_an_zwischenpunkt() {
        let pts = &umrisse()[10].1;
        let eg = WallChain {
            points: pts.clone(),
            closed: true,
            ref_side: RefSide::Left,
            layers: vec![Layer::new(140.0, 2), Layer::core(175.0, 1)],
            base: 0.0,
            height: OK_EG,
            joints: Default::default(),
        };
        assert_eq!(eg.segment_count(), 5);
        // Fluchtende Nachbarn (Segment 0 und 1) nur gemeinsam
        assert!(eg
            .stacked(&[300.0, 0.0, 0.0, 0.0, 0.0], OK_EG, OK_OG)
            .is_none());
        let og = eg
            .stacked(&[300.0, 300.0, 0.0, 0.0, 0.0], OK_EG, OK_OG)
            .unwrap();
        assert_eq!(
            og.segment_offsets_from(&eg).unwrap(),
            vec![300.0, 300.0, 0.0, 0.0, 0.0]
        );
    }
}

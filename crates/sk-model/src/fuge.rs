//! Fugen und Stufen im Wandzug (W-11, W-12, Jörn 11.10.).
//!
//! Eine **Fuge** ist ein Eckpunkt zwischen zwei Segmenten, die in einer
//! Flucht liegen (gleiche Richtung). Dort stoßen zwei getrennte Wände
//! aneinander; „Vereinen“ nimmt den Punkt heraus, „Teilen“ setzt einen.
//!
//! Wird ein Segment parallel verschoben, gleiten seine Nachbarn auf ihrer
//! Linie. Liegt der Nachbar in einer Flucht mit ihm, entsteht an der Fuge
//! eine **Stufe** (ein neues Segment quer dazu): so springt ein Teilstück
//! vor oder zurück. Schrumpft umgekehrt ein Nachbar auf null, weil das
//! Segment in die Flucht der Wand dahinter rückt, fällt er weg und es
//! bleibt eine Fuge; zieht man weiter, entsteht die Stufe auf der anderen
//! Seite.

use crate::wall::{cross2, right_of, WallChain};
use sk_math::{polygon, Vec3};

/// Kürzestes Teilstück beim Teilen (mm).
pub const MIN_PIECE: f64 = 100.0;

/// Zwei Richtungen (Einheitsvektoren) in einer Flucht, gleicher Sinn.
pub(crate) fn same_dir(a: Vec3, b: Vec3) -> bool {
    cross2(a, b).abs() < 1e-6 && a.dot(b) > 0.0
}

/// Ersatz eines Eckpunkts beim Verschieben: die Punkte, die an seine Stelle
/// treten, und der Abstand, bei dem das Segment bündig mit der Wand hinter
/// dem Nachbarn steht (falls es eine gibt).
struct Side {
    points: Vec<Vec3>,
}

impl WallChain {
    /// Fugen des Zuges: je Fuge das Segment, das an ihr beginnt (bereinigte
    /// Punkte; das Segment davor ist das vorige).
    pub fn straight_joints(&self) -> Vec<usize> {
        let Some((_, closed, dirs)) = self.layout() else {
            return Vec::new();
        };
        let m = dirs.len();
        (0..m)
            .filter(|&k| closed || k > 0)
            .filter(|&k| m > 1 && same_dir(dirs[(k + m - 1) % m], dirs[k]))
            .collect()
    }

    /// Zug mit Teilungspunkten auf Segment `seg`: `at` sind Abstände vom
    /// Anfangspunkt des Segments auf der Bezugslinie (mm). `None`, wenn ein
    /// Teilstück kürzer als [`MIN_PIECE`] würde.
    pub fn with_splits(&self, seg: usize, at: &[f64]) -> Option<WallChain> {
        let (pts, closed, dirs) = self.layout()?;
        let n = pts.len();
        let dir = *dirs.get(seg)?;
        let len = (pts[(seg + 1) % n] - pts[seg]).length();
        let mut ts: Vec<f64> = at.to_vec();
        if ts.is_empty() || ts.iter().any(|t| !t.is_finite()) {
            return None;
        }
        ts.sort_by(f64::total_cmp);
        let mut last = 0.0;
        for t in ts.iter().copied().chain([len]) {
            if t - last < MIN_PIECE - 1e-6 {
                return None;
            }
            last = t;
        }
        let mut out = Vec::with_capacity(n + ts.len());
        for (k, p) in pts.iter().enumerate() {
            out.push(*p);
            if k == seg {
                out.extend(ts.iter().map(|t| pts[seg] + dir * *t));
            }
        }
        Some(WallChain {
            points: out,
            closed,
            ..self.clone()
        })
    }

    /// Zug ohne die Fuge vor Segment `seg` (siehe [`WallChain::straight_joints`]):
    /// die beiden Segmente werden eins. `None`, wenn dort keine Fuge ist.
    pub fn without_joint(&self, seg: usize) -> Option<WallChain> {
        if !self.straight_joints().contains(&seg) {
            return None;
        }
        let mut pts = self.clean_points();
        pts.remove(seg);
        Some(WallChain {
            points: pts,
            ..self.clone()
        })
    }

    /// Zug mit der Fuge vor Segment `seg` um `d` längs der Flucht verschoben
    /// (+ in Laufrichtung): das Stück davor wird länger, das ab ihr kürzer.
    /// `None` ohne Fuge dort oder wenn ein Stück kürzer als [`MIN_PIECE`]
    /// würde.
    pub fn with_joint_moved(&self, seg: usize, d: f64) -> Option<WallChain> {
        if !d.is_finite() || !self.straight_joints().contains(&seg) {
            return None;
        }
        let mut pts = self.clean_points();
        let n = pts.len();
        let (a, b) = (pts[(seg + n - 1) % n], pts[(seg + 1) % n]);
        let dir = (b - a).normalized();
        let p = pts[seg] + dir * d;
        if (p - a).dot(dir) < MIN_PIECE - 1e-6 || (b - p).dot(dir) < MIN_PIECE - 1e-6 {
            return None;
        }
        pts[seg] = p;
        Some(WallChain {
            points: pts,
            ..self.clone()
        })
    }

    /// Verschiebt Segment `i` um `d` entlang seiner Normalen wie beim Ziehen
    /// am Gummiband (siehe [`WallChain::with_segment_moved`]). Dazu: Rückt es
    /// in die Flucht der Wand hinter einem schrumpfenden Nachbarn, fällt der
    /// Nachbar weg und es bleibt eine Fuge; steht es höchstens `snap` davor,
    /// rastet es dort ein; darüber hinaus springt die Stufe auf die andere
    /// Seite.
    pub fn with_segment_moved_snap(&self, i: usize, d: f64, snap: f64) -> Option<WallChain> {
        self.moved(i, d, snap, true)
    }

    /// Gemeinsamer Kern: `through` erlaubt das Wegfallen eines Nachbarn
    /// (Einrasten in die Flucht und darüber hinaus).
    pub(crate) fn moved(&self, i: usize, d: f64, snap: f64, through: bool) -> Option<WallChain> {
        let (pts, closed, dirs) = self.layout()?;
        let (n, m) = (pts.len(), dirs.len());
        if i >= m || !d.is_finite() {
            return None;
        }
        let nrm = right_of(dirs[i]);
        let prev = (closed || i > 0).then(|| (i + m - 1) % m);
        let next = (closed || i + 1 < m).then(|| (i + 1) % m);
        let (a, b) = (i, (i + 1) % n);
        // Bündig mit der Wand `k` hinter dem Nachbarn `j` (gleiche Richtung
        // wie `i`): Abstand, bei dem die Linie von `i` durch den fernen
        // Punkt `far` von `j` geht
        let flush = |j: Option<usize>, before: bool| -> Option<(f64, Vec3)> {
            let j = j?;
            if m < 4 || same_dir(dirs[j], dirs[i]) {
                return None;
            }
            let (k, far) = if before {
                ((closed || j > 0).then(|| (j + m - 1) % m)?, pts[j])
            } else {
                (
                    (closed || j + 1 < m).then(|| (j + 1) % m)?,
                    pts[(j + 1) % n],
                )
            };
            (k != i && k != j && same_dir(dirs[k], dirs[i])).then(|| ((far - pts[i]).dot(nrm), far))
        };
        let (fa, fb) = if through {
            (flush(prev, true), flush(next, false))
        } else {
            (None, None)
        };
        let mut d = d;
        let near = [fa, fb]
            .into_iter()
            .flatten()
            .map(|(f, _)| f)
            .filter(|f| (d - f).abs() <= snap)
            .min_by(|x, y| (d - x).abs().total_cmp(&(d - y).abs()));
        if let Some(f) = near {
            d = f;
        }
        if d.abs() < 1e-9 {
            return Some(WallChain {
                points: pts,
                closed,
                ..self.clone()
            });
        }
        let shift = nrm * d;
        // Ersatz des Eckpunkts `v` zwischen `i` und dem Nachbarn `j`
        let side = |v: usize, j: Option<usize>, before: bool, fl: Option<(f64, Vec3)>| {
            let Some(j) = j else {
                // Freies Ende: wandert parallel mit
                return Some(Side {
                    points: vec![pts[v] + shift],
                });
            };
            if same_dir(dirs[j], dirs[i]) {
                // Fuge: Stufe quer zur Flucht
                if d.abs() <= 1.0 {
                    return None;
                }
                let p = if before {
                    vec![pts[v], pts[v] + shift]
                } else {
                    vec![pts[v] + shift, pts[v]]
                };
                return Some(Side { points: p });
            }
            // Schnitt der verschobenen Linie mit der Linie des Nachbarn
            let c = cross2(dirs[j], dirs[i]);
            let x = pts[v] + dirs[j] * (cross2(shift, dirs[i]) / c);
            let rest = if before {
                x - pts[j]
            } else {
                pts[(j + 1) % n] - x
            };
            if rest.length() > 1.0 && rest.dot(dirs[j]) > 0.0 {
                return Some(Side { points: vec![x] });
            }
            // Der Nachbar verschwindet: nur an der Flucht einer Wand dahinter
            let (f, far) = fl?;
            if (d - f).abs() <= 1.0 {
                return Some(Side { points: Vec::new() });
            }
            // Darüber hinaus: Stufe auf der anderen Seite
            let s = far + nrm * (d - f);
            Some(Side { points: vec![s] })
        };
        let sa = side(a, prev, true, fa)?;
        let sb = side(b, next, false, fb)?;
        // Der Nachbar fällt weg: sein ferner Punkt ist schon Teil des Zuges
        let mut out = Vec::with_capacity(n + 2);
        for (v, p) in pts.iter().enumerate() {
            if v == a {
                out.extend(sa.points.iter().copied());
            } else if v == b {
                out.extend(sb.points.iter().copied());
            } else {
                out.push(*p);
            }
        }
        let k = out.len();
        let segs = if closed { k } else { k.saturating_sub(1) };
        if (closed && k < 3) || k < 2 {
            return None;
        }
        if (0..segs).any(|s| (out[(s + 1) % k] - out[s]).length() <= 1.0) {
            return None;
        }
        if closed && !polygon::is_simple(&out) {
            return None;
        }
        Some(WallChain {
            points: out,
            closed,
            ..self.clone()
        })
    }
    /// Wie [`WallChain::with_segment_offsets`], aber zwei Nachbarn in einer
    /// Flucht dürfen verschieden weit zurückspringen (W-12, Dachterrasse über
    /// einem Teilstück): An ihrer Fuge entsteht eine Stufe quer zur Flucht.
    /// Vorspringen darf dort keiner (ein Erker über einem Teil der Wand
    /// kommt später, Jörn 07.10.). Dazu je Segment des Ergebnisses das
    /// Segment von `self`, aus dem es versetzt ist; `None` für eine Stufe.
    /// Die Stufe an Punkt 0 steht am Ende, so bleibt Segment k über k, bis
    /// die erste Stufe kommt. `face`: Versatz der Außenfläche der Wand, die
    /// so liegt (ihr [`WallChain::outer_offset`]); die Stufe steht mit der
    /// Außenfläche auf der Fuge, gleich wo ihre Bezugslinie liegt
    /// (Geometrie #69).
    pub fn with_segment_offsets_stepped(
        &self,
        offsets: &[f64],
        face: f64,
    ) -> Option<(WallChain, Vec<Option<usize>>)> {
        let (pts, closed, dirs) = self.layout()?;
        let (n, m) = (pts.len(), dirs.len());
        if offsets.len() != m || offsets.iter().any(|d| !d.is_finite()) {
            return None;
        }
        let sign = self.outward_sign();
        let shift = |i: usize| right_of(dirs[i]) * (sign * offsets[i]);
        // Je Eckpunkt ein Punkt, an einer Stufe zwei
        let mut corner: Vec<Vec<Vec3>> = Vec::with_capacity(n);
        for (j, &pj) in pts.iter().enumerate() {
            let prev = (closed || j > 0).then(|| (j + m - 1) % m);
            let next = (closed || j < n - 1).then_some(j % m);
            let p = match (prev, next) {
                (Some(a), Some(b)) => {
                    let c = cross2(dirs[a], dirs[b]);
                    if c.abs() < 1e-9 {
                        if dirs[a].dot(dirs[b]) < 0.0 {
                            return None;
                        }
                        if (offsets[a] - offsets[b]).abs() <= 1e-6 {
                            vec![pj + shift(b)]
                        } else if offsets[a] > 1e-6 || offsets[b] > 1e-6 {
                            return None;
                        } else {
                            let back = right_of((shift(b) - shift(a)).normalized()) * -face;
                            vec![pj + shift(a) + back, pj + shift(b) + back]
                        }
                    } else {
                        let (pa, pb) = (pj + shift(a), pj + shift(b));
                        vec![pa + dirs[a] * (cross2(pb - pa, dirs[b]) / c)]
                    }
                }
                (Some(a), None) => vec![pj + shift(a)],
                (None, Some(b)) => vec![pj + shift(b)],
                (None, None) => return None,
            };
            corner.push(p);
        }
        // Punkte und Zuordnung: Segment k läuft vom letzten Punkt der Ecke k
        // zum ersten der Ecke k + 1
        let mut out = Vec::new();
        let mut map = Vec::new();
        for (j, c) in corner.iter().enumerate() {
            if j == 0 && closed {
                out.push(*c.last()?);
            } else {
                out.extend(c.iter().copied());
                map.extend(std::iter::repeat_n(None, c.len() - 1));
            }
            if j < m {
                map.push(Some(j));
            }
        }
        if closed && corner[0].len() == 2 {
            out.push(corner[0][0]);
            map.push(None);
        }
        let k = out.len();
        for (i, src) in map.iter().enumerate() {
            if !closed && i + 1 >= k {
                break;
            }
            // Länger als 1 mm wie in `clean_points`, sonst fiele das Stück
            // dort weg und die Zuordnung verrutschte (H-160d)
            let v = out[(i + 1) % k] - out[i];
            let ok = match src {
                Some(s) => v.length() > 1.0 && v.dot(dirs[*s]) > 0.0,
                None => v.length() > 1.0,
            };
            if !ok {
                return None;
            }
        }
        if closed && !polygon::is_simple(&out) {
            return None;
        }
        Some((
            WallChain {
                points: out,
                closed,
                ..self.clone()
            },
            map,
        ))
    }

    /// Versatz nach außen des Segments `j` von `self` gegenüber Segment `k`
    /// des Zuges `below` (Bezugslinie zu Bezugslinie); `None`, wenn sie
    /// nicht in gleicher Richtung parallel liegen.
    pub fn offset_against(&self, j: usize, below: &WallChain, k: usize) -> Option<f64> {
        self.offsets_against(below, [(j, k)]).pop().flatten()
    }

    /// [`WallChain::offset_against`] für viele Paare (j, k): Lage und
    /// Außenseite nur einmal, nicht je Paar (Review L9, Mengen auf „Groß“).
    pub fn offsets_against(
        &self,
        below: &WallChain,
        pairs: impl IntoIterator<Item = (usize, usize)>,
    ) -> Vec<Option<f64>> {
        let (Some((pa, _, da)), Some((pb, _, db))) = (below.layout(), self.layout()) else {
            return pairs.into_iter().map(|_| None).collect();
        };
        let sign = below.outward_sign();
        pairs
            .into_iter()
            .map(|(j, k)| {
                let (a, b) = (*da.get(k)?, *db.get(j)?);
                (cross2(a, b).abs() < 1e-9 && a.dot(b) > 0.0)
                    .then(|| (pb[j] - pa[k]).dot(right_of(a)) * sign)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solid::material;
    use crate::wall::{Layer, RefSide};
    use sk_math::vec3;

    fn zug(points: Vec<Vec3>) -> WallChain {
        WallChain {
            points,
            closed: true,
            ref_side: RefSide::Left,
            layers: vec![Layer::new(300.0, material::PLAIN)],
            base: 0.0,
            height: 3000.0,
            joints: Default::default(),
        }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-6
    }

    /// Rechteck 10 × 8 m im Uhrzeigersinn, Segment 1 ist die Nordwand
    /// (y = 8000, von links nach rechts).
    fn rechteck() -> WallChain {
        zug(vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ])
    }

    /// Review zu #163: Eine Stufe von genau 1 mm fiele in `clean_points`
    /// weg (dort gilt ≤ 1 mm als derselbe Punkt); dann stünde das Stück
    /// schief. Ab mehr als 1 mm bleibt sie als eigenes Segment.
    #[test]
    fn stufe_genau_ein_millimeter() {
        let c = zug(vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(3000.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ]);
        for d in [1.0, -1.0] {
            assert!(c.with_segment_moved_snap(1, d, 0.0).is_none(), "{d}");
        }
        let s = c.with_segment_moved_snap(1, 1.5, 0.0).unwrap();
        assert_eq!(s.clean_points().len(), s.points.len());
        assert_eq!(s.points.len(), 6);
    }

    /// Review H-163c: Nachbar und Flucht genau 1 mm. Das linke Stück der
    /// Nordwand rückt bis 1 mm vor die Flucht des rechten: Der Nachbar
    /// behielte 1 mm (fiele in `clean_points` weg), also fällt er weg und
    /// das Stück liegt in der Flucht. Bei 1,5 mm bleibt er als Stufe.
    #[test]
    fn nachbar_und_flucht_genau_ein_millimeter() {
        let c = l_form();
        let s = c.with_segment_moved_snap(1, 1999.0, 0.0).unwrap();
        assert_eq!(s.points.len(), 5, "{:?}", s.points);
        let s = c.with_segment_moved_snap(1, 1998.5, 0.0).unwrap();
        assert_eq!(s.points.len(), 6, "{:?}", s.points);
        assert_eq!(s.clean_points().len(), 6);
    }

    /// Review H-163c: `with_segment_offsets` lässt kein Segment von genau
    /// 1 mm stehen (die Stufe der L-Form schrumpft auf 1 mm), wohl aber
    /// eines von 1,5 mm.
    #[test]
    fn versatz_laesst_kein_segment_von_einem_millimeter() {
        let c = l_form();
        let mut o = vec![0.0; c.segment_count()];
        o[1] = -1999.0;
        assert!(c.with_segment_offsets(&o).is_none());
        o[1] = -1998.5;
        let s = c.with_segment_offsets(&o).unwrap();
        assert_eq!(s.clean_points().len(), s.points.len());
    }

    /// Review H-163c: Die Nordwand eines Trapezes (Schrägen 3 : 4, oben
    /// 1501 mm) schrumpft beim Zurückziehen um 1,5 mm je mm. Genau 1 mm
    /// bleibt nicht stehen, 1,75 mm schon; beim Ziehen wie beim Versatz.
    #[test]
    fn segment_genau_ein_millimeter() {
        let c = zug(vec![
            vec3(0.0, 0.0, 0.0),
            vec3(3000.0, 4000.0, 0.0),
            vec3(4501.0, 4000.0, 0.0),
            vec3(7501.0, 0.0, 0.0),
        ]);
        assert!(c.moved(1, -1000.0, 0.0, false).is_none());
        assert!(c.with_segment_offsets(&[0.0, 1000.0, 0.0, 0.0]).is_none());
        let s = c.moved(1, -999.5, 0.0, false).unwrap();
        assert!(near(s.points[2] - s.points[1], vec3(1.75, 0.0, 0.0)));
        let s = c.with_segment_offsets(&[0.0, 999.5, 0.0, 0.0]).unwrap();
        assert!(near(s.points[2] - s.points[1], vec3(1.75, 0.0, 0.0)));
    }

    /// L: Nordwand links bei y = 8000, rechts bei y = 6000, Stufe bei x = 4000.
    fn l_form() -> WallChain {
        zug(vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(4000.0, 8000.0, 0.0),
            vec3(4000.0, 6000.0, 0.0),
            vec3(10000.0, 6000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ])
    }

    /// Mittelstück der Nordwand 1,5 m zurück: zwei Stufen, die Stücke
    /// bleiben in ihrer Linie; vorspringen darf es an der Fuge nicht. Ohne
    /// Unterschied wie `with_segment_offsets`; gemessen zurück wie gesetzt.
    #[test]
    fn fuge_verschieben() {
        let w = rechteck().with_splits(1, &[3000.0, 6000.0]).unwrap();
        let c = w.with_joint_moved(2, 500.0).unwrap();
        assert!(near(c.points[2], vec3(3500.0, 8000.0, 0.0)));
        assert_eq!(c.straight_joints(), vec![2, 3]);
        assert!(w.with_joint_moved(2, -2950.0).is_none(), "zu kurz");
        assert!(w.with_joint_moved(1, 10.0).is_none(), "keine Fuge");
    }

    #[test]
    fn stufen_beim_versatz() {
        let w = rechteck().with_splits(1, &[3000.0, 6000.0]).unwrap();
        let o = [0.0, 0.0, -1500.0, 0.0, 0.0, 0.0];
        let (c, map) = w.with_segment_offsets_stepped(&o, 0.0).unwrap();
        assert_eq!(c.segment_count(), 8);
        let s = |k| Some(k);
        assert_eq!(map, vec![s(0), s(1), None, s(2), None, s(3), s(4), s(5)]);
        assert!(near(c.points[2], vec3(3000.0, 8000.0, 0.0)));
        assert!(near(c.points[3], vec3(3000.0, 6500.0, 0.0)));
        assert!(near(c.points[4], vec3(6000.0, 6500.0, 0.0)));
        assert!(near(c.points[5], vec3(6000.0, 8000.0, 0.0)));
        assert_eq!(c.offset_against(3, &w, 2), Some(-1500.0));
        assert_eq!(c.offset_against(1, &w, 1), Some(0.0));
        assert_eq!(c.offset_against(2, &w, 2), None);
        let mut v = o;
        v[2] = 300.0;
        assert!(
            w.with_segment_offsets_stepped(&v, 0.0).is_none(),
            "Erker später"
        );
        v[2] = 0.0;
        let (c, map) = w.with_segment_offsets_stepped(&v, 0.0).unwrap();
        assert_eq!(c.points, w.with_segment_offsets(&v).unwrap().points);
        assert_eq!(map, (0..6).map(Some).collect::<Vec<_>>());
        // Ganze Wand zurück: keine Stufe, Ecken geschnitten
        let v = [0.0, -500.0, -500.0, -500.0, 0.0, 0.0];
        let (c, map) = w.with_segment_offsets_stepped(&v, 0.0).unwrap();
        assert_eq!(c.segment_count(), 6);
        assert!(map.iter().all(|x| x.is_some()));
        // Stufe an Punkt 0: steht am Ende
        let z = zug(vec![
            vec3(5000.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ]);
        let (c, map) = z
            .with_segment_offsets_stepped(&[-1000.0, 0.0, 0.0, 0.0, 0.0], 0.0)
            .unwrap();
        assert_eq!(map, vec![s(0), s(1), s(2), s(3), s(4), None]);
        assert!(near(c.points[0], vec3(5000.0, 7000.0, 0.0)));
        assert!(near(c.points[5], vec3(5000.0, 8000.0, 0.0)));
    }

    #[test]
    fn teilen_setzt_fugen() {
        let w = rechteck();
        assert!(w.straight_joints().is_empty());
        let t = w.with_splits(1, &[6000.0, 3000.0]).unwrap();
        assert_eq!(t.segment_count(), 6);
        assert!(near(t.points[2], vec3(3000.0, 8000.0, 0.0)));
        assert!(near(t.points[3], vec3(6000.0, 8000.0, 0.0)));
        assert_eq!(t.straight_joints(), vec![2, 3]);
        // Zu kurze Stücke
        assert!(w.with_splits(1, &[50.0]).is_none());
        assert!(w.with_splits(1, &[3000.0, 3050.0]).is_none());
        assert!(w.with_splits(1, &[9950.0]).is_none());
        // Vereinen nimmt den Punkt wieder heraus
        let v = t.without_joint(2).unwrap().without_joint(2).unwrap();
        assert_eq!(v.clean_points(), w.clean_points());
        assert!(t.without_joint(1).is_none());
    }

    /// Mittelstück nach außen (Norden, −Normale) gezogen: zwei Stufen,
    /// die Nachbarn bleiben, wo sie sind.
    #[test]
    fn mittelstueck_springt_mit_zwei_stufen() {
        let t = rechteck().with_splits(1, &[3000.0, 6000.0]).unwrap();
        let v = t.with_segment_moved(2, -1500.0).unwrap();
        assert_eq!(v.segment_count(), 8);
        let p = v.clean_points();
        let soll = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(3000.0, 8000.0, 0.0),
            vec3(3000.0, 9500.0, 0.0),
            vec3(6000.0, 9500.0, 0.0),
            vec3(6000.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        for (a, b) in p.iter().zip(soll) {
            assert!(near(*a, b), "{p:?}");
        }
        // Zurück auf 0: wieder in einer Flucht, ohne Stufen
        let z = v.with_segment_moved_snap(3, 1500.0, 0.0).unwrap();
        assert_eq!(z.clean_points(), t.clean_points());
        assert_eq!(z.straight_joints(), vec![2, 3]);
    }

    /// Eckstück (an der Ecke rechts oben) zurück: eine Stufe, die Ostwand
    /// wird kürzer.
    #[test]
    fn eckstueck_springt_zurueck() {
        let t = rechteck().with_splits(1, &[6000.0]).unwrap();
        let v = t.with_segment_moved(2, 2000.0).unwrap();
        let p = v.clean_points();
        assert_eq!(p.len(), 6);
        assert!(near(p[2], vec3(6000.0, 8000.0, 0.0)));
        assert!(near(p[3], vec3(6000.0, 6000.0, 0.0)));
        assert!(near(p[4], vec3(10000.0, 6000.0, 0.0)));
        assert!(near(p[5], vec3(10000.0, 0.0, 0.0)));
    }

    /// L zusammenschieben: Der rechte Teil der Nordwand rückt in die Flucht
    /// des linken; die Stufe fällt weg, eine Fuge bleibt. Knapp davor rastet
    /// es ein, darüber hinaus springt die Stufe auf die andere Seite.
    #[test]
    fn l_zusammenschieben_laesst_eine_fuge() {
        let l = l_form();
        assert!(l.with_segment_moved_snap(3, -1990.0, 0.0).is_some());
        // Programmatisch (Bündig setzen) bleibt es beim Ablehnen
        assert!(l.with_segment_moved(3, -2000.0).is_none());
        let v = l.with_segment_moved_snap(3, -2000.0, 0.0).unwrap();
        let p = v.clean_points();
        assert_eq!(p.len(), 5, "{p:?}");
        assert!(near(p[2], vec3(4000.0, 8000.0, 0.0)));
        assert!(near(p[3], vec3(10000.0, 8000.0, 0.0)));
        assert_eq!(v.straight_joints(), vec![2]);
        // Rasten: 15 mm davor gilt als bündig
        let s = l.with_segment_moved_snap(3, -1985.0, 20.0).unwrap();
        assert_eq!(s.clean_points(), p);
        // Weiter: Stufe nach außen
        let w = l.with_segment_moved_snap(3, -2500.0, 0.0).unwrap();
        let q = w.clean_points();
        assert_eq!(q.len(), 6, "{q:?}");
        assert!(near(q[2], vec3(4000.0, 8000.0, 0.0)));
        assert!(near(q[3], vec3(4000.0, 8500.0, 0.0)));
        assert!(near(q[4], vec3(10000.0, 8500.0, 0.0)));
    }

    /// Nische schließen: beide Stufen fallen weg, zwei Fugen bleiben.
    #[test]
    fn nische_schliessen() {
        let t = rechteck().with_splits(1, &[3000.0, 6000.0]).unwrap();
        let v = t.with_segment_moved(2, 1000.0).unwrap();
        assert_eq!(v.segment_count(), 8);
        // Segment 3 ist jetzt das Mittelstück
        let z = v.with_segment_moved_snap(3, -1000.0, 0.0).unwrap();
        assert_eq!(z.segment_count(), 6);
        assert_eq!(z.straight_joints(), vec![2, 3]);
    }

    /// Ohne Wand dahinter in der Flucht bleibt es beim Ablehnen.
    #[test]
    fn ohne_flucht_kein_einrasten() {
        let w = rechteck();
        assert!(w.with_segment_moved(1, 8000.0).is_none());
        assert!(w.with_segment_moved_snap(1, 7999.5, 20.0).is_none());
        assert!(w.with_segment_moved_snap(1, 7990.0, 20.0).is_some());
    }
}

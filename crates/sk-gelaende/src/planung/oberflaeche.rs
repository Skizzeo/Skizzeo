//! Das Planungsgelände als Netz für die Darstellung (G3c,
//! plan-gelaendemodul.md „G3 Feinplan“, Darstellung): Wo Flächen und ihre
//! Böschungen das Urgelände ändern, treten an die Stelle seiner Dreiecke
//! die Stücke des Planungsgeländes. Jedes Dreieck des Urgeländes unter
//! einer Fläche oder einem Böschungsbereich wird an den Kanten, die es
//! kreuzen, und an den Linien, an denen zwei Ebenen gleich hoch sind, in
//! konvexe Stücke geteilt; in jedem Stück gilt eine Ebene. So ist das Netz exakt wie
//! [`Planung::hoehe`], auch an Böschungsfuß und -kopf.
//!
//! Mehrere Flächen: Eine Fläche gilt vor jeder Böschung, die zuletzt
//! gesetzte vor den früheren (die Leiste sagt es mit dem Hinweis
//! „überlappt“). Außerhalb der Flächen gilt min(max(U, Auftrag), Abtrag)
//! über die Böschungen aller Flächen, wie [`Planung::hoehe`] für eine.
//!
//! Wo die Höhe an der Kante zweier Stücke springt (eine Fläche ohne
//! Böschung, zwei Flächen, Böschung auf Böschung), steht eine senkrechte
//! Wand, damit das Netz geschlossen bleibt (Review #183 b, B2).

use super::{an, flaeche, innen, rahmen, Planung};
use crate::abfrage::Abfrage;
use crate::volumen::{halbieren, Bezug};
use std::collections::HashMap;

/// Woher ein Dreieck des Planungsgeländes stammt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Herkunft {
    /// Urgelände in einem geteilten Dreieck, wo keine Fläche wirkt.
    Ur,
    /// Teilfläche der Planung mit dieser Nummer.
    Flaeche(usize),
    /// Böschung der Planung `p`: Auftrag oder Abtrag, Nummer des Bereichs
    /// in [`Planung::auftrag`] bzw. [`Planung::abtrag`].
    Boeschung { p: usize, auftrag: bool, b: usize },
}

/// Das Planungsgelände über dem Urgelände.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Oberflaeche {
    /// Dreiecke des Urgeländes, die das Planungsgelände ersetzt (Nummern im
    /// Netz), aufsteigend.
    pub ersetzt: Vec<u32>,
    /// Die Dreiecke an ihrer Stelle, gegen den Uhrzeigersinn, mit Herkunft.
    pub dreiecke: Vec<([[f64; 3]; 3], Herkunft)>,
    /// Senkrechte Wände, wo die Höhe an der Kante zweier Stücke springt;
    /// die Normale zeigt zur tieferen Seite.
    pub waende: Vec<[[f64; 3]; 3]>,
    /// Kanten der Stücke auf dem Rand des Netzes, in seinem Umlauf (Inneres
    /// links), auf ihrer Höhe.
    pub rand: Vec<[[f64; 3]; 2]>,
}

/// Unter dieser Fläche (m²) gilt ein Stück als leer.
const LEER: f64 = 1e-10;

/// Unter diesem Höhenunterschied (m) gelten zwei Ebenen an einer Ecke als
/// gleich hoch: Gleiche Ebenen (zwei Kanten in einer Flucht) teilen sonst
/// am Rundungsrauschen in Stücke, die sich decken (Geometrie #183).
const GLEICH: f64 = 1e-9;

/// Höchstens so viele Schraffen je Planung.
const MAX_SCHRAFFEN: usize = 100_000;

/// Eine Fläche oder ein Böschungsbereich mit Ebene und Herkunft.
struct Gebiet<'a> {
    ring: &'a [[f64; 2]],
    rahmen: [f64; 4],
    ebene: Bezug,
    herkunft: Herkunft,
}

/// Das Planungsgelände der Planungen `planungen` (in dieser Reihenfolge
/// gesetzt) über dem Urgelände `u`. Nur Dreiecke, in denen sich etwas
/// ändert, werden ersetzt.
pub fn oberflaeche(u: &Abfrage, planungen: &[&Planung]) -> Oberflaeche {
    fn dazu<'a>(g: &mut Vec<Gebiet<'a>>, ring: &'a [[f64; 2]], ebene: Bezug, herkunft: Herkunft) {
        if ring.len() >= 3 && flaeche(ring) > LEER {
            g.push(Gebiet {
                ring,
                rahmen: rahmen(ring),
                ebene,
                herkunft,
            });
        }
    }
    let mut gebiete = Vec::new();
    for (p, pl) in planungen.iter().enumerate() {
        for t in &pl.teile {
            dazu(&mut gebiete, &t.ring, t.ebene, Herkunft::Flaeche(p));
        }
        for (auftrag, bereiche) in [(true, &pl.auftrag), (false, &pl.abtrag)] {
            for (b, x) in bereiche.iter().enumerate() {
                let h = Herkunft::Boeschung { p, auftrag, b };
                dazu(&mut gebiete, &x.ring, x.ebene, h);
            }
        }
    }
    // je Dreieck des Urgeländes die Gebiete darüber, in ihrer Reihenfolge
    let mut je: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, g) in gebiete.iter().enumerate() {
        for t in u.nummern_unter(g.ring) {
            je.entry(t).or_default().push(i);
        }
    }
    let mut nummern: Vec<u32> = je.keys().copied().collect();
    nummern.sort_unstable();
    // erst die Stücke aller Dreiecke, dann die Wände: an der Kante zweier
    // ersetzter Dreiecke stehen draußen die Stücke des Nachbarn
    let (gebiete, je) = (&gebiete, &je);
    let teile: Vec<(u32, Vec<Stueck>)> = verteilt(nummern.len(), |r| {
        let mut v = Vec::new();
        for &t in &nummern[r] {
            let stuecke = teilen(&u.punkte_von(t), gebiete, &je[&t]);
            if stuecke.iter().any(|s| s.1 != Herkunft::Ur) {
                v.push((t, stuecke));
            }
        }
        v
    })
    .into_iter()
    .flatten()
    .collect();
    let teile = &teile;
    let von = |n: u32| {
        teile
            .binary_search_by_key(&n, |x| x.0)
            .ok()
            .map(|j| teile[j].1.as_slice())
    };
    let mut out = Oberflaeche::default();
    for o in verteilt(teile.len(), |r| {
        let mut out = Oberflaeche::default();
        for (t, stuecke) in &teile[r] {
            out.ersetzt.push(*t);
            kanten(u, *t, stuecke, &von, &mut out);
            for (ring, h, e) in stuecke {
                let p: Vec<[f64; 3]> = ring
                    .iter()
                    .map(|q| [q[0], q[1], an(e, q[0], q[1])])
                    .collect();
                for i in 1..p.len().saturating_sub(1) {
                    let d = [p[0], p[i], p[i + 1]];
                    if flaeche(&d.map(|q| [q[0], q[1]])) > LEER {
                        out.dreiecke.push((d, *h));
                    }
                }
            }
        }
        out
    }) {
        out.ersetzt.extend(o.ersetzt);
        out.dreiecke.extend(o.dreiecke);
        out.waende.extend(o.waende);
        out.rand.extend(o.rand);
    }
    out
}

/// `f` über `0..n` in Abschnitten auf den Kernen, die Ergebnisse in der
/// Reihenfolge der Abschnitte: Das Ergebnis ist dasselbe wie in einem
/// Faden (Review #183 B1, Flächenänderung kalt).
fn verteilt<T: Send>(n: usize, f: impl Fn(std::ops::Range<usize>) -> T + Sync) -> Vec<T> {
    let k = std::thread::available_parallelism()
        .map_or(1, |k| k.get())
        .min(n.div_ceil(64))
        .max(1);
    if k == 1 {
        return vec![f(0..n)];
    }
    let s = n.div_ceil(k);
    let f = &f;
    std::thread::scope(|sc| {
        let h: Vec<_> = (0..k)
            .map(|i| sc.spawn(move || f(i * s..((i + 1) * s).min(n))))
            .collect();
        h.into_iter()
            .map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    })
}

/// Ein konvexes Stück eines Dreiecks mit Herkunft und Ebene, gegen den
/// Uhrzeigersinn.
type Stueck = (Vec<[f64; 2]>, Herkunft, Bezug);

/// Wände und Rand an den Kanten der Stücke `stuecke` des Dreiecks `t` des
/// Urgeländes `u`. Wo die Höhe an einer Kante springt, steht eine
/// senkrechte Wand (Review #183 b und B2): zwischen zwei Flächen, an einer
/// Fläche ohne Böschung und wo Böschung auf Böschung trifft. Jede Kante
/// gilt einmal: im Dreieck vom Stück mit der kleineren Nummer, an der
/// Grenze zweier ersetzter Dreiecke ([`Abfrage::nachbar`], Stücke aus
/// `von`) vom Dreieck mit der kleineren Nummer, gegen ein Dreieck, das
/// bleibt, gegen dessen Urgelände. Eine Kante wird an den Ecken der Stücke
/// auf ihr geteilt; je Abschnitt gilt draußen das Stück an seiner Mitte.
/// Kanten auf dem Rand des Netzes kommen in [`Oberflaeche::rand`].
fn kanten<'a>(
    u: &Abfrage,
    t: u32,
    stuecke: &[Stueck],
    von: &impl Fn(u32) -> Option<&'a [Stueck]>,
    out: &mut Oberflaeche,
) {
    let d3 = u.punkte_von(t);
    let ecken = d3.map(|q| [q[0], q[1]]);
    let drin = |q: [f64; 2], r: &[[f64; 2]]| {
        (0..r.len()).all(|k| kreuz(r[k], r[(k + 1) % r.len()], q) >= 0.0)
    };
    for (i, (r, _, e)) in stuecke.iter().enumerate() {
        let n = r.len();
        for k in 0..n {
            let (a, b) = (r[k], r[(k + 1) % n]);
            let d = [b[0] - a[0], b[1] - a[1]];
            let l2 = d[0] * d[0] + d[1] * d[1];
            if l2 < LEER {
                continue;
            }
            let l = l2.sqrt();
            // auf einer Kante des Dreiecks?
            let auf = |c: [f64; 2], e: [f64; 2], q: [f64; 2]| {
                let f = [e[0] - c[0], e[1] - c[1]];
                kreuz(c, e, q).abs() < 1e-9 * f[0].hypot(f[1]).max(1e-12)
            };
            let kante = (0..3).find(|&j| {
                let (c, e) = (ecken[j], ecken[(j + 1) % 3]);
                auf(c, e, a) && auf(c, e, b)
            });
            let draussen: &[Stueck] = match kante.map(|j| u.nachbar(t, j)) {
                // Rand des Netzes
                Some(None) => {
                    let z = |q: [f64; 2]| [q[0], q[1], an(e, q[0], q[1])];
                    out.rand.push([z(a), z(b)]);
                    continue;
                }
                Some(Some(nb)) => match von(nb) {
                    Some(_) if nb < t => continue,
                    Some(s) => s,
                    None => {
                        // Das Dreieck daneben bleibt: dort gilt sein Urgelände
                        let ee = ebene_durch(&u.punkte_von(nb));
                        let zi = [an(e, a[0], a[1]), an(e, b[0], b[1])];
                        let zj = [an(&ee, a[0], a[1]), an(&ee, b[0], b[1])];
                        wand(a, b, zi, zj, &mut out.waende);
                        continue;
                    }
                },
                None => stuecke,
            };
            let innen = kante.is_none();
            // Teilpunkte: Ecken der Stücke draußen auf der Kante
            let mut ts = vec![0.0, 1.0];
            for (j, (rj, _, _)) in draussen.iter().enumerate() {
                if innen && j == i {
                    continue;
                }
                for q in rj {
                    let w = [q[0] - a[0], q[1] - a[1]];
                    let s = (w[0] * d[0] + w[1] * d[1]) / l2;
                    let quer = (w[0] * d[1] - w[1] * d[0]) / l;
                    if quer.abs() < 1e-9 && s > 1e-9 && s < 1.0 - 1e-9 {
                        ts.push(s);
                    }
                }
            }
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|x, y| (*x - *y).abs() < 1e-12);
            // draußen ist rechts der Kante (Stücke gegen den Uhrzeigersinn)
            let o = [d[1] / l * 1e-7, -d[0] / l * 1e-7];
            for w in ts.windows(2) {
                let pt = |s: f64| [a[0] + d[0] * s, a[1] + d[1] * s];
                let (pa, pb) = (pt(w[0]), pt(w[1]));
                let m = pt((w[0] + w[1]) / 2.0);
                let q = [m[0] + o[0], m[1] + o[1]];
                let Some((j, (_, _, ee))) = draussen
                    .iter()
                    .enumerate()
                    .find(|(j, s)| !(innen && *j == i) && drin(q, &s.0))
                else {
                    continue;
                };
                // im Dreieck je Paar einmal
                if innen && j < i {
                    continue;
                }
                let zi = [an(e, pa[0], pa[1]), an(e, pb[0], pb[1])];
                let zj = [an(ee, pa[0], pa[1]), an(ee, pb[0], pb[1])];
                wand(pa, pb, zi, zj, &mut out.waende);
            }
        }
    }
}

/// Wand über der Strecke `a`→`b` zwischen den Höhen `zi` (links, innen)
/// und `zj` (rechts) an ihren Enden; die Normale zeigt zur tieferen Seite.
/// Kreuzen sich die Höhen, zwei Dreiecke mit je eigener Seite.
fn wand(a: [f64; 2], b: [f64; 2], zi: [f64; 2], zj: [f64; 2], out: &mut Vec<[[f64; 3]; 3]>) {
    let (da, db) = (zi[0] - zj[0], zi[1] - zj[1]);
    if da.abs() < GLEICH && db.abs() < GLEICH {
        return;
    }
    let p = |q: [f64; 2], z: f64| [q[0], q[1], z];
    // links höher: Normale nach rechts (draußen)
    let dazu = |out: &mut Vec<[[f64; 3]; 3]>, d: [[f64; 3]; 3], rechts: bool| {
        // ohne Fläche (alle Ecken übereinander oder auf einer Linie) keins
        let u = [d[1][0] - d[0][0], d[1][1] - d[0][1], d[1][2] - d[0][2]];
        let v = [d[2][0] - d[0][0], d[2][1] - d[0][1], d[2][2] - d[0][2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        if n[0] * n[0] + n[1] * n[1] + n[2] * n[2] < LEER * LEER {
            return;
        }
        out.push(if rechts { d } else { [d[0], d[2], d[1]] });
    };
    if da * db >= 0.0 {
        let rechts = da + db > 0.0;
        let (lo, hi) = (
            [zi[0].min(zj[0]), zi[1].min(zj[1])],
            [zi[0].max(zj[0]), zi[1].max(zj[1])],
        );
        if hi[1] - lo[1] > GLEICH {
            dazu(out, [p(a, lo[0]), p(b, lo[1]), p(b, hi[1])], rechts);
        }
        if hi[0] - lo[0] > GLEICH {
            dazu(out, [p(a, lo[0]), p(b, hi[1]), p(a, hi[0])], rechts);
        }
        return;
    }
    // die Höhen kreuzen sich bei s
    let s = da / (da - db);
    let c = [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s];
    let zc = zi[0] + (zi[1] - zi[0]) * s;
    let (lo_a, hi_a) = (zi[0].min(zj[0]), zi[0].max(zj[0]));
    let (lo_b, hi_b) = (zi[1].min(zj[1]), zi[1].max(zj[1]));
    dazu(out, [p(a, lo_a), p(c, zc), p(a, hi_a)], da > 0.0);
    dazu(out, [p(c, zc), p(b, lo_b), p(b, hi_b)], db > 0.0);
}

/// Kreuzprodukt (b − a) × (c − a).
fn kreuz(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn treffen(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// Ebene `z = a + b·x + c·y` durch die drei Punkte; senkrecht stehend
/// (entartet) waagerecht in ihrer mittleren Höhe.
fn ebene_durch(d: &[[f64; 3]; 3]) -> Bezug {
    let u = [d[1][0] - d[0][0], d[1][1] - d[0][1], d[1][2] - d[0][2]];
    let v = [d[2][0] - d[0][0], d[2][1] - d[0][1], d[2][2] - d[0][2]];
    let det = u[0] * v[1] - u[1] * v[0];
    if det.abs() < 1e-18 {
        return [(d[0][2] + d[1][2] + d[2][2]) / 3.0, 0.0, 0.0];
    }
    let b = (u[2] * v[1] - v[2] * u[1]) / det;
    let c = (v[2] * u[0] - u[2] * v[0]) / det;
    [d[0][2] - b * d[0][0] - c * d[0][1], b, c]
}

/// Mitte eines konvexen Polygons (Mittel der Ecken; liegt innen).
fn mitte(r: &[[f64; 2]]) -> [f64; 2] {
    let s = r.iter().fold([0.0, 0.0], |s, q| [s[0] + q[0], s[1] + q[1]]);
    let n = r.len().max(1) as f64;
    [s[0] / n, s[1] / n]
}

/// Das Dreieck `d` des Urgeländes in konvexe Stücke mit je einer Ebene und
/// ihrer Herkunft, gegen den Uhrzeigersinn. Ein Stück wird an der Geraden
/// jeder Kante der Gebiete `ix` geteilt, die es kreuzt: Danach liegt jedes
/// Stück ganz in oder ganz außerhalb jedes Gebiets, auch eines nicht
/// konvexen, und ein Dreieck, das keine Kante kreuzt, bleibt ganz. So
/// entstehen je Dreieck etwa so viele Stücke, wie Kanten es kreuzen.
fn teilen(
    d: &[[f64; 3]; 3],
    gebiete: &[Gebiet],
    ix: &[usize],
) -> Vec<(Vec<[f64; 2]>, Herkunft, Bezug)> {
    let ur = ebene_durch(d);
    let mut ring: Vec<[f64; 2]> = d.iter().map(|q| [q[0], q[1]]).collect();
    if kreuz(ring[0], ring[1], ring[2]) < 0.0 {
        ring.reverse();
    }
    let rt = rahmen(&ring);
    let ix: Vec<&Gebiet> = ix
        .iter()
        .map(|&i| &gebiete[i])
        .filter(|g| treffen(&g.rahmen, &rt))
        .collect();
    let mut stuecke = vec![(rt, ring)];
    for g in &ix {
        let n = g.ring.len();
        for k in 0..n {
            let (a, b) = (g.ring[k], g.ring[(k + 1) % n]);
            let rk = rahmen(&[a, b]);
            if !treffen(&rk, &rt) {
                continue;
            }
            let quer = move |q: [f64; 2]| kreuz(a, b, q);
            let mut neu = Vec::with_capacity(stuecke.len() + 1);
            for (r, s) in stuecke {
                let kreuzt =
                    treffen(&rk, &r) && im_konvex(a, b, &s).is_some_and(|(t0, t1)| t1 - t0 > 1e-12);
                if !kreuzt {
                    neu.push((r, s));
                    continue;
                }
                let teile: Vec<_> = [halbieren(&s, quer), halbieren(&s, |q| -quer(q))]
                    .into_iter()
                    .filter(|x| flaeche(x) > LEER)
                    .collect();
                if teile.len() < 2 {
                    neu.push((r, s));
                } else {
                    neu.extend(teile.into_iter().map(|x| (rahmen(&x), x)));
                }
            }
            stuecke = neu;
        }
    }
    let mut out = Vec::new();
    for (_, s) in stuecke {
        let c = mitte(&s);
        let drin: Vec<&&Gebiet> = ix
            .iter()
            .filter(|g| treffen(&g.rahmen, &[c[0], c[1], c[0], c[1]]) && innen(g.ring, c[0], c[1]))
            .collect();
        // eine Fläche vor jeder Böschung, die zuletzt gesetzte zuerst
        if let Some(g) = drin
            .iter()
            .rev()
            .find(|g| matches!(g.herkunft, Herkunft::Flaeche(_)))
        {
            out.push((s, g.herkunft, g.ebene));
            continue;
        }
        let mut ebenen: Vec<(Bezug, Herkunft)> = vec![(ur, Herkunft::Ur)];
        ebenen.extend(drin.iter().map(|g| (g.ebene, g.herkunft)));
        // an jeder Linie, an der zwei Ebenen gleich hoch sind, teilen; dann
        // gilt je Stück eine Ebene
        let mut teile = vec![s];
        for i in 0..ebenen.len() {
            for j in i + 1..ebenen.len() {
                let (a, b) = (ebenen[i].0, ebenen[j].0);
                let f = move |q: [f64; 2]| an(&a, q[0], q[1]) - an(&b, q[0], q[1]);
                teile = teile
                    .into_iter()
                    .flat_map(|s| {
                        let w: Vec<f64> = s.iter().map(|&q| f(q)).collect();
                        if w.iter().all(|&x| x >= -GLEICH) || w.iter().all(|&x| x <= GLEICH) {
                            return vec![s];
                        }
                        [halbieren(&s, f), halbieren(&s, |q| -f(q))]
                            .into_iter()
                            .filter(|r| flaeche(r) > LEER)
                            .collect()
                    })
                    .collect();
            }
        }
        for s in teile {
            let c = mitte(&s);
            let z = |e: &Bezug| an(e, c[0], c[1]);
            // min(max(U, Auftrag), Abtrag)
            let mut best = (z(&ur), Herkunft::Ur, ur);
            for (e, h) in &ebenen[1..] {
                if matches!(h, Herkunft::Boeschung { auftrag: true, .. }) && z(e) > best.0 {
                    best = (z(e), *h, *e);
                }
            }
            for (e, h) in &ebenen[1..] {
                if matches!(h, Herkunft::Boeschung { auftrag: false, .. }) && z(e) < best.0 {
                    best = (z(e), *h, *e);
                }
            }
            out.push((s, best.1, best.2));
        }
    }
    out
}

/// Parameter der Strecke `a` + t·(`b` − `a`), t in [0, 1], im konvexen
/// Polygon `v` gegen den Uhrzeigersinn (Cyrus-Beck); `None` ohne Stück.
fn im_konvex(a: [f64; 2], b: [f64; 2], v: &[[f64; 2]]) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let r = [b[0] - a[0], b[1] - a[1]];
    for i in 0..v.len() {
        let (p, q) = (v[i], v[(i + 1) % v.len()]);
        let k = [q[0] - p[0], q[1] - p[1]];
        // links der Kante ist innen: k × (x − p) ≥ 0
        let f0 = k[0] * (a[1] - p[1]) - k[1] * (a[0] - p[0]);
        let df = k[0] * r[1] - k[1] * r[0];
        if df.abs() < 1e-15 {
            if f0 < 0.0 {
                return None;
            }
            continue;
        }
        let t = -f0 / df;
        if df > 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, t1))
}

/// [`im_konvex`] für ein Dreieck.
fn im_dreieck(a: [f64; 2], b: [f64; 2], d: &[[f64; 3]; 3]) -> Option<(f64, f64)> {
    im_konvex(a, b, &d.map(|q| [q[0], q[1]]))
}

/// Böschungsschraffen im Abstand `abstand` (m) längs des Rands jeder
/// Planung, abwechselnd lang und kurz, vom Kopf zum Fuß: beim Auftrag ab
/// der Kante der Fläche, beim Abtrag ab der Durchstoßlinie draußen. Der
/// Abstand läuft über die Kanten weiter, so bekommt auch ein Rand aus
/// vielen kurzen Kanten (Rundung) Schraffen im Abstand. Eine Schraffe
/// liegt nur, wo ihre Böschung im Planungsgelände `o` sichtbar ist (Stücke
/// mit ihrer Herkunft); eine kurze ist halb so lang wie die lange an ihrer
/// Stelle. Je Schraffe ein Stück je sichtbarem Abschnitt (m, auf der
/// Böschung) und ob sie lang ist.
pub fn schraffen(
    o: &Oberflaeche,
    planungen: &[&Planung],
    abstand: f64,
) -> Vec<([[f64; 3]; 2], bool)> {
    let mut out = Vec::new();
    if !(abstand.is_finite() && abstand > 0.0) {
        return out;
    }
    let mut je: HashMap<Herkunft, Vec<usize>> = HashMap::new();
    for (i, (_, h)) in o.dreiecke.iter().enumerate() {
        if matches!(h, Herkunft::Boeschung { .. }) {
            je.entry(*h).or_default().push(i);
        }
    }
    for (p, pl) in planungen.iter().enumerate() {
        for (auftrag, bereiche) in [(true, &pl.auftrag), (false, &pl.abtrag)] {
            // längs des Rands in der Folge der Kanten
            let mut folge: Vec<usize> = (0..bereiche.len()).collect();
            folge.sort_by_key(|&b| bereiche[b].kante);
            let mut s0 = 0.0;
            let mut k = 0usize;
            for b in folge {
                let x = &bereiche[b];
                let r = &x.ring;
                if r.len() < 3 {
                    continue;
                }
                let (a, e) = (r[0], r[1]);
                let l = (e[0] - a[0]).hypot(e[1] - a[1]);
                if l < 1e-9 {
                    continue;
                }
                let (von_s, bis_s) = (s0, s0 + l);
                s0 = bis_s;
                let ds = je.get(&Herkunft::Boeschung { p, auftrag, b });
                let t = [(e[0] - a[0]) / l, (e[1] - a[1]) / l];
                // quer zur Kante zur Seite des Bereichs
                let mut n = [t[1], -t[0]];
                let quer = |n: [f64; 2], q: [f64; 2]| (q[0] - a[0]) * n[0] + (q[1] - a[1]) * n[1];
                if r[2..].iter().map(|&q| quer(n, q)).sum::<f64>() < 0.0 {
                    n = [-n[0], -n[1]];
                }
                let weit = r.iter().map(|&q| quer(n, q)).fold(0.0, f64::max);
                while (k as f64 + 0.5) * abstand < bis_s && k < MAX_SCHRAFFEN {
                    let s = (k as f64 + 0.5) * abstand;
                    let lang = k.is_multiple_of(2);
                    k += 1;
                    let (Some(ds), true) = (ds, s >= von_s && weit > 0.0) else {
                        continue;
                    };
                    let fuss = [a[0] + t[0] * (s - von_s), a[1] + t[1] * (s - von_s)];
                    let ende = [fuss[0] + n[0] * weit, fuss[1] + n[1] * weit];
                    // sichtbare Abschnitte, zusammengefasst: Die Stücke der
                    // Böschung liegen alle in ihrer Ebene
                    let mut st: Vec<(f64, f64)> = ds
                        .iter()
                        .filter_map(|&i| im_dreieck(fuss, ende, &o.dreiecke[i].0))
                        .filter(|(t0, t1)| t1 - t0 > 1e-12)
                        .collect();
                    st.sort_by(|a, b| a.0.total_cmp(&b.0));
                    let mut ab: Vec<(f64, f64)> = Vec::new();
                    for (t0, t1) in st {
                        match ab.last_mut() {
                            Some(l) if t0 <= l.1 + 1e-9 => l.1 = l.1.max(t1),
                            _ => ab.push((t0, t1)),
                        }
                    }
                    let (Some(lo), Some(hi)) = (ab.first().map(|x| x.0), ab.last().map(|x| x.1))
                    else {
                        continue;
                    };
                    let (von, bis) = match (lang, auftrag) {
                        (true, _) => (lo, hi),
                        (false, true) => (lo, (lo + hi) / 2.0),
                        (false, false) => ((lo + hi) / 2.0, hi),
                    };
                    let punkt = |t: f64| {
                        let q = [
                            fuss[0] + (ende[0] - fuss[0]) * t,
                            fuss[1] + (ende[1] - fuss[1]) * t,
                        ];
                        [q[0], q[1], an(&x.ebene, q[0], q[1])]
                    };
                    for (t0, t1) in ab {
                        let (t0, t1) = (t0.max(von), t1.min(bis));
                        if t1 - t0 > 1e-12 {
                            out.push(([punkt(t0), punkt(t1)], lang));
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netz::{netz, Rand};
    use crate::planung::{Baustein, Kante, Teil};

    /// Urgelände über [−20, 40]² aus einem Gitter zu 2 m.
    fn gelaende(z: impl Fn(f64, f64) -> f64) -> Abfrage {
        let mut p = Vec::new();
        for i in 0..=30 {
            for j in 0..=30 {
                let (x, y) = (-20.0 + 2.0 * i as f64, -20.0 + 2.0 * j as f64);
                p.push([x, y, z(x, y)]);
            }
        }
        Abfrage::neu(&netz(&p, &[], Rand::Huelle))
    }

    fn baustein(ring: &[[f64; 2]], e: Bezug, n: f64) -> Baustein {
        let k = ring.len();
        Baustein {
            teile: vec![Teil {
                ring: ring.to_vec(),
                ebene: e,
            }],
            rand: (0..k)
                .map(|i| {
                    let (a, b) = (ring[i], ring[(i + 1) % k]);
                    Kante {
                        a: [a[0], a[1], an(&e, a[0], a[1])],
                        b: [b[0], b[1], an(&e, b[0], b[1])],
                        neigung: Some(n),
                    }
                })
                .collect(),
        }
    }

    fn rechteck(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    /// Höhe des Netzes aus Urgelände ohne die ersetzten Dreiecke und den
    /// neuen an (x, y).
    fn hoehe_im_netz(u: &Abfrage, o: &Oberflaeche, x: f64, y: f64) -> Option<f64> {
        let neu = o
            .dreiecke
            .iter()
            .find_map(|(d, _)| im_dreieck([x, y], [x, y], d).map(|_| an(&ebene_durch(d), x, y)));
        if neu.is_some() {
            return neu;
        }
        let t = u.dreieck(x, y)?;
        (o.ersetzt.binary_search(&(t as u32)).is_err()).then(|| u.hoehe(x, y).unwrap())
    }

    /// Das Netz gegen [`Planung::hoehe`] auf einem Raster prüfen.
    fn wie_hoehe(u: &Abfrage, pl: &[&Planung], o: &Oberflaeche) {
        for i in 0..=150 {
            for j in 0..=150 {
                // nicht genau auf Gitterlinien des Urgeländes
                let (x, y) = (
                    -12.0 + 0.2 * i as f64 + 0.013,
                    -12.0 + 0.2 * j as f64 + 0.007,
                );
                let soll = pl
                    .iter()
                    .rev()
                    .find_map(|p| {
                        p.teile
                            .iter()
                            .any(|t| super::super::innen(&t.ring, x, y))
                            .then(|| p.hoehe(u, x, y))
                    })
                    .flatten()
                    .unwrap_or_else(|| {
                        // außerhalb der Flächen: alle Böschungen zusammen
                        let mut z = u.hoehe(x, y).unwrap();
                        for p in pl {
                            for b in p
                                .auftrag
                                .iter()
                                .filter(|b| super::super::innen(&b.ring, x, y))
                            {
                                z = z.max(an(&b.ebene, x, y));
                            }
                        }
                        for p in pl {
                            for b in p
                                .abtrag
                                .iter()
                                .filter(|b| super::super::innen(&b.ring, x, y))
                            {
                                z = z.min(an(&b.ebene, x, y));
                            }
                        }
                        z
                    });
                let ist = hoehe_im_netz(u, o, x, y).unwrap();
                assert!((ist - soll).abs() < 1e-9, "({x}, {y}): {ist} statt {soll}");
            }
        }
    }

    /// Die neuen Dreiecke decken genau die ersetzten.
    fn deckt(u: &Abfrage, o: &Oberflaeche) {
        let alt: f64 = o
            .ersetzt
            .iter()
            .map(|&t| flaeche(&u.punkte_von(t).map(|q| [q[0], q[1]])))
            .sum();
        let neu: f64 = o
            .dreiecke
            .iter()
            .map(|(d, _)| flaeche(&d.map(|q| [q[0], q[1]])))
            .sum();
        assert!((alt - neu).abs() < 1e-6, "{alt} {neu}");
        for (d, _) in &o.dreiecke {
            let a2 = (d[1][0] - d[0][0]) * (d[2][1] - d[0][1])
                - (d[1][1] - d[0][1]) * (d[2][0] - d[0][0]);
            assert!(a2 > 0.0, "gegen den Uhrzeigersinn");
        }
    }

    #[test]
    fn auftrag_und_abtrag_wie_hoehe() {
        let u = gelaende(|x, y| 10.0 + 0.05 * x - 0.03 * y + 0.2 * (x / 3.0).sin());
        let auf = Planung::neu(
            &baustein(&rechteck(0.0, 0.0, 6.0, 5.0), [11.0, 0.0, 0.0], 1.5),
            &u,
        );
        let ab = Planung::neu(
            &baustein(&rechteck(9.0, 2.0, 14.0, 9.0), [9.2, 0.02, 0.0], 1.5),
            &u,
        );
        for p in [&auf, &ab] {
            let o = oberflaeche(&u, &[p]);
            assert!(!o.ersetzt.is_empty());
            deckt(&u, &o);
            wie_hoehe(&u, &[p], &o);
        }
        // Herkunft: in der Fläche die Fläche, an der Kante die Böschung
        let o = oberflaeche(&u, &[&auf]);
        let bei = |x: f64, y: f64| {
            o.dreiecke
                .iter()
                .find(|(d, _)| im_dreieck([x, y], [x, y], d).is_some())
                .map(|x| x.1)
        };
        assert_eq!(bei(3.0, 2.5), Some(Herkunft::Flaeche(0)));
        assert!(matches!(
            bei(3.0, -0.3),
            Some(Herkunft::Boeschung {
                p: 0,
                auftrag: true,
                ..
            })
        ));
    }

    #[test]
    fn l_form_und_rund() {
        // nicht konvex: geteilt an den Kanten, nicht in Ohren
        let u = gelaende(|x, y| 10.0 + 0.04 * x + 0.02 * y);
        let l = [
            [0.0, 0.0],
            [9.0, 0.0],
            [9.0, 3.0],
            [4.0, 3.0],
            [4.0, 8.0],
            [0.0, 8.0],
        ];
        let p = Planung::neu(&baustein(&l, [10.6, 0.0, 0.0], 1.5), &u);
        let o = oberflaeche(&u, &[&p]);
        deckt(&u, &o);
        wie_hoehe(&u, &[&p], &o);
        // rund mit 400 Kanten: Schraffen im Abstand über die Kanten weg
        let kreis: Vec<[f64; 2]> = (0..400)
            .map(|i| {
                let w = i as f64 / 400.0 * std::f64::consts::TAU;
                [5.0 + 4.0 * w.cos(), 5.0 + 4.0 * w.sin()]
            })
            .collect();
        let p = Planung::neu(&baustein(&kreis, [11.0, 0.0, 0.0], 1.5), &u);
        let o = oberflaeche(&u, &[&p]);
        deckt(&u, &o);
        wie_hoehe(&u, &[&p], &o);
        let s = schraffen(&o, &[&p], 0.5);
        let lang = s.iter().filter(|x| x.1).count();
        let umfang = 400.0 * 2.0 * 4.0 * (std::f64::consts::PI / 400.0).sin();
        assert_eq!(s.len(), (umfang / 0.5).round() as usize);
        assert_eq!(lang, s.len().div_ceil(2));
    }

    /// Fläche ohne Böschung: an ihrem Rand eine Wand so hoch wie der
    /// Sprung, zur tieferen Seite gewandt; mit Böschung keine (Review #183
    /// b).
    #[test]
    fn senkrechter_rand_mit_wand() {
        let u = gelaende(|_, _| 10.0);
        let flaeche3 = |d: &[[f64; 3]; 3]| {
            let (a, b) = (
                [d[1][0] - d[0][0], d[1][1] - d[0][1], d[1][2] - d[0][2]],
                [d[2][0] - d[0][0], d[2][1] - d[0][1], d[2][2] - d[0][2]],
            );
            let n = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            (n, 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt())
        };
        // L-Form, 1 m hoch bzw. 0,5 m tief, Umfang 2 × (7 + 5) = 24 m
        let ring = [
            [0.1, 0.1],
            [7.1, 0.1],
            [7.1, 2.1],
            [3.1, 2.1],
            [3.1, 5.1],
            [0.1, 5.1],
        ];
        for (z, aussen) in [(11.0, true), (9.5, false)] {
            let mut b = baustein(&ring, [z, 0.0, 0.0], 1.5);
            for k in &mut b.rand {
                k.neigung = None;
            }
            let p = Planung::neu(&b, &u);
            let o = oberflaeche(&u, &[&p]);
            deckt(&u, &o);
            let flaeche: f64 = o.waende.iter().map(|d| flaeche3(d).1).sum();
            let soll = 24.0 * (z - 10.0_f64).abs();
            assert!((flaeche - soll).abs() < 1e-6, "{flaeche} {soll}");
            // die Normale zeigt zur tieferen Seite: beim Auftrag nach
            // außen, beim Abtrag nach innen
            for d in &o.waende {
                let (n, _) = flaeche3(d);
                assert!(n[2].abs() < 1e-9);
                let m = [
                    (d[0][0] + d[1][0] + d[2][0]) / 3.0 + n[0] * 1e-3,
                    (d[0][1] + d[1][1] + d[2][1]) / 3.0 + n[1] * 1e-3,
                ];
                assert_eq!(!innen(&ring, m[0], m[1]), aussen, "{d:?}");
            }
        }
        // mit Böschung schließt das Netz ohne Wand
        let p = Planung::neu(&baustein(&ring, [11.0, 0.0, 0.0], 1.5), &u);
        let o = oberflaeche(&u, &[&p]);
        let flaeche: f64 = o.waende.iter().map(|d| flaeche3(d).1).sum();
        assert!(flaeche < 1e-6, "{flaeche}");
    }

    /// Senkrechte Kante auf einer Kante des Urgeländes (y = 2 und y = 6 auf
    /// dem Gitter): draußen die Böschung der Nachbarkante bis zu ihrem Fuß,
    /// dann das Urgelände; die Wand folgt dem, auch wo das Gebiet das
    /// Dreieck der Fläche nur am Rand berührt (Geometrie #183).
    #[test]
    fn wand_auf_kante_des_urgelaendes() {
        let u = gelaende(|_, _| 10.0);
        let ring = [
            [0.0, 0.0],
            [8.0, 0.0],
            [8.0, 2.0],
            [4.0, 2.0],
            [4.0, 6.0],
            [0.0, 6.0],
        ];
        // Kante 2 (y = 2) senkrecht, die Böschung von Kante 3 reicht über sie
        let mut b = baustein(&ring, [11.0, 0.0, 0.0], 1.5);
        b.rand[2].neigung = None;
        let p = Planung::neu(&b, &u);
        let o = oberflaeche(&u, &[&p]);
        let hoch = |x: f64| {
            // Unterkante der Wände über (x, 2): das Gelände draußen
            let mut z = f64::MAX;
            for d in o
                .waende
                .iter()
                .filter(|d| d.iter().all(|q| (q[1] - 2.0).abs() < 1e-12))
            {
                for k in 0..3 {
                    let (p0, p1) = (d[k], d[(k + 1) % 3]);
                    if (p0[0] - x) * (p1[0] - x) < 0.0 {
                        z = z.min(p0[2] + (p1[2] - p0[2]) * (x - p0[0]) / (p1[0] - p0[0]));
                    }
                }
            }
            z
        };
        // am Fuß der Böschung von Kante 3 (x = 5,5) und dahinter auf 10
        assert!(
            (hoch(4.5) - (11.0 - 0.5 / 1.5)).abs() < 1e-9,
            "{}",
            hoch(4.5)
        );
        for x in [5.8, 6.3] {
            assert!((hoch(x) - 10.0).abs() < 1e-9, "{x}: {}", hoch(x));
        }
        // Kante 4 (y = 6) mit Böschung, Kante 3 senkrecht: an y = 6 keine Wand
        let mut b = baustein(&ring, [10.0, 0.2, -0.1], 1.5);
        b.rand[3].neigung = None;
        let p = Planung::neu(&b, &u);
        let o = oberflaeche(&u, &[&p]);
        assert!(o
            .waende
            .iter()
            .all(|d| !d.iter().all(|q| (q[1] - 6.0).abs() < 1e-12)));
    }

    #[test]
    fn kanten_in_einer_flucht() {
        // Zwischenpunkte: die Böschungen der Kanten einer Flucht haben
        // dieselbe Ebene, auf Hügeln mit Gefälle der Fläche
        let u =
            gelaende(|x, y| 10.0 + 0.05 * x - 0.03 * y + 0.4 * (x / 3.0).sin() * (y / 4.0).cos());
        let ring = [
            [0.0, 0.0],
            [5.0, 0.0],
            [10.0, 0.0],
            [10.0, 8.0],
            [6.5, 8.0],
            [6.5, 2.0],
            [5.8, 2.0],
            [5.8, 8.0],
            [5.799, 8.0],
            [0.0, 8.0],
        ];
        let p = Planung::neu(&baustein(&ring, [10.0, 0.2, 0.05], 1.5), &u);
        let o = oberflaeche(&u, &[&p]);
        deckt(&u, &o);
        wie_hoehe(&u, &[&p], &o);
    }

    #[test]
    fn zwei_flaechen_die_letzte_gilt() {
        let u = gelaende(|_, _| 10.0);
        let a = Planung::neu(
            &baustein(&rechteck(0.0, 0.0, 8.0, 8.0), [11.0, 0.0, 0.0], 1.5),
            &u,
        );
        let b = Planung::neu(
            &baustein(&rechteck(5.0, 5.0, 12.0, 12.0), [9.0, 0.0, 0.0], 2.0),
            &u,
        );
        let o = oberflaeche(&u, &[&a, &b]);
        deckt(&u, &o);
        wie_hoehe(&u, &[&a, &b], &o);
        assert_eq!(hoehe_im_netz(&u, &o, 6.0, 6.0), Some(9.0));
        assert_eq!(hoehe_im_netz(&u, &o, 2.0, 2.0), Some(11.0));
        let o = oberflaeche(&u, &[&b, &a]);
        assert_eq!(hoehe_im_netz(&u, &o, 6.0, 6.0), Some(11.0));
    }

    /// Das Netz ist dicht: Wo die Höhe an einer Kante der neuen Dreiecke
    /// springt, decken die Wände dort den Sprung von unten bis oben.
    /// Liefert die Zahl der Sprünge.
    fn dicht(u: &Abfrage, o: &Oberflaeche) -> usize {
        let mut spruenge = 0;
        for (d, _) in &o.dreiecke {
            for k in 0..3 {
                let (a, b) = (d[k], d[(k + 1) % 3]);
                let r = [b[0] - a[0], b[1] - a[1]];
                let l = r[0].hypot(r[1]);
                if l < 1e-6 {
                    continue;
                }
                let n = [r[1] / l * 1e-6, -r[0] / l * 1e-6];
                for s in [0.25, 0.5, 0.75] {
                    let q = [a[0] + r[0] * s, a[1] + r[1] * s];
                    let (Some(zl), Some(zr)) = (
                        hoehe_im_netz(u, o, q[0] - n[0], q[1] - n[1]),
                        hoehe_im_netz(u, o, q[0] + n[0], q[1] + n[1]),
                    ) else {
                        continue;
                    };
                    let (lo, hi) = (zl.min(zr), zl.max(zr));
                    if hi - lo < 1e-5 {
                        continue;
                    }
                    spruenge += 1;
                    // senkrechte Schnitte der Wände über q
                    let mut stuecke: Vec<(f64, f64)> = Vec::new();
                    for w in &o.waende {
                        let mut z = Vec::new();
                        for j in 0..3 {
                            let (p0, p1) = (w[j], w[(j + 1) % 3]);
                            let e = [p1[0] - p0[0], p1[1] - p0[1]];
                            let ll = e[0] * e[0] + e[1] * e[1];
                            if ll < 1e-18 {
                                continue;
                            }
                            let t = ((q[0] - p0[0]) * e[0] + (q[1] - p0[1]) * e[1]) / ll;
                            let quer = ((q[0] - p0[0]) * e[1] - (q[1] - p0[1]) * e[0]) / ll.sqrt();
                            if quer.abs() < 1e-7 && (-1e-9..=1.0 + 1e-9).contains(&t) {
                                z.push(p0[2] + (p1[2] - p0[2]) * t);
                            }
                        }
                        if z.len() >= 2 {
                            let lo = z.iter().copied().fold(f64::MAX, f64::min);
                            let hi = z.iter().copied().fold(f64::MIN, f64::max);
                            stuecke.push((lo, hi));
                        }
                    }
                    stuecke.sort_by(|x, y| x.0.total_cmp(&y.0));
                    let mut bis = lo;
                    for (a, b) in stuecke {
                        if a <= bis + 1e-5 {
                            bis = bis.max(b);
                        }
                    }
                    // Toleranz: die Höhen links und rechts sind 1e-6 m neben
                    // der Kante genommen, bei Böschungen bis 1:0,5
                    assert!(
                        bis >= hi - 1e-5,
                        "offen bei {q:?}: {lo} bis {hi}, Wand bis {bis}"
                    );
                }
            }
        }
        spruenge
    }

    /// Böschung auf Böschung (Review #183 B2): B tief in der
    /// Auftragsböschung von A, beide Reihenfolgen; dazu Zufallspaare. Das
    /// Netz bleibt wie [`Planung::hoehe`] und ist dicht.
    #[test]
    fn boeschung_auf_boeschung_dicht() {
        let u = gelaende(|x, y| 10.0 + 0.04 * x - 0.02 * y + 0.3 * (x / 4.0).sin());
        let a = Planung::neu(
            &baustein(&rechteck(0.0, 0.0, 8.0, 8.0), [12.0, 0.0, 0.0], 1.5),
            &u,
        );
        let b = Planung::neu(
            &baustein(&rechteck(9.0, 2.0, 12.0, 6.0), [8.0, 0.0, 0.0], 1.0),
            &u,
        );
        let mut spruenge = 0;
        for pl in [[&a, &b], [&b, &a]] {
            let o = oberflaeche(&u, &pl);
            deckt(&u, &o);
            wie_hoehe(&u, &pl, &o);
            spruenge += dicht(&u, &o);
        }
        assert!(spruenge > 0, "der Fall hat Sprünge");
        let mut s = 5u64;
        let mut zufall = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        for _ in 0..12 {
            let mut pl = Vec::new();
            for _ in 0..2 {
                let (x, y) = (-8.0 + 16.0 * zufall(), -8.0 + 16.0 * zufall());
                let (w, h) = (2.0 + 6.0 * zufall(), 2.0 + 6.0 * zufall());
                let z = 10.0 + 4.0 * (zufall() - 0.5);
                let n = 0.5 + 2.0 * zufall();
                pl.push(Planung::neu(
                    &baustein(&rechteck(x, y, x + w, y + h), [z, 0.0, 0.0], n),
                    &u,
                ));
            }
            let refs: Vec<&Planung> = pl.iter().collect();
            let o = oberflaeche(&u, &refs);
            deckt(&u, &o);
            dicht(&u, &o);
        }
    }

    /// Über den Rand des Netzes: Die Kanten der Stücke dort und die
    /// Randkanten der übrigen Dreiecke ergeben den Rand des Netzes.
    #[test]
    fn rand_ueber_den_netzrand() {
        let u = gelaende(|x, _| 10.0 + 0.02 * x);
        let p = Planung::neu(
            &baustein(&rechteck(30.0, 0.0, 44.0, 8.0), [11.5, 0.0, 0.0], 1.5),
            &u,
        );
        let o = oberflaeche(&u, &[&p]);
        assert!(!o.rand.is_empty());
        let lang = |k: &[[f64; 3]; 2]| (k[1][0] - k[0][0]).hypot(k[1][1] - k[0][1]);
        let weg: f64 = u
            .randkanten()
            .filter(|(t, _)| o.ersetzt.binary_search(t).is_ok())
            .map(|(_, k)| lang(&k))
            .sum();
        let neu: f64 = o.rand.iter().map(lang).sum();
        assert!((weg - neu).abs() < 1e-9, "{weg} {neu}");
        // auf der Höhe des Planungsgeländes: im Auftrag über dem Urgelände
        for k in &o.rand {
            for q in k {
                assert!(q[2] >= u.hoehe(q[0], q[1]).unwrap() - 1e-9);
            }
        }
        assert!(o.rand.iter().flatten().any(|q| (q[2] - 11.5).abs() < 1e-9));
    }

    #[test]
    fn ohne_aenderung_nichts_ersetzt() {
        // die Fläche liegt genau auf dem Gelände: kein Dreieck ändert sich
        let u = gelaende(|_, _| 10.0);
        let p = Planung::neu(
            &baustein(&rechteck(0.0, 0.0, 6.0, 6.0), [10.0, 0.0, 0.0], 1.5),
            &u,
        );
        let o = oberflaeche(&u, &[&p]);
        assert!(
            o.ersetzt.is_empty()
                || o.dreiecke
                    .iter()
                    .all(|(d, _)| d.iter().all(|q| (q[2] - 10.0).abs() < 1e-12))
        );
        assert_eq!(oberflaeche(&u, &[]), Oberflaeche::default());
    }

    #[test]
    fn schraffen_vom_kopf_zum_fuss() {
        // 1 m Auftrag auf ebenem Gelände, 1:1,5: lange Schraffen 1,5 m,
        // kurze 0,75 m, ab der Kante der Fläche
        let u = gelaende(|_, _| 10.0);
        let p = Planung::neu(
            &baustein(&rechteck(0.0, 0.0, 10.0, 10.0), [11.0, 0.0, 0.0], 1.5),
            &u,
        );
        let o = oberflaeche(&u, &[&p]);
        let s = schraffen(&o, &[&p], 1.0);
        let unten: Vec<_> = s
            .iter()
            .filter(|(q, _)| q[0][1] <= 0.0 && q[1][1] <= 0.0 && q[0][0] > 0.0 && q[0][0] < 10.0)
            .collect();
        // je Schraffe Stücke in mehreren Dreiecken: Längen je x summieren
        let mut je_x: HashMap<u64, (f64, bool, f64)> = HashMap::new();
        for (q, lang) in &unten {
            let l = (q[1][0] - q[0][0]).hypot(q[1][1] - q[0][1]);
            let e = je_x.entry(q[0][0].to_bits()).or_insert((0.0, *lang, 0.0));
            e.0 += l;
            e.2 = e.2.max(q[0][1].max(q[1][1]));
        }
        assert_eq!(je_x.len(), 10, "{je_x:?}");
        for (l, lang, oben) in je_x.values() {
            let soll = if *lang { 1.5 } else { 0.75 };
            assert!((l - soll).abs() < 1e-9, "{l} {lang}");
            assert!(oben.abs() < 1e-9, "ab der Kante");
        }
        // Höhen auf der Böschung: oben 11, am Fuß 10
        for (q, _) in &s {
            for p in q {
                assert!((10.0 - 1e-9..=11.0 + 1e-9).contains(&p[2]));
            }
        }
        // Abtrag: kurze Schraffen am Kopf draußen
        let p = Planung::neu(
            &baustein(&rechteck(0.0, 0.0, 10.0, 10.0), [9.0, 0.0, 0.0], 1.5),
            &u,
        );
        let o = oberflaeche(&u, &[&p]);
        let s = schraffen(&o, &[&p], 1.0);
        let kurz = s
            .iter()
            .filter(|(q, lang)| !lang && q[0][1] < 0.0 && q[0][0] > 0.0 && q[0][0] < 10.0)
            .map(|(q, _)| q[0][1].min(q[1][1]))
            .fold(0.0, f64::min);
        assert!((kurz + 1.5).abs() < 1e-9, "{kurz}");
        assert!(schraffen(&o, &[&p], 0.0).is_empty());
    }
}

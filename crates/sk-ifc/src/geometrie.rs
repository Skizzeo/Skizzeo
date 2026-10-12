//! Form der Bauteile für IFC: Dreiecksnetze je Baustoff
//! (`IfcTriangulatedFaceSet`, Reference View „Tessellation“) aus denselben
//! Körpern wie die Ansicht, Punkte je Netz zusammengeführt.

use sk_math::{polygon, vec3, Vec3};
use sk_model::{material, Solid};
use std::collections::HashMap;

/// Ein Netz: Punkte und Dreiecke (Nummern ab 0), alles aus Dreiecken
/// desselben Baustoffs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Netz {
    /// Darstellungsschlüssel des Baustoffs (ohne Schnittbit).
    pub mat: u16,
    pub punkte: Vec<Vec3>,
    pub dreiecke: Vec<[u32; 3]>,
}

/// Punkte gelten als gleich, wenn sie in jeder Richtung höchstens 0,001 mm
/// auseinanderliegen. Gesucht wird in der eigenen Zelle und den Nachbarzellen,
/// damit zwei Punkte, die nur in der letzten Stelle abweichen, nicht an einer
/// Rundungsgrenze getrennt werden. Verglichen wird mit dem zuerst gesehenen
/// Punkt; zwei Eingaben auf seinen beiden Seiten können also bis zur doppelten
/// Toleranz auseinanderliegen, je nach Reihenfolge.
const SCHWEISS: f64 = 1e-3;

fn zelle(p: Vec3) -> [i64; 3] {
    [p.x, p.y, p.z].map(|v| (v / SCHWEISS).floor() as i64)
}

/// Nummer des Punkts `q` im Netz, neu angelegt, wenn keiner nahe genug ist.
fn punkt(netz: &mut Netz, idx: &mut HashMap<[i64; 3], Vec<u32>>, q: Vec3) -> u32 {
    let z = zelle(q);
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                let Some(v) = idx.get(&[z[0] + dx, z[1] + dy, z[2] + dz]) else {
                    continue;
                };
                for &i in v {
                    let d = netz.punkte[i as usize] - q;
                    if d.x.abs() <= SCHWEISS && d.y.abs() <= SCHWEISS && d.z.abs() <= SCHWEISS {
                        return i;
                    }
                }
            }
        }
    }
    netz.punkte.push(q);
    let i = (netz.punkte.len() - 1) as u32;
    idx.entry(z).or_default().push(i);
    i
}

/// Die Netze eines Körpers je Baustoff, um `versatz` verschoben (Lage im
/// Geschoss). Dreiecke ohne Fläche fallen weg; jedes Dreieck läuft gegen den
/// Uhrzeigersinn um seine Normale (IFC: Außenseite).
pub fn netze(s: &Solid, versatz: Vec3) -> Vec<Netz> {
    let mut out: Vec<Netz> = Vec::new();
    let mut idx: Vec<HashMap<[i64; 3], Vec<u32>>> = Vec::new();
    for t in &s.triangles {
        let mat = t.mat & !material::CUT;
        let k = match out.iter().position(|n| n.mat == mat) {
            Some(k) => k,
            None => {
                out.push(Netz {
                    mat,
                    ..Netz::default()
                });
                idx.push(HashMap::new());
                out.len() - 1
            }
        };
        let p = t.p.map(|p| p - versatz);
        let n = (p[1] - p[0]).cross(p[2] - p[0]);
        if n.length() < 1e-6 {
            continue;
        }
        let mut ids = [0u32; 3];
        for (j, q) in p.iter().enumerate() {
            ids[j] = punkt(&mut out[k], &mut idx[k], *q);
        }
        if ids[0] == ids[1] || ids[1] == ids[2] || ids[0] == ids[2] {
            continue;
        }
        if n.dot(t.n) < 0.0 {
            ids.swap(1, 2);
        }
        out[k].dreiecke.push(ids);
    }
    out.retain(|n| !n.dreiecke.is_empty());
    for n in &mut out {
        if !geschlossen(n) {
            schliessen(n);
        }
    }
    out
}

/// Schließt die Löcher eines Netzes, die die Ansicht auslässt (Stoßflächen
/// zwischen Segmenten und Schichten, die man nie sieht): jede Randschleife,
/// die eben ist, bekommt eine Deckfläche. Schleifen, die nicht eben sind oder
/// sich nicht eindeutig verfolgen lassen, bleiben offen.
pub fn schliessen(n: &mut Netz) {
    let mut kanten: HashMap<(u32, u32), u32> = HashMap::new();
    for t in &n.dreiecke {
        for k in 0..3 {
            *kanten.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    // Zwei Abschnitte übereinander teilen sich eine Fläche, die nur einer von
    // ihnen trägt (z. B. Schale unter und neben dem Deckenband): sie liegt im
    // Inneren des gemeinsamen Körpers. Eine ebene Fläche (zusammenhängende
    // Dreiecke einer Ebene), deren Randkanten alle doppelt gleichsinnig
    // laufen, fällt weg.
    let doppelt = |t: &[u32; 3], kanten: &HashMap<(u32, u32), u32>| {
        (0..3).any(|k| kanten.get(&(t[k], t[(k + 1) % 3])).copied().unwrap_or(0) >= 2)
    };
    let kand: Vec<usize> = (0..n.dreiecke.len())
        .filter(|&i| doppelt(&n.dreiecke[i], &kanten))
        .collect();
    if !kand.is_empty() {
        let normale = |t: &[u32; 3]| {
            let [a, b, c] = t.map(|i| n.punkte[i as usize]);
            let v = (b - a).cross(c - a);
            v * (1.0 / v.length().max(1e-12))
        };
        // Flächen: Kandidaten über gegenläufige gemeinsame Kanten in einer Ebene
        let mut flaeche: Vec<usize> = (0..kand.len()).collect();
        fn wurzel(f: &mut [usize], i: usize) -> usize {
            let mut r = i;
            while f[r] != r {
                r = f[r];
            }
            f[i] = r;
            r
        }
        let mut an: HashMap<(u32, u32), usize> = HashMap::new();
        for (k, &ti) in kand.iter().enumerate() {
            let t = n.dreiecke[ti];
            for j in 0..3 {
                an.insert((t[j], t[(j + 1) % 3]), k);
            }
        }
        for (k, &ti) in kand.iter().enumerate() {
            let t = n.dreiecke[ti];
            for j in 0..3 {
                if let Some(&o) = an.get(&(t[(j + 1) % 3], t[j])) {
                    if normale(&t).dot(normale(&n.dreiecke[kand[o]])) > 0.9999 {
                        let (a, b) = (wurzel(&mut flaeche, k), wurzel(&mut flaeche, o));
                        flaeche[a] = b;
                    }
                }
            }
        }
        let mut gruppen: HashMap<usize, Vec<usize>> = HashMap::new();
        for (k, &ti) in kand.iter().enumerate() {
            let w = wurzel(&mut flaeche, k);
            gruppen.entry(w).or_default().push(ti);
        }
        let mut weg = vec![false; n.dreiecke.len()];
        let mut gs: Vec<Vec<usize>> = gruppen.into_values().collect();
        gs.sort();
        for g in gs {
            let eigen: std::collections::HashSet<(u32, u32)> = g
                .iter()
                .flat_map(|&ti| {
                    let t = n.dreiecke[ti];
                    (0..3).map(move |j| (t[j], t[(j + 1) % 3]))
                })
                .collect();
            let rand: Vec<(u32, u32)> = eigen
                .iter()
                .filter(|(a, b)| !eigen.contains(&(*b, *a)))
                .copied()
                .collect();
            if !rand.is_empty()
                && rand
                    .iter()
                    .all(|k| kanten.get(k).copied().unwrap_or(0) >= 2)
            {
                for &ti in &g {
                    weg[ti] = true;
                    let t = n.dreiecke[ti];
                    for j in 0..3 {
                        if let Some(c) = kanten.get_mut(&(t[j], t[(j + 1) % 3])) {
                            *c -= 1;
                        }
                    }
                }
            }
        }
        let mut k = 0;
        n.dreiecke.retain(|_| {
            k += 1;
            !weg[k - 1]
        });
    }
    kanten.retain(|_, c| *c > 0);
    if t_stoesse(n) {
        kanten.clear();
        for t in &n.dreiecke {
            for k in 0..3 {
                *kanten.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
            }
        }
    }
    // Randkanten: ohne Gegenkante; je Anfangspunkt genau eine, sonst
    // mehrdeutig
    let mut weiter: HashMap<u32, Option<u32>> = HashMap::new();
    for (&(a, b), &c) in &kanten {
        if c == 1 && !kanten.contains_key(&(b, a)) {
            weiter.entry(a).and_modify(|x| *x = None).or_insert(Some(b));
        }
    }
    let mut starts: Vec<u32> = weiter.keys().copied().collect();
    starts.sort_unstable();
    let mut used: std::collections::HashSet<u32> = Default::default();
    for s0 in starts {
        if used.contains(&s0) {
            continue;
        }
        let mut loop_ = vec![s0];
        let mut cur = s0;
        let ok = loop {
            let Some(Some(nx)) = weiter.get(&cur).copied() else {
                break false;
            };
            if nx == s0 {
                break true;
            }
            if loop_.contains(&nx) || loop_.len() > 4096 {
                break false;
            }
            loop_.push(nx);
            cur = nx;
        };
        used.extend(loop_.iter().copied());
        if !ok || loop_.len() < 3 {
            continue;
        }
        // Deckfläche läuft gegen die Schleife
        loop_.reverse();
        let pts: Vec<Vec3> = loop_.iter().map(|&i| n.punkte[i as usize]).collect();
        let normal = newell(&pts);
        if normal.length() < 1e-9 {
            continue;
        }
        let nn = normal * (1.0 / normal.length());
        let eben = pts.iter().all(|p| (*p - pts[0]).dot(nn).abs() < 0.05);
        if !eben {
            continue;
        }
        // In der Ebene zerlegen
        let u = if nn.z.abs() < 0.9 {
            vec3(-nn.y, nn.x, 0.0)
        } else {
            vec3(1.0, 0.0, 0.0)
        };
        let u = u * (1.0 / u.length());
        let v = nn.cross(u);
        // Ecken ohne Punkte, an denen die Schleife gerade weiterläuft (sie
        // kommen danach als Fächer zurück, damit die Kanten zu den
        // Nachbarflächen passen)
        let m = pts.len();
        let ecken: Vec<usize> = (0..m)
            .filter(|&i| {
                let (a, b, c) = (pts[(i + m - 1) % m], pts[i], pts[(i + 1) % m]);
                (b - a).cross(c - b).length() > 1e-6 * (b - a).length() * (c - b).length()
            })
            .collect();
        if ecken.len() < 3 {
            continue;
        }
        let flach: Vec<Vec3> = ecken
            .iter()
            .map(|&i| {
                let p = pts[i] - pts[0];
                vec3(p.dot(u), p.dot(v), 0.0)
            })
            .collect();
        // Punkte zwischen zwei Ecken (in Schleifenrichtung)
        let zwischen = |a: usize, b: usize| -> Vec<u32> {
            let mut out = Vec::new();
            let mut i = (a + 1) % m;
            while i != b {
                out.push(loop_[i]);
                i = (i + 1) % m;
            }
            out
        };
        for t in polygon::triangulate(&flach) {
            let mut e3 = t.map(|k| ecken[k]);
            let [a, b, c] = e3.map(|i| pts[i]);
            if (b - a).cross(c - a).dot(nn) < 0.0 {
                e3.swap(1, 2);
            }
            // Kanten, die zwei aufeinanderfolgende Ecken verbinden, über die
            // Zwischenpunkte führen
            let mut ring: Vec<u32> = Vec::with_capacity(3);
            for k in 0..3 {
                let (p, q) = (e3[k], e3[(k + 1) % 3]);
                ring.push(loop_[p]);
                let folgt = ecken
                    .iter()
                    .position(|&x| x == p)
                    .is_some_and(|i| ecken[(i + 1) % ecken.len()] == q);
                if folgt {
                    ring.extend(zwischen(p, q));
                }
            }
            // Fächer um eine Ecke, die nicht auf einer geraden Strecke liegt
            let r = ring.len();
            let start = (0..r)
                .find(|&k| {
                    e3.iter().any(|&x| loop_[x] == ring[k]) && {
                        let (a, b, c) = (
                            n.punkte[ring[(k + r - 1) % r] as usize],
                            n.punkte[ring[k] as usize],
                            n.punkte[ring[(k + 1) % r] as usize],
                        );
                        (b - a).cross(c - b).length() > 1e-9
                    }
                })
                .unwrap_or(0);
            for k in 1..r - 1 {
                n.dreiecke.push([
                    ring[start],
                    ring[(start + k) % r],
                    ring[(start + k + 1) % r],
                ]);
            }
        }
    }
}

/// Löst T-Stöße am Rand auf: liegt ein Randpunkt auf einer Randkante eines
/// anderen Dreiecks (eine Fläche reicht über zwei Abschnitte der
/// Nachbarfläche), wird das Dreieck dort geteilt. `true`, wenn etwas geteilt
/// wurde.
fn t_stoesse(n: &mut Netz) -> bool {
    let mut geteilt = false;
    for _ in 0..256 {
        let mut kanten: HashMap<(u32, u32), usize> = HashMap::new();
        for (i, t) in n.dreiecke.iter().enumerate() {
            for k in 0..3 {
                kanten.insert((t[k], t[(k + 1) % 3]), i);
            }
        }
        let rand: Vec<((u32, u32), usize)> = kanten
            .iter()
            .filter(|((a, b), _)| !kanten.contains_key(&(*b, *a)))
            .map(|(k, i)| (*k, *i))
            .collect();
        let mut punkte: Vec<u32> = rand.iter().flat_map(|((a, b), _)| [*a, *b]).collect();
        punkte.sort_unstable();
        punkte.dedup();
        let mut treffer = None;
        let mut rand = rand;
        rand.sort_unstable();
        'suche: for ((a, b), ti) in rand {
            let (pa, pb) = (n.punkte[a as usize], n.punkte[b as usize]);
            let d = pb - pa;
            let l2 = d.dot(d);
            if l2 < 1e-12 {
                continue;
            }
            for &c in &punkte {
                if c == a || c == b {
                    continue;
                }
                let pc = n.punkte[c as usize];
                let t = (pc - pa).dot(d) / l2;
                if t <= 1e-9 || t >= 1.0 - 1e-9 {
                    continue;
                }
                let lot = pa + d * t - pc;
                if lot.dot(lot) < 1e-4 {
                    treffer = Some((a, b, c, ti));
                    break 'suche;
                }
            }
        }
        let Some((a, b, c, ti)) = treffer else {
            return geteilt;
        };
        let t = n.dreiecke[ti];
        let k = (0..3)
            .find(|&k| t[k] == a && t[(k + 1) % 3] == b)
            .unwrap_or(0);
        let x = t[(k + 2) % 3];
        n.dreiecke[ti] = [a, c, x];
        n.dreiecke.push([c, b, x]);
        geteilt = true;
    }
    geteilt
}

/// Flächennormale eines (fast) ebenen Vielecks nach Newell, Länge = doppelte
/// Fläche.
fn newell(p: &[Vec3]) -> Vec3 {
    let mut n = Vec3::default();
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    n
}

/// Umschließender Quader aller Netze.
pub fn grenzen(netze: &[Netz]) -> Option<(Vec3, Vec3)> {
    let mut it = netze.iter().flat_map(|n| n.punkte.iter().copied());
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| {
        (
            vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
            vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
        )
    }))
}

/// Rauminhalt eines geschlossenen Netzes (Divergenzsatz), mm³. Für offene
/// Netze ohne Bedeutung.
pub fn volumen(n: &Netz) -> f64 {
    n.dreiecke
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| n.punkte[i as usize]);
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

/// Ist das Netz geschlossen: jede Kante genau zweimal, gegenläufig?
pub fn geschlossen(n: &Netz) -> bool {
    let mut kanten: HashMap<(u32, u32), i32> = HashMap::new();
    for t in &n.dreiecke {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let (key, d) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
            *kanten.entry(key).or_default() += d;
        }
    }
    !kanten.is_empty() && kanten.values().all(|&v| v == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wuerfel() -> Solid {
        let mut s = Solid::default();
        let p = |x: f64, y: f64, z: f64| vec3(x, y, z) * 1000.0;
        let (a, b, c, d) = (p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.));
        let (e, f, g, h) = (p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.));
        s.mat = 3;
        s.quad(a, d, c, b, vec3(0., 0., -1.));
        s.quad(e, f, g, h, vec3(0., 0., 1.));
        s.quad(a, b, f, e, vec3(0., -1., 0.));
        s.quad(b, c, g, f, vec3(1., 0., 0.));
        s.quad(c, d, h, g, vec3(0., 1., 0.));
        s.quad(d, a, e, h, vec3(-1., 0., 0.));
        s
    }

    #[test]
    fn wuerfel_geschlossen() {
        let n = netze(&wuerfel(), vec3(0.0, 0.0, 500.0));
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].punkte.len(), 8);
        assert_eq!(n[0].dreiecke.len(), 12);
        assert!(geschlossen(&n[0]));
        assert!((volumen(&n[0]) - 1e9).abs() < 1e-3);
        let (lo, hi) = grenzen(&n).unwrap();
        assert_eq!((lo.z, hi.z), (-500.0, 500.0));
    }

    #[test]
    fn stossflaechen_werden_geschlossen() {
        let mut s = wuerfel();
        // zwei gegenüberliegende Seiten fehlen, wie an einer Gehrung
        s.triangles.drain(4..6);
        s.triangles.drain(6..8);
        let n = netze(&s, Vec3::default());
        assert!(geschlossen(&n[0]), "{:?}", n[0].dreiecke);
        assert!((volumen(&n[0]) - 1e9).abs() < 1e-3);
    }

    #[test]
    fn falsch_herum_wird_gedreht() {
        let mut s = wuerfel();
        for t in &mut s.triangles {
            t.p.swap(1, 2);
        }
        let n = netze(&s, Vec3::default());
        assert!(geschlossen(&n[0]));
        assert!(volumen(&n[0]) > 0.0);
    }

    /// Befund Test zu O2: derselbe Punkt kommt aus zwei Rechnungen einmal als
    /// 717.31875 und einmal als 717.3187499999999 an; mit Rasterrundung lagen
    /// beide auf verschiedenen Seiten der .5-Grenze und das Netz war offen.
    #[test]
    fn schweissen_trennt_nicht_an_der_rundungsgrenze() {
        let mut s = wuerfel();
        let (a, b) = (717.31875, 717.3187499999999);
        assert_ne!((a * 1e4_f64).round(), (b * 1e4_f64).round());
        let mut i = 0;
        for t in &mut s.triangles {
            for p in &mut t.p {
                if p.z == 1000.0 {
                    p.z = if i % 2 == 0 { a } else { b };
                    i += 1;
                }
            }
        }
        let n = netze(&s, Vec3::default());
        assert_eq!(n[0].punkte.len(), 8);
        assert!(geschlossen(&n[0]));
        // Punkte, die weiter auseinanderliegen, bleiben getrennt
        let (mut netz, mut idx) = (Netz::default(), HashMap::new());
        let p = punkt(&mut netz, &mut idx, vec3(0.0, 0.0, a));
        assert_eq!(punkt(&mut netz, &mut idx, vec3(0.0, 0.0, b)), p);
        assert_ne!(punkt(&mut netz, &mut idx, vec3(0.0, 0.0, a + 2e-3)), p);
    }
}

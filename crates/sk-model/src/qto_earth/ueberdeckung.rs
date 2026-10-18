//! Überdeckung der Ränder mehrerer Gründungen (H-280-geo-a, VOB 18.10.
//! 15:05): Liegen zwei Häuser näher beieinander als ihre Ränder breit sind
//! (Oberboden-Rand, Überstand der Sandplatte), gibt es auf der Baustelle nur
//! eine Grube. Das Projekt zählt die Vereinigung der Ränder, und je Haus
//! wird die Überdeckung geteilt: Jede Stelle, die `k` Ränder decken, zählt
//! je Haus `1/k`. Stehen sich zwei Wände parallel gegenüber, ist das die
//! Mittellinie; sonst ist die Grenze im Grundriss eine andere, die Summe
//! der Häuser aber genau die Vereinigung, auch bei drei Häusern an einer
//! Ecke.
//!
//! Gerechnet wird mit den versetzten Umrissen (Ecken auf Gehrung, wie
//! `prisma`), geschnitten an den Dreiecken der anderen
//! (Sutherland–Hodgman), mit Einschluss und Ausschluss über die Nachbarn.

use sk_math::flaeche::versatz;
use sk_math::polygon::{signed_area, to_ccw, triangulate};
use sk_math::Vec3;

/// Ein Rand für [`anteile`]: Umriss der Platte und wie weit der Rand über
/// ihn hinausreicht (mm).
#[derive(Clone, Copy, Debug)]
pub struct Rand<'a> {
    pub umriss: &'a [Vec3],
    pub versatz: f64,
}

/// Anteil eines Randes an der Vereinigung aller.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Anteil {
    /// Fläche des versetzten Umrisses (mm²).
    pub flaeche: f64,
    /// Je anderer Rand (Nummer in der Eingabe), der diesen überdeckt: die
    /// Fläche, die dieser Rand ihm überlässt (mm²), aufsteigend nach
    /// Nummer.
    pub ab: Vec<(usize, f64)>,
}

impl Anteil {
    /// Was dem Rand bleibt (mm²): Die Summe über alle Ränder ist die
    /// Fläche ihrer Vereinigung.
    pub fn netto(&self) -> f64 {
        self.flaeche - self.ab.iter().map(|x| x.1).sum::<f64>()
    }

    /// Alles, was der Rand abgibt (mm²).
    pub fn summe_ab(&self) -> f64 {
        self.ab.iter().map(|x| x.1).sum()
    }
}

/// Unter dieser Fläche (mm²) überdeckt sich nichts.
const LEER: f64 = 1.0;

type P = [f64; 2];

/// Die Anteile der Ränder `raender` an ihrer Vereinigung, in derselben
/// Reihenfolge. `teilen(i, j)` sagt, ob sich die Ränder `i` und `j`
/// überdecken dürfen; zwei Häuser, die ineinander stehen, werden nicht
/// gekürzt (VOB Spruch zu #245/#248), dort steht der Hinweis.
pub fn anteile(raender: &[Rand], teilen: impl Fn(usize, usize) -> bool) -> Vec<Anteil> {
    let flaechen: Vec<Vec<P>> = raender.iter().map(versetzt).collect();
    let kaesten: Vec<[f64; 4]> = flaechen.iter().map(|f| kasten(f)).collect();
    // Nachbarn: Kästen, die sich berühren, nach x sortiert durchlaufen
    // (viele Häuser: nicht jedes gegen jedes)
    let mut folge: Vec<usize> = (0..flaechen.len())
        .filter(|&i| !flaechen[i].is_empty())
        .collect();
    folge.sort_by(|&a, &b| kaesten[a][0].total_cmp(&kaesten[b][0]));
    let mut nachbarn: Vec<Vec<usize>> = vec![Vec::new(); flaechen.len()];
    for (n, &i) in folge.iter().enumerate() {
        for &j in &folge[n + 1..] {
            if kaesten[j][0] > kaesten[i][2] {
                break;
            }
            if beruehrt(&kaesten[i], &kaesten[j]) && teilen(i, j) {
                nachbarn[i].push(j);
                nachbarn[j].push(i);
            }
        }
    }
    let dreiecke: Vec<Vec<[P; 3]>> = flaechen
        .iter()
        .zip(&nachbarn)
        .map(|(f, nb)| {
            if nb.is_empty() {
                return Vec::new();
            }
            let v: Vec<Vec3> = f.iter().map(|p| sk_math::vec3(p[0], p[1], 0.0)).collect();
            triangulate(&v)
                .into_iter()
                .map(|t| t.map(|k| f[k]))
                .collect()
        })
        .collect();
    let mut out = Vec::with_capacity(raender.len());
    for (i, f) in flaechen.iter().enumerate() {
        let mut a = Anteil {
            flaeche: if f.is_empty() { 0.0 } else { flaeche(f) },
            ab: Vec::new(),
        };
        let nachbarn = &mut nachbarn[i];
        nachbarn.sort_unstable();
        for &j in nachbarn.iter() {
            let mit_j = schneiden(std::slice::from_ref(f), &dreiecke[j]);
            if gesamt(&mit_j) <= LEER {
                continue;
            }
            // ∫ über die Überdeckung mit j von 1/k: Einschluss und
            // Ausschluss über die übrigen Nachbarn S mit dem Gewicht
            // (−1)^|S| / ((|S| + 1)(|S| + 2))
            let andere: Vec<usize> = nachbarn.iter().copied().filter(|&x| x != j).collect();
            let x = mit_anderen(&mit_j, &andere, 0, 0, &dreiecke);
            if x > LEER {
                a.ab.push((j, x));
            }
        }
        out.push(a);
    }
    out
}

/// Summe über die Teilmengen S von `andere[ab..]` (dazu schon `tiefe`
/// gewählte) von (−1)^|S| / ((|S| + 1)(|S| + 2)) · |`stuecke` ∩ S|.
fn mit_anderen(
    stuecke: &[Vec<P>],
    andere: &[usize],
    ab: usize,
    tiefe: usize,
    dreiecke: &[Vec<[P; 3]>],
) -> f64 {
    let s = tiefe as f64;
    let vorzeichen = if tiefe.is_multiple_of(2) { 1.0 } else { -1.0 };
    let mut x = vorzeichen / ((s + 1.0) * (s + 2.0)) * gesamt(stuecke);
    for (n, &k) in andere.iter().enumerate().skip(ab) {
        let weiter = schneiden(stuecke, &dreiecke[k]);
        if gesamt(&weiter) > LEER {
            x += mit_anderen(&weiter, andere, n + 1, tiefe + 1, dreiecke);
        }
    }
    x
}

/// Der Umriss des Randes, gegen den Uhrzeigersinn; leer, wenn er sich
/// nicht versetzen lässt.
fn versetzt(r: &Rand) -> Vec<P> {
    if r.umriss.len() < 3 {
        return Vec::new();
    }
    let u = to_ccw(r.umriss);
    let v = if r.versatz > 0.0 {
        match versatz(&u, &vec![-r.versatz; u.len()]) {
            Some(v) => v,
            None => return Vec::new(),
        }
    } else {
        u
    };
    v.iter().map(|p| [p.x, p.y]).collect()
}

/// Die Stücke `stuecke`, geschnitten mit jedem der Dreiecke `ds` (gegen
/// den Uhrzeigersinn): wieder Stücke, zusammen ihr Schnitt mit der Fläche
/// der Dreiecke.
fn schneiden(stuecke: &[Vec<P>], ds: &[[P; 3]]) -> Vec<Vec<P>> {
    let mut out = Vec::new();
    for s in stuecke {
        let k = kasten(s);
        for d in ds {
            if !beruehrt(&k, &kasten(d)) {
                continue;
            }
            let mut r = s.clone();
            for e in 0..3 {
                r = halbebene(&r, d[e], d[(e + 1) % 3]);
                if r.len() < 3 {
                    break;
                }
            }
            if r.len() >= 3 && flaeche(&r) > 0.0 {
                out.push(r);
            }
        }
    }
    out
}

/// Das Vieleck `s` links der Geraden `a`→`b` (Sutherland–Hodgman).
fn halbebene(s: &[P], a: P, b: P) -> Vec<P> {
    let seite = |p: P| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    let n = s.len();
    let mut out = Vec::with_capacity(n + 2);
    for i in 0..n {
        let (p, q) = (s[i], s[(i + 1) % n]);
        let (sp, sq) = (seite(p), seite(q));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let t = sp / (sp - sq);
            out.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
        }
    }
    out
}

fn flaeche(s: &[P]) -> f64 {
    let v: Vec<Vec3> = s.iter().map(|p| sk_math::vec3(p[0], p[1], 0.0)).collect();
    signed_area(&v)
}

fn gesamt(stuecke: &[Vec<P>]) -> f64 {
    stuecke.iter().map(|s| flaeche(s)).sum()
}

fn kasten(s: &[P]) -> [f64; 4] {
    s.iter().fold(
        [f64::MAX, f64::MAX, f64::MIN, f64::MIN],
        |[x0, y0, x1, y1], p| [x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1])],
    )
}

fn beruehrt(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// Kleinster Abstand zweier Umrisse im Grundriss (mm), für den Rechenweg;
/// 0, wenn sie sich schneiden oder einer im anderen liegt.
pub fn abstand(a: &[Vec3], b: &[Vec3]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if sk_math::flaeche::enthaelt(a, b[0]) || sk_math::flaeche::enthaelt(b, a[0]) {
        return 0.0;
    }
    let p = |v: &Vec3| [v.x, v.y];
    let kanten = |u: &[Vec3]| -> Vec<(P, P)> {
        (0..u.len())
            .map(|i| (p(&u[i]), p(&u[(i + 1) % u.len()])))
            .collect()
    };
    let (ka, kb) = (kanten(a), kanten(b));
    let mut d = f64::MAX;
    for &(a0, a1) in &ka {
        for &(b0, b1) in &kb {
            if kreuzen(a0, a1, b0, b1) {
                return 0.0;
            }
            d = d
                .min(punkt_strecke(a0, b0, b1))
                .min(punkt_strecke(a1, b0, b1))
                .min(punkt_strecke(b0, a0, a1))
                .min(punkt_strecke(b1, a0, a1));
        }
    }
    d
}

fn punkt_strecke(p: P, a: P, b: P) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let l2 = d[0] * d[0] + d[1] * d[1];
    let t = if l2 > 0.0 {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - d[0] * t).hypot(p[1] - a[1] - d[1] * t)
}

fn kreuzen(a: P, b: P, c: P, d: P) -> bool {
    let x = |o: P, p: P, q: P| (p[0] - o[0]) * (q[1] - o[1]) - (p[1] - o[1]) * (q[0] - o[0]);
    let (d1, d2) = (x(a, b, c), x(a, b, d));
    let (d3, d4) = (x(c, d, a), x(c, d, b));
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0) && d1 != 0.0 && d3 != 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    fn rechteck(x: f64, y: f64, w: f64, d: f64) -> Vec<Vec3> {
        vec![
            vec3(x, y, 0.0),
            vec3(x + w, y, 0.0),
            vec3(x + w, y + d, 0.0),
            vec3(x, y + d, 0.0),
        ]
    }

    fn summe(a: &[Anteil]) -> f64 {
        a.iter().map(Anteil::netto).sum()
    }

    /// Zwei Häuser 8 × 10 m nebeneinander, Rand 1,5 m: Bis 3 m Abstand
    /// decken sich die Ränder in einem Streifen 10 + 3 m lang; jedes Haus
    /// gibt die Hälfte ab, die Summe ist die Vereinigung.
    #[test]
    fn zwei_haeuser_mittellinie() {
        let x = 1500.0;
        let einzel = (8000.0 + 2.0 * x) * (10000.0 + 2.0 * x);
        for spalt in [0.0, 630.0, 1000.0, 2999.0, 3000.0, 4000.0] {
            let (a, b) = (
                rechteck(0.0, 0.0, 8000.0, 10000.0),
                rechteck(8000.0 + spalt, 0.0, 8000.0, 10000.0),
            );
            let r = [
                Rand {
                    umriss: &a,
                    versatz: x,
                },
                Rand {
                    umriss: &b,
                    versatz: x,
                },
            ];
            let v = anteile(&r, |_, _| true);
            let streifen = (2.0 * x - spalt).max(0.0) * (10000.0 + 2.0 * x);
            assert!((v[0].flaeche - einzel).abs() < 1e-3 * einzel);
            assert!(
                (summe(&v) - (2.0 * einzel - streifen)).abs() < 1e-6 * einzel,
                "{spalt}"
            );
            assert!((v[0].summe_ab() - streifen / 2.0).abs() < 1e-6 * einzel);
            assert!((v[1].summe_ab() - streifen / 2.0).abs() < 1e-6 * einzel);
            assert_eq!(v[0].ab.is_empty(), streifen <= LEER);
            assert!((abstand(&a, &b) - spalt).abs() < 1e-9);
        }
    }

    /// Drei Häuser an einer Ecke: Die Summe der Anteile ist die Fläche der
    /// Vereinigung, ausgezählt auf einem feinen Raster.
    #[test]
    fn drei_an_einer_ecke() {
        let u = [
            rechteck(0.0, 0.0, 6000.0, 6000.0),
            rechteck(7000.0, 0.0, 6000.0, 6000.0),
            rechteck(3000.0, 7000.0, 6000.0, 6000.0),
        ];
        let x = 1500.0;
        let r: Vec<Rand> = u
            .iter()
            .map(|u| Rand {
                umriss: u,
                versatz: x,
            })
            .collect();
        let v = anteile(&r, |_, _| true);
        let n = 1200;
        let (x0, y0, w) = (-x, -x, 13000.0 + 2.0 * x);
        let h = w / n as f64;
        let mut drin = 0usize;
        for i in 0..n {
            for j in 0..n {
                let (px, py) = (x0 + (i as f64 + 0.5) * h, y0 + (j as f64 + 0.5) * h);
                // Gehrung am Rechteck: die Ecken bleiben eckig
                if u.iter().any(|u| {
                    px >= u[0].x - x && px <= u[2].x + x && py >= u[0].y - x && py <= u[2].y + x
                }) {
                    drin += 1;
                }
            }
        }
        let raster = drin as f64 * h * h;
        assert!(
            (summe(&v) - raster).abs() < 2e-3 * raster,
            "{} {raster}",
            summe(&v)
        );
        // jedes Paar gibt sich gleich viel ab
        for i in 0..3 {
            for &(j, a) in &v[i].ab {
                let b = v[j].ab.iter().find(|x| x.0 == i).unwrap().1;
                assert!((a - b).abs() < 1e-6 * a);
            }
        }
    }

    /// Ein Paar, das nicht geteilt wird (ineinander), gibt nichts ab.
    #[test]
    fn ineinander_nicht_gekuerzt() {
        let (a, b) = (
            rechteck(0.0, 0.0, 8000.0, 10000.0),
            rechteck(7000.0, 0.0, 8000.0, 10000.0),
        );
        let r = [
            Rand {
                umriss: &a,
                versatz: 500.0,
            },
            Rand {
                umriss: &b,
                versatz: 500.0,
            },
        ];
        let v = anteile(&r, |_, _| false);
        assert!(v.iter().all(|a| a.ab.is_empty()));
        assert_eq!(abstand(&a, &b), 0.0);
    }

    /// L-förmiges Haus neben einem Rechteck: Summe gleich Vereinigung.
    #[test]
    fn l_haus() {
        let l = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 4000.0, 0.0),
            vec3(4000.0, 4000.0, 0.0),
            vec3(4000.0, 10000.0, 0.0),
            vec3(0.0, 10000.0, 0.0),
        ];
        let b = rechteck(5000.0, 5000.0, 5000.0, 5000.0);
        let x = 1500.0;
        let r = [
            Rand {
                umriss: &l,
                versatz: x,
            },
            Rand {
                umriss: &b,
                versatz: x,
            },
        ];
        let v = anteile(&r, |_, _| true);
        // Überdeckung: Rand des L oben rechts (x 4000..5500 bzw. y 4000..5500)
        // mit dem Rand des Rechtecks (x ab 3500, y ab 3500)
        let ub =
            2000.0 * (10000.0 + x - 3500.0) + 2000.0 * (10000.0 + x - 3500.0) - 2000.0 * 2000.0;
        assert!(
            (v[0].summe_ab() - ub / 2.0).abs() < 1e-6 * ub,
            "{} {}",
            v[0].summe_ab(),
            ub / 2.0
        );
        assert!((v[1].summe_ab() - ub / 2.0).abs() < 1e-6 * ub);
        assert!((abstand(&l, &b) - 1000.0).abs() < 1e-9);
    }
}

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
use sk_math::polygon::{is_simple, signed_area, to_ccw, triangulate};
use sk_math::{vec3, Vec3};

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

/// Ein Rand als Fläche: der Außenrand gegen den Uhrzeigersinn und die
/// Höfe, die er umschließt, aber nicht deckt (gegen den Uhrzeigersinn).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Randflaeche {
    pub aussen: Vec<Vec3>,
    pub hoefe: Vec<Vec<Vec3>>,
}

impl Randflaeche {
    /// Fläche ohne die Höfe (mm²).
    pub fn flaeche(&self) -> f64 {
        signed_area(&self.aussen).abs()
            - self.hoefe.iter().map(|h| signed_area(h).abs()).sum::<f64>()
    }
}

/// Unter dieser Fläche (mm²) überdeckt sich nichts.
const LEER: f64 = 1.0;

type P = [f64; 2];

/// Ein Stück einer Fläche mit Vorzeichen: der Außenrand zählt +1, ein Hof
/// darin −1. Schnitte multiplizieren die Vorzeichen; so bleibt jedes
/// Integral über die Fläche eine Summe über Stücke ohne Löcher.
type Stueck = (Vec<P>, f64);

/// Die Anteile der Ränder `raender` an ihrer Vereinigung, in derselben
/// Reihenfolge. `teilen(i, j)` sagt, ob sich die Ränder `i` und `j`
/// überdecken dürfen; zwei Häuser, die ineinander stehen, werden nicht
/// gekürzt (VOB Spruch zu #245/#248), dort steht der Hinweis.
pub fn anteile(raender: &[Rand], teilen: impl Fn(usize, usize) -> bool) -> Vec<Anteil> {
    let flaechen: Vec<Vec<Stueck>> = raender.iter().map(versetzt).collect();
    let kaesten: Vec<[f64; 4]> = flaechen
        .iter()
        .map(|f| f.first().map_or([0.0; 4], |s| kasten(&s.0)))
        .collect();
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
    let dreiecke: Vec<Vec<([P; 3], f64)>> = flaechen
        .iter()
        .zip(&nachbarn)
        .map(|(f, nb)| {
            if nb.is_empty() {
                return Vec::new();
            }
            f.iter()
                .flat_map(|(s, v)| {
                    let pts: Vec<Vec3> = s.iter().map(|p| sk_math::vec3(p[0], p[1], 0.0)).collect();
                    triangulate(&pts)
                        .into_iter()
                        .map(move |t| (t.map(|k| s[k]), *v))
                })
                .collect()
        })
        .collect();
    let mut out = Vec::with_capacity(raender.len());
    for (i, f) in flaechen.iter().enumerate() {
        let mut a = Anteil {
            flaeche: gesamt(f),
            ab: Vec::new(),
        };
        let nachbarn = &mut nachbarn[i];
        nachbarn.sort_unstable();
        for &j in nachbarn.iter() {
            let mit_j = schneiden(f, &dreiecke[j]);
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
    stuecke: &[Stueck],
    andere: &[usize],
    ab: usize,
    tiefe: usize,
    dreiecke: &[Vec<([P; 3], f64)>],
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

/// Der Umriss des Randes, gegen den Uhrzeigersinn. Hat der Umriss eine
/// Nische oder einen Rücksprung, schmaler als zweimal der Rand, kehrt sich
/// beim Versetzen eine Kante um (oder zwei Kanten laufen ineinander); dann
/// ist der Rand die Vereinigung aus Umriss, einem Streifen je Kante und
/// der Gehrung je Ecke: Er füllt die Nische, wie auf der Baustelle
/// (B-285-geo-a, sonst gab das Haus still nichts ab). Schließt der Rand
/// dabei einen Hof ein, den er nicht deckt, zählt der Hof nicht mit
/// (B-285-geo-b).
fn versetzt(r: &Rand) -> Vec<Stueck> {
    if r.umriss.len() < 3 {
        return Vec::new();
    }
    let u = to_ccw(r.umriss);
    let f = if r.versatz > 0.0 {
        match versatz(&u, &vec![-r.versatz; u.len()]).filter(|v| is_simple(v)) {
            Some(v) => Randflaeche {
                aussen: v,
                hoefe: Vec::new(),
            },
            None => vereinigt(&u, r.versatz),
        }
    } else {
        Randflaeche {
            aussen: u,
            hoefe: Vec::new(),
        }
    };
    if f.aussen.len() < 3 {
        return Vec::new();
    }
    let flach = |v: &[Vec3]| v.iter().map(|p| [p.x, p.y]).collect::<Vec<P>>();
    std::iter::once((flach(&f.aussen), 1.0))
        .chain(f.hoefe.iter().map(|h| (flach(h), -1.0)))
        .collect()
}

/// Der Umriss `u` (gegen den Uhrzeigersinn) um `x` nach außen, als
/// Vereinigung: Umriss, je Kante der Streifen außen davor, je Ecke nach
/// außen das Viereck bis zur Gehrung. Schließt sie einen Hof ein (ein Haus
/// mit Innenhof hinter einer schmalen Einfahrt), bleibt er ein Hof. Leer,
/// wenn sich nichts vereinigen lässt.
fn vereinigt(u: &[Vec3], x: f64) -> Randflaeche {
    let n = u.len();
    let d: Vec<Vec3> = (0..n)
        .map(|i| (u[(i + 1) % n] - u[i]).normalized())
        .collect();
    // außen ist rechts der Kante
    let aussen = |v: Vec3| vec3(v.y, -v.x, 0.0) * x;
    let mut teile = vec![u.to_vec()];
    for i in 0..n {
        let (a, b) = (u[i], u[(i + 1) % n]);
        if (b - a).length() > 0.0 {
            teile.push(vec![a, a + aussen(d[i]), b + aussen(d[i]), b]);
        }
        let h = (i + n - 1) % n;
        let (nh, ni) = (aussen(d[h]), aussen(d[i]));
        let kreuz = d[h].x * d[i].y - d[h].y * d[i].x;
        let cos = d[h].dot(d[i]);
        if kreuz > 1e-9 && cos > -1.0 + 1e-9 {
            // Gehrung: Schnitt der beiden versetzten Kanten
            let m = a + (nh + ni) * (1.0 / (1.0 + cos));
            teile.push(vec![a, a + nh, m, a + ni]);
        }
    }
    let a = sk_math::flaeche::anordnung(&teile, 1e-3);
    let Some(aussen) = a
        .aussen
        .into_iter()
        .max_by(|p, q| p.flaeche().total_cmp(&q.flaeche()))
    else {
        return Randflaeche::default();
    };
    // Die Bereiche sind die umschlossenen, nicht gedeckten Flächen. Alle
    // Teile hängen am Umriss, die Vereinigung ist also zusammenhängend und
    // jeder Bereich ein Hof in ihrem Außenrand.
    let hoefe = a
        .bereiche
        .into_iter()
        .map(|b| to_ccw(&b.rand.punkte))
        .collect();
    Randflaeche {
        aussen: to_ccw(&aussen.punkte),
        hoefe,
    }
}

/// Die Stücke `stuecke`, geschnitten mit jedem der Dreiecke `ds` (gegen
/// den Uhrzeigersinn): wieder Stücke, zusammen ihr Schnitt mit der Fläche
/// der Dreiecke; die Vorzeichen multiplizieren sich.
fn schneiden(stuecke: &[Stueck], ds: &[([P; 3], f64)]) -> Vec<Stueck> {
    let mut out = Vec::new();
    for (s, vs) in stuecke {
        let k = kasten(s);
        for (d, vd) in ds {
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
                out.push((r, vs * vd));
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

fn gesamt(stuecke: &[Stueck]) -> f64 {
    stuecke.iter().map(|(s, v)| v * flaeche(s)).sum()
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

    /// Haus 8 × 10 m mit Eingangsnische 1 m tief, schmaler als zweimal der
    /// Rand, Nachbar 1 m daneben (B-285-geo-a): Der Rand füllt die Nische,
    /// und die Summe ist die Vereinigung, ausgezählt auf einem Raster. Bei
    /// rechten Winkeln ist der Rand auf Gehrung das Rechteck um jedes
    /// Teilrechteck des Hauses, um den Rand vergrößert.
    #[test]
    fn nische_schmaler_als_rand() {
        let x = 1500.0;
        for breite in [1500.0, 2500.0, 3500.0, 5000.0] {
            let (n0, n1) = (4000.0 - breite / 2.0, 4000.0 + breite / 2.0);
            // Nische unten, vom Nachbarn weg und zu ihm hin gedreht
            let nischenhaus = vec![
                vec3(0.0, 0.0, 0.0),
                vec3(n0, 0.0, 0.0),
                vec3(n0, 1000.0, 0.0),
                vec3(n1, 1000.0, 0.0),
                vec3(n1, 0.0, 0.0),
                vec3(8000.0, 0.0, 0.0),
                vec3(8000.0, 10000.0, 0.0),
                vec3(0.0, 10000.0, 0.0),
            ];
            let teile = [
                [0.0, 1000.0, 8000.0, 10000.0],
                [0.0, 0.0, n0, 1000.0],
                [n1, 0.0, 8000.0, 1000.0],
                [9000.0, 0.0, 17000.0, 10000.0],
            ];
            let nachbar = rechteck(9000.0, 0.0, 8000.0, 10000.0);
            let r = [
                Rand {
                    umriss: &nischenhaus,
                    versatz: x,
                },
                Rand {
                    umriss: &nachbar,
                    versatz: x,
                },
            ];
            let v = anteile(&r, |_, _| true);
            assert!(!v[0].ab.is_empty() && !v[1].ab.is_empty(), "{breite}");
            let n = 1500;
            let (x0, y0, w) = (-x, -x, 17000.0 + 2.0 * x);
            let h = w / n as f64;
            let mut drin = 0usize;
            for i in 0..n {
                for j in 0..n {
                    let (px, py) = (x0 + (i as f64 + 0.5) * h, y0 + (j as f64 + 0.5) * h);
                    if teile.iter().any(|t| {
                        px >= t[0] - x && px <= t[2] + x && py >= t[1] - x && py <= t[3] + x
                    }) {
                        drin += 1;
                    }
                }
            }
            let raster = drin as f64 * h * h;
            // eine schmale Nische ist gefüllt: das Haus allein wie ohne sie
            let voll = (8000.0 + 2.0 * x) * (10000.0 + 2.0 * x);
            if breite < 2.0 * x {
                assert!(
                    (v[0].flaeche - voll).abs() < 1e-6 * voll,
                    "{breite}: {}",
                    v[0].flaeche
                );
            } else {
                assert!(v[0].flaeche < voll - 1e5, "{breite}");
            }
            assert!(
                (summe(&v) - raster).abs() < 2e-3 * raster,
                "{breite}: {} {raster}",
                summe(&v)
            );
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

    /// C-Haus 16 × 14 m mit Hof 12 × 10 m hinter einer Einfahrt `e` breit
    /// in der oberen Wand; als Rechtecke [x0, y0, x1, y1] (mm).
    fn c_haus(e: f64) -> (Vec<Vec3>, Vec<[f64; 4]>) {
        let (l, r) = (8000.0 - e / 2.0, 8000.0 + e / 2.0);
        let u = [
            (0.0, 0.0),
            (16000.0, 0.0),
            (16000.0, 14000.0),
            (r, 14000.0),
            (r, 12000.0),
            (14000.0, 12000.0),
            (14000.0, 2000.0),
            (2000.0, 2000.0),
            (2000.0, 12000.0),
            (l, 12000.0),
            (l, 14000.0),
            (0.0, 14000.0),
        ]
        .map(|(x, y)| vec3(x, y, 0.0))
        .to_vec();
        let teile = vec![
            [0.0, 0.0, 16000.0, 2000.0],
            [0.0, 2000.0, 2000.0, 14000.0],
            [14000.0, 2000.0, 16000.0, 14000.0],
            [2000.0, 12000.0, l, 14000.0],
            [r, 12000.0, 14000.0, 14000.0],
        ];
        (u, teile)
    }

    /// Schließt der Rand die Einfahrt eines Hauses mit Innenhof, bleibt der
    /// Hof dahinter frei: Er zählt nicht zum Rand, und eine Garage darin
    /// gibt nur ab, was sich wirklich deckt (B-285-geo-b, B-288-geo-a).
    #[test]
    fn innenhof_hinter_schmaler_einfahrt() {
        let x = 1500.0;
        // Rechteck: alle Punkte bis x (Maximumnorm) um die Teile, wie
        // Streifen und Gehrung bei rechten Winkeln
        let drin = |t: &[[f64; 4]], px: f64, py: f64| {
            t.iter()
                .any(|t| px >= t[0] - x && px <= t[2] + x && py >= t[1] - x && py <= t[3] + x)
        };
        // Garage im Hof, 1 m vor der oberen Wand: ihr Rand deckt sich mit dem
        // des Hauses nur in einem Streifen
        let garage = rechteck(6000.0, 7000.0, 4000.0, 4000.0);
        let garage_teile = [[6000.0, 7000.0, 10000.0, 11000.0]];
        for e in [1000.0, 2000.0, 2900.0, 3100.0, 5000.0] {
            let (c, teile) = c_haus(e);
            let allein = anteile(
                &[Rand {
                    umriss: &c,
                    versatz: x,
                }],
                |_, _| true,
            );
            let mit = anteile(
                &[
                    Rand {
                        umriss: &c,
                        versatz: x,
                    },
                    Rand {
                        umriss: &garage,
                        versatz: x,
                    },
                ],
                |_, _| true,
            );
            let n = 1200;
            let (x0, y0, w) = (-x, -x, 16000.0 + 2.0 * x);
            let h = w / n as f64;
            let (mut nur_c, mut beide) = (0usize, 0usize);
            for i in 0..n {
                for j in 0..n {
                    let (px, py) = (x0 + (i as f64 + 0.5) * h, y0 + (j as f64 + 0.5) * h);
                    let ic = drin(&teile, px, py);
                    nur_c += ic as usize;
                    beide += (ic || drin(&garage_teile, px, py)) as usize;
                }
            }
            let (nur_c, beide) = (nur_c as f64 * h * h, beide as f64 * h * h);
            assert!(
                (allein[0].flaeche - nur_c).abs() < 2e-3 * nur_c,
                "{e}: {} {nur_c}",
                allein[0].flaeche
            );
            if e < 2.0 * x {
                // um das Haus 19 × 17 m, ohne den Hof 9 × 7 m
                assert!((allein[0].flaeche - 260e6).abs() < 1e-6 * 260e6, "{e}");
            }
            assert!(
                (summe(&mit) - beide).abs() < 2e-3 * beide,
                "{e}: {} {beide}",
                summe(&mit)
            );
            assert!(!mit[1].ab.is_empty(), "{e}");
        }
    }
}

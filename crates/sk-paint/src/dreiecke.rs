//! Füllung eines Pfades als Dreiecke (für die Grafikkarte): Schrift, die in
//! 3D flach auf einer Ebene liegt (Bemaßung, Jörn 12.10.).
//!
//! Zerlegung in waagerechte Streifen: Zwischen zwei aufeinanderfolgenden
//! Höhen, an denen ein Eckpunkt liegt oder sich zwei Kanten kreuzen, laufen
//! alle Kanten gerade und kreuzungsfrei. Je Streifen werden die Kanten nach
//! ihrer Lage sortiert und mit der Umlaufzahl (ungleich null = innen, wie
//! [`crate::Canvas::fill`]) zu Trapezen gepaart. Löcher und Gegenformen
//! (0, 6, 8, A …) brauchen so keine Sonderbehandlung.

use crate::{pt, Path, Pt};

/// Kante eines Polygons von oben (`y0`) nach unten (`y1`), mit Umlaufrichtung.
#[derive(Clone, Copy, Debug)]
struct Kante {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    dir: i32,
}

impl Kante {
    fn x_bei(&self, y: f64) -> f64 {
        let t = (y - self.y0) / (self.y1 - self.y0);
        self.x0 + (self.x1 - self.x0) * t
    }
}

/// Höhe, an der sich zwei Kanten echt kreuzen.
fn kreuzung(a: &Kante, b: &Kante) -> Option<f64> {
    let lo = a.y0.max(b.y0);
    let hi = a.y1.min(b.y1);
    if hi - lo <= 1e-9 {
        return None;
    }
    let (da0, da1) = (a.x_bei(lo) - b.x_bei(lo), a.x_bei(hi) - b.x_bei(hi));
    if da0 * da1 >= 0.0 {
        return None;
    }
    Some(lo + (hi - lo) * da0 / (da0 - da1))
}

impl Path {
    /// Fläche des Pfades als Dreiecke (Kurven mit Toleranz `tol`), nach der
    /// Regel „Umlaufzahl ungleich null“. Leere Streifen und entartete Dreiecke
    /// fallen weg.
    pub fn triangles(&self, tol: f32) -> Vec<[Pt; 3]> {
        let mut kanten: Vec<Kante> = Vec::new();
        for poly in self.flatten(tol) {
            let n = poly.len();
            for i in 0..n {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                let (ax, ay, bx, by) = (a.x as f64, a.y as f64, b.x as f64, b.y as f64);
                if (ay - by).abs() < 1e-9 {
                    continue;
                }
                kanten.push(if ay < by {
                    Kante {
                        x0: ax,
                        y0: ay,
                        x1: bx,
                        y1: by,
                        dir: 1,
                    }
                } else {
                    Kante {
                        x0: bx,
                        y0: by,
                        x1: ax,
                        y1: ay,
                        dir: -1,
                    }
                });
            }
        }
        if kanten.is_empty() {
            return Vec::new();
        }
        let mut ys: Vec<f64> = kanten.iter().flat_map(|k| [k.y0, k.y1]).collect();
        for i in 0..kanten.len() {
            for j in i + 1..kanten.len() {
                if let Some(y) = kreuzung(&kanten[i], &kanten[j]) {
                    ys.push(y);
                }
            }
        }
        ys.sort_by(f64::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 1e-7);

        let mut out = Vec::new();
        let mut aktiv: Vec<(f64, f64, f64, i32)> = Vec::new();
        for w in ys.windows(2) {
            let (ya, yb) = (w[0], w[1]);
            let ym = 0.5 * (ya + yb);
            aktiv.clear();
            aktiv.extend(
                kanten
                    .iter()
                    .filter(|k| k.y0 <= ym && k.y1 >= ym)
                    .map(|k| (k.x_bei(ym), k.x_bei(ya), k.x_bei(yb), k.dir)),
            );
            aktiv.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut umlauf = 0;
            let mut links: Option<(f64, f64)> = None;
            for &(_, xa, xb, dir) in &aktiv {
                let vorher = umlauf;
                umlauf += dir;
                if vorher == 0 && umlauf != 0 {
                    links = Some((xa, xb));
                } else if vorher != 0 && umlauf == 0 {
                    if let Some((la, lb)) = links.take() {
                        let p = |x: f64, y: f64| pt(x as f32, y as f32);
                        let (a, b, c, d) = (p(la, ya), p(xa, ya), p(xb, yb), p(lb, yb));
                        if xa - la > 1e-9 {
                            out.push([a, b, c]);
                        }
                        if xb - lb > 1e-9 {
                            out.push([a, c, d]);
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flaeche(t: &[[Pt; 3]]) -> f64 {
        t.iter()
            .map(|[a, b, c]| {
                0.5 * (((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)) as f64).abs()
            })
            .sum()
    }

    #[test]
    fn rechteck_ist_zwei_dreiecke() {
        let mut p = Path::new();
        p.move_to(0.0, 0.0)
            .line_to(4.0, 0.0)
            .line_to(4.0, 3.0)
            .line_to(0.0, 3.0)
            .close();
        let t = p.triangles(0.1);
        assert_eq!(t.len(), 2);
        assert!((flaeche(&t) - 12.0).abs() < 1e-6);
    }

    #[test]
    fn loch_wird_ausgespart() {
        // Außen im Uhrzeigersinn, Loch gegenläufig (wie Glyphen)
        let mut p = Path::new();
        p.move_to(0.0, 0.0)
            .line_to(10.0, 0.0)
            .line_to(10.0, 10.0)
            .line_to(0.0, 10.0)
            .close();
        p.move_to(3.0, 3.0)
            .line_to(3.0, 7.0)
            .line_to(7.0, 7.0)
            .line_to(7.0, 3.0)
            .close();
        let t = p.triangles(0.1);
        assert!((flaeche(&t) - 84.0).abs() < 1e-6);
        // Kein Dreieck deckt die Mitte des Lochs
        let innen = |q: Pt, [a, b, c]: [Pt; 3]| {
            let s = |p1: Pt, p2: Pt| (p2.x - p1.x) * (q.y - p1.y) - (p2.y - p1.y) * (q.x - p1.x);
            let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
            !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
        };
        assert!(!t.iter().any(|d| innen(pt(5.0, 5.0), *d)));
    }

    #[test]
    fn ueberlappende_teilpfade_zaehlen_einmal() {
        // Zwei gleich laufende Quadrate, die sich zur Hälfte überdecken
        let mut p = Path::new();
        for x in [0.0, 2.0] {
            p.move_to(x, 0.0)
                .line_to(x + 4.0, 0.0)
                .line_to(x + 4.0, 4.0)
                .line_to(x, 4.0)
                .close();
        }
        assert!((flaeche(&p.triangles(0.1)) - 24.0).abs() < 1e-6);
    }

    #[test]
    fn kreuzende_kanten_werden_geteilt() {
        // Schleife (Acht): beide Hälften gefüllt, Fläche je 2
        let mut p = Path::new();
        p.move_to(0.0, 0.0)
            .line_to(2.0, 2.0)
            .line_to(2.0, 0.0)
            .line_to(0.0, 2.0)
            .close();
        assert!((flaeche(&p.triangles(0.1)) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn kreis_kommt_nahe_an_pi() {
        let mut p = Path::new();
        p.rounded_rect(0.0, 0.0, 2.0, 2.0, 1.0);
        let a = flaeche(&p.triangles(0.001));
        assert!((a - std::f64::consts::PI).abs() < 0.01, "{a}");
    }
}

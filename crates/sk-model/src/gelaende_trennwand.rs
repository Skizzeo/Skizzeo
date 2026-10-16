//! Freier Plattenrand (H-245a, VOB): An einer Haustrennwand liegt der
//! Plattenrand im oder am Umriss des Nachbarhauses. Das Gelände dort ist
//! das des Nachbarn, und Frost greift dort nicht an. Höhen am Plattenrand
//! (tiefster Punkt für die Einbindetiefe, höchster für die Lage) zählen
//! deshalb nur am freien Rand.

/// Unter diesem Abstand (mm) liegt der Plattenrand am Umriss eines anderen
/// Hauses: Wand an Wand, auch mit dem Übergang zwischen den Platten
/// (`gelaende_eben::FUGE`).
pub(super) const AN_DER_WAND: f64 = 20.0;

/// Die Strecken des geschlossenen Umrisses `ring` (mm), die weder in einem
/// der Umrisse `andere` liegen noch näher als [`AN_DER_WAND`] an ihnen.
pub(super) fn freie_strecken(
    ring: &[[f64; 2]],
    andere: &[&[[f64; 2]]],
) -> Vec<([f64; 2], [f64; 2])> {
    let tol = AN_DER_WAND;
    let n = ring.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = d[0].hypot(d[1]);
        if len < 1e-9 {
            continue;
        }
        let kasten = [
            a[0].min(b[0]) - tol,
            a[1].min(b[1]) - tol,
            a[0].max(b[0]) + tol,
            a[1].max(b[1]) + tol,
        ];
        let nah: Vec<&[[f64; 2]]> = andere
            .iter()
            .copied()
            .filter(|p| {
                p.iter().any(|q| q[0] >= kasten[0])
                    && p.iter().any(|q| q[0] <= kasten[2])
                    && p.iter().any(|q| q[1] >= kasten[1])
                    && p.iter().any(|q| q[1] <= kasten[3])
            })
            .collect();
        if nah.is_empty() {
            out.push((a, b));
            continue;
        }
        let punkt = |t: f64| [a[0] + t * d[0], a[1] + t * d[1]];
        // Teilungen an Schnitten und nahen Ecken, je ± den Abstand
        let dt = tol / len;
        let mut ts = vec![0.0, 1.0];
        for p in &nah {
            for j in 0..p.len() {
                let (c, e) = (p[j], p[(j + 1) % p.len()]);
                if let Some(t) = schnitt(a, b, c, e) {
                    ts.extend([t - dt, t + dt]);
                }
                let t = ((c[0] - a[0]) * d[0] + (c[1] - a[1]) * d[1]) / (len * len);
                if (0.0..=1.0).contains(&t) && abstand(punkt(t), c) < tol {
                    ts.extend([t - dt, t + dt]);
                }
            }
        }
        for t in &mut ts {
            *t = t.clamp(0.0, 1.0);
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup();
        for w in ts.windows(2) {
            let m = punkt((w[0] + w[1]) / 2.0);
            let frei = !nah.iter().any(|p| innen(p, m) || rand_abstand(p, m) < tol);
            if frei {
                out.push((punkt(w[0]), punkt(w[1])));
            }
        }
    }
    out
}

fn abstand(p: [f64; 2], q: [f64; 2]) -> f64 {
    (p[0] - q[0]).hypot(p[1] - q[1])
}

/// Anteil auf a–b, an dem es c–e echt schneidet.
fn schnitt(a: [f64; 2], b: [f64; 2], c: [f64; 2], e: [f64; 2]) -> Option<f64> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [e[0] - c[0], e[1] - c[1]];
    let nenner = r[0] * s[1] - r[1] * s[0];
    if nenner.abs() < 1e-12 {
        return None;
    }
    let q = [c[0] - a[0], c[1] - a[1]];
    let t = (q[0] * s[1] - q[1] * s[0]) / nenner;
    let u = (q[0] * r[1] - q[1] * r[0]) / nenner;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// Liegt `p` im Polygon (gerade/ungerade)?
fn innen(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut drin = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                drin = !drin;
            }
        }
    }
    drin
}

/// Kleinster Abstand von `p` zum Rand des Polygons.
fn rand_abstand(poly: &[[f64; 2]], p: [f64; 2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let d = [b[0] - a[0], b[1] - a[1]];
            let l2 = d[0] * d[0] + d[1] * d[1];
            let t = if l2 > 0.0 {
                (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            abstand(p, [a[0] + t * d[0], a[1] + t * d[1]])
        })
        .fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rechteck(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    fn laenge(s: &[([f64; 2], [f64; 2])]) -> f64 {
        s.iter().map(|(a, b)| abstand(*a, *b)).sum()
    }

    /// Ohne Nachbarn ist der ganze Rand frei.
    #[test]
    fn allein() {
        let a = rechteck(0.0, 0.0, 8000.0, 10000.0);
        assert_eq!(freie_strecken(&a, &[]).len(), 4);
        let weit = rechteck(9000.0, 0.0, 17000.0, 10000.0);
        assert!((laenge(&freie_strecken(&a, &[&weit])) - 36000.0).abs() < 1e-6);
    }

    /// Wand an Wand, bündig und überlappend: Die Trennwand fehlt, dazu je
    /// Ende der Abstand.
    #[test]
    fn wand_an_wand() {
        let a = rechteck(0.0, 0.0, 8000.0, 10000.0);
        let b = rechteck(8000.0, 0.0, 16000.0, 10000.0);
        let s = freie_strecken(&a, &[&b]);
        let soll = 36000.0 - 10000.0 - 2.0 * AN_DER_WAND;
        assert!((laenge(&s) - soll).abs() < 1e-6, "{}", laenge(&s));
        assert!(
            s.iter()
                .all(|(p, q)| p[0] < 8000.0 - AN_DER_WAND + 1e-9
                    && q[0] < 8000.0 - AN_DER_WAND + 1e-9)
        );
        // Platten mit Überstand über die Wand
        let a = rechteck(-315.0, -315.0, 8315.0, 10315.0);
        let b = rechteck(7685.0, -315.0, 16315.0, 10315.0);
        let s = freie_strecken(&a, &[&b]);
        assert!(s.iter().all(|(p, q)| p[0] <= 7685.0 && q[0] <= 7685.0));
        assert!(laenge(&s) > 2.0 * 7900.0 + 10630.0 - 1.0);
    }

    /// Eine kürzere Garage nimmt nur ihr Stück der Wand.
    #[test]
    fn garage() {
        let a = rechteck(0.0, 0.0, 8000.0, 10000.0);
        let g = rechteck(8000.0, 0.0, 11000.0, 3000.0);
        let s = freie_strecken(&a, &[&g]);
        let soll = 36000.0 - 3000.0 - 2.0 * AN_DER_WAND;
        assert!((laenge(&s) - soll).abs() < 1e-6, "{}", laenge(&s));
    }
}

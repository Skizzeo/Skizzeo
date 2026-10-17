//! Freier Plattenrand (H-245a, VOB): An einer Haustrennwand liegt der
//! Plattenrand im oder am Umriss des Nachbarhauses. Das Gelände dort ist
//! das des Nachbarn, und Frost greift dort nicht an. Höhen am Plattenrand
//! (tiefster Punkt für die Einbindetiefe, höchster für die Lage) zählen
//! deshalb nur am freien Rand.

/// Unter diesem Abstand (mm) liegt der Plattenrand am Umriss eines anderen
/// Hauses: Wand an Wand, samt dem Übergang zwischen den Platten. Der reicht
/// bis 3 × `gelaende_eben::FUGE` über die Sohle des Nachbarn hinaus (Fuge
/// vor seiner Platte, Streifen von 2 × FUGE); dazu eine Fuge Spiel.
pub(super) const AN_DER_WAND: f64 = 4.0 * sk_gelaende::pruefen::GLEICHE_STELLE * 1000.0;

/// Die Strecken des geschlossenen Umrisses `ring` (mm), die weder in einem
/// der Umrisse `andere` liegen noch näher als [`AN_DER_WAND`] an ihnen.
pub(super) fn freie_strecken(
    ring: &[[f64; 2]],
    andere: &[&[[f64; 2]]],
) -> Vec<([f64; 2], [f64; 2])> {
    stuecke(ring, andere)
        .into_iter()
        .filter(|s| s.3)
        .map(|(i, t0, t1, _)| (auf(ring, i, t0), auf(ring, i, t1)))
        .collect()
}

/// Die Streifen innerhalb von `ring` unter den Strecken, die nicht frei
/// sind ([`freie_strecken`]), also unter der Haustrennwand: je Folge
/// zusammenhängender Strecken ein Umriss, so breit wie der Ring bis
/// `innen` (Frostschürze), ohne `innen` [`AN_DER_WAND`]; an Ecken auf der
/// Winkelhalbierenden, an den Enden senkrecht. Gegen den Uhrzeigersinn;
/// leer ohne Haustrennwand, ohne freie Strecke und für einen Streifen,
/// der sich selbst schneiden würde.
pub(super) fn unter_der_trennwand(
    ring: &[[f64; 2]],
    andere: &[&[[f64; 2]]],
    innen: &[[f64; 2]],
) -> Vec<Vec<[f64; 2]>> {
    let mut st = stuecke(ring, andere);
    let Some(erstes) = st.iter().position(|s| s.3) else {
        return Vec::new();
    };
    if st.iter().all(|s| s.3) {
        return Vec::new();
    }
    st.rotate_left(erstes);
    let breite = if innen.len() >= 3 {
        st.iter()
            .filter(|s| !s.3)
            .map(|s| rand_abstand(innen, auf(ring, s.0, 0.5)))
            .fold(f64::INFINITY, f64::min)
    } else {
        AN_DER_WAND
    };
    let n = ring.len();
    let links = (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - a[1] * b[0]
        })
        .sum::<f64>()
        > 0.0;
    // nach innen gerichtete Einheitsnormale der Kante i
    let normale = |i: usize| {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx.hypot(dy);
        if links {
            [-dy / l, dx / l]
        } else {
            [dy / l, -dx / l]
        }
    };
    let versetzt = |p: [f64; 2], v: [f64; 2]| [p[0] + breite * v[0], p[1] + breite * v[1]];
    let mut out = Vec::new();
    // Folgen nicht freier Stücke; die erste ist frei, also endet jede Folge
    for folge in st.split(|s| s.3).filter(|f| !f.is_empty()) {
        let (erst, letzt) = (folge[0], folge[folge.len() - 1]);
        let mut aussen = vec![auf(ring, erst.0, erst.1)];
        let mut drin = vec![versetzt(aussen[0], normale(erst.0))];
        for w in folge.windows(2) {
            let p = auf(ring, w[0].0, w[0].2);
            aussen.push(p);
            if w[0].0 == w[1].0 {
                continue;
            }
            let (u, v) = (normale(w[0].0), normale(w[1].0));
            let k = 1.0 + u[0] * v[0] + u[1] * v[1];
            if k > 0.2 {
                drin.push(versetzt(p, [(u[0] + v[0]) / k, (u[1] + v[1]) / k]));
            } else {
                drin.extend([versetzt(p, u), versetzt(p, v)]);
            }
        }
        let p = auf(ring, letzt.0, letzt.2);
        aussen.push(p);
        drin.push(versetzt(p, normale(letzt.0)));
        drin.reverse();
        aussen.extend(drin);
        aussen.dedup_by(|p, q| abstand(*p, *q) < 1e-6);
        if !links {
            aussen.reverse();
        }
        let v: Vec<sk_math::Vec3> = aussen
            .iter()
            .map(|p| sk_math::vec3(p[0], p[1], 0.0))
            .collect();
        if sk_math::polygon::is_simple(&v) {
            out.push(aussen);
        }
    }
    out
}

/// Punkt auf der Kante `i` von `ring` beim Anteil `t`.
fn auf(ring: &[[f64; 2]], i: usize, t: f64) -> [f64; 2] {
    let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
}

/// Der Umriss `ring` in Stücken (Kante, Anteil von, bis, frei), der Reihe
/// nach; frei ist ein Stück, das weder in einem der Umrisse `andere` liegt
/// noch näher als [`AN_DER_WAND`] an ihnen.
fn stuecke(ring: &[[f64; 2]], andere: &[&[[f64; 2]]]) -> Vec<(usize, f64, f64, bool)> {
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
            out.push((i, 0.0, 1.0, true));
            continue;
        }
        let punkt = |t: f64| [a[0] + t * d[0], a[1] + t * d[1]];
        // Teilungen, wo sich die Lage zu den anderen ändern kann: an
        // Schnitten mit ihren Kanten und dort, wo der Abstand zu ihnen genau
        // `tol` ist (Kanten um `tol` versetzt, Kreise um ihre Ecken). Auch
        // eine Ecke knapp hinter dem Ende der Strecke teilt so (VOB B-245b:
        // Außenflächen 1 bis 19 mm auseinander).
        let mut ts = vec![0.0, 1.0];
        for p in &nah {
            for j in 0..p.len() {
                let (c, e) = (p[j], p[(j + 1) % p.len()]);
                ts.extend(schnitt(a, b, c, e));
                let k = [e[0] - c[0], e[1] - c[1]];
                let kl = k[0].hypot(k[1]);
                if kl > 1e-9 {
                    let nv = [-k[1] / kl * tol, k[0] / kl * tol];
                    for s in [1.0, -1.0] {
                        let v = |q: [f64; 2]| [q[0] + s * nv[0], q[1] + s * nv[1]];
                        ts.extend(schnitt(a, b, v(c), v(e)));
                    }
                }
                // |a + t·d − c| = tol
                let w = [a[0] - c[0], a[1] - c[1]];
                let (qa, qb, qc) = (
                    len * len,
                    2.0 * (w[0] * d[0] + w[1] * d[1]),
                    w[0] * w[0] + w[1] * w[1] - tol * tol,
                );
                let disk = qb * qb - 4.0 * qa * qc;
                if disk > 0.0 {
                    let r = disk.sqrt();
                    ts.extend(
                        [(-qb - r) / (2.0 * qa), (-qb + r) / (2.0 * qa)]
                            .into_iter()
                            .filter(|t| (0.0..=1.0).contains(t)),
                    );
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
            out.push((i, w[0], w[1], frei));
        }
    }
    out
}

/// Stehen die Umrisse `a` und `b` ineinander (H-245-geo-a)? Ja, wenn eine
/// Ecke oder ein Stück des einen Randes tiefer als [`AN_DER_WAND`] im anderen
/// Umriss liegt; die Ränder werden an ihren Schnitten geteilt und je Stück die
/// Mitte geprüft. Aneinander (Wand an Wand) ist nicht ineinander.
pub(super) fn ineinander(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    let tief = |q: &[[f64; 2]], m: [f64; 2]| innen(q, m) && rand_abstand(q, m) > AN_DER_WAND;
    [(a, b), (b, a)].into_iter().any(|(p, q)| {
        let n = p.len();
        (0..n).any(|i| {
            let (s, e) = (p[i], p[(i + 1) % n]);
            let mut ts = vec![0.0, 1.0];
            for j in 0..q.len() {
                ts.extend(schnitt(s, e, q[j], q[(j + 1) % q.len()]));
            }
            ts.sort_by(f64::total_cmp);
            let punkt = |t: f64| [s[0] + t * (e[0] - s[0]), s[1] + t * (e[1] - s[1])];
            tief(q, s) || ts.windows(2).any(|w| tief(q, punkt((w[0] + w[1]) / 2.0)))
        })
    })
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

    /// Ineinander erst, wenn sich die Wände überdecken: aneinander, mit
    /// Spalt oder nur in der Toleranz nicht; Streifen, Ecke, ganz innen und
    /// über Kreuz ja, in beiden Richtungen.
    #[test]
    fn ineinander_erst_bei_ueberdeckung() {
        let a = rechteck(0.0, 0.0, 8000.0, 10000.0);
        let nein = [
            rechteck(8000.0, 0.0, 16000.0, 10000.0),
            rechteck(8100.0, 0.0, 16000.0, 10000.0),
            rechteck(8000.0 - AN_DER_WAND, 0.0, 16000.0, 10000.0),
            rechteck(20000.0, 0.0, 26000.0, 10000.0),
        ];
        for b in &nein {
            assert!(!ineinander(&a, b) && !ineinander(b, &a), "{b:?}");
        }
        let ja = [
            rechteck(7800.0, 0.0, 16000.0, 10000.0),
            rechteck(7000.0, 9000.0, 12000.0, 14000.0),
            rechteck(1000.0, 1000.0, 3000.0, 3000.0),
            rechteck(3000.0, -2000.0, 5000.0, 12000.0),
        ];
        for b in &ja {
            assert!(ineinander(&a, b) && ineinander(b, &a), "{b:?}");
        }
    }

    /// Außenflächen knapp auseinander (VOB B-245b): Auch die Enden der
    /// Seiten nahe dem Nachbarn sind nicht frei, wie Wand an Wand.
    #[test]
    fn knapp_daneben() {
        let a = rechteck(-315.0, -315.0, 8315.0, 10315.0);
        for g in [0.0, 1.0, 5.0, 10.0, 19.0, 30.0] {
            let b = rechteck(8315.0 + g, -315.0, 16945.0 + g, 10315.0);
            let s = freie_strecken(&a, &[&b]);
            let ende = 8315.0 + g - AN_DER_WAND;
            assert!(
                s.iter()
                    .all(|(p, q)| p[0] <= ende + 1e-6 && q[0] <= ende + 1e-6),
                "{g}: {s:?}"
            );
            // unten und oben bis zum Ende, links ganz, rechts nichts
            let soll = 2.0 * (ende + 315.0) + 10630.0;
            assert!((laenge(&s) - soll).abs() < 1e-6, "{g}: {}", laenge(&s));
        }
    }

    fn flaeche(q: &[[f64; 2]]) -> f64 {
        (0..q.len())
            .map(|i| {
                let (a, b) = (q[i], q[(i + 1) % q.len()]);
                a[0] * b[1] - a[1] * b[0]
            })
            .sum::<f64>()
            / 2.0
    }

    /// Unter der Haustrennwand ein Streifen so breit wie die Schürze, an den
    /// Ecken bis zu den Stücken der Nachbarseiten, gegen den Uhrzeigersinn;
    /// allein keiner. Mit der Garage nur ihr Stück, die Enden senkrecht.
    #[test]
    fn streifen_unter_der_wand() {
        let a = rechteck(0.0, 0.0, 8000.0, 10000.0);
        let innen = rechteck(300.0, 300.0, 7700.0, 9700.0);
        assert!(unter_der_trennwand(&a, &[], &innen).is_empty());
        let b = rechteck(8000.0, 0.0, 16000.0, 10000.0);
        let s = unter_der_trennwand(&a, &[&b], &innen);
        assert_eq!(s.len(), 1);
        // an der Wand bis zur Schürze innen, dazu unten und oben je das
        // Stück AN_DER_WAND × 300
        let d = AN_DER_WAND;
        let soll = 300.0 * 9400.0 + 2.0 * d * 300.0;
        assert!((flaeche(&s[0]) - soll).abs() < 1e-3, "{}", flaeche(&s[0]));
        // im Uhrzeigersinn derselbe Streifen, wieder links herum
        let mut r = a.clone();
        r.reverse();
        let t = unter_der_trennwand(&r, &[&b], &innen);
        assert!((flaeche(&t[0]) - soll).abs() < 1e-3, "{}", flaeche(&t[0]));
        // Garage: bis AN_DER_WAND über ihr Ende, dort senkrecht; ohne
        // Schürze AN_DER_WAND breit
        let g = rechteck(8000.0, 0.0, 11000.0, 3000.0);
        let s = unter_der_trennwand(&a, &[&g], &innen);
        assert_eq!(s.len(), 1);
        let soll = 300.0 * (3000.0 + d - 300.0) + d * 300.0;
        assert!((flaeche(&s[0]) - soll).abs() < 1e-3, "{}", flaeche(&s[0]));
        let s = unter_der_trennwand(&a, &[&g], &[]);
        assert_eq!(s.len(), 1);
        assert!((flaeche(&s[0]) - d * (3000.0 + d)).abs() < 1e-3);
        assert!(s[0].iter().all(|p| p[0] >= 8000.0 - d - 1e-6));
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

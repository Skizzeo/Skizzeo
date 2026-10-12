//! Ringe des Raumumrisses (Umriss oder Loch, der Raum liegt links der
//! Kanten): je Kante nach innen versetzt um eine eigene Dicke, mit den
//! Längen der versetzten Kanten und der Fläche, die sie umschließen.

use sk_math::polygon::signed_area;
use sk_math::{vec3, Vec3};

/// Ein Ring nach dem Versatz.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Versatz {
    /// Eckpunkte; wo zwei Kanten in einer Flucht verschieden weit versetzt
    /// sind, stehen beide Punkte der Stufe.
    pub punkte: Vec<Vec3>,
    /// Länge jeder versetzten Kante (mm), nie negativ.
    pub laengen: Vec<f64>,
    /// Anfang und Ende jeder versetzten Kante.
    pub enden: Vec<(Vec3, Vec3)>,
}

impl Versatz {
    /// Fläche mit Vorzeichen: Umriss positiv, Loch negativ (mm²).
    pub fn flaeche(&self) -> f64 {
        signed_area(&self.punkte)
    }

    /// Umfang (mm).
    pub fn umfang(&self) -> f64 {
        self.laengen.iter().sum()
    }
}

fn kreuz(a: Vec3, b: Vec3) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Richtung (Einheitsvektor) und linke Normale der Kante von `a` nach `b`.
pub(crate) fn richtung(a: Vec3, b: Vec3) -> (Vec3, Vec3) {
    let d = vec3(b.x - a.x, b.y - a.y, 0.0);
    let l = d.length();
    if l < 1e-9 {
        return (vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 0.0));
    }
    let u = d * (1.0 / l);
    (u, vec3(-u.y, u.x, 0.0))
}

/// Schnittpunkt der Geraden `p + t u` und `q + s v`; `None`, wenn sie
/// (fast) parallel laufen.
fn schnitt(p: Vec3, u: Vec3, q: Vec3, v: Vec3) -> Option<Vec3> {
    let c = kreuz(u, v);
    if c.abs() < 1e-9 {
        return None;
    }
    let t = kreuz(q - p, v) / c;
    Some(p + u * t)
}

/// So weit (Vielfaches des größeren Versatzes) darf die Gehrung von der
/// Ecke liegen. Eine spitze Ecke von 10° gehrt bei gleichem Versatz
/// 11,5-fach; weiter draußen laufen die Kanten fast in einer Flucht, und
/// der Schnitt gäbe eine Spitze aus dem Raum (Geometrie #112).
const GEHRUNG_MAX: f64 = 16.0;

/// Gehrung der versetzten Kanten `(p, u)` und `(q, v)` an der Ecke `ecke`
/// mit den Versätzen `d`; `None` wie in einer Flucht (dann steht eine
/// Stufe), wenn sie parallel laufen oder ihr Schnitt weiter als
/// [`GEHRUNG_MAX`] von der Ecke liegt.
fn gehrung(p: Vec3, u: Vec3, q: Vec3, v: Vec3, ecke: Vec3, d: [f64; 2]) -> Option<Vec3> {
    let s = schnitt(p, u, q, v)?;
    let weit = GEHRUNG_MAX * d[0].abs().max(d[1].abs());
    ((s - ecke).length() <= weit).then_some(s)
}

/// Versetzt den Ring `punkte` (Anfang jeder Kante, geschlossen) je Kante um
/// `d[k]` nach links, also in den Raum.
pub(crate) fn versetzt(punkte: &[Vec3], d: &[f64]) -> Versatz {
    let n = punkte.len();
    let flach = |p: Vec3| vec3(p.x, p.y, 0.0);
    let kanten: Vec<(Vec3, Vec3, Vec3)> = (0..n)
        .map(|k| {
            let (a, b) = (flach(punkte[k]), flach(punkte[(k + 1) % n]));
            let (u, nl) = richtung(a, b);
            (a + nl * d[k], u, b + nl * d[k])
        })
        .collect();
    let mut out = Versatz {
        punkte: Vec::with_capacity(2 * n),
        laengen: Vec::with_capacity(n),
        enden: Vec::with_capacity(n),
    };
    for k in 0..n {
        let (q, u, ende) = kanten[k];
        let (v, w) = ((k + n - 1) % n, (k + 1) % n);
        let (qv, uv, _) = kanten[v];
        let (qn, un, _) = kanten[w];
        let s = gehrung(qv, uv, q, u, flach(punkte[k]), [d[v], d[k]]).unwrap_or(q);
        let e = gehrung(q, u, qn, un, flach(punkte[w]), [d[k], d[w]]).unwrap_or(ende);
        out.laengen.push((e - s).dot(u).max(0.0));
        out.enden.push((s, e));
        if out.punkte.last().is_none_or(|l| (*l - s).length() > 1e-6) {
            out.punkte.push(s);
        }
        out.punkte.push(e);
    }
    if out.punkte.len() > 1 && (out.punkte[0] - out.punkte[out.punkte.len() - 1]).length() < 1e-6 {
        out.punkte.pop();
    }
    out
}

/// Länge der Kante (mm, waagerecht).
pub(crate) fn laenge(a: Vec3, b: Vec3) -> f64 {
    vec3(b.x - a.x, b.y - a.y, 0.0).length()
}

/// Knickt der Ring an der Ecke vor Kante `k` nach rechts (der Raum hat
/// dort mehr als 180°, die Ecke springt in den Raum vor)? Kanten in einer
/// Flucht knicken nicht.
pub(crate) fn springt_vor(punkte: &[Vec3], k: usize) -> bool {
    let n = punkte.len();
    let (a, b, c) = (punkte[(k + n - 1) % n], punkte[k], punkte[(k + 1) % n]);
    let (u, _) = richtung(a, b);
    let (v, _) = richtung(b, c);
    kreuz(u, v) < -1e-6
}

/// Knickt der Ring an der Ecke vor Kante `k` nach links (Innenecke)?
pub(crate) fn innenecke(punkte: &[Vec3], k: usize) -> bool {
    let n = punkte.len();
    let (a, b, c) = (punkte[(k + n - 1) % n], punkte[k], punkte[(k + 1) % n]);
    let (u, _) = richtung(a, b);
    let (v, _) = richtung(b, c);
    kreuz(u, v) > 1e-6
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rechteck(w: f64, h: f64) -> Vec<Vec3> {
        vec![
            vec3(0.0, 0.0, 0.0),
            vec3(w, 0.0, 0.0),
            vec3(w, h, 0.0),
            vec3(0.0, h, 0.0),
        ]
    }

    #[test]
    fn rechteck_versetzt() {
        let r = rechteck(4000.0, 3000.0);
        let v = versetzt(&r, &[15.0; 4]);
        assert!((v.flaeche() - 3970.0 * 2970.0).abs() < 1e-6);
        assert_eq!(v.laengen, vec![3970.0, 2970.0, 3970.0, 2970.0]);
        // eine Seite mit Fliesen (15 + 10)
        let v = versetzt(&r, &[15.0, 25.0, 15.0, 15.0]);
        assert!((v.flaeche() - 3960.0 * 2970.0).abs() < 1e-6);
        assert!((v.laengen[0] - 3960.0).abs() < 1e-9);
    }

    #[test]
    fn stufe_in_der_flucht() {
        // Unterkante aus zwei Kanten in einer Flucht, verschieden versetzt
        let r = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(2000.0, 0.0, 0.0),
            vec3(4000.0, 0.0, 0.0),
            vec3(4000.0, 3000.0, 0.0),
            vec3(0.0, 3000.0, 0.0),
        ];
        let v = versetzt(&r, &[10.0, 30.0, 0.0, 0.0, 0.0]);
        assert_eq!(v.punkte.len(), 6);
        let soll = 4000.0 * 3000.0 - 2000.0 * 10.0 - 2000.0 * 30.0;
        assert!((v.flaeche() - soll).abs() < 1e-6);
        assert!((v.laengen[0] - 2000.0).abs() < 1e-9);
        assert!((v.laengen[1] - 2000.0).abs() < 1e-9);
        assert!(!springt_vor(&r, 1) && !innenecke(&r, 1));
    }

    #[test]
    fn loch_waechst() {
        // Pfeiler 300 × 300 im Uhrzeigersinn, Putz 15 rundum
        let p = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 300.0, 0.0),
            vec3(300.0, 300.0, 0.0),
            vec3(300.0, 0.0, 0.0),
        ];
        let v = versetzt(&p, &[15.0; 4]);
        assert!((v.flaeche() + 330.0 * 330.0).abs() < 1e-6);
        assert!((v.umfang() - 4.0 * 330.0).abs() < 1e-9);
        assert!((0..4).all(|k| springt_vor(&p, k)));
    }

    #[test]
    fn l_raum_ecken() {
        // L-förmig gegen den Uhrzeigersinn: Ecke bei (2000, 1000) springt vor
        let r = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(4000.0, 0.0, 0.0),
            vec3(4000.0, 1000.0, 0.0),
            vec3(2000.0, 1000.0, 0.0),
            vec3(2000.0, 3000.0, 0.0),
            vec3(0.0, 3000.0, 0.0),
        ];
        let vor: Vec<usize> = (0..6).filter(|&k| springt_vor(&r, k)).collect();
        assert_eq!(vor, vec![3]);
        let v = versetzt(&r, &[15.0; 6]);
        let soll = 3970.0 * 970.0 + 1970.0 * 2000.0;
        assert!((v.flaeche() - soll).abs() < 1e-6, "{}", v.flaeche());
    }

    /// Eine spitze Ecke von 10° gehrt noch (11,5-facher Versatz von der
    /// Ecke), erst fast in einer Flucht steht die Stufe.
    #[test]
    fn spitze_ecke_gehrt() {
        let a = 10f64.to_radians();
        let r = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(4000.0, 0.0, 0.0),
            vec3(4000.0, 4000.0 * a.tan(), 0.0),
        ];
        let v = versetzt(&r, &[15.0, 15.0, 15.0]);
        assert_eq!(v.punkte.len(), 3, "{:?}", v.punkte);
        let ecke = v.punkte[0];
        assert!((ecke.length() - 15.0 / (a / 2.0).sin()).abs() < 1e-6);
        assert!(
            (ecke.y / ecke.x - (a / 2.0).tan()).abs() < 1e-9,
            "auf der Winkelhalbierenden"
        );
    }

    /// Geometrie #112: Zwei Kanten fast in einer Flucht (Zwischenpunkt
    /// δ neben der Geraden), verschieden weit versetzt (Putz 15, mit
    /// Fliesen 25). Der Versatz bleibt am Raum: keine Spitze aus dem Raum,
    /// Fläche wie bei der Stufe in der Flucht (bis auf den Knick).
    #[test]
    fn fast_in_der_flucht_keine_spitze() {
        let stufe = |delta: f64| {
            let r = vec![
                vec3(0.0, 0.0, 0.0),
                vec3(2000.0, delta, 0.0),
                vec3(4000.0, 0.0, 0.0),
                vec3(4000.0, 3000.0, 0.0),
                vec3(0.0, 3000.0, 0.0),
            ];
            versetzt(&r, &[15.0, 25.0, 15.0, 15.0, 15.0])
        };
        let gerade = stufe(0.0);
        for delta in [1e-5, 1e-3, 0.1, 1.0] {
            let v = stufe(delta);
            for p in &v.punkte {
                assert!(
                    p.x > -1e-6 && p.x < 4000.0 + 1e-6 && p.y > -1e-6 && p.y < 3000.0 + 1e-6,
                    "δ {delta}: Punkt {p:?} außerhalb des Raums"
                );
            }
            let d = (v.flaeche() - gerade.flaeche()).abs();
            assert!(
                d < 4000.0 * delta + 1.0,
                "δ {delta}: Fläche um {d} mm² anders"
            );
            assert!(
                v.umfang() < gerade.umfang() + 1.0,
                "δ {delta}: Umfang {}",
                v.umfang()
            );
        }
    }

    /// Review #112 B2: 1 mm Knick auf 2 m mit verschiedenem Putz (15, mit
    /// Fliesen 30) stuft statt zu gehren; Längen und Fläche wie in der
    /// Flucht, ohne Kante der Länge null.
    #[test]
    fn knick_ein_millimeter_stuft() {
        let r = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(2000.0, 0.0, 0.0),
            vec3(4000.0, 1.0, 0.0),
            vec3(4000.0, 3000.0, 0.0),
            vec3(0.0, 3000.0, 0.0),
        ];
        let v = versetzt(&r, &[15.0, 30.0, 15.0, 15.0, 15.0]);
        for k in [0, 1] {
            assert!((v.laengen[k] - 1985.0).abs() < 1.0, "{:?}", v.laengen);
        }
        let a = v.flaeche() / 1e6;
        assert!((a - 11.760).abs() < 0.005, "{a} m²");
    }
}

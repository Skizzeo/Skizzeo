//! Körper eines Erweiterungsbauteils (Schrittplan E4): je Körper aus
//! `sk-szb` ein Prisma in Weltkoordinaten, dazu waagerechter und
//! senkrechter Schnitt mit Schnittflächen. Ein Körper ist immer ein ebener
//! Umriss (u, v), ausgezogen entlang w; daraus folgen Flächen, Kanten und
//! Schnitte ohne allgemeine Körperverschneidung.

use crate::solid::{edge_kind, material, Solid, Tri};
use sk_math::{polygon, vec3, Vec3};
use sk_szb::{Ergebnis, Koerper};

/// Lage eines Exemplars: Einfügepunkt, Drehung in Grad gegen den
/// Uhrzeigersinn, absolute Höhe des Einfügepunkts (UK Geschoss plus
/// `[hoehe] versatz`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lage {
    pub at: [f64; 2],
    pub rot: f64,
    pub z: f64,
}

impl Lage {
    /// Punkt in Bauteilkoordinaten nach Welt.
    pub fn welt(&self, p: [f64; 3]) -> Vec3 {
        let (s, c) = self.rot.to_radians().sin_cos();
        vec3(
            self.at[0] + p[0] * c - p[1] * s,
            self.at[1] + p[0] * s + p[1] * c,
            self.z + p[2],
        )
    }
}

/// Abbildung eines Körpers von (u, v, w) nach Welt: `p = a·q + t`.
struct Abb {
    t: Vec3,
    a: [Vec3; 3],
}

impl Abb {
    fn neu(k: &Koerper, lage: &Lage) -> Abb {
        let f = |u, v, w| lage.welt(k.punkt(u, v, w, 0.0));
        let t = f(0.0, 0.0, 0.0);
        Abb {
            t,
            a: [
                f(1.0, 0.0, 0.0) - t,
                f(0.0, 1.0, 0.0) - t,
                f(0.0, 0.0, 1.0) - t,
            ],
        }
    }

    fn p(&self, u: f64, v: f64, w: f64) -> Vec3 {
        self.t + self.a[0] * u + self.a[1] * v + self.a[2] * w
    }
}

/// Dreieck mit der Normalen aus seinen Ecken; `aussen` zeigt die
/// gewünschte Seite, ein entartetes fällt weg.
fn dreieck(s: &mut Solid, p: [Vec3; 3], aussen: Vec3) {
    let n = (p[1] - p[0]).cross(p[2] - p[0]);
    let l = n.length();
    if l.is_nan() || l <= 1e-9 {
        return;
    }
    let (p, n) = if n.dot(aussen) < 0.0 {
        ([p[0], p[2], p[1]], n * (-1.0 / l))
    } else {
        (p, n * (1.0 / l))
    };
    s.triangles.push(Tri {
        p,
        n,
        mat: s.mat,
        uv: [[0.0; 2]; 3],
        elem: s.elem,
        layer: s.layer,
    });
}

/// Umriss als Punkte in der Ebene (für die Zerlegung in Dreiecke).
fn eben(p: &[[f64; 2]]) -> Vec<Vec3> {
    p.iter().map(|q| vec3(q[0], q[1], 0.0)).collect()
}

/// Ein Körper als geschlossenes Prisma mit Kanten an Deckeln und Ecken.
fn prisma(s: &mut Solid, k: &Koerper, lage: &Lage) {
    let m = Abb::neu(k, lage);
    let u = &k.umriss;
    let n = u.len();
    if n < 3 {
        return;
    }
    let w_auf = m.a[2];
    let pts = eben(u);
    for t in polygon::triangulate(&pts) {
        let q = |i: usize, w: f64| m.p(u[t[i]][0], u[t[i]][1], w);
        dreieck(s, [q(0, k.bis), q(1, k.bis), q(2, k.bis)], w_auf);
        dreieck(s, [q(0, k.von), q(1, k.von), q(2, k.von)], w_auf * -1.0);
    }
    for i in 0..n {
        let (a, b) = (u[i], u[(i + 1) % n]);
        // rechts der Laufrichtung gegen den Uhrzeigersinn: außen
        let aus = m.a[0] * (b[1] - a[1]) - m.a[1] * (b[0] - a[0]);
        let p = |q: [f64; 2], w: f64| m.p(q[0], q[1], w);
        let (a0, b0, b1, a1) = (p(a, k.von), p(b, k.von), p(b, k.bis), p(a, k.bis));
        dreieck(s, [a0, b0, b1], aus);
        dreieck(s, [a0, b1, a1], aus);
        s.edge(a0, b0);
        s.edge(a1, b1);
        if !k.glatt {
            s.edge(a0, a1);
        }
    }
}

/// Höchstzahl der Dreiecke eines Exemplars (Review Nr. 11). Darüber zeigt
/// Skizzeo den umschließenden Quader, damit ein Bauteil die Darstellung
/// nicht lähmt; Mengen rechnen weiter mit allen Körpern.
pub const MAX_DREIECKE: usize = 200_000;

/// Dreiecke, die [`solid`] für `e` erzeugt.
pub fn dreiecke(e: &Ergebnis) -> usize {
    e.koerper
        .iter()
        .map(|k| 4 * k.umriss.len().max(1) - 4)
        .sum()
}

/// `e` im Budget: unverändert, oder über [`MAX_DREIECKE`] ein Quader um
/// alle Körper (`true`).
pub fn begrenzt(mut e: Ergebnis) -> (Ergebnis, bool) {
    if dreiecke(&e) <= MAX_DREIECKE {
        return (e, false);
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for k in &e.koerper {
        for &[u, v] in &k.umriss {
            for w in [k.von, k.bis] {
                let p = k.punkt(u, v, w, 0.0);
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
        }
    }
    let erster = e.koerper.swap_remove(0);
    e.koerper = vec![Koerper {
        umriss: vec![
            [lo[0], lo[1]],
            [hi[0], lo[1]],
            [hi[0], hi[1]],
            [lo[0], hi[1]],
        ],
        von: lo[2],
        bis: hi[2],
        ebene: sk_szb::rechnen::Ebene::Xy,
        ursprung: [0.0; 3],
        drehung: 0.0,
        glatt: false,
        volumen: (hi[0] - lo[0]) * (hi[1] - lo[1]) * (hi[2] - lo[2]),
        ..erster
    }];
    (e, true)
}

/// Körper für 3D und Ansichten; `mat` nennt den Darstellungsschlüssel je
/// Baustoff.
pub fn solid(e: &Ergebnis, lage: &Lage, mat: &dyn Fn(&str) -> u16) -> Solid {
    let mut s = Solid {
        edge_kind: edge_kind::VIEW,
        ..Solid::default()
    };
    for k in &e.koerper {
        s.mat = mat(&k.baustoff);
        prisma(&mut s, k, lage);
    }
    s
}

/// Schnitt mit der Ebene durch `p0` mit Normale `n`: der Teil hinter der
/// Ebene (Gegenseite von `n`) samt Schnittflächen und Schnittkonturen.
/// Grundriss: `n` nach oben, `p0` in Schnitthöhe.
pub fn geschnitten(
    voll: &Solid,
    e: &Ergebnis,
    lage: &Lage,
    mat: &dyn Fn(&str) -> u16,
    p0: Vec3,
    n: Vec3,
) -> Solid {
    let mut s = voll.clipped(p0, n);
    s.edge_kind = edge_kind::CUT;
    for k in &e.koerper {
        s.mat = mat(&k.baustoff) | material::CUT;
        kappen(&mut s, k, lage, p0, n);
    }
    s.edge_kind = edge_kind::VIEW;
    s.mat = material::PLAIN;
    s
}

/// Schnittflächen eines Körpers mit der Ebene `(p0, n)`, nach `n`
/// gerichtet, mit Schnittkonturen.
fn kappen(s: &mut Solid, k: &Koerper, lage: &Lage, p0: Vec3, n: Vec3) {
    let m = Abb::neu(k, lage);
    // Ebene in (u, v, w): mu·u + mv·v + mw·w = d
    let (mu, mv, mw) = (n.dot(m.a[0]), n.dot(m.a[1]), n.dot(m.a[2]));
    let d = n.dot(p0 - m.t);
    let laenge = (m.a[0].length() * m.a[1].length() * m.a[2].length()).max(1e-12);
    let flaechen: Vec<Vec<Vec3>> = if mw.abs() > 1e-9 * laenge {
        // Die Ebene schneidet die Achse w: der Umriss, soweit sein Punkt auf
        // der Ebene zwischen von und bis liegt
        let w = |p: [f64; 2]| (d - mu * p[0] - mv * p[1]) / mw;
        let mut poly = k.umriss.clone();
        for (grenze, unten) in [(k.von, true), (k.bis, false)] {
            poly = halb(&poly, |p| {
                let x = w(p) - grenze;
                if unten {
                    x
                } else {
                    -x
                }
            });
        }
        if poly.len() < 3 {
            return;
        }
        vec![poly.iter().map(|p| m.p(p[0], p[1], w(*p))).collect()]
    } else {
        // Ebene längs w: Strecken der Geraden im Umriss, je ein Rechteck
        streifen(&k.umriss, mu, mv, d)
            .into_iter()
            .map(|(a, b)| {
                vec![
                    m.p(a[0], a[1], k.von),
                    m.p(b[0], b[1], k.von),
                    m.p(b[0], b[1], k.bis),
                    m.p(a[0], a[1], k.bis),
                ]
            })
            .collect()
    };
    for f in flaechen {
        flaeche(s, &f, n);
        for i in 0..f.len() {
            s.edge(f[i], f[(i + 1) % f.len()]);
        }
    }
}

/// Ebenes Vieleck im Raum mit Normale `n` als Dreiecke.
fn flaeche(s: &mut Solid, f: &[Vec3], n: Vec3) {
    // in die Ebene drehen: zwei Achsen senkrecht zu n
    let x = if n.z.abs() < 0.9 {
        vec3(0.0, 0.0, 1.0).cross(n)
    } else {
        vec3(1.0, 0.0, 0.0).cross(n)
    }
    .normalized();
    let y = n.cross(x);
    let o = f[0];
    let flach: Vec<Vec3> = f
        .iter()
        .map(|p| vec3((*p - o).dot(x), (*p - o).dot(y), 0.0))
        .collect();
    for t in polygon::triangulate(&flach) {
        dreieck(s, [f[t[0]], f[t[1]], f[t[2]]], n);
    }
}

/// Teil des Vielecks, in dem `seite` nicht negativ ist (eine Halbebene;
/// Sutherland–Hodgman).
fn halb(p: &[[f64; 2]], seite: impl Fn([f64; 2]) -> f64) -> Vec<[f64; 2]> {
    let mut out = Vec::with_capacity(p.len() + 2);
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        let (sa, sb) = (seite(a), seite(b));
        if sa >= 0.0 {
            out.push(a);
        }
        if (sa < 0.0) != (sb < 0.0) {
            let f = sa / (sa - sb);
            out.push([a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]);
        }
    }
    out
}

/// Strecken der Geraden `mu·u + mv·v = d` innerhalb des Umrisses.
fn streifen(p: &[[f64; 2]], mu: f64, mv: f64, d: f64) -> Vec<([f64; 2], [f64; 2])> {
    let l = (mu * mu + mv * mv).sqrt();
    if l.is_nan() || l <= 1e-12 {
        return Vec::new();
    }
    // Richtung der Geraden und Fußpunkt
    let (r, o) = ([-mv / l, mu / l], [mu * d / (l * l), mv * d / (l * l)]);
    let seite = |q: [f64; 2]| mu * q[0] + mv * q[1] - d;
    let mut t: Vec<f64> = Vec::new();
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        let (sa, sb) = (seite(a), seite(b));
        // halboffen, damit eine Ecke auf der Geraden einmal zählt
        if (sa < 0.0) != (sb < 0.0) {
            let f = sa / (sa - sb);
            let x = [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
            t.push((x[0] - o[0]) * r[0] + (x[1] - o[1]) * r[1]);
        }
    }
    t.sort_by(f64::total_cmp);
    t.chunks_exact(2)
        .filter(|c| c[1] - c[0] > 1e-9)
        .map(|c| {
            let at = |s: f64| [o[0] + r[0] * s, o[1] + r[1] * s];
            (at(c[0]), at(c[1]))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_szb::rechnen::Ebene;
    use sk_szb::{Bestand, Geschoss};

    const BEISPIELE: [&str; 5] = [
        include_str!("../../sk-szb/beispiele/werk.bodenplatte.szb"),
        include_str!("../../sk-szb/beispiele/werk.stabgelaender.szb"),
        include_str!("../../sk-szb/beispiele/werk.streifenfundament.szb"),
        include_str!("../../sk-szb/beispiele/werk.stuetze.szb"),
        include_str!("../../sk-szb/beispiele/werk.treppe.szb"),
    ];

    fn ergebnis(t: &str) -> Ergebnis {
        sk_szb::pruefen(t, &Bestand::werk(), &Geschoss::PROBE).ergebnis
    }

    /// Volumen eines geschlossenen Netzes (Divergenzsatz), mm³.
    fn volumen(s: &Solid) -> f64 {
        s.triangles
            .iter()
            .map(|t| t.p[0].dot(t.p[1].cross(t.p[2])) / 6.0)
            .sum()
    }

    const LAGEN: [Lage; 3] = [
        Lage {
            at: [0.0, 0.0],
            rot: 0.0,
            z: 0.0,
        },
        Lage {
            at: [1234.5, -678.0],
            rot: 37.0,
            z: 2855.0,
        },
        Lage {
            at: [-50.0, 9000.0],
            rot: -90.0,
            z: -800.0,
        },
    ];

    /// Jedes Beispiel ist geschlossen und nach außen gerichtet: das
    /// Netzvolumen gleicht der Summe der Körper (Zylinder als Vieleck).
    #[test]
    fn netz_volumen_wie_die_koerper() {
        for t in BEISPIELE {
            let e = ergebnis(t);
            assert!(!e.koerper.is_empty());
            for lage in LAGEN {
                let s = solid(&e, &lage, &|_| 1);
                let soll: f64 = e
                    .koerper
                    .iter()
                    .map(|k| {
                        let a = sk_szb::rechnen::flaeche2(&k.umriss).abs();
                        a * (k.bis - k.von)
                    })
                    .sum();
                let ist = volumen(&s);
                assert!(
                    (ist - soll).abs() <= 1e-6 * soll.max(1.0),
                    "{ist} != {soll}"
                );
                for t in &s.triangles {
                    assert!((t.n.length() - 1.0).abs() < 1e-9);
                }
            }
        }
    }

    /// Der Grundriss in halber Höhe der Stütze: Schnittfläche gleich dem
    /// Querschnitt, nach oben gerichtet, Schnittkonturen ringsum.
    #[test]
    fn grundriss_der_stuetze() {
        let e = ergebnis(BEISPIELE[3]);
        let lage = LAGEN[1];
        let voll = solid(&e, &lage, &|_| 7);
        let p0 = vec3(0.0, 0.0, lage.z + 1000.0);
        let s = geschnitten(&voll, &e, &lage, &|_| 7, p0, vec3(0.0, 0.0, 1.0));
        let kappe: Vec<&Tri> = s
            .triangles
            .iter()
            .filter(|t| t.mat == 7 | material::CUT)
            .collect();
        let flaeche: f64 = kappe
            .iter()
            .map(|t| (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length() / 2.0)
            .sum();
        let soll = sk_szb::rechnen::flaeche2(&e.koerper[0].umriss).abs();
        assert!((flaeche - soll).abs() < 1e-6 * soll, "{flaeche} {soll}");
        assert!(kappe.iter().all(|t| t.n.z > 0.999));
        assert!(s.edges.iter().any(|x| x.kind == edge_kind::CUT));
        assert!(s
            .triangles
            .iter()
            .all(|t| t.p.iter().all(|p| p.z <= p0.z + 1e-9)));
    }

    /// Senkrechter Schnitt durch die Treppe (Umriss in xz, längs y
    /// ausgezogen): die Schnittfläche ist der Umriss selbst.
    #[test]
    fn schnitt_durch_die_treppe() {
        let e = ergebnis(BEISPIELE[4]);
        let lage = LAGEN[0];
        let voll = solid(&e, &lage, &|_| 3);
        let lauf: Vec<&Koerper> = e.koerper.iter().filter(|k| k.ebene == Ebene::Xz).collect();
        assert!(!lauf.is_empty(), "Treppe hat Umrisse in xz");
        let k = lauf[0];
        let y = k.ursprung[1] + (k.von + k.bis) / 2.0;
        let s = geschnitten(
            &voll,
            &e,
            &lage,
            &|_| 3,
            vec3(0.0, y, 0.0),
            vec3(0.0, 1.0, 0.0),
        );
        let flaeche: f64 = s
            .triangles
            .iter()
            .filter(|t| t.mat == 3 | material::CUT)
            .map(|t| (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length() / 2.0)
            .sum();
        let soll: f64 = lauf
            .iter()
            .filter(|x| y > x.ursprung[1] + x.von && y < x.ursprung[1] + x.bis)
            .map(|x| sk_szb::rechnen::flaeche2(&x.umriss).abs())
            .sum();
        assert!(soll > 0.0);
        assert!((flaeche - soll).abs() < 1e-6 * soll, "{flaeche} {soll}");
    }

    #[test]
    fn streifen_im_umriss() {
        // U-Form: die Gerade y = 5 trifft zwei Schenkel
        let u = [
            [0.0, 0.0],
            [30.0, 0.0],
            [30.0, 10.0],
            [20.0, 10.0],
            [20.0, 3.0],
            [10.0, 3.0],
            [10.0, 10.0],
            [0.0, 10.0],
        ];
        let s = streifen(&u, 0.0, 1.0, 5.0);
        assert_eq!(s.len(), 2);
        let l: f64 = s.iter().map(|(a, b)| (b[0] - a[0]).abs()).sum();
        assert!((l - 20.0).abs() < 1e-9);
    }
    /// Über dem Dreiecksbudget zeigt ein Exemplar den Quader um alle
    /// Körper; darunter bleibt es unverändert, und die Zählung stimmt mit
    /// dem Netz überein.
    #[test]
    fn dreiecksbudget() {
        for t in BEISPIELE {
            let e = ergebnis(t);
            let s = solid(&e, &LAGEN[1], &|_| 1);
            assert_eq!(dreiecke(&e), s.triangles.len());
            assert_eq!(begrenzt(e.clone()), (e, false));
        }
        let mut e = ergebnis(BEISPIELE[1]);
        let stab = e.koerper[0].clone();
        e.koerper = (0..600)
            .map(|i| {
                let mut k = stab.clone();
                k.umriss = (0..96)
                    .map(|j| {
                        let a = j as f64 * std::f64::consts::TAU / 96.0;
                        [10.0 * a.cos(), 10.0 * a.sin()]
                    })
                    .collect();
                k.ursprung[0] += 100.0 * i as f64;
                k
            })
            .collect();
        assert!(dreiecke(&e) > MAX_DREIECKE);
        let (b, grob) = begrenzt(e.clone());
        assert!(grob);
        assert_eq!(b.koerper.len(), 1);
        let s = solid(&b, &LAGEN[0], &|_| 1);
        assert_eq!(s.triangles.len(), 12);
        let (lo, hi) = s.bounds().unwrap();
        let voll = solid(&e, &LAGEN[0], &|_| 1).bounds().unwrap();
        assert!((lo - voll.0).length() < 1e-6 && (hi - voll.1).length() < 1e-6);
    }
}

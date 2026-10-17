//! STL für den 3D-Druck: alle Netze als Dreiecke in Millimetern im
//! Maßstab des Drucks (1:50 … 1:500), z nach oben, die untere vordere
//! linke Ecke im Nullpunkt (so liegt das Modell auf der Druckplatte).
//! Binär (kompakt, liest jeder Slicer) oder als Text.
//!
//! Wasserdicht sind die Körper, wenn die Netze der Szene geschlossen sind;
//! das stellt die App sicher. Die Crate meldet, welche Teile im Druck
//! dünner werden als eine Düse breit ist ([`MIN_DICKE`]).

use crate::{kreuz, laenge, punkt, sub, Szene, P3};

/// Dünner als das (mm im Druck) druckt eine übliche 0,4-mm-Düse nicht
/// sicher: zwei Bahnen.
pub const MIN_DICKE: f64 = 0.8;

/// Was im Druck herauskommt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DruckBericht {
    pub dreiecke: usize,
    /// Größe im Druck (mm), x, y, z.
    pub groesse: P3,
    /// Teile, die dünner als [`MIN_DICKE`] werden: Pfad in der Gliederung
    /// und geschätzte Dicke im Druck (mm), dünnste zuerst.
    pub duenn: Vec<(String, f64)>,
}

/// Binäre STL im Maßstab 1:`massstab`.
pub fn binaer(s: &Szene, massstab: f64) -> (Vec<u8>, DruckBericht) {
    let (tris, bericht) = druck(s, massstab);
    let mut o = Vec::with_capacity(84 + tris.len() * 50);
    // Der Kopf darf nicht mit „solid“ beginnen, sonst halten manche Leser
    // die Datei für Text
    let mut kopf = format!("Skizzeo {}, mm, Massstab 1:{}", s.name, massstab).into_bytes();
    kopf.truncate(80);
    kopf.resize(80, b' ');
    o.extend(kopf);
    o.extend((tris.len() as u32).to_le_bytes());
    for (n, p) in &tris {
        for v in std::iter::once(n).chain(p) {
            for c in v {
                o.extend((*c as f32).to_le_bytes());
            }
        }
        o.extend(0u16.to_le_bytes());
    }
    (o, bericht)
}

/// STL als Text im Maßstab 1:`massstab`.
pub fn text(s: &Szene, massstab: f64) -> (String, DruckBericht) {
    use std::fmt::Write;
    let (tris, bericht) = druck(s, massstab);
    let name = crate::ohne_leerraum(&s.name);
    let mut o = format!("solid {name}\n");
    let v = |x: &P3| format!("{:.4} {:.4} {:.4}", x[0], x[1], x[2]);
    for (n, p) in &tris {
        let _ = writeln!(o, "  facet normal {}", v(n));
        o.push_str("    outer loop\n");
        for q in p {
            let _ = writeln!(o, "      vertex {}", v(q));
        }
        o.push_str("    endloop\n  endfacet\n");
    }
    let _ = writeln!(o, "endsolid {name}");
    (o, bericht)
}

/// Dreiecke im Druck (Normale, Ecken) und der Bericht.
fn druck(s: &Szene, massstab: f64) -> (Vec<(P3, [P3; 3])>, DruckBericht) {
    let k = 1.0 / massstab.max(1.0);
    let lo = s.rahmen().map_or([0.0; 3], |r| r.0);
    let mut tris = Vec::with_capacity(s.dreiecke());
    let mut duenn: Vec<(String, f64)> = Vec::new();
    s.je_knoten(|pfad, kn| {
        for n in &kn.netze {
            for d in &n.dreiecke {
                tris.push((d.n, d.p.map(|p| sub(p, lo).map(|c| c * k))));
            }
            if let Some(t) = dicke(n).map(|t| t * k) {
                if t < MIN_DICKE {
                    let name = pfad.join(" / ");
                    match duenn.iter_mut().find(|(p, _)| *p == name) {
                        Some((_, alt)) => *alt = alt.min(t),
                        None => duenn.push((name, t)),
                    }
                }
            }
        }
    });
    duenn.sort_by(|a, b| a.1.total_cmp(&b.1));
    let groesse = s
        .rahmen()
        .map_or([0.0; 3], |(lo, hi)| sub(hi, lo).map(|c| c * k));
    let bericht = DruckBericht {
        dreiecke: tris.len(),
        groesse,
        duenn,
    };
    (tris, bericht)
}

/// Geschätzte Dicke eines geschlossenen Netzes (mm): doppelter Rauminhalt
/// durch Oberfläche. Für eine Platte ist das ihre Dicke, für einen
/// Stab etwa sein halber Durchmesser. `None` ohne Rauminhalt (offen oder
/// flach).
pub fn dicke(n: &crate::Netz) -> Option<f64> {
    let (mut v, mut a) = (0.0, 0.0);
    for d in &n.dreiecke {
        let [p, q, r] = d.p;
        v += punkt(p, kreuz(q, r)) / 6.0;
        a += laenge(kreuz(sub(q, p), sub(r, p))) / 2.0;
    }
    (v > 1e-6 && a > 1e-9).then(|| 2.0 * v / a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe;

    #[test]
    fn binaer_kopf_anzahl_und_masse() {
        let s = probe::szene();
        let (b, r) = binaer(&s, 100.0);
        assert!(!b.starts_with(b"solid"));
        assert_eq!(u32::from_le_bytes(b[80..84].try_into().unwrap()), 36);
        assert_eq!(b.len(), 84 + 36 * 50);
        assert_eq!(r.dreiecke, 36);
        // Gelände bei −5 m bis Decke bei 2,5 m: 75 mm breit im Druck
        assert!((r.groesse[0] - 75.0).abs() < 1e-9, "{:?}", r.groesse);
        // Alles liegt ab dem Nullpunkt
        let f = |i: usize| f32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        for t in 0..36 {
            for e in 1..4 {
                for c in 0..3 {
                    assert!(f(84 + t * 50 + e * 12 + c * 4) >= 0.0);
                }
            }
        }
    }

    #[test]
    fn text_ist_gueltig_aufgebaut() {
        let (t, _) = text(&probe::szene(), 200.0);
        assert!(t.starts_with("solid Haus\n"));
        assert!(t.ends_with("endsolid Haus\n"));
        assert_eq!(t.matches("facet normal").count(), 36);
        assert_eq!(t.matches("vertex").count(), 108);
    }

    #[test]
    fn duenne_teile_im_kleinen_massstab() {
        let s = probe::szene();
        // Würfel 1 m: Dicke 2V/A = 2·1/6 m = 333 mm, bei 1:500 0,67 mm
        let (_, r) = binaer(&s, 500.0);
        let namen: Vec<&str> = r.duenn.iter().map(|d| d.0.as_str()).collect();
        assert_eq!(namen[0], "Gebäude 1 / Erdgeschoss / DE-001");
        assert_eq!(r.duenn.len(), 3);
        let (_, r) = binaer(&s, 100.0);
        assert!(r.duenn.is_empty());
    }

    #[test]
    fn dicke_einer_platte() {
        let mut n = probe::wuerfel([0.0; 3], 1.0, 0);
        // Platte 10 m × 10 m × 0,2 m
        for d in &mut n.dreiecke {
            for p in &mut d.p {
                *p = [p[0] * 10_000.0, p[1] * 10_000.0, p[2] * 200.0];
            }
        }
        let t = dicke(&n).unwrap();
        assert!((t - 192.3).abs() < 0.1, "{t}");
    }
}

//! OBJ mit MTL (Wavefront) für Visualisierungsprogramme: Meter, y oder z
//! oben, Ursprung am Projektnullpunkt oder im Lagesystem ([`Lage`]). Jeder
//! Knoten mit Netzen wird ein Objekt (`o`) mit seinem Pfad in der
//! Gliederung als Name und als Gruppen (`g`), jedes Material ein Eintrag
//! der MTL-Datei mit seiner Farbe. Punkte und Normalen gleicher Lage
//! stehen je Datei einmal.

use crate::{ohne_leerraum, Lage, Szene};
use std::collections::HashMap;
use std::fmt::Write;

/// Die OBJ-Datei und die MTL-Datei, auf die sie mit `mtl` (Dateiname
/// neben der OBJ-Datei) verweist.
pub fn schreiben(s: &Szene, lage: &Lage, mtl: &str) -> (String, String) {
    let namen = material_namen(s);
    let mut o = String::new();
    let _ = writeln!(o, "# Skizzeo, {}", s.name);
    let _ = writeln!(o, "# Einheit m, {} oben", achse(lage));
    let _ = writeln!(o, "mtllib {mtl}");
    let mut punkte: HashMap<[u64; 3], usize> = HashMap::new();
    let mut normalen: HashMap<[u64; 3], usize> = HashMap::new();
    let bits = |p: [f64; 3]| p.map(|c| (c + 0.0).to_bits());
    s.je_knoten(|pfad, k| {
        let name = pfad
            .iter()
            .map(|n| ohne_leerraum(n))
            .collect::<Vec<_>>()
            .join("/");
        let _ = writeln!(o, "o {name}");
        let _ = writeln!(
            o,
            "g {}",
            pfad.iter()
                .map(|n| ohne_leerraum(n))
                .collect::<Vec<_>>()
                .join(" ")
        );
        for n in &k.netze {
            let _ = writeln!(o, "usemtl {}", namen[n.material]);
            for d in &n.dreiecke {
                let mut f = [(0, 0); 3];
                for (j, p) in d.p.iter().enumerate() {
                    let q = rund(lage.punkt(*p), 1e4);
                    let neu = punkte.len() + 1;
                    let vi = *punkte.entry(bits(q)).or_insert_with(|| {
                        let _ = writeln!(o, "v {} {} {}", q[0], q[1], q[2]);
                        neu
                    });
                    let nn = rund(lage.richtung(d.n), 1e6);
                    let neu = normalen.len() + 1;
                    let ni = *normalen.entry(bits(nn)).or_insert_with(|| {
                        let _ = writeln!(o, "vn {} {} {}", nn[0], nn[1], nn[2]);
                        neu
                    });
                    f[j] = (vi, ni);
                }
                let _ = writeln!(
                    o,
                    "f {}//{} {}//{} {}//{}",
                    f[0].0, f[0].1, f[1].0, f[1].1, f[2].0, f[2].1
                );
            }
        }
    });
    let mut m = format!("# Skizzeo, {}\n", s.name);
    for (mat, name) in s.materialien.iter().zip(&namen) {
        let [r, g, b] = mat.farbe.map(|c| f64::from(c) / 255.0);
        let _ = write!(
            m,
            "\nnewmtl {name}\nKa 0 0 0\nKd {r:.4} {g:.4} {b:.4}\nKs 0 0 0\nd 1\nillum 1\n"
        );
    }
    (o, m)
}

fn achse(l: &Lage) -> &'static str {
    match l.oben {
        crate::Oben::Y => "y",
        crate::Oben::Z => "z",
    }
}

/// Auf `1/k` gerundet; −0 wird 0.
fn rund(p: [f64; 3], k: f64) -> [f64; 3] {
    p.map(|c| (c * k).round() / k + 0.0)
}

/// Eindeutige Namen der Materialien für OBJ und MTL (ohne Leerraum, gleiche
/// Namen mit Nummer).
fn material_namen(s: &Szene) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in &s.materialien {
        let base = ohne_leerraum(&m.name);
        let mut n = base.clone();
        let mut i = 2;
        while out.contains(&n) {
            n = format!("{base}_{i}");
            i += 1;
        }
        out.push(n);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{probe, Lagesystem, Material, Oben};

    #[test]
    fn objekte_punkte_und_materialien() {
        let s = probe::szene();
        let (o, m) = schreiben(&s, &Lage::default(), "haus.mtl");
        assert!(o.contains("mtllib haus.mtl\n"));
        assert!(o.contains("o Gebäude_1/Erdgeschoss/AW-001\n"));
        assert!(o.contains("g Gebäude_1 Erdgeschoss AW-001\n"));
        assert!(o.contains("usemtl Porenbeton\n"));
        // Drei Würfel: je 8 Punkte, 6 Normalen gemeinsam, 36 Flächen
        assert_eq!(o.lines().filter(|l| l.starts_with("v ")).count(), 24);
        assert_eq!(o.lines().filter(|l| l.starts_with("vn ")).count(), 6);
        assert_eq!(o.lines().filter(|l| l.starts_with("f ")).count(), 36);
        // y oben: Punkt (1 m, 0, 1 m) des Wandwürfels steht als (1, 1, 0)
        assert!(o.contains("\nv 1 1 0\n"));
        assert!(m.contains("newmtl Rasen\n"));
        assert!(m.contains("Kd 0.3529 0.5490 0.2353\n"));
    }

    #[test]
    fn verweise_gueltig() {
        let s = probe::szene();
        let (o, _) = schreiben(&s, &Lage::default(), "x.mtl");
        let (mut v, mut vn) = (0, 0);
        for l in o.lines() {
            if l.starts_with("v ") {
                v += 1;
            } else if l.starts_with("vn ") {
                vn += 1;
            } else if let Some(f) = l.strip_prefix("f ") {
                for e in f.split(' ') {
                    let (a, b) = e.split_once("//").unwrap();
                    let (a, b): (usize, usize) = (a.parse().unwrap(), b.parse().unwrap());
                    assert!(a >= 1 && a <= v && b >= 1 && b <= vn, "{l}");
                }
            }
        }
    }

    #[test]
    fn lagesystem_z_oben() {
        let s = probe::szene();
        let lage = Lage {
            oben: Oben::Z,
            lagesystem: Some(Lagesystem {
                ost: 600_000.0,
                nord: 5_500_000.0,
                hoehe: 300.0,
                drehung: 0.0,
            }),
        };
        let (o, _) = schreiben(&s, &lage, "x.mtl");
        assert!(o.contains("\nv 600001 5500000 301\n"), "{o}");
    }

    #[test]
    fn gleiche_materialnamen_bekommen_nummer() {
        let mut s = probe::szene();
        s.materialien.push(Material {
            name: "Rasen".into(),
            farbe: [0, 0, 0],
        });
        assert_eq!(material_namen(&s)[3], "Rasen_2");
    }
}

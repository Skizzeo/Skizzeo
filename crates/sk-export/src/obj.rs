//! OBJ mit MTL (Wavefront) für Visualisierungsprogramme: Meter, y oder z
//! oben, Ursprung am Projektnullpunkt oder im Lagesystem ([`Lage`]). Jeder
//! Knoten mit Netzen wird ein Objekt (`o`) mit seinem Pfad in der
//! Gliederung als Name und als Gruppen (`g`), jedes Material ein Eintrag
//! der MTL-Datei mit seiner Farbe, ein Material mit Textur dazu mit seinem
//! Bild (`map_Kd`, PNG neben der Datei). Punkte, Normalen und
//! Texturkoordinaten gleicher Lage stehen je Datei einmal.

use crate::{ohne_leerraum, Lage, Szene};
use std::collections::HashMap;
use std::fmt::Write;

/// Was neben der OBJ-Datei liegt: die MTL-Datei und die Bilder der
/// Texturen, je mit ihrem Dateinamen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Obj {
    pub obj: String,
    pub mtl: String,
    pub bilder: Vec<(String, Vec<u8>)>,
}

/// Die OBJ-Datei mit ihrer MTL-Datei `{stamm}.mtl` und den Bildern
/// `{stamm}_{Material}.png`, alle neben der OBJ-Datei.
pub fn schreiben(s: &Szene, lage: &Lage, stamm: &str) -> Obj {
    let namen = material_namen(s);
    let mtl = format!("{stamm}.mtl");
    let mut o = String::new();
    let _ = writeln!(o, "# Skizzeo, {}", s.name);
    let _ = writeln!(o, "# Einheit m, {} oben", achse(lage));
    let _ = writeln!(o, "mtllib {mtl}");
    let mut punkte: HashMap<[u64; 3], usize> = HashMap::new();
    let mut normalen: HashMap<[u64; 3], usize> = HashMap::new();
    let mut koord: HashMap<[u64; 2], usize> = HashMap::new();
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
            let textur = s
                .materialien
                .get(n.material)
                .and_then(|m| m.textur.as_ref());
            for d in &n.dreiecke {
                let mut f = [(0, 0, 0); 3];
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
                    let ti = textur.map_or(0, |t| {
                        let st = t.st(*p, d.n).map(|c| (c * 1e5).round() / 1e5 + 0.0);
                        let neu = koord.len() + 1;
                        *koord.entry(st.map(f64::to_bits)).or_insert_with(|| {
                            // OBJ zählt t nach oben, glTF nach unten
                            let _ = writeln!(o, "vt {} {}", st[0], 1.0 - st[1]);
                            neu
                        })
                    });
                    f[j] = (vi, ti, ni);
                }
                let e = |(v, t, n): (usize, usize, usize)| {
                    if t > 0 {
                        format!("{v}/{t}/{n}")
                    } else {
                        format!("{v}//{n}")
                    }
                };
                let _ = writeln!(o, "f {} {} {}", e(f[0]), e(f[1]), e(f[2]));
            }
        }
    });
    let mut m = format!("# Skizzeo, {}\n", s.name);
    let mut bilder = Vec::new();
    for (mat, name) in s.materialien.iter().zip(&namen) {
        let [r, g, b] = mat.farbe.map(|c| f64::from(c) / 255.0);
        let _ = write!(
            m,
            "\nnewmtl {name}\nKa 0 0 0\nKd {r:.4} {g:.4} {b:.4}\nKs 0 0 0\nd 1\nillum 1\n"
        );
        if let Some(t) = &mat.textur {
            let datei = format!("{}_{}.png", ohne_leerraum(stamm), dateiname(name));
            let _ = writeln!(m, "map_Kd {datei}");
            bilder.push((datei, t.png.clone()));
        }
    }
    Obj {
        obj: o,
        mtl: m,
        bilder,
    }
}

/// Name ohne Zeichen, die Windows in Dateinamen nicht erlaubt.
fn dateiname(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c => c,
        })
        .collect()
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
        let Obj { obj: o, mtl: m, .. } = schreiben(&s, &Lage::default(), "haus");
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
        let o = schreiben(&s, &Lage::default(), "x").obj;
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
        let o = schreiben(&s, &lage, "x").obj;
        assert!(o.contains("\nv 600001 5500000 301\n"), "{o}");
    }

    #[test]
    fn textur_mit_bild_und_koordinaten() {
        let mut s = probe::szene();
        s.materialien[0].name = "Ziegel: rot".into();
        s.materialien[0].textur = Some(crate::Textur {
            png: vec![1, 2, 3],
            breite: 1000.0,
            hoehe: 1000.0,
        });
        let r = schreiben(&s, &Lage::default(), "Mein Haus");
        assert!(
            r.mtl.contains("map_Kd Mein_Haus_Ziegel__rot.png\n"),
            "{}",
            r.mtl
        );
        assert_eq!(
            r.bilder,
            vec![("Mein_Haus_Ziegel__rot.png".to_string(), vec![1, 2, 3])]
        );
        // Wandwürfel mit Koordinaten, die anderen ohne
        assert!(r.obj.lines().any(|l| l.starts_with("vt ")));
        assert_eq!(
            r.obj
                .lines()
                .filter(|l| l.starts_with("f ") && l.contains("//"))
                .count(),
            24
        );
        assert_eq!(
            r.obj
                .lines()
                .filter(|l| l.starts_with("f ") && !l.contains("//"))
                .count(),
            12
        );
    }

    #[test]
    fn gleiche_materialnamen_bekommen_nummer() {
        let mut s = probe::szene();
        s.materialien.push(Material {
            name: "Rasen".into(),
            farbe: [0, 0, 0],
            textur: None,
        });
        assert_eq!(material_namen(&s)[3], "Rasen_2");
    }
}

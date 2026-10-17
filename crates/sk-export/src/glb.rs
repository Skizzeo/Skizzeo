//! GLB (glTF 2.0 binär, ISO/IEC 12113) für Twinmotion, Lumion und andere:
//! eine Datei mit Kopf (JSON) und Puffer. Die Gliederung der Szene wird
//! der Knotenbaum (Projekt › Gebäude › Geschoss › Bauteilart › Bauteil,
//! Gelände), sodass sich jede Ebene im Zielprogramm ein- und ausblenden
//! lässt. Jedes Material trägt seine Farbe (PBR, matt).
//!
//! glTF rechnet in Metern mit y nach oben. Die Punkte stehen als 32-Bit-
//! Zahlen relativ zum Projektnullpunkt; das Lagesystem trägt die Wurzel als
//! Verschiebung und Drehung (doppelt genau im JSON), damit Landeskoordinaten
//! keine Millimeter kosten.

use crate::json::{liste, objekt, text, zahl};
use crate::{Knoten, Lage, Netz, Oben, Szene};
use std::collections::HashMap;

const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;

/// Die GLB-Datei. Von `lage` gilt nur das Lagesystem; glTF hat immer y
/// oben.
pub fn schreiben(s: &Szene, lage: &Lage) -> Vec<u8> {
    let mut w = Schreiber::default();
    // Bilder zuerst: je Material mit Textur ein Bild im Puffer
    let mut bilder = Vec::new();
    let mut tex_von: Vec<Option<usize>> = Vec::new();
    for m in &s.materialien {
        w.kacheln
            .push(m.textur.as_ref().map(|t| (t.breite, t.hoehe)));
        tex_von.push(m.textur.as_ref().map(|t| {
            let sicht = w.sicht(t.png.clone(), None);
            bilder.push(objekt([
                ("bufferView", sicht.to_string()),
                ("mimeType", text("image/png")),
            ]));
            bilder.len() - 1
        }));
    }
    let lokal = Lage {
        oben: Oben::Y,
        lagesystem: None,
    };
    let mut kinder = Vec::new();
    for k in &s.knoten {
        kinder.push(w.knoten(k, &lokal));
    }
    let mut wurzel = vec![
        ("name", text(&s.name)),
        ("children", liste(kinder.iter().map(|i| i.to_string()))),
    ];
    if let Some(l) = lage.lagesystem {
        // Drehung um die Hochachse (y) wie im Modell um z, Ost nach x,
        // Nord nach −z
        let (s2, c2) = (l.drehung / 2.0).sin_cos();
        wurzel.push(("rotation", liste([0.0, s2, 0.0, c2].map(zahl))));
        wurzel.push(("translation", liste([l.ost, l.hoehe, -l.nord].map(zahl))));
    }
    w.knoten.push(objekt(wurzel));
    let wurzel = w.knoten.len() - 1;

    let materialien = s.materialien.iter().zip(&tex_von).map(|(m, tex)| {
        // Mit Bild trägt das Bild die Farbe
        let [r, g, b] = match tex {
            Some(_) => [1.0; 3],
            None => m.farbe.map(linear),
        };
        let mut pbr = vec![("baseColorFactor", liste([r, g, b, 1.0].map(zahl)))];
        if let Some(i) = tex {
            pbr.push(("baseColorTexture", objekt([("index", i.to_string())])));
        }
        pbr.push(("metallicFactor", zahl(0.0)));
        pbr.push(("roughnessFactor", zahl(0.9)));
        objekt([
            ("name", text(&m.name)),
            ("pbrMetallicRoughness", objekt(pbr)),
            ("doubleSided", "true".into()),
        ])
    });
    let mut kopf = vec![
        (
            "asset",
            objekt([
                ("version", text("2.0")),
                (
                    "generator",
                    text(concat!("Skizzeo ", env!("CARGO_PKG_VERSION"))),
                ),
            ]),
        ),
        ("scene", zahl(0.0)),
        (
            "scenes",
            liste([objekt([
                ("name", text(&s.name)),
                ("nodes", liste([wurzel.to_string()])),
            ])]),
        ),
        ("nodes", liste(w.knoten)),
    ];
    if !w.netze.is_empty() {
        kopf.push(("meshes", liste(w.netze)));
        kopf.push(("accessors", liste(w.zugriffe)));
    }
    if !bilder.is_empty() {
        // Kacheln wiederholen sich, weich gefiltert mit Mipmaps
        kopf.push((
            "samplers",
            liste([objekt([
                ("magFilter", "9729".into()),
                ("minFilter", "9987".into()),
                ("wrapS", "10497".into()),
                ("wrapT", "10497".into()),
            ])]),
        ));
        kopf.push(("images", liste(bilder.clone())));
        kopf.push((
            "textures",
            liste(
                (0..bilder.len())
                    .map(|i| objekt([("sampler", "0".into()), ("source", i.to_string())])),
            ),
        ));
    }
    if !w.sichten.is_empty() {
        kopf.push(("bufferViews", liste(w.sichten)));
        kopf.push((
            "buffers",
            liste([objekt([("byteLength", w.puffer.len().to_string())])]),
        ));
    }
    if !s.materialien.is_empty() {
        kopf.push(("materials", liste(materialien)));
    }
    datei(objekt(kopf).into_bytes(), w.puffer)
}

/// Kopf und Puffer zur GLB-Datei: 12 Byte Kopf, dann die Abschnitte JSON
/// (mit Leerzeichen auf 4 Byte) und BIN (mit Nullen auf 4 Byte).
fn datei(mut json: Vec<u8>, mut bin: Vec<u8>) -> Vec<u8> {
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let mit_bin = !bin.is_empty();
    let laenge = 12 + 8 + json.len() + if mit_bin { 8 + bin.len() } else { 0 };
    let mut o = Vec::with_capacity(laenge);
    o.extend(0x4654_6C67u32.to_le_bytes());
    o.extend(2u32.to_le_bytes());
    o.extend((laenge as u32).to_le_bytes());
    o.extend((json.len() as u32).to_le_bytes());
    o.extend(0x4E4F_534Au32.to_le_bytes());
    o.extend(json);
    if mit_bin {
        o.extend((bin.len() as u32).to_le_bytes());
        o.extend(0x004E_4942u32.to_le_bytes());
        o.extend(bin);
    }
    o
}

/// sRGB (0 … 255) als linearer Anteil, wie glTF die Grundfarbe erwartet.
fn linear(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    let l = if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    };
    (l * 1e6).round() / 1e6
}

#[derive(Default)]
struct Schreiber {
    /// Kachelgröße (mm) je Material mit Textur.
    kacheln: Vec<Option<(f64, f64)>>,
    knoten: Vec<String>,
    netze: Vec<String>,
    zugriffe: Vec<String>,
    sichten: Vec<String>,
    puffer: Vec<u8>,
}

impl Schreiber {
    /// Schreibt den Knoten mit seinen Kindern; gibt seinen Platz.
    fn knoten(&mut self, k: &Knoten, lage: &Lage) -> usize {
        let kinder: Vec<usize> = k.kinder.iter().map(|c| self.knoten(c, lage)).collect();
        let mut f = vec![("name", text(&k.name))];
        if !kinder.is_empty() {
            f.push(("children", liste(kinder.iter().map(|i| i.to_string()))));
        }
        if !k.netze.is_empty() {
            let prims: Vec<String> = k.netze.iter().map(|n| self.primitiv(n, lage)).collect();
            self.netze.push(objekt([
                ("name", text(&k.name)),
                ("primitives", liste(prims)),
            ]));
            f.push(("mesh", (self.netze.len() - 1).to_string()));
        }
        self.knoten.push(objekt(f));
        self.knoten.len() - 1
    }

    /// Ein Netz als Primitiv: Punkte und Normalen (je Lage und Richtung
    /// einmal), Dreiecke als Nummern.
    fn primitiv(&mut self, n: &Netz, lage: &Lage) -> String {
        let kachel = self.kacheln.get(n.material).copied().flatten();
        let mut ecken: HashMap<[u32; 8], u32> = HashMap::new();
        let mut pos: Vec<[f32; 3]> = Vec::new();
        let mut nor: Vec<[f32; 3]> = Vec::new();
        let mut uv: Vec<[f32; 2]> = Vec::new();
        let mut idx: Vec<u32> = Vec::with_capacity(n.dreiecke.len() * 3);
        for d in &n.dreiecke {
            let nn = lage.richtung(d.n).map(|c| c as f32 + 0.0);
            for p in d.p {
                let q = lage.punkt(p).map(|c| c as f32 + 0.0);
                let st = kachel.map_or([0.0; 2], |(b, h)| {
                    let [u, v] = crate::muster_uv(p, d.n);
                    [
                        (u / b.max(1.0)) as f32 + 0.0,
                        (-v / h.max(1.0)) as f32 + 0.0,
                    ]
                });
                let key = [
                    q[0].to_bits(),
                    q[1].to_bits(),
                    q[2].to_bits(),
                    nn[0].to_bits(),
                    nn[1].to_bits(),
                    nn[2].to_bits(),
                    st[0].to_bits(),
                    st[1].to_bits(),
                ];
                let i = *ecken.entry(key).or_insert_with(|| {
                    pos.push(q);
                    nor.push(nn);
                    uv.push(st);
                    (pos.len() - 1) as u32
                });
                idx.push(i);
            }
        }
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for p in &pos {
            for c in 0..3 {
                lo[c] = lo[c].min(p[c]);
                hi[c] = hi[c].max(p[c]);
            }
        }
        let a_pos = self.vec3(&pos, Some((lo, hi)));
        let a_nor = self.vec3(&nor, None);
        let mut attr = vec![
            ("POSITION", a_pos.to_string()),
            ("NORMAL", a_nor.to_string()),
        ];
        if kachel.is_some() {
            let bytes = uv.iter().flatten().flat_map(|c| c.to_le_bytes()).collect();
            let sicht = self.sicht(bytes, Some(ARRAY_BUFFER));
            self.zugriffe.push(objekt([
                ("bufferView", sicht.to_string()),
                ("componentType", FLOAT.to_string()),
                ("count", uv.len().to_string()),
                ("type", text("VEC2")),
            ]));
            attr.push(("TEXCOORD_0", (self.zugriffe.len() - 1).to_string()));
        }
        let sicht = self.sicht(
            idx.iter().flat_map(|i| i.to_le_bytes()).collect(),
            Some(ELEMENT_ARRAY_BUFFER),
        );
        self.zugriffe.push(objekt([
            ("bufferView", sicht.to_string()),
            ("componentType", UNSIGNED_INT.to_string()),
            ("count", idx.len().to_string()),
            ("type", text("SCALAR")),
        ]));
        let a_idx = self.zugriffe.len() - 1;
        objekt([
            ("attributes", objekt(attr)),
            ("indices", a_idx.to_string()),
            ("material", n.material.to_string()),
            ("mode", "4".into()),
        ])
    }

    /// Zugriff auf eine Reihe von Vektoren (mit Rahmen für die Punkte).
    fn vec3(&mut self, v: &[[f32; 3]], rahmen: Option<([f32; 3], [f32; 3])>) -> usize {
        let bytes = v.iter().flatten().flat_map(|c| c.to_le_bytes()).collect();
        let sicht = self.sicht(bytes, Some(ARRAY_BUFFER));
        let mut f = vec![
            ("bufferView", sicht.to_string()),
            ("componentType", FLOAT.to_string()),
            ("count", v.len().to_string()),
            ("type", text("VEC3")),
        ];
        if let Some((lo, hi)) = rahmen {
            f.push(("min", liste(lo.map(|c| zahl(f64::from(c))))));
            f.push(("max", liste(hi.map(|c| zahl(f64::from(c))))));
        }
        self.zugriffe.push(objekt(f));
        self.zugriffe.len() - 1
    }

    /// Hängt `bytes` an den Puffer (auf 4 Byte ausgerichtet) und legt eine
    /// Sicht darauf an; Bilder ohne Ziel.
    fn sicht(&mut self, bytes: Vec<u8>, ziel: Option<u32>) -> usize {
        while !self.puffer.len().is_multiple_of(4) {
            self.puffer.push(0);
        }
        let start = self.puffer.len();
        let len = bytes.len();
        self.puffer.extend(bytes);
        let mut f = vec![
            ("buffer", "0".into()),
            ("byteOffset", start.to_string()),
            ("byteLength", len.to_string()),
        ];
        if let Some(z) = ziel {
            f.push(("target", z.to_string()));
        }
        self.sichten.push(objekt(f));
        self.sichten.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{probe, Lagesystem};

    fn kopf(b: &[u8]) -> String {
        let n = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
        String::from_utf8(b[20..20 + n].to_vec()).unwrap()
    }

    #[test]
    fn aufbau_der_datei() {
        let b = schreiben(&probe::szene(), &Lage::default());
        assert_eq!(&b[0..4], b"glTF");
        assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 2);
        assert_eq!(
            u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize,
            b.len()
        );
        assert_eq!(&b[16..20], b"JSON");
        assert_eq!(b.len() % 4, 0);
        let j = kopf(&b);
        let bin_at = 20 + j.len();
        assert_eq!(&b[bin_at + 4..bin_at + 8], b"BIN\0");
        let bin_len = u32::from_le_bytes(b[bin_at..bin_at + 4].try_into().unwrap()) as usize;
        assert_eq!(bin_at + 8 + bin_len, b.len());
        assert!(j.contains(&format!("\"buffers\":[{{\"byteLength\":{bin_len}}}]")));
    }

    #[test]
    fn gliederung_als_knotenbaum() {
        let j = kopf(&schreiben(&probe::szene(), &Lage::default()));
        // Kinder vor Eltern: Gelände 0, AW 1, DE 2, EG 3, Gebäude 4, Wurzel 5
        assert!(
            j.contains("\"nodes\":[{\"name\":\"Gelände\",\"mesh\":0}"),
            "{j}"
        );
        assert!(j.contains("{\"name\":\"Erdgeschoss\",\"children\":[1,2]}"));
        assert!(j.contains("{\"name\":\"Gebäude 1\",\"children\":[3]}"));
        assert!(j.contains("{\"name\":\"Haus\",\"children\":[0,4]}"));
        assert!(j.contains("\"scenes\":[{\"name\":\"Haus\",\"nodes\":[5]}]"));
        assert!(j.contains("\"name\":\"Porenbeton\""));
        // Würfel: 6 Seiten × 4 Ecken (Normale je Seite)
        assert!(j.contains("\"count\":24,\"type\":\"VEC3\",\"min\""), "{j}");
    }

    #[test]
    fn punkte_in_metern_y_oben() {
        let j = kopf(&schreiben(&probe::szene(), &Lage::default()));
        // Wandwürfel 0 … 1 m: y oben, Nord nach −z
        assert!(j.contains("\"min\":[0,0,-1],\"max\":[1,1,0]"), "{j}");
    }

    #[test]
    fn lagesystem_an_der_wurzel() {
        let lage = Lage {
            oben: Oben::Y,
            lagesystem: Some(Lagesystem {
                ost: 600_000.5,
                nord: 5_500_000.25,
                hoehe: 300.0,
                drehung: 0.0,
            }),
        };
        let j = kopf(&schreiben(&probe::szene(), &lage));
        assert!(j.contains("\"rotation\":[0,0,0,1],\"translation\":[600000.5,300,-5500000.25]"));
        // Die Punkte bleiben klein
        assert!(j.contains("\"min\":[0,0,-1],\"max\":[1,1,0]"));
    }

    #[test]
    fn textur_als_bild_mit_koordinaten() {
        let mut s = probe::szene();
        s.materialien[0].textur = Some(crate::Textur {
            png: vec![0x89, b'P', b'N', b'G', 1, 2],
            breite: 1000.0,
            hoehe: 500.0,
        });
        let b = schreiben(&s, &Lage::default());
        let j = kopf(&b);
        assert!(
            j.contains("\"images\":[{\"bufferView\":0,\"mimeType\":\"image/png\"}]"),
            "{j}"
        );
        assert!(j.contains("\"textures\":[{\"sampler\":0,\"source\":0}]"));
        assert!(j.contains("\"baseColorTexture\":{\"index\":0}"));
        // Nur das Netz mit Bild trägt Koordinaten
        assert_eq!(j.matches("TEXCOORD_0").count(), 1);
        // Das Bild steht vorn im Puffer
        let bin = 20 + j.len() + 8;
        assert_eq!(&b[bin..bin + 4], &[0x89, b'P', b'N', b'G']);
        // Wandwürfel 1 m: u 0 … 1 Kachel, t 0 … −2 Kacheln
        assert!(j.contains("\"type\":\"VEC2\""));
    }

    #[test]
    fn leere_szene_ohne_puffer() {
        let s = Szene {
            name: "leer".into(),
            ..Szene::default()
        };
        let b = schreiben(&s, &Lage::default());
        assert_eq!(
            u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize,
            b.len()
        );
        assert!(!kopf(&b).contains("buffers"));
    }

    #[test]
    fn grundfarbe_linear() {
        assert_eq!(linear(0), 0.0);
        assert_eq!(linear(255), 1.0);
        assert!((linear(128) - 0.215861).abs() < 1e-6);
    }
}

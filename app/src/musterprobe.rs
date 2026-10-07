//! Bildprüfung B7 `--musterprobe <ordner>` (paket-7 §8, b7-kennwerte §1):
//! je Musterart eine Kachel aus dem Flächen-Shader (`gpu-<art>.png`) und
//! dieselbe aus [`proctex::sample`] an den Pixelmitten (`cpu-<art>.png`),
//! dazu `friesisch` (Klinker friesisch-bunt mit Flammung und Relief) mit
//! Familienkarte und Steinliste. Vergleich je Bildpunkt und Kanal auf
//! höchstens 2 Farbstufen; ein Rand von 1 px an Fugen- und Rillenkanten
//! zählt nicht. Exit-Code 0 bestanden, 1 nicht bestanden, 2 ohne GL.

use crate::camera::Camera;
use crate::pattern_view::{self, Input, Look, Stage};
use sk_math::vec3;
use sk_model::proctex::{self, Pattern, PatternPreset};
use sk_render::{EdgeLooks, MeshData};
use sk_ui::theme::Theme;
use std::path::Path;

/// Abweichung je Kanal, die noch als gleich gilt (Farbstufen von 255).
pub const TOL: u8 = 2;

/// Eine Probe: Dateiname, Vorlage, Größe (px), mm je px, Ausschnitt ab
/// (u, v) in mm.
pub struct Probe {
    pub art: &'static str,
    pub preset: &'static PatternPreset,
    pub size: (usize, usize),
    pub mm: f64,
    pub at: (f64, f64),
}

/// Die sieben Proben: je Art die erste Werksvorlage, dazu `friesisch`.
/// Klinker friesisch-bunt, Reibeputz und Sichtbeton in Größe, Maßstab und
/// Ausschnitt wie b7-kennwerte §1; die übrigen 512 × 512 px zu 1 mm.
pub fn probes() -> Vec<Probe> {
    let mut v = Vec::new();
    for (art, gen) in [
        ("mauerwerk", "masonry"),
        ("putz", "plaster"),
        ("sichtbeton", "concrete"),
        ("holzschalung", "timber"),
        ("platten", "tiles"),
        ("naturstein", "stone"),
    ] {
        if let Some(p) = proctex::presets()
            .iter()
            .find(|p| proctex::gen_word(&p.pattern) == gen)
        {
            let (mm, at) = match gen {
                "plaster" => (0.35, (1000.0, 400.0)),
                _ => (1.0, (1000.0, 400.0)),
            };
            v.push(Probe {
                art,
                preset: p,
                size: (512, 512),
                mm,
                at,
            });
        }
    }
    if let Some(p) = proctex::preset_named("Klinker friesisch-bunt") {
        v.push(Probe {
            art: "friesisch",
            preset: p,
            size: (1024, 868),
            mm: 1.464,
            at: (3000.0, 2000.0),
        });
    }
    v
}

/// (u, v) der Mitte des Bildpunkts (x, y); v zählt nach oben, Zeile 0 oben.
fn spot(p: &Probe, x: usize, y: usize) -> (f64, f64) {
    let (_, h) = p.size;
    (
        p.at.0 + (x as f64 + 0.5) * p.mm,
        p.at.1 + ((h - y) as f64 - 0.5) * p.mm,
    )
}

/// CPU-Kachel als RGBA8, Zeile 0 oben.
pub fn cpu_image(p: &Probe) -> Vec<u8> {
    let (w, h) = p.size;
    let mut px = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = spot(p, x, y);
            let c = proctex::sample(&p.preset.pattern, p.preset.base, u, v);
            px.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    px
}

/// Ergebnis des Vergleichs: größte Abweichung (Farbstufen) und Bildpunkte
/// außerhalb der Toleranz (ohne den Rand an Kanten).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diff {
    pub max: u8,
    pub outside: usize,
}

/// Vergleicht GPU- mit CPU-Kachel. Ein Bildpunkt über [`TOL`] zählt nicht,
/// wenn er einem der 8 Nachbarn der CPU-Kachel gleicht: dort liegt eine
/// Fugen- oder Rillenkante höchstens 1 px anders.
pub fn compare(cpu: &[u8], gpu: &[u8], (w, h): (usize, usize)) -> Diff {
    let at = |img: &[u8], x: usize, y: usize| {
        let i = 4 * (y * w + x);
        [img[i], img[i + 1], img[i + 2]]
    };
    let near = |a: [u8; 3], b: [u8; 3]| (0..3).all(|k| a[k].abs_diff(b[k]) <= TOL);
    let mut d = Diff { max: 0, outside: 0 };
    for y in 0..h {
        for x in 0..w {
            let (c, g) = (at(cpu, x, y), at(gpu, x, y));
            let m = (0..3).map(|k| c[k].abs_diff(g[k])).max().unwrap_or(0);
            d.max = d.max.max(m);
            if m <= TOL {
                continue;
            }
            let edge = (y.saturating_sub(1)..(y + 2).min(h)).any(|ny| {
                (x.saturating_sub(1)..(x + 2).min(w))
                    .any(|nx| (nx, ny) != (x, y) && near(at(cpu, nx, ny), g))
            });
            if !edge {
                d.outside += 1;
            }
        }
    }
    d
}

/// Wandstück der Probe: Ebene y = 0 von (u0, v0) über `w` × `h` mm.
fn panel(u0: f64, v0: f64, w: f64, h: f64) -> MeshData {
    let mut m = MeshData::default();
    let q = [
        vec3(u0, 0.0, v0),
        vec3(u0 + w, 0.0, v0),
        vec3(u0 + w, 0.0, v0 + h),
        vec3(u0, 0.0, v0 + h),
    ];
    for i in [0, 1, 2, 0, 2, 3] {
        let p = q[i].to_f32();
        m.faces
            .push([p[0], p[1], p[2], 0.0, -1.0, 0.0, 0.0, 0.0, 0.0]);
    }
    m
}

/// Szene der GPU-Kachel: Aussehen (Schlüssel 0), Netz und Blick.
pub fn gpu_scene(p: &Probe, t: &Theme) -> (sk_render::Looks, MeshData, sk_render::View) {
    // Wilder Verband: Tabelle jetzt rechnen, die Probe wartet nicht
    if let Pattern::Masonry { seed, .. } = &p.preset.pattern {
        let _ = proctex::bond_table(*seed);
    }
    let look = Look {
        pattern: Some(p.preset.pattern.clone()),
        base: p.preset.base,
    };
    let (w, h) = p.size;
    let input = Input {
        after: look.clone(),
        before: look,
        variants: Vec::new(),
        stage: Stage::Near,
        big: [0, 0, w as i32, h as i32],
        split: None,
        tiles: Vec::new(),
        ink: [0.0; 4],
        paper: [1.0; 3],
        px_per_mm: 1.0,
        model_edges: EdgeLooks::default(),
        drawing_edges: EdgeLooks::default(),
    };
    let looks = pattern_view::preview_looks(&input, t);
    let (wm, hm) = (w as f64 * p.mm, h as f64 * p.mm);
    let cam = Camera::parallel(
        vec3(p.at.0 + wm * 0.5, 0.0, p.at.1 + hm * 0.5),
        90f64.to_radians(),
        0.0,
        hm * 0.5,
    );
    let mut view = cam.view(w as u32, h as u32);
    view.patterns = sk_render::pattern_mode::COLORS;
    (looks, panel(p.at.0, p.at.1, wm, hm), view)
}

/// Familienkarte (int8, −1 = Fuge) als `.npy` für kennwerte.py und die
/// Steinliste (CSV); Flächenanteile über 10 m² (3,2 × 3,2 m, 5 mm Raster)
/// und die Kennwerte der Steine als Zeile für die Konsole.
pub fn families(p: &Probe) -> (Vec<u8>, String, String) {
    let pat = &p.preset.pattern;
    let (w, h) = p.size;
    let mut fam = Vec::with_capacity(w * h);
    let mut stones = std::collections::BTreeSet::new();
    for y in 0..h {
        for x in 0..w {
            let (u, v) = spot(p, x, y);
            match proctex::masonry_probe(pat, u, v) {
                Some((f, s)) => {
                    fam.push(f as i8 as u8);
                    stones.insert((s.row, s.start, s.head, s.core, s.geflammt));
                }
                None => fam.push(-1i8 as u8),
            }
        }
    }
    let header = format!("{{'descr': '|i1', 'fortran_order': False, 'shape': ({h}, {w}), }}");
    let mut npy = b"\x93NUMPY\x01\x00".to_vec();
    let pad = 64 - (10 + header.len() + 1) % 64;
    let hlen = (header.len() + pad % 64 + 1) as u16;
    npy.extend_from_slice(&hlen.to_le_bytes());
    npy.extend_from_slice(header.as_bytes());
    npy.extend(std::iter::repeat_n(b' ', pad % 64));
    npy.push(b'\n');
    npy.extend_from_slice(&fam);
    let mut csv = String::from("schicht;anfang;kopf;kernfarbe;geflammt\n");
    for (r, s, k, c, g) in &stones {
        csv.push_str(&format!("{r};{s};{};{c};{}\n", *k as u8, *g as u8));
    }
    // Flächenanteile: 10 m² ohne Bild
    let mut area = [0usize; 3];
    let n = (3200.0 / 5.0) as usize;
    for j in 0..n {
        for i in 0..n {
            let (u, v) = (p.at.0 + 2.5 + i as f64 * 5.0, p.at.1 + 2.5 + j as f64 * 5.0);
            if let Some((f, _)) = proctex::masonry_probe(pat, u, v) {
                area[f.min(2)] += 1;
            }
        }
    }
    let all = area.iter().sum::<usize>().max(1) as f64;
    let runners: Vec<_> = stones.iter().filter(|s| !s.2).collect();
    let flamed = runners.iter().filter(|s| s.4).count();
    let heads = stones.len() - runners.len();
    let heads_flamed = stones.iter().filter(|s| s.2 && s.4).count();
    let line = format!(
        "Fläche Rot {:.1} %, Braun-grau {:.1} %, Silbergrau {:.1} %; Flammung {:.2}; \
         Köpfe geflammt {heads_flamed}; Kopfanteil {:.3}",
        100.0 * area[0] as f64 / all,
        100.0 * area[1] as f64 / all,
        100.0 * area[2] as f64 / all,
        flamed as f64 / runners.len().max(1) as f64,
        heads as f64 / stones.len().max(1) as f64,
    );
    (npy, csv, line)
}

/// Führt alle Proben aus und schreibt die Bilder nach `dir`; je Art eine
/// Zeile auf die Konsole. `gpu` zeichnet eine Kachel. Rückgabe: Exit-Code
/// 0 (alle bestanden) oder 1.
pub fn run(
    dir: &Path,
    t: &Theme,
    mut gpu: impl FnMut(
        &sk_render::Looks,
        &MeshData,
        sk_render::View,
        (i32, i32),
    ) -> Result<Vec<u8>, String>,
) -> i32 {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("musterprobe: Ordner {}: {e}", dir.display());
        return 1;
    }
    let mut code = 0;
    for p in probes() {
        let (w, h) = p.size;
        let cpu = cpu_image(&p);
        let (looks, mesh, view) = gpu_scene(&p, t);
        let write = |name: String, bytes: &[u8]| {
            let path = dir.join(name);
            if let Err(e) = std::fs::write(&path, bytes) {
                eprintln!("musterprobe: {}: {e}", path.display());
            }
        };
        write(
            format!("cpu-{}.png", p.art),
            &sk_paint::encode_png(w as u32, h as u32, &cpu),
        );
        if p.art == "friesisch" {
            let (npy, csv, line) = families(&p);
            write("friesisch-familien.npy".into(), &npy);
            write("friesisch-steine.csv".into(), csv.as_bytes());
            println!("friesisch (Rechnung): {line}");
        }
        match gpu(&looks, &mesh, view, (w as i32, h as i32)) {
            Ok(img) => {
                write(
                    format!("gpu-{}.png", p.art),
                    &sk_paint::encode_png(w as u32, h as u32, &img),
                );
                let d = compare(&cpu, &img, p.size);
                let ok = d.outside == 0;
                println!(
                    "{}: {} – größte Abweichung {}, außerhalb {} Bildpunkte – {}",
                    p.art,
                    p.preset.name,
                    d.max,
                    d.outside,
                    if ok { "bestanden" } else { "NICHT bestanden" }
                );
                if !ok {
                    code = 1;
                }
            }
            Err(e) => {
                println!("{}: {} – GPU: {e} – NICHT bestanden", p.art, p.preset.name);
                code = 1;
            }
        }
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sieben Proben, jede Art einmal; Vergleich und Kantenrand.
    #[test]
    fn proben_und_vergleich() {
        let p = probes();
        assert_eq!(p.len(), 7);
        let f = p.iter().find(|p| p.art == "friesisch").unwrap();
        assert_eq!((f.size, f.mm), ((1024, 868), 1.464));
        // Kante um 1 px verschoben zählt nicht, ein falscher Fleck schon
        let (w, h) = (4, 1);
        let cpu = [
            10, 10, 10, 255, 10, 10, 10, 255, 200, 200, 200, 255, 200, 200, 200, 255,
        ];
        let mut gpu = cpu;
        gpu[4..7].copy_from_slice(&[200, 200, 200]);
        assert_eq!(
            compare(&cpu, &gpu, (w, h)),
            Diff {
                max: 190,
                outside: 0
            }
        );
        gpu[12..15].copy_from_slice(&[90, 90, 90]);
        assert_eq!(compare(&cpu, &gpu, (w, h)).outside, 1);
        assert_eq!(compare(&cpu, &cpu, (w, h)), Diff { max: 0, outside: 0 });
    }

    /// Familienkarte als gültiges `.npy` (Kopf auf 64 Byte), Fugen −1.
    #[test]
    fn familienkarte() {
        let mut p = probes().into_iter().find(|p| p.art == "friesisch").unwrap();
        p.size = (40, 30);
        let (npy, csv, line) = families(&p);
        assert_eq!(&npy[..8], b"\x93NUMPY\x01\x00");
        let hl = u16::from_le_bytes([npy[8], npy[9]]) as usize;
        assert_eq!((10 + hl) % 64, 0);
        assert_eq!(npy.len(), 10 + hl + 40 * 30);
        assert!(npy[10 + hl..].contains(&0xff), "Fugen");
        assert!(npy[10 + hl..].iter().all(|&b| b == 0xff || b < 3));
        assert!(csv.lines().count() > 1);
        assert!(line.starts_with("Fläche Rot "), "{line}");
    }
}

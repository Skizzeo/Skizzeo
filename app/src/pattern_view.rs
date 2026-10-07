//! Vorschau des Fensters „Muster“ (Paket 7b) mit dem Flächen-Shader:
//! Netze, Kameras und Aussehens-Tabelle der kleinen Szenen. Das Fenster
//! selbst ist ein Zustand der Einstellungen (`prefs_pattern.rs`); der
//! Renderer zeichnet die Szenen in ein eigenes Bild
//! ([`sk_render::Preview`]).

use crate::camera::Camera;
use crate::draw_table::{fallback_look, look_rows, pattern_rows_for, MatLook};
use sk_math::{vec3, Vec3};
use sk_model::proctex::{self, Bond, Pattern};
use sk_model::solid::edge_kind;
use sk_render::{pattern_mode, EdgeLooks, Looks, MeshData, PreviewItem, BOND_TABLE_BYTES};
use sk_ui::theme::Theme;

/// Stufe der großen Vorschau (Segment „Nah | Ansicht 1:100 | Fern“).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    Near,
    Elevation,
    Far,
}

/// Fernstufe: Wandstück und Kamera so viel größer, das Muster bleibt.
const FAR: f64 = 14.0;
/// Breite der Varianten von vorne (mm, Einstellungen p7 §3.3).
pub const VARIANT_W: f64 = 1200.0;
/// Blick auf die Ecke: Neigung und Öffnungswinkel (Grad).
const PITCH: f64 = -24.0;
const FOV: f64 = 32.0;

/// Eine Fassung des Musters: Muster und Farbe der Oberfläche.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub pattern: Option<Pattern>,
    pub base: [u8; 3],
}

/// Was die Vorschau zeigt. Bereiche in Bildpunkten des Vorschaubilds
/// (links, oben, Breite, Höhe).
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    /// Neuer Stand (rechts vom Teiler) und Stand beim Öffnen.
    pub after: Look,
    pub before: Look,
    pub variants: Vec<Look>,
    pub stage: Stage,
    pub big: [i32; 4],
    /// Teiler „Vorher/Nachher“: Lage in Bildpunkten ab dem linken Rand der
    /// großen Vorschau; `None` = aus.
    pub split: Option<i32>,
    pub tiles: Vec<[i32; 4]>,
    /// Stift „Ansichtsmuster“: Farbe 0..1 und Breite in Bildpunkten.
    pub ink: [f32; 4],
    pub paper: [f32; 3],
    /// Bildpunkte je mm Papier (mit Skalierung), für „Ansicht 1:100“.
    pub px_per_mm: f32,
    /// Kanten in 3D und in der Zeichnung (aus der Zeichentabelle).
    pub model_edges: EdgeLooks,
    pub drawing_edges: EdgeLooks,
}

/// Kantenlänge der Würfelvorschau als Maß des Wandstücks (mm).
fn edge_mm(p: Option<&Pattern>) -> f64 {
    p.map_or(1000.0, crate::attr_pick::cube_edge_mm)
}

/// Wandstück mit Ecke: zwei Wände der Länge `l`, Höhe `h` und Dicke `d`
/// von der Ecke (0, 0) nach +x und +y. Sichtbar sind die Außenflächen
/// Süd (y = 0) und West (x = 0) und die Krone; Kanten außen herum.
pub fn corner_mesh(key: u16, l: f64, h: f64, d: f64) -> MeshData {
    let mut m = MeshData::default();
    let k = key as f32;
    let quad = |m: &mut MeshData, q: [Vec3; 4], n: [f32; 3]| {
        for i in [0, 1, 2, 0, 2, 3] {
            let p = q[i].to_f32();
            m.faces
                .push([p[0], p[1], p[2], n[0], n[1], n[2], k, 0.0, 0.0]);
        }
    };
    quad(
        &mut m,
        [
            vec3(0.0, 0.0, 0.0),
            vec3(l, 0.0, 0.0),
            vec3(l, 0.0, h),
            vec3(0.0, 0.0, h),
        ],
        [0.0, -1.0, 0.0],
    );
    quad(
        &mut m,
        [
            vec3(0.0, l, 0.0),
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 0.0, h),
            vec3(0.0, l, h),
        ],
        [-1.0, 0.0, 0.0],
    );
    let up = [0.0, 0.0, 1.0];
    quad(
        &mut m,
        [
            vec3(0.0, 0.0, h),
            vec3(l, 0.0, h),
            vec3(l, d, h),
            vec3(0.0, d, h),
        ],
        up,
    );
    quad(
        &mut m,
        [
            vec3(0.0, d, h),
            vec3(d, d, h),
            vec3(d, l, h),
            vec3(0.0, l, h),
        ],
        up,
    );
    let kind = edge_kind::VIEW as f32;
    for (a, b) in [
        (vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, h)),
        (vec3(0.0, 0.0, h), vec3(l, 0.0, h)),
        (vec3(0.0, 0.0, h), vec3(0.0, l, h)),
        (vec3(l, d, h), vec3(d, d, h)),
        (vec3(d, d, h), vec3(d, l, h)),
    ] {
        m.edges.push(([a.to_f32(), b.to_f32()], kind));
    }
    m
}

/// Fläche von vorne: `w` × `h` mm in der Ebene y = 0, Blick nach +y.
pub fn panel_mesh(key: u16, w: f64, h: f64) -> MeshData {
    let mut m = MeshData::default();
    let q = [
        vec3(0.0, 0.0, 0.0),
        vec3(w, 0.0, 0.0),
        vec3(w, 0.0, h),
        vec3(0.0, 0.0, h),
    ];
    for i in [0, 1, 2, 0, 2, 3] {
        let p = q[i].to_f32();
        m.faces
            .push([p[0], p[1], p[2], 0.0, -1.0, 0.0, key as f32, 0.0, 0.0]);
    }
    m
}

/// Maße des Wandstücks (Länge, Höhe, Dicke) zur Kante `e` (mm).
fn corner_size(e: f64) -> (f64, f64, f64) {
    (3.2 * e, 2.6 * e, 0.27 * e)
}

/// Kamera von Südwesten schräg von oben auf die Ecke, Krone im oberen
/// Teil des Bildes (soll-p7-1).
pub fn corner_camera(e: f64) -> Camera {
    let (_, h, _) = corner_size(e);
    let target = vec3(0.0, 0.0, h - 0.55 * e);
    let mut c = Camera::looking_at(vec3(-1.0, -1.0, 0.0), vec3(0.0, 0.0, 0.0), FOV);
    c.pitch = PITCH.to_radians();
    c.eye = target - c.forward() * (2.9 * e);
    c.focus = 2.9 * e;
    c
}

/// Parallelblick von vorne auf eine Fläche `w` × `h` mm (Ansicht,
/// Varianten).
pub fn front_camera(w: f64, h: f64) -> Camera {
    Camera::parallel(
        vec3(w * 0.5, 0.0, h * 0.5),
        90f64.to_radians(),
        0.0,
        h * 0.5,
    )
}

/// Aussehens-Tabelle: Schlüssel 0 nachher, 1 vorher, 2 … die Varianten.
/// Wilder Verband ohne fertige Tabelle zeigt die Mischfarbe (Deckkraft 0)
/// und rechnet sie im Hintergrund.
pub fn preview_looks(inp: &Input, t: &Theme) -> Looks {
    let mut all: Vec<&Look> = vec![&inp.after, &inp.before];
    all.extend(&inp.variants);
    let mut seeds: Vec<u32> = Vec::new();
    for l in &all {
        if let Some(Pattern::Masonry {
            bond: Bond::Wild,
            seed,
            ..
        }) = &l.pattern
        {
            if !seeds.contains(seed) {
                seeds.push(*seed);
            }
        }
    }
    let keys = all.len();
    let mut texels = vec![[0.0; 4]; keys * sk_render::LOOK_ROWS];
    for (k, l) in all.iter().enumerate() {
        let c = rgb(l.base);
        let m = MatLook {
            face: c,
            cut: c,
            cut_bg: c,
            pattern: pattern_rows_for(l.pattern.as_ref(), l.base, |s| {
                seeds.iter().position(|x| *x == s).unwrap_or(0)
            }),
            ..fallback_look(t)
        };
        let mut rows = look_rows(&m, 1.0);
        if let Some(p) = &l.pattern {
            if !proctex::pattern_ready(p) {
                rows[12][3] = 0.0;
            }
        }
        for (row, v) in rows.into_iter().enumerate() {
            texels[row * keys + k] = v;
        }
    }
    let bond = seeds
        .iter()
        .flat_map(|&s| {
            proctex::bond_table_ready(s)
                .map_or_else(|| vec![0; BOND_TABLE_BYTES], |t| t.cells.clone())
        })
        .collect();
    Looks {
        keys,
        texels,
        drawing: inp.drawing_edges,
        model: inp.model_edges,
        pattern_ink: inp.ink,
        bond,
    }
}

fn rgb(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| v as f32 / 255.0)
}

/// Netze und Teilbilder der Vorschau; Netz `i` gehört zu Teilbild `i`.
pub fn preview_scene(inp: &Input) -> (Vec<MeshData>, Vec<PreviewItem>) {
    let mut meshes = Vec::new();
    let mut items = Vec::new();
    let [bx, by, bw, bh] = inp.big;
    // Vorher links vom Teiler, nachher rechts; ohne Teiler nur nachher
    let halves: Vec<(u16, Option<[i32; 4]>)> = match inp.split {
        Some(x) => {
            let x = x.clamp(0, bw);
            vec![
                (1, Some([bx, by, x, bh])),
                (0, Some([bx + x, by, bw - x, bh])),
            ]
        }
        None => vec![(0, None)],
    };
    for (key, clip) in halves {
        let look = if key == 0 { &inp.after } else { &inp.before };
        let (mesh, view, lit, sky) = match inp.stage {
            Stage::Near | Stage::Far => {
                let k = if inp.stage == Stage::Far { FAR } else { 1.0 };
                let e = edge_mm(look.pattern.as_ref()) * k;
                let (l, h, d) = corner_size(e);
                let mut v = corner_camera(e).view(bw as u32, bh as u32);
                // Himmel als Verlauf über die ganze Höhe, ohne Boden
                v.eye_z = -1.0e9;
                v.horizon_px = 0.0;
                v.patterns = pattern_mode::COLORS;
                (corner_mesh(key, l, h, d), v, true, true)
            }
            Stage::Elevation => {
                // 1:100: ein Meter sind 10 mm Papier
                let k = (inp.px_per_mm / 100.0).max(1e-4) as f64;
                let (w, h) = (bw as f64 / k, bh as f64 / k);
                let mut v = front_camera(w, h).view(bw as u32, bh as u32);
                v.paper = Some(inp.paper);
                v.patterns = pattern_mode::LINES;
                (panel_mesh(key, w, h), v, false, false)
            }
        };
        items.push(PreviewItem {
            rect: inp.big,
            clip,
            view,
            mesh: meshes.len(),
            lit,
            sky,
        });
        meshes.push(mesh);
    }
    for (i, r) in inp.tiles.iter().enumerate().take(inp.variants.len()) {
        let h = VARIANT_W * r[3].max(1) as f64 / r[2].max(1) as f64;
        let mut v = front_camera(VARIANT_W, h).view(r[2] as u32, r[3] as u32);
        v.patterns = pattern_mode::COLORS;
        items.push(PreviewItem {
            rect: *r,
            clip: None,
            view: v,
            mesh: meshes.len(),
            lit: false,
            sky: false,
        });
        meshes.push(panel_mesh(2 + i as u16, VARIANT_W, h));
    }
    (meshes, items)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Das Wandstück steht mit der Ecke in der Bildmitte, die Krone im
    /// oberen Teil, die Wände reichen über die Ränder (soll-p7-1).
    #[test]
    fn ecke_im_bild() {
        let e = 891.0;
        let (l, h, _) = corner_size(e);
        let c = corner_camera(e);
        let (w, hh) = (560.0, 360.0);
        let top = c.project(vec3(0.0, 0.0, h), w, hh).unwrap();
        assert!((top.0 - w / 2.0).abs() < 1.0, "{top:?}");
        assert!(top.1 > 0.15 * hh && top.1 < 0.5 * hh, "{top:?}");
        let foot = c.project(vec3(0.0, 0.0, 0.0), w, hh).unwrap();
        assert!(foot.1 > hh, "Fuß unter dem Bild: {foot:?}");
        for end in [vec3(l, 0.0, h), vec3(0.0, l, h)] {
            let p = c.project(end, w, hh).unwrap();
            assert!(p.0 < 0.0 || p.0 > w, "Ende außerhalb: {p:?}");
        }
    }

    /// Ohne Teiler ein Teilbild, mit Teiler zwei (vorher links), dazu je
    /// Variante eines mit eigenem Schlüssel.
    #[test]
    fn teilbilder() {
        let p = proctex::masonry_default();
        let look = Look {
            pattern: Some(p.clone()),
            base: [200, 100, 80],
        };
        let mut inp = Input {
            after: look.clone(),
            before: look.clone(),
            variants: vec![look.clone(); 6],
            stage: Stage::Near,
            big: [0, 0, 560, 360],
            split: None,
            tiles: (0..6).map(|i| [i * 60, 380, 50, 30]).collect(),
            ink: [0.0, 0.0, 0.0, 1.0],
            paper: [1.0; 3],
            px_per_mm: 3.78,
            model_edges: EdgeLooks::default(),
            drawing_edges: EdgeLooks::default(),
        };
        let (m, it) = preview_scene(&inp);
        assert_eq!((m.len(), it.len()), (7, 7));
        assert_eq!(m[0].faces[0][6], 0.0);
        assert_eq!(m[6].faces[0][6], 7.0);
        inp.split = Some(200);
        let (_, it) = preview_scene(&inp);
        assert_eq!(it.len(), 8);
        assert_eq!(it[0].clip, Some([0, 0, 200, 360]));
        assert_eq!(it[1].clip, Some([200, 0, 360, 360]));
        let l = preview_looks(&inp, &Theme::dark());
        assert_eq!(l.keys, 8);
        assert_eq!(l.texels.len(), 8 * sk_render::LOOK_ROWS);
    }
}

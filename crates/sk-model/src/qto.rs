//! Mengen je Bauteil (IFC: Qto_WallBaseQuantities), aus der Parametrik
//! berechnet, nie aus dem Anzeigenetz. Alle Werte brutto (ohne Öffnungen und
//! ohne Verschnitt zwischen Wandzügen), Einheit mm, mm², mm³ und kg.

use crate::element::{ElementId, RunId};
use crate::library::MaterialId;
use crate::model::Model;
use sk_math::Vec3;

/// Mengen einer Schicht in einem Wandsegment.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerQto {
    pub material: MaterialId,
    /// Dicke in mm.
    pub thickness: f64,
    /// Länge der Schichtmittellinie im Segment (mm).
    pub length: f64,
    /// Grundfläche der Schicht im Segment, Viereck mit Gehrungsschnitten (mm²).
    pub area: f64,
    /// Grundfläche × Höhe (mm³).
    pub volume: f64,
    /// Volumen × Rohdichte (kg).
    pub mass: f64,
}

/// Mengen eines Wandsegments.
#[derive(Clone, Debug, PartialEq)]
pub struct WallQto {
    /// Länge auf der Bezugslinie (IFC Length), mm.
    pub length: f64,
    /// Gesamtdicke (IFC Width), mm.
    pub width: f64,
    /// Höhe (IFC Height), mm.
    pub height: f64,
    /// Summe der Schicht-Grundflächen (IFC GrossFootprintArea), mm².
    pub footprint: f64,
    /// Außenfläche: Länge der Außenkante × Höhe (IFC GrossSideArea), mm².
    pub side_outer: f64,
    /// Innenfläche: Länge der Innenkante × Höhe, mm².
    pub side_inner: f64,
    /// Summe der Schichtvolumen (IFC GrossVolume), mm³.
    pub volume: f64,
    /// Schichten von außen nach innen.
    pub layers: Vec<LayerQto>,
}

/// Fläche eines ebenen Vielecks in der Grundrissebene.
fn area(p: &[Vec3]) -> f64 {
    let n = p.len();
    let twice: f64 = (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    twice.abs() * 0.5
}

/// Mengen aller Segmente eines Wandzugs, in Segmentreihenfolge.
pub fn run_qto(model: &Model, run: RunId) -> Vec<WallQto> {
    let (Some(chain), Some(r)) = (model.chain(run), model.run(run)) else {
        return Vec::new();
    };
    let Some(set) = r
        .segments
        .first()
        .and_then(|e| model.element(*e))
        .and_then(|e| e.layer_set)
        .and_then(|s| model.layer_set(s))
    else {
        return Vec::new();
    };
    let pts = chain.clean_points();
    let offsets = chain.layer_offsets();
    let h = chain.height;
    let (c_out, c_in) = (
        chain.face_corners(chain.outer_offset()),
        chain.face_corners(chain.inner_offset()),
    );
    let faces: Vec<(Vec<Vec3>, Vec<Vec3>)> = offsets
        .iter()
        .map(|&(a, b, _)| (chain.face_corners(a), chain.face_corners(b)))
        .collect();
    let n = pts.len();
    (0..chain.segment_count())
        .map(|k| {
            let j = (k + 1) % n;
            let layers: Vec<LayerQto> = set
                .layers
                .iter()
                .zip(&faces)
                .map(|(l, (fa, fb))| {
                    let quad = [fa[k], fa[j], fb[j], fb[k]];
                    let area = area(&quad);
                    let volume = area * h;
                    let density = model.material(l.material).map_or(0.0, |m| m.density);
                    LayerQto {
                        material: l.material,
                        thickness: l.thickness,
                        length: ((fa[j] - fa[k]).length() + (fb[j] - fb[k]).length()) * 0.5,
                        area,
                        volume,
                        mass: volume * 1e-9 * density,
                    }
                })
                .collect();
            let footprint: f64 = layers.iter().map(|l| l.area).sum();
            WallQto {
                length: (pts[j] - pts[k]).length(),
                width: chain.thickness(),
                height: h,
                footprint,
                side_outer: (c_out[j] - c_out[k]).length() * h,
                side_inner: (c_in[j] - c_in[k]).length() * h,
                volume: footprint * h,
                layers,
            }
        })
        .collect()
}

/// Mengen einer Wand.
pub fn wall_qto(model: &Model, wall: ElementId) -> Option<WallQto> {
    let (run, seg) = model.segment_of(wall)?;
    run_qto(model, run).into_iter().nth(seg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Category;
    use crate::wall::RefSide;
    use sk_math::vec3;

    const M3: f64 = 1e9;

    #[test]
    fn rechteck_10_x_8() {
        let mut m = Model::with_seed(1);
        let set = m.defaults().exterior_wall;
        // Außenkante auf der Bezugslinie, im Uhrzeigersinn, Bezugsseite links
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let r = m
            .add_wall_run(&pts, true, RefSide::Left, 2750.0, set, Category::ExteriorWall)
            .unwrap();
        let q = run_qto(&m, r);
        assert_eq!(q.len(), 4);
        let vol: f64 = q.iter().map(|w| w.volume).sum();
        assert!((vol / M3 - 30.0935).abs() < 1e-4, "{}", vol / M3);
        let layer = |i: usize| -> f64 { q.iter().map(|w| w.layers[i].volume).sum::<f64>() / M3 };
        assert!((layer(0) - 13.6444).abs() < 1e-4, "{}", layer(0));
        assert!((layer(1) - 16.4491).abs() < 1e-4, "{}", layer(1));
        let len: f64 = q.iter().map(|w| w.length).sum();
        assert!((len - 36000.0).abs() < 1e-6);
        assert!(q.iter().all(|w| (w.width - 315.0).abs() < 1e-9));
        // Obere Wand 10 m: außen 10,00 m, innen 9,37 m lang
        let top = &q[1];
        assert!((top.side_outer / 1e6 - 27.5).abs() < 1e-9);
        assert!((top.side_inner / 1e6 - 9.37 * 2.75).abs() < 1e-9);
        // Masse: Volumen × Rohdichte
        let l = &top.layers[1];
        assert!((l.mass - l.volume / M3 * 350.0).abs() < 1e-9);
        assert_eq!(m.material(l.material).unwrap().name, "Gasbeton");
        assert_eq!(wall_qto(&m, m.wall_at(r, 1).unwrap()).as_ref(), Some(top));
    }

    #[test]
    fn gerade_wand() {
        let mut m = Model::with_seed(2);
        let set = m.defaults().exterior_wall;
        let pts = [vec3(0.0, 0.0, 0.0), vec3(5000.0, 0.0, 0.0)];
        let r = m
            .add_wall_run(&pts, false, RefSide::Left, 2750.0, set, Category::ExteriorWall)
            .unwrap();
        let q = run_qto(&m, r);
        assert_eq!(q.len(), 1);
        assert!((q[0].volume / M3 - 5.0 * 0.315 * 2.75).abs() < 1e-9);
        assert!((q[0].length - 5000.0).abs() < 1e-9);
        assert!((q[0].layers[0].length - 5000.0).abs() < 1e-9);
    }
}

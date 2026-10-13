//! OK Gelände für die Kanten der Ansichten (S11): Kanten, die das Gelände
//! kreuzen, teilt der Netzbau dort, damit der untere Teil gestrichelt
//! zeichnet. Ohne Aufmaß gilt die Ebene des Gebäudes, mit Aufmaß sein
//! Netz als Raster, dasselbe, das der Renderer für „unter Gelände“ fragt
//! (Höhenkette wie `terrain_z_at`, Geometrie zu G1). Aus `scene.rs`
//! ausgelagert.

use super::Scene;
use sk_model::Solid;
use sk_render::gelaende::TerrainGrid;
use sk_render::MeshData;
use std::rc::Rc;

/// Wovon das Raster abhängt: Szene, Aufmaß (Adresse, Punkte), Rahmen
/// und Höhe von ±0,00 (Bits).
pub(super) type RasterKey = (u64, usize, usize, [u64; 4]);

/// OK Gelände einer Stelle: das Raster, wo es gilt, sonst die Ebene des
/// Gebäudes (mm zu ±0,00).
#[derive(Clone, Copy)]
pub(super) struct Boden<'a> {
    pub raster: Option<&'a TerrainGrid>,
    pub eben: f64,
}

impl Boden<'_> {
    pub fn eben(z: f64) -> Boden<'static> {
        Boden {
            raster: None,
            eben: z,
        }
    }

    fn z(&self, x: f64, y: f64) -> f64 {
        self.raster.and_then(|r| r.z(x, y)).unwrap_or(self.eben)
    }
}

impl Scene {
    /// OK Gelände aus dem Aufmaß als Raster; `None` ohne Aufmaß, Lage
    /// oder Netz. Neu gerechnet nur, wenn sich Aufmaß, Lage oder die Höhe
    /// von ±0,00 geändert haben. Im offenen Schritt bleibt es stehen wie
    /// die Platte und folgt beim Loslassen oder Esc: Die Höhe von ±0,00
    /// fragt die Lage ab, und die rechnet nach jeder Wandbewegung alle Züge
    /// neu (Review #129 B1: 8,5 ms je Bewegung bei 1000 Häusern, 71 ms bei
    /// 4000).
    pub fn terrain_grid(&mut self) -> Option<Rc<TerrainGrid>> {
        if self.model.in_step() {
            if let Some((_, g)) = &self.gelaende_raster {
                return g.clone();
            }
        }
        let m = &self.model;
        let key = match (m.terrain(), m.survey_frame(), m.terrain_nhn()) {
            (Some(t), Some(f), Some(h)) => Some((
                self.kennung,
                t as *const _ as usize,
                t.points.len(),
                [f.e0, f.n0, f.g, h].map(f64::to_bits),
            )),
            _ => None,
        };
        if self.gelaende_raster.as_ref().map(|r| r.0) != Some(key) {
            let g = key.and_then(|_| TerrainGrid::aus_dreiecken(&m.terrain_triangles()));
            self.gelaende_raster = Some((key, g.map(Rc::new)));
        }
        self.gelaende_raster.as_ref().and_then(|r| r.1.clone())
    }
}

/// Hängt das Netz eines Körpers an `m` an.
pub(super) fn mesh_into(m: &mut MeshData, s: &Solid, boden: Boden) {
    m.faces.reserve(s.triangles.len() * 3);
    for t in &s.triangles {
        let n = t.n.to_f32();
        let key = t.mat as f32;
        for (v, uv) in t.p.iter().zip(t.uv) {
            let p = v.to_f32();
            m.faces.push([
                p[0],
                p[1],
                p[2],
                n[0],
                n[1],
                n[2],
                key,
                uv[0] as f32,
                uv[1] as f32,
            ]);
        }
    }
    // Kanten, die das Gelände kreuzen, an OK Gelände teilen: In den
    // Ansichten zeichnet der untere Teil dann gestrichelt in der Breite der
    // feinen Linie (S11, Review 3cf a); die Breite gilt je Kante.
    m.edges.reserve(s.edges.len());
    for e in &s.edges {
        let k = e.kind as f32;
        let (a, b) = (e.a, e.b);
        let (ga, gb) = (boden.z(a.x, a.y), boden.z(b.x, b.y));
        let (da, db) = (a.z - ga, b.z - gb);
        // Nur echt kreuzende: endet eine Kante auf dem Gelände, entstünde
        // ein Stück der Länge 0 (Toleranz wie im Shader, Review 3cg)
        if da.min(db) < -0.5 && da.max(db) > 0.5 {
            let t = da / (da - db);
            let mut g = a + (b - a) * t;
            g.z = ga + (gb - ga) * t;
            m.edges.push(([a.to_f32(), g.to_f32()], k));
            m.edges.push(([g.to_f32(), b.to_f32()], k));
        } else {
            m.edges.push(([a.to_f32(), b.to_f32()], k));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{szo, GuidGen};

    /// Mit Raster teilt der Netzbau eine senkrechte Kante dort, wo das
    /// schräge Gelände sie schneidet; daneben gilt die Ebene.
    #[test]
    fn kante_teilt_am_raster() {
        let p = |x: f64, y: f64| vec3(x, y, x / 10.0 - 500.0);
        let d = [
            [p(0.0, 0.0), p(20_000.0, 0.0), p(20_000.0, 10_000.0)],
            [p(0.0, 0.0), p(20_000.0, 10_000.0), p(0.0, 10_000.0)],
        ];
        let g = TerrainGrid::aus_dreiecken(&d).unwrap();
        let mut s = Solid::default();
        s.edge(vec3(8000.0, 5000.0, -2000.0), vec3(8000.0, 5000.0, 1000.0));
        s.edge(vec3(-100.0, 5000.0, -2000.0), vec3(-100.0, 5000.0, 1000.0));
        let mut m = MeshData::default();
        let boden = Boden {
            raster: Some(&g),
            eben: -400.0,
        };
        mesh_into(&mut m, &s, boden);
        let zs: Vec<_> = m.edges.iter().map(|(e, _)| (e[0][2], e[1][2])).collect();
        assert_eq!(zs.len(), 4);
        assert!(
            (zs[0].1 - 300.0).abs() < 0.01 && (zs[1].0 - 300.0).abs() < 0.01,
            "{zs:?}"
        );
        assert_eq!(&zs[2..], [(-2000.0, -400.0), (-400.0, 1000.0)]);
    }

    /// Das Raster kommt mit dem Aufmaß, bleibt dasselbe, solange sich
    /// nichts am Gelände ändert, und folgt einer neuen Höhenlage; im
    /// offenen Schritt erst beim Loslassen (Review #129 B1).
    #[test]
    fn raster_mit_dem_aufmass() {
        let text = include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo");
        let m = szo::read_with(text, GuidGen::with_seed(1), &sk_cost::lesen::ABSCHNITTE_SZO)
            .unwrap()
            .model;
        let mut s = Scene::with_model(m);
        assert!(s.terrain_grid().is_none());
        let csv =
            include_bytes!("../../crates/sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.csv");
        let (t, ..) = sk_model::Terrain::lesen(csv, "aufmass.csv", "2026-10-13", 32).unwrap();
        assert!(s.edit_model("Höhenaufmaß", |m| m.import_terrain(t)));
        let a = s.terrain_grid().unwrap();
        assert!(Rc::ptr_eq(&a, &s.terrain_grid().unwrap()));
        let (x, y) = (5000.0, 5000.0);
        let z = s.model().terrain_z_at(None, x, y);
        assert!((a.z(x, y).unwrap() - z).abs() < 20.0, "{z}");
        // Ein Gelände-Zug ändert den Versatz und damit ±0,00: Das Raster
        // steht, bis er losgelassen ist
        let nhn = s.model().terrain_nhn();
        s.begin("Gelände ziehen");
        s.drag_terrain(-900.0);
        assert_ne!(s.model().terrain_nhn(), nhn);
        assert!(Rc::ptr_eq(&a, &s.terrain_grid().unwrap()), "steht im Zug");
        s.commit();
        let neu = s.terrain_grid().unwrap();
        assert!(!Rc::ptr_eq(&a, &neu));
        let z = s.model().terrain_z_at(None, x, y);
        assert!((neu.z(x, y).unwrap() - z).abs() < 20.0, "{z}");
        let a = neu;
        assert!(s.edit_model("Höhenlage", |m| m.set_terrain_nhn(30.0)));
        let b = s.terrain_grid().unwrap();
        assert!(!Rc::ptr_eq(&a, &b));
        assert!((b.z(x, y).unwrap() - s.model().terrain_z_at(None, x, y)).abs() < 20.0);
    }
}

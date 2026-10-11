//! Öffnungen in der Szene (Paket O3): Außenwandfläche unter der Maus und
//! Anlegen einer Öffnung als ein Schritt.

use super::{ray_hits_box, Scene, Vis};
use crate::visible::Class;
use sk_math::Vec3;
use sk_model::oeffnung::{Opening, OuterFace};
use sk_model::{ElementId, RunId, Tri};

impl Scene {
    /// Außenwandfläche unter dem Strahl (3D und Ansichten) und der
    /// Treffpunkt; `None`, wenn zuerst etwas anderes getroffen wird (eine
    /// Laibung, eine Decke, eine Innenwand).
    pub fn outer_face_at(&self, origin: Vec3, dir: Vec3) -> Option<(OuterFace, Vec3)> {
        let filter = self.filtering();
        let vis = Vis {
            model: &self.model,
            anim: self.vis_anim.as_ref(),
            table: &self.vis_table,
            stamp: self.vis_stamp(),
        };
        let mut best: Option<(f64, RunId, u32)> = None;
        for c in self.cache.iter().flatten() {
            if !c.bounds.is_some_and(|b| ray_hits_box(origin, dir, b)) {
                continue;
            }
            let run = c.id;
            let keep = |t: &Tri| !filter || vis.class(run, t.elem, t.layer) == Class::Solid;
            if let Some((t, seg)) = c.solid.raycast_where(origin, dir, keep) {
                if best.is_none_or(|b| t < b.0) {
                    best = Some((t, run, seg));
                }
            }
        }
        let (t, run, seg) = best?;
        let p = origin + dir * t;
        let face = self.model.outer_face(run, seg as usize)?;
        let inside = p.z >= face.z.0 - 1.0 && p.z <= face.z.1 + 1.0;
        (face.on_line(p) && inside).then_some((face, p))
    }

    /// Legt die Öffnung `o` als einen Schritt an.
    pub fn add_opening(&mut self, o: Opening) -> Option<ElementId> {
        let mut id = None;
        self.edit_model("Öffnung erzeugt", |m| {
            id = m.add_opening(o);
            id.is_some()
        });
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::oeffnung::{HeightLine, HeightRef, OpeningKind};
    use sk_model::Model;

    /// Der Strahl von außen trifft die Außenwandfläche; nach dem Anlegen
    /// einer Öffnung dort geht er durch das Loch und trifft keine
    /// Außenfläche mehr.
    #[test]
    fn strahl_trifft_aussenflaeche_und_loch() {
        let mut m = Model::with_seed(311);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let st = m.run(eg).unwrap().storey;
        let mut s = Scene::with_model(m);
        let (o, d) = (vec3(5000.0, -9000.0, 1500.0), vec3(0.0, 1.0, 0.0));
        let (face, p) = s.outer_face_at(o, d).expect("Außenfläche");
        assert!(face.on_line(p) && face.storey == st);
        let a = face.at(face.u_of(p) - 600.0, 0.0);
        let c = face.at(face.u_of(p) + 600.0, 0.0);
        let op = Opening {
            kind: OpeningKind::Window,
            path: vec![vec3(a.x, a.y, 0.0), vec3(c.x, c.y, 0.0)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
        };
        assert!(s.add_opening(op).is_some());
        assert!(s.outer_face_at(o, d).is_none(), "durch das Loch");
        assert!(
            s.outer_face_at(vec3(5000.0, -9000.0, 500.0), d).is_some(),
            "Brüstung"
        );
    }
}

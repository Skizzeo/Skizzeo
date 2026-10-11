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

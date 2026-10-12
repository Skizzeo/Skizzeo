//! Öffnungen in der Szene (Paket O3): Außenwandfläche unter der Maus und
//! Anlegen einer Öffnung als ein Schritt.

use super::{ray_hits_box, Scene, Vis};
use crate::visible::Class;
use sk_math::Vec3;
use sk_model::hoehen::MIN_LINE_GAP;
use sk_model::oeffnung::{Opening, OuterFace};
use sk_model::{ElementId, RunId, StoreyId, Tri};

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

    /// Höhenlinie beim Ziehen im Geschossmanager (im offenen Schritt):
    /// Brüstung bzw. Sturz (`sturz`) des Geschosses `st` auf die Kote `z`,
    /// geklemmt: Brüstung ab OK Rohdecke, Sturz bis UK Decke, dazwischen
    /// mindestens [`MIN_LINE_GAP`]. Die Wände des Gebäudes rechnen neu, an
    /// die Linie gebundene Öffnungen wandern mit (Jörn 14:22).
    pub fn drag_height_line(&mut self, st: StoreyId, sturz: bool, z: f64) {
        let Some(s) = self.model.storey(st) else {
            return;
        };
        let (mut l, v) = (s.lines, z - s.elevation);
        let clear = self.model.clear_height(st);
        if sturz {
            l.lintel = v.min(clear).max(l.sill + MIN_LINE_GAP);
        } else {
            l.sill = v.max(0.0).min(l.lintel - MIN_LINE_GAP);
        }
        if s.lines != l && self.model.set_height_lines(st, l) {
            self.relevel(st);
        }
    }

    /// Legt die Öffnung `o` als einen Schritt an. Neu gerechnet werden nur
    /// die Züge, die sie berührt (Review #86, B1: nicht alle Züge).
    pub fn add_opening(&mut self, o: Opening) -> Option<ElementId> {
        self.begin("Öffnung erzeugt");
        let id = self.model.add_opening(o);
        for r in self.model.step_touched() {
            self.mark(r);
        }
        self.commit();
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

    /// Höhenlinie ziehen: geklemmt an OK Rohdecke, die lichte Höhe und den
    /// Abstand zur anderen Linie; die gebundene Öffnung wandert mit, ein
    /// Schritt, Rückgängig stellt alles zurück.
    #[test]
    fn hoehenlinie_ziehen() {
        let mut m = Model::with_seed(312);
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
        let (face, p) = s.outer_face_at(o, d).unwrap();
        let (a, c) = (
            face.at(face.u_of(p) - 600.0, 0.0),
            face.at(face.u_of(p) + 600.0, 0.0),
        );
        s.add_opening(Opening {
            kind: OpeningKind::Window,
            path: vec![vec3(a.x, a.y, 0.0), vec3(c.x, c.y, 0.0)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
        })
        .unwrap();
        let el = s.model().storey(st).unwrap().elevation;
        let clear = s.model().clear_height(st);
        let low = vec3(5000.0, -9000.0, el + 1000.0);
        assert!(s.outer_face_at(low, d).is_none(), "bei 1,00 m im Loch");
        s.begin("Höhenlinie ziehen");
        s.drag_height_line(st, false, el + 1200.0);
        assert_eq!(s.model().storey(st).unwrap().lines.sill, 1200.0);
        assert!(s.outer_face_at(low, d).is_some(), "Brüstung mitgewandert");
        s.drag_height_line(st, true, el + 1250.0);
        assert_eq!(
            s.model().storey(st).unwrap().lines.lintel,
            1300.0,
            "10 cm Abstand"
        );
        s.drag_height_line(st, true, el + 9000.0);
        assert_eq!(
            s.model().storey(st).unwrap().lines.lintel,
            clear,
            "bis UK Decke"
        );
        s.drag_height_line(st, false, el - 500.0);
        assert_eq!(
            s.model().storey(st).unwrap().lines.sill,
            0.0,
            "ab OK Rohdecke"
        );
        s.commit();
        s.undo();
        let l = s.model().storey(st).unwrap().lines;
        assert_eq!((l.sill, l.lintel), (sk_model::SILL, sk_model::LINTEL));
        assert!(s.outer_face_at(low, d).is_none());
    }
}

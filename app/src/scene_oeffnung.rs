//! Öffnungen in der Szene (Paket O3): Außenwandfläche unter der Maus und
//! Anlegen einer Öffnung als ein Schritt.

use super::{ray_hits_box, Scene, Vis};
use crate::visible::Class;
use sk_math::Vec3;
use sk_model::hoehen::{door_snap, MIN_LINE_GAP};
use sk_model::oeffnung::{HeightLine, Opening, OuterFace, MIN_OPENING};
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
    /// Linie `k` des Geschosses `st` auf die Kote `z`, geklemmt: Brüstung ab
    /// OK Rohdecke, Sturz bis UK Decke, dazwischen mindestens
    /// [`MIN_LINE_GAP`]; die Innentürlinie von [`MIN_OPENING`] bis UK Decke,
    /// nahe einer Normhöhe nach DIN 18101 dort gefangen ([`door_snap`]). Die Wände des Gebäudes rechnen neu, an
    /// die Linie gebundene Öffnungen wandern mit (Jörn 14:22).
    pub fn drag_height_line(&mut self, st: StoreyId, k: HeightLine, z: f64) {
        let Some(s) = self.model.storey(st) else {
            return;
        };
        let (mut l, v) = (s.lines, z - s.elevation);
        let clear = self.model.clear_height(st);
        match k {
            HeightLine::Lintel => l.lintel = v.min(clear).max(l.sill + MIN_LINE_GAP),
            HeightLine::Sill => l.sill = v.max(0.0).min(l.lintel - MIN_LINE_GAP),
            HeightLine::Door => l.door = door_snap(v).min(clear).max(MIN_OPENING),
        }
        if s.lines != l && self.model.set_height_lines(st, l) {
            self.relevel(st);
        }
    }

    /// Schnitthöhe beim Ziehen im Geschossmanager auf die Kote `z` des
    /// aktiven Geschosses, höchstens bis 1 mm unter UK Decke: darüber fiele
    /// die Linie aus ihrem Band und wäre nicht mehr zu greifen (Review #92
    /// H4).
    pub fn drag_plan_cut(&mut self, z: f64) {
        let st = self.active_storey();
        let el = self.model.storey(st).map_or(0.0, |s| s.elevation);
        let h = (z - el).min(self.model.clear_height(st) - 1.0);
        self.model.set_plan_cut_height(h);
    }

    /// Stellt die Schnitthöhe nach **Esc** samt roher Dateizeile zurück
    /// (Review #92 H3, L16).
    pub fn restore_plan_cut(&mut self, s: sk_model::hoehen::PlanCutState) {
        self.model.restore_plan_cut(s);
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

    /// Setzt im offenen Schritt den Aufschlag der Tür `id` (Griff im
    /// Grundriss). `false`, wenn das Modell ihn ablehnt.
    pub fn set_swing(&mut self, id: ElementId, s: sk_model::oeffnung::Swing) -> bool {
        let Some(mut o) = self.model.opening(id).cloned() else {
            return false;
        };
        o.swing = Some(s);
        let ok = self.model.set_opening(id, o);
        for r in self.model.step_touched() {
            self.mark(r);
        }
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::oeffnung::{HeightRef, OpeningKind};
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
            swing: None,
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
            swing: None,
        })
        .unwrap();
        let el = s.model().storey(st).unwrap().elevation;
        let clear = s.model().clear_height(st);
        let low = vec3(5000.0, -9000.0, el + 1000.0);
        assert!(s.outer_face_at(low, d).is_none(), "bei 1,00 m im Loch");
        s.begin("Höhenlinie ziehen");
        s.drag_height_line(st, HeightLine::Sill, el + 1200.0);
        assert_eq!(s.model().storey(st).unwrap().lines.sill, 1200.0);
        assert!(s.outer_face_at(low, d).is_some(), "Brüstung mitgewandert");
        s.drag_height_line(st, HeightLine::Lintel, el + 1250.0);
        assert_eq!(
            s.model().storey(st).unwrap().lines.lintel,
            1300.0,
            "10 cm Abstand"
        );
        s.drag_height_line(st, HeightLine::Lintel, el + 9000.0);
        assert_eq!(
            s.model().storey(st).unwrap().lines.lintel,
            clear,
            "bis UK Decke"
        );
        s.drag_height_line(st, HeightLine::Sill, el - 500.0);
        assert_eq!(
            s.model().storey(st).unwrap().lines.sill,
            0.0,
            "ab OK Rohdecke"
        );
        // Normhöhe 2,13 m über 16 cm Aufbau fängt, daneben frei
        s.drag_height_line(st, HeightLine::Door, el + 2280.0);
        assert_eq!(s.model().storey(st).unwrap().lines.door, 2290.0);
        s.drag_height_line(st, HeightLine::Door, el + 2220.0);
        assert_eq!(s.model().storey(st).unwrap().lines.door, 2220.0);
        s.drag_height_line(st, HeightLine::Door, el + 9000.0);
        assert_eq!(s.model().storey(st).unwrap().lines.door, clear);
        s.drag_height_line(st, HeightLine::Door, el - 500.0);
        assert_eq!(s.model().storey(st).unwrap().lines.door, MIN_OPENING);
        assert_eq!(
            s.levels()
                .bands
                .iter()
                .find(|b| b.id == st)
                .and_then(|b| b.lines)
                .map(|l| l.2),
            Some(el + MIN_OPENING)
        );
        s.commit();
        s.undo();
        let l = s.model().storey(st).unwrap().lines;
        assert_eq!(l, sk_model::hoehen::HeightLines::default());
        assert!(s.outer_face_at(low, d).is_none());
    }

    /// Schnitthöhe im Geschossmanager (Review #92 H3, H4): Ziehen endet
    /// 1 mm unter UK Decke des aktiven Geschosses; Esc stellt den Stand vom
    /// Greifen zurück.
    #[test]
    fn schnitthoehe_ziehen_und_zurueck() {
        let mut s = Scene::new();
        let st = s.active_storey();
        let el = s.model().storey(st).unwrap().elevation;
        let clear = s.model().clear_height(st);
        let vorher = s.model().plan_cut_state();
        s.drag_plan_cut(el + 9000.0);
        assert_eq!(s.model().plan_cut_height(), (clear - 1.0).round());
        s.drag_plan_cut(el + 1000.0);
        assert_eq!(s.model().plan_cut_height(), 1000.0);
        s.restore_plan_cut(vorher.clone());
        assert_eq!(s.model().plan_cut_state(), vorher);
        assert_eq!(s.model().plan_cut_height(), sk_model::hoehen::PLAN_CUT);
    }

    /// Einbauelement im Grundriss (Jörn 12.10. 05:24): zwei dünn
    /// gestrichelte Linien längs der Wand in der geschnittenen Öffnung, im
    /// Abstand der Elementtiefe, so breit wie das Loch hinter dem Anschlag.
    /// Liegt die Brüstung über der Schnitthöhe, keine Linien.
    #[test]
    fn einbauelement_im_grundriss() {
        use crate::ui::ViewKind;
        use sk_model::edge_kind;
        let mut m = Model::with_seed(313);
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
        let op = Opening {
            kind: OpeningKind::Window,
            path: vec![vec3(a.x, a.y, 0.0), vec3(c.x, c.y, 0.0)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
        };
        let id = s.add_opening(op.clone()).unwrap();
        let el = edge_kind::ELEMENT as f32;
        let linien = |s: &mut Scene| -> Vec<[[f32; 3]; 2]> {
            let m = s.mesh(ViewKind::Plan, None, &[]);
            m.edges.iter().filter(|e| e.1 == el).map(|e| e.0).collect()
        };
        let l = linien(&mut s);
        assert_eq!(l.len(), 2, "{l:?}");
        let cut = s.plan_cut() as f32;
        assert!(l.iter().flatten().all(|q| q[2] == cut), "in Schnitthöhe");
        // Längs der Wand (y fest), Abstand = Elementtiefe
        let tiefe = s.model().install_depth(&op, eg);
        let y: Vec<f32> = l.iter().map(|e| e[0][1]).collect();
        assert!(l.iter().all(|e| e[0][1] == e[1][1]), "{l:?}");
        assert!(
            ((y[0] - y[1]).abs() as f64 - tiefe.element).abs() < 0.01,
            "{y:?}"
        );
        let breite = (l[0][1][0] - l[0][0][0]).abs() as f64;
        assert!(breite >= 1200.0 - 0.01, "mindestens die Öffnung: {breite}");
        // Aussehen: Stift der feinen Linie, gestrichelt
        let looks = s.table().edge_looks(true, 1.0);
        let (f, k) = (edge_kind::FINE as usize, edge_kind::ELEMENT as usize);
        assert_eq!(looks.width[k], looks.width[f]);
        assert!(looks.dash[2 * k][1] > 0.0, "gestrichelt");
        // Brüstung über der Schnitthöhe: die Öffnung ist nicht geschnitten
        let el_z = s.model().storey(st).unwrap().elevation;
        s.begin("Höhenlinie ziehen");
        s.drag_height_line(st, HeightLine::Sill, s.plan_cut() + 200.0);
        s.commit();
        assert!(s.model().storey(st).unwrap().lines.sill > s.plan_cut() - el_z);
        assert!(s.model().opening(id).is_some());
        assert!(linien(&mut s).is_empty());
    }
}

//! Schnitthöhe des Grundrisses in der Szene (aus `scene.rs` ausgelagert):
//! aktives Geschoss, Hintergrund des Geschosses darunter (E16) und die
//! Klemme im niedrigen Geschoss unter der Decke darüber.

use super::{Scene, PLAN_CUT};
use sk_model::{RunId, StoreyId};

/// Schnitthöhe des Hintergrunds, einmal je Geschoss gerechnet.
pub(super) type UnderMemo = std::cell::Cell<Option<(StoreyId, f64)>>;

/// Deckendicke je Geschoss zu einem Modellstand (Revision).
pub(super) type Decken = std::cell::RefCell<(Option<u64>, Vec<(StoreyId, f64)>)>;

impl Scene {
    /// Schnitthöhe des Grundrisses: OK Rohdecke des aktiven Geschosses plus
    /// die Schnitthöhe des Projekts (Vorgabe 1,50 m), im Fundament die Mitte
    /// der Frostschürze (E18).
    pub fn plan_cut(&self) -> f64 {
        self.plan_cut_of(self.active_storey())
    }

    /// Schnitthöhe des Grundrisses, wenn `id` aktiv ist. Im Fundament über
    /// den Höhenbezug (Prüfregel 14): UK Fundament + halbe Tiefe bis UK
    /// Sohlplatte (die Mitte der Frostschürze), so folgt der Schnitt
    /// Plattendicke und Gründungstiefe.
    pub(super) fn plan_cut_of(&self, id: StoreyId) -> f64 {
        let Some(st) = self.model.storey(id) else {
            return PLAN_CUT;
        };
        let at = if st.kind == sk_model::LevelKind::Foundation {
            let slab = self.model.max_slab_thickness(id);
            sk_model::LevelRef {
                storey: id,
                edge: sk_model::LevelEdge::Bottom,
                offset: (st.height - slab) / 2.0,
            }
        } else {
            sk_model::LevelRef {
                storey: id,
                edge: sk_model::LevelEdge::Bottom,
                offset: self.cut_offset(id, st),
            }
        };
        self.model.level_z(at).unwrap_or(PLAN_CUT)
    }

    /// Schnitthöhe des Hintergrunds für den Zug `run`: sein Geschoss liegt
    /// direkt unter dem aktiven (E16).
    pub(super) fn under_cut(&self, run: RunId, memo: &UnderMemo) -> Option<f64> {
        self.under_cut_for(run, self.active_storey(), memo)
    }

    /// Wie [`Scene::under_cut`], wenn `active` aktiv ist. Die Schnitthöhe
    /// je Geschoss kommt aus `memo`: die lichte Höhe zählt die Decken
    /// (Modellgröße), je Zug neu gerechnet wüchse ein Grundriss quadratisch
    /// (Review #72).
    pub(super) fn under_cut_for(
        &self,
        run: RunId,
        active: StoreyId,
        memo: &UnderMemo,
    ) -> Option<f64> {
        if self.is_foundation(active) {
            return None;
        }
        let floor = self.model.storey(active).map_or(0.0, |st| st.elevation);
        let id = self.model.run(run)?.storey;
        let st = self.model.storey(id)?;
        let below = st.kind != sk_model::LevelKind::Foundation && (st.top() - floor).abs() < 1e-6;
        if !below {
            return None;
        }
        if let Some((s, z)) = memo.get() {
            if s == id {
                return Some(z);
            }
        }
        let z = st.elevation + self.cut_offset(id, st);
        memo.set(Some((id, z)));
        Some(z)
    }

    /// Schnitthöhe des Projekts über der Unterkante von `st` (Geschoss
    /// `id`), im niedrigen Geschoss knapp unter UK Rohdecke darüber; das
    /// Flachdach hat keine Decke und schneidet frei.
    fn cut_offset(&self, id: StoreyId, st: &sk_model::Storey) -> f64 {
        let h = self.model.plan_cut_height();
        if st.kind == sk_model::LevelKind::Roof {
            return h;
        }
        let clear = st.height - self.decke_ueber(id, st.height);
        h.min(clear - sk_model::PLAN_CUT_TOP_GAP).max(0.0)
    }

    /// Dicke der Decke über dem Geschoss `id` (Höhe `height`): Geschosshöhe
    /// minus lichte Höhe. Die lichte Höhe zählt die Decken des Modells; je
    /// Modellstand gemerkt, sonst kostete jede Mausbewegung, die die
    /// Schnitthöhe fragt, einen Lauf über alle Bauteile (Review #72). Die
    /// Deckendicke ändert sich nur in einem Schritt, der die Revision zählt;
    /// Höhen beim Ziehen bleiben live.
    fn decke_ueber(&self, id: StoreyId, height: f64) -> f64 {
        let rev = self.model.revision();
        let mut m = self.decken.borrow_mut();
        if m.0 != Some(rev) {
            *m = (Some(rev), Vec::new());
        }
        if let Some(&(_, t)) = m.1.iter().find(|e| e.0 == id) {
            return t;
        }
        let t = height - self.model.clear_height(id);
        m.1.push((id, t));
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{ElementKind, Model};

    /// Die gemerkte Deckendicke folgt einer neuen Dicke im nächsten Schritt;
    /// die Klemme hält 50 mm unter UK Decke.
    #[test]
    fn klemme_folgt_der_deckendicke() {
        let mut m = Model::with_seed(5);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let st = m.run(eg).unwrap().storey;
        let f = m.floor_of(eg).unwrap();
        let Some(ElementKind::Floor(fl)) = m.element(f).map(|e| e.kind.clone()) else {
            panic!("Decke")
        };
        assert_eq!(fl.top.storey, st);
        let mut s = Scene::with_model(m);
        s.set_active_storey(st);
        assert_eq!(s.active_storey(), st);
        s.set_plan_cut_height(5000.0);
        let top = s.model().storey(st).unwrap().top();
        let gap = sk_model::PLAN_CUT_TOP_GAP;
        assert!((s.plan_cut() - (top - fl.thickness - gap)).abs() < 1e-6);
        assert!(s.edit_model("Decke", |m| m.set_floor_thickness(f, fl.thickness + 40.0)));
        assert!((s.plan_cut() - (top - fl.thickness - 40.0 - gap)).abs() < 1e-6);
        s.undo();
        assert!((s.plan_cut() - (top - fl.thickness - gap)).abs() < 1e-6);
    }
}

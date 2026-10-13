//! Schnelloption je Ansicht in der Szene (Jörn 12.10.): Arten, die eine
//! Ansicht dazu ausblendet ([`sk_model::Model::view_hidden`]), heute die
//! Dachentwässerung. Im Netz, beim Zeigen und beim Wählen wirken sie wie im
//! Baum ausgeblendet, nur in dieser Ansicht; Baum, Mengen, Kosten und IFC
//! bleiben. Die Szene setzt sie, sobald sie das Netz einer Ansicht baut
//! oder in ihr sucht ([`Scene::sicht_fuer`]).

use super::Scene;
use crate::ui::ViewKind;
use sk_model::view::Visibility;

/// Platz der Ansicht in [`sk_model::SICHT_VIEWS`].
pub fn platz(v: ViewKind) -> usize {
    match v {
        ViewKind::Persp => 0,
        ViewKind::Plan => 1,
        ViewKind::Section => 2,
        ViewKind::Front => 3,
        ViewKind::Back => 4,
        ViewKind::Left => 5,
        ViewKind::Right => 6,
    }
}

impl Scene {
    /// Die Schnelloption der Ansicht `view` in die Sichtbarkeit, sofort und
    /// ohne Übergang (Wechsel der Ansicht, Öffnen); `true`, wenn sich die
    /// Sichtbarkeit änderte und das Netz neu muss.
    pub fn sicht_fuer(&mut self, view: ViewKind) -> bool {
        let cats = self.model.view_hidden(platz(view));
        if self.model.visibility().view_cat == *cats {
            return false;
        }
        let v = Visibility {
            view_cat: cats.clone(),
            ..self.model.visibility().clone()
        };
        self.set_vis(v)
    }

    /// Blendet die Ansicht `view` die Dachentwässerung aus?
    pub fn drain_hidden(&self, view: ViewKind) -> bool {
        self.model.view_drain_hidden(platz(view))
    }

    /// Häkchen „Dachentwässerung ausblenden“ der gezeigten Ansicht `view`:
    /// Ansichtszustand ohne Schritt; die Rohre gleiten aus bzw. ein wie
    /// beim Ausblenden im Baum. `true`, wenn sich etwas änderte.
    pub fn set_drain_hidden(&mut self, view: ViewKind, hide: bool) -> bool {
        if self.drain_hidden(view) == hide {
            return false;
        }
        self.model.set_view_drain_hidden(platz(view), hide);
        let v = Visibility {
            view_cat: self.model.view_hidden(platz(view)).clone(),
            ..self.model.visibility().clone()
        };
        self.fade_vis(v);
        true
    }

    /// Wahl am Zahnrad der Ansicht `view`: Bemaßung zeigen und
    /// Maßbezugspunkte zeigen ([`sk_model::Model::mass_sicht`]).
    pub fn mass_sicht(&self, view: ViewKind) -> (bool, bool) {
        self.model.mass_sicht(platz(view))
    }

    /// Setzt die Wahl am Zahnrad der Ansicht `view`: Ansichtszustand ohne
    /// Schritt wie die Dachentwässerung.
    pub fn set_mass_sicht(&mut self, view: ViewKind, zeigen: bool, bezug: bool) {
        self.model.set_mass_sicht(platz(view), zeigen, bezug);
    }

    /// „Auf alle Ansichten übertragen“: die Wahl der Ansicht `view` gilt in
    /// allen sieben; `true`, wenn sich eine andere Ansicht änderte.
    pub fn drain_to_all(&mut self, view: ViewKind) -> bool {
        let cats = self.model.view_hidden(platz(view)).clone();
        let mut neu = false;
        for i in 0..sk_model::SICHT_VIEWS.len() {
            if *self.model.view_hidden(i) != cats {
                self.model.set_view_hidden(i, cats.clone());
                neu = true;
            }
        }
        neu
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{Category, ElementId, Model};
    use sk_render::MeshData;

    /// Prüfhaus 10 × 8 m, EG und OG, Flachdach auf dem OG mit vier
    /// Regenfallrohren; aktiv ist die Ebene Flachdach.
    pub(crate) fn haus() -> Scene {
        let mut s = Scene::with_model(Model::with_seed(12));
        let mut eg = None;
        assert!(s.edit_model("Prüfhaus", |m| {
            let b = m.add_building(2);
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ];
            eg = m.build_from_polygon(b, &pts);
            eg.is_some()
        }));
        let og = s.model().runs_above(eg.unwrap())[0];
        let st = s.model().run(og).unwrap().storey;
        s.set_active_storey(st);
        assert!(s.set_flat_roof(true));
        let fd = s.model().roof_level(st).unwrap();
        s.set_active_storey(fd);
        s
    }

    fn rohre(s: &Scene) -> Vec<ElementId> {
        let m = s.model();
        (m.elements().iter())
            .filter(|(_, e)| e.category == Category::Downpipe)
            .map(|(id, _)| id)
            .collect()
    }

    fn gleich(a: &MeshData, b: &MeshData) -> bool {
        a.faces == b.faces && a.edges == b.edges
    }

    /// Jörn 12.10.: Das Häkchen einer Ansicht blendet die Dachentwässerung
    /// nur dort aus, gleitend und mit demselben Netz wie „Ausblenden“ der
    /// Art im Baum; dort ist sie nicht wählbar. Der Baum bleibt, „alles
    /// zeigen“ im Baum hebt die Schnelloption nicht auf. Übertragen gilt sie
    /// in allen sieben Ansichten.
    #[test]
    fn dachentwaesserung_je_ansicht() {
        let mut s = haus();
        let rf = rohre(&s);
        assert_eq!(rf.len(), 4);
        // Soll: im Baum die Art „Regenfallrohr“ ausgeblendet
        let mut baum = haus();
        let mut v = baum.model().visibility().clone();
        v.hidden_cat.insert(Category::Downpipe);
        assert!(baum.set_visibility(v));
        let views = [ViewKind::Persp, ViewKind::Plan, ViewKind::Front];
        let soll: Vec<MeshData> = views.iter().map(|v| baum.mesh(*v, None, &[])).collect();
        let voll: Vec<MeshData> = views.iter().map(|v| s.mesh(*v, None, &[])).collect();
        for (v, (a, b)) in views.iter().zip(soll.iter().zip(&voll)) {
            assert!(!gleich(a, b), "{v:?}: die Rohre sind zu sehen");
        }

        s.set_now(1000);
        assert!(s.set_drain_hidden(ViewKind::Persp, true));
        assert!(!s.set_drain_hidden(ViewKind::Persp, true), "schon aus");
        assert!(s.vis_animating(), "gleitet wie im Baum");
        assert!(s.skip_vis_animation());
        assert!(s.drain_hidden(ViewKind::Persp) && !s.drain_hidden(ViewKind::Front));
        assert!(gleich(&s.mesh(ViewKind::Persp, None, &[]), &soll[0]));
        assert!(rf.iter().all(|&id| !s.visible(id) && !s.pickable(id)));
        assert_eq!(sk_model::tree::hidden_count(s.model()), 0, "Baum bleibt");
        // „Alles zeigen“ im Baum: die Schnelloption bleibt
        assert!(!s.set_visibility(Default::default()));
        assert!(gleich(&s.mesh(ViewKind::Persp, None, &[]), &soll[0]));
        // Vorne und der Grundriss zeigen sie weiter, und dort sind sie wählbar
        assert!(gleich(&s.mesh(ViewKind::Front, None, &[]), &voll[2]));
        assert!(rf.iter().all(|&id| s.visible(id) && s.pickable(id)));
        assert!(gleich(&s.mesh(ViewKind::Plan, None, &[]), &voll[1]));
        // Zurück in 3D: sofort wieder aus, ohne Übergang
        assert!(s.sicht_fuer(ViewKind::Persp) && !s.vis_animating());
        assert!(!s.sicht_fuer(ViewKind::Persp), "passt schon");

        assert!(s.drain_to_all(ViewKind::Persp));
        assert!(!s.drain_to_all(ViewKind::Persp), "gilt schon überall");
        assert!(ViewKind::ALL.iter().all(|v| s.drain_hidden(*v)));
        for (v, soll) in views.iter().zip(&soll) {
            assert!(gleich(&s.mesh(*v, None, &[]), soll), "{v:?}");
        }
        // Wieder einblenden: nur diese Ansicht, die Mengen blieben immer
        assert!(s.set_drain_hidden(ViewKind::Front, false));
        assert!(s.skip_vis_animation());
        assert!(gleich(&s.mesh(ViewKind::Front, None, &[]), &voll[2]));
        assert!(s.drain_hidden(ViewKind::Persp));
        assert_eq!(*s.schedule(), *baum.schedule());
    }
}

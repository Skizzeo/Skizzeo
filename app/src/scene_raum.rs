//! Räume in der Szene (Grundriss G3, G4; aus `scene.rs` ausgelagert):
//! Abgleich beim Abschluss eines Schritts, der zuletzt angelegte Raum für
//! sein Namensfeld, Stempel ziehen, Raum anlegen und benennen.

use super::Scene;
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, Space, StoreyId};

impl Scene {
    /// Beim Abschluss eines Schritts: Räume der Geschosse, deren Wände er
    /// ändert, folgen den Wänden (Öffnen allein legt keine an, alte Dateien
    /// bleiben bytegleich). Die neu angelegten warten auf ihr Namensfeld
    /// ([`Scene::neuer_raum`]).
    pub(super) fn raeume_abgleichen(&mut self) {
        let g = self.model.wand_geschosse_im_schritt();
        if g.is_empty() {
            return;
        }
        let alle =
            |s: &Scene| -> Vec<ElementId> { g.iter().flat_map(|g| s.model.spaces(*g)).collect() };
        let vorher = alle(self);
        sk_raum::abgleichen(&mut self.model, &g);
        self.neue_raeume = alle(self)
            .into_iter()
            .filter(|id| !vorher.contains(id))
            .collect();
    }

    /// Der größte neu angelegte Raum des aktiven Geschosses, einmal
    /// abgeholt (die App öffnet sein Namensfeld): Beim Teilen ist es der
    /// eine neue Teil, beim ersten Umriss oder einer Datei ohne Räume der
    /// Hauptraum. Nach Rückgängig gibt es ihn nicht mehr.
    pub fn neuer_raum(&mut self) -> Option<ElementId> {
        let neu = std::mem::take(&mut self.neue_raeume);
        let g = self.active_storey();
        sk_raum::raeume(&self.model, g)
            .into_iter()
            .filter(|f| f.raum.is_some_and(|id| neu.contains(&id)))
            .max_by(|a, b| a.flaeche.total_cmp(&b.flaeche))
            .and_then(|f| f.raum)
    }

    /// Stempel im offenen Schritt auf `p` setzen, der Anker geht mit: Wird
    /// der Raum später geteilt, behält der Teil mit dem Stempel ihn.
    pub fn raum_stempel(&mut self, id: ElementId, p: Vec3) -> bool {
        let p = vec3(p.x, p.y, 0.0);
        self.model.set_space(id, |s| {
            s.stempel = Some(p);
            s.anker = p;
        })
    }

    /// Raum für eine Fläche ohne Raum anlegen (etwa aus einer Datei vor G3,
    /// bevor dort eine Wand geändert wurde), Anker an `p`: ein Schritt.
    pub fn raum_anlegen(&mut self, g: StoreyId, p: Vec3) -> ElementId {
        self.begin("Raum angelegt");
        let name = self.model.naechster_raumname();
        let id = self.model.add_space(g, Space::neu(p, name));
        self.commit();
        id
    }

    /// Raum umbenennen, ein Schritt „Raum benannt“; `false`, wenn der Name
    /// schon so lautet oder leer ist.
    pub fn raum_benennen(&mut self, id: ElementId, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.model.space(id).is_none_or(|s| s.name == name) {
            return false;
        }
        self.begin("Raum benannt");
        self.model.set_space(id, |s| s.name = name.into());
        self.commit();
        true
    }
}

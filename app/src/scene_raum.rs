//! Räume in der Szene (Grundriss G3, G4; aus `scene.rs` ausgelagert):
//! Erkennung aus den berechneten Wandzügen mit Zwischenspeicher, Abgleich
//! beim Abschluss eines Schritts, der zuletzt angelegte Raum für sein
//! Namensfeld, Stempel ziehen, Raum anlegen und benennen.

use super::{RunCache, Scene};
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, Model, RunId, Space, StoreyId, WallChain};
use sk_raum::RaumForm;
use std::borrow::Cow;

/// Ketten für die Raumerkennung aus den berechneten Wandzügen; ein Zug,
/// der neu gerechnet werden muss, kommt aus dem Modell. Fassung ist der
/// Bauzähler des Zugs.
struct SzenenKetten<'a> {
    model: &'a Model,
    cache: &'a [Option<RunCache>],
    dirty: &'a [RunId],
    alle: bool,
}

impl SzenenKetten<'_> {
    fn frisch(&self, run: RunId) -> Option<&RunCache> {
        if self.alle || self.dirty.contains(&run) {
            return None;
        }
        self.cache
            .get(run.index() as usize)?
            .as_ref()
            .filter(|c| c.id == run)
    }
}

impl sk_raum::Ketten for SzenenKetten<'_> {
    fn fassung(&self, run: RunId) -> Option<u64> {
        self.frisch(run).map(|c| c.builds as u64)
    }

    fn kette(&self, run: RunId) -> Option<Cow<'_, WallChain>> {
        match self.frisch(run) {
            Some(c) => Some(Cow::Borrowed(&c.chain)),
            None => self.model.chain(run).map(Cow::Owned),
        }
    }
}

impl Scene {
    /// Erkannte Flächen des Geschosses; nur geänderte Hausgruppen werden
    /// neu gerechnet (Review #87 B2, B3).
    fn raumlage(&self, g: StoreyId) -> sk_raum::Lage {
        let k = SzenenKetten {
            model: &self.model,
            cache: &self.cache,
            dirty: &self.dirty,
            alle: self.all_dirty,
        };
        self.erkennung.borrow_mut().lage(&self.model, &k, g)
    }

    /// Räume des Geschosses wie [`sk_raum::raeume`], je mit Grundfläche
    /// (mm²), aus dem Zwischenspeicher der Erkennung.
    pub fn raumformen(&self, g: StoreyId) -> Vec<(RaumForm, f64)> {
        self.raumlage(g).mit_grundflaeche(&self.model)
    }

    /// Stand der Räume für jedes Geschoss, das noch keinen hat (nach dem
    /// Öffnen, Rückgängig oder einer Änderung an Räumen): der Abgleich am
    /// Ende des Schritts vergleicht mit ihm (B4).
    pub(super) fn staende_fuellen(&mut self) {
        let fehlen: Vec<StoreyId> = self
            .model
            .storeys()
            .iter()
            .map(|(id, _)| id)
            .filter(|g| !self.staende.contains_key(g))
            .collect();
        for g in fehlen {
            let st = self.raumlage(g).stand(&self.model);
            self.staende.insert(g, st);
        }
    }

    /// Beim Abschluss eines Schritts: Räume der Geschosse, deren Wände er
    /// ändert, folgen den Wänden (Öffnen allein legt keine an, alte Dateien
    /// bleiben bytegleich). Die neu angelegten warten auf ihr Namensfeld
    /// ([`Scene::neuer_raum`]).
    pub(super) fn raeume_abgleichen(&mut self) {
        let wand = self.model.wand_geschosse_im_schritt();
        // Ändert der Schritt selbst Räume, gilt ihr Stand erst wieder, wenn
        // ihn der nächste Schritt neu rechnet
        let raum = self.model.raum_geschosse_im_schritt();
        for g in raum.iter().filter(|g| !wand.contains(g)) {
            self.staende.remove(g);
        }
        if wand.is_empty() {
            return;
        }
        self.neue_raeume.clear();
        for g in wand {
            let lage = self.raumlage(g);
            let alt = self.staende.remove(&g);
            let a = sk_raum::abgleichen_lage(&mut self.model, &lage, alt);
            if !raum.contains(&g) {
                self.staende.insert(g, a.stand);
            }
            self.neue_raeume
                .extend(a.neu.into_iter().map(|(id, f)| (id, g, f)));
        }
    }

    /// Der größte neu angelegte Raum des aktiven Geschosses, einmal
    /// abgeholt (die App öffnet sein Namensfeld): Beim Teilen ist es der
    /// eine neue Teil, beim ersten Umriss oder einer Datei ohne Räume der
    /// Hauptraum. Nach Rückgängig gibt es ihn nicht mehr. Ohne neuen Raum
    /// kostet die Frage nichts (Review #87 B1).
    pub fn neuer_raum(&mut self) -> Option<ElementId> {
        if self.neue_raeume.is_empty() {
            return None;
        }
        let g = self.active_storey();
        std::mem::take(&mut self.neue_raeume)
            .into_iter()
            .filter(|&(id, gg, _)| gg == g && self.model.space(id).is_some())
            .max_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(id, _, _)| id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::{Category, RefSide};

    /// `n` Häuser 10 × 8 m nebeneinander, je eine Innenwand bei x = 3 m;
    /// die Innenwände.
    fn stadt(n: usize) -> (Scene, StoreyId, Vec<RunId>) {
        let mut s = Scene::with_model(Model::with_seed(5));
        let st = s.model().defaults().storey;
        let mut innen = Vec::new();
        for i in 0..n {
            let ox = i as f64 * 15000.0;
            let mut w = None;
            s.edit_model("Haus", |m| {
                let aw = m.defaults().exterior_wall;
                let iw = m.defaults().interior_wall;
                let t: f64 = m.wall_layers(aw).iter().map(|l| l.thickness).sum();
                let p = |x: f64, y: f64| vec3(ox + x, y, 0.0);
                let pts = [
                    p(0.0, 0.0),
                    p(0.0, 8000.0),
                    p(10000.0, 8000.0),
                    p(10000.0, 0.0),
                ];
                m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall);
                let pts = [p(3000.0, t), p(3000.0, 8000.0 - t)];
                w = m.add_wall_run(&pts, false, RefSide::Center, st, iw, Category::InteriorWall);
                w.is_some()
            });
            innen.push(w.unwrap());
        }
        (s, st, innen)
    }

    fn name_bei(s: &Scene, st: StoreyId, p: Vec3) -> String {
        let r = s.raumformen(st);
        let (f, _) = r.iter().find(|(f, _)| f.enthaelt(p)).expect("Fläche");
        s.model().space(f.raum.unwrap()).unwrap().name.clone()
    }

    fn wand_auf(s: &mut Scene, w: RunId, x: f64) {
        let pts: Vec<Vec3> = s
            .model()
            .run(w)
            .unwrap()
            .points
            .iter()
            .map(|p| vec3(x, p.y, 0.0))
            .collect();
        assert!(s.set_run_points(w, &pts));
    }

    /// Review #87 B4 in der Szene: Die Innenwand wird in einem Zug von 3 m
    /// auf 1,4 m gezogen; links bleibt das Bad, rechts das Wohnen.
    #[test]
    fn wand_gezogen_namen_bleiben() {
        let (mut s, st, innen) = stadt(1);
        for (x, n) in [(1000.0, "Bad"), (6000.0, "Wohnen")] {
            let r = s.raumformen(st);
            let id = r
                .iter()
                .find(|(f, _)| f.enthaelt(vec3(x, 4000.0, 0.0)))
                .unwrap()
                .0
                .raum;
            assert!(s.raum_benennen(id.unwrap(), n));
        }
        s.begin("Wand verschieben");
        for x in [2500.0, 2000.0, 1400.0] {
            wand_auf(&mut s, innen[0], x);
            s.raumformen(st);
        }
        s.commit();
        assert_eq!(name_bei(&s, st, vec3(800.0, 4000.0, 0.0)), "Bad");
        assert_eq!(name_bei(&s, st, vec3(5000.0, 4000.0, 0.0)), "Wohnen");
        assert!(s.neuer_raum().is_none(), "kein neuer Raum");
        assert!(sk_raum::ruhende(s.model(), st).is_empty());
        // Rückgängig und wieder: dieselben Namen
        assert!(s.undo());
        assert_eq!(name_bei(&s, st, vec3(1000.0, 4000.0, 0.0)), "Bad");
        assert!(s.redo());
        assert_eq!(name_bei(&s, st, vec3(5000.0, 4000.0, 0.0)), "Wohnen");
    }

    /// Review #87 B2: Stempel ziehen rechnet keine Fläche neu, Wand ziehen
    /// nur das Haus der Wand; B1: ohne neuen Raum fragt die App umsonst.
    #[test]
    fn ziehen_rechnet_nur_das_eine_haus() {
        let (mut s, st, innen) = stadt(3);
        s.raumformen(st);
        let gerechnet = |s: &Scene| s.erkennung.borrow().gerechnet();
        let n0 = gerechnet(&s);
        assert!(s.neuer_raum().is_some());
        assert!(s.neuer_raum().is_none());
        let r = s.raumformen(st);
        let id = r[0].0.raum.unwrap();
        s.begin("Raumstempel verschoben");
        for k in 0..5 {
            let p = r[0].0.mitte + vec3(10.0 * k as f64, 0.0, 0.0);
            assert!(s.raum_stempel(id, p));
            s.raumformen(st);
        }
        s.commit();
        s.raumformen(st);
        assert_eq!(gerechnet(&s), n0, "Stempel ziehen");
        s.begin("Wand verschieben");
        for k in 1..=5 {
            wand_auf(&mut s, innen[1], 15000.0 + 3000.0 + 100.0 * k as f64);
            s.raumformen(st);
        }
        s.commit();
        s.raumformen(st);
        assert_eq!(gerechnet(&s), n0 + 5, "je Bewegung das eine Haus");
    }
}

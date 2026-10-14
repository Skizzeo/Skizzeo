//! Räume in der Szene (Grundriss G3, G4; aus `scene.rs` ausgelagert):
//! Erkennung aus den berechneten Wandzügen mit Zwischenspeicher, Abgleich
//! beim Abschluss eines Schritts, der Stand der Räume durch Rückgängig und
//! Wiederholen, der zuletzt angelegte Raum für sein Namensfeld, Stempel
//! ziehen, Raum anlegen und benennen.

use super::{RunCache, Scene};
use sk_math::{vec3, Vec3};
use sk_model::{
    Change, Direction, ElementId, ElementKind, Guid, Model, RunId, Space, StoreyId, Txn, WallChain,
};
use std::borrow::Cow;

/// Ein Schritt im Verlauf mit dem Wechsel des Raumstands je Geschoss,
/// dessen Wände er ändert ([`Scene::staende_folgen`]).
pub(super) struct Schritt {
    pub(super) txn: Txn,
    pub(super) raeume: Vec<(StoreyId, sk_raum::Wechsel)>,
}

/// Geschosse, deren Räume der Schritt `t` ändern kann (über Wandzüge,
/// Wände oder Räume); `None`, wenn es alle sein können (Aufbauten,
/// Baustoffe, Geschosse, Gebäude).
fn raum_geschosse(t: &Txn) -> Option<Vec<StoreyId>> {
    let mut v: Vec<StoreyId> = Vec::new();
    for c in &t.changes {
        let g: Vec<StoreyId> = match c {
            Change::Run { old, new, .. } => old.iter().chain(new).map(|r| r.storey).collect(),
            Change::Element { old, new, .. } => old
                .iter()
                .chain(new)
                .filter(|e| matches!(e.kind, ElementKind::Wall(_) | ElementKind::Space(_)))
                .map(|e| e.storey)
                .collect(),
            Change::LayerSet { .. }
            | Change::Material { .. }
            | Change::Storey { .. }
            | Change::Building { .. } => return None,
            _ => continue,
        };
        for g in g {
            if !v.contains(&g) {
                v.push(g);
            }
        }
    }
    Some(v)
}

/// Ketten für die Raumerkennung aus den berechneten Wandzügen; ein Zug,
/// der neu gerechnet werden muss, kommt aus dem Modell. Fassung ist der
/// Bauzähler des Zugs.
pub(crate) struct SzenenKetten<'a> {
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
    /// Ketten der Wandzüge aus den berechneten der Szene, für Erkennung und
    /// Raumbuch (Review #112 B1).
    pub(crate) fn ketten(&self) -> SzenenKetten<'_> {
        SzenenKetten {
            model: &self.model,
            cache: &self.cache,
            dirty: &self.dirty,
            alle: self.all_dirty,
        }
    }

    /// Erkannte Flächen des Geschosses; nur geänderte Hausgruppen werden
    /// neu gerechnet (Review #87 B2, B3).
    fn raumlage(&self, g: StoreyId) -> sk_raum::Lage {
        self.erkennung
            .borrow_mut()
            .lage(&self.model, &self.ketten(), g)
    }

    /// Räume des Geschosses je Hausgruppe für die Stempel: Nur Gruppen,
    /// deren Wände oder Räume sich geändert haben, werden neu zugeteilt
    /// (Review #87 R2).
    pub fn raumgruppen(&self, g: StoreyId) -> sk_raum::Raumformen {
        self.erkennung
            .borrow_mut()
            .raumformen(&self.model, &self.ketten(), g)
    }

    /// Mengenliste mit den Räumen und ihrem Raumbuch (RB5): Räume aus der
    /// Erkennung der Szene, Züge und Platten aus ihren berechneten Ketten,
    /// die Platten erst beim ersten Raum (Review #112 H8).
    pub(super) fn mengenliste(&self) -> sk_model::qto::Schedule {
        let k = self.ketten();
        let platten = std::cell::OnceCell::new();
        let raeume = |m: &sk_model::Model, g: StoreyId| {
            let formen = self.raumgruppen(g);
            let formen = formen.flaechen().map(|f| f.form.clone());
            let p = platten.get_or_init(|| sk_raum::raumliste::Platten::neu(m, &k));
            sk_raum::mengen::raeume_mit(m, &k, p, formen)
        };
        sk_model::qto::schedule_mit(&self.model, &raeume)
    }

    /// Wie oft einer Hausgruppe die Räume neu zugeteilt wurden (Tests).
    #[cfg(test)]
    pub fn raum_zuteilungen(&self) -> u64 {
        self.erkennung.borrow().zugeteilt()
    }

    /// Wie oft die Räume eines Geschosses neu gesammelt wurden (Tests).
    #[cfg(test)]
    pub fn raum_listen(&self) -> u64 {
        self.erkennung.borrow().raumlisten()
    }

    /// Vergisst die gesammelten Räume, der nächste Abruf sammelt sie neu
    /// (Messung).
    #[cfg(test)]
    pub fn raum_listen_vergessen(&self) {
        self.erkennung.borrow_mut().raumlisten_vergessen();
    }

    /// Räume des Geschosses wie [`sk_raum::raeume`], je mit Grundfläche
    /// (mm²), über alle Hausgruppen zugeteilt (Tests; die App liest
    /// [`Scene::raumgruppen`]).
    #[cfg(test)]
    pub fn raumformen(&self, g: StoreyId) -> Vec<(sk_raum::RaumForm, f64)> {
        self.raumlage(g).mit_grundflaeche(&self.model)
    }

    /// Stand der Räume für jedes Geschoss, das noch keinen hat (nach dem
    /// Öffnen, einer Änderung an Räumen ohne Wände oder Rückgängig ohne
    /// Wechsel): der Abgleich am Ende des Schritts vergleicht mit ihm (B4).
    /// Hausgruppen, deren Wände und Räume gleich geblieben sind, kommen aus
    /// dem Speicher der Erkennung.
    pub(super) fn staende_fuellen(&mut self) {
        let fehlen: Vec<StoreyId> = self
            .model
            .storeys()
            .iter()
            .map(|(id, _)| id)
            .filter(|g| !self.staende.contains_key(g))
            .collect();
        for g in fehlen {
            let st = self
                .erkennung
                .borrow_mut()
                .stand(&self.model, &self.ketten(), g);
            self.staende.insert(g, st);
        }
    }

    /// Beim Abschluss eines Schritts: Räume der Geschosse, deren Wände er
    /// ändert, folgen den Wänden (Öffnen allein legt keine an, alte Dateien
    /// bleiben bytegleich). Die neu angelegten bekommen im selben Schritt
    /// Vorlage und Ausbautypen ihrer Nutzung ([`sk_raum::ausbau::typen_anlegen`],
    /// Einstellungen #112 B1) und warten auf ihr Namensfeld
    /// ([`Scene::neuer_raum`]). Ergebnis: wie der Stand je Geschoss
    /// wechselt, für Rückgängig und Wiederholen.
    pub(super) fn raeume_abgleichen(&mut self) -> Vec<(StoreyId, sk_raum::Wechsel)> {
        let wand = self.model.wand_geschosse_im_schritt();
        // Ändert der Schritt selbst Räume, gilt ihr Stand erst wieder, wenn
        // ihn der nächste Schritt neu rechnet
        let raum = self.model.raum_geschosse_im_schritt();
        for g in raum.iter().filter(|g| !wand.contains(g)) {
            self.staende.remove(g);
        }
        let mut wechsel = Vec::new();
        if wand.is_empty() {
            return wechsel;
        }
        self.neue_raeume.clear();
        for g in wand {
            let lage = self.raumlage(g);
            let alt = self.staende.remove(&g);
            let a = sk_raum::abgleichen_lage(&mut self.model, &lage, alt);
            for &(id, _) in &a.neu {
                sk_raum::ausbau::typen_anlegen(&mut self.model, id);
            }
            if !raum.contains(&g) {
                self.staende.insert(g, a.stand);
                wechsel.extend(a.wechsel.map(|w| (g, w)));
            }
            self.neue_raeume
                .extend(a.neu.into_iter().map(|(id, f)| (id, g, f)));
        }
        wechsel
    }

    /// Rückgängig und Wiederholen: Der Stand eines Geschosses, dessen Wände
    /// der Schritt `s` geändert hat, folgt ihm über den Wechsel aus dessen
    /// Abgleich und wird nicht neu gerechnet (Review #87 R5). Fehlt einer
    /// von beiden, und für jedes andere Geschoss, an dessen Wänden oder
    /// Räumen der Schritt etwas ändert, rechnet der nächste Schritt den
    /// Stand neu; die übrigen Geschosse behalten ihn.
    pub(super) fn staende_folgen(&mut self, s: &Schritt, dir: Direction) {
        let Some(geschosse) = raum_geschosse(&s.txn) else {
            self.staende.clear();
            return;
        };
        for g in geschosse {
            let w = s.raeume.iter().find(|(h, _)| *h == g).map(|(_, w)| w);
            match (self.staende.get_mut(&g), w) {
                (Some(st), Some(w)) => match dir {
                    Direction::Undo => st.zurueck(w),
                    Direction::Redo => st.vor(w),
                },
                _ => {
                    self.staende.remove(&g);
                }
            }
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
    /// bevor dort eine Wand geändert wurde), Anker an `p`, mit Vorlage und
    /// Ausbautypen: ein Schritt.
    pub fn raum_anlegen(&mut self, g: StoreyId, p: Vec3) -> ElementId {
        self.begin("Raum angelegt");
        let name = self.model.naechster_raumname();
        let id = self.model.add_space(g, Space::neu(p, name));
        sk_raum::ausbau::typen_anlegen(&mut self.model, id);
        self.commit();
        id
    }

    /// Raum umbenennen, ein Schritt „Raum benannt“. `vorlage`: im Stempel
    /// gewählte Raumvorlage; ohne sie wählt ein Name, der eine Vorlage
    /// nennt ([`vorlage_im_namen`]), diese. Die Vorlage setzt die Nutzung
    /// (fehlt sie im Projekt, kommt sie samt Ausbautypen hinzu), ein freier
    /// Name lässt sie stehen. `false`, wenn sich nichts ändert oder der Name
    /// leer ist.
    pub fn raum_benennen(&mut self, id: ElementId, name: &str, vorlage: Option<Guid>) -> bool {
        let name = name.trim();
        let Some(alt) = self.model.space(id).filter(|_| !name.is_empty()) else {
            return false;
        };
        let vorlage = vorlage.or_else(|| vorlage_im_namen(&self.model.space_type_choices(), name));
        if alt.name == name && vorlage.is_none_or(|g| alt.nutzung == Some(g)) {
            return false;
        }
        self.begin("Raum benannt");
        let nutzung = vorlage.and_then(|g| self.model.ensure_space_type(g));
        self.model.set_space(id, |s| {
            s.name = name.into();
            if nutzung.is_some() {
                s.nutzung = nutzung;
            }
        });
        if nutzung.is_some() {
            sk_raum::ausbau::typen_anlegen(&mut self.model, id);
        }
        self.commit();
        true
    }
}

/// Raumvorlage, die der Name `name` nennt (Bedienbarkeit 40.5): der ganze
/// Name ohne Groß- und Kleinschreibung, sonst die längste Vorlage, die als
/// eigenes Wort darin steht („Bad OG“ ist Bad, „Flur OG“ ist „Diele,
/// Flur“, die Teile nach Komma zählen einzeln). Nennt er zwei gleich lange,
/// keine.
pub fn vorlage_im_namen(vorlagen: &[(Guid, String)], name: &str) -> Option<Guid> {
    let klein = name.trim().to_lowercase();
    if let Some((g, _)) = vorlagen.iter().find(|(_, n)| n.to_lowercase() == klein) {
        return Some(*g);
    }
    let wort = |teil: &str| {
        klein.match_indices(teil).any(|(i, _)| {
            let vor = klein[..i].chars().next_back();
            let nach = klein[i + teil.len()..].chars().next();
            !vor.is_some_and(char::is_alphanumeric) && !nach.is_some_and(char::is_alphanumeric)
        })
    };
    let mut best: Option<(usize, Option<Guid>)> = None;
    for (g, n) in vorlagen {
        let n = n.to_lowercase();
        for teil in n.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            if !wort(teil) {
                continue;
            }
            let l = teil.chars().count();
            best = match best {
                Some((b, _)) if b > l => best,
                Some((b, Some(x))) if b == l && x != *g => Some((l, None)),
                Some((b, None)) if b == l => best,
                _ => Some((l, Some(*g))),
            };
        }
    }
    best.and_then(|(_, g)| g)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bedienbarkeit 40.5: Ein Name, der eine Raumvorlage als Wort nennt,
    /// wählt sie; Teile vor und nach dem Komma zählen einzeln, die längste
    /// gewinnt, zwei gleich lange heben sich auf.
    #[test]
    fn vorlage_aus_dem_namen() {
        let m = sk_model::Model::with_seed(3);
        let v = m.space_type_choices();
        let g = |name: &str| v.iter().find(|x| x.1 == name).map(|x| x.0);
        let wahl = |name: &str| vorlage_im_namen(&v, name);
        assert_eq!(wahl("bad"), g("Bad"));
        assert_eq!(wahl("Bad OG"), g("Bad"));
        assert_eq!(wahl("OG-Bad"), g("Bad"));
        assert_eq!(wahl("Flur OG"), g("Diele, Flur"));
        assert_eq!(wahl("Küche 2"), g("Küche"));
        assert_eq!(wahl("Gäste-WC"), g("Gäste-WC"));
        assert_eq!(wahl("Gästezimmer mit Bad"), g("Gästezimmer"));
        assert_eq!(wahl("Badezimmer"), None, "nur als eigenes Wort");
        assert_eq!(wahl("Speis"), None);
        assert_eq!(wahl("Diele Küche"), None, "zwei gleich lange: keine");
    }
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
            assert!(s.raum_benennen(id.unwrap(), n, None));
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

    /// Jeder gehaltene Stand gleicht dem frisch gerechneten.
    fn staende_wie_neu(s: &Scene) {
        for (&g, st) in &s.staende {
            let neu = sk_raum::Erkennung::default().stand(s.model(), &s.ketten(), g);
            assert_eq!(st.zuordnung(), neu.zuordnung(), "Geschoss {}", g.index());
        }
    }

    /// Review #87 R5: Rückgängig und Wiederholen führen den Stand der Räume
    /// über den Wechsel aus dem Abgleich nach, statt ihn neu zu rechnen; ein
    /// anderes Geschoss behält seinen. Zufällige Schritte (Wand ziehen, Wand
    /// einziehen und entfernen, Raum benennen, Anker in eine andere Fläche
    /// setzen, Rückgängig, Wiederholen) halten nur Stände, die der frischen
    /// Rechnung gleichen.
    #[test]
    fn stand_folgt_rueckgaengig() {
        let (mut s, st, innen) = stadt(3);
        let og = s
            .model()
            .storeys()
            .iter()
            .map(|(id, _)| id)
            .find(|g| *g != st)
            .expect("zweites Geschoss");
        s.edit_model("Haus oben", |m| {
            let aw = m.defaults().exterior_wall;
            let pts = [
                vec3(0.0, 20000.0, 0.0),
                vec3(0.0, 28000.0, 0.0),
                vec3(10000.0, 28000.0, 0.0),
                vec3(10000.0, 20000.0, 0.0),
            ];
            m.add_wall_run(&pts, true, RefSide::Left, og, aw, Category::ExteriorWall)
                .is_some()
        });
        s.begin("Wand");
        wand_auf(&mut s, innen[1], 5000.0);
        s.commit();
        assert!(s.staende.contains_key(&st) && s.staende.contains_key(&og));
        staende_wie_neu(&s);
        let z = s.raum_zuteilungen();
        assert!(s.undo());
        assert!(s.staende.contains_key(&st) && s.staende.contains_key(&og));
        staende_wie_neu(&s);
        assert!(s.redo());
        assert!(s.staende.contains_key(&st) && s.staende.contains_key(&og));
        staende_wie_neu(&s);
        assert_eq!(s.raum_zuteilungen(), z, "nichts neu zugeteilt");
        // Zufällige Schritte
        let mut zufall = 0x2545_f491_4f6c_dd1du64;
        let mut wuerfel = |n: u64| {
            zufall ^= zufall << 13;
            zufall ^= zufall >> 7;
            zufall ^= zufall << 17;
            zufall % n
        };
        let mut quer: Vec<RunId> = Vec::new();
        for _ in 0..120 {
            match wuerfel(7) {
                0 => {
                    let w = innen[wuerfel(3) as usize];
                    s.begin("Wand ziehen");
                    wand_auf(&mut s, w, 1000.0 + wuerfel(8) as f64 * 1000.0);
                    s.commit();
                }
                1 => {
                    let ox = wuerfel(3) as f64 * 15000.0;
                    let y = 2000.0 + wuerfel(4) as f64 * 1500.0;
                    let mut w = None;
                    s.edit_model("Querwand", |m| {
                        let iw = m.defaults().interior_wall;
                        let pts = [vec3(ox + 300.0, y, 0.0), vec3(ox + 9700.0, y, 0.0)];
                        w = m.add_wall_run(
                            &pts,
                            false,
                            RefSide::Center,
                            st,
                            iw,
                            Category::InteriorWall,
                        );
                        w.is_some()
                    });
                    quer.extend(w);
                }
                2 => {
                    if let Some(w) = quer.pop() {
                        s.edit_model("Querwand weg", |m| m.remove_run(w));
                    }
                }
                3 => {
                    let ids = s.model().spaces(st);
                    if !ids.is_empty() {
                        let id = ids[wuerfel(ids.len() as u64) as usize];
                        s.raum_benennen(id, &format!("Raum {}", wuerfel(1000)), None);
                    }
                }
                4 => {
                    let ids = s.model().spaces(st);
                    if !ids.is_empty() {
                        let id = ids[wuerfel(ids.len() as u64) as usize];
                        let p = vec3(
                            wuerfel(3) as f64 * 15000.0 + 500.0 + wuerfel(9) as f64 * 1000.0,
                            500.0 + wuerfel(7) as f64 * 1000.0,
                            0.0,
                        );
                        s.edit_model("Anker", |m| m.set_space(id, |r| r.anker = p));
                    }
                }
                5 => {
                    s.undo();
                }
                _ => {
                    s.redo();
                }
            }
            staende_wie_neu(&s);
            // Wie der nächste Schritt: Fehlende Stände rechnet er
            s.staende_fuellen();
            staende_wie_neu(&s);
        }
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
        // R2: Zuteilen ebenso nur das eine Haus, ohne Änderung keins
        s.raumgruppen(st);
        let z0 = s.raum_zuteilungen();
        s.raumgruppen(st);
        assert_eq!(s.raum_zuteilungen(), z0);
        s.begin("Wand verschieben");
        for k in 1..=5 {
            wand_auf(&mut s, innen[2], 30000.0 + 3000.0 + 100.0 * k as f64);
            let r = s.raumgruppen(st);
            let mut v: Vec<_> = r
                .flaechen()
                .map(|f| (f.form.clone(), f.grundflaeche))
                .collect();
            let mut w = s.raumformen(st);
            let mitte = |f: &(sk_raum::RaumForm, f64)| (f.0.mitte.x, f.0.mitte.y);
            v.sort_by(|a, b| mitte(a).partial_cmp(&mitte(b)).unwrap());
            w.sort_by(|a, b| mitte(a).partial_cmp(&mitte(b)).unwrap());
            assert_eq!(v, w);
            assert_eq!(s.raum_zuteilungen(), z0 + k, "je Bewegung das eine Haus");
        }
        s.commit();
    }

    /// Review H-137b: Nach dem Loslassen einer Wand, deren Räume ihr mit
    /// dem Anker folgen, nach dem Verschieben eines Raumstempels und nach
    /// Rückgängig sammelt die Zuteilung die Räume des Geschosses nicht neu
    /// über alle Bauteile, sie reiht nur die geänderten neu ein. Das
    /// Ergebnis ist jedes Mal das der frischen Erkennung.
    #[test]
    fn loslassen_ohne_neue_raumliste() {
        let (mut s, st, innen) = stadt(3);
        let listen = |s: &Scene| s.raum_listen();
        let wie_neu = |s: &Scene| {
            let mitte = |f: &(sk_raum::RaumForm, f64)| (f.0.mitte.x, f.0.mitte.y);
            let mut v: Vec<_> = s
                .raumgruppen(st)
                .flaechen()
                .map(|f| (f.form.clone(), f.grundflaeche))
                .collect();
            let mut w = s.raumformen(st);
            v.sort_by(|a, b| mitte(a).partial_cmp(&mitte(b)).unwrap());
            w.sort_by(|a, b| mitte(a).partial_cmp(&mitte(b)).unwrap());
            assert_eq!(v, w);
        };
        wie_neu(&s);
        let n0 = listen(&s);
        for k in 1..=3 {
            s.begin("Wand verschieben");
            wand_auf(&mut s, innen[1], 15000.0 + 3000.0 + 100.0 * k as f64);
            s.commit();
            wie_neu(&s);
        }
        assert_eq!(listen(&s), n0, "Wandschritte");
        // Stempel eines Raums verschieben: ein Schritt an Räumen
        let r = s.raumformen(st);
        let id = r[0].0.raum.unwrap();
        s.begin("Raumstempel verschoben");
        assert!(s.raum_stempel(id, r[0].0.mitte + vec3(200.0, 0.0, 0.0)));
        s.commit();
        wie_neu(&s);
        assert!(s.undo());
        wie_neu(&s);
        assert!(s.undo());
        wie_neu(&s);
        assert_eq!(listen(&s), n0, "Stempel und Rückgängig");
        // Stempel im offenen Schritt ziehen, Bild für Bild, dann verwerfen
        // und noch einmal bis zum Loslassen (Review H-175h)
        for loslassen in [false, true] {
            s.begin("Raumstempel verschoben");
            for i in 1..=4 {
                let d = vec3(150.0 * i as f64, 50.0 * i as f64, 0.0);
                assert!(s.raum_stempel(id, r[0].0.mitte + d));
                wie_neu(&s);
            }
            if loslassen {
                s.commit();
            } else {
                s.rollback();
            }
            wie_neu(&s);
        }
        assert_eq!(listen(&s), n0, "Stempel im offenen Schritt");
    }
}

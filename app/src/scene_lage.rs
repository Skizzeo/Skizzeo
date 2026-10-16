//! Lage, Nordpfeil und Gelände ziehen in der Szene, aus `scene.rs`
//! ausgelagert. Mit Höhenaufmaß (Review #123 W45 B3): Liegt ein Aufmaß im
//! Projekt, bewegen Lage und Nordrichtung das Gelände unter dem Haus und
//! mit ihm die Gründungen. Ändert sich dabei eine Unterkante, rechnet die
//! Szene alles neu; sonst blieben 3D-Körper und Mengen beim alten Stand.
//! Beim Ziehen von OK Gelände folgen so die Züge der Gebäude, deren
//! Gründung sich bewegt hat (W45-Nachprüfung N1), die anderen Gebäude erst
//! beim Loslassen (Review #123 B1 zum Gelände-Zug), im Modell sofort, ihre
//! Körper im Leerlauf (Review #123 F1).

use super::Scene;
use sk_model::{BuildingId, ElementKind, LevelKind, RunId, StoreyId};
use std::collections::HashSet;
use std::time::{Duration, Instant};

impl Scene {
    /// Projektdaten samt Breite und Länge aus der Maske (Sonnenstand S1)
    /// als ein Schritt. `false`, wenn sich nichts geändert hat.
    pub fn projektdaten_setzen(
        &mut self,
        label: &'static str,
        p: sk_model::Project,
        ort: sk_model::Location,
    ) -> bool {
        self.begin(label);
        let uk = self.gruendungen_uk();
        let soil = p.soil;
        let a = self.model.set_project(p);
        let b = self.model.set_location(ort);
        // Bodenkennwerte der Erdarbeiten (Gruppe Baugrund) im selben Schritt
        let c = self.model.set_soil(soil);
        self.gruendungen_folgen(&uk);
        self.commit();
        a || b || c
    }

    /// Nordpfeil setzen, drehen oder verschieben (Sonnenstand S2): ein
    /// Schritt `label`. `false`, wenn sich nichts geändert hat.
    pub fn nordpfeil_setzen(
        &mut self,
        label: &'static str,
        nord: f64,
        fuss: Option<sk_model::Foot>,
    ) -> bool {
        self.begin(label);
        let uk = self.gruendungen_uk();
        let ok = self.model.set_north_arrow(nord, fuss);
        self.gruendungen_folgen(&uk);
        self.commit();
        ok
    }

    /// OK Gelände an der Gründung des aktiven Gebäudes beim Ziehen (Kote
    /// `z`, Gelände Thema 1), geklemmt. Seine Gründung folgt; neu sind nur
    /// seine Züge mit Sohlplatte, denn die Kanten teilt der Netzbau ohnehin
    /// je Bild an OK Gelände. Ohne Aufmaß bleiben andere Gebäude
    /// (Entscheidung B). Mit Aufmaß wandert ±0,00 aller Gebäude; die
    /// anderen stehen bis zum Loslassen auf ihrer alten Gründung, dann
    /// folgen sie in einem Gang ([`Scene::commit`], Review #123 B1 zum
    /// Gelände-Zug). Je Bild kostet das so viel wie das gezogene Gebäude.
    /// Die Mengen rechnet das Loslassen nach (Review Gelände).
    pub fn drag_terrain(&mut self, z: f64) {
        let Some(gr) = self.model.foundation_level_of(self.active_storey()) else {
            return;
        };
        let Some(b) = self.model.building_of(gr).filter(|_| z.is_finite()) else {
            return;
        };
        if self.gelaende_zug.is_none() {
            let Some(zug) = self.model.gelaende_zug(b) else {
                return;
            };
            self.gelaende_zug = Some((zug, self.platten_zuege(&[b])));
        }
        let (lo, hi) = self.model.terrain_z_of_level_range(gr);
        let Some((zug, runs)) = &self.gelaende_zug else {
            return;
        };
        if !self.model.gelaende_ziehen_auf(zug, gr, z.clamp(lo, hi)) {
            return;
        }
        for r in runs.clone() {
            self.mark(r);
        }
        self.rebuild_dirty(true);
    }

    /// Loslassen eines Zugs an OK Gelände (aus [`Scene::commit`]): Die
    /// übrigen Gründungen folgen im Modell. Die Züge mit Sohlplatte der
    /// Gebäude, deren Unterkante sich bewegt hat, werden im Leerlauf neu
    /// ([`Scene::nachziehen`]), nicht im Loslassen: Bei 1000 Häusern mit
    /// Aufmaß kostete es sonst 195 ms statt 41,5 wie ohne Aufmaß, bei 4000
    /// Häusern 15,3 s statt 6,1 (Review #123 F1).
    pub(super) fn gelaende_loslassen(&mut self) {
        let Some((zug, _)) = self.gelaende_zug.take() else {
            return;
        };
        let bewegt = self.model.gelaende_loslassen(zug);
        if !bewegt.is_empty() {
            let schon: HashSet<RunId> = self.nachzug.iter().copied().collect();
            let neu = self.platten_zuege(&bewegt);
            self.nachzug
                .extend(neu.into_iter().filter(|r| !schon.contains(r)));
        }
    }

    /// Im Leerlauf, nicht in einem offenen Schritt: Züge, die einem
    /// Gelände-Zug folgen, neu rechnen, bis `budget` um ist (mindestens
    /// einen). `None`, wenn nichts zu tun ist; `Some(true)`, wenn jetzt alle
    /// neu sind (dann folgen Bild und Mengen), sonst `Some(false)`.
    pub fn nachziehen(&mut self, budget: Duration) -> Option<bool> {
        if self.nachzug.is_empty() || self.model.in_step() {
            return None;
        }
        let t0 = Instant::now();
        while let Some(id) = self.nachzug.pop() {
            self.mass_zug(id);
            let b = self.cached(id).map_or(0, |c| c.builds);
            let rc = super::neubau::rechne(&self.model, id, b, false);
            self.store(id, rc, false);
            if t0.elapsed() >= budget {
                break;
            }
        }
        let fertig = self.nachzug.is_empty();
        if fertig {
            self.bounds_neu();
        }
        Some(fertig)
    }

    /// Folgen noch Züge einem Gelände-Zug ([`Scene::nachziehen`])?
    pub fn zieht_nach(&self) -> bool {
        !self.nachzug.is_empty()
    }

    /// Züge, die ohnehin neu werden, folgen nicht noch einmal.
    pub(super) fn nachzug_ohne(&mut self, ids: &[RunId]) {
        if !self.nachzug.is_empty() && !ids.is_empty() {
            let neu: HashSet<RunId> = ids.iter().copied().collect();
            self.nachzug.retain(|r| !neu.contains(r));
        }
    }

    /// Züge mit Sohlplatte der Gebäude `haeuser`.
    fn platten_zuege(&self, haeuser: &[BuildingId]) -> Vec<RunId> {
        let haeuser: HashSet<BuildingId> = haeuser.iter().copied().collect();
        let m = &self.model;
        m.elements()
            .iter()
            .filter_map(|(_, e)| match e.kind {
                ElementKind::GroundSlab(s) => Some(s.run),
                _ => None,
            })
            .filter(|r| {
                let b = m.run(*r).and_then(|x| m.building_of(x.storey));
                b.is_some_and(|b| haeuser.contains(&b))
            })
            .collect()
    }

    /// Unterkanten aller Gründungen.
    fn gruendungen_uk(&self) -> Vec<(StoreyId, f64)> {
        self.model
            .storeys()
            .iter()
            .filter(|(_, s)| s.kind == LevelKind::Foundation)
            .map(|(id, s)| (id, s.elevation))
            .collect()
    }

    /// Alles neu, wenn eine Unterkante nicht mehr wie `vorher` liegt.
    fn gruendungen_folgen(&mut self, vorher: &[(StoreyId, f64)]) {
        if self.gruendungen_uk() != vorher {
            self.mark_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{Crs, Location, Model, Terrain, TerrainPoint, Utm};

    const E0: f64 = 467_100.0;
    const N0: f64 = 5_878_700.0;

    /// Haus 10 × 8 m auf einem Aufmaß an seinem Platz (UTM 32), das nach
    /// Osten gewölbt ansteigt.
    fn haus_am_hang() -> Scene {
        haeuser_am_hang(&[0.0])
    }

    /// Häuser 10 × 8 m, je eines ab `ox` (mm) nach Osten, auf demselben
    /// Aufmaß; das erste bestimmt die Höhenlage.
    fn haeuser_am_hang(ox: &[f64]) -> Scene {
        let mut m = Model::new();
        for &ox in ox {
            let b = m.add_building(1);
            let ecken = [
                (0.0, 0.0),
                (0.0, 8000.0),
                (10_000.0, 8000.0),
                (10_000.0, 0.0),
            ];
            let ecken: Vec<_> = ecken.iter().map(|&(x, y)| vec3(ox + x, y, 0.0)).collect();
            m.build_from_polygon(b, &ecken).unwrap();
        }
        let mut t = Terrain::new(Crs::Utm(32));
        for i in 0..7 {
            for j in 0..7 {
                let (e, n) = (E0 - 20.0 + 10.0 * i as f64, N0 - 20.0 + 10.0 * j as f64);
                t.points.push(TerrainPoint {
                    nr: (i * 7 + j + 1).to_string(),
                    e,
                    n,
                    h: 30.0 + 0.001 * (e - E0).powi(2),
                    code: String::new(),
                    off: false,
                    hand: false,
                    gemessen: None,
                });
            }
        }
        m.begin("Aufmaß");
        let l = Location {
            utm: Some(Utm {
                e: E0,
                n: N0,
                zone: 32,
            }),
            ..m.location().normalized()
        };
        assert!(m.set_location(l));
        assert!(m.import_terrain(t));
        m.commit();
        Scene::with_model(m)
    }

    /// Höhenpunkt setzen mit dem Netz im Faden (Review #234 B-234b): Der
    /// Schritt lässt das alte Netz und die Gründung stehen; das Netz aus
    /// dem Faden bringt die Gründung dahin, wo sie ohne Faden stünde, im
    /// selben Schritt des Verlaufs.
    #[test]
    fn netz_im_faden_ein_schritt() {
        let uk = |s: &Scene| -> Vec<f64> { s.gruendungen_uk().iter().map(|x| x.1).collect() };
        let mut s = haus_am_hang();
        let (uk0, n0) = (uk(&s), s.model().terrain().unwrap().points.len());
        let netz0 = s.model().terrain().unwrap().mesh();
        // tief neben dem Haus: die Gründung geht mit
        let tief = s
            .model()
            .terrain_spots()
            .iter()
            .map(|x| x.p[2])
            .fold(f64::MAX, f64::min);
        let p = [5000.0, -1000.0, tief - 2000.0];
        let setzen = |m: &mut Model| !m.add_terrain_points(&[p]).is_empty();
        assert!(s.edit_model("Höhenpunkt", setzen));
        assert!(std::sync::Arc::ptr_eq(
            &s.model().terrain().unwrap().mesh(),
            &netz0
        ));
        assert_eq!(uk(&s), uk0);
        assert!(s.netz_rechnet());
        assert!(s.netz_abwarten());
        assert!(!s.netz_rechnet());
        let uk1 = uk(&s);
        assert_ne!(uk1, uk0, "die Gründung ist mitgegangen");
        let mut ohne = haus_am_hang();
        assert!(ohne.edit_model("Höhenpunkt", |m| {
            m.netz_im_faden(false);
            setzen(m)
        }));
        assert!(!ohne.netz_rechnet());
        assert_eq!(uk(&ohne), uk1);
        // ein Rückgängig nimmt beides zurück, Wiederholen bringt beides
        assert_eq!(s.undo_label(), Some("Höhenpunkt"));
        assert!(s.undo());
        assert_eq!(
            (uk(&s), s.model().terrain().unwrap().points.len()),
            (uk0, n0)
        );
        assert!(s.redo());
        assert_eq!(uk(&s), uk1);
        assert!(!s.netz_rechnet(), "das Gelände hat sein Netz");
    }

    /// Mengen, Kosten, Datei und IFC gleich nach dem Setzen eines Punkts
    /// kommen aus dem fertigen Netz, nicht aus dem vorläufigen
    /// (Integration zu #234 B-234b): wie in der Szene ohne Faden und wie
    /// nach dem Öffnen der Datei.
    #[test]
    fn mengen_warten_auf_das_netz() {
        let tief = |s: &Scene| {
            s.model()
                .terrain_spots()
                .iter()
                .map(|x| x.p[2])
                .fold(f64::MAX, f64::min)
        };
        let u = sk_model::qto::Umfang::projekt();
        let netto = |s: &mut Scene| s.kosten_vorschau(None, &[], &u).unwrap().netto;
        let mut s0 = haus_am_hang();
        let p = [5000.0, -1000.0, tief(&s0) - 2000.0];
        let setzen = |m: &mut Model| !m.add_terrain_points(&[p]).is_empty();
        let (mengen0, kosten0) = (s0.schedule().clone(), netto(&mut s0));
        // alle aus demselben Modell, damit auch die Datei gleich ist
        let mut ohne = Scene::with_model(s0.model().clone());
        assert!(ohne.edit_model("Höhenpunkt", |m| {
            m.netz_im_faden(false);
            setzen(m)
        }));
        let (mengen, kosten) = (ohne.schedule().clone(), netto(&mut ohne));
        assert_ne!(mengen, mengen0, "der Punkt ändert die Mengen");
        assert_ne!(kosten, kosten0, "der Punkt ändert die Kosten");
        // je Weg eine frische Szene, deren Netz noch im Faden ist
        let frisch = || {
            let mut s = Scene::with_model(s0.model().clone());
            assert!(s.edit_model("Höhenpunkt", setzen));
            assert!(s.netz_rechnet() && s.model().netz_vorlaeufig());
            s
        };
        let mut s = frisch();
        assert_eq!(*s.schedule(), mengen, "Mengenliste");
        assert!(!s.netz_rechnet() && !s.model().netz_vorlaeufig());
        let mut s = frisch();
        assert_eq!(netto(&mut s), kosten, "Kosten");
        // Speichern und wieder öffnen
        let mut s = frisch();
        let text = sk_model::szo::write(s.model_fertig());
        assert!(!s.model().netz_vorlaeufig());
        let soll = sk_model::szo::write(ohne.model());
        let anders: Vec<_> = text
            .lines()
            .zip(soll.lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert!(
            anders.is_empty() && text.len() == soll.len(),
            "Datei: {anders:?}"
        );
        let guids = sk_model::GuidGen::from_time();
        let l = sk_model::szo::read_with(&text, guids, &sk_cost::lesen::ABSCHNITTE_SZO);
        let mut offen = Scene::with_model(l.unwrap().model);
        assert_eq!(netto(&mut offen), kosten, "Kosten nach dem Öffnen");
        // IFC schreibt das fertige Netz
        let mut s = frisch();
        let _ = s.model_fertig();
        assert!(!s.netz_rechnet() && !s.model().netz_vorlaeufig());
        // die Änderungen beim Einsetzen gehören zum Schritt des Punkts
        assert!(s.undo());
        assert_eq!(*s.schedule(), mengen0);
        // auch wenn gleich der nächste Schritt beginnt
        let mut s = frisch();
        let weit = [60_000.0, 60_000.0, p[2]];
        assert!(s.edit_model("Weit weg", |m| !m.add_terrain_points(&[weit]).is_empty()));
        assert!(s.undo());
        assert_eq!(s.undo_label(), Some("Höhenpunkt"));
        assert_eq!(*s.schedule(), mengen, "der erste Punkt mit seinen Folgen");
        assert!(s.undo());
        assert_eq!(*s.schedule(), mengen0);
    }

    /// Projektdaten und Nordpfeil bewegen mit Aufmaß die Gründung; die
    /// Körper in 3D folgen im selben Schritt.
    #[test]
    fn lage_bewegt_gruendung_und_koerper() {
        let mut s = haus_am_hang();
        let gr = s.model().foundation_level().unwrap();
        let uk = |s: &Scene| s.model().storey(gr).unwrap().elevation;
        let tief = |s: &Scene| s.bounds().unwrap().0.z;
        assert!((tief(&s) - uk(&s)).abs() < 1e-3, "{} {}", tief(&s), uk(&s));
        let vorher = uk(&s);
        let ort = Location {
            utm: Some(Utm {
                e: E0 + 5.0,
                n: N0,
                zone: 32,
            }),
            ..*s.model().location()
        };
        let p = s.model().project().clone();
        assert!(s.projektdaten_setzen("Projektdaten geändert", p, ort));
        assert!((uk(&s) - vorher).abs() > 50.0, "{vorher} {}", uk(&s));
        assert!((tief(&s) - uk(&s)).abs() < 1e-3, "{} {}", tief(&s), uk(&s));
        let vorher = uk(&s);
        assert!(s.nordpfeil_setzen("Nordpfeil gedreht", 20.0, None));
        assert!((uk(&s) - vorher).abs() > 1.0, "{vorher} {}", uk(&s));
        assert!((tief(&s) - uk(&s)).abs() < 1e-3, "{} {}", tief(&s), uk(&s));
    }

    /// Gelände ziehen mit Aufmaß (W45-Nachprüfung N1): Das erste Haus
    /// bestimmt ±0,00. Zieht man sein Gelände, folgt die Gründung des
    /// zweiten im Modell, und die Szene zeigt dasselbe wie nach einem
    /// Neuaufbau.
    #[test]
    fn gelaende_ziehen_mit_aufmass_wie_neuaufbau() {
        use crate::ui::ViewKind;
        let mut s = haeuser_am_hang(&[0.0, 20_000.0]);
        let m = s.model();
        let erstes = m.building_of(m.defaults().storey);
        assert_eq!(m.level_building(), erstes);
        let gr2 = m
            .storeys()
            .iter()
            .find(|(_, x)| x.kind == LevelKind::Foundation && x.building != erstes)
            .map(|(id, _)| id)
            .unwrap();
        let uk2 = |s: &Scene| s.model().storey(gr2).unwrap().elevation;
        let bild = |s: &mut Scene| {
            let m = s.mesh(ViewKind::Front, None, &[]);
            (m.faces.clone(), m.edges.clone())
        };
        let vorher = uk2(&s);
        s.begin("Gelände ziehen");
        for z in [-300.0, 250.0, -900.0] {
            s.drag_terrain(z);
            // das andere Haus folgt erst beim Loslassen
            assert_eq!(uk2(&s), vorher);
        }
        s.commit();
        let nachher = uk2(&s);
        assert!((nachher - vorher).abs() > 100.0, "{vorher} {nachher}");
        let mut neu = Scene::with_model(s.model().clone());
        // seine Körper folgen im Leerlauf (Review #123 F1), nicht in einem
        // offenen Schritt
        assert!(s.zieht_nach() && bild(&mut s) != bild(&mut neu));
        s.begin("Gelände ziehen");
        assert_eq!(s.nachziehen(Duration::ZERO), None);
        s.rollback();
        while s.nachziehen(Duration::ZERO) == Some(false) {}
        assert!(!s.zieht_nach());
        assert!(bild(&mut s) == bild(&mut neu), "Bild wie nach Neuaufbau");
        assert_eq!(s.bounds(), neu.bounds());
        // Esc lässt Gelände und Gründungen, wie sie waren
        let nhn = s.model().terrain_nhn();
        s.begin("Gelände ziehen");
        s.drag_terrain(600.0);
        s.rollback();
        assert_eq!((uk2(&s), s.model().terrain_nhn()), (nachher, nhn));
        assert!(bild(&mut s) == bild(&mut neu), "Bild nach Esc");
        // Rückgängig nimmt Zug und Nachziehen in einem Schritt zurück
        assert!(s.undo());
        assert_eq!(uk2(&s), vorher);
        while s.nachziehen(Duration::ZERO) == Some(false) {}
        let mut alt = Scene::with_model(s.model().clone());
        assert!(bild(&mut s) == bild(&mut alt), "Bild nach Rückgängig");
    }

    /// `n` Häuser 10 × 8 m im Raster zu 15 m, je eines ein Gebäude, mit
    /// oder ohne gewelltes örtliches Aufmaß darunter (wie im Budget von
    /// sk-model).
    fn viele_haeuser(n: usize, aufmass: bool) -> Scene {
        let mut m = Model::with_seed(5);
        for i in 0..n {
            let b = m.add_building(1);
            let (ox, oy) = ((i % 64) as f64 * 15_000.0, (i / 64) as f64 * 15_000.0);
            let ecken = [
                (0.0, 0.0),
                (0.0, 8000.0),
                (10_000.0, 8000.0),
                (10_000.0, 0.0),
            ];
            let ecken: Vec<_> = ecken
                .iter()
                .map(|&(x, y)| vec3(ox + x, oy + y, 0.0))
                .collect();
            m.build_from_polygon(b, &ecken).unwrap();
        }
        if aufmass {
            let tiefe = n.div_ceil(64) as f64 * 15.0 + 80.0;
            let (nx, ny) = (53, (tiefe / 20.0) as usize + 1);
            let mut t = Terrain::new(Crs::Local);
            for i in 0..nx {
                for j in 0..ny {
                    let (e, nn) = (1000.0 + 20.0 * i as f64, 2000.0 + 20.0 * j as f64);
                    t.points.push(TerrainPoint {
                        nr: (i * ny + j + 1).to_string(),
                        e,
                        n: nn,
                        h: 50.0 + 3.0 * (e / 37.0).sin() + 2.0 * (nn / 23.0).cos(),
                        code: String::new(),
                        off: false,
                        hand: false,
                        gemessen: None,
                    });
                }
            }
            m.begin("Aufmaß");
            assert!(m.import_terrain(t));
            m.commit();
        }
        Scene::with_model(m)
    }

    /// Gelände-Züge an `n` Häusern ([`viele_haeuser`]) wie in Review zu
    /// #153 gemessen: einmal aufwärmen, dann Bestwert des Loslassens aus
    /// mindestens sieben Zügen über mindestens 300 ms, denn die Tests laufen
    /// nebeneinander. Dazu die Zeit des Nachziehens im Leerlauf und seine
    /// Durchläufe nach dem ersten. Mit Aufmaß steht danach dasselbe Bild wie
    /// nach einem Neuaufbau.
    fn loslassen(n: usize, aufmass: bool) -> (f64, f64, usize) {
        use crate::ui::ViewKind;
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
        let mut s = viele_haeuser(n, aufmass);
        let (mut best, mut nach, mut runden) = (f64::INFINITY, 0.0, 0);
        let start = Instant::now();
        for (k, z) in [-300.0, -650.0, -450.0, -800.0, -350.0]
            .into_iter()
            .cycle()
            .take(200)
            .enumerate()
        {
            s.begin("Gelände ziehen");
            s.drag_terrain(z + 40.0);
            s.drag_terrain(z);
            let t0 = Instant::now();
            s.commit();
            if k > 0 {
                best = best.min(ms(t0));
            }
            assert_eq!(s.zieht_nach(), aufmass);
            loop {
                let t0 = Instant::now();
                let r = s.nachziehen(Duration::from_millis(6));
                nach += ms(t0);
                if r != Some(false) {
                    break;
                }
                runden += 1;
            }
            if k >= 7 && ms(start) >= 300.0 {
                break;
            }
        }
        if aufmass {
            let mut neu = Scene::with_model(s.model().clone());
            let bild = |s: &mut Scene| s.mesh(ViewKind::Front, None, &[]).faces.len();
            assert_eq!(bild(&mut s), bild(&mut neu));
            assert_eq!(s.bounds(), neu.bounds());
        }
        eprintln!(
            "{n} Häuser {} Aufmaß: Loslassen {best:.1} ms (Bestwert), \
             Nachziehen {nach:.1} ms in {runden} weiteren Durchläufen",
            if aufmass { "mit" } else { "ohne" },
        );
        (best, nach, runden)
    }

    /// Loslassen eines Gelände-Zugs (Review #123 F1: mit Aufmaß 195 ms bei
    /// 1000 Häusern, ohne 41,5): Mit Aufmaß folgen die übrigen Gründungen
    /// im Modell, ihre Körper erst im Leerlauf, je Durchlauf im Budget. Das
    /// Loslassen bleibt bei 1000 Häusern mit und ohne Aufmaß unter 8 ms
    /// (Review L42; vorher mit Aufmaß 8,8 bis 9,8 ms, weil jede Gründung
    /// ihr Geschoss und das EG in den Abgleich brachte).
    #[test]
    fn loslassen_ohne_aufschlag() {
        let n = if cfg!(debug_assertions) { 64 } else { 1000 };
        let grenze = if cfg!(debug_assertions) { 400.0 } else { 8.0 };
        for aufmass in [false, true] {
            let (los, ..) = loslassen(n, aufmass);
            assert!(los < grenze, "{n} Häuser, Aufmaß {aufmass}: {los:.1} ms");
        }
    }

    /// Messprogramm (Review L42): dasselbe bei 4000 Häusern, nur im Release
    /// sinnvoll: `cargo test -p skizzeo --release loslassen_4000 --
    /// --ignored --nocapture`.
    #[test]
    #[ignore = "Messprogramm, 4000 Häuser"]
    fn loslassen_4000() {
        for aufmass in [false, true] {
            let (los, ..) = loslassen(4000, aufmass);
            assert!(los < 8.0, "4000 Häuser, Aufmaß {aufmass}: {los:.1} ms");
        }
    }

    /// Die automatische Sicherung wartet, solange ein Gelände-Zug offen
    /// ist (Review #123 F2): ±0,00 steht dann schon neu, die übrigen
    /// Gründungen noch alt. Nach dem Loslassen sichert sie den fertigen
    /// Stand.
    #[test]
    fn sicherung_nicht_im_gelaende_zug() {
        use crate::autosave::AutoSave;
        use crate::document::Document;
        let d = std::env::temp_dir().join(format!("skizzeo-sicherung-zug-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut s = haeuser_am_hang(&[0.0, 20_000.0]);
        let doc = Document::opened(d.join("haus.szo"), s.model().revision());
        let mut a = AutoSave::new(d.clone());
        let t = |min: u64| Duration::from_secs(min * 60);
        assert_eq!(a.tick(s.model(), &doc, t(0)), None);
        s.begin("Gelände ziehen");
        s.drag_terrain(-500.0);
        assert!(doc.is_dirty(s.model()));
        assert_eq!(a.wait(s.model(), &doc, t(6)), None);
        assert_eq!(a.tick(s.model(), &doc, t(6)), None, "im offenen Schritt");
        s.commit();
        let p = a.tick(s.model(), &doc, t(6)).expect("nach dem Loslassen");
        let gesichert = std::fs::read_to_string(&p).unwrap();
        assert_eq!(gesichert, sk_model::szo::write(s.model()));
        let _ = std::fs::remove_dir_all(&d);
    }
}

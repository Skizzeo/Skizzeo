//! Lage, Nordpfeil und Gelände ziehen in der Szene, aus `scene.rs`
//! ausgelagert. Mit Höhenaufmaß (Review #123 W45 B3): Liegt ein Aufmaß im
//! Projekt, bewegen Lage und Nordrichtung das Gelände unter dem Haus und
//! mit ihm die Gründungen. Ändert sich dabei eine Unterkante, rechnet die
//! Szene alles neu; sonst blieben 3D-Körper und Mengen beim alten Stand.
//! Beim Ziehen von OK Gelände folgen so die Züge der Gebäude, deren
//! Gründung sich bewegt hat (W45-Nachprüfung N1), die anderen Gebäude erst
//! beim Loslassen (Review #123 B1 zum Gelände-Zug).

use super::Scene;
use sk_model::{BuildingId, ElementKind, LevelKind, RunId, StoreyId};
use std::collections::HashSet;

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

    /// OK Gelände des aktiven Gebäudes beim Ziehen (Kote `z`, Gelände
    /// Thema 1), geklemmt. Seine Gründung folgt; neu sind nur seine Züge mit
    /// Sohlplatte, denn die Kanten teilt der Netzbau ohnehin je Bild an OK
    /// Gelände. Ohne Aufmaß bleiben andere Gebäude (Entscheidung B). Mit
    /// Aufmaß bestimmt ein Gebäude ±0,00 für alle; zieht man dessen
    /// Gelände, stehen die anderen bis zum Loslassen auf ihrer alten
    /// Gründung, dann folgen sie in einem Gang ([`Scene::commit`], Review
    /// #123 B1 zum Gelände-Zug). Je Bild kostet das so viel wie das gezogene
    /// Gebäude. Die Mengen rechnet das Loslassen nach (Review Gelände).
    pub fn drag_terrain(&mut self, z: f64) {
        let max = sk_model::MAX_TERRAIN_OFFSET;
        let Some(b) = self.active_building().filter(|_| z.is_finite()) else {
            return;
        };
        if self.gelaende_zug.is_none() {
            let Some(zug) = self.model.gelaende_zug(b) else {
                return;
            };
            self.gelaende_zug = Some((zug, self.platten_zuege(&[b])));
        }
        let Some((zug, runs)) = &self.gelaende_zug else {
            return;
        };
        if !self.model.gelaende_ziehen(zug, (-z).clamp(-max, max)) {
            return;
        }
        for r in runs.clone() {
            self.mark(r);
        }
        self.rebuild_dirty(true);
    }

    /// Loslassen eines Zugs an OK Gelände (aus [`Scene::commit`]): Die
    /// übrigen Gründungen folgen im Modell, die Züge mit Sohlplatte der
    /// Gebäude, deren Unterkante sich bewegt hat, werden neu.
    pub(super) fn gelaende_loslassen(&mut self) {
        let Some((zug, _)) = self.gelaende_zug.take() else {
            return;
        };
        let bewegt = self.model.gelaende_loslassen(zug);
        if !bewegt.is_empty() {
            for r in self.platten_zuege(&bewegt) {
                self.mark(r);
            }
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
    }
}

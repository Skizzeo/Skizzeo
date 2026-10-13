//! Lage und Nordpfeil in der Szene, aus `scene.rs` ausgelagert. Mit
//! Höhenaufmaß (Review #123 W45 B3): Liegt ein Aufmaß im Projekt, bewegen
//! Lage und Nordrichtung das Gelände unter dem Haus und mit ihm die
//! Gründungen. Ändert sich dabei eine Unterkante, rechnet die Szene alles
//! neu; sonst blieben 3D-Körper und Mengen beim alten Stand.

use super::Scene;
use sk_model::LevelKind;

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

    /// Unterkanten aller Gründungen.
    fn gruendungen_uk(&self) -> Vec<f64> {
        self.model
            .storeys()
            .iter()
            .filter(|(_, s)| s.kind == LevelKind::Foundation)
            .map(|(_, s)| s.elevation)
            .collect()
    }

    /// Alles neu, wenn eine Unterkante nicht mehr wie `vorher` liegt.
    fn gruendungen_folgen(&mut self, vorher: &[f64]) {
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
        let mut m = Model::new();
        let b = m.add_building(1);
        let ecken = [
            (0.0, 0.0),
            (0.0, 8000.0),
            (10_000.0, 8000.0),
            (10_000.0, 0.0),
        ];
        let ecken: Vec<_> = ecken.iter().map(|&(x, y)| vec3(x, y, 0.0)).collect();
        m.build_from_polygon(b, &ecken).unwrap();
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
}

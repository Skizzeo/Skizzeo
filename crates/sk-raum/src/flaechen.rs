//! Grundfläche eines Raums (Grundriss G4, Vorstufe der Wohnfläche G8): der
//! Rohbauumriss je Kante um den Ausbau seiner Wandseite nach innen versetzt
//! (schnittstelle-raum.md §3). Ausbau ist die Bekleidung des Wandtyps; ohne
//! sie, an Stirnseiten und bis das Raumbuch den Ausbau modelliert (RB3),
//! gilt der Innenputz der Vorgabe (Jörn 11.10. 16:55).

use crate::erkennung::{Kante, RaumForm};
use sk_math::Vec3;

/// Innenputz der Vorgabe je Wandseite (mm).
pub const PUTZ: f64 = 15.0;

/// Ausbaudicke an einer Kante (mm).
pub fn ausbau(k: &Kante) -> f64 {
    if k.ausbau_typ.is_empty() {
        PUTZ
    } else {
        k.ausbau_typ.iter().map(|a| a.0).sum()
    }
}

/// Fertigumriss: Rand und Löcher, je Kante um ihren Ausbau versetzt;
/// `None`, wenn der Raum dabei zerfällt (schmaler als der Putz).
pub fn fertigumriss(f: &RaumForm) -> Option<(Vec<Vec3>, Vec<Vec<Vec3>>)> {
    let ring = |k: &[Kante]| {
        let p: Vec<Vec3> = k.iter().map(|k| k.a).collect();
        let d: Vec<f64> = k.iter().map(ausbau).collect();
        sk_math::flaeche::versatz(&p, &d)
    };
    let rand = ring(&f.umriss)?;
    let loecher = f
        .loecher
        .iter()
        .map(|l| ring(l))
        .collect::<Option<Vec<_>>>()?;
    Some((rand, loecher))
}

/// Grundfläche (mm²): Fertigumriss ohne Löcher; zerfällt er, die
/// Rohbaufläche abzüglich Umfang mal Putz, nicht unter null.
pub fn grundflaeche(f: &RaumForm) -> f64 {
    let flaeche = sk_math::polygon::area;
    match fertigumriss(f) {
        Some((rand, loecher)) => flaeche(&rand) - loecher.iter().map(|l| flaeche(l)).sum::<f64>(),
        None => (f.flaeche - f.umfang * PUTZ).max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raeume;
    use sk_math::vec3;
    use sk_model::{Category, LayerFunction, MaterialLayer, Model, RefSide};

    fn nah(a: f64, b: f64) -> bool {
        (a - b).abs() < 1.0
    }

    /// Haus 10 × 8 m, Bezug außen; Innenwand bei x = 3 m, wenn `teilen`.
    fn haus(teilen: bool) -> (Model, sk_model::StoreyId, f64, f64) {
        let mut m = Model::new();
        let st = m.defaults().storey;
        let aw = m.defaults().exterior_wall;
        let iw = m.defaults().interior_wall;
        let dicke = |m: &Model, s| m.wall_layers(s).iter().map(|l| l.thickness).sum::<f64>();
        let (t, ti) = (dicke(&m, aw), dicke(&m, iw));
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
            .unwrap();
        if teilen {
            let p = [vec3(3000.0, t, 0.0), vec3(3000.0, 8000.0 - t, 0.0)];
            m.add_wall_run(&p, false, RefSide::Center, st, iw, Category::InteriorWall)
                .unwrap();
        }
        (m, st, t, ti)
    }

    #[test]
    fn putz_der_vorgabe_an_jeder_wandseite() {
        let (m, st, t, ti) = haus(true);
        let r = raeume(&m, st);
        let links = r
            .iter()
            .find(|f| f.enthaelt(vec3(1000.0, 4000.0, 0.0)))
            .unwrap();
        let (b, h) = (3000.0 - ti / 2.0 - t, 8000.0 - 2.0 * t);
        assert!(nah(links.flaeche, b * h));
        let g = grundflaeche(links);
        assert!(nah(g, (b - 2.0 * PUTZ) * (h - 2.0 * PUTZ)), "{g}");
    }

    #[test]
    fn bekleidung_des_typs_statt_vorgabe() {
        let (mut m, st, t, _) = haus(false);
        let aw = m.defaults().exterior_wall;
        let mut set = m.layer_set(aw).unwrap().clone();
        let mat = set.layers[0].material;
        set.layers
            .push(MaterialLayer::new(mat, 10.0, LayerFunction::Finish));
        assert!(m.set_layer_set(aw, set));
        let r = raeume(&m, st);
        let (b, h) = (10000.0 - 2.0 * t, 8000.0 - 2.0 * t);
        assert!(nah(r[0].flaeche, b * h));
        let g = grundflaeche(&r[0]);
        assert!(nah(g, (b - 20.0) * (h - 20.0)), "10 mm statt 15 mm: {g}");
    }

    #[test]
    fn pfeiler_im_raum_wird_mit_verputzt() {
        let (mut m, st, t, ti) = haus(false);
        // Freistehende Wand 1 m lang mitten im Raum
        let iw = m.defaults().interior_wall;
        let p = [vec3(4000.0, 4000.0, 0.0), vec3(5000.0, 4000.0, 0.0)];
        m.add_wall_run(&p, false, RefSide::Center, st, iw, Category::InteriorWall)
            .unwrap();
        let r = raeume(&m, st);
        assert_eq!((r.len(), r[0].loecher.len()), (1, 1));
        let (b, h) = (10000.0 - 2.0 * t, 8000.0 - 2.0 * t);
        let loch = (1000.0 + 2.0 * PUTZ) * (ti + 2.0 * PUTZ);
        let g = grundflaeche(&r[0]);
        assert!(nah(g, (b - 2.0 * PUTZ) * (h - 2.0 * PUTZ) - loch), "{g}");
    }
}

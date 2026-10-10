//! Automatikmengen der Bauvorbereitung (`site.*`): Baustelleneinrichtung,
//! Bauzaun, Schnurgerüst und Fassadengerüst, je Gebäude aus seiner Gründung
//! und seinen Außenwänden abgeleitet. Herleitung und Quellen:
//! kosten/bauvorbereitung-recherche.md.
//!
//! Die Werte sind roh (mm, mm², ganze Stück bzw. Monate); was eine
//! Bauleistung daraus macht (Einheit, Preis), steht im Katalog.

use crate::element::Category;
use crate::foundation::Foundation;
use crate::model::Model;
use crate::qto::AutoMenge;
use crate::RunId;
use sk_math::polygon;
use sk_math::Vec3;

/// Abstand des Bauzauns von der Außenkante des Gebäudes (mm): Arbeitsraum,
/// Gerüst und Lagerfläche um das Haus (keine Normvorgabe, Schätzung).
pub const ZAUN_ABSTAND: f64 = 3_000.0;
/// Außenseite des Fassadengerüsts vor der fertigen Fassade (mm): Wandabstand
/// 0,30 m (DGUV 201-011) und Gerüstbreite W09 1,00 m. DIN 18451 rechnet die
/// Länge an der Außenseite der Gerüstkonstruktion ab.
pub const GERUEST_ABSTAND: f64 = 1_300.0;
/// Gerüsthöhe über Wandkrone bzw. Attika (mm): Seitenschutz 1,00 m.
pub const GERUEST_UEBERSTAND: f64 = 1_000.0;

pub const LUMP: &str = "site.lump";
pub const MONTHS: &str = "site.months";
pub const FENCE: &str = "site.fence";
pub const SCAFFOLD: &str = "site.scaffold";

/// Schlüssel der Bauvorbereitung (`[service] auto=`), Einheit und Bedeutung.
pub const SCHLUESSEL: [(&str, &str, &str); 4] = [
    (LUMP, "psch", "1 je Gebäude (Pauschale)"),
    (
        MONTHS,
        "mon",
        "Vorhaltedauer Rohbau: 2 + Geschosse/2 Monate, aufgerundet",
    ),
    (
        FENCE,
        "m",
        "Bauzaun: konvexe Hülle des Gebäudes, 3,0 m Abstand",
    ),
    (
        SCAFFOLD,
        "m2",
        "Fassadengerüst: Länge an der Gerüstaußenseite × Höhe ab Gelände",
    ),
];

/// Kostengruppe (DIN 276): 391 Baustelleneinrichtung, 392 Gerüste.
pub const KG_BE: u16 = 391;
pub const KG_GERUEST: u16 = 392;

/// Umfang eines um `a` nach außen versetzten einfachen Polygons mit
/// Gehrungsecken: Umfang + 2a · Σ tan(Außenwinkel/2). Bei rechtwinkligen
/// Grundrissen ist das Umfang + 8a.
pub fn offset_perimeter(pts: &[Vec3], a: f64) -> f64 {
    let p = polygon::to_ccw(&polygon::simplified(pts));
    let n = p.len();
    if n < 3 {
        return 0.0;
    }
    let mut zuschlag = 0.0;
    for i in 0..n {
        let d0 = p[i] - p[(i + n - 1) % n];
        let d1 = p[(i + 1) % n] - p[i];
        let (l0, l1) = (d0.length(), d1.length());
        if l0 < 1e-9 || l1 < 1e-9 {
            continue;
        }
        // Außenwinkel, positiv an konvexen Ecken (gegen den Uhrzeigersinn)
        let w = (d0.x * d1.y - d0.y * d1.x).atan2(d0.x * d1.x + d0.y * d1.y);
        zuschlag += (w / 2.0).tan();
    }
    (polygon::perimeter(&p) + 2.0 * a * zuschlag).max(0.0)
}

/// Konvexe Hülle in der Ebene (gegen den Uhrzeigersinn, Andrew).
pub fn convex_hull(pts: &[Vec3]) -> Vec<Vec3> {
    let mut v: Vec<Vec3> = pts.iter().map(|p| sk_math::vec3(p.x, p.y, 0.0)).collect();
    v.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    v.dedup_by(|a, b| (*a - *b).length() < 1e-6);
    if v.len() < 3 {
        return v;
    }
    let cross = |o: Vec3, a: Vec3, b: Vec3| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut h: Vec<Vec3> = Vec::new();
    for pass in 0..2 {
        let start = h.len();
        let it: Box<dyn Iterator<Item = &Vec3>> = if pass == 0 {
            Box::new(v.iter())
        } else {
            Box::new(v.iter().rev())
        };
        for p in it {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], *p) <= 0.0 {
                h.pop();
            }
            h.push(*p);
        }
        h.pop();
    }
    h
}

/// Vorhaltedauer der Baustelleneinrichtung im Rohbau in Monaten:
/// 2 + 0,5 je Geschoss, aufgerundet (Rohbau massiv 1–4 Monate,
/// bauvorbereitung-recherche.md §3).
pub fn vorhaltemonate(geschosse: usize) -> u32 {
    if geschosse == 0 {
        return 0;
    }
    2 + geschosse.div_ceil(2) as u32
}

/// Automatikmengen `site.*` des Gebäudes über der Gründung `f` des
/// Wandzugs `run` (der Zug, unter dem die Sohlplatte liegt), mit Sohlplatte,
/// Geschoss und Gebäude aus `vorlage`. `gelaende`: OK Gelände relativ zu
/// ±0,00 (mm, nach oben positiv), Aufstandsfläche des Gerüsts.
pub fn site_mengen(
    model: &Model,
    run: RunId,
    f: &Foundation,
    vorlage: &AutoMenge,
    gelaende: f64,
) -> Vec<AutoMenge> {
    let gebaeude = model.run(run).and_then(|r| model.building_of(r.storey));
    // Außenwandzüge desselben Gebäudes (alle Geschosse)
    let zuege: Vec<RunId> = model
        .runs()
        .iter()
        .filter(|(_, r)| {
            r.closed
                && model.building_of(r.storey) == gebaeude
                && r.segments
                    .first()
                    .and_then(|e| model.element(*e))
                    .is_some_and(|e| e.category == Category::ExteriorWall)
        })
        .map(|(id, _)| id)
        .collect();
    let mut punkte: Vec<Vec3> = f.outline.clone();
    let mut krone = 0.0f64;
    let mut geschosse = Vec::new();
    let mut fassade: Option<Vec<Vec3>> = None;
    for id in &zuege {
        let Some(c) = model.chain(*id) else { continue };
        let aussen = c.face_corners(c.outer_offset());
        punkte.extend(aussen.iter().copied());
        krone = krone.max(c.top());
        if let Some(a) = &c.joints.attika {
            krone = krone.max(a.band.1);
        }
        if let Some(r) = model.run(*id) {
            if !geschosse.contains(&r.storey) {
                geschosse.push(r.storey);
            }
        }
        if *id == run {
            fassade = Some(aussen);
        }
    }
    let fassade = fassade.unwrap_or_else(|| f.outline.clone());
    let monate = vorhaltemonate(geschosse.len());
    let huelle = convex_hull(&punkte);
    let zaun = offset_perimeter(&huelle, ZAUN_ABSTAND);
    let laenge = offset_perimeter(&fassade, GERUEST_ABSTAND);
    // Aufstandsfläche: Gelände bis über die oberste Wandkrone bzw. Attika
    let hoehe = (krone + GERUEST_UEBERSTAND - gelaende).max(0.0);
    let m = |mm: f64| format!("{:.2} m", mm / 1000.0).replace('.', ",");
    let menge = |key, unit, value, kg, formula| AutoMenge {
        key,
        unit,
        value,
        kg: Some(kg),
        formula,
        ..vorlage.clone()
    };
    vec![
        menge(LUMP, "psch", 1.0, KG_BE, "1 je Gebäude".into()),
        menge(
            MONTHS,
            "mon",
            monate as f64,
            KG_BE,
            format!(
                "2 + {} Geschosse ÷ 2, aufgerundet = {monate} Monate",
                geschosse.len()
            ),
        ),
        menge(
            FENCE,
            "m",
            zaun,
            KG_BE,
            format!(
                "{} Hülle des Gebäudes, mit {} Abstand",
                m(polygon::perimeter(&huelle)),
                m(ZAUN_ABSTAND)
            ),
        ),
        menge(
            SCAFFOLD,
            "m2",
            laenge * hoehe,
            KG_GERUEST,
            format!("{} Länge × {} Höhe", m(laenge), m(hoehe)),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    fn rechteck(a: f64, b: f64) -> Vec<Vec3> {
        vec![
            vec3(0.0, 0.0, 0.0),
            vec3(a, 0.0, 0.0),
            vec3(a, b, 0.0),
            vec3(0.0, b, 0.0),
        ]
    }

    #[test]
    fn versatz_rechtwinklig_ist_umfang_plus_8a() {
        let r = rechteck(10_000.0, 8_000.0);
        assert!((offset_perimeter(&r, 3_000.0) - (36_000.0 + 24_000.0)).abs() < 1e-6);
        // Uhrzeigersinn gibt dasselbe
        let rev: Vec<Vec3> = r.iter().rev().copied().collect();
        assert!((offset_perimeter(&rev, 3_000.0) - 60_000.0).abs() < 1e-6);
        // L-Form: fünf konvexe, eine einspringende Ecke, also auch + 8a
        let l = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(10_000.0, 0.0, 0.0),
            vec3(10_000.0, 4_000.0, 0.0),
            vec3(4_000.0, 4_000.0, 0.0),
            vec3(4_000.0, 8_000.0, 0.0),
            vec3(0.0, 8_000.0, 0.0),
        ];
        assert!((offset_perimeter(&l, 1_000.0) - (36_000.0 + 8_000.0)).abs() < 1e-6);
        // Hülle der L-Form schneidet die Ecke ab
        let h = convex_hull(&l);
        assert_eq!(h.len(), 5);
        let schraege = (6_000.0f64.powi(2) + 4_000.0f64.powi(2)).sqrt();
        assert!(
            (polygon::perimeter(&h) - (10_000.0 + 4_000.0 + schraege + 4_000.0 + 8_000.0)).abs()
                < 1e-6
        );
    }

    #[test]
    fn vorhaltemonate_je_geschoss() {
        assert_eq!(vorhaltemonate(0), 0);
        assert_eq!(vorhaltemonate(1), 3);
        assert_eq!(vorhaltemonate(2), 3);
        assert_eq!(vorhaltemonate(3), 4);
    }

    #[test]
    fn haus_zwei_geschosse() {
        let mut m = Model::with_seed(12);
        let b = m.add_building(2);
        let eg = m
            .build_from_polygon(b, &rechteck(10_000.0, 8_000.0))
            .unwrap();
        let s = crate::qto::schedule(&m);
        let v: Vec<&AutoMenge> = s
            .auto
            .iter()
            .filter(|a| a.key.starts_with("site."))
            .collect();
        assert_eq!(v.len(), 4);
        assert!(v
            .iter()
            .all(|a| a.building == Some(b) && a.number.starts_with("SP-")));
        let w = |k: &str| v.iter().find(|x| x.key == k).unwrap().value;
        assert_eq!(w("site.lump"), 1.0);
        assert_eq!(w("site.months"), 3.0);
        // Außenmaß mit Dämmung größer als die Bezugslinie
        let c = m.chain(eg).unwrap();
        let aussen = c.face_corners(c.outer_offset());
        let u = polygon::perimeter(&aussen);
        assert!(u > 36_000.0);
        assert!((w("site.fence") - (u + 8.0 * ZAUN_ABSTAND)).abs() < 1.0);
        let oben = m.stack_above(eg);
        let krone = oben
            .iter()
            .chain([&eg])
            .filter_map(|r| m.chain(*r))
            .map(|c| c.top())
            .fold(0.0, f64::max);
        assert!(krone > 5_000.0);
        let soll = (u + 8.0 * GERUEST_ABSTAND) * (krone + GERUEST_UEBERSTAND);
        assert!(
            (w("site.scaffold") - soll).abs() / soll < 0.02,
            "{} {soll}",
            w("site.scaffold")
        );
    }
}

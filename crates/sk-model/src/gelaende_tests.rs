use super::*;
use crate::txn::Direction;
use sk_math::vec3;

fn haus() -> (Model, RunId, StoreyId, ElementId) {
    let mut m = Model::with_seed(12);
    let b = m.add_building(1);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.build_from_polygon(b, &pts).unwrap();
    let gr = m
        .storeys()
        .iter()
        .find(|(_, s)| s.kind == LevelKind::Foundation)
        .map(|(id, _)| id)
        .unwrap();
    let slab = m.foundation_of(eg).unwrap().0;
    (m, eg, gr, slab)
}

fn uk(m: &Model, gr: StoreyId) -> f64 {
    m.storey(gr).unwrap().elevation
}

fn schritt(m: &mut Model, f: impl FnOnce(&mut Model) -> bool) -> Txn {
    m.begin("Gelände");
    assert!(f(m));
    m.commit().expect("Schritt")
}

#[test]
fn neues_haus_steht_bei_null_und_frostfrei() {
    let (m, eg, gr, slab) = haus();
    assert_eq!(m.terrain_offset(), 0.0);
    assert_eq!(m.terrain_z(), 0.0);
    assert_eq!(m.slab_insulation(slab), 0.0);
    assert!(m.perimeter_of(slab).is_none());
    assert!(m.embedment_of(gr).unwrap() >= FROST_DEPTH - 1e-6);
    assert!(m.frost_safe(gr));
    let g = m.ground_basis(eg).unwrap();
    assert_eq!((g.terrain_z, g.slab_top_z, g.insulation), (0.0, 0.0, 0.0));
    assert_eq!(g.insulation_bottom_z, g.slab_bottom_z);
    assert!((g.slab_area - 80.0e6).abs() < 1.0, "{}", g.slab_area);
    assert_eq!(m.ground_bases().len(), 1);
}

/// Erdmengen aus der Gründung des Modells: höher gesetzt kommt
/// Auffüllung statt Aushub, tiefer gesetzt eine Baugrube mit
/// Arbeitsraum; die Perimeterdämmung ändert die Erdmengen nicht.
#[test]
fn erdmengen_folgen_dem_gelaende() {
    use crate::qto_earth::{BACKFILL, EXCAVATION, FILL, GRAVEL};
    let (mut m, _, _, slab) = haus();
    let summe = |m: &Model, k: &str| -> f64 {
        crate::qto::schedule(m)
            .auto
            .iter()
            .filter(|a| a.key == k)
            .map(|a| a.value)
            .sum()
    };
    let kies = summe(&m, GRAVEL);
    schritt(&mut m, |m| m.set_terrain_offset(400.0));
    assert!(summe(&m, FILL) > 0.0);
    assert_eq!(summe(&m, EXCAVATION), 0.0);
    schritt(&mut m, |m| m.set_terrain_offset(-600.0));
    assert_eq!(summe(&m, FILL), 0.0);
    let grube = summe(&m, EXCAVATION);
    assert!(summe(&m, BACKFILL) > 0.0);
    // Die Dämmung hebt das Haus um ihre Dicke: UK Dämmung und damit
    // der Aushub bleiben, wo sie waren
    schritt(&mut m, |m| m.set_slab_insulation(slab, 120.0));
    assert_eq!(m.terrain_offset(), -480.0);
    assert!((summe(&m, EXCAVATION) - grube).abs() < 1.0);
    assert!((summe(&m, GRAVEL) - kies).abs() < 1.0);
}

/// Zwei Gebäude auf verschiedener Geländehöhe: Jedes rechnet seine
/// Erdmengen mit dem eigenen Versatz.
#[test]
fn erdmengen_je_gebaeude() {
    use crate::qto_earth::{EXCAVATION, FILL};
    let (mut m, _, gr, _) = haus();
    let b1 = m.storey(gr).unwrap().building.unwrap();
    m.begin("Gebäude");
    let b2 = m.add_building(1);
    let pts = [
        vec3(20000.0, 0.0, 0.0),
        vec3(20000.0, 8000.0, 0.0),
        vec3(30000.0, 8000.0, 0.0),
        vec3(30000.0, 0.0, 0.0),
    ];
    m.build_from_polygon(b2, &pts).unwrap();
    m.commit();
    schritt(&mut m, |m| m.set_terrain_offset_of(b2, 400.0));
    let s = crate::qto::schedule(&m);
    let summe = |b: BuildingId, k: &str| -> f64 {
        s.auto
            .iter()
            .filter(|a| a.building == Some(b) && a.key == k)
            .map(|a| a.value)
            .sum()
    };
    // b1 auf Gelände: Bett der Schicht unter dem gewachsenen Boden
    assert!(summe(b1, EXCAVATION) > 0.0);
    assert_eq!(summe(b1, FILL), 0.0);
    // b2 0,40 m höher: Auffüllung statt Aushub
    assert_eq!(summe(b2, EXCAVATION), 0.0);
    assert!(summe(b2, FILL) > 0.0);
}

/// Bodenkennwerte (Erdarbeiten): ändern die Automatikmengen, stehen in
/// der Datei nur abweichend von der Vorgabe, Rückgängig stellt her.
#[test]
fn bodenkennwerte_im_projekt() {
    use crate::qto_earth::{Boden, TOPSOIL};
    let (mut m, ..) = haus();
    let summe = |m: &Model, k: &str| -> f64 {
        crate::qto::schedule(m)
            .auto
            .iter()
            .filter(|a| a.key == k)
            .map(|a| a.value)
            .sum()
    };
    let vorher = summe(&m, TOPSOIL);
    assert!(vorher > 0.0);
    let leer = crate::szo::write(&m);
    assert!(
        !leer.contains("topsoil="),
        "Vorgabe steht nicht in der Datei"
    );
    let b = Boden {
        oberboden: 200.0,
        boeschung: 60.0,
        ..Boden::default()
    };
    let t = schritt(&mut m, |m| m.set_soil(b));
    assert!((summe(&m, TOPSOIL) - vorher * 2.0 / 3.0).abs() < 1.0);
    let text = crate::szo::write(&m);
    // Regel 113: eine eigene Zeile, `[project]` bleibt
    assert!(
        text.contains("\n[soil] key=project topsoil=200 slope=60\n"),
        "{text}"
    );
    let projekt = |t: &str| {
        t.lines()
            .find(|l| l.starts_with("[project] "))
            .unwrap()
            .to_string()
    };
    assert_eq!(projekt(&text), projekt(&leer));
    let back = crate::szo::read(&text, crate::GuidGen::with_seed(1)).unwrap();
    assert_eq!(back.model.project().soil, b);
    assert_eq!(crate::szo::write(&back.model), text);
    m.apply(&t, Direction::Undo);
    assert_eq!(m.project().soil, Boden::default());
    assert_eq!(crate::szo::write(&m), leer);
    // außerhalb des Bereichs und ohne Änderung abgelehnt
    m.begin("x");
    let steil = Boden {
        boeschung: 95.0,
        ..Boden::default()
    };
    assert!(!m.set_soil(steil));
    assert!(!m.set_soil(Boden::default()));
    m.rollback();
    // Außerhalb des Bereichs: Hinweis, Vorgabe gilt, das Projekt öffnet
    let falsch = leer.replacen("[project] ", "[project] slope=10 ", 1);
    let r = crate::szo::read(&falsch, crate::GuidGen::with_seed(1)).unwrap();
    assert_eq!(r.model.project().soil, Boden::default());
    assert!(
        r.hints
            .iter()
            .any(|h| h.ends_with("Böschungswinkel 10° außerhalb 30 bis 90°, Vorgabe gilt")),
        "{:?}",
        r.hints
    );
}

/// Regel 113: Bodenkennwerte an `[project]` (ältere Dateien) gelten als
/// Rückfall und bleiben bytegleich; die erste Änderung zieht sie in
/// einem Rückgängig-Schritt nach `[soil]` um. Steht beides in der
/// Datei, gilt `[soil]`.
#[test]
fn bodenkennwerte_ziehen_nach_soil_um() {
    use crate::qto_earth::Boden;
    let projekt_zeile = |t: &str| {
        t.lines()
            .find(|l| l.starts_with("[project] "))
            .unwrap()
            .to_string()
    };
    let (m, ..) = haus();
    let leer = crate::szo::write(&m);
    let zeile = leer
        .lines()
        .find(|l| l.starts_with("[project] "))
        .unwrap()
        .to_string();
    let alt = leer.replace(&zeile, &format!("{zeile} topsoil=200 slope=60"));
    let r = crate::szo::read(&alt, crate::GuidGen::with_seed(1)).unwrap();
    assert!(r.hints.is_empty(), "{:?}", r.hints);
    let mut m = r.model;
    let b = Boden {
        oberboden: 200.0,
        boeschung: 60.0,
        ..Boden::default()
    };
    assert_eq!(m.project().soil, b);
    assert_eq!(crate::szo::write(&m), alt, "bytegleich");
    // Andere Änderung am Projekt: bleibt an `[project]`
    let mut p = m.project().clone();
    p.site = "Haus".into();
    schritt(&mut m, |m| m.set_project(p));
    assert!(projekt_zeile(&crate::szo::write(&m)).ends_with(" topsoil=200 slope=60"));
    assert!(!crate::szo::write(&m).contains("[soil]"));
    // Erste Änderung der Bodenkennwerte: Umzug, ein Schritt
    let vorher = crate::szo::write(&m);
    let neu = Boden {
        arbeitsraum: 800.0,
        ..b
    };
    let t = schritt(&mut m, |m| m.set_soil(neu));
    let text = crate::szo::write(&m);
    assert!(!projekt_zeile(&text).contains(" topsoil="), "{text}");
    assert!(
        text.contains("\n[soil] key=project topsoil=200 workspace=800 slope=60\n"),
        "{text}"
    );
    let back = crate::szo::read(&text, crate::GuidGen::with_seed(1)).unwrap();
    assert_eq!(back.model.project().soil, neu);
    assert_eq!(crate::szo::write(&back.model), text);
    m.apply(&t, Direction::Undo);
    assert_eq!(crate::szo::write(&m), vorher);
    // Beides: `[soil]` gilt, `[project]` bleibt bis zur Änderung; eine
    // leere `[soil]` neben alten Werten heißt Vorgabe
    let mit_soil = |soil: &str| {
        let z = projekt_zeile(&alt);
        alt.replace(&z, &format!("{z}\n{soil}"))
    };
    let beides = mit_soil("[soil] key=project topsoil=100");
    let r = crate::szo::read(&beides, crate::GuidGen::with_seed(1)).unwrap();
    assert_eq!(r.model.project().soil.oberboden, 100.0);
    assert_eq!(r.model.project().soil.boeschung, 45.0);
    assert_eq!(crate::szo::write(&r.model), beides);
    let leer_soil = mit_soil("[soil] key=project");
    let r = crate::szo::read(&leer_soil, crate::GuidGen::with_seed(1)).unwrap();
    assert_eq!(r.model.project().soil, Boden::default());
    assert_eq!(crate::szo::write(&r.model), leer_soil);
    let doppelt = mit_soil("[soil] key=project\n[soil] key=project");
    assert!(crate::szo::read(&doppelt, crate::GuidGen::with_seed(1)).is_err());
}

/// Werte außerhalb des Bereichs sperren das Projekt nicht: slope=95 und
/// ein Versatz außerhalb ±3 m öffnen mit zwei Hinweisen, die Vorgaben
/// gelten.
#[test]
fn ausserhalb_des_bereichs_oeffnet_mit_hinweis() {
    let (m, ..) = haus();
    let leer = crate::szo::write(&m);
    let projekt = leer
        .lines()
        .find(|l| l.starts_with("[project] "))
        .unwrap()
        .to_string();
    let gebaeude = leer
        .lines()
        .find(|l| l.starts_with("[building] "))
        .unwrap()
        .to_string();
    let falsch = leer
        .replace(&projekt, &format!("{projekt} slope=95"))
        .replace(&gebaeude, &format!("{gebaeude} terrain=9000"));
    let r = crate::szo::read(&falsch, crate::GuidGen::with_seed(1)).unwrap();
    let ab: Vec<_> = r.hints.iter().filter(|h| h.contains("außerhalb")).collect();
    assert_eq!(ab.len(), 2, "{:?}", r.hints);
    assert!(ab
        .iter()
        .any(|h| h.ends_with("Böschungswinkel 95° außerhalb 30 bis 90°, Vorgabe gilt")));
    assert!(ab
        .iter()
        .any(|h| h
            .ends_with("OK Sohle über Gelände 9000 mm außerhalb -3000 bis 3000 mm, Vorgabe gilt")));
    assert_eq!(r.model.project().soil, crate::qto_earth::Boden::default());
    assert_eq!(r.model.terrain_offset(), 0.0);
    assert_eq!(crate::szo::write(&r.model), leer);
    // Dezimalstellen mit Komma
    let komma = leer.replace(&projekt, &format!("{projekt} topsoil=1000.5"));
    let r = crate::szo::read(&komma, crate::GuidGen::with_seed(1)).unwrap();
    assert!(
        r.hints
            .iter()
            .any(|h| h.ends_with("Oberboden 1000,5 mm außerhalb 0 bis 1000 mm, Vorgabe gilt")),
        "{:?}",
        r.hints
    );
}

/// Höher gesetzt: die Schürze wächst mit, die Einbindetiefe bleibt;
/// tiefer gesetzt: die Schürze wird kürzer, aber nie flacher als 0,80
/// unter Gelände. Rückgängig stellt alles her.
#[test]
fn versatz_haelt_die_frosttiefe() {
    let (mut m, eg, gr, _) = haus();
    let e0 = m.embedment_of(gr).unwrap();
    let u0 = uk(&m, gr);
    let t = schritt(&mut m, |m| m.set_terrain_offset(400.0));
    assert_eq!(m.terrain_z(), -400.0);
    assert!((uk(&m, gr) - (u0 - 400.0)).abs() < 1e-6);
    assert!((m.embedment_of(gr).unwrap() - e0).abs() < 1e-6);
    assert!(m.check().is_empty(), "{:?}", m.check());
    m.apply(&t, Direction::Undo);
    assert_eq!(m.terrain_offset(), 0.0);
    assert_eq!(uk(&m, gr), u0);
    // Tiefer: UK Gründung wandert mit hoch, geklemmt an 0,80 unter
    // Gelände
    schritt(&mut m, |m| m.set_terrain_offset(-300.0));
    assert_eq!(m.terrain_z(), 300.0);
    assert!(m.embedment_of(gr).unwrap() >= FROST_DEPTH - 1e-6);
    assert!(m.frost_safe(gr));
    let (_, hi) = m.foundation_bottom_range_of(gr);
    assert!(hi <= m.terrain_z() - FROST_DEPTH + 1e-6);
    let g = m.ground_basis(eg).unwrap();
    assert!((g.embedment - (g.terrain_z - g.footing_bottom_z)).abs() < 1e-6);
    // außerhalb ±3,00 m und ohne Änderung abgelehnt
    m.begin("x");
    assert!(!m.set_terrain_offset(MAX_TERRAIN_OFFSET + 1.0));
    assert!(!m.set_terrain_offset(-300.0));
    assert!(m.commit().is_none());
}

/// UK Gründung lässt sich nicht über 0,80 unter Gelände ziehen.
#[test]
fn gruendung_nie_flacher_als_frosttiefe() {
    let (mut m, _, gr, _) = haus();
    // 30 cm unter Gelände: die Frosttiefe ist die engere Grenze
    schritt(&mut m, |m| m.set_terrain_offset(-300.0));
    let (lo, hi) = m.foundation_bottom_range_of(gr);
    assert!(lo < hi);
    assert!((hi - (300.0 - FROST_DEPTH)).abs() < 1e-6, "{hi}");
    assert!((m.embedment_of(gr).unwrap() - FROST_DEPTH).abs() < 1e-6);
    // 1 m unter Gelände: die Platte samt 10 cm Schürze ist tiefer
    schritt(&mut m, |m| m.set_terrain_offset(-1000.0));
    let (_, hi) = m.foundation_bottom_range_of(gr);
    assert!(hi < 1000.0 - FROST_DEPTH, "{hi}");
    assert!(m.embedment_of(gr).unwrap() > FROST_DEPTH);
}

/// Dämmung 120 hebt das Haus um 12 cm über das Gelände; Schürzentiefe
/// und Einbindetiefe bleiben; Mengen: Fläche = Platte ohne den Ring der
/// Schürze (Jörn 11.10.), Volumen = Fläche × Dicke. 0 schaltet sie ab, die Dämmung verschwindet.
#[test]
fn perimeterdaemmung_hebt_das_haus() {
    let (mut m, eg, gr, slab) = haus();
    let g0 = m.ground_basis(eg).unwrap();
    let t = schritt(&mut m, |m| m.set_slab_insulation(slab, PERIMETER_THICKNESS));
    assert_eq!(m.slab_insulation(slab), 120.0);
    assert_eq!(m.terrain_offset(), 120.0);
    let p = m.perimeter_of(slab).expect("Bauteil Perimeterdämmung");
    assert_eq!(
        m.element(p).unwrap().category,
        Category::PerimeterInsulation
    );
    assert!(m.perimeter_material().is_some());
    let g = m.ground_basis(eg).unwrap();
    assert_eq!(g.insulation, 120.0);
    assert!((g.insulation_bottom_z - (g.slab_bottom_z - 120.0)).abs() < 1e-6);
    assert!((g.embedment - g0.embedment).abs() < 1e-6);
    let depth = |g: &GroundBasis| g.insulation_bottom_z - g.footing_bottom_z;
    assert!((depth(&g) - depth(&g0)).abs() < 1e-6);
    assert!((uk(&m, gr) - (g0.footing_bottom_z - 120.0)).abs() < 1e-6);
    let f = m.foundation(eg).unwrap().unwrap();
    let q = crate::qto::perimeter_qto_of(&f).unwrap();
    assert!((q.area - (g.slab_area - g.footing_area)).abs() < 1.0);
    assert!((q.volume - q.area * 120.0).abs() < 1.0);
    assert!(m.check().is_empty(), "{:?}", m.check());
    // ungültige Dicken
    m.begin("x");
    assert!(!m.set_slab_insulation(slab, 10.0));
    assert!(!m.set_slab_insulation(slab, MAX_PERIMETER + 1.0));
    assert!(m.commit().is_none());
    // Rückgängig: wie vorher
    m.apply(&t, Direction::Undo);
    assert_eq!(m.terrain_offset(), 0.0);
    assert!(m.perimeter_of(slab).is_none());
    assert_eq!(m.ground_basis(eg).unwrap(), g0);
    // Ab- und wieder anschalten
    schritt(&mut m, |m| m.set_slab_insulation(slab, 120.0));
    schritt(&mut m, |m| m.set_slab_insulation(slab, 0.0));
    assert!(m.perimeter_of(slab).is_none());
    assert_eq!(m.terrain_offset(), 0.0);
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// Entscheidung B: Versatz und Dämmung gelten je Gebäude. Das zweite
/// Gebäude behält Gelände und Gründung, wenn das erste gedämmt oder
/// verschoben wird; die Abfrage nach Lage liefert die Ebene des
/// gefragten Gebäudes.
#[test]
fn versatz_und_daemmung_je_gebaeude() {
    let (mut m, eg1, gr1, slab1) = haus();
    let b1 = m.building_of(gr1).unwrap();
    m.begin("Haus 2");
    let b2 = m.add_building(1);
    let pts = [
        vec3(20000.0, 0.0, 0.0),
        vec3(20000.0, 8000.0, 0.0),
        vec3(30000.0, 8000.0, 0.0),
        vec3(30000.0, 0.0, 0.0),
    ];
    let eg2 = m.build_from_polygon(b2, &pts).unwrap();
    m.commit();
    let gr2 = m
        .storeys()
        .iter()
        .find(|(_, s)| s.kind == LevelKind::Foundation && s.building == Some(b2))
        .map(|(id, _)| id)
        .unwrap();
    let g2 = m.ground_basis(eg2).unwrap();
    schritt(&mut m, |m| m.set_slab_insulation(slab1, 120.0));
    schritt(&mut m, |m| m.set_terrain_offset_of(b1, 500.0));
    assert_eq!(m.terrain_offset_of(Some(b1)), 500.0);
    assert_eq!(m.terrain_offset_of(Some(b2)), 0.0);
    assert_eq!(m.terrain_z_at(Some(b1), 5000.0, 4000.0), -500.0);
    assert_eq!(m.terrain_z_at(Some(b2), 25000.0, 4000.0), 0.0);
    assert_eq!(m.ground_basis(eg2).unwrap(), g2);
    assert_eq!(m.ground_basis(eg1).unwrap().terrain_z, -500.0);
    assert!(m.frost_safe(gr1) && m.frost_safe(gr2));
    schritt(&mut m, |m| m.set_terrain_offset_of(b2, -200.0));
    assert_eq!(m.terrain_offset_of(Some(b1)), 500.0);
    assert_eq!(m.embedment_of(gr2), Some(g2.embedment));
    let text = crate::szo::write(&m);
    let back = crate::szo::read(&text, GuidGen::with_seed(5))
        .unwrap()
        .model;
    assert_eq!(crate::szo::write(&back), text);
    let versatz = |guid: Guid| {
        back.buildings()
            .iter()
            .find(|(_, b)| b.guid == guid)
            .map(|(_, b)| b.terrain)
    };
    assert_eq!(versatz(m.building(b1).unwrap().guid), Some(500.0));
    assert_eq!(versatz(m.building(b2).unwrap().guid), Some(-200.0));
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// Alte Datei: ohne Versatz und Dämmung bleibt der Text gleich; mit
/// beiden kommt alles zurück.
#[test]
fn datei_rundreise() {
    let (mut m, eg, _, slab) = haus();
    let plain = crate::szo::write(&m);
    assert!(!plain.contains("terrain=") && !plain.contains("insulation="));
    assert!(!plain.contains("[perimeter]"));
    let back = crate::szo::read(&plain, GuidGen::with_seed(5))
        .unwrap()
        .model;
    assert_eq!(crate::szo::write(&back), plain);
    schritt(&mut m, |m| m.set_slab_insulation(slab, 100.0));
    schritt(&mut m, |m| m.set_terrain_offset(-250.0));
    let text = crate::szo::write(&m);
    let back = crate::szo::read(&text, GuidGen::with_seed(5))
        .unwrap()
        .model;
    assert_eq!(crate::szo::write(&back), text);
    assert_eq!(back.terrain_offset(), -250.0);
    assert_eq!(back.slab_insulation(slab), 100.0);
    assert_eq!(back.ground_basis(eg), m.ground_basis(eg));
    assert!(!text.contains(" embed="));
    // Tief ins Gelände: die Schürze stößt an 10 cm, die gewollte
    // Einbindetiefe steht in der Datei und kommt zurück
    schritt(&mut m, |m| m.set_terrain_offset(-1000.0));
    let deep = crate::szo::write(&m);
    assert!(deep.contains(" embed=800"), "{deep}");
    let back = crate::szo::read(&deep, GuidGen::with_seed(5))
        .unwrap()
        .model;
    assert_eq!(crate::szo::write(&back), deep);
    schritt(&mut m, |m| m.set_terrain_offset(-250.0));
    assert_eq!(crate::szo::write(&m), text);
    // Dämmdicke außerhalb der Grenzen: mit Hinweis verworfen, ohne
    // Dämmbauteil (Review Gelände, Hinweis 2)
    let dick = text.replace(" insulation=100", " insulation=900");
    let r = crate::szo::read(&dick, GuidGen::with_seed(5)).unwrap();
    assert!(
        r.hints.iter().any(|h| h.contains("Perimeterdämmung")),
        "{:?}",
        r.hints
    );
    assert_eq!(r.model.slab_insulation(slab), 0.0);
    assert!(r.model.perimeter_of(slab).is_none());
    assert!(r.model.check().is_empty(), "{:?}", r.model.check());
    // außerhalb ±3 m: Hinweis, Versatz 0, das Projekt öffnet
    let bad = text.replace("terrain=-250", "terrain=9000");
    let r = crate::szo::read(&bad, GuidGen::with_seed(5)).unwrap();
    assert_eq!(r.model.terrain_offset(), 0.0);
    assert!(r
        .hints
        .iter()
        .any(|h| h.contains("OK Sohle über Gelände 9000")));
}

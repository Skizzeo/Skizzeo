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
    // Die Dämmung baut sich nach unten auf (Jörn 11.10.): Das Haus bleibt,
    // innerhalb der Schürze wird um ihre Dicke tiefer ausgehoben
    let f = m.foundation(m.run_of(slab).unwrap()).unwrap().unwrap();
    let innen = f.slab_area() - f.footing_area();
    schritt(&mut m, |m| m.set_slab_insulation(slab, 120.0));
    assert_eq!(m.terrain_offset(), -600.0);
    assert!((summe(&m, EXCAVATION) - (grube + innen * 120.0)).abs() < 1.0);
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

/// Kapillarbrechende Schicht als Schalter (Jörn 17.10., VOB B-BA-5/6):
/// aus ohne Kiesposition, in der Datei `capillary_on=0`, die Dicke bleibt;
/// Dateien ohne Schlüssel haben sie, neue Projekte nicht.
#[test]
fn kiesschicht_als_schalter() {
    use crate::qto_earth::{Boden, GRAVEL, SUBGRADE};
    let (mut m, ..) = haus();
    let mengen = |m: &Model, k: &str| -> Vec<crate::qto::AutoMenge> {
        crate::qto::schedule(m)
            .auto
            .into_iter()
            .filter(|a| a.key == k)
            .collect()
    };
    assert!(m.project().soil.kies);
    assert!(mengen(&m, GRAVEL).iter().map(|a| a.value).sum::<f64>() > 0.0);
    let leer = crate::szo::write(&m);
    let aus = Boden {
        kies: false,
        ..Boden::default()
    };
    let t = schritt(&mut m, |m| m.set_soil(aus));
    assert!(mengen(&m, GRAVEL).is_empty());
    assert!(mengen(&m, SUBGRADE).iter().any(|a| a
        .formula
        .contains("Kapillarbrechende Schicht: nicht eingebaut")));
    let text = crate::szo::write(&m);
    assert!(
        text.contains("\n[soil] key=project capillary_on=0\n"),
        "{text}"
    );
    let back = crate::szo::read(&text, crate::GuidGen::with_seed(1)).unwrap();
    assert_eq!(back.model.project().soil, aus);
    assert_eq!(back.model.project().soil.tragschicht, 150.0);
    assert_eq!(crate::szo::write(&back.model), text);
    m.apply(&t, Direction::Undo);
    assert!(m.project().soil.kies);
    assert_eq!(crate::szo::write(&m), leer);
    // neue Projekte ohne Schicht
    assert!(!Model::new().project().soil.kies);
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

/// Jörn 11.10.: Die Perimeterdämmung baut sich nach unten auf. Nachträglich
/// 0 → 10 cm: OK Sohlplatte, alle Geschosshöhen, Versatz zum Gelände, UK
/// Gründung, Einbindetiefe und Schürzentiefe bleiben; die Dämmung liegt
/// innerhalb der Schürze unter der Platte. Mengen: Fläche = Platte ohne
/// den Ring der Schürze, Volumen = Fläche × Dicke. 0 schaltet sie ab.
#[test]
fn perimeterdaemmung_aendert_die_hoehen_nicht() {
    let (mut m, eg, gr, slab) = haus();
    let g0 = m.ground_basis(eg).unwrap();
    let fuss = m.foundation_of(eg).unwrap().1.unwrap();
    let d0 = m.footing_depth(fuss).unwrap();
    let hoehen = |m: &Model| -> Vec<f64> { m.storeys().iter().map(|(_, s)| s.elevation).collect() };
    let h0 = hoehen(&m);
    let t = schritt(&mut m, |m| m.set_slab_insulation(slab, 100.0));
    assert_eq!(m.slab_insulation(slab), 100.0);
    assert_eq!(m.terrain_offset(), 0.0);
    assert_eq!(hoehen(&m), h0);
    assert_eq!(m.footing_depth(fuss), Some(d0));
    assert_eq!(uk(&m, gr), g0.footing_bottom_z);
    let p = m.perimeter_of(slab).expect("Bauteil Perimeterdämmung");
    assert_eq!(
        m.element(p).unwrap().category,
        Category::PerimeterInsulation
    );
    assert!(m.perimeter_material().is_some());
    let g = m.ground_basis(eg).unwrap();
    assert_eq!(g.insulation, 100.0);
    assert_eq!(
        (
            g.terrain_z,
            g.slab_top_z,
            g.slab_bottom_z,
            g.footing_bottom_z
        ),
        (
            g0.terrain_z,
            g0.slab_top_z,
            g0.slab_bottom_z,
            g0.footing_bottom_z
        )
    );
    assert!((g.insulation_bottom_z - (g.slab_bottom_z - 100.0)).abs() < 1e-6);
    assert!((g.embedment - g0.embedment).abs() < 1e-6);
    // Körper: Platte liegt auf der Schürze, Dämmung darunter innerhalb
    let f = m.foundation(eg).unwrap().unwrap();
    assert!((f.footing_height() - d0).abs() < 1e-6);
    let q = crate::qto::perimeter_qto_of(&f).unwrap();
    assert!((q.area - (g.slab_area - g.footing_area)).abs() < 1.0);
    assert!((q.volume - q.area * 100.0).abs() < 1.0);
    assert!(m.check().is_empty(), "{:?}", m.check());
    // 12 cm: wieder nur nach unten
    let t2 = schritt(&mut m, |m| m.set_slab_insulation(slab, PERIMETER_THICKNESS));
    assert_eq!(m.terrain_offset(), 0.0);
    assert_eq!(hoehen(&m), h0);
    assert_eq!(m.footing_depth(fuss), Some(d0));
    // ungültige Dicken; unter der Dämmung bleiben 10 cm Schürze
    m.begin("x");
    assert!(!m.set_slab_insulation(slab, 10.0));
    assert!(!m.set_slab_insulation(slab, MAX_PERIMETER + 1.0));
    assert!(m.commit().is_none());
    // Rückgängig: wie vorher
    m.apply(&t2, Direction::Undo);
    assert_eq!(m.slab_insulation(slab), 100.0);
    m.apply(&t, Direction::Undo);
    assert_eq!(m.terrain_offset(), 0.0);
    assert!(m.perimeter_of(slab).is_none());
    assert_eq!(m.ground_basis(eg).unwrap(), g0);
    assert_eq!(hoehen(&m), h0);
    // Ab- und wieder anschalten
    schritt(&mut m, |m| m.set_slab_insulation(slab, 100.0));
    schritt(&mut m, |m| m.set_slab_insulation(slab, 0.0));
    assert!(m.perimeter_of(slab).is_none());
    assert_eq!(m.ground_basis(eg).unwrap(), g0);
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// Die Schürze bleibt unter der Dämmung mindestens 10 cm tief: Liegt das
/// Haus 60 cm tief im Gelände, ist die Schürze nur 10 cm hoch; 12 cm
/// Dämmung lassen sie um 12 cm nach unten wachsen. OK Sohlplatte und
/// Versatz bleiben.
#[test]
fn flache_schuerze_waechst_unter_der_daemmung() {
    let (mut m, eg, gr, slab) = haus();
    schritt(&mut m, |m| m.set_terrain_offset(-600.0));
    let fuss = m.foundation_of(eg).unwrap().1.unwrap();
    assert_eq!(m.footing_depth(fuss), Some(MIN_FOOTING));
    let z0 = uk(&m, gr);
    let t = schritt(&mut m, |m| m.set_slab_insulation(slab, 120.0));
    assert_eq!(m.terrain_offset(), -600.0);
    assert_eq!(m.footing_depth(fuss), Some(MIN_FOOTING + 120.0));
    assert!((uk(&m, gr) - (z0 - 120.0)).abs() < 1e-6);
    assert!(m.check().is_empty(), "{:?}", m.check());
    m.apply(&t, Direction::Undo);
    assert_eq!(uk(&m, gr), z0);
}

/// Geometrie-Hinweis 1 zu #50: Weniger Dämmung nimmt das Wachstum der
/// Schürze zurück, die Dämmung ändert die Schürzentiefe also auch beim
/// Zurückstellen nicht. Ein normal liegendes Haus bleibt dabei ganz stehen.
#[test]
fn schuerze_schrumpft_mit_der_daemmung_zurueck() {
    let (mut m, eg, gr, slab) = haus();
    let fuss = m.foundation_of(eg).unwrap().1.unwrap();
    let (z_normal, d_normal) = (uk(&m, gr), m.footing_depth(fuss));
    schritt(&mut m, |m| m.set_slab_insulation(slab, 200.0));
    schritt(&mut m, |m| m.set_slab_insulation(slab, 0.0));
    assert_eq!((uk(&m, gr), m.footing_depth(fuss)), (z_normal, d_normal));
    // tief liegendes Haus: die Schürze steht am Mindestmaß
    schritt(&mut m, |m| m.set_terrain_offset(-600.0));
    let z0 = uk(&m, gr);
    let e0 = m.storey(gr).unwrap().embed;
    assert_eq!(m.footing_depth(fuss), Some(MIN_FOOTING));
    schritt(&mut m, |m| m.set_slab_insulation(slab, 200.0));
    assert!((uk(&m, gr) - (z0 - 200.0)).abs() < 1e-6);
    schritt(&mut m, |m| m.set_slab_insulation(slab, 120.0));
    assert!((uk(&m, gr) - (z0 - 120.0)).abs() < 1e-6);
    assert_eq!(m.footing_depth(fuss), Some(MIN_FOOTING + 120.0));
    let text = crate::szo::write(&m);
    let back = crate::szo::read(&text, GuidGen::with_seed(5))
        .unwrap()
        .model;
    assert_eq!(crate::szo::write(&back), text);
    let t = schritt(&mut m, |m| m.set_slab_insulation(slab, 0.0));
    assert_eq!(uk(&m, gr), z0, "UK −320 statt −520");
    assert_eq!(m.footing_depth(fuss), Some(MIN_FOOTING));
    assert_eq!(m.storey(gr).unwrap().embed, e0);
    assert_eq!(m.terrain_offset(), -600.0);
    assert!(m.check().is_empty(), "{:?}", m.check());
    m.apply(&t, Direction::Undo);
    assert!((uk(&m, gr) - (z0 - 120.0)).abs() < 1e-6);
    // Gelände zurück: wieder die normale Gründung
    m.apply(&t, Direction::Redo);
    schritt(&mut m, |m| m.set_terrain_offset(0.0));
    assert_eq!((uk(&m, gr), m.footing_depth(fuss)), (z_normal, d_normal));
}

/// Geometrie-Hinweis 2 zu #50: Eine dickere Platte lässt die Schürze nach
/// derselben 10-cm-Regel wachsen statt abgelehnt zu werden; eine dünnere
/// nimmt das zurück. Liegt UK tief genug, bleibt sie, die Schürze wird
/// kürzer.
#[test]
fn dicke_platte_laesst_die_schuerze_wachsen() {
    let (mut m, eg, gr, slab) = haus();
    let fuss = m.foundation_of(eg).unwrap().1.unwrap();
    let dicke = |m: &Model| match &m.element(slab).unwrap().kind {
        ElementKind::GroundSlab(s) => s.thickness,
        _ => unreachable!(),
    };
    let (z0, t0) = (uk(&m, gr), dicke(&m));
    schritt(&mut m, |m| m.set_slab_thickness(slab, 250.0));
    assert_eq!(uk(&m, gr), z0, "UK bleibt, Schürze kürzer");
    let t = schritt(&mut m, |m| m.set_slab_thickness(slab, 750.0));
    assert_eq!(m.footing_depth(fuss), Some(MIN_FOOTING));
    assert!((uk(&m, gr) - (-750.0 - MIN_FOOTING)).abs() < 1e-6);
    assert_eq!(m.terrain_offset(), 0.0);
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Dämmung darunter: die Schürze wächst weiter
    schritt(&mut m, |m| m.set_slab_insulation(slab, 100.0));
    assert!((uk(&m, gr) - (-950.0)).abs() < 1e-6);
    schritt(&mut m, |m| m.set_slab_insulation(slab, 0.0));
    m.apply(&t, Direction::Undo);
    assert_eq!(uk(&m, gr), z0);
    m.apply(&t, Direction::Redo);
    schritt(&mut m, |m| m.set_slab_thickness(slab, t0));
    assert_eq!(uk(&m, gr), z0, "dünnere Platte nimmt das Wachstum zurück");
    assert_eq!(m.storey(gr).unwrap().embed, None);
    // ungültig oder dicker als im Dialog: abgelehnt, nichts ändert sich
    schritt(&mut m, |m| m.set_slab_thickness(slab, MAX_SLAB_THICKNESS));
    assert!((uk(&m, gr) - (-MAX_SLAB_THICKNESS - MIN_FOOTING)).abs() < 1e-6);
    schritt(&mut m, |m| m.set_slab_thickness(slab, t0));
    for t in [
        0.0,
        -10.0,
        f64::NAN,
        MAX_SLAB_THICKNESS + 10.0,
        MAX_FOUNDATION,
    ] {
        m.begin("Dicke");
        assert!(!m.set_slab_thickness(slab, t), "{t}");
        m.commit();
        assert_eq!((uk(&m, gr), dicke(&m)), (z0, t0), "{t}");
    }
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

/// Review #123 (Hinweis zum Gelände-Zug): `gruendungs_basis` bildet OK EG
/// und Plattentiefe je Gebäude nach; ihr Bereich gleicht je Gründung
/// `foundation_bottom_range_at`, auch mit mehreren Geschossen, dicker
/// Platte und Dämmung.
#[test]
fn bereich_der_basis_wie_je_gruendung() {
    let mut m = Model::with_seed(13);
    for (i, geschosse) in [1u8, 3, 2].into_iter().enumerate() {
        let b = m.add_building(geschosse);
        let ox = i as f64 * 20_000.0;
        let pts = [
            vec3(ox, 0.0, 0.0),
            vec3(ox, 8000.0, 0.0),
            vec3(ox + 10000.0, 8000.0, 0.0),
            vec3(ox + 10000.0, 0.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
    }
    let platten: Vec<ElementId> = m
        .elements()
        .iter()
        .filter(|(_, e)| matches!(e.kind, ElementKind::GroundSlab(_)))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(platten.len(), 3);
    schritt(&mut m, |m| m.set_slab_thickness(platten[1], 350.0));
    schritt(&mut m, |m| m.set_slab_insulation(platten[2], 120.0));
    let basis = m.gruendungs_basis();
    let gruendungen: Vec<(StoreyId, Option<BuildingId>)> = m
        .storeys()
        .iter()
        .filter(|(_, s)| s.kind == LevelKind::Foundation)
        .map(|(id, s)| (id, s.building))
        .collect();
    assert_eq!(gruendungen.len(), 3);
    for (gr, b) in gruendungen {
        for t in [-1500.0, -300.0, 0.0, 450.0] {
            assert_eq!(
                basis.bereich(b, t),
                m.foundation_bottom_range_at(gr, t),
                "{gr:?} bei {t}"
            );
        }
    }
}

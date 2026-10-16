//! Abnahme RB7: Das Raumbuch in den Kosten (Werksstand 19,
//! kosten/werksstand19-raumbuch-plan.md, raumbuch/plan.md §6.2 und §10).
//! Jede Schicht eines Ausbautyps hat ihre Bauleistung nach Regel 81 mit
//! Raum und Seite, die Längen der Räume bepreist `auto=room.*`, die
//! Sockelleiste je Sockeltyp über den Baustoff.

use crate::scene::Scene;
use sk_cost::rechnung::Quelle;
use sk_cost::{Dez, Katalog, Kostenblatt, Umfang};
use sk_model::qto::Schedule;
use sk_model::{Guid, Model};

/// Haus mit zwei Geschossen und je einem Raum samt Ausbau aus der Vorlage
/// (wie das Mengenblatt in `schedule_view_tests.rs`).
fn haus() -> Scene {
    haus_mit(None)
}

/// Wie [`haus`], der Raum im Erdgeschoss mit der Nutzung `nutzung`.
fn haus_mit(nutzung: Option<Guid>) -> Scene {
    use sk_math::vec3;
    let mut m = Model::with_seed(9);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    m.build_from_polygon(b, &pts).unwrap();
    let geschosse: Vec<sk_model::StoreyId> = m
        .levels_in(Some(b))
        .into_iter()
        .filter(|g| m.storey(*g).unwrap().kind == sk_model::LevelKind::Storey)
        .collect();
    m.begin("Räume");
    assert!(sk_raum::abgleichen(&mut m, &geschosse) > 0);
    m.commit();
    for (i, g) in geschosse.into_iter().enumerate() {
        let id = m.spaces(g)[0];
        m.begin("Ausbau");
        if let (0, Some(n)) = (i, nutzung) {
            assert!(m.ensure_space_type(n).is_some());
            assert!(m.set_space(id, |s| s.nutzung = Some(n)));
        }
        assert!(sk_raum::ausbau::typen_anlegen(&mut m, id));
        m.commit();
    }
    Scene::with_model(m)
}

fn leistung(k: &Katalog, kurz: &str) -> Guid {
    k.leistungen
        .iter()
        .find(|l| l.kurz.starts_with(kurz))
        .unwrap_or_else(|| panic!("{kurz}"))
        .guid
}

/// Menge aller Positionen der Bauleistung `g` (0 ohne Position).
fn menge(b: &Kostenblatt, g: Guid) -> Dez {
    Dez(b
        .positionen
        .iter()
        .filter(|p| p.quelle == Quelle::Leistung(g))
        .map(|p| p.menge.0)
        .sum())
}

/// Kostengruppen im Ansatz der Bauleistung `g`.
fn kgs(b: &Kostenblatt, g: Guid) -> Vec<u16> {
    let mut v: Vec<u16> = b
        .positionen
        .iter()
        .filter(|p| p.quelle == Quelle::Leistung(g))
        .flat_map(|p| p.ansatz.iter().filter_map(|a| a.kg))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Fläche (m², 3 Stellen) der Raumzeilen mit dem Baustoff `name`.
fn flaeche(m: &Model, s: &Schedule, name: &str) -> Dez {
    let mm2: f64 = s
        .layer_rows(m)
        .iter()
        .filter(|(_, r)| r.finish.is_some())
        .filter(|(_, r)| m.material(r.material).is_some_and(|x| x.name == name))
        .map(|(_, r)| r.face)
        .sum();
    Dez((mm2 / 1e6 * 1000.0).round() as i64 * (Dez::SKALA / 1000))
}

#[test]
fn raumbuch_in_den_kosten() {
    let mut s = haus();
    let sched = s.schedule_in(&Umfang::projekt());
    let m = s.model();
    let k = sk_cost::lesen::katalog(m, None);
    let mit = sk_cost::lesen::kosten(m, &sched, &k, &Umfang::projekt());
    let ohne = sk_cost::lesen::kosten(m, &sk_model::qto::schedule(m), &k, &Umfang::projekt());
    assert!(mit.netto.0 > ohne.netto.0, "Räume kosten etwas");

    // jede Raumzeile hat ihre Bauleistung: keine Zeile ohne, keine
    // Schätzung, kein Befund der Rechnung, Regel 97 deckt die Ausbautypen
    let raeume: Vec<_> = sched
        .layer_rows(m)
        .iter()
        .filter(|(_, r)| r.finish.is_some())
        .map(|(_, r)| r.element)
        .collect();
    assert!(!raeume.is_empty());
    assert!(
        mit.ohne.iter().all(|o| !raeume.contains(&o.element)),
        "{:#?}",
        mit.ohne
    );
    assert!(mit.befunde.is_empty(), "{:#?}", mit.befunde);
    let stamm = sk_cost::lesen::befunde(m, &k);
    assert!(!stamm.iter().any(|b| b.regel == 97), "{stamm:#?}");

    // Estrich, Trittschall- und Wärmedämmung je mit voller Fläche, nie
    // bill_area (Prüfung #182)
    let estrich = menge(&mit, leistung(&k, "Zementestrich CT-C25-F4 schwimmend"));
    assert!(estrich.0 > 0);
    assert_eq!(estrich, flaeche(m, &sched, "Zementestrich CT-C25-F4"));
    // je Dicke eine Position, jede auf 3 Stellen gerundet
    let gleich = |a: Dez, b: Dez| (a.0 - b.0).abs() <= 2 * Dez::SKALA / 1000;
    for kurz in ["Trittschalldämmung EPS-T", "Wärmedämmung EPS 035 DEO"] {
        let x = menge(&mit, leistung(&k, kurz));
        assert!(gleich(x, estrich), "{kurz}: {x:?} statt {estrich:?}");
    }

    // Putz und Anstrich nach Seite: Wand und Decke getrennt
    for kurz in [
        "Innenputz Gipsputz",
        "Deckenputz Gipsputz",
        "Dispersionsanstrich Wand",
        "Dispersionsanstrich Decke",
    ] {
        assert!(menge(&mit, leistung(&k, kurz)).0 > 0, "{kurz}");
    }
    assert_eq!(
        menge(&mit, leistung(&k, "Deckenputz Gipsputz")),
        menge(&mit, leistung(&k, "Dispersionsanstrich Decke"))
    );

    // Kostengruppe nach Träger aus der Zeile (DIN 276:2018)
    assert_eq!(kgs(&mit, leistung(&k, "Deckenputz Gipsputz")), [354]);
    assert_eq!(kgs(&mit, leistung(&k, "Innenputz Gipsputz")), [336]);
    assert_eq!(
        kgs(&mit, leistung(&k, "Zementestrich CT-C25-F4")),
        [324, 353]
    );
    assert_eq!(
        kgs(&mit, leistung(&k, "Abdichtung Bodenplatte W1-E")),
        [325]
    );

    // Sockelleiste je Sockeltyp genau einmal, Randstreifen am Estrich
    let sockel: f64 = sched
        .auto
        .iter()
        .filter(|a| a.key == "room.skirting")
        .map(|a| a.value)
        .sum();
    assert!(sockel > 0.0);
    let eiche = menge(&mit, leistung(&k, "Sockelleiste Eiche"));
    let fliese = menge(&mit, leistung(&k, "Fliesensockel"));
    assert_eq!(
        eiche.0 + fliese.0,
        (sockel / 1e3 * 1000.0).round() as i64 * (Dez::SKALA / 1000)
    );
    assert!(menge(&mit, leistung(&k, "Randdämmstreifen PE")).0 > 0);
}

/// Übergangsprofil (`room.transition`) im Gewerk des Belags: am
/// Feinsteinzeug die Leistung mit `mat=` (Los 8), an jedem anderen Belag die
/// ohne (Los 9); dieselbe Länge nie zweimal.
#[test]
fn uebergang_je_belag() {
    use sk_model::model::{PARQUET_GUID, PORCELAIN_GUID};
    let mut s = haus();
    let mut sched = (*s.schedule_in(&Umfang::projekt())).clone();
    // beide Beläge im Projekt, auch wenn die Vorlage nur einen nutzt
    let mut m = s.model().clone();
    m.begin("Beläge");
    for g in [PORCELAIN_GUID, PARQUET_GUID] {
        assert!(m.ensure_finish_material(g).is_some());
    }
    m.commit();
    let m = &m;
    let k = sk_cost::lesen::katalog(m, None);
    let vorlage = sched
        .auto
        .iter()
        .find(|a| a.key == "room.skirting")
        .expect("Sockel")
        .clone();
    sched.auto.retain(|a| !a.key.starts_with("room."));
    for (g, mm) in [(PORCELAIN_GUID, 900.0), (PARQUET_GUID, 800.0)] {
        let material = m
            .materials()
            .iter()
            .find(|(_, x)| x.guid == g)
            .map(|(id, _)| id);
        assert!(material.is_some(), "{g:?} im Projekt");
        sched.auto.push(sk_model::qto::AutoMenge {
            key: "room.transition",
            value: mm,
            material,
            ..vorlage.clone()
        });
    }
    let b = sk_cost::lesen::kosten(m, &sched, &k, &Umfang::projekt());
    let fliese = menge(&b, leistung(&k, "Übergangsprofil Edelstahl"));
    let sonst = menge(&b, leistung(&k, "Übergangsprofil Aluminium"));
    assert_eq!(
        (fliese, sonst),
        (Dez(900 * Dez::SKALA / 1000), Dez(800 * Dez::SKALA / 1000))
    );
}

/// Review #197 B1: Unter Feinsteinzeug gleicht die Dämmung des Estrichs den
/// Fußbodenaufbau aus; bepreist wird die wirksame Dicke aus dem Raumbuch,
/// nicht die Nenndicke des Typs (Kurztext, Schicht und Einheitspreis).
#[test]
fn bad_mit_wirksamer_dicke() {
    use sk_model::model::BATH_SPACE_GUID;
    let mut s = haus_mit(Some(BATH_SPACE_GUID));
    let sched = s.schedule_in(&Umfang::projekt());
    let m = s.model();
    let k = sk_cost::lesen::katalog(m, None);
    let b = sk_cost::lesen::kosten(m, &sched, &k, &Umfang::projekt());
    let rows = sched.layer_rows(m);
    let anders: Vec<_> = rows
        .iter()
        .filter_map(|(_, r)| {
            let (t, j) = r.finish?;
            let nenn = m.layer_set(t)?.layers.get(j)?.thickness;
            ((nenn - r.thickness).abs() > 0.5).then_some((r, nenn))
        })
        .collect();
    assert!(!anders.is_empty(), "Bad gleicht die Dämmung aus");
    for (r, nenn) in anders {
        let stoff = m.material(r.material).unwrap().guid;
        let p: Vec<_> = b
            .positionen
            .iter()
            .filter(|p| p.schicht.is_some_and(|(g, _)| g == stoff))
            .filter(|p| p.ansatz.iter().any(|a| a.element == r.element))
            .collect();
        assert!(!p.is_empty(), "{}: keine Position", r.number);
        for p in p {
            assert_eq!(
                p.schicht.unwrap().1,
                sk_cost::zuordnung::dicke(r.thickness),
                "{}: {} statt {} mm",
                p.kurz,
                p.schicht.unwrap().1 .0 as f64 / Dez::SKALA as f64,
                r.thickness
            );
            assert!(
                !p.kurz.contains(&format!("d={}", nenn.round())),
                "{}",
                p.kurz
            );
        }
    }
    assert!(b.befunde.is_empty(), "{:#?}", b.befunde);
}

/// Review #197 H-197c: Eine Sockelleiste aus einem Baustoff ohne eigene
/// Bauleistung bleibt ohne Preis, aber nicht stumm.
#[test]
fn sockel_aus_eigenem_baustoff() {
    let mut s = haus();
    let mut sched = (*s.schedule_in(&Umfang::projekt())).clone();
    let m = s.model();
    let k = sk_cost::lesen::katalog(m, None);
    let gips = m
        .materials()
        .iter()
        .find(|(_, x)| x.name.contains("Gipsputz"))
        .map(|(id, _)| id)
        .expect("Gipsputz");
    for a in sched.auto.iter_mut().filter(|a| a.key == "room.skirting") {
        a.material = Some(gips);
    }
    let b = sk_cost::lesen::kosten(m, &sched, &k, &Umfang::projekt());
    assert_eq!(menge(&b, leistung(&k, "Sockelleiste Eiche")).0, 0);
    assert_eq!(menge(&b, leistung(&k, "Fliesensockel")).0, 0);
    assert!(
        b.befunde
            .iter()
            .any(|f| f.satz.contains("keine Bauleistung mit diesem Baustoff")
                && f.satz.contains("room.skirting")),
        "{:#?}",
        b.befunde
    );
}

/// LVs aller Lose mit Preisen und ohne Untertitel.
fn lvs(m: &Model, sched: &Schedule, k: &Katalog) -> Vec<sk_cost::lv::Lv> {
    k.lose
        .iter()
        .filter(|l| !l.retired && l.parent.is_none())
        .map(|l| {
            let w = sk_cost::lv::LvWahl {
                los: l.guid,
                untertitel: false,
                preise: true,
                heute: None,
            };
            sk_cost::lesen::lv(m, sched, k, &Umfang::projekt(), &w)
        })
        .collect()
}

/// R1-B3 (Integration 15.10.): Die Dämmung unter dem Estrich gleicht den
/// Fußbodenaufbau aus, ihre Dicke hängt am Belag und am Geschoss. Das LV
/// teilt die Bauleistung je Dicke in Teilpositionen mit Index (DIN 18353
/// Abschnitt 0.5); Σ LV = netto. Mit einer Dicke bleibt die OZ ohne Index.
#[test]
fn estrichdaemmung_je_dicke() {
    let mut s = haus();
    let sched = s.schedule_in(&Umfang::projekt());
    let m = s.model();
    let k = sk_cost::lesen::katalog(m, None);
    let b = sk_cost::lesen::kosten(m, &sched, &k, &Umfang::projekt());
    let alle = lvs(m, &sched, &k);
    let summe: i64 = alle
        .iter()
        .map(|lv| {
            let z = &lv.zusammenstellung;
            z.netto.unwrap().0 + z.ohne_los.map_or(0, |c| c.0)
        })
        .sum();
    assert_eq!(summe, b.netto.0, "Σ LV = netto");
    assert!(
        alle.iter()
            .flat_map(|lv| &lv.befunde)
            .all(|f| f.regel != 101),
        "keine Regel 101"
    );
    let daemmung = leistung(&k, "Wärmedämmung EPS 035 DEO unter Estrich");
    let teile: Vec<(String, String)> = alle
        .iter()
        .flat_map(|lv| lv.titel.iter().flat_map(|t| &t.positionen))
        .filter(|p| p.quelle == daemmung)
        .map(|p| (p.oz.clone(), p.kurztext.clone()))
        .collect();
    let oz = k.oz(k.leistung(daemmung).unwrap());
    assert_eq!(teile.len(), 2, "{teile:?}");
    for (i, (z, kurz)) in teile.iter().enumerate() {
        assert_eq!(*z, format!("{oz}.{}", i + 1));
        let d = if i == 0 { "40" } else { "45" };
        assert!(kurz.ends_with(&format!(", d = {d} mm")), "{kurz}");
        assert!(kurz.chars().count() <= 70, "{kurz}");
    }

    // Bad im EG: 45 mm unter Fliese auf der Sohlplatte wie 45 mm unter
    // Parkett auf der Decke, also eine Position ohne Index
    let mut s = haus_mit(Some(sk_model::model::BATH_SPACE_GUID));
    let sched = s.schedule_in(&Umfang::projekt());
    let m = s.model();
    let k = sk_cost::lesen::katalog(m, None);
    let ozs: Vec<String> = lvs(m, &sched, &k)
        .iter()
        .flat_map(|lv| lv.titel.iter().flat_map(|t| &t.positionen))
        .filter(|p| p.quelle == daemmung)
        .map(|p| p.oz.clone())
        .collect();
    assert_eq!(ozs, vec![oz]);
}

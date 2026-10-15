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
    for g in geschosse {
        let id = m.spaces(g)[0];
        m.begin("Ausbau");
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

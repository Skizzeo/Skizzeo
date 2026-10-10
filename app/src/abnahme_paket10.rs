//! Abnahme Morgenpaket 10.10. (Gelände Thema 1 und 4, Erdarbeiten Thema 2,
//! Bauvorbereitung Thema 3) und Erweiterungen, Katalog A320–A326 (A323
//! Geländelinie ist ein Bildvergleich, `gelaende/bilder/`). Rechnet über
//! die öffentliche Schnittstelle von `sk-model` und `sk-cost` und über den
//! Speicherweg der App (`szo::write`, `szo::read_with` mit den
//! Kostenabschnitten). Sollwerte: `bim/integration/sollwerte-referenzhaeuser.md`
//! §0, Mengenregeln `kosten/erdarbeiten-recherche.md` und
//! `kosten/bauvorbereitung-recherche.md` §3.3.

use sk_math::vec3;
use sk_model::erweiterung::{ExtDef, ExtPart};
use sk_model::{qto, szo, BuildingId, Category, ElementId, GuidGen, LevelKind, Model, RunId};

const RH1: &str = include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo");

const SZB: [&str; 5] = [
    include_str!("../../crates/sk-szb/beispiele/werk.bodenplatte.szb"),
    include_str!("../../crates/sk-szb/beispiele/werk.stabgelaender.szb"),
    include_str!("../../crates/sk-szb/beispiele/werk.streifenfundament.szb"),
    include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb"),
    include_str!("../../crates/sk-szb/beispiele/werk.treppe.szb"),
];

fn laden(text: &str) -> Model {
    let l =
        szo::read_with(text, GuidGen::with_seed(1), &sk_cost::lesen::ABSCHNITTE_SZO).expect("lädt");
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    l.model
}

fn rund(m: &Model) -> String {
    let t = szo::write(m);
    assert_eq!(szo::write(&laden(&t)), t, "Speichern und Laden bytegleich");
    t
}

fn schritt(m: &mut Model, f: impl FnOnce(&mut Model) -> bool) {
    m.begin("Abnahme");
    assert!(f(m));
    m.commit().expect("Schritt");
}

/// Zwei gleiche Häuser 10 × 8 m nebeneinander: Gebäude, Außenwandzug,
/// Gründungsgeschoss, Sohlplatte.
fn zwei_haeuser() -> (Model, [(BuildingId, RunId, ElementId); 2]) {
    let mut m = Model::with_seed(12);
    let mut out = Vec::new();
    for x0 in [0.0, 14_000.0] {
        m.begin("Haus");
        let b = m.add_building(1);
        let pts = [
            vec3(x0, 0.0, 0.0),
            vec3(x0, 8000.0, 0.0),
            vec3(x0 + 10_000.0, 8000.0, 0.0),
            vec3(x0 + 10_000.0, 0.0, 0.0),
        ];
        let run = m.build_from_polygon(b, &pts).unwrap();
        m.commit().expect("Haus");
        let slab = m.foundation_of(run).unwrap().0;
        out.push((b, run, slab));
    }
    (m, [out[0], out[1]])
}

/// Automatikmengen eines Gebäudes, sortiert: (Bauteilnummer, Schlüssel,
/// Wert gerundet in der kleinsten Einheit).
fn mengen(m: &Model, b: Option<BuildingId>) -> Vec<(String, &'static str, i64)> {
    let mut v: Vec<_> = qto::schedule(m)
        .auto
        .iter()
        .filter(|a| b.is_none() || a.building == b)
        .map(|a| (a.number.clone(), a.key, a.value.round() as i64))
        .collect();
    v.sort();
    v
}

fn summe(v: &[(String, &'static str, i64)], k: &str) -> i64 {
    v.iter().filter(|x| x.1 == k).map(|x| x.2).sum()
}

/// A320 Erweiterungen: fünf Werkbank-Bauteile mit je zwei Exemplaren (eines
/// gedreht) speichern und laden bytegleich; jedes Exemplar hat eine eigene
/// Nummer und ist mit Eigenschaften und Drehung wieder da.
#[test]
fn abnahme_a320_erweiterungen_rundlauf() {
    let mut m = Model::with_seed(3);
    let b = m.add_building(1);
    let eg = m
        .storeys()
        .iter()
        .find(|(_, s)| s.short == "EG" && s.building == Some(b))
        .map(|(id, _)| id)
        .unwrap();
    for (i, t) in SZB.iter().enumerate() {
        let d = ExtDef::lesen(t).unwrap();
        m.put_ext_def(d.clone()).unwrap();
        let x = 1000.0 + 1500.0 * i as f64;
        m.add_ext(eg, ExtPart::new(&d, [x, 2000.0])).unwrap();
        let mut p = ExtPart::new(&d, [x, 5000.0]);
        p.rot = 90.0;
        m.add_ext(eg, p).unwrap();
    }
    let t = rund(&m);
    assert_eq!(t.matches("[extdef]").count(), 5);
    assert_eq!(t.matches("[extpart]").count(), 10);
    assert_eq!(t.matches(" rot=90").count(), 5);
    let mut nummern: Vec<&str> = t
        .lines()
        .filter(|l| l.starts_with("[extpart]"))
        .filter_map(|l| l.split(" number=\"").nth(1)?.split('"').next())
        .collect();
    nummern.sort();
    nummern.dedup();
    assert_eq!(nummern.len(), 10, "jede Nummer einmal");
}

/// A321 Versatz je Gebäude: Haus 1 auf +0,40 und −1,50, Haus 2 bleibt
/// unberührt; `[building] terrain=` nur am geänderten Gebäude; zurück auf 0
/// ist die Datei wie vorher.
#[test]
fn abnahme_a321_versatz_je_gebaeude() {
    let (mut m, [h1, h2]) = zwei_haeuser();
    let t0 = rund(&m);
    let g2 = m.ground_basis(h2.1).unwrap();
    for v in [400.0, -1500.0] {
        schritt(&mut m, |m| m.set_terrain_offset_of(h1.0, v));
        assert_eq!(m.terrain_offset_of(Some(h1.0)), v);
        assert_eq!(m.terrain_z_at(Some(h1.0), 5000.0, 4000.0), -v);
        assert_eq!(m.ground_basis(h2.1).unwrap(), g2, "Haus 2 unberührt");
        let t = rund(&m);
        assert_eq!(t.matches(" terrain=").count(), 1, "nur Haus 1");
    }
    assert!(
        !m.set_terrain_offset_of(h1.0, 3001.0),
        "über 3,00 m abgelehnt"
    );
    schritt(&mut m, |m| m.set_terrain_offset_of(h1.0, 0.0));
    assert_eq!(rund(&m), t0);
}

/// A322 Frostschürze: bei jedem Versatz −3,00 bis +3,00 m reicht die
/// Gründung mindestens 0,80 m unter Gelände; `embed=` steht nur in der
/// Datei, solange die Schürze an ihrem Mindestmaß steht; hin und zurück
/// ist die Schürze wie vorher.
#[test]
fn abnahme_a322_frostschuerze() {
    let (mut m, [h1, _]) = zwei_haeuser();
    let fuss = m.foundation_of(h1.1).unwrap().1.unwrap();
    let gr = m
        .storeys()
        .iter()
        .find(|(_, s)| s.kind == LevelKind::Foundation && s.building == Some(h1.0))
        .map(|(id, _)| id)
        .unwrap();
    let d0 = m.footing_depth(fuss).unwrap();
    for v in [
        -3000.0, -1500.0, -800.0, -200.0, 200.0, 800.0, 1500.0, 3000.0,
    ] {
        schritt(&mut m, |m| m.set_terrain_offset_of(h1.0, v));
        assert!(m.frost_safe(gr), "Versatz {v}");
        assert!(m.embedment_of(gr).unwrap() >= 800.0 - 1e-6, "Versatz {v}");
        let t = rund(&m);
        // die Schürze stößt nur beim tiefen Einsetzen an ihr Mindestmaß
        assert_eq!(t.contains(" embed="), v <= -800.0, "Versatz {v}");
    }
    schritt(&mut m, |m| m.set_terrain_offset_of(h1.0, 0.0));
    assert_eq!(m.footing_depth(fuss), Some(d0), "hin und zurück");
}

/// A324 Perimeterdämmung am RH-1: 120 mm heben das Haus über sein Gelände,
/// die Erdmengen bleiben gleich; seit Werksbestand Stand 9 rechnet sie als
/// Position B70 (vorher ein Befund statt einer stillen Lücke); 10 mm abgelehnt,
/// 0 entfernt sie wieder.
#[test]
fn abnahme_a324_perimeterdaemmung() {
    let mut m = laden(RH1);
    let t0 = rund(&m);
    let slab = m
        .elements()
        .iter()
        .find(|(_, e)| e.category == Category::GroundSlab)
        .map(|(id, _)| id)
        .unwrap();
    let erde = |m: &Model| -> Vec<_> {
        mengen(m, None)
            .into_iter()
            .filter(|x| x.1.starts_with("earth."))
            .collect()
    };
    let e0 = erde(&m);
    assert!(!m.set_slab_insulation(slab, 10.0), "unter 20 mm abgelehnt");
    schritt(&mut m, |m| m.set_slab_insulation(slab, 120.0));
    let b = m.storey(m.element(slab).unwrap().storey).unwrap().building;
    assert_eq!(m.terrain_offset_of(b), 120.0, "Haus 120 mm höher");
    assert_eq!(erde(&m), e0, "Erdmengen gleich");
    let t = rund(&m);
    assert!(t.contains("[perimeter]") && t.contains(" insulation=120"));
    let k = sk_cost::lesen::katalog(&m, None);
    let blatt = sk_cost::lesen::kosten(&m, &qto::schedule(&m), &k, &sk_cost::Umfang::projekt());
    // Seit Werksbestand Stand 9 hat die Perimeterdämmung ihren Satz (B70)
    assert!(
        !blatt
            .befunde
            .iter()
            .any(|b| b.satz.contains("Perimeterdämmung")),
        "Perimeterdämmung mit Satz ohne Befund"
    );
    assert!(
        blatt.positionen.iter().any(|p| p
            .kurz
            .starts_with("Perimeterdämmung XPS 300 unter Bodenplatte")),
        "Perimeterdämmung als Position B70"
    );
    schritt(&mut m, |m| m.set_slab_insulation(slab, 0.0));
    // Der angelegte Baustoff bleibt im Projekt (wie jeder angelegte
    // Baustoff), die Nummernfolge PD merkt sich die vergebene Nummer
    let ohne: String = rund(&m)
        .lines()
        .filter(|l| !l.contains("XPS Perimeterdämmung"))
        .map(|l| format!("{}\n", l.replace(" next=PD:1", "")))
        .collect();
    assert!(ohne == t0, "ohne Dämmung wie vorher");
}

/// A325 Erdarbeiten: RH-1 rechnet die Mengen aus der Gründung wie
/// sollwerte-referenzhaeuser §0; ein Haus 1,50 m im Gelände bekommt
/// Böschung und Verfüllung, eines 0,60 m darüber Auffüllung statt Aushub;
/// das Nachbarhaus bleibt.
#[test]
fn abnahme_a325_erdarbeiten() {
    let rh1 = mengen(&laden(RH1), None);
    for (k, soll) in [
        ("earth.topsoil", 42_900_000_000),
        ("earth.excavation", 4_752_000_000),
        ("earth.trench", 6_055_000_000),
        ("earth.subgrade", 67_890_000),
        ("earth.gravel", 10_183_500_000),
        ("earth.disposal", 10_807_000_000),
    ] {
        let ist = summe(&rh1, k);
        assert!((ist - soll).abs() <= 1_000_000, "{k}: {ist} statt {soll}");
    }
    let (mut m, [h1, h2]) = zwei_haeuser();
    let m1 = mengen(&m, Some(h1.0));
    schritt(&mut m, |m| m.set_terrain_offset_of(h2.0, -1500.0));
    let tief = mengen(&m, Some(h2.0));
    assert!(summe(&tief, "earth.slope") > 0, "Böschung über 1,25 m");
    assert!(summe(&tief, "earth.backfill") > 0, "Arbeitsraum verfüllen");
    assert_eq!(mengen(&m, Some(h1.0)), m1, "Nachbarhaus bleibt");
    schritt(&mut m, |m| m.set_terrain_offset_of(h2.0, 600.0));
    let hoch = mengen(&m, Some(h2.0));
    assert!(summe(&hoch, "earth.fill") > 0, "Auffüllung");
    assert_eq!(summe(&hoch, "earth.slope"), 0);
    assert_eq!(mengen(&m, Some(h1.0)), m1, "Nachbarhaus bleibt");
}

/// A326 Bauvorbereitung: RH-1 mit Bauzaun 60 m, Gerüst 311,344 m²,
/// 3 Monaten und einer Pauschale; zwei Platten in einem Gebäude zählen
/// Pauschale, Monate und Bauzaun einmal, an derselben Platte auch nach
/// Speichern und Laden; das Gerüst steht auf dem Gelände des eigenen Hauses.
#[test]
fn abnahme_a326_bauvorbereitung() {
    let rh1 = mengen(&laden(RH1), None);
    assert_eq!(summe(&rh1, "site.lump"), 1);
    assert_eq!(summe(&rh1, "site.months"), 3);
    assert_eq!(summe(&rh1, "site.fence"), 60_000);
    assert!((summe(&rh1, "site.scaffold") - 311_344_000).abs() <= 1000);

    let mut m = Model::with_seed(12);
    let b = m.add_building(2);
    for (x0, w, d) in [(0.0, 10_000.0, 8_000.0), (20_000.0, 6_000.0, 5_000.0)] {
        let pts = [
            vec3(x0, 0.0, 0.0),
            vec3(x0 + w, 0.0, 0.0),
            vec3(x0 + w, d, 0.0),
            vec3(x0, d, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
    }
    let v = mengen(&m, Some(b));
    for k in ["site.lump", "site.months", "site.fence"] {
        assert_eq!(v.iter().filter(|x| x.1 == k).count(), 1, "{k} einmal");
    }
    assert_eq!(v.iter().filter(|x| x.1 == "site.scaffold").count(), 2);
    let t = rund(&m);
    assert_eq!(mengen(&laden(&t), None), mengen(&m, None), "Träger bleibt");

    let (mut m, [h1, h2]) = zwei_haeuser();
    let g1 = summe(&mengen(&m, Some(h1.0)), "site.scaffold");
    schritt(&mut m, |m| m.set_terrain_offset_of(h2.0, 600.0));
    assert_eq!(summe(&mengen(&m, Some(h1.0)), "site.scaffold"), g1);
    // Gerüstlänge (10 + 8) · 2 + 8 · 1,30 = 46,40 m, 0,60 m höher
    let g2 = summe(&mengen(&m, Some(h2.0)), "site.scaffold");
    assert!((g2 - g1 - 46_400 * 600).abs() <= 1000, "{g2} − {g1}");
}

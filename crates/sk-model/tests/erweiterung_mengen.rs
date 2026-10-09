//! Mengen der Erweiterungsbauteile in der Mengenliste (Schrittplan E8):
//! eine Gruppe je Bauteil mit den `[menge]`-Zeilen, Werte wie die Werkbank
//! (Abnahmetabelle des Auftrags, EG mit GH 2855 mm und Decke 220 mm).

use sk_model::erweiterung::{ExtDef, ExtPart};
use sk_model::qto::{schedule, ElementQto, ExtQto, GroupQto, Umfang};
use sk_model::{Category, ElementId, Model, StoreyId};

const BEISPIELE: [&str; 5] = [
    include_str!("../../sk-szb/beispiele/werk.bodenplatte.szb"),
    include_str!("../../sk-szb/beispiele/werk.stabgelaender.szb"),
    include_str!("../../sk-szb/beispiele/werk.streifenfundament.szb"),
    include_str!("../../sk-szb/beispiele/werk.stuetze.szb"),
    include_str!("../../sk-szb/beispiele/werk.treppe.szb"),
];

fn geschoss(m: &Model, kurz: &str) -> StoreyId {
    m.storeys()
        .iter()
        .find(|(_, s)| s.short == kurz && s.building.is_some())
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Geschoss {kurz}"))
}

fn projekt() -> Model {
    let mut m = Model::with_seed(1);
    m.add_building(2);
    for t in BEISPIELE {
        m.put_ext_def(ExtDef::lesen(t).unwrap()).unwrap();
    }
    m
}

fn setzen(m: &mut Model, key: &str, kurz: &str, at: [f64; 2]) -> ElementId {
    let s = geschoss(m, kurz);
    let p = ExtPart::new(m.ext_def(key).unwrap(), at);
    m.add_ext(s, p).unwrap()
}

fn gruppen(m: &Model, kurz: &str) -> Vec<GroupQto> {
    let st = geschoss(m, kurz);
    let s = schedule(m);
    s.buildings
        .iter()
        .flat_map(|b| &b.storeys)
        .find(|x| x.id == st)
        .map(|x| x.groups.clone())
        .unwrap_or_default()
}

fn ext(g: &GroupQto, i: usize) -> &ExtQto {
    match &g.rows[i].q {
        Some(ElementQto::Ext(x)) => x,
        q => panic!("{q:?} {:?}", g.rows[i].note),
    }
}

/// „Beton 0,152 m³“ wie in der Abnahmetabelle.
fn zeilen(x: &ExtQto) -> Vec<String> {
    x.mengen
        .iter()
        .map(|q| {
            let d = match q.einheit.as_str() {
                "m3" | "t" => 3,
                "stk" => 0,
                _ => 2,
            };
            format!("{} {}", sk_szb::zahl(q.wert.unwrap(), d), q.einheit)
        })
        .collect()
}

#[test]
fn abnahmetabelle_in_der_mengenliste() {
    let mut m = projekt();
    let g = m.ext_geschoss(geschoss(&m, "EG"));
    assert_eq!((g.gh, g.decke), (2855.0, 220.0), "Prüfgeschoss");
    for (i, d) in [
        "werk.treppe",
        "werk.stuetze",
        "werk.bodenplatte",
        "werk.streifenfundament",
        "werk.stabgelaender",
    ]
    .iter()
    .enumerate()
    {
        setzen(&mut m, d, "EG", [1000.0 * i as f64, 0.0]);
    }
    let gs: Vec<GroupQto> = gruppen(&m, "EG")
        .into_iter()
        .filter(|g| g.category == Category::Extension)
        .collect();
    // Eine Gruppe je Bauteil, nach Name in der Mehrzahl
    let keys: Vec<&str> = gs.iter().map(|g| g.ext.as_deref().unwrap()).collect();
    assert_eq!(
        keys,
        [
            "werk.bodenplatte",
            "werk.treppe",
            "werk.stabgelaender",
            "werk.stuetze",
            "werk.streifenfundament"
        ]
    );
    let soll: [&[&str]; 5] = [
        &["4,800 m3", "20,00 m", "0,384 t"],
        &["1,546 m3", "21,24 m2", "0,155 t", "1 stk"],
        &["3,00 m", "4 stk"],
        &["0,152 m3", "2,53 m2", "0,023 t", "1 stk"],
        &["2,000 m3", "0,080 t"],
    ];
    for (g, s) in gs.iter().zip(soll) {
        assert_eq!(g.rows.len(), 1);
        assert_eq!(g.total.count, 1);
        assert_eq!(zeilen(ext(g, 0)), s, "{:?}", g.ext);
    }
    let x = ext(&gs[3], 0);
    let namen: Vec<&str> = x.mengen.iter().map(|q| q.name.as_str()).collect();
    assert_eq!(namen[0], "Beton C25/30");
    assert!(x.volume > 0.15e9 && x.volume < 0.16e9, "{}", x.volume);
    // Gewerk und Kostengruppe aus dem Bauteil
    let beton = m.trade_by_code("18331");
    assert!(beton.is_some());
    assert!(x
        .mengen
        .iter()
        .all(|q| q.gewerk == beton && q.kg == Some(343)));
    let gl = ext(&gs[2], 0);
    assert_eq!(gl.mengen[0].gewerk, m.trade_by_code("18360"));
    assert_eq!(gl.mengen[0].kg, Some(359));
}

#[test]
fn zweite_stuetze_und_obergeschoss() {
    let mut m = projekt();
    let a = setzen(&mut m, "werk.stuetze", "EG", [0.0, 0.0]);
    let b = setzen(&mut m, "werk.stuetze", "EG", [3000.0, 0.0]);
    setzen(&mut m, "werk.stuetze", "OG", [0.0, 0.0]);
    let eg: Vec<GroupQto> = gruppen(&m, "EG")
        .into_iter()
        .filter(|g| g.ext.is_some())
        .collect();
    assert_eq!(eg.len(), 1);
    let rows: Vec<ElementId> = eg[0].rows.iter().map(|r| r.element).collect();
    assert_eq!(rows, [a, b]);
    assert_eq!(eg[0].total.count, 2);
    assert!((eg[0].total.volume - 2.0 * ext(&eg[0], 0).volume).abs() < 1.0);
    let og = gruppen(&m, "OG");
    assert_eq!(og.iter().filter(|g| g.ext.is_some()).count(), 1);
    // Umfang ohne OG: die Stütze im OG fällt heraus
    let s = schedule(&m);
    let u = Umfang {
        gebaeude: None,
        ohne: vec![geschoss(&m, "OG")],
    };
    let r = s.restrict(&m, &u);
    let n: usize = r
        .buildings
        .iter()
        .flat_map(|b| &b.storeys)
        .flat_map(|s| &s.groups)
        .filter(|g| g.ext.is_some())
        .map(|g| g.rows.len())
        .sum();
    assert_eq!(n, 2);
    // Kein Anteil an Gewerk, Kostengruppe oder Baustoffsumme der Wände
    assert!(s.layer_rows(&m).is_empty());
}

use super::*;
// Abnahmetests K4 (Jörns Wandtypen, Luftschicht, Schürzenbreite 30–45 cm)
// und K5 (monolithische Wand mit Randdämmstreifen), vorbereitet gegen main
// 137fca7. Paket: bim/paket-k4-k5-wandtypen.md („Fertig, wenn“),
// Spezifikation test/abnahme-wandtypen.md.
//
// Einbau: als `mod wandtypen { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, og_zug, wall_m3,
// decke_mengen, sohlplatte, tippe, m2, m3, cam3d; aus mod katalog nichts.
//
// Wichtig: Die Tests wechseln die Außenwand ausdrücklich auf den Werkstyp
// (set_run_type) und hängen nicht davon ab, welcher Typ in
// Model::with_seed Standard ist. Den neuen Standard AW-36 prüft A110 an
// Model::new().
//
// Angenommene Namen stehen NUR in den Adaptern. Weicht der Bau ab, bitte
// nur die Adapter anpassen.

use sk_model::{
    Bearing, Category, Element, ElementId, ElementKind, GuidGen, LayerFunction, LayerSet,
    LayerSetId, MatCategory, Model,
};

// ===== Adapter =====

/// Werkstyp nach Kurzzeichen.
fn typ(m: &Model, code: &str) -> LayerSetId {
    m.layer_sets()
        .iter()
        .find(|(_, t)| t.code == code)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Typ {code} fehlt"))
}
/// Typ eines Wandzugs wechseln (K1), ein Rückgängig-Schritt.
fn zugtyp(s: &mut Scene, run: RunId, id: LayerSetId) -> bool {
    s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id))
}
/// U-Wert eines Typs (nur Anzeige); `None` bei Innenwand oder fehlendem λ.
fn u_wert(m: &Model, id: LayerSetId) -> Option<f64> {
    m.u_value(id)
}
/// Deckenauflager des Typs: `None` = ganzer Kern, sonst (Tiefe mm, Baustoff).
fn auflager(t: &LayerSet) -> Option<(f64, sk_model::MaterialId)> {
    match t.bearing {
        Bearing::Core => None,
        Bearing::Depth { depth, strip } => Some((depth, strip)),
    }
}
fn setze_auflager(t: &mut LayerSet, tiefe: Option<(f64, sk_model::MaterialId)>) {
    t.bearing = match tiefe {
        None => Bearing::Core,
        Some((depth, strip)) => Bearing::Depth { depth, strip },
    };
}
/// Länge auf der Streifenachse (mm) und Volumen (mm³) eines Randdämmstreifens.
fn streifen_qto(s: &Scene, el: ElementId) -> (f64, f64) {
    let q = s.edge_strip_qto(el).expect("Streifenmengen");
    (q.length, q.volume)
}
/// (Wand, Decke) eines Randdämmstreifens.
fn streifen_von(m: &Model, el: ElementId) -> Option<(ElementId, ElementId)> {
    match m.element(el)?.kind {
        ElementKind::EdgeStrip { wall, floor } => Some((wall, floor)),
        _ => None,
    }
}
/// Bauteil ist ein Randdämmstreifen.
fn ist_streifen(e: &Element) -> bool {
    e.category == Category::EdgeInsulation
}
/// Baustoffkategorie Luft.
fn ist_luft(c: MatCategory) -> bool {
    c == MatCategory::Air
}
/// Firmenkatalog laden (App, K2): Hinweise statt Fehler.
fn firma_laden(p: &std::path::Path, vorgabe: bool) -> (crate::catalog::Company, Vec<String>) {
    crate::catalog::Company::load(p, vorgabe)
}

// ===== Hilfen (keine Annahmen über K4/K5 hinaus) =====

fn r4(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

/// Wechselt den Zug auf den Werkstyp, falls er ihn noch nicht hat.
fn auf_typ(s: &mut Scene, run: RunId, code: &str) {
    let id = typ(s.model(), code);
    let hat = s
        .model()
        .element(s.model().wall_at(run, 0).unwrap())
        .unwrap()
        .layer_set;
    if hat != Some(id) {
        assert!(zugtyp(s, run, id), "Wechsel auf {code}");
    }
}

/// Volumen (m³) aller Schichten eines Baustoffs in den 4 Segmenten des Zugs.
fn baustoff_m3(s: &Scene, run: RunId, name: &str) -> f64 {
    let m = s.model();
    let mut v = 0.0;
    for i in 0..4 {
        let q = s.wall_qto(m.wall_at(run, i).unwrap()).unwrap();
        for l in &q.layers {
            if m.material(l.material).unwrap().name == name {
                v += l.volume;
            }
        }
    }
    r4(v / 1e9)
}

/// Summe der Wandvolumen (m³) der 4 Segmente.
fn wand_m3(s: &Scene, run: RunId) -> f64 {
    let m = s.model();
    r4((0..4)
        .map(|i| s.wall_qto(m.wall_at(run, i).unwrap()).unwrap().volume)
        .sum::<f64>()
        / 1e9)
}

fn fs_m3(s: &Scene, aw: RunId) -> f64 {
    m3(s.foundation_qto(aw).unwrap().1.volume)
}
fn sp_m3(s: &Scene, aw: RunId) -> f64 {
    m3(s.foundation_qto(aw).unwrap().0.volume)
}

/// Randdämmstreifen im Modell: (Nummer, Element), nach Nummer.
fn streifen(s: &Scene) -> Vec<(String, ElementId)> {
    let mut v: Vec<_> = s
        .model()
        .elements()
        .iter()
        .filter(|(_, e)| ist_streifen(e))
        .map(|(id, e)| (e.number.clone(), id))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn pruefung(text: &str) -> Vec<String> {
    match sk_model::szo::read(text, GuidGen::with_seed(1)) {
        Err(e) => vec![e.to_string()],
        Ok(l) => {
            let mut v = l.model.check();
            v.extend(l.hints);
            v
        }
    }
}

// ===== Tests K4 =====

/// A110 (K4, Startbestand): Werkstypen AW-36, AW-49, IW-11,5, IW-17,5,
/// IW-24 mit Schichten außen → innen, AW-31,5 bleibt. Ein neues Projekt hat
/// den Standard AW-36 / IW-17,5. Feste Guids (gleich in jedem Projekt), die
/// Guids aus 137fca7 bleiben. Neue Baustoffe mit λ, Luft in eigener
/// Kategorie. U-Wert nur für Außenwände: AW-36 0,16, AW-49 0,15.
#[test]
fn a110_werkstypen_und_standard() {
    let m = Model::with_seed(110);
    let aufbau = |code: &str| -> (String, bool, Vec<(String, f64, bool)>, f64) {
        let t = m.layer_set(typ(&m, code)).unwrap();
        (
            t.name.clone(),
            t.category == sk_model::TypeCategory::ExteriorWall,
            t.layers
                .iter()
                .map(|l| {
                    (
                        m.material(l.material).unwrap().name.clone(),
                        l.thickness,
                        l.core,
                    )
                })
                .collect(),
            t.thickness(),
        )
    };
    let s = |x: &str| x.to_string();
    assert_eq!(
        aufbau("AW-36"),
        (
            s("AW mit WDVS 36"),
            true,
            vec![
                (s("Dämmung (WDVS)"), 120.0, false),
                (s("Porenbeton"), 240.0, true)
            ],
            360.0
        )
    );
    let aw49 = aufbau("AW-49");
    assert_eq!(
        (aw49.0.as_str(), aw49.1, aw49.3),
        ("AW mehrschalig 49", true, 490.0)
    );
    assert_eq!(
        aw49.2.iter().map(|l| (l.1, l.2)).collect::<Vec<_>>(),
        [(115.0, false), (60.0, false), (140.0, false), (175.0, true)]
    );
    assert_eq!(aw49.2[1].0, "Luft");
    for (code, d) in [("IW-11,5", 115.0), ("IW-17,5", 175.0), ("IW-24", 240.0)] {
        let a = aufbau(code);
        assert!(!a.1, "{code} Innenwand");
        assert_eq!(a.2, [(s("Porenbeton"), d, true)], "{code}");
    }
    assert_eq!(aufbau("AW-31,5").3, 315.0, "AW-31,5 bleibt");
    let luft = m.layer_set(typ(&m, "AW-49")).unwrap().layers[1];
    assert_eq!(luft.function, LayerFunction::AirGap);
    let mat = |name: &str| {
        m.materials()
            .iter()
            .find(|(_, x)| x.name.starts_with(name))
            .map(|(_, x)| x.clone())
            .unwrap_or_else(|| panic!("Baustoff {name}"))
    };
    assert!(ist_luft(mat("Luft").category));
    for (name, lambda) in [
        ("Verblender", 0.68),
        ("Kerndämmung", 0.035),
        ("Porenbeton", 0.09),
        ("Dämmung (WDVS)", 0.035),
        ("Stahlbeton", 2.3),
        ("Putz", 0.87),
    ] {
        assert_eq!(mat(name).lambda, Some(lambda), "λ {name}");
    }
    // U-Wert
    let u = |code: &str| u_wert(&m, typ(&m, code)).map(|u| (u * 100.0).round() / 100.0);
    assert_eq!(u("AW-36"), Some(0.16));
    assert_eq!(u("AW-49"), Some(0.15));
    assert_eq!(u("IW-17,5"), None, "Innenwand ohne U-Wert");
    // Standard und feste Guids
    let neu = Model::new();
    let d = neu.defaults();
    assert_eq!(neu.layer_set(d.exterior_wall).unwrap().code, "AW-36");
    assert_eq!(neu.layer_set(d.interior_wall).unwrap().code, "IW-17,5");
    for code in ["AW-31,5", "AW-36", "AW-49", "IW-11,5", "IW-17,5", "IW-24"] {
        assert_eq!(
            neu.layer_set(typ(&neu, code)).unwrap().guid,
            m.layer_set(typ(&m, code)).unwrap().guid,
            "{code}: feste Guid"
        );
    }
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// A111 (K4, AW-36): Prüfhaus mit AW-36 je Geschoss: WDVS 12,1692, Porenbeton
/// netto 21,5522 m³, Decke 75,7376 m² / 16,6623 m³, SP 17,600, FS 7,0238 m³;
/// Innenwand IW-11,5 2,2060, IW-17,5 3,3570, IW-24 4,6039 m³. EG und OG
/// gleich, alle gebunden (A-09).
#[test]
fn a111_mengen_aw36_und_innenwaende() {
    let mut s = Scene::with_model(Model::with_seed(111));
    let (aw, iw) = haus_b11(&mut s);
    auf_typ(&mut s, aw, "AW-36");
    let og = og_zug(&s, aw);
    for run in [aw, og] {
        assert_eq!(baustoff_m3(&s, run, "Dämmung (WDVS)"), 12.1692);
        assert_eq!(baustoff_m3(&s, run, "Porenbeton"), 21.5522);
        let d = decke_mengen(&s, run);
        assert_eq!((d.0, d.1), (75.7376, 16.6623));
    }
    assert_eq!((sp_m3(&s, aw), fs_m3(&s, aw)), (17.6, 7.0238));
    for (code, soll) in [("IW-11,5", 2.2060), ("IW-17,5", 3.3570), ("IW-24", 4.6039)] {
        auf_typ(&mut s, iw, code);
        assert_eq!(r4(wall_m3(&s, iw, 0)), soll, "{code}");
    }
    let m = s.model();
    assert!(m
        .elements()
        .iter()
        .all(|(_, e)| m.storey(e.storey).is_some()));
    assert!(m.check().is_empty(), "{:?}", m.check());
}

/// A112 (K4, AW-49 mit Luftschicht): Verblender 11,6687, Kerndämmung
/// 13,6058, Porenbeton netto 15,1157 m³ je Geschoss. Die Luftschicht zählt zur
/// Dicke, hat aber kein Volumen und keine Zeile; 5,9681 m³ (Luft als
/// Körper) taucht nirgends auf, auch nicht in der CSV. Decke an der
/// Kernaußenseite 69,0569 m² / 15,1925 m³; IW-17,5 3,2371 m³; SP und FS
/// unverändert.
#[test]
fn a112_mehrschalig_mit_luftschicht() {
    let mut s = Scene::with_model(Model::with_seed(112));
    let (aw, iw) = haus_b11(&mut s);
    auf_typ(&mut s, iw, "IW-17,5");
    auf_typ(&mut s, aw, "AW-49");
    let og = og_zug(&s, aw);
    for run in [aw, og] {
        assert_eq!(baustoff_m3(&s, run, "Verblender (Vormauerziegel)"), 11.6687);
        assert_eq!(baustoff_m3(&s, run, "Kerndämmung (Mineralwolle)"), 13.6058);
        assert_eq!(baustoff_m3(&s, run, "Porenbeton"), 15.1157);
        assert_eq!(baustoff_m3(&s, run, "Luft"), 0.0, "Luft ohne Volumen");
        // Summe der gerundeten Einzelwerte: ±1 in der vierten Stelle
        assert!(
            (wand_m3(&s, run) - (11.6687 + 13.6058 + 15.1157)).abs() < 2e-4,
            "Summe ohne Luft: {}",
            wand_m3(&s, run)
        );
        let d = decke_mengen(&s, run);
        assert_eq!((d.0, d.1), (69.0569, 15.1925));
    }
    assert_eq!(
        s.wall_qto(s.model().wall_at(aw, 0).unwrap()).unwrap().width,
        490.0
    );
    assert_eq!(r4(wall_m3(&s, iw, 0)), 3.2371);
    assert_eq!((sp_m3(&s, aw), fs_m3(&s, aw)), (17.6, 7.0238));
    let liste = s.schedule().clone();
    let csv = crate::schedule_view::csv(s.model(), &liste);
    let text = String::from_utf8_lossy(&csv);
    assert!(!text.contains("5,968"), "Luft als Körper in der CSV");
    assert!(
        !text
            .lines()
            .any(|z| z.starts_with("Luft") || z.contains(";Luft;")),
        "keine Zeile Luft"
    );
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A113 (K4, Frostschürze unabhängig vom Wandtyp, Jörn 17:34): Typwechsel
/// AW-36 ↔ AW-49 lässt Breite 35 cm und FS 7,0238 m³. Das Feld „Breite“
/// nimmt 30 und 45 cm, lehnt 29,5 und 45,5 ab; set_footing_width ebenso.
/// Neue Schürzen 350 mm. Eine alte Datei mit 60 cm lädt mit 60 cm ohne
/// Befund in check().
#[test]
fn a113_schuerze_30_bis_45_unabhaengig_vom_typ() {
    use crate::ui::Field;
    let mut s = Scene::with_model(Model::with_seed(113));
    let (aw, _iw) = haus_b11(&mut s);
    let breite = |s: &Scene| s.foundation_qto(aw).unwrap().1.width;
    assert_eq!(breite(&s), 350.0, "neue Schürze 35 cm");
    for code in ["AW-36", "AW-49", "AW-36", "AW-31,5"] {
        auf_typ(&mut s, aw, code);
        assert_eq!((breite(&s), fs_m3(&s, aw)), (350.0, 7.0238), "{code}");
    }
    let (_, footing) = sohlplatte(&s, aw);
    let mut ui = Ui::new(1.0, &Theme::dark());
    ui.fit(1.0, 1440, 900);
    for (eingabe, soll) in [
        ("29,5", 350.0),
        ("45,5", 350.0),
        ("30", 300.0),
        ("45", 450.0),
    ] {
        let out = tippe(&mut ui, &mut s, footing, Field::FootingWidth, eingabe);
        if out.submit.is_none() {
            assert!(
                ui.edit.as_ref().is_some_and(|e| e.error.is_some()),
                "Hinweis bei {eingabe}"
            );
            ui.key(Key::Escape, true, M).unwrap();
        }
        assert_eq!(breite(&s), soll, "Eingabe {eingabe}");
    }
    let mut m = s.model().clone();
    m.allow_unstepped();
    assert!(!m.set_footing_width(footing, 299.0));
    assert!(!m.set_footing_width(footing, 451.0));
    assert!(m.set_footing_width(footing, 350.0));
    // Alte Datei mit 60 cm
    s.edit_model("Breite", |m| m.set_footing_width(footing, 350.0));
    let text = sk_model::szo::write(s.model());
    let zeilen: Vec<String> = text
        .lines()
        .map(|l| {
            if l.starts_with("[footing]") {
                assert!(l.contains("w=350"), "{l}");
                l.replacen("w=350", "w=600", 1)
            } else {
                l.to_string()
            }
        })
        .collect();
    let alt = zeilen.join("\n") + "\n";
    let geladen = sk_model::szo::read(&alt, GuidGen::with_seed(1)).expect("lädt");
    assert!(
        geladen.model.check().is_empty(),
        "{:?}",
        geladen.model.check()
    );
    let s2 = Scene::with_model(geladen.model);
    let aw2 = s2
        .model()
        .runs()
        .ids()
        .find(|r| s2.foundation_qto(*r).is_some())
        .unwrap();
    assert_eq!(
        s2.foundation_qto(aw2).unwrap().1.width,
        600.0,
        "Wert bleibt"
    );
}

/// A114 (K4, Prüfregel 20): Luftschicht außen oder innen am Rand, als Kern
/// oder zweimal hintereinander wird abgelehnt oder von check() gemeldet.
#[test]
fn a114_pruefregel_20_luftschicht() {
    let mut s = Scene::with_model(Model::with_seed(114));
    haus_b11(&mut s);
    let m0 = s.model().clone();
    let aw49 = typ(&m0, "AW-49");
    let gut = m0.layer_set(aw49).unwrap().clone();
    let luft = gut.layers[1];
    let mut faelle: Vec<(&str, LayerSet)> = Vec::new();
    let mut t = gut.clone();
    t.layers.swap(0, 1);
    faelle.push(("Luft außen", t));
    let mut t = gut.clone();
    t.layers.push(luft);
    faelle.push(("Luft innen", t));
    let mut t = gut.clone();
    t.layers[1].core = true;
    faelle.push(("Luft als Kern", t));
    let mut t = gut.clone();
    t.layers.insert(1, luft);
    faelle.push(("zwei Luftschichten", t));
    for (name, t) in faelle {
        let mut m = m0.clone();
        m.allow_unstepped();
        let abgelehnt = !m.set_layer_set(aw49, t);
        assert!(
            abgelehnt || !m.check().is_empty(),
            "{name} schlägt nicht an"
        );
    }
    let mut m = m0.clone();
    m.allow_unstepped();
    assert!(m.set_layer_set(aw49, gut));
    assert!(m.check().is_empty());
}

/// A115 (K4, Firmenkatalog): Ein Firmenkatalog aus K1/K2 (nur AW-31,5,
/// IW-17,5, Standard AW-31,5) bekommt beim Laden die fehlenden Werkstypen
/// (nach Guid), mit Hinweis; vorhandene Typen werden nicht überschrieben;
/// der Standard wechselt einmalig auf AW-36. Beim zweiten Laden kein Hinweis
/// mehr, ein neues Projekt hat AW-36 / IW-17,5.
#[test]
fn a115_firmenkatalog_bekommt_werkstypen() {
    use sk_model::catalog::{export_type, write_szk, Library};
    let d = test_dir("a115");
    let p = d.join("firmenkatalog.szk");
    // Alter Katalog: AW-31,5 (mit geändertem Namen) und IW-17,5
    let mut alt = Model::with_seed(115);
    alt.allow_unstepped();
    let (a, i) = (typ(&alt, "AW-31,5"), typ(&alt, "IW-17,5"));
    let mut t = alt.layer_set(a).unwrap().clone();
    t.name = "AW 31,5 Büro".into();
    assert!(alt.set_layer_set(a, t));
    let mut lib = Library::default();
    for id in [a, i] {
        assert!(export_type(&alt, &mut lib, alt.layer_set(id).unwrap().guid));
    }
    lib.default_exterior = lib.type_by_guid(alt.layer_set(a).unwrap().guid);
    lib.default_interior = lib.type_by_guid(alt.layer_set(i).unwrap().guid);
    let vorher = write_szk(&lib);
    assert!(!vorher.contains("AW-36"));
    std::fs::write(&p, &vorher).unwrap();
    let (c, h) = firma_laden(&p, true);
    assert!(!h.is_empty(), "Hinweis: Werkstypen ergänzt");
    let neu = Model::from_library(c.library());
    let codes: Vec<String> = neu
        .layer_sets()
        .iter()
        .map(|(_, t)| t.code.clone())
        .collect();
    for code in ["AW-31,5", "AW-36", "AW-49", "IW-11,5", "IW-17,5", "IW-24"] {
        assert!(codes.iter().any(|c| c == code), "{code} fehlt: {codes:?}");
    }
    assert_eq!(
        neu.layer_set(typ(&neu, "AW-31,5")).unwrap().name,
        "AW 31,5 Büro",
        "nicht überschrieben"
    );
    assert_eq!(
        neu.layer_set(neu.defaults().exterior_wall).unwrap().code,
        "AW-36"
    );
    assert_eq!(
        neu.layer_set(neu.defaults().interior_wall).unwrap().code,
        "IW-17,5"
    );
    // Zweites Laden: nichts mehr zu ergänzen
    let (_, h2) = firma_laden(&p, true);
    assert!(h2.is_empty(), "{h2:?}");
}

// ===== Tests K5 =====

/// Prüfhaus mit AW-36,5 (monolithisch, Auflager 24 cm, Randdämmstreifen).
fn haus_k5(seed: u64) -> (Scene, RunId, RunId) {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let (aw, iw) = haus_b11(&mut s);
    auf_typ(&mut s, iw, "IW-17,5");
    auf_typ(&mut s, aw, "AW-36,5");
    (s, aw, iw)
}

/// A116 (K5, Mengen): AW-36,5 = Porenbeton 36,5, Auflager 24 cm mit Streifen
/// aus „Randdämmung“, U 0,24. Je Geschoss: Porenbeton netto 33,2197 m³ (wie
/// bei voller Tasche, kein doppelter Abzug); Decke 9,75 × 7,75 = 75,5625 m²,
/// 16,62375 m³; 4 Streifen (EG RD-001…004, OG RD-005…008), Achsen 9,875 /
/// 7,875 m (zusammen 35,50), Volumen 0,27156 / 0,21656 (zusammen 0,97625);
/// Decke + Streifen = 17,600 m³. SP 17,600, FS 7,0238, IW-17,5 3,3524 m³.
/// Streifen gehören zum Geschoss ihrer Wand (A-09), Kostengruppe 330.
#[test]
fn a116_monolithisch_mit_randdaemmstreifen() {
    let (mut s, aw, iw) = haus_k5(116);
    let m = s.model();
    let t = m.layer_set(typ(m, "AW-36,5")).unwrap();
    assert_eq!(t.thickness(), 365.0);
    let (tiefe, mat) = auflager(t).expect("Auflager mit Streifen");
    assert_eq!(tiefe, 240.0);
    let rd = m.material(mat).unwrap();
    assert_eq!(
        (rd.name.as_str(), rd.category, rd.lambda),
        ("Randdämmung", MatCategory::Insulation, Some(0.035))
    );
    assert_eq!(
        u_wert(m, typ(m, "AW-36,5")).map(|u| (u * 100.0).round() / 100.0),
        Some(0.24)
    );
    let og = og_zug(&s, aw);
    let st = streifen(&s);
    assert_eq!(
        st.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
        ["RD-001", "RD-002", "RD-003", "RD-004", "RD-005", "RD-006", "RD-007", "RD-008"]
    );
    for run in [aw, og] {
        assert_eq!(baustoff_m3(&s, run, "Porenbeton"), 33.2197);
        let (f, v, _) = decke_mengen(&s, run);
        assert_eq!(f, 75.5625);
        assert!((v - 16.62375).abs() < 1e-3, "Decke {v}");
        // Streifen dieses Zugs
        let walls: Vec<ElementId> = (0..4).map(|i| m.wall_at(run, i).unwrap()).collect();
        let mut laengen = Vec::new();
        let mut vol = 0.0;
        for (_, el) in &st {
            let (w, fl) = streifen_von(m, *el).expect("Verweise");
            if !walls.contains(&w) {
                continue;
            }
            assert_eq!(Some(fl), decke(&s, run), "Decke des Zugs");
            assert_eq!(
                m.element(*el).unwrap().storey,
                m.element(w).unwrap().storey,
                "A-09"
            );
            let (l, v) = streifen_qto(&s, *el);
            laengen.push((l / 1e3 * 1e3).round() / 1e3);
            vol += v / 1e9;
        }
        laengen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(laengen, [7.875, 7.875, 9.875, 9.875]);
        assert!((vol - 0.97625).abs() < 1e-4, "Streifen {vol}");
        assert!(
            (v + vol - 17.6).abs() < 1e-3,
            "Decke + Streifen = volle Tasche"
        );
    }
    assert_eq!((sp_m3(&s, aw), fs_m3(&s, aw)), (17.6, 7.0238));
    assert_eq!(r4(wall_m3(&s, iw, 0)), 3.3524);
    assert!(m.check().is_empty(), "{:?}", m.check());
    // Mengenliste: Gruppe Randdämmstreifen, ohne zusätzlichen Porenbeton-Abzug
    let liste = s.schedule().clone();
    let csv = crate::schedule_view::csv(s.model(), &liste);
    let text = String::from_utf8_lossy(&csv);
    assert!(text.contains("Randdämmstreifen"), "Gruppe in der Liste");
    assert!(text.contains("RD-001") && text.contains("RD-008"));
}

/// A117 (K5, Typwechsel und Gummiband): AW-36 → AW-36,5 legt die 8 Streifen
/// in einem Rückgängig-Schritt an, zurück auf AW-36 entfernt sie;
/// Rückgängig bringt sie mit denselben Guids und Nummern. Gummiband an einer
/// Wand: dieselben Streifen (Guid), nur Länge und Volumen ändern sich.
#[test]
fn a117_streifen_folgen_typ_und_gummiband() {
    let mut s = Scene::with_model(Model::with_seed(117));
    let (aw, _iw) = haus_b11(&mut s);
    auf_typ(&mut s, aw, "AW-36");
    assert!(streifen(&s).is_empty());
    let k5 = typ(s.model(), "AW-36,5");
    assert!(zugtyp(&mut s, aw, k5));
    assert_eq!(s.undo_label(), Some("Wandtyp geändert"));
    let guids = |s: &Scene| -> Vec<(String, sk_model::Guid)> {
        streifen(s)
            .into_iter()
            .map(|(n, id)| (n, s.model().element(id).unwrap().guid))
            .collect()
    };
    let mit = guids(&s);
    assert_eq!(mit.len(), 8);
    assert!(s.undo());
    assert!(streifen(&s).is_empty(), "ein Schritt");
    assert!(s.redo());
    assert_eq!(guids(&s), mit, "gleiche Guids und Nummern");
    let k4 = typ(s.model(), "AW-36");
    assert!(zugtyp(&mut s, aw, k4));
    assert!(streifen(&s).is_empty(), "zurück auf AW-36");
    assert!(s.undo());
    assert_eq!(guids(&s), mit);
    // Gummiband an der Wand y = 8 m: +1 m
    let laenge = |s: &Scene| -> f64 {
        streifen(s)
            .iter()
            .map(|(_, el)| streifen_qto(s, *el).0)
            .sum::<f64>()
    };
    let vorher = laenge(&s);
    ziehen_am_fuss(&mut s, 0.0, 1000.0);
    assert_eq!(guids(&s), mit, "dieselben Bauteile");
    assert!(laenge(&s) > vorher, "länger");
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

/// A118 (K5, Datei und Prüfregeln 21–22): Rundlauf stabil; Auflager tiefer
/// als die Wand oder 0 wird abgelehnt oder gemeldet (21); ein Streifen, der
/// auf eine fremde Decke zeigt, oder ein doppelter Streifen schlägt an (22).
#[test]
fn a118_datei_und_pruefregeln_21_22() {
    let (s, aw, _) = haus_k5(118);
    let text = sk_model::szo::write(s.model());
    let m2 = sk_model::szo::read(&text, GuidGen::with_seed(1))
        .unwrap()
        .model;
    assert_eq!(sk_model::szo::write(&m2), text, "Rundlauf");
    assert!(pruefung(&text).is_empty(), "{:?}", pruefung(&text));
    // Regel 21
    let id = typ(s.model(), "AW-36,5");
    let t = s.model().layer_set(id).unwrap().clone();
    let (_, mat) = auflager(&t).unwrap();
    for tiefe in [365.0, 400.0, 0.0] {
        let mut m = s.model().clone();
        m.allow_unstepped();
        let mut t2 = t.clone();
        setze_auflager(&mut t2, Some((tiefe, mat)));
        let abgelehnt = !m.set_layer_set(id, t2);
        assert!(abgelehnt || !m.check().is_empty(), "Auflager {tiefe}");
    }
    // Regel 22: Streifen zeigt auf die OG-Decke statt auf die EG-Decke
    let og = og_zug(&s, aw);
    let g = |el: ElementId| s.model().element(el).unwrap().guid.to_string();
    let (de_eg, de_og) = (g(decke(&s, aw).unwrap()), g(decke(&s, og).unwrap()));
    let erster = streifen(&s)[0].1;
    let sg = g(erster);
    let mut falsch = Vec::new();
    let mut doppelt = Vec::new();
    for l in text.lines() {
        if l.contains(&format!("guid={sg}")) {
            assert!(
                l.contains(&de_eg),
                "Streifen verweist auf die EG-Decke: {l}"
            );
            falsch.push(l.replacen(&de_eg, &de_og, 1));
            doppelt.push(l.to_string());
            doppelt.push(l.replacen(&sg, "0000000000000000000001", 1));
        } else {
            falsch.push(l.to_string());
            doppelt.push(l.to_string());
        }
    }
    for (name, t) in [("fremde Decke", falsch), ("doppelter Streifen", doppelt)] {
        let t = t.join("\n") + "\n";
        assert_ne!(t, text);
        assert!(!pruefung(&t).is_empty(), "{name} schlägt nicht an");
    }
}

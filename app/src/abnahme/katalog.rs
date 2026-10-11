use super::*;

// Abnahmetests Bauteilkatalog K1 (Typ im Datenmodell, .szo v4) und K2
// (Firmenkatalog .szk), vorbereitet gegen main 1f0763e.
// Paket: bim/paket-k1-k3-bauteilkatalog.md („Fertig, wenn“), Konzept
// bim/konzept-bauteilkatalog.md, Spezifikation test/abnahme-bauteilkatalog.md.
//
// Einbau: als `mod katalog { use super::*; … }` ans Ende von app/src/abnahme.rs.
// Die Datei `a102-haus-v3.szo` (Prüfhaus aus haus_b11, mit main 1f0763e
// gespeichert, Version 3) neben abnahme.rs als `abnahme_haus_v3.szo` legen.
// Nutzt aus abnahme.rs: haus_b11, mengen_b11, STANDARD, og_zug, schale,
// decke_hoehen, test_dir, m2.
//
// Angenommene Namen stehen NUR in den Adaptern unten. Weicht der Bau ab,
// bitte nur die Adapter anpassen, nicht die Tests.

use sk_model::catalog::{self as szk, Library, TypeState};
use sk_model::{
    Category, ElementId, Guid, GuidGen, LayerFunction, LayerSet, LayerSetId, MaterialLayer, Pen,
    PropSet, PropValue, TypeCategory,
};
use std::collections::BTreeMap;

const HAUS_V3: &str = include_str!("../abnahme_haus_v3.szo");
const AW1_GUID: &str = "33i8p9bQ580uop$jCXCU6X";
const IW1_GUID: &str = "2bI2Vt9ej8GAcbFzlgyhUb";

/// Mengen des Prüfhauses mit AW-31,5 auf 12 Dämmung + 24 Porenbeton (Paket,
/// nachgerechnet auf main 1f0763e über set_layer_set).
const K1_36: [f64; 6] = [12.1692, 21.5522, 3.357, 16.6623, 17.6, 7.0238];

// ===== Adapter K1 =====

fn kurz(t: &LayerSet) -> &str {
    &t.code
}
fn aussen(t: &LayerSet) -> bool {
    matches!(t.category, TypeCategory::ExteriorWall)
}
fn stand(t: &LayerSet) -> u32 {
    t.changed
}
fn typ_merkmale(t: &LayerSet) -> &PropSet {
    &t.props
}
fn typ_merkmale_mut(t: &mut LayerSet) -> &mut PropSet {
    &mut t.props
}

/// Neuer Typ (noch nicht im Modell).
fn neuer_typ(
    m: &mut Model,
    name: &str,
    code: &str,
    ist_aussen: bool,
    layers: Vec<MaterialLayer>,
) -> LayerSet {
    LayerSet {
        guid: m.new_guid(),
        name: name.into(),
        code: code.into(),
        category: if ist_aussen {
            TypeCategory::ExteriorWall
        } else {
            TypeCategory::InteriorWall
        },
        layers,
        props: PropSet::new(),
        note: String::new(),
        changed: 1,
        bearing: sk_model::Bearing::Core,
        height: None,
    }
}

/// `add_layer_set` prüft das Kurzzeichen; `None` bei doppelt oder leer.
fn typ_anlegen(m: &mut Model, t: LayerSet) -> Option<LayerSetId> {
    m.add_layer_set(t)
}
fn benutzer(m: &Model, id: LayerSetId) -> usize {
    m.type_users(id).len()
}
fn duplizieren(m: &mut Model, id: LayerSetId) -> LayerSetId {
    m.duplicate_type(id).expect("kopiert")
}
fn loeschen(m: &mut Model, id: LayerSetId) -> Result<(), usize> {
    m.remove_type(id)
}
fn als_standard(m: &mut Model, ist_aussen: bool, id: LayerSetId) {
    let cat = if ist_aussen {
        TypeCategory::ExteriorWall
    } else {
        TypeCategory::InteriorWall
    };
    m.set_default_type(cat, id);
}
/// Typ eines Wandzugs wechseln, ein Rückgängig-Schritt „Wandtyp geändert“.
fn zugtyp(s: &mut Scene, run: RunId, id: LayerSetId) -> bool {
    s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id))
}
fn merkmale(m: &Model, el: ElementId) -> PropSet {
    m.props_of(el)
}
/// Kopie des App-Modells, die sich ohne Rückgängig-Schritt ändern lässt.
fn kopie(m: &Model) -> Model {
    let mut m = m.clone();
    m.allow_unstepped();
    m
}

// ===== Adapter K2 =====

fn leere_bibliothek() -> Library {
    Library::default()
}
fn szk_schreiben(l: &Library) -> String {
    szk::write_szk(l)
}
fn szk_lesen(t: &str) -> Result<Library, String> {
    szk::read_szk(t).map_err(|e| e.to_string())
}
fn abgleich(m: &Model, l: &Library) -> BTreeMap<Guid, TypeState> {
    szk::compare(m, l).into_iter().collect()
}
/// Projekt ← Firma, ein Rückgängig-Schritt „Typ übernommen“.
fn uebernehmen(s: &mut Scene, l: &Library, g: Guid) -> Option<LayerSetId> {
    let mut r = None;
    s.edit_model("Typ übernommen", |m| {
        r = szk::import_type(m, l, g);
        r.is_some()
    });
    r
}
/// Projekt → Firma (nur die Bibliothek im Speicher).
fn zurueck(m: &Model, l: &mut Library, g: Guid) -> bool {
    szk::export_type(m, l, g)
}
/// Standardtyp der Bibliothek für neue Projekte (`[default]`).
fn bib_standard(l: &mut Library, ist_aussen: bool, g: Guid) {
    if ist_aussen {
        l.default_exterior = l.type_by_guid(g);
    } else {
        l.default_interior = l.type_by_guid(g);
    }
}
/// Neues Projekt aus dem Firmenkatalog (ohne Rückgängig).
fn neues_projekt(l: &Library) -> Model {
    Model::from_library(l)
}

// ===== Adapter Firmenkatalog als Datei (App) =====

/// Lädt den Firmenkatalog; `vorgabe`: Pfad ist der Vorgabeort
/// (%APPDATA%\Skizzeo\firmenkatalog.szk). Liefert Hinweise statt Fehler.
fn firma_laden(p: &std::path::Path, vorgabe: bool) -> (crate::catalog::Company, Vec<String>) {
    crate::catalog::Company::load(p, vorgabe)
}
fn firma_bibliothek(c: &crate::catalog::Company) -> &Library {
    c.library()
}
/// Zurückspeichern in die Datei. `true`: geschrieben; `false`: die Datei
/// wurde inzwischen von einem anderen geändert, die Rückfrage ist fällig.
fn firma_zurueck(c: &mut crate::catalog::Company, m: &Model, g: Guid) -> bool {
    matches!(c.save_type(m, g), crate::catalog::SaveResult::Saved)
}

// ===== Hilfen (keine Annahmen) =====

fn lesen(text: &str) -> Result<Model, String> {
    sk_model::szo::read(text, GuidGen::with_seed(1))
        .map(|l| l.model)
        .map_err(|e| e.to_string())
}

/// Alles, was beim Laden anschlägt: Ladefehler, Hinweise, Prüfregeln.
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

fn typ_nach_guid(m: &Model, g: Guid) -> Option<LayerSetId> {
    m.layer_sets()
        .iter()
        .find(|(_, t)| t.guid == g)
        .map(|(id, _)| id)
}

/// (Kurzzeichen, außen, Name, Stand, Merkmale leer), nach Kurzzeichen.
fn typen(m: &Model) -> Vec<(String, bool, String, u32, bool)> {
    let mut v: Vec<_> = m
        .layer_sets()
        .iter()
        .map(|(_, t)| {
            (
                kurz(t).to_string(),
                aussen(t),
                t.name.clone(),
                stand(t),
                typ_merkmale(t).is_empty(),
            )
        })
        .collect();
    v.sort();
    v
}

fn kurzzeichen(m: &Model) -> Vec<String> {
    typen(m).into_iter().map(|t| t.0).collect()
}

/// Außenwandzug im EG und Innenwandzug eines geladenen Prüfhauses.
fn zuege(s: &Scene) -> (RunId, RunId) {
    let m = s.model();
    let cat = |r: RunId| m.element(m.wall_at(r, 0).unwrap()).unwrap().category;
    let aw = m
        .runs()
        .ids()
        .find(|r| cat(*r) == Category::ExteriorWall && m.run_below(*r).is_none())
        .expect("EG-Außenwandzug");
    let iw = m
        .runs()
        .ids()
        .find(|r| cat(*r) == Category::InteriorWall)
        .expect("Innenwandzug");
    (aw, iw)
}

/// Typen aller Segmente eines Zugs.
fn zug_typen(s: &Scene, run: RunId) -> Vec<Option<LayerSetId>> {
    let m = s.model();
    (0..)
        .map_while(|i| m.wall_at(run, i))
        .map(|w| m.element(w).unwrap().layer_set)
        .collect()
}

/// Kopie des Typs mit neuen Dicken (Dämmung, Porenbeton) in mm.
fn umbau(m: &Model, id: LayerSetId, d0: f64, d1: f64) -> LayerSet {
    let mut t = m.layer_set(id).unwrap().clone();
    t.layers[0].thickness = d0;
    t.layers[1].thickness = d1;
    t
}

fn text(v: &str) -> PropValue {
    PropValue::Text(v.into())
}

/// Jedes Bauteil ist an ein vorhandenes Geschoss gebunden (A-09).
fn alle_gebunden(m: &Model) -> bool {
    m.elements()
        .iter()
        .all(|(_, e)| m.storey(e.storey).is_some())
}

// ===== Tests K1 =====

/// A102 (K1, .szo v4): Eine Datei der Version 3 lädt. Ihre Typen heißen
/// danach AW-31,5 (Außenwand) und IW-17,5 (Innenwand), Kurzzeichen aus
/// Kategorie und Dicke, alte Guids bleiben, Stand 1, ohne Merkmale,
/// Guids unverändert. Speichern schreibt Version 4 mit Kurzzeichen und
/// Kategorie; Laden und erneutes Speichern ergeben denselben Text. Die
/// Mengen bleiben, alle Bauteile sind an Geschosse gebunden.
#[test]
fn a102_alte_datei_wird_version_4() {
    assert_eq!(sk_model::szo::VERSION, 4);
    let m = lesen(HAUS_V3).expect("v3 lädt");
    assert_eq!(
        typen(&m),
        [
            (
                "AW-31,5".into(),
                true,
                "AW 31,5 Gasbeton + WDVS".into(),
                1,
                true
            ),
            ("IW-17,5".into(), false, "IW 17,5 Gasbeton".into(), 1, true),
        ]
    );
    let d = m.defaults();
    assert_eq!(
        m.layer_set(d.exterior_wall).unwrap().guid.to_string(),
        AW1_GUID
    );
    assert_eq!(
        m.layer_set(d.interior_wall).unwrap().guid.to_string(),
        IW1_GUID
    );
    assert!(m.check().is_empty(), "{:?}", m.check());
    assert!(alle_gebunden(&m));
    let t = sk_model::szo::write(&m);
    assert!(t.starts_with("SZO 4\n"), "{}", &t[..20]);
    assert!(t.contains(r#"code="AW-31,5""#) && t.contains(r#"code="IW-17,5""#));
    assert!(t.contains("cat=exterior") && t.contains("cat=interior"));
    let m2 = lesen(&t).unwrap();
    assert_eq!(sk_model::szo::write(&m2), t, "Rundlauf stabil");
    let s = Scene::with_model(m);
    let (aw, iw) = zuege(&s);
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD, "Mengen unverändert");
}

/// A103 (K1, F2 „Typänderung wirkt auf alle Wände“): AW-31,5 über
/// set_layer_set auf 12 Dämmung + 24 Porenbeton. Alle acht Außenwände (EG
/// und OG) folgen, die Decke wird größer (Kernaußenseite 2 cm weiter
/// außen), Sohlplatte und Frostschürze bleiben (Außenseite steht).
/// Höhenbezug bleibt (A-09). Stand +1, Guid bleibt. Ein Rückgängig stellt
/// alles wieder her.
#[test]
fn a103_typaenderung_wirkt_auf_alle_waende() {
    let mut s = Scene::with_model(Model::with_seed(103));
    let (aw, iw) = haus_b11(&mut s);
    let og = og_zug(&s, aw);
    let id = s.model().defaults().exterior_wall;
    let (g, st) = {
        let t = s.model().layer_set(id).unwrap();
        (t.guid, stand(t))
    };
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
    let neu = umbau(s.model(), id, 120.0, 240.0);
    assert!(s.edit_model("Typ geändert", |m| m.set_layer_set(id, neu)));
    assert_eq!(mengen_b11(&s, aw, iw), K1_36);
    assert_eq!(m2(s.floor_qto(aw).unwrap().area), 75.7376);
    assert_eq!(schale(&s, og), (21.5522, 12.1692), "OG folgt");
    assert_eq!(decke_hoehen(&s, aw), (2635.0, 2855.0), "Höhenbezug bleibt");
    assert_eq!(m2(s.foundation_qto(aw).unwrap().0.area), 80.0);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    let t = s.model().layer_set(id).unwrap();
    assert_eq!((t.guid, stand(t)), (g, st + 1), "Guid bleibt, Stand +1");
    assert!(s.undo());
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
    assert_eq!(schale(&s, og), (15.7613, 14.1654));
    assert_eq!(stand(s.model().layer_set(id).unwrap()), st);
}

/// A104 (K1, set_run_type): Ein Zug hat genau einen Typ. Ein Außenwandzug
/// nimmt keinen Innenwandtyp an (abgelehnt, kein Rückgängig-Schritt).
/// Duplizieren gibt neue Guid, „(Kopie)“ und ein freies Kurzzeichen.
/// Wechsel auf die Kopie mit 12 + 24 liefert dieselben Mengen wie A103,
/// der gekoppelte OG-Zug wechselt mit, ein Rückgängig-Schritt.
#[test]
fn a104_typ_eines_wandzugs_wechseln() {
    let mut s = Scene::with_model(Model::with_seed(104));
    let (aw, iw) = haus_b11(&mut s);
    let og = og_zug(&s, aw);
    let aw1 = s.model().defaults().exterior_wall;
    let iw1 = s.model().defaults().interior_wall;
    // Abgelehnt: falsche Kategorie, kein leerer Schritt
    let label = s.undo_label();
    assert!(
        !zugtyp(&mut s, aw, iw1),
        "Außenwand nimmt keinen Innenwandtyp"
    );
    assert!(
        !zugtyp(&mut s, iw, aw1),
        "Innenwand nimmt keinen Außenwandtyp"
    );
    assert_eq!(s.undo_label(), label, "kein Rückgängig-Schritt");
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
    // Duplizieren
    let mut dup = None;
    s.edit_model("Typ dupliziert", |m| {
        dup = Some(duplizieren(m, aw1));
        true
    });
    let dup = dup.unwrap();
    {
        let (a, d) = (
            s.model().layer_set(aw1).unwrap(),
            s.model().layer_set(dup).unwrap(),
        );
        assert_eq!(d.name, "AW 31,5 Porenbeton + WDVS (Kopie)");
        assert_ne!(d.guid, a.guid);
        assert!(!kurz(d).is_empty() && kurz(d) != "AW-31,5", "{}", kurz(d));
        assert!(aussen(d));
        assert_eq!(d.layers, a.layers);
    }
    assert_eq!(benutzer(s.model(), dup), 0);
    let neu = umbau(s.model(), dup, 120.0, 240.0);
    assert!(s.edit_model("Typ geändert", |m| m.set_layer_set(dup, neu)));
    assert_eq!(
        mengen_b11(&s, aw, iw),
        STANDARD,
        "unbenutzter Typ ändert nichts"
    );
    // Wechsel
    assert!(zugtyp(&mut s, aw, dup));
    assert_eq!(s.undo_label(), Some("Wandtyp geändert"));
    assert_eq!(mengen_b11(&s, aw, iw), K1_36);
    assert!(zug_typen(&s, aw).iter().all(|t| *t == Some(dup)));
    assert!(
        zug_typen(&s, og).iter().all(|t| *t == Some(dup)),
        "OG wechselt mit"
    );
    assert_eq!(schale(&s, og), (21.5522, 12.1692));
    assert_eq!((benutzer(s.model(), aw1), benutzer(s.model(), dup)), (0, 8));
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    // Ein Schritt zurück
    assert!(s.undo());
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
    assert!(zug_typen(&s, aw).iter().all(|t| *t == Some(aw1)));
    assert!(zug_typen(&s, og).iter().all(|t| *t == Some(aw1)));
}

/// A105 (K1, Prüfregeln 16–19): Löschen nur ohne Benutzer und nicht für
/// Standardtypen (AW-31,5: Err(8), IW-17,5: Err(1)); Kurzzeichen eindeutig und
/// nicht leer; Guid bleibt. Gezielt kaputte Dateien schlagen an: doppeltes
/// oder leeres Kurzzeichen, Zug mit zwei Typen, Kategorie falsch, Schicht 0.
#[test]
fn a105_pruefregeln_16_bis_19() {
    let mut s = Scene::with_model(Model::with_seed(105));
    haus_b11(&mut s);
    let aw1 = s.model().defaults().exterior_wall;
    let iw1 = s.model().defaults().interior_wall;
    let aw1_layers = s.model().layer_set(aw1).unwrap().layers.clone();
    let mut m = kopie(s.model());
    // Regel 18
    assert_eq!(loeschen(&mut m, aw1), Err(8), "EG und OG je 4 Segmente");
    assert_eq!(loeschen(&mut m, iw1), Err(1));
    // Regel 17
    let t = neuer_typ(&mut m, "Test", "AW-31,5", true, aw1_layers.clone());
    assert!(typ_anlegen(&mut m, t).is_none(), "Kurzzeichen doppelt");
    let t = neuer_typ(&mut m, "Test", "", true, aw1_layers.clone());
    assert!(typ_anlegen(&mut m, t).is_none(), "Kurzzeichen leer");
    let t = neuer_typ(&mut m, "Test", "AW-9", true, aw1_layers.clone());
    let neu = typ_anlegen(&mut m, t).expect("frei");
    assert_eq!(loeschen(&mut m, neu), Ok(()), "unbenutzt");
    assert!(m.layer_set(neu).is_none());
    // Standardtyp ohne Benutzer bleibt
    let dup = duplizieren(&mut m, aw1);
    als_standard(&mut m, true, dup);
    assert!(loeschen(&mut m, dup).is_err(), "Standardtyp");
    als_standard(&mut m, true, aw1);
    assert_eq!(loeschen(&mut m, dup), Ok(()));
    // Regel 19: Guid bleibt bei Änderung und Rundlauf
    let g = m.layer_set(aw1).unwrap().guid;
    let t2 = umbau(&m, aw1, 120.0, 240.0);
    assert!(m.set_layer_set(aw1, t2));
    assert_eq!(m.layer_set(aw1).unwrap().guid, g);
    let m2 = lesen(&sk_model::szo::write(&m)).unwrap();
    assert!(typ_nach_guid(&m2, g).is_some());
    // Kaputte Dateien (Grundlage: sauberer Text mit einem zweiten AW-Typ)
    let mut m = kopie(s.model());
    let dup = duplizieren(&mut m, aw1);
    let (awg, iwg, dupg) = (
        m.layer_set(aw1).unwrap().guid.to_string(),
        m.layer_set(iw1).unwrap().guid.to_string(),
        m.layer_set(dup).unwrap().guid.to_string(),
    );
    let sauber = sk_model::szo::write(&m);
    assert!(pruefung(&sauber).is_empty(), "{:?}", pruefung(&sauber));
    let zeile = |text: &str, wo: &dyn Fn(&str) -> bool, alt: &str, neu: &str| -> String {
        let mut erst = true;
        let mut out: Vec<String> = Vec::new();
        for l in text.lines() {
            if erst && wo(l) && l.contains(alt) {
                erst = false;
                out.push(l.replacen(alt, neu, 1));
            } else {
                out.push(l.to_string());
            }
        }
        assert!(!erst, "Stelle für {alt} nicht gefunden");
        out.join("\n") + "\n"
    };
    let iw_satz = format!("guid={iwg}");
    let aw_satz = format!("set={awg}");
    let kaputt = [
        (
            "Kurzzeichen doppelt",
            sauber.replacen(r#"code="IW-17,5""#, r#"code="AW-31,5""#, 1),
        ),
        (
            "Kurzzeichen leer",
            sauber.replacen(r#"code="IW-17,5""#, r#"code="""#, 1),
        ),
        (
            "Kategorie falsch",
            zeile(
                &sauber,
                &|l| l.starts_with("[layerset]") && l.contains(&iw_satz),
                "cat=interior",
                "cat=exterior",
            ),
        ),
        (
            "Zug mit zwei Typen",
            zeile(
                &sauber,
                &|l| l.starts_with("[wall]"),
                &aw_satz,
                &format!("set={dupg}"),
            ),
        ),
        (
            "Schicht 0 dick",
            zeile(
                &sauber,
                &|l| l.starts_with("[layer]") && l.contains(&aw_satz),
                "t=140",
                "t=0",
            ),
        ),
    ];
    for (name, t) in kaputt {
        assert_ne!(t, sauber, "{name}: Text verändert");
        assert!(!pruefung(&t).is_empty(), "{name} schlägt nicht an");
    }
}

/// A106 (K1, Merkmale): Typmerkmale gelten für alle Wände des Typs; ein
/// Bauteilmerkmal mit gleichem Schlüssel überschreibt (IFC-Regel). Außenwand,
/// tragend und Dicke sind abgeleitet und nie gespeichert. Merkmale stehen als
/// [typeprop] in der .szo und überstehen den Rundlauf.
#[test]
fn a106_merkmale_typ_und_bauteil() {
    let mut s = Scene::with_model(Model::with_seed(106));
    let (aw, _iw) = haus_b11(&mut s);
    let aw1 = s.model().defaults().exterior_wall;
    let (w0, w1) = (
        s.model().wall_at(aw, 0).unwrap(),
        s.model().wall_at(aw, 1).unwrap(),
    );
    let mut t = s.model().layer_set(aw1).unwrap().clone();
    typ_merkmale_mut(&mut t).insert("Brandschutz".into(), text("F90"));
    typ_merkmale_mut(&mut t).insert("Schallschutz".into(), text("R'w 53 dB"));
    assert!(s.edit_model("Merkmale", |m| m.set_layer_set(aw1, t)));
    assert!(s.edit_model("Merkmal", |m| m.set_prop(
        w0,
        "Brandschutz",
        Some(text("F30"))
    )));
    let p0 = merkmale(s.model(), w0);
    let p1 = merkmale(s.model(), w1);
    assert_eq!(
        p0.get("Brandschutz"),
        Some(&text("F30")),
        "Bauteil überschreibt"
    );
    assert_eq!(
        p0.get("Schallschutz"),
        Some(&text("R'w 53 dB")),
        "Typ kommt durch"
    );
    assert_eq!(p1.get("Brandschutz"), Some(&text("F90")));
    let tp = typ_merkmale(s.model().layer_set(aw1).unwrap());
    for k in ["Dicke", "Tragend", "Außenwand", "IsExternal", "LoadBearing"] {
        assert!(!tp.contains_key(k), "{k} ist abgeleitet");
    }
    let t = sk_model::szo::write(s.model());
    assert!(t.contains("[typeprop]") && t.contains(r#"key="Brandschutz""#));
    let m2 = lesen(&t).unwrap();
    assert_eq!(sk_model::szo::write(&m2), t);
    let id2 = typ_nach_guid(&m2, s.model().layer_set(aw1).unwrap().guid).unwrap();
    assert_eq!(typ_merkmale(m2.layer_set(id2).unwrap()), tp);
    assert!(s.edit_model("Merkmal", |m| m.set_prop(w0, "Brandschutz", None)));
    assert_eq!(
        merkmale(s.model(), w0).get("Brandschutz"),
        Some(&text("F90")),
        "ohne Bauteilwert gilt der Typ"
    );
}

// ===== Tests K2 =====

/// A107 (K2, Abgleich): Ein im Firmenkatalog geänderter AW-31,5 (12 + 24)
/// gilt als „abweichend“, IW-17,5 als „nur Projekt“. Rundlauf
/// read_szk(write_szk(x)) == x, Kopf „SZK 1“, keine Geschosse oder Bauteile.
/// Übernehmen überschreibt den Projekttyp (gleiche Guid, kein neuer Typ),
/// die Mengen werden die aus A103, danach „gleich“, obwohl der
/// Änderungsstand verschieden ist. Ein Rückgängig-Schritt.
#[test]
fn a107_abweichenden_typ_uebernehmen() {
    let mut s = Scene::with_model(Model::with_seed(107));
    let (aw, iw) = haus_b11(&mut s);
    let aw1 = s.model().defaults().exterior_wall;
    let g = s.model().layer_set(aw1).unwrap().guid;
    let gi = s
        .model()
        .layer_set(s.model().defaults().interior_wall)
        .unwrap()
        .guid;
    // Firma: derselbe Typ, zweimal geändert (Stand 3)
    let mut b = lesen(&sk_model::szo::write(s.model())).unwrap();
    let bid = typ_nach_guid(&b, g).unwrap();
    let t1 = umbau(&b, bid, 130.0, 230.0);
    assert!(b.set_layer_set(bid, t1));
    let t2 = umbau(&b, bid, 120.0, 240.0);
    assert!(b.set_layer_set(bid, t2));
    let mut lib = leere_bibliothek();
    assert!(zurueck(&b, &mut lib, g));
    let t = szk_schreiben(&lib);
    assert!(t.starts_with("SZK 1\n"));
    for satz in ["[wall]", "[storey]", "[building]", "[project]"] {
        assert!(!t.contains(satz), "{satz} gehört nicht in die .szk");
    }
    assert_eq!(szk_lesen(&t).unwrap(), lib, "Rundlauf");
    let a = abgleich(s.model(), &lib);
    assert_eq!(a.get(&g), Some(&TypeState::Differs));
    assert_eq!(a.get(&gi), Some(&TypeState::OnlyProject));
    let n = s.model().layer_sets().len();
    assert_eq!(
        uebernehmen(&mut s, &lib, g),
        Some(aw1),
        "überschreibt den Projekttyp"
    );
    assert_eq!(s.undo_label(), Some("Typ übernommen"));
    assert_eq!(s.model().layer_sets().len(), n);
    assert_eq!(s.model().layer_set(aw1).unwrap().guid, g, "Regel 19");
    assert_eq!(mengen_b11(&s, aw, iw), K1_36);
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert_ne!(stand(s.model().layer_set(aw1).unwrap()), 3);
    assert_eq!(
        abgleich(s.model(), &lib).get(&g),
        Some(&TypeState::Same),
        "Stand zählt nicht"
    );
    assert!(s.undo());
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
    assert_eq!(abgleich(s.model(), &lib).get(&g), Some(&TypeState::Differs));
}

/// A108 (K2, neuer Typ): Ein Typ, den es nur im Firmenkatalog gibt („nur
/// Firma“), kommt mit derselben Guid ins Projekt, mit seinem neuen Baustoff
/// und dessen neuem Stift. Der Stift bekommt die nächste freie Nummer (12,
/// weil das Projekt schon den Werks-Stift 10 und einen eigenen Stift 11
/// hat); keine Nummer doppelt,
/// alte Nummern unverändert. Vorhandene Baustoffe bleiben die des Projekts.
/// Belegt ein anderer Projekttyp das Kurzzeichen, bekommt der übernommene
/// „AW-37-2“ (seit K4 gibt es den Werkstyp AW-36). Ein Rückgängig entfernt Typ, Baustoff und Stift.
#[test]
fn a108_neuen_typ_mit_baustoff_und_stift_uebernehmen() {
    let mut s = Scene::with_model(Model::with_seed(108));
    haus_b11(&mut s);
    let aw1 = s.model().defaults().exterior_wall;
    let aw1_layers = s.model().layer_set(aw1).unwrap().layers.clone();
    // Firma: neuer Stift, neuer Baustoff, neuer Typ AW-37
    let mut b = lesen(&sk_model::szo::write(s.model())).unwrap();
    let pen_g = b.new_guid();
    let pen = b.add_pen(Pen {
        guid: pen_g,
        number: 11,
        name: "Kalksandstein".into(),
        color: [200, 90, 60],
        width_mm: 0.25,
    });
    let mat = |m: &Model, name: &str| {
        m.materials()
            .iter()
            .find(|(_, x)| x.name == name)
            .map(|(id, _)| id)
            .unwrap()
    };
    let mut ks = b.material(mat(&b, "Porenbeton")).unwrap().clone();
    ks.guid = b.new_guid();
    ks.name = "Kalksandstein".into();
    ks.density = 1800.0;
    ks.cut_fg = pen;
    let ks_g = ks.guid;
    let ks_id = b.add_material(ks);
    let daemm = mat(&b, "Dämmung (WDVS)");
    let schicht = |material, thickness, function, core| {
        let l = MaterialLayer::new(material, thickness, function);
        if core {
            l.core()
        } else {
            l
        }
    };
    let t = neuer_typ(
        &mut b,
        "AW 36 KS + WDVS",
        "AW-37",
        true,
        vec![
            schicht(daemm, 160.0, LayerFunction::Insulation, false),
            schicht(ks_id, 200.0, LayerFunction::Structure, true),
        ],
    );
    let tg = t.guid;
    typ_anlegen(&mut b, t).unwrap();
    let mut lib = leere_bibliothek();
    assert!(zurueck(&b, &mut lib, tg));
    assert_eq!(
        abgleich(s.model(), &lib).get(&tg),
        Some(&TypeState::OnlyCompany)
    );
    // Projekt: eigener Stift 11 (10 ist „Ansichtsmuster“), eigener Typ mit Kurzzeichen AW-37
    s.edit_model("Stift", |m| {
        let guid = m.new_guid();
        m.add_pen(Pen {
            guid,
            number: 11,
            name: "Eigener".into(),
            color: [0, 0, 0],
            width_mm: 0.18,
        });
        true
    });
    s.edit_model("Typ", |m| {
        let t = neuer_typ(m, "AW Test", "AW-37", true, aw1_layers.clone());
        typ_anlegen(m, t).is_some()
    });
    let stifte = |s: &Scene| -> BTreeMap<Guid, u16> {
        s.model()
            .attr()
            .pens()
            .iter()
            .map(|(_, p)| (p.guid, p.number))
            .collect()
    };
    let vorher = stifte(&s);
    let daemm_vorher = s
        .model()
        .material(mat(s.model(), "Dämmung (WDVS)"))
        .unwrap()
        .clone();
    let (n_typen, n_mat) = (s.model().layer_sets().len(), s.model().materials().len());
    // Übernehmen
    let id = uebernehmen(&mut s, &lib, tg).expect("übernommen");
    let t = s.model().layer_set(id).unwrap().clone();
    assert_eq!(t.guid, tg, "gleiche Guid");
    assert_eq!(kurz(&t), "AW-37-2", "Kurzzeichen belegt");
    assert_eq!(t.name, "AW 36 KS + WDVS");
    assert_eq!(
        t.layers.iter().map(|l| l.thickness).collect::<Vec<_>>(),
        [160.0, 200.0]
    );
    let (ks_proj, ks_mat) = s
        .model()
        .materials()
        .iter()
        .find(|(_, x)| x.guid == ks_g)
        .map(|(id, x)| (id, x.clone()))
        .expect("Baustoff reist mit");
    assert_eq!(t.layers[1].material, ks_proj);
    let p = s
        .model()
        .attr()
        .pen(ks_mat.cut_fg)
        .expect("Stift reist mit");
    assert_eq!((p.guid, p.number), (pen_g, 12), "nächste freie Nummer");
    let nachher = stifte(&s);
    for (g, n) in &vorher {
        assert_eq!(nachher.get(g), Some(n), "alte Stiftnummern bleiben");
    }
    let mut nummern: Vec<u16> = nachher.values().copied().collect();
    let anzahl = nummern.len();
    nummern.sort();
    nummern.dedup();
    assert_eq!(nummern.len(), anzahl, "keine Stiftnummer doppelt");
    assert_eq!(
        s.model()
            .material(mat(s.model(), "Dämmung (WDVS)"))
            .unwrap(),
        &daemm_vorher,
        "Projektbaustoff bleibt"
    );
    assert_eq!(
        s.model().materials().len(),
        n_mat + 1,
        "nur der fehlende Baustoff"
    );
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    assert!(s.undo());
    assert_eq!(s.model().layer_sets().len(), n_typen);
    assert_eq!(s.model().materials().len(), n_mat);
    assert_eq!(stifte(&s), vorher);
}

/// A109 (K2, Firmenkatalog als Datei): Fehlt die Datei am Vorgabeort, wird
/// sie mit dem Startbestand angelegt; an einem fremden Pfad wird nichts
/// angelegt, nur ein Hinweis. Eine kaputte .szk gibt einen Hinweis mit
/// Zeile, Skizzeo arbeitet mit dem Startbestand, die Datei bleibt unberührt.
/// Hat ein anderer die Datei seit dem Laden geändert, schreibt
/// Zurückspeichern nicht still darüber (Rückfrage). Sonst wird atomar
/// geschrieben. Ein neues Projekt bekommt Typen und Standardtypen des
/// Firmenkatalogs; Model::with_seed bleibt beim Startbestand.
#[test]
fn a109_firmenkatalog_datei_und_neues_projekt() {
    // Startbestand seit K4: sieben Werkstypen (K5)
    let start = [
        "AW-31,5", "AW-36", "AW-36,5", "AW-49", "IW-11,5", "IW-17,5", "IW-24",
    ]
    .map(String::from);
    let d = test_dir("a109");
    // Erster Start am Vorgabeort
    let vorgabe = d.join("firmenkatalog.szk");
    let (c, h) = firma_laden(&vorgabe, true);
    assert!(vorgabe.exists(), "angelegt");
    assert!(h.is_empty(), "{h:?}");
    assert_eq!(kurzzeichen(&neues_projekt(firma_bibliothek(&c))), start);
    // Fremder Pfad
    let fremd = d.join("netz").join("firma.szk");
    let (c, h) = firma_laden(&fremd, false);
    assert!(!fremd.exists(), "nichts angelegt");
    assert!(!h.is_empty(), "Hinweis");
    assert_eq!(kurzzeichen(&neues_projekt(firma_bibliothek(&c))), start);
    // Kaputte Datei
    let kaputt = d.join("kaputt.szk");
    let inhalt = "SZK 1\n[layerset] guid=@@@ name=\"X\" code=\"AW-31,5\" cat=exterior changed=1 note=\"\"\n[layer] set=@@@ mat=nix t=abc fn=insulation core=0\n";
    std::fs::write(&kaputt, inhalt).unwrap();
    let (c, h) = firma_laden(&kaputt, false);
    assert!(h.iter().any(|x| x.contains("Zeile")), "{h:?}");
    assert_eq!(kurzzeichen(&neues_projekt(firma_bibliothek(&c))), start);
    assert_eq!(
        std::fs::read_to_string(&kaputt).unwrap(),
        inhalt,
        "unberührt"
    );
    // Zurückspeichern
    let mut s = Scene::with_model(Model::with_seed(109));
    haus_b11(&mut s);
    let aw1 = s.model().defaults().exterior_wall;
    let g = s.model().layer_set(aw1).unwrap().guid;
    let neu = umbau(s.model(), aw1, 120.0, 240.0);
    assert!(s.edit_model("Typ geändert", |m| m.set_layer_set(aw1, neu)));
    let mut c = firma_laden(&vorgabe, true).0;
    let mut anderer = leere_bibliothek();
    assert!(zurueck(
        s.model(),
        &mut anderer,
        s.model()
            .layer_set(s.model().defaults().interior_wall)
            .unwrap()
            .guid
    ));
    std::fs::write(&vorgabe, szk_schreiben(&anderer)).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&vorgabe)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10))
        .unwrap();
    let fremdtext = std::fs::read_to_string(&vorgabe).unwrap();
    assert!(
        !firma_zurueck(&mut c, s.model(), g),
        "Rückfrage statt Überschreiben"
    );
    assert_eq!(std::fs::read_to_string(&vorgabe).unwrap(), fremdtext);
    let mut c = firma_laden(&vorgabe, true).0;
    assert!(
        firma_zurueck(&mut c, s.model(), g),
        "ohne fremde Änderung geschrieben"
    );
    let lib = szk_lesen(&std::fs::read_to_string(&vorgabe).unwrap()).unwrap();
    assert_eq!(abgleich(s.model(), &lib).get(&g), Some(&TypeState::Same));
    let reste: Vec<_> = std::fs::read_dir(&d)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("tmp") || n.ends_with('~'))
        .collect();
    assert!(reste.is_empty(), "atomar, keine Reste: {reste:?}");
    // Neues Projekt
    let mut b = lesen(&sk_model::szo::write(s.model())).unwrap();
    let bid = typ_nach_guid(&b, g).unwrap();
    let dup = duplizieren(&mut b, bid);
    let dg = b.layer_set(dup).unwrap().guid;
    let gi = b.layer_set(b.defaults().interior_wall).unwrap().guid;
    let mut lib = leere_bibliothek();
    assert!(zurueck(&b, &mut lib, dg));
    assert!(zurueck(&b, &mut lib, gi));
    bib_standard(&mut lib, true, dg);
    bib_standard(&mut lib, false, gi);
    let p = neues_projekt(&lib);
    let a = abgleich(&p, &lib);
    assert_eq!(a.len(), 2);
    assert!(a.values().all(|x| *x == TypeState::Same), "{a:?}");
    assert_eq!(p.layer_set(p.defaults().exterior_wall).unwrap().guid, dg);
    assert_eq!(p.layer_set(p.defaults().interior_wall).unwrap().guid, gi);
    assert!(p.check().is_empty(), "{:?}", p.check());
    assert_eq!(
        kurzzeichen(&Model::with_seed(1)),
        start,
        "Tests ohne Firmenkatalog"
    );
    // Nachtrag 16:55: Werkstypen mit festen Guids, in jedem Projekt gleich
    let werk = |m: &Model| {
        let d = m.defaults();
        (
            m.layer_set(d.exterior_wall).unwrap().guid,
            m.layer_set(d.interior_wall).unwrap().guid,
        )
    };
    assert_eq!(werk(&Model::with_seed(1)), werk(&Model::with_seed(2)));
    let neu = d.join("neu");
    std::fs::create_dir_all(&neu).unwrap();
    let c = firma_laden(&neu.join("firmenkatalog.szk"), true).0;
    // Seit K4 hat ein neues Projekt (Model::new) den Standard AW-36
    assert_eq!(
        werk(&Model::new()),
        werk(&neues_projekt(firma_bibliothek(&c))),
        "Startbestand des Firmenkatalogs = Werkstypen"
    );
}

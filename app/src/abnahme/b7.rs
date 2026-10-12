use super::*;

// Abnahmetests B7 „Mengenermittlung im zweiten Fenster“, Datenseite und
// Kopplung (bim/paket-b7-mengenliste.md, Jörn 06.10. 14:00), vorbereitet gegen
// main 89d28ac. Spezifikation: test/abnahme-mengenliste.md. Setzt F2
// (a96-a100-fensterschicht.rs: `Picking`) voraus.
//
// Die Daten hinter der Liste: Gliederung, Sollwerte, Taschen, Summen nach
// Baustoff, Bauteile ohne Körper, Rechenzeitpunkt mit „wird aktualisiert“,
// CSV; dazu A101 die Kopplung Liste ↔ Modell über den gemeinsamen Zustand.
// Aussehen des Mengenfensters (Zeilen, Bänder, Kacheln, Rollen,
// Aufleuchten) folgt nach Jörns Abnahme der Skizzen als Bildvergleich und
// Handtest.
//
// Die unterste Gruppe heißt „Fundament“ (Koordinator 13:37), Kurzname
// des Geschosses weiter „GR“.
//
// Prüfhaus: Gebäude aus dem Dialog (2 Geschosse, Standardhöhen Jörn 10:13),
// Rechteck 10 × 8 m, AW 31,5, EG-Innenwand IW 17,5 bei x = 5 m. Alle Sollwerte
// von Hand nachgerechnet (abnahme-mengenliste.md).
//
// Angenommene Namen stehen nur in den Adaptern. Neu angenommen: das Paket-API
// `sk_model::qto::{Schedule, ElementQto}` mit den Feldern aus dem Paket,
// `WallQto::pocket`, `WallQto::list_length`, `LayerQto::side_area`, `FloorQto::bearing`, für Bauteile
// ohne Körper `RowQto::q` als `Option` mit `RowQto::note`; in der App
// `Scene::schedule` (je Revision gemerkt), `Scene::schedule_runs` (Zähler der
// Berechnungen), `Scene::schedule_stale` („wird aktualisiert“) und im
// Mengenfenster `crate::schedule_view::{ListView, csv}`.

// ===== Adapter B7 =====

use sk_model::qto::{ElementQto, FloorQto, LayerQto, Schedule, WallQto};
use sk_model::Category;

/// Die Liste, wie das Mengenfenster sie zeigt (je Revision gemerkt).
fn liste(s: &mut Scene) -> Schedule {
    s.schedule().clone()
}

/// Steht oben im Mengenfenster „wird aktualisiert“ (Liste älter als das
/// Modell, z. B. beim Ziehen)?
fn veraltet(s: &Scene) -> bool {
    s.schedule_stale()
}

/// Wie oft die Liste bisher berechnet wurde.
fn berechnungen(s: &Scene) -> u64 {
    s.schedule_runs()
}

/// Inhalt der .csv aus „Als Tabelle speichern“ (Bytes, wie geschrieben).
fn csv(s: &mut Scene) -> Vec<u8> {
    let l = liste(s);
    crate::schedule_view::csv(s.model(), &l)
}

/// Volumen, das die Decke aus einer Wand bzw. Schicht nimmt (mm³).
fn tasche(w: &WallQto) -> f64 {
    w.pocket
}

/// Außenfläche einer Schicht (mm²), für die WDVS-m².
fn schicht_flaeche(l: &LayerQto) -> f64 {
    l.side_area
}

/// Länge in der Liste (mm): Außenwand Außenseite (side_outer / height),
/// Innenwand lichte Länge der Kernschicht (BIM 13:38).
fn listen_laenge(w: &WallQto) -> f64 {
    w.list_length
}

/// „davon Auflager in den Außenwänden“ einer Decke (mm³).
fn auflager(f: &FloorQto) -> f64 {
    f.bearing
}

/// Eine Zeile der Liste, flach: Geschoss der Gruppe (Kurzname, „GR“ für die
/// Gruppe Fundament), Bauteilart, Nummer, Mengen (`None` = kein Körper) und
/// der Grund in grauer Schrift.
struct Zeile {
    geschoss: String,
    art: Category,
    nummer: String,
    q: Option<ElementQto>,
    grund: Option<String>,
}

fn zeilen(m: &Model, l: &Schedule) -> Vec<Zeile> {
    let mut v = Vec::new();
    for b in &l.buildings {
        for st in &b.storeys {
            let kurz = m.storey(st.id).map_or(String::new(), |x| x.short.clone());
            for g in &st.groups {
                for r in &g.rows {
                    v.push(Zeile {
                        geschoss: kurz.clone(),
                        art: g.category,
                        nummer: r.number.clone(),
                        q: r.q.clone(),
                        grund: r.note.clone(),
                    });
                }
            }
        }
    }
    v
}

/// Gebäudenummern und je Gebäude die Gruppen in Listenreihenfolge:
/// (Geschoss, Bauteilart, Nummern).
type Gruppen = Vec<(String, Vec<(String, Category, Vec<String>)>)>;
fn gruppen(m: &Model, l: &Schedule) -> Gruppen {
    l.buildings
        .iter()
        .map(|b| {
            let nr = m.building(b.id).map_or(String::new(), |x| x.number.clone());
            let g = b
                .storeys
                .iter()
                .flat_map(|st| {
                    let kurz = m.storey(st.id).map_or(String::new(), |x| x.short.clone());
                    st.groups.iter().map(move |g| {
                        let n = g.rows.iter().map(|r| r.number.clone()).collect();
                        (kurz.clone(), g.category, n)
                    })
                })
                .collect();
            (nr, g)
        })
        .collect()
}

/// Summe nach Baustoff des ersten Gebäudes: (Name, m³, m²).
fn baustoffe(m: &Model, l: &Schedule) -> Vec<(String, f64, Option<f64>)> {
    l.buildings[0]
        .by_material
        .iter()
        .map(|x| {
            let name = m
                .material(x.material)
                .map_or(String::new(), |y| y.name.clone());
            (name, x.volume / 1e9, x.area.map(|a| a / 1e6))
        })
        .collect()
}

// Mengenfenster: Zeilen nach Bauteilnummer bzw. Gruppe (Geschoss-Kurzname,
// Bauteilart); der gemeinsame Zustand `Picking` kommt aus F2.
use crate::picking::Picking;
use crate::schedule_view::ListView;

fn mengenfenster(s: &mut Scene) -> ListView {
    ListView::new(s)
}
/// Maus über der Zeile eines Bauteils bzw. einer Gruppe.
fn zeile_hover(v: &mut ListView, s: &mut Scene, p: &mut Picking, nummer: &str) {
    v.hover_row(s, p, nummer)
}
fn gruppe_hover(v: &mut ListView, s: &mut Scene, p: &mut Picking, gs: &str, art: Category) {
    v.hover_group(s, p, gs, art)
}
/// Klick auf eine Zeile; `strg`/`umschalt` wie in Windows-Listen.
fn zeile_klick(
    v: &mut ListView,
    s: &mut Scene,
    p: &mut Picking,
    nummer: &str,
    strg: bool,
    umschalt: bool,
) {
    v.click_row(s, p, nummer, strg, umschalt)
}
fn gruppe_klick(v: &mut ListView, s: &mut Scene, p: &mut Picking, gs: &str, art: Category) {
    v.click_group(s, p, gs, art)
}
/// Die Liste folgt dem gemeinsamen Zustand (nach Auswahl oder Hover in einer
/// Ansicht des Hauptfensters).
fn folgen(v: &mut ListView, s: &mut Scene, p: &Picking) {
    v.follow(s, p);
}
/// Ist die Zeile sichtbar (Gruppe aufgeklappt, in den sichtbaren Bereich
/// gerollt)?
fn zeile_sichtbar(v: &ListView, nummer: &str) -> bool {
    v.row_visible(nummer)
}
/// Zeilen mit Auswahlband bzw. Hover-Band.
fn zeilen_gewaehlt(v: &ListView) -> Vec<String> {
    v.selected_rows()
}
fn zeilen_hover(v: &ListView) -> Vec<String> {
    v.hover_rows()
}
/// Esc im Mengenfenster.
fn liste_esc(v: &mut ListView, s: &mut Scene, p: &mut Picking) {
    v.escape(s, p)
}

// ===== Hilfen =====

/// Prüfhaus B7: Gebäude aus dem Dialog, EG-Innenwand bei x = 5 m.
fn haus_b7(s: &mut Scene) -> (RunId, RunId) {
    let (eg, _) = gebaeude(s);
    let iw = innenwand(
        s,
        &cam3d(),
        vec3(5000.0, 0.0, 0.0),
        vec3(5000.0, 8000.0, 0.0),
    );
    (eg, iw)
}

fn r4(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

/// Meter aus mm, 4 Nachkommastellen.
fn lfm(v: f64) -> f64 {
    r4(v / 1e3)
}

fn zeile<'a>(z: &'a [Zeile], nummer: &str) -> &'a Zeile {
    z.iter()
        .find(|x| x.nummer == nummer)
        .unwrap_or_else(|| panic!("Zeile {nummer} fehlt"))
}

fn wand(z: &Zeile) -> &WallQto {
    match &z.q {
        Some(ElementQto::Wall(w)) => w,
        q => panic!("{}: keine Wand {q:?}", z.nummer),
    }
}

fn decke(z: &Zeile) -> &FloorQto {
    match &z.q {
        Some(ElementQto::Floor(f)) => f,
        q => panic!("{}: keine Decke {q:?}", z.nummer),
    }
}

/// Außenwände eines Geschosses.
fn aussenwaende<'a>(z: &'a [Zeile], geschoss: &str) -> Vec<&'a Zeile> {
    z.iter()
        .filter(|x| x.geschoss == geschoss && x.art == Category::ExteriorWall)
        .collect()
}

fn baustoff_summe(v: &[(String, f64, Option<f64>)], name: &str) -> (f64, Option<f64>) {
    v.iter()
        .find(|x| x.0 == name)
        .map(|x| (r4(x.1), x.2.map(r4)))
        .unwrap_or_else(|| panic!("Baustoff {name} fehlt: {v:?}"))
}

// ===== Tests =====

/// A88 (B7 Aufbau, Gruppierung): Ein Gebäude GB-01; Gruppen nach Bauablauf:
/// Fundament (FS-001, SP-001 nach Kostengruppe 322, obwohl die Sohlplatte zum
/// EG gehört), EG (AW-001…004, IW-001, DE-001), OG (AW-005…008, DE-002);
/// innerhalb der Gruppe nach Nummer. Seit Raumbuch RB5 folgen je Geschoss
/// die Räume (G3): im EG R-001 und R-003 links und rechts der Innenwand,
/// im OG R-002. Jedes Bauteil ist an ein Geschoss des
/// Gebäudes gebunden (A-09). Die Liste wird nie gespeichert.
#[test]
fn a88_gliederung_nach_bauablauf() {
    let mut s = Scene::with_model(Model::with_seed(88));
    haus_b7(&mut s);
    let text = szo(&s);
    let l = liste(&mut s);
    let g = gruppen(s.model(), &l);
    assert_eq!(g.len(), 1, "ein Gebäude");
    assert_eq!(g[0].0, "GB-01");
    let n = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        g[0].1,
        vec![
            ("GR".into(), Category::StripFooting, n(&["FS-001"])),
            ("GR".into(), Category::GroundSlab, n(&["SP-001"])),
            (
                "EG".into(),
                Category::ExteriorWall,
                n(&["AW-001", "AW-002", "AW-003", "AW-004"])
            ),
            ("EG".into(), Category::InteriorWall, n(&["IW-001"])),
            ("EG".into(), Category::Floor, n(&["DE-001"])),
            ("EG".into(), Category::Space, n(&["R-001", "R-003"])),
            (
                "OG".into(),
                Category::ExteriorWall,
                n(&["AW-005", "AW-006", "AW-007", "AW-008"])
            ),
            ("OG".into(), Category::Floor, n(&["DE-002"])),
            ("OG".into(), Category::Space, n(&["R-002"])),
        ]
    );
    // Jedes Bauteil der Liste ist im Modell an ein Geschoss des Gebäudes
    // gebunden; außer bei Sohlplatte und Frostschürze (Gruppe Fundament nach
    // Kostengruppe 322, im Modell heute am EG) ist es das der Gruppe
    let m = s.model();
    let z = zeilen(m, &l);
    assert_eq!(z.len(), 16, "alle Bauteile und Räume, keines doppelt");
    for x in &z {
        let (_, el) = m
            .elements()
            .iter()
            .find(|(_, e)| e.number == x.nummer)
            .unwrap();
        let st = m.storey(el.storey).expect("Geschoss");
        assert!(m.building_of(el.storey).is_some(), "{}: Gebäude", x.nummer);
        if x.geschoss == "GR" {
            assert!(["GR", "EG"].contains(&st.short.as_str()), "{}", x.nummer);
        } else {
            assert_eq!(st.short, x.geschoss, "{}", x.nummer);
        }
        assert!(x.grund.is_none(), "{}: {:?}", x.nummer, x.grund);
    }
    assert_eq!(szo(&s), text, "Liste nicht in der Datei");
}

/// A89 (B7 Fertig, wenn: Fundament, Innenwand, Decken): FS-001 34,60 m und
/// 7,0238 m³; SP-001 80,00 m², 17,6000 m³, Umfang 36,00 m; IW-001 7,37 m,
/// 3,3985 m³ netto, Abzug Deckenstreifen 0,2837 m³ (Paket: 0,2838; genau
/// 0,283745); DE-001 und DE-002 je
/// 75,0384 m², 16,5084 m³, Rand 34,88 m, Auflager in den Außenwänden
/// 1,3159 m³. Die Länge der Innenwand in der Liste (`list_length`) ist die
/// lichte Länge der Kernschicht (7,37 m); `WallQto::length` bleibt die
/// Bezugslinie bis zur Außenkante (8,00 m).
#[test]
fn a89_sollwerte_gruendung_innenwand_decken() {
    let mut s = Scene::with_model(Model::with_seed(89));
    haus_b7(&mut s);
    let l = liste(&mut s);
    let z = zeilen(s.model(), &l);
    match &zeile(&z, "FS-001").q {
        Some(ElementQto::Footing(f)) => {
            assert_eq!(lfm(f.length), 34.6);
            assert_eq!(m3(f.volume), 7.0238);
        }
        q => panic!("FS-001 {q:?}"),
    }
    match &zeile(&z, "SP-001").q {
        Some(ElementQto::Slab(p)) => {
            assert_eq!(m2(p.area), 80.0);
            assert_eq!(m3(p.volume), 17.6);
            assert_eq!(lfm(p.perimeter), 36.0);
        }
        q => panic!("SP-001 {q:?}"),
    }
    let iw = wand(zeile(&z, "IW-001"));
    assert_eq!(lfm(listen_laenge(iw)), 7.37, "lichte Länge");
    assert_eq!(lfm(iw.length), 8.0, "Bezugslinie");
    assert_eq!(m3(iw.volume), 3.3985, "netto");
    assert_eq!(
        m3(tasche(iw)),
        0.2837,
        "Abzug Deckenstreifen 7,37 × 0,175 × 0,22"
    );
    for nr in ["DE-001", "DE-002"] {
        let d = decke(zeile(&z, nr));
        assert_eq!(m2(d.area), 75.0384, "{nr}");
        assert_eq!(m3(d.volume), 16.5084, "{nr}");
        assert_eq!(lfm(d.perimeter), 34.88, "{nr}");
        assert_eq!(m3(auflager(d)), 1.3159, "{nr}: davon Auflager");
    }
}

/// A90 (B7 Fertig, wenn: Außenwände): je Geschoss 4 Stück, 36,00 m;
/// Porenbeton 15,7613 m³ (Längswand 4,4014, Querwand 3,4792); Dämmung
/// 14,1654 m³ und 102,78 m²; Abzug Deckenauflager 1,3159 m³ (Längswand
/// 0,3675, Querwand 0,2905); netto 29,9266 m³ (Paket: 29,9267 aus den
/// gerundeten Schichtsummen; ungerundet 15,761256 + 14,165368). OG gleich EG.
#[test]
fn a90_sollwerte_aussenwaende_je_geschoss() {
    let mut s = Scene::with_model(Model::with_seed(90));
    haus_b7(&mut s);
    let l = liste(&mut s);
    let z = zeilen(s.model(), &l);
    for gs in ["EG", "OG"] {
        let aw = aussenwaende(&z, gs);
        assert_eq!(aw.len(), 4, "{gs}");
        let sum = |f: &dyn Fn(&WallQto) -> f64| aw.iter().map(|x| f(wand(x))).sum::<f64>();
        assert_eq!(lfm(sum(&listen_laenge)), 36.0, "{gs}: Länge");
        assert_eq!(lfm(sum(&|w| w.length)), 36.0, "{gs}: Bezugslinie");
        assert_eq!(
            m3(sum(&|w| w.layers[1].volume)),
            15.7613,
            "{gs}: Porenbeton"
        );
        assert_eq!(m3(sum(&|w| w.layers[0].volume)), 14.1654, "{gs}: Dämmung");
        assert_eq!(
            m2(sum(&|w| schicht_flaeche(&w.layers[0]))),
            102.78,
            "{gs}: WDVS-Fläche 36,00 × 2,855"
        );
        assert_eq!(m3(sum(&tasche)), 1.3159, "{gs}: Abzug Deckenauflager");
        assert_eq!(m3(sum(&|w| w.volume)), 29.9266, "{gs}: netto");
        for x in &aw {
            let w = wand(x);
            assert!(
                (listen_laenge(w) - w.side_outer / w.height).abs() < 1e-6,
                "{gs} {}: Außenseite",
                x.nummer
            );
            let (gb, t) = match lfm(listen_laenge(w)) {
                10.0 => (4.4014, 0.3675),
                8.0 => (3.4792, 0.2905),
                l => panic!("{}: Länge {l}", x.nummer),
            };
            assert_eq!(m3(w.layers[1].volume), gb, "{gs} {}: Porenbeton", x.nummer);
            assert_eq!(m3(tasche(w)), t, "{gs} {}: Tasche", x.nummer);
            assert_eq!(w.height, 2855.0, "{gs} {}: Wandhöhe", x.nummer);
        }
    }
}

/// A91 (B7 „Taschenvolumen als eigene Zeile“): Die Tasche wird genau einmal
/// gezählt: in der Decke enthalten, in der Wand abgezogen. Je Geschoss gilt
/// Auflager der Decke = Summe der Wandtaschen; Porenbeton netto + Tasche =
/// Porenbeton brutto (Fläche × 2,855); die Tasche liegt nur im Porenbeton, nicht
/// im WDVS; Stahlbeton = Platte + Schürze + beide Decken (mit Taschen).
#[test]
fn a91_tasche_nicht_doppelt() {
    let mut s = Scene::with_model(Model::with_seed(91));
    haus_b7(&mut s);
    let l = liste(&mut s);
    let m = s.model();
    let z = zeilen(m, &l);
    for (gs, de) in [("EG", "DE-001"), ("OG", "DE-002")] {
        let aw = aussenwaende(&z, gs);
        let taschen: f64 = aw.iter().map(|x| tasche(wand(x))).sum();
        assert!(
            (auflager(decke(zeile(&z, de))) - taschen).abs() < 1.0,
            "{gs}: Auflager der Decke = Summe der Wandtaschen"
        );
        for x in &aw {
            let w = wand(x);
            let gb = &w.layers[1];
            assert!(
                (gb.volume + tasche(w) - gb.area * 2855.0).abs() < 1.0,
                "{gs} {}: netto + Tasche = brutto",
                x.nummer
            );
            let wdvs = &w.layers[0];
            assert!(
                (wdvs.volume - wdvs.area * 2855.0).abs() < 1.0,
                "{gs} {}: WDVS ohne Abzug",
                x.nummer
            );
        }
    }
    // Stahlbeton: jedes Bauteil genau einmal, Decken mit Taschen
    let sb: f64 = ["SP-001", "FS-001", "DE-001", "DE-002"]
        .iter()
        .map(|nr| match &zeile(&z, nr).q {
            Some(ElementQto::Slab(p)) => p.volume,
            Some(ElementQto::Footing(f)) => f.volume,
            Some(ElementQto::Floor(f)) => f.volume,
            q => panic!("{nr} {q:?}"),
        })
        .sum();
    let b = baustoffe(m, &l);
    assert_eq!(baustoff_summe(&b, "Stahlbeton").0, r4(sb / 1e9));
    // Porenbeton: Wände netto plus Innenwand netto, Taschen nicht enthalten
    let gb: f64 = z
        .iter()
        .filter(|x| matches!(x.art, Category::ExteriorWall))
        .map(|x| wand(x).layers[1].volume)
        .sum::<f64>()
        + wand(zeile(&z, "IW-001")).volume;
    assert_eq!(baustoff_summe(&b, "Porenbeton").0, r4(gb / 1e9));
}

/// A92 (B7 Summen, „Immer aktuell“): Summe nach Baustoff GB-01: Stahlbeton
/// 57,6407 m³, Porenbeton 34,9210 m³, Dämmung (WDVS) 28,3307 m³ / 205,56 m²
/// (aus ungerundeten Werten). Seit Raumbuch RB5 dazu der Ausbau der drei
/// Räume (Vorlage Wohnzimmer) auf 135,6065 m² Boden bis Putz: Estrich
/// 6,5 cm, Trittschall 3 cm, Wärmedämmung 4 cm im EG und 4,5 cm im OG,
/// Parkett 2 cm, Abdichtung 0,5 cm nur auf der Sohlplatte, Gipsputz an
/// Wänden und Decken. Gummiband EG-Wand y = 8 um +1 m: SP 90,00 m²,
/// DE-001 und DE-002 je 84,7584 m², ohne Knopfdruck.
#[test]
fn a92_summe_nach_baustoff_und_immer_aktuell() {
    let mut s = Scene::with_model(Model::with_seed(92));
    haus_b7(&mut s);
    let l = liste(&mut s);
    let b = baustoffe(s.model(), &l);
    assert_eq!(baustoff_summe(&b, "Stahlbeton"), (57.6407, None));
    assert_eq!(baustoff_summe(&b, "Porenbeton"), (34.921, None));
    assert_eq!(
        baustoff_summe(&b, "Dämmung (WDVS)"),
        (28.3307, Some(205.56))
    );
    for (name, soll) in [
        ("Trittschalldämmung EPS-T", (4.0682, Some(135.6065))),
        ("Wärmedämmung EPS 035 DEO", (5.767, Some(135.6065))),
        ("Gipsputz", (4.5836, None)),
        ("Abdichtung Bitumenbahn, Sohlplatte", (0.3353, None)),
        ("Zementestrich CT-C25-F4", (8.8144, None)),
        ("Parkett Eiche", (2.7121, None)),
    ] {
        assert_eq!(baustoff_summe(&b, name), soll, "{name}");
    }
    assert_eq!(b.len(), 9, "nur vorhandene Baustoffe: {b:?}");
    // Gummiband
    ziehen_am_fuss(&mut s, 0.0, 1000.0);
    let l = liste(&mut s);
    let z = zeilen(s.model(), &l);
    match &zeile(&z, "SP-001").q {
        Some(ElementQto::Slab(p)) => assert_eq!(m2(p.area), 90.0),
        q => panic!("SP-001 {q:?}"),
    }
    for nr in ["DE-001", "DE-002"] {
        assert_eq!(m2(decke(zeile(&z, nr)).area), 84.7584, "{nr}");
    }
}

/// A93 (B7 Prüfung): Bauteile ohne Körper verschwinden nicht still. Mit
/// Sockelrücksprung 40 cm (größer als die Wand) bleiben SP-001 und FS-001 als
/// Zeile mit „–“ (keine Mengen) und dem Grund; sie zählen in keine Summe.
#[test]
fn a93_ohne_koerper_mit_grund() {
    let mut s = Scene::with_model(Model::with_seed(93));
    let (eg, _) = haus_b7(&mut s);
    let (slab, _) = sohlplatte(&s, eg);
    assert!(s.edit_model("Rücksprung", |m| m.set_slab_recess(slab, 400.0)));
    let l = liste(&mut s);
    let z = zeilen(s.model(), &l);
    assert_eq!(
        z.len(),
        16,
        "keine Zeile verschwunden (13 Bauteile, 3 Räume)"
    );
    for nr in ["SP-001", "FS-001"] {
        let x = zeile(&z, nr);
        assert!(x.q.is_none(), "{nr}: keine Mengen");
        let g = x.grund.as_deref().unwrap_or("");
        assert!(g.contains("Rücksprung zu groß"), "{nr}: Grund „{g}“");
    }
    let b = baustoffe(s.model(), &l);
    assert_eq!(
        baustoff_summe(&b, "Stahlbeton").0,
        33.0169,
        "nur die beiden Decken"
    );
    // Rückgängig: wieder mit Körper
    assert!(s.undo());
    let l = liste(&mut s);
    assert!(zeilen(s.model(), &l).iter().all(|x| x.q.is_some()));
}

/// A94 (B7 Leistung, Regel 4): Die Liste wird einmal je Modellrevision
/// berechnet und gemerkt, nie während des Ziehens (auch wenn die Ansicht sie
/// in jedem Bild anfragt) und erst nach dem Loslassen neu. Sie ändert weder
/// Revision noch Rückgängig-Liste noch Datei.
#[test]
fn a94_rechnung_nie_waehrend_des_ziehens() {
    let mut s = Scene::with_model(Model::with_seed(94));
    haus_b7(&mut s);
    let (rev, label, text) = (s.model().revision(), s.undo_label(), szo(&s));
    let n0 = berechnungen(&s);
    let erste = liste(&mut s);
    assert_eq!(berechnungen(&s), n0 + 1, "einmal berechnet");
    assert!(!veraltet(&s));
    for _ in 0..5 {
        liste(&mut s);
    }
    assert_eq!(berechnungen(&s), n0 + 1, "gemerkt, solange nichts ändert");
    assert_eq!(
        (s.model().revision(), s.undo_label(), szo(&s)),
        (rev, label, text),
        "nur Lesen"
    );
    // Ziehen der Geschosslinie über zehn Bilder, die Ansicht fragt jedes Mal
    s.begin("Geschoss ziehen");
    for i in 0..10 {
        s.drag_storey_top(geschoss(&s, "EG"), 2855.0 + 10.0 * i as f64);
        let l = liste(&mut s);
        assert_eq!(
            l.buildings.len(),
            erste.buildings.len(),
            "Stand vor dem Ziehen"
        );
    }
    assert_eq!(berechnungen(&s), n0 + 1, "nie während des Ziehens");
    assert!(veraltet(&s), "beim Ziehen: „wird aktualisiert“");
    s.commit();
    liste(&mut s);
    assert!(!veraltet(&s), "nach dem Loslassen aktuell");
    assert_eq!(berechnungen(&s), n0 + 2, "nach dem Loslassen einmal neu");
    // Gummiband: dasselbe
    let n = berechnungen(&s);
    ziehen_am_fuss(&mut s, 0.0, 500.0);
    assert_eq!(berechnungen(&s), n, "Ziehen allein rechnet nicht");
    liste(&mut s);
    assert_eq!(berechnungen(&s), n + 1);
}

/// A95 (B7 „Als Tabelle speichern“): Die .csv ist UTF-8 mit BOM, Semikolon,
/// Dezimalkomma, 4 Nachkommastellen, alle Gruppen aufgeklappt (jede
/// Bauteilnummer), Umlaute richtig; die Summen nach Baustoff stehen darin und
/// stimmen mit der Liste überein. Ohne Körper: Zeile mit Grund.
#[test]
fn a95_csv_mit_dezimalkomma() {
    let mut s = Scene::with_model(Model::with_seed(95));
    let (eg, _) = haus_b7(&mut s);
    let bytes = csv(&mut s);
    assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF], "UTF-8 mit BOM");
    let text = std::str::from_utf8(&bytes[3..]).expect("UTF-8");
    let zeilen_csv: Vec<&str> = text.lines().filter(|z| !z.trim().is_empty()).collect();
    assert!(zeilen_csv.iter().all(|z| !z.contains('\t')));
    assert!(zeilen_csv.iter().any(|z| z.contains(';')), "Semikolon");
    for nr in [
        "GB-01", "FS-001", "SP-001", "AW-001", "AW-002", "AW-003", "AW-004", "IW-001", "DE-001",
        "AW-005", "AW-006", "AW-007", "AW-008", "DE-002",
    ] {
        assert!(text.contains(nr), "{nr} fehlt (alle Gruppen aufgeklappt)");
    }
    for wert in [
        "7,0238", "17,6000", "80,0000", "34,6000", "16,5084", "75,0384", "15,7613", "14,1654",
        "1,3159", "3,3985", "57,6407", "34,9210", "28,3307", "205,5600",
    ] {
        assert!(text.contains(wert), "Wert {wert} fehlt");
    }
    for falsch in ["17.6000", "7.0238", "57.6407"] {
        assert!(!text.contains(falsch), "Dezimalpunkt: {falsch}");
    }
    for wort in ["Außenwände", "Dämmung", "Fundament"] {
        assert!(text.contains(wort), "{wort}");
    }
    assert!(
        !text.contains("Guid") && !text.contains("guid"),
        "keine Guids"
    );
    // Ohne Körper: Zeile mit Grund statt Zahl
    let (slab, _) = sohlplatte(&s, eg);
    assert!(s.edit_model("Rücksprung", |m| m.set_slab_recess(slab, 400.0)));
    let bytes = csv(&mut s);
    let text = String::from_utf8_lossy(&bytes);
    let sp = text.lines().find(|z| z.contains("SP-001")).expect("SP-001");
    assert!(sp.contains("Rücksprung zu groß"), "{sp}");
    assert!(!sp.contains("17,6000"));
}

/// A101 (B7 Kopplung, Jörn 14:00): Hover über AW-003 in der Liste hebt
/// AW-003 im Modell hervor, Hover über die Gruppe „Außenwände“ im EG alle
/// vier; Klick wählt (ersetzt), Strg+Klick fügt hinzu, Umschalt+Klick wählt
/// einen Bereich, Klick auf die OG-Außenwände wählt AW-005…008. Umgekehrt:
/// DE-002 in einer Ansicht gewählt → Liste klappt das OG auf, Zeile sichtbar
/// mit Auswahlband; Hover im Modell → Hover-Band. Esc in der Liste hebt die
/// Auswahl in beiden Fenstern auf. Nichts davon rechnet die Liste neu.
#[test]
fn a101_kopplung_liste_und_modell() {
    let mut s = Scene::with_model(Model::with_seed(101));
    haus_b7(&mut s);
    let el = |s: &Scene, nr: &str| {
        s.model()
            .elements()
            .iter()
            .find(|(_, e)| e.number == nr)
            .unwrap()
            .0
    };
    let mut p = Picking::default();
    let mut v = mengenfenster(&mut s);
    liste(&mut s);
    let n = berechnungen(&s);
    // Liste → Modell
    zeile_hover(&mut v, &mut s, &mut p, "AW-003");
    assert_eq!(p.hover, Some(el(&s, "AW-003")));
    assert_eq!(zeilen_hover(&v), ["AW-003"]);
    gruppe_hover(&mut v, &mut s, &mut p, "EG", Category::ExteriorWall);
    assert_eq!(
        zeilen_hover(&v),
        ["AW-001", "AW-002", "AW-003", "AW-004"],
        "Gruppe: alle vier hervorgehoben"
    );
    zeile_klick(&mut v, &mut s, &mut p, "AW-003", false, false);
    assert_eq!(p.selected, [el(&s, "AW-003")]);
    zeile_klick(&mut v, &mut s, &mut p, "AW-004", true, false);
    assert_eq!(p.selected, [el(&s, "AW-003"), el(&s, "AW-004")], "Strg");
    zeile_klick(&mut v, &mut s, &mut p, "AW-001", false, false);
    zeile_klick(&mut v, &mut s, &mut p, "AW-003", false, true);
    assert_eq!(
        p.selected,
        ["AW-001", "AW-002", "AW-003"].map(|nr| el(&s, nr)),
        "Umschalt: Bereich"
    );
    gruppe_klick(&mut v, &mut s, &mut p, "OG", Category::ExteriorWall);
    assert_eq!(
        p.selected,
        ["AW-005", "AW-006", "AW-007", "AW-008"].map(|nr| el(&s, nr))
    );
    assert_eq!(
        zeilen_gewaehlt(&v),
        ["AW-005", "AW-006", "AW-007", "AW-008"]
    );
    // Modell → Liste: DE-002 in 3D gewählt
    p.selected = vec![el(&s, "DE-002")];
    p.hover = Some(el(&s, "IW-001"));
    folgen(&mut v, &mut s, &p);
    assert!(zeile_sichtbar(&v, "DE-002"), "OG aufgeklappt, gerollt");
    assert_eq!(zeilen_gewaehlt(&v), ["DE-002"]);
    assert_eq!(zeilen_hover(&v), ["IW-001"]);
    // Esc
    liste_esc(&mut v, &mut s, &mut p);
    assert!(p.selected.is_empty());
    assert!(zeilen_gewaehlt(&v).is_empty());
    assert_eq!(berechnungen(&s), n, "Hover und Auswahl rechnen nicht neu");
}

use super::*;

// Abnahmetests A276–A287: Maßeingabe per Tastatur (Paket 8, 8a Zeichnen,
// 8b Ziehen). Grundlage: projektstruktur/paket-8-masseingabe.md mit
// Nachtrag 19:15 (Review 3o K1–K8, Koordinator 19:13 zu K5/K6).
// Spezifikation: test/abnahme-masseingabe.md. Vorbereitet gegen main
// 51161be.
//
// Einbau: als `mod masseingabe { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: tool, click, px, key, mv,
// down, up, cam_plan, cam3d, dialog_ok, gebaeude, geschoss, W, H.
//
// Angenommene Namen stehen NUR in den Adaptern (aus §3 des Plans):
// `crate::ui::{parse_measure, MeasureKind}`, `WallTool::{points, label,
// input}`, `WallEdit::input`, `MeasureInput { fields, active, error }`,
// `Model::{set_linked, set_locked, stack_offset}`.
//
// Die Reihenfolge der Esc-Kaskade in main.rs (K2) und das Abschließen
// eines Paneelfelds (K1) sind Verdrahtung: Handtests H180 und H181. Hier
// wird geprüft, was das Werkzeug selbst tut.

use crate::wall_tool::Outcome;
use sk_model::ElementId;

// ===== Adapter Paket 8 =====

#[derive(Clone, Copy)]
enum Art {
    Laenge,
    Winkel,
    Versatz,
}

/// Strenges Zahlbild (§1.1): Länge und Versatz in mm, Winkel in Grad.
fn zahl(text: &str, art: Art) -> Option<f64> {
    use crate::ui::MeasureKind as K;
    let k = match art {
        Art::Laenge => K::Length,
        Art::Winkel => K::Angle,
        Art::Versatz => K::Offset,
    };
    crate::ui::parse_measure(text, k).ok()
}

/// Gesetzte Punkte des Zugs (Bezugslinie), ohne den Cursorpunkt.
fn punkte(t: &WallTool) -> Vec<Vec3> {
    t.points().to_vec()
}

/// Text der Pille am Gummiband (Live-Länge oder Eingabe), `None` ohne Pille.
fn pille(t: &WallTool) -> Option<String> {
    t.label().map(|(_, text)| text)
}

/// Offene Eingabe: (Feld Länge, Feld Winkel, aktives Feld, Fehler).
#[derive(Debug, PartialEq)]
struct Eingabe {
    felder: [String; 2],
    aktiv: usize,
    fehler: bool,
}

fn eingabe(t: &WallTool) -> Option<Eingabe> {
    t.input().map(|i| Eingabe {
        felder: i.fields.clone(),
        aktiv: i.active,
        fehler: i.error.is_some(),
    })
}

/// Offene Eingabe beim Ziehen bzw. in der Nachkorrektur (nur Feld 0).
fn eingabe_ziehen(e: &WallEdit) -> Option<Eingabe> {
    e.input().map(|i| Eingabe {
        felder: i.fields.clone(),
        aktiv: i.active,
        fehler: i.error.is_some(),
    })
}

fn kette_loesen(s: &mut Scene, wand: ElementId) {
    assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(wand, false)));
}

fn sperren(s: &mut Scene, wand: ElementId) {
    s.edit_model("Gesperrt", |m| {
        m.set_locked(&[wand], true);
        true
    });
}

/// (Versatz in mm auf 0,01 gerundet, gekoppelt?).
fn versatz(s: &Scene, wand: ElementId) -> Option<(f64, bool)> {
    s.model()
        .stack_offset(wand)
        .map(|(o, l)| ((o * 100.0).round() / 100.0, l))
}

// ===== Adapter Nachtrag Paket 8 (A299, A300) =====

/// Text der Pille nach dem Loslassen bzw. nach Enter (Nachkorrektur).
fn pille_nachher(e: &WallEdit, s: &Scene) -> Option<String> {
    e.pill(s).map(|(text, _)| text)
}

/// Live-Länge am Gummiband (paket-p8-darstellung §2.1): zwei
/// Nachkommastellen, Komma, „ m“. Angenommener Name.
fn live_laenge(mm: f64) -> String {
    crate::ui::live_length_text(mm)
}

// ===== Hilfen =====

const OHNE: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
};

fn taste_mit(k: Key, mods: Modifiers) -> Event {
    Event::Key {
        key: k,
        down: true,
        repeat: false,
        mods,
    }
}

fn maus(t: &mut WallTool, c: &Camera, p: Vec3) {
    let (x, y) = px(c, p);
    t.handle(&mv(x, y), c, W, H, 1.0);
}

fn taste(t: &mut WallTool, c: &Camera, k: Key) -> Outcome {
    t.handle(&key(k), c, W, H, 1.0)
}

/// Zeichen für Zeichen tippen, wie über die Tastatur.
fn zeichen(t: &mut WallTool, c: &Camera, text: &str) {
    for ch in text.chars() {
        taste(t, c, Key::Char(ch));
    }
}

/// Länge (und Winkel) tippen und Enter.
fn setze(t: &mut WallTool, c: &Camera, laenge: &str, winkel: Option<&str>) -> Outcome {
    zeichen(t, c, laenge);
    if let Some(w) = winkel {
        taste(t, c, Key::Tab);
        zeichen(t, c, w);
    }
    taste(t, c, Key::Enter)
}

fn nah(a: Vec3, b: Vec3, tol: f64) -> bool {
    (a - b).length() < tol
}

fn gleich_punkte(a: &[Vec3], b: &[Vec3]) {
    assert_eq!(a.len(), b.len(), "{a:?} ≠ {b:?}");
    for (p, q) in a.iter().zip(b) {
        assert!(nah(*p, *q, 1e-3), "{p:?} ≠ {q:?} (0,001 mm)");
    }
}

/// Umfassendes Rechteck (Breite, Tiefe) von Punkten.
fn masse(p: &[Vec3]) -> (f64, f64) {
    let (mut lo, mut hi) = (vec3(f64::MAX, f64::MAX, 0.0), vec3(f64::MIN, f64::MIN, 0.0));
    for q in p {
        lo = vec3(lo.x.min(q.x), lo.y.min(q.y), 0.0);
        hi = vec3(hi.x.max(q.x), hi.y.max(q.y), 0.0);
    }
    (hi.x - lo.x, hi.y - lo.y)
}

fn csv(s: &mut Scene) -> Vec<u8> {
    let l = s.schedule().clone();
    crate::schedule_view::csv(s.model(), &l)
}

fn nummern(s: &Scene) -> Vec<String> {
    let mut v: Vec<String> = s
        .model()
        .elements()
        .iter()
        .map(|(_, e)| e.number.clone())
        .collect();
    v.sort();
    v
}

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

/// y-Lagen der Punkte eines Zugs nahe der Nordseite (y > 4 m).
fn nordseite(s: &Scene, run: RunId) -> Vec<f64> {
    let mut v: Vec<f64> = s
        .model()
        .run(run)
        .unwrap()
        .points
        .iter()
        .filter(|p| p.y > 4000.0)
        .map(|p| (p.y * 100.0).round() / 100.0)
        .collect();
    v.dedup();
    v
}

// Ziehen in 3D am Wandfuß (Nordwand bei x = 2,5 m)

fn e_maus(e: &mut WallEdit, s: &mut Scene, p: Vec3) {
    let c = cam3d();
    let (x, y) = px(&c, p);
    e.handle(&mv(x, y), s, &c, W, H, 1.0, true);
}

fn greifen(e: &mut WallEdit, s: &mut Scene, p: Vec3) {
    let c = cam3d();
    let (x, y) = px(&c, p);
    e.handle(&mv(x, y), s, &c, W, H, 1.0, true);
    e.handle(&down(x, y), s, &c, W, H, 1.0, true);
}

fn loslassen(e: &mut WallEdit, s: &mut Scene, p: Vec3) {
    let c = cam3d();
    let (x, y) = px(&c, p);
    e.handle(&up(x, y), s, &c, W, H, 1.0, true);
}

fn e_taste(e: &mut WallEdit, s: &mut Scene, k: Key) {
    let c = cam3d();
    e.handle(&key(k), s, &c, W, H, 1.0, true);
}

fn e_zeichen(e: &mut WallEdit, s: &mut Scene, text: &str) {
    for ch in text.chars() {
        e_taste(e, s, Key::Char(ch));
    }
}

/// Prüfhaus mit gelöster OG-Nordwand AW-006 (Fuß bei z = 2,855).
fn geloest(seed: u64) -> (Scene, ElementId) {
    let mut s = Scene::with_model(Model::with_seed(seed));
    gebaeude(&mut s);
    let aw6 = nr(&s, "AW-006");
    kette_loesen(&mut s, aw6);
    (s, aw6)
}

const OG: f64 = 2855.0;

fn og_fuss(y: f64) -> Vec3 {
    vec3(2500.0, y, OG)
}

// ===== Tests =====

/// A276 (§1.1 Zahlbild, Abnahme 1, Nachtrag 3o): eine Funktion für alle
/// Felder. Komma oder Punkt, streng nur Ziffern, ein Dezimalzeichen und
/// ein führendes Minus im Winkel- und Versatzfeld; Rundung auf 1 mm bzw.
/// 0,1°; Länge über 0 bis 200 m.
#[test]
fn a276_zahlbild() {
    use Art::*;
    for (t, mm) in [
        ("4,50", 4500.0),
        ("4.5", 4500.0),
        ("4,505", 4505.0),
        (",5", 500.0),
        ("4,", 4000.0),
        ("0,01", 10.0),
        ("200", 200_000.0),
        ("4,5054", 4505.0),
        ("012", 12_000.0),
    ] {
        assert_eq!(zahl(t, Laenge), Some(mm), "Länge „{t}“");
    }
    for t in [
        "", "4,5,0", "4.5,0", "a", "0", "0,000", "200,001", "-4", "-0,30", "inf", "NaN", "1e3",
        "+4", "4 5", "4,5m", "4-", "--4", "-",
    ] {
        assert_eq!(zahl(t, Laenge), None, "Länge „{t}“ ist kein gültiges Maß");
    }
    for (t, g) in [
        ("90", 90.0),
        ("-90", -90.0),
        ("-45", -45.0),
        ("22,5", 22.5),
        ("22.54", 22.5),
        ("0", 0.0),
    ] {
        assert_eq!(zahl(t, Winkel), Some(g), "Winkel „{t}“");
    }
    for t in ["", "+45", "4-5", "--45", "inf", "1e2", "4,5,0", "x"] {
        assert_eq!(zahl(t, Winkel), None, "Winkel „{t}“");
    }
    for (t, mm) in [
        ("0,30", 300.0),
        ("-0,30", -300.0),
        ("-0,25", -250.0),
        ("0,01", 10.0),
        ("0", 0.0),
        (",3", 300.0),
    ] {
        assert_eq!(zahl(t, Versatz), Some(mm), "Versatz „{t}“");
    }
    for t in ["", "+0,30", "0,3-", "--0,3", "NaN", "1e3", "a"] {
        assert_eq!(zahl(t, Versatz), None, "Versatz „{t}“");
    }
}

/// A277 (§1.1, Abnahme 2, K8): Maßzahl am Gummiband zeigt die Live-Länge;
/// eine Ziffer öffnet die Eingabe und das Gummiband zeigt schon die
/// getippte Länge in Richtung der Maus; Enter setzt den Punkt genau. Die
/// nächste Eingabe rechnet vom gesetzten Endpunkt aus.
#[test]
fn a277_laenge_tippen() {
    let s = Scene::with_model(Model::with_seed(277));
    let c = cam_plan(&s);
    let mut t = tool(&s);
    maus(&mut t, &c, vec3(1000.0, 0.0, 0.0));
    assert_eq!(pille(&t), None, "ohne gesetzten Punkt keine Maßzahl");
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    let p0 = punkte(&t)[0];
    maus(&mut t, &c, vec3(3000.0, 120.0, 0.0));
    let live = pille(&t).expect("Maßzahl am Gummiband");
    assert!(live.contains("3,00"), "Live-Länge 3,00: {live}");
    assert_eq!(eingabe(&t), None, "ohne Tippen keine Eingabe");

    zeichen(&mut t, &c, "4,50");
    assert_eq!(
        eingabe(&t),
        Some(Eingabe {
            felder: ["4,50".into(), String::new()],
            aktiv: 0,
            fehler: false
        })
    );
    let v = t.preview().expect("Vorschau beim Tippen");
    let ende = *v.points.last().unwrap();
    assert!(
        nah(ende, p0 + vec3(4500.0, 0.0, 0.0), 1e-3),
        "Gummiband zeigt die getippte Länge: {ende:?}"
    );
    maus(&mut t, &c, vec3(2000.0, 90.0, 0.0));
    let ende = *t.preview().unwrap().points.last().unwrap();
    assert!(
        nah(ende, p0 + vec3(4500.0, 0.0, 0.0), 1e-3),
        "die Maus ändert nur die Richtung: {ende:?}"
    );

    assert!(taste(&mut t, &c, Key::Enter).commit.is_none());
    gleich_punkte(&punkte(&t), &[p0, p0 + vec3(4500.0, 0.0, 0.0)]);
    assert_eq!(eingabe(&t), None, "Eingabe zu");
    assert!(pille(&t).is_some(), "wieder die Live-Länge");

    // K8: sofort die nächste Länge, gerechnet vom Endpunkt
    maus(&mut t, &c, vec3(4500.0, 3000.0, 0.0));
    setze(&mut t, &c, "2", None);
    gleich_punkte(
        &punkte(&t),
        &[
            p0,
            p0 + vec3(4500.0, 0.0, 0.0),
            p0 + vec3(4500.0, 2000.0, 0.0),
        ],
    );
}

/// A278 (Abnahme 3): Der Fang bleibt. Maus fast senkrecht (88°) mit
/// 90°-Sprung, „3“ Enter → genau senkrecht, 3000 mm.
#[test]
fn a278_fang_bleibt() {
    let s = Scene::with_model(Model::with_seed(278));
    let c = cam_plan(&s);
    let mut t = tool(&s);
    assert!(t.ortho, "90°-Sprung ist an");
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    let p0 = punkte(&t)[0];
    let w = 88f64.to_radians();
    maus(&mut t, &c, vec3(2000.0 * w.cos(), 2000.0 * w.sin(), 0.0));
    setze(&mut t, &c, "3", None);
    let p1 = punkte(&t)[1];
    assert!((p1.x - p0.x).abs() < 1e-6, "genau senkrecht: {p1:?}");
    assert!((p1.y - p0.y - 3000.0).abs() < 1e-3, "{p1:?}");
}

/// A279 (§1.1 Richtung, Abnahme 4): Winkel gegen die vorige Wand, 0°
/// geradeaus, plus links, minus rechts. Erste Wand gegen die Waagerechte
/// des Grundrisses (0° nach rechts, plus gegen den Uhrzeiger). Ein leeres
/// Winkelfeld heißt: Richtung von der Maus.
#[test]
fn a279_winkel() {
    let s = Scene::with_model(Model::with_seed(279));
    let c = cam_plan(&s);
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    let p0 = punkte(&t)[0];
    maus(&mut t, &c, vec3(3000.0, 0.0, 0.0));
    setze(&mut t, &c, "5", None);
    // Maus irgendwo: der Winkel bestimmt die Richtung
    maus(&mut t, &c, vec3(6000.0, -4000.0, 0.0));
    zeichen(&mut t, &c, "5");
    taste(&mut t, &c, Key::Tab);
    zeichen(&mut t, &c, "90");
    assert_eq!(
        eingabe(&t),
        Some(Eingabe {
            felder: ["5".into(), "90".into()],
            aktiv: 1,
            fehler: false
        })
    );
    taste(&mut t, &c, Key::Enter);
    setze(&mut t, &c, "5", Some("-90"));
    setze(&mut t, &c, "2", Some("0"));
    gleich_punkte(
        &punkte(&t),
        &[
            p0,
            p0 + vec3(5000.0, 0.0, 0.0),
            p0 + vec3(5000.0, 5000.0, 0.0),
            p0 + vec3(10000.0, 5000.0, 0.0),
            p0 + vec3(12000.0, 5000.0, 0.0),
        ],
    );
    // Leeres Winkelfeld: Richtung von der Maus
    maus(&mut t, &c, vec3(12000.0, 9000.0, 0.0));
    setze(&mut t, &c, "1", Some(""));
    assert!(nah(punkte(&t)[5], p0 + vec3(12000.0, 6000.0, 0.0), 1e-3));

    // Erste Wand: gegen die Waagerechte
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    let p0 = punkte(&t)[0];
    maus(&mut t, &c, vec3(1000.0, -2000.0, 0.0));
    setze(&mut t, &c, "4", Some("30"));
    let r = 30f64.to_radians();
    assert!(nah(
        punkte(&t)[1],
        p0 + vec3(4000.0 * r.cos(), 4000.0 * r.sin(), 0.0),
        1e-3
    ));
}

/// A280 (Abnahme 5 und 13, K6): Rechteck 10 × 8 m im Uhrzeigersinn nur
/// getippt; der vierte Punkt trifft den Start auf unter 1 mm und schließt
/// den Zug. Außenmaß 10,00 × 8,00, Mengen (CSV) und Nummern wie beim
/// geklickten Rechteck. Gegenprobe gegen den Uhrzeigersinn (Winkel +90):
/// Die Bezugslinie liegt innen, Innenmaß 10,00 × 8,00.
#[test]
fn a280_rechteck_getippt() {
    let mut geklickt = Scene::with_model(Model::with_seed(280));
    gebaeude(&mut geklickt);

    let mut s = Scene::with_model(Model::with_seed(280));
    dialog_ok(&mut s);
    let c = cam_plan(&s);
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    maus(&mut t, &c, vec3(0.0, 3000.0, 0.0));
    assert!(setze(&mut t, &c, "8", None).commit.is_none());
    assert!(setze(&mut t, &c, "10", Some("-90")).commit.is_none());
    assert!(setze(&mut t, &c, "8", Some("-90")).commit.is_none());
    let w = setze(&mut t, &c, "10", Some("-90"))
        .commit
        .expect("der vierte Punkt schließt den Zug");
    assert!(w.closed);
    gleich_punkte(&w.clean_points(), &RECHTECK);
    let (b, h) = masse(&w.face_corners(w.outer_offset()));
    assert!(
        (b - 10000.0).abs() < 1e-3 && (h - 8000.0).abs() < 1e-3,
        "Außenmaß {b} × {h}"
    );
    s.add_wall(&w).expect("Gebäude angelegt");
    assert_eq!(s.undo_label(), Some("Gebäude erstellt"));
    assert_eq!(nummern(&s), nummern(&geklickt));
    assert_eq!(
        String::from_utf8(csv(&mut s)).unwrap(),
        String::from_utf8(csv(&mut geklickt)).unwrap(),
        "Mengen wie beim geklickten Rechteck"
    );

    // Gegen den Uhrzeigersinn
    let s = Scene::with_model(Model::with_seed(2800));
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    maus(&mut t, &c, vec3(3000.0, 0.0, 0.0));
    setze(&mut t, &c, "10", None);
    setze(&mut t, &c, "8", Some("90"));
    setze(&mut t, &c, "10", Some("90"));
    let w = setze(&mut t, &c, "8", Some("90"))
        .commit
        .expect("schließt auch gegen den Uhrzeigersinn");
    let (b, h) = masse(&w.face_corners(w.inner_offset()));
    assert!(
        (b - 10000.0).abs() < 1e-3 && (h - 8000.0).abs() < 1e-3,
        "Innenmaß {b} × {h}"
    );
    let (b, _) = masse(&w.face_corners(w.outer_offset()));
    assert!(b > 10000.0 + 500.0, "außen größer: {b}");
}

/// A281 (§1.1, Abnahme 6, K6): Die Pille zeigt immer die Länge der
/// gewählten Bezugslinie (Abstand vom letzten Punkt zum Cursor), für
/// außen, innen und Mitte. „6“ Enter mit Bezugsseite außen → Außenkante
/// 6000 mm. Unter 1 dip Gummiband entfällt die Pille.
#[test]
fn a281_bezugslinie() {
    let s = Scene::with_model(Model::with_seed(281));
    let c = cam_plan(&s);
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    maus(&mut t, &c, vec3(0.0, 4250.0, 0.0));
    for seite in [RefSide::Left, RefSide::Right, RefSide::Center] {
        assert_eq!(t.ref_side, seite);
        let p = pille(&t).unwrap();
        assert!(p.contains("4,25"), "{seite:?}: {p}");
        taste(&mut t, &c, Key::Tab);
    }
    assert_eq!(t.ref_side, RefSide::Left);
    maus(&mut t, &c, vec3(0.0, 0.0, 0.0));
    assert_eq!(pille(&t), None, "Gummiband kürzer als 1 dip");
    maus(&mut t, &c, vec3(0.0, 2000.0, 0.0));
    setze(&mut t, &c, "6", None);
    let w = taste(&mut t, &c, Key::Enter)
        .commit
        .expect("Enter ohne Eingabe beendet den offenen Zug");
    let aussen = w.face_corners(w.outer_offset());
    let l = (aussen[1] - aussen[0]).length();
    assert!((l - 6000.0).abs() < 1e-3, "Außenkante {l}");
}

/// A282 (Abnahme 7, §1.1): Rücktaste löscht ein Zeichen, bei leerer
/// Eingabe den letzten Punkt. Esc schließt erst die Eingabe, das zweite
/// Esc leert den Zug. Enter bei leerer Eingabe beendet den offenen Zug.
/// Ein Klick während der Eingabe verwirft die Zahl und setzt den Punkt an
/// die Maus. Falsche Werte: Fehlerrand, Enter tut nichts. Höchstens 9
/// Zeichen je Feld.
#[test]
fn a282_ruecktaste_esc_fehler() {
    let s = Scene::with_model(Model::with_seed(282));
    let c = cam_plan(&s);
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    maus(&mut t, &c, vec3(3000.0, 0.0, 0.0));
    setze(&mut t, &c, "3", None);
    maus(&mut t, &c, vec3(3000.0, 3000.0, 0.0));
    zeichen(&mut t, &c, "4,5");
    taste(&mut t, &c, Key::Backspace);
    assert_eq!(eingabe(&t).unwrap().felder[0], "4,");
    taste(&mut t, &c, Key::Backspace);
    taste(&mut t, &c, Key::Backspace);
    assert_eq!(
        punkte(&t).len(),
        2,
        "Rücktaste in der Eingabe nimmt keinen Punkt"
    );
    assert!(
        eingabe(&t).is_none_or(|e| e.felder[0].is_empty()),
        "Eingabe leer"
    );
    taste(&mut t, &c, Key::Backspace);
    assert_eq!(punkte(&t).len(), 1, "leere Eingabe: letzter Punkt weg");

    // Esc zweistufig (K2 im Werkzeug)
    maus(&mut t, &c, vec3(3000.0, 0.0, 0.0));
    zeichen(&mut t, &c, "2");
    taste(&mut t, &c, Key::Escape);
    assert_eq!(eingabe(&t), None, "erstes Esc schließt die Eingabe");
    assert!(t.is_active(), "der Zug bleibt");
    assert_eq!(punkte(&t).len(), 1);
    taste(&mut t, &c, Key::Escape);
    assert!(!t.is_active(), "zweites Esc bricht den Zug ab");

    // Enter bei leerer Eingabe beendet den offenen Zug
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    maus(&mut t, &c, vec3(3000.0, 0.0, 0.0));
    setze(&mut t, &c, "3", None);
    let w = taste(&mut t, &c, Key::Enter).commit.expect("beendet");
    assert_eq!(w.clean_points().len(), 2);

    // Klick während der Eingabe: Zahl verworfen, Punkt an der Maus
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    let p0 = punkte(&t)[0];
    maus(&mut t, &c, vec3(2500.0, 0.0, 0.0));
    zeichen(&mut t, &c, "7");
    click(&mut t, &c, vec3(2500.0, 0.0, 0.0));
    assert_eq!(eingabe(&t), None);
    let p1 = punkte(&t)[1];
    assert!(
        (p1.x - p0.x - 2500.0).abs() < 50.0,
        "Punkt an der Maus, nicht bei 7 m: {p1:?}"
    );

    // Falsche Werte
    // Buchstaben gehen ans Werkzeug (K7), „1e3“ ist darum hier kein Fall
    for falsch in ["0", "250", "200,5"] {
        zeichen(&mut t, &c, falsch);
        assert!(eingabe(&t).unwrap().fehler, "„{falsch}“ ist falsch");
        let n = punkte(&t).len();
        assert!(taste(&mut t, &c, Key::Enter).commit.is_none());
        assert_eq!(punkte(&t).len(), n, "Enter tut nichts bei „{falsch}“");
        assert!(eingabe(&t).is_some(), "die Eingabe bleibt offen");
        taste(&mut t, &c, Key::Escape);
    }
    // Höchstens 9 Zeichen
    zeichen(&mut t, &c, "1234567890");
    assert_eq!(eingabe(&t).unwrap().felder[0].chars().count(), 9);
    taste(&mut t, &c, Key::Escape);
}

/// A283 (Abnahme 8, 18, 19; K5, K7): Ohne gesetzten Punkt öffnet eine
/// Ziffer nichts. Tab schaltet ohne Eingabe die Bezugsseite, mit offener
/// Eingabe nur das Feld. Eingabe nur ohne Strg und Alt; Umschalt bleibt
/// erlaubt; `R` schaltet auch bei offener Eingabe den 90°-Sprung.
#[test]
fn a283_tasten_neben_der_eingabe() {
    let s = Scene::with_model(Model::with_seed(283));
    let c = cam_plan(&s);
    let mut t = tool(&s);
    maus(&mut t, &c, vec3(1000.0, 0.0, 0.0));
    zeichen(&mut t, &c, "4");
    assert_eq!(eingabe(&t), None, "ohne Punkt keine Eingabe");
    assert!(!t.is_active());

    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    maus(&mut t, &c, vec3(3000.0, 0.0, 0.0));
    // Tab ohne Eingabe: Bezugsseite
    taste(&mut t, &c, Key::Tab);
    assert_eq!(t.ref_side, RefSide::Right);
    taste(&mut t, &c, Key::Tab);
    taste(&mut t, &c, Key::Tab);
    assert_eq!(t.ref_side, RefSide::Left);
    // Tab mit Eingabe: nur das Feld
    zeichen(&mut t, &c, "4");
    taste(&mut t, &c, Key::Tab);
    assert_eq!(t.ref_side, RefSide::Left, "Bezugsseite unverändert");
    assert_eq!(eingabe(&t).unwrap().aktiv, 1);
    taste(&mut t, &c, Key::Tab);
    assert_eq!(eingabe(&t).unwrap().aktiv, 0, "Tab wechselt zurück");
    assert_eq!(t.ref_side, RefSide::Left);

    // Strg und Alt erreichen die Eingabe nicht
    let strg = Modifiers { ctrl: true, ..OHNE };
    let alt = Modifiers { alt: true, ..OHNE };
    t.handle(&taste_mit(Key::Char('Z'), strg), &c, W, H, 1.0);
    t.handle(&taste_mit(Key::Char('5'), strg), &c, W, H, 1.0);
    t.handle(&taste_mit(Key::Char('6'), alt), &c, W, H, 1.0);
    assert_eq!(
        eingabe(&t).unwrap().felder[0],
        "4",
        "Strg/Alt: nichts getippt"
    );
    // Umschalt gehalten: die Ziffer kommt an
    let umschalt = Modifiers {
        shift: true,
        ..OHNE
    };
    t.handle(&taste_mit(Key::Shift, umschalt), &c, W, H, 1.0);
    t.handle(&taste_mit(Key::Char('3'), umschalt), &c, W, H, 1.0);
    assert_eq!(eingabe(&t).unwrap().felder[0], "43");
    // R geht ans Werkzeug
    let vorher = t.ortho;
    taste(&mut t, &c, Key::Char('R'));
    assert_eq!(t.ortho, !vorher, "R schaltet den 90°-Sprung");
    assert_eq!(
        eingabe(&t).unwrap().felder[0],
        "43",
        "R ist kein Zeichen der Zahl"
    );
}

/// A284 (§1.2, Abnahme 9 und 20, K8): Gelöste OG-Wand ziehen und den
/// Versatz tippen. „0,30“ Enter → +300 auf den Millimeter, ein Schritt
/// „Wand verschoben“, das Loslassen danach tut nichts, Rückgängig stellt
/// 0 her. Die Maus ändert den getippten Wert nicht mehr. „0,01“ → bündig,
/// „-0,25“ → −250. Esc schließt erst die Eingabe, das zweite bricht das
/// Ziehen ab.
#[test]
fn a284_ziehen_versatz_tippen() {
    let (mut s, aw6) = geloest(284);
    let mut e = WallEdit::default();
    greifen(&mut e, &mut s, og_fuss(8000.0));
    e_maus(&mut e, &mut s, og_fuss(8270.0));
    assert!(e.is_dragging());
    e_zeichen(&mut e, &mut s, "0,30");
    assert_eq!(eingabe_ziehen(&e).unwrap().felder[0], "0,30");
    e_maus(&mut e, &mut s, og_fuss(8500.0));
    e_taste(&mut e, &mut s, Key::Enter);
    assert_eq!(versatz(&s, aw6), Some((300.0, false)), "getippt, nicht 500");
    assert_eq!(s.undo_label(), Some("Wand verschoben"));
    assert!(!e.is_dragging(), "Enter beendet das Ziehen");
    loslassen(&mut e, &mut s, og_fuss(8500.0));
    assert_eq!(
        versatz(&s, aw6),
        Some((300.0, false)),
        "Loslassen tut nichts"
    );
    assert!(s.undo());
    assert_eq!(versatz(&s, aw6), Some((0.0, false)));
    assert_eq!(s.undo_label(), Some("Kopplung gelöst"), "genau ein Schritt");

    for (text, soll) in [("0,01", 0.0), ("-0,25", -250.0), ("0,333", 333.0)] {
        let (mut s, aw6) = geloest(2840);
        let mut e = WallEdit::default();
        greifen(&mut e, &mut s, og_fuss(8000.0));
        e_maus(&mut e, &mut s, og_fuss(8100.0));
        e_zeichen(&mut e, &mut s, text);
        e_taste(&mut e, &mut s, Key::Enter);
        assert_eq!(versatz(&s, aw6), Some((soll, false)), "„{text}“");
    }

    // Esc zweistufig
    let (mut s, aw6) = geloest(2841);
    let mut e = WallEdit::default();
    greifen(&mut e, &mut s, og_fuss(8000.0));
    e_maus(&mut e, &mut s, og_fuss(8200.0));
    e_zeichen(&mut e, &mut s, "0,4");
    e_taste(&mut e, &mut s, Key::Escape);
    assert!(eingabe_ziehen(&e).is_none(), "erstes Esc: Eingabe zu");
    assert!(e.is_dragging(), "das Ziehen läuft weiter");
    e_taste(&mut e, &mut s, Key::Escape);
    assert!(!e.is_dragging(), "zweites Esc bricht ab");
    assert_eq!(versatz(&s, aw6), Some((0.0, false)));
    assert_eq!(s.undo_label(), Some("Kopplung gelöst"), "kein Schritt");
}

/// A285 (§1.2, Abnahme 10): Gekoppelte Wand (EG-Nordwand, der Stapel
/// geht mit) ziehen und den Betrag tippen; die Seite zeigt die Maus.
/// Getippte Werte gelten ohne das 10-mm-Raster.
#[test]
fn a285_ziehen_gekoppelt() {
    for (maus_y, soll) in [(8100.0, 8253.0), (7900.0, 7747.0)] {
        let mut s = Scene::with_model(Model::with_seed(285));
        let (eg, og) = gebaeude(&mut s);
        let mut e = WallEdit::default();
        greifen(&mut e, &mut s, vec3(2500.0, 8000.0, 0.0));
        e_maus(&mut e, &mut s, vec3(2500.0, maus_y, 0.0));
        e_zeichen(&mut e, &mut s, "0,253");
        e_taste(&mut e, &mut s, Key::Enter);
        assert_eq!(nordseite(&s, eg), [soll], "Maus bei y = {maus_y}");
        assert_eq!(nordseite(&s, og), [soll], "der Stapel geht mit");
        assert_eq!(s.undo_label(), Some("Wand verschoben"));
        assert!(s.undo());
        assert_eq!(nordseite(&s, eg), [8000.0]);
        assert_eq!(s.undo_label(), Some("Gebäude erstellt"));
    }
}

/// A286 (§1.2 Nachkorrektur, Abnahme 11, 16, 17; K3, K4): Direkt nach
/// dem Loslassen den Wert tippen und Enter ersetzt den letzten Schritt;
/// danach genau ein Schritt, Rückgängig führt vor das Ziehen. Nach einem
/// anderen Schritt, nach Loslassen am Ausgangsort oder an einer
/// gesperrten Wand öffnet eine Ziffer nichts. Bei zwei Ziehen betrifft die
/// Nachkorrektur nur das letzte. Nachkorrektur auf die Ausgangslage:
/// danach gibt es nichts wiederherzustellen.
#[test]
fn a286_nachkorrektur() {
    let (mut s, aw6) = geloest(286);
    let mut e = WallEdit::default();
    greifen(&mut e, &mut s, og_fuss(8000.0));
    e_maus(&mut e, &mut s, og_fuss(8270.0));
    loslassen(&mut e, &mut s, og_fuss(8270.0));
    assert_eq!(versatz(&s, aw6), Some((270.0, false)));
    e_zeichen(&mut e, &mut s, "0,25");
    assert_eq!(eingabe_ziehen(&e).unwrap().felder[0], "0,25");
    e_taste(&mut e, &mut s, Key::Enter);
    assert_eq!(versatz(&s, aw6), Some((250.0, false)));
    assert_eq!(s.undo_label(), Some("Wand verschoben"));
    assert!(s.undo());
    assert_eq!(versatz(&s, aw6), Some((0.0, false)), "vor das Ziehen");
    assert_eq!(s.undo_label(), Some("Kopplung gelöst"), "genau ein Schritt");

    // Anderer Schritt dazwischen
    let (mut s, aw6) = geloest(2860);
    let mut e = WallEdit::default();
    greifen(&mut e, &mut s, og_fuss(8000.0));
    e_maus(&mut e, &mut s, og_fuss(8270.0));
    loslassen(&mut e, &mut s, og_fuss(8270.0));
    let og = geschoss(&s, "OG");
    assert!(s.edit_model("lichte Höhe OG", |m| m.set_clear_height(og, 2700.0)));
    e_zeichen(&mut e, &mut s, "0,25");
    assert!(
        eingabe_ziehen(&e).is_none(),
        "nach einem anderen Schritt nichts"
    );
    e_taste(&mut e, &mut s, Key::Enter);
    assert_eq!(versatz(&s, aw6), Some((270.0, false)));
    assert_eq!(s.undo_label(), Some("lichte Höhe OG"));

    // Loslassen am Ausgangsort: kein Schritt, keine Nachkorrektur
    let (mut s, aw6) = geloest(2861);
    let mut e = WallEdit::default();
    greifen(&mut e, &mut s, og_fuss(8000.0));
    e_maus(&mut e, &mut s, og_fuss(8200.0));
    e_maus(&mut e, &mut s, og_fuss(8000.0));
    loslassen(&mut e, &mut s, og_fuss(8000.0));
    assert_eq!(s.undo_label(), Some("Kopplung gelöst"));
    e_zeichen(&mut e, &mut s, "0,5");
    assert!(eingabe_ziehen(&e).is_none(), "kein eigener Schritt: nichts");
    e_taste(&mut e, &mut s, Key::Enter);
    assert_eq!(versatz(&s, aw6), Some((0.0, false)));
    assert_eq!(
        s.undo_label(),
        Some("Kopplung gelöst"),
        "nie einen fremden Schritt"
    );

    // Zwei Ziehen mit gleichem Etikett: nur das letzte
    let (mut s, aw6) = geloest(2862);
    let mut e = WallEdit::default();
    greifen(&mut e, &mut s, og_fuss(8000.0));
    e_maus(&mut e, &mut s, og_fuss(8270.0));
    loslassen(&mut e, &mut s, og_fuss(8270.0));
    greifen(&mut e, &mut s, og_fuss(8270.0));
    e_maus(&mut e, &mut s, og_fuss(8370.0));
    loslassen(&mut e, &mut s, og_fuss(8370.0));
    assert_eq!(versatz(&s, aw6), Some((370.0, false)));
    e_zeichen(&mut e, &mut s, "0,5");
    e_taste(&mut e, &mut s, Key::Enter);
    assert_eq!(versatz(&s, aw6), Some((500.0, false)));
    assert!(s.undo());
    assert_eq!(
        versatz(&s, aw6),
        Some((270.0, false)),
        "das erste Ziehen bleibt"
    );

    // K4: Nachkorrektur auf die Ausgangslage
    let (mut s, aw6) = geloest(2863);
    let mut e = WallEdit::default();
    greifen(&mut e, &mut s, og_fuss(8000.0));
    e_maus(&mut e, &mut s, og_fuss(8270.0));
    loslassen(&mut e, &mut s, og_fuss(8270.0));
    e_zeichen(&mut e, &mut s, "0");
    e_taste(&mut e, &mut s, Key::Enter);
    assert_eq!(versatz(&s, aw6), Some((0.0, false)));
    assert_eq!(s.redo_label(), None, "Redo ist leer");
    assert!(!s.redo(), "Strg+Y tut nichts");
    assert_eq!(versatz(&s, aw6), Some((0.0, false)));
}

/// A287 (§1.2, Abnahme 12): Gesperrte Wand: kein Ziehen, keine Eingabe,
/// Ziffern bewirken nichts.
#[test]
fn a287_gesperrt() {
    let (mut s, aw6) = geloest(287);
    sperren(&mut s, aw6);
    let label = s.undo_label();
    let mut e = WallEdit::default();
    greifen(&mut e, &mut s, og_fuss(8000.0));
    e_maus(&mut e, &mut s, og_fuss(8200.0));
    e_zeichen(&mut e, &mut s, "0,3");
    assert!(eingabe_ziehen(&e).is_none());
    e_taste(&mut e, &mut s, Key::Enter);
    loslassen(&mut e, &mut s, og_fuss(8200.0));
    e_zeichen(&mut e, &mut s, "0,3");
    assert!(eingabe_ziehen(&e).is_none(), "keine Nachkorrektur");
    e_taste(&mut e, &mut s, Key::Enter);
    assert_eq!(versatz(&s, aw6), Some((0.0, false)));
    assert_eq!(s.undo_label(), label, "kein Schritt");
}
/// A299 (paket-8 §1.2 und Abnahme 9, Koordinator 21:53): Ein getippter
/// Versatz unter 0,02 m setzt die Wand bündig, und die Pille sagt das
/// wie der Hinweis beim Ziehen: „Versatz bündig“ statt einer Zahl.
/// Ab 0,02 m bleibt die Zahl mit Vorzeichen. Enter beendet das Ziehen
/// wie das Loslassen, danach steht die Pille mit dem erreichten Wert
/// (Nachkorrektur, die App blendet sie nach 1,5 s aus). Dasselbe gilt,
/// wenn die Maus unter 2 cm loslässt.
#[test]
fn a299_bundig_nach_getipptem_versatz() {
    // Ausgang: gelöste OG-Wand mit Vorsprung +0,30 (eigener Schritt)
    let vorsprung = |seed: u64| {
        let (mut s, aw6) = geloest(seed);
        let mut e = WallEdit::default();
        greifen(&mut e, &mut s, og_fuss(8000.0));
        e_maus(&mut e, &mut s, og_fuss(8100.0));
        e_zeichen(&mut e, &mut s, "0,30");
        e_taste(&mut e, &mut s, Key::Enter);
        assert_eq!(versatz(&s, aw6), Some((300.0, false)));
        (s, aw6)
    };
    for (text, soll, pille) in [
        ("0,01", 0.0, "Versatz bündig"),
        ("0", 0.0, "Versatz bündig"),
        ("-0,019", 0.0, "Versatz bündig"),
        ("0,02", 20.0, "Versatz +0,02"),
        ("-0,25", -250.0, "Versatz \u{2212}0,25"),
    ] {
        let (mut s, aw6) = vorsprung(2990);
        let mut e = WallEdit::default();
        greifen(&mut e, &mut s, og_fuss(8300.0));
        e_maus(&mut e, &mut s, og_fuss(8200.0));
        e_zeichen(&mut e, &mut s, text);
        e_taste(&mut e, &mut s, Key::Enter);
        assert_eq!(versatz(&s, aw6), Some((soll, false)), "„{text}“");
        assert_eq!(
            pille_nachher(&e, &s).as_deref(),
            Some(pille),
            "Pille nach „{text}“ Enter"
        );
    }

    let (mut s, aw6) = vorsprung(2991);
    let mut e = WallEdit::default();
    greifen(&mut e, &mut s, og_fuss(8300.0));
    e_maus(&mut e, &mut s, og_fuss(8100.0));
    e_maus(&mut e, &mut s, og_fuss(8012.0));
    loslassen(&mut e, &mut s, og_fuss(8012.0));
    assert_eq!(versatz(&s, aw6), Some((0.0, false)), "rastet auf bündig");
    assert_eq!(
        pille_nachher(&e, &s).as_deref(),
        Some("Versatz bündig"),
        "Pille nach dem Loslassen"
    );
}

/// A300 (aq, paket-p8-darstellung §2.1, Koordinator 21:53): Die
/// Live-Länge am Gummiband steht immer als „4,37 m“: Komma, zwei
/// Nachkommastellen, „ m“, auch wenn der Wert nicht auf 10 mm liegt.
/// Das Werkzeug zeigt genau diesen Text für die Länge des Gummibands.
#[test]
fn a300_live_laenge_zwei_stellen_mit_m() {
    for (mm, soll) in [
        (4370.0, "4,37 m"),
        (4372.0, "4,37 m"),
        (4376.0, "4,38 m"),
        (3000.0, "3,00 m"),
        (12.0, "0,01 m"),
        (12345.6, "12,35 m"),
        (200000.0, "200,00 m"),
    ] {
        assert_eq!(live_laenge(mm), soll, "{mm} mm");
    }

    let s = Scene::with_model(Model::with_seed(300));
    let c = cam_plan(&s);
    let mut t = tool(&s);
    click(&mut t, &c, vec3(0.0, 0.0, 0.0));
    for p in [
        vec3(3000.0, 120.0, 0.0),
        vec3(4372.0, 1013.0, 0.0),
        vec3(-1234.0, 2345.0, 0.0),
    ] {
        maus(&mut t, &c, p);
        let ([a, b], text) = t.label().expect("Live-Länge");
        assert_eq!(text, live_laenge((b - a).length()), "bei {p:?}");
        assert!(text.ends_with(" m"), "{text}");
    }
}

/// A306 (Review 3v Niedrig 1, Koordinator 01:57): Beim Ziehen getippt
/// und ohne Enter losgelassen.
/// - Gültiger Wert: genau wie mit Enter. Gleiches Modell, ein Schritt
///   „Wand verschoben“, das Ziehen ist zu, die Eingabe ist zu, die Pille
///   zeigt den Wert wie nach Enter und tippt nicht mehr (blendet also
///   nach 1,5 s aus). Eine Nachkorrektur danach wirkt wie nach Enter.
/// - Ungültiger Wert („-“, „250“ über 200 m): die Eingabe bleibt offen
///   mit dem getippten Text und dem Fehlerpunkt. Die Wand steht, wo die
///   Maus losließ (wie ohne Tippen). Berichtigen und Enter setzt den
///   Wert; danach bleibt es ein Schritt. Esc schließt die Eingabe, die
///   Wand bleibt bei der Maus.
#[test]
fn a306_loslassen_ohne_enter() {
    let szo = |s: &Scene| sk_model::szo::write(s.model());
    let getippt = |seed: u64, mit_enter: bool| {
        let (mut s, aw6) = geloest(seed);
        let mut e = WallEdit::default();
        greifen(&mut e, &mut s, og_fuss(8000.0));
        e_maus(&mut e, &mut s, og_fuss(8270.0));
        e_zeichen(&mut e, &mut s, "0,30");
        e_maus(&mut e, &mut s, og_fuss(8500.0));
        if mit_enter {
            e_taste(&mut e, &mut s, Key::Enter);
        }
        loslassen(&mut e, &mut s, og_fuss(8500.0));
        (s, e, aw6)
    };
    let (mut s1, mut e1, aw6) = getippt(306, true);
    let (mut s2, mut e2, _) = getippt(306, false);
    assert_eq!(
        versatz(&s2, aw6),
        Some((300.0, false)),
        "getippt, nicht 500"
    );
    assert_eq!(szo(&s2), szo(&s1), "Modell wie mit Enter");
    assert_eq!(s2.undo_label(), Some("Wand verschoben"));
    assert!(!e2.is_dragging(), "Loslassen beendet das Ziehen");
    assert!(eingabe_ziehen(&e2).is_none(), "Eingabe zu wie nach Enter");
    assert_eq!(e2.pill(&s2), e1.pill(&s1), "Pille wie nach Enter");
    assert_eq!(
        e2.pill(&s2),
        Some(("Versatz +0,30".to_string(), false)),
        "Wert, nicht mehr tippend"
    );
    // Nachkorrektur wie nach Enter
    for (s, e) in [(&mut s1, &mut e1), (&mut s2, &mut e2)] {
        e_zeichen(e, s, "0,4");
        e_taste(e, s, Key::Enter);
    }
    assert_eq!(versatz(&s2, aw6), Some((400.0, false)), "Nachkorrektur");
    assert_eq!(szo(&s2), szo(&s1), "Nachkorrektur wie nach Enter");
    assert!(s2.undo());
    assert_eq!(versatz(&s2, aw6), Some((0.0, false)));
    assert_eq!(
        s2.undo_label(),
        Some("Kopplung gelöst"),
        "genau ein Schritt"
    );

    for text in ["-", "250"] {
        let neu = |seed: u64| {
            let (mut s, aw6) = geloest(seed);
            let mut e = WallEdit::default();
            greifen(&mut e, &mut s, og_fuss(8000.0));
            e_maus(&mut e, &mut s, og_fuss(8270.0));
            e_zeichen(&mut e, &mut s, text);
            loslassen(&mut e, &mut s, og_fuss(8270.0));
            (s, e, aw6)
        };
        let (mut s, mut e, aw6) = neu(3061);
        let i = eingabe_ziehen(&e).unwrap_or_else(|| panic!("„{text}“: Eingabe bleibt offen"));
        assert_eq!(i.felder[0], text, "getippter Text bleibt");
        assert!(i.fehler, "„{text}“: Fehlerpunkt");
        assert_eq!(
            e.pill(&s).map(|p| p.1),
            Some(true),
            "„{text}“: tippt weiter"
        );
        assert_eq!(
            versatz(&s, aw6),
            Some((270.0, false)),
            "„{text}“: bei der Maus"
        );
        // berichtigen und Enter
        for _ in 0..text.chars().count() {
            e_taste(&mut e, &mut s, Key::Backspace);
        }
        e_zeichen(&mut e, &mut s, "0,2");
        e_taste(&mut e, &mut s, Key::Enter);
        assert_eq!(
            versatz(&s, aw6),
            Some((200.0, false)),
            "„{text}“ berichtigt"
        );
        assert!(eingabe_ziehen(&e).is_none());
        assert_eq!(s.undo_label(), Some("Wand verschoben"));
        assert!(s.undo());
        assert_eq!(s.undo_label(), Some("Kopplung gelöst"), "genau ein Schritt");
        // Esc schließt die Eingabe, die Wand bleibt bei der Maus
        let (mut s, mut e, aw6) = neu(3062);
        e_taste(&mut e, &mut s, Key::Escape);
        assert!(eingabe_ziehen(&e).is_none(), "„{text}“: Esc schließt");
        assert!(!e.is_dragging());
        assert_eq!(
            versatz(&s, aw6),
            Some((270.0, false)),
            "„{text}“: bei der Maus"
        );
        assert_eq!(s.undo_label(), Some("Wand verschoben"));
        assert!(s.undo());
        assert_eq!(s.undo_label(), Some("Kopplung gelöst"), "ein Schritt");
    }

    // Ungültig getippt und ohne Bewegung losgelassen (Review 3z): kein
    // Verlaufsschritt, also keine Nachkorrektur; die Eingabe schließt
    // und bleibt nicht unsichtbar offen
    let (mut s, aw6) = geloest(3063);
    let mut e = WallEdit::default();
    greifen(&mut e, &mut s, og_fuss(8000.0));
    e_zeichen(&mut e, &mut s, "-");
    loslassen(&mut e, &mut s, og_fuss(8000.0));
    assert_eq!(versatz(&s, aw6), Some((0.0, false)), "Wand blieb");
    assert_eq!(s.undo_label(), Some("Kopplung gelöst"), "kein Schritt");
    assert_eq!(e.pill(&s), None);
    assert!(eingabe_ziehen(&e).is_none(), "Eingabe zu");

    // Gültig getippt und ohne Bewegung losgelassen: wie Enter
    let (mut s, aw6) = geloest(3064);
    let mut e = WallEdit::default();
    greifen(&mut e, &mut s, og_fuss(8000.0));
    e_zeichen(&mut e, &mut s, "0,2");
    loslassen(&mut e, &mut s, og_fuss(8000.0));
    assert_eq!(versatz(&s, aw6), Some((200.0, false)), "Wert gilt");
    assert_eq!(s.undo_label(), Some("Wand verschoben"));
    assert_eq!(e.pill(&s), Some(("Versatz +0,20".to_string(), false)));
    assert!(eingabe_ziehen(&e).is_none(), "Eingabe zu");
}

use super::*;

// Abnahmetests A161–A166: Schnitt B–B (längs, 90° zu A–A), Blickrichtung
// spiegeln, Schnittrad wie der Geschossbogen (Jörn 07.10. 05:44).
// Gestaltung: einstellungen/paket-e19-schnittrad.md (Prüfpunkte §7).
// Spezifikation: test/abnahme-schnitte.md. Aussehen, Übergänge und Bedienung
// im Fenster: H137–H142.
//
// Fassung 3: Erwartungen nach E19 §8 (Nachtrag nach 7663ddb, Entscheid des
// Koordinators 06:15): Unterzeile „Längsschnitt“/„Querschnitt“ aus der
// Gebäudegeometrie (Linie parallel zur längeren Seite = Längsschnitt, bei
// gleich langen Seiten oder ohne Gebäude keine), im Rad A oben und B unten,
// keine Kopfbeschriftung, der aktive Schnitt steht in der .szo und „Schnitt“
// öffnet den zuletzt benutzten.
//
// Die Adapter sind gegen die API von 7663ddb geschrieben (`sk_model::Cut`,
// `section::Sections`, `wheel::Track::Cuts`) und dort geprüft. Mit der neuen
// Sections-API des Folge-Commits tauscht der Bauthread nur die
// Adapterkörper; `ansicht_schnitt` soll dann dieselbe Entscheidung wie
// `set_view` treffen (zuletzt benutzter Schnitt).
//
// Einbau: als `mod schnitt_b { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: zeichne_rechteck, cam3d,
// cam_plan, px, down, mv, up, view_mesh, ViewKind, pattern, HAUS_SZO1, W, H.
//
// Prüfhaus: Rechteck 10 × 8 m (x 0…10, y 0…8). A–A liegt quer (Ebene
// y = const, Blick in +y, wie bisher), B–B längs (Ebene x = const, Blick im
// Grundriss nach rechts, also +x). Gespiegelt: Blick in −y bzw. −x. Die
// Ebene ist (Punkt, Normale zum Betrachter).

use crate::section::{Sections, CUT_A, CUT_B};
use crate::wheel::{Track, Wheel};
use sk_model::{Cut, CUT_NAMES};

/// Fensterzustand wie in main.rs: Szene (aktiver Schnitt, Schnitte für die
/// Datei), Schnittlinien im Grundriss, Schnittrad, Uhr in ms.
struct Fall {
    s: Scene,
    sc: Sections,
    w: Wheel,
    t: u64,
}

// ===== Adapter =====

/// Projekt geöffnet: Schnitte aus der Datei übernehmen (main.rs `open`).
fn fall(s: Scene) -> Fall {
    let mut sc = Sections::default();
    sc.load(&s);
    Fall {
        s,
        sc,
        w: Wheel::new(&Theme::dark(), true),
        t: 0,
    }
}

/// Klick auf „Schnitt“ in „Ansichten“ (main.rs `set_view`): zeigt den
/// zuletzt benutzten Schnitt.
fn ansicht_schnitt(f: &mut Fall) {
    f.w.set_track(Track::Cuts);
    f.sc.ensure(&f.s);
}

fn nummer(welcher: char) -> usize {
    CUT_NAMES
        .iter()
        .position(|n| n.starts_with(welcher))
        .unwrap()
}

/// Aktiver Schnitt: 'A' oder 'B'.
fn aktiv(f: &Fall) -> char {
    CUT_NAMES[f.s.active_cut()].chars().next().unwrap()
}

/// Ist das Rad (Geschossbogen bzw. Schnittrad) in dieser Ansicht zu sehen?
fn rad_sichtbar(f: &Fall, view: ViewKind) -> bool {
    f.w.visible(view, false)
}

/// Schnittrad: Mitte (Name, Unterzeile), Beschriftung an der oberen und an
/// der unteren Spitze (`None` = ausgegraut).
type Rad = ((String, String), Option<String>, Option<String>);
fn rad(f: &Fall) -> Rad {
    (
        f.w.center(&f.s),
        f.w.neighbor(&f.s, true),
        f.w.neighbor(&f.s, false),
    )
}

/// Hinweis an einer Spitze: (Zeile, Unterzeile); `None` an einer grauen.
fn hinweis(f: &Fall, hoch: bool) -> Option<(String, String)> {
    f.w.arrow_hint(&f.s, hoch, false)
}

/// Klick auf eine Spitze des Schnittrads.
fn spitze(f: &mut Fall, hoch: bool) {
    f.t += 1000;
    f.w.click_arrow(&mut f.s, hoch, false, f.t);
    f.t += 1000;
    f.w.tick(&mut f.s, f.t);
}

/// Mausrad über dem Schnittrad: `rasten` > 0 = hoch.
fn mausrad(f: &mut Fall, rasten: i32) {
    f.t += 1000;
    f.w.scroll(&mut f.s, rasten, f.t);
    f.t += 1000;
    f.w.tick(&mut f.s, f.t);
}

/// Knopf „Blickrichtung“ am Schnittrad (main.rs `mirror_cut`).
fn spiegeln(f: &mut Fall) {
    let i = f.s.active_cut();
    f.sc.lines[i].ensure(&f.s);
    f.sc.lines[i].mirror();
    f.s.set_cut(i, f.sc.lines[i].cut());
}

/// Ebene des aktiven Schnitts.
fn ebene(f: &Fall) -> Option<(Vec3, Vec3)> {
    f.sc.plane(f.s.active_cut())
}

/// Lage (mm; y bei A, x bei B) und ob gespiegelt.
fn lage(f: &Fall, welcher: char) -> Option<(f64, bool)> {
    let c = f.sc.lines[nummer(welcher)].cut();
    c.pos.map(|p| (p, c.flip))
}

/// Ereignis im Grundriss an die Schnittlinien (main.rs `cut_changed`).
fn grundriss(f: &mut Fall, e: &Event) -> bool {
    let c = cam_plan(&f.s);
    let out = f.sc.handle(e, &f.s, &c, W, H, 1.0, true);
    if let Some(i) = out.line.filter(|_| out.changed) {
        f.s.set_cut(i, f.sc.lines[i].cut());
    }
    out.mirrored
}

/// Linie im Grundriss mit der Maus quer verschieben: greifen bei `von`,
/// loslassen bei `nach` (Punkte im Grundriss, mm).
fn ziehen(f: &mut Fall, von: Vec3, nach: Vec3) {
    let c = cam_plan(&f.s);
    let (x, y) = px(&c, von);
    let (x2, y2) = px(&c, nach);
    grundriss(f, &down(x, y));
    grundriss(f, &mv(x2, y2));
    grundriss(f, &up(x2, y2));
}

/// Klick auf ein Endsymbol (Blickpfeil) der Linie `welcher` im Grundriss;
/// `true`, wenn gespiegelt wurde.
fn pfeil_klick(f: &mut Fall, welcher: char) -> bool {
    let c = cam_plan(&f.s);
    let i = nummer(welcher);
    let m = f.sc.lines[i].marks(&f.s, &c, W, H).remove(0);
    let gespiegelt = grundriss(f, &down(m.x, m.y));
    grundriss(f, &up(m.x, m.y));
    gespiegelt
}

/// Endsymbole im Grundriss: Kennung der Linie je Symbol.
fn endsymbole(f: &Fall) -> Vec<usize> {
    let c = cam_plan(&f.s);
    f.sc.marks(&f.s, &c, W, H)
        .into_iter()
        .map(|m| m.0)
        .collect()
}

fn speichern(f: &Fall) -> String {
    sk_model::szo::write(f.s.model())
}

fn oeffnen(text: &str) -> Fall {
    let l = sk_model::szo::read(text, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    fall(Scene::with_model(l.model))
}

// ===== Hilfen =====

fn prueffall(seed: u64) -> Fall {
    let mut s = Scene::with_model(Model::with_seed(seed));
    zeichne_rechteck(&mut s, &cam3d());
    let mut f = fall(s);
    ansicht_schnitt(&mut f);
    f
}

/// Gebäude aus einem geschlossenen EG-Polygon (Außenkanten, mm).
fn zeichne(s: &mut Scene, punkte: &[Vec3]) {
    let c = cam3d();
    let mut t = tool(s);
    for p in punkte {
        assert!(click(&mut t, &c, *p).is_none());
    }
    let wall = click(&mut t, &c, punkte[0]).expect("schließt");
    s.add_wall(&wall).expect("Wandzug angelegt");
}

/// Schnittflächen des aktiven Schnitts: schraffiert vorhanden, kleinster und
/// größter Wert aller Flächen auf der Achse `achse` (0 = x, 1 = y).
fn schnitt(f: &mut Fall, achse: usize) -> (bool, f32, f32) {
    let p = ebene(f);
    let m = view_mesh(&mut f.s, ViewKind::Section, p);
    let lo = m.faces.iter().map(|v| v[achse]).fold(f32::MAX, f32::min);
    let hi = m.faces.iter().map(|v| v[achse]).fold(f32::MIN, f32::max);
    let schraffiert = m.faces.iter().any(|v| v[9] != pattern::NONE);
    (schraffiert, lo, hi)
}

fn r(mitte: &str, unter: &str, oben: Option<&str>, unten: Option<&str>) -> Rad {
    (
        (mitte.into(), unter.into()),
        oben.map(Into::into),
        unten.map(Into::into),
    )
}

fn k(a: &str, b: &str) -> Option<(String, String)> {
    Some((a.into(), b.into()))
}

// ===== Tests =====

/// A161 (E19 §1, §2, §7.1, §7.6): Klick „Schnitt“ zeigt A–A. A liegt wie
/// bisher quer in der Mitte (y = 4000, Blick +y), B längs in der Mitte
/// (x = 5000, Blick nach rechts). Beide nicht gespiegelt. Das Schnittrad
/// steht nur im Schnitt: „A–A / Längsschnitt“ (A läuft parallel zur
/// längeren Seite), oben grau, unten „B–B“. Der Grundriss zeigt A und B mit je zwei Endsymbolen; B lässt sich
/// quer im 10-mm-Raster verschieben, der Schnitt folgt.
#[test]
fn a161_schnitt_b_liegt_mittig() {
    let mut f = prueffall(161);
    assert_eq!(lage(&f, 'A'), Some((4000.0, false)));
    assert_eq!(lage(&f, 'B'), Some((5000.0, false)));
    assert_eq!(aktiv(&f), 'A', "Klick „Schnitt“: A–A");
    assert_eq!(
        ebene(&f),
        Some((vec3(0.0, 4000.0, 0.0), vec3(0.0, -1.0, 0.0))),
        "A wie bisher"
    );
    assert!(rad_sichtbar(&f, ViewKind::Section));
    for v in [
        ViewKind::Plan,
        ViewKind::Persp,
        ViewKind::Front,
        ViewKind::Back,
        ViewKind::Left,
        ViewKind::Right,
    ] {
        assert!(!rad_sichtbar(&f, v), "{v:?}");
    }
    assert_eq!(rad(&f), r("A–A", "Längsschnitt", None, Some("B–B")));
    let mut ids = endsymbole(&f);
    ids.sort();
    assert_eq!(ids, vec![CUT_A, CUT_A, CUT_B, CUT_B], "je zwei Endsymbole");

    // B im Grundriss verschieben: 10-mm-Raster, B wird aktiv gezeigt
    ziehen(&mut f, vec3(5000.0, 2000.0, 0.0), vec3(6503.0, 2000.0, 0.0));
    let (bx, gesp) = lage(&f, 'B').unwrap();
    assert!(!gesp);
    assert_eq!(bx % 10.0, 0.0, "10-mm-Raster: {bx}");
    assert!((bx - 6500.0).abs() <= 10.0, "{bx}");
    assert_eq!(lage(&f, 'A'), Some((4000.0, false)), "A bleibt liegen");
    assert_eq!(f.s.model().cuts()[CUT_B].pos, Some(bx), "für die Datei");
    spitze(&mut f, false);
    assert_eq!(ebene(&f).unwrap().0.x, bx, "der Schnitt folgt der Linie");
}

/// A162 (E19 §2, §7.2, §7.3): Über das Rad zu B–B: Ebene x = 5000, Blick +x,
/// Schnittflächen und nichts davor. Rad „B–B / Querschnitt“. An den Enden
/// wirken Klick und Raste nicht. Eine Raste = ein Schnitt. Kein Wechsel
/// verschiebt eine Linie. Hinweise an den Spitzen.
#[test]
fn a162_wechsel_ueber_das_rad() {
    let mut f = prueffall(162);
    assert_eq!(hinweis(&f, false), k("Schnitt B–B ↓", "Querschnitt"));
    assert_eq!(hinweis(&f, true), None, "graue Spitze");
    spitze(&mut f, true);
    assert_eq!(aktiv(&f), 'A', "graue Spitze: keine Wirkung");
    mausrad(&mut f, 1);
    assert_eq!(aktiv(&f), 'A', "am Ende: Raste ohne Wirkung");

    spitze(&mut f, false);
    assert_eq!(aktiv(&f), 'B');
    assert_eq!(rad(&f), r("B–B", "Querschnitt", Some("A–A"), None));
    assert_eq!(hinweis(&f, true), k("Schnitt A–A ↑", "Längsschnitt"));
    assert_eq!(hinweis(&f, false), None);
    let (pkt, n) = ebene(&f).unwrap();
    assert_eq!(pkt.x, 5000.0);
    assert_eq!(n, vec3(-1.0, 0.0, 0.0), "Blick in +x");
    let (schraffiert, lo, _) = schnitt(&mut f, 0);
    assert!(schraffiert, "Schnittflächen");
    assert!(lo >= 5000.0 - 1e-2, "nichts vor der Ebene: {lo}");
    spitze(&mut f, false);
    assert_eq!(aktiv(&f), 'B', "graue Spitze: keine Wirkung");
    mausrad(&mut f, -1);
    assert_eq!(aktiv(&f), 'B', "am Ende: Raste ohne Wirkung");

    mausrad(&mut f, 1);
    assert_eq!(aktiv(&f), 'A', "Raste");
    mausrad(&mut f, -1);
    assert_eq!(aktiv(&f), 'B', "Raste zurück");
    spitze(&mut f, true);
    assert_eq!(aktiv(&f), 'A');
    assert_eq!(lage(&f, 'A'), Some((4000.0, false)));
    assert_eq!(lage(&f, 'B'), Some((5000.0, false)));
    let (_, lo, _) = schnitt(&mut f, 1);
    assert!(lo >= 4000.0 - 1e-2, "A: nichts vor y = 4000");
}

/// A163 (E19 §2, §7.4): „Blickrichtung“ kehrt nur den Blick des gezeigten
/// Schnitts um, die Lage bleibt. A gespiegelt: Normale +y, nur y ≤ 4000 zu
/// sehen, Rad „Längsschnitt · gespiegelt“. Kein Rückgängig-Schritt, keine neue
/// Revision. B bleibt, bis es selbst gespiegelt wird (Blick −x, nur
/// x ≤ 5000). Zweiter Klick: zurück. Im Grundriss spiegelt ein Klick auf
/// einen Blickpfeil dieselbe Linie.
#[test]
fn a163_spiegeln() {
    let mut f = prueffall(163);
    let rev = f.s.model().revision();
    let schritt = f.s.undo_label();
    spiegeln(&mut f);
    assert_eq!(lage(&f, 'A'), Some((4000.0, true)));
    assert_eq!(lage(&f, 'B'), Some((5000.0, false)), "B unberührt");
    assert_eq!(
        ebene(&f),
        Some((vec3(0.0, 4000.0, 0.0), vec3(0.0, 1.0, 0.0)))
    );
    assert_eq!(
        rad(&f),
        r("A–A", "Längsschnitt · gespiegelt", None, Some("B–B"))
    );
    let (schraffiert, _, hi) = schnitt(&mut f, 1);
    assert!(schraffiert);
    assert!(hi <= 4000.0 + 1e-2, "gespiegelt: nichts bei y > 4000: {hi}");
    assert_eq!(f.s.undo_label(), schritt, "kein Rückgängig-Schritt");
    assert_eq!(f.s.model().revision(), rev, "keine neue Revision");
    // Rückgängig/Wiederholen des Zeichnens lässt die Blickrichtung stehen
    assert!(f.s.undo() && f.s.redo());
    assert!(f.s.model().cuts()[CUT_A].flip, "Spiegelung bleibt");
    let rev = f.s.model().revision();

    spitze(&mut f, false);
    assert_eq!(rad(&f), r("B–B", "Querschnitt", Some("A–A"), None));
    spiegeln(&mut f);
    assert_eq!(lage(&f, 'B'), Some((5000.0, true)));
    assert_eq!(ebene(&f).unwrap().1, vec3(1.0, 0.0, 0.0), "Blick in −x");
    assert_eq!(rad(&f).0 .1, "Querschnitt · gespiegelt");
    let (_, _, hi) = schnitt(&mut f, 0);
    assert!(
        hi <= 5000.0 + 1e-2,
        "B gespiegelt: nichts bei x > 5000: {hi}"
    );
    spiegeln(&mut f);
    assert_eq!(
        lage(&f, 'B'),
        Some((5000.0, false)),
        "zweiter Klick: zurück"
    );
    spitze(&mut f, true);
    assert_eq!(lage(&f, 'A'), Some((4000.0, true)), "A bleibt gespiegelt");
    spiegeln(&mut f);
    assert_eq!(lage(&f, 'A'), Some((4000.0, false)));

    // Blickpfeil im Grundriss
    assert!(pfeil_klick(&mut f, 'B'), "Pfeil spiegelt");
    assert_eq!(lage(&f, 'B'), Some((5000.0, true)));
    assert_eq!(lage(&f, 'A'), Some((4000.0, false)), "nur diese Linie");
    assert!(f.s.model().cuts()[CUT_B].flip, "für die Datei");
    assert!(pfeil_klick(&mut f, 'B'));
    assert_eq!(lage(&f, 'B'), Some((5000.0, false)));
    assert_eq!(f.s.model().revision(), rev);
}

/// A164 (E19 §7.5): Lage und Spiegelung je Schnitt und der aktive Schnitt
/// stehen in der .szo. A auf y ≈ 2500 (10-mm-Raster) gespiegelt, B auf
/// x = 7000, B zuletzt benutzt: speichern, öffnen, speichern ist bytegleich,
/// alles kommt genau so zurück; „Schnitt“ zeigt wieder B–B.
#[test]
fn a164_schnitte_in_der_datei() {
    let mut f = prueffall(164);
    ziehen(&mut f, vec3(2000.0, 4000.0, 0.0), vec3(2000.0, 2503.0, 0.0));
    let (ay, _) = lage(&f, 'A').unwrap();
    assert_eq!(ay % 10.0, 0.0, "10-mm-Raster: {ay}");
    assert!((ay - 2500.0).abs() <= 10.0, "{ay}");
    spiegeln(&mut f);
    ziehen(&mut f, vec3(5000.0, 2000.0, 0.0), vec3(7000.0, 2000.0, 0.0));
    let (bx, _) = lage(&f, 'B').unwrap();
    spitze(&mut f, false);
    ansicht_schnitt(&mut f);
    assert_eq!(aktiv(&f), 'B', "„Schnitt“ zeigt den zuletzt benutzten");
    let t = speichern(&f);
    let mut f2 = oeffnen(&t);
    assert_eq!(speichern(&f2), t, "bytegleich");
    assert_eq!(
        f2.s.model().cuts(),
        &[
            Cut {
                pos: Some(ay),
                flip: true
            },
            Cut {
                pos: Some(bx),
                flip: false
            }
        ]
    );
    ansicht_schnitt(&mut f2);
    assert_eq!(lage(&f2, 'A'), Some((ay, true)));
    assert_eq!(lage(&f2, 'B'), Some((bx, false)));
    assert_eq!(aktiv(&f2), 'B', "zuletzt benutzter Schnitt");
    assert_eq!(ebene(&f2).unwrap().0.x, bx);
}

/// A165 (Koordinator, F-17): Eine Datei ohne Schnittangaben (SZO 1 und eine
/// heutige Datei, in der nie ein Schnitt gezeigt wurde) öffnet; im Schnitt
/// liegen A und B in der Mitte, nicht gespiegelt, A aktiv. Wer nur schaut,
/// schreibt keine Schnittzeilen.
#[test]
fn a165_alte_datei_bekommt_b() {
    let mut alt = oeffnen(HAUS_SZO1);
    assert_eq!(alt.s.model().cuts(), &[Cut::default(); 2]);
    ansicht_schnitt(&mut alt);
    let mitte = alt.s.center().expect("Modellmitte");
    let raster = |v: f64| (v / 10.0).round() * 10.0;
    assert_eq!(lage(&alt, 'A'), Some((raster(mitte.y), false)));
    assert_eq!(lage(&alt, 'B'), Some((raster(mitte.x), false)));
    assert_eq!(aktiv(&alt), 'A');

    let mut s = Scene::with_model(Model::with_seed(165));
    zeichne_rechteck(&mut s, &cam3d());
    let ohne = sk_model::szo::write(s.model());
    let mut f = oeffnen(&ohne);
    ansicht_schnitt(&mut f);
    spitze(&mut f, false);
    assert_eq!(speichern(&f), ohne, "nur schauen: keine Schnittzeilen");
    assert_eq!(lage(&f, 'A'), Some((4000.0, false)));
    assert_eq!(lage(&f, 'B'), Some((5000.0, false)));
}

/// A166 (E19 §8): Die Unterzeile folgt dem Gebäude. Ohne Gebäude keine. Im
/// Rechteck 10 × 8 m ist A (parallel zur 10-m-Seite) der Längsschnitt, im
/// Rechteck 8 × 10 m der Querschnitt (B umgekehrt). Quadrat 8 × 8 m: keine
/// Unterzeile. Neu bewertet, wenn sich das Gebäude ändert (Rückgängig).
#[test]
fn a166_bezeichnung_aus_dem_gebaeude() {
    let mut f = fall(Scene::with_model(Model::with_seed(166)));
    ansicht_schnitt(&mut f);
    assert_eq!(
        rad(&f).0,
        ("A–A".to_string(), String::new()),
        "ohne Gebäude"
    );
    zeichne(&mut f.s, &RECHTECK);
    assert_eq!(rad(&f).0 .1, "Längsschnitt", "10 × 8: neu bewertet");
    assert!(f.s.undo());
    assert_eq!(rad(&f).0 .1, "", "Rückgängig: wieder ohne Gebäude");

    let hoch = [
        vec3(0.0, 0.0, 0.0),
        vec3(8000.0, 0.0, 0.0),
        vec3(8000.0, 10000.0, 0.0),
        vec3(0.0, 10000.0, 0.0),
    ];
    let mut s = Scene::with_model(Model::with_seed(1660));
    zeichne(&mut s, &hoch);
    let mut f = fall(s);
    ansicht_schnitt(&mut f);
    assert_eq!(
        rad(&f),
        r("A–A", "Querschnitt", None, Some("B–B")),
        "8 × 10"
    );
    spitze(&mut f, false);
    assert_eq!(rad(&f).0 .1, "Längsschnitt", "8 × 10: B");

    let quadrat = [
        vec3(0.0, 0.0, 0.0),
        vec3(8000.0, 0.0, 0.0),
        vec3(8000.0, 8000.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
    ];
    let mut s = Scene::with_model(Model::with_seed(1661));
    zeichne(&mut s, &quadrat);
    let mut f = fall(s);
    ansicht_schnitt(&mut f);
    assert_eq!(rad(&f), r("A–A", "", None, Some("B–B")), "Quadrat");
    assert_eq!(hinweis(&f, false), k("Schnitt B–B ↓", ""));
    spitze(&mut f, false);
    assert_eq!(rad(&f).0, ("B–B".to_string(), String::new()));
}

/// A167 (Koordinator 06:17, F-17): Eine fehlerhafte [cut]-Zeile (pos fehlt,
/// pos keine Zahl, flip ungültig, Schnitt unbekannt) gibt je einen Hinweis
/// mit ihrer Zeilennummer und wird übersprungen; die Datei öffnet trotzdem,
/// gültige Zeilen gelten, der übersprungene Schnitt liegt im Schnitt mittig.
#[test]
fn a167_fehlerhafte_schnittzeile() {
    let mut f = prueffall(167);
    spitze(&mut f, false);
    spiegeln(&mut f);
    let gut = speichern(&f);
    let zeile_b = gut
        .lines()
        .find(|l| l.starts_with("[cut]"))
        .expect("Schnittzeile B")
        .to_string();
    assert!(zeile_b.contains("name=B"), "{zeile_b}");
    let basis: String = gut
        .lines()
        .filter(|l| !l.starts_with("[cut]"))
        .map(|l| format!("{l}\n"))
        .collect();
    let n = basis.lines().count();
    for (schlecht, warum) in [
        ("[cut] name=A flip=0", "pos fehlt"),
        ("[cut] name=A pos=abc flip=0", "pos keine Zahl"),
        ("[cut] name=A pos=2500 flip=2", "flip ungültig"),
        ("[cut] name=C pos=2500 flip=0", "Schnitt unbekannt"),
    ] {
        let text = format!("{basis}{schlecht}\n{zeile_b}\n");
        let l = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1))
            .unwrap_or_else(|e| panic!("{warum}: öffnet nicht: {e:?}"));
        let zeile = format!("Zeile {}:", n + 1);
        assert_eq!(
            l.hints.iter().filter(|h| h.contains("[cut]")).count(),
            1,
            "{warum}: ein Hinweis {:?}",
            l.hints
        );
        assert!(
            l.hints
                .iter()
                .any(|h| h.starts_with(&zeile) && h.contains("[cut]")),
            "{warum}: {zeile} {:?}",
            l.hints
        );
        assert_eq!(
            l.model.cuts()[CUT_A],
            Cut::default(),
            "{warum}: A übersprungen"
        );
        assert_eq!(
            l.model.cuts()[CUT_B],
            Cut {
                pos: Some(5000.0),
                flip: true
            },
            "{warum}: B gilt"
        );
        let mut f2 = fall(Scene::with_model(l.model));
        ansicht_schnitt(&mut f2);
        assert_eq!(lage(&f2, 'A'), Some((4000.0, false)), "{warum}: A mittig");
    }
}

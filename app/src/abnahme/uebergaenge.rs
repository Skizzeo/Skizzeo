use super::*;

// ===== Adapter =====

use crate::scene::CATALOG_STEP;
use std::cell::Cell;

thread_local! {
    /// Uhr der Szene beim letzten Auslöser: jeder Auslöser setzt die Uhr
    /// vorwärts, so bleibt sie wie in der App stetig.
    static AUSLOESER: Cell<u64> = const { Cell::new(0) };
}

fn ausloesen(s: &mut Scene) {
    let t = AUSLOESER.with(|c| {
        c.set(c.get() + 1_000_000);
        c.get()
    });
    s.set_now(t);
}

/// Typ über den Weg der Bedienung ändern (Katalog OK → „Ändern“): ein
/// Rückgängig-Schritt, Übergang startet bei t = 0.
fn aendern(s: &mut Scene, id: sk_model::LayerSetId, t: sk_model::LayerSet) -> bool {
    ausloesen(s);
    s.edit_types(CATALOG_STEP, |m| m.set_layer_set(id, t))
}
/// Rückgängig über die Bedienung (Strg+Z), Übergang rückwärts ab t = 0.
fn rueckgaengig(s: &mut Scene) -> bool {
    ausloesen(s);
    s.undo()
}
/// Uhr des Übergangs: `t` ms nach dem letzten Auslöser.
fn uhr(s: &mut Scene, t: u64) {
    s.grow_tick(AUSLOESER.with(|c| c.get()) + t);
}
/// Gezeichnete x-Lage (mm) der Außen- und der Innenfläche der Westwand im
/// EG zum Zeitpunkt `t` ms, aus dem 3D-Netz, wie es hochgeladen wird.
fn westwand(s: &mut Scene, t: u64) -> (f64, f64) {
    uhr(s, t);
    let m = s.mesh(ViewKind::Persp, None, &[]);
    let (mut aussen, mut innen) = (f64::MAX, f64::MIN);
    for v in &m.faces {
        let (x, z) = (v[0] as f64, v[2] as f64);
        // Westwand im EG: Flächen links der Mitte bis UK EG-Decke
        if x > 2000.0 || !(-1.0..2700.0).contains(&z) {
            continue;
        }
        if v[3] < -0.99 {
            aussen = aussen.min(x);
        } else if v[3] > 0.99 {
            innen = innen.max(x);
        }
    }
    (aussen, innen)
}
/// Stärke des Leuchtens der betroffenen Wände zum Zeitpunkt `t` (0 … 1).
fn leuchten(s: &mut Scene, t: u64) -> f32 {
    uhr(s, t);
    s.grow_glow().0
}
/// Klick ins Modell während des Übergangs.
fn klick(s: &mut Scene, t: u64) {
    uhr(s, t);
    s.skip_animation();
}
/// `anim_ms` aus Einstellungen → Bedienoberfläche → „Animationen“.
fn animationen(s: &mut Scene, an: bool) {
    let mut t = s.theme().clone();
    t.size.anim_ms = if an { 280.0 } else { 0.0 };
    s.set_theme(&t);
}
/// 3D-Netz, wie gezeichnet (Endstand ohne Übergang zum Vergleich).
fn netz(s: &mut Scene) -> Vec<[f32; 3]> {
    let m = s.mesh(ViewKind::Persp, None, &[]);
    let mut v: Vec<[f32; 3]> = m.faces.iter().map(|p| [p[0], p[1], p[2]]).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

// ===== Hilfen =====

/// AW-31,5 mit 12 Dämmung + 24 Porenbeton (36 cm): wächst um 4,5 cm nach innen.
fn dicker(s: &Scene) -> (sk_model::LayerSetId, sk_model::LayerSet) {
    let id = s.model().defaults().exterior_wall;
    let mut t = s.model().layer_set(id).unwrap().clone();
    t.layers[0].thickness = 120.0;
    t.layers[1].thickness = 240.0;
    (id, t)
}

/// Kurve ease-out 1 − (1 − u)³.
fn kurve(u: f64) -> f64 {
    1.0 - (1.0 - u.clamp(0.0, 1.0)).powi(3)
}

fn nahe(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.5
}

/// A119 (K3b §1, Prüfpunkte 1–2): „Ändern“ an AW-31,5 (8 Wände) → 36 cm.
/// Die Innenfläche wandert in 280 ms ease-out von 315 auf 360 mm, die
/// Außenfläche bleibt zu jedem Zeitpunkt stehen. Ab 280 ms ist der Endstand
/// erreicht und gleich dem Netz ohne Übergang. Das Leuchten klingt in 600 ms
/// linear von 0,43 auf 0 ab. Mengen sind sofort die neuen (keine Zwischenwerte),
/// ein Rückgängig-Schritt.
#[test]
fn a119_waende_wachsen_nach_aendern() {
    let mut s = Scene::with_model(Model::with_seed(119));
    let (aw, iw) = haus_b11(&mut s);
    animationen(&mut s, true);
    let (aussen0, innen0) = westwand(&mut s, 0);
    assert!(nahe(innen0 - aussen0, 315.0));
    let (id, t) = dicker(&s);
    assert!(aendern(&mut s, id, t));
    assert_eq!(s.undo_label(), Some("Bauteilkatalog geändert"));
    assert_eq!(
        mengen_b11(&s, aw, iw)[1],
        21.5522,
        "Mengen sofort neu (Porenbeton netto 36 cm)"
    );
    for t in [0u64, 35, 70, 140, 210, 279, 280, 400] {
        let (a, i) = westwand(&mut s, t);
        assert!(nahe(a, aussen0), "Außenfläche bei {t} ms: {a}");
        let soll = aussen0 + 315.0 + 45.0 * kurve(t as f64 / 280.0);
        assert!(nahe(i, soll), "Innenfläche bei {t} ms: {i}, soll {soll}");
    }
    for (t, soll) in [(0u64, 0.43f32), (300, 0.215), (600, 0.0), (900, 0.0)] {
        assert!(
            (leuchten(&mut s, t) - soll).abs() < 0.01,
            "Leuchten bei {t} ms"
        );
    }
    let mit = netz(&mut s);
    let mut s2 = Scene::with_model(Model::with_seed(119));
    haus_b11(&mut s2);
    animationen(&mut s2, false);
    let (id2, t2) = dicker(&s2);
    assert!(aendern(&mut s2, id2, t2));
    assert_eq!(mit, netz(&mut s2), "Endstand gleich dem ohne Übergang");
}

/// A120 (K3b §1, Prüfpunkte 3 und 5): Strg+Z zeigt den Übergang rückwärts
/// (Innenfläche 360 → 315, Außenfläche fest), Endstand und Mengen wie
/// vorher. Ein Klick während des Übergangs springt sofort auf den Endstand.
#[test]
fn a120_rueckgaengig_rueckwaerts_und_klick_springt() {
    let mut s = Scene::with_model(Model::with_seed(120));
    let (aw, iw) = haus_b11(&mut s);
    animationen(&mut s, true);
    let vorher = netz(&mut s);
    let (aussen0, _) = westwand(&mut s, 0);
    let (id, t) = dicker(&s);
    assert!(aendern(&mut s, id, t));
    westwand(&mut s, 400);
    assert!(rueckgaengig(&mut s));
    for t in [0u64, 140, 280] {
        let (a, i) = westwand(&mut s, t);
        assert!(nahe(a, aussen0), "Außenfläche bei {t} ms");
        let soll = aussen0 + 360.0 - 45.0 * kurve(t as f64 / 280.0);
        assert!(nahe(i, soll), "rückwärts bei {t} ms: {i}, soll {soll}");
    }
    assert_eq!(mengen_b11(&s, aw, iw), STANDARD);
    assert_eq!(netz(&mut s), vorher, "Geometrie wie vorher");
    // Klick mitten im Übergang
    let (id, t) = dicker(&s);
    assert!(aendern(&mut s, id, t));
    westwand(&mut s, 70);
    klick(&mut s, 70);
    let (a, i) = westwand(&mut s, 71);
    assert!(
        nahe(a, aussen0) && nahe(i, aussen0 + 360.0),
        "sofort Endstand: {i}"
    );
    assert!(leuchten(&mut s, 71) < 0.01, "Leuchten aus");
}

/// A121 (K3b §3, Prüfpunkt 4): Mit anim_ms = 0 springt die Wand sofort auf
/// den Endstand, kein Leuchten; das Bild gleicht dem Ende mit Übergang.
#[test]
fn a121_ohne_animation_sofort() {
    let mut s = Scene::with_model(Model::with_seed(121));
    haus_b11(&mut s);
    animationen(&mut s, false);
    let (aussen0, _) = westwand(&mut s, 0);
    let (id, t) = dicker(&s);
    assert!(aendern(&mut s, id, t));
    let (a, i) = westwand(&mut s, 0);
    assert!(nahe(a, aussen0) && nahe(i, aussen0 + 360.0), "sofort: {i}");
    assert!(leuchten(&mut s, 0) < 0.01, "kein Leuchten");
    let ohne = netz(&mut s);
    let mut s2 = Scene::with_model(Model::with_seed(121));
    haus_b11(&mut s2);
    animationen(&mut s2, true);
    let (id2, t2) = dicker(&s2);
    assert!(aendern(&mut s2, id2, t2));
    westwand(&mut s2, 400);
    assert_eq!(netz(&mut s2), ohne);
}

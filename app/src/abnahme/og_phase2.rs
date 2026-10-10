use super::*;
// Fassung 4 (07.10. 04:50): A156 Stufe beim Vorsprung bei UK UD +2,515 (OG-17,
// Außenschichten bis UK UD), Adapternamen wie im Bau (facing_support,
// set_floor_soffit). Ersetzt a145-a157-og-phase2-v3.rs.
// Fassung 3: feste Wandmengen von BIM 04:37, A159 für AW-49.
//
// Abnahmetests A145–A157 und A159: OG Phase 2, Geschosse getrennt bearbeiten
// (Kettensymbol je Segment). Jörn 07.10. 03:46 „OG Phase 2“, 04:12 „Baue es
// gemäß deinen Empfehlungen“ (Erker später, Rücksprungstreifen nur Darstellung).
// Grundlage: bim/paket-og-phase2.md (Regeln 28–35, „Fertig, wenn“),
// geometrie/paket-g7-og-phase2.md, einstellungen/paket-og2-gestaltung.md
// (soll-og2-1…3). Spezifikation: test/abnahme-og-phase2.md.
//
// Einbau: als `mod og_phase2 { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, decke, decke_mengen,
// schale, ziehen_am_fuss, view_mesh, ViewKind, WallEdit, cam3d, px, m2, m3,
// W, H, EG_SCHALE.
// Dazu ändert `a052-og-fuss-zieht-den-stapel.patch` den Schluss von A52: Am
// gekoppelten OG-Wandfuß gibt es jetzt einen Griff (er zieht den Stapel).
//
// Prüfhaus wie A51: Dialog, Rechteck 10 × 8 m, Standardhöhen. Die OG-Wand bei
// y = 8 ist AW-006, ihr Partner AW-002 (BIM „Fertig, wenn“).
//
// Angenommene Namen stehen nur in den Adaptern (Vorschlag aus BIM §3, frei
// wählbar): `Model::{set_linked, set_flush, move_segment, stack_offset,
// soffit_of, set_floor_soffit}`, `Scene::soffit_qto`,
// `crate::schedule_view::offset_note`, für die Mengenzeile „Abfangung
// Verblender“ `WallQto::facing_support` (Länge in mm je Wand).

use sk_model::ElementId;

// ===== Adapter =====

/// Kette am Segment öffnen (`false`) oder schließen (`true`): ein Schritt.
/// `false`, wenn die Wand nicht gestapelt ist (EG, Innenwand).
fn kette(s: &mut Scene, wand: ElementId, gekoppelt: bool) -> bool {
    let label = if gekoppelt {
        "Wand gekoppelt"
    } else {
        "Kopplung gelöst"
    };
    s.edit_model(label, |m| m.set_linked(wand, gekoppelt))
}

/// „Bündig setzen“ (Hinweis oder Paneel): Versatz 0 und gekoppelt.
fn buendig(s: &mut Scene, wand: ElementId) -> bool {
    s.edit_model("Bündig gesetzt", |m| m.set_flush(wand))
}

/// Versatz um `d` mm ändern (+ außen), wie Eintippen im Paneel oder Ziehen
/// am gelösten OG-Wandfuß; der Zustand bleibt.
fn versetzen(s: &mut Scene, wand: ElementId, d: f64) -> bool {
    s.edit_model("Wand verschoben", |m| m.move_segment(wand, d).is_some())
}

/// (Versatz in mm auf 0,01 gerundet, gekoppelt?) bzw. `None`, wenn nicht
/// gestapelt.
fn versatz(s: &Scene, wand: ElementId) -> Option<(f64, bool)> {
    s.model()
        .stack_offset(wand)
        .map(|(o, l)| ((o * 100.0).round() / 100.0, l))
}

/// Untersichtdämmung unter der Decke des Zugs `run`: (Nummer, m², m³).
fn ud(s: &Scene, run: RunId) -> Option<(String, f64, f64)> {
    let de = decke(s, run)?;
    let id = s.model().soffit_of(de)?;
    let q = s.soffit_qto(id).expect("Mengen der UD");
    Some((nummer(s, id), m2(q.area), m3(q.volume)))
}

fn ud_id(s: &Scene, run: RunId) -> Option<ElementId> {
    s.model().soffit_of(decke(s, run)?)
}

/// Dicke der Untersichtdämmung an der Decke des Zugs `run` (mm), ein Schritt.
fn ud_dicke(s: &mut Scene, run: RunId, mm: f64) -> bool {
    let de = decke(s, run).unwrap();
    s.edit_model("Untersichtdämmung", |m| m.set_floor_soffit(de, mm))
}

/// Mengenzeile „Abfangung Verblender“ einer Wand in mm (0 ohne Verblender).
fn abfangung(s: &Scene, wand: ElementId) -> f64 {
    s.wall_qto(wand).expect("Wandmengen").facing_support
}

/// Zusatz in der Zeile der Wand im Mengenfenster, z. B. „Versatz +0,30 m“.
fn versatz_text(s: &Scene, wand: ElementId) -> Option<String> {
    crate::schedule_view::offset_note(s.model(), wand)
}

// ===== Hilfen =====

fn prueffall(seed: u64) -> (Scene, RunId, RunId) {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let (eg, og) = gebaeude(&mut s);
    (s, eg, og)
}

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

fn nummer(s: &Scene, id: ElementId) -> String {
    s.model().element(id).unwrap().number.clone()
}

/// Abfangung Verblender aller Wände eines Zugs in m (3 Stellen).
fn abfangung_m(s: &Scene, run: RunId) -> f64 {
    let v: f64 = s
        .model()
        .run(run)
        .unwrap()
        .segments
        .iter()
        .map(|w| abfangung(s, *w))
        .sum();
    v.round() / 1e3
}

/// Volumen eines Baustoffs in allen Wänden eines Zugs (m³, 4 Stellen).
fn stoff(s: &Scene, run: RunId, name: &str) -> f64 {
    let m = s.model();
    let mut v = 0.0;
    for w in &m.run(run).unwrap().segments {
        for l in &s.wall_qto(*w).unwrap().layers {
            if m.material(l.material).unwrap().name == name {
                v += l.volume;
            }
        }
    }
    r4(v / 1e9)
}

fn pruefung(s: &Scene) {
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
}

fn guids(s: &Scene) -> Vec<sk_model::Guid> {
    let mut g: Vec<_> = s.model().elements().iter().map(|(_, e)| e.guid).collect();
    g.sort();
    g
}

fn punkte(s: &Scene, run: RunId) -> Vec<Vec3> {
    s.model().run(run).unwrap().points.clone()
}

fn mods(ctrl: bool) -> Modifiers {
    Modifiers {
        ctrl,
        ..Modifiers::default()
    }
}

/// Gummiband am Wandfuß bei (2,5 m | y0 | z) um dy ziehen, wahlweise mit Strg.
fn fuss_ziehen(s: &mut Scene, y0: f64, z: f64, dy: f64, strg: bool) {
    let c = cam3d();
    let m = mods(strg);
    let mut e = WallEdit::default();
    let (x, y) = px(&c, vec3(2500.0, y0, z));
    e.handle(&Event::MouseMove { x, y, mods: m }, s, &c, W, H, 1.0, true);
    e.handle(
        &Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: m,
        },
        s,
        &c,
        W,
        H,
        1.0,
        true,
    );
    let (x2, y2) = px(&c, vec3(2500.0, y0 + dy, z));
    e.handle(
        &Event::MouseMove {
            x: x2,
            y: y2,
            mods: m,
        },
        s,
        &c,
        W,
        H,
        1.0,
        true,
    );
    e.handle(
        &Event::MouseUp {
            button: MouseButton::Left,
            x: x2,
            y: y2,
            mods: m,
        },
        s,
        &c,
        W,
        H,
        1.0,
        true,
    );
}

/// EG-Wand bei y = 8 über das Modell um dy verschieben (ohne Raster der
/// Maus), wie das Gummiband: ein Schritt „Wand verschoben“.
fn eg_nord(s: &mut Scene, eg: RunId, dy: f64) -> bool {
    let p: Vec<Vec3> = punkte(s, eg)
        .into_iter()
        .map(|p| {
            if (p.y - 8000.0).abs() < 1.0 {
                vec3(p.x, p.y + dy, p.z)
            } else {
                p
            }
        })
        .collect();
    s.edit_model("Wand verschoben", |m| m.set_run_points(eg, &p).is_some())
}

/// Mengen im Überblick: DE-001, DE-002 (m², m³), Schale EG und OG.
type Stand = ((f64, f64), (f64, f64), (f64, f64), (f64, f64));
fn stand(s: &Scene, eg: RunId, og: RunId) -> Stand {
    let d = |r| {
        let q = decke_mengen(s, r);
        (q.0, q.1)
    };
    (d(eg), d(og), schale(s, eg), schale(s, og))
}

const BUENDIG: Stand = (
    (75.0384, 16.5084),
    (75.0384, 16.5084),
    EG_SCHALE,
    (15.7613, 14.1654),
);

// ===== Tests =====

/// A145 (BIM §3, „Fertig, wenn: Lösen“, Regel 28): Kette an AW-006 öffnen
/// ändert keine Menge, keine Nummer, keine Guid; ein Schritt „Kopplung
/// gelöst“. Schließen: „Wand gekoppelt“. EG- und Innenwände haben keine
/// Kette.
#[test]
fn a145_kette_loesen_und_schliessen() {
    let (mut s, eg, og) = prueffall(145);
    let aw6 = nr(&s, "AW-006");
    assert_eq!(versatz(&s, aw6), Some((0.0, true)), "neu: gekoppelt");
    assert_eq!(versatz(&s, nr(&s, "AW-002")), None, "EG nicht gestapelt");
    let vorher = guids(&s);
    assert!(kette(&mut s, aw6, false));
    assert_eq!(s.undo_label(), Some("Kopplung gelöst"));
    assert_eq!(versatz(&s, aw6), Some((0.0, false)));
    assert_eq!(stand(&s, eg, og), BUENDIG);
    assert_eq!(guids(&s), vorher);
    assert_eq!(nummer(&s, aw6), "AW-006");
    assert_eq!(punkte(&s, eg), punkte(&s, og), "Lage unverändert");
    pruefung(&s);
    assert!(kette(&mut s, aw6, true));
    assert_eq!(s.undo_label(), Some("Wand gekoppelt"));
    assert_eq!(versatz(&s, aw6), Some((0.0, true)));
    // EG-Wand: keine Kette, kein Schritt
    let aw2 = nr(&s, "AW-002");
    assert!(!kette(&mut s, aw2, false));
    assert_eq!(s.undo_label(), Some("Wand gekoppelt"), "kein neuer Schritt");
}

/// A146 (BIM „Fertig, wenn: Rücksprung“, G7 §4): Gelöstes AW-006 um 0,30 m
/// nach innen. DE-002 72,1224 m² / 15,8669 m³, OG-Porenbeton netto 15,4846 m³,
/// Dämmung OG 13,9255 m³; DE-001 und EG-Porenbeton unverändert; keine UD.
/// Seit der Dachterrasse (Paket 2b, Regel 45) läuft das EG-WDVS am
/// Rücksprung als Attika bis +3,055: EG-Dämmung 14,4543 statt 14,1654.
#[test]
fn a146_ruecksprung() {
    let (mut s, eg, og) = prueffall(146);
    let aw6 = nr(&s, "AW-006");
    kette(&mut s, aw6, false);
    assert!(versetzen(&mut s, aw6, -300.0));
    assert_eq!(s.undo_label(), Some("Wand verschoben"));
    assert_eq!(versatz(&s, aw6), Some((-300.0, false)));
    assert_eq!(
        stand(&s, eg, og),
        (
            (75.0384, 16.5084),
            (72.1224, 15.8669),
            (15.7613, 14.4543),
            (15.4846, 13.9255)
        )
    );
    assert_eq!(ud(&s, eg), None, "Rücksprung: keine Untersichtdämmung");
    assert_eq!(versatz_text(&s, aw6).as_deref(), Some("Versatz −0,30 m"));
    assert_eq!(versatz_text(&s, nr(&s, "AW-005")), None, "Versatz 0: leer");
    pruefung(&s);
}

/// A147 (BIM „Fertig, wenn: Vorsprung“, §4, Regeln 32 und 35): Gelöstes
/// AW-006 um 0,30 m nach außen. DE-002 77,9544 / 17,1500, OG-Porenbeton
/// 16,0379 m³, DE-001 kragt mit aus (77,9544 / 17,1500). UD-001 unter dem
/// ganzen Kragstreifen von Kern EG bis Kern OG (BIM 04:25): 2,9160 m², bei
/// 12 cm 0,3499 m³, bei 20 cm 0,5832 m³ (ein Schritt). Wandmengen (BIM
/// 04:37): EG-Porenbeton 15,7613, EG-Dämmung 13,6960 (endet an UK UD), OG-
/// Dämmung 14,9031 (reicht bis UK UD), dazu 0,0966 vor der Bekleidung
/// (70 mm tiefer, Jörn 14:03). Ohne Verblender keine Abfangung.
/// Dicke nur 40–300 mm. Direkt löschen wird abgelehnt.
#[test]
fn a147_vorsprung_mit_untersichtdaemmung() {
    let (mut s, eg, og) = prueffall(147);
    let aw6 = nr(&s, "AW-006");
    kette(&mut s, aw6, false);
    assert!(versetzen(&mut s, aw6, 300.0));
    assert_eq!(versatz(&s, aw6), Some((300.0, false)));
    assert_eq!(decke_mengen(&s, og).0, 77.9544, "DE-002");
    assert_eq!(decke_mengen(&s, og).1, 17.15);
    assert_eq!(decke_mengen(&s, eg).0, 77.9544, "DE-001 kragt aus");
    assert_eq!(decke_mengen(&s, eg).1, 17.15);
    assert_eq!(schale(&s, og), (16.0379, 14.9997), "OG: Dämmung bis UK UD");
    assert_eq!(
        schale(&s, eg),
        (15.7613, 13.696),
        "EG: Dämmung endet an UK UD"
    );
    assert_eq!(abfangung_m(&s, og), 0.0, "ohne Verblender keine Abfangung");
    assert_eq!(
        ud(&s, eg),
        Some(("UD-001".to_string(), 2.916, 0.3499)),
        "UD unter DE-001"
    );
    assert_eq!(ud(&s, og), None, "keine UD unter DE-002");
    assert_eq!(versatz_text(&s, aw6).as_deref(), Some("Versatz +0,30 m"));
    pruefung(&s);
    // Dicke 20 cm: ein Schritt, Fläche bleibt
    assert!(ud_dicke(&mut s, eg, 200.0));
    assert_eq!(ud(&s, eg), Some(("UD-001".to_string(), 2.916, 0.5832)));
    assert!(s.undo());
    assert_eq!(ud(&s, eg), Some(("UD-001".to_string(), 2.916, 0.3499)));
    // Grenzen 40–300 mm
    assert!(!ud_dicke(&mut s, eg, 39.0));
    assert!(!ud_dicke(&mut s, eg, 301.0));
    assert!(ud_dicke(&mut s, eg, 40.0));
    assert!(ud_dicke(&mut s, eg, 300.0));
    s.undo();
    s.undo();
    // Direkt löschen: abgelehnt mit dem Satz aus §4
    let id = ud_id(&s, eg).unwrap();
    let r = s
        .model()
        .can_delete(id)
        .expect_err("UD nicht direkt löschbar");
    assert_eq!(
            sk_model::refusal_text(s.model(), id, &r),
            "Die Untersichtdämmung folgt dem Vorsprung des Geschosses darüber; ihre Dicke steht bei der Decke."
        );
    // Gelöste OG-Außenwand bleibt Teil des Umrisses: nicht löschbar
    assert!(s.model().can_delete(aw6).is_err());
    // UD entsteht und verschwindet mit dem Schritt des Ziehens
    assert!(s.undo(), "Vorsprung zurück");
    assert_eq!(ud(&s, eg), None);
    assert_eq!(decke_mengen(&s, eg).0, 75.0384);
    assert_eq!(schale(&s, eg), EG_SCHALE, "EG-Dämmung wieder voll");
}

/// A148 (BIM „Fertig, wenn“, Regel 35, Regel 25): Bei +0,10 m kragt DE-001
/// aus (76,0104 m² / 16,7223 m³), UD-001 0,9720 m² / 0,1166 m³ (BIM 04:25:
/// auch kleine Vorsprünge haben eine UD); EG-Dämmung 13,6960, OG-Dämmung
/// 14,7242 (BIM 04:37). Auf +0,30 bleibt es UD-001 mit
/// derselben Guid. Bündig setzen: DE-001 wieder 75,0384, UD weg. Springt das
/// Segment erneut 0,30 m vor, entsteht UD-002 mit neuer Guid.
#[test]
fn a148_kleiner_vorsprung_und_neue_ud() {
    let (mut s, eg, og) = prueffall(148);
    let aw6 = nr(&s, "AW-006");
    kette(&mut s, aw6, false);
    assert!(versetzen(&mut s, aw6, 100.0));
    assert_eq!(
        decke_mengen(&s, eg).0,
        76.0104,
        "DE-001 kragt auch hier aus"
    );
    assert_eq!(decke_mengen(&s, eg).1, 16.7223);
    assert_eq!(schale(&s, eg), (15.7613, 13.696), "EG-Dämmung bis UK UD");
    assert_eq!(schale(&s, og).1, 14.7242, "OG-Dämmung bis UK UD");
    assert_eq!(ud(&s, eg), Some(("UD-001".to_string(), 0.972, 0.1166)));
    pruefung(&s);
    let g1 = s.model().element(ud_id(&s, eg).unwrap()).unwrap().guid;
    assert!(versetzen(&mut s, aw6, 200.0), "auf +0,30");
    assert_eq!(ud(&s, eg), Some(("UD-001".to_string(), 2.916, 0.3499)));
    assert_eq!(
        s.model().element(ud_id(&s, eg).unwrap()).unwrap().guid,
        g1,
        "dieselbe UD wächst mit"
    );
    assert!(buendig(&mut s, aw6));
    assert_eq!(s.undo_label(), Some("Bündig gesetzt"));
    assert_eq!(
        versatz(&s, aw6),
        Some((0.0, true)),
        "bündig koppelt zugleich"
    );
    assert_eq!(stand(&s, eg, og), BUENDIG);
    assert_eq!(ud(&s, eg), None);
    kette(&mut s, aw6, false);
    assert!(versetzen(&mut s, aw6, 300.0));
    let (n, a, v) = ud(&s, eg).expect("neue UD");
    assert_eq!((n.as_str(), a, v), ("UD-002", 2.916, 0.3499));
    assert_ne!(s.model().element(ud_id(&s, eg).unwrap()).unwrap().guid, g1);
    pruefung(&s);
}

/// A149 (BIM „Fertig, wenn: EG zieht, OG bleibt“, §2): Rücksprung −0,30
/// gelöst, dann EG-Wandfuß AW-002 um +1,00 m nach außen: DE-001 84,7584 m²,
/// das OG bleibt genau gleich, Versatz (−1300, gelöst), OG-Seitenwände
/// (gekoppelt) bleiben an x = 0 und x = 10.
#[test]
fn a149_eg_zieht_og_bleibt() {
    let (mut s, eg, og) = prueffall(149);
    let aw6 = nr(&s, "AW-006");
    kette(&mut s, aw6, false);
    versetzen(&mut s, aw6, -300.0);
    let og_vorher = punkte(&s, og);
    ziehen_am_fuss(&mut s, 0.0, 1000.0);
    assert_eq!(decke_mengen(&s, eg).0, 84.7584, "DE-001");
    assert_eq!(decke_mengen(&s, og).0, 72.1224, "DE-002 gleich");
    assert_eq!(schale(&s, og), (15.4846, 13.9255), "OG gleich");
    assert_eq!(versatz(&s, aw6), Some((-1300.0, false)));
    assert_eq!(punkte(&s, og), og_vorher, "OG-Zug unverändert");
    assert!(punkte(&s, og)
        .iter()
        .all(|p| p.x.abs() < 1e-6 || (p.x - 10000.0).abs() < 1e-6));
    for w in ["AW-005", "AW-007", "AW-008"] {
        assert!(versatz(&s, nr(&s, w)).unwrap().1, "{w} bleibt gekoppelt");
    }
    pruefung(&s);
    assert!(s.undo(), "ein Schritt");
    assert_eq!(versatz(&s, aw6), Some((-300.0, false)));
    assert_eq!(decke_mengen(&s, eg).0, 75.0384);
}

/// A150 (BIM „Fertig, wenn: Wieder koppeln“, OG-16): Kette schließen, der
/// Versatz −1300 bleibt. EG-Wandfuß −1,00 m: AW-006 geht mit, DE-002 danach
/// 62,4024 m² (OG-Rechteck 10 × 6,70 m, Kern 9,72 × 6,42). BIM nannte
/// 61,6224 mit dem Vermerk „Test prüft nach“; das passt nicht zu 10 × 6,70.
#[test]
fn a150_wieder_koppeln_versatz_bleibt() {
    let (mut s, eg, og) = prueffall(150);
    let aw6 = nr(&s, "AW-006");
    kette(&mut s, aw6, false);
    versetzen(&mut s, aw6, -300.0);
    ziehen_am_fuss(&mut s, 0.0, 1000.0);
    assert!(kette(&mut s, aw6, true));
    assert_eq!(versatz(&s, aw6), Some((-1300.0, true)), "Versatz bleibt");
    assert_eq!(decke_mengen(&s, og).0, 72.1224, "Koppeln bewegt nichts");
    fuss_ziehen(&mut s, 9000.0, 0.0, -1000.0, false);
    assert_eq!(decke_mengen(&s, eg).0, 75.0384, "DE-001");
    assert_eq!(decke_mengen(&s, og).0, 62.4024, "DE-002 geht mit");
    assert_eq!(versatz(&s, aw6), Some((-1300.0, true)));
    pruefung(&s);
}

/// A151 (Regel 31): Versatz 0 oder mindestens 20 mm. Beim eigenen Versetzen
/// rastet alles unter 20 mm auf 0. Seit Review 1t (T1) gilt das auch, wenn
/// ein gelöstes Segment dem EG folgt: Fällt der Abstand unter 20 mm, fängt
/// die EG-Linie das Segment bündig (mehr in A160).
#[test]
fn a151_raster_2_cm() {
    let (mut s, eg, _) = prueffall(151);
    let aw6 = nr(&s, "AW-006");
    kette(&mut s, aw6, false);
    for d in [10.0, 19.0, -10.0, -19.0] {
        versetzen(&mut s, aw6, d);
        assert_eq!(versatz(&s, aw6), Some((0.0, false)), "{d} mm rastet auf 0");
    }
    versetzen(&mut s, aw6, 20.0);
    assert_eq!(versatz(&s, aw6), Some((20.0, false)));
    versetzen(&mut s, aw6, -10.0);
    assert_eq!(versatz(&s, aw6), Some((0.0, false)), "10 mm rastet auf 0");
    // EG folgt: gemessen wären 15 mm, die EG-Linie fängt bündig
    assert!(eg_nord(&mut s, eg, -15.0));
    assert_eq!(versatz(&s, aw6), Some((0.0, false)));
    pruefung(&s);
}

/// A152 (BIM §3, Gestaltung §1): Ziehen am OG-Wandfuß.
/// - gelöst: nur die OG-Wand, EG bleibt
/// - gekoppelt mit Strg: nur diese Wand, sie bleibt gekoppelt; zieht man
///   danach das EG, gehen beide mit
/// - gekoppelt ohne Strg: wie am EG-Band, der ganze Stapel
#[test]
fn a152_ziehen_am_og_wandfuss() {
    // gelöst
    let (mut s, eg, og) = prueffall(152);
    let aw6 = nr(&s, "AW-006");
    kette(&mut s, aw6, false);
    fuss_ziehen(&mut s, 8000.0, 2855.0, 300.0, false);
    assert_eq!(versatz(&s, aw6), Some((300.0, false)));
    assert_eq!(decke_mengen(&s, og).0, 77.9544);
    assert_eq!(schale(&s, eg).0, EG_SCHALE.0, "EG bleibt");
    assert_eq!(s.undo_label(), Some("Wand verschoben"));

    // gekoppelt mit Strg
    let (mut s, eg, og) = prueffall(1520);
    let aw6 = nr(&s, "AW-006");
    fuss_ziehen(&mut s, 8000.0, 2855.0, -300.0, true);
    assert_eq!(versatz(&s, aw6), Some((-300.0, true)), "bleibt gekoppelt");
    assert_eq!(decke_mengen(&s, og).0, 72.1224);
    assert_eq!(decke_mengen(&s, eg).0, 75.0384);
    ziehen_am_fuss(&mut s, 0.0, 1000.0);
    assert_eq!(decke_mengen(&s, eg).0, 84.7584, "EG zieht");
    assert_eq!(versatz(&s, aw6), Some((-300.0, true)), "OG geht mit");
    assert_eq!(decke_mengen(&s, og).0, 81.8424, "OG 10 × 8,70 − 0,30");

    // gekoppelt ohne Strg: der ganze Stapel
    let (mut s, eg, og) = prueffall(1521);
    fuss_ziehen(&mut s, 8000.0, 2855.0, 1000.0, false);
    assert_eq!(decke_mengen(&s, eg).0, 84.7584, "EG mitgezogen");
    assert_eq!(decke_mengen(&s, og).0, 84.7584);
    assert_eq!(punkte(&s, eg), punkte(&s, og));
    assert_eq!(versatz(&s, nr(&s, "AW-006")), Some((0.0, true)));
    pruefung(&s);
}

/// A153 (BIM §2, Regel 28, „Fertig, wenn: Teilen“): Punkt im EG-Segment
/// AW-002 einfügen, während AW-006 gelöst um −0,30 steht: beide OG-Hälften
/// gelöst mit demselben Versatz, die neue Hälfte bekommt eine neue Nummer,
/// OG hat so viele Segmente wie EG.
#[test]
fn a153_teilen_vererbt_den_zustand() {
    let (mut s, eg, og) = prueffall(153);
    let aw6 = nr(&s, "AW-006");
    kette(&mut s, aw6, false);
    versetzen(&mut s, aw6, -300.0);
    let mut p = punkte(&s, eg);
    let i = (0..p.len())
        .find(|&i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            (a.y - 8000.0).abs() < 1.0 && (b.y - 8000.0).abs() < 1.0
        })
        .expect("Segment bei y = 8");
    p.insert(i + 1, vec3(5000.0, 8000.0, 0.0));
    assert!(s.edit_model("Punkt eingefügt", |m| m.set_run_points(eg, &p).is_some()));
    let seg_eg = s.model().run(eg).unwrap().segments.len();
    let seg_og = s.model().run(og).unwrap().segments.clone();
    assert_eq!(seg_eg, 5);
    assert_eq!(seg_og.len(), 5, "Regel 28");
    let geloest: Vec<_> = seg_og
        .iter()
        .filter_map(|w| versatz(&s, *w))
        .filter(|v| !v.1)
        .collect();
    assert_eq!(geloest, vec![(-300.0, false); 2], "beide Hälften gelöst");
    assert!(seg_og.contains(&aw6), "AW-006 behält seine Kennung");
    let neu: Vec<String> = seg_og
        .iter()
        .filter(|w| !versatz(&s, **w).unwrap().1 && **w != aw6)
        .map(|w| nummer(&s, *w))
        .collect();
    assert_eq!(neu.len(), 1);
    assert!(
        !["AW-001", "AW-002", "AW-003", "AW-004", "AW-005", "AW-006", "AW-007", "AW-008"]
            .contains(&neu[0].as_str())
    );
    assert_eq!(decke_mengen(&s, og).0, 72.1224, "OG-Umriss gleich");
    pruefung(&s);
}

/// A154 (BIM „Fertig, wenn: Rückgängig“): Lösen, Versetzen, EG ziehen,
/// Koppeln, Bündig setzen sind je genau ein Schritt; Rückgängig stellt
/// Versatz, Zustand und Guids wieder her, Wiederherstellen dieselben Guids.
#[test]
fn a154_jeder_schritt_einzeln_rueckgaengig() {
    let (mut s, eg, og) = prueffall(154);
    let aw6 = nr(&s, "AW-006");
    let mut stufen = vec![(versatz(&s, aw6), guids(&s), stand(&s, eg, og))];
    let schritte: [&dyn Fn(&mut Scene); 5] = [
        &|s| assert!(kette(s, aw6, false)),
        &|s| assert!(versetzen(s, aw6, 300.0)),
        &|s| ziehen_am_fuss(s, 0.0, 1000.0),
        &|s| assert!(kette(s, aw6, true)),
        &|s| assert!(buendig(s, aw6)),
    ];
    for f in schritte {
        f(&mut s);
        stufen.push((versatz(&s, aw6), guids(&s), stand(&s, eg, og)));
    }
    for k in (0..5).rev() {
        assert!(s.undo(), "Schritt {k}");
        assert_eq!(
            (versatz(&s, aw6), guids(&s), stand(&s, eg, og)),
            stufen[k],
            "nach Rückgängig {k}"
        );
    }
    for st in &stufen[1..] {
        assert!(s.redo());
        assert_eq!(&(versatz(&s, aw6), guids(&s), stand(&s, eg, og)), st);
    }
}

/// A155 (BIM §5): Datei bleibt Version 4. Ohne gelöste Segmente kein
/// `link=` und kein `[soffit]`; mit gelöstem Segment `link=0` nur in dessen
/// Zeile, speichern, öffnen, speichern bytegleich (auch mit UD-001 und
/// Dicke 200). `link=2` wird mit Zeilennummer abgelehnt.
#[test]
fn a155_datei_link_und_soffit() {
    let (mut s, eg, _) = prueffall(155);
    let t = sk_model::szo::write(s.model());
    assert!(t.starts_with("SZO 4"), "Version 4");
    assert!(!t.contains("link=") && !t.contains("[soffit]") && !t.contains("soffit="));
    let aw6 = nr(&s, "AW-006");
    kette(&mut s, aw6, false);
    versetzen(&mut s, aw6, 300.0);
    ud_dicke(&mut s, eg, 200.0);
    let t = sk_model::szo::write(s.model());
    let mit: Vec<&str> = t.lines().filter(|l| l.contains("link=")).collect();
    assert_eq!(mit.len(), 1, "nur das gelöste Segment");
    assert!(mit[0].contains("number=\"AW-006\"") && mit[0].contains("link=0"));
    let off = mit[0].find("off=").expect("off=");
    assert!(off < mit[0].find("link=").unwrap(), "link nach off");
    let sof: Vec<&str> = t.lines().filter(|l| l.starts_with("[soffit]")).collect();
    assert_eq!(sof.len(), 1);
    assert!(sof[0].contains("number=\"UD-001\""));
    assert!(t
        .lines()
        .any(|l| l.starts_with("[floor]") && l.contains("soffit=200")));
    let l = sk_model::szo::read(&t, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    assert_eq!(sk_model::szo::write(&l.model), t, "bytegleich");
    let s2 = Scene::with_model(l.model);
    assert_eq!(versatz(&s2, nr(&s2, "AW-006")), Some((300.0, false)));
    assert_eq!(ud(&s2, eg).map(|u| u.0), Some("UD-001".to_string()));
    // link=2
    let k = t.lines().position(|l| l.contains("link=0")).unwrap() + 1;
    let kaputt = t.replace("link=0", "link=2");
    let e = sk_model::szo::read(&kaputt, sk_model::GuidGen::with_seed(1))
        .err()
        .expect("link=2 abgelehnt");
    assert!(e.to_string().starts_with(&format!("Zeile {k}:")), "{e}");
}

/// A156 (BIM „Fertig, wenn: Ansicht“, §4, Gestaltung §1): Bei Versatz ≠ 0
/// zeigt die Ansicht Hinten (Nordfassade) eine waagerechte Stufenkante über
/// die Breite. Beim Rücksprung liegt sie an OK EG +2,855. Beim Vorsprung
/// laufen die OG-Außenschichten bis UK Untersichtdämmung herab (OG-17), die
/// Stufe liegt dann bei +2,515 (Kanten bei y 8000–8300). Gelöst, aber
/// bündig: keine Kante.
#[test]
fn a156_stufenkante_nur_bei_versatz() {
    let kante = |s: &mut Scene, z: f32, y0: f32, y1: f32| {
        let m = view_mesh(s, ViewKind::Back, None);
        m.edges
            .iter()
            .filter(|e| (e.0[0][2] - z).abs() < 1e-2 && (e.0[1][2] - z).abs() < 1e-2)
            .filter(|e| {
                e.0[0][1] > y0 - 1.0
                    && e.0[0][1] < y1 + 1.0
                    && e.0[0][0].min(e.0[1][0]) < 4000.0
                    && e.0[0][0].max(e.0[1][0]) > 6000.0
            })
            .count()
    };
    let (mut s, _, _) = prueffall(156);
    let aw6 = nr(&s, "AW-006");
    kette(&mut s, aw6, false);
    assert_eq!(
        kante(&mut s, 2855.0, 7800.0, 8200.0),
        0,
        "gelöst, aber bündig: keine Naht"
    );
    assert_eq!(
        kante(&mut s, 2515.0, 7800.0, 8400.0),
        0,
        "bündig: keine UD-Kante"
    );
    versetzen(&mut s, aw6, 300.0);
    assert!(
        kante(&mut s, 2515.0, 8000.0, 8300.0) > 0,
        "Vorsprung: Stufe an UK UD +2,515"
    );
    s.undo();
    versetzen(&mut s, aw6, -300.0);
    assert!(
        kante(&mut s, 2855.0, 7600.0, 8000.0) > 0,
        "Rücksprung: Stufe bei +2,855"
    );
}

/// A157 (Regeln 28–35): Prüfliste ohne Warnung in allen Fällen oben, auch
/// mit zwei gelösten Segmenten (AW-006 vor, AW-007 zurück) und nach
/// Speichern/Öffnen. Ein Umriss, der sich selbst schneiden würde (AW-006 um
/// 8,5 m nach innen), wird abgelehnt; der letzte gültige Stand bleibt.
#[test]
fn a157_pruefliste_und_ungueltige_lage() {
    let (mut s, _, og) = prueffall(157);
    let aw6 = nr(&s, "AW-006");
    let aw7 = nr(&s, "AW-007");
    kette(&mut s, aw6, false);
    kette(&mut s, aw7, false);
    versetzen(&mut s, aw6, 300.0);
    versetzen(&mut s, aw7, -300.0);
    pruefung(&s);
    let t = sk_model::szo::write(s.model());
    let l = sk_model::szo::read(&t, sk_model::GuidGen::with_seed(1)).unwrap();
    assert!(l.model.check().is_empty(), "{:?}", l.model.check());
    let vorher = punkte(&s, og);
    assert!(!versetzen(&mut s, aw6, -8500.0), "Umriss ungültig");
    assert_eq!(punkte(&s, og), vorher, "letzter gültiger Stand");
    assert_eq!(versatz(&s, aw6), Some((300.0, false)));
    pruefung(&s);
}

/// A159 (BIM „Fertig, wenn: AW-49“, §4, Regel 33): Dasselbe Prüfhaus mit
/// AW-49 (Verblender, Luftschicht, Kerndämmung, Porenbeton), AW-006 gelöst um
/// +0,30. EG: Kerndämmung 13,1531, Verblender 11,2822 m³ (enden an UK UD).
/// OG: Kerndämmung 14,3268, Verblender 12,2756 m³ (bis UK UD). UD 2,8110 m² /
/// 0,3373 m³ (Kragstreifen 9,37 × 0,30). Abfangung Verblender 10,485 m an
/// den OG-Wänden (Nord 9,885, West und Ost je 0,300). Bündig: keine
/// Abfangung mehr.
#[test]
fn a159_vorsprung_mit_verblender() {
    let (mut s, eg, og) = prueffall(159);
    let t = s
        .model()
        .layer_sets()
        .iter()
        .find(|(_, t)| t.code == "AW-49")
        .map(|(id, _)| id)
        .expect("Werkstyp AW-49");
    assert!(s.edit_model("Wandtyp geändert", |m| m.set_run_type(eg, t)));
    let aw6 = nr(&s, "AW-006");
    kette(&mut s, aw6, false);
    assert!(versetzen(&mut s, aw6, 300.0));
    assert_eq!(stoff(&s, eg, "Kerndämmung (Mineralwolle)"), 13.1531);
    assert_eq!(stoff(&s, eg, "Verblender (Vormauerziegel)"), 11.2822);
    assert_eq!(stoff(&s, og, "Kerndämmung (Mineralwolle)"), 14.3268);
    // 12,2756 + Verblender unter die Bekleidung (W3): 9,885 × 0,115 × 0,07
    assert_eq!(stoff(&s, og, "Verblender (Vormauerziegel)"), 12.3552);
    assert_eq!(ud(&s, eg), Some(("UD-001".to_string(), 2.811, 0.3373)));
    assert_eq!(abfangung_m(&s, og), 10.485, "Abfangung Verblender");
    assert_eq!(abfangung(&s, aw6), 9885.0, "Nord");
    for w in ["AW-005", "AW-007"] {
        assert!((abfangung(&s, nr(&s, w)) - 300.0).abs() < 0.5, "{w}");
    }
    assert_eq!(abfangung(&s, nr(&s, "AW-008")), 0.0, "Süd");
    assert_eq!(abfangung_m(&s, eg), 0.0, "nur an OG-Wänden");
    pruefung(&s);
    assert!(buendig(&mut s, aw6));
    assert_eq!(abfangung_m(&s, og), 0.0, "bündig: keine Abfangung");
    assert_eq!(ud(&s, eg), None);
}

// Abnahmetest A160: Fällt der Abstand eines gelösten OG-Segments beim Ziehen
// am EG unter 20 mm, fängt die EG-Linie das Segment bündig (Review 1t,
// Befund T1; Regel 31). Spezifikation: test/abnahme-og-phase2.md.
//
// Einbau: ans Ende von `mod og_phase2` in app/src/abnahme.rs (nutzt dessen
// Adapter und Hilfen: prueffall, nr, kette, versetzen, versatz, eg_nord, ud,
// abfangung_m, decke_mengen, punkte, pruefung). Dazu ändert
// `a151-eg-faengt-buendig.patch` den Schluss von A151 (dort blieben 15 mm
// stehen).
//
// Keine neuen Adapter: Der Fang steckt im Modell (Geometriekern), getestet
// wird über `eg_nord` (EG-Nordwand per `set_run_points`, wie das Gummiband).

/// Prüfhaus mit dem Werkstyp `typ` am EG-Zug, AW-006 gelöst bei +300.
fn vorsprung_300(seed: u64, typ: &str) -> (Scene, RunId, RunId, ElementId) {
    let (mut s, eg, og) = prueffall(seed);
    if typ != "AW-31,5" {
        let t = s
            .model()
            .layer_sets()
            .iter()
            .find(|(_, t)| t.code == typ)
            .map(|(id, _)| id)
            .expect("Werkstyp");
        assert!(s.edit_model("Wandtyp geändert", |m| m.set_run_type(eg, t)));
    }
    let aw6 = nr(&s, "AW-006");
    assert!(kette(&mut s, aw6, false));
    assert!(versetzen(&mut s, aw6, 300.0));
    (s, eg, og, aw6)
}

/// A160 (Review 1t T1, Regel 31): OG-Nordwand gelöst bei +300, dann den
/// EG-Nordfuß 290 (bzw. 295) nach außen. Gemessen wären 10 (5) mm; das
/// Segment rastet bündig ein: Versatz 0, OG- und EG-Zug gleich, kein
/// Kragstreifen an DE-001 (DE-001 = DE-002 = Kern 9,72 × 8,01 bzw. 8,015),
/// keine UD, bei AW-49 keine Abfangung. Wieder koppeln: 0. Gegenprobe 260:
/// 40 mm Vorsprung bleiben, mit Kragstreifen und UD (0,3888 m², 12 cm
/// 0,0467 m³).
#[test]
fn a160_eg_faengt_geloestes_og_buendig() {
    for (d, de) in [(290.0, 77.8572), (295.0, 77.9058)] {
        let (mut s, eg, og, aw6) = vorsprung_300(160, "AW-31,5");
        assert!(eg_nord(&mut s, eg, d));
        assert_eq!(versatz(&s, aw6), Some((0.0, false)), "{d}: gefangen");
        assert_eq!(punkte(&s, eg), punkte(&s, og), "{d}: bündig");
        assert_eq!(decke_mengen(&s, eg).0, de, "{d}: DE-001 ohne Kragstreifen");
        assert_eq!(decke_mengen(&s, og).0, de, "{d}: DE-002");
        assert_eq!(ud(&s, eg), None, "{d}: keine UD");
        pruefung(&s);
        assert!(kette(&mut s, aw6, true));
        assert_eq!(versatz(&s, aw6), Some((0.0, true)), "{d}: koppeln ergibt 0");
        assert!(s.undo() && s.undo(), "Koppeln und EG-Zug je ein Schritt");
        assert_eq!(versatz(&s, aw6), Some((300.0, false)));
    }

    // AW-49: keine Abfangung
    let (mut s, eg, og, aw6) = vorsprung_300(1600, "AW-49");
    assert!(abfangung_m(&s, og) > 0.0, "Ausgang: Abfangung bei +300");
    assert!(eg_nord(&mut s, eg, 290.0));
    assert_eq!(versatz(&s, aw6), Some((0.0, false)));
    assert_eq!(abfangung_m(&s, og), 0.0, "AW-49: keine Abfangung");
    assert_eq!(ud(&s, eg), None);
    pruefung(&s);

    // Gegenprobe 260: 40 mm Vorsprung bleiben
    let (mut s, eg, og, aw6) = vorsprung_300(1601, "AW-31,5");
    assert!(eg_nord(&mut s, eg, 260.0));
    assert_eq!(versatz(&s, aw6), Some((40.0, false)), "40 mm bleiben");
    assert_ne!(punkte(&s, eg), punkte(&s, og));
    assert_eq!(decke_mengen(&s, eg).0, 77.9544, "DE-001 kragt 40 mm aus");
    assert_eq!(decke_mengen(&s, og).0, 77.9544);
    assert_eq!(ud(&s, eg), Some(("UD-001".to_string(), 0.3888, 0.0467)));
    pruefung(&s);
}

// Abnahmetest A168: Ein gelöster OG-Versatz lässt keinen Wandstummel unter
// Regel 30 stehen (Robustheitsprüfung Geometriekern 07.10.,
// geometrie/g9-regel30-kein-stummel.patch). Spezifikation:
// test/abnahme-og-phase2.md.
//
// Einbau: ans Ende von `mod og_phase2` in app/src/abnahme.rs (nutzt dessen
// Adapter und Hilfen: kette, versetzen, versatz, punkte, pruefung).
//
// Keine neuen Adapter. Das Prüfhaus mit Sprung in der Nordwand entsteht direkt
// im Modell (`add_building`, `build_from_polygon`), damit auch Sprünge von
// 30 mm möglich sind, die das Zeichenraster nicht trifft.

/// Haus 10 m breit, Nordwand mit Sprung: links (x 0…5) bei y = 8000, rechts
/// (x 5…10) bei y = 8000 + `sprung`. Liefert Szene, EG- und OG-Zug.
fn haus_mit_sprung(seed: u64, sprung: f64) -> (Scene, RunId, RunId) {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(5000.0, 8000.0, 0.0),
        vec3(5000.0, 8000.0 + sprung, 0.0),
        vec3(10000.0, 8000.0 + sprung, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let mut eg = None;
    assert!(s.edit_model("Gebäude erstellt", |m| {
        let b = m.add_building(2);
        eg = m.build_from_polygon(b, &pts);
        eg.is_some()
    }));
    let eg = eg.unwrap();
    let og = s.model().runs_above(eg)[0];
    (s, eg, og)
}

/// Wand des Zugs `run`, deren Segment von `a` nach `b` (oder umgekehrt) läuft.
fn wand_zwischen(s: &Scene, run: RunId, a: Vec3, b: Vec3) -> ElementId {
    let r = s.model().run(run).unwrap();
    let n = r.points.len();
    let nah = |p: Vec3, q: Vec3| (p - q).length() < 1.0;
    (0..n)
        .find(|&k| {
            let (p, q) = (r.points[k], r.points[(k + 1) % n]);
            (nah(p, a) && nah(q, b)) || (nah(p, b) && nah(q, a))
        })
        .map(|k| r.segments[k])
        .expect("Wand zwischen den Punkten")
}

/// Länge des Sprungs (Segment bei x = 5000) im OG-Zug.
fn sprung_og(s: &Scene, og: RunId) -> f64 {
    let p = punkte(s, og);
    let n = p.len();
    (0..n)
        .map(|k| (p[k], p[(k + 1) % n]))
        .filter(|(a, b)| (a.x - 5000.0).abs() < 1.0 && (b.x - 5000.0).abs() < 1.0)
        .map(|(a, b)| (a - b).length())
        .next()
        .expect("Sprung im OG")
}

/// A168 (Regel 30, Geometriekern g9): Die OG-Wand vor dem 400-mm-Sprung wird
/// gelöst. +300 ließe vom Sprung 100 mm stehen (Mindestmaß min(Wanddicke
/// 315, Partner 400) = 315): abgelehnt, kein Schritt, Versatz bleibt 0, die
/// Prüfung bleibt leer. +90 (Rest 310) ebenso; +80 (Rest 320) geht. Bei
/// Sprüngen von 30 und 50 mm wird schon +20 abgelehnt (Rest 10 bzw. 30 unter
/// dem Partner). Das Gummiband am EG klemmt ebenso: OG links bei +80 gelöst,
/// den EG-Sprung auf 100 mm kürzen ließe im OG 20 mm stehen, abgelehnt, das
/// EG bleibt, wie es war.
#[test]
fn a168_versatz_laesst_keinen_stummel_stehen() {
    let (mut s, eg, og) = haus_mit_sprung(168, 400.0);
    let links = wand_zwischen(&s, og, vec3(0.0, 8000.0, 0.0), vec3(5000.0, 8000.0, 0.0));
    assert!(kette(&mut s, links, false));
    pruefung(&s);
    let schritt = s.undo_label();
    for d in [300.0, 90.0] {
        assert!(!versetzen(&mut s, links, d), "+{d}: Stummel abgelehnt");
        assert_eq!(versatz(&s, links), Some((0.0, false)), "+{d}: bleibt 0");
        assert_eq!(sprung_og(&s, og), 400.0, "+{d}: Sprung unverändert");
        assert_eq!(s.undo_label(), schritt, "+{d}: kein Schritt");
        pruefung(&s);
    }
    assert!(versetzen(&mut s, links, 80.0), "+80: Rest 320 mm");
    assert_eq!(versatz(&s, links), Some((80.0, false)));
    assert_eq!(sprung_og(&s, og), 320.0);
    pruefung(&s);

    // Gummiband am EG: rechten Teil der Nordwand 300 nach innen
    let vorher = punkte(&s, eg);
    let p: Vec<Vec3> = vorher
        .iter()
        .map(|p| {
            if (p.y - 8400.0).abs() < 1.0 {
                vec3(p.x, p.y - 300.0, p.z)
            } else {
                *p
            }
        })
        .collect();
    let schritt = s.undo_label();
    assert!(
        !s.edit_model("Wand verschoben", |m| m.set_run_points(eg, &p).is_some()),
        "EG klemmt: im OG blieben 20 mm"
    );
    assert_eq!(punkte(&s, eg), vorher, "EG unverändert");
    assert_eq!(versatz(&s, links), Some((80.0, false)));
    assert_eq!(s.undo_label(), schritt, "kein Schritt");
    pruefung(&s);

    // Kleine Sprünge: schon +20 ist zu viel
    for (seed, sprung) in [(1680, 30.0), (1681, 50.0)] {
        let (mut s, _, og) = haus_mit_sprung(seed, sprung);
        pruefung(&s);
        let links = wand_zwischen(&s, og, vec3(0.0, 8000.0, 0.0), vec3(5000.0, 8000.0, 0.0));
        assert!(kette(&mut s, links, false));
        assert!(
            !versetzen(&mut s, links, 20.0),
            "Sprung {sprung}: +20 abgelehnt"
        );
        assert_eq!(versatz(&s, links), Some((0.0, false)));
        assert_eq!(sprung_og(&s, og), sprung);
        pruefung(&s);
    }
}

// Abnahmetests A169–A173: „Bündig setzen“ mit freier Zielwand (Jörn 07.10.
// 07:53: erst „Bündig setzen“, dann die Zielwand anklicken; die andere Wand
// des Paars rückt an sie). Vorbelegungen des Koordinators 07:53: beide Wände
// des Paars leuchten, Vorschau beim Überfahren, OG-Wand angeklickt = EG rückt
// ans OG, EG-Wand angeklickt = OG rückt ans EG wie bisher, Esc oder Klick ins
// Leere bricht ab, ein Rückgängig-Schritt; wandert das EG, gehen Bodenplatte,
// Frostschürze und Decken mit wie beim Ziehen am EG; ein zu kurzer Wandrest
// (Regel 30, G9) lässt alles stehen, ein Hinweis sagt warum.
// Dazu BIM Regel 36 (paket-og-phase2.md §6, Testvorschlag über den
// Koordinator 07:56) und Gestaltung einstellungen/paket-e20-zielwahl.md
// (§3 Texte, §7 Prüfpunkte; Enter = EG-Wand, Klick auf ein anderes Bauteil
// bricht ab, Ablehnung schon beim Hover).
// Spezifikation: test/abnahme-og-phase2.md. Bedienung im Fenster: H143–H146.
//
// Einbau: ans Ende von `mod og_phase2` in app/src/abnahme.rs (nach A168; nutzt
// dessen Adapter und Hilfen: prueffall, nr, kette, versetzen, versatz,
// eg_nord, punkte, pruefung, stand, BUENDIG, ud, abfangung_m, haus_mit_sprung,
// wand_zwischen, sprung_og; aus abnahme.rs m2, m3).
//
// Adapter auf die Kern-API aus geometrie/g10-buendig-an-zielwand.patch:
// `Scene::flush_to(wand, ziel) -> Result<bool, FlushError>` (`wand` rückt an
// `ziel`, ein Schritt „Bündig gesetzt“, Ok(false) = schon bündig),
// `Model::can_flush_to` (rein, für den Hover), `Model::wall_below` für das
// Paar, `FlushError::message()` für die Hinweiskarte. Die Tests sprechen von
// der Seite der Bedienung: `wand` ist die OG-Wand, an der „Bündig setzen“
// gewählt wurde, `ziel` die angeklickte Wand des Paars; die andere rückt.
// Statuszeile beim Hover und Ablehnungskarte nach E20 §3 stehen in A174
// (`a174-zielwahl-texte.rs`, dort noch angenommene Namen).

// ===== Adapter =====

/// Wände, die nach „Bündig setzen“ leuchten: (OG-Wand, EG-Partner); `None`,
/// wenn die Wand nicht gestapelt ist.
fn zielwahl(s: &Scene, wand: ElementId) -> Option<(ElementId, ElementId)> {
    s.model().wall_below(wand).map(|eg| (wand, eg))
}

/// Die Wand, die rückt, wenn `ziel` angeklickt wird.
fn rueckt(s: &Scene, wand: ElementId, ziel: ElementId) -> ElementId {
    match s.model().wall_below(wand) {
        Some(eg) if ziel == wand => eg,
        _ => wand,
    }
}

/// Hover über `ziel`: geht es (Geist) oder nicht (roter Umriss, Grund)?
/// Ändert nichts.
fn vorschau(s: &Scene, wand: ElementId, ziel: ElementId) -> Result<(), String> {
    s.model()
        .can_flush_to(rueckt(s, wand, ziel), ziel)
        .map_err(|e| e.message().to_string())
}

/// Klick auf `ziel`: die andere Wand des Paars rückt bündig an sie und wird
/// gekoppelt, ein Schritt „Bündig gesetzt“. Ok(false): schon bündig. Err =
/// Grund (Hinweiskarte), nichts geändert.
fn buendig_an(s: &mut Scene, wand: ElementId, ziel: ElementId) -> Result<bool, String> {
    let w = rueckt(s, wand, ziel);
    s.flush_to(w, ziel).map_err(|e| e.message().to_string())
}

// ===== Hilfen =====

/// Bodenplatte und Frostschürze des EG-Zugs: (Fläche m², Volumen m³, Umfang
/// m) und (Länge m, Volumen m³).
type Gruendung = ((f64, f64, f64), (f64, f64));
fn gruendung(s: &Scene, eg: RunId) -> Gruendung {
    let (p, f) = s.foundation_qto(eg).expect("Gründung");
    let m = |v: f64| (v / 10.0).round() / 100.0;
    (
        (m2(p.area), m3(p.volume), m(p.perimeter)),
        (m(f.length), m3(f.volume)),
    )
}

/// Alles, was beim Bündigsetzen gleich bleiben oder wie beim Ziehen am EG
/// werden muss.
type Lage = (Vec<Vec3>, Vec<Vec3>, Stand, Gruendung);
fn lage(s: &Scene, eg: RunId, og: RunId) -> Lage {
    (
        punkte(s, eg),
        punkte(s, og),
        stand(s, eg, og),
        gruendung(s, eg),
    )
}

fn datei(s: &Scene) -> String {
    sk_model::szo::write(s.model())
}

/// Bezug: neues Prüfhaus, EG-Nordwand gekoppelt um `dy` gezogen.
fn bezug(seed: u64, dy: f64) -> Lage {
    let (mut s, eg, og) = prueffall(seed);
    assert!(eg_nord(&mut s, eg, dy));
    pruefung(&s);
    lage(&s, eg, og)
}

// ===== Tests =====

/// A169 (OG → EG wie bisher): AW-006 gelöst bei +300. „Bündig setzen“ lässt
/// AW-006 und AW-002 leuchten, beide sind gültige Ziele. Klick auf die EG-Wand AW-002: das OG rückt
/// ans EG, Versatz 0, gekoppelt, alle Mengen wie im bündigen Haus, das EG
/// bleibt. Ein Schritt „Bündig gesetzt“; Rückgängig gibt +300 gelöst zurück.
#[test]
fn a169_og_rueckt_ans_eg() {
    let (mut s, eg, og) = prueffall(169);
    let aw6 = nr(&s, "AW-006");
    let aw2 = nr(&s, "AW-002");
    assert!(kette(&mut s, aw6, false));
    assert!(versetzen(&mut s, aw6, 300.0));
    let eg_vorher = punkte(&s, eg);
    assert_eq!(zielwahl(&s, aw6), Some((aw6, aw2)), "beide leuchten");
    assert_eq!(
        zielwahl(&s, aw2),
        None,
        "EG-Wand allein: kein Paar darunter"
    );
    assert_eq!(vorschau(&s, aw6, aw2), Ok(()));
    assert_eq!(vorschau(&s, aw6, aw6), Ok(()));

    assert_eq!(buendig_an(&mut s, aw6, aw2), Ok(true));
    assert_eq!(s.undo_label(), Some("Bündig gesetzt"));
    assert_eq!(versatz(&s, aw6), Some((0.0, true)));
    assert_eq!(punkte(&s, eg), eg_vorher, "EG bleibt");
    assert_eq!(punkte(&s, og), eg_vorher, "OG bündig auf dem EG");
    assert_eq!(stand(&s, eg, og), BUENDIG);
    assert_eq!(ud(&s, eg), None);
    pruefung(&s);
    assert_eq!(buendig_an(&mut s, aw6, aw2), Ok(false), "schon bündig");
    assert_eq!(buendig_an(&mut s, aw6, aw6), Ok(false), "schon bündig");

    assert!(s.undo(), "ein Schritt");
    assert_eq!(versatz(&s, aw6), Some((300.0, false)));
    assert_eq!(punkte(&s, eg), eg_vorher);
}

/// A170 (EG → OG): AW-006 gelöst bei +300 (Vorsprung) bzw. −300
/// (Rücksprung). Klick auf die OG-Wand AW-006: die EG-Nordwand rückt unter
/// sie. Danach ist alles wie beim gekoppelten Ziehen der EG-Nordwand um
/// +300 bzw. −300 in einem neuen Haus: Lage von EG und OG, Decken, Schalen,
/// Bodenplatte und Frostschürze. Versatz 0, gekoppelt, keine UD, keine
/// Abfangung, Guids bleiben (beim Vorsprung entfällt nur die UD, beim
/// Rücksprung DT und AB). Ein Schritt; Rückgängig stellt EG und Versatz wieder her,
/// Wiederholen das Ergebnis.
#[test]
fn a170_eg_rueckt_ans_og() {
    for (seed, d) in [(170, 300.0), (1700, -300.0)] {
        let (mut s, eg, og) = prueffall(seed);
        let aw6 = nr(&s, "AW-006");
        assert!(kette(&mut s, aw6, false));
        assert!(versetzen(&mut s, aw6, d));
        let vorher = lage(&s, eg, og);
        let og_vorher = punkte(&s, og);
        let g = guids(&s);

        assert_eq!(buendig_an(&mut s, aw6, aw6), Ok(true), "{d}");
        assert_eq!(s.undo_label(), Some("Bündig gesetzt"));
        assert_eq!(versatz(&s, aw6), Some((0.0, true)), "{d}");
        assert_eq!(punkte(&s, og), og_vorher, "{d}: OG bleibt stehen");
        let nachher = lage(&s, eg, og);
        assert_eq!(nachher, bezug(seed + 1, d), "{d}: wie Ziehen am EG");
        assert_eq!(ud(&s, eg), None, "{d}: keine UD");
        assert_eq!(abfangung_m(&s, og), 0.0, "{d}: keine Abfangung");
        assert_eq!(
            decke_mengen(&s, eg).0,
            decke_mengen(&s, og).0,
            "{d}: DE-001 = OG-Kern, kein Streifen"
        );
        // Guids bleiben; nur die UD unter dem Vorsprung entfällt, beim
        // Rücksprung Dachterrasse und Attikablech (Paket 2a/2c)
        let neu = guids(&s);
        assert!(neu.iter().all(|x| g.contains(x)), "{d}: keine neue Guid");
        let weg = if d > 0.0 { 1 } else { 2 };
        assert_eq!(g.len() - neu.len(), weg, "{d}: nur UD bzw. DT und AB");
        pruefung(&s);

        assert!(s.undo(), "{d}: ein Schritt");
        assert_eq!(lage(&s, eg, og), vorher, "{d}: Rückgängig");
        assert_eq!(versatz(&s, aw6), Some((d, false)));
        assert!(s.redo());
        assert_eq!(lage(&s, eg, og), nachher, "{d}: Wiederholen");
    }

    // Regel 36: Vorsprung mit Verblender (AW-49), Ziel OG: UD-Nummer und
    // Abfangung entfallen
    let (mut s, eg, og, aw6) = vorsprung_300(1701, "AW-49");
    assert!(ud(&s, eg).is_some(), "Ausgang: UD unter dem Vorsprung");
    assert!(abfangung_m(&s, og) > 0.0, "Ausgang: Abfangung");
    assert_eq!(buendig_an(&mut s, aw6, aw6), Ok(true));
    assert_eq!(ud(&s, eg), None, "UD entfällt");
    assert_eq!(abfangung_m(&s, og), 0.0, "Abfangung 0");
    pruefung(&s);
}

/// A171 (Vorschau und Abbruch, E20 §1, §7.5): Mit AW-006 gelöst bei +300
/// geht der Hover über beide Wände des Paars. Vorschau und Abbruch (Esc,
/// Klick ins Leere oder auf ein anderes Bauteil) ändern nichts: Datei
/// bytegleich, kein Schritt. Eine Wand außerhalb des Paars ist kein Ziel
/// (Grund), auch dann ändert sich nichts.
#[test]
fn a171_vorschau_und_abbruch() {
    let (mut s, _, _) = prueffall(171);
    let aw6 = nr(&s, "AW-006");
    let aw2 = nr(&s, "AW-002");
    assert!(kette(&mut s, aw6, false));
    assert!(versetzen(&mut s, aw6, 300.0));
    let vorher = datei(&s);
    let schritt = s.undo_label();

    assert!(zielwahl(&s, aw6).is_some());
    assert_eq!(vorschau(&s, aw6, aw2), Ok(()));
    assert_eq!(vorschau(&s, aw6, aw6), Ok(()));
    // Abbruch: nichts angewendet
    assert_eq!(datei(&s), vorher, "Vorschau ändert nichts");
    assert_eq!(s.undo_label(), schritt, "kein Schritt");

    // Wand außerhalb des Paars
    for fremd in [nr(&s, "AW-005"), nr(&s, "AW-001")] {
        assert!(vorschau(&s, aw6, fremd).is_err());
        let grund = buendig_an(&mut s, aw6, fremd).expect_err("kein Ziel");
        assert!(!grund.is_empty());
        assert_eq!(datei(&s), vorher);
        assert_eq!(s.undo_label(), schritt);
    }
    pruefung(&s);
}

/// A172 (Ablehnung, Grundriss ungültig; Vorlage Kerntest G10): Haus mit
/// 200-mm-Sprung in der Nordwand (EG 8000 → 8200). Im OG links gelöst −300
/// (7700), rechts gelöst −300 (7900, OG-Sprung 200). Rückte die EG-Wand
/// rechts unter das OG, kehrte sich der EG-Sprung um (7900 unter 8000):
/// abgelehnt mit Grund, schon beim Hover; alles bleibt, kein Schritt,
/// Prüfung leer. Gegenprobe: das OG rechts ans EG geht (OG-Sprung 500).
#[test]
fn a172_ablehnung_ungueltiger_grundriss() {
    let (mut s, eg, og) = haus_mit_sprung(172, 200.0);
    let links = wand_zwischen(&s, og, vec3(0.0, 8000.0, 0.0), vec3(5000.0, 8000.0, 0.0));
    let rechts = wand_zwischen(
        &s,
        og,
        vec3(5000.0, 8200.0, 0.0),
        vec3(10000.0, 8200.0, 0.0),
    );
    let eg_rechts = wand_zwischen(
        &s,
        eg,
        vec3(5000.0, 8200.0, 0.0),
        vec3(10000.0, 8200.0, 0.0),
    );
    assert!(kette(&mut s, links, false));
    assert!(versetzen(&mut s, links, -300.0));
    assert!(kette(&mut s, rechts, false));
    assert!(versetzen(&mut s, rechts, -300.0));
    assert_eq!(sprung_og(&s, og), 200.0);
    pruefung(&s);
    let vorher = datei(&s);
    let schritt = s.undo_label();

    let grund = vorschau(&s, rechts, rechts).expect_err("Hover: abgelehnt");
    assert!(!grund.is_empty());
    let grund2 = buendig_an(&mut s, rechts, rechts).expect_err("abgelehnt");
    assert_eq!(grund, grund2, "Grund schon beim Hover");
    assert_eq!(datei(&s), vorher, "alles bleibt");
    assert_eq!(s.undo_label(), schritt, "kein Schritt");
    assert_eq!(versatz(&s, rechts), Some((-300.0, false)));
    pruefung(&s);

    // Gegenprobe: OG ans EG
    assert_eq!(buendig_an(&mut s, rechts, eg_rechts), Ok(true));
    assert_eq!(versatz(&s, rechts), Some((0.0, true)));
    assert_eq!(sprung_og(&s, og), 500.0);
    pruefung(&s);
}

/// A173 (Nachbarn im OG): AW-006 gelöst +300, die OG-Westwand gelöst −200.
/// EG rückt ans OG: die gekoppelten OG-Wände (Ost, Süd) folgen dem EG und
/// bleiben gekoppelt bei 0, die gelöste Westwand behält −200, die Nordwand
/// ist gekoppelt bei 0. Das OG schließt an der neuen Nordlinie y = 8300.
#[test]
fn a173_gekoppelte_nachbarn_gehen_mit() {
    let (mut s, eg, og) = prueffall(173);
    let aw6 = nr(&s, "AW-006");
    let west = wand_zwischen(&s, og, vec3(0.0, 0.0, 0.0), vec3(0.0, 8000.0, 0.0));
    let ost = wand_zwischen(&s, og, vec3(10000.0, 0.0, 0.0), vec3(10000.0, 8000.0, 0.0));
    let sued = wand_zwischen(&s, og, vec3(0.0, 0.0, 0.0), vec3(10000.0, 0.0, 0.0));
    assert!(kette(&mut s, aw6, false));
    assert!(versetzen(&mut s, aw6, 300.0));
    assert!(kette(&mut s, west, false));
    assert!(versetzen(&mut s, west, -200.0));
    pruefung(&s);

    assert_eq!(buendig_an(&mut s, aw6, aw6), Ok(true));
    assert_eq!(versatz(&s, aw6), Some((0.0, true)));
    assert_eq!(versatz(&s, west), Some((-200.0, false)), "gelöst bleibt");
    assert_eq!(versatz(&s, ost), Some((0.0, true)), "Ost folgt");
    assert_eq!(versatz(&s, sued), Some((0.0, true)), "Süd folgt");
    let nord = |r: RunId| {
        punkte(&s, r)
            .iter()
            .map(|p| (p.y * 100.0).round() / 100.0)
            .fold(f64::MIN, f64::max)
    };
    assert_eq!(nord(eg), 8300.0, "EG an der Nordlinie des OG");
    assert_eq!(nord(og), 8300.0, "OG schließt bei 8300");
    let og_p = punkte(&s, og);
    assert!(
        og_p.iter()
            .any(|p| (p.x - 10000.0).abs() < 0.01 && (p.y - 8300.0).abs() < 0.01),
        "Ostwand des OG reicht bis zur neuen Nordlinie: {og_p:?}"
    );
    pruefung(&s);
}

// Abnahmetest A174: Texte der Zielwahl „Bündig setzen“ nach
// einstellungen/paket-e20-zielwahl.md §3 (Statuszeile beim Hover mit Maß,
// Statuszeile nach dem Einrasten, Ablehnungskarte, Zielkarte).
// Spezifikation: test/abnahme-og-phase2.md. Aussehen: H143–H146.
//
// Einbau: ans Ende von `mod og_phase2` in app/src/abnahme.rs, nach A169–A173
// (nutzt prueffall, nr, kette, versetzen, haus_mit_sprung, wand_zwischen).
//
// Angenommene Namen stehen nur in den Adaptern (Vorschlag, frei wählbar):
// `crate::wall_edit::flush_status(model, wand, ziel) -> String` (die Wand
// `wand` rückt an `ziel`, wie `Scene::flush_to`; bei Ablehnung der Grund),
// `crate::wall_edit::flush_done(model, wand, ziel) -> String`,
// `crate::wall_edit::FLUSH_PICK` und `FLUSH_REFUSED` (je zwei Zeilen).
// Die Ablehnungskarte nach E20 weicht vom Text in `FlushError::message()`
// (G10) ab; der Test verlangt die E20-Zeilen auf der Karte.

// ===== Adapter =====

/// Statuszeile beim Hover: `wand` rückt an `ziel`.
fn status_hover(s: &Scene, wand: ElementId, ziel: ElementId) -> String {
    crate::flush_pick::status_text(s.model(), wand, ziel)
}

/// Statuszeile nach dem Einrasten.
fn status_fertig(s: &Scene, wand: ElementId, ziel: ElementId) -> String {
    crate::flush_pick::done_text(s.model(), wand, ziel).to_string()
}

fn zeilen(z: [&str; 2]) -> (String, String) {
    (z[0].to_string(), z[1].to_string())
}

// ===== Test =====

/// A174 (E20 §3): AW-006 gelöst bei +300. Hover über AW-002 (OG rückt):
/// „Die OG-Wand rückt 0,30 m an die EG-Wand.“, über AW-006 (EG rückt):
/// „Die EG-Wand rückt 0,30 m an die OG-Wand.“; Rücksprung 3,33 m wie in
/// Jörns Bild: „… 3,33 m …“. Danach „OG-Wand bündig gesetzt.“ bzw.
/// „EG-Wand bündig gesetzt.“. Zielkarte „Zielwand anklicken“ / „Die andere
/// Wand rückt bündig an sie heran. Esc bricht ab.“. Ablehnung: Hover nennt
/// den Grund, Karte „Die EG-Wand kann hier nicht nachrücken.“ / „Daneben
/// bliebe ein zu kurzes Wandstück. Die OG-Wand an die EG-Wand setzen oder
/// die Nachbarwand erst anpassen.“
#[test]
fn a174_texte_der_zielwahl() {
    let (mut s, _, _) = prueffall(174);
    let aw6 = nr(&s, "AW-006");
    let aw2 = nr(&s, "AW-002");
    assert!(kette(&mut s, aw6, false));
    assert!(versetzen(&mut s, aw6, 300.0));
    assert_eq!(
        status_hover(&s, aw6, aw2),
        "Die OG-Wand rückt 0,30 m an die EG-Wand."
    );
    assert_eq!(
        status_hover(&s, aw2, aw6),
        "Die EG-Wand rückt 0,30 m an die OG-Wand."
    );
    assert_eq!(status_fertig(&s, aw6, aw2), "OG-Wand bündig gesetzt.");
    assert_eq!(status_fertig(&s, aw2, aw6), "EG-Wand bündig gesetzt.");
    assert!(versetzen(&mut s, aw6, -3630.0), "Rücksprung 3,33 m");
    assert_eq!(
        status_hover(&s, aw2, aw6),
        "Die EG-Wand rückt 3,33 m an die OG-Wand."
    );
    assert_eq!(
        zeilen(crate::wall_edit::FLUSH_PICK),
        (
            "Zielwand anklicken".to_string(),
            "Die andere Wand rückt bündig an sie heran. Esc bricht ab.".to_string()
        )
    );

    // Ablehnung (Fall aus A172)
    let (mut s, eg, og) = haus_mit_sprung(1740, 200.0);
    let links = wand_zwischen(&s, og, vec3(0.0, 8000.0, 0.0), vec3(5000.0, 8000.0, 0.0));
    let rechts = wand_zwischen(
        &s,
        og,
        vec3(5000.0, 8200.0, 0.0),
        vec3(10000.0, 8200.0, 0.0),
    );
    let eg_rechts = wand_zwischen(
        &s,
        eg,
        vec3(5000.0, 8200.0, 0.0),
        vec3(10000.0, 8200.0, 0.0),
    );
    assert!(kette(&mut s, links, false) && versetzen(&mut s, links, -300.0));
    assert!(kette(&mut s, rechts, false) && versetzen(&mut s, rechts, -300.0));
    let grund = s.model().can_flush_to(eg_rechts, rechts).unwrap_err();
    assert_eq!(
        status_hover(&s, eg_rechts, rechts),
        grund.message(),
        "Grund beim Hover"
    );
    assert_eq!(
        zeilen(crate::wall_edit::FLUSH_REFUSED),
        (
            "Die EG-Wand kann hier nicht nachrücken.".to_string(),
            "Daneben bliebe ein zu kurzes Wandstück. Die OG-Wand an die EG-Wand setzen \
                 oder die Nachbarwand erst anpassen."
                .to_string()
        )
    );
}

use super::*;

// Abnahmetests A131–A136: Bauteile und Gebäude löschen (Paket „Löschen“).
// Grundlage: bim/paket-loeschen.md (Regeln 24–27, „Fertig, wenn“),
// einstellungen/paket-loeschen-gestaltung.md (Sätze), Jörns „Ja“ zu den
// Sollbildern soll-loeschen-1…4 (07.10. 02:27).
// Spezifikation: test/abnahme-loeschen.md. Handtests H112–H119.
//
// Einbau: als `mod loeschen { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: haus_b11, zeichne_rechteck,
// cam3d, tool, click, key, W, H, aw_schicht, decke_mengen, m3, band,
// test_dir, STANDARD.
//
// Angenommene Namen stehen nur in den Adaptern (Vorschlag aus dem Paket,
// Abschnitt 6): `Scene::delete_elements`, `Model::can_delete`,
// `sk_model::refusal_text`, `Scene::remove_building`,
// `Model::building_part_count`, `crate::delete::hint`. Heißen sie im Bau
// anders, ändert sich nur der Adapter.

use crate::picking::Picking;
use sk_model::{BuildingId, Category, ElementId, GuidGen, Model, RunId};

// ===== Adapter =====

/// Entf bzw. „Löschen“ im Kontextmenü mit der Auswahl `ids`: ein
/// Rückgängig-Schritt, wenn etwas gelöscht wurde. Ergebnis: die gelöschten
/// Bauteile und der Hinweis am Bauteil, Zeile für Zeile (leer: kein Hinweis).
fn loeschen(s: &mut Scene, ids: &[ElementId]) -> (Vec<ElementId>, Vec<String>) {
    let d = s.delete_elements(ids);
    let hint = crate::delete::hint(s.model(), &d);
    (d.removed, hint)
}

/// Satz, warum `id` nicht löschbar ist (Tooltip am gedimmten „Löschen“);
/// `None`: löschbar. Zwei Sätze stehen mit Leerzeichen hintereinander.
fn ablehnung(m: &Model, id: ElementId) -> Option<String> {
    m.can_delete(id)
        .err()
        .map(|r| sk_model::refusal_text(m, id, &r))
}

/// „Gebäude löschen …“ und in der Rückfrage „Löschen“.
fn gebaeude_loeschen(s: &mut Scene, b: BuildingId) -> bool {
    s.remove_building(b)
}

/// Zahl der Bauteile, die die Rückfrage nennt.
fn gebaeude_bauteile(m: &Model, b: BuildingId) -> usize {
    m.building_part_count(b)
}

// ===== Hilfen =====

/// Bauteil mit der Nummer `nr` („IW-001“).
fn nr(s: &Scene, nr: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nr)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nr} fehlt"))
}

fn hat(s: &Scene, n: &str) -> bool {
    s.model().elements().iter().any(|(_, e)| e.number == n)
}

fn nummer(s: &Scene, id: ElementId) -> String {
    s.model().element(id).unwrap().number.clone()
}

fn segmente(s: &Scene, run: RunId) -> Vec<String> {
    s.model()
        .run(run)
        .unwrap()
        .segments
        .iter()
        .map(|e| nummer(s, *e))
        .collect()
}

/// Innenwandzug mit dem Werkzeug durch `punkte`, Enter beendet.
fn iw_zug(s: &mut Scene, punkte: &[(f64, f64)]) -> RunId {
    let c = cam3d();
    let set = s.model().defaults().interior_wall;
    let mut t = tool(s);
    t.set_category(Category::InteriorWall, s.model().wall_layers(set));
    for &(x, y) in punkte {
        click(&mut t, &c, vec3(x, y, 0.0));
    }
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    s.add_wall_as(&w, Category::InteriorWall).unwrap()
}

/// Offener Außenwandzug neben dem Haus (kein Gebäudeumriss).
fn offener_zug(s: &mut Scene) -> RunId {
    let c = cam3d();
    let mut t = tool(s);
    click(&mut t, &c, vec3(14000.0, 0.0, 0.0));
    click(&mut t, &c, vec3(14000.0, 5000.0, 0.0));
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit.unwrap();
    assert!(!w.closed);
    s.add_wall(&w).unwrap()
}

fn wand_m3(s: &Scene, id: ElementId) -> f64 {
    s.wall_qto(id).unwrap().volume / 1e9
}

/// Mengen des Prüfhauses ohne die Innenwand (Dämmung, Porenbeton, Decke,
/// Sohlplatte, Frostschürze), gerundet wie `mengen_b11`.
fn mengen_ohne_iw(s: &Scene, aw: RunId) -> [f64; 5] {
    let r = |v: f64| (v * 1e4).round() / 1e4;
    let (sp, fs) = s.foundation_qto(aw).unwrap();
    [
        r(aw_schicht(s, aw, 0)),
        r(aw_schicht(s, aw, 1)),
        decke_mengen(s, aw).1,
        m3(sp.volume),
        m3(fs.volume),
    ]
}

fn sauber(s: &Scene, ctx: &str) {
    assert!(
        s.model().check().is_empty(),
        "{ctx}: {:?}",
        s.model().check()
    );
}

fn zeilen(v: &[&str]) -> Vec<String> {
    v.iter().map(|z| z.to_string()).collect()
}

// ===== Tests =====

/// A131 („Fertig, wenn“: Innenwand löschen, Mengenfenster): Am Prüfhaus
/// verschwindet IW-001 in einem Schritt „Bauteil gelöscht“. Außenwand,
/// Decke, Sohlplatte und Frostschürze behalten die Mengen aus A39; die
/// Innenwand fehlt in der Liste und in der .csv. Ist sie im Mengenfenster
/// gewählt und unter der Maus, fällt sie ohne Fehler heraus. Kein Hinweis.
/// Rückgängig bringt IW-001 mit derselben Guid und denselben Mengen zurück,
/// Wiederholen löscht sie wieder. Regeln 24–27 sauber.
#[test]
fn a131_innenwand_loeschen() {
    let mut s = Scene::with_model(Model::with_seed(131));
    let (aw, iw) = haus_b11(&mut s);
    let vorher = mengen_ohne_iw(&s, aw);
    assert_eq!(
        vorher,
        [
            STANDARD[0],
            STANDARD[1],
            STANDARD[3],
            STANDARD[4],
            STANDARD[5]
        ],
        "Prüfhaus wie A39"
    );
    let id = nr(&s, "IW-001");
    let guid = s.model().element(id).unwrap().guid;
    let text_vorher = sk_model::szo::write(s.model());
    assert_eq!(ablehnung(s.model(), id), None, "Innenwand ist löschbar");
    let mut p = Picking::default();
    p.click(id, false);
    p.set_hover(Some(id), vec![id]);

    let (weg, hinweis) = loeschen(&mut s, &[id]);
    assert_eq!(weg, vec![id]);
    assert!(
        hinweis.is_empty(),
        "ein Bauteil, nichts abgelehnt: {hinweis:?}"
    );
    assert_eq!(s.undo_label(), Some("Bauteil gelöscht"));
    assert!(s.model().element(id).is_none());
    assert!(
        s.model().run(iw).is_none(),
        "Zug mit einem Segment verschwindet"
    );
    assert_eq!(mengen_ohne_iw(&s, aw), vorher, "Nachbarn unverändert");
    sauber(&s, "nach dem Löschen");
    // Mengenfenster: Zeile weg, Auswahl und Hover ohne Fehler bereinigt
    let liste = s.schedule().clone();
    let csv = String::from_utf8_lossy(&crate::schedule_view::csv(s.model(), &liste)).to_string();
    assert!(!csv.contains("IW-001"), "{csv}");
    p.validate(&s);
    assert!(p.selected.is_empty() && p.hover.is_none() && p.hover_group.is_empty());

    assert!(s.undo());
    let id2 = nr(&s, "IW-001");
    assert_eq!(s.model().element(id2).unwrap().guid, guid, "gleiche Guid");
    assert_eq!(
        sk_model::szo::write(s.model()),
        text_vorher,
        "alles wie vorher"
    );
    assert_eq!(mengen_ohne_iw(&s, aw), vorher);
    assert!(s.redo());
    assert!(!hat(&s, "IW-001"), "Wiederholen löscht wieder");
    sauber(&s, "nach Wiederholen");
}

/// A132 (Regel 25, „Nummer bleibt vergeben“): IW-001 löschen, speichern,
/// öffnen, neue Innenwand zeichnen: Sie heißt IW-002. Die Datei nennt den
/// Zähler; eine ältere Datei ohne Zähler lädt wie bisher (höchste vorhandene
/// Nummer). Ein Zug mit drei Segmenten IW-002…IW-004, alle drei gelöscht:
/// ein Schritt „3 Bauteile gelöscht“, der Zug verschwindet; die nächste
/// Innenwand heißt IW-005.
#[test]
fn a132_nummern_werden_nie_neu_vergeben() {
    let mut s = Scene::with_model(Model::with_seed(132));
    haus_b11(&mut s);
    let id = nr(&s, "IW-001");
    loeschen(&mut s, &[id]);
    let text = sk_model::szo::write(s.model());
    let d = test_dir("a132");
    let p = d.join("haus.szo");
    std::fs::write(&p, &text).unwrap();
    let l = crate::document::load(&p).expect("öffnet");
    let mut s = Scene::with_model(l.model);
    sauber(&s, "nach dem Laden");
    let neu = iw_zug(&mut s, &[(5000.0, 0.0), (5000.0, 8000.0)]);
    assert_eq!(segmente(&s, neu), ["IW-002"], "IW-001 bleibt vergeben");

    // Drei Segmente auf einmal
    let z = iw_zug(
        &mut s,
        &[
            (1000.0, 1500.0),
            (1000.0, 3000.0),
            (3500.0, 3000.0),
            (3500.0, 6500.0),
        ],
    );
    assert_eq!(segmente(&s, z), ["IW-003", "IW-004", "IW-005"]);
    let alle = s.model().run(z).unwrap().segments.clone();
    let (weg, _) = loeschen(&mut s, &alle);
    assert_eq!(weg.len(), 3);
    assert_eq!(s.undo_label(), Some("3 Bauteile gelöscht"));
    assert!(s.model().run(z).is_none(), "ganzer Zug weg");
    sauber(&s, "Zug gelöscht");
    let n = iw_zug(&mut s, &[(7500.0, 0.0), (7500.0, 8000.0)]);
    assert_eq!(segmente(&s, n), ["IW-006"]);
    // Rundlauf: der Zähler übersteht Speichern und Öffnen erneut
    let text = sk_model::szo::write(s.model());
    let l = sk_model::szo::read(&text, GuidGen::with_seed(1)).expect("lädt");
    assert_eq!(sk_model::szo::write(&l.model), text, "Rundlauf");
}

/// A133 (Regel 11, 26: Teilen und Kürzen): Innenwandzug IW-002…IW-004 im
/// Prüfhaus. Mittleres Segment löschen: zwei Züge, der erste behält RunId
/// und Guid des Zuges mit IW-002, der zweite hat eine neue RunId und Guid mit
/// IW-004 (seg 0); Wände behalten Guid und Nummer; Bezugsseite, Typ,
/// Höhenbezug und Geschoss gleich; Punkte passend. Rückgängig: ein Zug mit
/// drei Segmenten und derselben Guid. Endsegment löschen: ein Zug mit zwei
/// Segmenten, RunId bleibt. Anfang und Ende zusammen gelöscht: der Rest
/// IW-003 bleibt als ein Zug.
#[test]
fn a133_zug_teilen_und_kuerzen() {
    let pkt = [
        (1000.0, 1500.0),
        (1000.0, 3000.0),
        (3500.0, 3000.0),
        (3500.0, 6500.0),
    ];
    let mut s = Scene::with_model(Model::with_seed(133));
    haus_b11(&mut s);
    let z = iw_zug(&mut s, &pkt);
    assert_eq!(segmente(&s, z), ["IW-002", "IW-003", "IW-004"]);
    let alt = s.model().run(z).unwrap().clone();
    let guid_wand = |s: &Scene, n: &str| s.model().element(nr(s, n)).unwrap().guid;
    let g2 = guid_wand(&s, "IW-002");
    let g4 = guid_wand(&s, "IW-004");
    let zuege_vorher = s.model().runs().len();

    // Mitte
    let ids = [nr(&s, "IW-003")];
    let (weg, _) = loeschen(&mut s, &ids);
    assert_eq!(weg.len(), 1);
    assert_eq!(s.undo_label(), Some("Bauteil gelöscht"));
    assert_eq!(s.model().runs().len(), zuege_vorher + 1, "geteilt");
    let erster = s.model().run(z).expect("erster Teil behält die RunId");
    assert_eq!(erster.guid, alt.guid, "… und die Guid des Zuges");
    assert_eq!(segmente(&s, z), ["IW-002"]);
    assert_eq!(erster.points.len(), 2);
    let zweit_id = s.model().segment_of(nr(&s, "IW-004")).unwrap().0;
    assert_ne!(zweit_id, z);
    let zweiter = s.model().run(zweit_id).unwrap();
    assert_ne!(zweiter.guid, alt.guid, "zweiter Teil: neue Guid");
    assert_eq!(segmente(&s, zweit_id), ["IW-004"]);
    assert_eq!(
        s.model().segment_of(nr(&s, "IW-004")).unwrap().1,
        0,
        "seg ab 0"
    );
    assert_eq!(zweiter.points.len(), 2);
    for t in [erster, zweiter] {
        assert_eq!(t.ref_side, alt.ref_side);
        assert_eq!(t.base, alt.base);
        assert_eq!(t.top, alt.top);
        assert_eq!(t.storey, alt.storey);
        assert!(!t.closed);
    }
    for (a, b) in [
        (erster.points[0], alt.points[0]),
        (erster.points[1], alt.points[1]),
    ]
    .into_iter()
    .chain([
        (zweiter.points[0], alt.points[2]),
        (zweiter.points[1], alt.points[3]),
    ]) {
        assert!((a - b).length() < 1e-6, "Punkte bleiben: {a:?} {b:?}");
    }
    assert_eq!(guid_wand(&s, "IW-002"), g2);
    assert_eq!(guid_wand(&s, "IW-004"), g4);
    let typ = |s: &Scene, n: &str| s.model().element(nr(s, n)).unwrap().layer_set;
    assert_eq!(typ(&s, "IW-002"), typ(&s, "IW-004"));
    sauber(&s, "geteilt");
    assert!(s.undo());
    assert_eq!(s.model().runs().len(), zuege_vorher);
    assert_eq!(s.model().run(z).unwrap().guid, alt.guid);
    assert_eq!(segmente(&s, z), ["IW-002", "IW-003", "IW-004"]);

    // Ende
    let ids = [nr(&s, "IW-004")];
    loeschen(&mut s, &ids);
    assert_eq!(s.model().runs().len(), zuege_vorher, "nicht geteilt");
    assert_eq!(segmente(&s, z), ["IW-002", "IW-003"]);
    assert_eq!(s.model().run(z).unwrap().points.len(), 3);
    sauber(&s, "gekürzt");
    assert!(s.undo());

    // Anfang und Ende in einem Gang
    let ids = [nr(&s, "IW-002"), nr(&s, "IW-004")];
    let (weg, _) = loeschen(&mut s, &ids);
    assert_eq!(weg.len(), 2);
    assert_eq!(s.undo_label(), Some("2 Bauteile gelöscht"));
    assert_eq!(s.model().runs().len(), zuege_vorher);
    let rest = s.model().segment_of(nr(&s, "IW-003")).unwrap();
    assert_eq!(rest.1, 0);
    assert_eq!(segmente(&s, rest.0), ["IW-003"]);
    sauber(&s, "Rest");
}

/// A134 („Anschluss frei“): Innenwand von (2,0|4,0) bis an IW-001 bei
/// x = 5 m (T-Anschluss). IW-001 löschen: Die erste behält ihre Punkte, kein
/// Anschluss verweist mehr auf IW-001, ihr Ende ist frei und rechtwinklig:
/// gleiches Volumen wie dieselbe Wand frei im Haus ohne IW-001.
#[test]
fn a134_anschluss_wird_frei() {
    use sk_model::join::JoinKind;
    let mut s = Scene::with_model(Model::with_seed(134));
    haus_b11(&mut s);
    let host = nr(&s, "IW-001");
    let z = iw_zug(&mut s, &[(2000.0, 4000.0), (5000.0, 4000.0)]);
    let w = s.model().wall_at(z, 0).unwrap();
    assert!(
        s.model()
            .joins()
            .iter()
            .any(|j| j.a == w && j.b == host && j.kind == JoinKind::T),
        "T-Anschluss an IW-001: {:?}",
        s.model().joins()
    );
    let punkte = s.model().run(z).unwrap().points.clone();
    loeschen(&mut s, &[host]);
    assert_eq!(s.model().run(z).unwrap().points, punkte, "Endpunkt bleibt");
    assert!(
        s.model().joins().iter().all(|j| j.a != host && j.b != host),
        "{:?}",
        s.model().joins()
    );
    sauber(&s, "Anschluss frei");

    let mut r = Scene::with_model(Model::with_seed(134));
    zeichne_rechteck(&mut r, &cam3d());
    let rz = iw_zug(&mut r, &[(2000.0, 4000.0), (5000.0, 4000.0)]);
    let soll = wand_m3(&r, r.model().wall_at(rz, 0).unwrap());
    let ist = wand_m3(&s, w);
    assert!(
        (ist - soll).abs() < 1e-6,
        "rechtwinkliges Ende: {ist} statt {soll}"
    );
}

/// A135 (Abschnitt 1 und 3, abgelehnt und gemischt): AW-001, eine OG-Außenwand,
/// DE-001, SP-001, FS-001 und RD-001 (Haus in AW-36,5) sind nicht löschbar.
/// Entf löscht nichts, der Hinweis nennt den Satz aus der Tabelle, kein
/// Rückgängig-Schritt, das Modell bleibt unverändert (Revision gleich).
/// Gemischt IW-001 + AW-001 + DE-001: nur IW-001 weg, Hinweis „1 Innenwand
/// gelöscht.“ / „Außenwände und Decken bleiben, sie gehören zum
/// Gebäudeumriss.“ / Verweis, ein Schritt „Bauteil gelöscht“. IW + AW:
/// „Die Außenwand bleibt, sie gehört zum Gebäudeumriss.“ Eine Wand eines
/// offenen Außenwandzugs ist löschbar; mit einer Innenwand zusammen heißt
/// Zeile 1 „2 Wände gelöscht.“
#[test]
fn a135_abgelehnt_und_gemischt() {
    let aw_satz = [
        "Außenwände gehören zum Gebäudeumriss.",
        "Zum Entfernen den Umriss ändern oder das ganze Gebäude löschen.",
    ];
    let de_satz =
        ["Die Decke ergibt sich aus dem Gebäudeumriss und bleibt, solange das Gebäude steht."];
    let gr_satz = ["Sohlplatte und Frostschürze folgen dem Umriss des Erdgeschosses."];
    let rd_satz = ["Der Randdämmstreifen gehört zum Wandtyp; zum Entfernen den Wandtyp ändern."];

    let mut s = Scene::with_model(Model::with_seed(135));
    let (aw, _) = haus_b11(&mut s);
    let mono = s
        .model()
        .layer_sets()
        .iter()
        .find(|(_, t)| t.code == "AW-36,5")
        .map(|(id, _)| id)
        .unwrap();
    assert!(s.edit_model("Wandtyp geändert", |m| m.set_run_type(aw, mono)));
    let og_aw = s
        .model()
        .elements()
        .iter()
        .find(|(_, e)| {
            e.category == Category::ExteriorWall
                && s.model().storey(e.storey).unwrap().short == "OG"
        })
        .map(|(id, _)| id)
        .expect("OG-Außenwand");
    let og_nr = nummer(&s, og_aw);
    for (n, satz) in [
        ("AW-001", &aw_satz[..]),
        (og_nr.as_str(), &aw_satz[..]),
        ("DE-001", &de_satz[..]),
        ("SP-001", &gr_satz[..]),
        ("FS-001", &gr_satz[..]),
        ("RD-001", &rd_satz[..]),
    ] {
        let id = nr(&s, n);
        assert_eq!(
            ablehnung(s.model(), id),
            Some(satz.join(" ")),
            "{n}: Tooltip"
        );
        let rev = s.model().revision();
        let label = s.undo_label();
        let (weg, hinweis) = loeschen(&mut s, &[id]);
        assert!(weg.is_empty(), "{n}");
        assert_eq!(hinweis, zeilen(satz), "{n}: Hinweis am Bauteil");
        assert_eq!(s.model().revision(), rev, "{n}: Datei unverändert");
        assert_eq!(s.undo_label(), label, "{n}: kein Rückgängig-Schritt");
        assert!(hat(&s, n));
    }
    // Alles Abgeleitete zusammen: weiterhin nichts gelöscht, ein Satz
    let alle: Vec<_> = ["AW-001", "DE-001", "SP-001"]
        .iter()
        .map(|n| nr(&s, n))
        .collect();
    let rev = s.model().revision();
    let (weg, hinweis) = loeschen(&mut s, &alle);
    assert!(weg.is_empty());
    assert!(
        !hinweis.is_empty() && hinweis.len() <= 2,
        "ein Satz, keine Liste: {hinweis:?}"
    );
    assert_eq!(s.model().revision(), rev);

    // Gemischt
    let iw = nr(&s, "IW-001");
    let sel = [iw, nr(&s, "AW-001"), nr(&s, "DE-001")];
    let (weg, hinweis) = loeschen(&mut s, &sel);
    assert_eq!(weg, vec![iw]);
    assert_eq!(
        hinweis,
        zeilen(&[
            "1 Wand gelöscht.",
            "Außenwände und Decken bleiben, sie gehören zum Gebäudeumriss."
        ])
    );
    assert_eq!(s.undo_label(), Some("Bauteil gelöscht"), "ein Schritt");
    assert!(hat(&s, "AW-001") && hat(&s, "DE-001"));
    sauber(&s, "gemischt");
    assert!(s.undo());
    let iw = nr(&s, "IW-001");
    let ids = [iw, nr(&s, "AW-001")];
    let (_, hinweis) = loeschen(&mut s, &ids);
    assert_eq!(
        hinweis,
        zeilen(&[
            "1 Wand gelöscht.",
            "Die Außenwand bleibt, sie gehört zum Gebäudeumriss."
        ])
    );
    assert!(s.undo());

    // Offener Außenwandzug: löschbar wie eine Innenwand
    let oz = offener_zug(&mut s);
    let ow = s.model().wall_at(oz, 0).unwrap();
    assert_eq!(ablehnung(s.model(), ow), None, "offener Zug löschbar");
    let iw = nr(&s, "IW-001");
    let ids = [iw, ow, nr(&s, "AW-001")];
    let (weg, hinweis) = loeschen(&mut s, &ids);
    assert_eq!(weg.len(), 2);
    assert_eq!(
        hinweis.first().map(String::as_str),
        Some("2 Wände gelöscht.")
    );
    assert!(s.model().run(oz).is_none());
    assert_eq!(s.undo_label(), Some("2 Bauteile gelöscht"));
    sauber(&s, "offener Zug");
}

/// A136 (Abschnitt 4, Regel 24/27): „Gebäude löschen“. Die Rückfrage nennt
/// 13 Bauteile (8 Außenwände, 1 Innenwand, 2 Decken, Sohlplatte,
/// Frostschürze). Danach: kein Bauteil, kein Zug, kein Anschluss, kein
/// Gebäude; die Geschosse bleiben als Vorlage (building = None) mit
/// denselben Bändern; Typen, Baustoffe und Standardtypen bleiben; `check()`
/// leer; das Werkzeug „Gebäude“ öffnet wieder den Dialog (keine Gebäude).
/// Ein Schritt „Gebäude gelöscht“; Rückgängig stellt die Datei bytegleich
/// her, Wiederholen löscht wieder. Ein neues Gebäude heißt GB-02, seine
/// Außenwände beginnen bei AW-009 (Nummern nie neu), die Geschosse haben
/// dieselben Höhen.
#[test]
fn a136_gebaeude_loeschen() {
    let mut s = Scene::with_model(Model::with_seed(136));
    haus_b11(&mut s);
    let (b, gb) = s
        .model()
        .buildings()
        .iter()
        .map(|(id, b)| (id, b.number.clone()))
        .next()
        .expect("Gebäude");
    assert_eq!(gb, "GB-01");
    assert_eq!(
        gebaeude_bauteile(s.model(), b),
        13,
        "Rückfrage nennt die Bauteile"
    );
    assert_eq!(s.model().elements().len(), 13);
    let baender: Vec<_> = ["GR", "EG", "OG"].iter().map(|k| band(&s, k)).collect();
    assert_eq!(baender[1], (0.0, 2855.0), "Standardhöhen EG");
    assert_eq!(baender[2], (2855.0, 5710.0), "Standardhöhen OG");
    let typen = s.model().layer_sets().len();
    let stoffe = s.model().materials().len();
    let std = *s.model().defaults();
    let text = sk_model::szo::write(s.model());

    assert!(gebaeude_loeschen(&mut s, b));
    assert_eq!(s.undo_label(), Some("Gebäude gelöscht"));
    let m = s.model();
    assert!(m.elements().is_empty(), "kein Bauteil");
    assert_eq!(m.runs().len(), 0);
    assert!(m.joins().is_empty());
    assert!(
        m.buildings().is_empty(),
        "Werkzeug Gebäude öffnet den Dialog"
    );
    assert!(
        m.storeys().iter().all(|(_, st)| st.building.is_none()),
        "Vorlage"
    );
    let nach: Vec<_> = ["GR", "EG", "OG"].iter().map(|k| band(&s, k)).collect();
    assert_eq!(nach, baender, "Höhen bleiben als Vorlage");
    assert_eq!(s.model().layer_sets().len(), typen);
    assert_eq!(s.model().materials().len(), stoffe);
    assert_eq!(s.model().defaults(), &std);
    sauber(&s, "Gebäude gelöscht");

    assert!(s.undo());
    assert_eq!(
        sk_model::szo::write(s.model()),
        text,
        "Rückgängig: alles zurück"
    );
    assert!(s.redo());
    assert!(s.model().elements().is_empty());

    // Neues Gebäude: GB-02, Nummern laufen weiter, gleiche Höhen
    let aw = zeichne_rechteck(&mut s, &cam3d());
    let gb2: Vec<_> = s
        .model()
        .buildings()
        .iter()
        .map(|(_, b)| b.number.clone())
        .collect();
    assert_eq!(gb2, ["GB-02"]);
    assert_eq!(nummer(&s, s.model().wall_at(aw, 0).unwrap()), "AW-009");
    let neu: Vec<_> = ["GR", "EG", "OG"].iter().map(|k| band(&s, k)).collect();
    assert_eq!(neu, baender);
    sauber(&s, "neues Gebäude");
    // Auch nach Speichern und Öffnen
    let t = sk_model::szo::write(s.model());
    let l = sk_model::szo::read(&t, GuidGen::with_seed(1)).expect("lädt");
    assert_eq!(sk_model::szo::write(&l.model), t, "Rundlauf");
}

use super::*;

// ===== Adapter F2 =====

use crate::windows::{Action, Show, WindowId, Windows};

type R = (i32, i32, i32, i32);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fenster {
    Haupt,
    Mengen,
}

/// Was die Fensterlogik vom UI-Thread verlangt.
#[derive(Clone, Debug, PartialEq)]
enum Tat {
    /// Fenster anlegen (sichtbar) an dieser Stelle.
    Lege(Fenster, R),
    /// Lage und Größe setzen (SetWindowPos).
    Setze(Fenster, R),
    Minimiere(Fenster),
    /// Wiederherstellen ohne Aktivieren (SW_SHOWNOACTIVATE).
    Hole(Fenster),
    Maximiere(Fenster),
    Vorne(Fenster),
    Schliesse(Fenster),
    /// „Änderungen speichern?“ wie bisher.
    Frage,
}

fn fenster(id: WindowId) -> Fenster {
    match id {
        WindowId::Main => Fenster::Haupt,
        WindowId::Quantity => Fenster::Mengen,
        // Das Geschossfenster (Jörn 11.10.) öffnet in diesen Abläufen nie
        WindowId::Levels => unreachable!("Geschossfenster in F2"),
        WindowId::Ifc => unreachable!("IFC-Vorschau in F2"),
    }
}

fn id(f: Fenster) -> WindowId {
    match f {
        Fenster::Haupt => WindowId::Main,
        Fenster::Mengen => WindowId::Quantity,
    }
}

fn taten(v: Vec<Action>) -> Vec<Tat> {
    v.into_iter()
        .map(|a| match a {
            Action::Create(w, r) => Tat::Lege(fenster(w), r),
            Action::Place(w, r) => Tat::Setze(fenster(w), r),
            Action::Show(w, Show::Minimize) => Tat::Minimiere(fenster(w)),
            Action::Show(w, Show::RestoreNoActivate) => Tat::Hole(fenster(w)),
            Action::Show(w, Show::Maximize) => Tat::Maximiere(fenster(w)),
            Action::Front(w) => Tat::Vorne(fenster(w)),
            Action::Close(w) => Tat::Schliesse(fenster(w)),
            Action::AskSave => Tat::Frage,
        })
        .collect()
}

/// Neue Fensterlogik mit Breite des Mengenfensters in dip (einstellungen.txt).
fn logik(breite_dip: f32) -> Windows {
    Windows::new(breite_dip)
}

/// Knopf „Mengenermittlung“. `haupt`: Lage des Hauptfensters; `maximiert`
/// mit Arbeitsbereich des Bildschirms; `scale` 1,0 = 96 dpi.
fn oeffne(w: &mut Windows, haupt: R, maximiert: bool, arbeit: R, scale: f32) -> Vec<Tat> {
    taten(w.open_quantity(haupt, maximiert, arbeit, scale))
}

/// Hauptfenster wurde verschoben oder in der Größe geändert.
fn haupt_bewegt(w: &mut Windows, r: R) -> Vec<Tat> {
    taten(w.main_moved(r))
}

/// Mengenfenster an der Titelleiste gezogen (Lage nach dem Ziehen).
fn mengen_gezogen(w: &mut Windows, r: R) -> Vec<Tat> {
    taten(w.quantity_moved(r))
}

/// Mengenfenster am Rand in der Größe geändert.
fn mengen_groesse(w: &mut Windows, r: R) -> Vec<Tat> {
    taten(w.quantity_resized(r))
}

/// Windows meldet: Fenster wurde minimiert / wiederhergestellt / maximiert
/// (vom Nutzer, der Taskleiste, Win+D, oder als Folge einer eigenen Tat).
fn minimiert(w: &mut Windows, f: Fenster) -> Vec<Tat> {
    taten(w.minimized(id(f)))
}
fn wiederhergestellt(w: &mut Windows, f: Fenster) -> Vec<Tat> {
    taten(w.restored(id(f)))
}
fn maximiert(w: &mut Windows, f: Fenster) -> Vec<Tat> {
    taten(w.maximized(id(f)))
}

/// Schließen angefordert; `geaendert`: ungespeicherte Änderungen.
fn schliessen(w: &mut Windows, f: Fenster, geaendert: bool) -> Vec<Tat> {
    taten(w.close_requested(id(f), geaendert))
}

fn offen(w: &Windows) -> bool {
    w.quantity_open()
}

fn angedockt(w: &Windows) -> bool {
    w.docked()
}

fn lage(w: &Windows) -> R {
    w.quantity_rect()
}

/// Titel des Mengenfensters.
fn titel(doc: &crate::document::Document, m: &Model) -> String {
    crate::windows::quantity_caption(doc, m.revision())
}

/// Abschnitt in einstellungen.txt und zurück; `monitore`: Arbeitsbereiche
/// der angeschlossenen Bildschirme.
fn merken(w: &Windows) -> String {
    crate::windows::write_settings(w)
}
fn lesen(text: &str, monitore: &[R]) -> Windows {
    crate::windows::read_settings(text, monitore)
}

// Gemeinsamer Hover- und Auswahlzustand (eine Szene, beide Fenster lesen ihn)
use crate::picking::Picking;

fn waehle(p: &mut Picking, e: sk_model::ElementId, strg: bool) {
    p.click(e, strg)
}
fn esc(p: &mut Picking) {
    p.clear()
}
fn pruefe(p: &mut Picking, s: &Scene) {
    p.validate(s);
}
/// Farben der Hilfslinien, die das Hover-Bauteil in einer Ansicht hervorheben.
fn hover_farben(s: &Scene, p: &Picking, v: ViewKind, th: &Theme) -> Vec<[f32; 4]> {
    // Im Schnitt die Schnittlinie wie beim ersten Gebrauch (Mitte des Modells)
    let mut sect = SectionLine::default();
    sect.ensure(s);
    let plane = (v == ViewKind::Section).then(|| sect.plane()).flatten();
    crate::selection::hover_helpers(s, p.hover, v, plane, 1.0, th)
        .iter()
        .map(|h| h.color)
        .collect()
}

// ===== Hilfen =====

const HAUPT: R = (100, 100, 1200, 800);
const ARBEIT: R = (0, 0, 1920, 1040);

fn offen_angedockt() -> Windows {
    let mut w = logik(520.0);
    oeffne(&mut w, HAUPT, false, ARBEIT, 1.0);
    w
}

// ===== Tests =====

/// A96 (F2 Öffnen, Titel, Schließen): Der Knopf legt das Mengenfenster rechts
/// bündig an (gleiche Oberkante und Höhe, 520 dip × Skalierung); ein zweiter
/// Klick holt es nur nach vorn. Titel „Mengenermittlung – Datei“ mit „•“.
/// Schließen am Mengenfenster schließt nur dieses; am Hauptfenster beide,
/// mit der Nachfrage bei ungespeicherten Änderungen.
#[test]
fn a96_oeffnen_titel_schliessen() {
    let mut w = logik(520.0);
    assert!(!offen(&w));
    assert_eq!(
        oeffne(&mut w, HAUPT, false, ARBEIT, 1.0),
        [Tat::Lege(Fenster::Mengen, (1300, 100, 520, 800))]
    );
    assert!(offen(&w) && angedockt(&w));
    assert_eq!(
        oeffne(&mut w, HAUPT, false, ARBEIT, 1.0),
        [Tat::Vorne(Fenster::Mengen)],
        "kein zweites Fenster"
    );
    let mut w15 = logik(520.0);
    assert_eq!(
        oeffne(&mut w15, HAUPT, false, ARBEIT, 1.5),
        [
            Tat::Setze(Fenster::Haupt, (100, 100, 1040, 800)),
            Tat::Lege(Fenster::Mengen, (1140, 100, 780, 800))
        ],
        "520 dip bei 150 %: Hauptfenster gibt rechts Breite ab"
    );
    // Titel
    let mut s = Scene::with_model(Model::with_seed(96));
    let doc = crate::document::Document::new(s.model().revision());
    assert_eq!(
        titel(&doc, s.model()),
        format!("Mengenermittlung – {}", doc.caption(s.model()))
    );
    zeichne_rechteck(&mut s, &cam3d());
    assert!(titel(&doc, s.model()).ends_with(" •"), "ungespeichert");
    // Schließen
    assert_eq!(
        schliessen(&mut w, Fenster::Mengen, true),
        [Tat::Schliesse(Fenster::Mengen)],
        "nur das Mengenfenster, keine Nachfrage"
    );
    assert!(!offen(&w));
    let mut w = offen_angedockt();
    assert_eq!(schliessen(&mut w, Fenster::Haupt, true), [Tat::Frage]);
    let t = schliessen(&mut w, Fenster::Haupt, false);
    assert!(t.contains(&Tat::Schliesse(Fenster::Mengen)), "{t:?}");
    assert!(t.contains(&Tat::Schliesse(Fenster::Haupt)), "{t:?}");
}

/// A97 (F2 Andocken): Angedockt folgt das Mengenfenster dem Hauptfenster
/// (Verschieben, Größe); Ziehen bis 24 px bleibt angedockt, weiter löst es;
/// frei folgt es nicht mehr; bis 12 px heran rastet es ein. Angedockt ändert
/// nur der rechte Rand die Breite. Bei maximiertem Hauptfenster ⅔ zu ⅓,
/// nach dem Schließen wieder maximiert.
#[test]
fn a97_andocken_loesen_einrasten() {
    let mut w = offen_angedockt();
    assert_eq!(
        haupt_bewegt(&mut w, (300, 50, 1200, 800)),
        [Tat::Setze(Fenster::Mengen, (1500, 50, 520, 800))]
    );
    assert_eq!(
        haupt_bewegt(&mut w, (300, 50, 1000, 700)),
        [Tat::Setze(Fenster::Mengen, (1300, 50, 520, 700))],
        "Größe des Hauptfensters"
    );
    // Breite nur am rechten Rand: Versuch am linken Rand bleibt am Anker
    assert_eq!(
        mengen_groesse(&mut w, (1250, 50, 600, 700)),
        [Tat::Setze(Fenster::Mengen, (1300, 50, 550, 700))]
    );
    assert_eq!(lage(&w), (1300, 50, 550, 700));
    // 20 px weg: bleibt angedockt und springt zurück
    assert_eq!(
        mengen_gezogen(&mut w, (1320, 50, 550, 700)),
        [Tat::Setze(Fenster::Mengen, (1300, 50, 550, 700))]
    );
    assert!(angedockt(&w));
    // 30 px weg: frei, bleibt wo es ist
    assert_eq!(mengen_gezogen(&mut w, (1330, 80, 550, 700)), []);
    assert!(!angedockt(&w));
    assert_eq!(
        haupt_bewegt(&mut w, (0, 0, 1000, 700)),
        [],
        "frei: folgt nicht"
    );
    // Auf den zweiten Bildschirm und zurück
    assert_eq!(mengen_gezogen(&mut w, (2100, 80, 550, 700)), []);
    assert_eq!(
        mengen_gezogen(&mut w, (1020, 90, 550, 700)),
        [],
        "20 px vor dem Rand: noch frei"
    );
    assert_eq!(
        mengen_gezogen(&mut w, (1010, 90, 550, 700)),
        [Tat::Setze(Fenster::Mengen, (1000, 0, 550, 700))],
        "12 px heran: rastet ein, Oberkante und Höhe vom Hauptfenster"
    );
    assert!(angedockt(&w));
    // Maximiertes Hauptfenster: Arbeitsbereich teilen
    let mut w = logik(520.0);
    let t = oeffne(&mut w, ARBEIT, true, ARBEIT, 1.0);
    assert_eq!(
        t,
        [
            Tat::Setze(Fenster::Haupt, (0, 0, 1280, 1040)),
            Tat::Lege(Fenster::Mengen, (1280, 0, 640, 1040)),
        ]
    );
    let t = schliessen(&mut w, Fenster::Mengen, false);
    assert_eq!(
        t,
        [
            Tat::Schliesse(Fenster::Mengen),
            Tat::Maximiere(Fenster::Haupt)
        ],
        "zurück in den vorherigen Zustand"
    );
}

/// A98 (F2 Taskleiste): Minimieren und Wiederherstellen immer gemeinsam,
/// egal an welchem Fenster oder Eintrag; die Rückmeldung der eigenen Tat
/// stößt nicht zurück (Sperre). Maximieren betrifft nur das eigene Fenster.
/// Ohne Mengenfenster nichts.
#[test]
fn a98_gemeinsam_minimieren() {
    let mut w = offen_angedockt();
    assert_eq!(
        minimiert(&mut w, Fenster::Haupt),
        [Tat::Minimiere(Fenster::Mengen)]
    );
    assert_eq!(
        minimiert(&mut w, Fenster::Mengen),
        [],
        "Rückmeldung: Sperre"
    );
    assert_eq!(
        wiederhergestellt(&mut w, Fenster::Mengen),
        [Tat::Hole(Fenster::Haupt)],
        "Taskleiste Mengenfenster holt beide"
    );
    assert_eq!(wiederhergestellt(&mut w, Fenster::Haupt), [], "Rückmeldung");
    assert_eq!(
        minimiert(&mut w, Fenster::Mengen),
        [Tat::Minimiere(Fenster::Haupt)]
    );
    assert_eq!(minimiert(&mut w, Fenster::Haupt), []);
    assert_eq!(
        wiederhergestellt(&mut w, Fenster::Haupt),
        [Tat::Hole(Fenster::Mengen)]
    );
    assert_eq!(wiederhergestellt(&mut w, Fenster::Mengen), []);
    // Nach der Rückmeldung ist die Sperre wieder offen
    assert_eq!(
        minimiert(&mut w, Fenster::Haupt),
        [Tat::Minimiere(Fenster::Mengen)]
    );
    minimiert(&mut w, Fenster::Mengen);
    wiederhergestellt(&mut w, Fenster::Haupt);
    wiederhergestellt(&mut w, Fenster::Mengen);
    assert_eq!(maximiert(&mut w, Fenster::Mengen), [], "nur das eigene");
    assert_eq!(maximiert(&mut w, Fenster::Haupt), []);
    let mut zu = logik(520.0);
    assert_eq!(minimiert(&mut zu, Fenster::Haupt), [], "ohne Mengenfenster");
}

/// A99 (F2 Merken): Offen/zu, angedockt/frei, Lage, Größe und Bildschirm
/// stehen in einstellungen.txt und kommen beim Neustart zurück; fehlt der
/// gemerkte Bildschirm, wird wieder angedockt. Die .szo bleibt unberührt.
#[test]
fn a99_lage_merken() {
    let monitore = [ARBEIT, (1920, 0, 1920, 1040)];
    let mut w = offen_angedockt();
    mengen_gezogen(&mut w, (2100, 80, 600, 900));
    assert!(!angedockt(&w));
    let text = merken(&w);
    let w2 = lesen(&text, &monitore);
    assert!(offen(&w2) && !angedockt(&w2));
    assert_eq!(lage(&w2), (2100, 80, 600, 900));
    assert_eq!(merken(&w2), text, "verlustfrei");
    let w1 = lesen(&text, &monitore[..1]);
    assert!(offen(&w1) && angedockt(&w1), "Bildschirm fehlt: angedockt");
    let mut zu = offen_angedockt();
    schliessen(&mut zu, Fenster::Mengen, false);
    assert!(!offen(&lesen(&merken(&zu), &monitore)), "zu bleibt zu");
    assert!(!offen(&lesen("", &monitore)), "ohne Eintrag zu");
    // Nicht in der .szo
    let mut s = Scene::with_model(Model::with_seed(99));
    zeichne_rechteck(&mut s, &cam3d());
    let szo = sk_model::szo::write(s.model());
    assert!(!szo.contains("Mengen") && !szo.contains("quantity"));
}

/// A100 (F2 gemeinsamer Zustand): Hover und Auswahl gibt es einmal; beide
/// Fenster lesen ihn. Strg+Klick fügt hinzu und nimmt weg, Esc hebt auf,
/// ein Bauteil, das es nicht mehr gibt, fällt heraus. Hover hebt das Bauteil
/// in 3D, Grundriss und Schnitt in der Rolle `interact.hover_element` hervor.
/// Nichts davon ändert Modell, Rückgängig oder Datei.
#[test]
fn a100_gemeinsamer_hover_und_auswahl() {
    let th = Theme::dark();
    let mut s = Scene::with_model(Model::with_seed(100));
    let aw = zeichne_rechteck(&mut s, &cam3d());
    let iw = innenwand(
        &mut s,
        &cam3d(),
        vec3(5000.0, 0.0, 0.0),
        vec3(5000.0, 8000.0, 0.0),
    );
    let (a1, a2) = (
        s.model().wall_at(aw, 0).unwrap(),
        s.model().wall_at(aw, 1).unwrap(),
    );
    let i1 = s.model().wall_at(iw, 0).unwrap();
    let (rev, label, text) = (s.model().revision(), s.undo_label(), szo(&s));
    let mut p = Picking::default();
    waehle(&mut p, a1, false);
    waehle(&mut p, a2, true);
    assert_eq!(p.selected, [a1, a2], "Strg fügt hinzu");
    waehle(&mut p, a1, true);
    assert_eq!(p.selected, [a2], "Strg nimmt weg");
    waehle(&mut p, i1, false);
    assert_eq!(p.selected, [i1], "Klick ersetzt");
    p.hover = Some(a1);
    for v in [ViewKind::Persp, ViewKind::Plan, ViewKind::Front] {
        let f = hover_farben(&s, &p, v, &th);
        assert!(!f.is_empty(), "{v:?}: hervorgehoben");
        assert!(f.iter().all(|c| *c == th.interact.hover_element), "{v:?}");
    }
    let mut sect = SectionLine::default();
    sect.ensure(&s);
    assert!(!hover_farben(&s, &p, ViewKind::Section, &th).is_empty());
    p.hover = None;
    assert!(hover_farben(&s, &p, ViewKind::Persp, &th).is_empty());
    assert_eq!(
        (s.model().revision(), s.undo_label(), szo(&s)),
        (rev, label, text),
        "nur Sitzungszustand"
    );
    // Innenwand rückgängig: fällt aus der Auswahl
    p.hover = Some(i1);
    assert!(s.undo());
    pruefe(&mut p, &s);
    assert!(p.selected.is_empty() && p.hover.is_none());
    waehle(&mut p, a1, false);
    esc(&mut p);
    assert!(p.selected.is_empty(), "Esc");
    // Prüfregel 1: eigene Rolle im Schema
    assert_ne!(th.interact.hover_element, th.interact.select);
}

// Nachtrag zu F2 (Koordinator 14:36, Bauthread): zwei Erwartungen für den
// kleinen Nachtrag zur Fensterschicht, vorbereitet gegen main 023eb7a, nachgezogen auf 656204f.
// Gehört in das Modul `f2` von abnahme.rs (nutzt dessen Adapter `logik`,
// `oeffne`, `schliessen`, `mengen_gezogen`, `lage`, `Tat`, `Fenster`,
// `ARBEIT`). Spezifikation: test/abnahme-mengenliste.md (A96, A98).
//
// Stand 656204f: eingebaut als `Windows::maximize_main(main, work)`; leere
// Liste = normal maximieren lassen. Lesart des Bauthreads (abgenommen
// 2026-10-06): Das Hauptfenster füllt den Arbeitsbereich abzüglich der
// Breite des Mengenfensters, das Mengenfenster behält seine Breite und
// rückt an den rechten Rand des Arbeitsbereichs (volle Höhe). Nochmal
// maximieren bringt die Lage von vorher zurück. Die gleichen Fälle decken
// die Unit-Tests in sk-platform/src/layout.rs ab
// (`oeffnen_gibt_breite_ab_und_zurueck`, `maximieren_fuellt_den_rest`);
// diese Datei hebt sie in den Abnahmekatalog (A96/A98).

// ===== Adapter Nachtrag =====

/// Hauptfenster soll maximiert werden; leer: normal maximieren lassen.
fn haupt_maximiert(w: &mut Windows, haupt: R, arbeit: R) -> Vec<Tat> {
    taten(w.maximize_main(haupt, arbeit))
}

// ===== Tests =====

/// A96 Nachtrag (1): Reicht rechts neben dem Hauptfenster der Platz auf
/// demselben Bildschirm nicht für 520 dip, gibt das Hauptfenster rechts
/// Breite ab (linke Kante, Oberkante und Höhe bleiben); das Mengenfenster
/// liegt ganz auf dem Bildschirm. Beim Schließen bekommt das Hauptfenster
/// seine alte Breite zurück. Mit genug Platz ändert sich nichts.
#[test]
fn a96_nachtrag_platz_rechts_zu_knapp() {
    let haupt: R = (300, 100, 1500, 800); // rechte Kante 1800, Bildschirm 1920
    let mut w = logik(520.0);
    assert_eq!(
        oeffne(&mut w, haupt, false, ARBEIT, 1.0),
        [
            Tat::Setze(Fenster::Haupt, (300, 100, 1100, 800)),
            Tat::Lege(Fenster::Mengen, (1400, 100, 520, 800)),
        ]
    );
    assert_eq!(
        schliessen(&mut w, Fenster::Mengen, false),
        [
            Tat::Schliesse(Fenster::Mengen),
            Tat::Setze(Fenster::Haupt, haupt),
        ],
        "alte Breite zurück"
    );
    // 150 %: 780 px
    let mut w = logik(520.0);
    assert_eq!(
        oeffne(&mut w, haupt, false, ARBEIT, 1.5),
        [
            Tat::Setze(Fenster::Haupt, (300, 100, 840, 800)),
            Tat::Lege(Fenster::Mengen, (1140, 100, 780, 800)),
        ]
    );
    // Genug Platz (rechte Kante 1300 + 520 ≤ 1920): Hauptfenster bleibt
    let mut w = logik(520.0);
    assert_eq!(
        oeffne(&mut w, (100, 100, 1200, 800), false, ARBEIT, 1.0),
        [Tat::Lege(Fenster::Mengen, (1300, 100, 520, 800))]
    );
    assert_eq!(
        schliessen(&mut w, Fenster::Mengen, false),
        [Tat::Schliesse(Fenster::Mengen)]
    );
}

/// A98 Nachtrag (2): Maximieren des Hauptfensters bei angedocktem
/// Mengenfenster: Das Hauptfenster füllt den Arbeitsbereich abzüglich der
/// Breite des Mengenfensters, das Mengenfenster behält seine Breite und
/// liegt rechts daneben auf voller Höhe. Nochmal maximieren gibt die alte
/// Lage zurück. Ist es gelöst oder auf einem anderen Bildschirm, maximiert
/// das Hauptfenster normal.
#[test]
fn a98_nachtrag_maximieren_angedockt() {
    let haupt: R = (100, 100, 1200, 800);
    let mut w = logik(520.0);
    oeffne(&mut w, haupt, false, ARBEIT, 1.0);
    assert_eq!(lage(&w), (1300, 100, 520, 800));
    assert_eq!(
        haupt_maximiert(&mut w, haupt, ARBEIT),
        [
            Tat::Setze(Fenster::Haupt, (0, 0, 1400, 1040)),
            Tat::Setze(Fenster::Mengen, (1400, 0, 520, 1040)),
        ],
        "Arbeitsbereich minus Mengenfenster, Breite bleibt"
    );
    let t = haupt_maximiert(&mut w, (0, 0, 1400, 1040), ARBEIT);
    assert_eq!(
        t.first(),
        Some(&Tat::Setze(Fenster::Haupt, haupt)),
        "zurück"
    );
    // Gelöst: normal maximieren
    let mut w = logik(520.0);
    oeffne(&mut w, haupt, false, ARBEIT, 1.0);
    mengen_gezogen(&mut w, (1400, 150, 520, 800));
    assert!(haupt_maximiert(&mut w, haupt, ARBEIT).is_empty(), "gelöst");
    // Auf dem zweiten Bildschirm: normal maximieren
    mengen_gezogen(&mut w, (2100, 80, 520, 800));
    assert!(
        haupt_maximiert(&mut w, haupt, ARBEIT).is_empty(),
        "anderer Bildschirm"
    );
    // Ohne Mengenfenster: normal
    let mut zu = logik(520.0);
    assert!(haupt_maximiert(&mut zu, haupt, ARBEIT).is_empty());
}

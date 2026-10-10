use super::*;

// Abnahmetests A265–A267: Nachträge zu Paket 3/4 aus der Ist/Soll-Prüfung
// (einstellungen/pruefung.md, c0689d2 und 4839f4e: Befunde y, aa, ab;
// Koordinator 16:04, Sammel-Commit „Nachträge P3/P4“). Spezifikation:
// test/abnahme-baumpanel.md.
//
// Einbau: als `mod nachtraege_p34 { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, vec3.
//
// Angenommene Namen nur in den Adaptern: `crate::tree_panel::lock_faded`
// (bisher die private Entscheidung in `TreePanel::row_icons`) und
// `crate::delete::locked_lines` (Zeilen der Hinweiskarte aus
// `show_locked`, main.rs:4557). Beide als reine Funktionen.
//
// Ohne Test bleiben (nur Bildvergleich bzw. Messung): w (blasse Linien
// gleich hell), x (Splitter am Belag), 3h/G2 und B1–B4 (Leistung).

use sk_model::tree::{NodeKey, ProjectTree, Tab};
use sk_model::{Category, ElementId, RefSide, RunId};

// ===== Adapter =====

/// Schloss der Zeile `i` blass (wirkt nicht, weil alles darunter
/// abgeleitet ist und seiner Quelle folgt)?
fn schloss_blass(m: &Model, t: &ProjectTree, tab: Tab, i: usize) -> bool {
    crate::tree_panel::lock_faded(m, t, tab, i)
}

/// Zeilen der Hinweiskarte, wenn das Greifen von `gegriffen` an der
/// Sperre von `gesperrt` scheitert.
fn sperrkarte(m: &Model, gesperrt: ElementId, gegriffen: ElementId) -> Vec<String> {
    crate::delete::locked_lines(m, gesperrt, gegriffen)
}

// ===== Hilfen (wie mod baumpanel) =====

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

fn sperren(s: &mut Scene, ids: &[ElementId], an: bool) {
    let label = if an { "Gesperrt" } else { "Entsperrt" };
    s.edit_model(label, |m| {
        m.set_locked(ids, an);
        true
    });
}

/// Prüfhaus wie Paket 4: 10 × 8 m, IW-001, OG-Nord −1,50 (DT-001,
/// AB-001), OG-Ost +0,60 (UD-001).
fn pruefhaus(seed: u64) -> (Scene, RunId, RunId) {
    let mut s = Scene::with_model(Model::with_seed(seed));
    let (eg, og) = gebaeude(&mut s);
    assert!(s.edit_model("Innenwand", |m| {
        let st = m.run(eg).unwrap().storey;
        let t = m.default_type(sk_model::TypeCategory::InteriorWall);
        let iw = [vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)];
        m.add_wall_run(&iw, false, RefSide::Center, st, t, Category::InteriorWall)
            .is_some()
    }));
    for (w, d) in [("AW-006", -1500.0), ("AW-007", 600.0)] {
        let w = nr(&s, w);
        assert!(s.edit_model("Kopplung gelöst", |m| m.set_linked(w, false)));
        assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, d).is_some()));
    }
    for n in ["UD-001", "DT-001", "AB-001", "IW-001"] {
        nr(&s, n);
    }
    (s, eg, og)
}

fn baum(s: &Scene) -> ProjectTree {
    sk_model::tree::build(s.model())
}

/// Nummern der Bauteile unter Zeile `i`.
fn nummern(s: &Scene, t: &ProjectTree, tab: Tab, i: usize) -> Vec<String> {
    t.elements(tab, i)
        .iter()
        .map(|&id| s.model().element(id).unwrap().number.clone())
        .collect()
}

/// A265 (Ist/Soll ab, soll-p4-3): Karte „Bauteil“: Eine Art mit genau
/// einem Bauteil heißt in der Einzahl („Dachterrasse (1)“,
/// „Sohlplatte (1)“, „Innenwand (1)“), sonst in der Mehrzahl
/// („Außenwände (8)“). Gilt für jede Artzeile.
#[test]
fn a265_einzahl_bei_eins() {
    let (s, _, _) = pruefhaus(265);
    let t = baum(&s);
    let mut eins = Vec::new();
    for (i, n) in t.tab(Tab::Kind).iter().enumerate() {
        let NodeKey::Kind(cat) = n.key else { continue };
        let spec = sk_model::kinds::spec(cat);
        let k = t.elements(Tab::Kind, i).len();
        let soll = if k == 1 {
            format!("{} (1)", spec.short)
        } else {
            format!("{} ({k})", spec.plural)
        };
        assert_eq!(n.label, soll, "{cat:?}");
        if k == 1 {
            eins.push(n.label.clone());
        }
    }
    for x in ["Dachterrasse (1)", "Sohlplatte (1)", "Innenwand (1)"] {
        assert!(eins.iter().any(|l| l == x), "{x} fehlt: {eins:?}");
    }
    assert!(t.tab(Tab::Kind).iter().any(|n| n.label == "Außenwände (8)"));
}

/// A266 (Ist/Soll y, soll-p4-1/-3): Das Schloss einer Zeile ist blass,
/// wenn alles darunter abgeleitet ist (folgt seiner Quelle), in den
/// Karten „Baum“ und „Bauteil“. Artzeilen der Frostschürzen,
/// Sohlplatten, Decken, Dachterrassen und Attikableche sind blass;
/// Außen- und Innenwände nicht. Einzelzeilen wie bisher (DE-001 blass,
/// AW-005 nicht).
#[test]
fn a266_schloss_blass_bei_abgeleitetem() {
    let (s, _, _) = pruefhaus(266);
    let m = s.model();
    let t = baum(&s);
    let mut gesehen = Vec::new();
    for (i, n) in t.tab(Tab::Kind).iter().enumerate() {
        if !matches!(n.key, NodeKey::Kind(_)) {
            continue;
        }
        let ids = t.elements(Tab::Kind, i);
        let abgeleitet = ids.iter().all(|&id| m.lock_source(id) != id);
        assert_eq!(
            schloss_blass(m, &t, Tab::Kind, i),
            abgeleitet,
            "{}",
            n.label
        );
        let pre = nummern(&s, &t, Tab::Kind, i)[0][..2].to_string();
        gesehen.push((pre, abgeleitet));
    }
    for pre in ["FS", "SP", "DE", "DT", "AB"] {
        assert!(
            gesehen.contains(&(pre.to_string(), true)),
            "{pre} blass: {gesehen:?}"
        );
    }
    for pre in ["AW", "IW"] {
        assert!(
            gesehen.contains(&(pre.to_string(), false)),
            "{pre} wirkt: {gesehen:?}"
        );
    }
    let baum_tab = t.tab(Tab::Tree);
    for (nummer, blass) in [("DE-001", true), ("AW-005", false)] {
        let g = m.element(nr(&s, nummer)).unwrap().guid;
        let i = baum_tab
            .iter()
            .position(|n| n.key == NodeKey::Element(g))
            .unwrap_or_else(|| panic!("{nummer} ohne Zeile"));
        assert_eq!(schloss_blass(m, &t, Tab::Tree, i), blass, "{nummer}");
    }
}

/// A267 (Ist/Soll aa, soll-p4-7, paket-4 §1.7): Die Hinweiskarte nennt
/// das Warum. Ziehen am EG-Fuß unter der gekoppelten, gesperrten
/// AW-005: „AW-005 ist gesperrt.“ und „Das Ziehen würde die gekoppelte
/// OG-Wand mitbewegen.“ Greifen der gesperrten Wand selbst: nur die
/// erste Zeile.
#[test]
fn a267_sperrkarte_zweite_zeile() {
    let (mut s, _, _) = pruefhaus(267);
    let a5 = nr(&s, "AW-005");
    let unter = s.model().wall_below(a5).expect("AW-005 gestapelt");
    sperren(&mut s, &[a5], true);
    assert_eq!(
        sperrkarte(s.model(), a5, unter),
        [
            "AW-005 ist gesperrt.",
            "Das Ziehen würde die gekoppelte OG-Wand mitbewegen."
        ]
    );
    assert_eq!(sperrkarte(s.model(), a5, a5), ["AW-005 ist gesperrt."]);
}

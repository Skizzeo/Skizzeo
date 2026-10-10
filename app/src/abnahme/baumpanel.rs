use super::*;

// Abnahmetests A213–A227: Baumpanel und Sperre (Paket 4,
// projektstruktur/paket-4-baumpanel.md §4, Darstellung
// einstellungen/paket-p34-darstellung.md §4–7, Koordinator 11:24 und 11:36).
// Spezifikation: test/abnahme-baumpanel.md.
//
// Einbau: als `mod baumpanel { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, csv aus dem
// Mengenfenster. Braucht Paket 3 (Visibility) für A217.
//
// Angenommene Namen stehen nur in den Adaptern (Vorlage §2):
// `sk_model::tree::{build, Tab, NodeKey, Node, ProjectTree}`; die Karte
// ist eine vollständige Liste in Vorordnung (Kinder folgen ihrem Eltern
// mit `depth + 1`), unabhängig vom Auf- und Zuklappen. `Model::set_locked`,
// `lock_source`, `is_locked`, `sk_model::edit_blocked`,
// `Refusal::Locked(id)`, `Model::commit -> Result<Option<Txn>, Locked>`, `Trade::short`, `Model::set_trade_short`,
// `crate::wall_edit::drag_set` (Menge, die das Ziehen an einem Fuß ändern
// würde, mit Kopplungsstapel), `sk_model::tree::{eye, lock}` (Zustand
// eines Astes für Auge und Schloss), `crate::tree_panel::{row_at, icon_at}`.

use sk_model::tree::{NodeKey, ProjectTree, Tab};
use sk_model::view::Visibility;
use sk_model::{Category, ElementId, Guid, RefSide, Refusal, RunId, TradeId};

// ===== Adapter Paket 4a =====

fn baum(s: &Scene) -> ProjectTree {
    sk_model::tree::build(s.model())
}

fn karte(t: &ProjectTree, tab: Tab) -> &[sk_model::tree::Node] {
    let i = match tab {
        Tab::Tree => 0,
        Tab::Kind => 1,
        Tab::Trade => 2,
    };
    &t.tabs[i]
}

/// Bauteile unter einem Knoten.
fn mitglieder(t: &ProjectTree, n: &sk_model::tree::Node) -> Vec<ElementId> {
    t.members[n.elements.clone()].to_vec()
}

/// Sperren oder entsperren wie das Schloss im Baum: ein Schritt.
fn sperren(s: &mut Scene, ids: &[ElementId], an: bool) {
    let label = if an { "Gesperrt" } else { "Entsperrt" };
    s.edit_model(label, |m| {
        m.set_locked(ids, an);
        true
    });
}

fn gesperrt(m: &Model, id: ElementId) -> bool {
    m.is_locked(id)
}

fn quelle(m: &Model, id: ElementId) -> ElementId {
    m.lock_source(id)
}

fn blockiert(m: &Model, ids: &[ElementId]) -> Option<ElementId> {
    sk_model::edit_blocked(m, ids)
}

/// Bauteile, die das Ziehen am Fuß von `wall` ändern würde (ohne Strg).
fn zugmenge(s: &Scene, wall: ElementId) -> Vec<ElementId> {
    crate::wall_edit::drag_set(s.model(), wall)
}

/// `Model::commit` mit Sicherheitsnetz: `Err(id)` des gesperrten Bauteils.
fn abschliessen(m: &mut Model) -> Result<(), ElementId> {
    match m.try_commit() {
        Ok(_) => Ok(()),
        Err(sk_model::Locked(id)) => Err(id),
    }
}

/// Kurzname eines Gewerks (Regel 67), `None` wenn keiner gilt (auch
/// nach ausdrücklichem Entfernen mit `short=""`).
fn kurz(t: &sk_model::Trade) -> Option<String> {
    t.short.clone().filter(|k| !k.is_empty())
}

/// Kurzname eingeben wie im Paneel: `""` entfernt ihn; `false`, wenn
/// abgelehnt (doppelt, Ziffer, länger als 14 Zeichen).
fn kurz_setzen(m: &mut Model, tr: TradeId, k: &str) -> bool {
    m.set_trade_short(tr, k)
}

/// Name eines Gewerks im Baum: Kurzname, sonst Langname.
fn kurzname(m: &Model, tr: TradeId) -> String {
    let t = m.trade(tr).unwrap();
    kurz(t).unwrap_or_else(|| t.name.clone())
}

fn kurz_gesetzt(t: &sk_model::Trade) -> bool {
    kurz(t).is_some()
}

fn grund_gesperrt(id: ElementId) -> Refusal {
    Refusal::Locked(id)
}

/// Hinweistext des Löschens bzw. der Hinweiskarte.
fn sperrtext(m: &Model, id: ElementId, r: &Refusal) -> String {
    sk_model::refusal_text(m, id, r)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Zustand {
    Voll,
    Halb,
    Zu,
}

/// Auge eines Knotens: voll (alles sichtbar), halb, zu.
fn auge(s: &Scene, t: &ProjectTree, tab: Tab, i: usize) -> Zustand {
    match sk_model::tree::eye(s.model(), t, tab, i) {
        sk_model::tree::State::Full => Zustand::Voll,
        sk_model::tree::State::Partial => Zustand::Halb,
        sk_model::tree::State::None => Zustand::Zu,
    }
}

/// Schloss eines Knotens: Voll = alles gesperrt, Zu = nichts gesperrt.
fn schloss(s: &Scene, t: &ProjectTree, i: usize) -> Zustand {
    match sk_model::tree::lock(s.model(), t, i) {
        sk_model::tree::State::Full => Zustand::Voll,
        sk_model::tree::State::Partial => Zustand::Halb,
        sk_model::tree::State::None => Zustand::Zu,
    }
}

fn sicht(s: &mut Scene, f: impl FnOnce(&mut Visibility)) {
    let mut v = s.model().visibility().clone();
    f(&mut v);
    s.set_visibility(v);
}

// ===== Adapter Paket 4b =====

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Symbol {
    Auge,
    Schloss,
    Isolieren,
    Loeschen,
}

/// Zeile unter `y` (Pixel ab Oberkante der Zeilenliste) bei Rollstand
/// `scroll` (Pixel), `rows` Zeilen, Anzeigemaßstab `scale`.
fn zeile_bei(y: f64, scroll: f64, rows: usize, scale: f64) -> Option<usize> {
    crate::tree_panel::row_at(y, scroll, rows, scale)
}

/// Symbol unter `x` (Pixel links von der rechten Kante des Symbolstreifens)
/// in einer Zeile; `hover`: Zeile überfahren (Isolieren und Löschen da),
/// `lock_faded`: Schloss blass (abgeleitet), `delete_faded`: Löschen blass.
fn symbol_bei(
    x: f64,
    scale: f64,
    hover: bool,
    lock_faded: bool,
    delete_faded: bool,
) -> Option<Symbol> {
    use crate::tree_panel::Icon;
    let row = crate::tree_panel::RowIcons {
        hover,
        lock_faded,
        delete_faded,
    };
    crate::tree_panel::icon_at(x, scale, &row).map(|i| match i {
        Icon::Eye => Symbol::Auge,
        Icon::Lock => Symbol::Schloss,
        Icon::Isolate => Symbol::Isolieren,
        Icon::Delete => Symbol::Loeschen,
    })
}

// ===== Hilfen =====

fn nr(s: &Scene, nummer: &str) -> ElementId {
    s.model()
        .elements()
        .iter()
        .find(|(_, e)| e.number == nummer)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{nummer} fehlt"))
}

fn guid(s: &Scene, nummer: &str) -> Guid {
    s.model().element(nr(s, nummer)).unwrap().guid
}

fn gewerk(s: &Scene, code: &str) -> TradeId {
    s.model().trade_by_code(code).unwrap()
}

fn storey_guid(s: &Scene, name: &str) -> Guid {
    s.model()
        .storeys()
        .iter()
        .find(|(_, st)| st.name == name)
        .map(|(_, st)| st.guid)
        .unwrap()
}

fn csv(s: &mut Scene) -> Vec<u8> {
    let l = s.schedule().clone();
    crate::schedule_view::csv(s.model(), &l)
}

/// Prüfhaus 10 × 8 m, AW-31,5, eine Innenwand im EG bei x = 5 m. OG:
/// AW-006 (Nord) gelöst und 1,50 m zurück (DT-001, AB-001), AW-007 gelöst
/// und 0,60 m vor (UD-001). AW-005 und AW-008 bleiben gekoppelt.
/// Bauteile: AW-001…008, IW-001, SP-001, FS-001, DE-001, DE-002, UD-001,
/// DT-001, AB-001.
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
    assert!(s.model().check().is_empty(), "{:?}", s.model().check());
    for n in ["UD-001", "DT-001", "AB-001", "IW-001"] {
        nr(&s, n);
    }
    (s, eg, og)
}

/// Direkte Kinder von Knoten `i` (Vorordnung mit Tiefe).
fn kinder(n: &[sk_model::tree::Node], i: usize) -> Vec<usize> {
    let d = n[i].depth;
    let mut out = Vec::new();
    for (k, x) in n.iter().enumerate().skip(i + 1) {
        if x.depth <= d {
            break;
        }
        if x.depth == d + 1 {
            out.push(k);
        }
    }
    out
}

fn finde(n: &[sk_model::tree::Node], key: &NodeKey) -> usize {
    n.iter()
        .position(|x| &x.key == key)
        .unwrap_or_else(|| panic!("{key:?} fehlt"))
}

fn rang(c: Category) -> u8 {
    sk_model::kinds::spec(c).qto_rank
}

fn kategorie(s: &Scene, key: &NodeKey) -> Category {
    match key {
        NodeKey::Group(_, c) | NodeKey::Kind(c) | NodeKey::KindType(c, _) => *c,
        NodeKey::Element(g) | NodeKey::TradeElement(_, g) => {
            s.model()
                .elements()
                .iter()
                .find(|(_, e)| e.guid == *g)
                .unwrap()
                .1
                .category
        }
        _ => panic!("{key:?} hat keine Art"),
    }
}

/// Nummern der Bauteile, sortiert.
fn nummern(s: &Scene, ids: &[ElementId]) -> Vec<String> {
    let mut v: Vec<String> = ids
        .iter()
        .map(|&id| s.model().element(id).unwrap().number.clone())
        .collect();
    v.sort();
    v
}

fn sortiert<T: Clone + Ord>(v: &[T]) -> Vec<T> {
    let mut v = v.to_vec();
    v.sort();
    v
}

/// A213 (§1.2, §2.1, §4.1; p34 §6.2): Karte „Baum“ am Prüfhaus. Projekt
/// → Gelände, Gebäude → Geschosse von oben: Obergeschoss, Erdgeschoss,
/// Gründung (Name ohne Höhe). Gruppen nach `qto_rank`; leere Arten
/// fehlen; eine Art mit genau einem Bauteil ist eine Bauteilzeile mit dem
/// Kurznamen („Decke“). DT, AB und UD stehen beim EG, SP und FS bei der
/// Gründung (wie das Mengenfenster). Das Gebäude umfasst alle Bauteile.
#[test]
fn a213_baum_karte() {
    let (s, _, _) = pruefhaus(213);
    let t = baum(&s);
    let n = karte(&t, Tab::Tree);
    assert_eq!(n[0].key, NodeKey::Project);
    assert_eq!(n[0].depth, 0);
    let oben: Vec<&NodeKey> = kinder(n, 0).iter().map(|&k| &n[k].key).collect();
    assert_eq!(oben.len(), 2);
    assert_eq!(oben[0], &NodeKey::Terrain);
    assert!(matches!(oben[1], NodeKey::Building(_)));
    let gb = kinder(n, 0)[1];
    let alle: Vec<ElementId> = s.model().elements().iter().map(|(id, _)| id).collect();
    assert_eq!(
        nummern(&s, &mitglieder(&t, &n[gb])),
        nummern(&s, &alle),
        "Gebäude umfasst alles"
    );

    let geschosse: Vec<&str> = kinder(n, gb).iter().map(|&k| n[k].label.as_str()).collect();
    assert_eq!(geschosse, ["Obergeschoss", "Erdgeschoss", "Gründung"]);
    for (name, soll) in [
        (
            "Obergeschoss",
            vec!["AW-005", "AW-006", "AW-007", "AW-008", "DE-002"],
        ),
        (
            "Erdgeschoss",
            vec![
                "AW-001", "AW-002", "AW-003", "AW-004", "IW-001", "DE-001", "UD-001", "DT-001",
                "AB-001",
            ],
        ),
        ("Gründung", vec!["SP-001", "FS-001"]),
    ] {
        let sg = storey_guid(&s, name);
        let i = finde(n, &NodeKey::Storey(sg));
        assert_eq!(n[i].label, name, "ohne Höhe");
        let ids: Vec<ElementId> = soll.iter().map(|x| nr(&s, x)).collect();
        assert_eq!(
            nummern(&s, &mitglieder(&t, &n[i])),
            nummern(&s, &ids),
            "{name}"
        );
        // Gruppen: Außenwände (4) als Gruppe, alles andere einzeln
        let k = kinder(n, i);
        let raenge: Vec<u8> = k.iter().map(|&c| rang(kategorie(&s, &n[c].key))).collect();
        assert!(
            raenge.windows(2).all(|w| w[0] <= w[1]),
            "{name}: {raenge:?}"
        );
        for &c in &k {
            match &n[c].key {
                NodeKey::Group(g, cat) => {
                    assert_eq!(*g, sg);
                    assert_eq!(*cat, Category::ExteriorWall);
                    assert_eq!(n[c].label, "Außenwände (4)");
                    assert_eq!(mitglieder(&t, &n[c]).len(), 4);
                    assert_eq!(kinder(n, c).len(), 4, "Bauteile unter der Gruppe");
                }
                NodeKey::Element(g) => {
                    let cat = kategorie(&s, &n[c].key);
                    assert_eq!(n[c].label, sk_model::kinds::spec(cat).short, "{g:?}");
                    assert_eq!(mitglieder(&t, &n[c]).len(), 1);
                }
                k => panic!("{name}: unerwartet {k:?}"),
            }
        }
        let gruppen = k
            .iter()
            .filter(|&&c| matches!(n[c].key, NodeKey::Group(..)))
            .count();
        assert_eq!(gruppen, if name == "Gründung" { 0 } else { 1 });
    }
    assert_eq!(
        n[finde(n, &NodeKey::Element(guid(&s, "DE-001")))].label,
        "Decke"
    );
}

/// A214 (§1.3, p34 §6.2): Karte „Bauteil“. Nur vorkommende Arten nach
/// `qto_rank`, Beschriftung „Außenwände (8)“; aufgeklappte Bauteile
/// tragen die Nummer als Namen. Hat eine Art mehr als einen Typ, steht
/// eine Typzeile dazwischen („… · 4×“), sonst nicht.
#[test]
fn a214_bauteil_karte() {
    let (mut s, eg, _) = pruefhaus(214);
    let t = baum(&s);
    let n = karte(&t, Tab::Kind);
    let oben: Vec<usize> = (0..n.len()).filter(|&i| n[i].depth == 0).collect();
    let arten: Vec<Category> = oben
        .iter()
        .map(|&i| match n[i].key {
            NodeKey::Kind(c) => c,
            ref k => panic!("{k:?}"),
        })
        .collect();
    let mut soll: Vec<Category> = s
        .model()
        .elements()
        .iter()
        .map(|(_, e)| e.category)
        .collect();
    soll.sort_by_key(|c| (rang(*c), *c));
    soll.dedup();
    assert_eq!(sortiert(&arten), sortiert(&soll), "nur vorkommende Arten");
    assert!(arten.windows(2).all(|w| rang(w[0]) <= rang(w[1])));
    let aw = finde(n, &NodeKey::Kind(Category::ExteriorWall));
    assert_eq!(n[aw].label, "Außenwände (8)");
    let k = kinder(n, aw);
    assert_eq!(k.len(), 8, "ein Typ: keine Typzeile");
    for &c in &k {
        assert!(matches!(n[c].key, NodeKey::Element(_)));
        let id = mitglieder(&t, &n[c])[0];
        assert_eq!(
            n[c].label,
            s.model().element(id).unwrap().number,
            "Nummer als Name"
        );
    }

    let t49 = s
        .model()
        .layer_sets()
        .iter()
        .find(|(_, x)| x.code == "AW-49")
        .map(|(id, _)| id)
        .unwrap();
    // freie Außenwand mit zweitem Typ (Typwechsel am Haus wirkt auf den
    // ganzen Stapel, darum ein eigener Zug)
    assert!(s.edit_model("Wand zeichnen", |m| {
        let st = m.run(eg).unwrap().storey;
        let w = [vec3(12000.0, 0.0, 0.0), vec3(15000.0, 0.0, 0.0)];
        m.add_wall_run(&w, false, RefSide::Left, st, t49, Category::ExteriorWall)
            .is_some()
    }));
    let t = baum(&s);
    let n = karte(&t, Tab::Kind);
    let aw = finde(n, &NodeKey::Kind(Category::ExteriorWall));
    assert_eq!(n[aw].label, "Außenwände (9)");
    let k = kinder(n, aw);
    assert_eq!(k.len(), 2, "zwei Typen: zwei Typzeilen");
    let mut anzahl: Vec<usize> = Vec::new();
    for &c in &k {
        assert!(matches!(
            n[c].key,
            NodeKey::KindType(Category::ExteriorWall, _)
        ));
        let z = kinder(n, c).len();
        assert!(n[c].label.contains(&format!("{z}×")), "{}", n[c].label);
        anzahl.push(z);
    }
    assert_eq!(sortiert(&anzahl), [1, 8]);
}

/// A215 (§1.4, §4.1): Karte „Gewerk“. Nur vorkommende Gewerke nach
/// `Trade.order`: Beton 18331, Mauer 18330, Dachdecker 18338, WDVS 18345.
/// Unter 18338 genau DT-001 (einmal, obwohl zwei Schichten) und AB-001.
/// Eine WDVS-Wand steht unter zwei Gewerken, mit Schicht als Hinweis.
/// Die Gewerkzeile zeigt den Kurznamen (`short=`, BIM), sonst den
/// Langnamen; die Normnummer steht nicht in der Zeile (nur im Hinweis).
#[test]
fn a215_gewerk_karte() {
    let (s, _, _) = pruefhaus(215);
    let t = baum(&s);
    let n = karte(&t, Tab::Trade);
    let oben: Vec<usize> = (0..n.len()).filter(|&i| n[i].depth == 0).collect();
    let codes: Vec<String> = oben
        .iter()
        .map(|&i| match n[i].key {
            NodeKey::Trade(tr) => s.model().trade(tr).unwrap().code.clone(),
            ref k => panic!("{k:?}"),
        })
        .collect();
    // 18351: Bekleidung unter der UD (W3)
    assert_eq!(codes, ["18331", "18330", "18338", "18345", "18351"]);
    for &i in &oben {
        let NodeKey::Trade(tr) = n[i].key else {
            unreachable!()
        };
        assert_eq!(
            n[i].label,
            kurzname(s.model(), tr),
            "Kurzname, sonst Langname"
        );
        assert!(
            !n[i].label.chars().any(|c| c.is_ascii_digit()),
            "{}",
            n[i].label
        );
    }
    for code in ["18338", "18345"] {
        let tr = s.model().trade(gewerk(&s, code)).unwrap();
        assert!(kurz_gesetzt(tr), "{code} hat einen Kurznamen (BIM)");
    }
    let unter = |code: &str| -> Vec<String> {
        let i = finde(n, &NodeKey::Trade(gewerk(&s, code)));
        let mut v: Vec<String> = kinder(n, i)
            .iter()
            .map(|&c| {
                let NodeKey::TradeElement(tr, _) = n[c].key else {
                    panic!("{:?}", n[c].key)
                };
                assert_eq!(tr, gewerk(&s, code));
                let id = mitglieder(&t, &n[c])[0];
                s.model().element(id).unwrap().number.clone()
            })
            .collect();
        v.sort();
        v
    };
    assert_eq!(unter("18338"), ["AB-001", "DT-001"]);
    assert_eq!(unter("18331"), ["DE-001", "DE-002", "FS-001", "SP-001"]);
    let aw: Vec<String> = (1..=8).map(|k| format!("AW-00{k}")).collect();
    let mut maurer = aw.clone();
    maurer.push("IW-001".into());
    maurer.sort();
    assert_eq!(unter("18330"), maurer);
    let mut wdvs = aw.clone();
    wdvs.push("UD-001".into());
    wdvs.sort();
    assert_eq!(unter("18345"), wdvs);
    let g = guid(&s, "AW-001");
    let i = finde(n, &NodeKey::TradeElement(gewerk(&s, "18330"), g));
    let h = n[i].layer_hint.clone().expect("Schicht als Hinweis");
    assert!(h.contains("Porenbeton"), "{h}");
    assert!(n
        .iter()
        .any(|x| x.key == NodeKey::TradeElement(gewerk(&s, "18345"), g)));
}

/// A216 (§2.1, §4.2): Schlüssel sind stabil. Wand ziehen und Baum neu
/// bauen ergibt dieselben `NodeKey`s in allen drei Karten. Ein neues
/// Bauteil fügt nur Schlüssel hinzu.
#[test]
fn a216_schluessel_stabil() {
    let (mut s, eg, _) = pruefhaus(216);
    let keys = |s: &Scene| -> Vec<Vec<NodeKey>> {
        let t = baum(s);
        [Tab::Tree, Tab::Kind, Tab::Trade]
            .into_iter()
            .map(|tab| karte(&t, tab).iter().map(|x| x.key.clone()).collect())
            .collect()
    };
    let vorher = keys(&s);
    let w = nr(&s, "AW-006");
    assert!(s.edit_model("Wand verschoben", |m| m.move_segment(w, -300.0).is_some()));
    assert_eq!(keys(&s), vorher, "gleiche Schlüssel nach Ziehen");
    assert!(s.edit_model("Innenwand", |m| {
        let st = m.run(eg).unwrap().storey;
        let t = m.default_type(sk_model::TypeCategory::InteriorWall);
        let iw = [vec3(0.0, 4000.0, 0.0), vec3(5000.0, 4000.0, 0.0)];
        m.add_wall_run(&iw, false, RefSide::Center, st, t, Category::InteriorWall)
            .is_some()
    }));
    let nachher = keys(&s);
    for (a, b) in vorher.iter().zip(&nachher) {
        for k in a {
            if !matches!(k, NodeKey::Element(_)) || b.contains(k) {
                continue;
            }
            // Die eine Innenwand wird zur Gruppe: ihre Bauteilzeile
            // wandert unter „Innenwände (2)“, der Schlüssel bleibt.
            panic!("{k:?} verschwunden");
        }
    }
}

/// A217 (§1.5, §4.3): Auge und Schloss am Ast. Auge: voll ohne
/// Ausgeblendetes; halb, wenn ein Teil ausgeblendet ist (bis hinauf zum
/// Projekt); zu, wenn alles darunter ausgeblendet ist. Gewerkzeile zu,
/// wenn das Gewerk ausgeblendet ist. Schloss: zu ohne Sperre, halb bei
/// einem gesperrten Bauteil, voll bei allen.
#[test]
fn a217_auge_und_schloss_am_ast() {
    let (mut s, _, _) = pruefhaus(217);
    let eg = storey_guid(&s, "Erdgeschoss");
    let t = baum(&s);
    let n = karte(&t, Tab::Tree);
    let gruppe = finde(n, &NodeKey::Group(eg, Category::ExteriorWall));
    let geschoss = finde(n, &NodeKey::Storey(eg));
    assert_eq!(auge(&s, &t, Tab::Tree, gruppe), Zustand::Voll);
    assert_eq!(auge(&s, &t, Tab::Tree, 0), Zustand::Voll);
    let g1 = guid(&s, "AW-001");
    sicht(&mut s, |v| {
        v.hidden.insert(g1);
    });
    assert_eq!(auge(&s, &t, Tab::Tree, gruppe), Zustand::Halb);
    assert_eq!(auge(&s, &t, Tab::Tree, geschoss), Zustand::Halb);
    assert_eq!(auge(&s, &t, Tab::Tree, 0), Zustand::Halb, "bis zum Projekt");
    let vier: Vec<Guid> = ["AW-001", "AW-002", "AW-003", "AW-004"]
        .iter()
        .map(|x| guid(&s, x))
        .collect();
    sicht(&mut s, |v| v.hidden.extend(vier));
    assert_eq!(auge(&s, &t, Tab::Tree, gruppe), Zustand::Zu);
    sicht(&mut s, |v| {
        v.hidden.clear();
        v.hidden_cat.insert(Category::ExteriorWall);
    });
    assert_eq!(auge(&s, &t, Tab::Tree, gruppe), Zustand::Zu);
    let k = karte(&t, Tab::Kind);
    let art = finde(k, &NodeKey::Kind(Category::ExteriorWall));
    assert_eq!(auge(&s, &t, Tab::Kind, art), Zustand::Zu);
    let wdvs = gewerk(&s, "18345");
    sicht(&mut s, |v| {
        v.hidden_cat.clear();
        v.hidden_trade.insert(wdvs);
    });
    let g = karte(&t, Tab::Trade);
    assert_eq!(
        auge(&s, &t, Tab::Trade, finde(g, &NodeKey::Trade(wdvs))),
        Zustand::Zu
    );
    let beton = gewerk(&s, "18331");
    assert_eq!(
        auge(&s, &t, Tab::Trade, finde(g, &NodeKey::Trade(beton))),
        Zustand::Voll
    );

    assert_eq!(schloss(&s, &t, gruppe), Zustand::Zu);
    let a1 = nr(&s, "AW-001");
    sperren(&mut s, &[a1], true);
    let t = baum(&s);
    assert_eq!(schloss(&s, &t, gruppe), Zustand::Halb);
    let ids: Vec<ElementId> = ["AW-001", "AW-002", "AW-003", "AW-004"]
        .iter()
        .map(|x| nr(&s, x))
        .collect();
    sperren(&mut s, &ids, true);
    let t = baum(&s);
    assert_eq!(schloss(&s, &t, gruppe), Zustand::Voll);
}

/// A218 (§1.7, §2.2, §4.4): Sperren ist ein Schritt „Gesperrt“ bzw.
/// „Entsperrt“; Strg+Z und Strg+Y wirken. Mengen, Guid und Nummer bleiben.
#[test]
fn a218_sperren_rueckgaengig() {
    let (mut s, _, _) = pruefhaus(218);
    let mengen = csv(&mut s);
    let a5 = nr(&s, "AW-005");
    let g5 = guid(&s, "AW-005");
    assert!(!gesperrt(s.model(), a5));
    sperren(&mut s, &[a5], true);
    assert!(gesperrt(s.model(), a5));
    assert_eq!(s.undo_label(), Some("Gesperrt"));
    assert_eq!(csv(&mut s), mengen, "Mengen bleiben");
    assert_eq!(guid(&s, "AW-005"), g5);
    assert!(s.undo());
    assert!(!gesperrt(s.model(), a5), "Strg+Z");
    assert!(s.redo());
    assert!(gesperrt(s.model(), a5), "Strg+Y");
    sperren(&mut s, &[a5], false);
    assert_eq!(s.undo_label(), Some("Entsperrt"));
    assert!(!gesperrt(s.model(), a5));
}

/// A219 (§2.2, Koordinator 11:24): `lock_source` folgt der ganzen Kette
/// `Refusal::Derived` bis zu einer Wand: DT → DE → AW, FS → SP → AW,
/// UD → DE → AW. AW-001 sperren: `is_locked` für DE-001, DT-001, AB-001,
/// UD-001, SP-001 und FS-001 jeweils true, ohne Sperre jeweils false.
/// DE-002 hängt am OG und bleibt frei.
#[test]
fn a219_sperrquelle_kette() {
    let (mut s, _, _) = pruefhaus(219);
    let m = s.model();
    for (id, e) in m.elements().iter() {
        if matches!(m.can_delete(id), Err(Refusal::Derived { .. })) {
            let q = quelle(m, id);
            let c = m.element(q).unwrap().category;
            assert!(
                matches!(c, Category::ExteriorWall | Category::InteriorWall),
                "{} → {c:?}",
                e.number
            );
        } else {
            assert_eq!(quelle(m, id), id, "{} ist seine eigene Quelle", e.number);
        }
    }
    let a1 = nr(&s, "AW-001");
    let kette = ["DE-001", "DT-001", "AB-001", "UD-001", "SP-001", "FS-001"];
    for x in kette {
        assert_eq!(quelle(s.model(), nr(&s, x)), a1, "{x}");
        assert!(!gesperrt(s.model(), nr(&s, x)), "{x} ohne Sperre");
    }
    sperren(&mut s, &[a1], true);
    for x in kette {
        assert!(gesperrt(s.model(), nr(&s, x)), "{x} folgt AW-001");
    }
    assert!(!gesperrt(s.model(), nr(&s, "DE-002")));
    assert!(!gesperrt(s.model(), nr(&s, "AW-002")));
    let a5 = nr(&s, "AW-005");
    sperren(&mut s, &[a1], false);
    sperren(&mut s, &[a5], true);
    assert!(gesperrt(s.model(), nr(&s, "DE-002")));
    for x in kette {
        assert!(!gesperrt(s.model(), nr(&s, x)), "{x}");
    }
}

/// A220 (§1.7, §2.2, §4.4): `edit_blocked` beim Greifen. Ziehen am
/// EG-Fuß unter einer gekoppelten, gesperrten OG-Wand: blockiert durch
/// die OG-Wand. Eigene Sperre: blockiert. Gelöste gesperrte OG-Wand
/// darüber: nicht blockiert (sie bleibt stehen). Bündig setzen an einer
/// gesperrten Wand, Typ ändern am Zug mit einem gesperrten Segment und
/// Zahlenfeld einer Decke mit gesperrter Quelle: blockiert. Ohne Sperre
/// nie blockiert.
#[test]
fn a220_greifen_blockiert() {
    let (mut s, eg, _) = pruefhaus(220);
    let alle: Vec<ElementId> = s.model().elements().iter().map(|(id, _)| id).collect();
    assert_eq!(blockiert(s.model(), &alle), None, "ohne Sperre");
    let a5 = nr(&s, "AW-005");
    let unter5 = s.model().wall_below(a5).expect("AW-005 gestapelt");
    assert!(zugmenge(&s, unter5).contains(&a5), "gekoppelt zieht mit");
    assert_eq!(blockiert(s.model(), &zugmenge(&s, unter5)), None);
    sperren(&mut s, &[a5], true);
    assert_eq!(blockiert(s.model(), &zugmenge(&s, unter5)), Some(a5));
    sperren(&mut s, &[a5], false);
    sperren(&mut s, &[unter5], true);
    assert_eq!(blockiert(s.model(), &zugmenge(&s, unter5)), Some(unter5));
    sperren(&mut s, &[unter5], false);

    let a6 = nr(&s, "AW-006");
    let unter6 = s.model().wall_below(a6).expect("AW-006 gestapelt");
    sperren(&mut s, &[a6], true);
    assert!(!zugmenge(&s, unter6).contains(&a6), "gelöst bleibt stehen");
    assert_eq!(blockiert(s.model(), &zugmenge(&s, unter6)), None);
    assert_eq!(blockiert(s.model(), &[a6]), Some(a6), "Bündig setzen");
    sperren(&mut s, &[a6], false);

    let zug: Vec<ElementId> = (0..4).filter_map(|i| s.model().wall_at(eg, i)).collect();
    let a2 = nr(&s, "AW-002");
    sperren(&mut s, &[a2], true);
    assert_eq!(blockiert(s.model(), &zug), Some(a2), "Typ ändern");
    sperren(&mut s, &[a2], false);

    let a1 = nr(&s, "AW-001");
    let de = nr(&s, "DE-001");
    sperren(&mut s, &[a1], true);
    let b = blockiert(s.model(), &[de]);
    assert!(b == Some(de) || b == Some(a1), "Zahlenfeld Decke: {b:?}");
}

/// A221 (§1.7): Abgeleitete Bauteile ändern sich weiter. Das ganze EG
/// ist gesperrt (Quelle von DE-001 und damit von DT-001). Die nicht
/// gesperrte, gelöste OG-Wand AW-006 lässt sich ziehen, und die
/// Dachterrasse folgt. Das Sicherheitsnetz greift nicht.
#[test]
fn a221_abgeleitetes_folgt_trotz_sperre() {
    let (mut s, _, _) = pruefhaus(221);
    let eg: Vec<ElementId> = ["AW-001", "AW-002", "AW-003", "AW-004"]
        .iter()
        .map(|x| nr(&s, x))
        .collect();
    sperren(&mut s, &eg, true);
    let dt = nr(&s, "DT-001");
    assert!(gesperrt(s.model(), dt), "DT folgt der Sperre der Quelle");
    let a6 = nr(&s, "AW-006");
    assert_eq!(blockiert(s.model(), &zugmenge(&s, a6)), None);
    let vorher = s.element_bounds(dt).unwrap();
    let versatz = s.model().stack_offset(a6).unwrap().0;
    assert!(s.edit_model("Wand verschoben", |m| m.move_segment(a6, -300.0).is_some()));
    assert_eq!(s.undo_label(), Some("Wand verschoben"));
    assert!((s.model().stack_offset(a6).unwrap().0 - (versatz - 300.0)).abs() < 0.5);
    let nachher = s.element_bounds(nr(&s, "DT-001")).unwrap();
    assert!(
        (nachher.0.y - vorher.0.y).abs() > 100.0,
        "DT folgt: {vorher:?} → {nachher:?}"
    );
    for &w in &eg {
        assert!(gesperrt(s.model(), w));
    }
}

/// A222 (§2.2): Sicherheitsnetz in `commit`. Ein Schritt, der eine
/// gesperrte Wand verschiebt oder ihren Typ ändert, wird ganz
/// zurückgerollt (`Err(Locked)`), kein Schritt offen, Lage unverändert.
/// Nur `locked` selbst zu ändern ist erlaubt. In der App bleibt der
/// Verlauf unverändert.
#[test]
fn a222_sicherheitsnetz() {
    let (mut s, _, og) = pruefhaus(222);
    let a5 = nr(&s, "AW-005");
    sperren(&mut s, &[a5], true);
    let lage = s.model().stack_offset(a5);
    let mut m = s.model().clone();
    m.begin("Wand verschoben");
    m.set_linked(a5, false);
    m.move_segment(a5, 300.0);
    assert_eq!(abschliessen(&mut m), Err(a5));
    assert!(!m.in_step());
    assert_eq!(m.stack_offset(a5), lage, "zurückgerollt");

    let t49 = m
        .layer_sets()
        .iter()
        .find(|(_, x)| x.code == "AW-49")
        .map(|(id, _)| id)
        .unwrap();
    m.begin("Wandtyp");
    m.set_run_type(og, t49);
    assert_eq!(abschliessen(&mut m), Err(a5), "Typ einer gesperrten Wand");
    assert_ne!(m.element(a5).unwrap().layer_set, Some(t49));

    m.begin("Entsperrt");
    m.set_locked(&[a5], false);
    assert_eq!(abschliessen(&mut m), Ok(()), "nur locked");
    assert!(!m.is_locked(a5));

    let schritt = s.undo_label();
    s.edit_model("Wand verschoben", |m| m.move_segment(a5, 300.0).is_some());
    assert_eq!(s.model().stack_offset(a5), lage, "App: nichts bewegt");
    assert_eq!(s.undo_label(), schritt, "kein Schritt im Verlauf");
}

/// A223 (§2.2 Löschregel, p34 Bild p4-7): Gesperrtes lässt sich nicht
/// löschen: `can_delete` → `Refusal::Locked`, `delete_elements` lehnt
/// ab, Text „IW-001 ist gesperrt.“ (Nummer des Bauteils). Entsperrt geht
/// es wieder.
#[test]
fn a223_loeschen_gesperrt() {
    let (mut s, _, _) = pruefhaus(223);
    let iw = nr(&s, "IW-001");
    assert_eq!(s.model().can_delete(iw), Ok(()));
    sperren(&mut s, &[iw], true);
    assert_eq!(s.model().can_delete(iw), Err(grund_gesperrt(iw)));
    let d = s.delete_elements(&[iw]);
    assert!(d.removed.is_empty());
    assert_eq!(d.refused, vec![(iw, grund_gesperrt(iw))]);
    assert!(s.model().element(iw).is_some(), "steht noch");
    let text = sperrtext(s.model(), iw, &grund_gesperrt(iw));
    assert!(text.starts_with("IW-001 ist gesperrt."), "{text}");
    sperren(&mut s, &[iw], false);
    assert_eq!(s.model().can_delete(iw), Ok(()));
}

/// A224 (§2.3, F-17): `.szo` bleibt 4. Je gesperrtes Bauteil eine Zeile
/// `[lock] elem=<guid>`, Rundlauf bytegleich, die Sperre ist nach dem
/// Öffnen da. Ohne Sperre (auch nach Sperren und Entsperren) ist die
/// Datei bytegleich wie vorher. Eine Zeile auf ein unbekanntes Bauteil
/// wird mit Hinweis verworfen (Sperre ist Modell).
#[test]
fn a224_datei_lock() {
    let (mut s, _, _) = pruefhaus(224);
    let ohne = sk_model::szo::write(s.model());
    assert!(!ohne.contains("[lock]"));
    let a5 = nr(&s, "AW-005");
    let iw = nr(&s, "IW-001");
    sperren(&mut s, &[a5, iw], true);
    let text = sk_model::szo::write(s.model());
    assert!(text.starts_with("SZO 4\n"));
    let zeilen: Vec<&str> = text.lines().filter(|l| l.starts_with("[lock]")).collect();
    let mut soll = vec![
        format!("[lock] elem={}", guid(&s, "AW-005")),
        format!("[lock] elem={}", guid(&s, "IW-001")),
    ];
    soll.sort();
    let mut ist: Vec<String> = zeilen.iter().map(|x| x.to_string()).collect();
    ist.sort();
    assert_eq!(ist, soll);
    let l = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert_eq!(sk_model::szo::write(&l.model), text, "Rundlauf");
    let w = Scene::with_model(l.model);
    assert!(gesperrt(w.model(), nr(&w, "AW-005")));
    assert!(gesperrt(w.model(), nr(&w, "IW-001")));
    assert!(!gesperrt(w.model(), nr(&w, "AW-006")));

    sperren(&mut s, &[a5, iw], false);
    assert_eq!(
        sk_model::szo::write(s.model()),
        ohne,
        "ohne Sperre bytegleich"
    );

    let fremd = format!("{ohne}[lock] elem=00000000-0000-0000-0000-00000000abcd\n");
    let l = sk_model::szo::read(&fremd, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    assert_eq!(l.hints.len(), 1, "{:?}", l.hints);
    assert_eq!(sk_model::szo::write(&l.model), ohne, "verworfen");
}

/// A225 (§2.3, §5, plan B7): Rücknahme von 4a. Eine Datei mit
/// `[lock]`-Zeilen öffnet auf dem Stand ohne 4a, Speichern behält die
/// Zeilen bytegleich. Läuft ohne Adapter; grün ab F-17b (A204, A211). Reihenfolge der
/// Zeilen frei (Koordinator 13:59).
#[test]
fn a225_lock_ruecknahme() {
    let mut s = Scene::with_model(Model::with_seed(225));
    gebaeude(&mut s);
    let text = sk_model::szo::write(s.model());
    let g: Vec<Guid> = s
        .model()
        .elements()
        .iter()
        .take(2)
        .map(|(_, e)| e.guid)
        .collect();
    let lock = format!("[lock] elem={}\n[lock] elem={}\n", g[0], g[1]);
    let l = sk_model::szo::read(&format!("{text}{lock}"), sk_model::GuidGen::with_seed(1))
        .expect("öffnet");
    assert!(l.model.check().is_empty());
    let neu = sk_model::szo::write(&l.model);
    // Ohne 4a bleiben die Zeilen in Dateireihenfolge; mit 4a schreibt
    // der Bau sie in seiner Reihenfolge. Verbindlich: dieselben Zeilen,
    // und ein zweiter Rundlauf ändert nichts.
    let mut ist: Vec<&str> = neu.lines().filter(|x| x.starts_with("[lock]")).collect();
    ist.sort();
    let mut soll: Vec<&str> = lock.lines().collect();
    soll.sort();
    assert_eq!(ist, soll, "Zeilen bleiben");
    let zwei = sk_model::szo::read(&neu, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    assert_eq!(sk_model::szo::write(&zwei.model), neu, "zweiter Rundlauf");
}

/// A226 (§3 4b, §4.7; p34 §5): Trefferflächen. Zeilenhöhe
/// `size.tree_row_h` 24 dip, Symbole 16 dip mit 4 dip Abstand, bei 100 %
/// und 150 %. Mitte einer Zeile trifft sie, der Rollstand verschiebt,
/// unter der letzten Zeile nichts. Im Symbolstreifen trifft die Mitte
/// jedes Feldes ein Symbol, die Lücke nichts. Ohne Überfahren nur Auge
/// und Schloss; ein blasses Schloss oder Löschen löst nichts aus.
#[test]
fn a226_trefferflaechen() {
    for scale in [1.0, 1.5] {
        let h = 24.0 * scale;
        assert_eq!(zeile_bei(0.5 * h, 0.0, 10, scale), Some(0));
        assert_eq!(zeile_bei(1.5 * h, 0.0, 10, scale), Some(1));
        assert_eq!(zeile_bei(9.5 * h, 0.0, 10, scale), Some(9));
        assert_eq!(
            zeile_bei(10.5 * h, 0.0, 10, scale),
            None,
            "unter der letzten"
        );
        assert_eq!(zeile_bei(0.5 * h, 2.0 * h, 10, scale), Some(2), "Rollstand");
        assert_eq!(zeile_bei(-1.0, 0.0, 10, scale), None);

        let feld = 16.0 * scale;
        let schritt = 20.0 * scale;
        let mitte = |k: f64| k * schritt + feld / 2.0;
        let luecke = |k: f64| k * schritt + feld + 2.0 * scale;
        let mut alle: Vec<Symbol> = (0..4)
            .map(|k| symbol_bei(mitte(k as f64), scale, true, false, false).expect("Feld"))
            .collect();
        alle.sort();
        assert_eq!(
            alle,
            [
                Symbol::Auge,
                Symbol::Schloss,
                Symbol::Isolieren,
                Symbol::Loeschen
            ],
            "{scale}"
        );
        for k in 0..3 {
            assert_eq!(
                symbol_bei(luecke(k as f64), scale, true, false, false),
                None
            );
        }
        let ruhig: Vec<Symbol> = (0..4)
            .filter_map(|k| symbol_bei(mitte(k as f64), scale, false, false, false))
            .collect();
        assert_eq!(
            sortiert(&ruhig),
            [Symbol::Auge, Symbol::Schloss],
            "ohne Überfahren"
        );
        let blass: Vec<Symbol> = (0..4)
            .filter_map(|k| symbol_bei(mitte(k as f64), scale, true, true, true))
            .collect();
        assert_eq!(
            sortiert(&blass),
            [Symbol::Auge, Symbol::Isolieren],
            "blass löst nichts aus"
        );
    }
}

/// A227 (BIM Regel 67, bim/gewerke-kurznamen.md): Kurznamen der Gewerke.
/// Startbestand nach Tabelle (alle 35 eindeutig ohne Groß-/Kleinschrift,
/// höchstens 14 Zeichen, keine Ziffer). Ohne eigene Kurznamen kein
/// `short=` in der Datei. Eigener Kurzname steht als `short="…"` in der
/// `[trade]`-Zeile, Rundlauf bytegleich, der Baum zeigt ihn; Mengen
/// bleiben. Eingabe doppelt, mit Ziffer oder zu lang: abgelehnt.
/// `short=""` entfernt ihn (Baum zeigt den Langnamen). Zurück auf den
/// Startwert: Datei bytegleich wie vorher. Doppelt beim Laden: der
/// zweite fällt mit Hinweis weg.
#[test]
fn a227_kurzname_datei() {
    let (mut s, _, _) = pruefhaus(227);
    for (code, k) in [
        ("18331", "Beton"),
        ("18330", "Maurer"),
        ("18338", "Dachdecker"),
        ("18345", "WDVS"),
        ("18333", "Betonwerkstein"),
        ("18386", "MSR-Technik"),
    ] {
        let t = s.model().trade(gewerk(&s, code)).unwrap();
        assert_eq!(kurz(t).as_deref(), Some(k), "{code}");
    }
    let mut alle: Vec<String> = s
        .model()
        .trades()
        .iter()
        .filter_map(kurz)
        .map(|k| k.to_lowercase())
        .collect();
    assert_eq!(alle.len(), 35, "jedes Startgewerk hat einen Kurznamen");
    for k in &alle {
        assert!(
            k.chars().count() <= 14 && !k.chars().any(|c| c.is_ascii_digit()),
            "{k}"
        );
    }
    alle.sort();
    alle.dedup();
    assert_eq!(alle.len(), 35, "eindeutig");

    // Nur `[trade]`-Zeilen: `[storey]` trägt seit B11 ein eigenes
    // `short=` (z. B. „EG“).
    let gewerkzeilen = |t: &str| -> Vec<String> {
        t.lines()
            .filter(|l| l.starts_with("[trade]") && l.contains("short="))
            .map(str::to_string)
            .collect()
    };
    let ohne = sk_model::szo::write(s.model());
    assert!(
        gewerkzeilen(&ohne).is_empty(),
        "Startbestand wird nicht geschrieben"
    );
    let mengen = csv(&mut s);
    let dd = gewerk(&s, "18338");
    assert!(s.edit_model("Gewerk geändert", |m| kurz_setzen(m, dd, "Dach")));
    let text = sk_model::szo::write(s.model());
    let mit = gewerkzeilen(&text);
    assert_eq!(mit.len(), 1, "{mit:?}");
    assert!(mit[0].starts_with("[trade]") && mit[0].contains("code=\"18338\""));
    assert!(mit[0].contains("short=\"Dach\""), "{}", mit[0]);
    let l = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    assert!(l.hints.is_empty(), "{:?}", l.hints);
    assert_eq!(sk_model::szo::write(&l.model), text, "Rundlauf");
    assert_eq!(kurzname(s.model(), dd), "Dach");
    let t = baum(&s);
    let n = karte(&t, Tab::Trade);
    assert_eq!(
        n[finde(n, &NodeKey::Trade(dd))].label,
        "Dach",
        "Baum zeigt ihn"
    );
    assert_eq!(csv(&mut s), mengen, "Mengen und CSV bleiben beim Langnamen");

    let wd = gewerk(&s, "18345");
    assert!(
        !s.edit_model("Gewerk geändert", |m| kurz_setzen(m, wd, "dach")),
        "doppelt"
    );
    assert!(
        !s.edit_model("Gewerk geändert", |m| kurz_setzen(m, wd, "WDVS2")),
        "Ziffer"
    );
    assert!(
        !s.edit_model("Gewerk geändert", |m| kurz_setzen(
            m,
            wd,
            "Wärmedämmsysteme"
        )),
        "länger als 14"
    );
    assert_eq!(kurzname(s.model(), wd), "WDVS");

    assert!(s.edit_model("Gewerk geändert", |m| kurz_setzen(m, dd, "")));
    let text = sk_model::szo::write(s.model());
    assert!(text
        .lines()
        .any(|l| l.contains("code=\"18338\"") && l.contains("short=\"\"")));
    let l = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    let w = Scene::with_model(l.model);
    let lang = w.model().trade(dd).unwrap().name.clone();
    assert_eq!(kurzname(w.model(), dd), lang, "entfernt: Langname");
    let t = baum(&w);
    let n = karte(&t, Tab::Trade);
    assert_eq!(n[finde(n, &NodeKey::Trade(dd))].label, lang);

    assert!(s.edit_model("Gewerk geändert", |m| kurz_setzen(m, dd, "Dachdecker")));
    assert_eq!(
        sk_model::szo::write(s.model()),
        ohne,
        "Startwert: bytegleich"
    );

    let b = gewerk(&s, "18331");
    let m = gewerk(&s, "18330");
    let zeile = |code: &str| {
        ohne.lines()
            .find(|l| l.starts_with("[trade]") && l.contains(&format!("code=\"{code}\"")))
            .unwrap()
            .to_string()
    };
    let (zb, zm) = (zeile("18331"), zeile("18330"));
    let doppelt = ohne
        .replace(&zb, &format!("{zb} short=\"Neu\""))
        .replace(&zm, &format!("{zm} short=\"neu\""));
    let l = sk_model::szo::read(&doppelt, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    assert!(!l.hints.is_empty(), "doppelt beim Laden: Hinweis");
    assert_eq!(
        kurz(l.model.trade(b).unwrap()).as_deref(),
        Some("Neu"),
        "der erste bleibt"
    );
    assert_ne!(
        kurz(l.model.trade(m).unwrap()).as_deref(),
        Some("neu"),
        "der zweite fällt weg"
    );
}

use super::*;

// Abnahmetests A288–A293: Hilfe im Programm (Paket 9, 9a Karte und F1,
// 9b Tooltips). Grundlage: projektstruktur/paket-9-hilfe.md mit Nachtrag
// 19:15 (Review 3p H9-1 bis H9-6), Text planung/hilfe-entwurf.txt.
// Spezifikation: test/abnahme-hilfe.md. Vorbereitet gegen main 51161be.
//
// Einbau: als `mod hilfe { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: kuerzel, zeilen, befehl, tool,
// click, px, key, mv, cam_plan, gebaeude, W, H.
// Dazu in abnahme.rs: `befehl` bekommt `Command::Help => "Hilfe".into()`
// und `Command::MeasureFrameTime`, A60 die Zeilen ("Hilfe", "F1", true)
// und ("Bildzeit messen (10 s)", "", true) direkt vor ("Schließen", …),
// siehe A294 (vorbereitet/a294-bildzeit.rs).
//
// Angenommene Namen stehen NUR in den Adaptern (aus §3 des Plans):
// `crate::help::{TEXT, parse, Topic, HelpCtx, topic, HelpCard,
// tooltip_lines}`, `Command::Help`.
//
// Verdrahtung in main.rs (Esc „innen zuerst“ H9-1, F1 vor der
// Fensterkette und nicht bei „Änderungen speichern?“ H9-2, Klicks in die
// Karte über Fenstern H9-6, Weiterarbeiten bei offener Karte, F1 im
// Mengenfenster) prüfen die Handtests H184–H187; die Leistung (H9-3)
// misst Review.

use crate::menu::{Command, FileMenu, Recent, Shortcuts};

// ===== Adapter Paket 9 =====

/// Der Hilfetext, wie er im Programm steckt (`include_str!`).
fn hilfetext() -> &'static str {
    crate::help::TEXT
}

/// Ein Thema: Kennung, Titel, Zeilen (Aktion, Bedienung).
#[derive(Clone, Debug, PartialEq)]
struct Thema {
    id: String,
    titel: String,
    zeilen: Vec<(String, String)>,
}

/// Gelesener Text: Themen, `[knopf]`-Zeilen (Name, Satz), Hinweise.
struct Gelesen {
    themen: Vec<Thema>,
    knoepfe: Vec<(String, String)>,
    hinweise: Vec<String>,
}

fn lesen(text: &str) -> Gelesen {
    let h = crate::help::parse(text);
    Gelesen {
        themen: h
            .sections
            .iter()
            .map(|s| Thema {
                id: s.id.clone(),
                titel: s.title.clone(),
                zeilen: s.rows.clone(),
            })
            .collect(),
        knoepfe: h.buttons.clone(),
        hinweise: h.errors.clone(),
    }
}

/// Kennungen aller `Topic`-Varianten.
fn alle_kennungen() -> Vec<&'static str> {
    crate::help::Topic::ALL.iter().map(|t| t.id()).collect()
}

/// Lage der App, aus der die Themenwahl (§1.2) das Thema bestimmt.
#[derive(Default)]
struct Lage {
    /// "einstellungen:<reiter>", "katalog", "baustoffe", "muster",
    /// "sicherungen"; Reiter: stifte, linientypen, schraffuren,
    /// oberflaechen, bedienoberflaeche.
    fenster: Option<&'static str>,
    dialog: bool,
    zielwahl: bool,
    ziehen: bool,
    /// "gebaeude" oder "innenwand"
    werkzeug: Option<&'static str>,
    isolieren: bool,
    /// Leiste „Gelände“ offen oder Geländefläche gewählt.
    gelaende: bool,
    /// "wand", "og-wand", "gruendung", "decke", "dachterrasse"
    auswahl: Option<&'static str>,
    ansicht: Option<ViewKind>,
}

fn thema(l: &Lage) -> &'static str {
    use crate::help::{HelpCtx, SelKind, Window};
    let fenster = l.fenster.map(|f| match f.split_once(':') {
        Some(("einstellungen", reiter)) => Window::Settings(match reiter {
            "stifte" => 0,
            "linientypen" => 1,
            "schraffuren" => 2,
            "oberflaechen" => 3,
            _ => 4,
        }),
        _ => match f {
            "katalog" => Window::Catalog,
            "baustoffe" => Window::Materials,
            "muster" => Window::Patterns,
            _ => Window::Backups,
        },
    });
    let ctx = HelpCtx {
        window: fenster,
        dialog: l.dialog,
        flush_pick: l.zielwahl,
        dragging: l.ziehen,
        tool: l.werkzeug.map(|w| w == "innenwand"),
        opening: matches!(l.werkzeug, Some("fenster" | "tuer")),
        isolating: l.isolieren,
        gelaende: l.gelaende,
        selection: l.auswahl.map(|a| match a {
            "wand" => SelKind::Wall,
            "og-wand" => SelKind::UpperWall,
            "gruendung" => SelKind::Foundation,
            "decke" => SelKind::Floor,
            _ => SelKind::Terrace,
        }),
        view: l.ansicht.unwrap_or(ViewKind::Persp),
    };
    crate::help::topic(&ctx).id()
}

type Karte = crate::help::HelpCard;

fn karte() -> Karte {
    crate::help::HelpCard::default()
}

fn f1() -> Key {
    Key::Other(0x70)
}

/// Taste an die Karte; `true`, wenn sie die Taste genommen hat.
fn karte_taste(k: &mut Karte, taste: Key) -> bool {
    k.key(taste)
}

fn offen(k: &Karte) -> bool {
    k.is_open()
}

/// Das Thema des Tuns ändert sich zur Zeit `t` (ms).
fn folge(k: &mut Karte, thema: &str, t: u64) {
    let topic = *crate::help::Topic::ALL
        .iter()
        .find(|x| x.id() == thema)
        .unwrap();
    k.follow(topic, t);
}

/// Thema, das die Karte zur Zeit `t` zeigt.
fn gezeigt(k: &mut Karte, t: u64) -> &'static str {
    k.tick(t);
    k.topic().id()
}

/// „Alle Themen ▸“, ein Thema der Liste, „‹ Zurück“.
fn alle_themen(k: &mut Karte) {
    k.toggle_list();
}
fn liste_offen(k: &Karte) -> bool {
    k.list_open()
}
fn waehle(k: &mut Karte, thema: &str) {
    let topic = *crate::help::Topic::ALL
        .iter()
        .find(|x| x.id() == thema)
        .unwrap();
    k.pick(topic);
}
fn zurueck(k: &mut Karte) {
    k.back();
}

/// Zeilen des Tooltips am Knopf `name` (Name, Satz, „F1: mehr“).
fn tooltip(name: &str) -> Vec<String> {
    crate::help::tooltip_lines(name)
}

// ===== Hilfen =====

const OHNE: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
};

/// Knöpfe mit Tooltip (§1.3); „Winkelsprung“ (vormals „90°-Sprung“, G2)
/// steht im Paneel Werkzeuge (Koordinator 21:45), „Erweiterungen“ dort ab
/// E6 (Schrittplan .szb).
const KNOEPFE: [&str; 34] = [
    "Gebäude",
    "Innenwand",
    "Wandöffnung",
    "Türöffnung",
    "Winkelsprung",
    "Erweiterungen",
    // Leiste „Gelände“ (plan-gelaendemodul.md §4.7)
    "Gelände",
    "Lageplan",
    "Flurstück übernehmen",
    "Grenze zeichnen",
    "Höhen",
    "Gelände verschieben",
    "Gelände drehen",
    "An Grenze ausrichten",
    "Höhenlage über Gelände",
    "Höhenlage absolut",
    "Umgebung ganz",
    "Umgebung blass",
    "Umgebung nicht",
    "Fläche zeichnen",
    "Fläche befestigt",
    "Fläche löschen",
    "Alle Hinweise",
    "Gelände entfernen",
    "3D",
    "Grundriss",
    "Schnitt",
    "Vorne",
    "Hinten",
    "Links",
    "Rechts",
    "Mengen · Kosten · AVA",
    // derselbe Knopf in den Stufen Pro und Basis (freischaltung.rs)
    "Mengen · Kosten",
    "Mengen",
    "?",
];

/// Alle fett gesetzten Wörter des Texts (`**…**`).
fn fett(text: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find("**") {
        let r = &rest[a + 2..];
        let Some(b) = r.find("**") else { break };
        v.push(r[..b].to_string());
        rest = &r[b + 2..];
    }
    v.sort();
    v.dedup();
    v
}

// ===== Tests =====

/// A288 (§2, Abnahme 1, H9-4): `hilfe.txt` lässt sich lesen. Jede
/// Kennung genau einmal, jedes Thema mit Titel und 1–10 Zeilen, getrennt
/// am ersten „|“. Dasselbe mit CRLF und mit BOM. Ein kaputter Text ergibt
/// Hinweise, keinen Absturz. `.gitattributes` hält den Text bei LF.
#[test]
fn a288_hilfetext_lesen() {
    let text = hilfetext();
    let g = lesen(text);
    assert!(g.hinweise.is_empty(), "{:?}", g.hinweise);
    assert!(g.themen.len() >= 20, "{} Themen", g.themen.len());
    let mut ids: Vec<&str> = g.themen.iter().map(|t| t.id.as_str()).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "jede Kennung einmal");
    for t in &g.themen {
        assert!(!t.titel.is_empty(), "{}: Titel", t.id);
        assert!(
            (1..=10).contains(&t.zeilen.len()),
            "{}: {} Zeilen",
            t.id,
            t.zeilen.len()
        );
        for (a, b) in &t.zeilen {
            assert!(
                !a.trim().is_empty() && !b.trim().is_empty(),
                "{}: {a} | {b}",
                t.id
            );
            assert!(!a.contains('|'), "Trennung am ersten „|“: {a}");
        }
    }
    let z = g
        .themen
        .iter()
        .find(|t| t.id == "zeichnen")
        .expect("zeichnen");
    assert_eq!(z.titel, "Wände zeichnen");

    // Trennung am ersten „|“
    let t2 = lesen("[thema] id=start titel=\"Test\"\nA | b | c\n");
    assert_eq!(
        t2.themen[0].zeilen,
        [("A".to_string(), "b | c".to_string())]
    );

    // CRLF und BOM
    let crlf = text.replace('\n', "\r\n");
    let bom = format!("\u{feff}{text}");
    for (art, t) in [("CRLF", crlf.as_str()), ("BOM", bom.as_str())] {
        let h = lesen(t);
        assert!(h.hinweise.is_empty(), "{art}: {:?}", h.hinweise);
        assert_eq!(h.themen, g.themen, "{art}: gleiche Themen");
        assert_eq!(h.knoepfe, g.knoepfe, "{art}: gleiche Knöpfe");
    }

    // Kaputt: Hinweise statt Absturz
    for kaputt in [
        "[thema] id=x titel=\"offen\nA | b\n",
        "ohne Kopf | Zeile\n",
        "[thema] id=y titel=\"Y\"\nkeine Trennung\n",
        "\u{feff}\r\n\r\n",
    ] {
        let h = lesen(kaputt);
        assert!(
            !h.hinweise.is_empty() || h.themen.iter().all(|t| t.zeilen.is_empty()),
            "„{kaputt}“"
        );
    }

    let ga = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../.gitattributes"))
        .expect(".gitattributes");
    assert!(
        ga.lines().any(
            |l| l.split_whitespace().collect::<Vec<_>>() == ["app/hilfe.txt", "text", "eol=lf"]
        ),
        "{ga}"
    );
}

/// A289 (§1.2, §1.3, §2, Abnahme 2, H9-5, Paket 8 §7): Jede
/// `Topic`-Variante hat ein Thema im Text und umgekehrt. Jeder Knopf mit
/// Tooltip hat eine `[knopf]`-Zeile und jede Zeile gehört zu einem Knopf.
/// Die Maßeingabe steht in „Wände zeichnen“ und „Wand verschieben“.
#[test]
fn a289_themen_und_knoepfe() {
    let g = lesen(hilfetext());
    let mut text_ids: Vec<String> = g.themen.iter().map(|t| t.id.clone()).collect();
    text_ids.sort();
    let mut enum_ids: Vec<String> = alle_kennungen().iter().map(|s| s.to_string()).collect();
    enum_ids.sort();
    assert_eq!(text_ids, enum_ids, "Themen im Text = Topic-Varianten");
    for pflicht in [
        "start",
        "navigation",
        "gebaeude",
        "zeichnen",
        "innenwand",
        "ziehen",
        "og",
        "buendig",
        "wand",
        "gruendung",
        "decke",
        "dachterrasse",
        "geschosse",
        "schnitt",
        "baum",
        "datei",
        "einstellungen",
        "einstellungen-stifte",
        "einstellungen-linientypen",
        "einstellungen-schraffuren",
        "einstellungen-oberflaechen",
        "einstellungen-bedienoberflaeche",
        "katalog",
        "baustoffe",
        "muster",
        "mengen",
        "loeschen",
        "sicherungen",
    ] {
        assert!(
            enum_ids.iter().any(|i| i == pflicht),
            "Thema {pflicht} fehlt"
        );
    }

    let mut namen: Vec<&str> = g.knoepfe.iter().map(|(n, _)| n.as_str()).collect();
    namen.sort();
    let mut soll = KNOEPFE.to_vec();
    soll.sort();
    assert_eq!(namen, soll, "[knopf]-Zeilen = Knöpfe mit Tooltip");
    for (n, satz) in &g.knoepfe {
        assert!(satz.ends_with('.'), "{n}: ein Satz");
    }

    let zeilen = |id: &str| -> String {
        g.themen
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .zeilen
            .iter()
            .map(|(a, b)| format!("{a} {b}\n"))
            .collect()
    };
    let z = zeilen("zeichnen");
    for w in [
        "**Tab**",
        "**Enter**",
        "**Esc**",
        "**Rücktaste**",
        "Länge tippen",
        "Winkel tippen",
    ] {
        assert!(z.contains(w), "zeichnen: {w}");
    }
    let z = zeilen("ziehen");
    for w in ["Wert tippen", "Nachkorrektur", "**Strg**", "**Esc**"] {
        assert!(z.contains(w), "ziehen: {w}");
    }
}

/// A290 (§1.2, Abnahme 3): Themenwahl, die erste zutreffende Zeile gilt,
/// auch die Vorrangfälle.
#[test]
fn a290_themenwahl() {
    use ViewKind::*;
    let f = |l: Lage| thema(&l);
    for (fenster, id) in [
        ("einstellungen:stifte", "einstellungen-stifte"),
        ("einstellungen:linientypen", "einstellungen-linientypen"),
        ("einstellungen:schraffuren", "einstellungen-schraffuren"),
        ("einstellungen:oberflaechen", "einstellungen-oberflaechen"),
        (
            "einstellungen:bedienoberflaeche",
            "einstellungen-bedienoberflaeche",
        ),
    ] {
        assert_eq!(
            f(Lage {
                fenster: Some(fenster),
                auswahl: Some("wand"),
                werkzeug: Some("gebaeude"),
                ..Lage::default()
            }),
            id,
            "Fenster vor Auswahl und Werkzeug"
        );
    }
    for (fenster, id) in [
        ("katalog", "katalog"),
        ("baustoffe", "baustoffe"),
        ("muster", "muster"),
        ("sicherungen", "sicherungen"),
    ] {
        assert_eq!(
            f(Lage {
                fenster: Some(fenster),
                ziehen: true,
                ..Lage::default()
            }),
            id
        );
    }
    assert_eq!(
        f(Lage {
            dialog: true,
            werkzeug: Some("gebaeude"),
            ..Lage::default()
        }),
        "gebaeude"
    );
    assert_eq!(
        f(Lage {
            zielwahl: true,
            auswahl: Some("og-wand"),
            ..Lage::default()
        }),
        "buendig"
    );
    assert_eq!(
        f(Lage {
            ziehen: true,
            werkzeug: Some("gebaeude"),
            ..Lage::default()
        }),
        "ziehen",
        "Ziehen bei aktivem Werkzeug"
    );
    assert_eq!(
        f(Lage {
            werkzeug: Some("gebaeude"),
            isolieren: true,
            ..Lage::default()
        }),
        "zeichnen"
    );
    assert_eq!(
        f(Lage {
            werkzeug: Some("innenwand"),
            ..Lage::default()
        }),
        "innenwand"
    );
    assert_eq!(
        f(Lage {
            isolieren: true,
            auswahl: Some("decke"),
            ..Lage::default()
        }),
        "baum"
    );
    // Gelände (Bedienbarkeit 43.2): vor Isolieren und Auswahl, nach
    // Werkzeug und Fenster
    for (l, id) in [
        (
            Lage {
                gelaende: true,
                isolieren: true,
                auswahl: Some("wand"),
                ..Lage::default()
            },
            "gelaende",
        ),
        (
            Lage {
                gelaende: true,
                werkzeug: Some("gebaeude"),
                ..Lage::default()
            },
            "zeichnen",
        ),
        (
            Lage {
                gelaende: true,
                fenster: Some("katalog"),
                ..Lage::default()
            },
            "katalog",
        ),
    ] {
        assert_eq!(f(l), id);
    }
    for (a, id) in [
        ("wand", "wand"),
        ("og-wand", "og"),
        ("gruendung", "gruendung"),
        ("decke", "decke"),
        ("dachterrasse", "dachterrasse"),
    ] {
        assert_eq!(
            f(Lage {
                auswahl: Some(a),
                ansicht: Some(Section),
                ..Lage::default()
            }),
            id,
            "Auswahl vor Ansicht"
        );
    }
    assert_eq!(
        f(Lage {
            ansicht: Some(Section),
            ..Lage::default()
        }),
        "schnitt"
    );
    assert_eq!(
        f(Lage {
            ansicht: Some(Plan),
            ..Lage::default()
        }),
        "geschosse"
    );
    for v in [Persp, Front, Back, Left, Right] {
        assert_eq!(
            f(Lage {
                ansicht: Some(v),
                ..Lage::default()
            }),
            "start",
            "{v:?}"
        );
    }
}

/// A291 (§1.1, Abnahme 4, H9-3): F1 öffnet und schließt die Karte, Esc
/// schließt sie; ohne offene Karte nimmt sie Esc nicht. „Alle Themen ▸“
/// klappt die Liste auf, ein Thema zeigt es, „‹ Zurück“ kehrt zum Thema
/// des Tuns zurück. Der Themenwechsel wartet 150 ms: Ein kurzes Ziehen
/// blendet nicht hin und her.
#[test]
fn a291_karte() {
    let mut k = karte();
    assert!(!offen(&k));
    assert!(
        !karte_taste(&mut k, Key::Escape),
        "geschlossen: Esc geht weiter"
    );
    assert!(!karte_taste(&mut k, Key::Char('4')));
    assert!(karte_taste(&mut k, f1()));
    assert!(offen(&k));
    assert!(
        !karte_taste(&mut k, Key::Char('4')),
        "Ziffern gehen ans Werkzeug"
    );
    assert!(!karte_taste(&mut k, Key::Enter));
    assert!(karte_taste(&mut k, f1()));
    assert!(!offen(&k), "F1 schließt");
    karte_taste(&mut k, f1());
    assert!(karte_taste(&mut k, Key::Escape));
    assert!(!offen(&k), "Esc schließt");

    // Themenliste
    let mut k = karte();
    folge(&mut k, "zeichnen", 0);
    karte_taste(&mut k, f1());
    assert_eq!(
        gezeigt(&mut k, 0),
        "zeichnen",
        "öffnet mit dem Thema des Tuns"
    );
    alle_themen(&mut k);
    assert!(liste_offen(&k));
    waehle(&mut k, "katalog");
    assert!(!liste_offen(&k));
    assert_eq!(gezeigt(&mut k, 10), "katalog");
    zurueck(&mut k);
    assert_eq!(gezeigt(&mut k, 20), "zeichnen");

    // Entprellt: 150 ms
    folge(&mut k, "ziehen", 1000);
    assert_eq!(gezeigt(&mut k, 1100), "zeichnen", "noch nicht gewechselt");
    folge(&mut k, "zeichnen", 1120);
    assert_eq!(
        gezeigt(&mut k, 1400),
        "zeichnen",
        "kurzes Ziehen: kein Wechsel"
    );
    folge(&mut k, "ziehen", 2000);
    assert_eq!(gezeigt(&mut k, 2140), "zeichnen");
    assert_eq!(gezeigt(&mut k, 2160), "ziehen", "nach 150 ms gewechselt");
}

/// A292 (§1.3, §1.4, Abnahme 6): Dateimenü mit „Hilfe“ und Kürzel „F1“
/// vor „Schließen“, der Eintrag schickt `Command::Help`. Tooltips der
/// Knöpfe: Name, Satz aus `[knopf]`, „F1: mehr“. Entdecken-Hinweise aus
/// Paket 8 und 9.
#[test]
fn a292_menue_tooltips_hinweise() {
    let r = Recent::default();
    let mut m = FileMenu::default();
    m.open();
    let z = zeilen(&m, true, &r);
    let hilfe = z
        .iter()
        .position(|(t, k, an)| t == "Hilfe" && k == "F1" && *an)
        .expect("Eintrag „Hilfe“ mit F1");
    let schliessen = z.iter().position(|(t, _, _)| t == "Schließen").unwrap();
    assert!(hilfe < schliessen, "vor „Schließen“: {z:?}");
    let items = m.items(true, &r);
    assert_eq!(
        befehl(items[hilfe].command),
        befehl(Some(Command::Help)),
        "Command::Help"
    );

    let g = lesen(hilfetext());
    for name in KNOEPFE {
        let satz = &g.knoepfe.iter().find(|(n, _)| n == name).unwrap().1;
        assert_eq!(
            tooltip(name),
            [name.to_string(), satz.clone(), "F1: mehr".to_string()],
            "Tooltip {name}"
        );
    }

    let flach = |t: &str| t.replace('\n', " ");
    let alle: Vec<String> = crate::hints::HINTS.iter().map(|(_, t)| flach(t)).collect();
    for soll in [
        "Tipp: Eine Zahl tippen und Enter setzt die Wand genau auf diese Länge.",
        "F1 zeigt die Hilfe zu dem, was du gerade tust.",
    ] {
        assert!(alle.iter().any(|t| t == soll), "Hinweis fehlt: {soll}");
    }
}

/// A293 (Abnahme 8, Paket 8 §7): Jede Taste, die der Hilfetext fett
/// nennt, ist belegt. Neue fett gesetzte Wörter müssen hier eingetragen
/// werden, so bleibt die Hilfe nicht hinter dem Programm zurück.
#[test]
fn a293_genannte_tasten_sind_belegt() {
    // Kürzel des Hauptfensters (menu.rs)
    let strg = Modifiers { ctrl: true, ..OHNE };
    let strg_um = Modifiers {
        ctrl: true,
        shift: true,
        alt: false,
    };
    let kuerzel_von = |t: &str| -> Option<(Key, Modifiers)> {
        Some(match t {
            "Strg+Z" => (Key::Char('Z'), strg),
            "Strg+Y" => (Key::Char('Y'), strg),
            "Strg+Umschalt+Z" => (Key::Char('Z'), strg_um),
            "Strg+N" => (Key::Char('N'), strg),
            "Strg+O" => (Key::Char('O'), strg),
            "Strg+S" => (Key::Char('S'), strg),
            "Strg+Umschalt+S" => (Key::Char('S'), strg_um),
            "Strg+W" => (Key::Char('W'), strg),
            "Strg+Komma" => (Key::Char(','), strg),
            "F10" => (Key::Other(0x79), OHNE),
            "Entf" => (Key::Delete, OHNE),
            _ => return None,
        })
    };
    // Tasten des Werkzeugs und anderer Stellen (geprüft unten)
    let werkzeug = ["Enter", "Tab", "Esc", "Rücktaste", "R", "Umschalt"];
    let anderswo = [
        ("Alt", "Menü beim Loslassen (unten)"),
        ("Strg", "nur diese Wand ziehen (unten)"),
        ("F1", "Hilfekarte (A291)"),
        ("Bild↑", "Geschoss bzw. Schnitt (wheel::KEY_PAGE_UP)"),
        ("Bild↓", "Geschoss bzw. Schnitt (wheel::KEY_PAGE_DOWN)"),
        ("←", "Kachelwahl der Sicherungskarte (backup_card)"),
        ("→", "Kachelwahl der Sicherungskarte (backup_card)"),
        ("Leertaste", "Bandseite der Tür bei der Maus (tuer_edit)"),
        ("Umschalt+Leertaste", "anderer Raum (tuer_edit)"),
        (
            "Strg+A",
            "alles wählen im Namensfeld des Raumstempels (stempel)",
        ),
        ("OK", "Knopf, keine Taste"),
    ];
    for t in fett(hilfetext()) {
        let bekannt = kuerzel_von(&t).is_some()
            || werkzeug.contains(&t.as_str())
            || anderswo.iter().any(|(a, _)| *a == t);
        assert!(bekannt, "„**{t}**“ ist hier nicht eingetragen");
        if let Some((k, mods)) = kuerzel_von(&t) {
            let mut sc = Shortcuts::default();
            assert!(
                kuerzel(&mut sc, k, true, mods, true).is_some(),
                "{t} unbelegt"
            );
        }
    }
    let mut sc = Shortcuts::default();
    kuerzel(&mut sc, Key::Alt, true, OHNE, true);
    assert_eq!(
        kuerzel(&mut sc, Key::Alt, false, OHNE, true).as_deref(),
        Some("Menü"),
        "Alt allein"
    );
    assert_eq!(crate::wheel::KEY_PAGE_UP, Key::Other(0x21));
    assert_eq!(crate::wheel::KEY_PAGE_DOWN, Key::Other(0x22));
    assert_eq!(crate::wall_edit::tuer_edit::LEERTASTE, Key::Other(0x20));

    // Werkzeug: Tab, R, Umschalt, Rücktaste, Enter, Esc
    let s = Scene::with_model(Model::with_seed(293));
    let c = cam_plan(&s);
    let mut t = tool(&s);
    let p = |x: f64, y: f64| vec3(x, y, 0.0);
    click(&mut t, &c, p(0.0, 0.0));
    click(&mut t, &c, p(4000.0, 0.0));
    click(&mut t, &c, p(4000.0, 3000.0));
    let seite = t.ref_side;
    t.handle(&key(Key::Tab), &c, W, H, 1.0);
    assert_ne!(t.ref_side, seite, "Tab");
    let ortho = t.ortho;
    t.handle(&key(Key::Char('R')), &c, W, H, 1.0);
    assert_ne!(t.ortho, ortho, "R");
    t.handle(&key(Key::Char('R')), &c, W, H, 1.0);
    // Umschalt kehrt den Winkelsprung kurz um: Cursor schräg statt rechtwinklig
    let schraeg = px(&c, p(5500.0, 4200.0));
    t.handle(&mv(schraeg.0, schraeg.1), &c, W, H, 1.0);
    let gerade = *t.preview().unwrap().points.last().unwrap();
    let um = Modifiers {
        shift: true,
        ..OHNE
    };
    t.handle(
        &Event::MouseMove {
            x: schraeg.0,
            y: schraeg.1,
            mods: um,
        },
        &c,
        W,
        H,
        1.0,
    );
    let frei = *t.preview().unwrap().points.last().unwrap();
    assert!(
        (gerade - frei).length() > 100.0,
        "Umschalt: {gerade:?} {frei:?}"
    );
    t.handle(&mv(schraeg.0, schraeg.1), &c, W, H, 1.0);
    t.handle(&key(Key::Backspace), &c, W, H, 1.0);
    let w = t.handle(&key(Key::Enter), &c, W, H, 1.0).commit;
    assert_eq!(
        w.map(|w| w.clean_points().len()),
        Some(2),
        "Rücktaste, Enter"
    );
    click(&mut t, &c, p(0.0, 5000.0));
    t.handle(&key(Key::Escape), &c, W, H, 1.0);
    assert!(!t.is_active(), "Esc");

    // Strg beim Ziehen: nur diese Wand
    let mut s = Scene::with_model(Model::with_seed(2930));
    gebaeude(&mut s);
    let nr = |n: &str| {
        s.model()
            .elements()
            .iter()
            .find(|(_, e)| e.number == n)
            .map(|(id, _)| id)
            .unwrap()
    };
    let (aw6, aw2) = (nr("AW-006"), nr("AW-002"));
    let mit = crate::wall_edit::drag_set_mods(s.model(), aw6, true);
    assert!(
        mit.contains(&aw6) && !mit.contains(&aw2),
        "mit Strg: nur das OG"
    );
    let ohne = crate::wall_edit::drag_set_mods(s.model(), aw6, false);
    assert!(ohne.contains(&aw2), "ohne Strg: der Stapel");
}

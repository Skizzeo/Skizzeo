//! Prüfung „Sätze für Menschen“ (Bausteingrenze §5, KA-2d1): jeder Satz,
//! der eine [`Meldung`] werden kann, in den Wörtern des Fensters.

use super::*;
use crate::catalog::Company;
use crate::scene::Scene;
use sk_cost::{Dez, Herkunft, HerkunftArt, Op, Rolle, SatzId};
use sk_model::{Guid, Model};

/// Englische Wörter und Wendungen aus Systemtexten (ganzes Wort, ohne
/// Groß-/Kleinschreibung; Bedienbarkeit Durchgang 8, Nachtrag).
const ENGLISCH: [&str; 27] = [
    "error",
    "failed",
    "denied",
    "os error",
    "not found",
    "permission",
    "access",
    "invalid",
    "cannot",
    "could not",
    "unable",
    "unexpected",
    "refused",
    "timed out",
    "timeout",
    "already exists",
    "no such",
    "broken pipe",
    "is a directory",
    "directory",
    "file",
    "path",
    "lock",
    "the system",
    "errno",
    "panicked",
    "unwrap",
];

/// Formen aus Programmtext (als Teil des Satzes).
const FORMEN: [&str; 9] = [
    "(os ", "kind:", "code:", "Err(", "Some(", "None", "\\", ".tmp", "stand-0",
];

/// Deutsche Technikwörter (ganzes Wort, ohne Groß-/Kleinschreibung).
const TECHNIK: [&str; 7] = [
    "Sperre",
    "Guid",
    "Kennung",
    "Abschnitt",
    "Datensatz",
    "Schlüssel",
    "Operation",
];

/// Schlüssel, die zugleich Einheitenzeichen sind („80 kg/m³“).
const EINHEITEN: [&str; 3] = ["t", "kg", "m"];

/// Ganze Wörter (Buchstaben, Ziffern, `_`).
fn woerter(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
}

/// Kommt `wendung` (ein oder mehrere Wörter) als ganze Wörter vor?
fn wendung_in(s: &str, wendung: &str) -> bool {
    let w: Vec<String> = woerter(s).map(str::to_lowercase).collect();
    let p: Vec<String> = woerter(wendung).map(str::to_lowercase).collect();
    w.windows(p.len()).any(|x| x == p.as_slice())
}

/// Was an `s` gegen die Regeln für Sätze verstößt (leer: sauber).
fn verstoesse(s: &str, schluessel: &[&str], ops: &[&str]) -> Vec<String> {
    let mut v = Vec::new();
    let w: Vec<&str> = woerter(s).collect();
    if w.windows(2)
        .any(|x| x[0] == "Regel" && x[1].starts_with(|c: char| c.is_ascii_digit()))
    {
        v.push("Regelnummer".into());
    }
    if s.contains("schlüssel=") {
        v.push("Dateischlüssel".into());
    }
    for k in schluessel {
        if !EINHEITEN.contains(k) && w.contains(k) {
            v.push(format!("Schlüssel „{k}“"));
        }
    }
    for o in ops {
        if s.contains(o) {
            v.push(format!("Op-Name „{o}“"));
        }
    }
    // IFC-Kurzform: 22 Zeichen aus 0-9, A-Z, a-z, _ und $
    let guid = |t: &str| {
        t.len() == 22
            && t.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            && t.chars()
                .any(|c| c.is_ascii_digit() || c == '_' || c == '$')
    };
    if s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
        .any(guid)
    {
        v.push("Guid".into());
    }
    if s.contains('{') || s.contains('}') {
        v.push("Klammer".into());
    }
    if s.split_whitespace()
        .any(|t| t.starts_with('/') || t.matches('/').count() >= 2 || t.contains(":\\"))
    {
        v.push("Pfad".into());
    }
    for e in ENGLISCH.iter().chain(TECHNIK.iter()) {
        if wendung_in(s, e) {
            v.push(format!("Wort „{e}“"));
        }
    }
    for f in FORMEN {
        if s.contains(f) {
            v.push(format!("Form „{f}“"));
        }
    }
    v
}

/// Feste Sätze aus dem Quelltext: der Text in `Meldung::satz("…")` und die
/// Vorlage in `Meldung::mit("…", …)` (Platzhalter als „Wert“).
fn aus_quelltext(quelle: &str, out: &mut Vec<String>) {
    for kopf in ["Meldung::satz(", "Meldung::mit("] {
        for (i, _) in quelle.match_indices(kopf) {
            let rest = quelle[i + kopf.len()..].trim_start();
            let Some(rest) = rest.strip_prefix('"') else {
                continue;
            };
            let mut text = String::new();
            let mut it = rest.chars();
            while let Some(c) = it.next() {
                match c {
                    '"' => break,
                    '\\' => match it.next() {
                        Some('n') => text.push('\n'),
                        Some(x) => text.push(x),
                        None => break,
                    },
                    c => text.push(c),
                }
            }
            out.push(text.replace("{}", "Wert"));
        }
    }
}

fn haus(text: &str) -> Model {
    sk_model::szo::read_with(
        text,
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model
}

fn h() -> Herkunft {
    Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "15:40")
}

fn wert(schluessel: &str, w: i64) -> Op {
    Op::FirmenwertSetzen {
        schluessel: schluessel.into(),
        wert: Dez::ganz(w),
    }
}

/// Je eine Operation jeder Art mit Beispielwerten.
fn alle_ops() -> Vec<Op> {
    let g = Guid(0x4711);
    let daten = sk_cost::op::Bauleistung {
        kurz: "Mauerwerk Porenbeton 17,5".into(),
        gewerk: g,
        titel: g,
        pos: 10,
        einheit: sk_cost::katalog::Einheit::M2,
        bezug: sk_cost::katalog::Bezug::Flaeche,
        stunden: Dez::ganz(1),
        geraet: Dez::NULL,
        sonst: Dez::NULL,
        nu: None,
        kg: Some(331),
        kategorien: vec!["wall".into()],
        mat: None,
        tmin: None,
        tmax: None,
        funktion: None,
    };
    let satz = || SatzId::neu("article", "1S7bUW0010080100000002");
    vec![
        Op::ArtikelAnlegen {
            baustoff: None,
            name: "Planstein PP2".into(),
            dicke: None,
            guete: String::new(),
            format: String::new(),
            einheit: sk_cost::katalog::Einheit::M2,
            preis: Some(Dez::ganz(20)),
            stand: "10/2026".into(),
            quelle: "Händler".into(),
            lieferant: String::new(),
            standard: false,
        },
        Op::PreisSetzen {
            artikel: g,
            preis: Some(Dez::ganz(20)),
            stand: "10/2026".into(),
            quelle: "Preisblatt".into(),
        },
        Op::PreisSetzen {
            artikel: g,
            preis: None,
            stand: "10/2026".into(),
            quelle: "Preisblatt".into(),
        },
        Op::BauleistungAnlegen(daten.clone()),
        Op::BauleistungAendern {
            bauleistung: g,
            daten,
        },
        Op::StoffanteilSetzen {
            bauleistung: g,
            nr: 1,
            anteil: Some(sk_cost::op::Stoff::Schicht {
                faktor: Dez::ganz(1),
            }),
        },
        Op::StoffanteilSetzen {
            bauleistung: g,
            nr: 2,
            anteil: None,
        },
        Op::FolgeSetzen {
            bauleistung: g,
            nr: 1,
            folge: Some((g, Dez::ganz(1))),
        },
        Op::FolgeSetzen {
            bauleistung: g,
            nr: 1,
            folge: None,
        },
        Op::BauleistungZuordnen {
            typ: g,
            schicht: 0,
            bauleistung: Some(g),
        },
        Op::BauleistungZuordnen {
            typ: g,
            schicht: 0,
            bauleistung: None,
        },
        wert("wage", 65),
        wert("surcharge", 12),
        wert("vat", 19),
        wert("steel.groundslab", 80),
        wert("zukunft", 1),
        Op::LosAnlegen {
            name: "Rohbau".into(),
            nr: "01".into(),
            los: None,
        },
        Op::LosAnlegen {
            name: "Mauerarbeiten".into(),
            nr: "02".into(),
            los: Some(g),
        },
        Op::Ausmustern { satz: satz() },
        Op::Wiederherstellen { satz: satz() },
        Op::HerkunftBestaetigen { satz: satz() },
        Op::AbweichungZuruecknehmen {
            saetze: vec![satz()],
        },
        Op::StandUebernehmen {
            saetze: vec![satz()],
        },
        Op::AbgleichLassen { stand: 3 },
        Op::LvGliederungSetzen { untertitel: true },
        Op::LvGliederungSetzen { untertitel: false },
    ]
}

/// Firmenkatalog am Vorgabeort, dazu Zeilen voller Fehler (wie die
/// Fehlerfälle der sk-cost-Tests).
fn kaputte_firma(d: &std::path::Path) -> Company {
    let p = d.join(crate::catalog::FILE_NAME);
    let (_, _) = Company::laden(&p, true);
    let mut text = std::fs::read_to_string(&p).unwrap();
    for l in [
        "[article] guid=0000000000000000000001 name=\"x\" unit=Banane",
        "[rate] key=wage num=65",
        "[rate] key=wage num=70",
        "[rate] key=vat num=101",
        "[article] guid=0000000000000000000002 name=\"y\" mat=0000000000000000000009 unit=m2",
        "[service] guid=0000000000000000000003 short=\"s\" trade=1S7Wf_00100800000004UR title=1S7bUW0010080300000002 pos=99 unit=m2 basis=volume",
        "[service] guid=0000000000000000000006 short=\"t\" trade=1S7Wf_00100800000004UR title=1S7bUW0010080300000002 pos=99 unit=m3 basis=volume hours=abc",
        "[origin] key=0000000000000000000002 rec=article kind=manual status=open date=2026-10-08",
        "[svcpart] guid=0000000000000000000004 service=1S7bUW0010080200000001 nr=1 layer=1 qty=1",
        "[article] guid=0000000000000000000007 name=\"zweiter\" mat=2wuC33GkTD9Qack6WJ4EsM t=175 unit=m2 price=1 std=1",
        "[lot] guid=0000000000000000000005 name=\"x\" nr=\"01\" parent=1S7bUW0010080300000001",
        "[log] key=1 stand=1 time=2026-10-08T07:00 role=admin op=preis_setzen rec=rate of=wage",
        "[log] key=3 stand=9 time=2026-10-08T07:00 role=admin op=preis_setzen rec=rate of=wage",
    ] {
        text.push_str(l);
        text.push('\n');
    }
    std::fs::write(&p, text).unwrap();
    Company::laden(&p, false).0
}

#[test]
fn nutzersaetze_sauber() {
    let d = std::env::temp_dir().join("skizzeo-nutzersaetze");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let mut saetze: Vec<(String, String)> = Vec::new();
    let mut dazu = |wo: &str, s: &str| saetze.push((wo.to_string(), s.to_string()));

    // Befunde der Häuser RH-1 bis RH-3, mit Werk und mit kaputter Firma
    let firma = kaputte_firma(&d);
    let haeuser = [
        include_str!("../../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        include_str!("../../../crates/sk-cost/referenz/rh2-mehrschalig.szo"),
        include_str!("../../../crates/sk-cost/referenz/rh3-versatz-dachterrasse.szo"),
    ];
    for (n, text) in haeuser.iter().enumerate() {
        let m = haus(text);
        for lib in [None, Some(firma.library())] {
            let k = sk_cost::lesen::katalog(&m, lib);
            for b in sk_cost::lesen::befunde(&m, &k) {
                dazu(&format!("RH-{} Befund", n + 1), &b.satz);
            }
            let mut s = Scene::with_model(m.clone());
            let blatt = s.kostenblatt(lib.map(|l| (l, 1)), &sk_model::qto::Umfang::projekt());
            for b in &blatt.befunde {
                dazu(&format!("RH-{} Kostenblatt", n + 1), &b.satz);
            }
            // „Bauleistung wählen“ für jede Zeile ohne Bauleistung
            let kat = s.katalog(lib.map(|l| (l, 1)));
            for z in &blatt.ohne {
                let Some(a) = sk_cost::wahl::auswahl(s.model(), &kat, z) else {
                    continue;
                };
                let zeile = format!("{} {}", z.nummer, "PIR-Dämmung");
                let w = crate::wahl_blatt::WahlBlatt::neu(
                    (0, z.element),
                    &zeile,
                    "13,219 m²".into(),
                    a,
                );
                for t in w.saetze() {
                    dazu("Bauleistung wählen", &t);
                }
                for x in w
                    .saetze()
                    .iter()
                    .filter_map(|t| t.strip_prefix("kommt dann zu "))
                {
                    dazu(
                        "Bauleistung wählen",
                        &crate::kosten_view::gewerk_hinweis(&zeile, x),
                    );
                }
            }
        }
        // Abgelehnte Änderungen: jede Operation mit Beispielwerten, als
        // Verwaltung und als Nutzer
        for op in alle_ops() {
            dazu("Op::bezeichnung", &op.bezeichnung());
            for rolle in [Rolle::Admin, Rolle::Nutzer] {
                if let Err(b) =
                    sk_cost::vorschau(&m, Some(firma.library()), rolle, std::slice::from_ref(&op))
                {
                    for x in b {
                        dazu("abgelehnt", &x.satz);
                    }
                }
            }
        }
    }

    // Jede Regel-Satzfunktion mit Beispielwerten, jedes Feld und jeder
    // Abschnitt der Feldtabelle als Wort
    use sk_cost::befund as r;
    for a in sk_cost::satz::ABSCHNITTE {
        dazu("r72", &r::r72(3, a.name, "ein bekanntes Wort fehlt"));
        dazu("r74", &r::r74(a.name, "Planstein PP2"));
        for f in a.felder {
            dazu("r76", &r::r76("Planstein PP2", f.name, "abc"));
            dazu("r79", &r::r79("Mauerwerk 17,5", f.name, "abc"));
        }
    }
    for t in [
        r::r71(),
        r::r73("Bauleistung Mauerwerk 17,5", "einen Artikel, den"),
        r::r73w("Porenbeton"),
        r::r75("Verrechnungslohn"),
        r::r77("Porenbeton", "17,5 cm", "Planstein PP2"),
        r::r80("Mauerwerk 17,5", "m³", "Fläche"),
        r::r85_kette("Mauerwerk 17,5", "Putz"),
        r::r86("01.0010"),
        r::r87("Planstein PP2"),
        r::r88(),
        r::r88_hand("Planstein PP2"),
        r::r89("Planstein PP2", "3"),
        r::r90("4"),
        r::r91(),
        r::r92(4, "3"),
        r::r93("Lohn 65,00 €/h", "diesen Firmenwert gibt es nicht"),
        r::r99("AW Porenbeton 17,5", "Porenbeton"),
    ] {
        dazu("Regel", &t);
    }

    // Abgleichzeile: ein Wert, mehrere Werte
    let (mut c, _) = Company::laden(&d.join("abgleich").join("firmenkatalog.szk"), true);
    c.fuer_firma(&h(), &[wert("wage", 60)]).unwrap();
    let mut m = haus(haeuser[0]);
    sk_cost::op::kopie_anlegen(&mut m, Some(c.library()));
    c.fuer_firma(&h(), &[wert("wage", 65)]).unwrap();
    let a = sk_cost::abgleich::abgleich(&m, Some(c.library())).expect("ein Unterschied");
    dazu("Abgleich", &a.zeile());
    dazu("Abgleich", &a.tooltip());
    c.fuer_firma(&h(), &[wert("surcharge", 12), wert("steel.groundslab", 95)])
        .unwrap();
    let a = sk_cost::abgleich::abgleich(&m, Some(c.library())).expect("drei Unterschiede");
    assert!(a.zeile().starts_with("3 Änderungen"), "{}", a.zeile());
    assert!(
        a.tooltip()
            .contains("Bewehrungsgrad Sohlplatte 95 kg/m³ (hier 80)"),
        "{}",
        a.tooltip()
    );
    dazu("Abgleich", &a.zeile());
    dazu("Abgleich", &a.tooltip());

    // Rückgängig-Texte des Lohns
    for g in [
        crate::preis_blatt::Gilt::NurHaus,
        crate::preis_blatt::Gilt::NeueHaeuser,
    ] {
        dazu(
            "Lohn",
            &crate::lohn_blatt::LohnBlatt::bezeichnung(Dez::ganz(65), g),
        );
    }

    // Dateifehler jeder Art
    let datei = d.join("firmenkatalog.szk");
    for e in io_beispiele() {
        let m = Meldung::aus_io(
            "Firmenkatalog nicht gespeichert",
            "Firmenkatalog speichern",
            &datei,
            &e,
        );
        dazu("aus_io", &m);
    }
    // fester Fall: ohne Vorsatz
    assert_eq!(
        Meldung::aus_io(
            "Firmenkatalog nicht gespeichert",
            "Firmenkatalog speichern",
            &datei,
            &std::io::Error::other("Access is denied. (os error 5)"),
        ),
        "Firmenkatalog speichern hat nicht geklappt."
    );

    // Feste Sätze: aus dem Quelltext und die Konstanten dahinter
    let mut fest = Vec::new();
    for q in [
        include_str!("../main.rs"),
        include_str!("../catalog.rs"),
        include_str!("../scene.rs"),
        include_str!("../kosten_view.rs"),
        include_str!("../preis_blatt.rs"),
        include_str!("../flush_pick.rs"),
        include_str!("../frame_time.rs"),
        include_str!("../ui.rs"),
        include_str!("../measure_input.rs"),
        include_str!("../meldung.rs"),
    ] {
        aus_quelltext(q, &mut fest);
    }
    assert!(fest.len() > 30, "{fest:?}");
    fest.extend(
        [
            crate::catalog::GESPERRT,
            crate::flush_pick::CANCELLED,
            crate::NICHTS_GEAENDERT,
        ]
        .map(String::from),
    );
    fest.extend(crate::hints::HINTS.iter().map(|(_, t)| t.to_string()));
    for k in [
        crate::ui::MeasureKind::Length,
        crate::ui::MeasureKind::Angle,
        crate::ui::MeasureKind::Offset,
    ] {
        fest.push(crate::ui::MeasureError(k).message().into());
    }
    for e in [
        sk_model::FlushError::NotPartners,
        sk_model::FlushError::Unsupported,
        sk_model::FlushError::Invalid,
    ] {
        fest.push(e.message().into());
    }
    for t in fest {
        dazu("fest", &t);
    }

    // Prüfen
    let kinds: Vec<&str> = sk_model::element::Category::ALL
        .into_iter()
        .map(|c| sk_model::kinds::spec(c).szo)
        .collect();
    let schluessel: Vec<&str> = sk_cost::wort::schluessel().chain(kinds).collect();
    let ops: Vec<&str> = sk_cost::op::NAMEN.iter().map(|(n, ..)| *n).collect();
    assert!(saetze.len() > 300, "{}", saetze.len());
    let schlecht: Vec<String> = saetze
        .iter()
        .filter_map(|(wo, s)| {
            let v = verstoesse(s, &schluessel, &ops);
            (!v.is_empty()).then(|| format!("{wo}: „{s}“ → {}", v.join(", ")))
        })
        .collect();
    let _ = std::fs::remove_dir_all(&d);
    assert!(
        schlecht.is_empty(),
        "{} Sätze nicht für Menschen:\n{}",
        schlecht.len(),
        schlecht.join("\n")
    );
}

/// Die Prüfung selbst fängt, wofür sie da ist.
#[test]
fn pruefung_faengt() {
    let ops = ["preis_setzen"];
    let k = ["price", "name", "groundslab"];
    for (s, soll) in [
        ("Regel 76 · article: price ist ungültig", "Regelnummer"),
        ("Bewehrungsgrad groundslab 80 kg/m³", "Schlüssel"),
        ("Änderung preis_setzen abgelehnt", "Op-Name"),
        ("Artikel 1S7bUW0010080100000002 fehlt", "Guid"),
        (
            "Firmenkatalog C:\\Daten\\firma.szk nicht gespeichert",
            "Pfad",
        ),
        ("Access is denied. (os error 5)", "denied"),
        ("Sperre übernommen", "Sperre"),
        ("{feld} fehlt", "Klammer"),
    ] {
        let v = verstoesse(s, &k, &ops);
        assert!(v.iter().any(|x| x.contains(soll)), "{s}: {v:?}");
    }
    assert!(verstoesse("Bewehrungsgrad Sohlplatte 80 kg/m³ (hier 100)", &k, &ops).is_empty());
    assert!(verstoesse("Name fehlt.", &k, &ops).is_empty());
}

use super::*;

// Abnahmetests zum Vor-Commit F-17b (Koordinator 11:36): unbekannte
// Abschnitte werden roh aufbewahrt und bytegleich zurückgeschrieben, in
// .szo und .szk. Kommt nach dem Fehler-Commit A199 und vor Paket 3. Ab
// diesem Stand gelten die Rücknahmetests der Pakete 3, 5, 6 und 7.
// Spezifikation: test/abnahme-baumpanel.md (Abschnitt F-17b).
//
// Einbau: als `mod fremdabschnitte { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude. Kein Adapter, nur
// `sk_model::szo::{read, write}` und `sk_model::{read_szk, write_szk}`.
//
// A204 stand vorher in a200-a209-sichtbarkeit.rs und ist hierher
// umgezogen. Ein Hinweis beim Öffnen („unbekannter Abschnitt …“) bleibt
// erlaubt, verlangt ist nur der Erhalt.

fn haus(seed: u64) -> String {
    let mut s = Scene::with_model(Model::with_seed(seed));
    gebaeude(&mut s);
    sk_model::szo::write(s.model())
}

fn oeffnen_speichern(text: &str) -> String {
    let l = sk_model::szo::read(text, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    assert!(l.model.check().is_empty(), "{:?}", l.model.check());
    sk_model::szo::write(&l.model)
}

/// Zeilen von `text`, die in `fremd` vorkommen, in Dateireihenfolge.
fn fremde_zeilen<'a>(text: &'a str, fremd: &[&str]) -> Vec<&'a str> {
    text.lines().filter(|l| fremd.contains(l)).collect()
}

/// A204 (paket-3 §5.5/§6, plan B7): Eine Datei mit `[hide]`-Zeilen aller
/// vier Formen öffnet auf dem Stand ohne Paket 3, und Speichern behält die
/// Zeilen bytegleich. Auf 818a6b0 rot (szo.rs:1033 verwirft unbekannte
/// Abschnitte), grün ab F-17b.
#[test]
fn a204_hide_ruecknahme() {
    let mut s = Scene::with_model(Model::with_seed(204));
    gebaeude(&mut s);
    let text = sk_model::szo::write(s.model());
    let g = s.model().elements().iter().next().unwrap().1.guid;
    let tg = s
        .model()
        .trade(s.model().trade_by_code("18345").unwrap())
        .unwrap()
        .guid;
    let hide =
        format!("[hide] elem={g}\n[hide] cat=interior\n[hide] trade={tg}\n[hide] terrain=1\n");
    let neu = oeffnen_speichern(&format!("{text}{hide}"));
    let soll: Vec<&str> = hide.lines().collect();
    assert_eq!(
        fremde_zeilen(&neu, &soll),
        soll,
        "Zeilen bleiben, Reihenfolge auch"
    );
}

/// A211 (F-17b, .szo): Unbekannte Abschnitte mit unbekannten Schlüsseln
/// zwischen zwei bekannten Abschnitten (hier zwischen den Zeilen eines
/// Geschosses und eines Zugs) und am Dateiende. Nach Öffnen und Speichern:
/// - jede fremde Zeile steht genau einmal und bytegleich da (Anführung,
///   Leerzeichen im Wert, Sonderzeichen),
/// - die fremden Zeilen behalten ihre Reihenfolge untereinander,
/// - ohne die fremden Zeilen ist die Datei bytegleich wie ohne sie
///   geschrieben (die bekannten Abschnitte ändern sich nicht),
/// - ein zweiter Rundlauf ändert nichts mehr.
///
/// Wo genau der Schreiber die fremden Zeilen einreiht (an Ort und Stelle
/// oder gesammelt am Ende), ist frei; Vorschlag: an Ort und Stelle hinter
/// dem bekannten Satz davor, wie `write_szk` es mit Fremdzeilen tut.
#[test]
fn a211_szo_fremde_abschnitte_bleiben() {
    let text = haus(211);
    let fremd = [
        "[zukunft] a=1 name=\"zwei Wörter\" b=-0.5",
        "[zukunft] c=µ d=λ",
        "[noch_neuer] x=1",
        "[zukunft] e=\"[run] keine Zeile\" f=",
    ];
    // erste zwei hinter den letzten [storey]-Satz, also vor den ersten
    // [run]-Satz; die dritte mitten zwischen die Züge; die vierte ans Ende
    let mut zeilen: Vec<String> = Vec::new();
    let letzte_storey = text
        .lines()
        .collect::<Vec<_>>()
        .iter()
        .rposition(|l| l.starts_with("[storey]"))
        .unwrap();
    let erster_run = text.lines().position(|l| l.starts_with("[run]")).unwrap();
    assert!(
        letzte_storey < erster_run,
        "Testannahme: Geschosse vor Zügen"
    );
    for (i, l) in text.lines().enumerate() {
        zeilen.push(l.to_string());
        if i == letzte_storey {
            zeilen.push(fremd[0].into());
            zeilen.push(fremd[1].into());
        }
        if i == erster_run {
            zeilen.push(fremd[2].into());
        }
    }
    zeilen.push(fremd[3].into());
    let mit = zeilen.join("\n") + "\n";

    let neu = oeffnen_speichern(&mit);
    assert!(neu.starts_with("SZO 4\n"));
    assert_eq!(
        fremde_zeilen(&neu, &fremd),
        fremd.to_vec(),
        "genau einmal, in Reihenfolge"
    );
    let ohne: String = neu
        .lines()
        .filter(|l| !fremd.contains(l))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(ohne, text, "bekannte Abschnitte unverändert");
    assert_eq!(oeffnen_speichern(&neu), neu, "zweiter Rundlauf");
    // ohne Fremdes bleibt alles wie heute
    assert_eq!(oeffnen_speichern(&text), text);
}

/// A212 (F-17b, .szk): Firmenkatalog mit unbekannten Abschnitten zwischen
/// bekannten Sätzen und am Ende. Nach Lesen und Schreiben stehen die
/// fremden Zeilen bytegleich, einmal und in Reihenfolge da; ohne sie ist
/// der Katalog wie vorher; ein zweiter Rundlauf ändert nichts. (Heute
/// schon durch `Library::foreign` erfüllt, sichert F-17b gegen Rückschritt.)
#[test]
fn a212_szk_fremde_abschnitte_bleiben() {
    let lib = sk_model::read_szk(include_str!("../firmenkatalog_k4.szk")).expect("liest");
    let text = sk_model::write_szk(&lib);
    let fremd = [
        // `[matprop]` (Paket 5) und `[patternpreset]` (Paket 7) sind
        // bekannte Abschnitte; hier nur Namen, die keine Fassung kennt.
        "[matzukunft] mat=x mu=5",
        "[musterzukunft] name=\"Läufer\" gen=bond",
        "[zukunft] y=2",
    ];
    let mut zeilen: Vec<String> = Vec::new();
    let erste_material = text
        .lines()
        .position(|l| l.starts_with("[material]"))
        .unwrap();
    for (i, l) in text.lines().enumerate() {
        if i == erste_material {
            zeilen.push(fremd[0].into());
            zeilen.push(fremd[1].into());
        }
        zeilen.push(l.to_string());
    }
    zeilen.push(fremd[2].into());
    let mit = zeilen.join("\n") + "\n";
    let neu = sk_model::write_szk(&sk_model::read_szk(&mit).expect("liest"));
    assert_eq!(fremde_zeilen(&neu, &fremd), fremd.to_vec());
    let ohne: String = neu
        .lines()
        .filter(|l| !fremd.contains(l))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(ohne, text);
    assert_eq!(sk_model::write_szk(&sk_model::read_szk(&neu).unwrap()), neu);
}

/// A228 (F-17, F-17b; BIM Regel 67): Unbekannte SCHLÜSSEL in bekannten
/// `.szo`-Abschnitten bleiben bytegleich. Auf 818a6b0 verwirft der Leser
/// sie („unbekannter Schlüssel „short“ in [trade] übersprungen“) und
/// schreibt die Zeile ohne sie (gemessen). Fälle: `short="Mauer"` an der
/// Gewerkzeile 18330 (Rücknahme von Regel 67), ein neuer Schlüssel an der
/// `[run]`-Zeile des EG. Nach Öffnen und Speichern stehen beide Zeilen
/// bytegleich da, auch nach dem Verschieben einer gelösten OG-Wand. Ein
/// Hinweis beim Öffnen bleibt erlaubt. Ändert sich der bekannte Teil der
/// Zeile selbst, darf der fremde Schlüssel wegfallen (wie `write_szk`).
#[test]
fn a228_szo_fremde_schluessel_bleiben() {
    let mut s = Scene::with_model(Model::with_seed(228));
    let (eg, og) = gebaeude(&mut s);
    let text = sk_model::szo::write(s.model());
    let gewerk = text
        .lines()
        .find(|l| l.starts_with("[trade]") && l.contains("code=\"18330\""))
        .expect("Gewerk 18330 im Haus")
        .to_string();
    // Zug des EG; geändert wird später nur das gelöste OG
    let g_eg = s.model().run(eg).unwrap().guid;
    let zug = text
        .lines()
        .find(|l| l.starts_with("[run]") && l.contains(&format!("guid={g_eg} ")))
        .expect("Zug EG")
        .to_string();
    let gewerk_neu = format!("{gewerk} short=\"Mauer\"");
    let zug_neu = format!("{zug} zukunft=7 text=\"a b\"");
    let mit = text.replace(&gewerk, &gewerk_neu).replace(&zug, &zug_neu);
    let neu = oeffnen_speichern(&mit);
    assert!(
        neu.lines().any(|l| l == gewerk_neu),
        "short= bleibt: {gewerk}"
    );
    assert!(
        neu.lines().any(|l| l == zug_neu),
        "fremder Schlüssel am Zug bleibt"
    );
    assert_eq!(oeffnen_speichern(&neu), neu, "zweiter Rundlauf");

    // Änderung an einem anderen Zug als dem mit dem fremden Schlüssel
    let l = sk_model::szo::read(&mit, sk_model::GuidGen::with_seed(1)).expect("öffnet");
    let mut w = Scene::with_model(l.model);
    let anderer = og;
    let wand = w.model().wall_at(anderer, 0).unwrap();
    assert!(w.edit_model("Wand verschoben", |m| {
        m.set_linked(wand, false);
        m.move_segment(wand, -300.0).is_some()
    }));
    let geaendert = sk_model::szo::write(w.model());
    assert!(geaendert.lines().any(|l| l == gewerk_neu), "nach Änderung");
    assert!(
        geaendert.lines().any(|l| l == zug_neu),
        "nach Änderung am anderen Zug"
    );
}

//! Dickenband der Leistung (Vertrag 0.5 Nachtrag `aenderung-dicke.md`):
//! die zwölf Grenzfälle der Werkbank `pruefdateien/dicke/*.szb` mit den
//! Befunden aus `erwartet.txt` im Prüfgeschoss EG.

use sk_szb::{pruefen, Bestand, Geschoss};

macro_rules! faelle {
    ($($n:literal),* $(,)?) => {
        [$(($n, include_str!(concat!("../pruefdateien/dicke/", $n)))),*]
    };
}

const FAELLE: [(&str, &str); 12] = faelle!(
    "d_artikel_fehlt.szb",
    "d_dmax_kaputt.szb",
    "d_dmin_groesser.szb",
    "d_doppel.szb",
    "d_formel_fehler.szb",
    "d_im_band.szb",
    "d_konstant_aussen.szb",
    "d_nur_dmin.szb",
    "d_ohne_band.szb",
    "d_ohne_dicke.szb",
    "d_vorgabe_aussen.szb",
    "d_werks_kennung_fremd.szb",
);

fn befunde(text: &str) -> Vec<String> {
    let p = pruefen(text, &Bestand::werk(), &Geschoss::PROBE);
    p.befunde
        .iter()
        .map(|b| {
            let stufe = if b.ist_fehler() { "F" } else { "H" };
            format!("{stufe} {} {}", b.zeile, b.text)
        })
        .collect()
}

/// `=== datei` gefolgt von den Befunden.
fn erwartet() -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for z in include_str!("../pruefdateien/dicke/erwartet.txt").lines() {
        match z.strip_prefix("=== ") {
            Some(n) => out.push((n.trim().to_string(), Vec::new())),
            None if !z.trim().is_empty() => out.last_mut().unwrap().1.push(z.to_string()),
            None => {}
        }
    }
    out
}

#[test]
fn dickenband_wie_die_werkbank() {
    let soll = erwartet();
    assert_eq!(soll.len(), FAELLE.len());
    for (name, text) in FAELLE {
        let s = &soll.iter().find(|(n, _)| n == name).unwrap().1;
        assert_eq!(&befunde(text), s, "{name}");
    }
}

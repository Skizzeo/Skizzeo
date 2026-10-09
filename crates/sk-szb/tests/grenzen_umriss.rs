//! Kosten der Umrissprüfung (Review 3ch-2, Vertrag §11): je Prisma-Exemplar
//! Punktzahl² Rechenschritte. Grenzfälle der Werkbank
//! `tests/grenzen/l_umriss*.szb` mit den Befunden aus `erwartet.txt`.

use sk_szb::{pruefen, Bestand, Geschoss};

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

#[test]
fn umriss_wie_die_werkbank() {
    // 40 Prismen à 256 Punkte: zu aufwendig
    assert_eq!(
        befunde(include_str!("../pruefdateien/grenzen/l_umriss.szb")),
        ["F 0 Bauteil zu aufwendig: mehr als 2.000.000 Rechenschritte"]
    );
    // 20 Prismen: in Ordnung
    assert_eq!(
        befunde(include_str!("../pruefdateien/grenzen/l_umriss_ok.szb")),
        Vec::<String>::new()
    );
}

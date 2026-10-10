//! R7 Datei-Rundlauf: .szo und .szk mit allen Abschnitten (ab KA-0 auch
//! die Kostenabschnitte, ab KA-3b5 `flow`/`flowstep`) laden und
//! speichern. Ohne Änderung bytegleich; mit zufällig eingestreuten
//! gültigen, fremden und kaputten Zeilen bleibt jede Zeile erhalten
//! (Kostenzeilen dürfen hinter die bekannten Sätze wandern), und ein
//! zweiter Rundlauf ist bytegleich. Dazu „Freigeben“ ohne Änderung: fremde
//! und kaputte Zeilen des Firmenkatalogs überstehen es.

use sk_cost::{lesen, verwaltung, Herkunft, HerkunftArt};
use sk_model::{read_szk_with, szo, write_szk, GuidGen};

const HAEUSER: [(&str, &str); 4] = [
    ("RH-1", include_str!("../referenz/rh1-standardhaus.szo")),
    ("RH-2", include_str!("../referenz/rh2-mehrschalig.szo")),
    (
        "RH-3",
        include_str!("../referenz/rh3-versatz-dachterrasse.szo"),
    ),
    // Alle Abschnitte und Schlüssel seit dem 10.10. (Briefing QS §3.2): zwei
    // Gebäude mit Versatz, Perimeterdämmung, Bodenkennwerte, fünf
    // Erweiterungen mit je zwei Exemplaren, Bauleistungen mit `auto=`
    (
        "r7-alle-abschnitte",
        include_str!("../referenz/r7-alle-abschnitte.szo"),
    ),
];

const KATALOGE: [(&str, &str); 3] = [
    ("werk.szk", sk_cost::WERK),
    (
        "firmenkatalog_k4.szk",
        include_str!("../../../app/src/firmenkatalog_k4.szk"),
    ),
    (
        "abnahme_firmenkatalog_k2.szk",
        include_str!("../../../app/src/abnahme_firmenkatalog_k2.szk"),
    ),
];

struct Zufall(u64);

impl Zufall {
    fn n(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn bis(&mut self, n: usize) -> usize {
        (self.n() % n as u64) as usize
    }
    fn guid(&mut self) -> String {
        const Z: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";
        let mut s = String::from(["0", "1", "2", "3"][self.bis(4)]);
        for _ in 0..21 {
            s.push(Z[self.bis(64)] as char);
        }
        s
    }
}

fn szo_rund(t: &str) -> Result<String, String> {
    szo::read_with(t, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .map(|l| szo::write(&l.model))
        .map_err(|e| format!("{e:?}"))
}

fn szk_rund(t: &str) -> Result<String, String> {
    read_szk_with(t, &lesen::ABSCHNITTE_SZK)
        .map(|l| write_szk(&l))
        .map_err(|e| format!("{e:?}"))
}

/// Zeilen ohne Kommentare (`#`), sortiert: Kommentare schreibt das
/// Programm neu, Kostenzeilen dürfen wandern.
/// Zeilen, die in `a` öfter stehen als in `b`.
fn mehr<'a>(a: &'a [&'a str], b: &[&str]) -> Vec<&'a str> {
    let mut b: Vec<&str> = b.to_vec();
    let mut v = Vec::new();
    for l in a {
        match b.iter().position(|x| x == l) {
            Some(k) => {
                b.swap_remove(k);
            }
            None => v.push(*l),
        }
    }
    v
}

fn sortiert(t: &str) -> Vec<&str> {
    let mut v: Vec<&str> = t.lines().filter(|l| !l.starts_with('#')).collect();
    v.sort();
    v
}

fn abschnitt(l: &str) -> &str {
    l.strip_prefix('[')
        .and_then(|r| r.split_once(']'))
        .map_or("", |(a, _)| a)
}

/// Gültige Kostenzeilen aus dem Werksbestand je Abschnitt in `liste`,
/// dazu Muster für die Abschnitte, die der Werksbestand nicht hat.
fn vorlagen(liste: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = sk_cost::WERK
        .lines()
        .filter(|l| liste.contains(&abschnitt(l)))
        .map(str::to_string)
        .collect();
    for z in [
        "[costproject] key=project catalog=firma stand=3 lvstorey=1",
        "[log] key=7 stand=1 time=2026-10-08T07:00 role=admin op=preis_setzen rec=rate of=wage",
        "[proposal] key=1 project=1S7bUW0010080900000001 name=\"Haus Muster\" rec=rate of=wage",
    ] {
        if liste.contains(&abschnitt(z)) {
            v.push(z.to_string());
        }
    }
    v
}

/// Eine zufällige Zeile: gültig (neue Kennung), mit fremdem Schlüssel,
/// mit kaputtem Wert, ohne Kennung, doppelte Kennung, fremder Abschnitt
/// oder Ablauf (`flow`, `flowstep`).
fn zeile(z: &mut Zufall, vorlagen: &[String], i: usize) -> (String, &'static str) {
    let v = &vorlagen[z.bis(vorlagen.len())];
    let neue_kennung = |z: &mut Zufall, v: &str| {
        let g = z.guid();
        let mut out = Vec::new();
        let mut ersetzt = false;
        for t in v.split(' ') {
            if !ersetzt && t.starts_with("guid=") {
                out.push(format!("guid={g}"));
                ersetzt = true;
            } else if !ersetzt && t.starts_with("key=") {
                out.push(format!("key=r7k{i}"));
                ersetzt = true;
            } else {
                out.push(t.to_string());
            }
        }
        out.join(" ")
    };
    match z.bis(7) {
        0 => (neue_kennung(z, v), "gültig"),
        1 => (
            format!("{} zukunft{i}=\"a b\"", neue_kennung(z, v)),
            "fremder Schlüssel",
        ),
        2 => {
            // Ein ungequoteter Wert wird ungültig; die Zeile bleibt lesbar
            let n = neue_kennung(z, v);
            let teile: Vec<&str> = n.split(' ').collect();
            let frei: Vec<usize> = (1..teile.len())
                .filter(|&k| {
                    teile[k]
                        .split_once('=')
                        .is_some_and(|(_, w)| !w.contains('"'))
                })
                .collect();
            let k = frei[z.bis(frei.len())];
            let schluessel = teile[k].split('=').next().unwrap_or("x");
            let mut t: Vec<String> = teile.iter().map(|s| s.to_string()).collect();
            t[k] = format!("{schluessel}=kaputt{i}");
            (t.join(" "), "kaputter Wert")
        }
        3 => {
            let n: Vec<&str> = v
                .split(' ')
                .filter(|t| !t.starts_with("guid=") && !t.starts_with("key="))
                .collect();
            (format!("{} r7={i}", n.join(" ")), "ohne Kennung")
        }
        4 => (format!("{v} r7dup={i}"), "doppelte Kennung"),
        5 => (
            format!("[zukunft{}] guid={} wert=\"x y\" n={i}", i % 3, z.guid()),
            "fremder Abschnitt",
        ),
        _ => (
            if z.bis(2) == 0 {
                format!(
                    "[flow] guid={} name=\"R7 Ablauf {i}\" ask=\"?\" kind=user",
                    z.guid()
                )
            } else {
                format!(
                    "[flowstep] guid={} flow={} nr={i} op=preis_setzen",
                    z.guid(),
                    z.guid()
                )
            },
            "Ablauf",
        ),
    }
}

/// Streut 1–6 Zeilen hinter die Kopfzeile(n) und hängt bei .szo
/// gelegentlich einen fremden Schlüssel an eine Modellzeile.
fn variante(
    z: &mut Zufall,
    text: &str,
    vorlagen: &[String],
    modell: bool,
    lauf: usize,
) -> (String, Vec<(String, &'static str)>) {
    let mut l: Vec<String> = text.lines().map(str::to_string).collect();
    let kopf = l.iter().take_while(|x| !x.starts_with('[')).count().max(1);
    let mut neu = Vec::new();
    for k in 0..1 + z.bis(6) {
        let (s, art) = zeile(z, vorlagen, lauf * 10 + k);
        let pos = kopf + z.bis(l.len() - kopf + 1);
        l.insert(pos, s.clone());
        neu.push((s, art));
    }
    if modell && z.bis(3) == 0 {
        let kand: Vec<usize> = (kopf..l.len())
            .filter(|&k| matches!(abschnitt(&l[k]), "wall" | "slab" | "floor" | "terrace"))
            .collect();
        if !kand.is_empty() {
            let k = kand[z.bis(kand.len())];
            l[k].push_str(&format!(" zukunft{lauf}=1"));
            neu.push((l[k].clone(), "fremder Schlüssel an Modellzeile"));
        }
    }
    (l.join("\n") + "\n", neu)
}

fn pruefen(
    name: &str,
    lauf: usize,
    ein: &str,
    neu: &[(String, &str)],
    rund: &dyn Fn(&str) -> Result<String, String>,
    befunde: &mut Vec<String>,
) {
    let aus = match rund(ein) {
        Ok(a) => a,
        Err(e) => {
            befunde.push(format!(
                "{name} Lauf {lauf}: lädt nicht ({e}); eingestreut {neu:?}"
            ));
            return;
        }
    };
    if sortiert(&aus) != sortiert(ein) {
        let (e, a) = (sortiert(ein), sortiert(&aus));
        let (fehlt, dazu) = (mehr(&e, &a), mehr(&a, &e));
        befunde.push(format!(
            "{name} Lauf {lauf}: Zeilen verändert; fehlt {fehlt:?}, dazu {dazu:?}; eingestreut {neu:?}"
        ));
        return;
    }
    match rund(&aus) {
        Ok(zwei) if zwei == aus => {}
        Ok(_) => befunde.push(format!(
            "{name} Lauf {lauf}: zweiter Rundlauf nicht bytegleich"
        )),
        Err(e) => befunde.push(format!(
            "{name} Lauf {lauf}: Gespeichertes lädt nicht ({e})"
        )),
    }
}

#[test]
fn r7_unveraendert_bytegleich() {
    for (name, t) in HAEUSER {
        assert_eq!(szo_rund(t).as_deref(), Ok(t), "{name}");
    }
    // Kataloge: der Werksbestand ist erzeugt, nicht geschrieben; nach dem
    // ersten Speichern stehen dieselben Zeilen, danach bytegleich
    for (name, t) in KATALOGE {
        let eins = szk_rund(t).unwrap();
        assert_eq!(sortiert(&eins), sortiert(t), "{name}");
        assert_eq!(szk_rund(&eins).as_deref(), Ok(eins.as_str()), "{name}");
    }
}

#[test]
fn r7_zufaellige_zeilen_bleiben() {
    let mut z = Zufall(0x5a1_7e57);
    let mut befunde = Vec::new();
    let v_szo = vorlagen(&lesen::ABSCHNITTE_SZO);
    let v_szk = vorlagen(&lesen::ABSCHNITTE_SZK);
    for lauf in 0..300 {
        let (name, t) = HAEUSER[lauf % HAEUSER.len()];
        let (ein, neu) = variante(&mut z, t, &v_szo, true, lauf);
        pruefen(name, lauf, &ein, &neu, &szo_rund, &mut befunde);
        let (name, t) = KATALOGE[lauf % 3];
        let (ein, neu) = variante(&mut z, t, &v_szk, false, lauf);
        pruefen(name, lauf, &ein, &neu, &szk_rund, &mut befunde);
    }
    assert!(
        befunde.is_empty(),
        "{} Befunde:\n{}",
        befunde.len(),
        befunde.join("\n")
    );
}

/// Freigeben ohne Änderung (Entwurf = Firmenkatalog mit `status=draft`):
/// Jede fremde und kaputte Zeile des Firmenkatalogs steht danach noch in
/// der Datei; nur der Kopf bekommt Stand, Datum und Status.
#[test]
fn r7_freigeben_behaelt_fremde_und_kaputte_zeilen() {
    let mut z = Zufall(0xf3e1_6eb0);
    let v_szk = vorlagen(&lesen::ABSCHNITTE_SZK);
    let herkunft = Herkunft::neu(HerkunftArt::Manual, "2026-10-09", "02:00");
    let mut befunde = Vec::new();
    for lauf in 0..100 {
        let (firma, neu) = variante(&mut z, sk_cost::WERK, &v_szk, false, lauf);
        // Doppelte und neu eingestreute Kopfzeilen lassen wir weg: der Kopf
        // wird beim Freigeben ersetzt
        if neu.iter().any(|(s, _)| abschnitt(s) == "catalog") {
            continue;
        }
        let entwurf = firma.replacen("status=released", "status=draft", 1);
        let neu_text = match verwaltung::freigeben(&firma, &entwurf, &herkunft) {
            Ok(f) => f.text,
            Err(b) => {
                let s: Vec<&str> = b.iter().map(|b| b.satz.as_str()).collect();
                befunde.push(format!(
                    "Lauf {lauf}: Freigeben abgelehnt {s:?}; eingestreut {neu:?}"
                ));
                continue;
            }
        };
        for (s, art) in &neu {
            if abschnitt(s) == "proposal" {
                continue;
            }
            if !neu_text.lines().any(|l| l == s) {
                befunde.push(format!("Lauf {lauf}: {art} verloren: {s}"));
            }
        }
        let ohne_kopf = |t: &str| -> Vec<String> {
            t.lines()
                .filter(|l| abschnitt(l) != "catalog" && !l.starts_with('#'))
                .map(str::to_string)
                .collect()
        };
        let (vorher, nachher) = (ohne_kopf(&firma), ohne_kopf(&neu_text));
        let mut a = vorher.clone();
        let mut b = nachher.clone();
        a.sort();
        b.sort();
        if a != b {
            let a: Vec<&str> = a.iter().map(String::as_str).collect();
            let b: Vec<&str> = b.iter().map(String::as_str).collect();
            let (fehlt, dazu) = (mehr(&a, &b), mehr(&b, &a));
            befunde.push(format!("Lauf {lauf}: fehlt {fehlt:?}, dazu {dazu:?}"));
        }
    }
    assert!(
        befunde.is_empty(),
        "{} Befunde:\n{}",
        befunde.len(),
        befunde.join("\n")
    );
}

/// Eine Zeile mit offenem Anführungszeichen: Die Datei lädt nicht, der
/// Fehler nennt genau diese Zeile (wie `kaputter_verweis_nennt_die_zeile`);
/// gespeichert wird nichts, also geht nichts verloren.
#[test]
fn r7_syntaxfehler_nennt_die_zeile() {
    let mut z = Zufall(0x5e_7a);
    for lauf in 0..60 {
        let (name, t, szk) = if lauf % 2 == 0 {
            let (n, t) = HAEUSER[lauf % HAEUSER.len()];
            (n, t, false)
        } else {
            let (n, t) = KATALOGE[lauf % 3];
            (n, t, true)
        };
        let mut l: Vec<&str> = t.lines().collect();
        let kopf = l.iter().take_while(|x| !x.starts_with('[')).count().max(1);
        let pos = kopf + z.bis(l.len() - kopf + 1);
        let kaputt = format!("[article] guid={} name=\"offen{lauf}", z.guid());
        l.insert(pos, &kaputt);
        let ein = l.join("\n") + "\n";
        let zeile = if szk {
            read_szk_with(&ein, &lesen::ABSCHNITTE_SZK)
                .map(|_| ())
                .map_err(|e| e.line)
        } else {
            szo::read_with(&ein, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
                .map(|_| ())
                .map_err(|e| e.line)
        };
        assert_eq!(zeile, Err(pos + 1), "{name} Lauf {lauf}");
    }
}

/// Regel 74: Kommt eine Kennung doppelt vor, gilt der erste Eintrag. Ein
/// Freigeben ohne Änderung darf daran nichts ändern: Beide Zeilen bleiben,
/// der Lohn bleibt 60.
#[test]
fn r7_freigeben_doppelte_kennung_erster_gilt() {
    let firma = format!("{}[rate] key=wage num=65\n", sk_cost::WERK);
    let lib = read_szk_with(&firma, &lesen::ABSCHNITTE_SZK).unwrap();
    assert_eq!(
        write_szk(&lib).matches("[rate] key=wage ").count(),
        2,
        "Rundlauf"
    );
    let entwurf = firma.replacen("status=released", "status=draft", 1);
    let herkunft = Herkunft::neu(HerkunftArt::Manual, "2026-10-09", "02:00");
    let neu = verwaltung::freigeben(&firma, &entwurf, &herkunft)
        .map_err(|b| b.into_iter().map(|b| b.satz).collect::<Vec<_>>())
        .unwrap()
        .text;
    let lohn: Vec<&str> = neu
        .lines()
        .filter(|l| l.starts_with("[rate] key=wage "))
        .collect();
    assert_eq!(
        lohn,
        ["[rate] key=wage num=60", "[rate] key=wage num=65"],
        "nach Freigeben ohne Änderung"
    );
}

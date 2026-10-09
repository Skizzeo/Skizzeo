//! Abgleich mit der Werkbank 0.5: Sollwerte, die der Test-Thread aus dem
//! Rechenteil der Werkbank erzeugt hat (`pruefdateien/werkbank-sollwerte.json`,
//! `pruefdateien/formeln-soll.tsv`), und die Abnahme des Auftrags.

use std::collections::BTreeMap;

use sk_szb::formel::{self, Umfeld};
use sk_szb::pruefen::typ_werte;
use sk_szb::rechnen::{self, vorgaben};
use sk_szb::{lesen, pruefen, Bestand, Geschoss};

const BEISPIELE: [(&str, &str); 5] = [
    (
        "werk.bodenplatte.szb",
        include_str!("../beispiele/werk.bodenplatte.szb"),
    ),
    (
        "werk.stabgelaender.szb",
        include_str!("../beispiele/werk.stabgelaender.szb"),
    ),
    (
        "werk.streifenfundament.szb",
        include_str!("../beispiele/werk.streifenfundament.szb"),
    ),
    (
        "werk.stuetze.szb",
        include_str!("../beispiele/werk.stuetze.szb"),
    ),
    (
        "werk.treppe.szb",
        include_str!("../beispiele/werk.treppe.szb"),
    ),
];
const FEHLER: &str = include_str!("../pruefdateien/fehler.szb");

// ---------- kleiner JSON-Leser, nur für die Sollwerte ----------

#[derive(Clone, Debug, PartialEq)]
enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    fn get(&self, k: &str) -> &J {
        match self {
            J::Obj(v) => v
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, j)| j)
                .unwrap_or(&J::Null),
            _ => &J::Null,
        }
    }

    fn arr(&self) -> &[J] {
        match self {
            J::Arr(v) => v,
            _ => &[],
        }
    }

    fn obj(&self) -> &[(String, J)] {
        match self {
            J::Obj(v) => v,
            _ => &[],
        }
    }

    fn num(&self) -> Option<f64> {
        match self {
            J::Num(v) => Some(*v),
            _ => None,
        }
    }

    fn str(&self) -> &str {
        match self {
            J::Str(s) => s,
            _ => "",
        }
    }
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

impl P<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn wert(&mut self) -> J {
        self.ws();
        match self.s[self.i] {
            b'{' => {
                self.i += 1;
                let mut v = Vec::new();
                loop {
                    self.ws();
                    if self.s[self.i] == b'}' {
                        self.i += 1;
                        return J::Obj(v);
                    }
                    let J::Str(k) = self.wert() else {
                        panic!("Schlüssel")
                    };
                    self.ws();
                    assert_eq!(self.s[self.i], b':');
                    self.i += 1;
                    v.push((k, self.wert()));
                    self.ws();
                    if self.s[self.i] == b',' {
                        self.i += 1;
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut v = Vec::new();
                loop {
                    self.ws();
                    if self.s[self.i] == b']' {
                        self.i += 1;
                        return J::Arr(v);
                    }
                    v.push(self.wert());
                    self.ws();
                    if self.s[self.i] == b',' {
                        self.i += 1;
                    }
                }
            }
            b'"' => {
                self.i += 1;
                let mut out = Vec::new();
                loop {
                    let c = self.s[self.i];
                    self.i += 1;
                    match c {
                        b'"' => return J::Str(String::from_utf8(out).unwrap()),
                        b'\\' => {
                            let e = self.s[self.i];
                            self.i += 1;
                            match e {
                                b'n' => out.push(b'\n'),
                                b't' => out.push(b'\t'),
                                b'u' => {
                                    let h =
                                        std::str::from_utf8(&self.s[self.i..self.i + 4]).unwrap();
                                    self.i += 4;
                                    let c = char::from_u32(u32::from_str_radix(h, 16).unwrap())
                                        .unwrap();
                                    out.extend_from_slice(c.to_string().as_bytes());
                                }
                                x => out.push(x),
                            }
                        }
                        x => out.push(x),
                    }
                }
            }
            b't' => {
                self.i += 4;
                J::Bool(true)
            }
            b'f' => {
                self.i += 5;
                J::Bool(false)
            }
            b'n' => {
                self.i += 4;
                J::Null
            }
            _ => {
                let a = self.i;
                while self.i < self.s.len() && b"+-.eE0123456789".contains(&self.s[self.i]) {
                    self.i += 1;
                }
                J::Num(
                    std::str::from_utf8(&self.s[a..self.i])
                        .unwrap()
                        .parse()
                        .unwrap(),
                )
            }
        }
    }
}

fn soll() -> J {
    let s = include_str!("../pruefdateien/werkbank-sollwerte.json");
    P {
        s: s.as_bytes(),
        i: 0,
    }
    .wert()
}

// ---------- Formeln ----------

/// Außerhalb von Linux (mingw-libm unter Windows): Winkelfunktionen und
/// Potenzen dürfen um höchstens 4 ulp abweichen wie im Zufallstest; unter
/// Linux gilt die Tabelle bitgenau (Hinweis des Test-Threads zu E1).
fn nah_ausser_linux(f: &str, ist: &str, soll: &str) -> bool {
    if cfg!(target_os = "linux")
        || !["sin", "cos", "tan", "atan", "^"]
            .iter()
            .any(|x| f.contains(x))
    {
        return false;
    }
    let zahl = |t: &str| t.strip_prefix("= ")?.parse::<f64>().ok();
    match (zahl(ist), zahl(soll)) {
        (Some(a), Some(b)) => (a.to_bits() as i64 - b.to_bits() as i64).abs() <= 4,
        _ => false,
    }
}

/// Jede Zeile der Tabelle: gleiches Ergebnis bis aufs Bit oder gleicher
/// Fehlertext.
#[test]
fn formeln_wie_die_werkbank() {
    let mut u = Umfeld::new();
    for (k, v) in [
        ("GH", 2855.0),
        ("DECKE", 220.0),
        ("LICHT", 2635.0),
        ("a", 3.0),
        ("b", 0.0),
        ("l", 3000.0),
    ] {
        u.insert(k.into(), v);
    }
    let mut n = 0;
    for z in include_str!("../pruefdateien/formeln-soll.tsv").lines() {
        let Some((f, soll)) = z.split_once('\t') else {
            continue;
        };
        let f = f.replace("\\.", ".");
        let ist = match formel::rechnen(&f, &u, None) {
            Ok(v) => format!("= {}", js(v)),
            Err(e) => format!("Fehler: {e}"),
        };
        if ist != soll && !nah_ausser_linux(&f, &ist, soll) {
            panic!("Formel {f}: {ist} statt {soll}");
        }
        n += 1;
    }
    assert_eq!(n, 47);
}

/// Zufallsformeln aus dem Rechenteil der Werkbank (Test-Thread,
/// `pruefdateien/formeln-zufall.tsv`, Ergebnis als Bits oder `E` mit Text):
/// Fehler und Fehlertext gleich, Werte bis auf wenige Einheiten der letzten
/// Stelle (sin, cos, atan, pow rechnet die Rust-Bibliothek nicht bitgleich
/// wie V8).
#[test]
fn zufallsformeln_wie_die_werkbank() {
    let mut u = Umfeld::new();
    for (k, v) in [
        ("GH", 2855.0),
        ("DECKE", 220.0),
        ("LICHT", 2635.0),
        ("a", 3.0),
        ("b", 0.0),
        ("l", 3000.0),
    ] {
        u.insert(k.into(), v);
    }
    let mut n = 0;
    for z in include_str!("../pruefdateien/formeln-zufall.tsv").lines() {
        let (f, soll) = z.split_once('\t').expect("Tab");
        let ist = formel::rechnen(f, &u, None);
        match (ist, soll.strip_prefix('=')) {
            (Ok(v), Some(h)) => {
                let s = f64::from_bits(u64::from_str_radix(h, 16).expect("Bits"));
                let ulp = (v.to_bits() as i64)
                    .wrapping_sub(s.to_bits() as i64)
                    .unsigned_abs();
                assert!(v == s || ulp <= 4, "Formel {f}: {v} statt {s}");
            }
            (Err(e), None) => assert_eq!(e, soll[1..], "Formel {f}"),
            (ist, _) => panic!("Formel {f}: {ist:?} statt {soll}"),
        }
        n += 1;
    }
    assert_eq!(n, 4012);
}

/// Zahl wie JavaScript `String(v)` für die Werte der Tabelle.
fn js(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let s = format!("{v}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

// ---------- Beispiele ----------

fn eg_og(bez: &str) -> Geschoss {
    let _ = bez;
    Geschoss::PROBE
}

/// UK des Bezugsgeschosses im Probegebäude der Werkbank.
fn uk(bez: &str) -> f64 {
    if bez == "OG" {
        2855.0
    } else {
        0.0
    }
}

fn befunde(p: &sk_szb::Pruefung) -> Vec<String> {
    p.befunde
        .iter()
        .map(|b| {
            format!(
                "{} {} {}",
                if b.ist_fehler() { "F" } else { "H" },
                b.zeile,
                b.text
            )
        })
        .collect()
}

fn mengen(def: &sk_szb::Def, e: &sk_szb::Ergebnis) -> Vec<(String, String, Option<f64>)> {
    e.mengen
        .iter()
        .map(|(i, v)| {
            let r = &def.menge[*i];
            (
                r.key().to_string(),
                r.get("einheit").unwrap_or("").to_string(),
                *v,
            )
        })
        .collect()
}

fn soll_mengen(j: &J) -> Vec<(String, String, Option<f64>)> {
    j.arr()
        .iter()
        .map(|m| {
            let m = m.arr();
            (m[0].str().to_string(), m[1].str().to_string(), m[2].num())
        })
        .collect()
}

fn soll_vol(j: &J) -> BTreeMap<String, f64> {
    j.obj()
        .iter()
        .map(|(k, v)| (k.clone(), v.num().unwrap()))
        .collect()
}

/// Befunde, Mengen, Volumen, Einfügehöhe, Körper, Werte und Vorgaben in
/// EG und OG, je Beispiel und für fehler.szb, gleich wie die Werkbank.
#[test]
fn beispiele_wie_die_werkbank() {
    let soll = soll();
    let best = Bestand::werk();
    let mut dateien: Vec<(&str, &str)> = BEISPIELE.to_vec();
    dateien.push(("fehler.szb", FEHLER));
    for (name, text) in dateien {
        let text = text.trim_end();
        let s = soll.get(name);
        assert_ne!(*s, J::Null, "{name} fehlt in den Sollwerten");
        for bez in ["EG", "OG"] {
            let sb = s.get(bez);
            let p = pruefen(text, &best, &eg_og(bez));
            let soll_bef: Vec<&str> = sb.get("befunde").arr().iter().map(J::str).collect();
            assert_eq!(befunde(&p), soll_bef, "{name} {bez}: Befunde");
            assert_eq!(
                mengen(&p.def, &p.ergebnis),
                soll_mengen(sb.get("mengen")),
                "{name} {bez}: Mengen"
            );
            assert_eq!(
                p.ergebnis.vol,
                soll_vol(sb.get("vol")),
                "{name} {bez}: Volumen"
            );
            assert_eq!(
                uk(bez) + p.ergebnis.z0,
                sb.get("z0").num().unwrap(),
                "{name} {bez}: z0"
            );
            assert_eq!(
                p.ergebnis.anzahl as f64,
                sb.get("koerper").num().unwrap(),
                "{name} {bez}: Körper"
            );
            let werte: Vec<(String, f64)> = p
                .ergebnis
                .werte
                .iter()
                .map(|(i, v)| (p.def.wert[*i].key().to_string(), *v))
                .collect();
            let soll_werte: Vec<(String, f64)> = sb
                .get("werte")
                .arr()
                .iter()
                .map(|w| (w.arr()[0].str().to_string(), w.arr()[1].num().unwrap()))
                .collect();
            assert_eq!(werte, soll_werte, "{name} {bez}: Werte");
            let pv = vorgaben(&p.def, &Geschoss::PROBE);
            assert_eq!(pv, soll_vol(sb.get("pv")), "{name} {bez}: Vorgaben");
        }
        // Jeder Typ mit seinen Mengen
        let (def, _) = lesen(text);
        for (k, t) in s.get("typen").obj() {
            let r = def.typ.iter().find(|r| r.key() == k).unwrap();
            let mut pv = vorgaben(&def, &Geschoss::PROBE);
            for (p, v) in typ_werte(r.get("werte").unwrap()).unwrap() {
                pv.insert(p, v);
            }
            assert_eq!(pv, soll_vol(t.get("pv")), "{name} Typ {k}: Werte");
            let e = rechnen::rechnen(&def, &pv, &Geschoss::PROBE);
            assert_eq!(
                mengen(&def, &e),
                soll_mengen(t.get("mengen")),
                "{name} Typ {k}: Mengen"
            );
            assert_eq!(e.vol, soll_vol(t.get("vol")), "{name} Typ {k}: Volumen");
            assert_eq!(
                e.anzahl as f64,
                t.get("koerper").num().unwrap(),
                "{name} Typ {k}: Körper"
            );
            let f: Vec<String> = e
                .befunde
                .iter()
                .map(|b| {
                    format!(
                        "{} {} {}",
                        if b.ist_fehler() { "F" } else { "H" },
                        b.zeile,
                        b.text
                    )
                })
                .collect();
            let sf: Vec<&str> = t.get("fehler").arr().iter().map(J::str).collect();
            assert_eq!(f, sf, "{name} Typ {k}: Befunde");
        }
    }
}

/// Abnahme des Auftrags: alle fünf Beispiele ohne Fehler, die Mengen der
/// Tabelle (gerundet wie angezeigt), fehler.szb mit elf Fehlern abgewiesen.
#[test]
fn abnahme_auftrag() {
    let best = Bestand::werk();
    let soll: [(&str, &[(&str, &str)]); 5] = [
        (
            "werk.stuetze.szb",
            &[
                ("beton", "0,152"),
                ("schalung", "2,53"),
                ("stahl", "0,023"),
                ("stueck", "1"),
            ],
        ),
        (
            "werk.streifenfundament.szb",
            &[("beton", "2,000"), ("stahl", "0,080")],
        ),
        (
            "werk.bodenplatte.szb",
            &[("beton", "4,800"), ("rand", "20,00"), ("stahl", "0,384")],
        ),
        ("werk.stabgelaender.szb", &[("lg", "3,00"), ("st", "4")]),
        (
            "werk.treppe.szb",
            &[
                ("beton", "1,546"),
                ("schalung", "21,24"),
                ("stahl", "0,155"),
                ("stueck", "1"),
            ],
        ),
    ];
    for (name, mengen) in soll {
        let text = BEISPIELE.iter().find(|(n, _)| *n == name).unwrap().1;
        let p = pruefen(text, &best, &Geschoss::PROBE);
        assert!(p.einlesbar(), "{name}: {:?}", p.befunde);
        for (k, s) in mengen {
            let (i, v) = p
                .ergebnis
                .mengen
                .iter()
                .find(|(i, _)| p.def.menge[*i].key() == *k)
                .unwrap();
            let stellen = match p.def.menge[*i].get("einheit") {
                Some("stk") => 0,
                Some("m3" | "t") => 3,
                _ => 2,
            };
            assert_eq!(sk_szb::zahl(v.unwrap(), stellen), *s, "{name} {k}");
        }
    }
    let p = pruefen(FEHLER, &best, &Geschoss::PROBE);
    assert!(!p.einlesbar());
    assert_eq!(p.fehler(), 11, "{:?}", p.befunde);
}

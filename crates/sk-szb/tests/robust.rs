//! Robustheit gegen fremde Dateien (Review, Durchsicht Schrittplan .szb):
//! zufällig beschädigte Beispiele und zufällige Formeln durch Leser,
//! Prüfung und Rechnung. Kein Absturz, kein Hängen; jede Rechnung bleibt
//! in ihren Grenzen. Zufall aus einem eigenen LCG, damit jeder Lauf gleich
//! ist.

use std::time::{Duration, Instant};

use sk_szb::formel::{self, Umfeld};
use sk_szb::{pruefen, Bestand, Geschoss};

const BEISPIELE: [&str; 6] = [
    include_str!("../beispiele/werk.bodenplatte.szb"),
    include_str!("../beispiele/werk.stabgelaender.szb"),
    include_str!("../beispiele/werk.streifenfundament.szb"),
    include_str!("../beispiele/werk.stuetze.szb"),
    include_str!("../beispiele/werk.treppe.szb"),
    include_str!("../pruefdateien/fehler.szb"),
];

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn bis(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const ERSATZ: [&str; 14] = [
    "0", "-1", "1e308", "-1e308", "inf", "NaN", "", "\"", "\\", "ä", "((((", "1/0", "500", "501",
];

/// Ändert einen Bauteiltext zufällig: Zeilen löschen oder verdoppeln,
/// Werte ersetzen, Zeichen einstreuen.
fn beschaedigen(r: &mut Lcg, text: &str) -> String {
    let mut zeilen: Vec<String> = text.lines().map(str::to_string).collect();
    for _ in 0..1 + r.bis(4) {
        if zeilen.is_empty() {
            break;
        }
        let i = r.bis(zeilen.len());
        match r.bis(5) {
            0 => {
                zeilen.remove(i);
            }
            1 => {
                let z = zeilen[i].clone();
                zeilen.insert(i, z);
            }
            2 => {
                // Wert nach einem '=' ersetzen
                let z = &zeilen[i];
                let gl: Vec<usize> = z
                    .char_indices()
                    .filter(|(_, c)| *c == '=')
                    .map(|(k, _)| k)
                    .collect();
                if let Some(&k) = gl.get(r.bis(gl.len())) {
                    let rest = &z[k + 1..];
                    let ende = if let Some(r2) = rest.strip_prefix('"') {
                        r2.find('"').map_or(rest.len(), |e| e + 2)
                    } else {
                        rest.find(' ').unwrap_or(rest.len())
                    };
                    let neu = format!(
                        "{}{}{}",
                        &z[..k + 1],
                        ERSATZ[r.bis(ERSATZ.len())],
                        &rest[ende..]
                    );
                    zeilen[i] = neu;
                }
            }
            3 => {
                let z = &zeilen[i];
                let ks: Vec<usize> = z.char_indices().map(|(k, _)| k).collect();
                let k = ks.get(r.bis(ks.len())).copied().unwrap_or(0);
                let c = ['"', '\\', '#', '[', ']', '=', ' ', 'ü', '(', ')', ';', ','][r.bis(12)];
                zeilen[i] = format!("{}{}{}", &z[..k], c, &z[k..]);
            }
            _ => {
                let z = &zeilen[i];
                let ks: Vec<usize> = z.char_indices().map(|(k, _)| k).collect();
                let k = ks.get(r.bis(ks.len())).copied().unwrap_or(0);
                zeilen[i] = z[..k].to_string();
            }
        }
    }
    zeilen.join("\n")
}

/// Zufällige Formel aus Zahlen, Namen, Operatoren, Funktionen.
fn formel(r: &mut Lcg, tiefe: usize) -> String {
    if tiefe == 0 || r.bis(4) == 0 {
        return match r.bis(6) {
            0 => format!("{}", r.bis(1000)),
            1 => "1e308".into(),
            2 => ["GH", "DECKE", "LICHT", "a", "b", "q", "pi"][r.bis(7)].into(),
            3 => ".5".into(),
            4 => "0".into(),
            _ => "-1".into(),
        };
    }
    let a = formel(r, tiefe - 1);
    let b = formel(r, tiefe - 1);
    match r.bis(10) {
        0 => format!("{a}+{b}"),
        1 => format!("{a}*{b}"),
        2 => format!("{a}/{b}"),
        3 => format!("{a}^{b}"),
        4 => format!("-({a})"),
        5 => format!("wenn({a},{b},{a})"),
        6 => format!(
            "{}({a})",
            ["abs", "wurzel", "sin", "atan", "rund", "auf"][r.bis(6)]
        ),
        7 => format!("max({a},{b},{a})"),
        8 => format!("{a}<{b}"),
        _ => format!("({a}-{b})"),
    }
}

#[test]
fn beschaedigte_dateien() {
    let best = Bestand::werk();
    let mut r = Lcg(20261009);
    let start = Instant::now();
    let mut abgewiesen = 0;
    for k in 0..1500 {
        let text = beschaedigen(&mut r, BEISPIELE[k % BEISPIELE.len()]);
        let t = Instant::now();
        let p = pruefen(&text, &best, &Geschoss::PROBE);
        assert!(
            t.elapsed() < Duration::from_secs(2),
            "Fall {k} zu langsam:\n{text}"
        );
        if !p.einlesbar() {
            abgewiesen += 1;
            assert!(p.befunde.iter().any(|b| b.ist_fehler()));
        }
        for v in p.ergebnis.vol.values() {
            assert!(v.is_finite(), "Fall {k}");
        }
        for (_, v) in &p.ergebnis.mengen {
            assert!(v.is_none_or(f64::is_finite), "Fall {k}");
        }
    }
    assert!(abgewiesen > 300, "{abgewiesen}");
    assert!(start.elapsed() < Duration::from_secs(60));
}

#[test]
fn zufaellige_formeln() {
    let mut r = Lcg(7);
    let mut u = Umfeld::new();
    for (k, v) in [
        ("GH", 2855.0),
        ("DECKE", 220.0),
        ("LICHT", 2635.0),
        ("a", 3.0),
        ("b", 0.0),
    ] {
        u.insert(k.into(), v);
    }
    for _ in 0..20_000 {
        let t = 1 + r.bis(7);
        let f = formel(&mut r, t);
        if let Ok(v) = formel::rechnen(&f, &u, None) {
            assert!(v.is_finite(), "{f}");
        }
    }
    // Zufällige Bytes als Formel und als Datei
    for _ in 0..5_000 {
        let n = r.bis(60);
        let s: String = (0..n)
            .map(|_| {
                char::from_u32([32 + r.bis(95) as u32, 0xe4, 0xfeff, 0x202e][r.bis(4)]).unwrap()
            })
            .collect();
        let _ = formel::rechnen(&s, &u, None);
        let _ = pruefen(&format!("SZB 0\n{s}"), &Bestand::werk(), &Geschoss::PROBE);
    }
}

/// Feste Grenzfälle: tiefe Verschachtelung, große `anzahl`, viele Punkte,
/// viele Körper, Umlaute an Byte-Grenzen.
#[test]
fn feste_grenzfaelle() {
    let best = Bestand::werk();
    let g = Geschoss::PROBE;
    let kopf = "SZB 0\n[bauteil] key=a.b name=\"x\" mehrzahl=\"x\" praefix=XY\n[hoehe] ab=uk\n";
    let fall = |zeilen: &str| pruefen(&format!("{kopf}{zeilen}"), &best, &g);
    let p = fall(&format!("[param] key=a name=\"a\" wert=\"{}1\"\n[koerper] form=quader baustoff=stahlbeton b=1 t=1 h=1\n", "-".repeat(5000)));
    assert!(!p.einlesbar());
    for anzahl in ["1e12", "501", "-1", "2.5", "1e308"] {
        let p = fall(&format!(
            "[koerper] form=quader baustoff=stahlbeton anzahl={anzahl} b=1 t=1 h=1\n"
        ));
        assert!(!p.einlesbar(), "{anzahl}");
    }
    let viele: Vec<String> = (0..300).map(|i| format!("{i},{}", i % 2)).collect();
    let p = fall(&format!(
        "[koerper] form=prisma ebene=xy baustoff=stahlbeton punkte=\"{}\" von=0 bis=1\n",
        viele.join("; ")
    ));
    assert!(
        p.befunde
            .iter()
            .any(|b| b.text.contains("mehr als 256 Punkte")),
        "{:?}",
        p.befunde
    );
    let koerper = "[koerper] form=quader baustoff=stahlbeton anzahl=500 b=1 t=1 h=1\n".repeat(5);
    let p = fall(&koerper);
    assert!(
        p.befunde
            .iter()
            .any(|b| b.text.contains("mehr als 2000 Körper")),
        "{:?}",
        p.befunde
    );
    for s in ["01/2ä0", "0ä/2026", "äää"] {
        let _ = fall(&format!("[artikel] key=x name=\"x\" einheit=m stand={s}\n"));
    }
    for e in ["A2-sä", "B-s1,ä", "Cfl-ä"] {
        let _ = fall(&format!(
            "[baustoff] key=x name=\"x\" kategorie=metal euroklasse={e}\n"
        ));
    }
    // Zeile und Datei zu lang
    let p = pruefen(
        &format!("SZB 0\n[notiz] text=\"{}\"\n", "x".repeat(30_000)),
        &best,
        &g,
    );
    assert!(p.befunde.iter().any(|b| b.text.contains("Zeile länger")));
    let p = pruefen(&"#".repeat(2 << 20), &best, &g);
    assert!(p.befunde.iter().any(|b| b.text.contains("Datei größer")));
}

/// Review 3cg: Eine gültige, kleine Datei (37 KB) mit vier Körpern zu je
/// `anzahl=500`, Formeln von 1900 Zeichen und 52 Parametern mit min/max
/// braucht in der Prüfung 169 s (release): Jede Formel wird je Exemplar
/// neu übersetzt, und die Grenzprüfung rechnet alles 2·P-mal. Die Zeit
/// muss begrenzt sein, mit Fehler „zu aufwendig“ statt Rechnen.
#[test]
fn aufwand_begrenzt() {
    let basis = include_str!("../beispiele/werk.stuetze.szb");
    let mut f = String::from("b");
    while f.len() < 1900 {
        f.push_str("+0*1");
    }
    let mut text = String::new();
    for l in basis.lines() {
        if l.starts_with("[koerper]") {
            for _ in 0..4 {
                text.push_str(&format!(
                    "[koerper] form=quader baustoff=stahlbeton teil=\"S\" funktion=loadbearing anzahl=500 x=\"{f}\" y=\"{f}\" z=0 b=\"{f}\" t=\"{f}\" h=h\n"
                ));
            }
            continue;
        }
        text.push_str(l);
        text.push('\n');
        if l.starts_with("[param] key=d") {
            for i in 0..50 {
                text.push_str(&format!(
                    "[param] key=p{i} name=\"P{i}\" einheit=mm wert=1 min=0 max=2 hilfe=\"x\"\n"
                ));
            }
        }
    }
    assert!(text.len() < 40_000);
    let t = Instant::now();
    let _ = pruefen(&text, &Bestand::werk(), &Geschoss::PROBE);
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
}

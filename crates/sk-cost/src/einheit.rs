//! Preis in anderer Einheit (BIM Regel 108, verwaltung.md §10a, KA-3a7):
//! Händler nennen Steinpreise je m³ oder je Stück. Gespeichert wird immer
//! der Preis in der Einheit des Artikels, **einmal** auf 4 Nachkommastellen
//! gerundet (Feldgrenze `price`), halb auf; auf Cent rundet erst die
//! Rechnung nach Regel 83.

use crate::befund::{Befund, Ort};
use crate::geld::{runden, Dez};
use crate::katalog::{Artikel, Einheit};
use crate::wort;

/// Ein `conv`, das Skizzeo aus „L×H“ in `format` vorschlägt. Es gilt erst
/// nach Bestätigung (Regel 108).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vorschlag {
    /// Stück je Einheit des Artikels, auf 4 Nachkommastellen.
    pub conv: Dez,
    /// Länge und Höhe in mm aus `format`.
    pub l: u32,
    pub h: u32,
    /// Fuge in mm: 1 bei Dünnbettmörtel (Planstein, Planbauplatte), sonst 10.
    pub fuge: u32,
}

impl Vorschlag {
    /// „6,6667 Steine je m² (aus 599×249, Fuge 1 mm)“
    pub fn text(&self, a: &Artikel) -> String {
        format!(
            "{} Steine je {} (aus {}×{}, Fuge {} mm)",
            zahl(self.conv, 0),
            a.einheit.zeichen(),
            self.l,
            self.h,
            self.fuge
        )
    }
}

/// Umgerechneter Preis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Umgerechnet {
    /// Preis in der Einheit des Artikels, auf 4 Nachkommastellen.
    pub preis: Dez,
    /// Die Rechnung für die Anzeige: „0,85 €/St × 6,67 St/m² = 5,6695 €/m²“.
    pub rechnung: String,
    /// Für `[origin] source`: „eingegeben 0,85 €/St × 6,67 St/m²“.
    pub eingabe: String,
}

/// Name einer Eingabeeinheit im Segment und in Sätzen: „je m³“, „je Stück“.
pub fn je_text(e: Einheit) -> String {
    match e {
        Einheit::St => "je Stück".into(),
        e => format!("je {}", e.zeichen()),
    }
}

/// Einheiten, in denen man den Preis dieses Artikels eingeben kann; die
/// erste ist die Einheit des Artikels. Nur was geht (paket-ka3a KA-3a7):
/// je m³ nur bei einem Artikel in m² mit Dicke, je Stück nur mit `conv`
/// oder einem lesbaren „L×H“ in `format`. Ein Artikel in Stück hat nur
/// seine Einheit; dann zeigt das Fenster kein Segment.
pub fn angebot(a: &Artikel) -> Vec<Einheit> {
    let mut v = vec![a.einheit];
    if a.einheit == Einheit::M2 && a.t.is_some_and(|t| t > Dez::NULL) {
        v.push(Einheit::M3);
    }
    if a.einheit != Einheit::St && (a.conv.is_some() || vorschlag(a).is_some()) {
        v.push(Einheit::St);
    }
    v
}

/// `conv` aus „L×H“ in `format`, wenn der Artikel keines hat:
/// 1.000.000 ÷ ((L + Fuge) × (H + Fuge)), nur bei einem Artikel in m².
pub fn vorschlag(a: &Artikel) -> Option<Vorschlag> {
    if a.conv.is_some() || a.einheit != Einheit::M2 {
        return None;
    }
    let format = a.satz.text("format")?;
    let (l, h) = l_mal_h(format)?;
    let fuge = fuge(a, format);
    let n = ((l + fuge) * (h + fuge)) as i128;
    // Dez hat 6 Stellen: 10^6 × 10^6 / n, dann auf 4 Stellen
    let conv = Dez((runden(1_000_000_000_000, n * 100) * 100) as i64);
    (conv > Dez::NULL && conv <= Dez::ganz(10_000)).then_some(Vorschlag { conv, l, h, fuge })
}

/// Rechnet einen Preis in `von` auf die Einheit des Artikels um (Regel
/// 108). `conv`: bestätigtes `conv`, sonst das des Artikels.
pub fn umrechnen(
    eingabe: Dez,
    von: Einheit,
    a: &Artikel,
    conv: Option<Dez>,
) -> Result<Umgerechnet, Befund> {
    let ziel = a.einheit.zeichen();
    let nicht = |feld: &str| {
        let t = format!(
            "Preis {} ist nicht wählbar: Artikel {} hat keine {}.",
            je_text(von),
            a.name,
            wort::feld(Some("article"), feld)
        );
        Befund::fehler(
            108,
            t,
            Ort::Satz {
                abschnitt: "article",
                kennung: a.guid.to_ifc(),
            },
        )
    };
    let (faktor, teiler, wort_faktor) = match von {
        e if e == a.einheit => {
            return Ok(Umgerechnet {
                preis: eingabe,
                rechnung: String::new(),
                eingabe: String::new(),
            })
        }
        Einheit::M3 if a.einheit == Einheit::M2 => {
            let t = a.t.filter(|t| *t > Dez::NULL).ok_or_else(|| nicht("t"))?;
            // Dicke in mm → m
            let m = Dez(t.0 / 1000);
            let genau = t.0 % 1000 == 0;
            let w = if genau {
                format!("{} m", zahl(m, 0))
            } else {
                format!("{} mm", zahl(t, 0))
            };
            (t.0 as i128, 1_000 * Dez::SKALA as i128, w)
        }
        Einheit::St if a.einheit != Einheit::St => {
            let c = conv.or(a.conv).ok_or_else(|| nicht("conv"))?;
            (
                c.0 as i128,
                Dez::SKALA as i128,
                format!("{} St/{ziel}", zahl(c, 0)),
            )
        }
        _ => return Err(nicht("unit")),
    };
    // Einmal auf 4 Stellen (10^-4 = 100 Einheiten von Dez), halb auf
    let preis = runden(eingabe.0 as i128 * faktor, teiler * 100) * 100;
    let preis = Dez(i64::try_from(preis).map_err(|_| nicht("price"))?);
    let links = format!("{} €/{} × {wort_faktor}", zahl(eingabe, 2), von.zeichen());
    Ok(Umgerechnet {
        preis,
        rechnung: format!("{links} = {} €/{ziel}", zahl(preis, 2)),
        eingabe: format!("eingegeben {links}"),
    })
}

/// „599×249“, „599 x 249 mm“: genau zwei ganze Zahlen in mm. „NF, 48 St/m²“
/// oder „240×115×71“ sind nicht lesbar (verwaltung.md §10a: eine stille
/// Rechnung aus dem Text wäre geraten).
fn l_mal_h(format: &str) -> Option<(u32, u32)> {
    let s = format.trim();
    let s = s.strip_suffix("mm").unwrap_or(s).trim_end();
    let mut teile = s.split(['×', 'x', 'X']).map(str::trim);
    let l = mass(teile.next()?)?;
    let h = mass(teile.next()?)?;
    teile.next().is_none().then_some((l, h))
}

fn mass(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok().filter(|v| (1..=10_000).contains(v))
}

/// Fuge in mm: Plansteine und Planbauplatten liegen in Dünnbettmörtel.
fn fuge(a: &Artikel, format: &str) -> u32 {
    let duenn = [a.name.as_str(), format, a.satz.text("grade").unwrap_or("")]
        .iter()
        .any(|t| {
            let t = t.to_lowercase();
            t.contains("plan") || t.contains("dünnbett")
        });
    if duenn {
        1
    } else {
        10
    }
}

/// Deutsche Zahl mit mindestens `min` Nachkommastellen: „0,85“, „6,67“,
/// „16,625“, „1.234,50“.
pub fn zahl(d: Dez, min: usize) -> String {
    let t = d.text();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r.to_string()),
        None => (false, t),
    };
    let (g, b) = match t.split_once('.') {
        Some((g, b)) => (g.to_string(), b.to_string()),
        None => (t, String::new()),
    };
    let mut s = String::new();
    for (i, c) in g.chars().enumerate() {
        if i > 0 && (g.len() - i).is_multiple_of(3) {
            s.push('.');
        }
        s.push(c);
    }
    let mut b = b;
    while b.len() < min {
        b.push('0');
    }
    let vz = if neg { "−" } else { "" };
    if b.is_empty() {
        format!("{vz}{s}")
    } else {
        format!("{vz}{s},{b}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::katalog::Katalog;

    fn werk() -> Katalog {
        crate::lesen::werk(&sk_model::Model::new())
    }

    fn artikel<'a>(k: &'a Katalog, name: &str) -> &'a Artikel {
        k.artikel
            .iter()
            .find(|a| a.name.starts_with(name))
            .unwrap_or_else(|| panic!("{name}"))
    }

    /// Abnahme 15a (verwaltung.md §15 Fall 15): A-PB240 mit 0,85 €/St.
    #[test]
    fn je_stueck_und_je_m3() {
        let k = werk();
        let pb240 = artikel(&k, "Porenbeton-Planstein PP2-0,35 d=24cm");
        assert_eq!(pb240.conv, Some(Dez(6_670_000)));
        let u = umrechnen(Dez(850_000), Einheit::St, pb240, None).unwrap();
        assert_eq!(u.preis, Dez(5_669_500));
        assert_eq!(u.rechnung, "0,85 €/St × 6,67 St/m² = 5,6695 €/m²");
        assert_eq!(u.eingabe, "eingegeben 0,85 €/St × 6,67 St/m²");
        let pb175 = artikel(&k, "Porenbeton-Planstein PP2-0,35 d=17,5cm");
        let u = umrechnen(Dez::ganz(95), Einheit::M3, pb175, None).unwrap();
        assert_eq!(u.preis, Dez(16_625_000));
        assert_eq!(u.rechnung, "95,00 €/m³ × 0,175 m = 16,625 €/m²");
        assert_eq!(angebot(pb175), vec![Einheit::M2, Einheit::M3, Einheit::St]);
    }

    /// Einmal auf 4 Stellen, halb auf.
    #[test]
    fn einmal_gerundet() {
        let k = werk();
        let pb240 = artikel(&k, "Porenbeton-Planstein PP2-0,35 d=24cm");
        // 0,333 × 6,67 = 2,22111 → 2,2211
        let u = umrechnen(Dez(333_000), Einheit::St, pb240, None).unwrap();
        assert_eq!(u.preis, Dez(2_221_100));
        // 0,0015 × 6,67 = 0,010005 → 0,0100
        let u = umrechnen(Dez(1_500), Einheit::St, pb240, None).unwrap();
        assert_eq!(u.preis, Dez(10_000));
        // 1,00075 × 1 (conv 1) → 1,0008
        let u = umrechnen(Dez(1_000_750), Einheit::St, pb240, Some(Dez::EINS)).unwrap();
        assert_eq!(u.preis, Dez(1_000_800));
    }

    /// Ohne `t` kein „je m³“, ein Artikel in Stück zeigt kein Segment,
    /// ohne `conv` mit „599×249“ der Vorschlag 6,6667 (Fuge 1 mm).
    #[test]
    fn nur_was_geht() {
        let k = werk();
        let mut a = artikel(&k, "Porenbeton-Planstein PP2-0,35 d=24cm").clone();
        a.t = None;
        assert!(!angebot(&a).contains(&Einheit::M3));
        let b = umrechnen(Dez::ganz(95), Einheit::M3, &a, None).unwrap_err();
        assert_eq!(b.regel, 108);
        assert!(b.satz.contains("hat keine Dicke."), "{}", b.satz);
        let mut st = a.clone();
        st.einheit = Einheit::St;
        st.conv = None;
        assert_eq!(angebot(&st), vec![Einheit::St]);

        a.conv = None;
        a.satz
            .setzen("format", Some(crate::satz::Wert::Text("599×249".into())));
        let v = vorschlag(&a).unwrap();
        assert_eq!((v.conv, v.fuge), (Dez(6_666_700), 1));
        assert_eq!(v.text(&a), "6,6667 Steine je m² (aus 599×249, Fuge 1 mm)");
        assert!(angebot(&a).contains(&Einheit::St));
        assert!(umrechnen(Dez::EINS, Einheit::St, &a, None).is_err());
        let u = umrechnen(Dez(850_000), Einheit::St, &a, Some(v.conv)).unwrap();
        assert_eq!(u.preis, Dez(5_666_700));

        a.name = "Kalksandstein".into();
        a.satz.setzen(
            "format",
            Some(crate::satz::Wert::Text("240 x 113 mm".into())),
        );
        let v = vorschlag(&a).unwrap();
        assert_eq!((v.l, v.h, v.fuge), (240, 113, 10));
        // 1.000.000 ÷ (250 × 123) = 32,5203…
        assert_eq!(v.conv, Dez(32_520_300));
        for f in ["NF, 48 St/m²", "240×115×71", "Sack 25 kg", "", "0×249"] {
            a.satz
                .setzen("format", Some(crate::satz::Wert::Text(f.into())));
            assert_eq!(vorschlag(&a), None, "{f}");
        }
    }

    #[test]
    fn zahlen() {
        assert_eq!(zahl(Dez(850_000), 2), "0,85");
        assert_eq!(zahl(Dez(6_670_000), 0), "6,67");
        assert_eq!(zahl(Dez(16_625_000), 2), "16,625");
        assert_eq!(zahl(Dez::ganz(1234), 2), "1.234,00");
        assert_eq!(zahl(Dez::ganz(48), 0), "48");
    }
}

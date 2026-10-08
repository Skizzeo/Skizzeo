//! Preis in anderer Einheit (BIM Regel 108, verwaltung.md §10a, KA-3a7):
//! Händler nennen Steinpreise je m³ oder je Stück. Gespeichert wird immer
//! der Preis in der Einheit des Artikels, **einmal** auf 4 Nachkommastellen
//! gerundet (Feldgrenze `price`), halb auf; auf Cent rundet erst die
//! Rechnung nach Regel 83.

use crate::befund::{Befund, Ort};
use crate::geld::{runden, Dez};
use crate::katalog::{Artikel, Einheit};
use crate::wort;
use sk_model::library::MatCategory;

/// Ein `conv`, das Skizzeo aus „L×H“ in `format` vorschlägt. Es gilt erst
/// nach Bestätigung (Regel 108).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vorschlag {
    /// Stück je Einheit des Artikels, auf 4 Nachkommastellen.
    pub conv: Dez,
    /// Länge und Höhe in mm aus `format`.
    pub l: u32,
    pub h: u32,
    /// Stoß- und Lagerfuge in mm ([`fuge`]).
    pub fuge: (u32, u32),
}

impl Vorschlag {
    /// „6,6667 Stück je m² (aus 599×249, Fuge 1 mm)“
    pub fn text(&self, a: &Artikel) -> String {
        format!("{} {}", zahl(self.conv, 0), self.wofuer(a.einheit))
    }

    /// „Stück je m² (aus 1000×625, ohne Fuge)“, „… Fuge 10/12 mm)“.
    pub fn wofuer(&self, e: Einheit) -> String {
        let fuge = match self.fuge {
            (0, 0) => "ohne Fuge".to_string(),
            (s, l) if s == l => format!("Fuge {s} mm"),
            (s, l) => format!("Fuge {s}/{l} mm"),
        };
        format!(
            "Stück je {} (aus {}×{}, {fuge})",
            e.zeichen(),
            self.l,
            self.h
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
/// 1.000.000 ÷ ((L + Stoßfuge) × (H + Lagerfuge)), nur bei einem Artikel in
/// m².
pub fn vorschlag(a: &Artikel) -> Option<Vorschlag> {
    if a.conv.is_some() || a.einheit != Einheit::M2 {
        return None;
    }
    let format = a.satz.text("format")?;
    let (l, h) = l_mal_h(format)?;
    let fuge = fuge(a, format);
    let n = ((l + fuge.0) * (h + fuge.1)) as i128;
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

/// Stoß- und Lagerfuge in mm (Kosten KA-3a7 A4, verwaltung.md §10a):
/// Plansteine und Planbauplatten liegen in Dünnbettmörtel (1 × 1), übrige
/// Mauersteine in Normalmörtel nach Maßordnung (10 × 12), Dämm- und
/// Bauplatten werden gestoßen (0).
fn fuge(a: &Artikel, format: &str) -> (u32, u32) {
    let texte = [a.name.as_str(), format, a.satz.text("grade").unwrap_or("")];
    let hat = |w: &[&str]| {
        texte.iter().any(|t| {
            let t = t.to_lowercase();
            w.iter().any(|w| t.contains(w))
        })
    };
    if hat(&["plan", "dünnbett"]) {
        (1, 1)
    } else if a.kategorie == Some(MatCategory::Masonry)
        || hat(&["klinker", "ziegel", "kalksand", "mauerstein", "vormauer"])
    {
        (10, 12)
    } else {
        (0, 0)
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
        assert_eq!((v.conv, v.fuge), (Dez(6_666_700), (1, 1)));
        assert_eq!(v.text(&a), "6,6667 Stück je m² (aus 599×249, Fuge 1 mm)");
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
        assert_eq!((v.l, v.h, v.fuge), (240, 113, (10, 12)));
        // 1.000.000 ÷ (250 × 125) = 32
        assert_eq!(v.conv, Dez::ganz(32));
        // Kosten KA-3a7 A4: Klinker NF in Normalmörtel 10 × 12 mm, die
        // Dämmplatte gestoßen ohne Fuge
        a.name = "Vormauerziegel".into();
        a.satz
            .setzen("format", Some(crate::satz::Wert::Text("240×71".into())));
        let v = vorschlag(&a).unwrap();
        // 1.000.000 ÷ (250 × 83) = 48,1927…
        assert_eq!(v.conv, Dez(48_192_800));
        assert_eq!(
            v.text(&a),
            "48,1928 Stück je m² (aus 240×71, Fuge 10/12 mm)"
        );
        let mut kd = artikel(&k, "Kerndämmplatte").clone();
        assert_eq!(kd.kategorie, Some(MatCategory::Insulation));
        kd.conv = None;
        kd.satz
            .setzen("format", Some(crate::satz::Wert::Text("1000×625".into())));
        let v = vorschlag(&kd).unwrap();
        assert_eq!((v.conv, v.fuge), (Dez(1_600_000), (0, 0)));
        assert_eq!(v.text(&kd), "1,6 Stück je m² (aus 1000×625, ohne Fuge)");
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

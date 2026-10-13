//! Punktlisten des Vermessers lesen (G1). Skizzeo liest den Inhalt, nicht
//! die Endung: Zeichensatz, Trennzeichen, Dezimalzeichen, Kopfzeile und die
//! Rolle jeder Spalte erkennt es selbst (plan-gelaendemodul.md §5 G1). Nur
//! Bytes rein, Punkte und Hinweise raus; was nicht passt, wird ein Satz und
//! nie ein Abbruch. Eine DXF liest [`crate::dxf`] (G1b).

use crate::codes::Codetabelle;
use crate::dxf::{self, Linie, Linienart};

/// Ein gemessener Punkt in Landeskoordinaten (m), Rechtswert ohne
/// Zonenkennzahl.
#[derive(Clone, Debug, PartialEq)]
pub struct Punkt {
    pub nr: String,
    /// Rechtswert (Ost).
    pub e: f64,
    /// Hochwert (Nord).
    pub n: f64,
    /// Höhe.
    pub h: f64,
    pub code: String,
    pub text: String,
    /// Zeile in der Datei, ab 1.
    pub zeile: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zeichensatz {
    Utf8,
    Windows1252,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trenner {
    Semikolon,
    Tab,
    Komma,
    /// Eine Folge von Leerzeichen oder Tabs.
    Leerzeichen,
}

/// Rolle einer Spalte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spalte {
    Nr,
    Rechts,
    Hoch,
    Hoehe,
    Code,
    Text,
    /// Nicht lesen.
    Aus,
}

/// Lagebezug, an den Zahlen erkannt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lage {
    /// ETRS89/UTM; `zone` nur, wenn die Zonenkennzahl vor dem Rechtswert stand.
    Utm {
        zone: Option<u8>,
    },
    /// Gauß-Krüger im Streifen 2 bis 5 (wird als örtliches System gelesen).
    GaussKrueger {
        streifen: u8,
    },
    Oertlich,
}

/// Was die Datei ist, wenn sie keine Punktliste ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Punktliste,
    Dxf,
    LandXml,
    Gsi,
    Dwg,
}

/// Was Skizzeo erkannt hat; jede Angabe lässt sich mit [`Vorgaben`]
/// überstimmen.
#[derive(Clone, Debug, PartialEq)]
pub struct Erkannt {
    pub zeichensatz: Zeichensatz,
    pub trenner: Trenner,
    /// Dezimalkomma statt Dezimalpunkt.
    pub komma: bool,
    /// Zeile der Kopfzeile, ab 1.
    pub kopf: Option<usize>,
    /// Rolle je Spalte.
    pub spalten: Vec<Spalte>,
    pub lage: Lage,
    /// Rechts und Hoch sind nicht sicher (örtliches System, Gauß-Krüger im
    /// Streifen 5): die Vorschau markiert sie, „Rechts und Hoch tauschen“.
    pub zweifel: bool,
    /// Die Lagewerte sehen nach Millimetern aus.
    pub millimeter: bool,
}

/// Einstellungen der Datei, nur für den, der der Erkennung widerspricht.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vorgaben {
    pub zeichensatz: Option<Zeichensatz>,
    pub trenner: Option<Trenner>,
    pub komma: Option<bool>,
    /// Zeilen am Anfang, die nicht gelesen werden.
    pub ueberspringen: usize,
    pub spalten: Option<Vec<Spalte>>,
    /// Rechts und Hoch tauschen.
    pub tauschen: bool,
    /// Meter je Einheit der Datei (1, 0,01, 0,001); `None` = Meter, bei
    /// einer DXF die Einheit aus ihrem Kopf.
    pub einheit: Option<f64>,
    /// Layer einer DXF, die nicht gelesen werden.
    pub ebenen_aus: Vec<String>,
}

/// Ergebnis des Lesens.
#[derive(Clone, Debug, PartialEq)]
pub struct Aufmass {
    pub punkte: Vec<Punkt>,
    /// Linien der Datei (nur DXF): Bruchkanten, Netzflächen, Grenzen.
    pub linien: Vec<Linie>,
    pub erkannt: Erkannt,
    pub hinweise: Vec<String>,
    /// Was nur eine DXF hat; `None` bei einer Punktliste.
    pub dxf: Option<dxf::Datei>,
}

impl Aufmass {
    /// Ein Satz für den Kopf des Dialogs, z. B. „109 Punkte · Semikolon ·
    /// Dezimalkomma · ETRS89/UTM Zone 32 (Rechtswert mit Zonenkennzahl) ·
    /// Höhen 24,15 bis 26,04 m“.
    pub fn satz(&self) -> String {
        let e = &self.erkannt;
        let anzahl = |n: usize, eins: &str, viele: &str| match n {
            1 => format!("1 {eins}"),
            n => format!("{n} {viele}"),
        };
        let mut s = vec![anzahl(self.punkte.len(), "Punkt", "Punkte")];
        if let Some(d) = &self.dxf {
            s.push(format!("DXF {}", d.version_name()).trim_end().to_string());
            if let Some(t) = &d.hoehe {
                s.push(format!("Höhe aus Attribut {t}"));
            }
            let art = |a: Linienart| self.linien.iter().filter(|l| l.art == a).count();
            for (n, eins, viele) in [
                (art(Linienart::Bruchkante), "Bruchkante", "Bruchkanten"),
                (art(Linienart::Masche), "Netzfläche", "Netzflächen"),
                (d.hoehenlinien, "Höhenlinie", "Höhenlinien"),
                (art(Linienart::Grenze), "Grenze", "Grenzen"),
            ] {
                if n > 0 {
                    s.push(anzahl(n, eins, viele));
                }
            }
        } else {
            s.push(
                match e.trenner {
                    Trenner::Semikolon => "Semikolon",
                    Trenner::Tab => "Tabulator",
                    Trenner::Komma => "Komma",
                    Trenner::Leerzeichen => "Leerzeichen",
                }
                .into(),
            );
            s.push(
                if e.komma {
                    "Dezimalkomma"
                } else {
                    "Dezimalpunkt"
                }
                .into(),
            );
            if e.zeichensatz == Zeichensatz::Windows1252 {
                s.push("Windows-1252".into());
            }
        }
        s.push(match e.lage {
            Lage::Utm { zone: Some(z) } => {
                format!("ETRS89/UTM Zone {z} (Rechtswert mit Zonenkennzahl)")
            }
            Lage::Utm { zone: None } => "ETRS89/UTM".into(),
            Lage::GaussKrueger { streifen } => format!("Gauß-Krüger Streifen {streifen}"),
            Lage::Oertlich => "örtliches System".into(),
        });
        if let Some((lo, hi)) = self.hoehen() {
            s.push(format!("Höhen {} bis {} m", dez(lo, 2), dez(hi, 2)));
        }
        s.join(" · ")
    }

    /// Kleinste und größte Höhe.
    pub fn hoehen(&self) -> Option<(f64, f64)> {
        let mut it = self.punkte.iter().map(|p| p.h);
        let h0 = it.next()?;
        Some(it.fold((h0, h0), |(lo, hi), h| (lo.min(h), hi.max(h))))
    }

    /// Bruchkanten aus den Linien-Codes und aus den Linien der Datei
    /// (Bruchkanten und Kanten der Netzflächen), jede Kante einmal, nur
    /// zwischen Punkten mit `drin`. Indizes in `punkte`.
    pub fn bruchkanten(&self, codes: &Codetabelle, drin: &[bool]) -> Vec<[usize; 2]> {
        let mut k = codes.bruchkanten(&self.punkte, drin);
        let mut da: std::collections::HashSet<[usize; 2]> =
            k.iter().map(|&[a, b]| [a.min(b), a.max(b)]).collect();
        let ok = |i: usize| i < self.punkte.len() && drin.get(i).copied().unwrap_or(true);
        for l in self
            .linien
            .iter()
            .filter(|l| matches!(l.art, Linienart::Bruchkante | Linienart::Masche))
        {
            let n = l.punkte.len();
            let schluss = (l.geschlossen && n > 2).then(|| [l.punkte[n - 1], l.punkte[0]]);
            for [a, b] in l.punkte.windows(2).map(|w| [w[0], w[1]]).chain(schluss) {
                if a != b && ok(a) && ok(b) && da.insert([a.min(b), a.max(b)]) {
                    k.push([a, b]);
                }
            }
        }
        k
    }
}

/// Zahl mit Dezimalkomma und `n` Stellen.
pub fn dez(v: f64, n: usize) -> String {
    // kaufmännisch gerundet: 24,145 → 24,15, auch wenn die Binärzahl knapp darunter liegt
    let f = 10f64.powi(n as i32);
    let r = (v * f + v.signum() * 1e-7).round() / f;
    format!("{r:.n$}").replace('.', ",")
}

/// Satz, wenn Skizzeo die Datei nicht liest, sonst `None` (Punktliste
/// und DXF).
pub fn fremd(f: Format) -> Option<&'static str> {
    match f {
        Format::Punktliste => None,
        Format::Dwg => Some("DWG ist ein geschlossenes Format. Bitte beim Vermesser die Punktliste oder eine DXF anfordern."),
        Format::Dxf => None,
        Format::LandXml => Some("Das ist eine LandXML-Datei. LandXML liest Skizzeo mit der Stufe G6; bis dahin bitte die Punktliste des Vermessers einlesen."),
        Format::Gsi => Some("Das ist eine Leica-GSI-Datei. GSI liest Skizzeo mit der Stufe G6; bis dahin bitte die Punktliste des Vermessers einlesen."),
    }
}

/// Bytes in Text: UTF-8 mit oder ohne BOM, sonst Windows-1252.
pub fn text(roh: &[u8], z: Option<Zeichensatz>) -> (String, Zeichensatz) {
    let ohne_bom = roh.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(roh);
    if z != Some(Zeichensatz::Windows1252) {
        if let Ok(s) = std::str::from_utf8(ohne_bom) {
            return (s.to_string(), Zeichensatz::Utf8);
        }
        if z == Some(Zeichensatz::Utf8) {
            return (
                String::from_utf8_lossy(ohne_bom).into_owned(),
                Zeichensatz::Utf8,
            );
        }
    }
    // 0x80 bis 0x9F nach Windows-1252, der Rest wie Latin-1
    const C1: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    let s = roh
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => C1[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect();
    (s, Zeichensatz::Windows1252)
}

/// Zeilen, getrennt an \r\n, \n und einzelnem \r (alte Mac-Dateien).
pub(crate) fn zeilen(s: &str) -> impl Iterator<Item = &str> {
    let mut rest = Some(s);
    std::iter::from_fn(move || {
        let r = rest.take()?;
        match r.find(['\n', '\r']) {
            Some(i) => {
                let n = if r[i..].starts_with("\r\n") { 2 } else { 1 };
                rest = Some(&r[i + n..]);
                Some(&r[..i])
            }
            None => (!r.is_empty()).then_some(r),
        }
    })
}

/// Format am Inhalt erkennen.
pub fn format(roh: &[u8]) -> Format {
    // UTF-8-BOM vor „0 / SECTION“ (Review #150 B4)
    let roh = roh.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(roh);
    if roh.starts_with(b"AC10") {
        return Format::Dwg;
    }
    if roh.starts_with(b"AutoCAD Binary DXF") {
        return Format::Dxf;
    }
    let kopf = String::from_utf8_lossy(&roh[..roh.len().min(4096)]);
    if kopf.contains("<LandXML") {
        return Format::LandXml;
    }
    let mut zeilen = zeilen(&kopf).map(str::trim).filter(|l| !l.is_empty());
    let erste: Vec<&str> = zeilen.by_ref().take(8).collect();
    if erste.windows(2).any(|w| w[0] == "0" && w[1] == "SECTION") {
        return Format::Dxf;
    }
    if let Some(l) = erste.first() {
        let l = l.trim_start_matches('*').as_bytes();
        if l.len() >= 7 && l.starts_with(b"11") && matches!(l[6], b'+' | b'-') {
            return Format::Gsi;
        }
    }
    Format::Punktliste
}

/// Zahl lesen; nur Ziffern, Vorzeichen, Dezimalzeichen und Exponent.
pub(crate) fn zahl(s: &str, komma: bool) -> Option<f64> {
    let s = s.trim().trim_matches('"').trim();
    if s.is_empty()
        || !s.bytes().any(|b| b.is_ascii_digit())
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b',' | b'e' | b'E'))
    {
        return None;
    }
    let v = if komma {
        if s.contains('.') && s.contains(',') {
            return None;
        }
        s.replace(',', ".").parse::<f64>().ok()?
    } else {
        if s.contains(',') {
            return None;
        }
        s.parse::<f64>().ok()?
    };
    v.is_finite().then_some(v)
}

fn felder(l: &str, t: Trenner) -> Vec<&str> {
    let f: Vec<&str> = match t {
        Trenner::Semikolon => l.split(';').collect(),
        Trenner::Tab => l.split('\t').collect(),
        Trenner::Komma => l.split(',').collect(),
        Trenner::Leerzeichen => l.split_whitespace().collect(),
    };
    f.into_iter()
        .map(|s| s.trim().trim_matches('"').trim())
        .collect()
}

fn kommentar(l: &str) -> bool {
    let l = l.trim();
    l.is_empty() || l.starts_with('#') || l.starts_with("//") || l.starts_with('!')
}

/// Wie viele Felder eine Zahl sind (bei Dezimalkomma `komma`).
fn zahlen(f: &[&str], komma: bool) -> usize {
    f.iter().filter(|s| zahl(s, komma).is_some()).count()
}

/// Rolle nach einem Wort der Kopfzeile. „X“ und „Y“ ordnen nichts zu.
fn rolle(w: &str) -> Option<Spalte> {
    let w: String = w
        .to_lowercase()
        .replace("ö", "oe")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    Some(match w.as_str() {
        "pnr" | "nr" | "punkt" | "punktnummer" | "punktnr" | "pktnr" | "pkt" | "name" | "id" => {
            Spalte::Nr
        }
        "rechtswert" | "ostwert" | "rechts" | "e" | "east" | "easting" | "ost" | "rw" => {
            Spalte::Rechts
        }
        "hochwert" | "nordwert" | "hoch" | "n" | "north" | "northing" | "nord" | "hw" => {
            Spalte::Hoch
        }
        "hoehe" | "h" | "z" | "hohe" | "height" | "elevation" | "nhn" => Spalte::Hoehe,
        "code" | "kennung" | "sy" | "cd" | "pcode" | "punktcode" => Spalte::Code,
        "bemerkung" | "beschreibung" | "text" | "kommentar" | "info" => Spalte::Text,
        _ => return None,
    })
}

/// Art einer Koordinatenspalte nach ihrem Wertebereich.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Art {
    /// UTM-Rechtswert mit Zonenkennzahl.
    UtmZone(u8),
    Utm,
    /// Hochwert (UTM und Gauß-Krüger); im Streifen 5 auch Gauß-Krüger-Rechtswert.
    Hoch,
    GaussKrueger(u8),
    Hoehe,
    Klein,
}

pub(crate) fn art(lo: f64, hi: f64) -> Art {
    let in_ = |a: f64, b: f64| lo >= a && hi <= b;
    for z in 31..=33u8 {
        let z0 = z as f64 * 1e6;
        if in_(z0 + 100_000.0, z0 + 900_000.0) {
            return Art::UtmZone(z);
        }
    }
    if in_(5_200_000.0, 6_100_000.0) {
        return Art::Hoch;
    }
    for s in 2..=5u8 {
        let s0 = s as f64 * 1e6;
        if in_(s0, s0 + 999_999.999) {
            return Art::GaussKrueger(s);
        }
    }
    if in_(100_000.0, 900_000.0) {
        return Art::Utm;
    }
    if in_(-50.0, 3000.0) {
        return Art::Hoehe;
    }
    Art::Klein
}

/// Punktliste lesen, alles erkannt.
pub fn lesen(roh: &[u8]) -> Result<Aufmass, String> {
    lesen_mit(roh, &Vorgaben::default())
}

/// Punktliste lesen; was in `v` steht, gilt statt der Erkennung.
pub fn lesen_mit(roh: &[u8], v: &Vorgaben) -> Result<Aufmass, String> {
    match format(roh) {
        Format::Dxf => return dxf::lesen(roh, v),
        f => {
            if let Some(s) = fremd(f) {
                return Err(s.into());
            }
        }
    }
    let (inhalt, zeichensatz) = text(roh, v.zeichensatz);
    let zeilen: Vec<(usize, &str)> = zeilen(&inhalt)
        .enumerate()
        .skip(v.ueberspringen)
        .map(|(i, l)| (i + 1, l))
        .collect();
    let inhalt_zeilen = || zeilen.iter().filter(|(_, l)| !kommentar(l));
    // Trennzeichen: die Wahl mit den meisten Zeilen aus drei oder mehr Zahlen
    let wahl = |t: Trenner| -> usize {
        inhalt_zeilen()
            .filter(|(_, l)| {
                let f = felder(l, t);
                zahlen(&f, false) >= 3 || (t != Trenner::Komma && zahlen(&f, true) >= 3)
            })
            .count()
    };
    let trenner = v.trenner.unwrap_or_else(|| {
        [
            Trenner::Semikolon,
            Trenner::Tab,
            Trenner::Komma,
            Trenner::Leerzeichen,
        ]
        .into_iter()
        .fold((Trenner::Leerzeichen, 0), |(bt, bn), t| match wahl(t) {
            n if n > bn => (t, n),
            _ => (bt, bn),
        })
        .0
    });
    // Dezimalkomma, wenn Zahlen mit Komma vorkommen und das Komma nicht trennt
    let komma = v.komma.unwrap_or_else(|| {
        trenner != Trenner::Komma
            && inhalt_zeilen().any(|(_, l)| {
                felder(l, trenner)
                    .iter()
                    .any(|s| s.contains(',') && zahl(s, true).is_some())
            })
    });
    let daten: Vec<(usize, Vec<&str>)> = inhalt_zeilen()
        .map(|(i, l)| (*i, felder(l, trenner)))
        .filter(|(_, f)| zahlen(f, komma) >= 3)
        .collect();
    let Some(erste) = daten.first().map(|d| d.0) else {
        return Err("In der Datei steht keine Zeile mit Rechtswert, Hochwert und Höhe.".into());
    };
    // Spaltenzahl: die häufigste
    let breite = {
        let mut n: Vec<(usize, usize)> = Vec::new();
        for (_, f) in &daten {
            match n.iter_mut().find(|x| x.0 == f.len()) {
                Some(x) => x.1 += 1,
                None => n.push((f.len(), 1)),
            }
        }
        n.iter()
            .max_by_key(|x| (x.1, std::cmp::Reverse(x.0)))
            .map_or(0, |x| x.0)
    };
    let mut hinweise = Vec::new();
    // Kopf: letzte Zeile mit Wörtern vor den Zahlenzeilen; eine
    // Kommentarzeile nur, wenn sie mindestens zwei bekannte Wörter hat
    let mut kopf = None;
    let mut ellipsoid = false;
    for (i, l) in zeilen.iter().filter(|(i, _)| *i < erste) {
        ellipsoid |= l.to_lowercase().contains("ellipsoid");
        let roh_l = l.trim().trim_start_matches(['#', '!', '/']).trim();
        let f = felder(roh_l, trenner);
        let woerter = f.iter().filter(|w| rolle(w).is_some()).count();
        let wort = f.iter().any(|w| w.chars().any(char::is_alphabetic));
        if (kommentar(l) && woerter >= 2) || (!kommentar(l) && wort && zahlen(&f, komma) < 3) {
            kopf = Some((*i, f));
        }
    }
    let mut spalten = vec![None::<Spalte>; breite];
    if let Some((_, f)) = &kopf {
        for (k, w) in f.iter().enumerate().take(breite) {
            spalten[k] = rolle(w);
        }
    }
    let mut zweifel = false;
    let spalten: Vec<Spalte> = match &v.spalten {
        Some(s) => {
            let mut s = s.clone();
            s.resize(breite, Spalte::Aus);
            s
        }
        None => {
            // je Spalte: Zahlenanteil unter den Zeilen, die sie haben
            let num = |k: usize| {
                let da = daten.iter().filter(|(_, f)| f.len() > k).count();
                let n = daten
                    .iter()
                    .filter(|(_, f)| f.get(k).is_some_and(|s| zahl(s, komma).is_some()))
                    .count();
                da * 2 >= daten.len() && n * 20 >= da * 19
            };
            let ganz = |k: usize| {
                daten
                    .iter()
                    .all(|(_, f)| f.get(k).is_none_or(|s| !s.contains(['.', ','])))
            };
            let schon = |s: &[Option<Spalte>], r: Spalte| s.contains(&Some(r));
            // Punktnummer: erste Spalte, wenn Text oder ganze Zahl vor drei Zahlen
            let zahl_spalten = (0..breite).filter(|&k| num(k)).count();
            if breite > 0
                && spalten[0].is_none()
                && !schon(&spalten, Spalte::Nr)
                && (!num(0) || (ganz(0) && zahl_spalten >= 4))
            {
                spalten[0] = Some(Spalte::Nr);
            }
            // Koordinaten: die ersten freien Zahlenspalten, nach Wertebereich
            let fehlen = [Spalte::Rechts, Spalte::Hoch, Spalte::Hoehe]
                .iter()
                .filter(|r| !schon(&spalten, **r))
                .count();
            let freie: Vec<usize> = (0..breite)
                .filter(|&k| spalten[k].is_none() && num(k))
                .take(fehlen)
                .collect();
            let arten: Vec<(usize, Art)> = freie
                .iter()
                .map(|&k| {
                    (
                        k,
                        bereich(&daten, k, komma).map_or(Art::Klein, |(lo, hi)| art(lo, hi)),
                    )
                })
                .collect();
            let setze = |s: &mut Vec<Option<Spalte>>, r: Spalte, k: usize| {
                if s[k].is_none() && !s.contains(&Some(r)) {
                    s[k] = Some(r);
                }
            };
            let rechts = arten
                .iter()
                .find(|(_, a)| matches!(a, Art::UtmZone(_) | Art::Utm | Art::GaussKrueger(_)));
            let hoch: Vec<usize> = arten
                .iter()
                .filter(|(_, a)| *a == Art::Hoch)
                .map(|x| x.0)
                .collect();
            match (rechts, hoch.as_slice()) {
                (Some(&(k, _)), [h, ..]) => {
                    setze(&mut spalten, Spalte::Rechts, k);
                    setze(&mut spalten, Spalte::Hoch, *h);
                }
                // Gauß-Krüger im Streifen 5: Rechts und Hoch beide 5.xxx.xxx
                (None, [a, b, ..]) => {
                    setze(&mut spalten, Spalte::Rechts, *a);
                    setze(&mut spalten, Spalte::Hoch, *b);
                    zweifel = true;
                }
                _ => zweifel = !(schon(&spalten, Spalte::Rechts) && schon(&spalten, Spalte::Hoch)),
            }
            for r in [Spalte::Rechts, Spalte::Hoch, Spalte::Hoehe] {
                if let Some(k) = freie.iter().copied().find(|&k| spalten[k].is_none()) {
                    setze(&mut spalten, r, k);
                }
            }
            // Code: erste freie Spalte mit Text oder ganzen Zahlen; Rest Text
            for k in 0..breite {
                if spalten[k].is_none() && (!num(k) || ganz(k)) && !schon(&spalten, Spalte::Code) {
                    spalten[k] = Some(Spalte::Code);
                } else if spalten[k].is_none() && !num(k) {
                    spalten[k] = Some(Spalte::Text);
                }
            }
            spalten
                .into_iter()
                .map(|s| s.unwrap_or(Spalte::Aus))
                .collect()
        }
    };
    let spalten: Vec<Spalte> = if v.tauschen {
        spalten
            .into_iter()
            .map(|s| match s {
                Spalte::Rechts => Spalte::Hoch,
                Spalte::Hoch => Spalte::Rechts,
                s => s,
            })
            .collect()
    } else {
        spalten
    };
    let pos = |r: Spalte| spalten.iter().position(|s| *s == r);
    let (Some(ke), Some(kn), Some(kh)) =
        (pos(Spalte::Rechts), pos(Spalte::Hoch), pos(Spalte::Hoehe))
    else {
        return Err("Rechtswert, Hochwert und Höhe sind nicht alle zugeordnet. Bitte die Spalten in der Vorschau wählen.".into());
    };
    // Lagebezug aus Rechts und Hoch
    let art_von = |k: usize| bereich(&daten, k, komma).map_or(Art::Klein, |(lo, hi)| art(lo, hi));
    let lage = match (art_von(ke), art_von(kn)) {
        (Art::UtmZone(z), Art::Hoch) => Lage::Utm { zone: Some(z) },
        (Art::Utm, Art::Hoch) => Lage::Utm { zone: None },
        (Art::GaussKrueger(s), Art::Hoch) => Lage::GaussKrueger { streifen: s },
        (Art::Hoch, Art::Hoch) => Lage::GaussKrueger { streifen: 5 },
        _ => Lage::Oertlich,
    };
    // örtlich: Millimeter, wenn Lage und Höhe über 10.000 liegen
    let gross = |k: usize| {
        bereich(&daten, k, komma).is_some_and(|(lo, hi)| lo.abs().max(hi.abs()) > 10_000.0)
    };
    let millimeter = lage == Lage::Oertlich && gross(kh) && (gross(ke) || gross(kn));
    let faktor = v.einheit.unwrap_or(1.0);
    let zone_m = match lage {
        Lage::Utm { zone: Some(z) } => z as f64 * 1e6,
        _ => 0.0,
    };
    let mut punkte = Vec::with_capacity(daten.len());
    let mut ohne = Vec::new();
    let mut voll = Vec::new();
    let datenzeilen: std::collections::HashSet<usize> = daten.iter().map(|d| d.0).collect();
    for (i, l) in inhalt_zeilen().filter(|(i, _)| *i >= erste) {
        if !datenzeilen.contains(i) {
            if kopf.as_ref().is_none_or(|k| k.0 != *i) {
                ohne.push(*i);
            }
            continue;
        }
        let f = felder(l, trenner);
        if breite >= 3 && f.len() >= 2 * breite {
            voll.push(*i);
        }
        let wert = |k: usize| f.get(k).and_then(|s| zahl(s, komma));
        let (Some(e), Some(n), Some(h)) = (wert(ke), wert(kn), wert(kh)) else {
            ohne.push(*i);
            continue;
        };
        let mut nr = String::new();
        let mut code = String::new();
        let mut txt: Vec<&str> = Vec::new();
        for (k, s) in f.iter().enumerate() {
            match spalten.get(k).copied().unwrap_or(Spalte::Text) {
                Spalte::Nr => nr = s.to_string(),
                Spalte::Code => code = s.to_string(),
                Spalte::Text if !s.is_empty() => txt.push(s),
                _ => {}
            }
        }
        if nr.is_empty() {
            nr = (punkte.len() + 1).to_string();
        }
        punkte.push(Punkt {
            nr,
            e: (e - zone_m) * faktor,
            n: n * faktor,
            h: h * faktor,
            code,
            text: txt.join(" "),
            zeile: *i,
        });
    }
    if !ohne.is_empty() {
        hinweise.push(liste(
            "ohne Rechtswert, Hochwert oder Höhe, bleibt außen vor",
            "Zeile",
            "Zeilen",
            &ohne,
        ));
    }
    if !voll.is_empty() {
        hinweise.push(liste(
            "mit doppelt so vielen Spalten wie die übrigen, vielleicht fehlt ein Zeilenumbruch",
            "Zeile",
            "Zeilen",
            &voll,
        ));
    }
    if ellipsoid {
        hinweise.push(
            "Die Höhen sehen ellipsoidisch aus. Bitte beim Vermesser NHN-Höhen anfordern.".into(),
        );
    }
    if let Lage::GaussKrueger { .. } = lage {
        hinweise.push(GAUSS_KRUEGER.into());
    }
    if millimeter && v.einheit.is_none() {
        hinweise.push(MILLIMETER.into());
    }
    Ok(Aufmass {
        punkte,
        linien: Vec::new(),
        erkannt: Erkannt {
            zeichensatz,
            trenner,
            komma,
            kopf: kopf.map(|k| k.0),
            spalten,
            lage,
            zweifel,
            millimeter,
        },
        hinweise,
        dxf: None,
    })
}

/// Hinweis zu Werten in Millimetern ohne Angabe (Punktliste und DXF).
pub(crate) const MILLIMETER: &str = "Die Werte sehen nach Millimetern aus. Die Einheit lässt sich unter „Einstellungen der Datei“ umstellen.";

/// Hinweis zu Gauß-Krüger (Punktliste und DXF).
pub(crate) const GAUSS_KRUEGER: &str = "Gauß-Krüger liest Skizzeo als örtliches System: Auf dem Grundstück stimmt alles, nur die Verbindung zu UTM und zum IFC-Export fehlt, bis der Nullpunkt in UTM eingetragen ist.";

/// Kleinster und größter Wert einer Spalte.
fn bereich(daten: &[(usize, Vec<&str>)], k: usize, komma: bool) -> Option<(f64, f64)> {
    let mut it = daten
        .iter()
        .filter_map(|(_, f)| f.get(k).and_then(|s| zahl(s, komma)));
    let v0 = it.next()?;
    Some(it.fold((v0, v0), |(lo, hi), v| (lo.min(v), hi.max(v))))
}

/// „Zeile 7 ohne …“ bzw. „Zeilen 7, 9, 12 und 3 weitere ohne …“.
pub(crate) fn liste(was: &str, eins: &str, viele: &str, nr: &[impl std::fmt::Display]) -> String {
    const ZEIGEN: usize = 8;
    let erst: Vec<String> = nr.iter().take(ZEIGEN).map(|n| n.to_string()).collect();
    match nr.len() {
        1 => format!("{eins} {} {was}.", erst[0]),
        n if n <= ZEIGEN => format!(
            "{viele} {} und {} {was}.",
            erst[..n - 1].join(", "),
            erst[n - 1]
        ),
        n => format!(
            "{viele} {} und {} weitere {was}.",
            erst.join(", "),
            n - ZEIGEN
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lies(s: &str) -> Aufmass {
        lesen(s.as_bytes()).unwrap()
    }

    #[test]
    fn semikolon_dezimalkomma_kopf_zone() {
        let a = lies("PNr;Rechtswert;Hochwert;Höhe;Code;Bemerkung\r\n1;32467108,214;5878741,637;24,337;GR;Grenzstein\r\n2;32467132,035;5878744,562;24,624;GR;\r\n");
        let e = &a.erkannt;
        assert_eq!(
            (e.trenner, e.komma, e.kopf),
            (Trenner::Semikolon, true, Some(1))
        );
        assert_eq!(e.lage, Lage::Utm { zone: Some(32) });
        assert_eq!(
            e.spalten,
            [
                Spalte::Nr,
                Spalte::Rechts,
                Spalte::Hoch,
                Spalte::Hoehe,
                Spalte::Code,
                Spalte::Text
            ]
        );
        assert_eq!(a.punkte.len(), 2);
        let p = &a.punkte[0];
        assert_eq!(
            (p.nr.as_str(), p.code.as_str(), p.text.as_str()),
            ("1", "GR", "Grenzstein")
        );
        assert!(
            (p.e - 467108.214).abs() < 1e-6
                && (p.n - 5878741.637).abs() < 1e-6
                && (p.h - 24.337).abs() < 1e-9
        );
        assert_eq!(
            a.satz(),
            "2 Punkte · Semikolon · Dezimalkomma · ETRS89/UTM Zone 32 (Rechtswert mit Zonenkennzahl) · Höhen 24,34 bis 24,62 m"
        );
    }

    #[test]
    fn hoch_zuerst_und_windows_1252() {
        let mut roh = b"# H\xF6hen DHHN2016\r\n# PktNr X Y H Code\r\n".to_vec();
        roh.extend_from_slice(b"1        5878741.637   467108.214   24.337  GR\r\n101      5878741.249   467105.463   24.302  GW\r\n");
        let a = lesen(&roh).unwrap();
        let e = &a.erkannt;
        assert_eq!(e.zeichensatz, Zeichensatz::Windows1252);
        assert_eq!(e.trenner, Trenner::Leerzeichen);
        assert_eq!(
            e.spalten,
            [
                Spalte::Nr,
                Spalte::Hoch,
                Spalte::Rechts,
                Spalte::Hoehe,
                Spalte::Code
            ]
        );
        assert!(!e.zweifel);
        assert_eq!(a.punkte[1].nr, "101");
        assert!((a.punkte[1].e - 467105.463).abs() < 1e-6);
        assert_eq!(text(b"H\xF6he \x80", None).0, "Höhe €");
    }

    #[test]
    fn oertlich_ohne_kopf_mit_zweifel_und_tausch() {
        let s = "P1 10.5 20.25 101.3\nP2 12.0 21.0 101.4\nP3 14.0 19.0 101.2\n";
        let a = lies(s);
        assert_eq!(a.erkannt.lage, Lage::Oertlich);
        assert!(a.erkannt.zweifel);
        assert_eq!(
            a.erkannt.spalten,
            [Spalte::Nr, Spalte::Rechts, Spalte::Hoch, Spalte::Hoehe]
        );
        let b = lesen_mit(
            s.as_bytes(),
            &Vorgaben {
                tauschen: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!((b.punkte[0].e, b.punkte[0].n), (20.25, 10.5));
        // ohne Punktnummer: fortlaufend
        let c = lies("1,5 2,5 3,5\n4,5 5,5 6,5\n");
        assert!(c.erkannt.komma);
        assert_eq!(c.punkte[1].nr, "2");
        assert_eq!(
            (c.punkte[1].e, c.punkte[1].n, c.punkte[1].h),
            (4.5, 5.5, 6.5)
        );
    }

    #[test]
    fn komma_als_trenner_und_zahlencode() {
        let a = lies("1,467108.214,5878741.637,24.337,10\n2,467132.035,5878744.562,24.624,10\n");
        assert_eq!(a.erkannt.trenner, Trenner::Komma);
        assert!(!a.erkannt.komma);
        assert_eq!(a.erkannt.lage, Lage::Utm { zone: None });
        assert_eq!(a.punkte[0].code, "10");
    }

    #[test]
    fn zeilenenden_cr_lf_und_crlf() {
        let lf = "1 100.0 200.0 30.0\n2 101.0 201.0 30.2\n\n3 102.0 199.0 30.5\n";
        let a = lies(lf);
        for s in [lf.replace('\n', "\r"), lf.replace('\n', "\r\n")] {
            let b = lies(&s);
            assert_eq!(b.punkte, a.punkte, "{s:?}");
            assert!(b.hinweise.is_empty());
        }
        assert_eq!(a.punkte[2].zeile, 4);
        assert_eq!(
            zeilen("a\r\rb\r\nc").collect::<Vec<_>>(),
            ["a", "", "b", "c"]
        );
        // fehlender Umbruch: doppelt so viele Spalten
        let c =
            lies("1 100.0 200.0 30.0\n2 101.0 201.0 30.2 3 102.0 199.0 30.5\n4 103.0 198.0 31.0\n");
        assert_eq!(c.punkte.len(), 3);
        assert_eq!(
            c.hinweise,
            ["Zeile 2 mit doppelt so vielen Spalten wie die übrigen, vielleicht fehlt ein Zeilenumbruch."]
        );
    }

    #[test]
    fn hinweise_und_fremde_formate() {
        let a = lies(
            "1 100.0 200.0 30.0\n2 101.0 201.0\n3 102.0 199.0 30.5 GP\nx\n4 103.0 198.0 31.0\n",
        );
        assert_eq!(a.punkte.len(), 3);
        assert_eq!(
            a.hinweise,
            ["Zeilen 2 und 4 ohne Rechtswert, Hochwert oder Höhe, bleibt außen vor."]
        );
        assert_eq!(format(b"AC1032\0\0"), Format::Dwg);
        assert!(lesen(b"AC1032")
            .unwrap_err()
            .starts_with("DWG ist ein geschlossenes Format"));
        assert_eq!(format(b"  0\r\nSECTION\r\n  2\r\nHEADER\r\n"), Format::Dxf);
        assert_eq!(
            format(b"<?xml version=\"1.0\"?>\n<LandXML>"),
            Format::LandXml
        );
        assert_eq!(
            format(b"*110001+0000000000000001 81..00+0000000467108214\n"),
            Format::Gsi
        );
        assert!(lesen(b"nur Text\nohne Zahlen\n").is_err());
        let g = lies("1 4567108.21 5878741.64 24.34\n2 4567110.00 5878750.00 24.50\n");
        assert_eq!(g.erkannt.lage, Lage::GaussKrueger { streifen: 4 });
        assert!(g.hinweise[0].starts_with("Gauß-Krüger liest Skizzeo"));
        let mm = lies("1 12000 15000 24500\n2 13000 16000 24600\n3 14000 15500 24550\n");
        assert!(mm.erkannt.millimeter);
        assert_eq!(
            liste("x", "Zeile", "Zeilen", &(1..=10).collect::<Vec<_>>()),
            "Zeilen 1, 2, 3, 4, 5, 6, 7, 8 und 2 weitere x."
        );
    }
}

//! Schriften im Inhaltsstrom (ISO 32000-1 §9.5 bis §9.10): Codes in
//! Zeichen und Breiten. Einfache Schriften mit WinAnsi, Standard und
//! `/Differences`, Type0 mit CMap (Codebereiche, CIDs) und `/W`; eine
//! Unicode-Tabelle (`/ToUnicode`, bfchar und bfrange) geht immer vor.

use crate::pdf::objekt::{Leser, Objekt, Verz, Wort};
use crate::pdf::Pdf;
use std::collections::BTreeMap;

/// Windows-1252 für 0x80 bis 0x9F; der Rest wie Latin-1.
const C1: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}',
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
];

/// Zeichen eines Bytes in WinAnsi.
pub fn win_ansi(b: u8) -> char {
    match b {
        0x80..=0x9F => C1[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// Zeichen zu einem Glyphennamen aus `/Differences` (die häufigen).
fn glyphe(n: &str) -> Option<char> {
    let mut c = n.chars();
    if let (Some(a), None) = (c.next(), c.next()) {
        return Some(a);
    }
    if let Some(h) = n.strip_prefix("uni").or_else(|| n.strip_prefix('u')) {
        if (4..=6).contains(&h.len()) {
            return u32::from_str_radix(&h[..h.len().min(6)], 16)
                .ok()
                .and_then(char::from_u32);
        }
    }
    const ZIFFERN: [&str; 10] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    if let Some(i) = ZIFFERN.iter().position(|z| *z == n) {
        return char::from_digit(i as u32, 10);
    }
    Some(match n {
        "space" | "nbspace" => ' ',
        "period" => '.',
        "comma" => ',',
        "colon" => ':',
        "semicolon" => ';',
        "hyphen" | "minus" => '-',
        "slash" => '/',
        "parenleft" => '(',
        "parenright" => ')',
        "equal" => '=',
        "plus" => '+',
        "percent" => '%',
        "ampersand" => '&',
        "quotesingle" | "quoteright" => '\'',
        "quotedbl" => '"',
        "underscore" => '_',
        "adieresis" => 'ä',
        "odieresis" => 'ö',
        "udieresis" => 'ü',
        "Adieresis" => 'Ä',
        "Odieresis" => 'Ö',
        "Udieresis" => 'Ü',
        "germandbls" => 'ß',
        "section" => '§',
        "degree" => '°',
        "endash" => '–',
        "emdash" => '—',
        "bullet" => '•',
        "Euro" => '€',
        "multiply" => '×',
        "periodcentered" => '·',
        _ => return None,
    })
}

/// Eine Schrift, bereit zum Lesen von Texten.
#[derive(Clone, Debug, Default)]
pub struct Schrift {
    pub name: String,
    /// Codebereiche (Länge, von, bis) bei Type0; leer = ein Byte je Code.
    bereiche: Vec<(usize, u32, u32)>,
    /// Code → CID (Type0 mit eigener CMap); fehlt der Eintrag, ist CID = Code.
    cids: BTreeMap<u32, u32>,
    /// Breite je Code (einfach) oder je CID (Type0), in 1/1000 Schriftgröße.
    breiten: BTreeMap<u32, f64>,
    standard_breite: f64,
    /// Code → Text aus `/ToUnicode`.
    unicode: BTreeMap<u32, String>,
    /// Zeichen je Byte bei einfachen Schriften.
    kodierung: Vec<char>,
}

/// Zahl aus einem Hex-Text (CMap-Code), höchstens 4 Bytes.
fn code(t: &[u8]) -> u32 {
    t.iter().take(4).fold(0u32, |v, &b| v << 8 | b as u32)
}

/// UTF-16BE in Text.
fn utf16(t: &[u8]) -> String {
    let w: Vec<u16> = t
        .chunks(2)
        .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
        .collect();
    String::from_utf16_lossy(&w)
}

/// Bereiche, CIDs und Unicode aus einer CMap lesen.
fn cmap(d: &[u8], s: &mut Schrift) {
    let mut l = Leser::neu(d, 0);
    let mut werte: Vec<Objekt> = Vec::new();
    let mut art: &[u8] = b"";
    while let Some(w) = l.wort() {
        match w {
            Wort::Objekt(o) => {
                if !art.is_empty() {
                    werte.push(o);
                }
            }
            Wort::Befehl(b) if b.starts_with(b"begin") => {
                art = b;
                werte.clear();
            }
            Wort::Befehl(b) if b.starts_with(b"end") => {
                match art {
                    b"begincodespacerange" => {
                        for p in werte.chunks_exact(2) {
                            if let (Some(a), Some(e)) = (p[0].text(), p[1].text()) {
                                s.bereiche.push((a.len().clamp(1, 4), code(a), code(e)));
                            }
                        }
                    }
                    b"begincidrange" => {
                        for p in werte.chunks_exact(3) {
                            if let (Some(a), Some(e), Some(c)) =
                                (p[0].text(), p[1].text(), p[2].ganz())
                            {
                                let (a, e) = (code(a), code(e));
                                for k in a..=e.min(a.saturating_add(65_535)) {
                                    s.cids.insert(k, (c + (k - a) as i64).max(0) as u32);
                                }
                            }
                        }
                    }
                    b"begincidchar" => {
                        for p in werte.chunks_exact(2) {
                            if let (Some(a), Some(c)) = (p[0].text(), p[1].ganz()) {
                                s.cids.insert(code(a), c.max(0) as u32);
                            }
                        }
                    }
                    b"beginbfchar" => {
                        for p in werte.chunks_exact(2) {
                            if let (Some(a), Some(z)) = (p[0].text(), p[1].text()) {
                                s.unicode.insert(code(a), utf16(z));
                            }
                        }
                    }
                    b"beginbfrange" => {
                        for p in werte.chunks_exact(3) {
                            let (Some(a), Some(e)) = (p[0].text(), p[1].text()) else {
                                continue;
                            };
                            let (a, e) = (code(a), code(e));
                            let e = e.min(a.saturating_add(65_535));
                            match &p[2] {
                                Objekt::Text(z) if !z.is_empty() => {
                                    let mut z = z.clone();
                                    for k in a..=e {
                                        s.unicode.insert(k, utf16(&z));
                                        // letztes Byte hochzählen
                                        if let Some(x) = z.last_mut() {
                                            *x = x.wrapping_add(1);
                                        }
                                    }
                                }
                                Objekt::Feld(f) => {
                                    for (k, z) in (a..=e).zip(f) {
                                        if let Some(z) = z.text() {
                                            s.unicode.insert(k, utf16(z));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                art = b"";
                werte.clear();
            }
            _ => {}
        }
    }
}

impl Schrift {
    /// Schrift aus ihrem Verzeichnis.
    pub fn neu(pdf: &Pdf, v: &Verz) -> Schrift {
        let mut s = Schrift {
            name: v.name("BaseFont").unwrap_or("").to_string(),
            standard_breite: 500.0,
            ..Schrift::default()
        };
        let zahl = |o: &Objekt| pdf.aufloesen(o).zahl();
        if v.name("Subtype") == Some("Type0") {
            // eigene CMap; Identity-H und andere fertige CMaps: zwei Bytes je Code
            if let Objekt::Strom(st) = pdf.aufloesen(v.get("Encoding").unwrap_or(&Objekt::Null)) {
                if let Some(d) = pdf.daten(&st) {
                    cmap(&d, &mut s);
                }
            }
            if s.bereiche.is_empty() {
                s.bereiche.push((2, 0, 0xFFFF));
            }
            let nachfahr = pdf
                .aufloesen(v.get("DescendantFonts").unwrap_or(&Objekt::Null))
                .feld()
                .and_then(|f| f.first().map(|o| pdf.aufloesen(o)));
            if let Some(cid) = nachfahr.as_ref().and_then(Objekt::verz) {
                s.standard_breite = cid.get("DW").and_then(zahl).unwrap_or(1000.0);
                if let Some(Objekt::Feld(w)) = cid.get("W").map(|o| pdf.aufloesen(o)) {
                    let w: Vec<Objekt> = w.iter().map(|o| pdf.aufloesen(o)).collect();
                    let mut i = 0;
                    while i + 1 < w.len() {
                        let Some(c0) = w[i].ganz() else { break };
                        match &w[i + 1] {
                            Objekt::Feld(f) => {
                                for (k, b) in f.iter().enumerate().take(65_536) {
                                    if let Some(b) = zahl(b) {
                                        s.breiten.insert((c0 + k as i64).max(0) as u32, b);
                                    }
                                }
                                i += 2;
                            }
                            o => {
                                let (Some(c1), Some(b)) = (o.ganz(), w.get(i + 2).and_then(zahl))
                                else {
                                    break;
                                };
                                for c in c0.max(0)..=c1.min(c0 + 65_535) {
                                    s.breiten.insert(c as u32, b);
                                }
                                i += 3;
                            }
                        }
                    }
                }
            }
        } else {
            s.kodierung = (0..=255u8).map(win_ansi).collect();
            let erst = v.get("FirstChar").and_then(zahl).unwrap_or(0.0).max(0.0) as u32;
            if let Some(Objekt::Feld(w)) = v.get("Widths").map(|o| pdf.aufloesen(o)) {
                for (k, b) in w.iter().enumerate().take(256) {
                    if let Some(b) = zahl(b) {
                        s.breiten.insert(erst + k as u32, b);
                    }
                }
            }
            if let Some(d) = pdf
                .aufloesen(v.get("FontDescriptor").unwrap_or(&Objekt::Null))
                .verz()
            {
                if let Some(b) = d.get("MissingWidth").and_then(zahl) {
                    s.standard_breite = b;
                }
            }
            if let Objekt::Verz(e) = pdf.aufloesen(v.get("Encoding").unwrap_or(&Objekt::Null)) {
                if let Some(Objekt::Feld(f)) = e.get("Differences").map(|o| pdf.aufloesen(o)) {
                    let mut k = 0usize;
                    for o in f {
                        match o {
                            Objekt::Ganz(n) => k = n.clamp(0, 255) as usize,
                            Objekt::Name(n) => {
                                if let (Some(c), Some(z)) = (glyphe(&n), s.kodierung.get_mut(k)) {
                                    *z = c;
                                }
                                k += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        if let Objekt::Strom(st) = pdf.aufloesen(v.get("ToUnicode").unwrap_or(&Objekt::Null)) {
            if let Some(d) = pdf.daten(&st) {
                let mut u = Schrift::default();
                cmap(&d, &mut u);
                s.unicode = u.unicode;
            }
        }
        s
    }

    /// Codes eines Textes: je Code das Zeichen, die Breite in 1/1000 der
    /// Schriftgröße und ob es das Byte 32 ist (dort wirkt `Tw`).
    pub fn zerlegen(&self, t: &[u8]) -> Vec<(String, f64, bool)> {
        let mut aus = Vec::new();
        let mut i = 0;
        while i < t.len() {
            let mut n = 1;
            if let Some(&(erste, _, _)) = self.bereiche.first() {
                n = (1..=4)
                    .find(|&n| {
                        i + n <= t.len() && {
                            let c = code(&t[i..i + n]);
                            self.bereiche
                                .iter()
                                .any(|&(l, a, e)| l == n && (a..=e).contains(&c))
                        }
                    })
                    .unwrap_or(erste);
                if i + n > t.len() {
                    // halber Code am Ende
                    break;
                }
            }
            let c = code(&t[i..i + n]);
            i += n;
            let zeichen = match self.unicode.get(&c) {
                Some(u) => u.clone(),
                None if self.bereiche.is_empty() => self
                    .kodierung
                    .get(c as usize)
                    .map(|z| z.to_string())
                    .unwrap_or_default(),
                // Type0 ohne Unicode-Tabelle: Code als Zeichen, wenn er eins ist
                None => char::from_u32(c)
                    .filter(|z| !z.is_control())
                    .map(String::from)
                    .unwrap_or_default(),
            };
            let cid = if self.bereiche.is_empty() {
                c
            } else {
                *self.cids.get(&c).unwrap_or(&c)
            };
            let b = *self.breiten.get(&cid).unwrap_or(&self.standard_breite);
            aus.push((zeichen, b, n == 1 && c == 32));
        }
        aus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmap_bfchar_bfrange_und_bereiche() {
        let mut s = Schrift::default();
        cmap(
            b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap
              1 begincodespacerange <0000> <FFFF> endcodespacerange
              2 beginbfchar <0003> <0020> <F041> <E000> endbfchar
              2 beginbfrange <0010> <0012> <0030> <0020> <0021> [<00C4> <00DF>] endbfrange
              1 begincidrange <0100> <0102> 7 endcidrange
              endcmap",
            &mut s,
        );
        assert_eq!(s.bereiche, vec![(2, 0, 0xFFFF)]);
        assert_eq!(s.unicode[&3], " ");
        assert_eq!(s.unicode[&0x11], "1");
        assert_eq!(s.unicode[&0x12], "2");
        assert_eq!(s.unicode[&0x21], "ß");
        assert_eq!(s.unicode[&0xF041], "\u{E000}");
        assert_eq!(s.cids[&0x102], 9);
        s.breiten.insert(9, 250.0);
        s.standard_breite = 1000.0;
        let z = s.zerlegen(&[0x00, 0x10, 0x01, 0x02, 0x00]);
        assert_eq!(z.len(), 2, "halber Code am Ende fällt weg");
        assert_eq!((z[0].0.as_str(), z[0].1), ("0", 1000.0));
        assert_eq!((z[1].0.as_str(), z[1].1), ("\u{102}", 250.0));
    }

    #[test]
    fn einfache_schrift_und_glyphen() {
        assert_eq!(win_ansi(0x80), '€');
        assert_eq!(win_ansi(0xDF), 'ß');
        assert_eq!(glyphe("udieresis"), Some('ü'));
        assert_eq!(glyphe("uni00DF"), Some('ß'));
        assert_eq!(glyphe("seven"), Some('7'));
        assert_eq!(glyphe("A"), Some('A'));
        assert_eq!(glyphe("xyz"), None);
        let s = Schrift {
            kodierung: (0..=255u8).map(win_ansi).collect(),
            standard_breite: 500.0,
            ..Schrift::default()
        };
        let z = s.zerlegen(b"Ma\xDF 1");
        let t: String = z.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(t, "Maß 1");
        assert!(z[3].2 && !z[0].2);
    }
}

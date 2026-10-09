//! Teilmenge einer TrueType-Schrift für das PDF: nur die benutzten Glyphen
//! (mit den Teilen zusammengesetzter Glyphen) behalten ihre Umrisse, alle
//! anderen werden leer. Die Glyphennummern bleiben gleich, so gilt im PDF
//! weiter `/CIDToGIDMap /Identity`. Behalten werden die Tabellen, die ein
//! eingebettetes TrueType braucht (PDF 1.7, 9.9): `head`, `hhea`, `maxp`,
//! `hmtx`, `loca`, `glyf` und, wenn vorhanden, `cvt `, `fpgm`, `prep`.

use super::{i16_at, u16_at, u32_at, Font};
use std::collections::BTreeSet;

/// Behaltene Tabellen, nach Kennung sortiert (so verlangt es das
/// Tabellenverzeichnis).
const TABELLEN: [&[u8; 4]; 9] = [
    b"cvt ", b"fpgm", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp", b"prep",
];

/// Versatz und Länge der Tabelle `tag`.
pub(super) fn tabelle(d: &[u8], tag: &[u8; 4]) -> Option<(usize, usize)> {
    let n = u16_at(d, 4)? as usize;
    (0..n).find_map(|i| {
        let rec = 12 + i * 16;
        (d.get(rec..rec + 4)? == tag)
            .then(|| Some((u32_at(d, rec + 8)? as usize, u32_at(d, rec + 12)? as usize)))?
    })
}

/// Prüfsumme einer Tabelle (Summe der großendigen u32, mit Nullen
/// aufgefüllt).
pub(super) fn pruefsumme(b: &[u8]) -> u32 {
    b.chunks(4).fold(0u32, |s, c| {
        let mut w = [0; 4];
        w[..c.len()].copy_from_slice(c);
        s.wrapping_add(u32::from_be_bytes(w))
    })
}

impl Font {
    /// Teile einer zusammengesetzten Glyphe (leer bei einer einfachen).
    fn komponenten(&self, gid: u16) -> Vec<u16> {
        let mut teile = Vec::new();
        let Some((start, ende)) = self.glyph_range(gid) else {
            return teile;
        };
        let d = &self.data;
        if i16_at(d, start).is_none_or(|n| n >= 0) {
            return teile;
        }
        let mut o = start + 10;
        while o + 4 <= ende {
            let (Some(flags), Some(sub)) = (u16_at(d, o), u16_at(d, o + 2)) else {
                break;
            };
            teile.push(sub);
            o += 4 + if flags & 1 != 0 { 4 } else { 2 };
            o += if flags & 0x8 != 0 {
                2
            } else if flags & 0x40 != 0 {
                4
            } else if flags & 0x80 != 0 {
                8
            } else {
                0
            };
            if flags & 0x20 == 0 {
                break;
            }
        }
        teile
    }

    /// Die Schriftdatei mit den Umrissen nur der Glyphen `glyphen` (und
    /// Glyphe 0, der Ersatzglyphe); zum Einbetten ins PDF.
    pub fn teilmenge(&self, glyphen: &BTreeSet<u16>) -> Vec<u8> {
        let d = &self.data;
        let mut behalten: BTreeSet<u16> = glyphen
            .iter()
            .copied()
            .filter(|g| *g < self.num_glyphs)
            .collect();
        behalten.insert(0);
        let mut offen: Vec<u16> = behalten.iter().copied().collect();
        while let Some(g) = offen.pop() {
            for k in self.komponenten(g) {
                if k < self.num_glyphs && behalten.insert(k) {
                    offen.push(k);
                }
            }
        }
        // glyf und loca (lang), leere Glyphen haben die Länge 0
        let mut glyf = Vec::new();
        let mut loca = Vec::with_capacity(4 * (self.num_glyphs as usize + 1));
        for g in 0..self.num_glyphs {
            loca.extend((glyf.len() as u32).to_be_bytes());
            if !behalten.contains(&g) {
                continue;
            }
            // Zeigt `loca` hinter das Dateiende, bleibt die Glyphe leer statt
            // einer Panik (Review 3bo); die Leinwand zeichnet sie ebenso nicht
            if let Some(umriss) = self.glyph_range(g).and_then(|(a, b)| d.get(a..b)) {
                glyf.extend(umriss);
                glyf.resize(glyf.len().next_multiple_of(4), 0);
            }
        }
        loca.extend((glyf.len() as u32).to_be_bytes());

        let mut tabellen: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
        for tag in TABELLEN {
            let inhalt = match tag {
                b"glyf" => std::mem::take(&mut glyf),
                b"loca" => std::mem::take(&mut loca),
                _ => {
                    let Some((o, l)) = tabelle(d, tag) else {
                        continue;
                    };
                    let Some(t) = d.get(o..o + l) else {
                        continue;
                    };
                    let mut t = t.to_vec();
                    if tag == b"head" && t.len() >= 54 {
                        // checkSumAdjustment erst am Ende; loca jetzt lang
                        t[8..12].fill(0);
                        t[50..52].copy_from_slice(&1i16.to_be_bytes());
                    }
                    t
                }
            };
            tabellen.push((tag, inhalt));
        }

        let n = tabellen.len() as u16;
        let stufe = 15 - n.leading_zeros() as u16; // floor(log2(n))
        let such = 16 * (1 << stufe);
        let mut out = Vec::new();
        out.extend([0, 1, 0, 0]);
        for v in [n, such, stufe, 16 * n - such] {
            out.extend(v.to_be_bytes());
        }
        let mut lage = 12 + 16 * tabellen.len();
        let mut koerper = Vec::new();
        let mut head = None;
        for (tag, t) in &tabellen {
            if *tag == b"head" {
                head = Some(lage);
            }
            out.extend(*tag);
            out.extend(pruefsumme(t).to_be_bytes());
            out.extend((lage as u32).to_be_bytes());
            out.extend((t.len() as u32).to_be_bytes());
            koerper.extend(t);
            koerper.resize(koerper.len().next_multiple_of(4), 0);
            lage = 12 + 16 * tabellen.len() + koerper.len();
        }
        out.extend(koerper);
        if let Some(h) = head {
            let ausgleich = 0xB1B0_AFBAu32.wrapping_sub(pruefsumme(&out));
            out[h + 8..h + 12].copy_from_slice(&ausgleich.to_be_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schrift() -> Option<Font> {
        std::fs::read("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf")
            .ok()
            .and_then(Font::parse)
    }

    /// Bytes der Glyphe `g` in einer Datei mit langer `loca`.
    fn glyphe(d: &[u8], g: u16) -> &[u8] {
        let (loca, _) = tabelle(d, b"loca").unwrap();
        let (glyf, _) = tabelle(d, b"glyf").unwrap();
        let at = |i: usize| u32_at(d, loca + 4 * i).unwrap() as usize;
        &d[glyf + at(g as usize)..glyf + at(g as usize + 1)]
    }

    /// Hinweis P: die benutzten Glyphen sind unverändert da (auch die Teile
    /// von „Ä“), alle anderen leer, die Prüfsummen stimmen, und die Datei
    /// ist ein Bruchteil der ganzen Schrift.
    #[test]
    fn teilmenge_behaelt_genau_die_benutzten_glyphen() {
        let Some(f) = schrift() else {
            return;
        };
        let text = "Größe 172,224 m² Ä";
        let glyphen: BTreeSet<u16> = text.chars().map(|c| f.glyph(c)).collect();
        let t = f.teilmenge(&glyphen);
        assert!(
            t.len() * 10 < f.data().len(),
            "{} von {}",
            t.len(),
            f.data().len()
        );

        // „Ä“ ist zusammengesetzt; seine Teile kommen mit
        let ae = f.glyph('Ä');
        let teile = f.komponenten(ae);
        assert!(!teile.is_empty(), "Ä nicht zusammengesetzt");
        for g in glyphen.iter().chain(&teile).chain([&0]) {
            let alt = f.glyph_range(*g).map_or(&[][..], |(a, b)| &f.data()[a..b]);
            let neu = glyphe(&t, *g);
            assert_eq!(&neu[..alt.len()], alt, "Glyphe {g}");
        }
        // Behalten: die benutzten Glyphen, 0 und alle Teile (auch von ö, ü)
        let mut behalten: BTreeSet<u16> = glyphen.clone();
        behalten.insert(0);
        loop {
            let teile: Vec<u16> = behalten.iter().flat_map(|g| f.komponenten(*g)).collect();
            let vorher = behalten.len();
            behalten.extend(teile);
            if behalten.len() == vorher {
                break;
            }
        }
        for g in &behalten {
            let alt = f.glyph_range(*g).map_or(&[][..], |(a, b)| &f.data()[a..b]);
            assert_eq!(&glyphe(&t, *g)[..alt.len()], alt, "Teil {g}");
        }
        let leer = (0..f.num_glyphs).filter(|g| !behalten.contains(g));
        assert!(leer.clone().count() > 1000);
        for g in leer {
            assert!(glyphe(&t, g).is_empty(), "Glyphe {g} nicht leer");
        }

        // Prüfsummen: jede Tabelle und die ganze Datei
        let n = u16_at(&t, 4).unwrap() as usize;
        assert_eq!(n, TABELLEN.len());
        for i in 0..n {
            let rec = 12 + 16 * i;
            let (o, l) = (
                u32_at(&t, rec + 8).unwrap() as usize,
                u32_at(&t, rec + 12).unwrap() as usize,
            );
            let mut inhalt = t[o..o + l].to_vec();
            if &t[rec..rec + 4] == b"head" {
                inhalt[8..12].fill(0);
            }
            assert_eq!(pruefsumme(&inhalt), u32_at(&t, rec + 4).unwrap());
        }
        assert_eq!(pruefsumme(&t), 0xB1B0_AFBA);
        // Kopf: lange loca, gleiche Glyphenzahl
        let (head, _) = tabelle(&t, b"head").unwrap();
        assert_eq!(i16_at(&t, head + 50), Some(1));
        let (maxp, _) = tabelle(&t, b"maxp").unwrap();
        assert_eq!(u16_at(&t, maxp + 4), Some(f.num_glyphs));
    }

    /// Review 3bo: Eine Schrift, deren `loca` hinter das Dateiende zeigt,
    /// gibt eine Teilmenge mit leerer Glyphe statt einer Panik.
    #[test]
    fn teilmenge_ohne_panik_bei_kaputter_loca() {
        let Some(f) = schrift() else {
            return;
        };
        let g = f.glyph('A');
        let mut d = f.data().to_vec();
        let (loca, _) = tabelle(&d, b"loca").unwrap();
        let (head, _) = tabelle(&d, b"head").unwrap();
        let i = g as usize + 1;
        if i16_at(&d, head + 50) == Some(1) {
            d[loca + 4 * i..loca + 4 * i + 4].copy_from_slice(&0x00F0_0000u32.to_be_bytes());
        } else {
            d[loca + 2 * i..loca + 2 * i + 2].copy_from_slice(&0xFFF0u16.to_be_bytes());
        }
        let kaputt = Font::parse(d).expect("Kopf ist heil");
        let t = kaputt.teilmenge(&[g].into_iter().collect());
        assert!(glyphe(&t, g).is_empty());
        assert_eq!(pruefsumme(&t), 0xB1B0_AFBA);
    }
}

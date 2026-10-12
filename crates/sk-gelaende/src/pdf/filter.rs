//! Stromfilter (ISO 32000-1 §7.4): Flate mit PNG- und TIFF-Prädiktoren
//! (Querverweisströme sind so gepackt), ASCIIHex, ASCII85 und RunLength.
//! Bildfilter (DCT, JPX, CCITT, JBIG2) und LZW liest Skizzeo nicht; Bilder
//! übergeht der Lageplan ohnehin.

use super::objekt::{Objekt, Verz};
use crate::inflate;

/// Daten durch einen Filter mit seinen Parametern; `None`, wenn der Filter
/// unbekannt ist oder die Daten nichts ergeben.
pub fn filter(name: &str, d: &[u8], parm: Option<&Verz>) -> Option<Vec<u8>> {
    let aus = match name {
        "FlateDecode" | "Fl" => {
            let (aus, ganz) = inflate::nachsichtig(d);
            if aus.is_empty() && !ganz {
                return None;
            }
            aus
        }
        "ASCIIHexDecode" | "AHx" => hex(d),
        "ASCII85Decode" | "A85" => a85(d)?,
        "RunLengthDecode" | "RL" => lauflaenge(d),
        "Crypt" => d.to_vec(),
        _ => return None,
    };
    match parm {
        Some(p) => praediktor(aus, p),
        None => Some(aus),
    }
}

fn hex(d: &[u8]) -> Vec<u8> {
    let mut aus = Vec::new();
    let mut halb: Option<u8> = None;
    for &b in d {
        if b == b'>' {
            break;
        }
        if let Some(v) = (b as char).to_digit(16) {
            match halb.take() {
                Some(h) => aus.push(h * 16 + v as u8),
                None => halb = Some(v as u8),
            }
        }
    }
    if let Some(h) = halb {
        aus.push(h * 16);
    }
    aus
}

fn a85(d: &[u8]) -> Option<Vec<u8>> {
    let mut aus = Vec::new();
    let mut gruppe = [0u8; 5];
    let mut n = 0;
    let d = d.strip_prefix(b"<~").unwrap_or(d);
    for &b in d {
        match b {
            b'~' => break,
            b'z' if n == 0 => aus.extend_from_slice(&[0; 4]),
            b'!'..=b'u' => {
                gruppe[n] = b - b'!';
                n += 1;
                if n == 5 {
                    let v = gruppe.iter().fold(0u64, |a, &g| a * 85 + g as u64);
                    aus.extend_from_slice(&u32::try_from(v).ok()?.to_be_bytes());
                    n = 0;
                }
            }
            _ if super::objekt::leer(b) => {}
            _ => return None,
        }
    }
    if n > 1 {
        for g in gruppe.iter_mut().skip(n) {
            *g = 84;
        }
        let v = gruppe.iter().fold(0u64, |a, &g| a * 85 + g as u64);
        aus.extend_from_slice(&u32::try_from(v).ok()?.to_be_bytes()[..n - 1]);
    }
    Some(aus)
}

fn lauflaenge(d: &[u8]) -> Vec<u8> {
    let mut aus = Vec::new();
    let mut i = 0;
    while let Some(&n) = d.get(i) {
        match n {
            128 => break,
            0..=127 => {
                let e = (i + 2 + n as usize).min(d.len());
                aus.extend_from_slice(&d[(i + 1).min(e)..e]);
                i = e;
            }
            _ => {
                if let Some(&b) = d.get(i + 1) {
                    aus.extend(std::iter::repeat_n(b, 257 - n as usize));
                }
                i += 2;
            }
        }
    }
    aus
}

/// PNG-Prädiktor (10 bis 15, je Zeile ein Filterbyte) oder TIFF 2 für
/// 8 Bit je Farbe.
fn praediktor(d: Vec<u8>, p: &Verz) -> Option<Vec<u8>> {
    let art = p.ganz("Predictor").unwrap_or(1);
    if art < 2 {
        return Some(d);
    }
    let farben = p.ganz("Colors").unwrap_or(1).clamp(1, 32) as usize;
    let bits = p.ganz("BitsPerComponent").unwrap_or(8).clamp(1, 16) as usize;
    let spalten = p.ganz("Columns").unwrap_or(1).clamp(1, 1 << 20) as usize;
    let bpp = (farben * bits).div_ceil(8);
    let zeile = (farben * bits * spalten).div_ceil(8);
    if art == 2 {
        if bits != 8 {
            return Some(d);
        }
        let mut d = d;
        for z in d.chunks_mut(zeile) {
            for i in bpp..z.len() {
                z[i] = z[i].wrapping_add(z[i - bpp]);
            }
        }
        return Some(d);
    }
    let mut aus = Vec::with_capacity(d.len());
    let mut vor = vec![0u8; zeile];
    for z in d.chunks(zeile + 1) {
        let (&typ, roh) = z.split_first()?;
        let mut cur = roh.to_vec();
        cur.resize(zeile, 0);
        for i in 0..zeile {
            let a = if i >= bpp { cur[i - bpp] } else { 0 };
            let b = vor[i];
            let c = if i >= bpp { vor[i - bpp] } else { 0 };
            cur[i] = cur[i].wrapping_add(match typ {
                1 => a,
                2 => b,
                3 => ((a as u16 + b as u16) / 2) as u8,
                4 => {
                    let p = a as i16 + b as i16 - c as i16;
                    let (pa, pb, pc) = (
                        (p - a as i16).abs(),
                        (p - b as i16).abs(),
                        (p - c as i16).abs(),
                    );
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                _ => 0,
            });
        }
        aus.extend_from_slice(&cur[..roh.len().min(zeile)]);
        vor = cur;
    }
    Some(aus)
}

/// Filter und Parameter eines Stroms als Liste (`/Filter` als Name oder
/// Feld, `/DecodeParms` ebenso); Bezüge sind schon aufgelöst.
pub fn liste(filter: Option<&Objekt>, parm: Option<&Objekt>) -> Vec<(String, Option<Verz>)> {
    let namen: Vec<String> = match filter {
        Some(Objekt::Name(n)) => vec![n.clone()],
        Some(Objekt::Feld(f)) => f
            .iter()
            .filter_map(|o| o.name().map(String::from))
            .collect(),
        _ => Vec::new(),
    };
    let parms: Vec<Option<Verz>> = match parm {
        Some(Objekt::Verz(v)) => vec![Some(v.clone())],
        Some(Objekt::Feld(f)) => f.iter().map(|o| o.verz().cloned()).collect(),
        _ => Vec::new(),
    };
    namen
        .into_iter()
        .enumerate()
        .map(|(i, n)| (n, parms.get(i).cloned().flatten()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_a85_lauflaenge() {
        assert_eq!(hex(b"48 65 6c 6C 6f 2>"), b"Hello ".to_vec());
        // „Man is“ aus dem Beispiel der Spezifikation
        assert_eq!(
            a85(b"9jqo^BlbD-BleB1DJ+*+F(f,q~>"),
            Some(b"Man is distinguished".to_vec())
        );
        assert_eq!(a85(b"z~>"), Some(vec![0; 4]));
        assert_eq!(a85(b"<~87cURD]i,\"Ebo7~>"), Some(b"Hello World".to_vec()));
        assert_eq!(a85(b"\xff"), None);
        assert_eq!(
            lauflaenge(&[2, b'a', b'b', b'c', 254, b'x', 128]),
            b"abcxxx".to_vec()
        );
        assert_eq!(lauflaenge(&[5, b'a']), b"a".to_vec());
    }

    #[test]
    fn png_praediktor() {
        // zwei Zeilen à 3 Bytes: „Sub“ und „Up“
        let d = vec![1, 10, 5, 5, 2, 1, 1, 1];
        let mut p = Verz::default();
        p.setzen("Predictor", Objekt::Ganz(12));
        p.setzen("Columns", Objekt::Ganz(3));
        assert_eq!(praediktor(d, &p), Some(vec![10, 15, 20, 11, 16, 21]));
        // Paeth, Avg und eine kurze letzte Zeile brechen nicht ab
        let d = vec![4, 1, 2, 3, 3, 4, 5, 6, 0, 9];
        assert_eq!(praediktor(d, &p).map(|v| v.len()), Some(7));
    }
}

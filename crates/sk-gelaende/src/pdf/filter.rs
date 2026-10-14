//! Stromfilter (ISO 32000-1 §7.4): Flate mit PNG- und TIFF-Prädiktoren
//! (Querverweisströme sind so gepackt), ASCIIHex, ASCII85 und RunLength.
//! Bildfilter (DCT, JPX, CCITT, JBIG2) und LZW liest Skizzeo nicht; Bilder
//! übergeht der Lageplan ohnehin.

use super::objekt::{Objekt, Verz};
use crate::inflate;

/// Warum ein Filter nichts ergab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fehler {
    /// Unbekannter Filter oder Daten, die nichts ergeben.
    Nichts,
    /// Die Daten ergäben mehr als die erlaubten Bytes (Bombe).
    Voll,
}

/// Daten durch einen Filter mit seinen Parametern, höchstens `grenze`
/// Bytes.
pub fn filter(name: &str, d: &[u8], parm: Option<&Verz>, grenze: usize) -> Result<Vec<u8>, Fehler> {
    let aus = match name {
        "FlateDecode" | "Fl" => {
            let (aus, ganz, voll) = inflate::nachsichtig_bis(d, grenze);
            if voll {
                return Err(Fehler::Voll);
            }
            if aus.is_empty() && !ganz {
                return Err(Fehler::Nichts);
            }
            aus
        }
        "ASCIIHexDecode" | "AHx" => hex(d),
        "ASCII85Decode" | "A85" => a85(d, grenze)?,
        "RunLengthDecode" | "RL" => lauflaenge(d, grenze)?,
        "Crypt" => d.to_vec(),
        _ => return Err(Fehler::Nichts),
    };
    if aus.len() > grenze {
        return Err(Fehler::Voll);
    }
    match parm {
        Some(p) => praediktor(aus, p).ok_or(Fehler::Nichts),
        None => Ok(aus),
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

fn a85(d: &[u8], grenze: usize) -> Result<Vec<u8>, Fehler> {
    let mut aus = Vec::new();
    let mut gruppe = [0u8; 5];
    let mut n = 0;
    let d = d.strip_prefix(b"<~").unwrap_or(d);
    for &b in d {
        match b {
            b'~' => break,
            _ if aus.len() > grenze => return Err(Fehler::Voll),
            b'z' if n == 0 => aus.extend_from_slice(&[0; 4]),
            b'!'..=b'u' => {
                gruppe[n] = b - b'!';
                n += 1;
                if n == 5 {
                    let v = gruppe.iter().fold(0u64, |a, &g| a * 85 + g as u64);
                    let v = u32::try_from(v).map_err(|_| Fehler::Nichts)?;
                    aus.extend_from_slice(&v.to_be_bytes());
                    n = 0;
                }
            }
            _ if super::objekt::leer(b) => {}
            _ => return Err(Fehler::Nichts),
        }
    }
    if n > 1 {
        for g in gruppe.iter_mut().skip(n) {
            *g = 84;
        }
        let v = gruppe.iter().fold(0u64, |a, &g| a * 85 + g as u64);
        let v = u32::try_from(v).map_err(|_| Fehler::Nichts)?;
        aus.extend_from_slice(&v.to_be_bytes()[..n - 1]);
    }
    Ok(aus)
}

fn lauflaenge(d: &[u8], grenze: usize) -> Result<Vec<u8>, Fehler> {
    let mut aus = Vec::new();
    let mut i = 0;
    while let Some(&n) = d.get(i) {
        if aus.len() > grenze {
            return Err(Fehler::Voll);
        }
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
    Ok(aus)
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
    // eine Zeile länger als die Daten ergibt nichts mehr (kein Speicher
    // nach /Columns, L1)
    let zeile = (farben * bits * spalten).div_ceil(8).min(d.len().max(1));
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
        const G: usize = 1 << 20;
        assert_eq!(
            a85(b"9jqo^BlbD-BleB1DJ+*+F(f,q~>", G),
            Ok(b"Man is distinguished".to_vec())
        );
        assert_eq!(a85(b"z~>", G), Ok(vec![0; 4]));
        assert_eq!(a85(b"<~87cURD]i,\"Ebo7~>", G), Ok(b"Hello World".to_vec()));
        assert_eq!(a85(b"\xff", G), Err(Fehler::Nichts));
        assert_eq!(
            lauflaenge(&[2, b'a', b'b', b'c', 254, b'x', 128], G),
            Ok(b"abcxxx".to_vec())
        );
        assert_eq!(lauflaenge(&[5, b'a'], G), Ok(b"a".to_vec()));
    }

    /// Bomben (Review W37 B2): sechsmal RunLength macht aus wenigen Bytes
    /// Gigabytes; jede Stufe hält an der Grenze an, ebenso Flate und
    /// ASCII85 mit „z“.
    #[test]
    fn bomben_halten_an_der_grenze() {
        let g = 1 << 16;
        let mut d = vec![129u8, b'x'];
        let mut stufen = 0;
        loop {
            match filter("RunLengthDecode", &d, None, g) {
                Ok(x) => {
                    stufen += 1;
                    // nächste Stufe: jedes Byte 128-mal
                    d = x.iter().flat_map(|&b| [129u8, b]).collect();
                }
                Err(e) => {
                    assert_eq!(e, Fehler::Voll);
                    break;
                }
            }
            assert!(stufen < 6, "keine Grenze");
        }
        let z = vec![b'z'; 1 << 20];
        assert_eq!(filter("A85", &z, None, g), Err(Fehler::Voll));
        // knapp 1 MB Nullen aus 6,6 kB: über der Grenze voll, darunter ganz
        let null = crate::inflate::bombe(4064);
        assert!(null.len() < 7000);
        assert_eq!(filter("FlateDecode", &null, None, g), Err(Fehler::Voll));
        assert_eq!(
            filter("FlateDecode", &null, None, 1 << 20).map(|v| v.len()),
            Ok(1 + 258 * 4064)
        );
        assert_eq!(filter("LZWDecode", b"x", None, g), Err(Fehler::Nichts));
        // Prädiktor mit riesigen /Columns: Zeile höchstens so lang wie die Daten
        let mut p = Verz::default();
        p.setzen("Predictor", Objekt::Ganz(12));
        p.setzen("Columns", Objekt::Ganz(1 << 20));
        p.setzen("Colors", Objekt::Ganz(32));
        p.setzen("BitsPerComponent", Objekt::Ganz(16));
        assert_eq!(praediktor(vec![0, 1, 2, 3], &p), Some(vec![1, 2, 3]));
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

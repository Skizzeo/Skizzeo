//! Minimaler PNG-Schreiber (RGBA8, unkomprimierte Deflate-Blöcke) und
//! -Leser für die eigenen Dateien (Soll-Bilder in Tests).

pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), width as usize * height as usize * 4);
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for row in rgba.chunks(width as usize * 4) {
        raw.push(0); // Filter: keiner
        raw.extend_from_slice(row);
    }

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 Bit, RGBA
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Wie [`encode_png`], aber gepackt ([`crate::deflate::zlib`]): kleiner
/// für Soll-Bilder, die im Repository liegen.
pub fn encode_png_gepackt(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), width as usize * height as usize * 4);
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for row in rgba.chunks(width as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &crate::deflate::zlib(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Liest eine PNG-Datei, wie die beiden Schreiber hier sie erzeugen: RGBA
/// mit 8 Bit, ohne Zeilenfilter, gespeichert oder mit festen Codes gepackt.
/// Anderes (Filter, Paletten, dynamische Codes) gibt `None`.
pub fn decode_png(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if data.get(..8)? != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return None;
    }
    let (mut i, mut idat, mut kopf) = (8usize, Vec::new(), None);
    while i + 8 <= data.len() {
        let len = u32::from_be_bytes(data[i..i + 4].try_into().ok()?) as usize;
        let kind = data.get(i + 4..i + 8)?;
        let body = data.get(i + 8..i + 8 + len)?;
        match kind {
            b"IHDR" => kopf = Some(body.to_vec()),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        i += 12 + len;
    }
    let k = kopf?;
    let w = u32::from_be_bytes(k.get(0..4)?.try_into().ok()?);
    let h = u32::from_be_bytes(k.get(4..8)?.try_into().ok()?);
    if k.get(8..13)? != [8, 6, 0, 0, 0] {
        return None;
    }
    let raw = crate::deflate::inflate(idat.get(2..)?)?;
    let zeile = w as usize * 4 + 1;
    if raw.len() < zeile * h as usize {
        return None;
    }
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for z in raw.chunks(zeile).take(h as usize) {
        if z[0] != 0 {
            return None;
        }
        rgba.extend_from_slice(&z[1..]);
    }
    Some((w, h, rgba))
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = data.chunks(65535).collect();
    for (i, b) in blocks.iter().enumerate() {
        out.push(u8::from(i + 1 == blocks.len()));
        let len = b.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(b);
    }
    if blocks.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &v in chunk {
            a += v as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_bekannter_wert() {
        assert_eq!(super::crc32(b"IEND"), 0xAE42_6082);
    }

    /// Beide Schreiber lesen sich zurück; das gepackte Bild ist kleiner;
    /// fremde oder kaputte Dateien geben `None` statt abzustürzen.
    #[test]
    fn hin_und_zurueck() {
        let (w, h) = (70u32, 40u32);
        let px: Vec<u8> = (0..w * h)
            .flat_map(|i| [(i % 7 * 30) as u8, (i / w * 5) as u8, 200, 255])
            .collect();
        let roh = encode_png(w, h, &px);
        let klein = encode_png_gepackt(w, h, &px);
        assert!(klein.len() < roh.len() / 2);
        for d in [&roh, &klein] {
            assert_eq!(decode_png(d), Some((w, h, px.clone())));
        }
        for n in [0, 8, 20, 40, klein.len() - 20] {
            assert_eq!(decode_png(&klein[..n]), None, "{n}");
        }
        let mut anders = klein.clone();
        anders[24] = 2; // Farbtyp RGB
        assert_eq!(decode_png(&anders), None);
    }
}

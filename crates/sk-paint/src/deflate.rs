//! Eigenes Deflate (RFC 1951) mit zlib-Hülle (RFC 1950) für die Ströme im
//! PDF: LZ77 über ein 32-KiB-Fenster mit Hash-Ketten, ein einziger Block
//! mit den festen Huffman-Codes. Keine dynamischen Tabellen; für Schriften
//! und Seiteninhalte reicht das (etwa halbe Größe).

/// Kürzeste und längste Übereinstimmung, Fenster und Kettenlänge.
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const WINDOW: usize = 32 * 1024;
const CHAIN: usize = 48;
const HASH_BITS: u32 = 15;

/// Längen 3–258: Code 257–285 mit Basis und Zusatzbits.
const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
/// Abstände 1–32768: Code 0–29 mit Basis und Zusatzbits.
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Bits niederwertigste zuerst, wie Deflate sie erwartet.
struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, v: u32, n: u32) {
        self.acc |= u64::from(v) << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// Ein Huffman-Code: höchstwertiges Bit zuerst.
    fn code(&mut self, code: u32, n: u32) {
        let r = code.reverse_bits() >> (32 - n);
        self.put(r, n);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

/// Fester Code für Literal oder Länge `sym` (0–287).
fn lit(b: &mut Bits, sym: u32) {
    match sym {
        0..=143 => b.code(0x30 + sym, 8),
        144..=255 => b.code(0x190 + sym - 144, 9),
        256..=279 => b.code(sym - 256, 7),
        _ => b.code(0xC0 + sym - 280, 8),
    }
}

fn laenge(b: &mut Bits, len: usize) {
    let i = LEN_BASE
        .iter()
        .rposition(|&x| x as usize <= len)
        .unwrap_or(0);
    lit(b, 257 + i as u32);
    b.put((len - LEN_BASE[i] as usize) as u32, u32::from(LEN_EXTRA[i]));
}

fn abstand(b: &mut Bits, d: usize) {
    let i = DIST_BASE
        .iter()
        .rposition(|&x| x as usize <= d)
        .unwrap_or(0);
    b.code(i as u32, 5);
    b.put((d - DIST_BASE[i] as usize) as u32, u32::from(DIST_EXTRA[i]));
}

fn hash(d: &[u8], i: usize) -> usize {
    let v = u32::from(d[i]) << 16 | u32::from(d[i + 1]) << 8 | u32::from(d[i + 2]);
    (v.wrapping_mul(2_654_435_761) >> (32 - HASH_BITS)) as usize
}

/// Roher Deflate-Strom von `data`.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut b = Bits {
        out: Vec::with_capacity(data.len() / 2 + 16),
        acc: 0,
        n: 0,
    };
    // BFINAL = 1, BTYPE = 01 (feste Codes)
    b.put(1, 1);
    b.put(1, 2);
    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let mut prev = vec![usize::MAX; WINDOW];
    let einfuegen = |head: &mut [usize], prev: &mut [usize], i: usize| {
        if i + MIN_MATCH <= data.len() {
            let h = hash(data, i);
            prev[i % WINDOW] = head[h];
            head[h] = i;
        }
    };
    let mut i = 0;
    while i < data.len() {
        let mut best = (0, 0);
        if i + MIN_MATCH <= data.len() {
            let mut j = head[hash(data, i)];
            let max = MAX_MATCH.min(data.len() - i);
            for _ in 0..CHAIN {
                if j == usize::MAX || i - j > WINDOW || j >= i {
                    break;
                }
                let n = data[j..]
                    .iter()
                    .zip(&data[i..i + max])
                    .take_while(|(a, b)| a == b)
                    .count();
                if n > best.0 {
                    best = (n, i - j);
                    if n == max {
                        break;
                    }
                }
                let p = prev[j % WINDOW];
                if p == usize::MAX || p >= j {
                    break;
                }
                j = p;
            }
        }
        if best.0 >= MIN_MATCH {
            laenge(&mut b, best.0);
            abstand(&mut b, best.1);
            for k in i..i + best.0 {
                einfuegen(&mut head, &mut prev, k);
            }
            i += best.0;
        } else {
            lit(&mut b, u32::from(data[i]));
            einfuegen(&mut head, &mut prev, i);
            i += 1;
        }
    }
    lit(&mut b, 256);
    b.finish()
}

/// Adler-32 (RFC 1950).
pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    b << 16 | a
}

/// zlib-Strom (für `/FlateDecode`).
pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9C];
    out.extend(deflate(data));
    out.extend(adler32(data).to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kleiner Entpacker für feste und gespeicherte Blöcke, nur zum Prüfen.
    fn inflate(d: &[u8]) -> Vec<u8> {
        let mut pos = 0usize;
        let bit = |pos: &mut usize| -> u32 {
            let v = (d[*pos / 8] >> (*pos % 8)) & 1;
            *pos += 1;
            u32::from(v)
        };
        let bits = |pos: &mut usize, n: u32| -> u32 {
            let mut v = 0;
            for k in 0..n {
                v |= bit(pos) << k;
            }
            v
        };
        let huff = |pos: &mut usize, n: u32| -> u32 {
            let mut v = 0;
            for _ in 0..n {
                v = v << 1 | bit(pos);
            }
            v
        };
        let mut out: Vec<u8> = Vec::new();
        loop {
            let last = bits(&mut pos, 1);
            let typ = bits(&mut pos, 2);
            assert_eq!(typ, 1, "nur feste Codes");
            loop {
                // Literal/Länge: 7, 8 oder 9 Bit
                let mut c = huff(&mut pos, 7);
                let sym = if c <= 0x17 {
                    256 + c
                } else {
                    c = c << 1 | bit(&mut pos);
                    if (0x30..=0xBF).contains(&c) {
                        c - 0x30
                    } else if (0xC0..=0xC7).contains(&c) {
                        280 + c - 0xC0
                    } else {
                        c = c << 1 | bit(&mut pos);
                        144 + c - 0x190
                    }
                };
                match sym {
                    0..=255 => out.push(sym as u8),
                    256 => break,
                    _ => {
                        let i = (sym - 257) as usize;
                        let len =
                            LEN_BASE[i] as usize + bits(&mut pos, u32::from(LEN_EXTRA[i])) as usize;
                        let di = huff(&mut pos, 5) as usize;
                        let dist = DIST_BASE[di] as usize
                            + bits(&mut pos, u32::from(DIST_EXTRA[di])) as usize;
                        let start = out.len() - dist;
                        for k in 0..len {
                            out.push(out[start + k]);
                        }
                    }
                }
            }
            if last == 1 {
                return out;
            }
        }
    }

    #[test]
    fn hin_und_zurueck() {
        let mut lang = Vec::new();
        for k in 0..20_000u32 {
            lang.extend(format!("BT /F1 9 Tf {k} {} Td (Zeile) Tj ET\n", k % 7).bytes());
        }
        let mut zufall = Vec::new();
        let mut x = 0x1234_5678u32;
        for _ in 0..70_000 {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            zufall.push((x % 7) as u8 * 37);
        }
        for d in [
            Vec::new(),
            b"a".to_vec(),
            b"abcabcabcabcabcabc".to_vec(),
            vec![0u8; 100_000],
            (0..=255u8).collect(),
            lang,
            zufall,
        ] {
            let z = deflate(&d);
            assert_eq!(inflate(&z), d, "Länge {}", d.len());
        }
        // gleichförmiger Inhalt wird kleiner
        let d = vec![b'x'; 50_000];
        assert!(deflate(&d).len() < 1_000);
    }

    #[test]
    fn zlib_huelle() {
        let z = zlib(b"Wikipedia");
        assert_eq!(&z[..2], &[0x78, 0x9C]);
        assert_eq!((0x78u32 * 256 + 0x9C) % 31, 0);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(&z[z.len() - 4..], &0x11E6_0398u32.to_be_bytes());
    }
}

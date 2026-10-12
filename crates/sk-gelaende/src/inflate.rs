//! Flate entpacken (RFC 1951, Hülle nach RFC 1950) für die Ströme im PDF
//! (plan-gelaendemodul.md §5 G1c, Technik). Gespeicherte Blöcke, feste und
//! eigene Huffman-Tabellen; Fehler in den Daten werden `None`, nie ein
//! Abbruch.
//!
//! Dekodiert wird wie in zlibs `puff.c`: kanonische Codes, Bit für Bit, ohne
//! Tabellen zum Nachschlagen. Für die Ströme einer Karte (einige 10 kB)
//! reicht das.

/// Mehr als so viele Bytes entpackt Skizzeo nicht (Schutz vor Bomben).
pub const GRENZE: usize = 1 << 28;

/// Bits lesen, das niedrigste zuerst.
struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    puffer: u64,
    n: u32,
}

impl<'a> Bits<'a> {
    fn neu(d: &'a [u8]) -> Self {
        Bits {
            d,
            pos: 0,
            puffer: 0,
            n: 0,
        }
    }

    /// `k` Bits (höchstens 16).
    fn bits(&mut self, k: u32) -> Option<u32> {
        while self.n < k {
            let b = *self.d.get(self.pos)?;
            self.pos += 1;
            self.puffer |= (b as u64) << self.n;
            self.n += 8;
        }
        let v = (self.puffer & ((1u64 << k) - 1)) as u32;
        self.puffer >>= k;
        self.n -= k;
        Some(v)
    }

    /// Rest des angefangenen Bytes verwerfen.
    fn byte_grenze(&mut self) {
        let r = self.n % 8;
        self.puffer >>= r;
        self.n -= r;
    }

    /// Lage des nächsten ganzen Bytes (nach [`Self::byte_grenze`]).
    fn byte_pos(&self) -> usize {
        self.pos - (self.n / 8) as usize
    }

    /// Nach `p` weiterlesen, der Puffer ist dann leer.
    fn setze(&mut self, p: usize) {
        self.pos = p;
        self.puffer = 0;
        self.n = 0;
    }
}

/// Kanonischer Huffman-Code: Anzahl der Codes je Länge und die Zeichen in
/// Code-Reihenfolge.
struct Huffman {
    anzahl: [u16; 16],
    zeichen: Vec<u16>,
}

impl Huffman {
    /// Aus den Codelängen je Zeichen (0 = kommt nicht vor). Überbelegte
    /// Längen sind ein Fehler; unvollständige Codes sind erlaubt (ein
    /// einzelner Abstandscode kommt so vor).
    fn neu(laengen: &[u8]) -> Option<Self> {
        let mut anzahl = [0u16; 16];
        for &l in laengen {
            *anzahl.get_mut(l as usize)? += 1;
        }
        let mut frei: i32 = 1;
        for &a in &anzahl[1..] {
            frei = frei * 2 - a as i32;
            if frei < 0 {
                return None;
            }
        }
        let mut start = [0u16; 16];
        for l in 1..15 {
            start[l + 1] = start[l] + anzahl[l];
        }
        let mut zeichen = vec![0u16; laengen.len()];
        for (z, &l) in laengen.iter().enumerate() {
            if l != 0 {
                zeichen[start[l as usize] as usize] = z as u16;
                start[l as usize] += 1;
            }
        }
        anzahl[0] = 0;
        Some(Huffman { anzahl, zeichen })
    }

    fn lesen(&self, b: &mut Bits) -> Option<u16> {
        let (mut code, mut erster, mut index) = (0i32, 0i32, 0i32);
        for l in 1..16 {
            code |= b.bits(1)? as i32;
            let n = self.anzahl[l] as i32;
            if code - erster < n {
                return self.zeichen.get((index + code - erster) as usize).copied();
            }
            index += n;
            erster = (erster + n) << 1;
            code <<= 1;
        }
        None
    }
}

const LAENGE_BASIS: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LAENGE_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const ABSTAND_BASIS: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const ABSTAND_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// Reihenfolge der Codelängen-Codes im Kopf eines eigenen Blocks.
const REIHE: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Einen komprimierten Block bis zum Endzeichen 256 entpacken.
fn block(b: &mut Bits, aus: &mut Vec<u8>, lit: &Huffman, abst: &Huffman) -> Option<()> {
    loop {
        let z = lit.lesen(b)? as usize;
        if z < 256 {
            aus.push(z as u8);
        } else if z == 256 {
            return Some(());
        } else {
            let i = z - 257;
            let laenge = *LAENGE_BASIS.get(i)? as usize + b.bits(LAENGE_EXTRA[i] as u32)? as usize;
            let k = abst.lesen(b)? as usize;
            let abstand =
                *ABSTAND_BASIS.get(k)? as usize + b.bits(*ABSTAND_EXTRA.get(k)? as u32)? as usize;
            if abstand > aus.len() || aus.len() + laenge > GRENZE {
                return None;
            }
            let von = aus.len() - abstand;
            for j in 0..laenge {
                let c = aus[von + j];
                aus.push(c);
            }
        }
    }
}

fn fest() -> (Huffman, Huffman) {
    let mut l = [0u8; 288];
    l[..144].fill(8);
    l[144..256].fill(9);
    l[256..280].fill(7);
    l[280..].fill(8);
    // beide Tabellen sind vollständig und gültig
    let lit = Huffman::neu(&l).unwrap_or(Huffman {
        anzahl: [0; 16],
        zeichen: Vec::new(),
    });
    let abst = Huffman::neu(&[5u8; 30]).unwrap_or(Huffman {
        anzahl: [0; 16],
        zeichen: Vec::new(),
    });
    (lit, abst)
}

/// Kopf eines Blocks mit eigenen Tabellen lesen.
fn eigen(b: &mut Bits) -> Option<(Huffman, Huffman)> {
    let nlit = b.bits(5)? as usize + 257;
    let nabst = b.bits(5)? as usize + 1;
    let ncode = b.bits(4)? as usize + 4;
    if nlit > 286 || nabst > 30 {
        return None;
    }
    let mut cl = [0u8; 19];
    for &i in &REIHE[..ncode] {
        cl[i] = b.bits(3)? as u8;
    }
    let ch = Huffman::neu(&cl)?;
    let mut l = vec![0u8; nlit + nabst];
    let mut i = 0;
    while i < l.len() {
        let z = ch.lesen(b)?;
        let (wert, n) = match z {
            0..=15 => (z as u8, 1),
            16 => (*l.get(i.checked_sub(1)?)?, 3 + b.bits(2)? as usize),
            17 => (0, 3 + b.bits(3)? as usize),
            _ => (0, 11 + b.bits(7)? as usize),
        };
        if i + n > l.len() {
            return None;
        }
        l[i..i + n].fill(wert);
        i += n;
    }
    if l[256] == 0 {
        return None;
    }
    Some((Huffman::neu(&l[..nlit])?, Huffman::neu(&l[nlit..])?))
}

/// Entpackt in `aus`, bis zum letzten Block oder zum ersten Fehler; gibt
/// die Zahl der gelesenen Bytes bei Erfolg zurück. Was vor einem Fehler
/// entpackt wurde, bleibt in `aus`.
fn entpacken(d: &[u8], aus: &mut Vec<u8>) -> Option<usize> {
    let mut b = Bits::neu(d);
    loop {
        let letzter = b.bits(1)? == 1;
        match b.bits(2)? {
            0 => {
                b.byte_grenze();
                let p = b.byte_pos();
                let kopf = d.get(p..p + 4)?;
                let n = u16::from_le_bytes([kopf[0], kopf[1]]);
                let nn = u16::from_le_bytes([kopf[2], kopf[3]]);
                if n != !nn || aus.len() + n as usize > GRENZE {
                    return None;
                }
                aus.extend_from_slice(d.get(p + 4..p + 4 + n as usize)?);
                b.setze(p + 4 + n as usize);
            }
            1 => {
                let (lit, abst) = fest();
                block(&mut b, aus, &lit, &abst)?;
            }
            2 => {
                let (lit, abst) = eigen(&mut b)?;
                block(&mut b, aus, &lit, &abst)?;
            }
            _ => return None,
        }
        if letzter {
            b.byte_grenze();
            return Some(b.byte_pos());
        }
    }
}

/// Rohen Deflate-Strom entpacken (RFC 1951).
pub fn inflate(d: &[u8]) -> Option<Vec<u8>> {
    let mut aus = Vec::new();
    entpacken(d, &mut aus)?;
    Some(aus)
}

/// Prüfsumme Adler-32 (RFC 1950).
pub fn adler32(d: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for k in d.chunks(5552) {
        for &x in k {
            a += x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

/// Gültiger Zlib-Kopf: Deflate, Fenster bis 32 kB, Prüfzahl, kein
/// vorgegebenes Wörterbuch.
fn kopf(d: &[u8]) -> bool {
    d.len() >= 2
        && d[0] & 0x0F == 8
        && d[0] >> 4 <= 7
        && u16::from_be_bytes([d[0], d[1]]).is_multiple_of(31)
        && d[1] & 0x20 == 0
}

/// Zlib-Strom entpacken (RFC 1950): Kopf, Deflate, Adler-32 am Ende.
pub fn zlib(d: &[u8]) -> Option<Vec<u8>> {
    if !kopf(d) {
        return None;
    }
    let mut aus = Vec::new();
    let n = entpacken(&d[2..], &mut aus)?;
    let summe = d.get(2 + n..2 + n + 4)?;
    (u32::from_be_bytes([summe[0], summe[1], summe[2], summe[3]]) == adler32(&aus)).then_some(aus)
}

/// Wie ein PDF-Leser: Zlib-Kopf, wenn er da ist, sonst roh; was bis zu
/// einem Fehler entpackt wurde, zählt (abgeschnittene Ströme kommen vor).
/// `true`, wenn der Strom vollständig war.
pub fn nachsichtig(d: &[u8]) -> (Vec<u8>, bool) {
    let mut aus = Vec::new();
    let ganz = entpacken(if kopf(d) { &d[2..] } else { d }, &mut aus).is_some();
    (aus, ganz)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dieselben Testdaten wie `tests/daten/flate-*.z` (erzeugt mit Pythons
    /// zlib aus genau dieser Folge): Wörter nach einem linearen
    /// Kongruenzgenerator, dazwischen Zahlen wie in einem Inhaltsstrom.
    fn testtext(n: usize) -> Vec<u8> {
        const W: [&str; 12] = [
            "Grenze ",
            "Flurstück ",
            "45/6 ",
            "m ",
            "l ",
            "S\n",
            "q ",
            "Q\n",
            "0.5 w ",
            "Gebäude ",
            "re f\n",
            "BT ",
        ];
        let mut x: u32 = 12345;
        let mut aus = Vec::new();
        while aus.len() < n {
            x = x.wrapping_mul(1103515245).wrapping_add(12345) & 0x7fff_ffff;
            if x.is_multiple_of(3) {
                aus.extend_from_slice(format!("{}.{:02} ", x % 600, x % 97).as_bytes());
            } else {
                aus.extend_from_slice(W[(x >> 8) as usize % W.len()].as_bytes());
            }
        }
        aus.truncate(n);
        aus
    }

    #[test]
    fn python_zlib_stufen() {
        let soll = testtext(16_000);
        for (z, ohne_kopf) in [
            (&include_bytes!("../tests/daten/flate-0.z")[..], false),
            (&include_bytes!("../tests/daten/flate-1.z")[..], false),
            (&include_bytes!("../tests/daten/flate-6.z")[..], false),
            (&include_bytes!("../tests/daten/flate-9.z")[..], false),
            (&include_bytes!("../tests/daten/flate-9-roh.z")[..], true),
        ] {
            let ist = if ohne_kopf { inflate(z) } else { zlib(z) };
            assert_eq!(ist.as_deref(), Some(&soll[..]));
            assert_eq!(nachsichtig(z), (soll.clone(), true));
        }
        // kurzer Text: feste Tabellen
        let kurz = include_bytes!("../tests/daten/flate-fest.z");
        assert_eq!(kurz[2] & 0b110, 0b010, "Block mit festen Tabellen");
        assert_eq!(
            zlib(kurz).as_deref(),
            Some(&b"Grenze Grenze Grenze 45/6"[..])
        );
        // die großen Ströme haben eigene Tabellen
        let z6 = include_bytes!("../tests/daten/flate-6.z");
        assert_eq!(z6[2] & 0b110, 0b100, "Block mit eigenen Tabellen");
    }

    #[test]
    fn leer_und_gespeichert() {
        // zlib.compress(b"") und ein gespeicherter Block von Hand
        assert_eq!(
            zlib(&[0x78, 0x9c, 0x03, 0x00, 0, 0, 0, 1]),
            Some(Vec::new())
        );
        let roh = [1, 3, 0, 0xFC, 0xFF, b'a', b'b', b'c'];
        assert_eq!(inflate(&roh), Some(b"abc".to_vec()));
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn kaputte_daten_brechen_nicht_ab() {
        let z = include_bytes!("../tests/daten/flate-6.z");
        // abgeschnitten: streng None, nachsichtig der Anfang
        let halb = &z[..z.len() / 2];
        assert_eq!(zlib(halb), None);
        let (teil, ganz) = nachsichtig(halb);
        assert!(!ganz && teil.len() > 3_000);
        assert_eq!(&teil[..], &testtext(16_000)[..teil.len()]);
        // falsche Prüfsumme
        let mut f = z.to_vec();
        let n = f.len();
        f[n - 1] ^= 1;
        assert_eq!(zlib(&f), None);
        // jedes Byte einmal verändert: kein Abbruch
        let mut x: u32 = 7;
        for i in (0..z.len()).step_by(4) {
            let mut f = z.to_vec();
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            f[i] ^= (x >> 16) as u8 | 1;
            let _ = zlib(&f);
            let _ = nachsichtig(&f);
        }
        for f in [&[][..], &[0x78][..], &[0xFF; 40][..], &[0x07, 0, 0][..]] {
            assert_eq!(inflate(f), None);
            assert_eq!(zlib(f), None);
        }
    }
}

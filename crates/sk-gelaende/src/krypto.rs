//! Was der Standard-Sicherheitshandler des PDF braucht, als eigener Code
//! (plan-gelaendemodul.md §5 G1c, Technik): MD5 (RFC 1321), RC4, SHA-256,
//! SHA-384 und SHA-512 (FIPS 180-2) und AES mit 128, 192 und 256 Bit
//! (FIPS 197) samt CBC (SP 800-38A). Nur zum Lesen geschützter Karten
//! gedacht, nicht gegen Seitenkanäle gehärtet.

/// MD5 eines Puffers.
pub fn md5(d: &[u8]) -> [u8; 16] {
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    const R: [u32; 16] = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
    let mut h: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    for b in polster(d, 64, false).chunks_exact(64) {
        let m: Vec<u32> = b
            .chunks_exact(4)
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .collect();
        let [mut a, mut bb, mut c, mut dd] = h;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((bb & c) | (!bb & dd), i),
                1 => ((dd & bb) | (!dd & c), (5 * i + 1) % 16),
                2 => (bb ^ c ^ dd, (3 * i + 5) % 16),
                _ => (c ^ (bb | !dd), (7 * i) % 16),
            };
            let t = dd;
            dd = c;
            c = bb;
            bb = bb.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(K[i])
                    .wrapping_add(m[g])
                    .rotate_left(R[(i / 16) * 4 + i % 4]),
            );
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, bb, c, dd]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut aus = [0u8; 16];
    for (i, w) in h.iter().enumerate() {
        aus[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    aus
}

/// Nachricht auffüllen: 0x80, Nullen, Länge in Bits (MD5 little endian,
/// SHA big endian) in den letzten 8 bzw. 16 Bytes des Blocks.
fn polster(d: &[u8], block: usize, gross: bool) -> Vec<u8> {
    let laenge = block / 8;
    let mut v = d.to_vec();
    v.push(0x80);
    while v.len() % block != block - laenge {
        v.push(0);
    }
    let bits = (d.len() as u128) * 8;
    if gross {
        v.extend_from_slice(&bits.to_be_bytes()[16 - laenge..]);
    } else {
        v.extend_from_slice(&bits.to_le_bytes()[..laenge]);
    }
    v
}

/// RC4: Schlüsselstrom auf die Daten (ver- und entschlüsseln ist dasselbe).
pub fn rc4(schluessel: &[u8], d: &[u8]) -> Vec<u8> {
    if schluessel.is_empty() {
        return d.to_vec();
    }
    let mut s: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut j = 0u8;
    for i in 0..256 {
        j = j
            .wrapping_add(s[i])
            .wrapping_add(schluessel[i % schluessel.len()]);
        s.swap(i, j as usize);
    }
    let (mut i, mut j) = (0u8, 0u8);
    d.iter()
        .map(|&b| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            b ^ s[s[i as usize].wrapping_add(s[j as usize]) as usize]
        })
        .collect()
}

/// SHA-256 eines Puffers.
pub fn sha256(d: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    for b in polster(d, 64, true).chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, c) in b.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let [a, b, c, d, e, f, g, hh] = v;
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g];
        }
        for (x, y) in h.iter_mut().zip(v) {
            *x = x.wrapping_add(y);
        }
    }
    let mut aus = [0u8; 32];
    for (i, w) in h.iter().enumerate() {
        aus[4 * i..4 * i + 4].copy_from_slice(&w.to_be_bytes());
    }
    aus
}

/// SHA-512 mit Anfangswerten `h`, ganzes Ergebnis.
fn sha512_mit(d: &[u8], mut h: [u64; 8]) -> [u8; 64] {
    #[rustfmt::skip]
    const K: [u64; 80] = [
        0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
        0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
        0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
        0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
        0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
        0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
        0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
        0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
        0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
        0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
        0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
        0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
        0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
        0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
        0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
        0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
        0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
        0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
        0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
        0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
    ];
    for b in polster(d, 128, true).chunks_exact(128) {
        let mut w = [0u64; 80];
        for (i, c) in b.chunks_exact(8).enumerate() {
            w[i] = u64::from_be_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..80 {
            let [a, b, c, d, e, f, g, hh] = v;
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g];
        }
        for (x, y) in h.iter_mut().zip(v) {
            *x = x.wrapping_add(y);
        }
    }
    let mut aus = [0u8; 64];
    for (i, w) in h.iter().enumerate() {
        aus[8 * i..8 * i + 8].copy_from_slice(&w.to_be_bytes());
    }
    aus
}

/// SHA-512 eines Puffers.
pub fn sha512(d: &[u8]) -> [u8; 64] {
    sha512_mit(
        d,
        [
            0x6a09e667f3bcc908,
            0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b,
            0xa54ff53a5f1d36f1,
            0x510e527fade682d1,
            0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b,
            0x5be0cd19137e2179,
        ],
    )
}

/// SHA-384 eines Puffers.
pub fn sha384(d: &[u8]) -> [u8; 48] {
    let h = sha512_mit(
        d,
        [
            0xcbbb9d5dc1059ed8,
            0x629a292a367cd507,
            0x9159015a3070dd17,
            0x152fecd8f70e5939,
            0x67332667ffc00b31,
            0x8eb44a8768581511,
            0xdb0c2e0d64f98fa7,
            0x47b5481dbefa4fa4,
        ],
    );
    let mut aus = [0u8; 48];
    aus.copy_from_slice(&h[..48]);
    aus
}

/// Multiplikation mit x in GF(2⁸).
fn xmal(b: u8) -> u8 {
    (b << 1) ^ if b & 0x80 != 0 { 0x1b } else { 0 }
}

/// Multiplikation in GF(2⁸).
fn mal(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0;
    while b != 0 {
        if b & 1 != 0 {
            p ^= a;
        }
        a = xmal(a);
        b >>= 1;
    }
    p
}

/// AES-Block-Chiffre mit fertigen Rundenschlüsseln.
pub struct Aes {
    runden: Vec<[u8; 16]>,
    s: [u8; 256],
    si: [u8; 256],
}

impl Aes {
    /// Schlüssel mit 16, 24 oder 32 Bytes, sonst `None`.
    pub fn neu(schluessel: &[u8]) -> Option<Aes> {
        let nk = schluessel.len() / 4;
        if !matches!(schluessel.len(), 16 | 24 | 32) {
            return None;
        }
        // S-Box: Inverses in GF(2⁸), dann die affine Abbildung
        let mut s = [0u8; 256];
        let (mut p, mut q) = (1u8, 1u8);
        loop {
            p ^= xmal(p);
            q ^= q << 1;
            q ^= q << 2;
            q ^= q << 4;
            if q & 0x80 != 0 {
                q ^= 0x09;
            }
            let x = q ^ q.rotate_left(1) ^ q.rotate_left(2) ^ q.rotate_left(3) ^ q.rotate_left(4);
            s[p as usize] = x ^ 0x63;
            if p == 1 {
                break;
            }
        }
        s[0] = 0x63;
        let mut si = [0u8; 256];
        for (i, &v) in s.iter().enumerate() {
            si[v as usize] = i as u8;
        }
        let nr = nk + 6;
        let mut w: Vec<[u8; 4]> = schluessel
            .chunks_exact(4)
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect();
        let mut rcon = 1u8;
        for i in nk..4 * (nr + 1) {
            let mut t = w[i - 1];
            if i % nk == 0 {
                t = [
                    s[t[1] as usize] ^ rcon,
                    s[t[2] as usize],
                    s[t[3] as usize],
                    s[t[0] as usize],
                ];
                rcon = xmal(rcon);
            } else if nk > 6 && i % nk == 4 {
                t = t.map(|b| s[b as usize]);
            }
            let v = w[i - nk];
            w.push([v[0] ^ t[0], v[1] ^ t[1], v[2] ^ t[2], v[3] ^ t[3]]);
        }
        let runden = w
            .chunks_exact(4)
            .map(|k| std::array::from_fn(|i| k[i / 4][i % 4]))
            .collect();
        Some(Aes { runden, s, si })
    }

    /// Einen Block verschlüsseln.
    pub fn verschluesseln(&self, b: &mut [u8; 16]) {
        let n = self.runden.len() - 1;
        xor(b, &self.runden[0]);
        for r in 1..=n {
            for x in b.iter_mut() {
                *x = self.s[*x as usize];
            }
            // Zeile r rückt um r Spalten nach links
            let a = *b;
            for (i, x) in b.iter_mut().enumerate() {
                let (z, sp) = (i % 4, i / 4);
                *x = a[z + 4 * ((sp + z) % 4)];
            }
            if r < n {
                for c in b.chunks_exact_mut(4) {
                    let a = [c[0], c[1], c[2], c[3]];
                    c[0] = xmal(a[0]) ^ mal(a[1], 3) ^ a[2] ^ a[3];
                    c[1] = a[0] ^ xmal(a[1]) ^ mal(a[2], 3) ^ a[3];
                    c[2] = a[0] ^ a[1] ^ xmal(a[2]) ^ mal(a[3], 3);
                    c[3] = mal(a[0], 3) ^ a[1] ^ a[2] ^ xmal(a[3]);
                }
            }
            xor(b, &self.runden[r]);
        }
    }

    /// Einen Block entschlüsseln.
    pub fn entschluesseln(&self, b: &mut [u8; 16]) {
        let n = self.runden.len() - 1;
        xor(b, &self.runden[n]);
        for r in (0..n).rev() {
            let a = *b;
            for (i, &v) in a.iter().enumerate() {
                let (z, sp) = (i % 4, i / 4);
                b[z + 4 * ((sp + z) % 4)] = self.si[v as usize];
            }
            xor(b, &self.runden[r]);
            if r > 0 {
                for c in b.chunks_exact_mut(4) {
                    let a = [c[0], c[1], c[2], c[3]];
                    c[0] = mal(a[0], 14) ^ mal(a[1], 11) ^ mal(a[2], 13) ^ mal(a[3], 9);
                    c[1] = mal(a[0], 9) ^ mal(a[1], 14) ^ mal(a[2], 11) ^ mal(a[3], 13);
                    c[2] = mal(a[0], 13) ^ mal(a[1], 9) ^ mal(a[2], 14) ^ mal(a[3], 11);
                    c[3] = mal(a[0], 11) ^ mal(a[1], 13) ^ mal(a[2], 9) ^ mal(a[3], 14);
                }
            }
        }
    }
}

fn xor(b: &mut [u8; 16], k: &[u8; 16]) {
    for (x, y) in b.iter_mut().zip(k) {
        *x ^= y;
    }
}

/// AES-CBC entschlüsseln, ohne Auffüllung zu entfernen; `None` bei falscher
/// Schlüssellänge oder wenn die Länge kein Vielfaches von 16 ist.
pub fn cbc_entschluesseln(schluessel: &[u8], iv: &[u8; 16], d: &[u8]) -> Option<Vec<u8>> {
    let aes = Aes::neu(schluessel)?;
    if !d.len().is_multiple_of(16) {
        return None;
    }
    let mut vor = *iv;
    let mut aus = Vec::with_capacity(d.len());
    for c in d.chunks_exact(16) {
        let mut b: [u8; 16] = std::array::from_fn(|i| c[i]);
        aes.entschluesseln(&mut b);
        xor(&mut b, &vor);
        aus.extend_from_slice(&b);
        vor = std::array::from_fn(|i| c[i]);
    }
    Some(aus)
}

/// AES-CBC verschlüsseln, ohne Auffüllung (für den Kennwort-Hash von R6).
pub fn cbc_verschluesseln(schluessel: &[u8], iv: &[u8; 16], d: &[u8]) -> Option<Vec<u8>> {
    let aes = Aes::neu(schluessel)?;
    if !d.len().is_multiple_of(16) {
        return None;
    }
    let mut vor = *iv;
    let mut aus = Vec::with_capacity(d.len());
    for c in d.chunks_exact(16) {
        let mut b: [u8; 16] = std::array::from_fn(|i| c[i]);
        xor(&mut b, &vor);
        aes.verschluesseln(&mut b);
        aus.extend_from_slice(&b);
        vor = b;
    }
    Some(aus)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let s: Vec<u8> = s.bytes().filter(|b| b.is_ascii_hexdigit()).collect();
        s.chunks(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn md5_rfc1321() {
        for (t, h) in [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("a", "0cc175b9c0f1b6a831c399e269772661"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                "abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
            (
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "d174ab98d277d9f5a5611c2c9f419d9f",
            ),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ] {
            assert_eq!(md5(t.as_bytes()).to_vec(), hex(h), "{t}");
        }
    }

    #[test]
    fn sha2_fips180() {
        let abc = b"abc";
        let zwei = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(
            sha256(abc).to_vec(),
            hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(
            sha256(zwei).to_vec(),
            hex("248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1")
        );
        assert_eq!(
            sha256(&vec![b'a'; 1_000_000]).to_vec(),
            hex("cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0")
        );
        assert_eq!(
            sha512(abc).to_vec(),
            hex(
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a
                 2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
            )
        );
        let lang = b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu";
        assert_eq!(
            sha512(lang).to_vec(),
            hex(
                "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa17299aeadb6889018
                 501d289e4900f7e4331b99dec4b5433ac7d329eeb6dd26545e96e55b874be909"
            )
        );
        assert_eq!(
            sha384(abc).to_vec(),
            hex("cb00753f45a35e8bb5a03d699ac65007272c32ab0eded163
                 1a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7")
        );
        assert_eq!(
            sha384(lang).to_vec(),
            hex("09330c33f71147e83d192fc782cd1b4753111b173b3b05d2
                 2fa08086e3b0f712fcc7c71a557e2db966c3e9fa91746039")
        );
    }

    #[test]
    fn rc4_bekannte_werte() {
        assert_eq!(rc4(b"Key", b"Plaintext"), hex("BBF316E8D940AF0AD3"));
        assert_eq!(rc4(b"Wiki", b"pedia"), hex("1021BF0420"));
        assert_eq!(
            rc4(b"Secret", b"Attack at dawn"),
            hex("45A01F645FC35B383552544B9BF5")
        );
        // RFC 6229, 40-Bit-Schlüssel 0102030405, die ersten 16 Bytes
        assert_eq!(
            rc4(&hex("0102030405"), &[0; 16]),
            hex("b2396305f03dc027ccc3524a0a1118a8")
        );
        assert_eq!(rc4(b"", b"abc"), b"abc".to_vec());
    }

    #[test]
    fn aes_fips197_und_cbc() {
        let klar = hex("00112233445566778899aabbccddeeff");
        for (k, c) in [
            (
                "000102030405060708090a0b0c0d0e0f",
                "69c4e0d86a7b0430d8cdb78070b4c55a",
            ),
            (
                "000102030405060708090a0b0c0d0e0f1011121314151617",
                "dda97ca4864cdfe06eaf70a0ec0d7191",
            ),
            (
                "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
                "8ea2b7ca516745bfeafc49904b496089",
            ),
        ] {
            let aes = Aes::neu(&hex(k)).unwrap();
            let mut b: [u8; 16] = klar.clone().try_into().unwrap();
            aes.verschluesseln(&mut b);
            assert_eq!(b.to_vec(), hex(c));
            aes.entschluesseln(&mut b);
            assert_eq!(b.to_vec(), klar);
        }
        assert!(Aes::neu(&[0; 15]).is_none());
        // SP 800-38A F.2.1 bis F.2.6
        let iv: [u8; 16] = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
        let klar = hex(
            "6bc1bee22e409f96e93d7e117393172a ae2d8a571e03ac9c9eb76fac45af8e51
             30c81c46a35ce411e5fbc1191a0a52ef f69f2445df4f9b17ad2b417be66c3710",
        );
        for (k, c) in [
            (
                "2b7e151628aed2a6abf7158809cf4f3c",
                "7649abac8119b246cee98e9b12e9197d 5086cb9b507219ee95db113a917678b2
                 73bed6b8e3c1743b7116e69e22229516 3ff1caa1681fac09120eca307586e1a7",
            ),
            (
                "603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4",
                "f58c4c04d6e5f1ba779eabfb5f7bfbd6 9cfc4e967edb808d679f777bc6702c7d
                 39f23369a9d9bacfa530e26304231461 b2eb05e2c39be9fcda6c19078c6a9d1b",
            ),
        ] {
            assert_eq!(cbc_verschluesseln(&hex(k), &iv, &klar), Some(hex(c)));
            assert_eq!(
                cbc_entschluesseln(&hex(k), &iv, &hex(c)),
                Some(klar.clone())
            );
        }
        assert_eq!(cbc_entschluesseln(&[0; 16], &iv, &[0; 15]), None);
    }
}

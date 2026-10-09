//! SHA-256 nach FIPS 180-4, HMAC (RFC 2104) und PBKDF2 (RFC 8018), eigene
//! Umsetzung (paket-ka3b KA-3b1, Review 3at): Prüfwert des
//! Verwaltungskennworts. Schutz vor Versehen, keine Sicherheit
//! (Bausteingrenze Fassung 2 §6.2). Liegt in sk-model, weil auch die
//! abgeleiteten Kennungen der Erweiterungen sie brauchen (E8c).

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

fn block(h: &mut [u32; 8], b: &[u8]) {
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
    let [mut a, mut bb, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & bb) ^ (a & c) ^ (bb & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = bb;
        bb = a;
        a = t1.wrapping_add(t2);
    }
    for (x, y) in h.iter_mut().zip([a, bb, c, d, e, f, g, hh]) {
        *x = x.wrapping_add(y);
    }
}

/// Prüfsumme von `daten` (32 Bytes).
pub fn sha256(daten: &[u8]) -> [u8; 32] {
    weiter(H0, 0, daten)
}

/// SHA-256 ab dem Zwischenstand `h` nach `vorher` Bytes (ganze Blöcke).
fn weiter(mut h: [u32; 8], vorher: u64, daten: &[u8]) -> [u8; 32] {
    let mut rest = daten.chunks_exact(64);
    for b in rest.by_ref() {
        block(&mut h, b);
    }
    let r = rest.remainder();
    let mut letzt = [0u8; 128];
    letzt[..r.len()].copy_from_slice(r);
    letzt[r.len()] = 0x80;
    let n = if r.len() < 56 { 64 } else { 128 };
    let bits = (vorher + daten.len() as u64).wrapping_mul(8);
    letzt[n - 8..n].copy_from_slice(&bits.to_be_bytes());
    for b in letzt[..n].chunks_exact(64) {
        block(&mut h, b);
    }
    let mut out = [0u8; 32];
    for (o, x) in out.chunks_exact_mut(4).zip(h) {
        o.copy_from_slice(&x.to_be_bytes());
    }
    out
}

/// HMAC-SHA256 (RFC 2104) mit vorgerechneten Zwischenständen für den
/// inneren und äußeren Block, damit PBKDF2 je Runde nur zwei Blöcke rechnet.
#[derive(Clone)]
pub struct Hmac {
    innen: [u32; 8],
    aussen: [u32; 8],
}

impl Hmac {
    pub fn neu(schluessel: &[u8]) -> Hmac {
        let mut k = [0u8; 64];
        if schluessel.len() > 64 {
            k[..32].copy_from_slice(&sha256(schluessel));
        } else {
            k[..schluessel.len()].copy_from_slice(schluessel);
        }
        let (mut innen, mut aussen) = (H0, H0);
        block(&mut innen, &k.map(|b| b ^ 0x36));
        block(&mut aussen, &k.map(|b| b ^ 0x5c));
        Hmac { innen, aussen }
    }

    pub fn rechnen(&self, daten: &[u8]) -> [u8; 32] {
        let innen = weiter(self.innen, 64, daten);
        weiter(self.aussen, 64, &innen)
    }
}

/// Inhalt bleibt verborgen (Schlüssel ist ein Kennwort).
impl std::fmt::Debug for Hmac {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Hmac(…)")
    }
}

/// HMAC-SHA256 von `daten` mit `schluessel`.
pub fn hmac(schluessel: &[u8], daten: &[u8]) -> [u8; 32] {
    Hmac::neu(schluessel).rechnen(daten)
}

/// PBKDF2-HMAC-SHA256 (RFC 8018 §5.2) in `aus`.
pub fn pbkdf2(kennwort: &[u8], salz: &[u8], runden: u32, aus: &mut [u8]) {
    let mac = Hmac::neu(kennwort);
    for (i, teil) in aus.chunks_mut(32).enumerate() {
        let mut s = salz.to_vec();
        s.extend_from_slice(&(i as u32 + 1).to_be_bytes());
        let mut u = mac.rechnen(&s);
        let mut t = u;
        for _ in 1..runden {
            u = mac.rechnen(&u);
            for (x, y) in t.iter_mut().zip(u) {
                *x ^= y;
            }
        }
        teil.copy_from_slice(&t[..teil.len()]);
    }
}

/// Vergleich in konstanter Zeit: läuft über alle Bytes, auch nach dem
/// ersten Unterschied.
pub fn gleich(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let d = a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y));
    std::hint::black_box(d) == 0
}

/// Bytes als Hexziffern, klein.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hexziffern als Bytes; `None` bei ungerader Länge oder fremdem Zeichen.
pub fn aus_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prüfvektoren aus FIPS 180-4 (NIST, Beispiele SHA-256) und die
    /// Blockgrenzen 55/56/64 Bytes.
    #[test]
    fn pruefvektoren() {
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            hex(&sha256(b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu")),
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
        );
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&sha256(&million)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        assert_eq!(
            hex(&sha256(&[b'a'; 55])),
            "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"
        );
        assert_eq!(
            hex(&sha256(&[b'a'; 56])),
            "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"
        );
        assert_eq!(
            hex(&sha256(&[b'a'; 64])),
            "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb"
        );
    }

    /// RFC 4231, Fälle 1–4, 6 und 7 (Fall 5 kürzt die Ausgabe).
    #[test]
    fn hmac_rfc4231() {
        let fall = |k: &[u8], d: &[u8], soll: &str| assert_eq!(hex(&hmac(k, d)), soll);
        fall(
            &[0x0b; 20],
            b"Hi There",
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
        );
        fall(
            b"Jefe",
            b"what do ya want for nothing?",
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
        );
        fall(
            &[0xaa; 20],
            &[0xdd; 50],
            "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe",
        );
        let k: Vec<u8> = (1..=25).collect();
        fall(
            &k,
            &[0xcd; 50],
            "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b",
        );
        fall(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First",
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54",
        );
        fall(
            &[0xaa; 131],
            b"This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm.",
            "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2",
        );
    }

    /// RFC 7914 §11 (PBKDF2-HMAC-SHA256, 64 Bytes).
    #[test]
    fn pbkdf2_rfc7914() {
        let mut aus = [0u8; 64];
        pbkdf2(b"passwd", b"salt", 1, &mut aus);
        assert_eq!(hex(&aus), "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc49ca9cccf179b645991664b39d77ef317c71b845b1e30bd509112041d3a19783");
        pbkdf2(b"Password", b"NaCl", 80_000, &mut aus);
        assert_eq!(hex(&aus), "4ddcd8f60b98be21830cee5ef22701f9641a4418d04c0414aeff08876b34ab56a1d425a1225833549adb841b51c9b3176a272bdebba1d078478f62b397f33c8d");
    }

    #[test]
    fn vergleich_und_hex() {
        assert!(gleich(b"abc", b"abc"));
        assert!(!gleich(b"abc", b"abd"));
        assert!(!gleich(b"abc", b"ab"));
        assert_eq!(aus_hex("00ff7a"), Some(vec![0, 255, 0x7a]));
        assert_eq!(aus_hex("0"), None);
        assert_eq!(aus_hex("zz"), None);
        assert_eq!(aus_hex("ä1"), None);
        assert!(format!("{:?}", Hmac::neu(b"geheim")).ends_with("(…)"));
    }
}

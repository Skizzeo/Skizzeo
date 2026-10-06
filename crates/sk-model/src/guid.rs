//! Dauerhafte Kennung eines Bauteils (128 Bit), gleich in Datei und IFC-Export.
//!
//! Textform wie in IFC: 22 Zeichen aus einem eigenen 64er-Alphabet, das erste
//! Zeichen trägt die obersten 2 Bit, jedes weitere 6 Bit.

use std::fmt;

/// Alphabet der IFC-Kurzform (IfcGloballyUniqueId).
const IFC_DIGITS: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Guid(pub u128);

impl Guid {
    /// IFC-Kurzform mit 22 Zeichen.
    pub fn to_ifc(self) -> String {
        (0..22)
            .map(|i| {
                // Das erste Zeichen trägt nur die obersten 2 Bit
                let v = if i == 0 {
                    self.0 >> 126
                } else {
                    (self.0 >> (126 - 6 * i)) & 0x3f
                };
                IFC_DIGITS[v as usize] as char
            })
            .collect()
    }

    /// Liest die IFC-Kurzform.
    pub fn from_ifc(s: &str) -> Option<Guid> {
        let b = s.as_bytes();
        if b.len() != 22 {
            return None;
        }
        let mut n: u128 = 0;
        for (i, c) in b.iter().enumerate() {
            let v = IFC_DIGITS.iter().position(|d| d == c)? as u128;
            if i == 0 && v > 3 {
                return None;
            }
            n = (n << if i == 0 { 2 } else { 6 }) | v;
        }
        Some(Guid(n))
    }

    /// Übliche Schreibweise 8-4-4-4-12.
    pub fn to_uuid(self) -> String {
        let h = format!("{:032x}", self.0);
        format!(
            "{}-{}-{}-{}-{}",
            &h[0..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..32]
        )
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_ifc())
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Guid({})", self.to_ifc())
    }
}

/// Erzeuger für Zufalls-Guids (Version 4). Startwert aus Uhrzeit und Prozess,
/// danach ein Zähler, der durch SplitMix64 gemischt wird.
#[derive(Clone, Debug)]
pub struct GuidGen {
    state: u64,
}

impl GuidGen {
    /// Startwert aus Uhrzeit (Nanosekunden) und Prozessnummer.
    pub fn from_time() -> GuidGen {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        let pid = std::process::id() as u64;
        GuidGen::with_seed(t ^ pid.rotate_left(32))
    }

    /// Fester Startwert (für Tests).
    pub fn with_seed(seed: u64) -> GuidGen {
        GuidGen { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        // SplitMix64
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub fn next_guid(&mut self) -> Guid {
        let n = ((self.next_u64() as u128) << 64) | self.next_u64() as u128;
        // Version 4 (zufällig), Variante RFC 4122
        let n = (n & !(0xf << 76)) | (0x4 << 76);
        let n = (n & !(0x3 << 62)) | (0x2 << 62);
        Guid(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ifc_kurzform() {
        assert_eq!(Guid(0).to_ifc(), "0000000000000000000000");
        assert_eq!(Guid(u128::MAX).to_ifc(), "3$$$$$$$$$$$$$$$$$$$$$");
        assert_eq!(Guid(1).to_ifc(), "0000000000000000000001");
        assert_eq!(Guid(64).to_ifc(), "0000000000000000000010");
        let mut g = GuidGen::with_seed(7);
        for _ in 0..100 {
            let id = g.next_guid();
            let s = id.to_ifc();
            assert_eq!(s.len(), 22);
            assert_eq!(Guid::from_ifc(&s), Some(id));
        }
        assert_eq!(Guid::from_ifc("4000000000000000000000"), None);
        assert_eq!(Guid::from_ifc("000"), None);
    }

    #[test]
    fn version_und_eindeutigkeit() {
        let mut g = GuidGen::with_seed(1);
        let ids: Vec<Guid> = (0..1000).map(|_| g.next_guid()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
        let u = ids[0].to_uuid();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert!("89ab".contains(&u[19..20]));
    }
}

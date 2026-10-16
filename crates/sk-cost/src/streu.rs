//! Tabellen über Schlüssel aus der eigenen Rechnung (Ids, Guids, Fächer der
//! Kostenrechnung), mit schnellem Streuwert statt des gegen Angriffe
//! gehärteten (wie `sk_math::flaeche`). Keine Ausgabe hängt an der
//! Reihenfolge einer solchen Tabelle.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

pub(crate) type Tafel<K, V> = HashMap<K, V, BuildHasherDefault<Streu>>;
pub(crate) type Menge<K> = HashSet<K, BuildHasherDefault<Streu>>;

/// Streuwert für [`Tafel`]: je Zahl drehen, mischen, mit einer ungeraden
/// Konstanten malnehmen; am Ende gedreht, damit die unteren Bits (der Platz
/// in der Tabelle) von allen Bits abhängen.
#[derive(Default)]
pub(crate) struct Streu(u64);

impl Streu {
    fn dazu(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for Streu {
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut stuecke = bytes.chunks_exact(8);
        for s in &mut stuecke {
            self.dazu(u64::from_le_bytes(s.try_into().expect("8 Bytes")));
        }
        for &b in stuecke.remainder() {
            self.dazu(u64::from(b));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.dazu(u64::from(n));
    }

    fn write_u16(&mut self, n: u16) {
        self.dazu(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.dazu(u64::from(n));
    }

    fn write_u64(&mut self, n: u64) {
        self.dazu(n);
    }

    fn write_u128(&mut self, n: u128) {
        self.dazu(n as u64);
        self.dazu((n >> 64) as u64);
    }

    fn write_usize(&mut self, n: usize) {
        self.dazu(n as u64);
    }

    fn write_i64(&mut self, n: i64) {
        self.dazu(n as u64);
    }

    fn write_isize(&mut self, n: isize) {
        self.dazu(n as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::{BuildHasher, Hash};

    fn streu<T: Hash>(x: T) -> u64 {
        BuildHasherDefault::<Streu>::default().hash_one(x)
    }

    /// Guids, die sich nur in einer Hälfte unterscheiden, streuen
    /// verschieden; ebenso die Lage eines Bytes in einem Text.
    #[test]
    fn streut_beide_haelften_und_text() {
        assert_ne!(streu(1u128), streu(1u128 << 64));
        assert_ne!(streu("ab"), streu("ba"));
        assert_ne!(
            streu((Some(1u32), None::<u32>)),
            streu((None::<u32>, Some(1u32)))
        );
    }
}

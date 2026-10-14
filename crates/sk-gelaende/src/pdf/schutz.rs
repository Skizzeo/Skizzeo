//! Standard-Sicherheitshandler (ISO 32000-1 §7.6.3, ISO 32000-2 §7.6.4):
//! RC4 mit 40 bis 128 Bit (V 1/2, R 2 bis 4), AESV2 (V 4) und AESV3
//! (V 5, R 5 und 6). Skizzeo öffnet nur mit leerem Benutzerkennwort
//! (plan-gelaendemodul.md §5 G1c); ist eines gesetzt, sagt es das in einem
//! Satz.

use super::objekt::{Objekt, Verz};
use crate::krypto::{cbc_entschluesseln, cbc_verschluesseln, md5, rc4, sha256, sha384, sha512};

/// Satz, wenn die Datei ein Öffnungskennwort hat.
pub const KENNWORT: &str = "Die Datei ist mit Kennwort geschützt.";

/// Auffüllung des Kennworts (Algorithmus 2, Schritt a).
const POLSTER: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// Verfahren für Texte oder Ströme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verfahren {
    Ohne,
    Rc4,
    Aes128,
    Aes256,
}

/// Geöffneter Schutz: Dateischlüssel und Verfahren.
#[derive(Clone, Debug)]
pub struct Schutz {
    schluessel: Vec<u8>,
    pub stroeme: Verfahren,
    pub texte: Verfahren,
    /// Metadatenströme sind mit verschlüsselt.
    pub metadaten: bool,
    pub v: i64,
    pub r: i64,
}

fn bytes(v: &Verz, k: &str) -> Vec<u8> {
    v.get(k)
        .and_then(Objekt::text)
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
}

/// Verfahren eines Crypt-Filters aus `/CF` (V 4 und 5).
fn crypt_filter(enc: &Verz, name: Option<&str>) -> Option<Verfahren> {
    let name = name.unwrap_or("Identity");
    if name == "Identity" {
        return Some(Verfahren::Ohne);
    }
    let cf = enc.get("CF")?.verz()?.get(name)?.verz()?;
    match cf.name("CFM").unwrap_or("None") {
        "None" => Some(Verfahren::Ohne),
        "V2" => Some(Verfahren::Rc4),
        "AESV2" => Some(Verfahren::Aes128),
        "AESV3" => Some(Verfahren::Aes256),
        _ => None,
    }
}

impl Schutz {
    /// Schutz aus dem `/Encrypt`-Verzeichnis und dem ersten Teil der
    /// Datei-ID öffnen.
    pub fn neu(enc: &Verz, id0: &[u8]) -> Result<Schutz, String> {
        let filter = enc.name("Filter").unwrap_or("Standard");
        if filter != "Standard" {
            return Err(format!(
                "Die Datei ist mit dem Verfahren {filter} verschlüsselt; Skizzeo liest nur den Standard-Schutz ohne Kennwort."
            ));
        }
        let v = enc.ganz("V").unwrap_or(0);
        let r = enc.ganz("R").unwrap_or(0);
        let unbekannt =
            || format!("Die Verschlüsselung der Datei (V {v}, R {r}) kennt Skizzeo nicht.");
        let metadaten = !matches!(enc.get("EncryptMetadata"), Some(Objekt::Bool(false)));
        let (stroeme, texte) = match v {
            1 | 2 => (Verfahren::Rc4, Verfahren::Rc4),
            4 | 5 => (
                crypt_filter(enc, enc.name("StmF")).ok_or_else(unbekannt)?,
                crypt_filter(enc, enc.name("StrF")).ok_or_else(unbekannt)?,
            ),
            _ => return Err(unbekannt()),
        };
        let o = bytes(enc, "O");
        let u = bytes(enc, "U");
        let schluessel = match r {
            2..=4 => {
                let n = if r == 2 {
                    5
                } else if v >= 4 {
                    16
                } else {
                    (enc.ganz("Length").unwrap_or(40) / 8).clamp(5, 16) as usize
                };
                let p = enc.ganz("P").unwrap_or(0) as i32;
                let mut x = POLSTER.to_vec();
                x.extend_from_slice(&o[..o.len().min(32)]);
                x.extend_from_slice(&p.to_le_bytes());
                x.extend_from_slice(id0);
                if r >= 4 && !metadaten {
                    x.extend_from_slice(&[0xFF; 4]);
                }
                let mut h = md5(&x);
                if r >= 3 {
                    for _ in 0..50 {
                        h = md5(&h[..n]);
                    }
                }
                let k = h[..n].to_vec();
                // Benutzerkennwort leer? (Algorithmus 4 und 5)
                let passt = if r == 2 {
                    u.len() >= 32 && rc4(&k, &POLSTER) == u[..32]
                } else {
                    let mut x = POLSTER.to_vec();
                    x.extend_from_slice(id0);
                    let mut c = rc4(&k, &md5(&x));
                    for i in 1..=19u8 {
                        let ki: Vec<u8> = k.iter().map(|b| b ^ i).collect();
                        c = rc4(&ki, &c);
                    }
                    u.len() >= 16 && c[..] == u[..16]
                };
                if !passt {
                    return Err(KENNWORT.into());
                }
                k
            }
            5 | 6 => {
                if u.len() < 48 {
                    return Err(unbekannt());
                }
                let hash = |salz: &[u8]| {
                    if r == 5 {
                        sha256(salz).to_vec()
                    } else {
                        hash_2b(b"", salz)
                    }
                };
                if hash(&u[32..40])[..32] != u[..32] {
                    return Err(KENNWORT.into());
                }
                let ue = bytes(enc, "UE");
                let k = hash(&u[40..48]);
                cbc_entschluesseln(&k[..32], &[0; 16], &ue[..ue.len().min(32)])
                    .filter(|k| k.len() == 32)
                    .ok_or_else(unbekannt)?
            }
            _ => return Err(unbekannt()),
        };
        Ok(Schutz {
            schluessel,
            stroeme,
            texte,
            metadaten,
            v,
            r,
        })
    }

    /// Schlüssel für ein Objekt (Algorithmus 1); AES-256 nimmt den
    /// Dateischlüssel selbst.
    fn objektschluessel(&self, nr: u32, gen: u16, aes: bool) -> Vec<u8> {
        let mut x = self.schluessel.clone();
        x.extend_from_slice(&nr.to_le_bytes()[..3]);
        x.extend_from_slice(&gen.to_le_bytes());
        if aes {
            x.extend_from_slice(b"sAlT");
        }
        let n = (self.schluessel.len() + 5).min(16);
        md5(&x)[..n].to_vec()
    }

    fn mit(&self, art: Verfahren, nr: u32, gen: u16, d: &[u8]) -> Vec<u8> {
        match art {
            Verfahren::Ohne => d.to_vec(),
            Verfahren::Rc4 => rc4(&self.objektschluessel(nr, gen, false), d),
            Verfahren::Aes128 => aes(&self.objektschluessel(nr, gen, true), d),
            Verfahren::Aes256 => aes(&self.schluessel, d),
        }
    }

    /// Text des Objekts `nr gen` entschlüsseln.
    pub fn text(&self, nr: u32, gen: u16, d: &[u8]) -> Vec<u8> {
        self.mit(self.texte, nr, gen, d)
    }

    /// Strom des Objekts `nr gen` entschlüsseln.
    pub fn strom(&self, nr: u32, gen: u16, d: &[u8]) -> Vec<u8> {
        self.mit(self.stroeme, nr, gen, d)
    }

    /// Alle Texte in einem Objekt entschlüsseln (Ströme nicht).
    pub fn texte_in(&self, nr: u32, gen: u16, o: &mut Objekt) {
        match o {
            Objekt::Text(t) => *t = self.text(nr, gen, t),
            Objekt::Feld(f) => f.iter_mut().for_each(|x| self.texte_in(nr, gen, x)),
            Objekt::Verz(v) => v.0.iter_mut().for_each(|(_, x)| self.texte_in(nr, gen, x)),
            Objekt::Strom(s) => s
                .verz
                .0
                .iter_mut()
                .for_each(|(_, x)| self.texte_in(nr, gen, x)),
            _ => {}
        }
    }
}

/// AES-CBC mit dem IV in den ersten 16 Bytes, Auffüllung nach PKCS#7
/// entfernt; was nicht aufgeht, wird abgeschnitten statt abgewiesen.
fn aes(k: &[u8], d: &[u8]) -> Vec<u8> {
    if d.len() < 32 {
        return Vec::new();
    }
    let iv: [u8; 16] = std::array::from_fn(|i| d[i]);
    let n = (d.len() - 16) / 16 * 16;
    let mut aus = cbc_entschluesseln(k, &iv, &d[16..16 + n]).unwrap_or_default();
    if let Some(&p) = aus.last() {
        let p = p as usize;
        if (1..=16).contains(&p)
            && aus.len() >= p
            && aus[aus.len() - p..].iter().all(|&b| b as usize == p)
        {
            aus.truncate(aus.len() - p);
        }
    }
    aus
}

/// Kennwort-Hash für R 6 (ISO 32000-2, Algorithmus 2.B), hier für das
/// Benutzerkennwort (ohne U-Daten).
pub fn hash_2b(kennwort: &[u8], salz: &[u8]) -> Vec<u8> {
    let mut x = kennwort.to_vec();
    x.extend_from_slice(salz);
    let mut k = sha256(&x).to_vec();
    let mut runde = 0u32;
    loop {
        let mut einmal = kennwort.to_vec();
        einmal.extend_from_slice(&k);
        let k1 = einmal.repeat(64);
        let iv: [u8; 16] = std::array::from_fn(|i| k[16 + i]);
        let Some(e) = cbc_verschluesseln(&k[..16], &iv, &k1) else {
            return k;
        };
        let summe: u32 = e[..16].iter().map(|&b| b as u32).sum();
        k = match summe % 3 {
            0 => sha256(&e).to_vec(),
            1 => sha384(&e).to_vec(),
            _ => sha512(&e).to_vec(),
        };
        runde += 1;
        if runde >= 64 && e.last().is_some_and(|&b| b as u32 + 32 <= runde) {
            break;
        }
    }
    k.truncate(32);
    k
}

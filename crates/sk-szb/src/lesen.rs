//! Zeilenleser SZB 0 (Vertrag §4): Kopfzeile `SZB 0`, danach je Zeile
//! `[abschnitt] schluessel=wert …`, Texte in `"…"` mit `\"` und `\\`,
//! `#` bis Zeilenende ist Kommentar.

use crate::Befund;

/// Ein Datensatz: Felder in Dateireihenfolge und die Zeilennummer (ab 1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Satz {
    pub zeile: usize,
    pub felder: Vec<(String, String)>,
}

impl Satz {
    /// Wert des Felds `k`, wenn vorhanden.
    pub fn get(&self, k: &str) -> Option<&str> {
        self.felder
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    }

    pub fn hat(&self, k: &str) -> bool {
        self.get(k).is_some()
    }

    /// `k=ja`
    pub fn ja(&self, k: &str) -> bool {
        self.get(k) == Some("ja")
    }

    /// Der Schlüssel (`key`) oder leer.
    pub fn key(&self) -> &str {
        self.get("key").unwrap_or("")
    }
}

/// Die Abschnitte eines Bauteils in Vertragsreihenfolge.
pub const ABSCHNITTE: [&str; 12] = [
    "bauteil",
    "bedienung",
    "hoehe",
    "param",
    "typ",
    "wert",
    "baustoff",
    "koerper",
    "artikel",
    "leistung",
    "menge",
    "notiz",
];

/// Ein gelesenes Bauteil: je Abschnitt die Sätze in Dateireihenfolge.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Def {
    pub bauteil: Vec<Satz>,
    pub bedienung: Vec<Satz>,
    pub hoehe: Vec<Satz>,
    pub param: Vec<Satz>,
    pub typ: Vec<Satz>,
    pub wert: Vec<Satz>,
    pub baustoff: Vec<Satz>,
    pub koerper: Vec<Satz>,
    pub artikel: Vec<Satz>,
    pub leistung: Vec<Satz>,
    pub menge: Vec<Satz>,
    pub notiz: Vec<Satz>,
}

impl Def {
    /// Die Sätze des Abschnitts `name`, `None` für einen unbekannten.
    pub fn abschnitt(&self, name: &str) -> Option<&Vec<Satz>> {
        Some(match name {
            "bauteil" => &self.bauteil,
            "bedienung" => &self.bedienung,
            "hoehe" => &self.hoehe,
            "param" => &self.param,
            "typ" => &self.typ,
            "wert" => &self.wert,
            "baustoff" => &self.baustoff,
            "koerper" => &self.koerper,
            "artikel" => &self.artikel,
            "leistung" => &self.leistung,
            "menge" => &self.menge,
            "notiz" => &self.notiz,
            _ => return None,
        })
    }

    fn abschnitt_mut(&mut self, name: &str) -> Option<&mut Vec<Satz>> {
        Some(match name {
            "bauteil" => &mut self.bauteil,
            "bedienung" => &mut self.bedienung,
            "hoehe" => &mut self.hoehe,
            "param" => &mut self.param,
            "typ" => &mut self.typ,
            "wert" => &mut self.wert,
            "baustoff" => &mut self.baustoff,
            "koerper" => &mut self.koerper,
            "artikel" => &mut self.artikel,
            "leistung" => &mut self.leistung,
            "menge" => &mut self.menge,
            "notiz" => &mut self.notiz,
            _ => return None,
        })
    }

    /// Feld `k` aus `[bauteil]`.
    pub fn bauteil_feld(&self, k: &str) -> Option<&str> {
        self.bauteil.first().and_then(|s| s.get(k))
    }

    /// Feld `k` aus `[bedienung]`.
    pub fn bedienung_feld(&self, k: &str) -> Option<&str> {
        self.bedienung.first().and_then(|s| s.get(k))
    }

    /// Der `[param]` mit Schlüssel `key`.
    pub fn param_von(&self, key: &str) -> Option<&Satz> {
        self.param.iter().find(|s| s.key() == key)
    }
}

/// Größte Datei (Bytes); größere werden nicht gelesen.
pub const MAX_DATEI: usize = 1 << 20;
/// Längste Zeile (Zeichen).
pub const MAX_ZEILE: usize = 20_000;
/// Höchstzahl der Sätze je Abschnitt; `[param]` weniger, weil die
/// Grenzprüfung je Parameter zweimal alles rechnet.
pub fn max_saetze(sec: &str) -> usize {
    match sec {
        "bauteil" | "bedienung" | "hoehe" => 16,
        "param" | "typ" => 64,
        _ => 256,
    }
}

/// Liest den Text eines Bauteils. Syntaxfehler und unbekannte Abschnitte
/// werden Befunde; was lesbar ist, steht trotzdem in [`Def`]. Ein
/// Byte-Order-Mark und `\r` am Zeilenende gelten als Leerraum wie in der
/// Werkbank.
pub fn lesen(text: &str) -> (Def, Vec<Befund>) {
    let mut def = Def::default();
    let mut bef = Vec::new();
    if text.len() > MAX_DATEI {
        bef.push(Befund::fehler(
            0,
            format!("Datei größer als {} KB", MAX_DATEI / 1024),
        ));
        return (def, bef);
    }
    let mut kopf = false;
    for (ix, zeile) in text.split('\n').enumerate() {
        let nr = ix + 1;
        let zeile = zeile.trim_start_matches('\u{feff}');
        if zeile.chars().count() > MAX_ZEILE {
            bef.push(Befund::fehler(
                nr,
                format!("Zeile länger als {MAX_ZEILE} Zeichen"),
            ));
            continue;
        }
        let s = zeile.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        if !kopf {
            kopf = true;
            if ist_kopf(s) {
                continue;
            }
            bef.push(Befund::fehler(
                nr,
                "Kopfzeile „SZB 0“ fehlt oder steht nicht am Anfang",
            ));
            if s.starts_with("SZB") {
                continue;
            }
        }
        match satz(zeile) {
            Ok(None) => {}
            Ok(Some((sec, felder))) => match def.abschnitt_mut(&sec) {
                Some(v) if v.len() >= max_saetze(&sec) => bef.push(Befund::fehler(
                    nr,
                    format!("mehr als {} Sätze [{sec}]", max_saetze(&sec)),
                )),
                Some(v) => v.push(Satz { zeile: nr, felder }),
                None => bef.push(Befund::fehler(nr, format!("unbekannter Abschnitt [{sec}]"))),
            },
            Err(e) => bef.push(Befund::fehler(nr, e)),
        }
    }
    if !kopf {
        bef.push(Befund::fehler(0, "Kopfzeile „SZB 0“ fehlt"));
    }
    (def, bef)
}

/// `SZB` und `0`, getrennt durch Leerraum.
fn ist_kopf(s: &str) -> bool {
    let mut t = s.split_whitespace();
    s.starts_with("SZB") && t.next() == Some("SZB") && t.next() == Some("0") && t.next().is_none()
}

type Felder = Vec<(String, String)>;

/// Eine Zeile `[abschnitt] k=v …`; `None` für Kommentar oder Leerzeile.
fn satz(zeile: &str) -> Result<Option<(String, Felder)>, String> {
    let c: Vec<char> = zeile.chars().collect();
    let n = c.len();
    let mut i = 0;
    let ws = |i: &mut usize| {
        while *i < n && c[*i].is_whitespace() {
            *i += 1;
        }
    };
    ws(&mut i);
    if i >= n || c[i] == '#' {
        return Ok(None);
    }
    if c[i] != '[' {
        return Err("Zeile muss mit [abschnitt] beginnen".into());
    }
    let Some(j) = (i..n).find(|&k| c[k] == ']') else {
        return Err("„]“ fehlt".into());
    };
    let sec: String = c[i + 1..j].iter().collect::<String>().trim().to_string();
    i = j + 1;
    let mut felder: Felder = Vec::new();
    loop {
        ws(&mut i);
        if i >= n || c[i] == '#' {
            break;
        }
        let k0 = i;
        while i < n && c[i] != '=' && !c[i].is_whitespace() {
            i += 1;
        }
        let key: String = c[k0..i].iter().collect();
        if i >= n || c[i] != '=' {
            return Err(format!("„{key}“: erwartet schluessel=wert"));
        }
        i += 1;
        let mut v = String::new();
        if i < n && c[i] == '"' {
            i += 1;
            let mut zu = false;
            while i < n {
                match c[i] {
                    '\\' => {
                        match c.get(i + 1) {
                            Some('n') => v.push('\n'),
                            Some(&x) => v.push(x),
                            None => {}
                        }
                        i += 2;
                    }
                    '"' => {
                        i += 1;
                        zu = true;
                        break;
                    }
                    x => {
                        v.push(x);
                        i += 1;
                    }
                }
            }
            if !zu {
                return Err(format!("„{key}“: schließendes \" fehlt"));
            }
        } else {
            let v0 = i;
            while i < n && !c[i].is_whitespace() {
                i += 1;
            }
            v = c[v0..i].iter().collect();
        }
        if felder.iter().any(|(k, _)| *k == key) {
            return Err(format!("„{key}“ doppelt"));
        }
        felder.push((key, v));
    }
    Ok(Some((sec, felder)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeilen_und_texte() {
        let (d, b) = lesen(
            "# Kommentar\nSZB 0\n[bauteil] key=a.b name=\"Stütze \\\"rund\\\"\" # Ende\n\n[param] key=x wert=1\n",
        );
        assert!(b.is_empty(), "{b:?}");
        assert_eq!(d.bauteil[0].get("name"), Some("Stütze \"rund\""));
        assert_eq!(d.bauteil[0].zeile, 3);
        assert_eq!(d.param[0].key(), "x");
        assert_eq!(d.param[0].zeile, 5);
    }

    #[test]
    fn fehler_der_zeilen() {
        let (_, b) = lesen("SZB 0\nbauteil key=a\n[x] a=1\n[param] key=a key=b\n[param] name=\"offen\n[param] key\n[param\n");
        let t: Vec<_> = b.iter().map(|b| (b.zeile, b.text.as_str())).collect();
        assert_eq!(
            t,
            [
                (2, "Zeile muss mit [abschnitt] beginnen"),
                (3, "unbekannter Abschnitt [x]"),
                (4, "„key“ doppelt"),
                (5, "„name“: schließendes \" fehlt"),
                (6, "„key“: erwartet schluessel=wert"),
                (7, "„]“ fehlt"),
            ]
        );
        assert!(b.iter().all(Befund::ist_fehler));
    }

    #[test]
    fn kopfzeile() {
        let (_, b) = lesen("[bauteil] key=a.b\n");
        assert_eq!(
            b[0].text,
            "Kopfzeile „SZB 0“ fehlt oder steht nicht am Anfang"
        );
        let (_, b) = lesen("SZB 1\n[bauteil] key=a.b\n");
        assert_eq!(b.len(), 1);
        let (_, b) = lesen("  # nur Kommentar\n");
        assert_eq!(b, [Befund::fehler(0, "Kopfzeile „SZB 0“ fehlt")]);
        let (_, b) = lesen("SZB   0\r\n[bauteil] key=a.b\r\n");
        assert!(b.is_empty(), "{b:?}");
    }
}

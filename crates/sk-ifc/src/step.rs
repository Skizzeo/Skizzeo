//! Austauschdatei nach ISO 10303-21 (STEP-Klartext), in der IFC geschrieben
//! wird: Kopf, Datensätze `#n=NAME(…);`, Werte, Zeichenketten mit
//! `\X2\…\X0\` für alles außer druckbarem ASCII. Schreiben und Lesen stehen
//! hier, damit beide dieselben Werte kennen.

use std::fmt::Write as _;

/// Ein Wert in einem Datensatz.
#[derive(Clone, Debug, PartialEq)]
pub enum V {
    /// Verweis `#n`.
    Ref(u32),
    /// Zeichenkette.
    Str(String),
    /// Länge in Millimetern, auf 0,0001 mm gerundet (Koordinaten, Dicken).
    Len(f64),
    /// Zahl mit voller Genauigkeit.
    Real(f64),
    Int(i64),
    /// Aufzählung `.NAME.` (ohne Punkte).
    Enum(String),
    /// Wahrheitswert `.T.`/`.F.`.
    Bool(bool),
    /// Logischer Wert unbekannt `.U.`.
    Unknown,
    /// Nicht gesetzt `$`.
    Null,
    /// Abgeleitet `*`.
    Derived,
    List(Vec<V>),
    /// Getypter Wert, z. B. `IFCLABEL('x')`.
    Typed(String, Box<V>),
}

/// Zeichenkette.
pub fn s(t: &str) -> V {
    V::Str(t.to_string())
}

/// Aufzählung.
pub fn e(t: &str) -> V {
    V::Enum(t.to_string())
}

/// Verweis.
pub fn r(id: u32) -> V {
    V::Ref(id)
}

/// Liste von Verweisen.
pub fn refs(ids: &[u32]) -> V {
    V::List(ids.iter().map(|&i| V::Ref(i)).collect())
}

/// Liste von Längen.
pub fn lens(xs: &[f64]) -> V {
    V::List(xs.iter().map(|&x| V::Len(x)).collect())
}

/// Getypter Wert.
pub fn typed(t: &str, v: V) -> V {
    V::Typed(t.to_string(), Box::new(v))
}

/// Zahl im STEP-Format: immer mit Dezimalpunkt, nie `-0`.
fn real(out: &mut String, x: f64) {
    let x = if x == 0.0 || !x.is_finite() { 0.0 } else { x };
    let t = format!("{x}");
    out.push_str(&t);
    if !t.contains(['.', 'e', 'E']) {
        out.push('.');
    }
}

/// Zeichenkette nach ISO 10303-21: `'` und `\` verdoppelt, alles außer
/// druckbarem ASCII als `\X2\` (UTF-16, je Zeichen vier Hexziffern) bzw.
/// `\X4\` jenseits der Grundebene.
pub fn encode(out: &mut String, t: &str) {
    out.push('\'');
    let mut wide = false;
    for c in t.chars() {
        let printable = (' '..='~').contains(&c);
        if printable {
            if wide {
                out.push_str("\\X0\\");
                wide = false;
            }
            match c {
                '\'' => out.push_str("''"),
                '\\' => out.push_str("\\\\"),
                c => out.push(c),
            }
        } else if (c as u32) > 0xffff {
            if wide {
                out.push_str("\\X0\\");
                wide = false;
            }
            let _ = write!(out, "\\X4\\{:08X}\\X0\\", c as u32);
        } else {
            if !wide {
                out.push_str("\\X2\\");
                wide = true;
            }
            let _ = write!(out, "{:04X}", c as u32);
        }
    }
    if wide {
        out.push_str("\\X0\\");
    }
    out.push('\'');
}

fn value(out: &mut String, v: &V) {
    match v {
        V::Ref(i) => {
            let _ = write!(out, "#{i}");
        }
        V::Str(t) => encode(out, t),
        V::Len(x) => real(out, (x * 1e4).round() / 1e4),
        V::Real(x) => real(out, *x),
        V::Int(i) => {
            let _ = write!(out, "{i}");
        }
        V::Enum(t) => {
            let _ = write!(out, ".{t}.");
        }
        V::Bool(b) => out.push_str(if *b { ".T." } else { ".F." }),
        V::Unknown => out.push_str(".U."),
        V::Null => out.push('$'),
        V::Derived => out.push('*'),
        V::List(xs) => {
            out.push('(');
            for (k, x) in xs.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                }
                value(out, x);
            }
            out.push(')');
        }
        V::Typed(t, x) => {
            out.push_str(t);
            out.push('(');
            value(out, x);
            out.push(')');
        }
    }
}

/// Schreibt Datensätze mit fortlaufender Nummer ab 1.
#[derive(Default)]
pub struct Writer {
    body: String,
    next: u32,
}

impl Writer {
    pub fn new() -> Writer {
        Writer::default()
    }

    /// Hängt `NAME(args)` an und gibt seine Nummer zurück.
    pub fn add(&mut self, name: &str, args: Vec<V>) -> u32 {
        self.next += 1;
        let _ = write!(self.body, "#{}={}(", self.next, name);
        for (k, a) in args.iter().enumerate() {
            if k > 0 {
                self.body.push(',');
            }
            value(&mut self.body, a);
        }
        self.body.push_str(");\n");
        self.next
    }

    /// Anzahl der Datensätze.
    pub fn count(&self) -> u32 {
        self.next
    }

    /// Die ganze Datei mit Kopf. `header` sind die drei Kopfsätze
    /// `FILE_DESCRIPTION`, `FILE_NAME`, `FILE_SCHEMA` mit ihren Werten.
    pub fn finish(self, header: &[(&str, Vec<V>)]) -> String {
        let mut out = String::with_capacity(self.body.len() + 512);
        out.push_str("ISO-10303-21;\nHEADER;\n");
        for (name, args) in header {
            out.push_str(name);
            value(&mut out, &V::List(args.clone()));
            out.push_str(";\n");
        }
        out.push_str("ENDSEC;\nDATA;\n");
        out.push_str(&self.body);
        out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
        out
    }
}

/// Ein gelesener Datensatz.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    /// Name in Großbuchstaben, z. B. `IFCWALL`.
    pub name: String,
    pub args: Vec<V>,
}

/// Eine gelesene Datei: Kopfsätze und Datensätze nach Nummer.
#[derive(Clone, Debug, Default)]
pub struct File {
    pub header: Vec<Entity>,
    pub data: std::collections::BTreeMap<u32, Entity>,
    /// Nummern, die mehr als einmal vorkommen.
    pub twice: Vec<u32>,
}

impl File {
    pub fn get(&self, id: u32) -> Option<&Entity> {
        self.data.get(&id)
    }

    /// Alle Datensätze eines Namens (Großbuchstaben), nach Nummer.
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = (u32, &'a Entity)> + 'a {
        self.data
            .iter()
            .filter(move |(_, e)| e.name == name)
            .map(|(i, e)| (*i, e))
    }

    /// Schema aus `FILE_SCHEMA`, z. B. „IFC4“.
    pub fn schema(&self) -> Option<&str> {
        let h = self.header.iter().find(|h| h.name == "FILE_SCHEMA")?;
        match h.args.first()? {
            V::List(xs) => match xs.first()? {
                V::Str(t) => Some(t),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Lesefehler mit Stelle (Byte) und Grund.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadError {
    pub at: usize,
    pub what: &'static str,
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn err<T>(&self, what: &'static str) -> Result<T, ReadError> {
        Err(ReadError { at: self.i, what })
    }

    /// Überspringt Leerraum und Kommentare `/* … */`.
    fn ws(&mut self) {
        loop {
            while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            if self.b[self.i..].starts_with(b"/*") {
                match find(&self.b[self.i + 2..], b"*/") {
                    Some(k) => self.i += k + 4,
                    None => self.i = self.b.len(),
                }
            } else {
                return;
            }
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.b.get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> Result<(), ReadError> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            self.err("Zeichen erwartet")
        }
    }

    fn word(&mut self) -> String {
        self.ws();
        let s = self.i;
        while self.i < self.b.len()
            && (self.b[self.i].is_ascii_alphanumeric() || matches!(self.b[self.i], b'_' | b'-'))
        {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.b[s..self.i]).to_ascii_uppercase()
    }

    fn number(&mut self) -> Result<V, ReadError> {
        let s = self.i;
        while self.i < self.b.len()
            && matches!(
                self.b[self.i],
                b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'
            )
        {
            self.i += 1;
        }
        let t = std::str::from_utf8(&self.b[s..self.i]).unwrap_or("");
        if t.contains(['.', 'e', 'E']) {
            t.parse().map(V::Real).or_else(|_| self.err("Zahl"))
        } else {
            t.parse().map(V::Int).or_else(|_| self.err("Zahl"))
        }
    }

    fn string(&mut self) -> Result<V, ReadError> {
        self.i += 1;
        let mut raw = Vec::new();
        loop {
            match self.b.get(self.i) {
                None => return self.err("Zeichenkette offen"),
                Some(b'\'') if self.b.get(self.i + 1) == Some(&b'\'') => {
                    raw.push(b'\'');
                    self.i += 2;
                }
                Some(b'\'') => {
                    self.i += 1;
                    break;
                }
                Some(&c) => {
                    raw.push(c);
                    self.i += 1;
                }
            }
        }
        Ok(V::Str(decode(&raw)))
    }

    fn value(&mut self) -> Result<V, ReadError> {
        match self.peek() {
            Some(b'#') => {
                self.i += 1;
                match self.number()? {
                    V::Int(n) if n > 0 && n <= u32::MAX as i64 => Ok(V::Ref(n as u32)),
                    _ => self.err("Verweis"),
                }
            }
            Some(b'\'') => self.string(),
            Some(b'$') => {
                self.i += 1;
                Ok(V::Null)
            }
            Some(b'*') => {
                self.i += 1;
                Ok(V::Derived)
            }
            Some(b'.') => {
                self.i += 1;
                let w = self.word();
                self.eat(b'.')?;
                Ok(match w.as_str() {
                    "T" => V::Bool(true),
                    "F" => V::Bool(false),
                    "U" => V::Unknown,
                    _ => V::Enum(w),
                })
            }
            Some(b'(') => Ok(V::List(self.list()?)),
            Some(b'"') => {
                // Binärwert: als Zeichenkette ohne Deutung
                self.i += 1;
                let s = self.i;
                while self.i < self.b.len() && self.b[self.i] != b'"' {
                    self.i += 1;
                }
                let t = String::from_utf8_lossy(&self.b[s..self.i]).into_owned();
                self.i += 1;
                Ok(V::Str(t))
            }
            Some(c) if c == b'-' || c == b'+' || c.is_ascii_digit() => self.number(),
            Some(c) if c.is_ascii_alphabetic() => {
                let w = self.word();
                let mut args = self.list()?;
                if args.len() == 1 {
                    Ok(V::Typed(w, Box::new(args.remove(0))))
                } else {
                    Ok(V::Typed(w, Box::new(V::List(args))))
                }
            }
            _ => self.err("Wert erwartet"),
        }
    }

    fn list(&mut self) -> Result<Vec<V>, ReadError> {
        self.eat(b'(')?;
        let mut out = Vec::new();
        if self.peek() == Some(b')') {
            self.i += 1;
            return Ok(out);
        }
        loop {
            out.push(self.value()?);
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b')') => {
                    self.i += 1;
                    return Ok(out);
                }
                _ => return self.err("Liste"),
            }
        }
    }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

/// Deutet die Steuerfolgen einer Zeichenkette (`\X2\`, `\X4\`, `\X\`,
/// `\\`); unbekannte bleiben stehen.
fn decode(raw: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    let hex = |s: &[u8]| u32::from_str_radix(std::str::from_utf8(s).unwrap_or("x"), 16).ok();
    while i < raw.len() {
        if raw[i..].starts_with(b"\\X2\\") || raw[i..].starts_with(b"\\X4\\") {
            let w = if raw[i + 2] == b'2' { 4 } else { 8 };
            i += 4;
            let mut units: Vec<u16> = Vec::new();
            while i + w <= raw.len() && !raw[i..].starts_with(b"\\X0\\") {
                let Some(v) = hex(&raw[i..i + w]) else { break };
                if w == 4 {
                    units.push(v as u16);
                } else if let Some(c) = char::from_u32(v) {
                    let mut buf = [0u16; 2];
                    units.extend_from_slice(c.encode_utf16(&mut buf));
                }
                i += w;
            }
            out.push_str(&String::from_utf16_lossy(&units));
            if raw[i..].starts_with(b"\\X0\\") {
                i += 4;
            }
        } else if raw[i..].starts_with(b"\\X\\") && i + 5 <= raw.len() {
            if let Some(c) = hex(&raw[i + 3..i + 5]).and_then(char::from_u32) {
                out.push(c);
            }
            i += 5;
        } else if raw[i..].starts_with(b"\\\\") {
            out.push('\\');
            i += 2;
        } else {
            // UTF-8 von Programmen, die nicht kodieren, unverändert lesen
            let n = utf8_len(raw[i]);
            out.push_str(&String::from_utf8_lossy(&raw[i..(i + n).min(raw.len())]));
            i += n;
        }
    }
    out
}

fn utf8_len(b: u8) -> usize {
    match b {
        0xf0..=0xff => 4,
        0xe0..=0xef => 3,
        0xc0..=0xdf => 2,
        _ => 1,
    }
}

/// Liest eine STEP-Datei (Kopf und Datenteil).
pub fn read(text: &str) -> Result<File, ReadError> {
    let mut p = Parser {
        b: text.as_bytes(),
        i: 0,
    };
    let mut f = File::default();
    if p.word() != "ISO-10303-21" {
        return p.err("Kopf ISO-10303-21 fehlt");
    }
    p.eat(b';')?;
    let mut in_data = false;
    loop {
        match p.peek() {
            None => return p.err("Dateiende ohne END-ISO-10303-21"),
            Some(b'#') if in_data => {
                p.i += 1;
                let id = match p.number()? {
                    V::Int(n) if n > 0 && n <= u32::MAX as i64 => n as u32,
                    _ => return p.err("Nummer"),
                };
                p.eat(b'=')?;
                let name = p.word();
                if name.is_empty() {
                    return p.err("Name fehlt");
                }
                let args = p.list()?;
                p.eat(b';')?;
                if f.data.insert(id, Entity { name, args }).is_some() {
                    f.twice.push(id);
                }
            }
            _ => {
                let w = p.word();
                match w.as_str() {
                    "HEADER" => p.eat(b';')?,
                    "DATA" => {
                        if p.peek() == Some(b'(') {
                            p.list()?;
                        }
                        p.eat(b';')?;
                        in_data = true;
                    }
                    "ENDSEC" => {
                        p.eat(b';')?;
                        in_data = false;
                    }
                    "END-ISO-10303-21" => return Ok(f),
                    "" => return p.err("unerwartetes Zeichen"),
                    _ if !in_data => {
                        let args = p.list()?;
                        p.eat(b';')?;
                        f.header.push(Entity { name: w, args });
                    }
                    _ => return p.err("Datensatz ohne Nummer"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(t: &str) -> String {
        let mut o = String::new();
        encode(&mut o, t);
        o
    }

    #[test]
    fn umlaute_und_zeichen() {
        assert_eq!(enc("Dämmung"), "'D\\X2\\00E4\\X0\\mmung'");
        assert_eq!(enc("Außenwand"), "'Au\\X2\\00DF\\X0\\enwand'");
        assert_eq!(enc("±0,00"), "'\\X2\\00B1\\X0\\0,00'");
        assert_eq!(enc("O'Neil \\ a"), "'O''Neil \\\\ a'");
        assert_eq!(enc("Größe"), "'Gr\\X2\\00F600DF\\X0\\e'");
        for t in [
            "Dämmung",
            "O'Neil \\ a",
            "λ = 0,035 W/(m·K)",
            "Ziegel 🧱",
            "",
        ] {
            let mut w = Writer::new();
            w.add("IFCLABEL", vec![s(t)]);
            let f = read(&w.finish(&[])).unwrap();
            assert_eq!(f.get(1).unwrap().args[0], s(t), "{t}");
        }
    }

    #[test]
    fn zahlen() {
        let z = |v: V| {
            let mut o = String::new();
            value(&mut o, &v);
            o
        };
        assert_eq!(z(V::Len(3000.0)), "3000.");
        assert_eq!(z(V::Len(2999.99999999)), "3000.");
        assert_eq!(z(V::Len(-0.00001)), "0.");
        assert_eq!(z(V::Len(12.34567)), "12.3457");
        assert_eq!(z(V::Real(0.035)), "0.035");
        assert_eq!(z(V::Real(1e-5)), "0.00001");
        assert_eq!(z(V::Int(-3)), "-3");
    }

    #[test]
    fn lesen_und_schreiben() {
        let mut w = Writer::new();
        let a = w.add("IFCCARTESIANPOINT", vec![lens(&[0.0, 1.5, -2.0])]);
        let b = w.add(
            "IFCWALL",
            vec![
                s("0abc"),
                V::Null,
                s("AW-001"),
                V::Derived,
                r(a),
                e("STANDARD"),
                V::Bool(true),
                V::Unknown,
                typed("IFCLABEL", s("x")),
                V::List(vec![]),
                V::Int(7),
            ],
        );
        let t = w.finish(&[("FILE_SCHEMA", vec![V::List(vec![s("IFC4")])])]);
        let f = read(&t).unwrap();
        assert_eq!(f.schema(), Some("IFC4"));
        assert_eq!(f.data.len(), 2);
        let wall = f.get(b).unwrap();
        assert_eq!(wall.name, "IFCWALL");
        assert_eq!(wall.args[4], V::Ref(a));
        assert_eq!(wall.args[5], e("STANDARD"));
        assert_eq!(wall.args[8], typed("IFCLABEL", s("x")));
        assert_eq!(
            f.get(a).unwrap().args[0],
            V::List(vec![V::Real(0.0), V::Real(1.5), V::Real(-2.0)])
        );
    }

    #[test]
    fn fremde_schreibweisen() {
        let t = "ISO-10303-21;\nHEADER;FILE_SCHEMA(('IFC2X3'));ENDSEC;\nDATA;\n\
                 /* Kommentar */ #1= IFCPROPERTYSINGLEVALUE('A',$,IFCREAL(1.E-3),$);\n\
                 #2=IFCLABEL('Gr\\X\\F6\\X\\DFe');\n#2=IFCLABEL('x');\nENDSEC;\nEND-ISO-10303-21;";
        let f = read(t).unwrap();
        assert_eq!(f.schema(), Some("IFC2X3"));
        assert_eq!(f.get(1).unwrap().args[2], typed("IFCREAL", V::Real(0.001)));
        assert_eq!(f.twice, vec![2]);
        let g = read(&t.replace("#2=IFCLABEL('x');\n", "")).unwrap();
        assert_eq!(g.get(2).unwrap().args[0], s("Größe"));
        assert!(read("ISO-10303-21;\nDATA;\n#1=X(;").is_err());
    }
}

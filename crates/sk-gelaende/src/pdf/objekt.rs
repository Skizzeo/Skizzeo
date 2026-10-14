//! PDF-Objekte und ihr Leser (ISO 32000-1 §7.2 und §7.3): Zahlen, Texte
//! mit Escapes und in Hex, Namen mit `#xx`, Felder, Verzeichnisse, Bezüge.
//! Derselbe Leser zerlegt den Inhaltsstrom einer Seite in Operanden und
//! Befehle.

use std::cell::Cell;

/// Ein PDF-Objekt. Ein Strom trägt seine Bytes so, wie sie in der Datei
/// stehen (nur entschlüsselt); die Filter löst [`super::Pdf::daten`].
#[derive(Clone, Debug, PartialEq)]
pub enum Objekt {
    Null,
    Bool(bool),
    Ganz(i64),
    Zahl(f64),
    /// Bytes eines Textes, Escapes und Hex aufgelöst.
    Text(Vec<u8>),
    /// Name ohne `/`, `#xx` aufgelöst, je Byte ein Zeichen (Latin-1).
    Name(String),
    Feld(Vec<Objekt>),
    Verz(Verz),
    Strom(Strom),
    Bezug(u32, u16),
}

/// Verzeichnis (Dictionary) in der Reihenfolge der Datei.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Verz(pub Vec<(String, Objekt)>);

/// Strom: Verzeichnis und Rohdaten.
#[derive(Clone, Debug, PartialEq)]
pub struct Strom {
    pub verz: Verz,
    pub daten: Vec<u8>,
}

impl Verz {
    /// Wert zu einem Schlüssel (ohne `/`).
    pub fn get(&self, k: &str) -> Option<&Objekt> {
        self.0.iter().find(|(s, _)| s == k).map(|(_, o)| o)
    }

    /// Name zu einem Schlüssel.
    pub fn name(&self, k: &str) -> Option<&str> {
        self.get(k)?.name()
    }

    /// Ganze Zahl zu einem Schlüssel.
    pub fn ganz(&self, k: &str) -> Option<i64> {
        self.get(k)?.ganz()
    }

    /// Zahl zu einem Schlüssel.
    pub fn zahl(&self, k: &str) -> Option<f64> {
        self.get(k)?.zahl()
    }

    /// Wert setzen oder ersetzen.
    pub fn setzen(&mut self, k: &str, o: Objekt) {
        match self.0.iter_mut().find(|(s, _)| s == k) {
            Some(e) => e.1 = o,
            None => self.0.push((k.to_string(), o)),
        }
    }
}

impl Objekt {
    pub fn name(&self) -> Option<&str> {
        match self {
            Objekt::Name(n) => Some(n),
            _ => None,
        }
    }

    pub fn zahl(&self) -> Option<f64> {
        match *self {
            Objekt::Ganz(i) => Some(i as f64),
            Objekt::Zahl(z) => Some(z),
            _ => None,
        }
    }

    /// Ganze Zahl; eine Zahl mit Komma nur, wenn sie ganz ist.
    pub fn ganz(&self) -> Option<i64> {
        match *self {
            Objekt::Ganz(i) => Some(i),
            Objekt::Zahl(z) if z.fract() == 0.0 && z.abs() < 9e15 => Some(z as i64),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&[u8]> {
        match self {
            Objekt::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn feld(&self) -> Option<&[Objekt]> {
        match self {
            Objekt::Feld(f) => Some(f),
            _ => None,
        }
    }

    /// Verzeichnis, bei einem Strom das des Stroms.
    pub fn verz(&self) -> Option<&Verz> {
        match self {
            Objekt::Verz(v) => Some(v),
            Objekt::Strom(s) => Some(&s.verz),
            _ => None,
        }
    }

    pub fn strom(&self) -> Option<&Strom> {
        match self {
            Objekt::Strom(s) => Some(s),
            _ => None,
        }
    }
}

/// Was der Leser als Nächstes sieht.
#[derive(Clone, Debug, PartialEq)]
pub enum Wort<'a> {
    Objekt(Objekt),
    /// Ein Schlüsselwort: Befehl im Inhaltsstrom, `obj`, `stream`, `R` …
    Befehl(&'a [u8]),
    FeldZu,
    VerzZu,
}

/// Tiefer verschachtelte Felder und Verzeichnisse liest Skizzeo nicht.
const TIEFE: usize = 64;

/// So viele Bytes kostet ein Element eines Felds oder Verzeichnisses im
/// Speicher höchstens. „0 “ sind zwei Bytes in der Datei, als Element ein
/// Vielfaches; darum zählt das [`Budget`] jedes gelesene Element mit
/// (Review #128 N1).
pub const ELEMENT: usize = std::mem::size_of::<(String, Objekt)>();

/// Ohne [`Budget`] liest ein [`Leser`] höchstens so viele Elemente.
const OHNE_BUDGET: usize = 1 << 16;

/// Was eine Datei noch kosten darf, in Bytes: entpackte und kopierte
/// Ströme, gelesene Objekte und ihre Elemente (Review W37 und #128 N1).
/// Alles, was aus derselben Datei liest, teilt es.
#[derive(Debug)]
pub struct Budget {
    rest: Cell<usize>,
    voll: Cell<bool>,
}

impl Budget {
    pub fn neu(n: usize) -> Self {
        Budget {
            rest: Cell::new(n),
            voll: Cell::new(false),
        }
    }

    /// `n` Bytes; `None`, wenn es nicht reicht (dann ist es aufgebraucht
    /// und [`Budget::voll`] gilt).
    pub fn kosten(&self, n: usize) -> Option<()> {
        match self.rest.get().checked_sub(n) {
            Some(r) => {
                self.rest.set(r);
                Some(())
            }
            None => {
                self.leeren();
                None
            }
        }
    }

    /// Was übrig ist.
    pub fn rest(&self) -> usize {
        self.rest.get()
    }

    /// Wollte die Datei mehr, als übrig war?
    pub fn voll(&self) -> bool {
        self.voll.get()
    }

    /// Aufgebraucht: Danach liest nichts mehr aus der Datei.
    pub fn leeren(&self) {
        self.rest.set(0);
        self.voll.set(true);
    }

    /// Eine Grenze war erreicht, das Übrige bleibt.
    pub fn melden(&self) {
        self.voll.set(true);
    }
}

pub fn leer(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}

pub fn trenner(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn hexwert(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|v| v as u8)
}

/// Leser über einem Puffer; `pos` darf man setzen.
pub struct Leser<'a> {
    pub d: &'a [u8],
    pub pos: usize,
    /// Jedes Element eines Felds oder Verzeichnisses kostet [`ELEMENT`]
    /// Bytes davon; reicht es nicht, ist das Objekt unlesbar.
    budget: Option<&'a Budget>,
    /// Elemente ohne Budget bisher (höchstens [`OHNE_BUDGET`]).
    elemente: usize,
}

impl<'a> Leser<'a> {
    /// Leser ohne Budget, für kleine Puffer.
    pub fn neu(d: &'a [u8], pos: usize) -> Self {
        Leser {
            d,
            pos,
            budget: None,
            elemente: 0,
        }
    }

    /// Leser, dessen Felder und Verzeichnisse vom `budget` der Datei
    /// zehren (Review #128 N1).
    pub fn mit_budget(d: &'a [u8], pos: usize, budget: &'a Budget) -> Self {
        Leser {
            budget: Some(budget),
            ..Leser::neu(d, pos)
        }
    }

    /// Ein Element mehr in einem Feld oder Verzeichnis; `None`, wenn es
    /// nicht mehr ins Budget passt.
    fn element(&mut self) -> Option<()> {
        match self.budget {
            Some(b) => b.kosten(ELEMENT),
            None => {
                self.elemente += 1;
                (self.elemente <= OHNE_BUDGET).then_some(())
            }
        }
    }

    /// Leerraum und Kommentare überspringen.
    pub fn leerraum(&mut self) {
        while let Some(&b) = self.d.get(self.pos) {
            if leer(b) {
                self.pos += 1;
            } else if b == b'%' {
                while self
                    .d
                    .get(self.pos)
                    .is_some_and(|&b| b != b'\n' && b != b'\r')
                {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    /// Nächstes Wort; `None` am Ende oder bei Unlesbarem.
    pub fn wort(&mut self) -> Option<Wort<'a>> {
        self.wort_in(0)
    }

    fn wort_in(&mut self, tiefe: usize) -> Option<Wort<'a>> {
        self.leerraum();
        let mut b = *self.d.get(self.pos)?;
        // einzelnes `)`, `>`, `{`, `}`: überspringen
        while matches!(b, b')' | b'{' | b'}')
            || (b == b'>' && self.d.get(self.pos + 1) != Some(&b'>'))
        {
            self.pos += 1;
            self.leerraum();
            b = *self.d.get(self.pos)?;
        }
        match b {
            b'/' => {
                self.pos += 1;
                Some(Wort::Objekt(Objekt::Name(self.name())))
            }
            b'(' => {
                self.pos += 1;
                Some(Wort::Objekt(Objekt::Text(self.klammertext())))
            }
            b'<' if self.d.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                if tiefe >= TIEFE {
                    return None;
                }
                let mut v = Verz::default();
                loop {
                    match self.wort_in(tiefe + 1)? {
                        Wort::VerzZu => return Some(Wort::Objekt(Objekt::Verz(v))),
                        Wort::Objekt(Objekt::Name(k)) => {
                            let o = match self.wert(tiefe + 1)? {
                                Some(o) => o,
                                // Schlüssel ohne Wert am Ende: wie null
                                None => {
                                    v.setzen(&k, Objekt::Null);
                                    return Some(Wort::Objekt(Objekt::Verz(v)));
                                }
                            };
                            self.element()?;
                            if o != Objekt::Null {
                                v.setzen(&k, o);
                            }
                        }
                        // Unsinn zwischen den Einträgen überspringen
                        _ => {}
                    }
                }
            }
            b'<' => {
                self.pos += 1;
                Some(Wort::Objekt(Objekt::Text(self.hextext())))
            }
            b'>' if self.d.get(self.pos + 1) == Some(&b'>') => {
                self.pos += 2;
                Some(Wort::VerzZu)
            }
            b'[' => {
                self.pos += 1;
                if tiefe >= TIEFE {
                    return None;
                }
                let mut f = Vec::new();
                loop {
                    match self.wert(tiefe + 1)? {
                        Some(o) => {
                            self.element()?;
                            f.push(o);
                        }
                        None => return Some(Wort::Objekt(Objekt::Feld(f))),
                    }
                }
            }
            b']' => {
                self.pos += 1;
                Some(Wort::FeldZu)
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => {
                let o = self.zahl();
                if let Objekt::Ganz(nr) = o {
                    if let Some(r) = self.bezug(nr) {
                        return Some(Wort::Objekt(r));
                    }
                }
                Some(Wort::Objekt(o))
            }
            _ => {
                let a = self.pos;
                while self
                    .d
                    .get(self.pos)
                    .is_some_and(|&b| !leer(b) && !trenner(b))
                {
                    self.pos += 1;
                }
                let w = &self.d[a..self.pos];
                Some(match w {
                    b"true" => Wort::Objekt(Objekt::Bool(true)),
                    b"false" => Wort::Objekt(Objekt::Bool(false)),
                    b"null" => Wort::Objekt(Objekt::Null),
                    _ => Wort::Befehl(w),
                })
            }
        }
    }

    /// Wert in einem Feld oder Verzeichnis: `Some(None)` am schließenden
    /// `]` oder `>>`; ein Schlüsselwort wird übergangen.
    fn wert(&mut self, tiefe: usize) -> Option<Option<Objekt>> {
        loop {
            match self.wort_in(tiefe)? {
                Wort::Objekt(o) => return Some(Some(o)),
                Wort::FeldZu | Wort::VerzZu => return Some(None),
                Wort::Befehl(_) => {}
            }
        }
    }

    /// Ein Objekt; Schlüsselwörter davor überspringt der Leser nicht.
    pub fn objekt(&mut self) -> Option<Objekt> {
        match self.wort()? {
            Wort::Objekt(o) => Some(o),
            _ => None,
        }
    }

    /// Nach einer ganzen Zahl: `gen R`?
    fn bezug(&mut self, nr: i64) -> Option<Objekt> {
        let merk = self.pos;
        let zurueck = |s: &mut Self| {
            s.pos = merk;
            None
        };
        self.leerraum();
        let a = self.pos;
        while self.d.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
        if a == self.pos || !self.d.get(self.pos).is_some_and(|&b| leer(b)) {
            return zurueck(self);
        }
        let gen: Option<u16> = std::str::from_utf8(&self.d[a..self.pos])
            .ok()
            .and_then(|s| s.parse().ok());
        self.leerraum();
        let r = self.d.get(self.pos) == Some(&b'R')
            && self
                .d
                .get(self.pos + 1)
                .is_none_or(|&b| leer(b) || trenner(b));
        match (gen, u32::try_from(nr)) {
            (Some(g), Ok(n)) if r => {
                self.pos += 1;
                Some(Objekt::Bezug(n, g))
            }
            _ => zurueck(self),
        }
    }

    fn zahl(&mut self) -> Objekt {
        let a = self.pos;
        while self
            .d
            .get(self.pos)
            .is_some_and(|&b| matches!(b, b'+' | b'-' | b'.' | b'0'..=b'9'))
        {
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.d[a..self.pos]).unwrap_or("0");
        // „--5“ oder „5.-“ kommen in schlechten Dateien vor: was davor lesbar ist
        let rein: String = s
            .chars()
            .enumerate()
            .filter(|&(i, c)| c.is_ascii_digit() || c == '.' || (i == 0 && c != '+'))
            .map(|(_, c)| c)
            .collect();
        if !rein.contains('.') {
            if let Ok(i) = rein.parse::<i64>() {
                return Objekt::Ganz(i);
            }
        }
        let mut punkt = false;
        let rein: String = rein
            .chars()
            .filter(|&c| c != '.' || !std::mem::replace(&mut punkt, true))
            .collect();
        Objekt::Zahl(
            rein.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .unwrap_or(0.0),
        )
    }

    fn name(&mut self) -> String {
        let mut n = String::new();
        while let Some(&b) = self.d.get(self.pos) {
            if leer(b) || trenner(b) {
                break;
            }
            self.pos += 1;
            if b == b'#' {
                let h = (self.d.get(self.pos).copied().and_then(hexwert))
                    .zip(self.d.get(self.pos + 1).copied().and_then(hexwert));
                if let Some((x, y)) = h {
                    self.pos += 2;
                    n.push((x * 16 + y) as char);
                    continue;
                }
            }
            n.push(b as char);
        }
        n
    }

    fn klammertext(&mut self) -> Vec<u8> {
        let mut t = Vec::new();
        let mut offen = 1;
        while let Some(&b) = self.d.get(self.pos) {
            self.pos += 1;
            match b {
                b'(' => {
                    offen += 1;
                    t.push(b);
                }
                b')' => {
                    offen -= 1;
                    if offen == 0 {
                        break;
                    }
                    t.push(b);
                }
                b'\r' => {
                    // Zeilenende im Text ist immer \n
                    if self.d.get(self.pos) == Some(&b'\n') {
                        self.pos += 1;
                    }
                    t.push(b'\n');
                }
                b'\\' => {
                    let Some(&c) = self.d.get(self.pos) else {
                        break;
                    };
                    self.pos += 1;
                    match c {
                        b'n' => t.push(b'\n'),
                        b'r' => t.push(b'\r'),
                        b't' => t.push(b'\t'),
                        b'b' => t.push(8),
                        b'f' => t.push(12),
                        b'0'..=b'7' => {
                            let mut v = (c - b'0') as u32;
                            for _ in 0..2 {
                                match self.d.get(self.pos) {
                                    Some(&o @ b'0'..=b'7') => {
                                        v = v * 8 + (o - b'0') as u32;
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            t.push(v as u8);
                        }
                        b'\r' => {
                            if self.d.get(self.pos) == Some(&b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        _ => t.push(c),
                    }
                }
                _ => t.push(b),
            }
        }
        t
    }

    fn hextext(&mut self) -> Vec<u8> {
        let mut t = Vec::new();
        let mut halb: Option<u8> = None;
        while let Some(&b) = self.d.get(self.pos) {
            self.pos += 1;
            if b == b'>' {
                break;
            }
            if let Some(v) = hexwert(b) {
                match halb.take() {
                    Some(h) => t.push(h * 16 + v),
                    None => halb = Some(v),
                }
            }
        }
        if let Some(h) = halb {
            t.push(h * 16);
        }
        t
    }
}

/// Text aus einem PDF-Text (Info, Lesezeichen): UTF-16BE mit BOM, UTF-8
/// mit BOM, sonst PDFDocEncoding (wie Latin-1 bis auf wenige Zeichen).
pub fn text(b: &[u8]) -> String {
    if let Some(r) = b.strip_prefix(&[0xFE, 0xFF]) {
        let w: Vec<u16> = r
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&w);
    }
    if let Some(r) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(r).into_owned();
    }
    b.iter()
        .map(|&c| match c {
            0x80 => '•',
            0x84 => '—',
            0x85 => '–',
            0xA0 => '€',
            _ => c as char,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lies(s: &str) -> Option<Objekt> {
        Leser::neu(s.as_bytes(), 0).objekt()
    }

    #[test]
    fn zahlen_namen_texte() {
        assert_eq!(lies("42"), Some(Objekt::Ganz(42)));
        assert_eq!(lies("-3.5"), Some(Objekt::Zahl(-3.5)));
        assert_eq!(lies(".25"), Some(Objekt::Zahl(0.25)));
        assert_eq!(lies("+7"), Some(Objekt::Ganz(7)));
        assert_eq!(lies("--5"), Some(Objekt::Ganz(-5)));
        assert_eq!(lies("1.2.3"), Some(Objekt::Zahl(1.23)));
        assert_eq!(lies("/AS-ALKIS-2"), Some(Objekt::Name("AS-ALKIS-2".into())));
        assert_eq!(lies("/A#20B#2f"), Some(Objekt::Name("A B/".into())));
        assert_eq!(
            lies(
                r"(a\(b\)c \\ \101\0612 (inner) x\
y)"
            ),
            Some(Objekt::Text(b"a(b)c \\ A12 (inner) xy".to_vec()))
        );
        assert_eq!(
            lies("(zwei\r\nZeilen\\n)"),
            Some(Objekt::Text(b"zwei\nZeilen\n".to_vec()))
        );
        assert_eq!(
            lies("<48 65 6C6c 6F7>"),
            Some(Objekt::Text(b"Hellop".to_vec()))
        );
        assert_eq!(lies("<>"), Some(Objekt::Text(Vec::new())));
        assert_eq!(lies("true"), Some(Objekt::Bool(true)));
        assert_eq!(lies("null"), Some(Objekt::Null));
        assert_eq!(text(b"\xFE\xFF\x00F\x00l\x00\xFC"), "Flü");
        assert_eq!(text(b"Stra\xDFe"), "Straße");
    }

    /// Review #128 N1: Elemente kosten Budget; ohne Budget gibt es eine
    /// feste Grenze.
    #[test]
    fn elemente_kosten_budget() {
        let b = Budget::neu(10 * ELEMENT);
        let f = Leser::mit_budget(b"[1 2 3 4 5 6 7 8 9 10]", 0, &b).objekt();
        assert_eq!(f.and_then(|f| f.feld().map(<[Objekt]>::len)), Some(10));
        assert_eq!(b.rest(), 0);
        assert!(!b.voll());
        assert!(Leser::mit_budget(b"<< /A 1 >>", 0, &b).objekt().is_none());
        assert!(b.voll());
        assert!(lies(&format!("[{}]", "0 ".repeat(OHNE_BUDGET))).is_some());
        assert!(lies(&format!("[{}]", "0 ".repeat(OHNE_BUDGET + 1))).is_none());
        assert!(lies(&format!("<<{}>>", "/A 0 ".repeat(OHNE_BUDGET + 1))).is_none());
    }

    #[test]
    fn felder_verzeichnisse_bezuege() {
        let o = lies("<< /Type /Page /Kids [3 0 R 4 0 R] /Box [0 0 595.28 841.89] /N null %x\n /Leer << >> >>").unwrap();
        let v = o.verz().unwrap();
        assert_eq!(v.name("Type"), Some("Page"));
        assert_eq!(
            v.get("Kids"),
            Some(&Objekt::Feld(vec![
                Objekt::Bezug(3, 0),
                Objekt::Bezug(4, 0)
            ]))
        );
        assert_eq!(
            v.get("Box").unwrap().feld().unwrap()[2],
            Objekt::Zahl(595.28)
        );
        assert_eq!(v.get("N"), None);
        assert_eq!(v.get("Leer"), Some(&Objekt::Verz(Verz::default())));
        // zwei Zahlen ohne R sind kein Bezug
        let mut l = Leser::neu(b"[1 2 3 0 R 5]", 0);
        assert_eq!(
            l.objekt(),
            Some(Objekt::Feld(vec![
                Objekt::Ganz(1),
                Objekt::Ganz(2),
                Objekt::Bezug(3, 0),
                Objekt::Ganz(5)
            ]))
        );
        let mut l = Leser::neu(b"12 0 obj", 0);
        assert_eq!(l.wort(), Some(Wort::Objekt(Objekt::Ganz(12))));
        assert_eq!(l.wort(), Some(Wort::Objekt(Objekt::Ganz(0))));
        assert_eq!(l.wort(), Some(Wort::Befehl(b"obj")));
        assert_eq!(l.wort(), None);
        let mut l = Leser::neu(b"1 0 0 1 10 20 cm", 0);
        let mut n = 0;
        while let Some(w) = l.wort() {
            n += 1;
            if n == 7 {
                assert_eq!(w, Wort::Befehl(b"cm"));
            }
        }
        assert_eq!(n, 7);
    }

    #[test]
    fn kaputtes_bricht_nicht_ab() {
        for s in [
            "<<",
            "[",
            "(",
            "<",
            "<< /A",
            "<< /A [1 2",
            ")))",
            "<< 1 2 >>",
            "/",
            "[[[[",
        ] {
            let _ = lies(s);
        }
        let tief = "[".repeat(10_000);
        assert_eq!(lies(&tief), None);
        let tief = "<<".repeat(10_000);
        assert_eq!(lies(&tief), None);
    }
}

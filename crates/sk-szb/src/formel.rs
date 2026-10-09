//! Formeln (Vertrag §6): Zahlen mit Dezimalpunkt, `+ - * / ^ ( )`,
//! Vergleiche `< > <= >= == !=` (ergeben 1 oder 0), die Funktionen
//! `min max abs wurzel sin cos tan atan atan2 rund ab auf wenn` (Winkel in
//! Grad), `volumen(baustoff)` nur in `[menge]` und die Konstante `pi`.
//!
//! Vorrang wie in der Werkbank: Vergleich (höchstens einer) < Strich <
//! Punkt < Vorzeichen < Potenz; die Potenz bindet nach rechts und nimmt
//! rechts ein Vorzeichen (`2^-1`).

use std::collections::BTreeMap;

/// Werte der Namen: Parameter, abgeleitete Werte, `GH`, `DECKE`, `LICHT`,
/// `i`, `n`.
pub type Umfeld = BTreeMap<String, f64>;

/// Volumen je Baustoffschlüssel in m³ für `volumen()`.
pub type Volumen = BTreeMap<String, f64>;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Plus,
    Minus,
    Mal,
    Durch,
    Hoch,
    Kleiner,
    Groesser,
    KleinerGleich,
    GroesserGleich,
    Gleich,
    Ungleich,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fx {
    Min,
    Max,
    Abs,
    Wurzel,
    Sin,
    Cos,
    Tan,
    Atan,
    Atan2,
    Rund,
    Ab,
    Auf,
    Wenn,
}

impl Fx {
    fn von(name: &str) -> Option<(Fx, usize, usize)> {
        Some(match name {
            "min" => (Fx::Min, 1, 9),
            "max" => (Fx::Max, 1, 9),
            "abs" => (Fx::Abs, 1, 1),
            "wurzel" => (Fx::Wurzel, 1, 1),
            "sin" => (Fx::Sin, 1, 1),
            "cos" => (Fx::Cos, 1, 1),
            "tan" => (Fx::Tan, 1, 1),
            "atan" => (Fx::Atan, 1, 1),
            "atan2" => (Fx::Atan2, 2, 2),
            "rund" => (Fx::Rund, 1, 1),
            "ab" => (Fx::Ab, 1, 1),
            "auf" => (Fx::Auf, 1, 1),
            "wenn" => (Fx::Wenn, 3, 3),
            _ => return None,
        })
    }

    fn rechnen(self, a: &[f64]) -> f64 {
        let grad = std::f64::consts::PI / 180.0;
        match self {
            // Wie Math.min/Math.max: ein NaN macht das Ergebnis NaN
            Fx::Min => a.iter().copied().fold(f64::INFINITY, |m, x| {
                if m.is_nan() || x.is_nan() {
                    f64::NAN
                } else {
                    m.min(x)
                }
            }),
            Fx::Max => a.iter().copied().fold(f64::NEG_INFINITY, |m, x| {
                if m.is_nan() || x.is_nan() {
                    f64::NAN
                } else {
                    m.max(x)
                }
            }),
            Fx::Abs => a[0].abs(),
            Fx::Wurzel => a[0].sqrt(),
            Fx::Sin => (a[0] * grad).sin(),
            Fx::Cos => (a[0] * grad).cos(),
            Fx::Tan => (a[0] * grad).tan(),
            Fx::Atan => a[0].atan() / grad,
            Fx::Atan2 => a[0].atan2(a[1]) / grad,
            // Wie Math.round: .5 rundet nach oben; ohne `x + 0.5`, das bei
            // 0,49999999999999994 und ab 2^52 falsch aufrundet
            Fx::Rund => {
                let f = a[0].floor();
                if a[0] - f >= 0.5 {
                    f + 1.0
                } else {
                    f
                }
            }
            Fx::Ab => a[0].floor(),
            Fx::Auf => a[0].ceil(),
            Fx::Wenn => {
                if a[0] != 0.0 {
                    a[1]
                } else {
                    a[2]
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Knoten {
    Zahl(f64),
    Name(String),
    Minus(Box<Knoten>),
    Volumen(String),
    Funktion(Fx, Vec<Knoten>),
    Zwei(Op, Box<Knoten>, Box<Knoten>),
}

/// Eine übersetzte Formel.
#[derive(Clone, Debug, PartialEq)]
pub struct Formel {
    wurzel: Knoten,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Zahl(f64),
    Id(String),
    Op(&'static str),
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut t = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let x = c[i];
        if x.is_whitespace() {
            i += 1;
            continue;
        }
        if x.is_ascii_digit() || x == '.' {
            let mut j = i;
            while j < c.len() && (c[j].is_ascii_digit() || c[j] == '.') {
                j += 1;
            }
            if matches!(c.get(j), Some('e' | 'E'))
                && matches!(c.get(j + 1), Some(d) if *d == '-' || *d == '+' || d.is_ascii_digit())
            {
                j += 2;
                while j < c.len() && c[j].is_ascii_digit() {
                    j += 1;
                }
            }
            let v: String = c[i..j].iter().collect();
            match zahl_lesen(&v) {
                Some(z) => t.push(Tok::Zahl(z)),
                None => return Err(format!("Zahl „{v}“ ungültig")),
            }
            i = j;
            continue;
        }
        if x.is_ascii_alphabetic() || x == '_' {
            let mut j = i;
            while j < c.len() && (c[j].is_ascii_alphanumeric() || c[j] == '_') {
                j += 1;
            }
            t.push(Tok::Id(c[i..j].iter().collect()));
            i = j;
            continue;
        }
        let zwei: String = c[i..(i + 2).min(c.len())].iter().collect();
        if let Some(o) = ["<=", ">=", "==", "!="].into_iter().find(|o| *o == zwei) {
            t.push(Tok::Op(o));
            i += 2;
            continue;
        }
        if let Some(o) = ["+", "-", "*", "/", "^", "(", ")", ",", "<", ">"]
            .into_iter()
            .find(|o| o.starts_with(x))
        {
            t.push(Tok::Op(o));
            i += 1;
            continue;
        }
        return Err(format!("Zeichen „{x}“ in Formel nicht erlaubt"));
    }
    Ok(t)
}

/// Zahl in der Form `12`, `12.`, `12.5`, `.5`, wahlweise mit Exponent.
fn zahl_lesen(v: &str) -> Option<f64> {
    let (m, e) = match v.find(['e', 'E']) {
        Some(k) => (&v[..k], Some(&v[k + 1..])),
        None => (v, None),
    };
    // Wie die Werkbank: `\d+\.?\d*` oder `\.\d+`
    let ziffern = |t: &str| t.chars().all(|c| c.is_ascii_digit());
    let ok_m = match m.split_once('.') {
        Some((a, b)) => ziffern(a) && ziffern(b) && !(a.is_empty() && b.is_empty()),
        None => !m.is_empty() && ziffern(m),
    };
    let ok_e = match e {
        None => true,
        Some(e) => {
            let d = e.strip_prefix(['+', '-']).unwrap_or(e);
            !d.is_empty() && d.chars().all(|c| c.is_ascii_digit())
        }
    };
    if !(ok_m && ok_e) {
        return None;
    }
    v.parse().ok().or_else(|| format!("{v}0").parse().ok())
}

/// Längste Formel (Zeichen) und tiefste Verschachtelung aus Klammern,
/// Vorzeichen und Funktionen: Ein Stapelüberlauf wäre ein Programmabbruch.
pub const MAX_LAENGE: usize = 2000;
pub const MAX_TIEFE: usize = 64;

struct Leser {
    t: Vec<Tok>,
    p: usize,
    tiefe: usize,
}

impl Leser {
    fn tiefer(&mut self) -> Result<(), String> {
        self.tiefe += 1;
        if self.tiefe > MAX_TIEFE {
            return Err("Formel zu tief verschachtelt".into());
        }
        Ok(())
    }

    fn ist(&self, o: &str) -> bool {
        matches!(self.t.get(self.p), Some(Tok::Op(x)) if *x == o)
    }

    fn essen(&mut self, o: &str) -> Result<(), String> {
        if !self.ist(o) {
            return Err(format!("„{o}“ erwartet"));
        }
        self.p += 1;
        Ok(())
    }

    fn vergleich(&mut self) -> Result<Knoten, String> {
        let a = self.summe()?;
        for (o, op) in [
            ("<=", Op::KleinerGleich),
            (">=", Op::GroesserGleich),
            ("==", Op::Gleich),
            ("!=", Op::Ungleich),
            ("<", Op::Kleiner),
            (">", Op::Groesser),
        ] {
            if self.ist(o) {
                self.p += 1;
                let b = self.summe()?;
                return Ok(Knoten::Zwei(op, Box::new(a), Box::new(b)));
            }
        }
        Ok(a)
    }

    fn summe(&mut self) -> Result<Knoten, String> {
        let mut a = self.produkt()?;
        loop {
            let op = if self.ist("+") {
                Op::Plus
            } else if self.ist("-") {
                Op::Minus
            } else {
                return Ok(a);
            };
            self.p += 1;
            a = Knoten::Zwei(op, Box::new(a), Box::new(self.produkt()?));
        }
    }

    fn produkt(&mut self) -> Result<Knoten, String> {
        let mut a = self.vorzeichen()?;
        loop {
            let op = if self.ist("*") {
                Op::Mal
            } else if self.ist("/") {
                Op::Durch
            } else {
                return Ok(a);
            };
            self.p += 1;
            a = Knoten::Zwei(op, Box::new(a), Box::new(self.vorzeichen()?));
        }
    }

    fn vorzeichen(&mut self) -> Result<Knoten, String> {
        let minus = self.ist("-");
        if !minus && !self.ist("+") {
            return self.potenz();
        }
        self.p += 1;
        self.tiefer()?;
        let a = self.vorzeichen()?;
        self.tiefe -= 1;
        Ok(if minus { Knoten::Minus(Box::new(a)) } else { a })
    }

    fn potenz(&mut self) -> Result<Knoten, String> {
        let a = self.einfach()?;
        if self.ist("^") {
            self.p += 1;
            self.tiefer()?;
            let b = self.vorzeichen()?;
            self.tiefe -= 1;
            return Ok(Knoten::Zwei(Op::Hoch, Box::new(a), Box::new(b)));
        }
        Ok(a)
    }

    fn einfach(&mut self) -> Result<Knoten, String> {
        let Some(k) = self.t.get(self.p).cloned() else {
            return Err("Formel endet zu früh".into());
        };
        match k {
            Tok::Zahl(v) => {
                self.p += 1;
                Ok(Knoten::Zahl(v))
            }
            Tok::Id(name) => {
                self.p += 1;
                if self.ist("(") {
                    self.p += 1;
                    if name == "volumen" {
                        let Some(Tok::Id(b)) = self.t.get(self.p).cloned() else {
                            return Err("volumen() erwartet einen Baustoff-Schlüssel".into());
                        };
                        self.p += 1;
                        self.essen(")")?;
                        return Ok(Knoten::Volumen(b));
                    }
                    let Some((fx, lo, hi)) = Fx::von(&name) else {
                        return Err(format!("unbekannte Funktion „{name}“"));
                    };
                    self.tiefer()?;
                    let mut args = Vec::new();
                    if !self.ist(")") {
                        args.push(self.vergleich()?);
                        while self.ist(",") {
                            self.p += 1;
                            args.push(self.vergleich()?);
                        }
                    }
                    self.tiefe -= 1;
                    self.essen(")")?;
                    if args.len() < lo || args.len() > hi {
                        return Err(format!("{name}() mit falscher Anzahl Werte"));
                    }
                    return Ok(Knoten::Funktion(fx, args));
                }
                if name == "pi" {
                    return Ok(Knoten::Zahl(std::f64::consts::PI));
                }
                Ok(Knoten::Name(name))
            }
            Tok::Op("(") => {
                self.p += 1;
                self.tiefer()?;
                let a = self.vergleich()?;
                self.tiefe -= 1;
                self.essen(")")?;
                Ok(a)
            }
            Tok::Op(o) => Err(format!("„{o}“ unerwartet")),
        }
    }
}

impl Formel {
    /// Übersetzt den Text; der Fehler ist ein Satz für den Befund.
    pub fn neu(src: &str) -> Result<Formel, String> {
        if src.chars().count() > MAX_LAENGE {
            return Err(format!("Formel länger als {MAX_LAENGE} Zeichen"));
        }
        let t = lex(src)?;
        if t.is_empty() {
            return Err("leere Formel".into());
        }
        let mut l = Leser { t, p: 0, tiefe: 0 };
        let wurzel = l.vergleich()?;
        if let Some(k) = l.t.get(l.p) {
            let s = match k {
                Tok::Zahl(v) => format!("{v}"),
                Tok::Id(s) => s.clone(),
                Tok::Op(o) => o.to_string(),
            };
            return Err(format!("„{s}“ unerwartet"));
        }
        Ok(Formel { wurzel })
    }

    /// Alle Namen, die die Formel liest (ohne Funktionen und `pi`), in
    /// der Reihenfolge ihres ersten Auftretens.
    pub fn namen(&self) -> Vec<String> {
        fn geh(k: &Knoten, out: &mut Vec<String>) {
            match k {
                Knoten::Name(n) => {
                    if !out.contains(n) {
                        out.push(n.clone());
                    }
                }
                Knoten::Minus(a) => geh(a, out),
                Knoten::Zwei(_, a, b) => {
                    geh(a, out);
                    geh(b, out);
                }
                Knoten::Funktion(_, v) => v.iter().for_each(|a| geh(a, out)),
                Knoten::Zahl(_) | Knoten::Volumen(_) => {}
            }
        }
        let mut out = Vec::new();
        geh(&self.wurzel, &mut out);
        out
    }

    /// Die Baustoffe in `volumen(…)`, je Vorkommen einmal.
    pub fn volumen_baustoffe(&self) -> Vec<String> {
        fn geh(k: &Knoten, out: &mut Vec<String>) {
            match k {
                Knoten::Volumen(b) => out.push(b.clone()),
                Knoten::Minus(a) => geh(a, out),
                Knoten::Zwei(_, a, b) => {
                    geh(a, out);
                    geh(b, out);
                }
                Knoten::Funktion(_, v) => v.iter().for_each(|a| geh(a, out)),
                Knoten::Zahl(_) | Knoten::Name(_) => {}
            }
        }
        let mut out = Vec::new();
        geh(&self.wurzel, &mut out);
        out
    }

    /// Rechnet die Formel; `vol` nur in `[menge]`.
    pub fn wert(&self, u: &Umfeld, vol: Option<&Volumen>) -> Result<f64, String> {
        let mut rest = u64::MAX;
        self.wert_im(u, vol, &mut rest)
    }

    /// Wie [`Formel::wert`] mit höchstens `rest` Rechenschritten (ein
    /// Schritt je Knoten); `rest` nimmt ab. Reicht es nicht, ist der Fehler
    /// [`ZU_AUFWENDIG`] und `rest` 0.
    pub fn wert_im(
        &self,
        u: &Umfeld,
        vol: Option<&Volumen>,
        rest: &mut u64,
    ) -> Result<f64, String> {
        let v = rechne(&self.wurzel, u, vol, rest)?;
        if !v.is_finite() {
            return Err("Ergebnis ist keine Zahl".into());
        }
        Ok(v)
    }
}

/// Fehler, wenn die Rechenschritte nicht reichen (Review 3cg).
pub const ZU_AUFWENDIG: &str = "Bauteil zu aufwendig";

fn rechne(k: &Knoten, u: &Umfeld, vol: Option<&Volumen>, rest: &mut u64) -> Result<f64, String> {
    if *rest == 0 {
        return Err(ZU_AUFWENDIG.into());
    }
    *rest -= 1;
    let mut rechne = |k: &Knoten| rechne(k, u, vol, rest);
    Ok(match k {
        Knoten::Zahl(v) => *v,
        Knoten::Name(n) => match u.get(n) {
            Some(v) => *v,
            None => return Err(format!("unbekannter Name „{n}“")),
        },
        Knoten::Minus(a) => -rechne(a)?,
        Knoten::Volumen(b) => match vol {
            Some(v) => v.get(b).copied().unwrap_or(0.0),
            None => return Err("volumen() nur in [menge]".into()),
        },
        Knoten::Funktion(fx, args) => {
            let a = args
                .iter()
                .map(&mut rechne)
                .collect::<Result<Vec<_>, _>>()?;
            fx.rechnen(&a)
        }
        Knoten::Zwei(op, a, b) => {
            let (a, b) = (rechne(a)?, rechne(b)?);
            let ja = |x: bool| if x { 1.0 } else { 0.0 };
            match op {
                Op::Plus => a + b,
                Op::Minus => a - b,
                Op::Mal => a * b,
                Op::Durch => {
                    if b == 0.0 {
                        return Err("Teilung durch 0".into());
                    }
                    a / b
                }
                // Wie Math.pow: 1 und -1 hoch ±∞ oder NaN ist NaN (powf: 1)
                Op::Hoch if a.abs() == 1.0 && !b.is_finite() => f64::NAN,
                Op::Hoch => a.powf(b),
                Op::Kleiner => ja(a < b),
                Op::Groesser => ja(a > b),
                Op::KleinerGleich => ja(a <= b),
                Op::GroesserGleich => ja(a >= b),
                Op::Gleich => ja((a - b).abs() < 1e-9),
                Op::Ungleich => ja((a - b).abs() >= 1e-9),
            }
        }
    })
}

/// Übersetzt und rechnet in einem Schritt.
pub fn rechnen(src: &str, u: &Umfeld, vol: Option<&Volumen>) -> Result<f64, String> {
    Formel::neu(src)?.wert(u, vol)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> Result<f64, String> {
        let mut u = Umfeld::new();
        u.insert("GH".into(), 2855.0);
        u.insert("b".into(), 240.0);
        u.insert("i".into(), 2.0);
        rechnen(s, &u, None)
    }

    fn nah(s: &str, soll: f64) {
        let v = r(s).unwrap_or_else(|e| panic!("{s}: {e}"));
        assert!((v - soll).abs() < 1e-9, "{s} = {v}, soll {soll}");
    }

    #[test]
    fn vorrang_und_vorzeichen() {
        nah("1+2*3", 7.0);
        nah("(1+2)*3", 9.0);
        nah("2^3^2", 512.0);
        nah("-2^2", -4.0);
        nah("2^-1", 0.5);
        nah("10-4-3", 3.0);
        nah("8/4/2", 1.0);
        nah("--3", 3.0);
        nah("+3", 3.0);
        nah("1+2<4", 1.0);
        nah("3==3.0000000001", 1.0);
        nah("3!=3", 0.0);
        nah("GH/16", 2855.0 / 16.0);
        nah("1.5e3", 1500.0);
        nah(".5+5.", 5.5);
        nah("pi", std::f64::consts::PI);
    }

    #[test]
    fn funktionen() {
        nah("min(3,1,2)", 1.0);
        nah("max(b,300)", 300.0);
        nah("abs(-4)", 4.0);
        nah("wurzel(16)", 4.0);
        nah("sin(30)", 0.5);
        nah("cos(60)", 0.5);
        nah("tan(45)", 1.0);
        nah("atan(1)", 45.0);
        nah("atan2(1,0)", 90.0);
        nah("rund(2.5)", 3.0);
        nah("rund(-2.5)", -2.0);
        nah("ab(2.7)", 2.0);
        nah("auf(2.1)", 3.0);
        nah("wenn(i>1,10,20)", 10.0);
        nah("auf((3000-80)/136)-1", 21.0);
    }

    #[test]
    fn fehler() {
        let e = |s: &str| r(s).unwrap_err();
        assert_eq!(e(""), "leere Formel");
        assert_eq!(e("1+"), "Formel endet zu früh");
        assert_eq!(e("(1"), "„)“ erwartet");
        assert_eq!(e("1 2"), "„2“ unerwartet");
        assert_eq!(e("x+1"), "unbekannter Name „x“");
        assert_eq!(e("foo(1)"), "unbekannte Funktion „foo“");
        assert_eq!(e("wenn(1,2)"), "wenn() mit falscher Anzahl Werte");
        assert_eq!(e("1/0"), "Teilung durch 0");
        assert_eq!(e("wurzel(-1)"), "Ergebnis ist keine Zahl");
        assert_eq!(e("1,5"), "„,“ unerwartet");
        assert_eq!(e("2 € 3"), "Zeichen „€“ in Formel nicht erlaubt");
        assert_eq!(e("1.2.3"), "Zahl „1.2.3“ ungültig");
        assert_eq!(e("volumen(stahlbeton)"), "volumen() nur in [menge]");
        assert_eq!(
            e("volumen(1)"),
            "volumen() erwartet einen Baustoff-Schlüssel"
        );
        assert_eq!(e(")"), "„)“ unerwartet");
        // Tiefe und Länge: kein Stapelüberlauf
        assert_eq!(e(&"-".repeat(1500)), "Formel zu tief verschachtelt");
        assert_eq!(
            e(&format!("{}1{}", "(".repeat(70), ")".repeat(70))),
            "Formel zu tief verschachtelt"
        );
        assert_eq!(e(&"abs(".repeat(70)), "Formel zu tief verschachtelt");
        assert_eq!(
            e(&format!("{}2", "2^".repeat(70))),
            "Formel zu tief verschachtelt"
        );
        assert_eq!(e(&"1+".repeat(1001)), "Formel länger als 2000 Zeichen");
        nah(&format!("{}1{}", "(".repeat(60), ")".repeat(60)), 1.0);
        nah(&format!("{}1", "1+".repeat(999)), 1000.0);
    }

    #[test]
    fn volumen_und_namen() {
        let f = Formel::neu("volumen(stahlbeton)*0.15+b/1000").unwrap();
        let mut vol = Volumen::new();
        vol.insert("stahlbeton".into(), 2.0);
        let mut u = Umfeld::new();
        u.insert("b".into(), 1000.0);
        assert_eq!(f.wert(&u, Some(&vol)), Ok(1.3));
        assert_eq!(f.namen(), ["b"]);
        assert_eq!(f.volumen_baustoffe(), ["stahlbeton"]);
        // unbekannter Baustoff: 0 m³
        assert_eq!(rechnen("volumen(holz)", &u, Some(&vol)), Ok(0.0));
    }
}

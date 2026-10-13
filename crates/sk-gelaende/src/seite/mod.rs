//! Inhaltsstrom einer PDF-Seite lesen (plan-gelaendemodul.md §5 G1c):
//! Grafikzustand (q, Q, cm, w, Farben), Pfade mit Kurven als Polylinien,
//! Malen, Texte mit WinAnsi oder Unicode-Tabelle und Formulare über `Do`.
//! Bilder werden nur als Rechteck gemerkt.
//!
//! Ergebnis ist die Seite in pt nach der Matrix: Linienzüge mit Strich und
//! Füllung, Texte mit Lage, Größe und Winkel. Unlesbares wird übergangen.

pub mod schrift;

use crate::pdf::objekt::{Leser, Objekt, Verz, Wort};
use crate::pdf::{Blatt, Pdf};
use schrift::Schrift;
use std::collections::BTreeMap;

/// Farbe als RGB, 0 bis 255.
pub type Farbe = [u8; 3];

/// Affine Matrix [a b c d e f] wie im PDF: x' = a x + c y + e.
pub type Matrix = [f64; 6];

pub const EINS: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Erst `m`, dann `n`.
pub fn mal(m: &Matrix, n: &Matrix) -> Matrix {
    [
        m[0] * n[0] + m[1] * n[2],
        m[0] * n[1] + m[1] * n[3],
        m[2] * n[0] + m[3] * n[2],
        m[2] * n[1] + m[3] * n[3],
        m[4] * n[0] + m[5] * n[2] + n[4],
        m[4] * n[1] + m[5] * n[3] + n[5],
    ]
}

/// Punkt durch die Matrix.
pub fn punkt(m: &Matrix, x: f64, y: f64) -> [f64; 2] {
    [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]
}

/// Ein Linienzug in pt.
#[derive(Clone, Debug, PartialEq)]
pub struct Zug {
    pub punkte: Vec<[f64; 2]>,
    /// Mit `h` (oder `re`) geschlossen.
    pub zu: bool,
}

/// Ein gemalter Pfad.
#[derive(Clone, Debug, PartialEq)]
pub struct Pfad {
    pub zuege: Vec<Zug>,
    /// Strichfarbe und -breite in pt nach der Matrix; `None` = kein Strich.
    pub strich: Option<(Farbe, f64)>,
    pub fuellung: Option<Farbe>,
    /// Füllregel gerade-ungerade statt Nonzero.
    pub gerade_ungerade: bool,
    /// Der Pfad hatte Kurven (Kreise, Bögen).
    pub kurven: bool,
}

/// Ein Text auf der Seite.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub text: String,
    /// Anfang der Grundlinie in pt.
    pub lage: [f64; 2],
    /// Ende der Grundlinie nach den Zeichenbreiten.
    pub ende: [f64; 2],
    /// Schriftgröße in pt nach der Matrix.
    pub groesse: f64,
    /// Richtung der Grundlinie in Grad, gegen den Uhrzeigersinn von +x.
    pub winkel: f64,
    pub schrift: String,
    pub farbe: Farbe,
}

/// Die gelesene Seite.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seite {
    /// x0, y0, x1, y1 in pt.
    pub mediabox: [f64; 4],
    pub pfade: Vec<Pfad>,
    pub texte: Vec<Text>,
    /// Bilder als Rechteck x0, y0, x1, y1 in pt (übergangen).
    pub bilder: Vec<[f64; 4]>,
}

#[derive(Clone)]
struct Zustand {
    ctm: Matrix,
    strich: Farbe,
    fuell: Farbe,
    breite: f64,
    schrift: Option<usize>,
    groesse: f64,
    tc: f64,
    tw: f64,
    th: f64,
    tl: f64,
    rise: f64,
}

/// Tiefer verschachtelte Formulare liest Skizzeo nicht.
const FORMTIEFE: usize = 12;
/// Höchstens so viele Formularaufrufe je Seite, auch in der Breite (zehn
/// Aufrufe je Stufe wären sonst 10^12, Review W37 B4).
const FORMLAEUFE: usize = 20_000;

struct Lauf<'p> {
    pdf: &'p Pdf,
    seite: Seite,
    z: Zustand,
    stapel: Vec<Zustand>,
    pfad: Vec<Zug>,
    kurven: bool,
    tm: Matrix,
    tlm: Matrix,
    schriften: Vec<Schrift>,
    /// Schriften nach Objektnummer.
    schrift_nr: BTreeMap<u32, usize>,
    /// Letzter Text im laufenden BT, an den der nächste anschließen kann.
    anschluss: Option<usize>,
    /// Formulare im Aufruf (gegen Kreise).
    formulare: Vec<u32>,
    /// Formularaufrufe bisher (gegen Bomben in der Breite).
    laeufe: usize,
}

fn farbe(v: &[f64]) -> Farbe {
    let k = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    match v.len() {
        1 => [k(v[0]); 3],
        3 => [k(v[0]), k(v[1]), k(v[2])],
        4 => {
            let s = 1.0 - v[3].clamp(0.0, 1.0);
            [
                k((1.0 - v[0]) * s),
                k((1.0 - v[1]) * s),
                k((1.0 - v[2]) * s),
            ]
        }
        _ => [0; 3],
    }
}

/// Kubische Bézierkurve als Polylinie (ohne Anfangspunkt), Abweichung
/// unter 0,02 pt.
fn bezier(p0: [f64; 2], p1: [f64; 2], p2: [f64; 2], p3: [f64; 2]) -> Vec<[f64; 2]> {
    let dd = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        ((a[0] - 2.0 * b[0] + c[0]).powi(2) + (a[1] - 2.0 * b[1] + c[1]).powi(2)).sqrt()
    };
    let m = dd(p0, p1, p2).max(dd(p1, p2, p3));
    let n = ((0.75 * m / 0.02).sqrt().ceil()).clamp(1.0, 100.0) as usize;
    (1..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            [
                a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
                a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
            ]
        })
        .collect()
}

impl<'p> Lauf<'p> {
    fn zahlen(ops: &[Objekt]) -> Vec<f64> {
        ops.iter().filter_map(Objekt::zahl).collect()
    }

    /// Aktueller Punkt; nach `h` der Anfang des geschlossenen Zugs
    /// (Geometrie zu #128: `c` direkt nach `h` begann sonst am letzten Punkt).
    fn letzter(&self) -> Option<[f64; 2]> {
        let z = self.pfad.last()?;
        if z.zu {
            z.punkte.first().copied()
        } else {
            z.punkte.last().copied()
        }
    }

    fn hin(&mut self, p: [f64; 2]) {
        match self.pfad.last_mut() {
            Some(z) if !z.zu => z.punkte.push(p),
            Some(z) => {
                // nach h geht es am Anfang des geschlossenen Zugs weiter
                let a = z.punkte[0];
                self.pfad.push(Zug {
                    punkte: vec![a, p],
                    zu: false,
                });
            }
            None => self.pfad.push(Zug {
                punkte: vec![p],
                zu: false,
            }),
        }
    }

    fn malen(&mut self, strich: bool, fuell: bool, schliessen: bool, gu: bool) {
        let mut zuege = std::mem::take(&mut self.pfad);
        let kurven = std::mem::replace(&mut self.kurven, false);
        if schliessen {
            for z in &mut zuege {
                z.zu = true;
            }
        }
        zuege.retain(|z| !z.punkte.is_empty());
        if zuege.is_empty() || (!strich && !fuell) {
            return;
        }
        let m = &self.z.ctm;
        let skala = (m[0] * m[3] - m[1] * m[2]).abs().sqrt();
        self.seite.pfade.push(Pfad {
            zuege,
            strich: strich.then_some((self.z.strich, self.z.breite * skala)),
            fuellung: fuell.then_some(self.z.fuell),
            gerade_ungerade: gu,
            kurven,
        });
    }

    fn schrift(&mut self, res: &Verz, name: &str) -> Option<usize> {
        let fonts = self.pdf.verz(res, "Font")?;
        let o = fonts.get(name)?;
        if let Objekt::Bezug(n, _) = o {
            if let Some(&i) = self.schrift_nr.get(n) {
                return Some(i);
            }
        }
        let v = self.pdf.aufloesen(o).verz()?.clone();
        self.schriften.push(Schrift::neu(self.pdf, &v));
        let i = self.schriften.len() - 1;
        if let Objekt::Bezug(n, _) = o {
            self.schrift_nr.insert(*n, i);
        }
        Some(i)
    }

    /// Texte zeigen (Tj, TJ): je Teil die Bytes und die Zahl aus TJ
    /// dahinter (in 1/1000 der Schriftgröße, negativ nach rechts).
    fn zeigen(&mut self, teile: &[(Vec<u8>, f64)]) {
        let Some(si) = self.z.schrift else {
            return;
        };
        let (tfs, th) = (self.z.groesse, self.z.th);
        let mut text = String::new();
        let start = mal(&self.tm, &self.z.ctm);
        let lage = punkt(&start, 0.0, self.z.rise);
        for (t, abstand) in teile {
            for (z, b, leer) in self.schriften[si].zerlegen(t) {
                text.push_str(&z);
                let tx = (b / 1000.0 * tfs + self.z.tc + if leer { self.z.tw } else { 0.0 }) * th;
                self.tm = mal(&[1.0, 0.0, 0.0, 1.0, tx, 0.0], &self.tm);
            }
            let tx = -abstand / 1000.0 * tfs * th;
            if *abstand < -250.0 && !text.ends_with(' ') {
                text.push(' ');
            }
            self.tm = mal(&[1.0, 0.0, 0.0, 1.0, tx, 0.0], &self.tm);
        }
        let m = mal(&self.tm, &self.z.ctm);
        let ende = punkt(&m, 0.0, self.z.rise);
        let groesse = tfs * (m[0] * m[3] - m[1] * m[2]).abs().sqrt();
        let winkel = m[1].atan2(m[0]).to_degrees();
        if text.trim().is_empty() {
            return;
        }
        // an den vorigen Text desselben BT anschließen, wenn er dort endete
        if let Some(i) = self.anschluss {
            let v = &mut self.seite.texte[i];
            let (s, c) = v.winkel.to_radians().sin_cos();
            let (dx, dy) = (lage[0] - v.ende[0], lage[1] - v.ende[1]);
            let laengs = dx * c + dy * s;
            let quer = -dx * s + dy * c;
            if (v.groesse - groesse).abs() < 0.02 * groesse
                && (v.winkel - winkel).abs() < 0.5
                && quer.abs() < 0.2 * groesse
                && laengs > -0.3 * groesse
                && laengs < 0.6 * groesse
            {
                if laengs > 0.2 * groesse && !v.text.ends_with(' ') {
                    v.text.push(' ');
                }
                v.text.push_str(&text);
                v.ende = ende;
                return;
            }
        }
        self.seite.texte.push(Text {
            text,
            lage,
            ende,
            groesse,
            winkel,
            schrift: self.schriften[si].name.clone(),
            farbe: self.z.fuell,
        });
        self.anschluss = Some(self.seite.texte.len() - 1);
    }

    fn bild(&mut self) {
        let m = &self.z.ctm;
        let ecken = [
            punkt(m, 0.0, 0.0),
            punkt(m, 1.0, 0.0),
            punkt(m, 0.0, 1.0),
            punkt(m, 1.0, 1.0),
        ];
        let x0 = ecken.iter().map(|p| p[0]).fold(f64::MAX, f64::min);
        let y0 = ecken.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
        let x1 = ecken.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        let y1 = ecken.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
        self.seite.bilder.push([x0, y0, x1, y1]);
    }

    fn form(&mut self, res: &Verz, name: &str, tiefe: usize) {
        let Some(xo) = self.pdf.verz(res, "XObject") else {
            return;
        };
        let Some(o) = xo.get(name) else { return };
        let nr = match o {
            Objekt::Bezug(n, _) => Some(*n),
            _ => None,
        };
        let Objekt::Strom(s) = self.pdf.aufloesen(o) else {
            return;
        };
        match s.verz.name("Subtype") {
            Some("Image") => self.bild(),
            Some("Form") if tiefe < FORMTIEFE && self.laeufe < FORMLAEUFE => {
                if nr.is_some_and(|n| self.formulare.contains(&n)) {
                    return;
                }
                self.laeufe += 1;
                let Some(d) = self.pdf.daten(&s) else { return };
                let m: Vec<f64> = s
                    .verz
                    .get("Matrix")
                    .and_then(Objekt::feld)
                    .map(|f| f.iter().filter_map(Objekt::zahl).collect())
                    .unwrap_or_default();
                let res2 = self
                    .pdf
                    .verz(&s.verz, "Resources")
                    .unwrap_or_else(|| res.clone());
                let gerettet = self.z.clone();
                let stapel = self.stapel.len();
                if m.len() == 6 {
                    self.z.ctm = mal(&[m[0], m[1], m[2], m[3], m[4], m[5]], &self.z.ctm);
                }
                self.formulare.extend(nr);
                let pfad = std::mem::take(&mut self.pfad);
                self.lesen(&d, &res2, tiefe + 1);
                self.pfad = pfad;
                if nr.is_some() {
                    self.formulare.pop();
                }
                self.stapel.truncate(stapel);
                self.z = gerettet;
            }
            _ => {}
        }
    }

    /// Einen Inhaltsstrom mit seinen Ressourcen lesen.
    fn lesen(&mut self, d: &[u8], res: &Verz, tiefe: usize) {
        let mut l = Leser::neu(d, 0);
        let mut ops: Vec<Objekt> = Vec::new();
        while let Some(w) = l.wort() {
            let befehl = match w {
                Wort::Objekt(o) => {
                    if ops.len() >= 64 {
                        ops.remove(0);
                    }
                    ops.push(o);
                    continue;
                }
                Wort::Befehl(b) => b,
                _ => {
                    ops.clear();
                    continue;
                }
            };
            let z = Self::zahlen(&ops);
            let ctm = self.z.ctm;
            match befehl {
                b"q" => {
                    if self.stapel.len() < 256 {
                        self.stapel.push(self.z.clone());
                    }
                }
                b"Q" => {
                    if let Some(z) = self.stapel.pop() {
                        self.z = z;
                    }
                }
                b"cm" if z.len() == 6 => {
                    self.z.ctm = mal(&[z[0], z[1], z[2], z[3], z[4], z[5]], &self.z.ctm);
                }
                b"w" if !z.is_empty() => self.z.breite = z[0].abs(),
                b"gs" => {
                    let lw = ops
                        .last()
                        .and_then(Objekt::name)
                        .and_then(|n| self.pdf.verz(res, "ExtGState")?.get(n).cloned())
                        .and_then(|o| self.pdf.aufloesen(&o).verz()?.zahl("LW"));
                    if let Some(lw) = lw {
                        self.z.breite = lw.abs();
                    }
                }
                b"g" | b"rg" | b"k" | b"sc" | b"scn" => self.z.fuell = farbe(&z),
                b"G" | b"RG" | b"K" | b"SC" | b"SCN" => self.z.strich = farbe(&z),
                b"cs" => self.z.fuell = [0; 3],
                b"CS" => self.z.strich = [0; 3],
                b"m" if z.len() >= 2 => {
                    let p = punkt(&ctm, z[0], z[1]);
                    // ein m direkt nach m ersetzt den Anfang
                    match self.pfad.last_mut() {
                        Some(zug) if zug.punkte.len() == 1 && !zug.zu => zug.punkte[0] = p,
                        _ => self.pfad.push(Zug {
                            punkte: vec![p],
                            zu: false,
                        }),
                    }
                }
                b"l" if z.len() >= 2 => self.hin(punkt(&ctm, z[0], z[1])),
                b"c" | b"v" | b"y" => {
                    let Some(p0) = self.letzter() else {
                        ops.clear();
                        continue;
                    };
                    let (p1, p2, p3) = match (befehl, z.len()) {
                        (b"c", 6..) => (
                            punkt(&ctm, z[0], z[1]),
                            punkt(&ctm, z[2], z[3]),
                            punkt(&ctm, z[4], z[5]),
                        ),
                        (b"v", 4..) => (p0, punkt(&ctm, z[0], z[1]), punkt(&ctm, z[2], z[3])),
                        (b"y", 4..) => {
                            let p3 = punkt(&ctm, z[2], z[3]);
                            (punkt(&ctm, z[0], z[1]), p3, p3)
                        }
                        _ => {
                            ops.clear();
                            continue;
                        }
                    };
                    for p in bezier(p0, p1, p2, p3) {
                        self.hin(p);
                    }
                    self.kurven = true;
                }
                b"re" if z.len() >= 4 => {
                    let (x, y, w, h) = (z[0], z[1], z[2], z[3]);
                    self.pfad.push(Zug {
                        punkte: vec![
                            punkt(&ctm, x, y),
                            punkt(&ctm, x + w, y),
                            punkt(&ctm, x + w, y + h),
                            punkt(&ctm, x, y + h),
                        ],
                        zu: true,
                    });
                }
                b"h" => {
                    if let Some(zug) = self.pfad.last_mut() {
                        zug.zu = true;
                    }
                }
                b"S" => self.malen(true, false, false, false),
                b"s" => self.malen(true, false, true, false),
                b"f" | b"F" => self.malen(false, true, true, false),
                b"f*" => self.malen(false, true, true, true),
                b"B" => self.malen(true, true, false, false),
                b"B*" => self.malen(true, true, false, true),
                b"b" => self.malen(true, true, true, false),
                b"b*" => self.malen(true, true, true, true),
                b"n" => {
                    self.pfad.clear();
                    self.kurven = false;
                }
                b"BT" => {
                    self.tm = EINS;
                    self.tlm = EINS;
                    self.anschluss = None;
                }
                b"ET" => self.anschluss = None,
                b"Tf" => {
                    if let (Some(n), Some(&g)) = (ops.first().and_then(Objekt::name), z.last()) {
                        self.z.schrift = self.schrift(res, n);
                        self.z.groesse = g;
                    }
                }
                b"Tm" if z.len() == 6 => {
                    self.tlm = [z[0], z[1], z[2], z[3], z[4], z[5]];
                    self.tm = self.tlm;
                }
                b"Td" | b"TD" if z.len() == 2 => {
                    if befehl == b"TD" {
                        self.z.tl = -z[1];
                    }
                    self.tlm = mal(&[1.0, 0.0, 0.0, 1.0, z[0], z[1]], &self.tlm);
                    self.tm = self.tlm;
                }
                b"T*" => {
                    self.tlm = mal(&[1.0, 0.0, 0.0, 1.0, 0.0, -self.z.tl], &self.tlm);
                    self.tm = self.tlm;
                }
                b"Tc" if !z.is_empty() => self.z.tc = z[0],
                b"Tw" if !z.is_empty() => self.z.tw = z[0],
                b"Tz" if !z.is_empty() => self.z.th = z[0] / 100.0,
                b"TL" if !z.is_empty() => self.z.tl = z[0],
                b"Ts" if !z.is_empty() => self.z.rise = z[0],
                b"Tj" | b"'" | b"\"" => {
                    if befehl != b"Tj" {
                        if befehl == b"\"" && z.len() >= 2 {
                            self.z.tw = z[0];
                            self.z.tc = z[1];
                        }
                        self.tlm = mal(&[1.0, 0.0, 0.0, 1.0, 0.0, -self.z.tl], &self.tlm);
                        self.tm = self.tlm;
                    }
                    if let Some(t) = ops.last().and_then(Objekt::text) {
                        let t = t.to_vec();
                        self.zeigen(&[(t, 0.0)]);
                    }
                }
                b"TJ" => {
                    if let Some(f) = ops.last().and_then(Objekt::feld) {
                        let mut teile: Vec<(Vec<u8>, f64)> = Vec::new();
                        for o in f {
                            match o {
                                Objekt::Text(t) => teile.push((t.clone(), 0.0)),
                                o => {
                                    let a = o.zahl().unwrap_or(0.0);
                                    match teile.last_mut() {
                                        Some(t) => t.1 += a,
                                        None => teile.push((Vec::new(), a)),
                                    }
                                }
                            }
                        }
                        self.zeigen(&teile);
                    }
                }
                b"Do" => {
                    if let Some(n) = ops.last().and_then(Objekt::name) {
                        let n = n.to_string();
                        self.form(res, &n, tiefe);
                    }
                }
                b"BI" => {
                    // eingebettetes Bild: bis ID, dann bis EI zwischen Leerraum
                    while let Some(w) = l.wort() {
                        if w == Wort::Befehl(b"ID") {
                            break;
                        }
                    }
                    let mut p = l.pos + 1;
                    while p + 2 <= d.len() {
                        if &d[p..p + 2] == b"EI"
                            && crate::pdf::objekt::leer(d[p - 1])
                            && d.get(p + 2).is_none_or(|&b| crate::pdf::objekt::leer(b))
                        {
                            break;
                        }
                        p += 1;
                    }
                    l.pos = (p + 2).min(d.len());
                    self.bild();
                }
                _ => {}
            }
            ops.clear();
        }
    }
}

/// Eine Seite des PDF lesen.
pub fn seite(pdf: &Pdf, blatt: &Blatt) -> Seite {
    let inhalt = pdf.inhalt(blatt);
    let mut l = Lauf {
        pdf,
        seite: Seite {
            mediabox: blatt.mediabox,
            ..Seite::default()
        },
        z: Zustand {
            ctm: EINS,
            strich: [0; 3],
            fuell: [0; 3],
            breite: 1.0,
            schrift: None,
            groesse: 0.0,
            tc: 0.0,
            tw: 0.0,
            th: 1.0,
            tl: 0.0,
            rise: 0.0,
        },
        stapel: Vec::new(),
        pfad: Vec::new(),
        kurven: false,
        tm: EINS,
        tlm: EINS,
        schriften: Vec::new(),
        schrift_nr: BTreeMap::new(),
        anschluss: None,
        formulare: Vec::new(),
        laeufe: 0,
    };
    l.lesen(&inhalt, &blatt.ressourcen, 0);
    l.seite
}

#[cfg(test)]
mod tests;

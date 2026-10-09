//! LV als A4-Blatt (kosten/lv-blatt-a4.md, architektur/paket-projektdaten.md
//! §5): Seiten aus Text und Linien für die Vorschau im Reiter AVA und für
//! „Als PDF speichern“. Dieselben Seiten zeichnet die Vorschau und schreibt
//! das PDF, so zeigt die Vorschau genau den Ausdruck.
//!
//! Das Blatt rechnet nichts: Mengen, EP, GP und Summen kommen aus dem
//! [`Lv`], wie am Bildschirm und in der CSV.

use crate::kosten_view::menge_text;
use sk_cost::geld::Cent;
use sk_cost::lv::{Lv, LvPosition, LvTitel};
use sk_paint::font::Font;
use sk_paint::pdf::{mm, Op, Seite, A4};

/// Ränder: links 20 mm (Lochung), sonst 15 mm; Nutzbreite 175 mm.
const LINKS: f32 = 20.0;
const RAND: f32 = 15.0;
const BREITE: f32 = 175.0;

/// Spalten (mm ab dem linken Rand): OZ 22, Kurztext 76, Menge 22
/// (rechtsbündig), ME 10, EP 20 und GP 25 (rechtsbündig).
const KURZ_X: f32 = 22.0;
const KURZ_W: f32 = 76.0;
const MENGE_R: f32 = 120.0;
const ME_X: f32 = 121.5;
const EP_R: f32 = 150.0;
const GP_R: f32 = 175.0;

/// Schriftgrößen (pt) und Zeilenabstand.
const TEXT: f32 = 9.0;
const KLEIN: f32 = 8.0;
const TITEL: f32 = 10.0;
const ZEILE: f32 = 11.5;
/// Grau für Bezeichnungen und leise Zeilen (0 schwarz, 255 weiß).
const LEISE: u8 = 105;
/// Leere Linie für den Bieter in EP und GP.
const LINIE_EP: f32 = 16.0;
const LINIE_GP: f32 = 21.0;

/// Was das Blatt neben dem LV braucht: wie am Bildschirm.
#[derive(Clone, Debug, PartialEq)]
pub struct Angaben {
    /// `Project.site`, sonst der Dateiname ohne Endung (Regel 110).
    pub bauvorhaben: String,
    /// „Gebäude 1 · alle Geschosse“.
    pub umfang: String,
    /// „09.10.2026“.
    pub stand: String,
    /// Nur mit Preisen: „Referenzpreise 10/2026, unverbindlich“.
    pub preisquelle: Option<String>,
}

/// Schalter in der Vorschau-Leiste; der Arbeitsplatz merkt sie sich.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wahl {
    pub preise: bool,
    pub titelblatt: bool,
    pub verzeichnis: bool,
}

/// Eintrag im Inhaltsverzeichnis: Text, Einrückung und Seite (ab 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eintrag {
    pub text: String,
    pub tief: bool,
    pub seite: usize,
}

/// Das fertige Blatt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Blatt {
    pub seiten: Vec<Seite>,
    /// Einträge mit ihren Seiten, auch ohne Verzeichnis (für die Tests).
    pub inhalt: Vec<Eintrag>,
}

/// Dateiname: „LV-Rohbau-Haus-2026-10-09-Anfrage.pdf“ bzw.
/// „…-mit-Preisen.pdf“; `datum` ist „2026-10-09“.
pub fn dateiname(lv: &Lv, bauvorhaben: &str, datum: &str, preise: bool) -> String {
    let teil = |t: &str| {
        let s: String = t
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        s.split('-')
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    };
    let fassung = if preise { "mit-Preisen" } else { "Anfrage" };
    format!(
        "LV-{}-{}-{}-{fassung}.pdf",
        teil(&lv.kopf.los),
        teil(bauvorhaben),
        datum
    )
}

/// „LV 1 Rohbau“: so heißt das Los auf dem Blatt und am Bildschirm.
pub fn los_text(lv: &Lv) -> String {
    format!("LV {} {}", lv.kopf.los_nr, lv.kopf.los)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Teilt `text` in Zeilen der Breite `breite` wie [`umbrechen`], aber
/// bevorzugt nach „, “ und nach „ · “: „Musterweg 1,“ | „27777
/// Ganderkesee“ statt zwischen Postleitzahl und Ort (Test Hinweis N).
fn umbrechen_glieder(f: &Font, text: &str, pt: f32, breite: f32) -> Vec<String> {
    let mut glieder: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find([',', '·']) {
        let ende = i + rest[i..].chars().next().map_or(1, char::len_utf8);
        glieder.push(rest[..ende].trim().to_string());
        rest = &rest[ende..];
    }
    glieder.push(rest.trim().to_string());
    let mut zeilen: Vec<String> = Vec::new();
    let mut zeile = String::new();
    for g in glieder.into_iter().filter(|g| !g.is_empty()) {
        let probe = if zeile.is_empty() {
            g.clone()
        } else {
            format!("{zeile} {g}")
        };
        if f.width(&probe, pt) <= breite {
            zeile = probe;
            continue;
        }
        if !zeile.is_empty() {
            zeilen.push(std::mem::take(&mut zeile));
        }
        // Ein Glied breiter als die Zeile: am Leerzeichen wie sonst
        let mut teil = umbrechen(f, &g, pt, breite);
        zeile = teil.pop().unwrap_or_default();
        zeilen.extend(teil);
    }
    if !zeile.is_empty() || zeilen.is_empty() {
        zeilen.push(zeile);
    }
    zeilen
}

/// Menge ohne Einheit: „172,224“.
fn menge(p: &LvPosition) -> String {
    menge_text(p.menge, p.einheit)
        .trim_end_matches(p.einheit.zeichen())
        .trim()
        .to_string()
}

/// Mehrzeiliges in einer Zeile mit „, “.
fn einzeilig(t: &str) -> String {
    t.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Umbruch auf `breite` (pt) am Leerzeichen; ein zu langes Wort wird hart
/// geteilt. Nie gekürzt.
fn umbrechen(f: &Font, text: &str, pt: f32, breite: f32) -> Vec<String> {
    let mut zeilen = Vec::new();
    for absatz in text.lines() {
        let mut zeile = String::new();
        for wort in absatz.split_whitespace() {
            let probe = if zeile.is_empty() {
                wort.to_string()
            } else {
                format!("{zeile} {wort}")
            };
            if f.width(&probe, pt) <= breite {
                zeile = probe;
                continue;
            }
            if !zeile.is_empty() {
                zeilen.push(std::mem::take(&mut zeile));
            }
            // Ein Wort breiter als die Zeile: hart teilen
            let mut rest = wort.to_string();
            while f.width(&rest, pt) > breite {
                let mut k = rest.len();
                while k > 0 && (!rest.is_char_boundary(k) || f.width(&rest[..k], pt) > breite) {
                    k -= 1;
                }
                let k = k.max(rest.chars().next().map_or(1, char::len_utf8));
                zeilen.push(rest[..k].to_string());
                rest = rest[k..].to_string();
            }
            zeile = rest;
        }
        if !zeile.is_empty() {
            zeilen.push(zeile);
        }
    }
    if zeilen.is_empty() {
        zeilen.push(String::new());
    }
    zeilen
}

/// Schriften des Blatts.
#[derive(Clone, Copy)]
pub struct Schriften<'a> {
    pub regular: &'a Font,
    pub fett: &'a Font,
}

impl Schriften<'_> {
    fn f(&self, fett: bool) -> &Font {
        if fett {
            self.fett
        } else {
            self.regular
        }
    }
}

/// Zeichnet auf eine Seite; x in mm ab dem linken Rand, y in pt.
struct Stift<'a> {
    s: Schriften<'a>,
    ops: Vec<Op>,
}

impl Stift<'_> {
    fn x(v: f32) -> f32 {
        mm(LINKS + v)
    }

    fn text(&mut self, x: f32, y: f32, pt: f32, fett: bool, grau: u8, text: &str) {
        if text.is_empty() {
            return;
        }
        self.ops.push(Op::Text {
            x: Self::x(x),
            y,
            pt,
            fett,
            grau,
            text: text.to_string(),
        });
    }

    fn rechts(&mut self, xr: f32, y: f32, pt: f32, fett: bool, grau: u8, text: &str) {
        let w = self.s.f(fett).width(text, pt);
        if text.is_empty() {
            return;
        }
        self.ops.push(Op::Text {
            x: Self::x(xr) - w,
            y,
            pt,
            fett,
            grau,
            text: text.to_string(),
        });
    }

    fn mitte(&mut self, xm: f32, y: f32, pt: f32, text: &str) {
        let w = self.s.regular.width(text, pt);
        self.ops.push(Op::Text {
            x: Self::x(xm) - w * 0.5,
            y,
            pt,
            fett: false,
            grau: LEISE,
            text: text.to_string(),
        });
    }

    /// Waagrechte Linie von `x0` bis `x1` (mm).
    fn linie(&mut self, x0: f32, x1: f32, y: f32, breite: f32) {
        self.ops.push(Op::Linie {
            x0: Self::x(x0),
            y0: y,
            x1: Self::x(x1),
            y1: y,
            breite,
            grau: 0,
        });
    }

    /// Leere Linie für den Bieter, rechtsbündig an `xr` (mm), `w` pt breit.
    fn leer(&mut self, xr: f32, y: f32, w: f32) {
        self.ops.push(Op::Linie {
            x0: Self::x(xr) - w,
            y0: y + 1.5,
            x1: Self::x(xr),
            y1: y + 1.5,
            breite: 0.5,
            grau: 0,
        });
    }

    fn seite(&mut self) -> Seite {
        Seite {
            ops: std::mem::take(&mut self.ops),
        }
    }
}

/// Ein unteilbares Stück im Fluss der LV-Seiten.
enum Stueck<'a> {
    Titel(&'a LvTitel, Vec<String>),
    Untertitel(String, Vec<String>),
    Position(&'a LvPosition, Vec<String>),
    /// „Summe 01.02 Erdgeschoss“.
    UntertitelSumme(String, Option<Cent>),
    /// „Summe 01 Betonarbeiten“, `(unvollständig)`.
    TitelSumme(String, Option<Cent>, bool),
}

impl Stueck<'_> {
    fn hoehe(&self) -> f32 {
        match self {
            Stueck::Titel(_, z) => 10.0 + z.len() as f32 * 13.0 + 3.0,
            Stueck::Untertitel(_, z) => 4.0 + z.len() as f32 * ZEILE + 2.0,
            Stueck::Position(_, z) => z.len() as f32 * ZEILE + 3.0,
            Stueck::UntertitelSumme(..) => ZEILE + 5.0,
            Stueck::TitelSumme(..) => ZEILE + 9.0,
        }
    }

    /// Steht nie allein unten: nimmt das nächste Stück mit.
    fn mit_naechstem(&self) -> bool {
        matches!(self, Stueck::Titel(..) | Stueck::Untertitel(..))
    }

    /// Steht nie allein oben: nimmt das vorige Stück mit.
    fn mit_vorigem(&self) -> bool {
        matches!(self, Stueck::UntertitelSumme(..) | Stueck::TitelSumme(..))
    }
}

/// Die Seiten des LV ohne Titelblatt und Verzeichnis, mit den Seiten (ab 0)
/// der Einträge.
struct Fluss<'a> {
    lv: &'a Lv,
    a: &'a Angaben,
    w: Wahl,
    st: Stift<'a>,
    seiten: Vec<Seite>,
    y: f32,
    inhalt: Vec<Eintrag>,
    /// Seiten vor dem LV (Titelblatt, Verzeichnis): für „Übertrag von
    /// Seite n“ wie in der Fußzeile.
    versatz: usize,
}

/// Unterkante des Inhalts (pt): über der Fußzeile.
fn unten() -> f32 {
    A4.1 - mm(RAND) - 22.0
}

impl<'a> Fluss<'a> {
    fn s(&self) -> Schriften<'a> {
        self.st.s
    }

    fn fassung(&self) -> &'static str {
        self.lv.kopf.art
    }

    fn los_text(&self) -> String {
        los_text(self.lv)
    }

    fn eintrag(&mut self, text: String, tief: bool) {
        let seite = self.seiten.len();
        self.inhalt.push(Eintrag { text, tief, seite });
    }

    /// Betrag oder, für die Anfrage, eine leere Linie.
    fn betrag(&mut self, xr: f32, y: f32, fett: bool, wert: Option<Cent>, linie: f32) {
        if self.w.preise {
            if let Some(c) = wert {
                self.st.rechts(xr, y, TEXT, fett, 0, &c.deutsch());
            }
        } else {
            self.st.leer(xr, y, linie);
        }
    }

    /// Kopf auf Seite 1 (§2, Projektdaten §5).
    fn kopf_seite1(&mut self) {
        let s = self.s();
        let k = &self.lv.kopf;
        let mut y = mm(RAND) + 16.0;
        self.st.text(0.0, y, 16.0, true, 0, "Leistungsverzeichnis");
        y += 16.0;
        let oben = y;
        // Links: Bauvorhaben, Projekt, Projekt-Nr., Bauherr, Aufsteller
        let wert_x = 24.0;
        let wert_w = mm(72.0);
        let mut links: Vec<(&str, Vec<String>, bool)> = Vec::new();
        links.push(("Bauvorhaben", vec![self.a.bauvorhaben.clone()], false));
        let projekt: Vec<String> = [k.projektart.as_deref(), k.bauort.as_deref()]
            .into_iter()
            .flatten()
            .map(einzeilig)
            .filter(|t| !t.is_empty())
            .collect();
        if !projekt.is_empty() {
            // Der Bauort ist vertragswichtig: Umbruch, nie gekürzt
            links.push((
                "Projekt",
                umbrechen_glieder(s.regular, &projekt.join(" · "), TEXT, wert_w),
                false,
            ));
        }
        if let Some(n) = k.projektnummer.as_deref().map(einzeilig) {
            links.push(("Projekt-Nr.", vec![n], false));
        }
        for (label, name, anschrift) in [
            ("Bauherr", &k.bauherr, &k.bauherr_anschrift),
            ("Aufsteller", &k.aufsteller, &k.aufsteller_anschrift),
        ] {
            match name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                Some(n) => {
                    let mut z = vec![n.to_string()];
                    if let Some(a) = anschrift {
                        z.extend(
                            a.lines()
                                .map(str::trim)
                                .filter(|l| !l.is_empty())
                                .map(String::from),
                        );
                    }
                    links.push((label, z, false));
                }
                // Fehlt er, eine leere Linie, nie ein Platzhalter
                None => links.push((label, vec![String::new()], true)),
            }
        }
        for (label, zeilen, leer) in links {
            self.st.text(0.0, y, KLEIN, false, LEISE, label);
            if leer {
                self.st.linie(wert_x, wert_x + 60.0, y + 1.5, 0.5);
            }
            for z in zeilen {
                for t in umbrechen(s.regular, &z, TEXT, wert_w) {
                    self.st.text(wert_x, y, TEXT, false, 0, &t);
                    y += ZEILE;
                }
            }
            y += 2.0;
        }
        // Rechts: Los, Umfang, Stand, Fassung, Währung, Preisquelle
        let rx = 100.0;
        let rw = 16.0;
        let rbreit = mm(BREITE - rx - rw);
        let mut ry = oben;
        let zeile = |st: &mut Stift, label: &str, text: &str, fett: bool, ry: &mut f32| {
            st.text(rx, *ry, KLEIN, false, LEISE, label);
            for t in umbrechen(s.regular, text, TEXT, rbreit) {
                st.text(rx + rw, *ry, TEXT, fett, 0, &t);
                *ry += ZEILE;
            }
            *ry += 2.0;
        };
        let los = self.los_text();
        let fassung = self.fassung();
        let a = self.a;
        zeile(&mut self.st, "Los", &los, true, &mut ry);
        zeile(&mut self.st, "Umfang", &a.umfang, false, &mut ry);
        zeile(&mut self.st, "Stand", &a.stand, false, &mut ry);
        zeile(&mut self.st, "", fassung, true, &mut ry);
        let netto = format!("{}, {}", k.waehrung, k.netto);
        zeile(&mut self.st, "", &netto, false, &mut ry);
        if self.w.preise {
            if let Some(q) = a.preisquelle.as_deref() {
                // „Preise  Firmenkatalog …“, nicht „Preise  Preise …“ (Hinweis S)
                let q = q.strip_prefix("Preise ").unwrap_or(q);
                zeile(&mut self.st, "Preise", q, false, &mut ry);
            }
        }
        y = y.max(ry) + 8.0;
        // Vorbemerkungen im Wortlaut; im Verzeichnis nur, wenn es welche gibt
        if let Some(v) = k.vorbemerkungen.clone() {
            self.eintrag("Vorbemerkungen".into(), false);
            self.st.text(0.0, y, TEXT, true, 0, "Vorbemerkungen");
            y += ZEILE + 1.0;
            for t in umbrechen(s.regular, &v, TEXT, mm(BREITE)) {
                self.st.text(0.0, y, TEXT, false, 0, &t);
                y += ZEILE;
            }
            y += 4.0;
        }
        self.st.linie(0.0, BREITE, y - ZEILE + 6.0, 0.5);
        self.y = y + 6.0;
    }

    /// Kurzkopf der Folgeseiten: „Haus · Projekt-Nr. 01/26 · LV 1 Rohbau ·
    /// Stand 09.10.2026 · Anfrage ohne Preise“.
    fn kurzkopf(&mut self) {
        let mut teile = vec![self.a.bauvorhaben.clone()];
        if let Some(n) = self.lv.kopf.projektnummer.as_deref().map(einzeilig) {
            teile.push(format!("Projekt-Nr. {n}"));
        }
        teile.push(self.los_text());
        teile.push(format!("Stand {}", self.a.stand));
        teile.push(self.fassung().to_string());
        let y = mm(RAND) + 8.0;
        self.st
            .text(0.0, y, KLEIN, false, LEISE, &teile.join(" · "));
        self.y = y + 14.0;
    }

    fn spaltenkopf(&mut self) {
        let y = self.y + 2.0;
        let st = &mut self.st;
        st.text(0.0, y, KLEIN, false, LEISE, "OZ");
        st.text(KURZ_X, y, KLEIN, false, LEISE, "Kurztext");
        st.rechts(MENGE_R, y, KLEIN, false, LEISE, "Menge");
        st.text(ME_X, y, KLEIN, false, LEISE, "ME");
        st.rechts(EP_R, y, KLEIN, false, LEISE, "EP");
        st.rechts(GP_R, y, KLEIN, false, LEISE, "GP");
        st.linie(0.0, BREITE, y + 4.0, 0.5);
        self.y = y + 4.0 + 12.0;
    }

    fn neue_seite(&mut self) {
        let s = self.st.seite();
        self.seiten.push(s);
        self.kurzkopf();
    }

    /// Übertrag unten: „Übertrag 01 Betonarbeiten“ mit der laufenden Summe;
    /// fehlt ein Preis, die bekannten GP und „(unvollständig)“ wie die
    /// Titelsumme.
    fn uebertrag_unten(&mut self, titel: &LvTitel, (summe, fehlt): (Cent, bool)) {
        let y = unten() - 2.0;
        self.st.linie(EP_R + 2.0, GP_R, y - ZEILE + 2.5, 0.5);
        let mut text = format!("Übertrag {} {}", titel.nr, titel.name);
        if fehlt && self.w.preise {
            text.push_str(" (unvollständig)");
        }
        self.st.text(KURZ_X, y, TEXT, false, 0, &text);
        self.betrag(GP_R, y, false, Some(summe), LINIE_GP);
    }

    /// Übertrag oben auf der nächsten Seite, unter den Spaltenköpfen; `von`
    /// zählt wie die Fußzeile.
    fn uebertrag_oben(&mut self, von: usize, (summe, fehlt): (Cent, bool)) {
        let y = self.y;
        let mut text = format!("Übertrag von Seite {von}");
        if fehlt && self.w.preise {
            text.push_str(" (unvollständig)");
        }
        self.st.text(KURZ_X, y, TEXT, false, 0, &text);
        self.betrag(GP_R, y, false, Some(summe), LINIE_GP);
        self.y = y + ZEILE + 4.0;
    }

    fn zeichne(&mut self, x: &Stueck) {
        let s = self.s();
        match x {
            Stueck::Titel(t, z) => {
                let mut y = self.y + 10.0 + TITEL;
                self.st.text(0.0, y, TITEL, true, 0, &t.nr);
                for l in z {
                    self.st.text(KURZ_X, y, TITEL, true, 0, l);
                    y += 13.0;
                }
            }
            Stueck::Untertitel(oz, z) => {
                let mut y = self.y + 4.0 + TEXT;
                self.st.text(0.0, y, TEXT, true, 0, oz);
                for l in z {
                    self.st.text(KURZ_X, y, TEXT, true, 0, l);
                    y += ZEILE;
                }
            }
            Stueck::Position(p, z) => {
                let y = self.y + TEXT;
                self.st.text(0.0, y, TEXT, false, 0, &p.oz);
                for (i, l) in z.iter().enumerate() {
                    self.st
                        .text(KURZ_X, y + i as f32 * ZEILE, TEXT, false, 0, l);
                }
                // Menge in der ersten Zeile, auch wenn der Kurztext umbricht
                self.st.rechts(MENGE_R, y, TEXT, false, 0, &menge(p));
                self.st.text(ME_X, y, TEXT, false, 0, p.einheit.zeichen());
                if self.w.preise {
                    // Ohne Preis (K12) bleiben EP und GP leer, ohne Linie
                    if let (Some(ep), Some(gp)) = (p.ep, p.gp) {
                        self.st.rechts(EP_R, y, TEXT, false, 0, &ep.deutsch());
                        self.st.rechts(GP_R, y, TEXT, false, 0, &gp.deutsch());
                    }
                } else {
                    self.st.leer(EP_R, y, LINIE_EP);
                    self.st.leer(GP_R, y, LINIE_GP);
                }
            }
            Stueck::UntertitelSumme(label, summe) => {
                let y = self.y + 2.0 + TEXT;
                self.st.text(KURZ_X, y, TEXT, false, 0, label);
                self.betrag(GP_R, y, false, *summe, LINIE_GP);
            }
            Stueck::TitelSumme(label, summe, unvollst) => {
                let y = self.y + 6.0 + TEXT;
                self.st.linie(EP_R + 2.0, GP_R, y - TEXT - 2.5, 0.5);
                let mut text = label.clone();
                if *unvollst && self.w.preise {
                    text.push_str(" (unvollständig)");
                }
                let breit = mm(EP_R - KURZ_X);
                let z = umbrechen(s.fett, &text, TEXT, breit);
                self.st.text(KURZ_X, y, TEXT, true, 0, &z[0]);
                self.betrag(GP_R, y, true, *summe, LINIE_GP);
            }
        }
        self.y += x.hoehe();
    }

    /// Stücke des LV in Lesereihenfolge.
    fn stuecke(&self) -> Vec<Stueck<'a>> {
        let s = self.s();
        let lv = self.lv;
        let kurz_w = mm(KURZ_W) - 4.0;
        let titel_w = mm(EP_R - KURZ_X);
        let mut v = Vec::new();
        for t in lv.titel.iter().filter(|t| !t.positionen.is_empty()) {
            v.push(Stueck::Titel(t, umbrechen(s.fett, &t.name, TITEL, titel_w)));
            let mut uu: Option<u32> = None;
            for p in &t.positionen {
                if p.untertitel != uu {
                    if let Some(u) = t.untertitel.iter().find(|u| Some(u.nr) == uu) {
                        v.push(Stueck::UntertitelSumme(
                            format!("Summe {} {}", u.oz, u.name),
                            u.summe,
                        ));
                    }
                    uu = p.untertitel;
                    if let Some(u) = t.untertitel.iter().find(|u| Some(u.nr) == uu) {
                        let z = umbrechen(s.fett, &u.name, TEXT, titel_w);
                        v.push(Stueck::Untertitel(u.oz.clone(), z));
                    }
                }
                let z = umbrechen(s.regular, &p.kurztext, TEXT, kurz_w);
                v.push(Stueck::Position(p, z));
            }
            if let Some(u) = t.untertitel.iter().find(|u| Some(u.nr) == uu) {
                v.push(Stueck::UntertitelSumme(
                    format!("Summe {} {}", u.oz, u.name),
                    u.summe,
                ));
            }
            v.push(Stueck::TitelSumme(
                format!("Summe {} {}", t.nr, t.name),
                t.summe,
                t.unvollstaendig,
            ));
        }
        v
    }

    /// Die LV-Seiten mit Umbruch und Übertrag (§5).
    fn positionen(&mut self) {
        let stuecke = self.stuecke();
        // Unteilbare Gruppen: Titel mit der ersten Position, Summe mit der
        // letzten
        let mut gruppen: Vec<Vec<&Stueck>> = Vec::new();
        for x in &stuecke {
            let anhaengen = gruppen
                .last()
                .and_then(|g| g.last())
                .is_some_and(|l| l.mit_naechstem())
                || x.mit_vorigem();
            match gruppen.last_mut() {
                Some(g) if anhaengen => g.push(x),
                _ => gruppen.push(vec![x]),
            }
        }
        let reserve = ZEILE + 6.0;
        // Offener Titel und seine laufende Summe
        let mut offen: Option<&LvTitel> = None;
        // Bekannte GP und ob einer fehlt (K12)
        let mut lauf = (Cent(0), false);
        for g in gruppen {
            let h: f32 = g.iter().map(|x| x.hoehe()).sum();
            let schliesst = g.iter().any(|x| matches!(x, Stueck::TitelSumme(..)));
            let oeffnet = g.iter().any(|x| matches!(x, Stueck::Titel(..)));
            let bleibt_offen = (offen.is_some() || oeffnet) && !schliesst;
            let platz = unten() - if bleibt_offen { reserve } else { 0.0 };
            if self.y + h > platz && self.y > mm(RAND) + 60.0 {
                let von = self.versatz + self.seiten.len() + 1;
                if let Some(t) = offen {
                    self.uebertrag_unten(t, lauf);
                }
                self.neue_seite();
                self.spaltenkopf();
                if offen.is_some() {
                    self.uebertrag_oben(von, lauf);
                }
            }
            for x in g {
                match x {
                    Stueck::Titel(t, _) => {
                        offen = Some(t);
                        lauf = (Cent(0), false);
                        self.eintrag(format!("{} {}", t.nr, t.name), false);
                    }
                    Stueck::Untertitel(oz, z) => {
                        self.eintrag(format!("{oz} {}", z.join(" ")), true);
                    }
                    Stueck::Position(p, _) => {
                        // Unvollständig wie die Titelsumme: auch ein GP mit
                        // fehlendem Artikelpreis (Review 3bo)
                        let fehlt = lauf.1 || p.preis_fehlt || p.mehrere_preise;
                        lauf = match p.gp {
                            Some(b) => (Cent(lauf.0 .0 + b.0), fehlt),
                            None => (lauf.0, true),
                        };
                    }
                    Stueck::TitelSumme(..) => offen = None,
                    Stueck::UntertitelSumme(..) => {}
                }
                self.zeichne(x);
            }
        }
    }

    /// Zusammenstellung auf einer neuen Seite (§6), dann bei der Anfrage
    /// der Bieterblock.
    fn zusammenstellung(&mut self) {
        let z = &self.lv.zusammenstellung;
        self.neue_seite();
        self.eintrag("Zusammenstellung".into(), false);
        let mut y = self.y + 10.0 + TITEL;
        let kopf = format!("Zusammenstellung · {}", self.los_text());
        self.st.text(0.0, y, TITEL, true, 0, &kopf);
        y += 10.0;
        self.st.linie(0.0, BREITE, y, 0.5);
        y += 6.0 + TEXT;
        let zeilen: Vec<(String, String, Option<Cent>)> = z.zeilen.clone();
        let fortsetzung = |f: &mut Self| {
            f.neue_seite();
            let mut y = f.y + 10.0 + TITEL;
            let kopf = format!("Zusammenstellung · {} (Fortsetzung)", f.los_text());
            f.st.text(0.0, y, TITEL, true, 0, &kopf);
            y += 10.0;
            f.st.linie(0.0, BREITE, y, 0.5);
            y + 6.0 + TEXT
        };
        for (nr, name, summe) in zeilen {
            if y > unten() - ZEILE {
                y = fortsetzung(self);
            }
            self.st.text(0.0, y, TEXT, false, 0, &nr);
            self.st.text(KURZ_X, y, TEXT, false, 0, &name);
            self.betrag(GP_R, y, false, summe, LINIE_GP);
            y += ZEILE;
            // Untertitel eingerückt, ihr Betrag in der EP-Spalte: nicht
            // doppelt in der Summe
            if let Some(t) = self.lv.titel.iter().find(|t| t.nr == nr) {
                for u in &t.untertitel {
                    self.st.text(KURZ_X + 5.0, y, TEXT, false, LEISE, &u.oz);
                    self.st.text(KURZ_X + 22.0, y, TEXT, false, LEISE, &u.name);
                    if self.w.preise {
                        if let Some(c) = u.summe {
                            self.st.rechts(EP_R, y, TEXT, false, LEISE, &c.deutsch());
                        }
                    }
                    y += ZEILE;
                }
            }
        }
        if y > unten() - 5.0 * ZEILE {
            y = fortsetzung(self);
        }
        // netto mit Linie, MwSt., brutto mit Doppellinie
        y += 4.0;
        self.st.linie(EP_R + 2.0, GP_R, y - TEXT - 2.5, 0.5);
        self.st.text(KURZ_X, y, TEXT, true, 0, "Summe netto");
        self.betrag(GP_R, y, true, z.netto, LINIE_GP);
        y += ZEILE;
        let satz = z.mwst_satz.text().replace('.', ",");
        self.st
            .text(KURZ_X, y, TEXT, false, 0, &format!("zzgl. {satz} % MwSt."));
        self.betrag(GP_R, y, false, z.mwst, LINIE_GP);
        y += ZEILE + 4.0;
        self.st.linie(EP_R + 2.0, GP_R, y - TEXT - 3.5, 0.5);
        self.st.linie(EP_R + 2.0, GP_R, y - TEXT - 2.0, 0.5);
        self.st.text(KURZ_X, y, TEXT, true, 0, "Summe brutto");
        self.betrag(GP_R, y, true, z.brutto, LINIE_GP);
        y += ZEILE + 4.0;
        if self.w.preise {
            if let Some(g) = z.geschaetzt {
                let t = format!(
                    "Nicht ausgeschrieben (geschätzt): {} €, nicht in der Summe",
                    g.deutsch()
                );
                self.st.text(KURZ_X, y, TEXT, false, LEISE, &t);
                y += ZEILE;
            }
            if z.unvollstaendig {
                let t = "Unvollständig: Nicht jede Position hat einen Preis.";
                self.st.text(KURZ_X, y, TEXT, false, 0, t);
                y += ZEILE;
            }
        }
        self.y = y;
        if !self.w.preise {
            self.bieterblock();
        }
    }

    /// Drei Linien für den Bieter (§8).
    fn bieterblock(&mut self) {
        let h = 3.0 * 34.0 + 20.0;
        if self.y + h > unten() {
            self.neue_seite();
        }
        self.eintrag("Angaben des Bieters".into(), false);
        let mut y = self.y + 16.0;
        self.st.text(0.0, y, TEXT, true, 0, "Angaben des Bieters");
        for label in [
            "Firma, Anschrift",
            "Ort, Datum",
            "Unterschrift und Stempel des Bieters",
        ] {
            y += 30.0;
            self.st.linie(0.0, 110.0, y, 0.5);
            self.st.text(0.0, y + 9.0, KLEIN, false, LEISE, label);
        }
        self.y = y + 14.0;
    }
}

/// Titelblatt (Schalter): ohne Kurzkopf und Fußzeile, zählt als Seite 1.
fn titelblatt(lv: &Lv, a: &Angaben, s: Schriften) -> Seite {
    let mut st = Stift { s, ops: Vec::new() };
    let k = &lv.kopf;
    let mut y = mm(95.0);
    st.text(0.0, y, 26.0, true, 0, "Leistungsverzeichnis");
    y += 26.0;
    let los = format!("LV {} {}", k.los_nr, k.los);
    st.text(0.0, y, 16.0, false, 0, los.trim());
    y += 40.0;
    let zeile = |st: &mut Stift, label: &str, text: &str, y: &mut f32| {
        st.text(0.0, *y, KLEIN, false, LEISE, label);
        for l in text.lines().filter(|l| !l.trim().is_empty()) {
            for t in umbrechen(s.regular, l.trim(), 11.0, mm(120.0)) {
                st.text(35.0, *y, 11.0, false, 0, &t);
                *y += 14.0;
            }
        }
        *y += 6.0;
    };
    let mit = |name: &Option<String>, anschrift: &Option<String>| {
        let mut t = name.clone().unwrap_or_default();
        if let Some(a) = anschrift {
            t.push('\n');
            t.push_str(a);
        }
        t
    };
    let felder = [
        ("Bauvorhaben", a.bauvorhaben.clone()),
        ("Projektart", k.projektart.clone().unwrap_or_default()),
        ("Bauort", k.bauort.clone().unwrap_or_default()),
        ("Projekt-Nr.", k.projektnummer.clone().unwrap_or_default()),
        ("Bauherr", mit(&k.bauherr, &k.bauherr_anschrift)),
        ("Planung", mit(&k.aufsteller, &k.aufsteller_anschrift)),
        ("Fassung", k.art.to_string()),
        ("Stand", a.stand.clone()),
    ];
    for (label, text) in felder {
        if !text.trim().is_empty() {
            zeile(&mut st, label, &text, &mut y);
        }
    }
    st.seite()
}

/// Inhaltsverzeichnis: Einträge mit Punktlinie und Seitenzahl rechts.
fn verzeichnis(inhalt: &[Eintrag], s: Schriften, je_seite: usize) -> Vec<Seite> {
    let mut seiten = Vec::new();
    for (i, teil) in inhalt.chunks(je_seite.max(1)).enumerate() {
        let mut st = Stift { s, ops: Vec::new() };
        let mut y = mm(RAND) + 20.0;
        if i == 0 {
            st.text(0.0, y, 14.0, true, 0, "Inhalt");
        }
        y += 24.0;
        for e in teil {
            let x = if e.tief { 8.0 } else { 0.0 };
            st.text(x, y, TEXT, !e.tief, 0, &e.text);
            let n = e.seite.to_string();
            st.rechts(BREITE, y, TEXT, false, 0, &n);
            // Punktlinie zwischen Text und Zahl
            let von = mm(LINKS + x) + s.f(!e.tief).width(&e.text, TEXT) + 4.0;
            let bis = mm(LINKS + BREITE) - s.regular.width(&n, TEXT) - 4.0;
            let paar = s.regular.width(". ", TEXT).max(1.0);
            let anzahl = ((bis - von) / paar).max(0.0) as usize;
            if anzahl > 0 {
                let punkte = ". ".repeat(anzahl);
                let w = s.regular.width(&punkte, TEXT);
                st.ops.push(Op::Text {
                    x: bis - w,
                    y,
                    pt: TEXT,
                    fett: false,
                    grau: LEISE,
                    text: punkte,
                });
            }
            y += 16.0;
        }
        seiten.push(st.seite());
    }
    seiten
}

/// Einträge je Verzeichnisseite.
fn je_verzeichnisseite() -> usize {
    ((unten() - mm(RAND) - 44.0) / 16.0) as usize
}

/// Das ganze Blatt: Titelblatt und Verzeichnis nach Wahl, Seite 1 mit Kopf,
/// LV-Seiten, Zusammenstellung, Bieterblock; Fußzeile auf jeder Seite außer
/// dem Titelblatt.
pub fn blatt(lv: &Lv, a: &Angaben, w: Wahl, s: Schriften) -> Blatt {
    let fluss = |versatz: usize| {
        let mut f = Fluss {
            lv,
            a,
            w,
            st: Stift { s, ops: Vec::new() },
            seiten: Vec::new(),
            y: 0.0,
            inhalt: Vec::new(),
            versatz,
        };
        f.kopf_seite1();
        f.spaltenkopf();
        f.positionen();
        f.zusammenstellung();
        let letzte = f.st.seite();
        f.seiten.push(letzte);
        f
    };
    // Erst ohne Verzeichnis umbrechen, dann dessen Seiten davorrechnen;
    // der zweite Lauf bricht gleich um und zählt die Überträge wie die
    // Fußzeile (Kosten A6)
    let je = je_verzeichnisseite();
    let n_verz = if w.verzeichnis {
        fluss(0).inhalt.len().div_ceil(je)
    } else {
        0
    };
    let davor = usize::from(w.titelblatt) + n_verz;
    let f = fluss(davor);
    let mut inhalt = f.inhalt;
    for e in &mut inhalt {
        e.seite += davor + 1;
    }
    let mut seiten = Vec::new();
    if w.titelblatt {
        seiten.push(titelblatt(lv, a, s));
    }
    if w.verzeichnis {
        seiten.extend(verzeichnis(&inhalt, s, je));
    }
    seiten.extend(f.seiten);
    // Fußzeile: links Bauvorhaben · Projekt-Nr., Mitte Planung, rechts
    // „Seite x von y“
    let y_seiten = seiten.len();
    let mut links = a.bauvorhaben.clone();
    if let Some(n) = lv.kopf.projektnummer.as_deref().map(einzeilig) {
        links.push_str(&format!(" · Projekt-Nr. {n}"));
    }
    let planung = lv
        .kopf
        .aufsteller
        .as_deref()
        .map(|p| format!("Planung: {}", einzeilig(p)));
    for (i, seite) in seiten.iter_mut().enumerate() {
        if w.titelblatt && i == 0 {
            continue;
        }
        let mut st = Stift { s, ops: Vec::new() };
        let y = A4.1 - mm(RAND);
        st.linie(0.0, BREITE, y - 10.0, 0.4);
        st.text(0.0, y, KLEIN, false, LEISE, &links);
        if let Some(p) = &planung {
            st.mitte(BREITE * 0.5, y, KLEIN, p);
        }
        let n = format!("Seite {} von {y_seiten}", i + 1);
        st.rechts(BREITE, y, KLEIN, false, LEISE, &n);
        seite.ops.extend(st.ops);
    }
    Blatt { seiten, inhalt }
}

#[cfg(test)]
mod tests;

//! Maske „Projektdaten“ (Paket PD-2, architektur/paket-projektdaten.md §4,
//! Sollbild soll-projektdaten): bei Datei › Neu und über den Knopf
//! „Projektdaten“ im linken Paneel. Ein Blatt mit drei Gruppen; jedes Feld
//! darf leer bleiben. „Übernehmen“ gibt die Projektdaten zurück, die App
//! setzt sie (bei „Neu“ ohne Rückgängig-Schritt, sonst als ein Schritt).
//! Die Maske zeigt keinen Befund und sperrt nichts.

use sk_model::{Location, Project};
use sk_paint::{Canvas, Path};
use sk_platform::{Key, Modifiers};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, Fonts, Rect};

/// Breite des Blatts und Ränder (dip).
const W: f32 = 520.0;
const PAD: f32 = 22.0;
/// Linker Rand der Felder (dip), davor die Bezeichnung.
const FELD_X: f32 = 134.0;
/// Kopf mit Titel, Fuß mit Satz und Knöpfen (dip).
const KOPF_H: f32 = 54.0;
const FUSS_H: f32 = 58.0;
/// Gruppenüberschrift, Feldzeile, Zeile in mehrzeiligen Feldern, Abstand.
const GRUPPE_H: f32 = 30.0;
const ZEILE_H: f32 = 26.0;
const TEXTZEILE: f32 = 18.0;
const LUFT: f32 = 10.0;
/// Knöpfe unten rechts.
const KNOPF_H: f32 = 30.0;
const KNOPF_W: [f32; 2] = [110.0, 130.0];
/// Zeile der Vorschlagsliste.
const LISTE_ZEILE: f32 = 26.0;

pub const LEER_SATZ: &str = "Jedes Feld darf leer bleiben.";
pub const VOM_LETZTEN: &str = "vom letzten Projekt";
pub const VORSCHLAEGE: [&str; 6] = [
    "Neubau Einfamilienhaus",
    "Neubau Doppelhaushälfte",
    "Neubau Mehrfamilienhaus",
    "Anbau",
    "Umbau",
    "Sanierung",
];

/// Ein Feld: Bezeichnung, leises Beispiel, Zeilen, höchstens so viele
/// Zeichen, Breite (dip; 0 = bis zum Rand), linker Rand (dip; 0 = unter
/// den anderen, sonst neben dem vorigen Feld der [`REIHE`] mit der
/// Bezeichnung davor) und Einheit dahinter.
struct Feld {
    name: &'static str,
    beispiel: &'static str,
    zeilen: usize,
    max: usize,
    breite: f32,
    x: f32,
    einheit: &'static str,
}

/// Gruppen ab 0 (Bauvorhaben), 4 (Bauherr), 6 (Planung). Breite und Länge
/// (Sonnenstand S1) stehen hinten, damit die Nummern 0–7 bleiben (der
/// AVA-Kopf öffnet die Maske an einem Feld); gezeigt und mit Tab erreicht
/// werden sie unter dem Bauort ([`REIHE`]). Leise vorbelegt ist der
/// Standardort Ganderkesee.
const FELDER: [Feld; 10] = [
    Feld {
        name: "Projektart",
        beispiel: "z. B. Neubau Einfamilienhaus",
        zeilen: 1,
        max: 70,
        breite: 0.0,
        x: 0.0,
        einheit: "",
    },
    Feld {
        name: "Bezeichnung",
        beispiel: "z. B. Haus Mustermann",
        zeilen: 1,
        max: 120,
        breite: 0.0,
        x: 0.0,
        einheit: "",
    },
    Feld {
        name: "Bauort",
        beispiel: "z. B. Musterweg 1\n12345 Musterstadt",
        zeilen: 2,
        max: 200,
        breite: 0.0,
        x: 0.0,
        einheit: "",
    },
    Feld {
        name: "Projektnummer",
        beispiel: "z. B. 01/26",
        zeilen: 1,
        max: 20,
        breite: 110.0,
        x: 0.0,
        einheit: "",
    },
    Feld {
        name: "Name",
        beispiel: "z. B. Max Mustermann",
        zeilen: 1,
        max: 120,
        breite: 0.0,
        x: 0.0,
        einheit: "",
    },
    Feld {
        name: "Anschrift",
        beispiel: "z. B. Musterstraße 5\n12345 Musterstadt",
        zeilen: 3,
        max: 200,
        breite: 0.0,
        x: 0.0,
        einheit: "",
    },
    Feld {
        name: "Name",
        beispiel: "z. B. Dipl.-Ing. Erika Muster",
        zeilen: 1,
        max: 120,
        breite: 0.0,
        x: 0.0,
        einheit: "",
    },
    Feld {
        name: "Anschrift",
        beispiel: "z. B. Planerweg 2\n12345 Musterstadt",
        zeilen: 3,
        max: 200,
        breite: 0.0,
        x: 0.0,
        einheit: "",
    },
    Feld {
        // Breiten- und Längengrad in einer Zeile „Lage“ (Bedienbarkeit
        // 28.1: „Breite“ und „Länge“ heißen sonst Bauteilmaße)
        name: "Lage",
        beispiel: "53,0589",
        zeilen: 1,
        max: 12,
        breite: 96.0,
        x: 0.0,
        einheit: "° N",
    },
    Feld {
        name: "",
        beispiel: "8,591",
        zeilen: 1,
        max: 12,
        breite: 96.0,
        x: 276.0,
        einheit: "° O",
    },
];
/// Felder in Lese- und Tab-Reihenfolge.
const REIHE: [usize; 10] = [0, 1, 2, 8, 9, 3, 4, 5, 6, 7];
const GRUPPEN: [(usize, &str); 3] = [(0, "BAUVORHABEN"), (4, "BAUHERR"), (6, "PLANUNG")];
/// Wort des Felds für Meldungen (Bezeichnung mit Gruppe, wo doppelt).
const WORT: [&str; 10] = [
    "Projektart",
    "Bezeichnung",
    "Bauort",
    "Projektnummer",
    "Bauherr",
    "Anschrift Bauherr",
    "Planung",
    "Anschrift Planung",
    "Breitengrad",
    "Längengrad",
];

/// Breite (Feld 8) und Länge (Feld 9): Grenze in Grad.
const GRAD_MAX: [f64; 2] = [90.0, 180.0];

/// Gradzahl aus einem Feld: Komma oder Punkt, Minus, ein `°` dahinter
/// erlaubt. `Ok(None)` für leer, `Err` für keine Zahl im Bereich.
fn grad(text: &str, max: f64) -> Result<Option<f64>, ()> {
    let t = text.trim();
    let t = t.strip_suffix('°').unwrap_or(t).trim_end();
    if t.is_empty() {
        return Ok(None);
    }
    let v: f64 = t
        .replace(',', ".")
        .replace('−', "-")
        .parse()
        .map_err(|_| ())?;
    (v.is_finite() && v.abs() <= max)
        .then_some(Some(v))
        .ok_or(())
}

/// Gradzahl für ein Feld, mit Komma.
/// Breite (Feld 8) bzw. Länge (Feld 9) von Ganderkesee: gilt, solange die
/// Datei keine Lage hat.
fn ganderkesee(i: usize) -> f64 {
    let g = sk_math::sonne::Lage::GANDERKESEE;
    if i == 8 {
        g.breite
    } else {
        g.laenge
    }
}

fn grad_text(v: Option<f64>) -> String {
    v.map_or_else(String::new, |v| v.to_string().replace('.', ","))
}

/// Was die Maus treffen kann.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ziel {
    Feld(usize),
    /// ▾ an der Projektart.
    Pfeil,
    /// Zeile der offenen Vorschlagsliste.
    Vorschlag(usize),
    /// „Später“ bzw. „Abbrechen“.
    Links,
    Uebernehmen,
    Schliessen,
}

/// Ausgang der Maske.
#[derive(Clone, Debug, PartialEq)]
pub enum Antwort {
    Uebernehmen(Box<Project>),
    /// „Später“, „Abbrechen“, Esc oder ×.
    Verwerfen,
}

/// Lage der Teile, relativ zum Blatt (Pixel).
struct Lage {
    felder: [Rect; 10],
    gruppen: [f32; 3],
    knoepfe: [Rect; 2],
    schliessen: Rect,
    h: f32,
}

pub struct Maske {
    /// Bei Datei › Neu: „Später“ statt „Abbrechen“.
    pub neu: bool,
    felder: [TextEdit; 10],
    fokus: usize,
    /// Vorschlagsliste offen: markierte Zeile der gefilterten Vorschläge.
    liste: Option<Option<usize>>,
    /// Planung vom letzten Projekt vorbelegt.
    vom_letzten: bool,
    basis: Project,
    /// Lage beim Öffnen; die Nordrichtung setzt die Maske nicht.
    ort: Location,
    hover: Option<Ziel>,
    pressed: Option<Ziel>,
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x.round(), y.round(), w.round(), h.round())
}

impl Maske {
    /// Maske mit den Werten von `p`. Bei `neu` und leerer Planung belegt
    /// `planung` (Name, Anschrift vom letzten Projekt) die zwei Felder vor.
    pub fn new(p: &Project, neu: bool, planung: Option<(String, String)>) -> Maske {
        let werte = [
            &p.kind,
            &p.site,
            &p.place,
            &p.number,
            &p.client,
            &p.client_addr,
            &p.author,
            &p.author_addr,
        ];
        let mut felder: [TextEdit; 10] = Default::default();
        for (f, w) in felder.iter_mut().zip(werte) {
            *f = TextEdit::new(w);
            f.end(false);
        }
        let mut vom_letzten = false;
        if let Some((name, anschrift)) = planung.filter(|_| neu) {
            if p.author.is_empty() && p.author_addr.is_empty() && !name.is_empty() {
                felder[6] = TextEdit::new(&name);
                felder[6].end(false);
                felder[7] = TextEdit::new(&anschrift);
                felder[7].end(false);
                vom_letzten = true;
            }
        }
        Maske {
            neu,
            felder,
            fokus: 0,
            liste: None,
            vom_letzten,
            basis: p.clone(),
            ort: Location::default(),
            hover: None,
            pressed: None,
        }
    }

    /// Breite und Länge aus der Lage des Projekts (Sonnenstand S1). Fehlt
    /// ein Wert, steht der geltende von Ganderkesee als echter Wert im Feld
    /// (Jörn 09.10. 14:10, S10).
    pub fn mit_ort(mut self, l: &Location) -> Maske {
        for (i, v) in [(8, l.lat), (9, l.lon)] {
            self.felder[i] = TextEdit::new(&grad_text(v.or(Some(ganderkesee(i)))));
            self.felder[i].end(false);
        }
        self.ort = *l;
        self
    }

    /// Die Lage aus Breite und Länge; ein Wert, der keine Zahl im Bereich
    /// ist, lässt den alten stehen (die Maske meldet ihn). Der unveränderte
    /// vorbelegte Wert bleibt ungesetzt, so ändert bloßes Übernehmen die
    /// Datei nicht.
    pub fn ort(&self) -> Location {
        let w = |i: usize, alt: Option<f64>| {
            let t = &self.felder[i].text;
            if alt.is_none() && *t == grad_text(Some(ganderkesee(i))) {
                return None;
            }
            grad(t, GRAD_MAX[i - 8]).unwrap_or(alt)
        };
        Location {
            lat: w(8, self.ort.lat),
            lon: w(9, self.ort.lon),
            ..self.ort
        }
    }

    /// Text eines Felds (Reihenfolge wie [`FELDER`]).
    #[cfg(test)]
    pub fn wert(&self, i: usize) -> &str {
        &self.felder[i].text
    }

    #[cfg(test)]
    pub fn fokus(&self) -> usize {
        self.fokus
    }

    #[cfg(test)]
    pub fn vom_letzten(&self) -> bool {
        self.vom_letzten
    }

    /// Die Projektdaten aus den Feldern.
    pub fn projekt(&self) -> Project {
        let w = |i: usize| self.felder[i].text.clone();
        Project {
            kind: w(0),
            site: w(1),
            place: w(2),
            number: w(3),
            client: w(4),
            client_addr: w(5),
            author: w(6),
            author_addr: w(7),
            ..self.basis.clone()
        }
    }

    /// Meldung zu einem Wert, der nicht passt (zu lang, Umbruch in einem
    /// einzeiligen Feld); er bleibt trotzdem erhalten (Regel 110).
    pub fn meldung(&self) -> Option<String> {
        REIHE.iter().find_map(|&i| {
            let (e, f, wort) = (&self.felder[i], &FELDER[i], WORT[i]);
            let n = e.text.chars().count();
            if n > f.max {
                Some(format!("{wort}: höchstens {} Zeichen.", f.max))
            } else if f.zeilen == 1 && e.text.contains('\n') {
                Some(format!("{wort}: nur eine Zeile."))
            } else if i >= 8 && grad(&e.text, GRAD_MAX[i - 8]).is_err() {
                let g = GRAD_MAX[i - 8];
                Some(format!("{wort}: Zahl von −{g} bis {g}."))
            } else {
                None
            }
        })
    }

    /// Leiser Satz im Fuß, wenn nur Breite oder nur Länge gesetzt ist: die
    /// andere kommt vom Standardort (Abnahme S1, Punkt 1).
    pub fn hinweis(&self) -> Option<String> {
        let g = |i: usize| grad(&self.felder[i].text, GRAD_MAX[i - 8]);
        let fehlt = match (g(8), g(9)) {
            (Ok(Some(_)), Ok(None)) => 9,
            (Ok(None), Ok(Some(_))) => 8,
            _ => return None,
        };
        // Der Wert von Ganderkesee steht als Beispiel im leeren Feld
        Some(format!("{} fehlt, es gilt Ganderkesee.", WORT[fehlt]))
    }

    /// Vorschläge zur Projektart, nach dem Getippten gefiltert (leer: alle).
    pub fn vorschlaege(&self) -> Vec<&'static str> {
        let t = self.felder[0].text.trim().to_lowercase();
        let alle: Vec<&str> = VORSCHLAEGE.to_vec();
        if t.is_empty() || VORSCHLAEGE.iter().any(|v| v.to_lowercase() == t) {
            return alle;
        }
        alle.into_iter()
            .filter(|v| v.to_lowercase().contains(&t))
            .collect()
    }

    #[cfg(test)]
    pub fn liste_offen(&self) -> bool {
        self.liste.is_some()
    }

    fn waehle(&mut self, k: usize) {
        if let Some(v) = self.vorschlaege().get(k) {
            self.felder[0] = TextEdit::new(v);
            self.felder[0].end(false);
        }
        self.liste = None;
    }

    /// Cursor ins Feld `i` (Reihenfolge wie [`FELDER`]), Inhalt markiert.
    pub fn fokus_auf(&mut self, i: usize) {
        if i != self.fokus {
            self.fokus = i;
            self.felder[i].select_all();
        }
        self.liste = None;
    }

    // --- Lage ----------------------------------------------------------------

    fn lage(&self, s: f32) -> Lage {
        let mut y = KOPF_H + 12.0;
        let mut felder = [Rect::new(0.0, 0.0, 0.0, 0.0); 10];
        let mut gruppen = [0.0; 3];
        for i in REIHE {
            let f = &FELDER[i];
            if let Some(g) = GRUPPEN.iter().position(|(a, _)| *a == i) {
                gruppen[g] = y * s;
                y += GRUPPE_H;
            }
            let h = if f.zeilen == 1 {
                ZEILE_H
            } else {
                TEXTZEILE * f.zeilen as f32 + 10.0
            };
            let w = if f.breite > 0.0 {
                f.breite
            } else {
                W - PAD - FELD_X
            };
            if f.x > 0.0 {
                // in der Zeile des vorigen Felds
                y -= h + LUFT;
            }
            let x = if f.x > 0.0 { f.x } else { FELD_X };
            felder[i] = rect(x * s, y * s, w * s, h * s);
            y += h + LUFT;
        }
        let fuss = y + 4.0;
        let h = fuss + FUSS_H;
        let ky = fuss + (FUSS_H - KNOPF_H) * 0.5;
        let mut x = W - PAD;
        let mut knoepfe = [Rect::new(0.0, 0.0, 0.0, 0.0); 2];
        for i in (0..2).rev() {
            x -= KNOPF_W[i];
            knoepfe[i] = rect(x * s, ky * s, KNOPF_W[i] * s, KNOPF_H * s);
            x -= 10.0;
        }
        Lage {
            felder,
            gruppen,
            knoepfe,
            schliessen: rect((W - PAD - 20.0) * s, 17.0 * s, 20.0 * s, 20.0 * s),
            h: h * s,
        }
    }

    /// Größe (Pixel) ohne Schatten.
    fn size(&self, s: f32) -> (f32, f32) {
        ((W * s).round(), self.lage(s).h.round())
    }

    /// Lage im Fenster: mittig unter der Titelleiste.
    pub fn rect(&self, s: f32, win_w: u32, win_h: u32, top: u32) -> Rect {
        let (w, h) = self.size(s);
        let x = ((win_w as f32 - w) * 0.5).round().max(0.0);
        let y = (top as f32 + (win_h as f32 - top as f32 - h) * 0.4)
            .round()
            .max(top as f32);
        Rect::new(x, y, w, h)
    }

    /// Zeilen der Vorschlagsliste (relativ zum Blatt).
    fn liste_rects(&self, s: f32) -> Vec<Rect> {
        if self.liste.is_none() {
            return Vec::new();
        }
        let f = self.lage(s).felder[0];
        (0..self.vorschlaege().len())
            .map(|k| {
                rect(
                    f.x,
                    f.y + f.h + 4.0 * s + 4.0 * s + k as f32 * LISTE_ZEILE * s,
                    f.w,
                    LISTE_ZEILE * s,
                )
            })
            .collect()
    }

    fn ziel(&self, r: Rect, s: f32, x: f64, y: f64) -> Option<Ziel> {
        let (lx, ly) = (x - r.x as f64, y - r.y as f64);
        if let Some(k) = self.liste_rects(s).iter().position(|z| z.contains(lx, ly)) {
            return Some(Ziel::Vorschlag(k));
        }
        let l = self.lage(s);
        if l.schliessen.contains(lx, ly) {
            return Some(Ziel::Schliessen);
        }
        if l.knoepfe[0].contains(lx, ly) {
            return Some(Ziel::Links);
        }
        if l.knoepfe[1].contains(lx, ly) {
            return Some(Ziel::Uebernehmen);
        }
        let f0 = l.felder[0];
        if f0.contains(lx, ly) && lx >= (f0.x + f0.w - 26.0 * s) as f64 {
            return Some(Ziel::Pfeil);
        }
        l.felder
            .iter()
            .position(|f| f.contains(lx, ly))
            .map(Ziel::Feld)
    }

    // --- Maus ----------------------------------------------------------------

    /// Maus bewegt; `true`, wenn neu zu zeichnen ist.
    pub fn mouse_move(&mut self, r: Rect, s: f32, x: f64, y: f64) -> bool {
        let h = self.ziel(r, s, x, y);
        let changed = h != self.hover;
        self.hover = h;
        changed
    }

    /// Über einem Textfeld (für den Mauszeiger).
    pub fn ueber_feld(&self) -> bool {
        matches!(self.hover, Some(Ziel::Feld(_)))
    }

    pub fn press(&mut self, r: Rect, s: f32, fonts: &Fonts, t: &Theme, x: f64, y: f64) {
        let z = self.ziel(r, s, x, y);
        self.pressed = z;
        match z {
            Some(Ziel::Feld(i)) => {
                if i != self.fokus {
                    self.fokus = i;
                }
                self.liste = None;
                let l = self.lage(s);
                let f = l.felder[i];
                let (lx, ly) = ((x - r.x as f64) as f32, (y - r.y as f64) as f32);
                let at = self.stelle(fonts, t, s, f, i, lx, ly);
                self.felder[i].place(at, false);
            }
            Some(Ziel::Pfeil) => {
                self.fokus = 0;
                self.liste = if self.liste.is_some() {
                    None
                } else {
                    Some(None)
                };
            }
            Some(Ziel::Vorschlag(k)) => self.waehle(k),
            None => self.liste = None,
            _ => {}
        }
    }

    pub fn release(&mut self, r: Rect, s: f32, x: f64, y: f64) -> Option<Antwort> {
        let p = self.pressed.take()?;
        if self.ziel(r, s, x, y) != Some(p) {
            return None;
        }
        match p {
            Ziel::Links | Ziel::Schliessen => Some(Antwort::Verwerfen),
            Ziel::Uebernehmen => Some(Antwort::Uebernehmen(Box::new(self.projekt()))),
            _ => None,
        }
    }

    /// Byte-Stelle im Feld `i` unter (x, y) relativ zum Blatt.
    #[allow(clippy::too_many_arguments)]
    fn stelle(&self, fonts: &Fonts, t: &Theme, s: f32, f: Rect, i: usize, x: f32, y: f32) -> usize {
        let text = &self.felder[i].text;
        let zeile = (((y - f.y - 5.0 * s) / (TEXTZEILE * s)).floor().max(0.0)) as usize;
        let mut anfang = 0;
        for (k, l) in text.split('\n').enumerate() {
            if k == zeile || anfang + l.len() >= text.len() {
                let px = t.size.font_small * s;
                let tx = widgets::text_field_x(f, s, t);
                return anfang + widgets::caret_at(fonts.regular.as_ref(), l, px, tx, x);
            }
            anfang += l.len() + 1;
        }
        text.len()
    }

    // --- Tasten --------------------------------------------------------------

    /// Getipptes Zeichen ins Feld mit dem Fokus; in der Projektart öffnet es
    /// die Vorschläge (Tippen filtert).
    pub fn text(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        self.felder[self.fokus].insert(ch.encode_utf8(&mut [0; 4]));
        if self.fokus == 0 {
            self.liste = (!self.vorschlaege().is_empty()).then_some(None);
        }
    }

    /// Eine Taste. Enter übernimmt (in mehrzeiligen Feldern: neue Zeile,
    /// Strg+Enter übernimmt), Esc schließt die Liste bzw. verwirft, Tab
    /// läuft durch die Felder.
    pub fn key(&mut self, k: Key, mods: Modifiers) -> Option<Antwort> {
        const HOCH: Key = Key::Other(0x26);
        const RUNTER: Key = Key::Other(0x28);
        let i = self.fokus;
        let mehrzeilig = FELDER[i].zeilen > 1;
        let n = self.vorschlaege().len();
        let e = &mut self.felder[i];
        let sh = mods.shift;
        match k {
            Key::Escape if self.liste.is_some() => self.liste = None,
            Key::Escape => return Some(Antwort::Verwerfen),
            Key::Enter => {
                if let Some(Some(k)) = self.liste {
                    self.waehle(k);
                } else if mehrzeilig && !mods.ctrl {
                    e.insert("\n");
                } else {
                    return Some(Antwort::Uebernehmen(Box::new(self.projekt())));
                }
            }
            Key::Tab => {
                let n = REIHE.len();
                let k = REIHE.iter().position(|&j| j == i).unwrap_or(0);
                self.fokus_auf(REIHE[(k + if sh { n - 1 } else { 1 }) % n]);
            }
            RUNTER if i == 0 => {
                self.liste = Some(match self.liste {
                    Some(Some(k)) if n > 0 => Some((k + 1).min(n - 1)),
                    _ if n > 0 => Some(0),
                    _ => None,
                });
            }
            HOCH if i == 0 => {
                if let Some(Some(k)) = self.liste {
                    self.liste = Some(Some(k.saturating_sub(1)));
                }
            }
            RUNTER | HOCH if mehrzeilig => zeile_wechseln(e, k == RUNTER, sh),
            Key::Backspace => e.backspace(),
            Key::Delete => e.delete(),
            Key::Left => e.left(sh),
            Key::Right => e.right(sh),
            Key::Home => e.home(sh),
            Key::End => e.end(sh),
            Key::Char('A') if mods.ctrl => e.select_all(),
            Key::Char('C') if mods.ctrl => sk_platform::set_clipboard_text(e.selected()),
            Key::Char('X') if mods.ctrl => {
                let cut = e.cut();
                sk_platform::set_clipboard_text(&cut);
            }
            Key::Char('V') if mods.ctrl => {
                let paste = sk_platform::clipboard_text().unwrap_or_default();
                let paste = paste.replace("\r\n", "\n");
                if mehrzeilig {
                    e.insert(&paste);
                } else {
                    e.insert(paste.lines().next().unwrap_or(""));
                }
            }
            Key::Char('Z') if mods.ctrl => {
                e.undo();
            }
            _ => {}
        }
        None
    }

    // --- Zeichnen ------------------------------------------------------------

    /// Bild samt Schatten; Lage links oben = `rect` minus Schatten.
    pub fn paint(&self, t: &Theme, fonts: &Fonts, s: f32) -> Canvas {
        let u = &t.ui;
        let (w, h) = self.size(s);
        let m = (t.size.panel_shadow * s).round();
        let mut c = Canvas::new((w + 2.0 * m) as usize, (h + 2.0 * m) as usize);
        widgets::panel(&mut c, Rect::new(m, m, w, h), s, t);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let cap = |px: f32| regular.map_or(px * 0.7, |f| f.cap_height(px));
        let l = self.lage(s);
        let at = |r: Rect| Rect::new(r.x + m, r.y + m, r.w, r.h);
        // Kopf: Titel, bei „Neu“ leise „neues Projekt“, ×
        let px = t.size.font_title * s;
        let base = m + (KOPF_H * s + cap(px)) * 0.5;
        let x0 = m + PAD * s;
        widgets::text(&mut c, bold, "Projektdaten", px, x0, base, u.text);
        if self.neu {
            let tw = bold.map_or(0.0, |f| f.width("Projektdaten", px));
            let px2 = t.size.font_small * s;
            widgets::text(
                &mut c,
                regular,
                "neues Projekt",
                px2,
                x0 + tw + 10.0 * s,
                base,
                u.text_dim,
            );
        }
        let x = at(l.schliessen);
        let col = if self.hover == Some(Ziel::Schliessen) {
            u.text
        } else {
            u.text_dim
        };
        let (cx, cy, d) = (x.x + x.w * 0.5, x.y + x.h * 0.5, 4.5 * s);
        let st = s.max(1.0) * 1.2;
        let mut p = Path::new();
        p.segment((cx - d, cy - d), (cx + d, cy + d), st);
        p.segment((cx - d, cy + d), (cx + d, cy - d), st);
        c.fill(&p, col);
        c.fill_rect(m, m + KOPF_H * s, w, s.max(1.0), u.border);
        // Gruppen und Felder
        let gpx = 10.5 * s;
        for (g, (_, name)) in GRUPPEN.iter().enumerate() {
            let y = m + l.gruppen[g] + (GRUPPE_H * s * 0.5 + cap(gpx)) * 0.5 + 4.0 * s;
            widgets::text(&mut c, bold, name, gpx, x0, y, u.text_dim);
            if g == 2 && self.vom_letzten {
                widgets::text(
                    &mut c,
                    regular,
                    VOM_LETZTEN,
                    gpx,
                    m + FELD_X * s,
                    y,
                    u.text_dim,
                );
            }
        }
        let fpx = t.size.font_small * s;
        for (i, f) in FELDER.iter().enumerate() {
            let r = at(l.felder[i]);
            let fokus = i == self.fokus;
            let lbl_y = r.y + (ZEILE_H * s + cap(fpx)) * 0.5;
            let lbl_x = if f.x > 0.0 {
                r.x - 8.0 * s - regular.map_or(0.0, |ft| ft.width(f.name, fpx))
            } else {
                x0
            };
            widgets::text(&mut c, regular, f.name, fpx, lbl_x, lbl_y, u.text_dim);
            let ex = r.x + r.w + 6.0 * s;
            widgets::text(&mut c, regular, f.einheit, fpx, ex, lbl_y, u.text_dim);
            if i == 0 {
                widgets::combo(
                    &mut c,
                    fonts,
                    r,
                    "",
                    self.hover == Some(Ziel::Feld(0)) || self.hover == Some(Ziel::Pfeil),
                    fokus,
                    s,
                    t,
                );
            } else {
                let st = widgets::FieldState {
                    hover: self.hover == Some(Ziel::Feld(i)),
                    focus: fokus,
                    ..Default::default()
                };
                widgets::text_field(&mut c, fonts, r, &st, s, t);
            }
            self.paint_inhalt(&mut c, fonts, t, s, r, i, fokus);
        }
        // Fuß: Satz oder Meldung links, Knöpfe rechts
        let fuss_y = m + l.knoepfe[0].y - (FUSS_H - KNOPF_H) * 0.5 * s;
        c.fill_rect(m, fuss_y, w, s.max(1.0), u.border);
        let k0 = at(l.knoepfe[0]);
        let satz_y = k0.y + (k0.h + cap(fpx)) * 0.5;
        let (satz, col) = match (self.meldung(), self.hinweis()) {
            (Some(m), _) => (m, u.accent),
            (None, Some(h)) => (h, u.text_dim),
            (None, None) => (LEER_SATZ.to_string(), u.text_dim),
        };
        let satz = widgets::ellipsize(regular, &satz, 11.5 * s, k0.x - x0 - 12.0 * s);
        widgets::text(&mut c, regular, &satz, 11.5 * s, x0, satz_y, col);
        let links = if self.neu { "Später" } else { "Abbrechen" };
        for (k, (label, aktiv)) in [(links, false), ("Übernehmen", true)].iter().enumerate() {
            let ziel = if k == 0 {
                Ziel::Links
            } else {
                Ziel::Uebernehmen
            };
            let st = ButtonState {
                hover: self.hover == Some(ziel),
                pressed: self.pressed == Some(ziel) && self.hover == Some(ziel),
                active: *aktiv,
                disabled: false,
            };
            widgets::button(&mut c, fonts, at(l.knoepfe[k]), label, st, s, t);
        }
        // Vorschlagsliste über allem
        let zeilen = self.liste_rects(s);
        if let (Some(markiert), Some(erste)) = (self.liste, zeilen.first()) {
            let n = zeilen.len() as f32;
            let r = Rect::new(
                erste.x + m,
                erste.y + m - 4.0 * s,
                erste.w,
                n * LISTE_ZEILE * s + 8.0 * s,
            );
            widgets::panel_filled(&mut c, r, s, t, u.menu_bg);
            let jetzt = self.felder[0].text.trim();
            for (k, z) in zeilen.iter().enumerate() {
                let z = at(*z);
                let v = self.vorschlaege()[k];
                let heiss = markiert == Some(k) || self.hover == Some(Ziel::Vorschlag(k));
                if heiss {
                    c.fill_rect(z.x + 3.0 * s, z.y, z.w - 6.0 * s, z.h, u.hover);
                }
                let gewaehlt = v == jetzt;
                let f = if gewaehlt { bold } else { regular };
                let b = z.y + (z.h + cap(fpx)) * 0.5;
                widgets::text(&mut c, f, v, fpx, z.x + 12.0 * s, b, u.text);
                if gewaehlt {
                    widgets::text(&mut c, regular, "✓", fpx, z.x + z.w - 22.0 * s, b, u.accent);
                }
            }
        }
        c
    }

    /// Text, Beispiel, Markierung und Schreibmarke eines Felds, auch
    /// mehrzeilig.
    #[allow(clippy::too_many_arguments)]
    fn paint_inhalt(
        &self,
        c: &mut Canvas,
        fonts: &Fonts,
        t: &Theme,
        s: f32,
        r: Rect,
        i: usize,
        fokus: bool,
    ) {
        let Some(f) = fonts.regular.as_ref() else {
            return;
        };
        let u = &t.ui;
        let e = &self.felder[i];
        let px = t.size.font_small * s;
        let x = widgets::text_field_x(r, s, t);
        let rechts = r.x + r.w - if i == 0 { 26.0 } else { 8.0 } * s;
        let erste = if FELDER[i].zeilen == 1 {
            r.y + (r.h + f.cap_height(px)) * 0.5
        } else {
            r.y + 5.0 * s + (TEXTZEILE * s + f.cap_height(px)) * 0.5
        };
        let base = |k: usize| (erste + k as f32 * TEXTZEILE * s).round();
        if e.text.is_empty() {
            for (k, l) in FELDER[i].beispiel.lines().enumerate() {
                if k < FELDER[i].zeilen {
                    let l = widgets::ellipsize(Some(f), l, px, rechts - x);
                    f.draw(c, &l, px, x.round(), base(k), u.text_disabled);
                }
            }
        }
        let (a, z) = e.selection();
        let mut anfang = 0;
        for (k, l) in e.text.split('\n').enumerate() {
            let ende = anfang + l.len();
            if k < FELDER[i].zeilen.max(1) || fokus {
                let y = base(k);
                if y < r.y + r.h {
                    if fokus && a < z && a <= ende && z >= anfang {
                        let von = a.max(anfang) - anfang;
                        let bis = z.min(ende) - anfang;
                        let x0 = x + f.width(&l[..von], px);
                        let x1 = x + f.width(&l[..bis], px);
                        c.fill_rect(
                            x0,
                            y - f.cap_height(px) - 4.0 * s,
                            x1 - x0,
                            TEXTZEILE * s - 2.0 * s,
                            u.text_select,
                        );
                    }
                    let zeig = if fokus {
                        l.to_string()
                    } else {
                        widgets::ellipsize(Some(f), l, px, rechts - x)
                    };
                    f.draw(c, &zeig, px, x.round(), y, u.field_text);
                    if fokus && e.caret >= anfang && e.caret <= ende {
                        let cx = x + f.width(&l[..e.caret - anfang], px);
                        let b = s.round().max(1.0);
                        c.fill_rect(
                            cx.round(),
                            y - f.cap_height(px) - 3.0 * s,
                            b,
                            f.cap_height(px) + 7.0 * s,
                            u.caret,
                        );
                    }
                }
            }
            anfang = ende + 1;
        }
    }
}

/// Schreibmarke eine Zeile hoch oder runter, an dieselbe Zeichenzahl.
fn zeile_wechseln(e: &mut TextEdit, runter: bool, select: bool) {
    let t = &e.text;
    let anfang = t[..e.caret].rfind('\n').map_or(0, |i| i + 1);
    let spalte = t[anfang..e.caret].chars().count();
    let ziel_anfang = if runter {
        match t[e.caret..].find('\n') {
            Some(i) => e.caret + i + 1,
            None => return,
        }
    } else {
        if anfang == 0 {
            return;
        }
        t[..anfang - 1].rfind('\n').map_or(0, |i| i + 1)
    };
    let zeile = &t[ziel_anfang..];
    let zeile = &zeile[..zeile.find('\n').unwrap_or(zeile.len())];
    let off: usize = zeile.chars().take(spalte).map(char::len_utf8).sum();
    e.place(ziel_anfang + off, select);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ohne() -> Project {
        Project::new(sk_model::Guid(7), "Projekt")
    }

    fn tippen(m: &mut Maske, s: &str) {
        for ch in s.chars() {
            if ch == '\n' {
                m.key(Key::Enter, Modifiers::default());
            } else {
                m.text(ch);
            }
        }
    }

    fn tab(m: &mut Maske) {
        m.key(Key::Tab, Modifiers::default());
    }

    /// Paket PD Abnahme 1: Neu, Planung vom letzten Projekt vorbelegt,
    /// Jörns Beispiel eingetippt; Enter in mehrzeiligen Feldern macht eine
    /// neue Zeile, Strg+Enter übernimmt.
    #[test]
    fn neu_mit_jorns_beispiel() {
        let planung = Some((
            "Dipl.-Ing. (FH) Jörn Horstmann".to_string(),
            "Denkmalsweg 18b\n27777 Ganderkesee".to_string(),
        ));
        let mut m = Maske::new(&ohne(), true, planung);
        assert!(m.vom_letzten());
        assert_eq!(m.fokus(), 0, "Cursor in Projektart");
        // Tippen filtert die Vorschläge, Enter auf dem markierten übernimmt ihn
        tippen(&mut m, "einfam");
        assert_eq!(m.vorschlaege(), ["Neubau Einfamilienhaus"]);
        assert!(m.liste_offen());
        m.key(Key::Other(0x28), Modifiers::default());
        m.key(Key::Enter, Modifiers::default());
        assert_eq!(m.wert(0), "Neubau Einfamilienhaus");
        assert!(!m.liste_offen());
        tab(&mut m);
        tippen(&mut m, "Haus Mustermann");
        tab(&mut m);
        tippen(&mut m, "Musterweg 1\n27777 Ganderkesee");
        // Breite und Länge unter dem Bauort; leer bleibt der Standardort
        tab(&mut m);
        assert_eq!(m.fokus(), 8);
        tab(&mut m);
        assert_eq!(m.fokus(), 9);
        tab(&mut m);
        assert_eq!(m.fokus(), 3);
        tippen(&mut m, "01/26");
        tab(&mut m);
        tippen(&mut m, "Max Mustermann");
        tab(&mut m);
        tippen(&mut m, "Lindenstraße 12\n27777 Ganderkesee");
        let strg = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        let Some(Antwort::Uebernehmen(p)) = m.key(Key::Enter, strg) else {
            panic!("Strg+Enter übernimmt");
        };
        assert_eq!(p.kind, "Neubau Einfamilienhaus");
        assert_eq!(p.site, "Haus Mustermann");
        assert_eq!(p.place, "Musterweg 1\n27777 Ganderkesee");
        assert_eq!(p.number, "01/26");
        assert_eq!(p.client, "Max Mustermann");
        assert_eq!(p.client_addr, "Lindenstraße 12\n27777 Ganderkesee");
        assert_eq!(p.author, "Dipl.-Ing. (FH) Jörn Horstmann");
        assert_eq!(p.author_addr, "Denkmalsweg 18b\n27777 Ganderkesee");
        assert_eq!(p.guid, sk_model::Guid(7));
        assert!(m.meldung().is_none());
        assert!(m.ort().is_unset());
    }

    /// Sonnenstand S1: Breite und Länge mit Komma oder Punkt, Grad und
    /// Minus; leer heißt nicht gesetzt. Unbrauchbares wird gemeldet und
    /// lässt den alten Wert stehen. Die Nordrichtung bleibt, wie sie war.
    #[test]
    fn breite_und_laenge() {
        let alt = Location {
            lat: Some(53.0589),
            lon: Some(8.591),
            north: Some(12.0),
        };
        let mut m = Maske::new(&ohne(), false, None).mit_ort(&alt);
        assert_eq!((m.wert(8), m.wert(9)), ("53,0589", "8,591"));
        assert_eq!(m.ort(), alt);
        // Umschalt+Tab läuft rückwärts: von der Projektnummer zur Länge
        m.fokus_auf(3);
        let sh = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        m.key(Key::Tab, sh);
        assert_eq!(m.fokus(), 9);
        m.fokus_auf(8);
        tippen(&mut m, "−33,87°");
        m.fokus_auf(9);
        tippen(&mut m, "151.21");
        assert!(m.meldung().is_none(), "{:?}", m.meldung());
        let l = m.ort();
        assert_eq!(
            (l.lat, l.lon, l.north),
            (Some(-33.87), Some(151.21), Some(12.0))
        );
        m.fokus_auf(8);
        tippen(&mut m, "95");
        assert_eq!(
            m.meldung().as_deref(),
            Some("Breitengrad: Zahl von −90 bis 90.")
        );
        assert_eq!(m.ort().lat, Some(53.0589), "alter Wert bleibt");
        m.fokus_auf(9);
        tippen(&mut m, "Ost");
        m.fokus_auf(8);
        m.key(Key::Delete, Modifiers::default());
        assert_eq!(
            m.meldung().as_deref(),
            Some("Längengrad: Zahl von −180 bis 180.")
        );
        assert_eq!(m.ort().lat, None, "leer: nicht gesetzt");
        for (t, v) in [
            ("", Ok(None)),
            (" 8,5 ° ", Ok(Some(8.5))),
            ("-180", Ok(Some(-180.0))),
            ("180,01", Err(())),
            ("1e400", Err(())),
            ("NaN", Err(())),
        ] {
            assert_eq!(grad(t, 180.0), v, "{t}");
        }
        assert_eq!(grad_text(Some(-0.5)), "-0,5");
    }

    /// Abnahme S1, Punkt 1: Nur Breite oder nur Länge gesetzt sagt der Fuß
    /// leise, welcher Wert vom Standardort kommt; der Satz passt ungekürzt.
    #[test]
    fn hinweis_auf_den_standardort() {
        let mut m = Maske::new(&ohne(), false, None);
        assert_eq!(m.hinweis(), None);
        m.fokus_auf(8);
        tippen(&mut m, "52,52");
        assert_eq!(
            m.hinweis().as_deref(),
            Some("Längengrad fehlt, es gilt Ganderkesee.")
        );
        m.fokus_auf(9);
        tippen(&mut m, "13,4");
        assert_eq!(m.hinweis(), None);
        m.fokus_auf(8);
        m.key(Key::Delete, Modifiers::default());
        let h = m.hinweis().unwrap();
        assert_eq!(h, "Breitengrad fehlt, es gilt Ganderkesee.");
        assert_eq!(m.ort().lat, None);
        // Unbrauchbares: die Meldung gilt, kein Hinweis
        tippen(&mut m, "x");
        assert!(m.meldung().is_some());
        assert_eq!(m.hinweis(), None);
        let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
        let Some(f) = std::fs::read(lib.join("LiberationSans-Regular.ttf"))
            .ok()
            .and_then(sk_paint::font::Font::parse)
        else {
            return;
        };
        let l = m.lage(1.0);
        let platz = l.knoepfe[0].x - PAD - 12.0;
        assert!(f.width(&h, 11.5) < platz, "{} ≥ {platz}", f.width(&h, 11.5));
    }

    /// Abnahme 2: Esc verwirft; über den Knopf kein „vom letzten Projekt“;
    /// Überlänge wird gemeldet, nicht gesperrt.
    #[test]
    fn verwerfen_und_ueberlaenge() {
        let mut m = Maske::new(&ohne(), true, None);
        assert_eq!(
            m.key(Key::Escape, Modifiers::default()),
            Some(Antwort::Verwerfen)
        );
        let mut p = ohne();
        p.number = "01/26".into();
        let mut m = Maske::new(&p, false, Some(("X".into(), String::new())));
        assert!(!m.vom_letzten());
        for _ in 0..5 {
            tab(&mut m);
        }
        m.key(Key::End, Modifiers::default());
        tippen(&mut m, &"9".repeat(20));
        assert_eq!(
            m.meldung().as_deref(),
            Some("Projektnummer: höchstens 20 Zeichen.")
        );
        let Some(Antwort::Uebernehmen(q)) = m.key(Key::Enter, Modifiers::default()) else {
            panic!("Enter übernimmt im einzeiligen Feld");
        };
        assert_eq!(q.number.chars().count(), 25);
    }

    #[test]
    fn zeile_hoch_und_runter() {
        let mut e = TextEdit::new("Lindenstraße 12\n27777 Ganderkesee");
        e.end(false);
        zeile_wechseln(&mut e, false, false);
        assert_eq!(&e.text[..e.caret], "Lindenstraße 12");
        e.home(false);
        e.right(false);
        e.right(false);
        zeile_wechseln(&mut e, true, false);
        assert_eq!(&e.text[..e.caret], "Lindenstraße 12\n27");
    }

    /// Zeichnen mit und ohne Schrift: kein Absturz, Höhe passt zu den
    /// Feldern; offene Liste bleibt im Blatt.
    #[test]
    fn zeichnet() {
        let t = Theme::dark();
        let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
        let lade = |n: &str| {
            std::fs::read(lib.join(n))
                .ok()
                .and_then(sk_paint::font::Font::parse)
        };
        let fonts = Fonts {
            regular: lade("LiberationSans-Regular.ttf"),
            bold: lade("LiberationSans-Bold.ttf"),
            italic: None,
        };
        let mut m = Maske::new(&ohne(), true, None);
        m.key(Key::Other(0x28), Modifiers::default());
        assert!(m.liste_offen());
        for s in [1.0, 1.5] {
            let c = m.paint(&t, &fonts, s);
            assert!(c.height as f32 > 500.0 * s);
            let l = m.lage(s);
            let unten = m.liste_rects(s).last().map_or(0.0, |r| r.y + r.h);
            assert!(unten < l.h, "Liste im Blatt");
            // Breite und Länge in einer Zeile zwischen Bauort und
            // Projektnummer; Einheit, Bezeichnung und Rand überdecken sich nicht
            let [b, la] = [l.felder[8], l.felder[9]];
            assert_eq!(b.y, la.y);
            assert!(l.felder[2].y + l.felder[2].h < b.y);
            assert!(b.y + b.h < l.felder[3].y);
            assert_eq!(b.x, l.felder[2].x);
            if let Some(f) = fonts.regular.as_ref() {
                let px = t.size.font_small * s;
                let einheit = b.x + b.w + 6.0 * s + f.width("° N", px);
                // Die zweite Zahl hat keine eigene Bezeichnung
                assert!(einheit + 8.0 * s < la.x, "{einheit} {}", la.x);
                assert!(la.x + la.w + 6.0 * s + f.width("° O", px) < (W - PAD / 2.0) * s);
            }
        }
    }

    /// Ist-Bilder zu `soll-projektdaten` und `soll-projektdaten-knopf`:
    /// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbild_projektdaten -- --ignored`
    #[test]
    #[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
    fn istbild_projektdaten() {
        let Some(dir) = std::env::var_os("SKIZZEO_ISTBILDER") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let t = Theme::dark();
        let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
        let lade = |n: &str| {
            std::fs::read(lib.join(n))
                .ok()
                .and_then(sk_paint::font::Font::parse)
        };
        let fonts = Fonts {
            regular: lade("LiberationSans-Regular.ttf"),
            bold: lade("LiberationSans-Bold.ttf"),
            italic: None,
        };
        let ab = |c: Canvas, n: &str| std::fs::write(dir.join(n), c.to_png()).unwrap();
        // a) Neu, Planung vom letzten Projekt, Jörns Beispiel bis Bauherr
        let planung = Some((
            "Dipl.-Ing. (FH) Jörn Horstmann".to_string(),
            "Denkmalsweg 18b\n27777 Ganderkesee".to_string(),
        ));
        let mut m = Maske::new(&ohne(), true, planung);
        tippen(&mut m, "Neubau Einfamilienhaus");
        m.key(Key::Escape, Modifiers::default());
        for (k, w) in [
            "Haus Mustermann",
            "Musterweg 1\n27777 Ganderkesee",
            "01/26",
            "Max Mustermann",
            "Phantasiestraße 7\n27777 Ganderkesee",
        ]
        .iter()
        .enumerate()
        {
            tab(&mut m);
            tippen(&mut m, w);
            assert_eq!(m.fokus(), k + 1);
            if k == 1 {
                // Breite und Länge leer
                tab(&mut m);
                tab(&mut m);
            }
        }
        ab(m.paint(&t, &fonts, 1.0), "ist-projektdaten-a-neu.png");
        // b) über den Knopf, leer, Liste der Projektart offen
        let mut m = Maske::new(&ohne(), false, None);
        m.key(Key::Other(0x28), Modifiers::default());
        ab(m.paint(&t, &fonts, 1.0), "ist-projektdaten-b-knopf.png");
        ab(m.paint(&t, &fonts, 1.5), "ist-projektdaten-b-knopf-150.png");
        // c) nur die Breite gesetzt: leiser Hinweis auf den Standardort
        let mut m = Maske::new(&ohne(), false, None);
        m.fokus_auf(8);
        tippen(&mut m, "52,52");
        ab(m.paint(&t, &fonts, 1.0), "ist-projektdaten-c-breite.png");
        // Knopf im linken Paneel: gesetzt und leer, rechts daneben die
        // Kachel des Nordpfeils (gesetzt: Nord 12°, Sonnenstand S2)
        // (aktiv: beim Aufziehen, Hinweise unten für den Nordpfeil)
        for (n, p, sc, aktiv) in [
            ("gesetzt", true, 1.0, false),
            ("leer", false, 1.0, false),
            ("gesetzt-200", true, 2.0, false),
            ("leer-200", false, 2.0, false),
            ("aufziehen", false, 1.0, true),
            ("aufziehen-200", false, 2.0, true),
        ] {
            let mut ui = crate::ui::Ui::new(sc, &t);
            ui.nord = p.then_some(12.0);
            ui.nord_aktiv = aktiv;
            ui.fonts = Fonts {
                regular: lade("LiberationSans-Regular.ttf"),
                bold: lade("LiberationSans-Bold.ttf"),
                italic: None,
            };
            let mut pr = ohne();
            if p {
                pr.site = "Haus Mustermann".into();
                pr.number = "01/26".into();
            }
            ui.projekt_zeilen = crate::ui::projekt_zeilen(&pr);
            let (c, _, _) = ui.paint(&t, crate::ui::Panel::Tools, 1280, 32);
            ab(c.clone(), &format!("ist-projektdaten-knopf-{n}.png"));
        }
    }

    /// S10 (Jörn 09.10. 14:10): Ohne Lage stehen Breite und Länge von
    /// Ganderkesee als echte Werte in den Feldern. Unverändert übernommen
    /// bleibt die Lage ungesetzt; ein geänderter Wert setzt beide.
    #[test]
    fn lage_vorbelegt() {
        let leer = Location::default();
        let m = Maske::new(&ohne(), true, None).mit_ort(&leer);
        assert_eq!((m.wert(8), m.wert(9)), ("53,0589", "8,591"));
        assert_eq!(m.ort(), leer, "unverändert: keine Lage");
        assert_eq!(m.hinweis(), None);
        let mut m = Maske::new(&ohne(), false, None).mit_ort(&leer);
        m.felder[8] = TextEdit::new("48,137");
        assert_eq!(
            m.ort(),
            Location {
                lat: Some(48.137),
                lon: None,
                north: None
            },
            "die unveränderte Länge bleibt ungesetzt, es gilt Ganderkesee"
        );
        // Eine gesetzte Lage steht wie bisher
        let muenchen = Location {
            lat: Some(48.137),
            lon: Some(11.575),
            north: Some(30.0),
        };
        let m = Maske::new(&ohne(), false, None).mit_ort(&muenchen);
        assert_eq!((m.wert(8), m.wert(9)), ("48,137", "11,575"));
        assert_eq!(m.ort(), muenchen);
    }
}

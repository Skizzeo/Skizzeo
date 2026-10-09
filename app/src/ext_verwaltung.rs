//! Fenster „Erweiterungen“ (Schrittplan E5, §2 Nr. 3, 4 und 6): eine Liste
//! mit den eingelesenen Bauteilen, denen nur im Projekt und dem
//! Lieferumfang; zum gewählten Ein/Aus, Aktualisieren, Entfernen bzw. „In
//! Erweiterungen übernehmen“ und aufklappbar „Für Entwickler“. Dasselbe
//! Blatt zeigt die Rückfragen beim Einlesen, Aktualisieren und Entfernen.
//! Das Fenster rechnet und schreibt nichts: es gibt eine [`Antwort`], die
//! App ([`crate::ext_app`]) führt sie aus und gibt neue Zeilen.

use crate::ext_ablage::{Ablage, Fall, Vorschlag};
use sk_cost::neue_saetze::Stand;
use sk_model::erweiterung::{anzeige, ExtDef};
use sk_model::{Category, Model};
use sk_paint::{Canvas, Path};
use sk_platform::Key;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, Fonts, Rect};

/// Breite des Blatts und Ränder (dip).
const W: f32 = 660.0;
const PAD: f32 = 22.0;
const KOPF_H: f32 = 54.0;
const FUSS_H: f32 = 58.0;
const GRUPPE_H: f32 = 30.0;
/// Zeile der Liste: Name und darunter die Angaben.
const ZEILE_H: f32 = 44.0;
/// Textzeile in „Für Entwickler“ und in den Rückfragen.
const TEXT_H: f32 = 18.0;
/// Zeile mit Haken (neue Sätze für den Firmenkatalog, E8c).
const HAKEN_H: f32 = 24.0;
const KNOPF_H: f32 = 30.0;
/// Höchste Höhe des Inhalts (dip); darüber rollt die Liste.
const INHALT_MAX: f32 = 470.0;
const INHALT_MIN: f32 = 200.0;
/// Pfeiltasten (Windows-Tastencodes).
const HOCH: Key = Key::Other(0x26);
const RUNTER: Key = Key::Other(0x28);
/// Längster Name in der Liste.
const MAX: usize = 60;

/// Die Bauteilarten mit eigenem Werkzeug (§2 Nr. 4).
pub const LIEFERUMFANG: [Category; 9] = [
    Category::ExteriorWall,
    Category::InteriorWall,
    Category::Floor,
    Category::GroundSlab,
    Category::StripFooting,
    Category::EdgeInsulation,
    Category::SoffitInsulation,
    Category::RoofTerrace,
    Category::Coping,
];

/// Woher eine Zeile kommt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// Eingelesen, eingeschaltet.
    An,
    /// Eingelesen, ausgeschaltet (Unterordner „Aus“).
    Aus,
    /// Nur in der Projektdatei, nicht im Ordner.
    NurProjekt,
    Lieferumfang,
}

/// Eine Zeile der Liste.
#[derive(Clone, Debug, PartialEq)]
pub struct Zeile {
    /// `key` der Erweiterung; leer im Lieferumfang.
    pub key: String,
    pub art: Art,
    pub name: String,
    /// „werk.stuetze · Version 1 · Skizzeo · Präfix ST“.
    pub detail: String,
    /// Rechts: „2 im Projekt“, „aus“, …
    pub stand: String,
    /// Das Projekt hat eine andere Version: „Aktualisieren“.
    pub aktualisieren: bool,
    /// Exemplare im Projekt.
    pub im_projekt: usize,
    /// Zeilen unter „Für Entwickler“: Notizen, Hinweise, Quelle.
    pub entwickler: Vec<String>,
}

/// Zeilen der Liste aus Ablage und Projekt: Eingelesene nach Gruppe und
/// Name, dann die nur im Projekt, dann der Lieferumfang.
pub fn zeilen(a: &Ablage, m: &Model) -> Vec<Zeile> {
    let detail = |d: &ExtDef| {
        let mut t = format!("{} · Version {}", d.key, d.version);
        let autor = d.def.bauteil_feld("autor").unwrap_or("");
        if !autor.is_empty() {
            t.push_str(&format!(" · {}", anzeige(autor, 40)));
        }
        t.push_str(&format!(" · Präfix {}", d.prefix()));
        t
    };
    let notizen = |d: &ExtDef| -> Vec<String> {
        d.def
            .notiz
            .iter()
            .map(|n| {
                format!(
                    "Notiz {}: {}",
                    anzeige(n.get("art").unwrap_or(""), 20),
                    anzeige(n.get("text").unwrap_or(""), 160)
                )
            })
            .collect()
    };
    let anzahl = |n: usize| match n {
        0 => "nicht im Projekt".to_string(),
        n => format!("{n} im Projekt"),
    };
    let mut out = Vec::new();
    for e in &a.eintraege {
        let d = &e.def;
        let n = m.ext_uses(&d.key).len();
        let projekt = m.ext_def(&d.key).filter(|p| p.text != d.text);
        let stand = match (e.an, projekt) {
            (false, _) => "aus".to_string(),
            (true, Some(p)) => format!("im Projekt Version {}", p.version),
            (true, None) => anzahl(n),
        };
        let mut entw = notizen(d);
        if e.hinweise.is_empty() {
            entw.push("Prüfung: ohne Befund".into());
        } else {
            entw.push(format!("Prüfung: {} Hinweise", e.hinweise.len()));
            entw.extend(e.hinweise.iter().cloned());
        }
        entw.push(format!(
            "Quelle: {}",
            anzeige(&e.pfad.to_string_lossy(), 160)
        ));
        out.push(Zeile {
            key: d.key.clone(),
            art: if e.an { Art::An } else { Art::Aus },
            name: anzeige(d.name(), MAX),
            detail: detail(d),
            stand,
            aktualisieren: projekt.is_some(),
            im_projekt: n,
            entwickler: entw,
        });
    }
    for d in m.ext_defs() {
        if a.eintrag(&d.key).is_some() {
            continue;
        }
        let n = m.ext_uses(&d.key).len();
        let mut entw = notizen(d);
        entw.push("Quelle: Projektdatei".into());
        out.push(Zeile {
            key: d.key.clone(),
            art: Art::NurProjekt,
            name: anzeige(d.name(), MAX),
            detail: detail(d),
            stand: anzahl(n),
            aktualisieren: false,
            im_projekt: n,
            entwickler: entw,
        });
    }
    for c in LIEFERUMFANG {
        let n = m.elements().iter().filter(|(_, e)| e.category == c).count();
        out.push(Zeile {
            key: String::new(),
            art: Art::Lieferumfang,
            name: c.name().to_string(),
            detail: format!("Präfix {}", c.prefix()),
            stand: anzahl(n),
            aktualisieren: false,
            im_projekt: n,
            entwickler: Vec::new(),
        });
    }
    out
}

/// Was nach „Ja“ einer Rückfrage geschieht.
#[derive(Clone, Debug)]
pub enum Tat {
    /// Datei in den Ordner schreiben; das Projekt folgt, wenn es die
    /// Erweiterung nutzt.
    Einlesen(Box<Vorschlag>),
    /// Das Projekt auf die eingelesene Fassung bringen.
    Aktualisieren(Box<ExtDef>),
    /// Datei löschen.
    Entfernen(String),
}

/// Ein Satz der Erweiterung für den Firmenkatalog mit Haken (E8c).
#[derive(Clone, Debug, PartialEq)]
pub struct Haken {
    pub text: String,
    pub an: bool,
    pub waehlbar: bool,
}

/// Eine Rückfrage oder ein Befund auf dem Blatt.
#[derive(Clone, Debug)]
pub struct Frage {
    pub titel: String,
    /// Fettgedruckt unter dem Titel.
    pub name: String,
    pub satz: String,
    /// Satz als Warnung (rot).
    pub warnung: bool,
    /// Abschnitte mit Überschrift und Zeilen.
    pub abschnitte: Vec<(String, Vec<String>)>,
    /// Beschriftung von „Ja“; `None`: nur „Schließen“.
    pub ja: Option<&'static str>,
    pub tat: Option<Tat>,
    /// Neue Sätze für den Firmenkatalog, unten über dem Fuß (E8c); die
    /// angehakten gehen mit „Ja“ an die Firma.
    pub haken: Vec<Haken>,
}

impl Frage {
    /// Ergebnis der Prüfung beim Einlesen.
    pub fn einlesen(v: Vorschlag) -> Frage {
        let alt = |a: u32| format!("Eingelesen ist Version {a}.");
        let (satz, ja, warnung) = match v.fall {
            Fall::Neu => ("Neue Erweiterung.".to_string(), "Einlesen", false),
            Fall::Gleich if v.projekt.is_none() => {
                ("Diese Fassung ist schon eingelesen.".to_string(), "", false)
            }
            Fall::Gleich => (
                "Diese Fassung ist schon eingelesen.".to_string(),
                "Ins Projekt",
                false,
            ),
            Fall::Hoeher(a) => (
                format!("{} Die neue Version ersetzt sie.", alt(a)),
                "Aktualisieren",
                false,
            ),
            Fall::Anders => (
                "Gleiche Version mit anderem Inhalt: ersetzt die eingelesene Fassung.".to_string(),
                "Ersetzen",
                false,
            ),
            Fall::Kleiner(a) => (
                format!(
                    "{} Version {} ist älter: Zurücksetzen nur, wenn gewollt.",
                    alt(a),
                    v.def.version
                ),
                "Zurücksetzen",
                true,
            ),
        };
        let mut abschnitte = Vec::new();
        if let Some(p) = v.projekt {
            let titel = format!("Im Projekt (Version {p})");
            let z = if v.aenderungen.is_empty() {
                vec!["Gesetzte Bauteile bleiben gleich.".to_string()]
            } else {
                v.aenderungen.clone()
            };
            abschnitte.push((titel, z));
        }
        if !v.hinweise.is_empty() {
            abschnitte.push((
                format!("Hinweise der Prüfung ({})", v.hinweise.len()),
                v.hinweise.clone(),
            ));
        }
        let name = format!(
            "{} · {} · Version {}",
            anzeige(v.def.name(), MAX),
            v.def.key,
            v.def.version
        );
        let ja = (!ja.is_empty()).then_some(ja);
        // Neue Sätze für den Firmenkatalog (E8c), je einer mit Haken
        let haken = match ja {
            Some(_) => v
                .saetze
                .iter()
                .map(|n| {
                    let art = if n.rec == "service" {
                        "Bauleistung"
                    } else {
                        "Artikel"
                    };
                    let neu = n.stand == Stand::Neu;
                    Haken {
                        text: format!("{art} {} · {}", anzeige(&n.name, 80), n.stand.text()),
                        an: neu,
                        waehlbar: neu,
                    }
                })
                .collect(),
            None => Vec::new(),
        };
        Frage {
            titel: "Bauteil einlesen".into(),
            name,
            satz,
            warnung,
            abschnitte,
            ja,
            tat: ja.map(|_| Tat::Einlesen(Box::new(v))),
            haken,
        }
    }

    /// Abgewiesene Datei mit ihren Fehlern.
    pub fn fehler(datei: &str, fehler: Vec<String>) -> Frage {
        Frage {
            titel: "Bauteil einlesen".into(),
            name: anzeige(datei, 80),
            satz: "Nicht eingelesen: die Datei hat Fehler.".into(),
            warnung: true,
            abschnitte: vec![(format!("Fehler ({})", fehler.len()), fehler)],
            ja: None,
            tat: None,
            haken: Vec::new(),
        }
    }

    /// Rückfrage „Aktualisieren“ mit den Änderungen an gesetzten Bauteilen.
    pub fn aktualisieren(d: ExtDef, alt: u32, aenderungen: Vec<String>) -> Frage {
        Frage {
            titel: "Aktualisieren".into(),
            name: format!("{} · {}", anzeige(d.name(), MAX), d.key),
            satz: format!(
                "Das Projekt nutzt Version {alt}, eingelesen ist Version {}.",
                d.version
            ),
            warnung: false,
            abschnitte: vec![("Gesetzte Bauteile ändern sich".into(), aenderungen)],
            ja: Some("Aktualisieren"),
            tat: Some(Tat::Aktualisieren(Box::new(d))),
            haken: Vec::new(),
        }
    }

    /// Rückfrage „Entfernen“.
    pub fn entfernen(z: &Zeile) -> Frage {
        let satz = match z.im_projekt {
            0 => "Die Datei wird gelöscht.".to_string(),
            n => format!("Die Datei wird gelöscht; das Projekt behält seine {n} Bauteile."),
        };
        Frage {
            titel: "Entfernen".into(),
            name: format!("{} · {}", z.name, z.key),
            satz,
            warnung: true,
            abschnitte: Vec::new(),
            ja: Some("Entfernen"),
            tat: Some(Tat::Entfernen(z.key.clone())),
            haken: Vec::new(),
        }
    }
}

/// Was die App tun soll.
#[derive(Clone, Debug)]
pub enum Antwort {
    /// Fenster zu.
    Zu,
    /// Datei wählen und prüfen („Bauteil einlesen …“).
    Einlesen,
    Schalten(String, bool),
    /// Rückfrage zum Aktualisieren rechnen.
    Aktualisieren(String),
    Uebernehmen(String),
    /// „Ja“ einer Rückfrage.
    Tat(Tat),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Knopf {
    Einlesen,
    Schalten,
    Aktualisieren,
    Entfernen,
    Uebernehmen,
    Zu,
    Ja,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ziel {
    Zeile(usize),
    Haken(usize),
    Entwickler(usize),
    Knopf(Knopf),
    Schliessen,
}

/// Das Fenster.
pub struct Fenster {
    pub zeilen: Vec<Zeile>,
    sel: Option<usize>,
    /// Zeile mit aufgeklapptem „Für Entwickler“.
    offen: Option<usize>,
    /// Gerollt (dip).
    scroll: f32,
    /// Rückfrage statt Liste.
    pub frage: Option<Frage>,
    /// Nur für die Rückfrage geöffnet (Datei › Bauteil einlesen …): „Nein“
    /// schließt das Fenster.
    nur_frage: bool,
    /// Satz im Kopf nach einer Tat.
    pub meldung: Option<String>,
    hover: Option<Ziel>,
    pressed: Option<Ziel>,
}

/// Lage der Teile relativ zum Blatt (Pixel).
struct Lage {
    w: f32,
    h: f32,
    inhalt: Rect,
    schliessen: Rect,
    knoepfe: Vec<(Knopf, Rect, &'static str)>,
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w, h)
}

/// Platz für den Inhalt (dip) bei `h` Pixeln unter der Titelleiste.
fn platz(s: f32, h: f32) -> f32 {
    h / s - KOPF_H - FUSS_H - 40.0
}

impl Fenster {
    /// Rückfrage „Bauteil einlesen“ offen (Thema der Hilfe, E9).
    pub fn beim_einlesen(&self) -> bool {
        self.frage
            .as_ref()
            .is_some_and(|f| f.titel == "Bauteil einlesen")
    }

    pub fn new(zeilen: Vec<Zeile>) -> Fenster {
        Fenster {
            zeilen,
            sel: None,
            offen: None,
            scroll: 0.0,
            frage: None,
            nur_frage: false,
            meldung: None,
            hover: None,
            pressed: None,
        }
    }

    /// Nur für eine Rückfrage (Datei › Bauteil einlesen …).
    pub fn mit_frage(zeilen: Vec<Zeile>, f: Frage) -> Fenster {
        let mut w = Fenster::new(zeilen);
        w.frage = Some(f);
        w.nur_frage = true;
        w
    }

    /// Neue Zeilen nach einer Tat; die Wahl bleibt beim selben `key`.
    pub fn setze_zeilen(&mut self, zeilen: Vec<Zeile>, waehle: Option<&str>) {
        let key = waehle
            .map(str::to_string)
            .or_else(|| self.gewaehlt().map(|z| z.key.clone()));
        self.zeilen = zeilen;
        self.sel = key.and_then(|k| self.zeilen.iter().position(|z| !k.is_empty() && z.key == k));
        self.offen = self.offen.filter(|_| self.sel.is_some()).and(self.sel);
        self.nur_frage = false;
        self.frage = None;
    }

    pub fn gewaehlt(&self) -> Option<&Zeile> {
        self.zeilen.get(self.sel?)
    }

    /// Rückfrage zeigen (aus der Liste heraus).
    pub fn frage(&mut self, f: Frage) {
        self.frage = Some(f);
        self.hover = None;
        self.pressed = None;
    }

    /// Beschriftung des Schalters zur gewählten Zeile.
    fn knoepfe_zeile(&self) -> Vec<(Knopf, &'static str)> {
        let Some(z) = self.gewaehlt() else {
            return Vec::new();
        };
        match z.art {
            Art::An | Art::Aus => {
                let mut v = vec![(
                    Knopf::Schalten,
                    if z.art == Art::An {
                        "Ausschalten"
                    } else {
                        "Einschalten"
                    },
                )];
                if z.aktualisieren && z.art == Art::An {
                    v.push((Knopf::Aktualisieren, "Aktualisieren"));
                }
                v.push((Knopf::Entfernen, "Entfernen"));
                v
            }
            Art::NurProjekt => vec![(Knopf::Uebernehmen, "In Erweiterungen übernehmen")],
            Art::Lieferumfang => Vec::new(),
        }
    }

    /// Höhe des Listeninhalts (dip, ohne Rollen) und je Zeile ihr y.
    fn liste(&self) -> (f32, Vec<(Option<&'static str>, f32)>) {
        let mut y = 0.0;
        let mut out = Vec::new();
        let mut gruppe = None;
        for (i, z) in self.zeilen.iter().enumerate() {
            let g = match z.art {
                Art::An | Art::Aus => "EINGELESEN",
                Art::NurProjekt => "NUR IM PROJEKT",
                Art::Lieferumfang => "LIEFERUMFANG",
            };
            let kopf = (gruppe != Some(g)).then(|| {
                gruppe = Some(g);
                g
            });
            if kopf.is_some() {
                y += GRUPPE_H;
            }
            out.push((kopf, y));
            y += ZEILE_H;
            if self.offen == Some(i) {
                y += z.entwickler.len() as f32 * TEXT_H + 8.0;
            }
        }
        if !self
            .zeilen
            .iter()
            .any(|z| matches!(z.art, Art::An | Art::Aus))
        {
            // Platz für den Satz „Noch keine Erweiterung eingelesen.“
            y += GRUPPE_H + ZEILE_H;
        }
        (y, out)
    }

    /// Zeilen der Rückfrage (dip-Höhe grob, umbrochen wird beim Zeichnen).
    fn frage_hoehe(f: &Frage) -> f32 {
        let mut h = 3.0 * TEXT_H + 12.0;
        for (_, z) in &f.abschnitte {
            h += GRUPPE_H + z.len() as f32 * TEXT_H * 1.6;
        }
        if !f.haken.is_empty() {
            h += GRUPPE_H + f.haken.len() as f32 * HAKEN_H + 8.0;
        }
        h
    }

    /// Zeilen mit Haken unten im Inhalt (Lage-Koordinaten); davor ihre
    /// Überschrift.
    fn haken_rects(&self, l: &Lage, s: f32) -> Vec<Rect> {
        let n = self.frage.as_ref().map_or(0, |f| f.haken.len());
        let unten = l.inhalt.y + l.inhalt.h - 8.0 * s;
        (0..n)
            .map(|i| {
                let y = unten - (n - i) as f32 * HAKEN_H * s;
                rect(PAD * s, y, (W - 2.0 * PAD) * s, HAKEN_H * s)
            })
            .collect()
    }

    fn inhalt_h(&self) -> f32 {
        let h = match &self.frage {
            Some(f) => Fenster::frage_hoehe(f),
            None => self.liste().0,
        };
        h.clamp(INHALT_MIN, INHALT_MAX)
    }

    /// Lage bei `platz` dip für den Inhalt.
    fn lage(&self, s: f32, platz: f32) -> Lage {
        let ih = self.inhalt_h().min(platz.max(INHALT_MIN));
        let (w, h) = (W * s, (KOPF_H + ih + FUSS_H) * s);
        let ky = h - (FUSS_H + KNOPF_H) * 0.5 * s;
        let mut knoepfe = Vec::new();
        let mut x = w - PAD * s;
        let mut rechts = |k: Knopf, label: &'static str, bw: f32, v: &mut Vec<_>| {
            x -= bw * s;
            v.push((k, rect(x, ky, bw * s, KNOPF_H * s), label));
            x -= 8.0 * s;
        };
        match &self.frage {
            Some(f) => {
                if let Some(ja) = f.ja {
                    rechts(Knopf::Ja, ja, 130.0, &mut knoepfe);
                    rechts(Knopf::Zu, "Abbrechen", 100.0, &mut knoepfe);
                } else {
                    rechts(Knopf::Zu, "Schließen", 100.0, &mut knoepfe);
                }
            }
            None => {
                rechts(Knopf::Zu, "Schließen", 100.0, &mut knoepfe);
                for (k, label) in self.knoepfe_zeile().into_iter().rev() {
                    let bw = if k == Knopf::Uebernehmen {
                        210.0
                    } else {
                        110.0
                    };
                    rechts(k, label, bw, &mut knoepfe);
                }
                knoepfe.push((
                    Knopf::Einlesen,
                    rect(PAD * s, ky, 150.0 * s, KNOPF_H * s),
                    "Bauteil einlesen …",
                ));
            }
        }
        Lage {
            w,
            h,
            inhalt: rect(0.0, KOPF_H * s, w, ih * s),
            schliessen: rect(
                w - (PAD + 22.0) * s,
                (KOPF_H - 26.0) * 0.5 * s,
                26.0 * s,
                26.0 * s,
            ),
            knoepfe,
        }
    }

    /// Lage des Blatts im Fenster.
    pub fn rect(&self, s: f32, win_w: u32, win_h: u32, top: u32) -> Rect {
        let l = self.lage(s, platz(s, win_h as f32 - top as f32));
        let x = ((win_w as f32 - l.w) * 0.5).round().max(0.0);
        let y = (top as f32 + (win_h as f32 - top as f32 - l.h) * 0.4)
            .round()
            .max(top as f32);
        Rect::new(x, y, l.w, l.h)
    }

    fn ziel(&self, r: Rect, s: f32, x: f64, y: f64) -> Option<Ziel> {
        let l = self.lage(s, r.h / s - KOPF_H - FUSS_H);
        let (lx, ly) = (x - r.x as f64, y - r.y as f64);
        if l.schliessen.contains(lx, ly) {
            return Some(Ziel::Schliessen);
        }
        if let Some((k, _, _)) = l.knoepfe.iter().find(|(_, b, _)| b.contains(lx, ly)) {
            return Some(Ziel::Knopf(*k));
        }
        let haken = self.haken_rects(&l, s);
        if let Some(i) = haken.iter().position(|b| b.contains(lx, ly)) {
            return Some(Ziel::Haken(i));
        }
        if self.frage.is_some() || !l.inhalt.contains(lx, ly) {
            return None;
        }
        let ly = (ly as f32 - l.inhalt.y) / s + self.scroll;
        let (_, ys) = self.liste();
        for (i, (kopf, y0)) in ys.iter().enumerate() {
            let _ = kopf;
            if ly >= *y0 && ly < y0 + ZEILE_H {
                let z = &self.zeilen[i];
                if !z.entwickler.is_empty() && self.sel == Some(i) {
                    let ex = (W - PAD - 130.0) * s;
                    if lx as f32 >= ex && ly >= y0 + ZEILE_H * 0.5 {
                        return Some(Ziel::Entwickler(i));
                    }
                }
                return Some(Ziel::Zeile(i));
            }
        }
        None
    }

    /// Maus bewegt; `true`, wenn neu zu zeichnen ist.
    pub fn mouse_move(&mut self, r: Rect, s: f32, x: f64, y: f64) -> bool {
        let h = self.ziel(r, s, x, y);
        let changed = h != self.hover;
        self.hover = h;
        changed
    }

    pub fn press(&mut self, r: Rect, s: f32, x: f64, y: f64) {
        let z = self.ziel(r, s, x, y);
        self.pressed = z;
        match z {
            Some(Ziel::Zeile(i)) => {
                if self.zeilen[i].art != Art::Lieferumfang {
                    if self.sel != Some(i) {
                        self.offen = None;
                    }
                    self.sel = Some(i);
                    self.meldung = None;
                }
            }
            Some(Ziel::Entwickler(i)) => {
                self.offen = if self.offen == Some(i) { None } else { Some(i) };
            }
            _ => {}
        }
    }

    pub fn release(&mut self, r: Rect, s: f32, x: f64, y: f64) -> Option<Antwort> {
        let p = self.pressed.take()?;
        if self.ziel(r, s, x, y) != Some(p) {
            return None;
        }
        match p {
            Ziel::Schliessen => self.nein(),
            Ziel::Knopf(k) => self.knopf(k),
            Ziel::Haken(i) => {
                if let Some(h) = self.frage.as_mut().and_then(|f| f.haken.get_mut(i)) {
                    h.an = h.waehlbar && !h.an;
                }
                None
            }
            _ => None,
        }
    }

    /// „Abbrechen“, Esc oder ×: von der Rückfrage zurück zur Liste
    /// (`None`), sonst zu.
    fn nein(&mut self) -> Option<Antwort> {
        if self.frage.take().is_some() && !self.nur_frage {
            self.hover = None;
            return None;
        }
        Some(Antwort::Zu)
    }

    fn knopf(&mut self, k: Knopf) -> Option<Antwort> {
        let z = self.gewaehlt().cloned();
        match k {
            Knopf::Zu => self.nein(),
            Knopf::Ja => {
                let mut f = self.frage.take()?;
                self.nur_frage = false;
                // nur die angehakten neuen Sätze gehen an die Firma
                if let Some(Tat::Einlesen(v)) = f.tat.as_mut() {
                    let mut an = f.haken.iter().map(|h| h.an);
                    v.saetze.retain(|_| an.next().unwrap_or(false));
                }
                f.tat.map(Antwort::Tat)
            }
            Knopf::Einlesen => Some(Antwort::Einlesen),
            Knopf::Schalten => z.map(|z| Antwort::Schalten(z.key, z.art != Art::An)),
            Knopf::Aktualisieren => z.map(|z| Antwort::Aktualisieren(z.key)),
            Knopf::Uebernehmen => z.map(|z| Antwort::Uebernehmen(z.key)),
            Knopf::Entfernen => {
                let f = Frage::entfernen(&z?);
                self.frage(f);
                None
            }
        }
    }

    /// Rad über der Liste.
    pub fn wheel(&mut self, r: Rect, s: f32, dy: f64) -> bool {
        if self.frage.is_some() {
            return false;
        }
        let l = self.lage(s, r.h / s - KOPF_H - FUSS_H);
        let max = (self.liste().0 - l.inhalt.h / s).max(0.0);
        let neu = (self.scroll - dy as f32 * 40.0).clamp(0.0, max);
        let changed = neu != self.scroll;
        self.scroll = neu;
        changed
    }

    /// Esc: Rückfrage bzw. Fenster zu; Enter: „Ja“ der Rückfrage;
    /// Pfeile wählen in der Liste.
    pub fn key(&mut self, k: Key) -> Option<Antwort> {
        match k {
            Key::Escape => self.nein(),
            Key::Enter if self.frage.as_ref().is_some_and(|f| f.ja.is_some()) => {
                self.knopf(Knopf::Ja)
            }
            HOCH | RUNTER if self.frage.is_none() => {
                let wahl: Vec<usize> = (0..self.zeilen.len())
                    .filter(|&i| self.zeilen[i].art != Art::Lieferumfang)
                    .collect();
                let pos = self.sel.and_then(|s| wahl.iter().position(|&i| i == s));
                let neu = match (pos, k) {
                    (None, _) => wahl.first(),
                    (Some(p), HOCH) => wahl.get(p.saturating_sub(1)),
                    (Some(p), _) => wahl.get(p + 1).or(wahl.last()),
                };
                if let Some(&i) = neu {
                    if self.sel != Some(i) {
                        self.offen = None;
                    }
                    self.sel = Some(i);
                }
                None
            }
            _ => None,
        }
    }

    /// Das Blatt; `r` aus [`Fenster::rect`].
    pub fn paint(&self, t: &Theme, fonts: &Fonts, s: f32, r: Rect) -> Canvas {
        let u = &t.ui;
        let l = self.lage(s, r.h / s - KOPF_H - FUSS_H);
        let m = (t.size.panel_shadow * s).round();
        let mut c = Canvas::new((l.w + 2.0 * m) as usize, (l.h + 2.0 * m) as usize);
        widgets::panel(&mut c, Rect::new(m, m, l.w, l.h), s, t);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let cap = |px: f32| regular.map_or(px * 0.7, |f| f.cap_height(px));
        let at = |r: Rect| Rect::new(r.x + m, r.y + m, r.w, r.h);
        let x0 = m + PAD * s;
        let breit = l.w - 2.0 * PAD * s;
        // Kopf
        let px = t.size.font_title * s;
        let titel = self
            .frage
            .as_ref()
            .map_or("Erweiterungen", |f| f.titel.as_str());
        widgets::text(
            &mut c,
            bold,
            titel,
            px,
            x0,
            m + (KOPF_H * s + cap(px)) * 0.5,
            u.text,
        );
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
        c.fill_rect(m, m + KOPF_H * s, l.w, s.max(1.0), u.border);
        let inh = at(l.inhalt);
        let fpx = t.size.font_small * s;
        let gpx = 10.5 * s;
        match &self.frage {
            Some(f) => {
                let mut y = inh.y + 14.0 * s;
                let zeile = |c: &mut Canvas, font, text: &str, px: f32, y: f32, col| {
                    let text = widgets::ellipsize(font, text, px, breit);
                    widgets::text(c, font, &text, px, x0, y + cap(px), col);
                };
                zeile(&mut c, bold, &f.name, t.size.font * s, y, u.text);
                y += TEXT_H * 1.4 * s;
                for z in widgets::wrap(regular, &f.satz, fpx, breit) {
                    let col = if f.warnung { u.danger } else { u.text };
                    zeile(&mut c, regular, &z, fpx, y, col);
                    y += TEXT_H * s;
                }
                let haken = self.haken_rects(&l, s);
                let grenze = haken
                    .first()
                    .map_or(inh.y + inh.h, |r| m + r.y - GRUPPE_H * s);
                for (kopf, zeilen) in &f.abschnitte {
                    y += 12.0 * s;
                    if y + TEXT_H * s > grenze {
                        break;
                    }
                    zeile(&mut c, bold, &kopf.to_uppercase(), gpx, y, u.text_dim);
                    y += TEXT_H * 1.2 * s;
                    for z in zeilen {
                        for w in widgets::wrap(regular, z, fpx, breit - 12.0 * s) {
                            if y + TEXT_H * s > grenze {
                                break;
                            }
                            widgets::text(
                                &mut c,
                                regular,
                                &w,
                                fpx,
                                x0 + 12.0 * s,
                                y + cap(fpx),
                                u.text,
                            );
                            y += TEXT_H * s;
                        }
                    }
                }
                if let Some(r0) = haken.first() {
                    let ky = m + r0.y - GRUPPE_H * s + 12.0 * s;
                    zeile(&mut c, bold, "FÜR DEN FIRMENKATALOG", gpx, ky, u.text_dim);
                }
                for (i, (h, r)) in f.haken.iter().zip(&haken).enumerate() {
                    let r = at(*r);
                    let b = 14.0 * s;
                    let kr = Rect::new(r.x, r.y + (r.h - b) * 0.5, b, b);
                    if h.waehlbar {
                        let hover = self.hover == Some(Ziel::Haken(i));
                        widgets::checkbox(&mut c, kr, h.an, hover, s, t);
                    }
                    let tx = r.x + b + 8.0 * s;
                    let col = if h.waehlbar { u.text } else { u.text_dim };
                    let text = widgets::ellipsize(regular, &h.text, fpx, r.w - b - 8.0 * s);
                    let ty = r.y + (r.h + cap(fpx)) * 0.5;
                    widgets::text(&mut c, regular, &text, fpx, tx, ty, col);
                }
            }
            None => self.paint_liste(&mut c, t, fonts, s, inh, x0, breit),
        }
        // Fuß
        let fuss_y = m + l.h - FUSS_H * s;
        c.fill_rect(m, fuss_y, l.w, s.max(1.0), u.border);
        for (k, r, label) in &l.knoepfe {
            let ziel = Ziel::Knopf(*k);
            let st = ButtonState {
                hover: self.hover == Some(ziel),
                pressed: self.pressed == Some(ziel) && self.hover == Some(ziel),
                active: *k == Knopf::Ja,
                disabled: false,
            };
            widgets::button(&mut c, fonts, at(*r), label, st, s, t);
        }
        // Satz nach einer Tat im Kopf, rechts vom Titel
        if let (Some(mld), None) = (&self.meldung, &self.frage) {
            let tw = bold.map_or(0.0, |f| f.width(titel, px));
            let x = x0 + tw + 16.0 * s;
            let mpx = 11.5 * s;
            let mld = widgets::ellipsize(regular, mld, mpx, m + l.schliessen.x - x - 10.0 * s);
            widgets::text(
                &mut c,
                regular,
                &mld,
                mpx,
                x,
                m + (KOPF_H * s + cap(mpx)) * 0.5,
                u.accent,
            );
        }
        c
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_liste(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        s: f32,
        inh: Rect,
        x0: f32,
        breit: f32,
    ) {
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let cap = |px: f32| regular.map_or(px * 0.7, |f| f.cap_height(px));
        let (fpx, gpx, npx) = (t.size.font_small * s, 10.5 * s, t.size.font * s);
        let (gesamt, ys) = self.liste();
        let sichtbar = |y: f32, h: f32| y >= inh.y - 0.5 && y + h <= inh.y + inh.h + 0.5;
        let oben = inh.y - self.scroll * s;
        let mut leer_gezeigt = false;
        for (i, (kopf, y0)) in ys.iter().enumerate() {
            let z = &self.zeilen[i];
            if !leer_gezeigt && !matches!(z.art, Art::An | Art::Aus) {
                leer_gezeigt = true;
                if i == 0 {
                    // Noch nichts eingelesen: Gruppe mit einem Satz
                    let gy = oben;
                    if sichtbar(gy, GRUPPE_H * s) {
                        widgets::text(
                            c,
                            bold,
                            "EINGELESEN",
                            gpx,
                            x0,
                            gy + (GRUPPE_H * s + cap(gpx)) * 0.5 + 4.0 * s,
                            u.text_dim,
                        );
                    }
                    let ty = gy + GRUPPE_H * s + (ZEILE_H * s * 0.5 + cap(fpx)) * 0.5;
                    if sichtbar(ty - cap(fpx), cap(fpx)) {
                        widgets::text(
                            c,
                            regular,
                            "Noch keine Erweiterung eingelesen: „Bauteil einlesen …“ wählt eine .szb.",
                            fpx,
                            x0,
                            ty,
                            u.text_dim,
                        );
                    }
                }
            }
            let leer_dy = if self
                .zeilen
                .iter()
                .any(|z| matches!(z.art, Art::An | Art::Aus))
            {
                0.0
            } else {
                GRUPPE_H + ZEILE_H
            };
            let y = oben + (y0 + leer_dy) * s;
            if let Some(g) = kopf {
                let gy = y - GRUPPE_H * s;
                if sichtbar(gy, GRUPPE_H * s) {
                    widgets::text(
                        c,
                        bold,
                        g,
                        gpx,
                        x0,
                        gy + (GRUPPE_H * s + cap(gpx)) * 0.5 + 4.0 * s,
                        u.text_dim,
                    );
                }
            }
            let zh = ZEILE_H * s;
            let extra = if self.offen == Some(i) {
                (z.entwickler.len() as f32 * TEXT_H + 8.0) * s
            } else {
                0.0
            };
            let gewaehlt = self.sel == Some(i);
            let ganz = Rect::new(inh.x + 8.0 * s, y, inh.w - 16.0 * s, zh + extra);
            if gewaehlt && sichtbar(y, zh) {
                let mut p = Path::new();
                p.rounded_rect(
                    ganz.x,
                    ganz.y,
                    ganz.w,
                    ganz.h.min(inh.y + inh.h - y),
                    6.0 * s,
                );
                c.fill(&p, u.pressed);
            } else if self.hover == Some(Ziel::Zeile(i))
                && z.art != Art::Lieferumfang
                && sichtbar(y, zh)
            {
                let mut p = Path::new();
                p.rounded_rect(ganz.x, ganz.y, ganz.w, zh, 6.0 * s);
                c.fill(&p, u.hover);
            }
            if sichtbar(y, zh) {
                let aus = z.art == Art::Aus;
                let name_col = if aus { u.text_dim } else { u.text };
                let stand_w = regular.map_or(0.0, |f| f.width(&z.stand, fpx));
                let n = widgets::ellipsize(bold, &z.name, npx, breit - stand_w - 24.0 * s);
                widgets::text(
                    c,
                    bold,
                    &n,
                    npx,
                    x0,
                    y + 6.0 * s + cap(npx) + 4.0 * s,
                    name_col,
                );
                let stand_col = if z.aktualisieren {
                    u.accent
                } else {
                    u.text_dim
                };
                widgets::text(
                    c,
                    regular,
                    &z.stand,
                    fpx,
                    x0 + breit - stand_w,
                    y + 6.0 * s + cap(npx) + 4.0 * s,
                    stand_col,
                );
                let ent_w = if gewaehlt && !z.entwickler.is_empty() {
                    140.0 * s
                } else {
                    0.0
                };
                let dt = widgets::ellipsize(regular, &z.detail, fpx, breit - ent_w);
                let dy = y + zh - 10.0 * s;
                widgets::text(c, regular, &dt, fpx, x0, dy, u.text_dim);
                if ent_w > 0.0 {
                    let label = "Für Entwickler";
                    let lw = regular.map_or(0.0, |f| f.width(label, fpx));
                    let lx = x0 + breit - lw;
                    let col = if self.hover == Some(Ziel::Entwickler(i)) {
                        u.accent_hover
                    } else {
                        u.accent
                    };
                    widgets::disclosure(
                        c,
                        lx - 9.0 * s,
                        dy - cap(fpx) * 0.5,
                        self.offen == Some(i),
                        col,
                        s,
                    );
                    widgets::text(c, regular, label, fpx, lx, dy, col);
                }
            }
            if self.offen == Some(i) {
                let mut ey = y + zh;
                for e in &z.entwickler {
                    if sichtbar(ey, TEXT_H * s) {
                        let e = widgets::ellipsize(regular, e, fpx, breit - 24.0 * s);
                        widgets::text(
                            c,
                            regular,
                            &e,
                            fpx,
                            x0 + 12.0 * s,
                            ey + cap(fpx) + 3.0 * s,
                            u.text,
                        );
                    }
                    ey += TEXT_H * s;
                }
            }
        }
        // Bildlaufleiste, wenn die Liste länger ist
        let ih = inh.h / s;
        if gesamt > ih + 0.5 {
            let r = Rect::new(
                inh.x + inh.w - 8.0 * s,
                inh.y + 4.0 * s,
                4.0 * s,
                inh.h - 8.0 * s,
            );
            widgets::scrollbar(c, r, self.scroll / gesamt, ih / gesamt, false, s, t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext_ablage;
    use sk_model::erweiterung::ExtPart;
    use std::path::PathBuf;

    const STUETZE: &str = include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb");
    const TREPPE: &str = include_str!("../../crates/sk-szb/beispiele/werk.treppe.szb");
    const PLATTE: &str = include_str!("../../crates/sk-szb/beispiele/werk.bodenplatte.szb");

    fn ordner(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("skizzeo-ev-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn v2(t: &str) -> String {
        t.replace("version=1 ", "version=2 ")
    }

    /// Ordner mit Stütze (Version 2, an) und Treppe (aus); Projekt mit
    /// zwei Stützen Version 1 und einer Bodenplatte, die nur im Projekt ist.
    fn lage(name: &str) -> (PathBuf, Ablage, Model) {
        let dir = ordner(name);
        let mut a = Ablage::lesen(&dir);
        for t in [v2(STUETZE), TREPPE.to_string()] {
            let (d, h) = ext_ablage::text_pruefen(&t).unwrap();
            a.schreiben(&d, h).unwrap();
        }
        a.schalten("werk.treppe", false).unwrap();
        let mut m = Model::new();
        m.add_building(1);
        let eg = m
            .storeys()
            .iter()
            .find(|(_, s)| s.short == "EG" && s.building.is_some())
            .map(|(id, _)| id)
            .unwrap();
        for (t, n) in [(STUETZE, 2), (PLATTE, 1)] {
            let d = ExtDef::lesen(t).unwrap();
            m.put_ext_def(d.clone()).unwrap();
            for i in 0..n {
                m.add_ext(eg, ExtPart::new(&d, [3000.0 * i as f64, 0.0]))
                    .unwrap();
            }
        }
        (dir, a, m)
    }

    #[test]
    fn zeilen_aus_ordner_und_projekt() {
        let (dir, a, m) = lage("zeilen");
        let z = zeilen(&a, &m);
        let _ = std::fs::remove_dir_all(&dir);
        let kurz: Vec<(&str, Art, &str, bool)> = z
            .iter()
            .take(3)
            .map(|z| (z.key.as_str(), z.art, z.stand.as_str(), z.aktualisieren))
            .collect();
        assert_eq!(
            kurz,
            [
                ("werk.stuetze", Art::An, "im Projekt Version 1", true),
                ("werk.treppe", Art::Aus, "aus", false),
                ("werk.bodenplatte", Art::NurProjekt, "1 im Projekt", false),
            ]
        );
        assert_eq!(
            z[0].detail,
            "werk.stuetze · Version 2 · Skizzeo · Präfix ST"
        );
        assert_eq!(z[0].im_projekt, 2);
        assert!(z[0]
            .entwickler
            .iter()
            .any(|e| e.starts_with("Notiz stand: Startbeispiel")));
        assert!(z[0].entwickler.last().unwrap().starts_with("Quelle: "));
        assert_eq!(z[2].entwickler.last().unwrap(), "Quelle: Projektdatei");
        // Lieferumfang: neun Arten, ohne Aus und Entfernen
        let l: Vec<&Zeile> = z.iter().filter(|z| z.art == Art::Lieferumfang).collect();
        assert_eq!(l.len(), 9);
        assert_eq!(l[0].name, Category::ExteriorWall.name());
        assert_eq!(
            l[0].detail,
            format!("Präfix {}", Category::ExteriorWall.prefix())
        );
        assert_eq!(l[0].stand, "nicht im Projekt");
    }

    fn mitte(r: Rect) -> (f64, f64) {
        ((r.x + r.w * 0.5) as f64, (r.y + r.h * 0.5) as f64)
    }

    fn klick(w: &mut Fenster, r: Rect, p: (f64, f64)) -> Option<Antwort> {
        w.press(r, 1.0, p.0, p.1);
        w.release(r, 1.0, p.0, p.1)
    }

    fn drueck(w: &mut Fenster, r: Rect, k: Knopf) -> Option<Antwort> {
        let l = w.lage(1.0, r.h - KOPF_H - FUSS_H);
        let b = l.knoepfe.iter().find(|b| b.0 == k).expect("Knopf").1;
        let p = (r.x as f64 + mitte(b).0, r.y as f64 + mitte(b).1);
        klick(w, r, p)
    }

    /// Wählen, Knöpfe zur Zeile, Rückfrage „Entfernen“ mit Esc und Enter,
    /// „Für Entwickler“ auf und zu, Pfeiltasten.
    #[test]
    fn fenster_bedienen() {
        let (dir, a, m) = lage("fenster");
        let mut w = Fenster::new(zeilen(&a, &m));
        let _ = std::fs::remove_dir_all(&dir);
        let r = w.rect(1.0, 1280, 800, 32);
        let z0 = (
            r.x as f64 + 60.0,
            r.y as f64 + (KOPF_H + w.liste().1[0].1 + 10.0) as f64,
        );
        // ohne Wahl nur „Bauteil einlesen …“ und „Schließen“
        assert_eq!(w.lage(1.0, 400.0).knoepfe.len(), 2);
        assert!(klick(&mut w, r, z0).is_none());
        assert_eq!(w.gewaehlt().unwrap().key, "werk.stuetze");
        let ks: Vec<&str> = w.lage(1.0, 400.0).knoepfe.iter().map(|k| k.2).collect();
        assert_eq!(
            ks,
            [
                "Schließen",
                "Entfernen",
                "Aktualisieren",
                "Ausschalten",
                "Bauteil einlesen …"
            ]
        );
        match drueck(&mut w, r, Knopf::Schalten) {
            Some(Antwort::Schalten(k, false)) => assert_eq!(k, "werk.stuetze"),
            x => panic!("{x:?}"),
        }
        match drueck(&mut w, r, Knopf::Aktualisieren) {
            Some(Antwort::Aktualisieren(k)) => assert_eq!(k, "werk.stuetze"),
            x => panic!("{x:?}"),
        }
        // Entfernen fragt erst; Esc zurück zur Liste, Enter bestätigt
        assert!(drueck(&mut w, r, Knopf::Entfernen).is_none());
        let f = w.frage.as_ref().unwrap();
        assert_eq!(
            f.satz,
            "Die Datei wird gelöscht; das Projekt behält seine 2 Bauteile."
        );
        assert!(w.key(Key::Escape).is_none());
        assert!(w.frage.is_none());
        drueck(&mut w, r, Knopf::Entfernen);
        match w.key(Key::Enter) {
            Some(Antwort::Tat(Tat::Entfernen(k))) => assert_eq!(k, "werk.stuetze"),
            x => panic!("{x:?}"),
        }
        assert!(w.frage.is_none());
        // Für Entwickler: unten rechts in der gewählten Zeile
        let ent = (r.x as f64 + (W - PAD - 40.0) as f64, z0.1 + 22.0);
        klick(&mut w, r, ent);
        assert_eq!(w.offen, Some(0));
        klick(&mut w, r, ent);
        assert_eq!(w.offen, None);
        // Pfeile: zur Treppe, zur Bodenplatte, nie in den Lieferumfang
        w.key(RUNTER);
        assert_eq!(w.gewaehlt().unwrap().key, "werk.treppe");
        let ks: Vec<&str> = w.lage(1.0, 400.0).knoepfe.iter().map(|k| k.2).collect();
        assert_eq!(
            ks,
            [
                "Schließen",
                "Entfernen",
                "Einschalten",
                "Bauteil einlesen …"
            ]
        );
        w.key(RUNTER);
        w.key(RUNTER);
        assert_eq!(w.gewaehlt().unwrap().key, "werk.bodenplatte");
        match drueck(&mut w, r, Knopf::Uebernehmen) {
            Some(Antwort::Uebernehmen(k)) => assert_eq!(k, "werk.bodenplatte"),
            x => panic!("{x:?}"),
        }
        assert!(matches!(w.key(Key::Escape), Some(Antwort::Zu)));
        // nur für eine Rückfrage geöffnet: Abbrechen schließt
        let f = Frage::fehler("x.szb", vec!["Zeile 2: kaputt".into()]);
        let mut w = Fenster::mit_frage(Vec::new(), f);
        let r = w.rect(1.0, 1280, 800, 32);
        assert!(matches!(drueck(&mut w, r, Knopf::Zu), Some(Antwort::Zu)));
    }

    /// Satz und Knopf der Rückfrage je Fall (tests/LIESMICH.md).
    #[test]
    fn rueckfrage_je_fall() {
        let dir = ordner("faelle");
        let mut a = Ablage::lesen(&dir);
        let m = Model::new();
        let frage = |a: &Ablage, t: &str| Frage::einlesen(ext_ablage::pruefen(t, a, &m).unwrap());
        let f = frage(&a, STUETZE);
        assert_eq!(
            (f.satz.as_str(), f.ja),
            ("Neue Erweiterung.", Some("Einlesen"))
        );
        assert_eq!(f.name, "Stahlbetonstütze · werk.stuetze · Version 1");
        let (d, h) = ext_ablage::text_pruefen(&v2(STUETZE)).unwrap();
        a.schreiben(&d, h).unwrap();
        let f = frage(&a, &v2(STUETZE));
        assert_eq!(f.ja, None, "{}", f.satz);
        let anders = v2(STUETZE).replace("min=200 max=600", "min=200 max=650");
        assert_eq!(frage(&a, &anders).ja, Some("Ersetzen"));
        let f = frage(&a, STUETZE);
        assert_eq!(f.ja, Some("Zurücksetzen"));
        assert!(f.warnung);
        assert_eq!(
            f.satz,
            "Eingelesen ist Version 2. Version 1 ist älter: Zurücksetzen nur, wenn gewollt."
        );
        let f = frage(&a, &STUETZE.replace("version=1 ", "version=3 "));
        assert_eq!(f.ja, Some("Aktualisieren"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// E8c: neue Sätze der Stütze mit Haken; abgehakt geht nur der Rest
    /// mit „Einlesen“ an die Firma, Vorhandenes ist nicht wählbar.
    #[test]
    fn haken_beim_einlesen() {
        let a = Ablage::default();
        let m = Model::new();
        let mut v = ext_ablage::pruefen(STUETZE, &a, &m).unwrap();
        v.saetze = sk_cost::neue_saetze::neue_saetze(&m, &sk_model::Library::standard(), &v.def);
        let f = Frage::einlesen(v);
        let texte: Vec<&str> = f.haken.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(
            texte,
            [
                "Bauleistung Stahlbetonstütze C25/30 XC1 betonieren, Querschnitt bis 0,36 m² · neu",
                "Bauleistung Stützenschalung glatt, kein Sichtbeton, Höhe bis 3,0 m · neu",
            ]
        );
        assert!(f.haken.iter().all(|h| h.an && h.waehlbar));
        assert!(!Fenster::new(Vec::new()).beim_einlesen());
        let mut w = Fenster::mit_frage(Vec::new(), f);
        assert!(w.beim_einlesen(), "Hilfe-Thema „Bauteil einlesen“");
        let r = w.rect(1.0, 1280, 800, 32);
        let l = w.lage(1.0, r.h - KOPF_H - FUSS_H);
        let hr = w.haken_rects(&l, 1.0);
        assert_eq!(hr.len(), 2);
        assert!(hr[0].y >= l.inhalt.y && hr[1].y + hr[1].h <= l.inhalt.y + l.inhalt.h);
        let p = (r.x as f64 + mitte(hr[0]).0, r.y as f64 + mitte(hr[0]).1);
        klick(&mut w, r, p);
        assert!(!w.frage.as_ref().unwrap().haken[0].an);
        let _ = w.paint(&Theme::dark(), &schriften(), 1.0, r);
        match drueck(&mut w, r, Knopf::Ja) {
            Some(Antwort::Tat(Tat::Einlesen(v))) => {
                let keys: Vec<&str> = v.saetze.iter().map(|n| n.key.as_str()).collect();
                assert_eq!(keys, ["stuetze_schalung"]);
            }
            x => panic!("{x:?}"),
        }
        // Vorhandenes (gleiche Kennung im Katalog) ohne Haken
        let mut v = ext_ablage::pruefen(STUETZE, &a, &m).unwrap();
        v.saetze = sk_cost::neue_saetze::neue_saetze(&m, &sk_model::Library::standard(), &v.def);
        v.saetze[0].stand = sk_cost::neue_saetze::Stand::Vorhanden;
        let f = Frage::einlesen(v);
        assert!(!f.haken[0].an && !f.haken[0].waehlbar);
        assert!(f.haken[0].text.ends_with("vorhanden, Firmenpreis gilt"));
    }

    fn schriften() -> Fonts {
        let f = Fonts::system();
        if f.regular.is_some() {
            return f;
        }
        let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
        let lade = |n: &str| {
            std::fs::read(lib.join(n))
                .ok()
                .and_then(sk_paint::font::Font::parse)
        };
        Fonts {
            regular: lade("LiberationSans-Regular.ttf"),
            bold: lade("LiberationSans-Bold.ttf"),
            italic: lade("LiberationSans-Italic.ttf"),
        }
    }

    /// Ist-Bilder E5: Liste mit aufgeklapptem „Für Entwickler“, Rückfrage
    /// beim Einlesen mit Änderungen, abgewiesene Datei, leere Liste. Nur
    /// auf Wunsch: `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo
    /// istbilder_e5 -- --ignored`
    #[test]
    #[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
    fn istbilder_e5() {
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(PathBuf::from) else {
            return;
        };
        let fonts = schriften();
        std::fs::create_dir_all(&ziel).unwrap();
        let t = Theme::dark();
        let (dir, a, m) = lage("bilder");
        let mut w = Fenster::new(zeilen(&a, &m));
        w.sel = Some(0);
        w.offen = Some(0);
        w.meldung = Some("„Stahlbetonstütze“ Version 2 eingelesen.".into());
        let r = w.rect(1.0, 1280, 800, 32);
        std::fs::write(
            ziel.join("ist-e5-liste.png"),
            w.paint(&t, &fonts, 1.0, r).to_png(),
        )
        .unwrap();
        // Standardtyp und Vorgaben zusammen (Werkbank f044de00)
        let breiter = v2(STUETZE)
            .replace(
                "werte=\"b=240; d=240\" standard=ja",
                "werte=\"b=300; d=300\" standard=ja",
            )
            .replace("wert=240 min=200", "wert=300 min=200");
        let v = ext_ablage::pruefen(&breiter, &a, &m).unwrap();
        w.frage(Frage::einlesen(v));
        let r = w.rect(1.0, 1280, 800, 32);
        std::fs::write(
            ziel.join("ist-e5-einlesen.png"),
            w.paint(&t, &fonts, 1.0, r).to_png(),
        )
        .unwrap();
        let f = include_str!("../../crates/sk-szb/pruefdateien/fehler.szb");
        let e = ext_ablage::pruefen(f, &a, &m).unwrap_err();
        let w = Fenster::mit_frage(Vec::new(), Frage::fehler("fehler.szb", e));
        let r = w.rect(1.0, 1280, 800, 32);
        std::fs::write(
            ziel.join("ist-e5-fehler.png"),
            w.paint(&t, &fonts, 1.0, r).to_png(),
        )
        .unwrap();
        let w = Fenster::new(zeilen(&Ablage::default(), &Model::new()));
        let r = w.rect(1.0, 1280, 800, 32);
        std::fs::write(
            ziel.join("ist-e5-leer.png"),
            w.paint(&t, &fonts, 1.0, r).to_png(),
        )
        .unwrap();
        // E8c: neue Sätze mit Haken, einer schon im Katalog
        let mut v = ext_ablage::pruefen(TREPPE, &Ablage::default(), &m).unwrap();
        v.saetze = sk_cost::neue_saetze::neue_saetze(&m, &sk_model::Library::standard(), &v.def);
        v.saetze[1].stand = sk_cost::neue_saetze::Stand::Vorhanden;
        let w = Fenster::mit_frage(Vec::new(), Frage::einlesen(v));
        let r = w.rect(1.0, 1280, 800, 32);
        std::fs::write(
            ziel.join("ist-e8c-einlesen.png"),
            w.paint(&t, &fonts, 1.0, r).to_png(),
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Mengen der Stützen im EG als Text der Abnahmetabelle.
    fn mengen(m: &Model) -> Vec<String> {
        use crate::schedule_view::{Grouping, ListView};
        let mut s = crate::scene::Scene::with_model(m.clone());
        ListView::grouped(&mut s, Grouping::Storey).line_texts()
    }

    /// E9, ganzer Weg: einlesen, einsetzen, Mengen, speichern, öffnen; neue
    /// Version mit breiterer Stütze ändert das Projekt erst nach der
    /// Rückfrage, und danach wie frisch gesetzt.
    #[test]
    fn ganzer_weg() {
        let dir = ordner("ganzer-weg");
        let mut a = Ablage::lesen(&dir);
        let mut m = Model::with_seed(5);
        m.add_building(1);
        // Einlesen: Rückfrage, „Einlesen“, Datei im Ordner
        let v = ext_ablage::pruefen(STUETZE, &a, &m).unwrap();
        let mut w = Fenster::mit_frage(Vec::new(), Frage::einlesen(v));
        assert!(w.beim_einlesen());
        let r = w.rect(1.0, 1280, 800, 32);
        let Some(Antwort::Tat(Tat::Einlesen(v))) = drueck(&mut w, r, Knopf::Ja) else {
            panic!("Einlesen")
        };
        a.schreiben(&v.def, v.hinweise.clone()).unwrap();
        let a = Ablage::lesen(&dir);
        let d = a
            .bibliothek()
            .defs
            .into_iter()
            .find(|d| d.key == "werk.stuetze");
        let d = d.expect("im Ordner");
        // Einsetzen: zwei Stützen, ein Schritt je Stütze
        let eg = m
            .storeys()
            .iter()
            .find(|(_, s)| s.short == "EG" && s.building.is_some())
            .map(|(id, _)| id)
            .unwrap();
        for x in [0.0, 3000.0] {
            m.begin("Stütze setzen");
            if m.ext_def(&d.key).is_none() {
                m.put_ext_def(d.clone()).unwrap();
            }
            m.add_ext(eg, ExtPart::new(&d, [x, 0.0])).unwrap();
            m.commit().unwrap();
        }
        // Mengen, Speichern und Öffnen: dieselben Zeilen
        let t = mengen(&m);
        assert!(
            t.iter().any(|l| l
                .trim_start()
                .starts_with("Stahlbetonstützen | ST-001 … 002 | 2")),
            "{t:#?}"
        );
        let mut m = sk_model::szo::read(&sk_model::szo::write(&m), sk_model::GuidGen::with_seed(9))
            .unwrap()
            .model;
        assert_eq!(mengen(&m), t);
        // Neue Version, breiter: Rückfrage nennt beide Stützen, Projekt bleibt
        let breiter = v2(STUETZE)
            .replace(
                "st24 name=\"Stütze 24/24\" werte=\"b=240; d=240\" standard=ja",
                "st24 name=\"Stütze 24/24\" werte=\"b=300; d=300\" standard=ja",
            )
            .replace("wert=240 min=200", "wert=300 min=200");
        let v = ext_ablage::pruefen(&breiter, &a, &m).unwrap();
        let f = Frage::einlesen(v);
        assert_eq!(f.ja, Some("Aktualisieren"));
        let (titel, zeilen) = &f.abschnitte[0];
        assert_eq!(titel, "Im Projekt (Version 1)");
        assert!(
            zeilen[0].starts_with("ST-001: Form ändert sich") && zeilen[1].starts_with("ST-002: ")
        );
        let mut w = Fenster::mit_frage(Vec::new(), f);
        assert_eq!(mengen(&m), t, "vor der Antwort unverändert");
        let r = w.rect(1.0, 1280, 800, 32);
        let Some(Antwort::Tat(Tat::Einlesen(v))) = drueck(&mut w, r, Knopf::Ja) else {
            panic!("Aktualisieren")
        };
        assert_eq!(v.projekt, Some(1));
        m.begin("Erweiterung aktualisiert");
        m.put_ext_def(v.def.clone()).unwrap();
        m.commit().unwrap();
        assert_eq!(m.ext_def("werk.stuetze").unwrap().version, 2);
        let neu = mengen(&m);
        assert_ne!(neu, t);
        // wie frisch gesetzt
        let mut f = Model::with_seed(5);
        f.add_building(1);
        f.put_ext_def(v.def.clone()).unwrap();
        let eg = f
            .storeys()
            .iter()
            .find(|(_, s)| s.short == "EG" && s.building.is_some())
            .map(|(id, _)| id)
            .unwrap();
        for x in [0.0, 3000.0] {
            f.add_ext(eg, ExtPart::new(&v.def, [x, 0.0])).unwrap();
        }
        assert_eq!(neu, mengen(&f));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

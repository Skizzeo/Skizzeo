//! Preisblatt (KA-2c, paket-ka2 §4, Einstellungen §3 KA-2 Punkt 9,
//! soll-ka-2c): ein kleines helles Blatt am EP einer Position, kein
//! Fenster. EP-Balken Lohn und Stoff, rechts der EP groß mit dem Firmenwert
//! leise darunter, Felder Aufwandswert und Preise der Hauptstoffe,
//! Nebenstoffe als Zeile, „gilt auch für“, Segment „Gilt für“ mit der
//! Folgezeile und „Firmenpreis zurückholen“.
//!
//! Das Blatt rechnet nichts selbst: den EP beim Tippen liefert
//! `sk_cost::preis::aufbau` auf dem Katalog des Plans (`Scene::kosten_live`),
//! die Operationen `sk_cost::preis::preis_ops`. Geschrieben wird erst mit
//! Enter oder Klick daneben, Esc verwirft (Regel 94, Bedienbarkeit 4.2).

use sk_cost::katalog::Katalog;
use sk_cost::preis::{self, Aufbau, Eingabe};
use sk_cost::{Dez, Op, SatzId};
use sk_model::Guid;
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{Key, Modifiers};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, Fonts};
use std::rc::Rc;

/// Breite des Blatts (dip).
const W: f32 = 420.0;
const PAD: f32 = 16.0;
const RADIUS: f32 = 8.0;
/// Spitze zum EP (dip).
const SPITZE: f32 = 8.0;
const ZEILE: f32 = 34.0;
const FELD_X: f32 = 170.0;
const FELD_W: f32 = 104.0;
const FELD_H: f32 = 26.0;
const BALKEN_H: f32 = 12.0;
const SEG_H: f32 = 26.0;

/// Wofür ein geänderter Preis gilt (Segment „Gilt für“).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gilt {
    #[default]
    NurHaus,
    NeueHaeuser,
}

impl Gilt {
    const ALLE: [Gilt; 2] = [Gilt::NurHaus, Gilt::NeueHaeuser];

    fn label(self) -> &'static str {
        match self {
            Gilt::NurHaus => "Nur dieses Haus",
            Gilt::NeueHaeuser => "Auch für neue Häuser",
        }
    }
}

/// Was ein Feld einstellt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Art {
    Stunden,
    Preis(Guid),
}

struct Feld {
    art: Art,
    name: String,
    einheit: String,
    /// Wert beim Öffnen (wirksamer Katalog) und Wert der Firma.
    alt: Dez,
    firma: Option<Dez>,
    edit: TextEdit,
    /// Gelesener Wert; `None`, wenn der Text keine Zahl ab 0 ist.
    wert: Option<Dez>,
}

/// Ergebnis eines Ereignisses im Blatt.
#[derive(Clone, Debug, PartialEq)]
pub enum Aus {
    /// Nur neu zeichnen.
    Repaint,
    /// Die Eingabe hat sich geändert: Vorschau neu rechnen.
    Live,
    /// Enter oder Klick daneben: ausführen und schließen. Leer, wenn sich
    /// nichts geändert hat.
    Anwenden { ops: Vec<Op>, gilt: Gilt },
    /// Esc oder ×: nichts schreiben.
    Verwerfen,
    /// „Firmenpreis zurückholen“ für die geänderten Sätze.
    Zuruecknehmen(Vec<SatzId>),
}

/// Teil des Blatts unter der Maus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ziel {
    Schliessen,
    Feld(usize),
    Segment(Gilt),
    Zurueck,
    Innen,
}

type Rect = (f32, f32, f32, f32);

fn inside((rx, ry, rw, rh): Rect, x: f32, y: f32) -> bool {
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

/// Dezimalzahl deutsch mit mindestens `min` Nachkommastellen und
/// Tausenderpunkten („1.600,00“, „0,45“, „4,4“).
pub fn zahl(d: Dez, min: usize) -> String {
    let t = d.text();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.as_str()),
    };
    let (g, b) = t.split_once('.').unwrap_or((t, ""));
    let mut ganz = String::new();
    for (i, ch) in g.chars().enumerate() {
        if i > 0 && (g.len() - i) % 3 == 0 {
            ganz.push('.');
        }
        ganz.push(ch);
    }
    let mut bruch = b.to_string();
    while bruch.len() < min {
        bruch.push('0');
    }
    let mut s = if neg { format!("−{ganz}") } else { ganz };
    if !bruch.is_empty() {
        s.push(',');
        s.push_str(&bruch);
    }
    s
}

/// Liest eine getippte Zahl: „20,50“, „1.600,5“, „0.45“. Höchstens vier
/// Nachkommastellen, nicht negativ.
pub fn lesen(text: &str) -> Option<Dez> {
    let t = text.trim();
    let t = if t.contains(',') {
        t.replace('.', "").replace(',', ".")
    } else {
        t.to_string()
    };
    Dez::lesen(&t, 4).filter(|d| d.0 >= 0)
}

pub struct PreisBlatt {
    /// Position im Kostenblatt und Schlüssel ihrer Zeile.
    pub pos: usize,
    pub key: u64,
    titel: String,
    /// Einheit der Position („m²“).
    einheit: &'static str,
    katalog: Rc<Katalog>,
    aufbau: Aufbau,
    firma: Option<Aufbau>,
    /// Aufbau mit den getippten Werten (Vorschau), sonst `aufbau`.
    live: Option<Aufbau>,
    felder: Vec<Feld>,
    fokus: usize,
    /// „Planstein gilt auch für: IW Porenbeton 17,5“ je Hauptstoff.
    auch: Vec<String>,
    abweichend: Vec<SatzId>,
    pub gilt: Gilt,
    /// Befundsatz der Vorschau (ungültige Eingabe).
    pub fehler: Option<String>,
    hot: Option<Ziel>,
    /// Monat der Preise („10/2026“) für `PreisSetzen`.
    stand: String,
    /// EP-Zelle (px): links, oben, rechts, unten.
    anker: Rect,
    pub scale: f32,
    /// Fensterbreite und -höhe (px).
    pub fenster: (f32, f32),
}

impl PreisBlatt {
    /// Blatt zur Position `a` (Aufbau auf dem wirksamen Katalog `k`);
    /// `firma`: derselbe Aufbau auf dem Firmen- oder Werkskatalog.
    pub fn neu(
        k: Rc<Katalog>,
        a: Aufbau,
        firma: Option<Aufbau>,
        (pos, key): (usize, u64),
        stand: String,
    ) -> PreisBlatt {
        let einheit = a.einheit.zeichen();
        let mut felder = Vec::new();
        if a.nu.is_none() {
            let firma_h = firma.as_ref().map(|f| f.stunden);
            felder.push(Feld {
                art: Art::Stunden,
                name: "Aufwandswert".into(),
                einheit: format!("h/{einheit}"),
                alt: a.stunden,
                firma: firma_h,
                edit: TextEdit::new(&zahl(a.stunden, 2)),
                wert: Some(a.stunden),
            });
            for t in a.stoffe.iter().filter(|t| t.haupt) {
                let Some(g) = t.artikel else { continue };
                let firma_p = firma.as_ref().and_then(|f| {
                    f.stoffe
                        .iter()
                        .find(|x| x.artikel == Some(g))
                        .map(|x| x.preis)
                });
                felder.push(Feld {
                    art: Art::Preis(g),
                    name: t.name.clone(),
                    einheit: format!("€/{}", t.einheit.zeichen()),
                    alt: t.preis,
                    firma: firma_p,
                    edit: TextEdit::new(&zahl(t.preis, 2)),
                    wert: Some(t.preis),
                });
            }
        }
        let mut auch = Vec::new();
        for t in a.stoffe.iter().filter(|t| t.haupt) {
            let Some(g) = t.artikel else { continue };
            let weitere = preis::auch_fuer(&k, g, a.leistung);
            if !weitere.is_empty() {
                // Kopfwort des Namens: „Porenbeton-Planstein PP2…“ → „Planstein“
                let wort = t.name.split(' ').next().unwrap_or(&t.name);
                let kurz = wort.rsplit('-').next().unwrap_or(wort);
                auch.push(format!("{kurz} gilt auch für: {}", weitere.join(", ")));
            }
        }
        let abweichend = preis::abweichend(&k, &a);
        // Fokus im ersten Stoffpreis (die häufigste Änderung), sonst im
        // Aufwandswert
        let fokus = felder
            .iter()
            .position(|f| matches!(f.art, Art::Preis(_)))
            .unwrap_or(0);
        PreisBlatt {
            pos,
            key,
            titel: format!("Einheitspreis · {}", a.kurz),
            einheit,
            katalog: k,
            aufbau: a,
            firma,
            live: None,
            felder,
            fokus,
            auch,
            abweichend,
            gilt: Gilt::default(),
            fehler: None,
            hot: None,
            stand,
            anker: (0.0, 0.0, 0.0, 0.0),
            scale: 1.0,
            fenster: (0.0, 0.0),
        }
    }

    /// Die getippten Werte; ungültige Felder zählen nicht.
    fn eingabe(&self) -> Eingabe {
        let mut e = Eingabe::default();
        for f in &self.felder {
            match (f.art, f.wert) {
                (Art::Stunden, Some(h)) => e.stunden = Some(h),
                (Art::Preis(g), Some(p)) => e.preise.push((g, p)),
                _ => {}
            }
        }
        e
    }

    /// Operationen der getippten Werte (leer, wenn nichts anders ist).
    pub fn ops(&self) -> Vec<Op> {
        preis::preis_ops(&self.katalog, &self.aufbau, &self.eingabe(), &self.stand)
    }

    /// Ist ein Feld ungültig? Dann geht Enter nicht (Bedienbarkeit 4.2).
    pub fn ungueltig(&self) -> bool {
        self.felder.iter().any(|f| f.wert.is_none())
    }

    /// Vorschau übernehmen: Aufbau auf dem Katalog des Plans, oder der
    /// Befundsatz, wenn der Plan nicht geht.
    pub fn set_live(&mut self, live: Result<Option<Aufbau>, String>) {
        match live {
            Ok(a) => {
                self.live = a;
                self.fehler = None;
            }
            Err(satz) => {
                self.live = None;
                self.fehler = Some(satz);
            }
        }
    }

    /// Lage der EP-Zelle (px), an die das Blatt zeigt.
    pub fn set_anker(&mut self, r: Rect) {
        self.anker = r;
    }

    fn zeige(&self) -> &Aufbau {
        self.live.as_ref().unwrap_or(&self.aufbau)
    }

    /// Steht ein Feld nicht auf dem Firmenwert?
    fn anders_als_firma(&self) -> bool {
        self.felder
            .iter()
            .any(|f| f.firma.is_some_and(|x| f.wert != Some(x)))
    }

    fn zurueck_sichtbar(&self) -> bool {
        !self.abweichend.is_empty() || self.anders_als_firma()
    }

    // --- Lage ----------------------------------------------------------------

    fn nebenstoffe(&self) -> usize {
        self.zeige().stoffe.iter().filter(|t| !t.haupt).count()
    }

    /// Höhe des Inhalts (dip).
    fn hoehe(&self) -> f32 {
        let mut h = PAD + 22.0; // Titel
        h += 12.0 + BALKEN_H + 26.0; // Balken und Beschriftung
        h += ZEILE * (self.felder.len() + self.nebenstoffe()) as f32;
        if self.aufbau.nu.is_some() {
            h += ZEILE;
        }
        h += 18.0 * self.auch.len() as f32;
        if self.fehler.is_some() || self.ungueltig() {
            h += 18.0;
        }
        h += 14.0 + SEG_H; // Segment
        h += 12.0 + 16.0; // Folgezeile
        if self.gilt == Gilt::NeueHaeuser {
            h += 16.0;
        }
        if self.zurueck_sichtbar() {
            h += 22.0;
        }
        h + PAD
    }

    /// Fläche des Blatts (px) und ob es unter dem Anker liegt.
    fn rect(&self) -> (Rect, bool) {
        let s = self.scale;
        let (w, h) = (W * s, self.hoehe() * s);
        let (ax0, ay0, ax1, ay1) = self.anker;
        let (fw, fh) = self.fenster;
        let rand = 8.0 * s;
        let cx = (ax0 + ax1) * 0.5;
        let x = (cx - w * 0.8).min(fw - w - rand).max(rand);
        let unten = ay1 + SPITZE * s + h + rand <= fh || ay0 - SPITZE * s - h < rand;
        let y = if unten {
            ay1 + SPITZE * s
        } else {
            ay0 - SPITZE * s - h
        };
        ((x, y, w, h), unten)
    }

    /// Feld `i` (px).
    fn feld_rect(&self, i: usize) -> Rect {
        let s = self.scale;
        let ((x, y, _, _), _) = self.rect();
        let y0 = y + (PAD + 22.0 + 12.0 + BALKEN_H + 26.0) * s;
        (
            x + FELD_X * s,
            y0 + (i as f32 * ZEILE + (ZEILE - FELD_H) * 0.5) * s,
            FELD_W * s,
            FELD_H * s,
        )
    }

    /// Oberkante des Segments (px, relativ zum Blatt in dip gerechnet).
    fn seg_y(&self) -> f32 {
        let mut y = PAD + 22.0 + 12.0 + BALKEN_H + 26.0;
        y += ZEILE * (self.felder.len() + self.nebenstoffe()) as f32;
        if self.aufbau.nu.is_some() {
            y += ZEILE;
        }
        y += 18.0 * self.auch.len() as f32;
        if self.fehler.is_some() || self.ungueltig() {
            y += 18.0;
        }
        y + 14.0
    }

    fn segmente(&self, fonts: &Fonts) -> Vec<(Gilt, Rect)> {
        let s = self.scale;
        let ((x, y, _, _), _) = self.rect();
        let px = 10.5 * s;
        let bold = fonts.bold.as_ref().or(fonts.regular.as_ref());
        let mut sx = x + 84.0 * s;
        let sy = y + self.seg_y() * s;
        let inset = 2.0 * s;
        Gilt::ALLE
            .iter()
            .map(|&g| {
                let w = bold.map_or(120.0 * s, |f| f.width(g.label(), px)) + 28.0 * s;
                let r = (sx + inset, sy + inset, w, SEG_H * s - 2.0 * inset);
                sx += w;
                (g, r)
            })
            .collect()
    }

    fn zurueck_rect(&self, fonts: &Fonts) -> Option<Rect> {
        if !self.zurueck_sichtbar() {
            return None;
        }
        let s = self.scale;
        let ((x, y, w, h), _) = self.rect();
        let px = 10.5 * s;
        let tw = fonts
            .bold
            .as_ref()
            .or(fonts.regular.as_ref())
            .map_or(140.0 * s, |f| f.width("Firmenpreis zurückholen", px));
        Some((x + w - PAD * s - tw, y + h - (PAD + 18.0) * s, tw, 18.0 * s))
    }

    fn schliessen_rect(&self) -> Rect {
        let s = self.scale;
        let ((x, y, w, _), _) = self.rect();
        (
            x + w - (PAD + 16.0) * s,
            y + (PAD - 2.0) * s,
            20.0 * s,
            20.0 * s,
        )
    }

    fn hit(&self, fonts: &Fonts, x: f32, y: f32) -> Option<Ziel> {
        let (r, _) = self.rect();
        if !inside(r, x, y) {
            return None;
        }
        if inside(self.schliessen_rect(), x, y) {
            return Some(Ziel::Schliessen);
        }
        if let Some(i) = (0..self.felder.len()).find(|&i| inside(self.feld_rect(i), x, y)) {
            return Some(Ziel::Feld(i));
        }
        if let Some((g, _)) = self
            .segmente(fonts)
            .into_iter()
            .find(|(_, r)| inside(*r, x, y))
        {
            return Some(Ziel::Segment(g));
        }
        if self.zurueck_rect(fonts).is_some_and(|r| inside(r, x, y)) {
            return Some(Ziel::Zurueck);
        }
        Some(Ziel::Innen)
    }

    /// Liegt (x, y) auf dem Blatt (px)?
    pub fn enthaelt(&self, x: f32, y: f32) -> bool {
        inside(self.rect().0, x, y)
    }

    // --- Ereignisse ----------------------------------------------------------

    fn anwenden(&self) -> Option<Aus> {
        if self.ungueltig() || self.fehler.is_some() {
            return None;
        }
        Some(Aus::Anwenden {
            ops: self.ops(),
            gilt: self.gilt,
        })
    }

    /// Feld `i` neu lesen; `Live`, wenn sich die Operationen ändern.
    fn geaendert(&mut self, i: usize) -> Option<Aus> {
        let vorher = self.ops();
        let f = &mut self.felder[i];
        f.wert = lesen(&f.edit.text);
        Some(if self.ops() != vorher {
            Aus::Live
        } else {
            Aus::Repaint
        })
    }

    pub fn mouse_move(&mut self, fonts: &Fonts, x: f32, y: f32) -> bool {
        let hot = self.hit(fonts, x, y);
        let look = |h: Option<Ziel>| h.filter(|z| *z != Ziel::Innen);
        let changed = look(hot) != look(self.hot);
        self.hot = hot;
        changed
    }

    /// Klick (px). Daneben schreibt (wie Enter), ein ungültiges Feld hält
    /// das Blatt offen.
    pub fn mouse_down(&mut self, fonts: &Fonts, x: f32, y: f32) -> Option<Aus> {
        match self.hit(fonts, x, y) {
            None => self.anwenden().or(Some(Aus::Repaint)),
            Some(Ziel::Schliessen) => Some(Aus::Verwerfen),
            Some(Ziel::Feld(i)) => {
                let s = self.scale;
                let f = &self.felder[i];
                let r = self.feld_rect(i);
                let px = 11.0 * s;
                let font = fonts.regular.as_ref();
                let unit_w = font.map_or(0.0, |ft| ft.width(&f.einheit, px));
                let text_w = font.map_or(0.0, |ft| ft.width(&f.edit.text, px));
                let tx = r.0 + r.2 - 8.0 * s - unit_w - 4.0 * s - text_w;
                let c = widgets::caret_at(font, &f.edit.text, px, tx, x);
                self.fokus = i;
                self.felder[i].edit.place(c, false);
                Some(Aus::Repaint)
            }
            Some(Ziel::Segment(g)) => (g != self.gilt).then(|| {
                self.gilt = g;
                Aus::Repaint
            }),
            Some(Ziel::Zurueck) => {
                if !self.abweichend.is_empty() {
                    return Some(Aus::Zuruecknehmen(self.abweichend.clone()));
                }
                // Noch nichts geschrieben: die Felder auf den Firmenwert
                let vorher = self.ops();
                for f in &mut self.felder {
                    if let Some(x) = f.firma {
                        f.edit = TextEdit::new(&zahl(x, 2));
                        f.wert = Some(x);
                    }
                }
                Some(if self.ops() != vorher {
                    Aus::Live
                } else {
                    Aus::Repaint
                })
            }
            Some(Ziel::Innen) => None,
        }
    }

    pub fn key(&mut self, key: Key, mods: Modifiers) -> Option<Aus> {
        match key {
            Key::Escape => return Some(Aus::Verwerfen),
            Key::Enter => return self.anwenden().or(Some(Aus::Repaint)),
            Key::Tab => {
                let n = self.felder.len();
                if n > 0 {
                    self.fokus = if mods.shift {
                        (self.fokus + n - 1) % n
                    } else {
                        (self.fokus + 1) % n
                    };
                    self.felder[self.fokus].edit.select_all();
                }
                return Some(Aus::Repaint);
            }
            _ => {}
        }
        let i = self.fokus;
        let e = &mut self.felder.get_mut(i)?.edit;
        let sh = mods.shift;
        let changed = match key {
            Key::Backspace => {
                e.backspace();
                true
            }
            Key::Delete => {
                e.delete();
                true
            }
            Key::Left => {
                e.left(sh);
                false
            }
            Key::Right => {
                e.right(sh);
                false
            }
            Key::Home => {
                e.home(sh);
                false
            }
            Key::End => {
                e.end(sh);
                false
            }
            Key::Char('A') if mods.ctrl => {
                e.select_all();
                false
            }
            Key::Char('C') if mods.ctrl => {
                sk_platform::set_clipboard_text(e.selected());
                false
            }
            Key::Char('X') if mods.ctrl => {
                let cut = e.cut();
                sk_platform::set_clipboard_text(&cut);
                true
            }
            Key::Char('V') if mods.ctrl => {
                let paste = sk_platform::clipboard_text().unwrap_or_default();
                e.insert(paste.lines().next().unwrap_or("").trim());
                true
            }
            Key::Char('Z') if mods.ctrl => e.undo(),
            _ => return None,
        };
        if changed {
            self.geaendert(i)
        } else {
            Some(Aus::Repaint)
        }
    }

    /// Getipptes Zeichen: nur Ziffern, Komma und Punkt.
    pub fn text(&mut self, ch: char) -> Option<Aus> {
        if !(ch.is_ascii_digit() || matches!(ch, ',' | '.')) {
            return None;
        }
        let i = self.fokus;
        self.felder
            .get_mut(i)?
            .edit
            .insert(ch.encode_utf8(&mut [0; 4]));
        self.geaendert(i)
    }

    /// Tooltip am Segment (Bedienbarkeit 4.6).
    pub fn tip_at(&self, fonts: &Fonts, x: f32, y: f32) -> Option<String> {
        match self.hit(fonts, x, y)? {
            Ziel::Segment(Gilt::NeueHaeuser) => {
                Some("Auch für neue Häuser: speichert im Firmenkatalog".into())
            }
            _ => None,
        }
    }

    // --- Zeichnen ------------------------------------------------------------

    pub fn paint(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let s = self.scale;
        let u = &t.ui;
        let ((x, y, w, h), unten) = self.rect();
        flaeche(c, (x, y, w, h), self.anker, unten, s, t);
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let (Some(f), Some(fb)) = (regular, bold) else {
            return;
        };
        let x0 = x + PAD * s;
        let x1 = x + w - PAD * s;
        let a = self.zeige();
        // Titel und ×
        let px = 12.0 * s;
        let tb = y + PAD * s + fb.cap_height(px);
        let titel = widgets::ellipsize(Some(fb), &self.titel, px, x1 - x0 - 28.0 * s);
        fb.draw(c, &titel, px, x0, tb.round(), u.sheet_text);
        let (cx, cy, cw, ch) = self.schliessen_rect();
        let col = if self.hot == Some(Ziel::Schliessen) {
            u.sheet_text
        } else {
            u.sheet_text_dim
        };
        let (mx, my, d) = (cx + cw * 0.5, cy + ch * 0.5, 4.0 * s);
        let mut p = Path::new();
        p.segment((mx - d, my - d), (mx + d, my + d), 1.4 * s);
        p.segment((mx - d, my + d), (mx + d, my - d), 1.4 * s);
        c.fill(&p, col);
        // Balken Lohn, Stoff, Gerät, Sonstiges; rechts der EP groß
        let by = y + (PAD + 22.0 + 12.0) * s;
        let ep_text = format!("{} €/{}", a.ep.deutsch(), self.einheit);
        let px_ep = 17.0 * s;
        let ep_w = fb.width(&ep_text, px_ep);
        let bw = (x1 - x0 - ep_w - 24.0 * s).max(60.0 * s);
        let teile = [
            (a.lohn, u.text_dim, "Lohn"),
            (a.stoff, u.accent, "Stoff"),
            (a.geraet, u.border, "Gerät"),
            (a.sonst, u.text_disabled, "Sonstiges"),
        ];
        let summe: i64 = teile.iter().map(|(c, _, _)| c.0.max(0)).sum();
        let px_s = 10.0 * s;
        if a.nu.is_none() && summe > 0 {
            let gap = 2.0 * s;
            let mut bx = x0;
            for (betrag, farbe, name) in teile {
                if betrag.0 <= 0 {
                    continue;
                }
                let sw = bw * betrag.0 as f32 / summe as f32;
                let mut p = Path::new();
                p.rounded_rect(bx, by, (sw - gap).max(1.0), BALKEN_H * s, 2.0 * s);
                c.fill(&p, farbe);
                let text = format!("{name} {}", betrag.deutsch());
                if f.width(&text, px_s) < sw + 40.0 * s {
                    f.draw(
                        c,
                        &text,
                        px_s,
                        bx,
                        by + (BALKEN_H + 14.0) * s,
                        u.sheet_text_dim,
                    );
                }
                bx += sw;
            }
        } else if let Some(nu) = a.nu {
            let text = format!("Nachunternehmer {}", nu.deutsch());
            f.draw(c, &text, px_s, x0, by + BALKEN_H * s, u.sheet_text_dim);
        }
        fb.draw(
            c,
            &ep_text,
            px_ep,
            x1 - ep_w,
            by + BALKEN_H * s,
            u.sheet_text,
        );
        if let Some(fa) = &self.firma {
            let text = format!("Firma {}", fa.ep.deutsch());
            let tw = f.width(&text, px_s);
            f.draw(
                c,
                &text,
                px_s,
                x1 - tw,
                by + (BALKEN_H + 14.0) * s,
                u.sheet_text_dim,
            );
        }
        // Felder
        let px_f = 11.0 * s;
        let mut zy = y + (PAD + 22.0 + 12.0 + BALKEN_H + 26.0) * s;
        let mitte = |zy: f32, ft: &sk_paint::font::Font, px: f32| {
            (zy + (ZEILE * s + ft.cap_height(px)) * 0.5).round()
        };
        for (i, fe) in self.felder.iter().enumerate() {
            let base = mitte(zy, f, px_f);
            let name = widgets::ellipsize(Some(f), &fe.name, px_f, (FELD_X - 12.0) * s);
            f.draw(c, &name, px_f, x0, base, u.sheet_text_dim);
            self.paint_feld(c, t, fonts, i);
            let notiz = match fe.art {
                Art::Stunden => {
                    let mut n = format!("× {} €/h", zahl(a.lohnsatz, 2));
                    if let Some(fh) = fe.firma.filter(|x| *x != fe.alt) {
                        n = format!("Firma {} · {n}", zahl(fh, 2));
                    }
                    n
                }
                Art::Preis(_) => fe
                    .firma
                    .map_or(String::new(), |x| format!("Firma {}", zahl(x, 2))),
            };
            let nx = x + (FELD_X + FELD_W + 12.0) * s;
            let notiz = widgets::ellipsize(Some(f), &notiz, px_s, x1 - nx);
            f.draw(c, &notiz, px_s, nx, base, u.sheet_text_dim);
            zy += ZEILE * s;
        }
        // Nebenstoffe als Zeile: „Dünnbettmörtel 4,4 kg   4,84 €/m²   1,10 €/kg“
        for st in a.stoffe.iter().filter(|t| !t.haupt) {
            let base = mitte(zy, f, px_f);
            let name = format!("{} {} {}", st.name, zahl(st.menge, 0), st.einheit.zeichen());
            let name = widgets::ellipsize(Some(f), &name, px_f, (FELD_X - 12.0) * s);
            f.draw(c, &name, px_f, x0, base, u.sheet_text_dim);
            let betrag = format!("{} €/{}", Aufbau::betrag(st).deutsch(), self.einheit);
            let rx = x + (FELD_X + FELD_W - 8.0) * s;
            f.draw(
                c,
                &betrag,
                px_f,
                rx - f.width(&betrag, px_f),
                base,
                u.sheet_text,
            );
            let preis = format!("{} €/{}", zahl(st.preis, 2), st.einheit.zeichen());
            let nx = x + (FELD_X + FELD_W + 12.0) * s;
            f.draw(c, &preis, px_s, nx, base, u.sheet_text_dim);
            zy += ZEILE * s;
        }
        if self.aufbau.nu.is_some() {
            let base = mitte(zy, f, px_f);
            let text = "Nachunternehmerpreis, änderbar in der Verwaltung";
            f.draw(c, text, px_s, x0, base, u.sheet_text_dim);
            zy += ZEILE * s;
        }
        for l in &self.auch {
            let l = widgets::ellipsize(Some(f), l, px_s, x1 - x0);
            f.draw(c, &l, px_s, x0, zy + 12.0 * s, u.sheet_text_dim);
            zy += 18.0 * s;
        }
        if self.fehler.is_some() || self.ungueltig() {
            let satz = self
                .fehler
                .clone()
                .unwrap_or_else(|| "Bitte eine Zahl ab 0 eingeben, etwa 20,50.".into());
            let satz = widgets::ellipsize(Some(f), &satz, px_s, x1 - x0);
            f.draw(c, &satz, px_s, x0, zy + 12.0 * s, u.field_invalid);
        }
        // Segment „Gilt für“
        let segs = self.segmente(fonts);
        if let (Some(&(_, first)), Some(&(_, last))) = (segs.first(), segs.last()) {
            let inset = 2.0 * s;
            let (sx, sy, sh) = (first.0 - inset, first.1 - inset, first.3 + 2.0 * inset);
            let sw = last.0 + last.2 + inset - sx;
            let mut p = Path::new();
            p.rounded_rect(sx, sy, sw, sh, 6.0 * s);
            c.fill(&p, u.sheet_tile);
            f.draw(
                c,
                "Gilt für",
                px_s,
                x0,
                sy + (sh + f.cap_height(px_s)) * 0.5,
                u.sheet_text_dim,
            );
            let px_seg = 10.5 * s;
            for &(g, (rx, ry, rw, rh)) in &segs {
                let (font, col) = if g == self.gilt {
                    let mut p = Path::new();
                    p.rounded_rect(rx, ry, rw, rh, 5.0 * s);
                    c.fill(&p, u.sheet_rule);
                    let mut p = Path::new();
                    let b = s.max(1.0);
                    p.rounded_rect(rx + b, ry + b, rw - 2.0 * b, rh - 2.0 * b, 5.0 * s - b);
                    c.fill(&p, u.sheet_card);
                    (fb, u.sheet_text)
                } else if self.hot == Some(Ziel::Segment(g)) {
                    (f, u.sheet_text)
                } else {
                    (f, u.sheet_text_dim)
                };
                let tw = font.width(g.label(), px_seg);
                font.draw(
                    c,
                    g.label(),
                    px_seg,
                    rx + (rw - tw) * 0.5,
                    ry + (rh + font.cap_height(px_seg)) * 0.5,
                    col,
                );
            }
            // Folgezeile in eigener Zeile (Bedienbarkeit 4.10)
            let fy = sy + sh + 12.0 * s + f.cap_height(px_s);
            let neu_ep = format!("{} €/{}", a.ep.deutsch(), self.einheit);
            match self.gilt {
                Gilt::NurHaus => {
                    let firma = self.firma.as_ref().map_or(self.aufbau.ep, |x| x.ep);
                    let text = format!(
                        "Neue Häuser rechnen weiter mit {} €/{}.",
                        firma.deutsch(),
                        self.einheit
                    );
                    f.draw(c, &text, px_s, x0, fy, u.sheet_text_dim);
                }
                Gilt::NeueHaeuser => {
                    let text = format!("Neue Häuser rechnen dann mit {neu_ep}.");
                    fb.draw(c, &text, px_s, x0, fy, u.accent);
                    f.draw(
                        c,
                        "Strg+Z nimmt es nur für dieses Haus zurück.",
                        px_s,
                        x0,
                        fy + 16.0 * s,
                        u.sheet_text_dim,
                    );
                }
            }
        }
        if let Some((rx, ry, _, rh)) = self.zurueck_rect(fonts) {
            let px_l = 10.5 * s;
            let col = if self.hot == Some(Ziel::Zurueck) {
                u.accent_hover
            } else {
                u.accent
            };
            fb.draw(
                c,
                "Firmenpreis zurückholen",
                px_l,
                rx,
                ry + (rh + fb.cap_height(px_l)) * 0.5,
                col,
            );
        }
    }

    fn paint_feld(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, i: usize) {
        let s = self.scale;
        let u = &t.ui;
        let fe = &self.felder[i];
        let (x, y, w, h) = self.feld_rect(i);
        let fokus = i == self.fokus;
        let rand = if fe.wert.is_none() {
            u.field_invalid
        } else if fokus {
            u.accent
        } else if self.hot == Some(Ziel::Feld(i)) {
            u.sheet_text_dim
        } else {
            u.sheet_rule
        };
        let b = if fokus { 1.5 * s } else { s.max(1.0) };
        let mut p = Path::new();
        p.rounded_rect(x, y, w, h, 4.0 * s);
        c.fill(&p, rand);
        let mut p = Path::new();
        p.rounded_rect(x + b, y + b, w - 2.0 * b, h - 2.0 * b, 4.0 * s - b);
        c.fill(&p, u.sheet_card);
        let Some(f) = fonts.regular.as_ref() else {
            return;
        };
        let px = 11.0 * s;
        let px_u = 10.0 * s;
        let base = (y + (h + f.cap_height(px)) * 0.5).round();
        let unit_w = f.width(&fe.einheit, px_u);
        let ux = x + w - 8.0 * s - unit_w;
        f.draw(c, &fe.einheit, px_u, ux, base, u.sheet_text_dim);
        let text = &fe.edit.text;
        let tx = ux - 4.0 * s - f.width(text, px);
        if fokus {
            let (a, b) = fe.edit.selection();
            if a < b {
                let sx = tx + f.width(&text[..a], px);
                let sw = f.width(&text[a..b], px);
                c.fill_rect(sx, y + 5.0 * s, sw, h - 10.0 * s, u.text_select);
            }
        }
        f.draw(c, text, px, tx, base, u.sheet_text);
        if fokus {
            let cx = (tx + f.width(&text[..fe.edit.caret], px)).round();
            c.fill_rect(cx, y + 6.0 * s, s.max(1.0), h - 12.0 * s, u.sheet_text);
        }
    }
}

/// Helle Fläche mit Schatten wie Menüs, Radius 8 und der Spitze zum EP.
fn flaeche(c: &mut Canvas, (x, y, w, h): Rect, anker: Rect, unten: bool, s: f32, t: &Theme) {
    let u = &t.ui;
    let rad = RADIUS * s;
    for i in 1..=4 {
        let d = i as f32 * 2.0 * s;
        let mut p = Path::new();
        p.rounded_rect(x - d * 0.5, y - d * 0.25, w + d, h + d, rad + d);
        c.fill(&p, u.shadow);
    }
    let b = s.max(1.0);
    let mut p = Path::new();
    p.rounded_rect(x, y, w, h, rad);
    c.fill(&p, u.sheet_rule);
    // Spitze: Rand, dann Fläche darüber
    let cx = ((anker.0 + anker.2) * 0.5).clamp(x + rad + SPITZE * s, x + w - rad - SPITZE * s);
    let sp = SPITZE * s;
    let spitze = |c: &mut Canvas, d: f32, col: Rgba| {
        let mut p = Path::new();
        if unten {
            p.move_to(cx - sp + d, y + b)
                .line_to(cx, y - sp + d * 1.4)
                .line_to(cx + sp - d, y + b)
                .close();
        } else {
            p.move_to(cx - sp + d, y + h - b)
                .line_to(cx, y + h + sp - d * 1.4)
                .line_to(cx + sp - d, y + h - b)
                .close();
        }
        c.fill(&p, col);
    };
    spitze(c, 0.0, u.sheet_rule);
    let mut p = Path::new();
    p.rounded_rect(x + b, y + b, w - 2.0 * b, h - 2.0 * b, rad - b);
    c.fill(&p, u.sheet_card);
    spitze(c, b, u.sheet_card);
}

/// Monat der Preise wie in `PreisSetzen` („10/2026“).
pub fn stand_jetzt() -> String {
    let (j, m, _, _, _) = sk_platform::local_date_time();
    format!("{m:02}/{j}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_cost::rechnung::Quelle;
    use sk_cost::Cent;
    use sk_model::{qto, szo, GuidGen, Model};

    fn rh1() -> Model {
        szo::read_with(
            include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
            GuidGen::with_seed(1),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .expect("lädt")
        .model
    }

    fn mauerwerk(m: &Model) -> (Rc<Katalog>, sk_cost::Kostenblatt, usize) {
        let k = Rc::new(sk_cost::lesen::katalog(m, None));
        let b = sk_cost::lesen::kosten(m, &qto::schedule(m), &k, &sk_cost::Umfang::projekt());
        let i = b
            .positionen
            .iter()
            .position(|p| p.kurz.contains("Porenbeton") && p.kurz.contains("17,5"))
            .expect("Mauerwerk 17,5");
        assert!(matches!(b.positionen[i].quelle, Quelle::Leistung(_)));
        (k, b, i)
    }

    fn blatt(m: &Model) -> PreisBlatt {
        let (k, b, i) = mauerwerk(m);
        let a = preis::aufbau(m, &k, &b.positionen[i]).unwrap();
        let fk = sk_cost::lesen::firma_oder_werk(m, None);
        let f = preis::aufbau(m, &fk, &b.positionen[i]);
        let mut pb = PreisBlatt::neu(k, a, f, (i, 7), "10/2026".into());
        pb.fenster = (1200.0, 900.0);
        pb.set_anker((900.0, 300.0, 980.0, 322.0));
        pb
    }

    #[test]
    fn zahlen_lesen_und_schreiben() {
        assert_eq!(zahl(Dez::lesen("0.45", 4).unwrap(), 2), "0,45");
        assert_eq!(zahl(Dez::lesen("4.4", 4).unwrap(), 0), "4,4");
        assert_eq!(zahl(Dez::lesen("1600", 4).unwrap(), 2), "1.600,00");
        assert_eq!(zahl(Dez::lesen("22.16", 4).unwrap(), 2), "22,16");
        assert_eq!(lesen("20,50"), Dez::lesen("20.5", 4));
        assert_eq!(lesen("1.600,5"), Dez::lesen("1600.5", 4));
        assert_eq!(lesen("0.45"), Dez::lesen("0.45", 4));
        for x in ["", ",", "-1", "1,2,3", "abc", "1,23456"] {
            assert_eq!(lesen(x), None, "{x}");
        }
    }

    /// Abnahme 11 (soll-ka-2c): Felder Aufwandswert und Planstein, Mörtel
    /// als Zeile; „20,50“ getippt gibt genau ein `PreisSetzen`, Enter
    /// schreibt, Esc verwirft; ein ungültiges Feld lässt Enter nicht zu.
    #[test]
    fn tippen_enter_und_esc() {
        let m = rh1();
        let mut pb = blatt(&m);
        let namen: Vec<&str> = pb.felder.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(namen.len(), 2, "{namen:?}");
        assert_eq!(namen[0], "Aufwandswert");
        assert!(namen[1].contains("Planstein"), "{namen:?}");
        assert_eq!(pb.felder[0].edit.text, "0,45");
        assert_eq!(pb.nebenstoffe(), 1);
        assert!(
            pb.auch[0].starts_with("Planstein gilt auch für: "),
            "{:?}",
            pb.auch
        );
        assert_eq!(pb.fokus, 1, "Fokus im Stoffpreis");
        assert!(pb.ops().is_empty());
        let none = Modifiers::default();
        // Ganzes Feld markiert: Tippen ersetzt
        for ch in "20,5".chars() {
            pb.text(ch);
        }
        assert_eq!(pb.felder[1].edit.text, "20,5");
        let ops = pb.ops();
        assert_eq!(ops.len(), 1);
        assert!(
            matches!(&ops[0], Op::PreisSetzen { preis: Some(p), stand, .. }
            if *p == Dez::lesen("20.5", 4).unwrap() && stand == "10/2026")
        );
        // Vorschau wie die Scene sie rechnet
        let plan = sk_cost::vorschau(&m, None, sk_cost::Rolle::Admin, &ops).unwrap();
        let (_, b, i) = mauerwerk(&m);
        pb.set_live(Ok(preis::aufbau(&m, &plan.katalog, &b.positionen[i])));
        assert_eq!(pb.zeige().ep, Cent(5_234));
        assert!(pb.zurueck_sichtbar(), "Feld weicht von der Firma ab");
        // Segment wählt nur
        pb.gilt = Gilt::NeueHaeuser;
        assert_eq!(
            pb.key(Key::Enter, none),
            Some(Aus::Anwenden {
                ops: ops.clone(),
                gilt: Gilt::NeueHaeuser
            })
        );
        // Ungültig: Enter geht nicht
        pb.text(',');
        assert!(pb.ungueltig());
        assert_eq!(pb.key(Key::Enter, none), Some(Aus::Repaint));
        assert_eq!(pb.key(Key::Escape, none), Some(Aus::Verwerfen));
        // Buchstaben kommen nicht ins Feld
        assert_eq!(pb.text('x'), None);
    }

    /// Lage: unter dem EP, im Fenster; ohne Platz unten darüber; Klick
    /// daneben schreibt, Klick auf × verwirft.
    #[test]
    fn lage_und_klick() {
        let m = rh1();
        let mut pb = blatt(&m);
        let fonts = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        let ((x, y, w, h), unten) = pb.rect();
        assert!(unten);
        assert!(x >= 0.0 && x + w <= 1200.0 && y > 322.0 && y + h <= 900.0);
        assert!(x < 940.0 && x + w > 940.0, "Spitze unter dem EP");
        pb.set_anker((900.0, 820.0, 980.0, 842.0));
        let ((_, y2, _, h2), unten) = pb.rect();
        assert!(!unten && y2 + h2 < 820.0);
        assert_eq!(
            pb.mouse_down(&fonts, 5.0, 5.0),
            Some(Aus::Anwenden {
                ops: Vec::new(),
                gilt: Gilt::NurHaus
            })
        );
        let (cx, cy, _, _) = pb.schliessen_rect();
        assert_eq!(
            pb.mouse_down(&fonts, cx + 5.0, cy + 5.0),
            Some(Aus::Verwerfen)
        );
        let mut c = Canvas::new(1200, 900);
        pb.paint(&mut c, &Theme::dark(), &fonts);
    }
}

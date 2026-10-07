//! Karten zu den Sicherungen (F-13): die Startkarte nach einem Absturz
//! (Sollbild `soll-sichern-1.png`) und die Liste „Sicherungen …“ aus dem
//! Dateimenü in derselben Kartenform. Gestaltung:
//! `einstellungen/paket-f13-sichern.md`.

use crate::autosave::{self, Entry, Found};
use sk_model::Model;
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::Key;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, Fonts, Rect};
use std::path::Path as FsPath;
use std::time::{Instant, SystemTime};

/// Maße (dip): Breite mit zwei Kacheln bzw. einer, Innenabstand, Kachel,
/// Abstand der Kacheln, Papier in der Kachel, Knöpfe.
const W: f32 = 560.0;
const W_NARROW: f32 = 380.0;
const PAD: f32 = 24.0;
const TILE_H: f32 = 150.0;
const TILE_GAP: f32 = 16.0;
const PAPER_INSET: f32 = 10.0;
const PAPER_H: f32 = 84.0;
const BUTTON: (f32, f32) = (150.0, 40.0);
const BUTTON_GAP: f32 = 12.0;
/// Zeilenabstand von Text und Fußzeile.
const LINE: f32 = 18.0;
const FOOT_LINE: f32 = 17.0;
/// Die Liste zeigt höchstens so viele Sicherungen (je Projekt gibt es nur
/// die jüngste).
pub const LIST_MAX: usize = 6;
/// Zwei Klicks auf dieselbe Kachel innerhalb dieser Zeit: Doppelklick.
const DOUBLE_MS: u128 = 450;

const TEXT: &str =
    "Skizzeo wurde zuletzt nicht normal beendet. Die Sicherung ist neuer als die gespeicherte Datei.";
const FOOT_START: &str = "Verwerfen öffnet die gespeicherte Datei. Die Sicherung bleibt 7 Tage im Ordner „Sicherungen“ (Dateimenü) und lässt sich dort noch öffnen.";
const FOOT_START_NEW: &str = "Verwerfen beginnt ein leeres Projekt. Die Sicherung bleibt 7 Tage im Ordner „Sicherungen“ (Dateimenü) und lässt sich dort noch öffnen.";
const TEXT_LIST: &str = "Je Projekt die jüngste Sicherung der letzten 7 Tage. Ein Klick öffnet sie wie „Wiederherstellen“.";
const TEXT_EMPTY: &str =
    "Es gibt keine Sicherungen. Skizzeo sichert alle 5 Minuten, wenn sich etwas geändert hat.";

/// Miniatur-Grundriss: Wandflächen der Erdgeschosse (mm, Draufsicht).
pub type Plan = Vec<[(f64, f64); 4]>;

/// Wandflächen der Erdgeschosse aller Gebäude.
pub fn plan_of(m: &Model) -> Plan {
    let mut out = Plan::new();
    for (id, r) in m.runs().iter() {
        if m.ground_storey(r.storey) != Some(r.storey) {
            continue;
        }
        let Some(c) = m.chain(id) else {
            continue;
        };
        for seg in 0..c.segment_count() {
            if let Some(q) = c.segment_footprint(seg) {
                out.push(q.map(|p| (p.x, p.y)));
            }
        }
    }
    out
}

/// Miniatur der Datei `p` (leer, wenn sie sich nicht lesen lässt).
fn plan_of_file(p: &FsPath) -> Plan {
    crate::document::load(p).map_or(Plan::new(), |l| plan_of(&l.model))
}

/// Eine Kachel: Miniatur, Name, Zeit, zweite Zeile (Akzent oder gedimmt).
#[derive(Clone, Debug)]
pub struct Tile {
    pub title: String,
    pub when: String,
    pub sub: String,
    pub sub_accent: bool,
    plan: Plan,
}

/// Antwort einer Karte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Startkarte: Sicherung öffnen.
    Restore,
    /// Startkarte: gespeicherte Datei (bzw. leeres Projekt) öffnen.
    Discard,
    /// Liste: Sicherung Nummer `i` öffnen.
    Open(usize),
    /// Liste: schließen.
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Tile(usize),
    Button(usize),
}

/// Startkarte oder Liste der Sicherungen.
#[derive(Clone, Debug)]
pub struct BackupCard {
    /// Startkarte: die gefundene Sicherung; `None`: Liste.
    pub found: Option<Found>,
    /// Liste: die gezeigten Sicherungen, die jüngste zuerst.
    pub entries: Vec<Entry>,
    title: String,
    text: String,
    footer: String,
    tiles: Vec<Tile>,
    /// Gewählte Kachel (Startkarte: 0 = Sicherung, 1 = gespeicherte Datei).
    pub focus: usize,
    hover: Option<Hit>,
    pressed: Option<Hit>,
    last_click: Option<(Instant, usize)>,
}

/// Ortszeit des Zeitpunkts `t` als Kachelangabe („heute, 03:41“).
fn when(t: SystemTime, now: SystemTime, local_now: (u16, u8, u8, u8, u8)) -> String {
    autosave::when_text(autosave::local_at(t, now, local_now), local_now)
}

fn modified(p: &FsPath) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn file_name(p: Option<&FsPath>) -> String {
    p.and_then(FsPath::file_name)
        .map_or("Unbenannt".to_string(), |n| {
            n.to_string_lossy().into_owned()
        })
}

impl BackupCard {
    /// Startkarte zur Sicherung `f`; `now` und `local_now` für die Zeiten.
    pub fn start(f: Found, now: SystemTime, local_now: (u16, u8, u8, u8, u8)) -> BackupCard {
        let bt = modified(&f.backup).unwrap_or(now);
        let mut tiles = vec![Tile {
            title: "Sicherung".into(),
            when: when(bt, now, local_now),
            sub: String::new(),
            sub_accent: true,
            plan: plan_of_file(&f.backup),
        }];
        if let Some(o) = f.original.as_deref().filter(|o| o.is_file()) {
            let ot = modified(o).unwrap_or(bt);
            let min = bt
                .duration_since(ot)
                .map_or(1, |d| d.as_secs().div_ceil(60));
            tiles[0].sub = autosave::age_text(min);
            tiles.push(Tile {
                title: "Gespeicherte Datei".into(),
                when: when(ot, now, local_now),
                sub: o.display().to_string(),
                sub_accent: false,
                plan: plan_of_file(o),
            });
        }
        let footer = if tiles.len() > 1 {
            FOOT_START
        } else {
            FOOT_START_NEW
        };
        BackupCard {
            title: autosave::card_title(f.original.as_deref()),
            text: TEXT.into(),
            footer: footer.into(),
            tiles,
            found: Some(f),
            entries: Vec::new(),
            focus: 0,
            hover: None,
            pressed: None,
            last_click: None,
        }
    }

    /// Liste „Sicherungen …“ aus dem Ordner `dir`.
    pub fn list(dir: &FsPath, now: SystemTime, local_now: (u16, u8, u8, u8, u8)) -> BackupCard {
        let all = autosave::list_in(dir);
        let more = all.len().saturating_sub(LIST_MAX);
        let entries: Vec<Entry> = all.into_iter().take(LIST_MAX).collect();
        let tiles = entries
            .iter()
            .map(|e| Tile {
                title: file_name(e.original.as_deref()),
                when: when(e.modified, now, local_now),
                sub: e
                    .original
                    .as_ref()
                    .map_or("nie gespeichert".into(), |p| p.display().to_string()),
                sub_accent: false,
                plan: plan_of_file(&e.path),
            })
            .collect::<Vec<_>>();
        let footer = match more {
            0 => format!("Ordner: {}", dir.display()),
            1 => format!("1 ältere im Ordner {}", dir.display()),
            n => format!("{n} ältere im Ordner {}", dir.display()),
        };
        BackupCard {
            found: None,
            title: "Sicherungen".into(),
            text: if tiles.is_empty() {
                TEXT_EMPTY
            } else {
                TEXT_LIST
            }
            .into(),
            footer,
            tiles,
            entries,
            focus: 0,
            hover: None,
            pressed: None,
            last_click: None,
        }
    }

    fn is_start(&self) -> bool {
        self.found.is_some()
    }

    /// Kacheln je Zeile.
    fn columns(&self) -> usize {
        if self.is_start() {
            self.tiles.len().max(1)
        } else {
            2
        }
    }

    fn rows(&self) -> usize {
        self.tiles.len().div_ceil(self.columns())
    }

    fn width(&self) -> f32 {
        if self.is_start() && self.tiles.len() < 2 {
            W_NARROW
        } else {
            W
        }
    }

    fn labels(&self) -> &'static [&'static str] {
        if self.is_start() {
            &["Verwerfen", "Wiederherstellen"]
        } else {
            &["Schließen"]
        }
    }

    /// Antwort von Enter bzw. der hervorgehobene Knopf.
    fn default_answer(&self) -> Answer {
        match (self.is_start(), self.focus) {
            (true, 0) => Answer::Restore,
            (true, _) => Answer::Discard,
            (false, _) if self.tiles.is_empty() => Answer::Close,
            (false, i) => Answer::Open(i),
        }
    }

    fn wrap(&self, fonts: &Fonts, s: f32, text: &str, px: f32) -> Vec<String> {
        widgets::wrap(
            fonts.regular.as_ref(),
            text,
            px * s,
            (self.width() - 2.0 * PAD) * s,
        )
    }

    /// Senkrechte Lage (dip): Oberkante der Kacheln, der Knöpfe, Trennlinie,
    /// Höhe.
    fn layout(&self, fonts: &Fonts, t: &Theme, s: f32) -> (f32, f32, f32, f32) {
        let n = self.wrap(fonts, s, &self.text, t.size.font).len().max(1) as f32;
        let tiles = 62.0 + LINE * (n - 1.0) + 22.0;
        let rows = self.rows() as f32;
        let tiles_h = rows * TILE_H + (rows - 1.0).max(0.0) * TILE_GAP;
        let buttons = tiles + tiles_h + 20.0;
        let sep = buttons + BUTTON.1 + 16.0;
        let f = self
            .wrap(fonts, s, &self.footer, t.size.font_small)
            .len()
            .max(1) as f32;
        let h = sep + 22.0 + FOOT_LINE * (f - 1.0) + 21.0;
        (tiles, buttons, sep, h)
    }

    /// Größe (Pixel, ohne Schatten).
    fn size(&self, fonts: &Fonts, t: &Theme, s: f32) -> (f32, f32) {
        let h = self.layout(fonts, t, s).3;
        ((self.width() * s).round(), (h * s).round())
    }

    /// Lage im Fenster: mittig über dem Modell.
    pub fn rect(&self, fonts: &Fonts, t: &Theme, s: f32, win_w: u32, win_h: u32, top: u32) -> Rect {
        let (w, h) = self.size(fonts, t, s);
        let x = ((win_w as f32 - w) * 0.5).round().max(0.0);
        let y = (top as f32 + (win_h as f32 - top as f32 - h) * 0.5)
            .round()
            .max(top as f32);
        Rect::new(x, y, w, h)
    }

    /// Kacheln (Pixel, relativ zur Fläche).
    fn tile_rects(&self, fonts: &Fonts, t: &Theme, s: f32) -> Vec<Rect> {
        let top = self.layout(fonts, t, s).0;
        let cols = self.columns();
        let tw = (self.width() - 2.0 * PAD - (cols as f32 - 1.0) * TILE_GAP) / cols as f32;
        (0..self.tiles.len())
            .map(|i| {
                let (c, r) = ((i % cols) as f32, (i / cols) as f32);
                Rect::new(
                    ((PAD + c * (tw + TILE_GAP)) * s).round(),
                    ((top + r * (TILE_H + TILE_GAP)) * s).round(),
                    (tw * s).round(),
                    (TILE_H * s).round(),
                )
            })
            .collect()
    }

    /// Knöpfe rechtsbündig (Pixel, relativ zur Fläche).
    fn button_rects(&self, fonts: &Fonts, t: &Theme, s: f32) -> Vec<Rect> {
        let y = self.layout(fonts, t, s).1;
        let n = self.labels().len();
        let mut x = self.width() - PAD;
        let mut out = vec![Rect::new(0.0, 0.0, 0.0, 0.0); n];
        for i in (0..n).rev() {
            x -= BUTTON.0;
            out[i] = Rect::new(
                (x * s).round(),
                (y * s).round(),
                (BUTTON.0 * s).round(),
                (BUTTON.1 * s).round(),
            );
            x -= BUTTON_GAP;
        }
        out
    }

    fn hit(&self, r: Rect, fonts: &Fonts, t: &Theme, s: f32, x: f64, y: f64) -> Option<Hit> {
        let (lx, ly) = (x - r.x as f64, y - r.y as f64);
        if let Some(i) = self
            .button_rects(fonts, t, s)
            .iter()
            .position(|b| b.contains(lx, ly))
        {
            return Some(Hit::Button(i));
        }
        self.tile_rects(fonts, t, s)
            .iter()
            .position(|b| b.contains(lx, ly))
            .map(Hit::Tile)
    }

    /// Enter öffnet die gewählte Kachel (vorgewählt: Wiederherstellen), Esc
    /// verwirft bzw. schließt, ←/→ wechseln die Kachel.
    pub fn key(&mut self, k: Key) -> Option<Answer> {
        let n = self.tiles.len();
        match k {
            Key::Enter => Some(self.default_answer()),
            Key::Escape => Some(if self.is_start() {
                Answer::Discard
            } else {
                Answer::Close
            }),
            Key::Right | Key::Tab if n > 0 => {
                self.focus = (self.focus + 1) % n;
                None
            }
            Key::Left if n > 0 => {
                self.focus = (self.focus + n - 1) % n;
                None
            }
            _ => None,
        }
    }

    /// Maus bewegt; `true`, wenn neu zu zeichnen ist.
    #[allow(clippy::too_many_arguments)]
    pub fn mouse_move(
        &mut self,
        r: Rect,
        fonts: &Fonts,
        t: &Theme,
        s: f32,
        x: f64,
        y: f64,
    ) -> bool {
        let h = self.hit(r, fonts, t, s, x, y);
        let changed = h != self.hover;
        self.hover = h;
        changed
    }

    #[allow(clippy::too_many_arguments)]
    pub fn press(&mut self, r: Rect, fonts: &Fonts, t: &Theme, s: f32, x: f64, y: f64) {
        self.pressed = self.hit(r, fonts, t, s, x, y);
    }

    /// Losgelassen auf demselben Ziel: Knopf antwortet; Kachel wird gewählt
    /// (Startkarte; ein Doppelklick öffnet sie) bzw. geöffnet (Liste).
    #[allow(clippy::too_many_arguments)]
    pub fn release(
        &mut self,
        r: Rect,
        fonts: &Fonts,
        t: &Theme,
        s: f32,
        x: f64,
        y: f64,
        now: Instant,
    ) -> Option<Answer> {
        let p = self.pressed.take()?;
        if self.hit(r, fonts, t, s, x, y) != Some(p) {
            return None;
        }
        match p {
            Hit::Button(i) => Some(match (self.is_start(), i) {
                (true, 0) => Answer::Discard,
                (true, _) => Answer::Restore,
                (false, _) => Answer::Close,
            }),
            Hit::Tile(i) if !self.is_start() => Some(Answer::Open(i)),
            Hit::Tile(i) => {
                let double = self.last_click.is_some_and(|(at, j)| {
                    j == i && now.duration_since(at).as_millis() < DOUBLE_MS
                });
                self.focus = i;
                if double {
                    self.last_click = None;
                    Some(self.default_answer())
                } else {
                    self.last_click = Some((now, i));
                    None
                }
            }
        }
    }

    /// Bild samt Schatten; Lage links oben = `rect` minus Schatten.
    pub fn paint(&self, t: &Theme, fonts: &Fonts, s: f32) -> Canvas {
        let (w, h) = self.size(fonts, t, s);
        let m = (t.size.panel_shadow * s).round();
        let mut c = Canvas::new((w + 2.0 * m) as usize, (h + 2.0 * m) as usize);
        widgets::panel(&mut c, Rect::new(m, m, w, h), s, t);
        let u = &t.ui;
        let (regular, bold) = (fonts.regular.as_ref(), fonts.bold.as_ref());
        let cap = |px: f32| regular.map_or(px * 0.7, |f| f.cap_height(px));
        let x = m + PAD * s;
        // Titel und Text
        let px = t.size.font_title * s;
        let base = (m + 34.0 * s + cap(px) * 0.5).round();
        widgets::text(&mut c, bold.or(regular), &self.title, px, x, base, u.text);
        let px2 = t.size.font * s;
        let mut b2 = m + 62.0 * s + cap(px2) * 0.5;
        for l in self.wrap(fonts, s, &self.text, t.size.font) {
            widgets::text(&mut c, regular, &l, px2, x, b2.round(), u.text_dim);
            b2 += LINE * s;
        }
        // Kacheln
        let at = |r: &Rect| Rect::new(r.x + m, r.y + m, r.w, r.h);
        for (i, (tile, r)) in self
            .tiles
            .iter()
            .zip(self.tile_rects(fonts, t, s))
            .enumerate()
        {
            let chosen = self.focus == i;
            let hover = self.hover == Some(Hit::Tile(i));
            self.paint_tile(&mut c, t, fonts, s, at(&r), tile, chosen, hover);
        }
        // Knöpfe, „Enter = …“
        let rects = self.button_rects(fonts, t, s);
        let labels = self.labels();
        let main = match self.default_answer() {
            Answer::Discard => Some(0),
            Answer::Restore => Some(1),
            _ => None,
        };
        for (i, b) in rects.iter().enumerate() {
            let st = ButtonState {
                hover: self.hover == Some(Hit::Button(i)),
                pressed: self.pressed == Some(Hit::Button(i)) && self.hover == Some(Hit::Button(i)),
                active: main == Some(i),
                disabled: false,
            };
            widgets::button(&mut c, fonts, at(b), labels[i], st, s, t);
        }
        let small = t.size.font_small * s;
        let hint = match self.default_answer() {
            Answer::Restore => Some("Enter = Wiederherstellen"),
            Answer::Discard => Some("Enter = Verwerfen"),
            Answer::Open(_) => Some("Enter = Öffnen"),
            Answer::Close => None,
        };
        if let (Some(hint), Some(b)) = (hint, rects.first()) {
            let hw = regular.map_or(0.0, |f| f.width(hint, small));
            if PAD * s + hw + 12.0 * s <= b.x {
                let y = (m + b.y + (b.h + cap(small)) * 0.5).round();
                widgets::text(&mut c, regular, hint, small, x, y, u.text_dim);
            }
        }
        // Trennlinie und Fußzeile
        let sep = m + (self.layout(fonts, t, s).2 * s).round();
        c.fill_rect(
            m + 16.0 * s,
            sep,
            w - 32.0 * s,
            s.round().max(1.0),
            u.border,
        );
        let mut fb = sep + 22.0 * s + cap(small) * 0.5;
        for l in self.wrap(fonts, s, &self.footer, t.size.font_small) {
            widgets::text(&mut c, regular, &l, small, x, fb.round(), u.text_dim);
            fb += FOOT_LINE * s;
        }
        c
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_tile(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        s: f32,
        r: Rect,
        tile: &Tile,
        chosen: bool,
        hover: bool,
    ) {
        let u = &t.ui;
        let rad = t.size.corner_radius * s;
        let b = s.round().max(1.0);
        let (border, fill) = match (chosen, hover) {
            (true, _) => (u.accent, u.hover),
            (false, true) => (u.border, u.hover),
            (false, false) => (u.border, u.field_hover),
        };
        let mut p = Path::new();
        p.rounded_rect(r.x, r.y, r.w, r.h, rad);
        c.fill(&p, border);
        let mut p = Path::new();
        p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
        c.fill(&p, fill);
        // Papier mit dem Grundriss
        let pr = Rect::new(
            r.x + PAPER_INSET * s,
            r.y + PAPER_INSET * s,
            r.w - 2.0 * PAPER_INSET * s,
            PAPER_H * s,
        );
        let mut p = Path::new();
        p.rounded_rect(pr.x, pr.y, pr.w, pr.h, 6.0 * s);
        c.fill(&p, t.env.paper_fallback);
        paint_plan(c, &tile.plan, pr, s, u.sheet_text);
        // Name, Zeit, zweite Zeile
        let (regular, bold) = (fonts.regular.as_ref(), fonts.bold.as_ref());
        let cap = |px: f32| regular.map_or(px * 0.7, |f| f.cap_height(px));
        let px = t.size.font * s;
        let small = t.size.font_small * s;
        let y1 = (r.y + 110.0 * s + cap(px) * 0.5).round();
        let tx = r.x + 12.0 * s;
        let inner = r.w - 24.0 * s;
        let ww = regular.map_or(0.0, |f| f.width(&tile.when, px));
        let name = widgets::ellipsize(bold.or(regular), &tile.title, px, inner - ww - 12.0 * s);
        widgets::text(c, bold.or(regular), &name, px, tx, y1, u.text);
        widgets::text(
            c,
            regular,
            &tile.when,
            px,
            r.x + r.w - 12.0 * s - ww,
            y1,
            u.text,
        );
        let y2 = (r.y + 130.0 * s + cap(small) * 0.5).round();
        let sub = widgets::ellipsize(regular, &tile.sub, small, inner);
        let col = if tile.sub_accent {
            u.accent
        } else {
            u.text_dim
        };
        widgets::text(c, regular, &sub, small, tx, y2, col);
    }
}

/// Grundriss eingepasst in `r` (Rand 12 dip), Wände gefüllt.
fn paint_plan(c: &mut Canvas, plan: &Plan, r: Rect, s: f32, ink: Rgba) {
    let pts = plan.iter().flatten();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in pts {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    if x0 > x1 {
        return;
    }
    let pad = 12.0 * s as f64;
    let (bw, bh) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    let k = ((r.w as f64 - 2.0 * pad) / bw).min((r.h as f64 - 2.0 * pad) / bh);
    let ox = r.x as f64 + (r.w as f64 - bw * k) * 0.5;
    let oy = r.y as f64 + (r.h as f64 - bh * k) * 0.5;
    // Norden oben: y des Modells nach oben
    let map = |(x, y): (f64, f64)| ((ox + (x - x0) * k) as f32, (oy + (y1 - y) * k) as f32);
    for q in plan {
        let mut p = Path::new();
        let a = map(q[0]);
        p.move_to(a.0, a.1);
        for &v in &q[1..] {
            let b = map(v);
            p.line_to(b.0, b.1);
        }
        p.close();
        c.fill(&p, ink);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fonts() -> Fonts {
        Fonts {
            regular: None,
            bold: None,
            italic: None,
        }
    }

    fn found(original: Option<PathBuf>) -> Found {
        Found {
            backup: PathBuf::from("gibt-es-nicht.szo"),
            original,
        }
    }

    /// Enter stellt wieder her, Esc verwirft, ←/→ wechselt die Kachel und
    /// damit, was Enter tut.
    #[test]
    fn startkarte_tasten() {
        let mut c = BackupCard::start(found(None), SystemTime::now(), (2026, 10, 7, 3, 45));
        assert_eq!(c.tiles.len(), 1);
        assert_eq!(c.key(Key::Enter), Some(Answer::Restore));
        assert_eq!(c.key(Key::Escape), Some(Answer::Discard));
        assert_eq!(c.key(Key::Right), None);
        assert_eq!(c.focus, 0, "nur eine Kachel");
        c.tiles.push(c.tiles[0].clone());
        c.key(Key::Right);
        assert_eq!(c.key(Key::Enter), Some(Answer::Discard));
        c.key(Key::Left);
        assert_eq!(c.key(Key::Enter), Some(Answer::Restore));
    }

    /// Klick wählt eine Kachel, Doppelklick öffnet sie, die Knöpfe antworten.
    #[test]
    fn startkarte_maus() {
        let (t, f, s) = (Theme::dark(), fonts(), 1.0);
        let mut c = BackupCard::start(found(None), SystemTime::now(), (2026, 10, 7, 3, 45));
        c.tiles.push(c.tiles[0].clone());
        let r = c.rect(&f, &t, s, 1280, 800, 32);
        assert_eq!(r.w, W);
        let tiles = c.tile_rects(&f, &t, s);
        let (x, y) = (
            (r.x + tiles[1].x + 20.0) as f64,
            (r.y + tiles[1].y + 20.0) as f64,
        );
        let now = Instant::now();
        c.press(r, &f, &t, s, x, y);
        assert_eq!(c.release(r, &f, &t, s, x, y, now), None);
        assert_eq!(c.focus, 1);
        c.press(r, &f, &t, s, x, y);
        assert_eq!(c.release(r, &f, &t, s, x, y, now), Some(Answer::Discard));
        let b = c.button_rects(&f, &t, s);
        let (x, y) = ((r.x + b[1].x + 5.0) as f64, (r.y + b[1].y + 5.0) as f64);
        assert!(c.mouse_move(r, &f, &t, s, x, y));
        c.press(r, &f, &t, s, x, y);
        assert_eq!(c.release(r, &f, &t, s, x, y, now), Some(Answer::Restore));
        let img = c.paint(&t, &f, s);
        assert!(img.width as f32 > W);
    }

    /// Ohne gespeicherte Datei ist die Karte schmaler.
    #[test]
    fn unbenannt_schmal() {
        let (t, f) = (Theme::dark(), fonts());
        let c = BackupCard::start(found(None), SystemTime::now(), (2026, 10, 7, 3, 45));
        assert_eq!(c.rect(&f, &t, 1.0, 1280, 800, 32).w, W_NARROW);
        assert_eq!(c.title, "Sicherung von Unbenannt gefunden");
    }
}

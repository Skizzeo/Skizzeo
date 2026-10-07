//! Hilfe im Programm (Paket 9): der Text `hilfe.txt`, die Wahl des Themas
//! nach dem, was man gerade tut, die Hilfekarte (F1) und die Tooltips der
//! Werkzeugknöpfe. Rein bis auf das Bild der Karte: Tasten, Zeit und Lage
//! hinein, Zustand heraus; `main.rs` verdrahtet.

use crate::ui::ViewKind;
use sk_model::szo::Record;
use sk_paint::font::Font;
use sk_paint::{Canvas, Path};
use sk_platform::Key;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, Fonts, Rect};
use std::sync::OnceLock;

/// Der Hilfetext, fest im Programm.
pub const TEXT: &str = include_str!("../hilfe.txt");

/// F1.
pub const KEY_F1: Key = Key::Other(0x70);

/// Der Themenwechsel bei offener Karte wartet so lange (ms, Nachtrag H9-3).
pub const DEBOUNCE_MS: u64 = 150;

/// Ein Thema: Kennung, Titel, Zeilen (Aktion, Bedienung).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub rows: Vec<(String, String)>,
}

/// Gelesener Hilfetext: Themen, Knöpfe (Name, Satz) und Hinweise zu
/// Fehlern im Text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Help {
    pub sections: Vec<Section>,
    pub buttons: Vec<(String, String)>,
    pub errors: Vec<String>,
}

impl Help {
    pub fn section(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }
}

/// Liest den Text (Nachtrag H9-4): BOM und `\r` fallen weg, Kopfzeilen nach
/// den Regeln von `.szo`, Inhaltszeilen getrennt am ersten „|“. Fehler
/// ergeben Hinweise, nie einen Absturz.
pub fn parse(text: &str) -> Help {
    #[derive(PartialEq)]
    enum Part {
        None,
        Topic,
        Buttons,
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut h = Help::default();
    let mut part = Part::None;
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            part = Part::None;
            match Record::parse(n, line) {
                Ok(Some(r)) if r.section == "thema" => match (r.get("id"), r.get("titel")) {
                    (Ok(id), Ok(title)) if h.section(id).is_some() => {
                        h.errors.push(format!("Zeile {n}: Thema „{id}“ doppelt"));
                        let _ = title;
                    }
                    (Ok(id), Ok(title)) => {
                        h.sections.push(Section {
                            id: id.into(),
                            title: title.into(),
                            rows: Vec::new(),
                        });
                        part = Part::Topic;
                    }
                    (Err(e), _) | (_, Err(e)) => h.errors.push(e.to_string()),
                },
                Ok(Some(r)) if r.section == "knopf" => part = Part::Buttons,
                Ok(Some(r)) => h
                    .errors
                    .push(format!("Zeile {n}: unbekannter Abschnitt [{}]", r.section)),
                Ok(None) => {}
                Err(e) => h.errors.push(e.to_string()),
            }
            continue;
        }
        let Some((a, b)) = line.split_once('|') else {
            h.errors.push(format!("Zeile {n}: „|“ fehlt"));
            continue;
        };
        let row = (a.trim().to_string(), b.trim().to_string());
        if row.0.is_empty() || row.1.is_empty() {
            h.errors.push(format!("Zeile {n}: leere Spalte"));
            continue;
        }
        match part {
            Part::Topic => h.sections.last_mut().unwrap().rows.push(row),
            Part::Buttons => h.buttons.push(row),
            Part::None => h.errors.push(format!("Zeile {n}: Zeile ohne Thema")),
        }
    }
    for s in &h.sections {
        if !(1..=10).contains(&s.rows.len()) {
            h.errors.push(format!(
                "Thema „{}“: {} Zeilen (1 bis 10)",
                s.id,
                s.rows.len()
            ));
        }
    }
    h
}

/// Der Text des Programms, einmal gelesen.
pub fn help() -> &'static Help {
    static HELP: OnceLock<Help> = OnceLock::new();
    HELP.get_or_init(|| parse(TEXT))
}

/// Text ohne Fettschrift-Marken.
pub fn plain(text: &str) -> String {
    text.replace("**", "")
}

/// Zeilen des Tooltips am Knopf `name`: Name, Satz aus `[knopf]`,
/// „F1: mehr“ (9b).
pub fn tooltip_lines(name: &str) -> Vec<String> {
    let mut v = vec![name.to_string()];
    if let Some((_, satz)) = help().buttons.iter().find(|(n, _)| n == name) {
        v.push(satz.clone());
    }
    v.push("F1: mehr".into());
    v
}

/// Themen der Hilfe (§1.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Topic {
    #[default]
    Start,
    Navigation,
    Building,
    Draw,
    InnerWall,
    Drag,
    Upper,
    Flush,
    Wall,
    Foundation,
    Floor,
    Terrace,
    Levels,
    Section,
    Tree,
    File,
    Settings,
    SettingsPens,
    SettingsLineTypes,
    SettingsFills,
    SettingsSurfaces,
    SettingsUi,
    Catalog,
    Materials,
    Patterns,
    Quantities,
    Delete,
    Backups,
}

impl Topic {
    /// Alle Themen in der Reihenfolge der Liste „Alle Themen“.
    pub const ALL: [Topic; 28] = [
        Topic::Start,
        Topic::Navigation,
        Topic::Building,
        Topic::Draw,
        Topic::InnerWall,
        Topic::Drag,
        Topic::Upper,
        Topic::Flush,
        Topic::Wall,
        Topic::Foundation,
        Topic::Floor,
        Topic::Terrace,
        Topic::Levels,
        Topic::Section,
        Topic::Tree,
        Topic::File,
        Topic::Settings,
        Topic::SettingsPens,
        Topic::SettingsLineTypes,
        Topic::SettingsFills,
        Topic::SettingsSurfaces,
        Topic::SettingsUi,
        Topic::Catalog,
        Topic::Materials,
        Topic::Patterns,
        Topic::Quantities,
        Topic::Delete,
        Topic::Backups,
    ];

    /// Kennung im Hilfetext.
    pub fn id(self) -> &'static str {
        match self {
            Topic::Start => "start",
            Topic::Navigation => "navigation",
            Topic::Building => "gebaeude",
            Topic::Draw => "zeichnen",
            Topic::InnerWall => "innenwand",
            Topic::Drag => "ziehen",
            Topic::Upper => "og",
            Topic::Flush => "buendig",
            Topic::Wall => "wand",
            Topic::Foundation => "gruendung",
            Topic::Floor => "decke",
            Topic::Terrace => "dachterrasse",
            Topic::Levels => "geschosse",
            Topic::Section => "schnitt",
            Topic::Tree => "baum",
            Topic::File => "datei",
            Topic::Settings => "einstellungen",
            Topic::SettingsPens => "einstellungen-stifte",
            Topic::SettingsLineTypes => "einstellungen-linientypen",
            Topic::SettingsFills => "einstellungen-schraffuren",
            Topic::SettingsSurfaces => "einstellungen-oberflaechen",
            Topic::SettingsUi => "einstellungen-bedienoberflaeche",
            Topic::Catalog => "katalog",
            Topic::Materials => "baustoffe",
            Topic::Patterns => "muster",
            Topic::Quantities => "mengen",
            Topic::Delete => "loeschen",
            Topic::Backups => "sicherungen",
        }
    }

    pub fn from_id(id: &str) -> Option<Topic> {
        Topic::ALL.into_iter().find(|t| t.id() == id)
    }

    /// Titel aus dem Text; fehlt das Thema, die Kennung.
    pub fn title(self) -> &'static str {
        help()
            .section(self.id())
            .map_or(self.id(), |s| s.title.as_str())
    }

    /// Zeilen aus dem Text; fehlt das Thema, ein Hinweis statt Inhalt.
    fn rows(self) -> Vec<(String, String)> {
        match help().section(self.id()) {
            Some(s) => s.rows.clone(),
            None => vec![("Hilfe".into(), "Zu diesem Thema fehlt der Text.".into())],
        }
    }
}

/// Offenes Fenster für die Themenwahl.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// Einstellungen mit dem aktiven Reiter (0 Stifte … 4 Bedienoberfläche).
    Settings(u8),
    Catalog,
    Materials,
    Patterns,
    Backups,
}

/// Art des gewählten Bauteils.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelKind {
    Wall,
    UpperWall,
    Foundation,
    Floor,
    Terrace,
}

/// Lage der App, reine Daten (§3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HelpCtx {
    pub window: Option<Window>,
    /// Dialog „Gebäude erstellen“ offen.
    pub dialog: bool,
    /// Zielwahl „Bündig setzen“ läuft.
    pub flush_pick: bool,
    /// Ziehen läuft.
    pub dragging: bool,
    /// Werkzeug aktiv: `true` = Innenwand, `false` = Gebäude.
    pub tool: Option<bool>,
    pub isolating: bool,
    pub selection: Option<SelKind>,
    pub view: ViewKind,
}

/// Thema nach §1.2: die erste zutreffende Zeile gilt.
pub fn topic(c: &HelpCtx) -> Topic {
    if let Some(w) = c.window {
        return match w {
            Window::Settings(0) => Topic::SettingsPens,
            Window::Settings(1) => Topic::SettingsLineTypes,
            Window::Settings(2) => Topic::SettingsFills,
            Window::Settings(3) => Topic::SettingsSurfaces,
            Window::Settings(_) => Topic::SettingsUi,
            Window::Catalog => Topic::Catalog,
            Window::Materials => Topic::Materials,
            Window::Patterns => Topic::Patterns,
            Window::Backups => Topic::Backups,
        };
    }
    if c.dialog {
        return Topic::Building;
    }
    if c.flush_pick {
        return Topic::Flush;
    }
    if c.dragging {
        return Topic::Drag;
    }
    if let Some(inner) = c.tool {
        return if inner { Topic::InnerWall } else { Topic::Draw };
    }
    if c.isolating {
        return Topic::Tree;
    }
    if let Some(s) = c.selection {
        return match s {
            SelKind::Wall => Topic::Wall,
            SelKind::UpperWall => Topic::Upper,
            SelKind::Foundation => Topic::Foundation,
            SelKind::Floor => Topic::Floor,
            SelKind::Terrace => Topic::Terrace,
        };
    }
    match c.view {
        ViewKind::Section => Topic::Section,
        ViewKind::Plan => Topic::Levels,
        _ => Topic::Start,
    }
}

/// Grund der Karte mit Schatten je (Breite, Höhe, Skalierung, Schema).
pub type Ground = ((usize, usize, u32, u64), Canvas);

/// Zeile mit Stücken (Text, fett).
type RichLine = Vec<(String, bool)>;

/// Stelle in der Karte (Koordinaten relativ zur Karte, ohne Schatten).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Close,
    /// „Alle Themen ▸“
    All,
    /// „‹ Zurück“
    Back,
    /// Thema der aufgeklappten Liste.
    Item(Topic),
    /// Sonst in der Karte.
    Card,
}

/// Lage der Teile im zuletzt gezeichneten Bild (Pixel, relativ zur Karte).
#[derive(Clone, Debug, Default)]
struct Layout {
    w: f32,
    h: f32,
    close: Rect,
    all: Rect,
    back: Option<Rect>,
    body: Rect,
    /// Höhe des ganzen Inhalts (rollt, wenn höher als `body`).
    content: f32,
    items: Vec<(Topic, f32, f32)>,
}

/// Die Hilfekarte (§1.1): offen, Thema des Tuns (entprellt), gewähltes
/// Thema aus der Liste, Liste offen, Rollstand und Hover.
#[derive(Clone, Debug, Default)]
pub struct HelpCard {
    open: bool,
    doing: Topic,
    pending: Option<(Topic, u64)>,
    picked: Option<Topic>,
    list: bool,
    scroll: f32,
    hover: Option<Hit>,
    layout: Layout,
}

impl HelpCard {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Öffnen bzw. schließen; geöffnet mit dem Thema des Tuns.
    pub fn set_open(&mut self, open: bool) {
        if open && !self.open {
            if let Some((t, _)) = self.pending.take() {
                self.doing = t;
            }
            self.picked = None;
            self.list = false;
            self.scroll = 0.0;
        }
        self.open = open;
        self.hover = None;
    }

    /// Öffnet mit einem bestimmten Thema (Mengenfenster: „mengen“).
    pub fn open_with(&mut self, t: Topic) {
        self.set_open(true);
        self.picked = (t != self.doing).then_some(t);
    }

    /// Taste: F1 öffnet und schließt, Esc schließt nur eine offene Karte.
    /// `true`, wenn die Karte die Taste genommen hat.
    pub fn key(&mut self, key: Key) -> bool {
        match key {
            KEY_F1 => {
                self.set_open(!self.open);
                true
            }
            Key::Escape if self.open => {
                self.set_open(false);
                true
            }
            _ => false,
        }
    }

    /// Das Thema des Tuns ist `t` (Zeit in ms). Bei offener Karte gilt ein
    /// neues Thema erst nach [`DEBOUNCE_MS`].
    pub fn follow(&mut self, t: Topic, now: u64) {
        if !self.open {
            self.doing = t;
            self.pending = None;
        } else if t == self.doing {
            self.pending = None;
        } else if self.pending.map(|p| p.0) != Some(t) {
            self.pending = Some((t, now));
        }
    }

    /// Entprellten Wechsel übernehmen; `true`, wenn das gezeigte Thema
    /// wechselt.
    pub fn tick(&mut self, now: u64) -> bool {
        match self.pending {
            Some((t, t0)) if now >= t0 + DEBOUNCE_MS => {
                self.doing = t;
                self.pending = None;
                if self.picked.is_none() && !self.list {
                    self.scroll = 0.0;
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Wartezeit bis zum entprellten Wechsel (ms).
    pub fn wait(&self, now: u64) -> Option<u64> {
        self.pending
            .filter(|_| self.open)
            .map(|(_, t0)| (t0 + DEBOUNCE_MS).saturating_sub(now))
    }

    /// Gezeigtes Thema.
    pub fn topic(&self) -> Topic {
        self.picked.unwrap_or(self.doing)
    }

    pub fn toggle_list(&mut self) {
        self.list = !self.list;
        self.scroll = 0.0;
    }

    #[cfg(test)]
    pub fn list_open(&self) -> bool {
        self.list
    }

    /// Thema aus der Liste zeigen.
    pub fn pick(&mut self, t: Topic) {
        self.picked = Some(t);
        self.list = false;
        self.scroll = 0.0;
    }

    /// „‹ Zurück“: wieder das Thema des Tuns.
    pub fn back(&mut self) {
        self.picked = None;
        self.list = false;
        self.scroll = 0.0;
    }

    /// Schlüssel des Bildes: ändert er sich, ist neu zu zeichnen.
    pub fn key_of_image(&self) -> (Topic, bool, bool, u32, Option<Hit>) {
        (
            self.topic(),
            self.list,
            self.picked.is_some(),
            self.scroll.to_bits(),
            self.hover,
        )
    }

    /// Stelle unter `(x, y)` relativ zur Karte; `None` außerhalb.
    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        let l = &self.layout;
        if !self.open || x < 0.0 || y < 0.0 || x >= l.w || y >= l.h {
            return None;
        }
        let inside = |r: &Rect| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
        if inside(&l.close) {
            return Some(Hit::Close);
        }
        if inside(&l.all) {
            return Some(Hit::All);
        }
        if l.back.as_ref().is_some_and(inside) {
            return Some(Hit::Back);
        }
        if self.list && inside(&l.body) {
            let cy = y - l.body.y + self.scroll;
            if let Some((t, ..)) = l.items.iter().find(|(_, a, h)| cy >= *a && cy < a + h) {
                return Some(Hit::Item(*t));
            }
        }
        Some(Hit::Card)
    }

    /// Hover setzen; `true`, wenn neu zu zeichnen ist.
    pub fn set_hover(&mut self, h: Option<Hit>) -> bool {
        let h = h.filter(|h| *h != Hit::Card);
        let changed = h != self.hover;
        self.hover = h;
        changed
    }

    /// Klick an der Stelle `h`.
    pub fn click(&mut self, h: Hit) {
        match h {
            Hit::Close => self.set_open(false),
            Hit::All => self.toggle_list(),
            Hit::Back => self.back(),
            Hit::Item(t) => self.pick(t),
            Hit::Card => {}
        }
    }

    /// Mausrad über der Karte (Pixel nach unten); `true`, wenn sich der
    /// Rollstand ändert.
    pub fn scroll_by(&mut self, d: f32) -> bool {
        let max = (self.layout.content - self.layout.body.h).max(0.0);
        let s = (self.scroll + d).clamp(0.0, max);
        let changed = s != self.scroll;
        self.scroll = s;
        changed
    }

    /// Bild der Karte mit Schatten (Rand `margin` Pixel ringsum), höchstens
    /// `max_h` Pixel hoch ohne Schatten. `ground` hält den Grund je Größe.
    pub fn paint(
        &mut self,
        fonts: &Fonts,
        s: f32,
        t: &Theme,
        max_h: f32,
        ground: &mut Option<Ground>,
    ) -> (Canvas, f32) {
        let px = t.size.font_small * s;
        let reg = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(reg);
        let w = (360.0 * s).round();
        let pad = (12.0 * s).round();
        let head = (36.0 * s).round();
        let foot = (34.0 * s).round();
        let line = (18.0 * s).round();
        let gap = (6.0 * s).round();
        let left_w = (124.0 * s).round();
        let right_w = w - 2.0 * pad - left_w - (8.0 * s).round();
        let width =
            |f: Option<&Font>, x: &str| f.map_or(x.len() as f32 * px * 0.5, |f| f.width(x, px));

        // Inhalt: Zeilen des Themas bzw. die Liste
        let topic = self.topic();
        let mut rows: Vec<(Vec<String>, Vec<RichLine>)> = Vec::new();
        let mut items = Vec::new();
        let content = if self.list {
            let ih = (26.0 * s).round();
            for (i, tp) in Topic::ALL.iter().enumerate() {
                items.push((*tp, i as f32 * ih, ih));
            }
            Topic::ALL.len() as f32 * ih
        } else {
            let mut h = 0.0;
            for (a, b) in topic.rows() {
                let left = widgets::wrap(reg, &a, px, left_w - 4.0 * s);
                let lines = rich_wrap(reg, bold, &b, px, right_w);
                h += lines.len().max(left.len()).max(1) as f32 * line + gap;
                rows.push((left, lines));
            }
            (h - gap).max(0.0) + (10.0 * s).round()
        };
        let body_max = (max_h - head - foot).max(line * 3.0);
        let body_h = content.min(body_max).ceil();
        let h = head + body_h + foot;
        let max = (content - body_h).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);

        // Grund mit Schatten je Größe (wie 3k), dann nur noch kopieren
        let margin = (t.size.panel_shadow * s).ceil();
        let (cw, ch) = ((w + 2.0 * margin) as usize, (h + 2.0 * margin) as usize);
        let gk = (cw, ch, s.to_bits(), t.rev);
        if ground.as_ref().map(|g| g.0) != Some(gk) {
            let mut c = Canvas::new(cw, ch);
            widgets::panel(&mut c, Rect::new(margin, margin, w, h), s, t);
            *ground = Some((gk, c));
        }
        let mut c = ground.as_ref().unwrap().1.clone();
        let (ox, oy) = (margin, margin);

        // Kopf: „Hilfe · Titel“ und ✕
        let cap = reg.map_or(px * 0.7, |f| f.cap_height(px));
        let base = |top: f32, hh: f32| (top + (hh + cap) * 0.5).round();
        let close_sz = (24.0 * s).round();
        let close = Rect::new(
            w - pad - close_sz + (4.0 * s).round(),
            ((head - close_sz) * 0.5).round(),
            close_sz,
            close_sz,
        );
        let title = if self.list {
            "Hilfe · Alle Themen".to_string()
        } else {
            format!("Hilfe · {}", topic.title())
        };
        let title = widgets::ellipsize(bold, &title, px, close.x - pad - 4.0 * s);
        widgets::text(
            &mut c,
            bold,
            &title,
            px,
            ox + pad,
            oy + base(0.0, head),
            t.ui.text,
        );
        if self.hover == Some(Hit::Close) {
            let mut p = Path::new();
            p.rounded_rect(ox + close.x, oy + close.y, close.w, close.h, 4.0 * s);
            c.fill(&p, t.ui.hover);
        }
        let (cx, cy, d, sw) = (
            ox + close.x + close.w * 0.5,
            oy + close.y + close.h * 0.5,
            4.5 * s,
            (1.25 * s).max(1.0),
        );
        let mut p = Path::new();
        p.segment((cx - d, cy - d), (cx + d, cy + d), sw);
        p.segment((cx + d, cy - d), (cx - d, cy + d), sw);
        c.fill(&p, t.ui.text_dim);
        widgets::separator(
            &mut c,
            ox + pad,
            oy + head - s.max(1.0),
            w - 2.0 * pad,
            s,
            t,
        );

        // Inhalt, gerollt und auf den Inhaltsbereich beschnitten
        let body = Rect::new(0.0, head, w, body_h);
        let mut sub = Canvas::new(w as usize, body_h.max(1.0) as usize);
        sub.clear(t.ui.bg);
        let sy = -self.scroll;
        if self.list {
            for (tp, a, ih) in &items {
                let y = sy + a;
                if y + ih < 0.0 || y > body_h {
                    continue;
                }
                if self.hover == Some(Hit::Item(*tp)) {
                    sub.fill_rect((4.0 * s).round(), y, w - (8.0 * s).round(), *ih, t.ui.hover);
                }
                if *tp == self.doing {
                    let mut p = Path::new();
                    let r = 2.5 * s;
                    p.rounded_rect(pad - r * 0.5, y + ih * 0.5 - r, 2.0 * r, 2.0 * r, r);
                    sub.fill(&p, t.ui.accent);
                }
                let col = if *tp == topic {
                    t.ui.text
                } else {
                    t.ui.text_dim
                };
                widgets::text(
                    &mut sub,
                    reg,
                    tp.title(),
                    px,
                    pad + (10.0 * s).round(),
                    base(y, *ih),
                    col,
                );
            }
        } else {
            let mut y = sy + (4.0 * s).round();
            for (left, lines) in &rows {
                let n = lines.len().max(left.len()).max(1) as f32;
                if y + n * line >= 0.0 && y <= body_h {
                    for (k, a) in left.iter().enumerate() {
                        let yy = base(y + k as f32 * line, line);
                        widgets::text(&mut sub, reg, a, px, pad, yy, t.ui.text);
                    }
                    for (k, l) in lines.iter().enumerate() {
                        let mut x = pad + left_w + (8.0 * s).round();
                        let yy = base(y + k as f32 * line, line);
                        for (word, b) in l {
                            let (f, col) = if *b {
                                (bold, t.ui.text)
                            } else {
                                (reg, t.ui.text_dim)
                            };
                            widgets::text(&mut sub, f, word, px, x, yy, col);
                            x += width(f, word);
                        }
                    }
                }
                y += n * line + gap;
            }
        }
        c.blit(&sub, (ox + body.x) as i32, (oy + body.y) as i32);
        if max > 0.0 {
            let track = Rect::new(
                ox + w - (8.0 * s).round(),
                oy + body.y + 2.0 * s,
                (4.0 * s).round(),
                body_h - 4.0 * s,
            );
            let len = track.h * body_h / content;
            let start = track.y + (track.h - len) * self.scroll / max;
            let mut p = Path::new();
            p.rounded_rect(track.x, start, track.w, len, track.w * 0.5);
            c.fill(&p, t.ui.border);
        }

        // Fuß: „Alle Themen ▸“, „‹ Zurück“, „F1 schließt“
        let fy = head + body_h;
        widgets::separator(&mut c, ox + pad, oy + fy, w - 2.0 * pad, s, t);
        // Verweis im Fuß; `arrow`: Pfeil dahinter (zu bzw. offen)
        let link =
            |c: &mut Canvas, label: &str, x: f32, hover: bool, arrow: Option<bool>| -> Rect {
                let aw = if arrow.is_some() {
                    (14.0 * s).round()
                } else {
                    0.0
                };
                let lw = width(reg, label) + aw;
                let r = Rect::new(
                    x - (6.0 * s).round(),
                    fy + (5.0 * s).round(),
                    lw + (12.0 * s).round(),
                    foot - (10.0 * s).round(),
                );
                if hover {
                    let mut p = Path::new();
                    p.rounded_rect(ox + r.x, oy + r.y, r.w, r.h, 4.0 * s);
                    c.fill(&p, t.ui.hover);
                }
                widgets::text(c, reg, label, px, ox + x, oy + base(fy, foot), t.ui.accent);
                if let Some(open) = arrow {
                    let ax = ox + x + width(reg, label) + (8.0 * s).round();
                    let ay = oy + base(fy, foot) - cap * 0.5;
                    widgets::disclosure(c, ax, ay, open, t.ui.accent, s);
                }
                r
            };
        let all = link(
            &mut c,
            "Alle Themen",
            pad,
            self.hover == Some(Hit::All),
            Some(self.list),
        );
        let back = (self.picked.is_some() || self.list).then(|| {
            link(
                &mut c,
                "‹ Zurück",
                all.x + all.w + (14.0 * s).round(),
                self.hover == Some(Hit::Back),
                None,
            )
        });
        let hint = "F1 schließt";
        widgets::text(
            &mut c,
            reg,
            hint,
            px,
            ox + w - pad - width(reg, hint),
            oy + base(fy, foot),
            t.ui.text_dim,
        );

        self.layout = Layout {
            w,
            h,
            close,
            all,
            back,
            body,
            content,
            items,
        };
        (c, margin)
    }

    /// Größe der Karte im zuletzt gezeichneten Bild (ohne Schatten).
    pub fn size(&self) -> (f32, f32) {
        (self.layout.w, self.layout.h)
    }
}

/// Text mit `**fett**` in Zeilen von höchstens `max_w` Pixeln: je Zeile die
/// Stücke (Text, fett), Leerzeichen hängen am Stück davor.
fn rich_wrap(
    reg: Option<&Font>,
    bold: Option<&Font>,
    text: &str,
    px: f32,
    max_w: f32,
) -> Vec<RichLine> {
    let width = |b: bool, x: &str| {
        let f = if b { bold } else { reg };
        f.map_or(x.chars().count() as f32 * px * 0.5, |f| f.width(x, px))
    };
    // Wörter (getrennt an Leerzeichen) aus Stücken mit Fett-Kennzeichen:
    // „**Strg+Z**,“ ist ein Wort aus zwei Stücken
    let mut words: Vec<Vec<(String, bool)>> = vec![Vec::new()];
    for (i, part) in text.split("**").enumerate() {
        let b = i % 2 == 1;
        for (k, piece) in part.split(' ').enumerate() {
            if k > 0 && !words.last().unwrap().is_empty() {
                words.push(Vec::new());
            }
            if !piece.is_empty() {
                words.last_mut().unwrap().push((piece.to_string(), b));
            }
        }
    }
    words.retain(|w| !w.is_empty());
    let mut lines: Vec<RichLine> = Vec::new();
    let mut cur: RichLine = Vec::new();
    let mut x = 0.0;
    for word in words {
        let ww: f32 = word.iter().map(|(t, b)| width(*b, t)).sum();
        if !cur.is_empty() {
            // Leerzeichen in der Schrift des vorigen Stücks
            let last = cur.last().unwrap().1;
            let sp = width(last, " ");
            if x + sp + ww > max_w {
                lines.push(std::mem::take(&mut cur));
                x = 0.0;
            } else {
                cur.last_mut().unwrap().0.push(' ');
                x += sp;
            }
        }
        x += ww;
        cur.extend(word);
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_des_programms_ist_heil() {
        let h = help();
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        for t in Topic::ALL {
            assert!(h.section(t.id()).is_some(), "{}", t.id());
            assert_eq!(Topic::from_id(t.id()), Some(t));
        }
        assert_eq!(Topic::Draw.title(), "Wände zeichnen");
    }

    #[test]
    fn fett_und_umbruch() {
        let w = rich_wrap(None, None, "**Enter** oder **Tab** springt", 10.0, 1000.0);
        assert_eq!(
            w,
            [vec![
                ("Enter ".to_string(), true),
                ("oder ".to_string(), false),
                ("Tab ".to_string(), true),
                ("springt".to_string(), false)
            ]]
        );
        let w = rich_wrap(None, None, "**Strg+Z**, wiederholen", 10.0, 1000.0);
        assert_eq!(
            w,
            [vec![
                ("Strg+Z".to_string(), true),
                (", ".to_string(), false),
                ("wiederholen".to_string(), false)
            ]]
        );
        // schmal: ein Wort je Zeile
        let w = rich_wrap(None, None, "a b c", 10.0, 6.0);
        assert_eq!(w.len(), 3);
    }

    #[test]
    fn mengenfenster_oeffnet_mit_mengen() {
        let mut k = HelpCard::default();
        k.follow(Topic::Draw, 0);
        k.open_with(Topic::Quantities);
        assert!(k.is_open());
        assert_eq!(k.topic(), Topic::Quantities);
        k.back();
        assert_eq!(k.topic(), Topic::Draw);
        assert_eq!(k.wait(0), None);
        k.follow(Topic::Drag, 100);
        assert_eq!(k.wait(200), Some(50));
    }
}

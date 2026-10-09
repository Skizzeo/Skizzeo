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
    /// Nordpfeil aufziehen, drehen, verschieben (Sonnenstand S2).
    North,
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
    Verwaltung,
    Patterns,
    Quantities,
    Costs,
    Ava,
    Delete,
    Backups,
}

impl Topic {
    /// Alle Themen in der Reihenfolge der Liste „Alle Themen“.
    pub const ALL: [Topic; 32] = [
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
        Topic::North,
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
        Topic::Verwaltung,
        Topic::Patterns,
        Topic::Quantities,
        Topic::Costs,
        Topic::Ava,
        Topic::Delete,
        Topic::Backups,
    ];

    /// Gruppen der Liste „Alle Themen“ (Darstellung p9 §2, Liste).
    pub const GROUPS: [(&'static str, &'static [Topic]); 5] = [
        (
            "Grundlagen",
            &[Topic::Start, Topic::Navigation, Topic::File, Topic::Delete],
        ),
        (
            "Zeichnen",
            &[
                Topic::Building,
                Topic::Draw,
                Topic::InnerWall,
                Topic::Drag,
                Topic::Upper,
                Topic::Flush,
            ],
        ),
        (
            "Bauteile",
            &[Topic::Wall, Topic::Foundation, Topic::Floor, Topic::Terrace],
        ),
        (
            "Ansicht",
            &[
                Topic::Levels,
                Topic::Section,
                Topic::North,
                Topic::Tree,
                Topic::Quantities,
                Topic::Costs,
                Topic::Ava,
            ],
        ),
        (
            "Fenster",
            &[
                Topic::Settings,
                Topic::SettingsPens,
                Topic::SettingsLineTypes,
                Topic::SettingsFills,
                Topic::SettingsSurfaces,
                Topic::SettingsUi,
                Topic::Catalog,
                Topic::Materials,
                Topic::Verwaltung,
                Topic::Patterns,
                Topic::Backups,
            ],
        ),
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
            Topic::North => "nordpfeil",
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
            Topic::Verwaltung => "verwaltung",
            Topic::Patterns => "muster",
            Topic::Quantities => "mengen",
            Topic::Costs => "kosten",
            Topic::Ava => "ava",
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
    /// „Verwaltung …“ (KA-3a2); eigenes Thema mit KA-3a6.
    Verwaltung,
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
            Window::Verwaltung => Topic::Verwaltung,
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

/// Wie [`topic`]; beim Aufziehen, Drehen oder Verschieben des Nordpfeils
/// (`nord`) gilt „Nordpfeil“, außer ein Fenster oder der Dialog ist offen.
pub fn topic_mit_nord(c: &HelpCtx, nord: bool) -> Topic {
    if nord && c.window.is_none() && !c.dialog {
        Topic::North
    } else {
        topic(c)
    }
}

/// Grund der Karte mit Schatten je (Breite, Höhe, Skalierung, Schema).
pub type Ground = ((usize, usize, u32, u64), Canvas);

/// Schlüssel des Kartenbilds: gezeigtes Thema, Liste offen, gewählt,
/// Rollstand, Hover, Thema des Tuns (für „Zurück zu …“ und „gerade“).
pub type ImageKey = (Topic, bool, bool, u32, Option<Hit>, Topic);

/// Zeile mit Stücken (Text, fett).
type RichLine = Vec<(String, bool)>;

/// Stelle in der Karte (Koordinaten relativ zur Karte, ohne Schatten).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Close,
    /// „Alle Themen ▸“
    All,
    /// „‹ Zurück zu „…““
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
    /// „Alle Themen ▸“ bzw. „‹ Zurück zu „…““ (es steht immer nur eins)
    all: Option<Rect>,
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
    /// Bild und Inhaltsbild des letzten Malens: behalten den Speicher
    /// (Review 3w, wie 3k: frische Seiten kosten mehr als das Malen).
    bufs: Option<(Canvas, Canvas)>,
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
    pub fn key_of_image(&self) -> ImageKey {
        (
            self.topic(),
            self.list,
            self.picked.is_some(),
            self.scroll.to_bits(),
            self.hover,
            self.doing,
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
        if l.all.as_ref().is_some_and(inside) {
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
    /// Maße nach Darstellung p9 §2: Breite 360, Kopf 44, Fuß 36, Spalte
    /// 118, Zeile 16, 7 zwischen den Einträgen, Listenzeile 24 dip.
    pub fn paint(
        &mut self,
        fonts: &Fonts,
        s: f32,
        t: &Theme,
        max_h: f32,
        ground: &mut Option<Ground>,
    ) -> (&Canvas, f32) {
        let px = t.size.font_small * s;
        let px_head = t.size.font * s;
        let px_group = t.size.font_detail * s;
        let reg = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(reg);
        let w = (360.0 * s).round();
        let pad = (12.0 * s).round();
        let head = (44.0 * s).round();
        let foot = (36.0 * s).round();
        let line = (16.0 * s).round();
        let gap = (7.0 * s).round();
        let left_w = (118.0 * s).round();
        let right_w = w - 2.0 * pad - left_w - (8.0 * s).round();
        let width_at = |f: Option<&Font>, x: &str, px: f32| {
            f.map_or(x.chars().count() as f32 * px * 0.5, |f| f.width(x, px))
        };
        let width = |f: Option<&Font>, x: &str| width_at(f, x, px);
        let cap_at = |px: f32| reg.map_or(px * 0.7, |f| f.cap_height(px));
        let cap = cap_at(px);
        let base = |top: f32, hh: f32| (top + (hh + cap) * 0.5).round();

        // Inhalt: Zeilen des Themas bzw. die Liste in Gruppen
        let topic = self.topic();
        let mut rows: Vec<(Vec<String>, Vec<RichLine>)> = Vec::new();
        let mut items = Vec::new();
        let mut groups = Vec::new();
        let ih = (24.0 * s).round();
        let gh = (26.0 * s).round();
        let content = if self.list {
            let mut y = 0.0;
            for (name, topics) in Topic::GROUPS {
                groups.push((name, y));
                y += gh;
                for tp in topics {
                    items.push((*tp, y, ih));
                    y += ih;
                }
                y += (4.0 * s).round();
            }
            y
        } else {
            let mut h = 0.0;
            for (a, b) in topic.rows() {
                let left = widgets::wrap(reg, &a, px, left_w - 4.0 * s);
                let lines = rich_wrap(reg, bold, &b, px, right_w);
                h += lines.len().max(left.len()).max(1) as f32 * line + gap;
                rows.push((left, lines));
            }
            (h - gap).max(0.0) + (12.0 * s).round()
        };
        let body_max = (max_h - head - foot).max(line * 3.0).floor();
        // Die Liste nimmt immer die volle Höhe (die Karte springt nicht)
        let body_h = if self.list {
            body_max
        } else {
            content.min(body_max).ceil()
        };
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
        let mut bufs = self
            .bufs
            .take()
            .unwrap_or_else(|| (Canvas::new(0, 0), Canvas::new(0, 0)));
        let (c, sub) = (&mut bufs.0, &mut bufs.1);
        c.copy_from(&ground.as_ref().unwrap().1);
        let (ox, oy) = (margin, margin);

        // Kopf: Akzentpunkt, „Hilfe · “ gedämpft, Titel fett, ×
        let close_sz = (24.0 * s).round();
        let close = Rect::new(
            w - pad - close_sz + (4.0 * s).round(),
            ((head - close_sz) * 0.5).round(),
            close_sz,
            close_sz,
        );
        let hy = oy + (0.5 * (head + cap_at(px_head))).round();
        let dot = (8.0 * s).round();
        let mut p = Path::new();
        p.rounded_rect(
            ox + pad,
            oy + ((head - dot) * 0.5).round(),
            dot,
            dot,
            dot * 0.5,
        );
        c.fill(&p, t.ui.accent);
        let lead = "Hilfe · ";
        let lx = ox + pad + dot + (8.0 * s).round();
        widgets::text(c, reg, lead, px_head, lx, hy, t.ui.text_dim);
        let tx = lx + width_at(reg, lead, px_head);
        let title = if self.list {
            "Alle Themen"
        } else {
            topic.title()
        };
        let title = widgets::ellipsize(bold, title, px_head, ox + close.x - tx - 4.0 * s);
        widgets::text(c, bold, &title, px_head, tx, hy, t.ui.text);
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
        widgets::separator(c, ox + pad, oy + head - s.max(1.0), w - 2.0 * pad, s, t);

        // Inhalt, gerollt und auf den Inhaltsbereich beschnitten
        let body = Rect::new(0.0, head, w, body_h);
        sub.reuse(w as usize, body_h.max(1.0) as usize);
        sub.clear(t.ui.bg);
        let sy = -self.scroll;
        if self.list {
            let inset = (4.0 * s).round();
            for (name, a) in &groups {
                let y = sy + a;
                if y + gh < 0.0 || y > body_h {
                    continue;
                }
                // Kapitälchen: Großbuchstaben, fett, klein, gedämpft
                let gy = (y + gh - (8.0 * s).round()).round();
                widgets::text(
                    sub,
                    bold,
                    &name.to_uppercase(),
                    px_group,
                    pad,
                    gy,
                    t.ui.text_dim,
                );
            }
            let now = "gerade";
            for (tp, a, ih) in &items {
                let y = sy + a;
                if y + ih < 0.0 || y > body_h {
                    continue;
                }
                let current = *tp == self.doing;
                let row_w = w - 2.0 * inset;
                if current {
                    sub.fill_rect(inset, y, row_w, *ih, t.ui.pressed);
                    sub.fill_rect(inset, y, (3.0 * s).round(), *ih, t.ui.accent);
                } else if self.hover == Some(Hit::Item(*tp)) {
                    sub.fill_rect(inset, y, row_w, *ih, t.ui.hover);
                }
                if current && self.hover == Some(Hit::Item(*tp)) {
                    sub.fill_rect(
                        inset + (3.0 * s).round(),
                        y,
                        row_w - (3.0 * s).round(),
                        *ih,
                        t.ui.hover,
                    );
                }
                let nw = if current { width(reg, now) } else { 0.0 };
                let x = pad + (6.0 * s).round();
                let label =
                    widgets::ellipsize(reg, tp.title(), px, w - x - pad - nw - (12.0 * s).round());
                widgets::text(sub, reg, &label, px, x, base(y, *ih), t.ui.text);
                if current {
                    widgets::text(
                        sub,
                        reg,
                        now,
                        px,
                        w - pad - (6.0 * s).round() - nw,
                        base(y, *ih),
                        t.ui.text_dim,
                    );
                }
            }
        } else {
            let mut y = sy + (6.0 * s).round();
            for (left, lines) in &rows {
                let n = lines.len().max(left.len()).max(1) as f32;
                if y + n * line >= 0.0 && y <= body_h {
                    for (k, a) in left.iter().enumerate() {
                        let yy = base(y + k as f32 * line, line);
                        widgets::text(sub, reg, a, px, pad, yy, t.ui.text);
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
                            widgets::text(sub, f, word, px, x, yy, col);
                            x += width(f, word);
                        }
                    }
                }
                y += n * line + gap;
            }
        }
        c.blit(sub, (ox + body.x) as i32, (oy + body.y) as i32);
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

        // Fuß: „Alle Themen ▸“ bzw. „‹ Zurück zu „…““, „F1 schließt“
        let fy = head + body_h;
        widgets::separator(c, ox + pad, oy + fy, w - 2.0 * pad, s, t);
        let hint = "F1 schließt";
        let hint_w = width(reg, hint);
        widgets::text(
            c,
            reg,
            hint,
            px,
            ox + w - pad - hint_w,
            oy + base(fy, foot),
            t.ui.text_dim,
        );
        // Verweis im Fuß, fett; `arrow`: Pfeil dahinter (zu bzw. offen)
        let link = |c: &mut Canvas, label: &str, hover: bool, arrow: Option<bool>| -> Rect {
            let aw = if arrow.is_some() {
                (14.0 * s).round()
            } else {
                0.0
            };
            let x = pad;
            let label = widgets::ellipsize(
                bold,
                label,
                px,
                w - 2.0 * pad - hint_w - aw - (24.0 * s).round(),
            );
            let lw = width(bold, &label) + aw;
            let r = Rect::new(
                x - (6.0 * s).round(),
                fy + (6.0 * s).round(),
                lw + (12.0 * s).round(),
                foot - (12.0 * s).round(),
            );
            if hover {
                let mut p = Path::new();
                p.rounded_rect(ox + r.x, oy + r.y, r.w, r.h, 4.0 * s);
                c.fill(&p, t.ui.hover);
            }
            widgets::text(
                c,
                bold,
                &label,
                px,
                ox + x,
                oy + base(fy, foot),
                t.ui.accent,
            );
            if let Some(open) = arrow {
                let ax = ox + x + width(bold, &label) + (8.0 * s).round();
                let ay = oy + base(fy, foot) - cap * 0.5;
                widgets::disclosure(c, ax, ay, open, t.ui.accent, s);
            }
            r
        };
        let (all, back) = if self.picked.is_some() || self.list {
            let label = format!("‹ Zurück zu „{}“", self.doing.title());
            let r = link(c, &label, self.hover == Some(Hit::Back), None);
            (None, Some(r))
        } else {
            let r = link(c, "Alle Themen", self.hover == Some(Hit::All), Some(false));
            (Some(r), None)
        };

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
        let c = &self.bufs.insert(bufs).0;
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

    /// Hinweis W (S2): Beim Nordpfeil zeigt F1 „Nordpfeil“, mit offenem
    /// Fenster oder Dialog weiter deren Thema.
    #[test]
    fn thema_nordpfeil() {
        let mut c = HelpCtx {
            window: None,
            dialog: false,
            flush_pick: false,
            dragging: false,
            tool: None,
            isolating: false,
            selection: None,
            view: ViewKind::Plan,
        };
        assert_eq!(topic_mit_nord(&c, true), Topic::North);
        assert_eq!(topic_mit_nord(&c, false), Topic::Levels);
        c.window = Some(Window::Catalog);
        assert_eq!(topic_mit_nord(&c, true), Topic::Catalog);
        let s = help().section("nordpfeil").expect("Thema Nordpfeil");
        for a in [
            "Setzen",
            "Genau",
            "Drehen",
            "Verschieben",
            "Platz",
            "Abbrechen",
        ] {
            assert!(s.rows.iter().any(|(x, _)| x == a), "{a}");
        }
    }

    /// KA-2d (paket-ka2 §7): Thema „Kosten“ mit Verrechnungslohn, „Auch für
    /// neue Häuser“ und „Bauleistung wählen …“; „Mengen und Kosten“ mit der
    /// Zeile „Gebäude“ und ohne die alte Zeile „Preise“.
    #[test]
    fn hilfe_kosten_und_mengen() {
        let h = help();
        let k = h.section("kosten").expect("Thema Kosten");
        assert_eq!(k.title, "Kosten");
        let zeile =
            |s: &Section, a: &str| s.rows.iter().find(|(x, _)| x == a).map(|(_, b)| b.clone());
        for a in [
            "Verrechnungslohn",
            "Preis ändern",
            "Für neue Häuser",
            "Andere Werte",
            "Grau",
        ] {
            assert!(zeile(k, a).is_some(), "{a}");
        }
        assert!(zeile(k, "Grau").unwrap().contains("„Bauleistung wählen …“"));
        let m = h.section("mengen").unwrap();
        assert_eq!(m.title, "Mengen, Kosten, AVA");
        assert!(zeile(m, "Gebäude").unwrap().contains("ganze Projekt"));
        assert_eq!(zeile(m, "Preise"), None);
        assert!(m.rows.len() <= 10);
        assert_eq!(Topic::from_id("kosten"), Some(Topic::Costs));
    }

    /// KA-4d (paket-ka4 §7): Thema „AVA · Leistungsverzeichnis“ mit den
    /// Zeilen und der Zeile zur OZ in der Tabelle (Kosten-Nachprüfung 16:05); F1 im Blatt AVA.
    #[test]
    fn hilfe_ava() {
        let h = help();
        let a = h.section("ava").expect("Thema AVA");
        assert_eq!(a.title, "AVA · Leistungsverzeichnis");
        let namen: Vec<&str> = a.rows.iter().map(|(x, _)| x.as_str()).collect();
        assert_eq!(
            namen,
            [
                "Los wählen",
                "Anfrage",
                "Position",
                "Kopf",
                "Prüfen",
                "Geschosse getrennt",
                "Ausgabe",
                "Drucken",
                "OZ in der Tabelle"
            ]
        );
        // Bedienbarkeit 26.3: Druckvorschau und PDF stehen in der Hilfe
        let d = &a.rows.iter().find(|(x, _)| x == "Drucken").unwrap().1;
        assert!(d.contains("„Druckvorschau“") && d.contains("als PDF speichern"));
        assert_eq!(Topic::from_id("ava"), Some(Topic::Ava));
    }

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

    /// Review 3w: Das Malen behält Bild und Inhaltsbild. Nach Thema, Liste,
    /// Hover und anderer Höhe gleicht das Bild dem einer frischen Karte.
    #[test]
    fn behaltener_speicher_malt_gleich() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let mut ground = None;
        let mut k = HelpCard::default();
        k.follow(Topic::Start, 0);
        k.set_open(true);
        k.paint(&fonts, 1.5, &t, 900.0, &mut ground);
        k.toggle_list();
        k.set_hover(Some(Hit::Close));
        k.paint(&fonts, 1.5, &t, 900.0, &mut ground);
        k.pick(Topic::Draw);
        k.set_hover(None);
        let a = k
            .paint(&fonts, 1.5, &t, 400.0, &mut ground)
            .0
            .to_premul_rgba8();
        let mut f = HelpCard::default();
        f.follow(Topic::Start, 0);
        f.set_open(true);
        f.pick(Topic::Draw);
        let b = f
            .paint(&fonts, 1.5, &t, 400.0, &mut None)
            .0
            .to_premul_rgba8();
        assert!(a == b, "Bild mit behaltenem Speicher weicht ab");
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

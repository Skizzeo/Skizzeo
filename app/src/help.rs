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
    /// Werkzeug für Erweiterungsbauteile (E6).
    Ext,
    /// Wand- und Türöffnungen (Paket O3).
    Openings,
    /// Räume und Raumstempel (Grundriss G3, G4; Bedienbarkeit 39.5).
    Rooms,
    /// Rückfrage „Bauteil einlesen“ (E9).
    Einlesen,
    Drag,
    /// Wand teilen und an der Fuge vereinen (W-11, W-12).
    Split,
    Upper,
    Flush,
    Wall,
    Foundation,
    Floor,
    Terrace,
    /// Flachdach mit Aufkantung, Gefälle und Abläufen (Bedienbarkeit 34.3).
    FlatRoof,
    Levels,
    Section,
    /// Maßketten in Grundriss und 3D, Zahnrad und Reiter „Bemaßung“.
    Bemassung,
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
    /// Firmenvorgaben des Arbeitsplatzes (Sonnenstand S8).
    SettingsVorgaben,
    Catalog,
    Materials,
    Verwaltung,
    Patterns,
    Quantities,
    Costs,
    Ava,
    Delete,
    Backups,
    /// Ausbaustufen und was jede kann, „Ausbaustufe …“ im Dateimenü
    /// (Bedienbarkeit 41.5).
    Ausbaustufen,
    /// Gelände: Platte, Lageplan, Höhen, Lage, Höhenlage, Geländeflächen,
    /// Erdmengen (Bedienbarkeit 43.2, 42.4).
    Gelaende,
}

impl Topic {
    /// Alle Themen in der Reihenfolge der Liste „Alle Themen“.
    pub const ALL: [Topic; 42] = [
        Topic::Start,
        Topic::Navigation,
        Topic::Building,
        Topic::Draw,
        Topic::InnerWall,
        Topic::Ext,
        Topic::Openings,
        Topic::Rooms,
        Topic::Einlesen,
        Topic::Drag,
        Topic::Split,
        Topic::Upper,
        Topic::Flush,
        Topic::Wall,
        Topic::Foundation,
        Topic::Floor,
        Topic::Terrace,
        Topic::FlatRoof,
        Topic::Levels,
        Topic::Section,
        Topic::Bemassung,
        Topic::North,
        Topic::Tree,
        Topic::File,
        Topic::Settings,
        Topic::SettingsPens,
        Topic::SettingsLineTypes,
        Topic::SettingsFills,
        Topic::SettingsSurfaces,
        Topic::SettingsUi,
        Topic::SettingsVorgaben,
        Topic::Catalog,
        Topic::Materials,
        Topic::Verwaltung,
        Topic::Patterns,
        Topic::Quantities,
        Topic::Costs,
        Topic::Ava,
        Topic::Delete,
        Topic::Backups,
        Topic::Ausbaustufen,
        Topic::Gelaende,
    ];

    /// Gruppen der Liste „Alle Themen“ (Darstellung p9 §2, Liste).
    pub const GROUPS: [(&'static str, &'static [Topic]); 5] = [
        (
            "Grundlagen",
            &[
                Topic::Start,
                Topic::Navigation,
                Topic::File,
                Topic::Delete,
                Topic::Ausbaustufen,
            ],
        ),
        (
            "Zeichnen",
            &[
                Topic::Building,
                Topic::Draw,
                Topic::InnerWall,
                Topic::Ext,
                Topic::Openings,
                Topic::Rooms,
                Topic::Einlesen,
                Topic::Drag,
                Topic::Split,
                Topic::Upper,
                Topic::Flush,
                Topic::Gelaende,
            ],
        ),
        (
            "Bauteile",
            &[
                Topic::Wall,
                Topic::Foundation,
                Topic::Floor,
                Topic::Terrace,
                Topic::FlatRoof,
            ],
        ),
        (
            "Ansicht",
            &[
                Topic::Levels,
                Topic::Section,
                Topic::Bemassung,
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
                Topic::SettingsVorgaben,
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
            Topic::Ext => "erweiterungen",
            Topic::Openings => "oeffnungen",
            Topic::Rooms => "raeume",
            Topic::Einlesen => "einlesen",
            Topic::Drag => "ziehen",
            Topic::Split => "teilen",
            Topic::Upper => "og",
            Topic::Flush => "buendig",
            Topic::Wall => "wand",
            Topic::Foundation => "gruendung",
            Topic::Floor => "decke",
            Topic::Terrace => "dachterrasse",
            Topic::FlatRoof => "flachdach",
            Topic::Levels => "geschosse",
            Topic::Section => "schnitt",
            Topic::Bemassung => "bemassung",
            Topic::North => "nordpfeil",
            Topic::Tree => "baum",
            Topic::File => "datei",
            Topic::Settings => "einstellungen",
            Topic::SettingsPens => "einstellungen-stifte",
            Topic::SettingsLineTypes => "einstellungen-linientypen",
            Topic::SettingsFills => "einstellungen-schraffuren",
            Topic::SettingsSurfaces => "einstellungen-oberflaechen",
            Topic::SettingsUi => "einstellungen-bedienoberflaeche",
            Topic::SettingsVorgaben => "einstellungen-vorgaben",
            Topic::Catalog => "katalog",
            Topic::Materials => "baustoffe",
            Topic::Verwaltung => "verwaltung",
            Topic::Patterns => "muster",
            Topic::Quantities => "mengen",
            Topic::Costs => "kosten",
            Topic::Ava => "ava",
            Topic::Delete => "loeschen",
            Topic::Backups => "sicherungen",
            Topic::Ausbaustufen => "ausbaustufen",
            Topic::Gelaende => "gelaende",
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

/// Erste Zeile im Thema „Ausbaustufen“, rechts die Stufe dieser Installation.
pub const STUFE_HIER: &str = "Diese Installation";

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
    /// Fenster oder Tür (Bedienbarkeit 38.4).
    Opening,
    /// Raum (Bedienbarkeit 39.5).
    Space,
}

/// Art des Bauteils `id` für die Themenwahl; `None` für Erweiterungen
/// (Thema „Erweiterungen“, siehe `App::sync_help`) und Unbekanntes.
pub fn sel_kind(m: &sk_model::Model, id: sk_model::ElementId) -> Option<SelKind> {
    use sk_model::ElementKind as K;
    Some(match m.element(id)?.kind {
        K::Wall(_) if m.stack_offset(id).is_some() => SelKind::UpperWall,
        K::Wall(_) => SelKind::Wall,
        K::GroundSlab(_) | K::StripFooting(_) | K::PerimeterInsulation { .. } => {
            SelKind::Foundation
        }
        K::Floor(_) | K::EdgeStrip { .. } | K::SoffitInsulation { .. } => SelKind::Floor,
        K::RoofTerrace { .. } | K::Coping { .. } | K::Roof { .. } => SelKind::Terrace,
        K::Opening(_) => SelKind::Opening,
        K::Space(_) => SelKind::Space,
        K::Ext(_) | K::Downpipe { .. } => return None,
    })
}

/// Lage der App, reine Daten (§3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HelpCtx {
    pub window: Option<Window>,
    /// Dialog „Gebäude erstellen“ offen.
    pub dialog: bool,
    /// Zielwahl „Bündig setzen“ läuft.
    pub flush_pick: bool,
    /// Ziehen läuft, eine Eingabe ist offen, oder das Tun an der Wand hat
    /// ein eigenes Thema (Teilen, Ecke, Bemaßung unter der Maus).
    pub dragging: bool,
    /// Werkzeug aktiv: `true` = Innenwand, `false` = Gebäude.
    pub tool: Option<bool>,
    /// Werkzeug „Fenster“ oder „Tür“ aktiv.
    pub opening: bool,
    pub isolating: bool,
    /// Werkzeug „Fläche zeichnen“, Geländefläche oder Geländeplatte gewählt,
    /// oder Leiste „Gelände“ offen ohne gewähltes Bauteil (Bedienbarkeit
    /// 43.2, [`gelaende_gilt`]).
    pub gelaende: bool,
    pub selection: Option<SelKind>,
    pub view: ViewKind,
}

/// F1 zeigt „Gelände“ (Review #192 H-192b): beim Werkzeug und bei gewählter
/// Fläche oder Platte immer, bei offener Leiste nur, solange kein Bauteil
/// gewählt ist; sonst gilt das Thema des Bauteils.
pub fn gelaende_gilt(leiste_offen: bool, eigenes: bool, bauteil: bool) -> bool {
    eigenes || (leiste_offen && !bauteil)
}

/// Thema nach §1.2: die erste zutreffende Zeile gilt.
pub fn topic(c: &HelpCtx) -> Topic {
    if let Some(w) = c.window {
        return match w {
            Window::Settings(0) => Topic::SettingsPens,
            Window::Settings(1) => Topic::SettingsLineTypes,
            Window::Settings(2) => Topic::SettingsFills,
            Window::Settings(3) => Topic::SettingsSurfaces,
            Window::Settings(5) => Topic::SettingsVorgaben,
            Window::Settings(6) => Topic::Bemassung,
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
    if c.opening {
        return Topic::Openings;
    }
    if let Some(inner) = c.tool {
        return if inner { Topic::InnerWall } else { Topic::Draw };
    }
    if c.gelaende {
        return Topic::Gelaende;
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
            SelKind::Opening => Topic::Openings,
            SelKind::Space => Topic::Rooms,
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
    /// Freie Module: die Liste „Alle Themen“ zeigt nur ihre Themen.
    pub frei: crate::freischaltung::Freischaltung,
}

impl HelpCard {
    /// Titel des Themas; das Mengenfenster heißt nach den freien Blättern
    /// (Review #120 H5).
    fn titel(&self, t: Topic) -> &'static str {
        match (t, self.frei.blaetter()) {
            (Topic::Quantities, 1 | 2) => "Mengen",
            (Topic::Quantities, 3) => "Mengen, Kosten",
            _ => t.title(),
        }
    }

    /// Zeilen des Themas ohne die gesperrter Module
    /// ([`crate::freischaltung::hilfe_zeigt`]); der Knopf zum Mengenfenster
    /// heißt wie im Paneel. „Ausbaustufen“ nennt zuerst die Stufe dieser
    /// Installation.
    fn zeilen(&self, t: Topic) -> Vec<(String, String)> {
        let knopf = self.frei.knopf();
        let stufe = (t == Topic::Ausbaustufen).then(|| (STUFE_HIER.into(), self.frei.name()));
        stufe
            .into_iter()
            .chain(t.rows())
            .filter(|(a, _)| crate::freischaltung::hilfe_zeigt(self.frei, t, a))
            .map(|(a, b)| (a, b.replace(crate::cards::KNOPF, knopf)))
            .collect()
    }

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
        // auch auf der schon offenen Karte das Thema von oben, nicht die
        // Liste „Alle Themen“ (Review #156 B1)
        self.list = false;
        self.scroll = 0.0;
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
                for tp in topics.iter().filter(|t| !self.frei.gesperrt(**t)) {
                    items.push((*tp, y, ih));
                    y += ih;
                }
                y += (4.0 * s).round();
            }
            y
        } else {
            let mut h = 0.0;
            for (a, b) in self.zeilen(topic) {
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
            self.titel(topic)
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
                let label = widgets::ellipsize(
                    reg,
                    self.titel(*tp),
                    px,
                    w - x - pad - nw - (12.0 * s).round(),
                );
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
            let label = format!("‹ Zurück zu „{}“", self.titel(self.doing));
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
            opening: false,
            isolating: false,
            gelaende: false,
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

    /// Bedienbarkeit 38.4: Im Werkzeug „Fenster“/„Tür“ und mit einer
    /// gewählten Öffnung zeigt F1 „Wand- und Türöffnungen“; das Thema nennt
    /// den Grundriss, Ändern und Löschen in höchstens zehn Zeilen.
    #[test]
    fn thema_oeffnungen() {
        let mut c = HelpCtx {
            window: None,
            dialog: false,
            flush_pick: false,
            dragging: false,
            tool: None,
            opening: true,
            isolating: false,
            gelaende: false,
            selection: None,
            view: ViewKind::Plan,
        };
        assert_eq!(topic(&c), Topic::Openings);
        c.opening = false;
        c.selection = Some(SelKind::Opening);
        assert_eq!(topic(&c), Topic::Openings);
        c.window = Some(Window::Catalog);
        assert_eq!(topic(&c), Topic::Catalog);
        let s = help().section("oeffnungen").expect("Thema Öffnungen");
        for a in ["Im Grundriss", "Ändern, Löschen", "Höhenlinien"] {
            assert!(s.rows.iter().any(|(x, _)| x == a), "{a}");
        }
        assert!(s.rows.len() <= 10);
    }

    /// Bedienbarkeit 43.2: Bei offener Leiste „Gelände“ oder gewählter
    /// Geländefläche zeigt F1 „Gelände“; ohne Modul nur Platte und Weg zum
    /// Modul, mit Modul der Ablauf ohne den Weg dorthin.
    #[test]
    fn thema_gelaende() {
        use crate::freischaltung::{Freischaltung, Modul};
        let mut c = HelpCtx {
            window: None,
            dialog: false,
            flush_pick: false,
            dragging: false,
            tool: None,
            opening: false,
            isolating: true,
            gelaende: true,
            selection: Some(SelKind::Wall),
            view: ViewKind::Plan,
        };
        assert_eq!(topic(&c), Topic::Gelaende);
        c.tool = Some(false);
        assert_eq!(topic(&c), Topic::Draw);
        // Leiste offen und Wand gewählt: „Wand“, Fläche gewählt: „Gelände“
        assert!(!gelaende_gilt(true, false, true));
        assert!(gelaende_gilt(true, false, false));
        assert!(gelaende_gilt(false, true, true));
        assert!(!gelaende_gilt(false, false, false));
        assert!(Topic::GROUPS
            .iter()
            .any(|(_, t)| t.contains(&Topic::Gelaende)));
        let s = help().section("gelaende").expect("Thema Gelände");
        assert!(s.rows.len() <= 10);
        let zeilen = |f: Freischaltung| -> Vec<String> {
            let k = HelpCard {
                frei: f,
                ..HelpCard::default()
            };
            k.zeilen(Topic::Gelaende).into_iter().map(|z| z.0).collect()
        };
        assert_eq!(zeilen(Freischaltung::mit(&[])), ["Platte", "Modul"]);
        let alle = zeilen(Freischaltung::mit(&[
            Modul::Gelaende,
            Modul::Mengen,
            Modul::Kosten,
        ]));
        for a in [
            "Platte",
            "Lageplan",
            "Höhen",
            "Lage",
            "Höhenlage",
            "Geländeflächen",
            "Erdmengen",
        ] {
            assert!(alle.iter().any(|x| x == a), "{a}");
        }
        assert!(!alle.iter().any(|x| x == "Modul"));
        // Erdmengen stehen unter „Kosten“ und nennen das Gelände nicht
        // (Bedienbarkeit #192 B, Review H-192a)
        let pro = zeilen(Freischaltung::mit(&[Modul::Gelaende, Modul::Mengen]));
        assert!(!pro.iter().any(|x| x == "Erdmengen"));
        let kosten = zeilen(Freischaltung::mit(&[Modul::Mengen, Modul::Kosten]));
        assert!(kosten.iter().any(|x| x == "Erdmengen"));
    }

    /// Bedienbarkeit 39.5: Mit gewähltem Raum zeigt F1 „Räume“; das Thema
    /// nennt Entstehen, Benennen, Verschieben, Paneel und Entfernen.
    #[test]
    fn thema_raeume() {
        let c = HelpCtx {
            window: None,
            dialog: false,
            flush_pick: false,
            dragging: false,
            tool: None,
            opening: false,
            isolating: false,
            gelaende: false,
            selection: Some(SelKind::Space),
            view: ViewKind::Plan,
        };
        assert_eq!(topic(&c), Topic::Rooms);
        let mut s = crate::scene::Scene::with_model(sk_model::Model::with_seed(3));
        let st = s.model().defaults().storey;
        assert!(s.edit_model("Haus", |m| {
            let aw = m.defaults().exterior_wall;
            let pts = [
                sk_math::vec3(0.0, 0.0, 0.0),
                sk_math::vec3(0.0, 5000.0, 0.0),
                sk_math::vec3(5000.0, 5000.0, 0.0),
                sk_math::vec3(5000.0, 0.0, 0.0),
            ];
            let art = sk_model::Category::ExteriorWall;
            m.add_wall_run(&pts, true, sk_model::RefSide::Left, st, aw, art)
                .is_some()
        }));
        let raum = s.model().spaces(st);
        assert_eq!(raum.len(), 1, "der Raum entsteht von selbst");
        assert_eq!(sel_kind(s.model(), raum[0]), Some(SelKind::Space));
        let t = help().section("raeume").expect("Thema Räume");
        for a in [
            "Entstehen",
            "Benennen",
            "Verschieben",
            "Paneel",
            "Entfernen",
            "Nutzung",
            "Ausbau zeigen",
        ] {
            assert!(t.rows.iter().any(|(x, _)| x == a), "{a}");
        }
        assert!(t.rows.len() <= 10);
    }

    /// Bedienbarkeit 42.2: „Ausbau innen“ ist ab Werk aus. Die Themen, zu
    /// denen F1 im Grundriss, im Schnitt und beim Raum führt, nennen den
    /// Schalter im Zahnrad.
    #[test]
    fn ausbau_innen_in_den_themen_von_f1() {
        let h = help();
        for id in ["geschosse", "schnitt", "raeume"] {
            let t = h.section(id).expect(id);
            assert!(
                t.rows.iter().any(|(_, b)| b.contains("„Ausbau innen“")),
                "{id}"
            );
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
        assert!(zeile(m, "Karten")
            .unwrap()
            .contains("„Wohnfläche“ rechnet nach WoFlV"));
        assert_eq!(zeile(m, "Preise"), None);
        // Bedienbarkeit 38.5: Umschalter und Rechenweg
        let sicht = zeile(m, "Sicht").expect("Zeile Sicht");
        assert!(
            sicht.contains("„VOB | Bestellung“") && sicht.contains("Rechenweg"),
            "{sicht}"
        );
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
                "Bauteil gewählt",
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

    /// Review #156 B1: Ist die Karte mit „Alle Themen“ offen und weit
    /// gerollt, zeigt „Ausbaustufe …“ trotzdem das Thema, von oben.
    #[test]
    fn oeffnen_mit_thema_aus_der_liste() {
        let mut k = HelpCard::default();
        k.follow(Topic::Draw, 0);
        k.set_open(true);
        k.toggle_list();
        k.scroll = 120.0;
        assert!(k.list_open());
        k.open_with(Topic::Ausbaustufen);
        assert!(k.is_open() && !k.list_open());
        assert_eq!(k.topic(), Topic::Ausbaustufen);
        assert_eq!(k.scroll, 0.0);
        // ebenso aus einem anderen Thema heraus
        k.pick(Topic::Quantities);
        k.scroll = 80.0;
        k.open_with(Topic::Ausbaustufen);
        assert_eq!((k.topic(), k.scroll), (Topic::Ausbaustufen, 0.0));
    }

    /// Review #120 H5: Das Mengenfenster heißt nach den freien Blättern,
    /// und Zeilen gesperrter Module fehlen; der Knopf heißt wie im Paneel.
    #[test]
    fn hilfe_nach_stufe() {
        use crate::freischaltung::Freischaltung;
        let mut k = HelpCard::default();
        let zeile = |k: &HelpCard, t: Topic, a: &str| k.zeilen(t).into_iter().find(|z| z.0 == a);
        for (w, titel, mengen, preise) in [
            ("basis", Topic::Quantities.title(), false, false),
            ("pro", "Mengen", true, false),
            ("ultimate", "Mengen, Kosten, AVA", true, true),
        ] {
            k.frei = Freischaltung::stufe(w).unwrap();
            assert_eq!(k.titel(Topic::Quantities), titel, "{w}");
            assert_eq!(k.titel(Topic::Draw), Topic::Draw.title(), "{w}");
            let start = zeile(&k, Topic::Start, "Mengen");
            assert_eq!(start.is_some(), mengen, "{w}");
            if let Some((_, b)) = start {
                assert!(b.contains(k.frei.knopf()), "{w}: {b}");
            }
            assert_eq!(
                zeile(&k, Topic::Materials, "Bestellung").is_some(),
                mengen,
                "{w}"
            );
            assert_eq!(
                zeile(&k, Topic::Materials, "Preise").is_some(),
                preise,
                "{w}"
            );
            assert!(zeile(&k, Topic::Materials, "Ansehen").is_some(), "{w}");
            assert_eq!(
                zeile(&k, Topic::Foundation, "Baugrund").is_some(),
                mengen,
                "{w}"
            );
            assert_eq!(zeile(&k, Topic::Terrace, "Mengen").is_some(), mengen, "{w}");
            // Review #120 H3: Haken und Preise beim Einlesen nur mit Kosten,
            // ohne sie kommen alle neuen Sätze in den Firmenkatalog
            for a in ["Firmenkatalog", "Preise"] {
                assert_eq!(zeile(&k, Topic::Einlesen, a).is_some(), preise, "{w}: {a}");
            }
            assert_eq!(
                zeile(&k, Topic::Einlesen, "Neue Sätze").is_some(),
                !preise,
                "{w}"
            );
            for a in ["Einzelplatz oder Kennwort", "Eigener Baustoff"] {
                let z = zeile(&k, Topic::Einlesen, a);
                assert!(z.is_some_and(|z| !z.1.contains("angehakt")), "{w}: {a}");
            }
            // Bedienbarkeit 41.5: „Ausbaustufen“ nennt zuerst diese Stufe,
            // dann jede Stufe, in jeder Stufe gleich
            let stufen = k.zeilen(Topic::Ausbaustufen);
            assert_eq!(stufen[0], (STUFE_HIER.into(), k.frei.name()), "{w}");
            let namen: Vec<&str> = stufen[1..5].iter().map(|z| z.0.as_str()).collect();
            assert_eq!(namen, ["Basis", "Pro", "Ultimate", "Geländemodul"], "{w}");
            assert!(!k.frei.gesperrt(Topic::Ausbaustufen), "{w}");
        }
        assert_eq!(Freischaltung::ALLES.name(), "Ultimate mit Geländemodul");
    }
}

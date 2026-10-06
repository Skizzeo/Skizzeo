//! Einstellungsfenster (E5): verschiebbares, modales Fenster im
//! Programmfenster mit den Reitern „Stifte“ (Projekt) und „Bedienoberfläche“
//! (Programm). Linientypen, Schraffuren, Oberflächen und Baustoffe folgen mit
//! E6.
//!
//! Projektseite: beim Öffnen beginnt der Schritt „Einstellungen geändert“,
//! jede Eingabe wirkt sofort in allen Ansichten, „Übernehmen“ schließt den
//! Schritt und beginnt einen neuen, „Abbrechen“ verwirft ihn. Programmseite:
//! Kopie des Schemas beim Öffnen und bei „Übernehmen“; „Übernehmen“ und „OK“
//! schreiben `einstellungen.txt`.
//!
//! Alles hier ist Zustand und Zeichnen; `main.rs` lädt die Bilder hoch.

use crate::scene::Scene;
use crate::settings::{Settings, F4_ROLES, RGBA_ROLES, SIZE_ROLES};
use sk_model::{AttrRef, GuidGen, Model, Pen, PenId, SurfaceId};
use sk_paint::{hsv_to_rgb, rgb_to_hsv, Canvas, Path, Rgba};
use sk_platform::{Cursor, Event, Key, Modifiers, MouseButton};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, FieldState, Fonts, Rect};
use std::time::{Duration, Instant};

#[path = "prefs_attr.rs"]
mod attr_tabs;
#[cfg(test)]
pub use attr_tabs::name_free;
use attr_tabs::{attr_unit, reset_attr_tab};

/// Reiter des Fensters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Pens,
    LineTypes,
    Fills,
    Surfaces,
    Materials,
    Ui,
}

impl Tab {
    /// Reihenfolge in der Reiterleiste: fünf Projekt-, ein Programmreiter.
    const ALL: [Tab; 6] = [
        Tab::Pens,
        Tab::LineTypes,
        Tab::Fills,
        Tab::Surfaces,
        Tab::Materials,
        Tab::Ui,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Pens => "Stifte",
            Tab::LineTypes => "Linientypen",
            Tab::Fills => "Schraffuren",
            Tab::Surfaces => "Oberflächen",
            Tab::Materials => "Baustoffe",
            Tab::Ui => "Bedienoberfläche",
        }
    }

    /// Alle Reiter sind bedienbar (seit E6).
    fn enabled(self) -> bool {
        true
    }

    /// Platz der Attributreiter (Linientypen … Baustoffe) in den Feldern
    /// `attr_sel` und `attr_scroll`.
    fn attr_slot(self) -> Option<usize> {
        match self {
            Tab::LineTypes => Some(0),
            Tab::Fills => Some(1),
            Tab::Surfaces => Some(2),
            Tab::Materials => Some(3),
            _ => None,
        }
    }
}

/// Was sich das Fenster für die Sitzung merkt (nicht in der Datei).
#[derive(Clone, Debug, Default)]
pub struct Memory {
    /// Linke obere Ecke im Programmfenster (Pixel); `None`: rechts oben.
    pub pos: Option<(f32, f32)>,
    pub tab: Tab,
    /// Zuletzt benutzte Farben im Farbwähler (höchstens 8).
    pub recent: Vec<Rgba>,
}

/// Lage des Fensters im Programmfenster und Skalierung.
#[derive(Clone, Copy, Debug)]
pub struct Win {
    pub w: u32,
    pub h: u32,
    /// Unterkante der Titelleiste.
    pub top: u32,
    pub scale: f32,
}

/// Was die App nach einem Ereignis tun muss.
#[derive(Clone, Copy, Debug, Default)]
pub struct Out {
    /// Fensterbild neu zeichnen bzw. Aufklapper neu zeichnen.
    pub repaint: bool,
    pub popup: bool,
    /// Fenster verschoben (Bilder nur versetzen).
    pub moved: bool,
    /// Farbschema geändert (alles neu zeichnen, Zeichentabelle neu).
    pub theme: bool,
    /// Projektattribute geändert (Zeichentabelle, Titel „•“).
    pub model: bool,
    /// Fenster geschlossen (OK, Abbrechen, ×).
    pub closed: bool,
}

impl Out {
    fn all() -> Out {
        Out {
            repaint: true,
            popup: true,
            ..Out::default()
        }
    }
}

/// Bedienknöpfe unten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Btn {
    Reset,
    Cancel,
    Apply,
    Ok,
}

/// Eingabefelder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldId {
    PenName,
    PenWidth,
    Softness,
    PxPerMm,
    Size(usize),
    PickR,
    PickG,
    PickB,
    PickHex,
    /// Name des gewählten Eintrags im Attributreiter.
    Name,
    /// Linientyp: Zeile, 0 = Strich, 1 = Lücke.
    Dash(usize, usize),
    /// Schraffur: Schar, 0 Winkel, 1 Abstand, 2 Versatz, 3 Strich, 4 Lücke.
    Hatch(usize, usize),
    /// Schraffur Zickzack: Periode in Schichtdicken.
    Zigzag,
}

/// Auswahllisten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComboId {
    PenWidth,
    Scheme,
    FillKind,
    FillSpace,
    MatFill,
    MatFg,
    MatBg,
    MatSurface,
}

/// Wessen Farbe ein Farbfeld zeigt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorTarget {
    Pen(PenId),
    /// Platz in [`RGBA_ROLES`] bzw. [`F4_ROLES`].
    Rgba(usize),
    F4(usize),
    Accent,
    SkyTop,
    SkyHorizon,
    /// Oberfläche: Ansichtsfläche bzw. Schnittfläche in 3D.
    SurfFace(SurfaceId),
    SurfCut(SurfaceId),
}

/// Bildlaufleisten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BarId {
    Pens,
    Ui,
    /// Liste im Attributreiter.
    List,
}

/// Was unter der Maus liegt.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Close,
    Head,
    Tab(Tab),
    /// Gruppenkopf „PROJEKT“ (`true`) bzw. „PROGRAMM“.
    TabGroup(bool),
    Btn(Btn),
    PenRow(PenId),
    PenUsed(PenId),
    PenNew,
    PenDup,
    PenDel,
    Field(FieldId),
    Swatch(ColorTarget),
    Combo(ComboId),
    Bar(BarId),
    /// Attributreiter: Zeile der Liste, Knöpfe darunter.
    Row(usize),
    AttrNew,
    AttrDup,
    AttrDel,
    /// Kontrollkästchen „Punkt danach“ der Musterzeile.
    Check(usize),
    /// „Zeile/Schar hinzufügen“ bzw. „… entfernen“.
    RowAdd,
    RowDel,
    Group(usize),
    Dot(ColorTarget),
    Advanced,
    /// Aufklapper: Zeile der Auswahlliste, Farbwähler.
    Item(usize),
    PickSv,
    PickHue,
    PickRecent(usize),
    PickOld,
    /// Nachfrage „Zurücksetzen?“: 0 = Zurücksetzen, 1 = Abbrechen.
    Confirm(usize),
}

/// Ziehen mit der Maus.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Fenster am Kopf: Abstand der Maus von der linken oberen Ecke.
    Window(f32, f32),
    /// Schieber einer Bildlaufleiste, Abstand im Schieber.
    Bar(BarId, f32),
    Sv,
    Hue,
    /// Markieren im Textfeld.
    Select,
}

/// Feld in Eingabe: Text, Stand beim Hineingehen.
#[derive(Clone, Debug)]
struct Edit {
    field: FieldId,
    text: TextEdit,
    orig: String,
    /// Fehlertext, wenn der Inhalt ungültig ist.
    invalid: Option<String>,
}

/// Farbwähler.
#[derive(Clone, Debug)]
struct Picker {
    target: ColorTarget,
    /// Farbe beim Öffnen (Esc stellt sie her).
    before: Rgba,
    hue: f32,
    sat: f32,
    val: f32,
    /// Farbfeld, an dem er hängt (Fensterkoordinaten).
    anchor: Rect,
    /// Gerechnetes Sättigung/Helligkeit-Feld (Farbton, Größe, Bild).
    sv: Option<(f32, usize, Canvas)>,
    hue_img: Option<(usize, Canvas)>,
}

/// Aufgeklappte Auswahlliste.
#[derive(Clone, Debug)]
struct Combo {
    id: ComboId,
    items: Vec<String>,
    /// Bildchen vor dem Eintrag (Farbfeld, Kachel), soweit vorhanden.
    icons: Vec<Option<Canvas>>,
    sel: Option<usize>,
    anchor: Rect,
}

/// Aufklapper über dem Fenster.
#[derive(Clone, Debug)]
enum Popup {
    Combo(Combo),
    Picker(Box<Picker>),
    /// Nachfrage „… auf Standard zurücksetzen?“ (Knopf mit Fokus).
    Confirm(usize),
}

/// Breiten der ISO-128-Reihe (mm).
const ISO_WIDTHS: [f32; 7] = [0.13, 0.18, 0.25, 0.35, 0.50, 0.70, 1.00];
/// Neuer Stift: Breite.
const NEW_PEN_WIDTH: f32 = 0.25;
/// Kürzestes Fenster (dip); darunter wird nicht weiter geschrumpft.
const MIN_W: f32 = 640.0;
const MIN_H: f32 = 440.0;
/// Kopf, Fuß, Innenabstand, Zeile der Reiterleiste (dip).
const HEAD: f32 = 48.0;
const FOOT: f32 = 56.0;
const PAD: f32 = 16.0;
const TAB_ROW: f32 = 30.0;
/// Breite des Bearbeitungsbereichs rechts im Reiter „Stifte“ (dip).
const PEN_SIDE: f32 = 210.0;
/// Bereich für Feldwerte im Reiter „Bedienoberfläche“ (dip vom Spaltenrand).
const VALUE_X: f32 = 168.0;
/// Dauer des Aufblinkens bei einem Klick neben das Fenster.
const FLASH: Duration = Duration::from_millis(280);
/// Zuletzt benutzte Farben im Farbwähler.
const RECENT_MAX: usize = 8;

/// Farbgruppen im Reiter „Bedienoberfläche“: Überschrift und Schlüssel der
/// Rollen ([`RGBA_ROLES`], [`F4_ROLES`]).
const GROUPS: [(&str, &[&str]); 8] = [
    (
        "Flächen",
        &[
            "ui.bg",
            "ui.border",
            "ui.hover",
            "ui.pressed",
            "ui.shadow",
            "ui.menu_bg",
            "ui.hud_bg",
            "env.scrim",
        ],
    ),
    (
        "Schrift",
        &[
            "ui.text",
            "ui.text_dim",
            "ui.text_disabled",
            "ui.on_accent",
            "ui.tooltip_bg",
            "ui.tooltip_text",
        ],
    ),
    ("Akzent", &["ui.accent", "ui.accent_hover", "ui.hud_glow"]),
    (
        "Eingabefelder",
        &[
            "ui.field",
            "ui.field_border",
            "ui.field_hover",
            "ui.field_focus",
            "ui.field_invalid",
            "ui.field_text",
            "ui.field_unit",
            "ui.caret",
            "ui.text_select",
            "ui.field_readonly",
        ],
    ),
    (
        "Geschosse und Maße",
        &[
            "ui.level_line",
            "ui.level_line_active",
            "ui.level_handle",
            "ui.level_handle_hover",
            "ui.level_handle_drag",
            "ui.dim_line",
            "ui.dim_text",
            "ui.dim_text_hover",
        ],
    ),
    (
        "Titelleiste",
        &[
            "title.bg",
            "title.glyph",
            "title.glyph_inactive",
            "title.hover",
            "title.pressed",
            "title.close_hover",
            "title.close_pressed",
            "title.close_glyph_hover",
        ],
    ),
    (
        "Bearbeiten (Hilfslinien)",
        &[
            "interact.select",
            "interact.draw",
            "interact.track",
            "interact.guide",
            "interact.start",
            "interact.drag",
            "interact.drag_hot",
            "interact.drag_ghost",
            "interact.shadow_tool",
            "interact.shadow_band",
        ],
    ),
    (
        "Blatt (Mengenliste)",
        &[
            "ui.sheet_bg",
            "ui.sheet_text",
            "ui.sheet_text_dim",
            "ui.sheet_hint",
            "ui.sheet_rule",
            "ui.sheet_tile",
            "ui.sheet_hover",
            "interact.hover_element",
            "ui.sheet_flash",
            "ui.sheet_select",
            "ui.sheet_select_group",
        ],
    ),
];

/// Farbfeld zu einem Rollenschlüssel (der Akzent zieht seine Folgerollen mit).
fn role_target(key: &str) -> ColorTarget {
    if key == "ui.accent" {
        return ColorTarget::Accent;
    }
    if let Some(i) = RGBA_ROLES.iter().position(|r| r.0 == key) {
        return ColorTarget::Rgba(i);
    }
    let i = F4_ROLES.iter().position(|r| r.0 == key).unwrap_or(0);
    ColorTarget::F4(i)
}

fn role_label(c: ColorTarget) -> &'static str {
    match c {
        ColorTarget::Rgba(i) => RGBA_ROLES[i].1,
        ColorTarget::F4(i) => F4_ROLES[i].1,
        ColorTarget::Accent => "Akzent",
        ColorTarget::SkyTop => "Himmel oben",
        ColorTarget::SkyHorizon => "Himmel Horizont",
        ColorTarget::Pen(_) => "Farbe",
        ColorTarget::SurfFace(_) => "Farbe Ansichtsfläche",
        ColorTarget::SurfCut(_) => "Farbe Schnittfläche 3D",
    }
}

// --- Hilfen (auch für die Abnahme) ----------------------------------------

/// Verwender eines Stifts als deutsche Bezeichnungen.
pub fn pen_users(m: &Model, id: PenId) -> Vec<String> {
    m.attr_users(AttrRef::Pen(id))
        .iter()
        .map(|u| u.label())
        .collect()
}

/// Zahl mit deutschem Komma.
fn num(v: f32, decimals: usize) -> String {
    format!("{v:.decimals$}").replace('.', ",")
}

/// Zahl mit höchstens einer Nachkommastelle (Maße in dip).
fn num_short(v: f32) -> String {
    if (v - v.round()).abs() < 1e-4 {
        format!("{}", v.round() as i64)
    } else {
        num(v, 1)
    }
}

/// Satz unter „Strichstärke am Bildschirm“.
pub fn px_hint(px_per_mm: f32) -> String {
    format!(
        "Eine 0,50-mm-Linie ist {} px breit",
        num(0.5 * px_per_mm, 1)
    )
}

/// Farbe als `#RRGGBB`.
pub fn to_hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// `#RRGGBB` (das `#` darf fehlen, Groß- und Kleinschreibung egal).
pub fn parse_hex(t: &str) -> Option<[u8; 3]> {
    let t = t.trim();
    let t = t.strip_prefix('#').unwrap_or(t);
    if t.len() != 6 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&t[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

/// Zahl aus einer Eingabe (Komma oder Punkt).
fn parse_num(t: &str) -> Option<f32> {
    t.trim()
        .replace(',', ".")
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
}

/// Neue Enden des Himmels (`unten` am Horizont, `oben`); die Zwischenstufen
/// behalten je Kanal ihr Verhältnis zwischen den Enden.
pub fn set_sky_ends(th: &mut Theme, unten: Rgba, oben: Rgba) {
    let sky = &mut th.env.sky;
    let n = sky.len();
    if n == 0 {
        return;
    }
    let (a0, a1) = (sky[0].1, sky[n - 1].1);
    let ch = |c: Rgba| [c.0 as f32, c.1 as f32, c.2 as f32, c.3 as f32];
    let (o0, o1, n0, n1) = (ch(a0), ch(a1), ch(unten), ch(oben));
    let (t0, t1) = (sky[0].0, sky[n - 1].0);
    for (t, c) in sky.iter_mut() {
        let old = ch(*c);
        let mut v = [0u8; 4];
        for k in 0..4 {
            let d = o1[k] - o0[k];
            // Ohne Unterschied der alten Enden zählt die Lage der Stufe
            let r = if d.abs() > 0.5 {
                (old[k] - o0[k]) / d
            } else {
                (*t - t0) / (t1 - t0).max(1e-6)
            };
            v[k] = (n0[k] + r * (n1[k] - n0[k])).round().clamp(0.0, 255.0) as u8;
        }
        *c = Rgba(v[0], v[1], v[2], v[3]);
    }
    sky[0].1 = unten;
    sky[n - 1].1 = oben;
    th.rev += 1;
}

/// Größe „anim_ms“ in [`SIZE_ROLES`].
fn anim_role() -> usize {
    SIZE_ROLES
        .iter()
        .position(|r| r.0 == "anim_ms")
        .unwrap_or(0)
}

/// Bereich, Nachkommastellen und Einheit eines Zahlenfelds.
fn field_range(f: FieldId) -> Option<(f32, f32, usize, &'static str)> {
    let dark = Theme::dark();
    match f {
        FieldId::PenWidth => Some((0.0, 2.0, 2, "mm")),
        FieldId::Softness => Some((0.0, 5.0, 2, "")),
        FieldId::PxPerMm => Some((2.0, 12.0, 1, "px/mm")),
        FieldId::Size(i) => match SIZE_ROLES[i].0 {
            "anim_ms" => Some((0.0, 600.0, 0, "ms")),
            "hover_delay_hud" => Some((0.0, 2.0, 2, "s")),
            "arc_span_deg" => Some((20.0, 80.0, 0, "°")),
            _ => {
                let mut d = dark;
                let v = *(SIZE_ROLES[i].2)(&mut d);
                Some((0.5 * v, 3.0 * v, 1, "dip"))
            }
        },
        FieldId::PickR | FieldId::PickG | FieldId::PickB => Some((0.0, 255.0, 0, "")),
        FieldId::PenName | FieldId::PickHex => None,
        FieldId::Name | FieldId::Dash(..) | FieldId::Hatch(..) | FieldId::Zigzag => None,
    }
}

fn field_label(f: FieldId) -> &'static str {
    match f {
        FieldId::PenName => "Name",
        FieldId::PenWidth => "Eigene Breite",
        FieldId::Softness => "Horizont weich",
        FieldId::PxPerMm => "Strichstärke",
        FieldId::Size(i) => SIZE_ROLES[i].1,
        FieldId::PickR => "R",
        FieldId::PickG => "G",
        FieldId::PickB => "B",
        FieldId::PickHex => "Hex",
        FieldId::Name => "Name",
        FieldId::Dash(..) | FieldId::Hatch(..) | FieldId::Zigzag => "",
    }
}

fn is_iso(w: f32) -> bool {
    ISO_WIDTHS.iter().any(|i| (i - w).abs() < 1e-4)
}

/// Stifte nach Nummer.
fn pens_sorted(m: &Model) -> Vec<(PenId, Pen)> {
    let mut v: Vec<(PenId, Pen)> = m
        .attr()
        .pens()
        .iter()
        .map(|(id, p)| (id, p.clone()))
        .collect();
    v.sort_by_key(|p| p.1.number);
    v
}

/// Farbe einer Rolle aus dem Schema lesen.
fn role_color(t: &Theme, c: ColorTarget) -> Rgba {
    let mut t = t.clone();
    match c {
        ColorTarget::Rgba(i) => *(RGBA_ROLES[i].2)(&mut t),
        ColorTarget::F4(i) => Rgba::from_f32(*(F4_ROLES[i].2)(&mut t)),
        ColorTarget::Accent => t.ui.accent,
        ColorTarget::SkyTop => t.env.sky.last().map_or(t.ui.bg, |s| s.1),
        ColorTarget::SkyHorizon => t.env.sky.first().map_or(t.ui.bg, |s| s.1),
        ColorTarget::Pen(_) | ColorTarget::SurfFace(_) | ColorTarget::SurfCut(_) => t.ui.text,
    }
}

// --- Fenster ----------------------------------------------------------------

/// Das Einstellungsfenster.
#[derive(Debug)]
pub struct Prefs {
    open: bool,
    /// Revision zu Beginn des laufenden Schritts (für Abbrechen).
    start_rev: u64,
    /// Schema beim Öffnen bzw. letzten Übernehmen.
    saved: Theme,
    pub tab: Tab,
    pos: Option<(f32, f32)>,
    pen_sel: Option<PenId>,
    pen_scroll: f32,
    /// Stift, der beim nächsten Zeichnen sichtbar werden soll (Platz).
    pending_scroll: Option<usize>,
    ui_scroll: f32,
    groups_open: [bool; GROUPS.len()],
    /// Attributreiter: gewählte Zeile und Bildlauf je Reiter.
    attr_sel: [usize; 4],
    attr_scroll: [f32; 4],
    advanced: bool,
    /// „Eigene …“ gewählt: Feld für die Breite statt des Hinweises.
    custom_width: bool,
    hover: Option<Target>,
    pressed: Option<Target>,
    edit: Option<Edit>,
    popup: Option<Popup>,
    drag: Option<Drag>,
    /// Pfeiltasten wählen in der Tabelle (sonst in der Reiterleiste).
    table_focus: bool,
    flash_until: Option<Instant>,
    recent: Vec<Rgba>,
    /// Fehler beim Schreiben der Einstellungsdatei.
    error: Option<String>,
    mouse: (f64, f64),
}

impl Prefs {
    /// Öffnet das Fenster: beginnt den Schritt „Einstellungen geändert“ und
    /// merkt sich das Schema.
    pub fn open(s: &mut Scene, th: &Theme) -> Prefs {
        let start_rev = s.begin_settings();
        let pen_sel = pens_sorted(s.model()).first().map(|p| p.0);
        Prefs {
            open: true,
            start_rev,
            saved: th.clone(),
            tab: Tab::Pens,
            pos: None,
            pen_sel,
            pen_scroll: 0.0,
            pending_scroll: None,
            ui_scroll: 0.0,
            groups_open: [false; GROUPS.len()],
            attr_sel: [0; 4],
            attr_scroll: [0.0; 4],
            advanced: false,
            custom_width: false,
            hover: None,
            pressed: None,
            edit: None,
            popup: None,
            drag: None,
            table_focus: false,
            flash_until: None,
            recent: Vec::new(),
            error: None,
            mouse: (-1.0, -1.0),
        }
    }

    /// Lage, Reiter und zuletzt benutzte Farben aus der Sitzung.
    pub fn with_memory(mut self, m: &Memory) -> Prefs {
        self.pos = m.pos;
        self.tab = m.tab;
        self.recent = m.recent.clone();
        self
    }

    pub fn memory(&self) -> Memory {
        Memory {
            pos: self.pos,
            tab: self.tab,
            recent: self.recent.clone(),
        }
    }

    /// Übernehmen: Projekt ein Schritt (wenn geändert), Programm in die Datei.
    pub fn apply(&mut self, s: &mut Scene, th: &Theme, st: &mut Settings) {
        s.commit();
        self.start_rev = s.begin_settings();
        self.saved = th.clone();
        self.error = st.save_if_changed(th).err();
    }

    /// OK: wie Übernehmen, dann schließen.
    pub fn ok(&mut self, s: &mut Scene, th: &Theme, st: &mut Settings) {
        s.commit();
        self.error = st.save_if_changed(th).err();
        self.close_now();
    }

    /// Abbrechen: alles seit Öffnen bzw. letztem Übernehmen zurück.
    pub fn cancel(&mut self, s: &mut Scene, th: &mut Theme) {
        s.cancel_settings(self.start_rev);
        *th = self.saved.clone();
        self.close_now();
    }

    fn close_now(&mut self) {
        self.open = false;
        self.popup = None;
        self.edit = None;
        self.drag = None;
    }

    /// „Auf Standard zurücksetzen“ im Reiter `tab` (nach der Nachfrage).
    pub fn reset_tab(&mut self, s: &mut Scene, th: &mut Theme, tab: Tab) {
        match tab {
            Tab::Pens => {
                // Startsatz nach Nummer; Guid und eigene Stifte bleiben
                let (start, _) = sk_model::attr::defaults(&mut GuidGen::with_seed(0));
                s.edit_attr(|m| {
                    for (_, d) in start.pens().iter() {
                        let found = m.attr().pens().iter().find(|p| p.1.number == d.number);
                        if let Some((id, p)) = found.map(|(id, p)| (id, p.clone())) {
                            let pen = Pen {
                                name: d.name.clone(),
                                color: d.color,
                                width_mm: d.width_mm,
                                ..p
                            };
                            if pen != *m.attr().pen(id).unwrap_or(&pen) {
                                m.set_pen(id, pen);
                            }
                        }
                    }
                    true
                });
                self.custom_width = false;
            }
            Tab::Ui => {
                let rev = th.rev;
                *th = Theme::dark();
                th.rev = rev + 1;
            }
            tab => reset_attr_tab(s, tab),
        }
        self.edit = None;
    }

    /// Neuer Stift als Kopie von `from` oder nach Vorgabe.
    fn add_pen(&mut self, s: &mut Scene, from: Option<PenId>) -> PenId {
        let mut id = None;
        s.edit_attr(|m| {
            let number = m.next_pen_number();
            let guid = m.new_guid();
            let pen = match from.and_then(|f| m.attr().pen(f)) {
                Some(p) => Pen {
                    guid,
                    number,
                    name: format!("{} Kopie", p.name),
                    ..p.clone()
                },
                None => Pen {
                    guid,
                    number,
                    name: format!("Stift {number}"),
                    color: [0, 0, 0],
                    width_mm: NEW_PEN_WIDTH,
                },
            };
            id = Some(m.add_pen(pen));
            true
        });
        let id = id.expect("Stift angelegt");
        self.pen_sel = Some(id);
        self.custom_width = false;
        self.scroll_to_pen(s);
        id
    }

    /// Stift löschen (Knopf „Löschen“); `false`, wenn er verwendet wird.
    pub fn remove_pen(&mut self, s: &mut Scene, id: PenId) -> bool {
        let list = pens_sorted(s.model());
        let at = list.iter().position(|p| p.0 == id);
        if !s.edit_attr(|m| m.remove_pen(id)) {
            return false;
        }
        if self.pen_sel == Some(id) {
            let list = pens_sorted(s.model());
            self.pen_sel = at
                .map(|i| i.min(list.len().saturating_sub(1)))
                .and_then(|i| list.get(i))
                .map(|p| p.0);
        }
        true
    }

    // --- Lage ---------------------------------------------------------------

    /// Fenstergröße (Pixel): Vorgabe, bei kleinem Programmfenster kleiner, nie
    /// unter 640 × 440 dip.
    fn size(&self, t: &Theme, w: &Win) -> (f32, f32) {
        let s = w.scale;
        let m = t.size.panel_margin * s;
        let avail_w = w.w as f32 - 2.0 * m;
        let avail_h = w.h as f32 - w.top as f32 - 2.0 * m;
        let ww = (t.size.settings_w * s).min(avail_w).max(MIN_W * s);
        let hh = (t.size.settings_h * s).min(avail_h).max(MIN_H * s);
        (ww.round(), hh.round())
    }

    /// Fensterfläche im Programmfenster (ohne Schatten). Es bleibt ganz im
    /// Programmfenster unter der Titelleiste, soweit es hineinpasst.
    fn frame(&self, t: &Theme, w: &Win) -> Rect {
        let (ww, hh) = self.size(t, w);
        let m = t.size.panel_margin * w.scale;
        let (x, y) = self.pos.unwrap_or((w.w as f32 - ww - m, w.top as f32 + m));
        let x = x.min(w.w as f32 - ww).max(0.0);
        let y = y.min(w.h as f32 - hh).max(w.top as f32);
        Rect::new(x.round(), y.round(), ww, hh)
    }

    /// Lage des Fensterbilds (links oben samt Schatten).
    pub fn origin(&self, t: &Theme, w: &Win) -> (i32, i32) {
        let f = self.frame(t, w);
        let m = (t.size.panel_shadow * w.scale).round();
        ((f.x - m) as i32, (f.y - m) as i32)
    }

    /// Inhaltsbereich rechts der Reiterleiste (Fensterkoordinaten).
    fn content(&self, t: &Theme, w: &Win) -> Rect {
        let f = self.frame(t, w);
        let s = w.scale;
        let x0 = f.x + t.size.settings_tabs_w * s + PAD * s;
        let y0 = f.y + (HEAD + 12.0) * s;
        let x1 = f.x + f.w - PAD * s;
        let y1 = f.y + f.h - (FOOT + 12.0) * s;
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    fn tab_rect(&self, t: &Theme, w: &Win, tab: Tab) -> Rect {
        let f = self.frame(t, w);
        let s = w.scale;
        let i = Tab::ALL.iter().position(|&x| x == tab).unwrap_or(0) as f32;
        // Zwischen den Gruppen Platz für „PROGRAMM“
        let extra = if tab == Tab::Ui { 32.0 } else { 0.0 };
        Rect::new(
            f.x + 8.0 * s,
            f.y + (HEAD + 32.0 + i * TAB_ROW + extra) * s,
            t.size.settings_tabs_w * s - 16.0 * s,
            (TAB_ROW - 2.0) * s,
        )
    }

    fn group_rect(&self, t: &Theme, w: &Win, project: bool) -> Rect {
        let f = self.frame(t, w);
        let s = w.scale;
        let y = if project {
            HEAD + 10.0
        } else {
            HEAD + 32.0 + 5.0 * TAB_ROW + 10.0
        };
        Rect::new(f.x + 16.0 * s, f.y + y * s, 120.0 * s, 18.0 * s)
    }

    fn close_rect(&self, t: &Theme, w: &Win) -> Rect {
        let f = self.frame(t, w);
        let s = w.scale;
        Rect::new(f.x + f.w - 40.0 * s, f.y + 10.0 * s, 28.0 * s, 28.0 * s)
    }

    fn button_rects(&self, t: &Theme, w: &Win) -> [(Btn, Rect, &'static str); 4] {
        let f = self.frame(t, w);
        let s = w.scale;
        let (bh, y) = (30.0 * s, f.y + f.h - (FOOT - 13.0) * s);
        let r = |x: f32, bw: f32| Rect::new(x.round(), y.round(), (bw * s).round(), bh.round());
        let right = f.x + f.w - PAD * s;
        let ok_x = right - 112.0 * s;
        let apply_x = ok_x - 8.0 * s - 116.0 * s;
        let cancel_x = apply_x - 8.0 * s - 104.0 * s;
        [
            (
                Btn::Reset,
                r(f.x + PAD * s, 220.0),
                "Auf Standard zurücksetzen",
            ),
            (Btn::Cancel, r(cancel_x, 104.0), "Abbrechen"),
            (Btn::Apply, r(apply_x, 116.0), "Übernehmen"),
            (Btn::Ok, r(ok_x, 112.0), "OK"),
        ]
    }

    // --- Reiter „Stifte“: Lage ---------------------------------------------

    fn pens_layout(&self, t: &Theme, w: &Win, s: &Scene) -> PensLayout {
        let c = self.content(t, w);
        let sc = w.scale;
        let side = (PEN_SIDE * sc).min(c.w * 0.45);
        let table = Rect::new(c.x, c.y, c.w - side - PAD * sc, c.h);
        let row = t.size.table_row * sc;
        let body_bottom = c.y + c.h - 84.0 * sc;
        let body = Rect::new(
            table.x,
            c.y + row,
            table.w,
            (body_bottom - c.y - row).max(row),
        );
        let pens = pens_sorted(s.model());
        let content_h = pens.len() as f32 * row;
        let bar = (content_h > body.h).then(|| {
            let bw = t.size.scrollbar * sc;
            Rect::new(body.x + body.w - bw, body.y, bw, body.h)
        });
        let row_w = body.w - bar.map_or(0.0, |b| b.w + 4.0 * sc);
        let scroll = self.pen_scroll.clamp(0.0, (content_h - body.h).max(0.0));
        let rows = pens
            .iter()
            .enumerate()
            .map(|(i, (id, _))| {
                (
                    *id,
                    Rect::new(body.x, body.y + i as f32 * row - scroll, row_w, row),
                )
            })
            .collect();
        let by = body.y + body.h + 12.0 * sc;
        let b = |x: f32, bw: f32| {
            Rect::new(
                (c.x + x * sc).round(),
                by.round(),
                (bw * sc).round(),
                (30.0 * sc).round(),
            )
        };
        let side_x = c.x + c.w - side;
        let fw = side - 64.0 * sc;
        let fx = side_x + 64.0 * sc;
        let field_h = t.size.field_height * sc;
        PensLayout {
            table,
            body,
            bar,
            scroll,
            content_h,
            rows,
            new: b(0.0, 76.0),
            dup: b(84.0, 116.0),
            del: b(208.0, 90.0),
            hint_y: by + 30.0 * sc + 20.0 * sc,
            side: Rect::new(side_x, c.y, side, c.h),
            name: Rect::new(fx, c.y + 36.0 * sc, fw, field_h),
            color: Rect::new(
                fx,
                c.y + 76.0 * sc,
                t.size.swatch_w * sc,
                t.size.swatch_h * sc,
            ),
            width: Rect::new(fx, c.y + 108.0 * sc, fw, field_h),
            custom: Rect::new(fx, c.y + 144.0 * sc, fw.min(110.0 * sc), field_h),
            preview: Rect::new(
                side_x,
                c.y + 218.0 * sc,
                side,
                (c.y + c.h - (c.y + 218.0 * sc))
                    .min(168.0 * sc)
                    .max(40.0 * sc),
            ),
        }
    }

    /// Spalten der Stifttabelle (dip vom Tabellenrand): Nr., Farbe, Muster,
    /// Breite, Name; „Verw.“ rechtsbündig.
    const COLS: [f32; 5] = [8.0, 40.0, 86.0, 166.0, 240.0];

    // --- Reiter „Bedienoberfläche“: Lage -----------------------------------

    fn ui_layout(&self, t: &Theme, w: &Win) -> UiLayout {
        let c = self.content(t, w);
        let s = w.scale;
        let bw = t.size.scrollbar * s;
        let cw = c.w - bw - 8.0 * s;
        let lw = (cw * 0.58).round();
        let lx = c.x;
        let rx = c.x + lw + PAD * s;
        let rw = cw - lw - PAD * s;
        let field_h = t.size.field_height * s;
        let sw = (t.size.swatch_w * s, t.size.swatch_h * s);
        let mut items: Vec<(Rect, Target)> = Vec::new();
        let mut texts: Vec<UiText> = Vec::new();
        // Linke Spalte: Farbschema
        let mut y = 0.0;
        texts.push(UiText::heading(lx, y + 18.0 * s, "Farbschema"));
        y += 34.0 * s;
        texts.push(UiText::label(lx, y + 18.0 * s, "Grundschema"));
        items.push((
            Rect::new(
                lx + VALUE_X * s,
                y,
                (170.0 * s).min(lw - VALUE_X * s),
                field_h,
            ),
            Target::Combo(ComboId::Scheme),
        ));
        y += 36.0 * s;
        texts.push(UiText::label(lx, y + 15.0 * s, "Akzentfarbe"));
        items.push((
            Rect::new(lx + VALUE_X * s, y + 2.0 * s, sw.0, sw.1),
            Target::Swatch(ColorTarget::Accent),
        ));
        texts.push(UiText::dim(
            lx,
            y + sw.1 + 18.0 * s,
            "Knöpfe, Fokus, aktive Geschosslinie",
        ));
        y += 50.0 * s;
        for (g, (title, roles)) in GROUPS.iter().enumerate() {
            items.push((Rect::new(lx, y, lw, 28.0 * s), Target::Group(g)));
            texts.push(UiText::group(lx, y + 19.0 * s, *title, self.groups_open[g]));
            y += 30.0 * s;
            if self.groups_open[g] {
                for key in roles.iter() {
                    let ct = role_target(key);
                    texts.push(UiText::label(lx + 20.0 * s, y + 15.0 * s, role_label(ct)));
                    let sx = lx + 190.0 * s;
                    items.push((Rect::new(sx, y + 2.0 * s, sw.0, sw.1), Target::Swatch(ct)));
                    items.push((
                        Rect::new(sx + sw.0 + 8.0 * s, y + 4.0 * s, 16.0 * s, 16.0 * s),
                        Target::Dot(ct),
                    ));
                    y += 26.0 * s;
                }
                y += 4.0 * s;
            }
        }
        let left_h = y;
        // Rechte Spalte: 3D-Umgebung, Bildschirm
        let vx = rx + (VALUE_X - 8.0) * s;
        let mut y = 0.0;
        texts.push(UiText::heading(rx, y + 18.0 * s, "3D-Umgebung"));
        y += 34.0 * s;
        let swatch_row = |texts: &mut Vec<UiText>,
                          items: &mut Vec<(Rect, Target)>,
                          y: &mut f32,
                          label: &'static str,
                          ct: ColorTarget| {
            texts.push(UiText::label(rx, *y + 15.0 * s, label));
            items.push((Rect::new(vx, *y + 2.0 * s, sw.0, sw.1), Target::Swatch(ct)));
            *y += 30.0 * s;
        };
        swatch_row(
            &mut texts,
            &mut items,
            &mut y,
            "Himmel oben",
            ColorTarget::SkyTop,
        );
        swatch_row(
            &mut texts,
            &mut items,
            &mut y,
            "Himmel Horizont",
            ColorTarget::SkyHorizon,
        );
        let gradient = Rect::new(rx, y + 2.0 * s, (200.0 * s).min(rw), 16.0 * s);
        texts.push(UiText::dim(rx, y + 34.0 * s, "Horizont → oben"));
        y += 44.0 * s;
        swatch_row(
            &mut texts,
            &mut items,
            &mut y,
            "Boden",
            role_target("env.ground"),
        );
        texts.push(UiText::label(rx, y + 18.0 * s, "Horizont weich"));
        items.push((
            Rect::new(vx, y, (90.0 * s).min(rw - (vx - rx)), field_h),
            Target::Field(FieldId::Softness),
        ));
        y += 34.0 * s;
        swatch_row(
            &mut texts,
            &mut items,
            &mut y,
            "Flächen ohne Baustoff",
            role_target("env.face"),
        );
        swatch_row(
            &mut texts,
            &mut items,
            &mut y,
            "Kanten ohne Stift",
            role_target("env.edge"),
        );
        y += 12.0 * s;
        texts.push(UiText::heading(rx, y + 18.0 * s, "Bildschirm"));
        y += 34.0 * s;
        texts.push(UiText::label(rx, y + 18.0 * s, "Strichstärke"));
        items.push((
            Rect::new(vx, y, (104.0 * s).min(rw - (vx - rx)), field_h),
            Target::Field(FieldId::PxPerMm),
        ));
        y += 32.0 * s;
        let hint_y = y + 14.0 * s;
        y += 26.0 * s;
        // Übergänge (E18): 0 schaltet alle Animationen aus
        texts.push(UiText::label(rx, y + 18.0 * s, "Animationen"));
        items.push((
            Rect::new(vx, y, (90.0 * s).min(rw - (vx - rx)), field_h),
            Target::Field(FieldId::Size(anim_role())),
        ));
        y += 30.0 * s;
        texts.push(UiText::dim(rx, y + 14.0 * s, "0 ms schaltet sie aus."));
        y += 26.0 * s;
        items.push((Rect::new(rx, y, rw, 28.0 * s), Target::Advanced));
        texts.push(UiText::group(
            rx,
            y + 19.0 * s,
            "Maße (für Fortgeschrittene)",
            self.advanced,
        ));
        y += 32.0 * s;
        if self.advanced {
            for (i, role) in SIZE_ROLES.iter().enumerate() {
                texts.push(UiText::label(rx + 12.0 * s, y + 18.0 * s, role.1));
                items.push((
                    Rect::new(vx, y, (90.0 * s).min(rw - (vx - rx)), field_h),
                    Target::Field(FieldId::Size(i)),
                ));
                y += 30.0 * s;
            }
            y += 4.0 * s;
        }
        texts.push(UiText::dim(
            rx,
            y + 16.0 * s,
            "Gilt für alle Projekte auf diesem Rechner.",
        ));
        y += 28.0 * s;
        let content_h = left_h.max(y) + 8.0 * s;
        let scroll = self.ui_scroll.clamp(0.0, (content_h - c.h).max(0.0));
        let bar = (content_h > c.h).then(|| Rect::new(c.x + c.w - bw, c.y, bw, c.h));
        // In Fensterkoordinaten
        let shift = |r: Rect| Rect::new(r.x, r.y + c.y - scroll, r.w, r.h);
        UiLayout {
            area: c,
            bar,
            scroll,
            content_h,
            items: items.into_iter().map(|(r, tg)| (shift(r), tg)).collect(),
            texts: texts
                .into_iter()
                .map(|mut x| {
                    x.y += c.y - scroll;
                    x
                })
                .collect(),
            gradient: shift(gradient),
            hint_y: hint_y + c.y - scroll,
            hint_x: rx,
        }
    }

    // --- Treffer ------------------------------------------------------------

    fn hit(&self, t: &Theme, w: &Win, s: &Scene, x: f64, y: f64) -> Option<Target> {
        if let Some(p) = &self.popup {
            if let Some(h) = self.popup_hit(p, t, w, x, y) {
                return Some(h);
            }
        }
        let f = self.frame(t, w);
        if !f.contains(x, y) {
            return None;
        }
        if self.close_rect(t, w).contains(x, y) {
            return Some(Target::Close);
        }
        if y < (f.y + HEAD * w.scale) as f64 {
            return Some(Target::Head);
        }
        for (b, r, _) in self.button_rects(t, w) {
            if r.contains(x, y) {
                return Some(Target::Btn(b));
            }
        }
        for tab in Tab::ALL {
            if self.tab_rect(t, w, tab).contains(x, y) {
                return Some(Target::Tab(tab));
            }
        }
        for project in [true, false] {
            if self.group_rect(t, w, project).contains(x, y) {
                return Some(Target::TabGroup(project));
            }
        }
        match self.tab {
            Tab::Pens => {
                let l = self.pens_layout(t, w, s);
                if l.bar.is_some_and(|b| b.contains(x, y)) {
                    return Some(Target::Bar(BarId::Pens));
                }
                if l.body.contains(x, y) {
                    for (id, r) in &l.rows {
                        if r.contains(x, y) {
                            let used_x = r.x + r.w - 44.0 * w.scale;
                            return Some(if x >= used_x as f64 {
                                Target::PenUsed(*id)
                            } else {
                                Target::PenRow(*id)
                            });
                        }
                    }
                    return None;
                }
                let sel = self.pen_sel.is_some();
                for (r, tg) in [
                    (l.new, Target::PenNew),
                    (l.dup, Target::PenDup),
                    (l.del, Target::PenDel),
                ] {
                    if r.contains(x, y) {
                        return Some(tg);
                    }
                }
                if sel {
                    if l.name.contains(x, y) {
                        return Some(Target::Field(FieldId::PenName));
                    }
                    if l.color.contains(x, y) {
                        return self.pen_sel.map(|p| Target::Swatch(ColorTarget::Pen(p)));
                    }
                    if l.width.contains(x, y) {
                        return Some(Target::Combo(ComboId::PenWidth));
                    }
                    if self.show_custom(s) && l.custom.contains(x, y) {
                        return Some(Target::Field(FieldId::PenWidth));
                    }
                }
                None
            }
            Tab::Ui => {
                let l = self.ui_layout(t, w);
                if l.bar.is_some_and(|b| b.contains(x, y)) {
                    return Some(Target::Bar(BarId::Ui));
                }
                if !l.area.contains(x, y) {
                    return None;
                }
                l.items
                    .iter()
                    .find(|(r, _)| r.contains(x, y))
                    .map(|(_, tg)| *tg)
                    .filter(|tg| match tg {
                        Target::Dot(ct) => self.role_changed(t, *ct),
                        _ => true,
                    })
            }
            _ => self.attr_hit(t, w, s, x, y),
        }
    }

    /// Feld „Eigene Breite“ zeigen: gewählt oder Breite außerhalb der Reihe.
    fn show_custom(&self, s: &Scene) -> bool {
        self.custom_width
            || self
                .pen_sel
                .and_then(|id| s.model().attr().pen(id))
                .is_some_and(|p| !is_iso(p.width_mm))
    }

    fn role_changed(&self, t: &Theme, ct: ColorTarget) -> bool {
        role_color(t, ct) != role_color(&Theme::dark(), ct)
    }

    // --- Aufklapper: Lage ---------------------------------------------------

    fn combo_rows(&self, c: &Combo, t: &Theme, w: &Win) -> (Rect, Vec<Rect>) {
        let s = w.scale;
        let row = 26.0 * s;
        let pad = 4.0 * s;
        let h = c.items.len() as f32 * row + 2.0 * pad;
        let mut y = c.anchor.y + c.anchor.h + 2.0 * s;
        if y + h > w.h as f32 {
            y = (c.anchor.y - 2.0 * s - h).max(w.top as f32);
        }
        let r = Rect::new(c.anchor.x, y, c.anchor.w, h);
        let rows = (0..c.items.len())
            .map(|i| Rect::new(r.x, r.y + pad + i as f32 * row, r.w, row))
            .collect();
        let _ = t;
        (r, rows)
    }

    fn picker_rect(&self, p: &Picker, t: &Theme, w: &Win) -> Rect {
        let s = w.scale;
        let (pw, ph) = (t.size.picker_w * s, t.size.picker_h * s);
        // Im Einstellungsfenster bleiben: unter dem Farbfeld, sonst darüber,
        // passt beides nicht, links daneben; nur ein zu kleines Fenster lässt
        // ihn hinausragen
        let f = self.frame(t, w);
        let (left, top) = (f.x.max(0.0), f.y.max(w.top as f32));
        let right = (f.x + f.w).min(w.w as f32);
        let bottom = (f.y + f.h).min(w.h as f32);
        let a = p.anchor;
        let mut x = a.x.min(right - pw).max(left);
        let below = a.y + a.h + 4.0 * s;
        let above = a.y - 4.0 * s - ph;
        let y = if below + ph <= bottom {
            below
        } else if above >= top {
            above
        } else {
            x = (a.x - 8.0 * s - pw).max(left);
            (a.y + a.h * 0.5 - ph * 0.5).min(bottom - ph).max(top)
        };
        Rect::new(x.round(), y.round(), pw.round(), ph.round())
    }

    fn picker_layout(&self, p: &Picker, t: &Theme, w: &Win) -> PickerLayout {
        let r = self.picker_rect(p, t, w);
        let s = w.scale;
        // Rechts neben Farbfeld und Farbtonleiste bleibt Platz für „vorher/jetzt“.
        let side = (r.w - 130.0 * s).min(r.h - 120.0 * s).round();
        let sv = Rect::new(r.x + 16.0 * s, r.y + 16.0 * s, side, side);
        let hue = Rect::new(sv.x + sv.w + 12.0 * s, sv.y, 18.0 * s, side);
        let sx = hue.x + hue.w + 14.0 * s;
        let old = Rect::new(sx, sv.y + 18.0 * s, 36.0 * s, 20.0 * s);
        let new = Rect::new(sx, sv.y + 64.0 * s, 36.0 * s, 20.0 * s);
        let fy = sv.y + sv.h + 12.0 * s;
        let fh = t.size.field_height * s;
        let fw = 50.0 * s;
        let rgb =
            [0, 1, 2].map(|i| Rect::new(sv.x + 16.0 * s + i as f32 * (fw + 24.0 * s), fy, fw, fh));
        let hex = Rect::new(sv.x + 40.0 * s, fy + fh + 8.0 * s, 100.0 * s, fh);
        let ry = hex.y + fh + 10.0 * s;
        let recent = (0..RECENT_MAX)
            .map(|i| {
                Rect::new(
                    sv.x + 64.0 * s + i as f32 * 26.0 * s,
                    ry,
                    20.0 * s,
                    14.0 * s,
                )
            })
            .collect();
        PickerLayout {
            rect: r,
            sv,
            hue,
            old,
            new,
            rgb,
            hex,
            recent,
            recent_y: ry,
        }
    }

    fn confirm_layout(&self, t: &Theme, w: &Win) -> (Rect, [Rect; 2]) {
        let f = self.frame(t, w);
        let s = w.scale;
        let (cw, ch) = ((440.0 * s).round(), (118.0 * s).round());
        let r = Rect::new(
            (f.x + (f.w - cw) * 0.5).round(),
            (f.y + (f.h - ch) * 0.4).round(),
            cw,
            ch,
        );
        let y = r.y + ch - 20.0 * s - 30.0 * s;
        let b2 = Rect::new(r.x + cw - 20.0 * s - 104.0 * s, y, 104.0 * s, 30.0 * s);
        let b1 = Rect::new(b2.x - 8.0 * s - 128.0 * s, y, 128.0 * s, 30.0 * s);
        (r, [b1, b2])
    }

    fn popup_hit(&self, p: &Popup, t: &Theme, w: &Win, x: f64, y: f64) -> Option<Target> {
        match p {
            Popup::Combo(c) => {
                let (r, rows) = self.combo_rows(c, t, w);
                if !r.contains(x, y) {
                    return None;
                }
                Some(
                    rows.iter()
                        .position(|r| r.contains(x, y))
                        .map_or(Target::Combo(c.id), Target::Item),
                )
            }
            Popup::Picker(pk) => {
                let l = self.picker_layout(pk, t, w);
                if !l.rect.contains(x, y) {
                    return None;
                }
                let grow =
                    |r: Rect, d: f32| Rect::new(r.x - d, r.y - d, r.w + 2.0 * d, r.h + 2.0 * d);
                if grow(l.sv, 4.0 * w.scale).contains(x, y) {
                    return Some(Target::PickSv);
                }
                if grow(l.hue, 4.0 * w.scale).contains(x, y) {
                    return Some(Target::PickHue);
                }
                for (i, f) in [FieldId::PickR, FieldId::PickG, FieldId::PickB]
                    .into_iter()
                    .enumerate()
                {
                    if l.rgb[i].contains(x, y) {
                        return Some(Target::Field(f));
                    }
                }
                if l.hex.contains(x, y) {
                    return Some(Target::Field(FieldId::PickHex));
                }
                if l.old.contains(x, y) {
                    return Some(Target::PickOld);
                }
                for (i, r) in l.recent.iter().enumerate().take(self.recent.len()) {
                    if r.contains(x, y) {
                        return Some(Target::PickRecent(i));
                    }
                }
                // Im Wähler, aber auf nichts: nimmt den Klick, tut nichts
                Some(Target::Item(usize::MAX))
            }
            Popup::Confirm(_) => {
                let (r, b) = self.confirm_layout(t, w);
                if !r.contains(x, y) {
                    return None;
                }
                Some(
                    b.iter()
                        .position(|b| b.contains(x, y))
                        .map_or(Target::Item(usize::MAX), Target::Confirm),
                )
            }
        }
    }
}

/// Lage im Reiter „Stifte“ (Fensterkoordinaten).
struct PensLayout {
    table: Rect,
    body: Rect,
    bar: Option<Rect>,
    scroll: f32,
    content_h: f32,
    rows: Vec<(PenId, Rect)>,
    new: Rect,
    dup: Rect,
    del: Rect,
    hint_y: f32,
    side: Rect,
    name: Rect,
    color: Rect,
    width: Rect,
    custom: Rect,
    preview: Rect,
}

/// Text im Reiter „Bedienoberfläche“: Lage (Grundlinie) und Art.
struct UiText {
    x: f32,
    y: f32,
    text: String,
    kind: TextKind,
}

#[derive(Clone, Copy, PartialEq)]
enum TextKind {
    Heading,
    Label,
    Dim,
    /// Aufklappbare Gruppe (offen?).
    Group(bool),
    /// Fette Zwischenüberschrift.
    Title,
    /// Leise, eine Zeile je Eintrag (`\n`), lange Einträge umbrochen statt
    /// gekürzt (Spalte „Verwendet von“ neben der Vorschau).
    Wrap,
}

impl UiText {
    fn heading(x: f32, y: f32, text: impl Into<String>) -> UiText {
        UiText {
            x,
            y,
            text: text.into(),
            kind: TextKind::Heading,
        }
    }
    fn label(x: f32, y: f32, text: impl Into<String>) -> UiText {
        UiText {
            x,
            y,
            text: text.into(),
            kind: TextKind::Label,
        }
    }
    fn dim(x: f32, y: f32, text: impl Into<String>) -> UiText {
        UiText {
            x,
            y,
            text: text.into(),
            kind: TextKind::Dim,
        }
    }
    /// Fette Zwischenüberschrift (nicht aufklappbar).
    fn group_title(x: f32, y: f32, text: impl Into<String>) -> UiText {
        UiText {
            x,
            y,
            text: text.into(),
            kind: TextKind::Title,
        }
    }
    fn wrapped(x: f32, y: f32, text: impl Into<String>) -> UiText {
        UiText {
            x,
            y,
            text: text.into(),
            kind: TextKind::Wrap,
        }
    }
    fn group(x: f32, y: f32, text: impl Into<String>, open: bool) -> UiText {
        UiText {
            x,
            y,
            text: text.into(),
            kind: TextKind::Group(open),
        }
    }
}

/// Lage im Reiter „Bedienoberfläche“ (Fensterkoordinaten, schon verschoben).
struct UiLayout {
    area: Rect,
    bar: Option<Rect>,
    scroll: f32,
    content_h: f32,
    items: Vec<(Rect, Target)>,
    texts: Vec<UiText>,
    gradient: Rect,
    hint_y: f32,
    hint_x: f32,
}

/// Lage im Farbwähler.
struct PickerLayout {
    rect: Rect,
    sv: Rect,
    hue: Rect,
    old: Rect,
    new: Rect,
    rgb: [Rect; 3],
    hex: Rect,
    recent: Vec<Rect>,
    recent_y: f32,
}

// --- Bedienung ----------------------------------------------------------------

/// Was ein Ereignis im Fenster braucht.
pub struct Ctx<'a> {
    pub scene: &'a mut Scene,
    pub theme: &'a mut Theme,
    pub settings: &'a mut Settings,
    pub fonts: &'a Fonts,
    pub win: Win,
}

impl Prefs {
    /// Ein Ereignis, solange das Fenster offen ist (es nimmt alle Maus- und
    /// Tastenereignisse).
    pub fn handle(&mut self, e: &Event, cx: &mut Ctx) -> Out {
        let mut out = Out::default();
        match *e {
            Event::MouseMove { x, y, mods } => self.mouse_move(x, y, mods, cx, &mut out),
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            } => self.mouse_down(x, y, mods, cx, &mut out),
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => self.mouse_up(x, y, cx, &mut out),
            Event::Wheel { delta, x, y, .. } => self.wheel(delta, x, y, cx, &mut out),
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } => self.key(key, mods, cx, &mut out),
            Event::Text(c) => self.text(c, cx, &mut out),
            Event::MouseLeave if self.hover.is_some() => {
                self.hover = None;
                out = Out::all();
            }
            _ => {}
        }
        out
    }

    fn mouse_move(&mut self, x: f64, y: f64, _mods: Modifiers, cx: &mut Ctx, out: &mut Out) {
        self.mouse = (x, y);
        let (t, w) = (&*cx.theme, cx.win);
        match self.drag {
            Some(Drag::Window(dx, dy)) => {
                let before = self.frame(t, &w);
                self.pos = Some((x as f32 - dx, y as f32 - dy));
                let after = self.frame(t, &w);
                self.pos = Some((after.x, after.y));
                out.moved = (before.x, before.y) != (after.x, after.y);
                return;
            }
            Some(Drag::Bar(bar, grab)) => {
                self.drag_bar(bar, grab, y, cx);
                out.repaint = true;
                return;
            }
            Some(Drag::Sv) | Some(Drag::Hue) => {
                self.drag_picker(x, y, cx, out);
                return;
            }
            Some(Drag::Select) => {
                if let Some(f) = self.edit.as_ref().map(|e| e.field) {
                    if let Some(i) = self.caret_from_mouse(f, x, cx) {
                        if let Some(e) = self.edit.as_mut() {
                            e.text.place(i, true);
                        }
                        self.touch_field(f, out);
                    }
                }
                return;
            }
            None => {}
        }
        let h = self.hit(t, &w, cx.scene, x, y);
        if h != self.hover {
            let popup = self.popup.is_some();
            self.hover = h;
            out.repaint = true;
            out.popup = popup;
        }
    }

    fn mouse_down(&mut self, x: f64, y: f64, mods: Modifiers, cx: &mut Ctx, out: &mut Out) {
        self.mouse = (x, y);
        let w = cx.win;
        let hit = self.hit(cx.theme, &w, cx.scene, x, y);
        *out = Out::all();
        // Nachfrage: nur ihre Knöpfe
        if let Some(Popup::Confirm(_)) = self.popup {
            match hit {
                Some(Target::Confirm(i)) => self.pressed = Some(Target::Confirm(i)),
                Some(_) => {}
                None => self.flash(),
            }
            return;
        }
        let in_popup = self.popup.is_some()
            && hit.is_some_and(|h| {
                matches!(
                    h,
                    Target::Item(_)
                        | Target::PickSv
                        | Target::PickHue
                        | Target::PickRecent(_)
                        | Target::PickOld
                ) || matches!(h, Target::Field(f) if is_picker_field(f))
                    || (matches!(h, Target::Combo(_))
                        && matches!(self.popup, Some(Popup::Combo(_))))
            });
        if self.popup.is_some() && !in_popup {
            // Klick daneben: Auswahlliste zu, Farbwähler übernimmt
            self.end_edit(true, cx, out);
            self.close_popup(true, cx, out);
            return;
        }
        let Some(target) = hit else {
            self.end_edit(true, cx, out);
            self.flash();
            return;
        };
        if self
            .edit
            .as_ref()
            .is_some_and(|e| Target::Field(e.field) != target)
        {
            self.end_edit(true, cx, out);
        }
        match target {
            Target::Head => {
                let f = self.frame(cx.theme, &w);
                self.drag = Some(Drag::Window(x as f32 - f.x, y as f32 - f.y));
            }
            Target::Tab(tab) => {
                if tab.enabled() {
                    self.tab = tab;
                    self.table_focus = false;
                    self.custom_width = false;
                }
            }
            Target::PenRow(id) | Target::PenUsed(id) => {
                if self.pen_sel != Some(id) {
                    self.custom_width = false;
                }
                self.pen_sel = Some(id);
                self.table_focus = true;
            }
            Target::Field(f) => {
                if self.edit.as_ref().is_none_or(|e| e.field != f) {
                    self.begin_edit(f, cx);
                }
                if let Some(i) = self.caret_from_mouse(f, x, cx) {
                    if let Some(e) = self.edit.as_mut() {
                        e.text.place(i, mods.shift);
                    }
                }
                self.drag = Some(Drag::Select);
            }
            Target::Swatch(ct) => {
                let anchor = self.swatch_rect(ct, cx);
                self.open_picker(ct, anchor, cx);
            }
            Target::Combo(id) => {
                if matches!(&self.popup, Some(Popup::Combo(c)) if c.id == id) {
                    self.popup = None;
                } else {
                    self.open_combo(id, cx);
                }
            }
            Target::Bar(bar) => {
                let grab = self.bar_grab(bar, y, cx);
                self.drag = Some(Drag::Bar(bar, grab));
                self.drag_bar(bar, grab, y, cx);
            }
            Target::Group(g) => self.groups_open[g] = !self.groups_open[g],
            Target::Advanced => self.advanced = !self.advanced,
            Target::Dot(ct) => {
                let c = role_color(&Theme::dark(), ct);
                self.set_color(ct, c, cx, out);
            }
            Target::Row(i) => {
                if let Some(k) = self.tab.attr_slot() {
                    self.attr_sel[k] = i;
                }
                self.table_focus = true;
            }
            Target::Check(i) => self.toggle_dot(i, cx, out),
            Target::PickSv => self.drag = Some(Drag::Sv),
            Target::PickHue => self.drag = Some(Drag::Hue),
            Target::Close
            | Target::Btn(_)
            | Target::PenNew
            | Target::PenDup
            | Target::PenDel
            | Target::AttrNew
            | Target::AttrDup
            | Target::AttrDel
            | Target::RowAdd
            | Target::RowDel
            | Target::Item(_)
            | Target::PickRecent(_)
            | Target::PickOld
            | Target::Confirm(_) => self.pressed = Some(target),
            Target::TabGroup(_) => {}
        }
        if matches!(self.drag, Some(Drag::Sv | Drag::Hue)) {
            self.drag_picker(x, y, cx, out);
        }
    }

    fn mouse_up(&mut self, x: f64, y: f64, cx: &mut Ctx, out: &mut Out) {
        let dragged = self.drag.take();
        if matches!(dragged, Some(Drag::Window(..))) {
            return;
        }
        if dragged.is_some() {
            *out = Out::all();
        }
        let Some(p) = self.pressed.take() else {
            return;
        };
        *out = Out::all();
        if self.hit(cx.theme, &cx.win, cx.scene, x, y) == Some(p) {
            self.click(p, cx, out);
        }
    }

    fn wheel(&mut self, delta: f64, x: f64, y: f64, cx: &mut Ctx, out: &mut Out) {
        if self.popup.is_some() {
            return;
        }
        let (t, w) = (&*cx.theme, cx.win);
        let step = -(delta as f32) * 3.0 * t.size.table_row * w.scale;
        match self.tab {
            Tab::Pens => {
                let l = self.pens_layout(t, &w, cx.scene);
                if l.body.contains(x, y) {
                    let max = (l.content_h - l.body.h).max(0.0);
                    self.pen_scroll = (l.scroll + step).clamp(0.0, max);
                    out.repaint = true;
                }
            }
            Tab::Ui => {
                let l = self.ui_layout(t, &w);
                if l.area.contains(x, y) {
                    let max = (l.content_h - l.area.h).max(0.0);
                    self.ui_scroll = (l.scroll + step).clamp(0.0, max);
                    out.repaint = true;
                }
            }
            tab => {
                let l = self.attr_layout(t, &w, cx.scene);
                if let (true, Some(k)) = (l.body.contains(x, y), tab.attr_slot()) {
                    let max = (l.content_h - l.body.h).max(0.0);
                    self.attr_scroll[k] = (l.scroll + step).clamp(0.0, max);
                    out.repaint = true;
                }
            }
        }
        if out.repaint {
            self.hover = self.hit(t, &w, cx.scene, x, y);
        }
    }

    /// Klick (Drücken und Loslassen auf demselben Ziel).
    fn click(&mut self, p: Target, cx: &mut Ctx, out: &mut Out) {
        match p {
            Target::Close | Target::Btn(Btn::Cancel) => {
                self.cancel(cx.scene, cx.theme);
                out.theme = true;
                out.model = true;
                out.closed = true;
            }
            Target::Btn(Btn::Ok) => {
                self.ok(cx.scene, cx.theme, cx.settings);
                out.closed = true;
                out.model = true;
            }
            Target::Btn(Btn::Apply) => {
                self.apply(cx.scene, cx.theme, cx.settings);
                out.model = true;
            }
            Target::Btn(Btn::Reset) => {
                if self.tab.enabled() {
                    self.popup = Some(Popup::Confirm(0));
                }
            }
            Target::Confirm(0) => {
                self.popup = None;
                self.reset_tab(cx.scene, cx.theme, self.tab);
                out.theme = true;
                out.model = true;
            }
            Target::Confirm(_) => self.popup = None,
            Target::PenNew => {
                self.add_pen(cx.scene, None);
                out.model = true;
            }
            Target::PenDup => {
                if let Some(id) = self.pen_sel {
                    self.add_pen(cx.scene, Some(id));
                    out.model = true;
                }
            }
            Target::PenDel => {
                if let Some(id) = self.pen_sel {
                    out.model |= self.remove_pen(cx.scene, id);
                }
            }
            Target::AttrNew
            | Target::AttrDup
            | Target::AttrDel
            | Target::RowAdd
            | Target::RowDel => self.attr_click(p, cx, out),
            Target::Item(i) => self.choose(i, cx, out),
            Target::PickOld => {
                if let Some(Popup::Picker(p)) = &self.popup {
                    let (ct, c) = (p.target, p.before);
                    self.set_color(ct, c, cx, out);
                }
            }
            Target::PickRecent(i) => {
                if let (Some(Popup::Picker(p)), Some(&c)) = (&self.popup, self.recent.get(i)) {
                    let ct = p.target;
                    self.set_color(ct, c, cx, out);
                }
            }
            _ => {}
        }
    }

    fn key(&mut self, key: Key, mods: Modifiers, cx: &mut Ctx, out: &mut Out) {
        *out = Out::all();
        if let Some(Popup::Confirm(f)) = self.popup {
            match key {
                Key::Enter => self.click(Target::Confirm(f), cx, out),
                Key::Escape => self.popup = None,
                Key::Tab | Key::Left | Key::Right => {
                    self.popup = Some(Popup::Confirm(1 - f.min(1)))
                }
                _ => {}
            }
            return;
        }
        if self.edit.is_some() {
            self.edit_key(key, mods, cx, out);
            return;
        }
        let up = key == Key::Other(0x26);
        let down = key == Key::Other(0x28);
        if let Some(Popup::Combo(c)) = self.popup.as_mut() {
            let n = c.items.len();
            match key {
                _ if (up || down) && n > 0 => {
                    let i = c
                        .sel
                        .map_or(0, |i| if down { (i + 1) % n } else { (i + n - 1) % n });
                    c.sel = Some(i);
                }
                Key::Enter => {
                    if let Some(i) = c.sel {
                        self.choose(i, cx, out);
                    }
                }
                Key::Escape => self.popup = None,
                _ => {}
            }
            return;
        }
        if matches!(self.popup, Some(Popup::Picker(_))) {
            match key {
                Key::Enter => self.close_popup(true, cx, out),
                Key::Escape => self.close_popup(false, cx, out),
                _ => {}
            }
            return;
        }
        match key {
            // Strg+Z/Y gehören, solange das Fenster offen ist, niemandem
            Key::Char('Z') | Key::Char('Y') if mods.ctrl => {}
            Key::Enter => self.click(Target::Btn(Btn::Ok), cx, out),
            Key::Escape => self.click(Target::Btn(Btn::Cancel), cx, out),
            _ if (up || down) && self.table_focus && self.tab == Tab::Pens => {
                let list = pens_sorted(cx.scene.model());
                let i = list.iter().position(|p| Some(p.0) == self.pen_sel);
                let n = list.len();
                let j = match i {
                    Some(i) if down => (i + 1).min(n.saturating_sub(1)),
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                self.pen_sel = list.get(j).map(|p| p.0);
                self.custom_width = false;
                self.scroll_to_pen(cx.scene);
            }
            _ if (up || down) && self.table_focus && self.tab.attr_slot().is_some() => {
                self.step_row(down, cx.scene);
            }
            _ if up || down => {
                let tabs: Vec<Tab> = Tab::ALL.into_iter().filter(|t| t.enabled()).collect();
                let i = tabs.iter().position(|&t| t == self.tab).unwrap_or(0);
                let j = if down {
                    (i + 1) % tabs.len()
                } else {
                    (i + tabs.len() - 1) % tabs.len()
                };
                self.tab = tabs[j];
            }
            _ => {}
        }
    }

    /// Tasten im Feld: Bearbeiten, Enter/Tab übernimmt, Esc stellt her.
    fn edit_key(&mut self, key: Key, mods: Modifiers, cx: &mut Ctx, out: &mut Out) {
        let Some(e) = self.edit.as_mut() else {
            return;
        };
        let f = e.field;
        let sh = mods.shift;
        let mut changed = true;
        match key {
            Key::Escape => {
                let orig = e.orig.clone();
                e.text = TextEdit::new(&orig);
                self.apply_text(f, cx, out);
                self.edit = None;
                return;
            }
            Key::Enter | Key::Tab => {
                self.end_edit(true, cx, out);
                return;
            }
            Key::Backspace => e.text.backspace(),
            Key::Delete => e.text.delete(),
            Key::Left => {
                e.text.left(sh);
                changed = false;
            }
            Key::Right => {
                e.text.right(sh);
                changed = false;
            }
            Key::Home => {
                e.text.home(sh);
                changed = false;
            }
            Key::End => {
                e.text.end(sh);
                changed = false;
            }
            Key::Char('A') if mods.ctrl => {
                e.text.select_all();
                changed = false;
            }
            Key::Char('C') if mods.ctrl => {
                sk_platform::set_clipboard_text(e.text.selected());
                changed = false;
            }
            Key::Char('X') if mods.ctrl => {
                let cut = e.text.cut();
                sk_platform::set_clipboard_text(&cut);
            }
            Key::Char('V') if mods.ctrl => {
                let paste = sk_platform::clipboard_text().unwrap_or_default();
                let line: String = paste.lines().next().unwrap_or("").into();
                e.text.insert(&line);
            }
            Key::Char('Z') if mods.ctrl => changed = e.text.undo(),
            _ => changed = false,
        }
        if changed {
            self.apply_text(f, cx, out);
        }
    }

    fn text(&mut self, ch: char, cx: &mut Ctx, out: &mut Out) {
        let Some(e) = self.edit.as_mut() else {
            return;
        };
        let f = e.field;
        let ok = match f {
            FieldId::PenName | FieldId::Name => true,
            FieldId::PickHex => ch.is_ascii_hexdigit() || ch == '#',
            _ => ch.is_ascii_digit() || matches!(ch, ',' | '.' | '-'),
        };
        if !ok {
            return;
        }
        e.text.insert(ch.encode_utf8(&mut [0; 4]));
        *out = Out::all();
        self.apply_text(f, cx, out);
    }

    // --- Felder ---------------------------------------------------------------

    /// Wert eines Felds, wie er außerhalb der Eingabe dasteht.
    fn field_value(&self, f: FieldId, s: &Scene, t: &Theme) -> String {
        let pen = self.pen_sel.and_then(|id| s.model().attr().pen(id));
        let pick = match &self.popup {
            Some(Popup::Picker(p)) => self.color_of(p.target, s, t),
            _ => t.ui.bg,
        };
        match f {
            FieldId::PenName => pen.map_or(String::new(), |p| p.name.clone()),
            FieldId::PenWidth => pen.map_or(String::new(), |p| num(p.width_mm, 2)),
            FieldId::Softness => num(t.env.horizon_softness, 2),
            FieldId::PxPerMm => num(t.px_per_mm, 1),
            FieldId::Size(i) => {
                let mut c = t.clone();
                num_short(*(SIZE_ROLES[i].2)(&mut c))
            }
            FieldId::PickR => pick.0.to_string(),
            FieldId::PickG => pick.1.to_string(),
            FieldId::PickB => pick.2.to_string(),
            FieldId::PickHex => to_hex([pick.0, pick.1, pick.2]),
            FieldId::Name | FieldId::Dash(..) | FieldId::Hatch(..) | FieldId::Zigzag => {
                self.attr_field_value(f, s)
            }
        }
    }

    fn begin_edit(&mut self, f: FieldId, cx: &mut Ctx) {
        let v = self.field_value(f, cx.scene, cx.theme);
        self.edit = Some(Edit {
            field: f,
            text: TextEdit::new(&v),
            orig: v,
            invalid: None,
        });
    }

    /// Feld verlassen: gültiger Inhalt bleibt (er galt schon), ungültiger
    /// fällt auf den Stand davor zurück (`keep = false`: immer zurück).
    fn end_edit(&mut self, keep: bool, cx: &mut Ctx, out: &mut Out) {
        let Some(e) = self.edit.as_mut() else {
            return;
        };
        let f = e.field;
        if !keep || e.invalid.is_some() {
            let orig = e.orig.clone();
            e.text = TextEdit::new(&orig);
            self.apply_text(f, cx, out);
        }
        self.edit = None;
        *out = Out::all();
    }

    /// Prüft den Feldinhalt und lässt ihn gültig sofort wirken.
    fn apply_text(&mut self, f: FieldId, cx: &mut Ctx, out: &mut Out) {
        let Some(text) = self.edit.as_ref().map(|e| e.text.text.clone()) else {
            return;
        };
        let res = self.apply_value(f, &text, cx, out);
        if let Some(e) = self.edit.as_mut() {
            e.invalid = res.err();
        }
        *out = Out {
            repaint: true,
            popup: true,
            ..*out
        };
    }

    fn apply_value(
        &mut self,
        f: FieldId,
        text: &str,
        cx: &mut Ctx,
        out: &mut Out,
    ) -> Result<(), String> {
        if is_attr_field(f) {
            return self.apply_attr_value(f, text, cx, out);
        }
        if f == FieldId::PenName {
            let Some(id) = self.pen_sel else {
                return Ok(());
            };
            let name = text.trim();
            if name.is_empty() {
                return Err("Der Stift braucht einen Namen".into());
            }
            let taken = cx
                .scene
                .model()
                .attr()
                .pens()
                .iter()
                .any(|(o, p)| o != id && p.name == name);
            if taken {
                return Err(format!("„{name}“ gibt es schon"));
            }
            let name = name.to_string();
            out.model |= cx.scene.edit_attr(|m| {
                let Some(mut p) = m.attr().pen(id).cloned() else {
                    return false;
                };
                if p.name == name {
                    return false;
                }
                p.name = name;
                m.set_pen(id, p)
            });
            return Ok(());
        }
        if f == FieldId::PickHex {
            let c = parse_hex(text).ok_or("Farbe als #RRGGBB eingeben")?;
            self.pick_rgb(Rgba::from_rgb8(c), cx, out);
            return Ok(());
        }
        let (lo, hi, dec, unit) = field_range(f).unwrap_or((0.0, 0.0, 0, ""));
        let range = || {
            let u = if unit.is_empty() {
                String::new()
            } else {
                format!(" {unit}")
            };
            format!(
                "{}: erlaubt {} bis {}{u}",
                field_label(f),
                num(lo, dec),
                num(hi, dec)
            )
        };
        let v = parse_num(text)
            .filter(|v| *v >= lo - 1e-6 && *v <= hi + 1e-6)
            .ok_or_else(range)?;
        let t = &mut *cx.theme;
        match f {
            FieldId::PenWidth => {
                let Some(id) = self.pen_sel else {
                    return Ok(());
                };
                let v = (v * 100.0).round() / 100.0;
                out.model |= cx.scene.edit_attr(|m| {
                    let Some(mut p) = m.attr().pen(id).cloned() else {
                        return false;
                    };
                    if (p.width_mm - v).abs() < 1e-6 {
                        return false;
                    }
                    p.width_mm = v;
                    m.set_pen(id, p)
                });
            }
            FieldId::Softness => {
                if t.env.horizon_softness != v {
                    t.env.horizon_softness = v;
                    t.rev += 1;
                    out.theme = true;
                }
            }
            FieldId::PxPerMm => {
                if t.px_per_mm != v {
                    t.px_per_mm = v;
                    t.rev += 1;
                    out.theme = true;
                }
            }
            FieldId::Size(i) => {
                let slot = (SIZE_ROLES[i].2)(t);
                if *slot != v {
                    *slot = v;
                    t.rev += 1;
                    out.theme = true;
                }
            }
            FieldId::PickR | FieldId::PickG | FieldId::PickB => {
                if let Some(Popup::Picker(p)) = &self.popup {
                    let mut c = self.color_of(p.target, cx.scene, cx.theme);
                    let v = v.round() as u8;
                    match f {
                        FieldId::PickR => c.0 = v,
                        FieldId::PickG => c.1 = v,
                        _ => c.2 = v,
                    }
                    self.pick_rgb(c, cx, out);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Stelle im Feldtext unter der Maus.
    fn caret_from_mouse(&self, f: FieldId, x: f64, cx: &Ctx) -> Option<usize> {
        let e = self.edit.as_ref().filter(|e| e.field == f)?;
        let r = self.field_rect(f, cx)?;
        let (t, s) = (&*cx.theme, cx.win.scale);
        let px = t.size.font_small * s;
        let text = &e.text.text;
        let x0 = if matches!(f, FieldId::PenName | FieldId::PickHex | FieldId::Name) {
            widgets::text_field_x(r, s, t)
        } else {
            let unit = field_range(f).map_or_else(|| attr_unit(f), |r| r.3);
            widgets::field_x(cx.fonts, r, text, unit, s, t)
        };
        Some(widgets::caret_at(
            cx.fonts.regular.as_ref(),
            text,
            px,
            x0,
            x as f32,
        ))
    }

    fn field_rect(&self, f: FieldId, cx: &Ctx) -> Option<Rect> {
        let (t, w) = (&*cx.theme, cx.win);
        match f {
            FieldId::PenName => Some(self.pens_layout(t, &w, cx.scene).name),
            FieldId::PenWidth => Some(self.pens_layout(t, &w, cx.scene).custom),
            FieldId::PickR | FieldId::PickG | FieldId::PickB | FieldId::PickHex => {
                let Some(Popup::Picker(p)) = &self.popup else {
                    return None;
                };
                let l = self.picker_layout(p, t, &w);
                Some(match f {
                    FieldId::PickR => l.rgb[0],
                    FieldId::PickG => l.rgb[1],
                    FieldId::PickB => l.rgb[2],
                    _ => l.hex,
                })
            }
            _ if is_attr_field(f) => self
                .attr_layout(t, &w, cx.scene)
                .items
                .iter()
                .find(|(_, tg)| *tg == Target::Field(f))
                .map(|(r, _)| *r),
            _ => self
                .ui_layout(t, &w)
                .items
                .iter()
                .find(|(_, tg)| *tg == Target::Field(f))
                .map(|(r, _)| *r),
        }
    }

    fn touch_field(&self, _f: FieldId, out: &mut Out) {
        out.repaint = true;
        out.popup = true;
    }

    // --- Farben ---------------------------------------------------------------

    fn color_of(&self, ct: ColorTarget, s: &Scene, t: &Theme) -> Rgba {
        match ct {
            ColorTarget::Pen(id) => s
                .model()
                .attr()
                .pen(id)
                .map_or(t.ui.text, |p| Rgba::from_rgb8(p.color)),
            ColorTarget::SurfFace(id) | ColorTarget::SurfCut(id) => {
                let face = matches!(ct, ColorTarget::SurfFace(_));
                s.model().attr().surface(id).map_or(t.ui.text, |o| {
                    Rgba::from_rgb8(if face { o.color } else { o.cut_color })
                })
            }
            _ => role_color(t, ct),
        }
    }

    /// Setzt die Farbe; Rollen mit Deckkraft behalten sie.
    fn set_color(&mut self, ct: ColorTarget, c: Rgba, cx: &mut Ctx, out: &mut Out) {
        let t = &mut *cx.theme;
        match ct {
            ColorTarget::Pen(id) => {
                out.model |= cx.scene.edit_attr(|m| {
                    let Some(mut p) = m.attr().pen(id).cloned() else {
                        return false;
                    };
                    if p.color == [c.0, c.1, c.2] {
                        return false;
                    }
                    p.color = [c.0, c.1, c.2];
                    m.set_pen(id, p)
                });
            }
            ColorTarget::SurfFace(id) | ColorTarget::SurfCut(id) => {
                let face = matches!(ct, ColorTarget::SurfFace(_));
                out.model |= cx.scene.edit_attr(|m| {
                    let Some(mut o) = m.attr().surface(id).cloned() else {
                        return false;
                    };
                    let slot = if face { &mut o.color } else { &mut o.cut_color };
                    if *slot == [c.0, c.1, c.2] {
                        return false;
                    }
                    *slot = [c.0, c.1, c.2];
                    m.set_surface(id, o)
                });
            }
            ColorTarget::Accent => {
                if t.ui.accent != c {
                    t.set_accent(c);
                    out.theme = true;
                }
            }
            ColorTarget::Rgba(i) => {
                let slot = (RGBA_ROLES[i].2)(t);
                let v = Rgba(c.0, c.1, c.2, slot.3);
                if *slot != v {
                    *slot = v;
                    t.rev += 1;
                    out.theme = true;
                }
            }
            ColorTarget::F4(i) => {
                let slot = (F4_ROLES[i].2)(t);
                let mut v = Rgba(c.0, c.1, c.2, 255).to_f32();
                v[3] = slot[3];
                if Rgba::from_f32(*slot) != Rgba::from_f32(v) {
                    *slot = v;
                    t.rev += 1;
                    out.theme = true;
                }
            }
            ColorTarget::SkyTop | ColorTarget::SkyHorizon => {
                let (lo, hi) = (
                    role_color(t, ColorTarget::SkyHorizon),
                    role_color(t, ColorTarget::SkyTop),
                );
                let (lo, hi) = if ct == ColorTarget::SkyTop {
                    (lo, Rgba(c.0, c.1, c.2, hi.3))
                } else {
                    (Rgba(c.0, c.1, c.2, lo.3), hi)
                };
                if (lo, hi)
                    != (
                        role_color(t, ColorTarget::SkyHorizon),
                        role_color(t, ColorTarget::SkyTop),
                    )
                {
                    set_sky_ends(t, lo, hi);
                    out.theme = true;
                }
            }
        }
        out.repaint = true;
        out.popup = true;
    }

    /// Farbe aus R/G/B oder Hex: Farbwähler folgt (Farbton bleibt bei Grau).
    fn pick_rgb(&mut self, c: Rgba, cx: &mut Ctx, out: &mut Out) {
        let Some(Popup::Picker(p)) = self.popup.as_mut() else {
            return;
        };
        let (h, s, v) = rgb_to_hsv(c);
        if s > 0.0 && v > 0.0 {
            p.hue = h;
        }
        if v > 0.0 {
            p.sat = s;
        }
        p.val = v;
        let ct = p.target;
        self.set_color(ct, c, cx, out);
    }

    fn drag_picker(&mut self, x: f64, y: f64, cx: &mut Ctx, out: &mut Out) {
        let (t, w) = (&*cx.theme, cx.win);
        let Some(Popup::Picker(p)) = &self.popup else {
            return;
        };
        let l = self.picker_layout(p, t, &w);
        let Some(Popup::Picker(p)) = self.popup.as_mut() else {
            return;
        };
        let fx = ((x as f32 - l.sv.x) / l.sv.w).clamp(0.0, 1.0);
        let fy = ((y as f32 - l.sv.y) / l.sv.h).clamp(0.0, 1.0);
        match self.drag {
            Some(Drag::Sv) => {
                p.sat = fx;
                p.val = 1.0 - fy;
            }
            Some(Drag::Hue) => {
                let fy = ((y as f32 - l.hue.y) / l.hue.h).clamp(0.0, 1.0);
                p.hue = 360.0 * (1.0 - fy);
            }
            _ => return,
        }
        let c = hsv_to_rgb(p.hue, p.sat, p.val);
        let ct = p.target;
        self.set_color(ct, c, cx, out);
    }

    fn swatch_rect(&self, ct: ColorTarget, cx: &Ctx) -> Rect {
        let (t, w) = (&*cx.theme, cx.win);
        match ct {
            ColorTarget::Pen(_) => self.pens_layout(t, &w, cx.scene).color,
            ColorTarget::SurfFace(_) | ColorTarget::SurfCut(_) => self
                .attr_layout(t, &w, cx.scene)
                .items
                .iter()
                .find(|(_, tg)| *tg == Target::Swatch(ct))
                .map_or(self.frame(t, &w), |(r, _)| *r),
            _ => self
                .ui_layout(t, &w)
                .items
                .iter()
                .find(|(_, tg)| *tg == Target::Swatch(ct))
                .map_or(self.frame(t, &w), |(r, _)| *r),
        }
    }

    fn open_picker(&mut self, ct: ColorTarget, anchor: Rect, cx: &mut Ctx) {
        let c = self.color_of(ct, cx.scene, cx.theme);
        let (hue, sat, val) = rgb_to_hsv(c);
        self.popup = Some(Popup::Picker(Box::new(Picker {
            target: ct,
            before: c,
            hue,
            sat,
            val,
            anchor,
            sv: None,
            hue_img: None,
        })));
    }

    fn open_combo(&mut self, id: ComboId, cx: &mut Ctx) {
        let (t, w) = (&*cx.theme, cx.win);
        let mut icons = Vec::new();
        let (items, sel, anchor) = match id {
            ComboId::PenWidth => {
                let cur = self
                    .pen_sel
                    .and_then(|p| cx.scene.model().attr().pen(p))
                    .map_or(0.0, |p| p.width_mm);
                let mut items: Vec<String> = ISO_WIDTHS
                    .iter()
                    .map(|w| format!("{} mm", num(*w, 2)))
                    .collect();
                items.push("Eigene …".into());
                let sel = ISO_WIDTHS
                    .iter()
                    .position(|w| (w - cur).abs() < 1e-4)
                    .unwrap_or(ISO_WIDTHS.len());
                (items, sel, self.pens_layout(t, &w, cx.scene).width)
            }
            ComboId::Scheme => {
                let r = self
                    .ui_layout(t, &w)
                    .items
                    .iter()
                    .find(|(_, tg)| *tg == Target::Combo(ComboId::Scheme))
                    .map_or(self.frame(t, &w), |(r, _)| *r);
                (vec![Theme::dark().name], 0, r)
            }
            _ => {
                let (items, ic, sel, anchor) = self.attr_combo(id, cx);
                icons = ic;
                (items, sel, anchor)
            }
        };
        self.popup = Some(Popup::Combo(Combo {
            id,
            items,
            icons,
            sel: Some(sel),
            anchor,
        }));
    }

    /// Eintrag der Auswahlliste wählen.
    fn choose(&mut self, i: usize, cx: &mut Ctx, out: &mut Out) {
        let Some(Popup::Combo(c)) = self.popup.take() else {
            return;
        };
        if c.id == ComboId::Scheme {
            return;
        }
        if c.id != ComboId::PenWidth {
            self.attr_choose(c.id, i, cx, out);
            return;
        }
        if let Some(&w) = ISO_WIDTHS.get(i) {
            self.custom_width = false;
            let _ = self.apply_value(FieldId::PenWidth, &num(w, 2), cx, out);
        } else {
            self.custom_width = true;
            self.begin_edit(FieldId::PenWidth, cx);
        }
    }

    /// Aufklapper schließen. Farbwähler: `keep` übernimmt (merkt die Farbe
    /// unter „Zuletzt“), sonst gilt wieder die Farbe von vorher.
    fn close_popup(&mut self, keep: bool, cx: &mut Ctx, out: &mut Out) {
        if let Some(Popup::Picker(p)) = self.popup.take() {
            if keep {
                let c = self.color_of(p.target, cx.scene, cx.theme);
                if c != p.before {
                    self.recent.retain(|r| *r != c);
                    self.recent.insert(0, c);
                    self.recent.truncate(RECENT_MAX);
                }
            } else {
                self.set_color(p.target, p.before, cx, out);
            }
        }
        if self.edit.as_ref().is_some_and(|e| is_picker_field(e.field)) {
            self.edit = None;
        }
        *out = Out {
            theme: out.theme,
            model: out.model,
            ..Out::all()
        };
    }

    // --- Bildlauf ---------------------------------------------------------------

    fn bar_geometry(&self, bar: BarId, cx: &Ctx) -> Option<(Rect, f32, f32, f32)> {
        let (t, w) = (&*cx.theme, cx.win);
        match bar {
            BarId::Pens => {
                let l = self.pens_layout(t, &w, cx.scene);
                l.bar.map(|b| (b, l.scroll, l.content_h, l.body.h))
            }
            BarId::Ui => {
                let l = self.ui_layout(t, &w);
                l.bar.map(|b| (b, l.scroll, l.content_h, l.area.h))
            }
            BarId::List => {
                let l = self.attr_layout(t, &w, cx.scene);
                l.bar.map(|b| (b, l.scroll, l.content_h, l.body.h))
            }
        }
    }

    /// Abstand der Maus vom Schieberanfang; neben dem Schieber springt er mit
    /// seiner Mitte zur Maus.
    fn bar_grab(&self, bar: BarId, y: f64, cx: &Ctx) -> f32 {
        let Some((b, scroll, total, view)) = self.bar_geometry(bar, cx) else {
            return 0.0;
        };
        let max = (total - view).max(1.0);
        let (ty, th) = widgets::scroll_thumb(b, scroll / total, view / total, cx.win.scale);
        let _ = max;
        let y = y as f32;
        if y >= ty && y < ty + th {
            y - ty
        } else {
            th * 0.5
        }
    }

    fn drag_bar(&mut self, bar: BarId, grab: f32, y: f64, cx: &Ctx) {
        let Some((b, _, total, view)) = self.bar_geometry(bar, cx) else {
            return;
        };
        let (_, th) = widgets::scroll_thumb(b, 0.0, view / total, cx.win.scale);
        let free = (b.h - th).max(1.0);
        let f = ((y as f32 - grab - b.y) / free).clamp(0.0, 1.0);
        let v = f * (total - view).max(0.0);
        match bar {
            BarId::Pens => self.pen_scroll = v,
            BarId::Ui => self.ui_scroll = v,
            BarId::List => {
                if let Some(i) = self.tab.attr_slot() {
                    self.attr_scroll[i] = v;
                }
            }
        }
    }

    /// Bildlauf so, dass der gewählte Stift sichtbar ist.
    fn scroll_to_pen(&mut self, s: &Scene) {
        let Some(sel) = self.pen_sel else {
            return;
        };
        let i = pens_sorted(s.model()).iter().position(|p| p.0 == sel);
        let Some(i) = i else {
            return;
        };
        // Zeilenhöhe hier ohne Skalierung: Bildlauf wird beim Zeichnen begrenzt
        self.pending_scroll = Some(i);
    }

    // --- Rand, Hinweis, Zeiger --------------------------------------------------

    fn flash(&mut self) {
        self.flash_until = Some(Instant::now() + FLASH);
    }

    /// Wartezeit bis zum Ende des Aufblinkens.
    pub fn wait(&self) -> Option<Duration> {
        self.flash_until
            .map(|u| u.saturating_duration_since(Instant::now()))
    }

    /// Aufblinken vorbei: `true`, wenn neu zu zeichnen ist.
    pub fn tick(&mut self) -> bool {
        if self.flash_until.is_some_and(|u| Instant::now() >= u) {
            self.flash_until = None;
            return true;
        }
        false
    }

    pub fn cursor(&self) -> Cursor {
        let field = |t: Option<Target>| matches!(t, Some(Target::Field(_)));
        if matches!(self.drag, Some(Drag::Select)) || field(self.hover) {
            Cursor::IBeam
        } else {
            Cursor::Arrow
        }
    }

    /// Hinweis an der Maus.
    pub fn tip(&self, s: &Scene) -> Option<String> {
        if self.drag.is_some() {
            return None;
        }
        let list = |id: PenId| {
            let u = pen_users(s.model(), id);
            let n = u.len();
            let mut text = u.into_iter().take(6).collect::<Vec<_>>().join(", ");
            if n > 6 {
                text.push_str(&format!(" und {} weitere", n - 6));
            }
            text
        };
        match self.hover? {
            Target::TabGroup(true) => Some("Wird mit dem Projekt gespeichert".into()),
            Target::TabGroup(false) => Some("Gilt für alle Projekte auf diesem Rechner".into()),
            Target::Tab(t) if !t.enabled() => Some("Folgt mit dem nächsten Stand".into()),
            Target::PenUsed(id) => {
                let l = list(id);
                (!l.is_empty()).then(|| format!("Verwendet von: {l}"))
            }
            Target::PenDel => {
                let id = self.pen_sel?;
                let l = list(id);
                (!l.is_empty()).then(|| format!("Wird verwendet von {l}"))
            }
            Target::Dot(_) => Some("Geändert, Klick setzt zurück".into()),
            Target::AttrDel => {
                let l = self.attr_users_text(s);
                (!l.is_empty()).then(|| format!("Wird verwendet von {l}"))
            }
            _ => None,
        }
    }
}

fn is_attr_field(f: FieldId) -> bool {
    matches!(
        f,
        FieldId::Name | FieldId::Dash(..) | FieldId::Hatch(..) | FieldId::Zigzag
    )
}

fn is_picker_field(f: FieldId) -> bool {
    matches!(
        f,
        FieldId::PickR | FieldId::PickG | FieldId::PickB | FieldId::PickHex
    )
}

// --- Zeichnen -------------------------------------------------------------------

/// Text mit Grundlinie bei `y`.
fn label(
    c: &mut Canvas,
    f: Option<&sk_paint::font::Font>,
    t: &str,
    px: f32,
    x: f32,
    y: f32,
    col: Rgba,
) {
    widgets::text(c, f, t, px, x, y, col);
}

impl Prefs {
    /// Gewählten Stift in den sichtbaren Bereich rollen (nach Neu, Pfeilen).
    fn settle_scroll(&mut self, t: &Theme, w: &Win, s: &Scene) {
        let Some(i) = self.pending_scroll.take() else {
            return;
        };
        let l = self.pens_layout(t, w, s);
        let row = t.size.table_row * w.scale;
        let (top, bottom) = (i as f32 * row, (i + 1) as f32 * row);
        if top < l.scroll {
            self.pen_scroll = top;
        } else if bottom > l.scroll + l.body.h {
            self.pen_scroll = bottom - l.body.h;
        }
    }

    /// Fensterbild samt Schatten und seine Lage im Programmfenster.
    pub fn paint(&mut self, t: &Theme, fonts: &Fonts, w: &Win, sc: &Scene) -> (Canvas, i32, i32) {
        self.settle_scroll(t, w, sc);
        let f = self.frame(t, w);
        let s = w.scale;
        let m = (t.size.panel_shadow * s).round();
        let mut c = Canvas::new((f.w + 2.0 * m) as usize, (f.h + 2.0 * m) as usize);
        let at = |r: Rect| Rect::new(r.x - f.x + m, r.y - f.y + m, r.w, r.h);
        let u = &t.ui;
        widgets::panel(&mut c, Rect::new(m, m, f.w, f.h), s, t);
        if self.flash_until.is_some() {
            let b = (2.0 * s).round();
            let rad = t.size.corner_radius * s;
            let mut p = Path::new();
            p.rounded_rect(m, m, f.w, f.h, rad);
            p.rounded_rect_hole(m + b, m + b, f.w - 2.0 * b, f.h - 2.0 * b, rad - b);
            c.fill(&p, u.field_focus);
        }
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        // Kopf
        let px_title = t.size.font_title * s;
        label(
            &mut c,
            bold,
            "Einstellungen",
            px_title,
            m + 24.0 * s,
            m + 30.0 * s,
            u.text,
        );
        let cr = at(self.close_rect(t, w));
        if self.hover == Some(Target::Close) {
            let mut p = Path::new();
            p.rounded_rect(cr.x, cr.y, cr.w, cr.h, 4.0 * s);
            c.fill(&p, u.hover);
        }
        let (cx0, cy0, d) = (cr.x + cr.w * 0.5, cr.y + cr.h * 0.5, 5.5 * s);
        let mut p = Path::new();
        p.segment((cx0 - d, cy0 - d), (cx0 + d, cy0 + d), 1.4 * s);
        p.segment((cx0 - d, cy0 + d), (cx0 + d, cy0 - d), 1.4 * s);
        c.fill(&p, u.text_dim);
        let line = s.round().max(1.0);
        let tabs_w = t.size.settings_tabs_w * s;
        c.fill_rect(m, m + HEAD * s, f.w, line, u.border);
        c.fill_rect(m, m + f.h - FOOT * s, f.w, line, u.border);
        c.fill_rect(
            m + tabs_w,
            m + HEAD * s,
            line,
            f.h - (HEAD + FOOT) * s,
            u.border,
        );
        // Reiterleiste
        let small = t.size.font_small * s;
        for project in [true, false] {
            let r = at(self.group_rect(t, w, project));
            let text = if project { "PROJEKT" } else { "PROGRAMM" };
            label(&mut c, bold, text, small, r.x, r.y + 14.0 * s, u.text_dim);
        }
        for tab in Tab::ALL {
            let r = at(self.tab_rect(t, w, tab));
            let hover = self.hover == Some(Target::Tab(tab));
            widgets::tab_item(
                &mut c,
                fonts,
                r,
                tab.label(),
                self.tab == tab,
                hover,
                !tab.enabled(),
                s,
                t,
            );
        }
        // Inhalt
        match self.tab {
            Tab::Pens => self.paint_pens(&mut c, t, fonts, w, sc, &at),
            Tab::Ui => self.paint_ui(&mut c, t, fonts, w, &at),
            _ => self.paint_attr(&mut c, t, fonts, w, sc, &at),
        }
        // Fuß
        for (b, r, text) in self.button_rects(t, w) {
            let st = ButtonState {
                hover: self.hover == Some(Target::Btn(b)),
                pressed: self.pressed == Some(Target::Btn(b)) && self.hover == Some(Target::Btn(b)),
                active: b == Btn::Ok,
                disabled: b == Btn::Reset && !self.tab.enabled(),
            };
            widgets::button(&mut c, fonts, at(r), text, st, s, t);
        }
        let msg = self
            .edit
            .as_ref()
            .and_then(|e| e.invalid.clone())
            .or_else(|| self.error.clone());
        if let Some(msg) = msg {
            let [reset, cancel, ..] = self.button_rects(t, w);
            let x0 = at(reset.1).x + reset.1.w + 14.0 * s;
            let max = at(cancel.1).x - 14.0 * s - x0;
            let text = widgets::ellipsize(regular, &msg, small, max);
            let y = at(reset.1).y + 20.0 * s;
            label(&mut c, regular, &text, small, x0, y, u.field_invalid);
        }
        let (x, y) = self.origin(t, w);
        (c, x, y)
    }

    fn field_state<'a>(&'a self, f: FieldId, value: &'a str, unit: &'a str) -> FieldState<'a> {
        let e = self.edit.as_ref().filter(|e| e.field == f);
        FieldState {
            text: e.map_or(value, |e| e.text.text.as_str()),
            unit,
            hover: self.hover == Some(Target::Field(f)),
            focus: e.is_some(),
            invalid: e.is_some_and(|e| e.invalid.is_some()),
            caret: e.map(|e| e.text.caret),
            select: e.map(|e| e.text.selection()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_pens(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        sc: &Scene,
        at: &dyn Fn(Rect) -> Rect,
    ) {
        let s = w.scale;
        let u = &t.ui;
        let l = self.pens_layout(t, w, sc);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let (font, small) = (t.size.font * s, t.size.font_small * s);
        let model = sc.model();
        let tb = at(l.table);
        let cols = Self::COLS.map(|x| tb.x + x * s);
        let used_right = tb.x + l.body.w - 8.0 * s - l.bar.map_or(0.0, |b| b.w + 4.0 * s);
        let head_y = tb.y + 18.0 * s;
        for (i, h) in ["Nr.", "Farbe", "Muster", "Breite", "Name"]
            .iter()
            .enumerate()
        {
            label(c, bold, h, small, cols[i], head_y, u.text_dim);
        }
        let verw_w = bold.map_or(0.0, |f| f.width("Verw.", small));
        label(
            c,
            bold,
            "Verw.",
            small,
            used_right - verw_w,
            head_y,
            u.text_dim,
        );
        c.fill_rect(
            tb.x,
            at(l.body).y - s.round(),
            l.table.w,
            s.round().max(1.0),
            u.border,
        );
        // Zeilen in einem eigenen Bild (Bildlauf schneidet ab)
        let body = at(l.body);
        let mut bc = Canvas::new(body.w.ceil() as usize, body.h.ceil() as usize);
        let paper = Rgba::from_rgb8(model.attr().display().paper);
        let pens = pens_sorted(model);
        for ((id, pen), (_, r)) in pens.iter().zip(&l.rows) {
            let r = Rect::new(r.x - l.body.x, r.y - l.body.y, r.w, r.h);
            if r.y + r.h < 0.0 || r.y > body.h {
                continue;
            }
            let sel = self.pen_sel == Some(*id);
            let hover =
                matches!(self.hover, Some(Target::PenRow(h) | Target::PenUsed(h)) if h == *id);
            if sel {
                let b = s.round().max(1.0);
                bc.fill_rect(r.x, r.y, r.w, r.h, u.accent);
                bc.fill_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, u.pressed);
            } else if hover {
                bc.fill_rect(r.x, r.y, r.w, r.h, u.hover);
            }
            let x = |i: usize| cols[i] - tb.x;
            let base = r.y + (r.h + regular.map_or(font * 0.7, |f| f.cap_height(font))) * 0.5;
            label(
                &mut bc,
                regular,
                &pen.number.to_string(),
                font,
                x(0),
                base,
                u.text,
            );
            let sw = Rect::new(
                x(1),
                r.y + (r.h - t.size.swatch_h * s) * 0.5,
                t.size.swatch_w * s,
                t.size.swatch_h * s,
            );
            widgets::swatch(&mut bc, sw, Rgba::from_rgb8(pen.color), false, s, t);
            let pat = Rect::new(x(2), sw.y, 68.0 * s, sw.h);
            bc.fill_rect(pat.x, pat.y, pat.w, pat.h, paper);
            let lw = pen.width_mm * t.px_per_mm * s;
            if pen.width_mm > 0.0 {
                let lw = lw.max(0.6);
                let my = pat.y + pat.h * 0.5;
                let mut p = Path::new();
                p.segment((pat.x + 4.0 * s, my), (pat.x + 4.0 * s + 60.0 * s, my), lw);
                bc.fill(&p, Rgba::from_rgb8(pen.color));
            }
            label(
                &mut bc,
                regular,
                &format!("{} mm", num(pen.width_mm, 2)),
                font,
                x(3),
                base,
                u.text,
            );
            let users = model.attr_users(AttrRef::Pen(*id)).len().to_string();
            let uw = regular.map_or(0.0, |f| f.width(&users, font));
            let ur = used_right - tb.x;
            let name_w = ur - uw - 12.0 * s - x(4);
            let name = widgets::ellipsize(regular, &pen.name, font, name_w);
            label(&mut bc, regular, &name, font, x(4), base, u.text);
            label(&mut bc, regular, &users, font, ur - uw, base, u.text);
        }
        c.blit(&bc, body.x as i32, body.y as i32);
        if let Some(b) = l.bar {
            let total = l.content_h.max(1.0);
            let hover = self.hover == Some(Target::Bar(BarId::Pens))
                || matches!(self.drag, Some(Drag::Bar(BarId::Pens, _)));
            widgets::scrollbar(c, at(b), l.scroll / total, l.body.h / total, hover, s, t);
        }
        // Knöpfe und Hinweis
        let used = self
            .pen_sel
            .map(|id| pen_users(model, id))
            .unwrap_or_default();
        for (r, tg, text, disabled) in [
            (l.new, Target::PenNew, "Neu", false),
            (l.dup, Target::PenDup, "Duplizieren", self.pen_sel.is_none()),
            (
                l.del,
                Target::PenDel,
                "Löschen",
                self.pen_sel.is_none() || !used.is_empty(),
            ),
        ] {
            let st = ButtonState {
                hover: self.hover == Some(tg) && !disabled,
                pressed: self.pressed == Some(tg) && self.hover == Some(tg) && !disabled,
                active: false,
                disabled,
            };
            widgets::button(c, fonts, at(r), text, st, s, t);
        }
        if !used.is_empty() {
            let text = format!("Löschen gesperrt: wird verwendet von {}", used.join(", "));
            let text = widgets::ellipsize(regular, &text, small, l.table.w);
            label(
                c,
                regular,
                &text,
                small,
                tb.x,
                at(Rect::new(0.0, l.hint_y, 0.0, 0.0)).y,
                u.text_dim,
            );
        }
        // Bearbeitungsbereich
        let Some((id, pen)) = pens.iter().find(|p| Some(p.0) == self.pen_sel) else {
            return;
        };
        let side = at(l.side);
        label(
            c,
            bold,
            &format!("Stift {}", pen.number),
            t.size.font_title * s,
            side.x,
            side.y + 18.0 * s,
            u.text,
        );
        let lab = |c: &mut Canvas, r: Rect, text: &str| {
            let r = at(r);
            let cap = regular.map_or(font * 0.7, |f| f.cap_height(font));
            label(
                c,
                regular,
                text,
                font,
                side.x,
                r.y + (r.h + cap) * 0.5,
                u.text_dim,
            );
        };
        lab(c, l.name, "Name");
        let st = self.field_state(FieldId::PenName, &pen.name, "");
        widgets::text_field(c, fonts, at(l.name), &st, s, t);
        lab(c, l.color, "Farbe");
        let hover = self.hover == Some(Target::Swatch(ColorTarget::Pen(*id)));
        widgets::swatch(c, at(l.color), Rgba::from_rgb8(pen.color), hover, s, t);
        let rgb = format!("{} · {} · {}", pen.color[0], pen.color[1], pen.color[2]);
        let cr = at(l.color);
        label(
            c,
            regular,
            &rgb,
            small,
            cr.x + cr.w + 10.0 * s,
            cr.y + cr.h * 0.5 + 4.5 * s,
            u.text_dim,
        );
        lab(c, l.width, "Breite");
        let open = matches!(&self.popup, Some(Popup::Combo(cb)) if cb.id == ComboId::PenWidth);
        let shown = if self.custom_width || !is_iso(pen.width_mm) {
            "Eigene …".to_string()
        } else {
            format!("{} mm", num(pen.width_mm, 2))
        };
        widgets::combo(
            c,
            fonts,
            at(l.width),
            &shown,
            self.hover == Some(Target::Combo(ComboId::PenWidth)),
            open,
            s,
            t,
        );
        if self.show_custom(sc) {
            lab(c, l.custom, "Eigene");
            let v = num(pen.width_mm, 2);
            let st = self.field_state(FieldId::PenWidth, &v, "mm");
            widgets::field(c, fonts, at(l.custom), &st, s, t);
        } else {
            let y = at(l.custom).y + 12.0 * s;
            label(
                c,
                regular,
                "ISO 128: 0,13 · 0,18 · 0,25 · 0,35",
                small,
                side.x,
                y,
                u.text_dim,
            );
            label(
                c,
                regular,
                "0,50 · 0,70 · 1,00 · Eigene …",
                small,
                side.x,
                y + 17.0 * s,
                u.text_dim,
            );
        }
        let pr = at(l.preview);
        label(
            c,
            bold,
            "Vorschau 1:50",
            font,
            side.x,
            pr.y - 10.0 * s,
            u.text,
        );
        self.paint_preview(c, pr, pen.number, t, fonts, s, sc);
    }

    /// Wandstück im Grundriss mit den heutigen Stiften: Umriss „Schnitt
    /// kräftig“, Schichtgrenze, Dämmung im Zickzack, Mauerwerk schraffiert.
    #[allow(clippy::too_many_arguments)]
    fn paint_preview(
        &self,
        c: &mut Canvas,
        r: Rect,
        number: u16,
        t: &Theme,
        fonts: &Fonts,
        s: f32,
        sc: &Scene,
    ) {
        let model = sc.model();
        let a = model.attr();
        let d = a.display();
        let u = &t.ui;
        let paper = Rgba::from_rgb8(d.paper);
        let b = s.round().max(1.0);
        c.fill_rect(r.x, r.y, r.w, r.h, u.border);
        c.fill_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, paper);
        let pen = |id| a.pen(id);
        let px = |w: f32| (w * t.px_per_mm * s).max(0.6);
        let col = |p: Option<&Pen>| p.map_or(t.env.edge, |p| Rgba::from_rgb8(p.color));
        let kind = |k: u8| d.drawing[k as usize].pen;
        let find = |zig: bool| {
            model.materials().iter().map(|(_, m)| m).find(|m| {
                a.fill(m.cut_fill).is_some_and(|f| match &f.kind {
                    sk_model::FillKind::Zigzag { .. } => zig,
                    sk_model::FillKind::Lines(_) => !zig,
                    _ => false,
                })
            })
        };
        let (ins, mas) = (find(true), find(false));
        let wall = Rect::new(
            r.x + 24.0 * s,
            r.y + 34.0 * s,
            r.w - 48.0 * s,
            (r.h - 78.0 * s).max(20.0 * s),
        );
        let split = wall.y + wall.h * 0.45;
        // Gründe der beiden Schichten
        let bg = |m: Option<&sk_model::Material>| {
            m.and_then(|m| pen(m.cut_bg))
                .map_or(paper, |p| Rgba::from_rgb8(p.color))
        };
        c.fill_rect(wall.x, wall.y, wall.w, split - wall.y, bg(ins));
        c.fill_rect(wall.x, split, wall.w, wall.y + wall.h - split, bg(mas));
        // Mauerwerk: Schraffur in eigenem Bild, damit sie an der Schicht endet
        if let Some(m) = mas {
            let hp = pen(m.cut_fg);
            let lines = a.fill(m.cut_fill).and_then(|f| match &f.kind {
                sk_model::FillKind::Lines(l) => l.first().copied(),
                _ => None,
            });
            if let (Some(hp), Some(l)) = (hp, lines) {
                let (lw, lh) = (
                    wall.w.ceil() as usize,
                    (wall.y + wall.h - split).ceil() as usize,
                );
                let mut hc = Canvas::new(lw, lh);
                let step = (l.spacing_mm * t.px_per_mm * s).max(3.0);
                let h = lh as f32;
                let mut x = -h;
                while x < lw as f32 + h {
                    let mut p = Path::new();
                    p.segment((x, 0.0), (x + h, h), px(hp.width_mm));
                    hc.fill(&p, col(Some(hp)));
                    x += step * std::f32::consts::SQRT_2;
                }
                c.blit(&hc, wall.x as i32, split as i32);
            }
        }
        // Dämmung: Zickzack über die Schichtdicke
        if let Some(hp) = ins.and_then(|m| pen(m.cut_fg)) {
            let th = split - wall.y;
            let step = th * 0.5;
            let mut p = Path::new();
            let mut x = wall.x;
            let mut top = false;
            while x < wall.x + wall.w - 1e-3 {
                let nx = (x + step).min(wall.x + wall.w);
                let (y0, y1) = if top {
                    (wall.y, split)
                } else {
                    (split, wall.y)
                };
                let k = (nx - x) / step;
                p.segment((x, y0), (nx, y0 + (y1 - y0) * k), px(hp.width_mm));
                x = nx;
                top = !top;
            }
            c.fill(&p, col(Some(hp)));
        }
        // Schichtgrenze und Umriss
        let layer = pen(kind(sk_model::edge_kind::CUT_LAYER));
        if let Some(lp) = layer.filter(|p| p.width_mm > 0.0) {
            let mut p = Path::new();
            p.segment((wall.x, split), (wall.x + wall.w, split), px(lp.width_mm));
            c.fill(&p, col(Some(lp)));
        }
        if let Some(cp) = pen(kind(sk_model::edge_kind::CUT)).filter(|p| p.width_mm > 0.0) {
            let lw = px(cp.width_mm);
            let mut p = Path::new();
            let (x0, y0, x1, y1) = (wall.x, wall.y, wall.x + wall.w, wall.y + wall.h);
            p.segment((x0, y0), (x1, y0), lw)
                .segment((x1, y0), (x1, y1), lw)
                .segment((x1, y1), (x0, y1), lw)
                .segment((x0, y1), (x0, y0), lw);
            c.fill(&p, col(Some(cp)));
        }
        let regular = fonts.regular.as_ref();
        let small = t.size.font_small * s;
        let text = format!("Wand im Grundriss mit Stift {number}");
        let tw = regular.map_or(0.0, |f| f.width(&text, small));
        label(
            c,
            regular,
            &text,
            small,
            r.x + (r.w - tw) * 0.5,
            r.y + r.h - 14.0 * s,
            u.field_unit,
        );
    }

    fn paint_ui(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        at: &dyn Fn(Rect) -> Rect,
    ) {
        let s = w.scale;
        let u = &t.ui;
        let l = self.ui_layout(t, w);
        let area = at(l.area);
        let mut cc = Canvas::new(area.w.ceil() as usize, area.h.ceil() as usize);
        let rel = |r: Rect| Rect::new(r.x - l.area.x, r.y - l.area.y, r.w, r.h);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let (font, small) = (t.size.font * s, t.size.font_small * s);
        for tx in &l.texts {
            let (x, y) = (tx.x - l.area.x, tx.y - l.area.y);
            if y < -20.0 * s || y > area.h + 20.0 * s {
                continue;
            }
            match tx.kind {
                TextKind::Heading => {
                    label(&mut cc, bold, &tx.text, t.size.font_title * s, x, y, u.text)
                }
                TextKind::Label => label(&mut cc, regular, &tx.text, font, x, y, u.text_dim),
                TextKind::Dim | TextKind::Wrap => {
                    label(&mut cc, regular, &tx.text, small, x, y, u.field_unit)
                }
                TextKind::Title => label(&mut cc, bold, &tx.text, font, x, y, u.text),
                TextKind::Group(open) => {
                    widgets::disclosure(&mut cc, x + 4.0 * s, y - 5.0 * s, open, u.text, s);
                    label(&mut cc, bold, &tx.text, font, x + 16.0 * s, y, u.text);
                }
            }
        }
        // Verlauf des Himmels: Horizont links, oben rechts
        let g = rel(l.gradient);
        let sky = &t.env.sky;
        let (gw, gh) = (g.w.round().max(1.0) as usize, g.h.round().max(1.0) as usize);
        let k1 = sky.last().map_or(1.0, |x| x.0).max(1e-6);
        let grad = Canvas::from_fn(gw, gh, |x, _| {
            let k = x as f32 / (gw.max(2) - 1) as f32 * k1;
            let i = sky.iter().position(|p| p.0 >= k).unwrap_or(sky.len() - 1);
            if i == 0 {
                return sky[0].1;
            }
            let (a, b) = (sky[i - 1], sky[i]);
            let f = ((k - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0);
            let mix = |p: u8, q: u8| (p as f32 + (q as f32 - p as f32) * f).round() as u8;
            Rgba(
                mix(a.1 .0, b.1 .0),
                mix(a.1 .1, b.1 .1),
                mix(a.1 .2, b.1 .2),
                255,
            )
        });
        cc.blit(&grad, g.x as i32, g.y as i32);
        // Satz zur Strichstärke
        label(
            &mut cc,
            regular,
            &px_hint(t.px_per_mm),
            small,
            l.hint_x - l.area.x,
            l.hint_y - l.area.y,
            u.field_unit,
        );
        for (r, tg) in &l.items {
            let rr = rel(*r);
            if rr.y + rr.h < 0.0 || rr.y > area.h {
                continue;
            }
            let hover = self.hover == Some(*tg);
            match *tg {
                Target::Combo(id) => {
                    let open = matches!(&self.popup, Some(Popup::Combo(cb)) if cb.id == id);
                    widgets::combo(&mut cc, fonts, rr, &t.name, hover, open, s, t);
                }
                Target::Swatch(ct) => {
                    widgets::swatch(&mut cc, rr, role_color(t, ct), hover, s, t);
                }
                Target::Dot(ct) => {
                    if self.role_changed(t, ct) {
                        let rad = 4.0 * s;
                        let (x, y) = (rr.x + rr.w * 0.5, rr.y + rr.h * 0.5);
                        widgets::ring(&mut cc, x, y, rad, rad, u.accent);
                        if hover {
                            label(
                                &mut cc,
                                regular,
                                "geändert, Klick = zurück",
                                small,
                                rr.x + rr.w + 4.0 * s,
                                y + 4.5 * s,
                                u.field_unit,
                            );
                        }
                    }
                }
                Target::Field(f) => {
                    let v = self.field_value_theme(f, t);
                    let unit = field_range(f).map_or("", |r| r.3);
                    let st = self.field_state(f, &v, unit);
                    widgets::field(&mut cc, fonts, rr, &st, s, t);
                }
                Target::Group(_) | Target::Advanced if hover => {
                    let mut p = Path::new();
                    p.rounded_rect(rr.x - 4.0 * s, rr.y, rr.w + 4.0 * s, rr.h, 4.0 * s);
                    let mut under = Canvas::new(cc.width, cc.height);
                    under.fill(&p, u.hover);
                    under.blit(&cc, 0, 0);
                    cc = under;
                }
                _ => {}
            }
        }
        c.blit(&cc, area.x as i32, area.y as i32);
        if let Some(b) = l.bar {
            let total = l.content_h.max(1.0);
            let hover = self.hover == Some(Target::Bar(BarId::Ui))
                || matches!(self.drag, Some(Drag::Bar(BarId::Ui, _)));
            widgets::scrollbar(c, at(b), l.scroll / total, l.area.h / total, hover, s, t);
        }
    }

    /// Feldwert der Programmseite (ohne Szene).
    fn field_value_theme(&self, f: FieldId, t: &Theme) -> String {
        match f {
            FieldId::Softness => num(t.env.horizon_softness, 2),
            FieldId::PxPerMm => num(t.px_per_mm, 1),
            FieldId::Size(i) => {
                let mut c = t.clone();
                num_short(*(SIZE_ROLES[i].2)(&mut c))
            }
            _ => String::new(),
        }
    }

    /// Bild des Aufklappers (Auswahlliste, Farbwähler, Nachfrage) samt
    /// Schatten und Lage; `None`, wenn keiner offen ist.
    pub fn paint_popup(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        sc: &Scene,
    ) -> Option<(Canvas, i32, i32)> {
        let s = w.scale;
        let m = (t.size.panel_shadow * s).round();
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let small = t.size.font_small * s;
        let popup = self.popup.take()?;
        let res = match &popup {
            Popup::Combo(cb) => {
                let (r, rows) = self.combo_rows(cb, t, w);
                let mut c = Canvas::new((r.w + 2.0 * m) as usize, (r.h + 2.0 * m) as usize);
                widgets::panel_filled(&mut c, Rect::new(m, m, r.w, r.h), s, t, u.menu_bg);
                for (i, (row, text)) in rows.iter().zip(&cb.items).enumerate() {
                    let rr = Rect::new(row.x - r.x + m, row.y - r.y + m, row.w, row.h);
                    let hot = self.hover == Some(Target::Item(i))
                        || (self.hover.is_none() && cb.sel == Some(i));
                    if hot {
                        let mut p = Path::new();
                        p.rounded_rect(
                            rr.x + 4.0 * s,
                            rr.y + s,
                            rr.w - 8.0 * s,
                            rr.h - 2.0 * s,
                            4.0 * s,
                        );
                        c.fill(&p, u.hover);
                    }
                    let cap = regular.map_or(small * 0.7, |f| f.cap_height(small));
                    let mut x = rr.x + t.size.field_pad * s;
                    if let Some(Some(icon)) = cb.icons.get(i) {
                        let iy = rr.y + (rr.h - icon.height as f32) * 0.5;
                        c.blit(icon, x as i32, iy as i32);
                        x += icon.width as f32 + 8.0 * s;
                    }
                    label(
                        &mut c,
                        regular,
                        text,
                        small,
                        x,
                        rr.y + (rr.h + cap) * 0.5,
                        u.text,
                    );
                }
                Some((c, (r.x - m) as i32, (r.y - m) as i32))
            }
            Popup::Picker(p) => {
                let mut p = p.clone();
                let l = self.picker_layout(&p, t, w);
                let r = l.rect;
                let mut c = Canvas::new((r.w + 2.0 * m) as usize, (r.h + 2.0 * m) as usize);
                let at = |q: Rect| Rect::new(q.x - r.x + m, q.y - r.y + m, q.w, q.h);
                widgets::panel_filled(&mut c, Rect::new(m, m, r.w, r.h), s, t, u.menu_bg);
                let side = l.sv.w.round() as usize;
                if p.sv
                    .as_ref()
                    .is_none_or(|(h, n, _)| *h != p.hue || *n != side)
                {
                    p.sv = Some((p.hue, side, widgets::sv_field(side, side, p.hue)));
                }
                if p.hue_img.as_ref().is_none_or(|(n, _)| *n != side) {
                    let hw = l.hue.w.round() as usize;
                    p.hue_img = Some((side, widgets::hue_bar(hw, side)));
                }
                let sv = at(l.sv);
                if let Some((_, _, img)) = &p.sv {
                    c.blit(img, sv.x as i32, sv.y as i32);
                }
                let hb = at(l.hue);
                if let Some((_, img)) = &p.hue_img {
                    c.blit(img, hb.x as i32, hb.y as i32);
                }
                // Marken: Kreis im Feld, Dreieck an der Leiste
                let (mx, my) = (sv.x + p.sat * sv.w, sv.y + (1.0 - p.val) * sv.h);
                let ring_col = if p.val > 0.55 { u.field } else { u.text };
                widgets::ring(&mut c, mx, my, 5.0 * s, 1.5 * s, ring_col);
                let ty = hb.y + (1.0 - p.hue / 360.0) * hb.h;
                let mut tri = Path::new();
                tri.move_to(hb.x + hb.w + 2.0 * s, ty)
                    .line_to(hb.x + hb.w + 9.0 * s, ty - 5.0 * s)
                    .line_to(hb.x + hb.w + 9.0 * s, ty + 5.0 * s)
                    .close();
                c.fill(&tri, u.text);
                let now = self.color_of(p.target, sc, t);
                let (old, new) = (at(l.old), at(l.new));
                label(
                    &mut c,
                    regular,
                    "vorher",
                    small,
                    old.x,
                    old.y - 6.0 * s,
                    u.text_dim,
                );
                widgets::swatch(
                    &mut c,
                    old,
                    p.before,
                    self.hover == Some(Target::PickOld),
                    s,
                    t,
                );
                label(
                    &mut c,
                    regular,
                    "jetzt",
                    small,
                    new.x,
                    new.y - 6.0 * s,
                    u.text_dim,
                );
                widgets::swatch(&mut c, new, now, false, s, t);
                let cap = regular.map_or(small * 0.7, |f| f.cap_height(small));
                for (i, (f, name)) in [
                    (FieldId::PickR, "R"),
                    (FieldId::PickG, "G"),
                    (FieldId::PickB, "B"),
                ]
                .into_iter()
                .enumerate()
                {
                    let fr = at(l.rgb[i]);
                    label(
                        &mut c,
                        regular,
                        name,
                        small,
                        fr.x - 14.0 * s,
                        fr.y + (fr.h + cap) * 0.5,
                        u.text_dim,
                    );
                    let v = [now.0, now.1, now.2][i].to_string();
                    let st = self.field_state(f, &v, "");
                    widgets::field(&mut c, fonts, fr, &st, s, t);
                }
                let hx = at(l.hex);
                label(
                    &mut c,
                    regular,
                    "Hex",
                    small,
                    hx.x - 38.0 * s,
                    hx.y + (hx.h + cap) * 0.5,
                    u.text_dim,
                );
                let hexv = to_hex([now.0, now.1, now.2]);
                let st = self.field_state(FieldId::PickHex, &hexv, "");
                widgets::text_field(&mut c, fonts, hx, &st, s, t);
                let ry = at(Rect::new(0.0, l.recent_y, 0.0, 0.0)).y;
                label(
                    &mut c,
                    regular,
                    "Zuletzt",
                    small,
                    sv.x,
                    ry + 11.0 * s,
                    u.text_dim,
                );
                for (i, (rr, col)) in l.recent.iter().zip(&self.recent).enumerate() {
                    widgets::swatch(
                        &mut c,
                        at(*rr),
                        *col,
                        self.hover == Some(Target::PickRecent(i)),
                        s,
                        t,
                    );
                }
                let _ = bold;
                self.popup = Some(Popup::Picker(p));
                return Some((c, (r.x - m) as i32, (r.y - m) as i32));
            }
            Popup::Confirm(focus) => {
                let (r, b) = self.confirm_layout(t, w);
                let mut c = Canvas::new((r.w + 2.0 * m) as usize, (r.h + 2.0 * m) as usize);
                widgets::panel(&mut c, Rect::new(m, m, r.w, r.h), s, t);
                let q = match self.tab {
                    Tab::Pens => "Alle Stifte auf Standard zurücksetzen?",
                    Tab::LineTypes => "Alle Linientypen auf Standard zurücksetzen?",
                    Tab::Fills => "Alle Schraffuren auf Standard zurücksetzen?",
                    Tab::Surfaces => "Alle Oberflächen auf Standard zurücksetzen?",
                    Tab::Materials => "Baustoffdarstellung auf Standard zurücksetzen?",
                    Tab::Ui => "Bedienoberfläche auf Standard zurücksetzen?",
                };
                label(
                    &mut c,
                    bold,
                    q,
                    t.size.font_title * s,
                    m + 20.0 * s,
                    m + 36.0 * s,
                    u.text,
                );
                for (i, (br, text)) in b.iter().zip(["Zurücksetzen", "Abbrechen"]).enumerate() {
                    let rr = Rect::new(br.x - r.x + m, br.y - r.y + m, br.w, br.h);
                    let st = ButtonState {
                        hover: self.hover == Some(Target::Confirm(i)),
                        pressed: self.pressed == Some(Target::Confirm(i)),
                        active: *focus == i,
                        disabled: false,
                    };
                    widgets::button(&mut c, fonts, rr, text, st, s, t);
                }
                Some((c, (r.x - m) as i32, (r.y - m) as i32))
            }
        };
        self.popup = Some(popup);
        res
    }
}

/// Zugänge für die Abnahmetests (am Dateiende, damit die Prüfung auf
/// Farbliterale die ganze Datei sieht).
#[cfg(test)]
impl Prefs {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Eine Eingabe auf der Projektseite (wirkt sofort).
    pub fn edit(&mut self, s: &mut Scene, f: impl FnOnce(&mut Model) -> bool) -> bool {
        s.edit_attr(f)
    }

    /// Neuer Stift (Knopf „Neu“): höchste Nummer + 1, schwarz, 0,25 mm.
    pub fn new_pen(&mut self, s: &mut Scene) -> PenId {
        self.add_pen(s, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Der Farbwähler bleibt im Einstellungsfenster, auch wenn sein Farbfeld
    /// am rechten oder unteren Rand liegt (Abnahme E5).
    #[test]
    fn farbwaehler_bleibt_im_fenster() {
        let mut s = Scene::new();
        let th = Theme::dark();
        let w = Win {
            w: 1600,
            h: 1000,
            top: 32,
            scale: 1.0,
        };
        let mut p = Prefs::open(&mut s, &th);
        let f = p.frame(&th, &w);
        for anchor in [
            Rect::new(f.x + f.w - 30.0, f.y + 100.0, 24.0, 18.0),
            Rect::new(f.x + f.w - 30.0, f.y + f.h - 24.0, 24.0, 18.0),
        ] {
            p.popup = Some(Popup::Picker(Box::new(Picker {
                target: ColorTarget::Accent,
                before: Rgba(0, 0, 0, 255),
                hue: 0.0,
                sat: 0.0,
                val: 0.0,
                anchor,
                sv: None,
                hue_img: None,
            })));
            let Some(Popup::Picker(pk)) = &p.popup else {
                unreachable!()
            };
            let r = p.picker_rect(pk, &th, &w);
            assert!(r.x >= f.x && r.x + r.w <= f.x + f.w + 0.5, "{r:?} in {f:?}");
            assert!(r.y >= f.y && r.y + r.h <= f.y + f.h + 0.5, "{r:?} in {f:?}");
        }
    }
}

//! Mengenermittlung im zweiten Fenster (B7): die Liste aus
//! [`sk_model::qto::schedule`] als Blatt, mit Auf- und Zuklappen, Hover und
//! Auswahl über den gemeinsamen Zustand [`Picking`], Aufleuchten geänderter
//! Werte und „Als Tabelle speichern“ (.csv). Entf und Rechtsklick löschen
//! wie im Hauptfenster (H119); gelöschte Zeilen blenden aus, die übrigen
//! rücken nach.
//!
//! Die Liste wird nie hier berechnet: [`Scene::schedule`] rechnet einmal je
//! Modellstand. Hover und Auswahl setzen nur das Blatt neu.

use crate::delete::{self, Link};
use crate::picking::Picking;
use crate::scene::Scene;
use crate::umfang_view;
use sk_model::qto::{BuildingQto, ElementQto, GroupQto, LayerRow, Schedule, StoreyQto, Umfang};
use sk_model::{Category, Deleted, ElementId, LevelKind, Model};
use sk_paint::font::Font;
use sk_paint::{Canvas, Path, Rgba};
use sk_ui::theme::Theme;
use sk_ui::widgets::{Fonts, Rect};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Band einer sichtbaren Zeile: Fensterpixel (von, bis) und Farbe mit
/// Akzentstrich, `None` ohne Band.
pub type RowBand = (i32, i32, Option<(Rgba, bool)>);

/// Kopf über der Liste (dip ab Unterkante der Titelleiste): Titelzeile,
/// Unterzeile, Geschoss-Chips (KA-1), Spaltenköpfe und Linie; bleibt beim
/// Rollen stehen.
const HEAD: f32 = 92.0 + CHIP_ROW;
/// Zeile der Geschoss-Chips (KA-1); Oberkante 62 dip unter der Titelleiste
/// (dazu die Zeile des Umschalters).
const CHIP_ROW: f32 = umfang_view::ROW;
const CHIP_TOP: f32 = 62.0;
/// Höhe einer Geschosszeile und Luft davor (dip).
const STOREY_ROW: f32 = 24.0;
const STOREY_GAP: f32 = 8.0;
/// Kacheln der Summe nach Baustoff (dip).
const TILE_H: f32 = 64.0;
const TILE_GAP: f32 = 12.0;
/// Innenrand der Kachel, Schrift ihres Namens und Luft unter den Kacheln
/// (dip). Die schmalste Kachel steht in `size.sheet_tile_min_w`.
const TILE_PAD: f32 = 10.0;
const TILE_NAME_PX: f32 = 10.0;
const TILE_AFTER: f32 = 8.0;
/// Knopf „Als Tabelle speichern“ (dip).
const BUTTON_H: f32 = 28.0;
const BUTTON_PAD: f32 = 16.0;
/// Doppelklick: höchstens so lange zwischen zwei Klicks.
const DOUBLE_MS: u128 = 450;
/// Zeitkonstante des weichen Rollens (s).
const SCROLL_SMOOTHING: f32 = 0.07;
/// Rechte Kanten der Zahlenspalten und linke Kante der Nummer, als Anteil
/// der Inhaltsbreite.
const COL_NR: f32 = 0.405;
const COL_LEN: f32 = 0.69;
const COL_AREA: f32 = 0.85;

/// Gliederung der Liste (Paket 1b): nach Geschoss (Standard) oder nach
/// Gewerk. Gemerkt in `einstellungen.txt`, nie in der Datei, ohne Rückgängig.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grouping {
    #[default]
    Storey,
    Trade,
}

impl Grouping {
    pub const ALL: [Grouping; 2] = [Grouping::Storey, Grouping::Trade];

    pub fn label(self) -> &'static str {
        match self {
            Grouping::Storey => "Geschoss",
            Grouping::Trade => "Gewerk",
        }
    }

    /// Wert in `einstellungen.txt`.
    pub fn key(self) -> &'static str {
        match self {
            Grouping::Storey => "geschoss",
            Grouping::Trade => "gewerk",
        }
    }

    pub fn from_key(k: &str) -> Option<Grouping> {
        Grouping::ALL.into_iter().find(|g| g.key() == k)
    }
}

/// Umschalter „Gliedern nach“: Abstände (dip).
const TOGGLE_PAD: f32 = 12.0;
const TOGGLE_INSET: f32 = 3.0;
/// Eigene Zeile des Umschalters unter der Unterzeile (dip): der Abstand
/// Unterzeile–Spaltenkopf.
const TOGGLE_ROW: f32 = 27.0;

/// Schlüssel einer Zeile: zum Auf- und Zuklappen und zum Wiedererkennen nach
/// einer Neuberechnung (Aufleuchten).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
enum Key {
    #[default]
    None,
    Building(u32),
    Storey(u32),
    /// Geschoss, Bauteilart, Aufbau: bleibt, wenn davor eine Gruppe wegfällt.
    Group(u32, u8, u32),
    Element(u32, u32),
    Control(u32, u8, u32),
    /// Kontrollzeile einer Wandgruppe (wie [`Key::Group`]).
    GroupControl(u32, u8, u32),
    Tile(u32, u32),
    /// Nach Gewerk: Gebäude, Reihe des Gewerks.
    Trade(u32, u32),
    /// Schichtgruppe: Gebäude und Reihe (je 16 Bit), Bauteilart, Baustoff,
    /// Dicke in 1/100 mm.
    Layer(u32, u8, u32, u32),
    /// Bauteil mit Schicht.
    LayerRow(u32, u32, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Building,
    Storey,
    Group,
    Row,
    Control,
    Rule,
    SumHead,
    Tiles,
}

/// Zelle: Bauteil, Nr., Länge · Stück, Fläche, Volumen.
const CELLS: usize = 5;

#[derive(Clone, Debug)]
struct Tile {
    key: Key,
    name: String,
    value: String,
    extra: Option<String>,
}

#[derive(Clone, Debug)]
struct Line {
    kind: Kind,
    depth: u8,
    key: Key,
    /// Sichtbar, solange dieses Geschoss offen ist …
    storey: Key,
    /// … und diese Gruppe (Wände).
    group: Key,
    elements: Vec<ElementId>,
    cells: [String; CELLS],
    /// Grund, warum kein Körper entsteht, oder Versatz einer gestapelten
    /// Wand (grau, kursiv).
    note: Option<String>,
    /// Gedimmter Zusatz hinter dem Namen („DIN 18331“).
    tag: Option<String>,
    tiles: Vec<Tile>,
}

impl Line {
    fn new(kind: Kind, depth: u8, key: Key) -> Line {
        Line {
            kind,
            depth,
            key,
            storey: Key::None,
            group: Key::None,
            elements: Vec::new(),
            cells: Default::default(),
            note: None,
            tag: None,
            tiles: Vec::new(),
        }
    }
}

/// Was unter der Maus liegt.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hot {
    Line(usize),
    /// Dreieck einer Gruppenzeile (klappt auf und zu).
    Toggle(usize),
    Button,
    /// Eine Hälfte des Umschalters „Gliedern nach“.
    Grouping(Grouping),
    /// Umfangsleiste: Gebäudefeld, Chips, „Alle“ (KA-1).
    Umfang(umfang_view::Hot),
}

/// Was ein Ereignis in der Liste für die App bedeutet.
#[derive(Clone, Debug, PartialEq)]
pub enum ListOut {
    /// Hover oder Auswahl im gemeinsamen Zustand haben sich geändert.
    Picking { selection: bool },
    /// Doppelklick: die aktive Ansicht holt diese Bauteile ins Bild.
    Zoom(Vec<ElementId>),
    /// „Als Tabelle speichern“.
    SaveCsv,
    /// Gliederung umgeschaltet (in den Einstellungen merken).
    Grouping(Grouping),
    /// Nur das Blatt neu zeichnen.
    Repaint,
    /// Preisblatt im Reiter Kosten: über `Scene::kosten_folge` schreiben.
    Kosten(crate::kosten_view::Schreiben),
    /// Aus dem Prüfen des AVA: Reiter Kosten, zur grauen Zeile dieses
    /// Bauteils und „Bauleistung wählen …“ (Bedienbarkeit 12.2).
    WaehlenIm(ElementId),
    /// „Bauleistung öffnen ↗“ im AVA: Verwaltung mit dieser Bauleistung.
    Verwaltung(sk_model::Guid),
    /// „Kennwort eingeben …“ im Preis- oder Lohnblatt: nur die Abfrage
    /// (Bedienbarkeit 16.1).
    Kennwort,
    /// Ablauf `kind=user` für dieses Haus starten (paket-ka3b §3).
    Ablauf(sk_model::Guid),
    /// AVA-Kopf: Maske „Projektdaten“ mit dem Cursor in diesem Feld
    /// (Paket PD-3).
    Projektdaten(usize),
}

/// Nachrücken nach dem Löschen (H119): weggefallene Zeilen blenden in
/// `fade_ms` aus, danach rücken die übrigen in `anim_ms` an ihre neue Lage.
struct Motion {
    start: Instant,
    /// Sichtbare Zeilen vor dem Löschen, in Listenreihenfolge.
    old: Vec<Line>,
}

/// Lage während des Nachrückens: Versatz je Zeile (Index, dip, zur neuen
/// Lage addiert) und weggefallene Zeilen (alte Oberkante, Höhe, Deckkraft).
type MotionState<'a> = (HashMap<usize, f32>, Vec<(&'a Line, f32, f32, f32)>);

/// Bauteilgruppe für Hover und Klick auf eine Gruppenzeile: Kurzname des
/// Geschosses der Gruppe, Bauteilart, Bauteile.
type GroupRef = (String, Category, Vec<ElementId>);

pub struct ListView {
    grouping: Grouping,
    /// Umfangsleiste des Blatts (KA-1): Gebäude und abgewählte Geschosse,
    /// für die Sitzung.
    pub leiste: umfang_view::Leiste,
    /// Umfang, aus dem die Zeilen stammen.
    umfang_shown: Option<Umfang>,
    /// Oberkante des Blatts (dip): unter der Titelleiste und, im
    /// Mengenfenster, unter der Kartenleiste (KA-2a).
    pub top: f32,
    lines: Vec<Line>,
    groups: Vec<GroupRef>,
    /// Alle Bauteilzeilen in Listenreihenfolge (Bereichsauswahl).
    order: Vec<ElementId>,
    numbers: HashMap<ElementId, String>,
    closed_storeys: HashSet<Key>,
    open_groups: HashSet<Key>,
    /// Stand des gemeinsamen Zustands, wie das Blatt ihn zeigt.
    hover: Vec<ElementId>,
    selected: Vec<ElementId>,
    anchor: Option<ElementId>,
    shown_primary: Option<ElementId>,
    /// Gerollt (dip) und Ziel des weichen Rollens.
    scroll: f32,
    target: f32,
    pub w: u32,
    pub h: u32,
    pub scale: f32,
    /// Berechnung der Liste, aus der die Zeilen stammen.
    runs: Option<u64>,
    /// Liste älter als das Modell („wird aktualisiert“) und seit wann.
    stale: Option<Instant>,
    /// Kopfzeile: Gebäude und Stand.
    subtitle: String,
    /// Bauteile mit Ausgeblendetem (Paket 4): steht hinter der Unterzeile.
    hidden: usize,
    /// Zellen beim letzten Aufbau und ihr Aufleuchten.
    cells_old: HashMap<(Key, u8), String>,
    flash: HashMap<(Key, u8), Instant>,
    hot: Option<Hot>,
    button_down: bool,
    last_click: Option<(Instant, usize)>,
    last_tick: Option<Instant>,
    /// Die Auswahl stammt vom Klick auf diese Gruppenzeile (Entf meldet dann
    /// „N Wände gelöscht.“ mit „Rückgängig“).
    focus_group: Option<Key>,
    /// Zuletzt eine Geschoss- oder Summenzeile angeklickt: Entf löscht nichts.
    focus_none: bool,
    /// Abgelehnte Zeilen leuchten einmal auf (`sheet_flash`).
    row_flash: HashMap<Key, Instant>,
    motion: Option<Motion>,
    /// Zeilen, unter der der Hinweis nach Entf steht: die erste, die es noch
    /// gibt.
    hint_at: Vec<Key>,
    /// Zusätzliche Kopfhöhe (dip), wenn der Umschalter eine eigene Zeile
    /// braucht (schmales Fenster, lange Unterzeile); beim Zeichnen bestimmt.
    head_extra: Cell<f32>,
    /// Maße des zuletzt gezeichneten Farbschemas: Lage und Rollgrenze ohne
    /// Schema rechnen damit (Review K3).
    sizes: Cell<Option<sk_ui::theme::Sizes>>,
}

impl ListView {
    /// Liste für das Modell der Szene (berechnet sie, falls nötig).
    #[cfg(test)]
    pub fn new(s: &mut Scene) -> ListView {
        ListView::grouped(s, Grouping::Storey)
    }

    /// Liste mit Gliederung `g`.
    pub fn grouped(s: &mut Scene, g: Grouping) -> ListView {
        let mut v = ListView {
            grouping: g,
            leiste: umfang_view::Leiste::default(),
            umfang_shown: None,
            top: 32.0,
            lines: Vec::new(),
            groups: Vec::new(),
            order: Vec::new(),
            numbers: HashMap::new(),
            closed_storeys: HashSet::new(),
            open_groups: HashSet::new(),
            hover: Vec::new(),
            selected: Vec::new(),
            anchor: None,
            shown_primary: None,
            scroll: 0.0,
            target: 0.0,
            w: 520,
            h: 800,
            scale: 1.0,
            runs: None,
            stale: None,
            subtitle: String::new(),
            hidden: 0,
            cells_old: HashMap::new(),
            flash: HashMap::new(),
            hot: None,
            button_down: false,
            last_click: None,
            last_tick: None,
            focus_group: None,
            focus_none: false,
            row_flash: HashMap::new(),
            motion: None,
            hint_at: Vec::new(),
            head_extra: Cell::new(0.0),
            sizes: Cell::new(None),
        };
        v.sync(s, false);
        v
    }

    /// An den Modellstand angleichen. `true`, wenn neu gezeichnet werden muss.
    /// `animate`: geänderte Werte leuchten auf.
    pub fn sync(&mut self, s: &mut Scene, animate: bool) -> bool {
        if self.leiste.sync(s.model()) {
            self.umfang_shown = None;
        }
        let sched = s.schedule_in(&self.leiste.umfang);
        let runs = s.schedule_runs();
        let mut changed = false;
        let umfang_neu = self.umfang_shown.as_ref() != Some(&self.leiste.umfang);
        if self.runs != Some(runs) || umfang_neu {
            // Ein anderer Umfang ist kein geänderter Wert: nichts leuchtet
            let animate = animate && self.runs.is_some() && !umfang_neu;
            self.rebuild(s.model(), &sched, animate);
            self.runs = Some(runs);
            self.umfang_shown = Some(self.leiste.umfang.clone());
            changed = true;
        }
        let hidden = sk_model::tree::hidden_count(s.model());
        if hidden != self.hidden {
            self.hidden = hidden;
            changed = true;
        }
        let stale = s.schedule_stale();
        if stale != self.stale.is_some() {
            self.stale = stale.then(Instant::now);
            changed = true;
        }
        changed
    }

    /// Unterzeile: Gebäude und Stand, dahinter „2 Bauteile ausgeblendet“,
    /// wenn im Modell etwas ausgeblendet ist (die Mengen bleiben ganz).
    fn subtitle_text(&self) -> String {
        match self.hidden {
            0 => self.subtitle.clone(),
            1 => format!("{} · 1 Bauteil ausgeblendet", self.subtitle),
            n => format!("{} · {n} Bauteile ausgeblendet", self.subtitle),
        }
    }

    /// Gliederung umschalten; die Zeilen baut das nächste [`ListView::sync`]
    /// neu (ohne Aufleuchten). `true`, wenn sie sich ändert.
    pub fn set_grouping(&mut self, g: Grouping) -> bool {
        if g == self.grouping {
            return false;
        }
        self.grouping = g;
        self.runs = None;
        self.motion = None;
        self.hint_at.clear();
        self.scroll = 0.0;
        self.target = 0.0;
        true
    }

    /// Zeilen neu aufbauen; Auf- und Zuklappen bleibt erhalten.
    fn rebuild(&mut self, m: &Model, sched: &Schedule, animate: bool) {
        let (lines, groups) = build_lines(m, sched, self.grouping);
        // Fallen sichtbare Zeilen weg, rücken die übrigen sichtbar nach
        let keys: HashSet<Key> = lines.iter().map(|l| l.key).collect();
        let old: Vec<Line> = self
            .lines
            .iter()
            .filter(|l| self.line_visible(l))
            .cloned()
            .collect();
        let gone = old
            .iter()
            .any(|l| l.key != Key::None && !keys.contains(&l.key));
        self.motion = (animate && gone).then(|| Motion {
            start: Instant::now(),
            old,
        });
        self.lines = lines;
        self.groups = groups;
        self.order = self
            .lines
            .iter()
            .filter(|l| l.kind == Kind::Row)
            .flat_map(|l| l.elements.first().copied())
            .collect();
        self.numbers = self
            .order
            .iter()
            .filter_map(|&e| Some((e, m.element(e)?.number.clone())))
            .collect();
        self.subtitle =
            umfang_view::umfang_text(m, &self.leiste.umfang, sk_platform::local_date_time());
        // Geänderte Werte merken
        let mut now_cells = HashMap::new();
        for l in &self.lines {
            for (i, c) in l.cells.iter().enumerate().skip(2) {
                if !c.is_empty() {
                    now_cells.insert((l.key, i as u8), c.clone());
                }
            }
            for t in &l.tiles {
                now_cells.insert((t.key, 0), t.value.clone());
                if let Some(x) = &t.extra {
                    now_cells.insert((t.key, 1), x.clone());
                }
            }
        }
        if animate {
            let now = Instant::now();
            for (k, v) in &now_cells {
                if self.cells_old.get(k).is_some_and(|o| o != v) {
                    self.flash.insert(*k, now);
                }
            }
        }
        self.cells_old = now_cells;
        self.clamp();
    }

    // --- Sichtbarkeit und Lage -------------------------------------------

    fn line_visible(&self, l: &Line) -> bool {
        (l.storey == Key::None || !self.closed_storeys.contains(&l.storey))
            && (l.group == Key::None || self.open_groups.contains(&l.group))
    }

    /// Maße aus `t`, sonst die des zuletzt gezeichneten Schemas.
    fn sizes(&self, t: Option<&Theme>) -> Option<sk_ui::theme::Sizes> {
        t.map(|t| t.size).or(self.sizes.get())
    }

    fn line_h(&self, l: &Line, t: Option<&Theme>) -> f32 {
        let row = self.sizes(t).map_or(22.0, |z| z.qto_row);
        match l.kind {
            Kind::Building => 30.0,
            Kind::Storey => STOREY_ROW + STOREY_GAP,
            Kind::Group | Kind::Row | Kind::Control => row,
            Kind::Rule => 16.0,
            Kind::SumHead => 30.0,
            Kind::Tiles => {
                let rows = l
                    .tiles
                    .len()
                    .div_ceil(self.tile_slots(t, l.tiles.len()))
                    .max(1);
                rows as f32 * (TILE_H + TILE_GAP) + TILE_AFTER
            }
        }
    }

    /// Kacheln je Zeile der Summe nach Baustoff: mindestens drei Plätze
    /// (wenige Kacheln bleiben ein Drittel breit), höchstens so viele, wie
    /// mit `size.sheet_tile_min_w` in die Inhaltsbreite passen.
    fn tile_slots(&self, t: Option<&Theme>, n: usize) -> usize {
        let z = self.sizes(t);
        let pad = z.map_or(28.0, |z| z.sheet_pad);
        let max_w = z.map_or(900.0, |z| z.qto_max_w);
        let cw = (self.w as f32 / self.scale - 2.0 * pad).min(max_w).max(0.0);
        let min_w = z.map_or(120.0, |z| z.sheet_tile_min_w);
        let fit = ((cw + TILE_GAP) / (min_w + TILE_GAP)).floor().max(1.0) as usize;
        n.max(3).min(fit)
    }

    /// Sichtbare Zeilen mit Oberkante und Höhe (dip, ab Listenanfang).
    fn layout(&self, t: Option<&Theme>) -> Vec<(usize, f32, f32)> {
        let mut y = 0.0;
        let mut out = Vec::new();
        for (i, l) in self.lines.iter().enumerate() {
            if !self.line_visible(l) {
                continue;
            }
            let h = self.line_h(l, t);
            out.push((i, y, h));
            y += h;
        }
        out
    }

    fn content_h(&self, t: Option<&Theme>) -> f32 {
        self.layout(t).last().map_or(0.0, |(_, y, h)| y + h)
    }

    /// Höhe des Listenbereichs (dip).
    fn view_h(&self) -> f32 {
        (self.h as f32 / self.scale - self.top_dip() - self.head()).max(0.0)
    }

    /// Oberkante des Blatts (dip).
    fn top_dip(&self) -> f32 {
        self.top
    }

    /// Höhe des Kopfs über der Liste (dip).
    fn head(&self) -> f32 {
        HEAD + self.head_extra.get()
    }

    /// Bestimmt die Kopfhöhe nach der Lage des Umschalters.
    fn fit_head(&self, t: &Theme, fonts: &Fonts) {
        self.sizes.set(Some(t.size));
        let own = self.toggle_layout(t, fonts).2;
        self.head_extra.set(if own { TOGGLE_ROW } else { 0.0 });
    }

    fn clamp(&mut self) {
        let max = (self.content_h(None) - self.view_h() + 24.0).max(0.0);
        self.target = self.target.clamp(0.0, max);
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// Klappt Geschoss und Gruppe des Bauteils auf und rollt es ins Bild.
    fn reveal(&mut self, id: ElementId) {
        let Some(i) = self
            .lines
            .iter()
            .position(|l| l.kind == Kind::Row && l.elements.first() == Some(&id))
        else {
            return;
        };
        let (st, g) = (self.lines[i].storey, self.lines[i].group);
        self.closed_storeys.remove(&st);
        if g != Key::None {
            self.open_groups.insert(g);
        }
        let Some(&(_, y, h)) = self.layout(None).iter().find(|x| x.0 == i) else {
            return;
        };
        let view = self.view_h();
        if y < self.target {
            self.target = (y - h).max(0.0);
        } else if y + h > self.target + view {
            self.target = y + 2.0 * h - view;
        }
        self.clamp();
    }

    // --- Gemeinsamer Zustand ---------------------------------------------

    /// Die Liste folgt dem gemeinsamen Zustand (Auswahl oder Hover in einer
    /// Ansicht des Hauptfensters). `true`, wenn neu gezeichnet werden muss.
    pub fn follow(&mut self, _s: &mut Scene, p: &Picking) -> bool {
        let hover: Vec<ElementId> = p.hovered().collect();
        if p.selected != self.selected {
            self.focus_group = None;
            self.focus_none = false;
        }
        let mut changed = hover != self.hover || p.selected != self.selected;
        self.hover = hover;
        self.selected = p.selected.clone();
        let prim = p.primary();
        if prim != self.shown_primary {
            self.shown_primary = prim;
            if let Some(id) = prim {
                self.reveal(id);
                changed = true;
            }
        }
        changed
    }

    #[cfg(test)]
    fn by_number(&self, s: &Scene, nr: &str) -> Option<ElementId> {
        s.model().element_by_number(nr)
    }

    #[cfg(test)]
    fn group(&self, gs: &str, art: Category) -> Vec<ElementId> {
        self.groups
            .iter()
            .filter(|g| g.0 == gs && g.1 == art)
            .flat_map(|g| g.2.iter().copied())
            .collect()
    }

    #[cfg(test)]
    /// Maus über der Zeile eines Bauteils.
    pub fn hover_row(&mut self, s: &mut Scene, p: &mut Picking, nr: &str) {
        let id = self.by_number(s, nr);
        p.set_hover(id, Vec::new());
        self.hover = id.into_iter().collect();
    }

    #[cfg(test)]
    /// Maus über einer Gruppenzeile (alle Bauteile der Gruppe).
    pub fn hover_group(&mut self, _s: &mut Scene, p: &mut Picking, gs: &str, art: Category) {
        let g = self.group(gs, art);
        p.set_hover(None, g.clone());
        self.hover = g;
    }

    #[cfg(test)]
    /// Klick auf eine Zeile: ersetzt die Auswahl; Strg fügt hinzu oder nimmt
    /// weg, Umschalt wählt den Bereich ab dem letzten Klick.
    pub fn click_row(&mut self, s: &mut Scene, p: &mut Picking, nr: &str, ctrl: bool, shift: bool) {
        let Some(id) = self.by_number(s, nr) else {
            return;
        };
        self.click_element(p, id, ctrl, shift);
    }

    fn click_element(&mut self, p: &mut Picking, id: ElementId, ctrl: bool, shift: bool) {
        self.focus_group = None;
        self.focus_none = false;
        let range = shift
            .then(|| {
                let a = self.order.iter().position(|e| Some(*e) == self.anchor)?;
                let b = self.order.iter().position(|e| *e == id)?;
                Some(self.order[a.min(b)..=a.max(b)].to_vec())
            })
            .flatten();
        match range {
            Some(r) if ctrl => {
                for e in r {
                    if !p.selected.contains(&e) {
                        p.selected.push(e);
                    }
                }
            }
            Some(r) => p.selected = r,
            None => {
                // Wie beim Loslassen im Hauptfenster (A139)
                match crate::selection::release_pick(None, Some(Some(id)), ctrl, false, &p.selected)
                {
                    crate::selection::PickChange::Keep => {}
                    crate::selection::PickChange::Replace(e) => {
                        p.select_only(e);
                    }
                    crate::selection::PickChange::Add(e)
                    | crate::selection::PickChange::Remove(e) => p.click(e, true),
                }
                self.anchor = Some(id);
            }
        }
        // Selbst gewählt: nicht wegrollen
        self.shown_primary = p.primary();
        self.selected = p.selected.clone();
    }

    #[cfg(test)]
    /// Klick auf eine Gruppenzeile: wählt alle ihre Bauteile.
    pub fn click_group(&mut self, _s: &mut Scene, p: &mut Picking, gs: &str, art: Category) {
        let g = self.group(gs, art);
        let key = self
            .lines
            .iter()
            .find(|l| l.kind == Kind::Group && l.elements == g)
            .map(|l| l.key);
        self.select_all(p, g, key);
    }

    fn select_all(&mut self, p: &mut Picking, g: Vec<ElementId>, key: Option<Key>) {
        self.focus_group = key;
        self.focus_none = false;
        self.anchor = g.first().copied();
        p.selected = g;
        self.shown_primary = p.primary();
        self.selected = p.selected.clone();
    }

    /// Esc: Auswahl in beiden Fenstern aufheben.
    #[cfg(test)]
    pub fn escape(&mut self, _s: &mut Scene, p: &mut Picking) {
        self.clear_selection(p);
    }

    pub fn clear_selection(&mut self, p: &mut Picking) {
        self.focus_group = None;
        self.focus_none = false;
        p.selected.clear();
        self.selected.clear();
        self.shown_primary = None;
    }

    // --- Löschen (H119) ----------------------------------------------------

    /// Entf löscht: Es ist etwas gewählt, und zuletzt wurde keine Geschoss-
    /// oder Summenzeile angeklickt.
    pub fn part_selected(&self, p: &Picking) -> bool {
        !p.selected.is_empty() && !self.focus_none
    }

    /// Entf, während das Mengenfenster vorne ist: löscht die gemeinsame
    /// Auswahl nach denselben Regeln wie im Hauptfenster (ein Schritt) und
    /// gibt den Hinweis unter der Zeile Zeile für Zeile zurück (leer: kein
    /// Hinweis).
    #[cfg(test)]
    pub fn delete_key(&mut self, s: &mut Scene, p: &mut Picking) -> Vec<String> {
        if !self.part_selected(p) {
            return vec![delete::NO_PART.to_string()];
        }
        let ids = p.selected.clone();
        let d = s.delete_elements(&ids);
        self.erased(s, &d, p, Instant::now()).0
    }

    /// Nach dem Löschen aus der Liste: Hinweis (Zeilen und Verweis), Zeile
    /// dafür, Aufleuchten der abgelehnten Zeilen, bereinigte Auswahl. Eine
    /// Gruppenzeile meldet immer „N Wände gelöscht.“ mit „Rückgängig“.
    pub fn erased(
        &mut self,
        s: &Scene,
        d: &Deleted,
        p: &mut Picking,
        now: Instant,
    ) -> (Vec<String>, Option<(&'static str, Link)>) {
        let m = s.model();
        let group = self.focus_group.filter(|_| !d.removed.is_empty());
        let lines = if group.is_some() && d.refused.is_empty() {
            vec![delete::removed_line(d.removed.len())]
        } else {
            delete::hint(m, d)
        };
        let link = if group.is_some() {
            Some(("Rückgängig", Link::Undo))
        } else {
            delete::hint_link(m, d)
        };
        // Hinweis unter der abgelehnten Zeile, sonst unter der Gruppe bzw.
        // unter der Zeile, die über der gelöschten stand
        let first_removed = self
            .order
            .iter()
            .find(|e| d.removed.contains(e))
            .map(|e| self.row_key(*e));
        let anchor = match d.refused.first() {
            Some(r) if d.removed.is_empty() => Some(self.row_key(r.0)),
            _ => group.or(first_removed),
        };
        self.hint_at.clear();
        if let Some(a) = anchor {
            self.hint_at.push(a);
            if let Some(i) = self.lines.iter().position(|l| l.key == a) {
                let above: Vec<Key> = self.lines[..i]
                    .iter()
                    .rev()
                    .filter(|l| l.key != Key::None && self.line_visible(l))
                    .map(|l| l.key)
                    .collect();
                self.hint_at.extend(above);
            }
        }
        for (id, _) in &d.refused {
            self.row_flash.insert(self.row_key(*id), now);
        }
        p.validate(s);
        self.hover = p.hovered().collect();
        self.selected = p.selected.clone();
        self.shown_primary = p.primary();
        if !d.removed.is_empty() {
            self.focus_group = None;
        }
        (lines, link)
    }

    /// Schlüssel der ersten Zeile des Bauteils (nach Gewerk steht eine Wand
    /// in mehreren Gewerken).
    fn row_key(&self, e: ElementId) -> Key {
        self.lines
            .iter()
            .find(|l| l.kind == Kind::Row && l.elements.first() == Some(&e))
            .map_or(elem_key(e), |l| l.key)
    }

    /// Rechtsklick auf eine Zeile: wählt sie, falls sie es noch nicht ist,
    /// und gibt das Bauteil für das Menü und die Bauteile der Zeile („Im
    /// Modell zeigen“). `None` auf Geschoss- und Summenzeilen.
    pub fn context_at(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        p: &mut Picking,
        x: f64,
        y: f64,
    ) -> Option<(ElementId, Vec<ElementId>)> {
        let (Hot::Line(i) | Hot::Toggle(i)) = self.hit(t, fonts, x, y)? else {
            return None;
        };
        let l = self.lines[i].clone();
        let first = *l.elements.first()?;
        match l.kind {
            Kind::Row => {
                if !p.selected.contains(&first) {
                    self.click_element(p, first, false, false);
                }
            }
            Kind::Group => {
                if p.selected != l.elements {
                    self.select_all(p, l.elements.clone(), Some(l.key));
                }
            }
            _ => return None,
        }
        self.focus_none = false;
        Some((first, l.elements))
    }

    /// Rechteck (Fensterpixel) der Zeile, unter der der Hinweis nach Entf
    /// steht; `None`: unten in der Mitte.
    pub fn hint_rect(&self, t: &Theme) -> Option<Rect> {
        let s = self.scale;
        let layout = self.layout(Some(t));
        let (_, y, h) = self.hint_at.iter().find_map(|k| {
            layout
                .iter()
                .find(|(i, _, _)| self.lines[*i].key == *k)
                .copied()
        })?;
        let (x0, cw) = self.content_x(t);
        let pad = 10.0 * s;
        let ys = (self.top_dip() + self.head()) * s + y * s - self.scroll_px() as f32;
        Some(Rect::new(x0 - pad, ys, cw + 2.0 * pad, h * s))
    }

    /// Lage während des Nachrückens (siehe [`MotionState`]); `None`, wenn
    /// nichts nachrückt.
    fn motion_state(&self, t: &Theme, now: Instant) -> Option<MotionState<'_>> {
        let mo = self.motion.as_ref()?;
        let (fade, anim) = (t.size.fade_ms.max(0.0), t.size.anim_ms);
        if anim <= 0.0 {
            return None;
        }
        let e = now.saturating_duration_since(mo.start).as_secs_f32() * 1000.0;
        if e >= fade + anim {
            return None;
        }
        let ghost_a = if fade > 0.0 { 1.0 - e / fade } else { 0.0 };
        let k = if e < fade {
            1.0
        } else {
            let x = ((e - fade) / anim).clamp(0.0, 1.0);
            1.0 - x * x * (3.0 - 2.0 * x)
        };
        let mut old_y: HashMap<Key, f32> = HashMap::new();
        let mut ghosts = Vec::new();
        let new_keys: HashSet<Key> = self.lines.iter().map(|l| l.key).collect();
        let mut y = 0.0;
        for l in &mo.old {
            let h = self.line_h(l, Some(t));
            if l.key != Key::None {
                old_y.insert(l.key, y);
                if !new_keys.contains(&l.key) && ghost_a > 0.0 {
                    ghosts.push((l, y, h, ghost_a));
                }
            }
            y += h;
        }
        let mut shift = HashMap::new();
        let mut last = 0.0;
        for (i, y, _) in self.layout(Some(t)) {
            if let Some(oy) = old_y.get(&self.lines[i].key) {
                last = oy - y;
            }
            shift.insert(i, last * k);
        }
        Some((shift, ghosts))
    }

    /// Deckkraft des Aufleuchtens einer abgelehnten Zeile.
    fn row_flash_alpha(&self, key: Key, t: &Theme, now: Instant, anim: bool) -> Option<f32> {
        if !anim {
            return None;
        }
        let at = self.row_flash.get(&key)?;
        let k = now.duration_since(*at).as_secs_f32() * 1000.0 / t.size.flash_ms.max(1.0);
        (k < 1.0).then_some(1.0 - k * k)
    }

    #[cfg(test)]
    /// Ist die Zeile des Bauteils sichtbar (aufgeklappt und im Bild)?
    pub fn row_visible(&self, nr: &str) -> bool {
        let view = self.view_h();
        self.layout(None).iter().any(|&(i, y, h)| {
            let l = &self.lines[i];
            l.kind == Kind::Row
                && l.elements.first().and_then(|e| self.numbers.get(e)) == Some(&nr.to_string())
                && y >= self.target
                && y + h <= self.target + view
        })
    }

    #[cfg(test)]
    /// Alle Zeilen als Text (Einzug, Bezeichnung, Zusatz, Zellen mit „ | “),
    /// auch zugeklappte; Summenkacheln als „Baustoff: Wert“.
    pub fn line_texts(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|l| {
                let mut t = "  ".repeat(l.depth as usize) + &l.cells[0];
                if let Some(tag) = &l.tag {
                    t = format!("{t} [{tag}]");
                }
                for c in &l.cells[1..] {
                    t = format!("{t} | {c}");
                }
                for x in &l.tiles {
                    t = format!("{t} {}: {}", x.name, x.value);
                }
                t
            })
            .collect()
    }

    #[cfg(test)]
    fn rows_where(&self, f: impl Fn(ElementId) -> bool) -> Vec<String> {
        self.order
            .iter()
            .filter(|e| f(**e))
            .filter_map(|e| self.numbers.get(e).cloned())
            .collect()
    }

    #[cfg(test)]
    /// Bauteilzeilen mit Auswahlband, in Listenreihenfolge.
    pub fn selected_rows(&self) -> Vec<String> {
        self.rows_where(|e| self.selected.contains(&e))
    }

    #[cfg(test)]
    /// Bauteilzeilen mit Hover-Band, in Listenreihenfolge.
    pub fn hover_rows(&self) -> Vec<String> {
        self.rows_where(|e| self.hover.contains(&e))
    }

    // --- Bedienung im Fenster --------------------------------------------

    /// Inhaltsbreite und linker Rand (px).
    fn content_x(&self, t: &Theme) -> (f32, f32) {
        let s = self.scale;
        let pad = t.size.sheet_pad * s;
        let w = (self.w as f32 - 2.0 * pad)
            .min(t.size.qto_max_w * s)
            .max(0.0);
        (pad, w)
    }

    fn button_rect(&self, t: &Theme, fonts: &Fonts) -> (f32, f32, f32, f32) {
        let s = self.scale;
        let (x0, w) = self.content_x(t);
        let tw = fonts
            .bold
            .as_ref()
            .or(fonts.regular.as_ref())
            .map_or(130.0 * s, |f| f.width("Als Tabelle speichern", 11.0 * s));
        let bw = tw + 2.0 * BUTTON_PAD * s;
        (x0 + w - bw, (self.top_dip() + 14.0) * s, bw, BUTTON_H * s)
    }

    /// Umschalter „Gliedern nach“ (px): Lage der Beschriftung (x, Grundlinie)
    /// oder `None`, wenn sie keinen Platz hat, und die beiden Hälften. Steht
    /// rechts neben dem Titel vor „Als Tabelle speichern“, wenn Platz ist
    /// (auch für die Pille „wird aktualisiert“), sonst darunter rechtsbündig.
    #[allow(clippy::type_complexity)]
    /// Lage des Umschalters: Beschriftung, Hälften und ob er eine eigene
    /// Zeile braucht. Breit in der Titelzeile vor dem Knopf; sonst
    /// rechtsbündig auf der Unterzeile, Mitte auf Mitte; reicht auch dort
    /// der Platz nicht, darunter mit dem Abstand Unterzeile–Spaltenkopf.
    #[allow(clippy::type_complexity)]
    fn toggle_layout(
        &self,
        t: &Theme,
        fonts: &Fonts,
    ) -> (
        Option<(f32, f32)>,
        [(Grouping, (f32, f32, f32, f32)); 2],
        bool,
    ) {
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let (bx, by, _, bh) = self.button_rect(t, fonts);
        let top = self.top_dip() * s;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let px = 10.5 * s;
        let width = |f: Option<&Font>, text: &str, px: f32| {
            f.map_or(text.chars().count() as f32 * px * 0.55, |f| {
                f.width(text, px)
            })
        };
        let half = |g: Grouping| width(bold, g.label(), px) + 2.0 * TOGGLE_PAD * s;
        let inset = TOGGLE_INSET * s;
        let pw = half(Grouping::Storey) + half(Grouping::Trade) + 2.0 * inset;
        let label_w = width(regular, "Gliedern nach", px) + 10.0 * s;
        let title_end = x0
            + width(bold, "Mengenermittlung", 19.0 * s)
            + 12.0 * s
            + width(regular, "wird aktualisiert", 10.0 * s)
            + 40.0 * s
            + 16.0 * s;
        let right = bx - 12.0 * s;
        let cap = regular.map_or(7.5 * s, |f| f.cap_height(px));
        let (pill_x, pill_y, ph, label, own) = if right - pw >= title_end {
            let label = right - pw - label_w >= title_end;
            (right - pw, by, bh, label, false)
        } else {
            let ph = 20.0 * s;
            let sub_end = x0 + width(regular, &self.subtitle_text(), px) + 16.0 * s;
            let x = x0 + cw - pw;
            // Mitte der Unterzeile (Grundlinie top + 52)
            let mid = top + 52.0 * s - cap * 0.5;
            if x >= sub_end {
                // Nie unter den Knopf darüber (Jörn 08.10.: der Knopf lag
                // bei schmalem Fenster hinter dem Umschalter)
                let y = (mid - ph * 0.5).max(by + bh + 4.0 * s);
                (x, y, ph, x - label_w >= sub_end, false)
            } else {
                let y = mid + TOGGLE_ROW * s - ph * 0.5;
                (x, y, ph, x - label_w >= x0, true)
            }
        };
        let label = label.then_some((pill_x - label_w, pill_y + (ph + cap) * 0.5));
        let w0 = half(Grouping::Storey);
        let halves = [
            (
                Grouping::Storey,
                (pill_x + inset, pill_y + inset, w0, ph - 2.0 * inset),
            ),
            (
                Grouping::Trade,
                (
                    pill_x + inset + w0,
                    pill_y + inset,
                    half(Grouping::Trade),
                    ph - 2.0 * inset,
                ),
            ),
        ];
        (label, halves, own)
    }

    // --- Umfang: Gebäudefeld und Geschoss-Chips (KA-1) ---------------------

    /// Lage der Umfangsleiste unter der Unterzeile (und dem Umschalter, wenn
    /// er eine eigene Zeile hat).
    pub fn leiste_lage(&self, t: &Theme) -> umfang_view::Lage {
        let s = self.scale;
        umfang_view::Lage {
            x0: self.content_x(t).0,
            y: (self.top_dip() + CHIP_TOP + self.head_extra.get()) * s,
            s,
        }
    }

    /// Liegt etwas über der Liste (offenes Gebäudefeld) oder wackelt ein
    /// Chip? Dann zeichnet das Fenster ganz statt nur einzelner Zeilen.
    pub fn overlay_open(&self) -> bool {
        self.leiste.overlay_open()
    }

    fn hit(&self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<Hot> {
        self.fit_head(t, fonts);
        let (x, y) = (x as f32, y as f32);
        let s = self.scale;
        if let Some(h) = self.leiste.hit(fonts, self.leiste_lage(t), x, y) {
            return Some(Hot::Umfang(h));
        }
        // Über der offenen Liste des Felds liegt nichts anderes
        if self.leiste.field_open() {
            return None;
        }
        let (bx, by, bw, bh) = self.button_rect(t, fonts);
        if x >= bx && x < bx + bw && y >= by && y < by + bh {
            return Some(Hot::Button);
        }
        let inset = TOGGLE_INSET * s;
        for (g, (hx, hy, hw, hh)) in self.toggle_layout(t, fonts).1 {
            if x >= hx && x < hx + hw && y >= hy - inset && y < hy + hh + inset {
                return Some(Hot::Grouping(g));
            }
        }
        let list_top = (self.top_dip() + self.head()) * s;
        if y < list_top {
            return None;
        }
        let ly = (y - list_top) / s + self.scroll;
        let (x0, _) = self.content_x(t);
        self.layout(Some(t))
            .into_iter()
            .find(|&(_, top, h)| ly >= top && ly < top + h)
            .map(|(i, _, _)| {
                let l = &self.lines[i];
                let tri = x0 + (l.depth as f32 * t.size.qto_indent + 14.0) * s;
                if l.kind == Kind::Group && x < tri {
                    Hot::Toggle(i)
                } else {
                    Hot::Line(i)
                }
            })
    }

    /// Bauteile unter der Maus für den gemeinsamen Zustand.
    fn hover_of(&self, hot: Option<Hot>) -> (Option<ElementId>, Vec<ElementId>) {
        match hot {
            Some(Hot::Line(i)) | Some(Hot::Toggle(i)) => {
                let l = &self.lines[i];
                match l.kind {
                    Kind::Row => (l.elements.first().copied(), Vec::new()),
                    Kind::Group | Kind::Storey => (None, l.elements.clone()),
                    _ => (None, Vec::new()),
                }
            }
            _ => (None, Vec::new()),
        }
    }

    pub fn mouse_move(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        p: &mut Picking,
        x: f64,
        y: f64,
    ) -> Option<ListOut> {
        let hot = self.hit(t, fonts, x, y);
        // Nur Knopf, Umschalter und Umfangsleiste sehen anders aus, wenn die
        // Maus darübersteht
        let button = |h: Option<Hot>| match h {
            Some(Hot::Button | Hot::Grouping(_) | Hot::Umfang(_)) => h,
            _ => None,
        };
        let repaint = button(hot) != button(self.hot);
        self.hot = hot;
        self.leiste.hot = match hot {
            Some(Hot::Umfang(h)) => Some(h),
            _ => None,
        };
        let (one, group) = self.hover_of(hot);
        if p.set_hover(one, group) {
            self.hover = p.hovered().collect();
            return Some(ListOut::Picking { selection: false });
        }
        repaint.then_some(ListOut::Repaint)
    }

    pub fn mouse_leave(&mut self, p: &mut Picking) -> Option<ListOut> {
        let button = matches!(
            self.hot.take(),
            Some(Hot::Button | Hot::Grouping(_) | Hot::Umfang(_))
        ) || self.button_down;
        self.leiste.hot = None;
        self.button_down = false;
        if p.set_hover(None, Vec::new()) {
            self.hover.clear();
            return Some(ListOut::Picking { selection: false });
        }
        button.then_some(ListOut::Repaint)
    }

    pub fn mouse_down(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        p: &mut Picking,
        (x, y): (f64, f64),
        mods: sk_platform::Modifiers,
    ) -> Option<ListOut> {
        let hot = self.hit(t, fonts, x, y);
        if let Some(Hot::Umfang(h)) = hot {
            self.leiste.click(h, mods.ctrl);
            return Some(ListOut::Repaint);
        }
        // Ein Klick daneben schließt die Liste des Gebäudefelds
        if self.leiste.close_field() {
            return Some(ListOut::Repaint);
        }
        match hot {
            Some(Hot::Umfang(_)) => None,
            Some(Hot::Button) => {
                self.button_down = true;
                Some(ListOut::Repaint)
            }
            Some(Hot::Grouping(g)) => self.set_grouping(g).then_some(ListOut::Grouping(g)),
            Some(Hot::Toggle(i)) => {
                let k = self.lines[i].key;
                if !self.open_groups.remove(&k) {
                    self.open_groups.insert(k);
                }
                self.clamp();
                Some(ListOut::Repaint)
            }
            Some(Hot::Line(i)) => {
                let now = Instant::now();
                let double = self.last_click.is_some_and(|(at, j)| {
                    j == i && now.duration_since(at).as_millis() < DOUBLE_MS
                });
                self.last_click = Some((now, i));
                let l = self.lines[i].clone();
                if double && !l.elements.is_empty() {
                    self.last_click = None;
                    return Some(ListOut::Zoom(l.elements));
                }
                // Geschoss- und Summenzeilen sind keine Bauteile (H119)
                self.focus_none = !matches!(l.kind, Kind::Group | Kind::Row);
                match l.kind {
                    Kind::Storey => {
                        if !self.closed_storeys.remove(&l.key) {
                            self.closed_storeys.insert(l.key);
                        }
                        self.clamp();
                        Some(ListOut::Repaint)
                    }
                    Kind::Group => {
                        self.select_all(p, l.elements, Some(l.key));
                        Some(ListOut::Picking { selection: true })
                    }
                    Kind::Row => {
                        let id = *l.elements.first()?;
                        self.click_element(p, id, mods.ctrl, mods.shift);
                        Some(ListOut::Picking { selection: true })
                    }
                    _ => None,
                }
            }
            None => None,
        }
    }

    pub fn mouse_up(&mut self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<ListOut> {
        if !std::mem::take(&mut self.button_down) {
            return None;
        }
        if self.hit(t, fonts, x, y) == Some(Hot::Button) {
            Some(ListOut::SaveCsv)
        } else {
            Some(ListOut::Repaint)
        }
    }

    pub fn wheel(&mut self, delta: f64, t: &Theme, animate: bool) -> Option<ListOut> {
        self.target -= delta as f32 * 3.0 * t.size.qto_row;
        self.clamp();
        if !animate {
            self.scroll = self.target;
        }
        Some(ListOut::Repaint)
    }

    /// Rollen und Aufleuchten weiterführen. `true`, solange dafür weitere
    /// Bilder nötig sind (die Pille „wird aktualisiert“ zählt nicht, siehe
    /// [`ListView::pill_key`]).
    pub fn tick(&mut self, t: &Theme, now: Instant) -> bool {
        let dt = self
            .last_tick
            .map_or(0.0, |l| now.duration_since(l).as_secs_f32());
        self.last_tick = Some(now);
        let anim = t.size.anim_ms > 0.0;
        let mut busy = false;
        if (self.target - self.scroll).abs() > 0.3 && anim {
            let f = 1.0 - (-dt / SCROLL_SMOOTHING).exp();
            self.scroll += (self.target - self.scroll) * f;
            busy = true;
        } else {
            self.scroll = self.target;
        }
        let flash = std::time::Duration::from_millis(t.size.flash_ms.max(0.0) as u64);
        self.flash
            .retain(|_, at| anim && now.duration_since(*at) < flash);
        self.row_flash
            .retain(|_, at| anim && now.duration_since(*at) < flash);
        let motion = std::time::Duration::from_secs_f32(
            (t.size.fade_ms.max(0.0) + t.size.anim_ms.max(0.0)) / 1000.0,
        );
        if self
            .motion
            .as_ref()
            .is_some_and(|m| !anim || now.duration_since(m.start) >= motion)
        {
            self.motion = None;
        }
        busy |= self.leiste.tick(t, now);
        busy |= self.flashing();
        busy
    }

    // --- Zeichnen ----------------------------------------------------------

    /// Anzahl der Zeilen (alle, auch zugeklappte).
    #[cfg(test)]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Pille „wird aktualisiert“: Lage im Fenster (px, ganzzahlig, mit
    /// einem Pixel Rand), solange die Liste auf neue Mengen wartet.
    pub fn pill_rect(&self, t: &Theme, fonts: &Fonts) -> Option<(i32, i32, i32, i32)> {
        self.stale?;
        let s = self.scale;
        let f = fonts.regular.as_ref()?;
        let b = fonts.bold.as_ref()?;
        let (x0, _) = self.content_x(t);
        let x = x0 + b.width("Mengenermittlung", 19.0 * s) + 12.0 * s;
        let pw = f.width("wird aktualisiert", 10.0 * s) + 40.0 * s;
        let (py, ph) = (self.top_dip() * s + 18.0 * s, 20.0 * s);
        let (x1, y1) = (x.floor() as i32 - 1, py.floor() as i32 - 1);
        let (x2, y2) = ((x + pw).ceil() as i32 + 1, (py + ph).ceil() as i32 + 1);
        Some((x1, y1, x2 - x1, y2 - y1))
    }

    /// Was die Pille gerade zeigt (leuchtender Punkt, Einblendstufe): ändert
    /// sich das nicht, braucht sie kein neues Bild.
    pub fn pill_key(&self, t: &Theme, now: Instant) -> Option<(u8, u8)> {
        let since = self.stale?;
        let d = now.duration_since(since);
        let fade = if t.size.anim_ms > 0.0 {
            (d.as_secs_f32() / 0.15).min(1.0)
        } else {
            1.0
        };
        // Ohne Animationen stehen die Punkte still
        let phase = if t.size.anim_ms > 0.0 {
            (d.as_millis() / 220 % 3) as u8
        } else {
            0
        };
        Some((phase, (fade * 8.0).round() as u8))
    }

    /// Pille zeichnen; `(ox, oy)`: Ursprung der Leinwand im Fenster.
    pub fn paint_pill(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        now: Instant,
        (ox, oy): (f32, f32),
    ) {
        let (Some((phase, _)), Some(since), Some(f), Some(b)) = (
            self.pill_key(t, now),
            self.stale,
            fonts.regular.as_ref(),
            fonts.bold.as_ref(),
        ) else {
            return;
        };
        let (s, u) = (self.scale, &t.ui);
        let fade = if t.size.anim_ms > 0.0 {
            (now.duration_since(since).as_secs_f32() / 0.15).min(1.0)
        } else {
            1.0
        };
        let (x0, _) = self.content_x(t);
        let x = x0 + b.width("Mengenermittlung", 19.0 * s) + 12.0 * s - ox;
        let text = "wird aktualisiert";
        let pw = f.width(text, 10.0 * s) + 40.0 * s;
        let (py, ph) = (self.top_dip() * s + 18.0 * s - oy, 20.0 * s);
        let mut p = Path::new();
        p.rounded_rect(x, py, pw, ph, ph * 0.5);
        c.fill(&p, alpha(u.sheet_tile, fade));
        for k in 0..3u8 {
            let a = if k == phase { 1.0 } else { 0.35 };
            let mut d = Path::new();
            let cx = x + (10.0 + 6.0 * k as f32) * s;
            d.rounded_rect(
                cx - 2.0 * s,
                py + ph * 0.5 - 2.0 * s,
                4.0 * s,
                4.0 * s,
                2.0 * s,
            );
            c.fill(&d, alpha(u.sheet_text_dim, a * fade));
        }
        f.draw(
            c,
            text,
            10.0 * s,
            x + 30.0 * s,
            py + (ph + f.cap_height(10.0 * s)) * 0.5,
            alpha(u.sheet_text_dim, fade),
        );
    }

    /// Wie der Knopf „Als Tabelle speichern“ aussieht (Maus darüber, gedrückt).
    pub fn button_look(&self) -> (bool, bool) {
        (self.hot == Some(Hot::Button), self.button_down)
    }

    /// Bänder der sichtbaren Zeilen in Fensterpixeln (von, bis, Band), in
    /// derselben Auswahl wie beim Zeichnen. Ändert sich bei Hover oder
    /// Auswahl nur ein Band, reicht es, dessen Zeilen neu zu zeichnen.
    pub fn row_bands(&self, t: &Theme) -> Vec<RowBand> {
        let s = self.scale;
        let list_top = (self.top_dip() + self.head()) * s;
        let mut out = Vec::new();
        for (i, y, h) in self.layout(Some(t)) {
            let ys = list_top + y * s - self.scroll_px() as f32;
            if ys + h * s < list_top - 40.0 * s {
                continue;
            }
            if ys > self.h as f32 {
                break;
            }
            let l = &self.lines[i];
            let (band_y, band_h) = match l.kind {
                Kind::Storey => (ys + STOREY_GAP * s, STOREY_ROW * s),
                _ => (ys, h * s),
            };
            let (y0, y1) = (
                band_y.floor() as i32 - 1,
                (band_y + band_h).ceil() as i32 + 1,
            );
            out.push((y0, y1, self.band(l, t)));
        }
        out
    }

    /// Blatt unter der Titelleiste; die Leinwand ist schon mit `sheet_bg` gefüllt.
    pub fn paint(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, now: Instant) {
        self.fit_head(t, fonts);
        let s = self.scale;
        let top = self.top_dip() * s;
        let (x0, cw) = self.content_x(t);
        let u = &t.ui;
        let anim = t.size.anim_ms > 0.0;

        // Liste (unter dem Kopf, gerollt)
        let list_top = top + self.head() * s;
        let rows = self.layout(Some(t));
        let pad_band = 10.0 * s;
        let motion = self.motion_state(t, now);
        // Zeichnet die Leinwand nur einen Streifen (Hover, Auswahl, Pille),
        // bleiben Zeilen und Kopf außerhalb weg: Schrift kostet je Zeile
        let (v0, v1) = c.visible_y();
        let margin = 4.0 * s;
        let outside = |a: f32, b: f32| b + margin < v0 || a - margin > v1;
        // Weggefallene Zeilen an ihrer alten Lage, blenden aus
        if let Some((_, ghosts)) = &motion {
            for &(l, y, h, a) in ghosts {
                let ys = list_top + y * s - self.scroll_px() as f32;
                if outside(ys, ys + h * s) || ys + h * s < list_top {
                    continue;
                }
                self.paint_line(c, t, l, (x0, cw), ys, h * s, fonts, now, anim);
                c.fill_rect(
                    x0 - pad_band,
                    ys,
                    cw + 2.0 * pad_band,
                    h * s,
                    alpha(u.sheet_bg, 1.0 - a),
                );
            }
        }
        for &(i, y, h) in &rows {
            let off = motion
                .as_ref()
                .and_then(|m| m.0.get(&i))
                .copied()
                .unwrap_or(0.0);
            let ys = list_top + (y + off) * s - self.scroll_px() as f32;
            if ys + h * s < list_top - 40.0 * s {
                continue;
            }
            if motion.is_none() && (ys > self.h as f32 || ys - margin > v1) {
                break;
            }
            if ys > self.h as f32 {
                continue;
            }
            if outside(ys, ys + h * s) {
                continue;
            }
            let l = &self.lines[i];
            let (band_y, band_h) = match l.kind {
                Kind::Storey => (ys + STOREY_GAP * s, STOREY_ROW * s),
                _ => (ys, h * s),
            };
            // Bänder
            let band = self.band(l, t);
            if let Some((col, bar)) = band {
                let mut p = Path::new();
                p.rounded_rect(x0 - pad_band, band_y, cw + 2.0 * pad_band, band_h, 3.0 * s);
                c.fill(&p, col);
                if bar {
                    c.fill_rect(x0 - pad_band, band_y, 3.0 * s, band_h, u.accent);
                }
            }
            if let Some(a) = self.row_flash_alpha(l.key, t, now, anim) {
                let mut p = Path::new();
                p.rounded_rect(x0 - pad_band, band_y, cw + 2.0 * pad_band, band_h, 3.0 * s);
                c.fill(&p, alpha(u.sheet_flash, a));
            }
            self.paint_line(c, t, l, (x0, cw), band_y, band_h, fonts, now, anim);
        }
        // Kopf deckt die weggerollten Zeilen ab
        if !outside(top, top + self.head() * s) {
            self.paint_head(c, t, fonts, now);
        }
        self.paint_scrollbar(c, t);
        self.leiste
            .paint_field_list(c, t, fonts, self.leiste_lage(t));
    }

    /// Laufleiste am rechten Rand der Liste.
    pub fn paint_scrollbar(&self, c: &mut Canvas, t: &Theme) {
        let s = self.scale;
        let list_top = (self.top_dip() + self.head()) * s;
        let content = self.content_h(Some(t));
        let view = self.view_h();
        if content > view + 1.0 {
            let track = view * s;
            let bar_h = (track * view / content).max(24.0 * s);
            let max = (content - view + 24.0).max(1.0);
            let by = list_top + (track - bar_h) * (self.scroll / max).clamp(0.0, 1.0);
            let mut p = Path::new();
            let bw = 4.0 * s;
            p.rounded_rect(self.w as f32 - bw - 4.0 * s, by, bw, bar_h, bw * 0.5);
            c.fill(&p, t.ui.sheet_rule);
        }
    }

    /// Gerollt wird in ganzen Pixeln: So gleicht ein verschobenes Bild dem
    /// neu gezeichneten, und beim Rollen müssen nur die frei werdenden
    /// Zeilen neu gezeichnet werden (U6b).
    pub fn scroll_px(&self) -> i32 {
        (self.scroll * self.scale).round() as i32
    }

    /// Erste Fensterzeile der Liste unter dem Kopf; ab hier verschiebt das
    /// Rollen das Bild.
    pub fn list_y(&self) -> i32 {
        ((self.top_dip() + self.head()) * self.scale).ceil() as i32
    }

    /// Linke Kante der Spalte, in der nur die Laufleiste liegt (beim Rollen
    /// neu gezeichnet). `None`, wenn Zeilen bis dorthin reichen (sehr
    /// kleiner Seitenrand): dann zeichnet Rollen das ganze Bild.
    pub fn scrollbar_x(&self, t: &Theme) -> Option<i32> {
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let rows_right = x0 + cw + 10.0 * s + 1.0;
        let x = (self.w as f32 - 9.0 * s).floor();
        (rows_right <= x).then_some(x as i32)
    }

    /// Geänderte Werte leuchten noch auf (dann ganze Bilder).
    pub fn flashing(&self) -> bool {
        !self.flash.is_empty() || !self.row_flash.is_empty() || self.motion.is_some()
    }

    /// Kopf über der Liste: Titel, Unterzeile, Pille, Knopf, Spaltenköpfe.
    fn paint_head(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, now: Instant) {
        let s = self.scale;
        let top = self.top_dip() * s;
        let (x0, cw) = self.content_x(t);
        let u = &t.ui;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        c.fill_rect(0.0, top, self.w as f32, self.head() * s, u.sheet_bg);
        if let Some(f) = bold {
            f.draw(
                c,
                "Mengenermittlung",
                19.0 * s,
                x0,
                top + 34.0 * s,
                u.sheet_text,
            );
        }
        if let Some(f) = regular {
            f.draw(
                c,
                &self.subtitle_text(),
                10.5 * s,
                x0,
                top + 52.0 * s,
                u.sheet_text_dim,
            );
        }
        // „wird aktualisiert“
        self.paint_pill(c, t, fonts, now, (0.0, 0.0));
        // Knopf „Als Tabelle speichern“ im normalen Knopfstil
        let (bx, by, bw, bh) = self.button_rect(t, fonts);
        let bg = if self.button_down {
            u.pressed
        } else if self.hot == Some(Hot::Button) {
            u.hover
        } else {
            u.bg
        };
        let mut p = Path::new();
        p.rounded_rect(bx, by, bw, bh, t.size.corner_radius * s);
        c.fill(&p, bg);
        if let Some(f) = bold {
            let px = 11.0 * s;
            f.draw(
                c,
                "Als Tabelle speichern",
                px,
                bx + BUTTON_PAD * s,
                by + (bh + f.cap_height(px)) * 0.5,
                u.text,
            );
        }
        self.paint_toggle(c, t, fonts);
        // Spaltenköpfe und Linie
        if let Some(f) = regular {
            let px = 10.0 * s;
            let base = top + (79.0 + CHIP_ROW + self.head_extra.get()) * s;
            let first = match self.grouping {
                Grouping::Storey => "Bauteil",
                Grouping::Trade => "Leistung · Bauteil",
            };
            f.draw(c, first, px, x0, base, u.sheet_text_dim);
            f.draw(c, "Nr.", px, x0 + cw * COL_NR, base, u.sheet_text_dim);
            for (text, right) in [
                ("Länge · Stück", COL_LEN),
                ("Fläche", COL_AREA),
                ("Volumen", 1.0),
            ] {
                let tw = f.width(text, px);
                f.draw(c, text, px, x0 + cw * right - tw, base, u.sheet_text_dim);
            }
        }
        let rule = top + (86.0 + CHIP_ROW + self.head_extra.get()) * s;
        c.fill_rect(x0, rule, cw, s.max(1.0), u.sheet_rule);
        self.leiste.paint(c, t, fonts, self.leiste_lage(t), now);
    }

    /// Umschalter „Gliedern nach: Geschoss | Gewerk“ (Pille in `sheet_tile`,
    /// die gewählte Hälfte in der Fläche des Knopfs).
    fn paint_toggle(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let s = self.scale;
        let u = &t.ui;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let px = 10.5 * s;
        let (label, halves, _) = self.toggle_layout(t, fonts);
        let inset = TOGGLE_INSET * s;
        let (_, (x, y, _, h)) = halves[0];
        let (_, (x1, _, w1, _)) = halves[1];
        let (px0, py0, pw, ph) = (
            x - inset,
            y - inset,
            x1 + w1 + inset - (x - inset),
            h + 2.0 * inset,
        );
        let mut p = Path::new();
        p.rounded_rect(px0, py0, pw, ph, ph * 0.5);
        c.fill(&p, u.sheet_tile);
        if let (Some((lx, ly)), Some(f)) = (label, regular) {
            f.draw(c, "Gliedern nach", px, lx, ly, u.sheet_text_dim);
        }
        for (g, (hx, hy, hw, hh)) in halves {
            let on = g == self.grouping;
            let (font, col) = if on {
                let mut p = Path::new();
                p.rounded_rect(hx, hy, hw, hh, hh * 0.5);
                c.fill(&p, u.sheet_card);
                (bold, u.sheet_text)
            } else if self.hot == Some(Hot::Grouping(g)) {
                (regular, u.sheet_text)
            } else {
                (regular, u.sheet_text_dim)
            };
            let Some(f) = font else { continue };
            let tw = f.width(g.label(), px);
            f.draw(
                c,
                g.label(),
                px,
                hx + (hw - tw) * 0.5,
                hy + (hh + f.cap_height(px)) * 0.5,
                col,
            );
        }
    }

    /// Band einer Zeile: Farbe und Leiste links (Auswahl).
    fn band(&self, l: &Line, t: &Theme) -> Option<(Rgba, bool)> {
        let u = &t.ui;
        let sel = |e: &ElementId| self.selected.contains(e);
        let hov = |e: &ElementId| self.hover.contains(e);
        match l.kind {
            Kind::Row => {
                let e = l.elements.first()?;
                if sel(e) {
                    Some((u.sheet_select, true))
                } else if hov(e) {
                    Some((u.sheet_hover, false))
                } else {
                    None
                }
            }
            Kind::Group | Kind::Storey => {
                if l.elements.is_empty() {
                    return None;
                }
                let open = match l.kind {
                    Kind::Group => self.open_groups.contains(&l.key),
                    _ => !self.closed_storeys.contains(&l.key),
                };
                let all_sel = l.elements.iter().all(sel);
                let any_sel = l.elements.iter().any(sel);
                let group_hover = self.hover.len() > 1 && l.elements == self.hover;
                if all_sel && l.kind == Kind::Group {
                    Some((u.sheet_select, true))
                } else if group_hover || (!open && l.elements.iter().any(hov)) {
                    Some((u.sheet_hover, false))
                } else if any_sel {
                    Some((u.sheet_select_group, false))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Bezeichnung einer Zeile, wie sie gezeigt wird: endet vor der
    /// Nummernspalte (sonst „…“).
    fn label_text(&self, l: &Line, f: &Font, px: f32, (x0, cw): (f32, f32), text_x: f32) -> String {
        if l.cells[1].is_empty() {
            l.cells[0].clone()
        } else {
            let room = x0 + cw * COL_NR - 8.0 * self.scale - text_x;
            sk_ui::widgets::ellipsize(Some(f), &l.cells[0], px, room)
        }
    }

    /// Voller Name der Zeile unter der Maus, wenn er gekürzt gezeigt wird
    /// (Hinweis nach der Wartezeit).
    pub fn tip_at(&self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<String> {
        let hot = self.hit(t, fonts, x, y)?;
        if let Hot::Umfang(h) = hot {
            return self.leiste.tip(h);
        }
        let (Hot::Line(i) | Hot::Toggle(i)) = hot else {
            return None;
        };
        let l = &self.lines[i];
        // Nur einfache Zeilen und Gruppen werden gekürzt (Schrift 11)
        if !matches!(l.kind, Kind::Row | Kind::Group) {
            return None;
        }
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let f = fonts.regular.as_ref()?;
        let px = 11.0 * s;
        let indent = x0 + l.depth as f32 * t.size.qto_indent * s;
        let text_x = indent + if l.kind == Kind::Group { 12.0 * s } else { 0.0 };
        let label = self.label_text(l, f, px, (x0, cw), text_x);
        let x = x as f32;
        (label != l.cells[0] && x >= text_x && x <= text_x + f.width(&label, px))
            .then(|| l.cells[0].clone())
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_line(
        &self,
        c: &mut Canvas,
        t: &Theme,
        l: &Line,
        (x0, cw): (f32, f32),
        y: f32,
        h: f32,
        fonts: &Fonts,
        now: Instant,
        anim: bool,
    ) {
        let s = self.scale;
        let u = &t.ui;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let italic = fonts.italic.as_ref().or(regular);
        let indent = x0 + l.depth as f32 * t.size.qto_indent * s;
        let (font, px, col): (Option<&Font>, f32, Rgba) = match l.kind {
            Kind::Building => (bold, 13.0 * s, u.sheet_text),
            Kind::Storey => (bold, 12.0 * s, u.sheet_text),
            Kind::Control => (italic, 10.5 * s, u.sheet_hint),
            Kind::SumHead => (bold, 12.0 * s, u.sheet_text),
            _ => (regular, 11.0 * s, u.sheet_text),
        };
        let base = (y + (h + font.map_or(8.0, |f| f.cap_height(px))) * 0.5).round();
        match l.kind {
            Kind::Rule => {
                c.fill_rect(x0, y + h * 0.5, cw, s.max(1.0), u.sheet_text);
                return;
            }
            Kind::Tiles => {
                self.paint_tiles(c, t, l, (x0, cw), y, fonts, now, anim);
                return;
            }
            _ => {}
        }
        // Dreieck vor Geschoss- und Gruppenzeilen
        let mut text_x = indent;
        if matches!(l.kind, Kind::Storey | Kind::Group) {
            let open = match l.kind {
                Kind::Group => self.open_groups.contains(&l.key),
                _ => !self.closed_storeys.contains(&l.key),
            };
            let (cx, cy, r) = (indent + 4.0 * s, y + h * 0.5, 4.0 * s);
            let mut p = Path::new();
            if open {
                p.move_to(cx - r, cy - r * 0.6)
                    .line_to(cx + r, cy - r * 0.6)
                    .line_to(cx, cy + r * 0.7)
                    .close();
            } else {
                p.move_to(cx - r * 0.6, cy - r)
                    .line_to(cx + r * 0.7, cy)
                    .line_to(cx - r * 0.6, cy + r)
                    .close();
            }
            c.fill(&p, u.sheet_text_dim);
            text_x += 12.0 * s;
        }
        let Some(f) = font else { return };
        let label = self.label_text(l, f, px, (x0, cw), text_x);
        f.draw(c, &label, px, text_x, base, col);
        if let (Some(tag), Some(fr)) = (&l.tag, regular) {
            let tx = text_x + f.width(&label, px) + 10.0 * s;
            fr.draw(c, tag, 10.0 * s, tx, base, u.sheet_text_dim);
        }
        if let Some(note) = &l.note {
            if let Some(fi) = italic {
                let nx = text_x + f.width(&label, px) + 10.0 * s;
                fi.draw(c, note, 10.5 * s, nx, base, u.sheet_hint);
            }
        }
        // Nummer gedämpft, endet vor der Länge (sonst „…“)
        if let Some(fr) = regular {
            if !l.cells[1].is_empty() {
                let mut room = x0 + cw * COL_LEN - x0 - cw * COL_NR - 8.0 * s;
                let len_font = if l.kind == Kind::Group { bold } else { regular };
                if let (false, Some(lf)) = (l.cells[2].is_empty(), len_font) {
                    room -= lf.width(&l.cells[2], 11.0 * s);
                }
                // Bereich „AW-001 … 020“ zur Not enger oder nur der Anfang
                let full = &l.cells[1];
                let first = full.split(" … ").next().unwrap_or(full);
                let fits = |x: &str| fr.width(x, 10.5 * s) <= room;
                let nr = [full.clone(), full.replace(" … ", "…"), format!("{first}…")]
                    .into_iter()
                    .find(|x| fits(x))
                    .unwrap_or_else(|| sk_ui::widgets::ellipsize(Some(fr), first, 10.5 * s, room));
                fr.draw(c, &nr, 10.5 * s, x0 + cw * COL_NR, base, u.sheet_text_dim);
            }
        }
        // Zahlen rechtsbündig; Zwischensummen fett, Kontrollzeilen kursiv
        let num_font = match l.kind {
            Kind::Group => bold,
            Kind::Control => italic,
            _ => regular,
        };
        let Some(nf) = num_font else { return };
        let npx = if l.kind == Kind::Control {
            10.5 * s
        } else {
            11.0 * s
        };
        for (k, right) in [(2usize, COL_LEN), (3, COL_AREA), (4, 1.0)] {
            let text = &l.cells[k];
            if text.is_empty() {
                continue;
            }
            let tw = nf.width(text, npx);
            let x = x0 + cw * right - tw;
            if let Some(a) = self.flash_alpha(l.key, k as u8, t, now, anim) {
                let mut p = Path::new();
                p.rounded_rect(
                    x - 5.0 * s,
                    y + 2.0 * s,
                    tw + 10.0 * s,
                    h - 4.0 * s,
                    4.0 * s,
                );
                c.fill(&p, alpha(u.sheet_flash, a));
            }
            nf.draw(c, text, npx, x, base, col);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_tiles(
        &self,
        c: &mut Canvas,
        t: &Theme,
        l: &Line,
        (x0, cw): (f32, f32),
        y: f32,
        fonts: &Fonts,
        now: Instant,
        anim: bool,
    ) {
        let s = self.scale;
        let u = &t.ui;
        let slots = self.tile_slots(Some(t), l.tiles.len());
        let n = slots as f32;
        let gap = TILE_GAP * s;
        let tw = ((cw - gap * (n - 1.0)) / n).max(40.0 * s);
        for (k, tile) in l.tiles.iter().enumerate() {
            let x = x0 + (k % slots) as f32 * (tw + gap);
            let y = y + (k / slots) as f32 * (TILE_H + TILE_GAP) * s;
            let mut p = Path::new();
            p.rounded_rect(x, y, tw, TILE_H * s, 6.0 * s);
            c.fill(&p, u.sheet_tile);
            if let Some(f) = fonts.regular.as_ref() {
                let (px, pad) = (TILE_NAME_PX * s, TILE_PAD * s);
                let name = sk_ui::widgets::ellipsize(Some(f), &tile.name, px, tw - 2.0 * pad);
                f.draw(c, &name, px, x + pad, y + 18.0 * s, u.sheet_text_dim);
            }
            if let Some(f) = fonts.bold.as_ref().or(fonts.regular.as_ref()) {
                let vx = x + TILE_PAD * s;
                if let Some(a) = self.flash_alpha(tile.key, 0, t, now, anim) {
                    let w = f.width(&tile.value, 16.0 * s);
                    let mut p = Path::new();
                    p.rounded_rect(vx - 4.0 * s, y + 24.0 * s, w + 8.0 * s, 22.0 * s, 4.0 * s);
                    c.fill(&p, alpha(u.sheet_flash, a));
                }
                f.draw(c, &tile.value, 16.0 * s, vx, y + 41.0 * s, u.sheet_text);
                if let Some(x2) = &tile.extra {
                    f.draw(c, x2, 10.5 * s, vx, y + 56.0 * s, u.sheet_text);
                }
            }
        }
    }

    /// Deckkraft des Aufleuchtens einer Zelle (1 → 0 über `flash_ms`).
    fn flash_alpha(&self, key: Key, col: u8, t: &Theme, now: Instant, anim: bool) -> Option<f32> {
        if !anim {
            return None;
        }
        let at = self.flash.get(&(key, col))?;
        let k = now.duration_since(*at).as_secs_f32() * 1000.0 / t.size.flash_ms.max(1.0);
        (k < 1.0).then_some(1.0 - k * k)
    }
}

/// Farbe mit verringerter Deckkraft.
fn alpha(c: Rgba, a: f32) -> Rgba {
    Rgba(
        c.0,
        c.1,
        c.2,
        (c.3 as f32 * a.clamp(0.0, 1.0)).round() as u8,
    )
}

// --- Aufbau der Zeilen -------------------------------------------------------

fn storey_key(id: sk_model::StoreyId) -> Key {
    Key::Storey(id.index())
}

/// Geschoss, Bauteilart, Aufbau einer Wandgruppe ([`Key::Group`]).
fn group_id(st: sk_model::StoreyId, g: &GroupQto) -> (u32, u8, u32) {
    (
        st.index(),
        g.category as u8,
        g.layer_set.map_or(u32::MAX, |s| s.index()),
    )
}

fn elem_key(e: ElementId) -> Key {
    Key::Element(e.index(), e.generation())
}

/// Name eines Geschosses in der Liste: die Gründung heißt „Fundament“ (E18).
fn storey_name(m: &Model, id: sk_model::StoreyId) -> (String, String) {
    match m.storey(id) {
        Some(s) if s.kind == LevelKind::Foundation => ("Fundament".into(), s.short.clone()),
        Some(s) => (s.name.clone(), s.short.clone()),
        None => ("–".into(), String::new()),
    }
}

/// Titel einer Wandgruppe: im Blatt kurz („Außenwände AW 31,5“), in der
/// Tabelle mit dem ganzen Namen des Aufbaus.
fn group_title(m: &Model, g: &GroupQto, short: bool) -> String {
    let k = sk_model::kinds::spec(g.category);
    let (name, code) = if is_wall(g.category) {
        (k.plural, k.prefix)
    } else {
        (k.name, "")
    };
    let set = g.layer_set.and_then(|s| m.layer_set(s));
    match set {
        Some(s) if short => format!(
            "{name} {code} {}",
            cm(s.thickness()).trim_end_matches(" cm")
        ),
        Some(s) => format!("{name} {}", s.name),
        None => name.to_string(),
    }
}

/// Nummernbereich einer Gruppe: „AW-001 … 004“.
fn number_range(m: &Model, g: &GroupQto) -> String {
    let n: Vec<&str> = g.rows.iter().map(|r| r.number.as_str()).collect();
    match n.as_slice() {
        [] => String::new(),
        [one] => one.to_string(),
        [first, .., last] => {
            let tail = last.rsplit('-').next().unwrap_or(last);
            let _ = m;
            format!("{first} … {tail}")
        }
    }
}

fn is_wall(c: Category) -> bool {
    matches!(c, Category::ExteriorWall | Category::InteriorWall)
}

/// Dicke in cm ohne überflüssige Nachkommastellen: „22 cm“, „17,5 cm“.
fn cm(mm: f64) -> String {
    let v = mm / 10.0;
    if (v - v.round()).abs() < 1e-6 {
        format!("{} cm", v.round() as i64)
    } else {
        format!("{} cm", de(v, 1))
    }
}

/// Zusatz in der Zeile einer gestapelten Wand mit Versatz (OG Phase 2):
/// „Versatz +0,30 m“, „Versatz −0,30 m“; `None` bei Versatz 0 oder einer
/// Wand ohne Partner darunter.
pub fn offset_note(m: &Model, wall: ElementId) -> Option<String> {
    let (o, _) = m.stack_offset(wall)?;
    let v = o / 1000.0;
    if (v * 100.0).round() == 0.0 {
        return None;
    }
    let sign = if v > 0.0 { '+' } else { '−' };
    Some(format!("Versatz {sign}{} m", de(v.abs(), 2)))
}

/// Zahl mit Dezimalkomma und Tausenderpunkt.
pub fn de(v: f64, dec: usize) -> String {
    let neg = v < 0.0 && (v * 10f64.powi(dec as i32)).round() != 0.0;
    let s = format!("{:.*}", dec, v.abs());
    let (int, frac) = match s.split_once('.') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (s.clone(), None),
    };
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    let mut out = String::new();
    if neg {
        out.push('−');
    }
    out.push_str(&grouped);
    if let Some(f) = frac {
        out.push(',');
        out.push_str(&f);
    }
    out
}

fn m_len(mm: f64) -> String {
    format!("{} m", de(mm / 1e3, 2))
}

fn m_area(mm2: f64) -> String {
    format!("{} m²", de(mm2 / 1e6, 2))
}

fn m_vol(mm3: f64) -> String {
    format!("{} m³", de(mm3 / 1e9, 3))
}

fn build_lines(m: &Model, sched: &Schedule, by: Grouping) -> (Vec<Line>, Vec<GroupRef>) {
    let mut lines = Vec::new();
    let mut groups = Vec::new();
    let many = sched.buildings.len() > 1;
    for b in &sched.buildings {
        let info = m.building(b.id);
        if many {
            let mut l = Line::new(Kind::Building, 0, Key::Building(b.id.index()));
            l.cells[0] = info.map_or(String::new(), |x| format!("{} ({})", x.name, x.number));
            lines.push(l);
        }
        match by {
            Grouping::Storey => {
                for st in &b.storeys {
                    storey_lines(m, st, &mut lines, &mut groups);
                }
            }
            Grouping::Trade => trade_lines(m, b, &mut lines),
        }
        lines.push(Line::new(Kind::Rule, 0, Key::None));
        let mut head = Line::new(Kind::SumHead, 0, Key::None);
        head.cells[0] = format!(
            "Summe nach Baustoff · {}",
            info.map_or(String::new(), |x| x.name.clone())
        );
        lines.push(head);
        let mut tiles = Line::new(Kind::Tiles, 0, Key::None);
        tiles.tiles = b
            .by_material
            .iter()
            .map(|x| Tile {
                key: Key::Tile(b.id.index(), x.material.index()),
                name: m
                    .material(x.material)
                    .map_or(String::new(), |y| y.name.clone()),
                // Blech in m statt m³ (Attikablech)
                value: x.length.map_or_else(|| m_vol(x.volume), m_len),
                extra: x.area.map(m_area),
            })
            .collect();
        lines.push(tiles);
    }
    for st in &sched.loose {
        storey_lines(m, st, &mut lines, &mut groups);
    }
    (lines, groups)
}

/// Schichten, die nach Gewerk zu aufklappbaren Gruppen zusammengefasst
/// werden (wie die Wände nach Geschoss); die übrigen Bauteile stehen einzeln.
fn layer_grouped(c: Category) -> bool {
    is_wall(c) || c == Category::EdgeInsulation
}

/// ATV-Nummer gedimmt hinter dem Gewerk: „DIN 18331“; eigene Nummern der
/// Firma stehen, wie sie sind.
fn trade_tag(code: &str) -> String {
    if code.len() == 5 && code.starts_with("18") && code.bytes().all(|b| b.is_ascii_digit()) {
        format!("DIN {code}")
    } else {
        code.to_string()
    }
}

/// Baustoff mit Dicke ohne Einheit: „Porenbeton 17,5“.
fn layer_name(m: &Model, r: &LayerRow) -> String {
    let name = m.material(r.material).map_or("–", |x| x.name.as_str());
    format!("{name} {}", cm(r.thickness).trim_end_matches(" cm"))
}

/// Titel einer Schichtgruppe: „Porenbeton 17,5 · Außenwände“, mit
/// „, mit Attika“, wenn die Schicht über einen Terrassenrand hochläuft.
fn layer_group_title(m: &Model, r: &LayerRow, rows: &[LayerRow]) -> String {
    let attika = if rows.iter().any(|x| x.attika) {
        ", mit Attika"
    } else {
        ""
    };
    format!(
        "{} · {}{attika}",
        layer_name(m, r),
        sk_model::kinds::spec(r.category).plural
    )
}

/// Bezeichnung eines einzeln stehenden Bauteils nach Gewerk: wie nach
/// Geschoss, bei mehrschichtigen Bauteilen „Bauteilart · Schicht“.
fn layer_row_label(m: &Model, r: &LayerRow) -> String {
    if !r.whole {
        let name = m.material(r.material).map_or("–", |x| x.name.as_str());
        return format!("{} · {name} {}", r.category.name(), cm(r.thickness));
    }
    let core = m
        .element(r.element)
        .and_then(|e| e.kind.core_thickness())
        .unwrap_or(r.thickness);
    match r.category {
        Category::StripFooting => "Frostschürze".into(),
        Category::GroundSlab => format!("Sohlplatte {}", cm(core)),
        Category::Floor => format!("Decke über {} {}", storey_name(m, r.storey).1, cm(core)),
        Category::SoffitInsulation => format!("Untersichtdämmung {}", cm(r.thickness)),
        c => c.name().into(),
    }
}

/// Mengen je Bauteil einer Schichtgruppe: Zeilen desselben Bauteils (zwei
/// Schichten aus gleichem Baustoff und gleicher Dicke) zusammen.
fn per_element(rows: &[LayerRow]) -> Vec<LayerRow> {
    let mut out: Vec<LayerRow> = Vec::new();
    for r in rows {
        match out.iter_mut().find(|x| x.element == r.element) {
            Some(x) => {
                x.volume += r.volume;
                x.area += r.area;
                x.attika |= r.attika;
            }
            None => out.push(r.clone()),
        }
    }
    out
}

/// Zeilen eines Gebäudes nach Gewerk (Paket 1b): Gewerk (aufklappbar, mit
/// ATV-Nummer) → Schichtgruppe „Schicht · Bauteilgruppe“ (aufklappbar,
/// Bauteile mit Geschoss) bzw. einzelne Bauteile.
fn trade_lines(m: &Model, b: &BuildingQto, lines: &mut Vec<Line>) {
    let bi = b.id.index();
    for t in &b.by_trade {
        let Some(tr) = m.trade(t.trade) else { continue };
        let order = tr.order as u32;
        let tkey = Key::Trade(bi, order);
        let mut head = Line::new(Kind::Storey, 0, tkey);
        head.cells[0] = tr.name.clone();
        head.tag = Some(trade_tag(&tr.code));
        for r in &t.rows {
            if !head.elements.contains(&r.element) {
                head.elements.push(r.element);
            }
        }
        lines.push(head);
        let mut i = 0;
        while i < t.rows.len() {
            let r = &t.rows[i];
            let row_key = |r: &LayerRow| {
                let e = r.element;
                Key::LayerRow(e.index(), e.generation(), r.layer.min(255) as u8)
            };
            if !layer_grouped(r.category) {
                let mut rl = Line::new(Kind::Row, 1, row_key(r));
                rl.storey = tkey;
                rl.elements = vec![r.element];
                rl.cells[0] = layer_row_label(m, r);
                rl.cells[1] = r.number.clone();
                if r.length > 0.0 {
                    rl.cells[2] = m_len(r.length);
                }
                if r.area > 0.0 {
                    rl.cells[3] = m_area(r.area);
                }
                // Attikablech: abgerechnet in m, ohne Volumen
                if r.category != Category::Coping {
                    rl.cells[4] = m_vol(r.volume);
                }
                lines.push(rl);
                i += 1;
                continue;
            }
            let same = |x: &LayerRow| {
                x.category == r.category
                    && x.material == r.material
                    && (x.thickness - r.thickness).abs() < 1e-6
            };
            let n = t.rows[i..].iter().take_while(|x| same(x)).count();
            let rows = per_element(&t.rows[i..i + n]);
            i += n;
            let gkey = Key::Layer(
                (bi << 16) | (order & 0xffff),
                r.category as u8,
                r.material.index(),
                (r.thickness * 100.0).round() as u32,
            );
            let mut gl = Line::new(Kind::Group, 1, gkey);
            gl.storey = tkey;
            gl.elements = rows.iter().map(|x| x.element).collect();
            gl.cells[0] = layer_group_title(m, r, &rows);
            gl.cells[1] = match rows.as_slice() {
                [] => String::new(),
                [one] => one.number.clone(),
                [first, .., last] => {
                    let tail = last.number.rsplit('-').next().unwrap_or(&last.number);
                    format!("{} … {tail}", first.number)
                }
            };
            let len: f64 = rows.iter().map(|x| x.length).sum();
            gl.cells[2] = if len > 0.0 {
                format!("{} · {}", m_len(len), rows.len())
            } else {
                rows.len().to_string()
            };
            let area: f64 = rows.iter().map(|x| x.area).sum();
            if r.insulation && area > 0.0 {
                gl.cells[3] = m_area(area);
            }
            gl.cells[4] = m_vol(rows.iter().map(|x| x.volume).sum());
            lines.push(gl);
            for x in &rows {
                let mut rl = Line::new(Kind::Row, 2, row_key(x));
                rl.storey = tkey;
                rl.group = gkey;
                rl.elements = vec![x.element];
                rl.cells[0] = format!("{} · {}", x.number, storey_name(m, x.storey).1);
                if x.length > 0.0 {
                    rl.cells[2] = m_len(x.length);
                }
                if x.insulation && x.area > 0.0 {
                    rl.cells[3] = m_area(x.area);
                }
                rl.cells[4] = m_vol(x.volume);
                lines.push(rl);
            }
        }
    }
}

fn storey_lines(m: &Model, st: &StoreyQto, lines: &mut Vec<Line>, groups: &mut Vec<GroupRef>) {
    let (name, short) = storey_name(m, st.id);
    let skey = storey_key(st.id);
    let mut head = Line::new(Kind::Storey, 0, skey);
    head.cells[0] = name;
    head.elements = st
        .groups
        .iter()
        .flat_map(|g| g.rows.iter().map(|r| r.element))
        .collect();
    lines.push(head);
    for g in &st.groups {
        groups.push((
            short.clone(),
            g.category,
            g.rows.iter().map(|r| r.element).collect(),
        ));
        if is_wall(g.category) {
            let (a, b, c) = group_id(st.id, g);
            let gkey = Key::Group(a, b, c);
            let mut gl = Line::new(Kind::Group, 1, gkey);
            gl.storey = skey;
            gl.elements = g.rows.iter().map(|r| r.element).collect();
            gl.cells[0] = group_title(m, g, true);
            gl.cells[1] = number_range(m, g);
            gl.cells[2] = format!("{} · {}", m_len(g.total.length), g.total.count);
            gl.cells[4] = m_vol(g.total.volume);
            lines.push(gl);
            for r in &g.rows {
                let mut rl = Line::new(Kind::Row, 2, elem_key(r.element));
                rl.storey = skey;
                rl.group = gkey;
                rl.elements = vec![r.element];
                match &r.q {
                    Some(ElementQto::Wall(w)) => {
                        rl.cells[0] = format!(
                            "{}   {} · Höhe {}",
                            r.number,
                            m_len(w.list_length),
                            de(w.height / 1e3, 3) + " m"
                        );
                        rl.cells[4] = m_vol(w.volume);
                        // Gestapelte Wand mit Versatz (OG Phase 2)
                        rl.note = offset_note(m, r.element);
                    }
                    _ => {
                        rl.cells[0] = r.number.clone();
                        rl.cells[4] = "–".into();
                        rl.note = r.note.clone();
                    }
                }
                lines.push(rl);
                // Verblender über einem Vorsprung: Halfeneisen (OG-17)
                if let Some(ElementQto::Wall(w)) = &r.q {
                    if w.facing_support > 0.0 {
                        let e = r.element;
                        let key = Key::Control(e.index(), 2, e.generation());
                        let mut cl = Line::new(Kind::Control, 3, key);
                        cl.storey = skey;
                        cl.group = gkey;
                        cl.elements = vec![e];
                        cl.cells[0] = "Abfangung Verblender".into();
                        cl.cells[2] = m_len(w.facing_support);
                        lines.push(cl);
                    }
                }
            }
            if g.total.pocket > 0.0 {
                let mut cl = Line::new(Kind::Control, 2, Key::GroupControl(a, b, c));
                cl.storey = skey;
                cl.group = gkey;
                cl.cells[0] = match g.category {
                    Category::ExteriorWall => "Abzug Deckenauflager (in der Decke enthalten)",
                    _ => "Abzug Deckenstreifen (in der Decke enthalten)",
                }
                .into();
                cl.cells[4] = m_vol(-g.total.pocket);
                lines.push(cl);
            }
            continue;
        }
        for r in &g.rows {
            let mut rl = Line::new(Kind::Row, 1, elem_key(r.element));
            rl.storey = skey;
            rl.elements = vec![r.element];
            rl.cells[1] = r.number.clone();
            let mut control = None;
            let mut parts: Vec<(&str, f64)> = Vec::new();
            match &r.q {
                Some(ElementQto::Footing(f)) => {
                    rl.cells[0] = "Frostschürze".into();
                    rl.cells[2] = m_len(f.length);
                    rl.cells[4] = m_vol(f.volume);
                }
                Some(ElementQto::Slab(p)) => {
                    rl.cells[0] = format!("Sohlplatte {}", cm(p.thickness));
                    rl.cells[3] = m_area(p.area);
                    rl.cells[4] = m_vol(p.volume);
                }
                Some(ElementQto::Floor(f)) => {
                    rl.cells[0] = format!("Decke über {short} {}", cm(f.thickness));
                    rl.cells[3] = m_area(f.area);
                    rl.cells[4] = m_vol(f.volume);
                    if f.bearing > 0.0 {
                        control = Some(f.bearing);
                    }
                }
                Some(ElementQto::Wall(w)) => {
                    rl.cells[0] = g.category.name().into();
                    rl.cells[4] = m_vol(w.volume);
                }
                Some(ElementQto::Strip(f)) => {
                    rl.cells[0] = format!("Randdämmstreifen {} × {}", cm(f.width), cm(f.height));
                    rl.cells[2] = m_len(f.length);
                    rl.cells[4] = m_vol(f.volume);
                }
                Some(ElementQto::Soffit(f)) => {
                    rl.cells[0] = format!("Untersichtdämmung {}", cm(f.thickness));
                    rl.cells[3] = m_area(f.area);
                    rl.cells[4] = m_vol(f.volume);
                }
                Some(ElementQto::Terrace(t)) => {
                    rl.cells[0] = "Dachterrasse".into();
                    rl.cells[3] = m_area(t.area);
                    rl.cells[4] = m_vol(t.volume);
                    parts = vec![("Dämmung", t.insulation_volume), ("Belag", t.finish_volume)];
                }
                Some(ElementQto::Coping(c)) => {
                    // Abgerechnet in m, ohne Volumen
                    rl.cells[0] = "Attikablech".into();
                    rl.cells[2] = m_len(c.length);
                }
                None => {
                    rl.cells[0] = g.category.name().into();
                    rl.cells[4] = "–".into();
                    rl.note = r.note.clone();
                }
            }
            let ekey = r.element;
            lines.push(rl);
            if let Some(b) = control {
                let mut cl = Line::new(
                    Kind::Control,
                    2,
                    Key::Control(ekey.index(), 1, ekey.generation()),
                );
                cl.storey = skey;
                cl.cells[0] = "davon Auflager in den Außenwänden".into();
                cl.cells[4] = m_vol(b);
                lines.push(cl);
            }
            // Dachterrasse: Dämmung und Belag je für sich (soll-dt-3)
            for (i, (name, v)) in parts.into_iter().enumerate() {
                let mut cl = Line::new(
                    Kind::Control,
                    2,
                    Key::Control(ekey.index(), 3 + i as u8, ekey.generation()),
                );
                cl.storey = skey;
                cl.cells[0] = format!("davon {name}");
                cl.cells[4] = m_vol(v);
                lines.push(cl);
            }
        }
    }
}

// --- CSV ---------------------------------------------------------------------

/// Zahl für die .csv: Dezimalkomma, 4 Nachkommastellen, ASCII-Minus.
fn csv_num(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = if s == "-0.0000" { "0.0000".into() } else { s };
    s.replace('.', ",")
}

fn csv_field(s: &str) -> String {
    if s.contains([';', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// „Als Tabelle speichern“: UTF-8 mit BOM, Semikolon, Dezimalkomma, alle
/// Gruppen aufgeklappt, 4 Nachkommastellen (m, m², m³).
#[cfg(test)]
pub fn csv(m: &Model, sched: &Schedule) -> Vec<u8> {
    csv_grouped(m, sched, Grouping::Storey)
}

/// Wie [`csv_grouped`], mit der Kopfzeile des Umfangs als erster Zeile
/// (paket-ka1.md §3), nach dem BOM.
pub fn csv_mit_kopf(m: &Model, sched: &Schedule, by: Grouping, kopf: &str) -> Vec<u8> {
    let mut b = csv_grouped(m, sched, by);
    debug_assert!(b.starts_with(&[0xEF, 0xBB, 0xBF]), "CSV mit BOM");
    let zeile = format!("{}\r\n", csv_field(kopf));
    b.splice(3..3, zeile.into_bytes());
    b
}

/// Wie [`csv`] in der Gliederung des Mengenfensters (§12 Prüfpunkt 5).
pub fn csv_grouped(m: &Model, sched: &Schedule, by: Grouping) -> Vec<u8> {
    match by {
        Grouping::Storey => csv_storeys(m, sched),
        Grouping::Trade => csv_trades(m, sched),
    }
}

/// Nach Gewerk (paket-1a §7): je Gewerk die Schichtgruppen mit
/// Zwischensumme, die Bauteile mit Geschoss und der Kostengruppe der
/// Schicht, die Summe des Gewerks; danach die Summen nach Baustoff.
fn csv_trades(m: &Model, sched: &Schedule) -> Vec<u8> {
    let mut out = String::new();
    let mut row = |cols: [&str; 10]| {
        let v: Vec<String> = cols.iter().map(|c| csv_field(c)).collect();
        out.push_str(&v.join(";"));
        out.push_str("\r\n");
    };
    row([
        "Gebäude",
        "Gewerk",
        "Kostengruppe",
        "Bauteil",
        "Nr.",
        "Geschoss",
        "Länge (m)",
        "Fläche (m²)",
        "Volumen (m³)",
        "Hinweis",
    ]);
    let len = |mm: f64| csv_num(mm / 1e3);
    let area = |mm2: f64| csv_num(mm2 / 1e6);
    let vol = |mm3: f64| csv_num(mm3 / 1e9);
    let opt = |v: f64, f: &dyn Fn(f64) -> String| if v > 0.0 { f(v) } else { String::new() };
    for b in &sched.buildings {
        let gb = m.building(b.id).map_or(String::new(), |x| x.number.clone());
        for t in &b.by_trade {
            let Some(tr) = m.trade(t.trade) else { continue };
            let trade = format!("{} {}", tr.code, tr.name);
            let mut i = 0;
            while i < t.rows.len() {
                let r = &t.rows[i];
                let kg = |x: &LayerRow| x.kg.map_or(String::new(), |k| k.to_string());
                if !layer_grouped(r.category) {
                    row([
                        &gb,
                        &trade,
                        &kg(r),
                        &layer_row_label(m, r),
                        &r.number,
                        &storey_name(m, r.storey).0,
                        &opt(r.length, &len),
                        &opt(r.area, &area),
                        &if r.category == Category::Coping {
                            String::new()
                        } else {
                            vol(r.volume)
                        },
                        "",
                    ]);
                    i += 1;
                    continue;
                }
                let n = t.rows[i..]
                    .iter()
                    .take_while(|x| {
                        x.category == r.category
                            && x.material == r.material
                            && (x.thickness - r.thickness).abs() < 1e-6
                    })
                    .count();
                let rows = per_element(&t.rows[i..i + n]);
                i += n;
                let title = layer_group_title(m, r, &rows);
                let a: f64 = rows.iter().map(|x| x.area).sum();
                let range = match rows.as_slice() {
                    [] => String::new(),
                    [one] => one.number.clone(),
                    [first, .., last] => {
                        let tail = last.number.rsplit('-').next().unwrap_or(&last.number);
                        format!("{} … {tail}", first.number)
                    }
                };
                row([
                    &gb,
                    &trade,
                    &kg(r),
                    &format!("{title} (Summe, {} Stück)", rows.len()),
                    &range,
                    "",
                    &opt(rows.iter().map(|x| x.length).sum(), &len),
                    &if r.insulation {
                        opt(a, &area)
                    } else {
                        String::new()
                    },
                    &vol(rows.iter().map(|x| x.volume).sum()),
                    "",
                ]);
                for x in &rows {
                    row([
                        &gb,
                        &trade,
                        &kg(x),
                        &title,
                        &x.number,
                        &storey_name(m, x.storey).0,
                        &opt(x.length, &len),
                        &if x.insulation {
                            opt(x.area, &area)
                        } else {
                            String::new()
                        },
                        &vol(x.volume),
                        "",
                    ]);
                }
            }
            row([
                &gb,
                &trade,
                "",
                &format!("Summe {}", tr.name),
                "",
                "",
                &t.length.map_or(String::new(), len),
                &t.area.map_or(String::new(), area),
                &vol(t.volume),
                "",
            ]);
        }
        for x in &b.by_material {
            let name = m
                .material(x.material)
                .map_or(String::new(), |y| y.name.clone());
            row([
                &gb,
                "Summe nach Baustoff",
                "",
                &name,
                "",
                "",
                &x.length.map_or(String::new(), len),
                &x.area.map_or(String::new(), area),
                &if x.length.is_some() {
                    String::new()
                } else {
                    vol(x.volume)
                },
                "",
            ]);
        }
    }
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(out.as_bytes());
    bytes
}

/// Nach Geschoss (B7).
fn csv_storeys(m: &Model, sched: &Schedule) -> Vec<u8> {
    let mut out = String::new();
    let mut row = |cols: [&str; 10]| {
        let v: Vec<String> = cols.iter().map(|c| csv_field(c)).collect();
        out.push_str(&v.join(";"));
        out.push_str("\r\n");
    };
    row([
        "Gebäude",
        "Geschoss",
        "Kostengruppe",
        "Bauteil",
        "Nr.",
        "Länge (m)",
        "Stück",
        "Fläche (m²)",
        "Volumen (m³)",
        "Hinweis",
    ]);
    let len = |mm: f64| csv_num(mm / 1e3);
    let area = |mm2: f64| csv_num(mm2 / 1e6);
    let vol = |mm3: f64| csv_num(mm3 / 1e9);
    let mut storeys: Vec<(String, &StoreyQto)> = Vec::new();
    for b in &sched.buildings {
        let nr = m.building(b.id).map_or(String::new(), |x| x.number.clone());
        for st in &b.storeys {
            storeys.push((nr.clone(), st));
        }
    }
    for st in &sched.loose {
        storeys.push((String::new(), st));
    }
    for (gb, st) in storeys {
        let (sname, short) = storey_name(m, st.id);
        for g in &st.groups {
            let kg = g.category.din276().map_or(String::new(), |k| k.to_string());
            let title = if is_wall(g.category) {
                group_title(m, g, false)
            } else {
                g.category.name().to_string()
            };
            // Zwischensumme der Wandgruppe (die übrigen sind einzelne Bauteile)
            let t = g.total;
            if is_wall(g.category) {
                row([
                    &gb,
                    &sname,
                    &kg,
                    &format!("{title} (Summe)"),
                    &number_range(m, g),
                    &if t.length > 0.0 {
                        len(t.length)
                    } else {
                        String::new()
                    },
                    &t.count.to_string(),
                    &if t.area > 0.0 {
                        area(t.area)
                    } else {
                        String::new()
                    },
                    &vol(t.volume),
                    "",
                ]);
            }
            for r in &g.rows {
                let (bauteil, l, a, v, note) = match &r.q {
                    Some(ElementQto::Wall(w)) => (
                        format!("{} {}, Höhe {} m", title, r.number, de(w.height / 1e3, 3)),
                        len(w.list_length),
                        String::new(),
                        vol(w.volume),
                        offset_note(m, r.element).unwrap_or_default(),
                    ),
                    Some(ElementQto::Footing(f)) => (
                        "Frostschürze".into(),
                        len(f.length),
                        String::new(),
                        vol(f.volume),
                        String::new(),
                    ),
                    Some(ElementQto::Slab(p)) => (
                        format!("Sohlplatte {}", cm(p.thickness)),
                        String::new(),
                        area(p.area),
                        vol(p.volume),
                        String::new(),
                    ),
                    Some(ElementQto::Floor(f)) => (
                        format!("Decke über {short} {}", cm(f.thickness)),
                        String::new(),
                        area(f.area),
                        vol(f.volume),
                        String::new(),
                    ),
                    Some(ElementQto::Strip(f)) => (
                        format!("Randdämmstreifen {} × {}", cm(f.width), cm(f.height)),
                        len(f.length),
                        String::new(),
                        vol(f.volume),
                        String::new(),
                    ),
                    Some(ElementQto::Soffit(f)) => (
                        format!("Untersichtdämmung {}", cm(f.thickness)),
                        String::new(),
                        area(f.area),
                        vol(f.volume),
                        String::new(),
                    ),
                    Some(ElementQto::Terrace(t)) => (
                        "Dachterrasse".into(),
                        String::new(),
                        area(t.area),
                        vol(t.volume),
                        format!(
                            "Dämmung {} m³, Belag {} m³",
                            de(t.insulation_volume / 1e9, 3),
                            de(t.finish_volume / 1e9, 3)
                        ),
                    ),
                    Some(ElementQto::Coping(c)) => (
                        "Attikablech".into(),
                        len(c.length),
                        String::new(),
                        String::new(),
                        format!("Abwicklung {} mm", c.girth.round()),
                    ),
                    None => (
                        g.category.name().to_string(),
                        String::new(),
                        String::new(),
                        "–".into(),
                        r.note.clone().unwrap_or_default(),
                    ),
                };
                row([
                    &gb, &sname, &kg, &bauteil, &r.number, &l, "1", &a, &v, &note,
                ]);
                if let Some(ElementQto::Wall(w)) = &r.q {
                    if w.facing_support > 0.0 {
                        row([
                            &gb,
                            &sname,
                            &kg,
                            "Abfangung Verblender",
                            &r.number,
                            &len(w.facing_support),
                            "",
                            "",
                            "",
                            "",
                        ]);
                    }
                }
                if let Some(ElementQto::Floor(f)) = &r.q {
                    if f.bearing > 0.0 {
                        row([
                            &gb,
                            &sname,
                            &kg,
                            "davon Auflager in den Außenwänden",
                            &r.number,
                            "",
                            "",
                            "",
                            &vol(f.bearing),
                            "in der Decke enthalten",
                        ]);
                    }
                }
            }
            if is_wall(g.category) {
                // Schichten je Baustoff
                let mut mats: Vec<(sk_model::MaterialId, f64, f64)> = Vec::new();
                for r in &g.rows {
                    if let Some(ElementQto::Wall(w)) = &r.q {
                        for l in &w.layers {
                            match mats.iter_mut().find(|x| x.0 == l.material) {
                                Some(x) => {
                                    x.1 += l.volume;
                                    x.2 += l.side_area;
                                }
                                None => mats.push((l.material, l.volume, l.side_area)),
                            }
                        }
                    }
                }
                for (mat, v, a) in mats {
                    let Some(x) = m.material(mat) else { continue };
                    if x.category == sk_model::MatCategory::Air {
                        continue;
                    }
                    let ins = x.category == sk_model::MatCategory::Insulation;
                    row([
                        &gb,
                        &sname,
                        &kg,
                        &format!("{title}: {}", x.name),
                        "",
                        "",
                        "",
                        &if ins { area(a) } else { String::new() },
                        &vol(v),
                        "",
                    ]);
                }
                if t.pocket > 0.0 {
                    let what = match g.category {
                        Category::ExteriorWall => "Abzug Deckenauflager",
                        _ => "Abzug Deckenstreifen",
                    };
                    row([
                        &gb,
                        &sname,
                        &kg,
                        &format!("{title}: {what}"),
                        "",
                        "",
                        "",
                        "",
                        &vol(-t.pocket),
                        "in der Decke enthalten",
                    ]);
                }
            }
        }
    }
    for b in &sched.buildings {
        let nr = m.building(b.id).map_or(String::new(), |x| x.number.clone());
        for x in &b.by_material {
            let name = m
                .material(x.material)
                .map_or(String::new(), |y| y.name.clone());
            row([
                &nr,
                "Summe nach Baustoff",
                "",
                &name,
                "",
                &x.length.map_or(String::new(), len),
                "",
                &x.area.map_or(String::new(), area),
                &if x.length.is_some() {
                    String::new()
                } else {
                    vol(x.volume)
                },
                "",
            ]);
        }
    }
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(out.as_bytes());
    bytes
}

#[cfg(test)]
#[path = "schedule_view_tests.rs"]
mod tests;

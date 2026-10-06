//! Mengenermittlung im zweiten Fenster (B7): die Liste aus
//! [`sk_model::qto::schedule`] als Blatt, mit Auf- und Zuklappen, Hover und
//! Auswahl über den gemeinsamen Zustand [`Picking`], Aufleuchten geänderter
//! Werte und „Als Tabelle speichern“ (.csv).
//!
//! Die Liste wird nie hier berechnet: [`Scene::schedule`] rechnet einmal je
//! Modellstand. Hover und Auswahl setzen nur das Blatt neu.

use crate::picking::Picking;
use crate::scene::Scene;
use sk_model::qto::{ElementQto, GroupQto, Schedule, StoreyQto};
use sk_model::{Category, ElementId, LevelKind, Model};
use sk_paint::font::Font;
use sk_paint::{Canvas, Path, Rgba};
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Kopf über der Liste (dip ab Unterkante der Titelleiste): Titelzeile,
/// Unterzeile, Spaltenköpfe und Linie; bleibt beim Rollen stehen.
const HEAD: f32 = 92.0;
/// Höhe einer Geschosszeile und Luft davor (dip).
const STOREY_ROW: f32 = 24.0;
const STOREY_GAP: f32 = 8.0;
/// Kacheln der Summe nach Baustoff (dip).
const TILE_H: f32 = 64.0;
const TILE_GAP: f32 = 12.0;
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

/// Schlüssel einer Zeile: zum Auf- und Zuklappen und zum Wiedererkennen nach
/// einer Neuberechnung (Aufleuchten).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
enum Key {
    #[default]
    None,
    Building(u32),
    Storey(u32),
    Group(u32, u8, u32),
    Element(u32, u32),
    Control(u32, u8, u32),
    Tile(u32, u32),
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
    /// Grund, warum kein Körper entsteht (grau, kursiv).
    note: Option<String>,
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
    /// Nur das Blatt neu zeichnen.
    Repaint,
}

/// Bauteilgruppe für Hover und Klick auf eine Gruppenzeile: Kurzname des
/// Geschosses der Gruppe, Bauteilart, Bauteile.
type GroupRef = (String, Category, Vec<ElementId>);

pub struct ListView {
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
    /// Zellen beim letzten Aufbau und ihr Aufleuchten.
    cells_old: HashMap<(Key, u8), String>,
    flash: HashMap<(Key, u8), Instant>,
    hot: Option<Hot>,
    button_down: bool,
    last_click: Option<(Instant, usize)>,
    last_tick: Option<Instant>,
}

impl ListView {
    /// Liste für das Modell der Szene (berechnet sie, falls nötig).
    pub fn new(s: &mut Scene) -> ListView {
        let mut v = ListView {
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
            cells_old: HashMap::new(),
            flash: HashMap::new(),
            hot: None,
            button_down: false,
            last_click: None,
            last_tick: None,
        };
        v.sync(s, false);
        v
    }

    /// An den Modellstand angleichen. `true`, wenn neu gezeichnet werden muss.
    /// `animate`: geänderte Werte leuchten auf.
    pub fn sync(&mut self, s: &mut Scene, animate: bool) -> bool {
        s.schedule();
        let runs = s.schedule_runs();
        let mut changed = false;
        if self.runs != Some(runs) {
            let sched = s.schedule().clone();
            self.rebuild(s.model(), &sched, animate && self.runs.is_some());
            self.runs = Some(runs);
            changed = true;
        }
        let stale = s.schedule_stale();
        if stale != self.stale.is_some() {
            self.stale = stale.then(Instant::now);
            changed = true;
        }
        changed
    }

    /// Zeilen neu aufbauen; Auf- und Zuklappen bleibt erhalten.
    fn rebuild(&mut self, m: &Model, sched: &Schedule, animate: bool) {
        let (lines, groups) = build_lines(m, sched);
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
        let (y, mo, d, h, mi) = sk_platform::local_date_time();
        let who = match sched.buildings.as_slice() {
            [b] => m
                .building(b.id)
                .map_or(String::new(), |x| format!("{} ({})", x.name, x.number)),
            [] => "Ohne Gebäude".to_string(),
            v => format!("{} Gebäude", v.len()),
        };
        self.subtitle = format!("{who} · Stand {d:02}.{mo:02}.{y}, {h:02}:{mi:02}");
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

    fn line_h(&self, l: &Line, t: Option<&Theme>) -> f32 {
        let row = t.map_or(22.0, |t| t.size.qto_row);
        match l.kind {
            Kind::Building => 30.0,
            Kind::Storey => STOREY_ROW + STOREY_GAP,
            Kind::Group | Kind::Row | Kind::Control => row,
            Kind::Rule => 16.0,
            Kind::SumHead => 30.0,
            Kind::Tiles => TILE_H + TILE_GAP + 8.0,
        }
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
        (self.h as f32 / self.scale - self.top_dip() - HEAD).max(0.0)
    }

    /// Unterkante der Titelleiste (dip).
    fn top_dip(&self) -> f32 {
        32.0
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
                p.click(id, ctrl);
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
        self.select_all(p, g);
    }

    fn select_all(&mut self, p: &mut Picking, g: Vec<ElementId>) {
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
        p.selected.clear();
        self.selected.clear();
        self.shown_primary = None;
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

    fn hit(&self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<Hot> {
        let (x, y) = (x as f32, y as f32);
        let s = self.scale;
        let (bx, by, bw, bh) = self.button_rect(t, fonts);
        if x >= bx && x < bx + bw && y >= by && y < by + bh {
            return Some(Hot::Button);
        }
        let list_top = (self.top_dip() + HEAD) * s;
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
        let repaint = hot != self.hot;
        self.hot = hot;
        let (one, group) = self.hover_of(hot);
        if p.set_hover(one, group) {
            self.hover = p.hovered().collect();
            return Some(ListOut::Picking { selection: false });
        }
        repaint.then_some(ListOut::Repaint)
    }

    pub fn mouse_leave(&mut self, p: &mut Picking) -> Option<ListOut> {
        self.hot = None;
        self.button_down = false;
        if p.set_hover(None, Vec::new()) {
            self.hover.clear();
            return Some(ListOut::Picking { selection: false });
        }
        Some(ListOut::Repaint)
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
        match hot {
            Some(Hot::Button) => {
                self.button_down = true;
                Some(ListOut::Repaint)
            }
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
                match l.kind {
                    Kind::Storey => {
                        if !self.closed_storeys.remove(&l.key) {
                            self.closed_storeys.insert(l.key);
                        }
                        self.clamp();
                        Some(ListOut::Repaint)
                    }
                    Kind::Group => {
                        self.select_all(p, l.elements);
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
    /// ganze Bilder nötig sind (die Pille „wird aktualisiert“ zählt nicht,
    /// siehe [`ListView::pill_key`]).
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
        busy |= !self.flash.is_empty();
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

    /// Blatt unter der Titelleiste; die Leinwand ist schon mit `sheet_bg` gefüllt.
    pub fn paint(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, now: Instant) {
        let s = self.scale;
        let top = self.top_dip() * s;
        let (x0, cw) = self.content_x(t);
        let u = &t.ui;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let italic = fonts.italic.as_ref().or(regular);
        let anim = t.size.anim_ms > 0.0;

        // Liste (unter dem Kopf, gerollt)
        let list_top = top + HEAD * s;
        let rows = self.layout(Some(t));
        let pad_band = 10.0 * s;
        for &(i, y, h) in &rows {
            let ys = list_top + (y - self.scroll) * s;
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
            self.paint_line(c, t, l, (x0, cw), band_y, band_h, fonts, now, anim);
        }
        // Kopf deckt die weggerollten Zeilen ab
        c.fill_rect(0.0, top, self.w as f32, HEAD * s, u.sheet_bg);
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
                &self.subtitle,
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
        // Spaltenköpfe und Linie
        if let Some(f) = regular {
            let px = 10.0 * s;
            let base = top + 79.0 * s;
            f.draw(c, "Bauteil", px, x0, base, u.sheet_text_dim);
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
        c.fill_rect(x0, top + 86.0 * s, cw, s.max(1.0), u.sheet_rule);
        // Laufleiste
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
            c.fill(&p, u.sheet_rule);
        }
        let _ = italic;
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
        // Bezeichnung endet vor der Nummernspalte
        let label = if l.cells[1].is_empty() {
            l.cells[0].clone()
        } else {
            let room = x0 + cw * COL_NR - 8.0 * s - text_x;
            sk_ui::widgets::ellipsize(Some(f), &l.cells[0], px, room)
        };
        f.draw(c, &label, px, text_x, base, col);
        if let Some(note) = &l.note {
            if let Some(fi) = italic {
                let nx = text_x + f.width(&label, px) + 10.0 * s;
                fi.draw(c, note, 10.5 * s, nx, base, u.sheet_hint);
            }
        }
        // Nummer gedämpft
        if let Some(fr) = regular {
            if !l.cells[1].is_empty() {
                fr.draw(
                    c,
                    &l.cells[1],
                    10.5 * s,
                    x0 + cw * COL_NR,
                    base,
                    u.sheet_text_dim,
                );
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
        let n = l.tiles.len().max(3) as f32;
        let gap = TILE_GAP * s;
        let tw = ((cw - gap * (n - 1.0)) / n).max(40.0 * s);
        for (k, tile) in l.tiles.iter().enumerate() {
            let x = x0 + k as f32 * (tw + gap);
            let mut p = Path::new();
            p.rounded_rect(x, y, tw, TILE_H * s, 6.0 * s);
            c.fill(&p, u.sheet_tile);
            if let Some(f) = fonts.regular.as_ref() {
                f.draw(
                    c,
                    &tile.name,
                    10.0 * s,
                    x + 10.0 * s,
                    y + 18.0 * s,
                    u.sheet_text_dim,
                );
            }
            if let Some(f) = fonts.bold.as_ref().or(fonts.regular.as_ref()) {
                let vx = x + 10.0 * s;
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

fn group_key(st: sk_model::StoreyId, i: usize, g: &GroupQto) -> Key {
    Key::Group(
        st.index(),
        i as u8,
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
    let (name, code) = match g.category {
        Category::ExteriorWall => ("Außenwände", "AW"),
        Category::InteriorWall => ("Innenwände", "IW"),
        c => (c.name(), ""),
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

fn build_lines(m: &Model, sched: &Schedule) -> (Vec<Line>, Vec<GroupRef>) {
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
        for st in &b.storeys {
            storey_lines(m, st, &mut lines, &mut groups);
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
                value: m_vol(x.volume),
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
    for (gi, g) in st.groups.iter().enumerate() {
        groups.push((
            short.clone(),
            g.category,
            g.rows.iter().map(|r| r.element).collect(),
        ));
        if is_wall(g.category) {
            let gkey = group_key(st.id, gi, g);
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
                    }
                    _ => {
                        rl.cells[0] = r.number.clone();
                        rl.cells[4] = "–".into();
                        rl.note = r.note.clone();
                    }
                }
                lines.push(rl);
            }
            if g.total.pocket > 0.0 {
                let mut cl = Line::new(Kind::Control, 2, Key::Control(st.id.index(), gi as u8, 0));
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
pub fn csv(m: &Model, sched: &Schedule) -> Vec<u8> {
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
                        String::new(),
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
                "",
                "",
                &x.area.map_or(String::new(), area),
                &vol(x.volume),
                "",
            ]);
        }
    }
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(out.as_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zahlen_deutsch() {
        assert_eq!(de(1234.5678, 3), "1.234,568");
        assert_eq!(de(-1.3159, 3), "−1,316");
        assert_eq!(de(0.0, 2), "0,00");
        assert_eq!(de(-0.0001, 2), "0,00");
        assert_eq!(cm(220.0), "22 cm");
        assert_eq!(cm(175.0), "17,5 cm");
        assert_eq!(csv_num(17.6), "17,6000");
        assert_eq!(csv_num(-1.31593), "-1,3159");
    }
}

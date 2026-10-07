//! Baumpanel (Paket 4 §1): Karten „Baum“, „Bauteil“, „Gewerk“ in der
//! rechten Spalte unter „Ansichten“. Zeilen aus [`sk_model::tree`], je
//! Zeile Auge, Schloss, Isolieren und Löschen. Das Panel rechnet nur aus,
//! was eine Handlung bewirken soll ([`Out`]); ausgeführt wird sie in der App.
//! Alle Maße kommen aus den Größenrollen (`tree_*`, `panel_*`), alle Farben
//! aus dem Farbschema.

use crate::picking::Picking;
use crate::scene::Scene;
use sk_model::tree::{self, Node, NodeKey, ProjectTree, State, Tab};
use sk_model::view::{Isolate, Visibility};
use sk_model::{ElementId, Guid, LevelKind, Model, Refusal};
use sk_paint::font::Font;
use sk_paint::{Canvas, Path};
use sk_platform::{Event, Modifiers, MouseButton};
use sk_ui::theme::{Sizes, Theme};
use sk_ui::widgets::{self, Fill, Fonts, Rect};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::time::Instant;

/// Zwei Klicks auf dieselbe Zeile innerhalb dieser Zeit: Doppelklick.
const DOUBLE_MS: u128 = 450;

/// Symbole einer Zeile, von rechts: Schloss, Auge, Isolieren, Löschen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Eye,
    Lock,
    Isolate,
    Delete,
}

/// Reihenfolge im Symbolstreifen, von rechts nach links.
pub const ICONS: [Icon; 4] = [Icon::Lock, Icon::Eye, Icon::Isolate, Icon::Delete];

/// Was eine Zeile an Symbolen zeigt und was davon wirkt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowIcons {
    /// Zeile überfahren: Isolieren und Löschen erscheinen.
    pub hover: bool,
    /// Schloss blass (abgeleitet: folgt der Quelle).
    pub lock_faded: bool,
    /// Löschen blass (Geschoss, Gelände, Abgeleitetes).
    pub delete_faded: bool,
}

/// Symbole, die eine Zeile haben kann, von rechts gepackt (§1.4, §1.5):
/// Projekt nur Schloss und Auge, Gelände nur das Auge, Karte „Bauteil“
/// ohne Löschen, in der Karte „Gewerk“ Auge und Isolieren nur an der
/// Gewerkzeile.
pub fn slots(tab: Tab, key: &NodeKey) -> &'static [Icon] {
    match (tab, key) {
        (Tab::Tree, NodeKey::Project) => &[Icon::Lock, Icon::Eye],
        (Tab::Tree, NodeKey::Terrain) => &[Icon::Eye],
        (Tab::Tree, _) => &ICONS,
        (Tab::Kind, _) => &[Icon::Lock, Icon::Eye, Icon::Isolate],
        (Tab::Trade, NodeKey::Trade(_)) => &[Icon::Eye, Icon::Isolate],
        (Tab::Trade, _) => &[],
    }
}

/// Ist das Symbol zu sehen (Isolieren und Löschen nur beim Überfahren)?
fn shown(i: Icon, row: &RowIcons) -> bool {
    !matches!(i, Icon::Isolate | Icon::Delete) || row.hover
}

/// Löst das Symbol etwas aus (da und nicht blass)?
fn live(i: Icon, row: &RowIcons) -> bool {
    shown(i, row)
        && match i {
            Icon::Lock => !row.lock_faded,
            Icon::Delete => !row.delete_faded,
            _ => true,
        }
}

/// Zeile unter `y` (Pixel ab Oberkante der Zeilenliste) bei Rollstand
/// `scroll` (Pixel), `rows` Zeilen der Höhe `row_h` (Pixel).
pub fn row_at_px(y: f64, scroll: f64, rows: usize, row_h: f64) -> Option<usize> {
    let at = y + scroll;
    if at < 0.0 || row_h <= 0.0 {
        return None;
    }
    let i = (at / row_h).floor() as usize;
    (i < rows).then_some(i)
}

/// Wie [`row_at_px`] mit der Zeilenhöhe der Größen `size` bei `scale`.
#[cfg(test)]
pub fn row_at_in(size: &Sizes, y: f64, scroll: f64, rows: usize, scale: f64) -> Option<usize> {
    row_at_px(y, scroll, rows, size.tree_row_h as f64 * scale)
}

/// Symbol unter `x` (Pixel links von der rechten Kante des Streifens) in
/// einer Zeile mit den Plätzen `slots`. Blasse Symbole und solche, die
/// nicht da sind, lösen nichts aus.
pub fn icon_at_in(
    size: &Sizes,
    x: f64,
    scale: f64,
    row: &RowIcons,
    slots: &[Icon],
) -> Option<Icon> {
    let icon = size.tree_icon as f64 * scale;
    let step = icon + size.tree_icon_gap as f64 * scale;
    if x < 0.0 || step <= 0.0 {
        return None;
    }
    let k = (x / step).floor() as usize;
    if x - k as f64 * step > icon {
        return None;
    }
    let i = *slots.get(k)?;
    live(i, row).then_some(i)
}

/// [`row_at_in`] mit den Vorgabegrößen (Abnahme A226).
#[cfg(test)]
pub fn row_at(y: f64, scroll: f64, rows: usize, scale: f64) -> Option<usize> {
    row_at_in(&sk_ui::theme::Theme::dark().size, y, scroll, rows, scale)
}

/// [`icon_at_in`] mit den Vorgabegrößen an einer Bauteilzeile der Karte
/// „Baum“ (Abnahme A226).
#[cfg(test)]
pub fn icon_at(x: f64, scale: f64, row: &RowIcons) -> Option<Icon> {
    icon_at_in(&sk_ui::theme::Theme::dark().size, x, scale, row, &ICONS)
}

/// Was unter der Maus liegt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Tab(Tab),
    /// Band beim Isolieren; `true`: „Beenden“.
    Band(bool),
    /// Hinweiszeile; `true`: „Alles zeigen“.
    Shown(bool),
    /// Sichtbare Zeile `i`: Fläche, Pfeil, Symbol.
    Row(usize),
    Arrow(usize),
    Icon(usize, Icon),
    /// Grenze zu den Eigenschaften.
    Divider,
    /// Panelfläche ohne Wirkung.
    Panel,
}

impl Hit {
    fn row(self) -> Option<usize> {
        match self {
            Hit::Row(i) | Hit::Arrow(i) | Hit::Icon(i, _) => Some(i),
            _ => None,
        }
    }
}

/// Handlung aus dem Panel, ausgeführt von der App.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Neue Auswahl (Reihenfolge des Wählens).
    Select(Vec<ElementId>),
    /// Geschosszeile: alles darunter wählen und das Geschoss zusätzlich
    /// aktiv machen.
    Storey(sk_model::StoreyId, Vec<ElementId>),
    /// Doppelklick: im Modell zeigen.
    Zoom(Vec<ElementId>),
    /// Rechtsklick: Kontextmenü am Bauteil `target` (Fensterkoordinaten).
    Context {
        target: ElementId,
        ids: Vec<ElementId>,
        x: f64,
        y: f64,
    },
    /// Neue Sichtbarkeit (mit Übergang).
    Visibility(Visibility),
    /// Sperren (`true`) bzw. Entsperren, ein Schritt.
    Lock(Vec<ElementId>, bool),
    /// Löschen wie Entf.
    Delete(Vec<ElementId>),
    /// „Gebäude löschen …“ mit Rückfrage.
    DeleteBuilding(sk_model::BuildingId),
    /// Eine Karte wurde gezeigt (Entdecken-Hinweise).
    Tab(Tab),
}

/// Ergebnis eines Ereignisses.
#[derive(Debug, Default)]
pub struct Out {
    /// Die Maus steht über dem Panel: das Ereignis gehört ihm.
    pub consumed: bool,
    pub action: Option<Action>,
    /// Höhe oder Lage hat sich geändert (Eigenschaften rücken).
    pub relayout: bool,
}

/// Laufender Übergang der Panelhöhe (Pixel).
#[derive(Clone, Copy, Debug)]
struct Grow {
    from: f32,
    to: f32,
    start: Instant,
}

/// Das Baumpanel: Zustand der Sitzung (Karte, offene Äste, Rollstand).
pub struct TreePanel {
    pub tab: Tab,
    /// Zugeklappt: nur die Kopfzeile (Klick auf die aktive Karte).
    pub collapsed: bool,
    open: [BTreeSet<NodeKey>; 3],
    /// Bekannte Äste der Karte „Baum“ (neue Gebäude und Geschosse gehen auf).
    seen: BTreeSet<NodeKey>,
    tree: ProjectTree,
    built: Option<u64>,
    /// Sichtbare Zeilen der aktiven Karte (Index in ihre Liste).
    rows: Vec<usize>,
    /// Rollstand (Pixel).
    scroll: f32,
    pub hover: Option<Hit>,
    /// Isolierter Ast (Karte, Schlüssel, Name fürs Band).
    isolated: Option<(Tab, NodeKey, String)>,
    /// Gezogene Höhe des Baums mit Auswahl (dip), gilt für die Sitzung und
    /// steht in `einstellungen.txt`.
    pub split: Option<f32>,
    /// Lage im Fenster (Pixel, ohne Schatten) und die Höhe, auf die es geht.
    pub rect: Rect,
    grow: Option<Grow>,
    /// Schloss, das nach dem Verweis der Hinweiskarte einmal leuchtet
    /// (Knoten der Karte „Baum“, Beginn).
    flash: Option<(usize, Instant)>,
    /// Mit Auswahl: die Grenze steht unter dem Panel.
    pub divider: bool,
    drag: Option<(f64, f32)>,
    anchor: Option<usize>,
    last_click: Option<(usize, Instant)>,
    /// Zuletzt gezeichneter Stand (neu zeichnen nur bei Änderung).
    look: Option<Look>,
    pub dirty: bool,
    /// Nur diese sichtbaren Zeilen neu (Hover, Leuchten; Review 3i B2).
    rows_dirty: Vec<usize>,
    /// Zeile, die zuletzt durch den Modell-Hover leuchtete (B1).
    glow: Option<usize>,
    /// Panelgrund je (Breite, Höhe, Grenze, Maßstab, Schema) (B3).
    base: RefCell<Option<(BaseKey, Canvas)>>,
    /// Letztes ganzes Bild und seine Panelhöhe; im Übergang die größere (B4).
    img: Option<(Canvas, f32)>,
    /// Höhe, auf die das zuletzt gezeigte Bild gekürzt war.
    cut_h: Option<f32>,
    /// Lage der Zeilenliste im letzten ganzen Bild.
    shown: Option<Shown>,
    /// Bildvergleiche (Befehlszeile): aufzuklappende Zeilen, überfahrene
    /// Zeile und zu isolierende Zeile, je nach Namensanfang.
    pub cli_open: Vec<String>,
    pub cli_hover: Option<String>,
    pub cli_isolate: Option<String>,
}

/// Was das Bild bestimmt, außer der Maus und der leuchtenden Zeile.
#[derive(Clone, Debug, PartialEq)]
struct Look {
    rev: u64,
    vis: Visibility,
    selected: Vec<ElementId>,
    theme: u64,
    scale: f32,
}

type BaseKey = (u32, u32, u32, u32, u64);

/// Zeilenliste im letzten ganzen Bild: Oberkante, Höhe (Pixel), sichtbare
/// Zeilen, Panelhöhe.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shown {
    top: f32,
    list_h: f32,
    first: usize,
    last: usize,
    h: f32,
}

/// Was eine Zeile zum Zeichnen braucht (ganz oder als Teilbild).
struct RowCx<'a> {
    m: &'a Model,
    t: &'a Theme,
    regular: Option<&'a Font>,
    bold: Option<&'a Font>,
    selected: BTreeSet<Guid>,
    model_hover: Option<Guid>,
    iso: bool,
    w: f32,
    s: f32,
    rh: f32,
    gap: f32,
    pad: f32,
    px: f32,
    cap: f32,
    inset: f32,
}

/// Bild des Panels für die App.
pub enum TreeOut {
    /// Ganzes Bild (an `rect - panel_shadow`).
    Full(Canvas),
    /// Teilbilder: Lage im ganzen Bild (Pixel) und Bild.
    Parts(Vec<(usize, usize, Canvas)>),
}

impl Default for TreePanel {
    fn default() -> Self {
        TreePanel::new()
    }
}

impl TreePanel {
    pub fn new() -> TreePanel {
        TreePanel {
            tab: Tab::Tree,
            collapsed: false,
            open: Default::default(),
            seen: BTreeSet::new(),
            tree: ProjectTree::default(),
            built: None,
            rows: Vec::new(),
            scroll: 0.0,
            hover: None,
            isolated: None,
            split: None,
            rect: Rect::default(),
            grow: None,
            flash: None,
            divider: false,
            drag: None,
            anchor: None,
            last_click: None,
            look: None,
            dirty: true,
            rows_dirty: Vec::new(),
            glow: None,
            base: RefCell::default(),
            img: None,
            cut_h: None,
            shown: None,
            cli_open: Vec::new(),
            cli_hover: None,
            cli_isolate: None,
        }
    }

    /// Sichtbare Zeile, deren Name mit `text` beginnt; „Name#2“ ist die
    /// zweite solche Zeile.
    fn row_named(&self, m: &Model, text: &str) -> Option<usize> {
        let (text, k) = match text.split_once('#') {
            Some((t, k)) => (t, k.parse::<usize>().unwrap_or(1).max(1)),
            None => (text, 1),
        };
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, &i)| named(m, &self.nodes()[i], text))
            .nth(k - 1)
            .map(|(vi, _)| vi)
    }

    /// Angaben der Befehlszeile anwenden, sobald die Karten stehen: Äste
    /// aufklappen, Zeile überfahren; Isolieren liefert die Handlung.
    pub fn apply_cli(&mut self, m: &Model) -> Option<Action> {
        self.built?;
        for text in std::mem::take(&mut self.cli_open) {
            let keys: Vec<NodeKey> = self
                .nodes()
                .iter()
                .filter(|n| named(m, n, &text))
                .map(|n| n.key.clone())
                .collect();
            self.open[self.tab.index()].extend(keys);
            self.refresh_rows();
        }
        if let Some(text) = self.cli_hover.take() {
            self.hover = self.row_named(m, &text).map(Hit::Row);
            self.dirty = true;
        }
        let text = self.cli_isolate.take()?;
        let vi = self.row_named(m, &text)?;
        self.icon(m, vi, Icon::Isolate)
    }

    /// Neues Projekt geöffnet: Äste wie beim Öffnen (Projekt, Gebäude,
    /// Geschosse auf, Gruppen zu), oben.
    pub fn reset(&mut self) {
        self.open = Default::default();
        self.seen.clear();
        self.built = None;
        self.scroll = 0.0;
        self.isolated = None;
        self.anchor = None;
        self.dirty = true;
    }

    fn nodes(&self) -> &[Node] {
        self.tree.tab(self.tab)
    }

    /// Baut die Karten neu, wenn sich das Modell geändert hat, und hält
    /// Isolieren und Bild aktuell. `true`, wenn neu zu zeichnen ist.
    pub fn sync(&mut self, scene: &Scene, picking: &Picking, t: &Theme, scale: f32) -> bool {
        let m = scene.model();
        let rev = m.revision();
        if self.built != Some(rev) {
            self.tree = tree::build(m);
            self.built = Some(rev);
            // Neue Gebäude und Geschosse gehen auf (beim Öffnen alle)
            for n in self.tree.tab(Tab::Tree) {
                let opens = matches!(
                    n.key,
                    NodeKey::Project | NodeKey::Building(_) | NodeKey::Storey(_)
                );
                if opens && self.seen.insert(n.key.clone()) {
                    self.open[0].insert(n.key.clone());
                }
            }
            self.refresh_rows();
        }
        if m.visibility().isolate.is_none() {
            self.isolated = None;
        }
        let look = Look {
            rev,
            vis: m.visibility().clone(),
            selected: picking.selected.clone(),
            theme: t.rev,
            scale,
        };
        if self.look.as_ref() != Some(&look) {
            self.look = Some(look);
            self.dirty = true;
        }
        // Leuchtende Zeile (Modell-Hover): nur die zwei Zeilen neu (B1, B2)
        let glow = self.glow_row(m, picking);
        if glow != self.glow {
            self.rows_dirty.extend(self.glow.into_iter().chain(glow));
            self.glow = glow;
        }
        self.dirty || !self.rows_dirty.is_empty()
    }

    fn refresh_rows(&mut self) {
        let nodes = self.tree.tab(self.tab);
        let open = &self.open[self.tab.index()];
        let mut rows = Vec::new();
        let mut i = 0;
        while i < nodes.len() {
            rows.push(i);
            let n = &nodes[i];
            i = if n.has_children(i) && !open.contains(&n.key) {
                n.end
            } else {
                i + 1
            };
        }
        self.rows = rows;
        self.dirty = true;
    }

    /// Einzug (Stufen) einer Zeile: in der Karte „Baum“ stehen Gelände und
    /// Gebäude ohne Einzug unter dem Projekt (p34 §6.1).
    fn indent(&self, n: &Node) -> f32 {
        match self.tab {
            Tab::Tree => n.depth.saturating_sub(1) as f32,
            _ => n.depth as f32,
        }
    }

    // --- Maße -------------------------------------------------------------

    /// Höhe der Kopfzeile mit den Karten (Pixel).
    fn header(&self, z: &Sizes, s: f32) -> f32 {
        ((z.tree_row_h + z.panel_pad) * s).round()
    }

    /// Höhe des Bands bzw. der Hinweiszeile über den Zeilen (Pixel).
    fn extra(&self, m: &Model, z: &Sizes, s: f32) -> f32 {
        if self.isolating(m) {
            ((z.tree_row_h + 3.0 * z.tree_icon_gap) * s).round()
        } else if hidden_elsewhere(m, self.tab).is_some() {
            (z.tree_row_h * s).round()
        } else {
            0.0
        }
    }

    /// Oberkante der Zeilenliste im Panel (Pixel).
    fn list_top(&self, m: &Model, z: &Sizes, s: f32) -> f32 {
        self.header(z, s) + (z.tree_icon_gap * s).round() + self.extra(m, z, s)
    }

    /// Höhe des zugeklappten Panels bzw. die Mindesthöhe (Kopf und vier
    /// Zeilen), Pixel.
    pub fn min_height(&self, z: &Sizes, s: f32) -> f32 {
        let head = self.header(z, s) + (z.tree_icon_gap * s).round();
        if self.collapsed {
            head
        } else {
            head + 4.0 * (z.tree_row_h * s).round()
        }
    }

    fn isolating(&self, m: &Model) -> bool {
        m.visibility().isolate.is_some() && !self.collapsed
    }

    /// Größter Rollstand (Pixel) bei Listenhöhe `list_h`.
    fn max_scroll(&self, z: &Sizes, s: f32, list_h: f32) -> f32 {
        (self.rows.len() as f32 * (z.tree_row_h * s).round() - list_h).max(0.0)
    }

    // --- Höhe -------------------------------------------------------------

    /// Stellt Lage und Zielhöhe ein (Pixel); mit `animate` (Auswahl kam
    /// oder ging) gleitet die Höhe in `anim_ms` dorthin. `true`, wenn sich
    /// etwas ändert.
    pub fn place(&mut self, r: Rect, t: &Theme, now: Instant, animate: bool) -> bool {
        let target = self.grow.map_or(self.rect.h, |g| g.to);
        if r == Rect::new(self.rect.x, self.rect.y, self.rect.w, target) {
            return false;
        }
        let cur = self.height(now, t);
        let animate = animate
            && self.rect.h > 0.0
            && t.size.anim_ms > 0.0
            && self.drag.is_none()
            && (cur - r.h).abs() >= 1.0;
        self.grow = animate.then_some(Grow {
            from: cur,
            to: r.h,
            start: now,
        });
        self.rect = r;
        self.dirty = true;
        true
    }

    /// Steht ein neues Bild aus (ganz oder Zeilen)?
    pub fn needs_paint(&self) -> bool {
        self.dirty || !self.rows_dirty.is_empty()
    }

    /// Gleitet die Höhe gerade?
    pub fn is_growing(&self) -> bool {
        self.grow.is_some() || self.flash.is_some()
    }

    /// Höhe in diesem Bild (Pixel).
    pub fn height(&self, now: Instant, t: &Theme) -> f32 {
        let Some(g) = self.grow else {
            return self.rect.h;
        };
        let ms = t.size.anim_ms.max(1.0);
        let k = (now.duration_since(g.start).as_secs_f32() * 1000.0 / ms).min(1.0);
        let e = 1.0 - (1.0 - k).powi(3);
        g.from + (g.to - g.from) * e
    }

    /// Läuft der Übergang der Höhe noch? Endet er, ist er vorbei.
    pub fn growing(&mut self, now: Instant, t: &Theme) -> bool {
        let ms = |at: Instant| now.duration_since(at).as_secs_f32() * 1000.0;
        // Leuchtendes Schloss: nur seine Zeile neu
        if let Some((i, at)) = self.flash {
            match self.rows.iter().position(|&r| r == i) {
                Some(vi) => self.rows_dirty.push(vi),
                None => self.dirty = true,
            }
            if ms(at) >= t.size.flash_ms {
                self.flash = None;
            }
        }
        let Some(g) = self.grow else {
            return self.flash.is_some();
        };
        // Die Höhe gleitet ohne neues Malen ([`TreePanel::render`], B4)
        if ms(g.start) >= t.size.anim_ms {
            self.grow = None;
        }
        true
    }

    /// Verweis „Entsperren im Baum mit dem Schloss“: Karte „Baum“, Ast zum
    /// Bauteil auf, hinrollen, sein Schloss leuchtet einmal (`flash_ms`).
    pub fn flash_lock(&mut self, m: &Model, id: ElementId, t: &Theme, s: f32, now: Instant) {
        self.collapsed = false;
        if self.tab != Tab::Tree {
            self.tab = Tab::Tree;
            self.refresh_rows();
        }
        self.reveal(m, id, t, s);
        let g = m.element(id).map(|e| e.guid);
        self.flash = self
            .nodes()
            .iter()
            .position(|n| matches!(n.key, NodeKey::Element(x) if Some(x) == g))
            .map(|i| (i, now));
        self.dirty = true;
    }

    // --- Treffer ----------------------------------------------------------

    /// Liegt `(x, y)` (Fenster) auf dem Panel oder seiner Grenze?
    pub fn over(&self, x: f64, y: f64, t: &Theme, s: f32) -> bool {
        self.rect.contains(x, y) || self.on_divider(x, y, t, s)
    }

    fn on_divider(&self, x: f64, y: f64, t: &Theme, s: f32) -> bool {
        if !self.divider || self.collapsed {
            return false;
        }
        let r = self.rect;
        let m = (t.size.panel_margin * s).round();
        Rect::new(r.x, r.y + r.h, r.w, m).contains(x, y)
    }

    /// Was unter `(x, y)` (Fenster) liegt.
    pub fn hit(&self, m: &Model, x: f64, y: f64, t: &Theme, s: f32) -> Option<Hit> {
        if self.on_divider(x, y, t, s) {
            return Some(Hit::Divider);
        }
        let r = self.rect;
        if !r.contains(x, y) {
            return None;
        }
        let z = &t.size;
        let (lx, ly) = ((x - r.x as f64) as f32, (y - r.y as f64) as f32);
        let pad = z.panel_pad * s;
        let head = self.header(z, s);
        if ly < head {
            let col = ((lx - pad) / ((r.w - 2.0 * pad) / 3.0)).floor();
            let i = col.clamp(0.0, 2.0) as usize;
            return Some(Hit::Tab(Tab::ALL[i]));
        }
        if self.collapsed {
            return Some(Hit::Panel);
        }
        let top0 = head + (z.tree_icon_gap * s).round();
        let top = self.list_top(m, z, s);
        if ly < top {
            let right = lx > r.w * 0.5;
            return Some(if self.isolating(m) {
                Hit::Band(right)
            } else if ly >= top0 {
                Hit::Shown(right)
            } else {
                Hit::Panel
            });
        }
        let rh = (z.tree_row_h * s).round();
        let Some(vi) = row_at_px(
            (ly - top) as f64,
            self.scroll as f64,
            self.rows.len(),
            rh as f64,
        ) else {
            return Some(Hit::Panel);
        };
        let i = self.rows[vi];
        let n = &self.nodes()[i];
        let from_right = (r.w - pad - lx) as f64;
        let icons = self.row_icons(m, i, true);
        if let Some(ic) = icon_at_in(z, from_right, s as f64, &icons, slots(self.tab, &n.key)) {
            return Some(Hit::Icon(vi, ic));
        }
        if n.has_children(i) {
            let ax = self.arrow_x(n, z, s);
            if (lx - ax).abs() <= z.tree_indent * s * 0.5 + z.tree_icon_gap * s * 0.5 {
                return Some(Hit::Arrow(vi));
            }
        }
        Some(Hit::Row(vi))
    }

    fn arrow_x(&self, n: &Node, z: &Sizes, s: f32) -> f32 {
        (z.panel_pad + z.tree_icon_gap + z.tree_indent * self.indent(n)) * s
    }

    fn text_x(&self, n: &Node, z: &Sizes, s: f32) -> f32 {
        self.arrow_x(n, z, s) + (z.tree_indent - 0.5 * z.tree_icon_gap) * s
    }

    /// Symbole der Zeile `i` (Index in die Karte): blass, was nicht wirkt.
    fn row_icons(&self, m: &Model, i: usize, hover: bool) -> RowIcons {
        let n = &self.nodes()[i];
        let ids = self.tree.elements(self.tab, i);
        let lock_faded = lock_faded(m, &self.tree, self.tab, i);
        let delete_faded = match n.key {
            NodeKey::Project | NodeKey::Terrain | NodeKey::Storey(_) => true,
            NodeKey::Building(_) => false,
            _ => {
                !ids.is_empty()
                    && ids
                        .iter()
                        .all(|&id| matches!(m.can_delete(id), Err(Refusal::Derived { .. })))
            }
        };
        RowIcons {
            hover,
            lock_faded,
            delete_faded,
        }
    }

    // --- Ereignisse -------------------------------------------------------

    /// Verarbeitet ein Mausereignis (Fensterkoordinaten). Über dem Panel
    /// gehört es ihm (`consumed`).
    pub fn handle(
        &mut self,
        e: &Event,
        scene: &Scene,
        picking: &Picking,
        t: &Theme,
        s: f32,
    ) -> Out {
        let m = scene.model();
        let mut out = Out::default();
        if let Some((y0, h0)) = self.drag {
            out.consumed = true;
            match *e {
                Event::MouseMove { y, .. } => {
                    let dip = (h0 + (y - y0) as f32) / s;
                    if self.split != Some(dip) {
                        self.split = Some(dip);
                        out.relayout = true;
                    }
                }
                Event::MouseUp { .. } | Event::MouseLeave => self.drag = None,
                _ => {}
            }
            return out;
        }
        match *e {
            Event::MouseMove { x, y, .. } => {
                let hit = self.hit(m, x, y, t, s);
                self.set_hover(hit);
                out.consumed = hit.is_some();
            }
            Event::MouseLeave => self.set_hover(None),
            Event::Wheel { delta, x, y, .. } => {
                if !self.over(x, y, t, s) {
                    return out;
                }
                out.consumed = true;
                let z = &t.size;
                let rh = (z.tree_row_h * s).round();
                let list_h = self.rect.h - self.list_top(m, z, s) - (z.tree_icon_gap * s).round();
                let to = (self.scroll - delta as f32 * 3.0 * rh)
                    .clamp(0.0, self.max_scroll(z, s, list_h));
                if to != self.scroll {
                    self.scroll = to;
                    self.dirty = true;
                    // Die Zeile unter der Maus wechselt
                    self.hover = self.hit(m, x, y, t, s);
                }
            }
            Event::MouseDown {
                button, x, y, mods, ..
            } => {
                let Some(hit) = self.hit(m, x, y, t, s) else {
                    return out;
                };
                out.consumed = true;
                if button == MouseButton::Right {
                    out.action = hit.row().and_then(|vi| self.context(m, vi, x, y));
                    return out;
                }
                if button != MouseButton::Left {
                    return out;
                }
                out.action = self.press(hit, m, picking, mods, &mut out.relayout, x, y);
            }
            Event::MouseUp { x, y, .. } => out.consumed = self.over(x, y, t, s),
            _ => {}
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn press(
        &mut self,
        hit: Hit,
        m: &Model,
        picking: &Picking,
        mods: Modifiers,
        relayout: &mut bool,
        _x: f64,
        y: f64,
    ) -> Option<Action> {
        self.dirty = true;
        match hit {
            Hit::Tab(tab) => {
                if tab == self.tab {
                    self.collapsed = !self.collapsed;
                } else {
                    self.tab = tab;
                    self.collapsed = false;
                    self.scroll = 0.0;
                    self.anchor = None;
                    self.refresh_rows();
                }
                *relayout = true;
                Some(Action::Tab(tab))
            }
            Hit::Band(true) => Some(Action::Visibility(Visibility {
                isolate: None,
                ..m.visibility().clone()
            })),
            Hit::Shown(true) => Some(Action::Visibility(show_all(m))),
            Hit::Arrow(vi) => {
                let key = self.nodes()[self.rows[vi]].key.clone();
                let open = &mut self.open[self.tab.index()];
                if !open.remove(&key) {
                    open.insert(key);
                }
                self.refresh_rows();
                None
            }
            Hit::Icon(vi, ic) => self.icon(m, vi, ic),
            Hit::Row(vi) => self.click_row(m, picking, vi, mods),
            Hit::Divider => {
                self.drag = Some((y, self.rect.h));
                None
            }
            _ => None,
        }
    }

    /// Klick auf eine Zeile: wählt (Strg ergänzt, Umschalt Bereich);
    /// Doppelklick zeigt im Modell, eine Geschosszeile macht das Geschoss
    /// aktiv.
    fn click_row(
        &mut self,
        m: &Model,
        picking: &Picking,
        vi: usize,
        mods: Modifiers,
    ) -> Option<Action> {
        let i = self.rows[vi];
        let ids = self.tree.elements(self.tab, i).to_vec();
        let now = Instant::now();
        let double = self
            .last_click
            .is_some_and(|(j, at)| j == vi && now.duration_since(at).as_millis() < DOUBLE_MS);
        self.last_click = (!double).then_some((vi, now));
        if double {
            return (!ids.is_empty()).then_some(Action::Zoom(ids));
        }
        if let NodeKey::Storey(g) = self.nodes()[i].key {
            if !mods.ctrl && !mods.shift {
                if let Some(sid) = m.storeys().iter().find(|(_, st)| st.guid == g).map(|x| x.0) {
                    // Wählt zugleich alles darunter
                    self.anchor = Some(vi);
                    return Some(Action::Storey(sid, ids));
                }
            }
        }
        let mut sel = picking.selected.clone();
        if mods.shift {
            if let Some(a) = self.anchor.filter(|&a| a < self.rows.len()) {
                let (lo, hi) = (a.min(vi), a.max(vi));
                let range: Vec<ElementId> = (lo..=hi)
                    .filter(|&v| {
                        matches!(
                            self.nodes()[self.rows[v]].key,
                            NodeKey::Element(_) | NodeKey::TradeElement(..)
                        )
                    })
                    .flat_map(|v| self.tree.elements(self.tab, self.rows[v]).to_vec())
                    .collect();
                if !mods.ctrl {
                    sel.clear();
                }
                for id in range {
                    if !sel.contains(&id) {
                        sel.push(id);
                    }
                }
                return Some(Action::Select(sel));
            }
        }
        self.anchor = Some(vi);
        if mods.ctrl {
            let all = !ids.is_empty() && ids.iter().all(|id| sel.contains(id));
            if all {
                sel.retain(|id| !ids.contains(id));
            } else {
                for id in ids {
                    if !sel.contains(&id) {
                        sel.push(id);
                    }
                }
            }
            return Some(Action::Select(sel));
        }
        if ids.is_empty() {
            return None;
        }
        Some(Action::Select(ids))
    }

    /// Bauteile der Zeile `vi`: Auswahl nach einem Klick auf eine
    /// Geschosszeile (die App wählt sie nach dem Wechsel des Geschosses).
    pub fn row_elements(&self, vi: usize) -> Vec<ElementId> {
        self.rows
            .get(vi)
            .map_or(Vec::new(), |&i| self.tree.elements(self.tab, i).to_vec())
    }

    /// Rechtsklick: Kontextmenü am ersten Bauteil der Zeile.
    fn context(&self, m: &Model, vi: usize, x: f64, y: f64) -> Option<Action> {
        let ids = self.row_elements(vi);
        let target = *ids.first()?;
        m.element(target)?;
        Some(Action::Context { target, ids, x, y })
    }

    /// Klick auf ein Symbol.
    fn icon(&mut self, m: &Model, vi: usize, ic: Icon) -> Option<Action> {
        let i = self.rows[vi];
        let n = self.nodes()[i].clone();
        let ids = self.tree.elements(self.tab, i).to_vec();
        match ic {
            Icon::Eye => Some(Action::Visibility(self.eye_toggle(m, i))),
            Icon::Lock => {
                let mut src: Vec<ElementId> = Vec::new();
                for &id in &ids {
                    let s = m.lock_source(id);
                    if !src.contains(&s) {
                        src.push(s);
                    }
                }
                if src.is_empty() {
                    return None;
                }
                let all = src.iter().all(|&id| m.is_locked(id));
                Some(Action::Lock(src, !all))
            }
            Icon::Isolate => {
                let v = m.visibility().clone();
                let same = self
                    .isolated
                    .as_ref()
                    .is_some_and(|(t, k, _)| *t == self.tab && *k == n.key);
                if same && v.isolate.is_some() {
                    self.isolated = None;
                    return Some(Action::Visibility(Visibility { isolate: None, ..v }));
                }
                let iso = match n.key {
                    NodeKey::Kind(c) => Isolate::Category(c),
                    NodeKey::Trade(tr) => Isolate::Trade(tr),
                    _ => {
                        let g: BTreeSet<Guid> = ids
                            .iter()
                            .filter_map(|&id| m.element(id).map(|e| e.guid))
                            .collect();
                        if g.is_empty() {
                            return None;
                        }
                        Isolate::Elements(g)
                    }
                };
                self.isolated = Some((self.tab, n.key.clone(), band_name(m, &n)));
                Some(Action::Visibility(Visibility {
                    isolate: Some(iso),
                    ..v
                }))
            }
            Icon::Delete => match n.key {
                NodeKey::Building(g) => m
                    .buildings()
                    .iter()
                    .find(|(_, b)| b.guid == g)
                    .map(|(id, _)| Action::DeleteBuilding(id)),
                _ => (!ids.is_empty()).then_some(Action::Delete(ids)),
            },
        }
    }

    /// Neue Sichtbarkeit nach einem Klick aufs Auge der Zeile `i`.
    fn eye_toggle(&self, m: &Model, i: usize) -> Visibility {
        let mut v = m.visibility().clone();
        let n = &self.nodes()[i];
        let hide = tree::eye(m, &self.tree, self.tab, i) != State::None;
        let guids: Vec<Guid> = self
            .tree
            .elements(self.tab, i)
            .iter()
            .filter_map(|&id| m.element(id).map(|e| e.guid))
            .collect();
        match (self.tab, &n.key) {
            (_, NodeKey::Terrain) => v.terrain_hidden = hide,
            (Tab::Tree, NodeKey::Project) if !hide => return show_all(m),
            (Tab::Kind, NodeKey::Kind(c)) => {
                if hide {
                    v.hidden_cat.insert(*c);
                } else {
                    v.hidden_cat.remove(c);
                    for g in &guids {
                        v.hidden.remove(g);
                    }
                }
            }
            (Tab::Trade, NodeKey::Trade(tr)) => {
                if hide {
                    v.hidden_trade.insert(*tr);
                } else {
                    v.hidden_trade.remove(tr);
                }
            }
            _ => {
                for g in guids {
                    if hide {
                        v.hidden.insert(g);
                    } else {
                        v.hidden.remove(&g);
                    }
                }
                if n.key == NodeKey::Project {
                    v.terrain_hidden = hide;
                }
            }
        }
        v
    }

    /// Bauteile der Zeile unter der Maus (leuchten im Modell).
    pub fn hover_ids(&self) -> Vec<ElementId> {
        self.hover
            .and_then(Hit::row)
            .map_or(Vec::new(), |vi| self.row_elements(vi))
    }

    /// Auswahl im Modell: Ast aufklappen und zur Zeile rollen (wie
    /// `ListView::reveal`).
    pub fn reveal(&mut self, m: &Model, id: ElementId, t: &Theme, s: f32) {
        let Some(g) = m.element(id).map(|e| e.guid) else {
            return;
        };
        let nodes = self.tree.tab(self.tab);
        let Some(at) = nodes.iter().position(|n| match n.key {
            NodeKey::Element(x) | NodeKey::TradeElement(_, x) => x == g,
            _ => false,
        }) else {
            return;
        };
        // Vorfahren: Äste, die `at` umfassen
        let open = &mut self.open[self.tab.index()];
        let mut changed = false;
        for n in nodes.iter().take(at) {
            if n.end > at && !open.contains(&n.key) {
                open.insert(n.key.clone());
                changed = true;
            }
        }
        if changed {
            self.refresh_rows();
        }
        let Some(vi) = self.rows.iter().position(|&i| i == at) else {
            return;
        };
        let z = &t.size;
        let rh = (z.tree_row_h * s).round();
        let list_h = self.rect.h - self.list_top(m, z, s) - (z.tree_icon_gap * s).round();
        let (y0, y1) = (vi as f32 * rh, (vi + 1) as f32 * rh);
        let to = if y0 < self.scroll {
            y0
        } else if y1 > self.scroll + list_h {
            y1 - list_h
        } else {
            self.scroll
        };
        let to = to.clamp(0.0, self.max_scroll(z, s, list_h));
        if to != self.scroll {
            self.scroll = to;
        }
        self.dirty = true;
    }

    /// Hinweis an der Maus für das, was unter ihr liegt.
    pub fn tip(&self, m: &Model) -> Option<String> {
        let hit = self.hover?;
        match hit {
            Hit::Band(true) => return Some("Isolieren beenden\nEsc".into()),
            Hit::Shown(true) => return Some("Alles zeigen".into()),
            _ => {}
        }
        let vi = hit.row()?;
        let i = *self.rows.get(vi)?;
        let n = self.nodes().get(i)?;
        let ids = self.tree.elements(self.tab, i);
        match hit {
            Hit::Icon(_, Icon::Eye) => Some(
                if tree::eye(m, &self.tree, self.tab, i) == State::None {
                    "Einblenden"
                } else {
                    "Ausblenden"
                }
                .into(),
            ),
            Hit::Icon(_, Icon::Lock) => Some(
                if lock_state(m, ids) == State::Full {
                    "Entsperren"
                } else {
                    "Sperren"
                }
                .into(),
            ),
            Hit::Icon(_, Icon::Isolate) => Some(
                if self
                    .isolated
                    .as_ref()
                    .is_some_and(|(t, k, _)| *t == self.tab && *k == n.key)
                {
                    "Isolieren beenden".into()
                } else {
                    "Isolieren\nNur diese Bauteile deckend".into()
                },
            ),
            Hit::Icon(_, Icon::Delete) => Some("Löschen".into()),
            Hit::Row(_) | Hit::Arrow(_) => {
                // Blasse Symbole sagen beim Überfahren der Zeile, warum
                // (Lage des Symbols ist kein Treffer)
                row_tip(m, n, ids)
            }
            _ => None,
        }
    }

    /// Hinweis an einem blassen Symbol (Schloss bzw. Löschen) unter `x`
    /// (Fenster), sonst `None`.
    pub fn faded_tip(&self, m: &Model, x: f64, t: &Theme, s: f32) -> Option<String> {
        let vi = self.hover.and_then(Hit::row)?;
        let i = *self.rows.get(vi)?;
        let n = self.nodes().get(i)?;
        let z = &t.size;
        let from_right = (self.rect.x + self.rect.w - z.panel_pad * s) as f64 - x;
        let icons = self.row_icons(m, i, true);
        let step = ((z.tree_icon + z.tree_icon_gap) * s) as f64;
        if from_right < 0.0 {
            return None;
        }
        let k = (from_right / step).floor() as usize;
        if from_right - k as f64 * step > (z.tree_icon * s) as f64 {
            return None;
        }
        let ids = self.tree.elements(self.tab, i);
        match *slots(self.tab, &n.key).get(k)? {
            Icon::Lock if icons.lock_faded => {
                let id = *ids.first()?;
                let src = m.lock_source(id);
                Some(format!("Folgt {}", m.element(src)?.number))
            }
            Icon::Delete if icons.delete_faded => Some(delete_reason(m, n, ids)),
            _ => None,
        }
    }

    // --- Zeichnen ---------------------------------------------------------

    /// Neue Lage der Maus im Panel. Wechselt nur die Zeile unter ihr, werden
    /// nur die beiden Zeilen neu gemalt (B2); die durch den Modell-Hover
    /// leuchtende Zeile erlischt, solange die Maus im Panel ist.
    fn set_hover(&mut self, hit: Option<Hit>) {
        if hit == self.hover {
            return;
        }
        let rowish = |h: Option<Hit>| match h {
            None | Some(Hit::Panel) => true,
            Some(h) => h.row().is_some(),
        };
        if rowish(self.hover) && rowish(hit) {
            self.rows_dirty.extend(
                self.hover
                    .and_then(Hit::row)
                    .into_iter()
                    .chain(hit.and_then(Hit::row)),
            );
            if self.hover.is_none() != hit.is_none() {
                self.rows_dirty.extend(self.glow);
            }
        } else {
            self.dirty = true;
        }
        self.hover = hit;
    }

    /// Zeichnet das Panel (Höhe `h` Pixel, ohne Schatten) samt Schatten und
    /// Grenze darunter. Liefert das Bild; es liegt bei
    /// `rect - panel_shadow`.
    pub fn paint(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        scene: &Scene,
        picking: &Picking,
        h: f32,
        s: f32,
    ) -> Canvas {
        self.dirty = false;
        self.rows_dirty.clear();
        let m = scene.model();
        let z = &t.size;
        let u = &t.ui;
        let sh = (z.panel_shadow * s).round();
        let w = self.rect.w;
        let grip = self.grip(z, s);
        // Panelgrund je Größe, Schema und Maßstab einmal (Review 3i B3)
        let key = (w.to_bits(), h.to_bits(), grip.to_bits(), s.to_bits(), t.rev);
        let mut c = {
            let mut base = self.base.borrow_mut();
            if base.as_ref().map(|b| b.0) != Some(key) {
                let mut c = Canvas::new((w + 2.0 * sh) as usize, (h + 2.0 * sh + grip) as usize);
                widgets::panel(&mut c, Rect::new(sh, sh, w, h), s, t);
                *base = Some((key, c));
            }
            base.as_ref().map(|b| b.1.clone()).expect("eben gesetzt")
        };
        let cx = self.row_cx(t, fonts, m, picking, s);
        let (regular, bold) = (cx.regular, cx.bold);
        let pad = cx.pad;
        let gap = cx.gap;
        let px = cx.px;
        let head = self.header(z, s);
        // Karten
        let colw = (w - 2.0 * pad) / 3.0;
        let cap = cx.cap;
        let ty = sh + ((pad * 0.5 + head - gap + cap) * 0.5).round();
        for (k, tab) in Tab::ALL.into_iter().enumerate() {
            let active = tab == self.tab;
            let label = tab_label(tab);
            let f = if active { bold } else { regular };
            let tw = f.map_or(0.0, |f| f.width(label, px));
            let cx = sh + pad + colw * (k as f32 + 0.5);
            let hover = self.hover == Some(Hit::Tab(tab));
            let col = if active || hover { u.text } else { u.text_dim };
            widgets::text(&mut c, f, label, px, cx - tw * 0.5, ty, col);
            if active {
                let lw = tw + 2.0 * gap;
                let lh = (0.75 * gap).max(2.0 * s).round();
                let mut p = Path::new();
                p.rounded_rect(cx - lw * 0.5, sh + head - lh - s.round(), lw, lh, lh * 0.5);
                c.fill(&p, u.accent);
            }
        }
        widgets::separator(&mut c, sh + pad, sh + head, w - 2.0 * pad, s, t);
        self.shown = None;
        if self.collapsed {
            return c;
        }
        let inset = cx.inset;
        let top0 = head + gap;
        let rh = cx.rh;
        // Band beim Isolieren bzw. Hinweiszeile
        if self.isolating(m) {
            let bh = rh + gap;
            let by = sh + top0 + gap;
            let mut p = Path::new();
            p.rounded_rect(
                sh + inset,
                by,
                w - 2.0 * inset,
                bh,
                z.corner_radius * s * 0.6,
            );
            c.fill(&p, u.isolate_band);
            let name = match &self.isolated {
                Some((_, _, n)) => n.clone(),
                None => isolate_name(m),
            };
            let end = "Beenden";
            let ew = regular.map_or(0.0, |f| f.width(end, px));
            let base = by + ((bh + cap) * 0.5).round();
            let left = sh + pad - gap;
            let right = sh + w - pad - gap;
            let room = right - ew - 2.0 * gap - left;
            let label = widgets::ellipsize(bold, &format!("Nur {name}"), px, room);
            widgets::text(&mut c, bold, &label, px, left, base, u.on_accent);
            widgets::text(&mut c, regular, end, px, right - ew, base, u.on_accent);
        } else if let Some(text) = hidden_elsewhere(m, self.tab) {
            // Klein wie die Angaben rechts; passt es nicht, etwas kleiner
            // (bis 85 %), erst dann gekürzt
            let all = "Alles zeigen";
            let left = sh + pad - gap;
            let right = sh + w - pad;
            let width =
                |px: f32| regular.map_or(0.0, |f| f.width(&text, px) + f.width(all, px)) + gap;
            let mut spx = z.tree_small * s;
            let need = width(spx);
            if need > right - left {
                spx = (spx * (right - left) / need).max(0.85 * spx);
            }
            let aw = regular.map_or(0.0, |f| f.width(all, spx));
            let base = sh + top0 + ((rh + cap) * 0.5).round();
            let room = right - aw - gap - left;
            let text = widgets::ellipsize(regular, &text, spx, room);
            widgets::text(&mut c, regular, &text, spx, left, base, u.text_dim);
            let col = if self.hover == Some(Hit::Shown(true)) {
                u.accent_hover
            } else {
                u.accent
            };
            widgets::text(&mut c, regular, all, spx, right - aw, base, col);
        }
        // Zeilen in eigenem Bild, damit angeschnittene oben und unten
        // sauber enden
        let top = self.list_top(m, z, s);
        let list_h = (h - top - gap).max(0.0);
        self.scroll = self.scroll.clamp(0.0, self.max_scroll(z, s, list_h));
        if list_h < 1.0 {
            return c;
        }
        let mut list = Canvas::new(w as usize, list_h as usize);
        list.clear(u.bg);
        let first = (self.scroll / rh).floor() as usize;
        let last = (((self.scroll + list_h) / rh).ceil() as usize).min(self.rows.len());
        for vi in first..last {
            self.paint_row(&mut list, &cx, vi);
        }
        c.put(&list, sh as usize, (sh + top) as usize);
        self.shown = Some(Shown {
            top: sh + top,
            list_h,
            first,
            last,
            h,
        });
        // Grenze: zwei kurze Striche unter dem Panel
        if grip > 0.0 {
            let lw = rh;
            let gy = sh + h + (grip * 0.5).round();
            let col = if matches!(self.hover, Some(Hit::Divider)) || self.drag.is_some() {
                u.text
            } else {
                u.text_dim
            };
            let lh = s.round().max(1.0);
            let d = (0.75 * gap).round().max(2.0);
            c.fill_rect(sh + (w - lw) * 0.5, gy - d * 0.5 - lh * 0.5, lw, lh, col);
            c.fill_rect(sh + (w - lw) * 0.5, gy + d * 0.5 - lh * 0.5, lw, lh, col);
        }
        c
    }

    /// Höhe der Grenze unter dem Panel (Pixel).
    fn grip(&self, z: &Sizes, s: f32) -> f32 {
        if self.divider && !self.collapsed {
            (z.panel_margin * s).round()
        } else {
            0.0
        }
    }

    /// Was jede Zeile zum Zeichnen braucht.
    fn row_cx<'a>(
        &self,
        t: &'a Theme,
        fonts: &'a Fonts,
        m: &'a Model,
        picking: &Picking,
        s: f32,
    ) -> RowCx<'a> {
        let z = &t.size;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let px = z.font_small * s;
        let gap = (z.tree_icon_gap * s).round();
        let pad = z.panel_pad * s;
        RowCx {
            m,
            t,
            regular,
            bold,
            selected: picking
                .selected
                .iter()
                .filter_map(|&id| m.element(id).map(|e| e.guid))
                .collect(),
            model_hover: self.glow_guid(m, picking),
            iso: m.visibility().isolate.is_some(),
            w: self.rect.w,
            s,
            rh: (z.tree_row_h * s).round(),
            gap,
            pad,
            px,
            cap: regular.map_or(px * 0.7, |f| f.cap_height(px)),
            inset: pad - 2.0 * gap,
        }
    }

    /// Bauteil, das durch den Modell-Hover leuchtet (nicht, solange die
    /// Maus über dem Panel ist).
    fn glow_guid(&self, m: &Model, picking: &Picking) -> Option<Guid> {
        picking
            .hover
            .filter(|_| self.hover.is_none())
            .and_then(|id| m.element(id).map(|e| e.guid))
    }

    /// Sichtbare Zeile (Index in `rows`), die durch den Modell-Hover
    /// leuchtet (Review 3i B1): Hover über Bauteile ohne sichtbare Zeile
    /// ändert das Bild nicht.
    fn glow_row(&self, m: &Model, picking: &Picking) -> Option<usize> {
        let g = self.glow_guid(m, picking)?;
        let sh = self.shown?;
        (sh.first..sh.last).find(|&vi| {
            self.rows.get(vi).is_some_and(|&i| {
                matches!(
                    self.nodes()[i].key,
                    NodeKey::Element(x) | NodeKey::TradeElement(_, x) if x == g
                )
            })
        })
    }

    /// Zeichnet die sichtbare Zeile `vi` in die Zeilenliste `list` (ihre
    /// Koordinaten; für Teilbilder mit Ursprung).
    fn paint_row(&self, list: &mut Canvas, cx: &RowCx, vi: usize) {
        let y = vi as f32 * cx.rh - self.scroll;
        let (m, t, u, z) = (cx.m, cx.t, &cx.t.ui, &cx.t.size);
        let (s, w, rh, gap, pad, px, cap, inset) =
            (cx.s, cx.w, cx.rh, cx.gap, cx.pad, cx.px, cx.cap, cx.inset);
        let (regular, bold) = (cx.regular, cx.bold);
        let Some(&i) = self.rows.get(vi) else {
            return;
        };
        let n = &self.nodes()[i];
        let hovered = self.hover.and_then(Hit::row) == Some(vi);
        let guid = match n.key {
            NodeKey::Element(g) | NodeKey::TradeElement(_, g) => Some(g),
            _ => None,
        };
        let sel = guid.is_some_and(|g| cx.selected.contains(&g));
        let glow = guid.is_some() && guid == cx.model_hover;
        if sel || hovered || glow {
            let mut p = Path::new();
            p.rounded_rect(inset, y, w - 2.0 * inset, rh, 4.0 * s);
            list.fill(&p, if sel { u.pressed } else { u.tree_hover });
        }
        if sel {
            let bw = (0.75 * gap).max(2.0 * s).round();
            let mut p = Path::new();
            p.rounded_rect(inset, y + gap, bw, rh - 2.0 * gap, bw * 0.5);
            list.fill(&p, u.accent);
        }
        let ids = self.tree.elements(self.tab, i);
        let dim = cx.iso && !ids.is_empty() && ids.iter().all(|&id| m.masks(id).solid == 0);
        let cy = y + rh * 0.5;
        if n.has_children(i) {
            let open = self.open[self.tab.index()].contains(&n.key);
            let col = if self.hover == Some(Hit::Arrow(vi)) {
                u.text
            } else {
                u.text_dim
            };
            widgets::disclosure(list, self.arrow_x(n, z, s), cy, open, col, s);
        }
        // Symbole von rechts
        let icons = self.row_icons(m, i, hovered);
        let xr = w - pad;
        let step = (z.tree_icon + z.tree_icon_gap) * s;
        let mut strip_left = xr;
        for (k, &ic) in slots(self.tab, &n.key).iter().enumerate() {
            if !shown(ic, &icons) {
                continue;
            }
            let cx = xr - z.tree_icon * s * 0.5 - k as f32 * step;
            strip_left = cx - z.tree_icon * s * 0.5;
            let under = self.hover == Some(Hit::Icon(vi, ic));
            let faded = !live(ic, &icons);
            let col = if faded {
                u.text_disabled
            } else if under {
                u.text
            } else {
                u.text_dim
            };
            match ic {
                Icon::Eye => {
                    let st = tree::eye(m, &self.tree, self.tab, i);
                    widgets::eye_icon(list, cx, cy, fill(st), col, s);
                }
                Icon::Lock => {
                    let st = lock_state(m, ids);
                    // Gesperrt: Schloss in Schrift, auch ohne Maus
                    let col = if st == State::Full && !faded {
                        u.text
                    } else {
                        col
                    };
                    // Nach dem Verweis der Hinweiskarte: Akzent, der
                    // in `flash_ms` vergeht
                    let col = match self.flash {
                        Some((at, start)) if at == i && self.tab == Tab::Tree => {
                            let k = Instant::now().duration_since(start).as_secs_f32() * 1000.0
                                / t.size.flash_ms.max(1.0);
                            mix(u.accent, col, k.clamp(0.0, 1.0))
                        }
                        _ => col,
                    };
                    widgets::lock_icon(list, cx, cy, fill(st), col, s);
                }
                Icon::Isolate => {
                    let on = self
                        .isolated
                        .as_ref()
                        .is_some_and(|(t, k, _)| *t == self.tab && *k == n.key);
                    let col = if on { u.accent } else { col };
                    widgets::isolate_icon(list, cx, cy, col, s);
                }
                Icon::Delete => widgets::trash_icon(list, cx, cy, col, s),
            }
        }
        // Name, rechts klein die Angabe, solange sie mit Abstand passt
        let top_row = matches!(
            (self.tab, &n.key),
            (Tab::Tree, NodeKey::Project | NodeKey::Building(_))
                | (Tab::Kind, NodeKey::Kind(_))
                | (Tab::Trade, NodeKey::Trade(_))
        );
        let f = if top_row { bold } else { regular };
        let tx = self.text_x(n, z, s);
        let label = row_label(m, n);
        let end = strip_left - 1.5 * gap;
        let right = right_text(m, self.tab, n);
        let spx = z.tree_small * s;
        let lw = f.map_or(0.0, |f| f.width(&label, px));
        let rw = regular.map_or(0.0, |f| f.width(&right, spx));
        let show_right = !right.is_empty() && !hovered && tx + lw + 1.5 * gap + rw <= end;
        let max_w = if show_right {
            end - rw - 1.5 * gap - tx
        } else {
            end - tx
        };
        let label = widgets::ellipsize(f, &label, px, max_w.max(0.0));
        let col = if dim { u.text_dim } else { u.text };
        let base = (cy + cap * 0.5).round();
        widgets::text(list, f, &label, px, tx, base, col);
        if show_right {
            widgets::text(list, regular, &right, spx, end - rw, base, u.text_dim);
        }
    }

    /// Bild für die App (Review 3i B2, 3j B4): ganz neu, wenn sich mehr als
    /// die Hover-Zeilen geändert hat; sonst nur die betroffenen Zeilenbänder.
    /// Gleitet die Höhe, wird einmal in der größeren Höhe gezeichnet und je
    /// Bild nur der sichtbare Teil mit dem Unterrand gezeigt. `None`:
    /// nichts zu tun.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        scene: &Scene,
        picking: &Picking,
        h: f32,
        s: f32,
    ) -> Option<TreeOut> {
        let tall = self.grow.map_or(h, |g| g.from.max(g.to).max(h).round());
        let full = self.dirty
            || self.img.as_ref().is_none_or(|(c, at)| {
                *at != tall
                    || c.width as f32 != self.rect.w + 2.0 * (t.size.panel_shadow * s).round()
            });
        if full {
            let c = self.paint(t, fonts, scene, picking, tall, s);
            self.img = Some((c, tall));
            self.cut_h = None;
        } else if !self.rows_dirty.is_empty() {
            let parts = self.repaint_rows(t, fonts, scene.model(), picking, s);
            if self.cut_h.is_none() || self.cut_h == Some(tall) {
                return Some(TreeOut::Parts(parts));
            }
        } else if self.cut_h == Some(h) || (self.cut_h.is_none() && tall == h) {
            return None;
        }
        let (img, at) = self.img.as_ref()?;
        if *at == h {
            self.cut_h = None;
            return Some(TreeOut::Full(img.clone()));
        }
        // Übergang: oberer Teil bis zum Ende der Liste, dann der Unterrand
        // samt runden Ecken und Schatten; deren Zeilen neben der Liste
        // stammen vom Unterrand, unter der Liste von oben
        let z = &t.size;
        let sh = (z.panel_shadow * s).round();
        let gap = (z.tree_icon_gap * s).round();
        let grip = self.grip(z, s);
        let foot = (gap + sh + grip) as usize;
        let keep = ((sh + h - gap).max(0.0) as usize).min(img.height);
        let tail = (foot + (2.0 * z.corner_radius * s).ceil() as usize).min(keep + foot);
        let from = keep + foot - tail;
        let mut c = Canvas::new(img.width, keep + foot);
        c.copy_rows(img, 0, from);
        c.copy_rows_at(img, img.height.saturating_sub(tail), from, tail);
        if let Some(v) = self.shown {
            let y0 = from.max(v.top as usize);
            c.copy_span(img, y0, keep, sh as usize, (sh + self.rect.w) as usize);
        }
        self.cut_h = Some(h);
        Some(TreeOut::Full(c))
    }

    /// Malt nur die Zeilenbänder in `rows_dirty` neu, ins letzte ganze Bild,
    /// und liefert sie als Ausschnitte (Lage im Bild, Bild).
    fn repaint_rows(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        m: &Model,
        picking: &Picking,
        s: f32,
    ) -> Vec<(usize, usize, Canvas)> {
        let mut vis: Vec<usize> = std::mem::take(&mut self.rows_dirty);
        vis.sort_unstable();
        vis.dedup();
        let Some(sh) = self.shown else {
            return Vec::new();
        };
        let cx = self.row_cx(t, fonts, m, picking, s);
        let pad = (t.size.panel_shadow * s).round() as usize;
        let mut out = Vec::new();
        let Some((img, _)) = self.img.take() else {
            return out;
        };
        let mut img = img;
        // Was eine Zeile über ihr Band hinaus malt (Kantenglättung der
        // Symbole), deckt ein Rand ab; darin malen die Nachbarn mit
        let edge = (2.0 * s).ceil();
        for vi in vis.into_iter().filter(|v| (sh.first..sh.last).contains(v)) {
            // Band der Zeile in Listenkoordinaten, auf die Liste begrenzt
            let y = vi as f32 * cx.rh - self.scroll;
            let y0 = (y - edge).floor().max(0.0);
            let y1 = (y + cx.rh + edge).ceil().min(sh.list_h.floor());
            if y1 <= y0 {
                continue;
            }
            let mut band = Canvas::new(cx.w as usize, (y1 - y0) as usize);
            band.set_origin(0.0, y0);
            band.clear(t.ui.bg);
            for n in vi.saturating_sub(1).max(sh.first)..(vi + 2).min(sh.last) {
                self.paint_row(&mut band, &cx, n);
            }
            band.set_origin(0.0, 0.0);
            let at = (sh.top + y0) as usize;
            img.put(&band, pad, at);
            out.push((pad, at, band));
        }
        self.img = Some((img, sh.h));
        out
    }

    // --- einstellungen.txt ------------------------------------------------

    /// Zeile `[baum]`: Karte, zugeklappt, gezogene Grenze (dip).
    pub fn settings_line(&self) -> String {
        let mut l = sk_model::szo::Line::new("baum")
            .text("karte", tab_word(self.tab))
            .flag("zu", self.collapsed);
        if let Some(sp) = self.split {
            l = l.num("grenze", sp.round() as f64);
        }
        let mut out = String::new();
        l.finish(&mut out);
        out
    }

    /// Liest die Zeile `[baum]` aus dem Text von `einstellungen.txt`.
    pub fn load_settings(&mut self, text: &str) {
        for (i, line) in text.lines().enumerate() {
            let Ok(Some(r)) = sk_model::szo::Record::parse(i + 1, line) else {
                continue;
            };
            if r.section != "baum" {
                continue;
            }
            if let Some(t) = r
                .opt("karte")
                .and_then(|w| Tab::ALL.into_iter().find(|t| tab_word(*t) == w))
            {
                self.tab = t;
            }
            self.collapsed = r.opt("zu") == Some("1");
            self.split = r
                .opt("grenze")
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| v.is_finite() && *v > 0.0);
        }
        self.refresh_rows();
    }
}

/// Schloss der Zeile `i` der Karte `tab` blass: Es wirkt nicht, weil das
/// Bauteil bzw. alles unter der Art- oder Gruppenzeile abgeleitet ist und
/// seiner Quelle folgt (y).
pub fn lock_faded(m: &Model, tree: &ProjectTree, tab: Tab, i: usize) -> bool {
    let Some(n) = tree.tab(tab).get(i) else {
        return false;
    };
    let ids = tree.elements(tab, i);
    match n.key {
        NodeKey::Element(_) => ids.first().is_some_and(|&id| m.lock_source(id) != id),
        NodeKey::Kind(_) | NodeKey::Group(..) | NodeKey::KindType(..) => {
            !ids.is_empty() && ids.iter().all(|&id| m.lock_source(id) != id)
        }
        _ => false,
    }
}

/// Passt die Zeile zur Angabe der Befehlszeile? `art=exterior`,
/// `gewerk=18345`, `geschoss=EG`, `nr=AW-005` (nur ASCII, die Befehlszeile
/// unter Windows), sonst der Namensanfang.
fn named(m: &Model, n: &Node, text: &str) -> bool {
    if let Some(w) = text.strip_prefix("art=") {
        return match n.key {
            NodeKey::Kind(c) | NodeKey::Group(_, c) => sk_model::kinds::spec(c).szo == w,
            _ => false,
        };
    }
    if let Some(code) = text.strip_prefix("gewerk=") {
        return match n.key {
            NodeKey::Trade(t) => m.trades().iter().any(|x| x.id() == t && x.code == code),
            _ => false,
        };
    }
    if let Some(short) = text.strip_prefix("geschoss=") {
        return match n.key {
            NodeKey::Storey(g) => m
                .storeys()
                .iter()
                .any(|(_, s)| s.guid == g && s.short == short),
            _ => false,
        };
    }
    if let Some(nr) = text.strip_prefix("nr=") {
        return matches!(n.key, NodeKey::Element(_) | NodeKey::TradeElement(..)) && n.label == nr;
    }
    row_label(m, n).starts_with(text)
}

/// Sperrzustand der Bauteile (über ihre Quelle).
fn lock_state(m: &Model, ids: &[ElementId]) -> State {
    if ids.is_empty() {
        return State::None;
    }
    let n = ids.iter().filter(|&&id| m.is_locked(id)).count();
    if n == ids.len() {
        State::Full
    } else if n == 0 {
        State::None
    } else {
        State::Partial
    }
}

fn fill(s: State) -> Fill {
    match s {
        State::Full => Fill::Full,
        State::Partial => Fill::Partial,
        State::None => Fill::None,
    }
}

fn tab_label(t: Tab) -> &'static str {
    match t {
        Tab::Tree => "Baum",
        Tab::Kind => "Bauteil",
        Tab::Trade => "Gewerk",
    }
}

fn tab_word(t: Tab) -> &'static str {
    match t {
        Tab::Tree => "baum",
        Tab::Kind => "bauteil",
        Tab::Trade => "gewerk",
    }
}

/// Name einer Zeile: das Fundament heißt wie im Geschosspaneel, in der
/// Karte „Gewerk“ steht die Schicht hinter der Nummer.
fn row_label(m: &Model, n: &Node) -> String {
    match &n.key {
        NodeKey::Storey(g) => {
            let st = m.storeys().iter().find(|(_, s)| s.guid == *g).map(|x| x.1);
            match st {
                Some(st) if st.kind == LevelKind::Foundation => {
                    crate::scene::FOUNDATION_NAME.into()
                }
                _ => n.label.clone(),
            }
        }
        NodeKey::TradeElement(..) => match &n.layer_hint {
            Some(h) => format!("{} · {h}", n.label),
            None => n.label.clone(),
        },
        _ => n.label.clone(),
    }
}

/// Kleinangabe rechts: Geschoss bzw. Typkürzel an Einzelbauteilen
/// (Karte „Bauteil“ und aufgeklappte Gruppen); Spannen nur im Hinweis.
fn right_text(m: &Model, tab: Tab, n: &Node) -> String {
    match (tab, &n.key) {
        (Tab::Kind, NodeKey::Element(_)) => n.right.clone(),
        (Tab::Tree, NodeKey::Element(g)) if n.depth >= 4 => {
            let e = m.elements().iter().find(|(_, e)| e.guid == *g).map(|x| x.1);
            e.and_then(|e| e.layer_set)
                .and_then(|t| m.layer_set(t))
                .map_or(String::new(), |t| short_type(&t.name))
        }
        _ => String::new(),
    }
}

/// „AW mit WDVS 36“ → „AW 36“: Kürzel und Dicke des Typs.
fn short_type(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    match (words.first(), words.last()) {
        (Some(a), Some(b))
            if words.len() > 1 && b.chars().next().is_some_and(|c| c.is_ascii_digit()) =>
        {
            format!("{a} {b}")
        }
        _ => String::new(),
    }
}

/// Hinweis beim Überfahren einer Zeile: Nummer, Spanne, Höhe, Normnummer.
fn row_tip(m: &Model, n: &Node, ids: &[ElementId]) -> Option<String> {
    match &n.key {
        NodeKey::Storey(g) => {
            let st = m
                .storeys()
                .iter()
                .find(|(_, s)| s.guid == *g)
                .map(|x| x.1)?;
            Some(format!(
                "{}\nOK RD {} m",
                row_label(m, n),
                crate::ui::kote_text(st.top())
            ))
        }
        NodeKey::Trade(tr) => {
            let t = m.trades().iter().find(|t| t.id() == *tr)?;
            Some(format!("DIN {}\n{}", t.code, t.name))
        }
        NodeKey::Element(_) => {
            let e = m.element(*ids.first()?)?;
            (e.number != n.label).then(|| e.number.clone())
        }
        NodeKey::Group(..) | NodeKey::Kind(_) | NodeKey::KindType(..) => {
            (!n.right.is_empty()).then(|| n.right.clone())
        }
        _ => None,
    }
}

/// Warum Löschen an der Zeile blass ist.
fn delete_reason(m: &Model, n: &Node, ids: &[ElementId]) -> String {
    match n.key {
        NodeKey::Storey(_) => "Geschosse entstehen mit dem Gebäude.".into(),
        NodeKey::Terrain | NodeKey::Project => String::new(),
        _ => {
            let Some(&id) = ids.first() else {
                return String::new();
            };
            match m.can_delete(id) {
                Err(r) => sk_model::refusal_lines(m, id, &r).join("\n"),
                Ok(()) => String::new(),
            }
        }
    }
}

/// Name des isolierten Asts fürs Band („Erdgeschoss“, „Außenwände“).
fn band_name(m: &Model, n: &Node) -> String {
    let l = row_label(m, n);
    // „Außenwände (4)“ → „Außenwände“
    match l.rfind(" (") {
        Some(k) if l.ends_with(')') => l[..k].to_string(),
        _ => l,
    }
}

/// Name für ein Isolieren, das nicht aus dem Baum kam (Befehlszeile).
fn isolate_name(m: &Model) -> String {
    match &m.visibility().isolate {
        Some(Isolate::Category(c)) => sk_model::kinds::spec(*c).plural.into(),
        Some(Isolate::Trade(t)) => m
            .trades()
            .iter()
            .find(|x| x.id() == *t)
            .map_or(String::new(), |x| x.display_name().into()),
        Some(Isolate::Elements(g)) if g.len() == 1 => m
            .elements()
            .iter()
            .find(|(_, e)| g.contains(&e.guid))
            .map_or(String::new(), |(_, e)| e.number.clone()),
        Some(Isolate::Elements(g)) => format!("{} Bauteile", g.len()),
        None => String::new(),
    }
}

/// „Alles zeigen“: nichts mehr ausgeblendet, Isolieren bleibt.
fn show_all(m: &Model) -> Visibility {
    Visibility {
        isolate: m.visibility().isolate.clone(),
        ..Visibility::default()
    }
}

/// Text der Hinweiszeile in Karte `tab`, wenn in einer anderen Karte etwas
/// ausgeblendet ist (§1.5): „1 Gewerk ausgeblendet“.
pub fn hidden_elsewhere(m: &Model, tab: Tab) -> Option<String> {
    let v = m.visibility();
    let count = |n: usize, one: &str, many: &str| -> Option<String> {
        (n > 0).then(|| format!("{n} {} ausgeblendet", if n == 1 { one } else { many }))
    };
    let parts: Vec<String> = [
        (tab != Tab::Tree)
            .then(|| count(v.hidden.len(), "Bauteil", "Bauteile"))
            .flatten(),
        (tab != Tab::Kind)
            .then(|| count(v.hidden_cat.len(), "Art", "Arten"))
            .flatten(),
        (tab != Tab::Trade)
            .then(|| count(v.hidden_trade.len(), "Gewerk", "Gewerke"))
            .flatten(),
    ]
    .into_iter()
    .flatten()
    .collect();
    match parts.len() {
        0 => None,
        1 => parts.into_iter().next(),
        _ => Some("Weiteres ausgeblendet".into()),
    }
}

/// Farbe zwischen `a` (k = 0) und `b` (k = 1).
fn mix(a: sk_paint::Rgba, b: sk_paint::Rgba, k: f32) -> sk_paint::Rgba {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    sk_paint::Rgba(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2), m(a.3, b.3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{RefSide, WallChain};

    fn haus() -> Scene {
        let mut s = Scene::with_model(Model::with_seed(7));
        let w = WallChain {
            base: 0.0,
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            layers: Vec::new(),
            height: 3500.0,
            joints: Default::default(),
        };
        s.add_wall(&w).unwrap();
        s
    }

    /// Größter Unterschied je Kanal (8-Bit-Stufen); Teilbilder mit
    /// Ursprung runden Kanten auf halben Bildpunkten um höchstens eine Stufe
    /// anders.
    fn steps(a: &Canvas, b: &Canvas) -> u8 {
        assert_eq!((a.width, a.height), (b.width, b.height));
        bytes(a)
            .iter()
            .zip(bytes(b))
            .map(|(p, q)| p.abs_diff(q))
            .max()
            .unwrap_or(0)
    }

    fn bytes(c: &Canvas) -> Vec<u8> {
        let mut v = Vec::new();
        c.premul_rgba8_into(&mut v);
        v
    }

    fn panel(s: &Scene, t: &Theme, h: f32) -> TreePanel {
        let mut p = TreePanel::new();
        p.place(Rect::new(10.0, 10.0, 280.0, h), t, Instant::now(), false);
        p.sync(s, &Picking::default(), t, 1.0);
        p
    }

    /// Review 3i B2: Wechselt nur die Zeile unter der Maus, malt das Panel
    /// nur die beiden Zeilenbänder neu; das Bild ist (bis auf eine Stufe)
    /// dasselbe wie ganz neu gemalt.
    #[test]
    fn hover_malt_nur_zeilen() {
        let (s, t, f) = (haus(), Theme::dark(), Fonts::system());
        let pk = Picking::default();
        let mut p = panel(&s, &t, 400.0);
        assert!(matches!(
            p.render(&t, &f, &s, &pk, 400.0, 1.0),
            Some(TreeOut::Full(_))
        ));
        assert!(
            p.render(&t, &f, &s, &pk, 400.0, 1.0).is_none(),
            "nichts neu"
        );
        for hit in [
            Some(Hit::Row(2)),
            Some(Hit::Icon(3, Icon::Eye)),
            Some(Hit::Panel),
            None,
        ] {
            p.set_hover(hit);
            assert!(!p.dirty);
            match p.render(&t, &f, &s, &pk, 400.0, 1.0) {
                Some(TreeOut::Parts(parts)) => assert!(parts.len() <= 2),
                None => {}
                _ => panic!("nur Zeilen erwartet: {hit:?}"),
            }
            let mut q = panel(&s, &t, 400.0);
            q.hover = hit;
            let ganz = q.paint(&t, &f, &s, &pk, 400.0, 1.0);
            assert!(steps(&p.img.as_ref().unwrap().0, &ganz) <= 1, "{hit:?}");
        }
    }

    /// Review 3j B4: Gleitet die Höhe, wird nicht je Bild neu gemalt; das
    /// gekürzte Bild gleicht einem ganz gemalten in dieser Höhe.
    #[test]
    fn gleiten_ohne_neues_malen() {
        let (s, t, f) = (haus(), Theme::dark(), Fonts::system());
        let pk = Picking::default();
        let mut p = panel(&s, &t, 400.0);
        p.render(&t, &f, &s, &pk, 400.0, 1.0);
        let now = Instant::now();
        p.place(Rect::new(10.0, 10.0, 280.0, 250.0), &t, now, true);
        p.divider = false;
        let Some(TreeOut::Full(_)) = p.render(&t, &f, &s, &pk, 400.0, 1.0) else {
            panic!("Bild in der größeren Höhe");
        };
        let tall = p.img.as_ref().unwrap().0.height;
        for h in [360.0, 300.0, 251.0] {
            let Some(TreeOut::Full(c)) = p.render(&t, &f, &s, &pk, h, 1.0) else {
                panic!("gekürztes Bild");
            };
            assert_eq!(p.img.as_ref().unwrap().0.height, tall, "nicht neu gemalt");
            let mut q = panel(&s, &t, h);
            let ganz = q.paint(&t, &f, &s, &pk, h, 1.0);
            assert_eq!((c.width, c.height), (ganz.width, ganz.height));
            assert!(bytes(&c) == bytes(&ganz), "{h}");
        }
    }
}

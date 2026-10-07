//! Löschen in der Oberfläche (Paket „Löschen“, Gestaltung Fassung 1): Sätze
//! des Hinweises am Bauteil, das Kontextmenü am Bauteil und die Rückfrage
//! „Gebäude löschen“.
//!
//! Alles hier ist Zustand und Zeichnen ohne Fenster; `main.rs` führt aus.

use crate::menu::{self, MenuItem};
use sk_model::{refusal_lines, BuildingId, Category, Deleted, ElementId, Model, Refusal};
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::Key;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, Fonts, Rect};
use std::time::{Duration, Instant};

/// So lange steht der Hinweis am Bauteil (ohne Ein- und Ausblenden).
pub const HINT_TIME: Duration = Duration::from_secs(5);
/// Nach dem Verlassen mit der Maus bleibt er mindestens noch so lange.
const HINT_LINGER: Duration = Duration::from_millis(1200);

/// Verweis im Hinweis am Bauteil.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Link {
    DeleteBuilding(BuildingId),
    /// Typ der Wand ändern (Randdämmstreifen: seine Wand).
    ChangeType(ElementId),
    Undo,
}

/// Wörter für die nicht gelöschten Bauteile einer gemischten Auswahl:
/// Einzahl mit Artikel, Mehrzahl, Pronomen.
fn kind_words(c: Category) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match c {
        Category::ExteriorWall => ("Die Außenwand", "Außenwände", "sie"),
        Category::Floor => ("Die Decke", "Decken", "sie"),
        Category::GroundSlab => ("Die Sohlplatte", "Sohlplatten", "sie"),
        Category::StripFooting => ("Die Frostschürze", "Frostschürzen", "sie"),
        Category::EdgeInsulation => ("Der Randdämmstreifen", "Randdämmstreifen", "er"),
        _ => return None,
    })
}

/// Zeile 2 der gemischten Auswahl: was bleibt und warum, ein Satz.
fn rest_sentence(m: &Model, refused: &[(ElementId, Refusal)]) -> String {
    let mut kinds: Vec<(Category, usize)> = Vec::new();
    for (id, _) in refused {
        let Some(c) = m.element(*id).map(|e| e.category) else {
            continue;
        };
        match kinds.iter_mut().find(|k| k.0 == c) {
            Some(k) => k.1 += 1,
            None => kinds.push((c, 1)),
        }
    }
    kinds.retain(|k| kind_words(k.0).is_some());
    kinds.sort_by_key(|k| k.0);
    let strip = kinds.iter().any(|k| k.0 == Category::EdgeInsulation);
    let where_ = match (strip, kinds.len()) {
        (true, 1) => "zum Wandtyp",
        (true, _) => "zum Gebäudeumriss oder zum Wandtyp",
        _ => "zum Gebäudeumriss",
    };
    match kinds.as_slice() {
        [] => String::new(),
        [(c, 1)] => {
            let (one, _, pron) = kind_words(*c).unwrap_or_default();
            format!("{one} bleibt, {pron} gehört {where_}.")
        }
        [(c, _)] => {
            let (_, many, _) = kind_words(*c).unwrap_or_default();
            format!("Die {many} bleiben, sie gehören {where_}.")
        }
        _ => {
            let names: Vec<&str> = kinds
                .iter()
                .filter_map(|k| kind_words(k.0).map(|w| w.1))
                .collect();
            let (last, head) = names.split_last().unwrap_or((&"", &[]));
            format!(
                "{} und {last} bleiben, sie gehören {where_}.",
                head.join(", ")
            )
        }
    }
}

/// Hinweis am Bauteil nach dem Löschen, Zeile für Zeile; leer, wenn alles
/// gelöscht wurde (dann blendet es nur aus). Ist nichts gelöscht, steht der
/// Satz zum ersten abgelehnten Bauteil; sonst Zeile 1, was gelöscht wurde,
/// und Zeile 2, was bleibt.
pub fn hint(m: &Model, d: &Deleted) -> Vec<String> {
    let Some(&(id, r)) = d.refused.first() else {
        return Vec::new();
    };
    if d.removed.is_empty() {
        return refusal_lines(m, id, &r)
            .into_iter()
            .map(String::from)
            .collect();
    }
    // Allgemein „Wand“, wie in soll-loeschen-4 abgenommen
    let first = match d.removed.len() {
        1 => "1 Wand gelöscht.".to_string(),
        n => format!("{n} Wände gelöscht."),
    };
    vec![first, rest_sentence(m, &d.refused)]
}

/// Verweis im Hinweis: „Rückgängig“ nach einem Teil-Löschen, sonst der
/// Ausweg zum ersten abgelehnten Bauteil.
pub fn hint_link(m: &Model, d: &Deleted) -> Option<(&'static str, Link)> {
    let &(id, r) = d.refused.first()?;
    if !d.removed.is_empty() {
        return Some(("Rückgängig", Link::Undo));
    }
    match r {
        Refusal::BuildingOutline(Some(b)) => Some(("Gebäude löschen …", Link::DeleteBuilding(b))),
        Refusal::Derived { from }
            if m.element(id).map(|e| e.category) == Some(Category::EdgeInsulation) =>
        {
            Some(("Wandtyp ändern …", Link::ChangeType(from)))
        }
        _ => None,
    }
}

/// Bauteile, an denen der Hinweis steht: die abgelehnten (sie leuchten).
pub fn hint_anchor(d: &Deleted) -> Vec<ElementId> {
    d.refused.iter().map(|r| r.0).collect()
}

// --- Hinweis am Bauteil ---------------------------------------------------

/// Dunkle Karte unter dem Bauteil: Akzentpunkt, Zeile 1 fett, Zeile 2
/// gedimmt, darunter ein Verweis. Blendet ein, steht [`HINT_TIME`], unter
/// der Maus länger, und blendet aus. Ein neuer Hinweis ersetzt den alten.
#[derive(Clone, Debug)]
pub struct HintCard {
    pub lines: Vec<String>,
    pub link: Option<(&'static str, Link)>,
    pub anchor: Vec<ElementId>,
    start: Instant,
    deadline: Instant,
    /// Lage im Fenster (Pixel, ohne Schatten), sobald gezeichnet.
    pub rect: Option<Rect>,
    pub link_hover: bool,
    hover: bool,
}

/// Maße der Karte (dip): Rand links bis zum Punkt, Punkt, Text ab, Rand
/// rechts, oben, Zeilenabstand, Abstand zum Verweis, Rand unten.
const CARD_DOT_X: f32 = 16.0;
const CARD_DOT: f32 = 8.0;
const CARD_TEXT_X: f32 = 34.0;
const CARD_RIGHT: f32 = 28.0;
const CARD_TOP: f32 = 22.0;
const CARD_LINE: f32 = 20.0;
const CARD_LINK: f32 = 24.0;
const CARD_BOTTOM: f32 = 18.0;

impl HintCard {
    pub fn new(
        lines: Vec<String>,
        link: Option<(&'static str, Link)>,
        anchor: Vec<ElementId>,
        now: Instant,
    ) -> HintCard {
        HintCard {
            lines,
            link,
            anchor,
            start: now,
            deadline: now + HINT_TIME,
            rect: None,
            link_hover: false,
            hover: false,
        }
    }

    /// Größe (Pixel, ohne Schatten).
    pub fn size(&self, t: &Theme, fonts: &Fonts, s: f32) -> (f32, f32) {
        let (regular, bold) = (fonts.regular.as_ref(), fonts.bold.as_ref());
        let bold = bold.or(regular);
        let (px, small) = (t.size.font * s, t.size.font_small * s);
        let mut tw: f32 = 0.0;
        for (i, l) in self.lines.iter().enumerate() {
            let (f, p) = if i == 0 { (bold, px) } else { (regular, small) };
            tw = tw.max(f.map_or(l.len() as f32 * p * 0.5, |f| f.width(l, p)));
        }
        if let Some((l, _)) = self.link {
            tw = tw.max(bold.map_or(0.0, |f| f.width(l, px)));
        }
        let n = self.lines.len().max(1) as f32;
        let mut h = CARD_TOP + (n - 1.0) * CARD_LINE + CARD_BOTTOM;
        if self.link.is_some() {
            h += CARD_LINK;
        }
        (
            (tw + (CARD_TEXT_X + CARD_RIGHT) * s).ceil(),
            (h * s + regular.map_or(px * 0.7, |f| f.cap_height(px))).ceil(),
        )
    }

    /// Lage unter den Bauteilen (Rechteck im Fenster, Pixel): mittig darunter,
    /// passt es nicht, darüber; nie über den Rand.
    pub fn place(&mut self, size: (f32, f32), bounds: Option<Rect>, win: (f32, f32, f32), s: f32) {
        let (w, h) = size;
        let (ww, wh, top) = win;
        let gap = 14.0 * s;
        let (x, y) = match bounds {
            Some(b) => {
                let x = b.x + (b.w - w) * 0.5;
                let below = b.y + b.h + gap;
                let y = if below + h <= wh - 8.0 * s {
                    below
                } else if b.y - gap - h >= top + 8.0 * s {
                    b.y - gap - h
                } else {
                    wh - h - 24.0 * s
                };
                (x, y)
            }
            None => ((ww - w) * 0.5, wh - h - 24.0 * s),
        };
        let x = x.clamp(8.0 * s, (ww - w - 8.0 * s).max(8.0 * s));
        let y = y.clamp(top + 8.0 * s, (wh - h - 8.0 * s).max(top));
        self.rect = Some(Rect::new(x.round(), y.round(), w, h));
    }

    /// Deckkraft jetzt (Ein- und Ausblenden in `fade_ms`); `None`: vorbei.
    pub fn alpha(&self, now: Instant, fade_ms: f32) -> Option<f32> {
        if now >= self.deadline && !self.hover {
            return None;
        }
        if fade_ms <= 0.0 {
            return Some(1.0);
        }
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        let a_in = (ms(now.saturating_duration_since(self.start)) / fade_ms).min(1.0);
        let a_out = if self.hover {
            1.0
        } else {
            (ms(self.deadline.saturating_duration_since(now)) / fade_ms).min(1.0)
        };
        Some(a_in.min(a_out).max(0.0))
    }

    /// Wann sich die Deckkraft wieder ändert (für die Ereignisschleife).
    pub fn wait(&self, now: Instant, fade_ms: f32) -> Duration {
        let fade = Duration::from_secs_f32(fade_ms.max(0.0) / 1000.0);
        if now < self.start + fade {
            return Duration::from_millis(16);
        }
        if self.hover {
            return Duration::from_millis(250);
        }
        let out = self.deadline.checked_sub(fade).unwrap_or(self.deadline);
        if now >= out {
            Duration::from_millis(16)
        } else {
            out - now
        }
    }

    /// Maus bewegt (Fensterkoordinaten): über der Karte bleibt sie stehen,
    /// der Verweis wird hell. `true`, wenn neu zu zeichnen ist.
    pub fn mouse_move(&mut self, x: f64, y: f64, s: f32, t: &Theme, now: Instant) -> bool {
        let Some(r) = self.rect else {
            return false;
        };
        let over = r.contains(x, y);
        if self.hover && !over {
            self.deadline = self.deadline.max(now + HINT_LINGER);
        }
        self.hover = over;
        let link = over && self.link_rect(r, s, t).is_some_and(|l| l.contains(x, y));
        let changed = link != self.link_hover;
        self.link_hover = link;
        changed
    }

    /// Bereich des Verweises im Fenster.
    fn link_rect(&self, r: Rect, s: f32, t: &Theme) -> Option<Rect> {
        self.link?;
        let n = self.lines.len().max(1) as f32;
        // Grundlinie des Verweises wie in `paint`; Versalhöhe genähert
        let cap = t.size.font * s * 0.7;
        let base = r.y + cap + (CARD_TOP + (n - 1.0) * CARD_LINE + CARD_LINK) * s;
        let (x, pad) = (r.x + (CARD_TEXT_X - 6.0) * s, 6.0 * s);
        Some(Rect::new(
            x,
            base - cap - pad,
            r.w - CARD_TEXT_X * s,
            cap + 2.0 * pad,
        ))
    }

    /// Klick (Fensterkoordinaten): `Some(Some(link))` auf den Verweis,
    /// `Some(None)` sonst auf die Karte, `None` daneben.
    pub fn click(&self, x: f64, y: f64, s: f32, t: &Theme) -> Option<Option<Link>> {
        let r = self.rect?;
        if !r.contains(x, y) {
            return None;
        }
        let hit = self.link_rect(r, s, t).is_some_and(|l| l.contains(x, y));
        Some(if hit { self.link.map(|l| l.1) } else { None })
    }

    /// Bild samt Schatten; links oben = `rect` minus Schatten.
    pub fn paint(&self, t: &Theme, fonts: &Fonts, s: f32) -> Canvas {
        let (w, h) = self.size(t, fonts, s);
        let m = (t.size.panel_shadow * s).round();
        let mut c = Canvas::new((w + 2.0 * m) as usize, (h + 2.0 * m) as usize);
        widgets::panel(&mut c, Rect::new(m, m, w, h), s, t);
        let u = &t.ui;
        let (regular, bold) = (fonts.regular.as_ref(), fonts.bold.as_ref());
        let bold = bold.or(regular);
        let (px, small) = (t.size.font * s, t.size.font_small * s);
        let cap = regular.map_or(px * 0.7, |f| f.cap_height(px));
        let mut base = m + CARD_TOP * s + cap * 0.5;
        let mut p = Path::new();
        let d = CARD_DOT * s;
        p.rounded_rect(
            m + CARD_DOT_X * s,
            base - cap * 0.5 - d * 0.5,
            d,
            d,
            d * 0.5,
        );
        c.fill(&p, u.accent);
        base = base.round() + (cap * 0.5).round();
        let x = m + CARD_TEXT_X * s;
        for (i, l) in self.lines.iter().enumerate() {
            if i == 0 {
                widgets::text(&mut c, bold, l, px, x, base, u.text);
            } else {
                base += CARD_LINE * s;
                widgets::text(&mut c, regular, l, small, x, base, u.text_dim);
            }
        }
        if let Some((l, _)) = self.link {
            base += CARD_LINK * s;
            let col = if self.link_hover {
                u.accent_hover
            } else {
                u.accent
            };
            widgets::text(&mut c, bold, l, px, x, base, col);
        }
        c
    }
}

// --- Kontextmenü am Bauteil -----------------------------------------------

/// Befehl einer Zeile des Kontextmenüs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    ChangeType,
    Properties,
    Delete,
    DeleteBuilding,
}

/// Breite des Kontextmenüs (dip).
const CONTEXT_W: f32 = 240.0;

/// Rechtsklick auf ein Bauteil: „Wandtyp ändern …“, „Eigenschaften“,
/// „Löschen   Entf“ (gedimmt, wenn nichts Löschbares gewählt ist) und
/// abgesetzt in `ui.danger` „Gebäude löschen …“.
#[derive(Clone, Debug)]
pub struct ContextMenu {
    pub target: ElementId,
    items: Vec<(MenuItem, Option<Action>)>,
    /// Links oben im Fenster (Pixel, ohne Schatten).
    x: f32,
    y: f32,
    /// Markierte Zeile (nur wählbare) und Zeile unter der Maus (auch gedimmt).
    sel: Option<usize>,
    hover: Option<usize>,
    pressed: bool,
    /// Satz, warum „Löschen“ gedimmt ist (Tooltip).
    pub refusal: Option<String>,
}

impl ContextMenu {
    /// Menü für `target` mit der Auswahl `selection` an der Maus `(x, y)`,
    /// so verschoben, dass es ins Fenster passt.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        m: &Model,
        target: ElementId,
        selection: &[ElementId],
        x: f64,
        y: f64,
        win: (u32, u32, u32),
        t: &Theme,
        s: f32,
    ) -> ContextMenu {
        let cat = m.element(target).map(|e| e.category);
        let typed = matches!(
            cat,
            Some(Category::ExteriorWall | Category::InteriorWall | Category::EdgeInsulation)
        );
        let row = |label: &str, shortcut: &str, enabled: bool, danger: bool| MenuItem {
            label: label.into(),
            shortcut: shortcut.into(),
            enabled,
            danger,
            ..MenuItem::default()
        };
        let mut items = Vec::new();
        if typed {
            items.push((
                row("Wandtyp ändern …", "", true, false),
                Some(Action::ChangeType),
            ));
        }
        items.push((
            row("Eigenschaften", "", true, false),
            Some(Action::Properties),
        ));
        items.push((menu::separator(), None));
        let deletable = selection.iter().any(|id| m.can_delete(*id).is_ok());
        items.push((
            row("Löschen", "Entf", deletable, false),
            Some(Action::Delete),
        ));
        let refusal = if deletable {
            None
        } else {
            m.can_delete(target)
                .err()
                .map(|r| sk_model::refusal_text(m, target, &r))
        };
        if m.building_of_element(target).is_some() {
            items.push((menu::separator(), None));
            items.push((
                row("Gebäude löschen …", "", true, true),
                Some(Action::DeleteBuilding),
            ));
        }
        let mut c = ContextMenu {
            target,
            items,
            x: 0.0,
            y: 0.0,
            sel: None,
            hover: None,
            pressed: false,
            refusal,
        };
        let (w, h) = c.size(t, s);
        let (ww, wh, top) = (win.0 as f32, win.1 as f32, win.2 as f32);
        let mut mx = x as f32;
        let mut my = y as f32;
        if mx + w > ww {
            mx = (mx - w).max(0.0);
        }
        if my + h > wh {
            my = (my - h).max(top);
        }
        c.x = mx.round();
        c.y = my.round();
        c
    }

    /// Zeilen: Text, wählbar, Befehl (Trenner ohne Text und Befehl).
    #[cfg(test)]
    pub fn actions(&self) -> Vec<(String, bool, Option<Action>)> {
        self.items
            .iter()
            .map(|(it, a)| (it.label.clone(), it.enabled, *a))
            .collect()
    }

    fn row_h(&self, it: &MenuItem, t: &Theme, s: f32) -> f32 {
        if it.separator {
            menu::SEP_H * s
        } else {
            t.size.menu_row * s
        }
    }

    fn size(&self, t: &Theme, s: f32) -> (f32, f32) {
        let h: f32 = self.items.iter().map(|(it, _)| self.row_h(it, t, s)).sum();
        ((CONTEXT_W * s).round(), (h + 2.0 * menu::PAD * s).round())
    }

    pub fn rect(&self, t: &Theme, s: f32) -> Rect {
        let (w, h) = self.size(t, s);
        Rect::new(self.x, self.y, w, h)
    }

    /// Zeile unter `(x, y)`; `None` daneben, `Some(None)` im Menü ohne Zeile.
    fn hit(&self, t: &Theme, s: f32, x: f64, y: f64) -> Option<Option<usize>> {
        let r = self.rect(t, s);
        if !r.contains(x, y) {
            return None;
        }
        let mut ry = r.y + menu::PAD * s;
        for (i, (it, _)) in self.items.iter().enumerate() {
            let h = self.row_h(it, t, s);
            if y >= ry as f64 && y < (ry + h) as f64 {
                return Some((!it.separator).then_some(i));
            }
            ry += h;
        }
        Some(None)
    }

    /// Maus bewegt; `true`, wenn neu zu zeichnen ist.
    pub fn mouse_move(&mut self, t: &Theme, s: f32, x: f64, y: f64) -> bool {
        let before = (self.sel, self.hover);
        self.hover = self.hit(t, s, x, y).flatten();
        self.sel = self.hover.filter(|i| self.items[*i].0.enabled);
        before != (self.sel, self.hover)
    }

    /// Tooltip über dem gedimmten „Löschen“: warum.
    pub fn tip(&self) -> Option<String> {
        let i = self.hover?;
        let (it, a) = &self.items[i];
        (*a == Some(Action::Delete) && !it.enabled)
            .then(|| self.refusal.clone())
            .flatten()
    }

    /// Maustaste gedrückt; `false`: daneben (das Menü schließt).
    pub fn press(&mut self, t: &Theme, s: f32, x: f64, y: f64) -> bool {
        if self.hit(t, s, x, y).is_none() {
            return false;
        }
        self.pressed = true;
        true
    }

    /// Losgelassen: Befehl der Zeile (gedimmte Zeilen nichts).
    pub fn release(&mut self, t: &Theme, s: f32, x: f64, y: f64) -> Option<Action> {
        if !std::mem::take(&mut self.pressed) {
            return None;
        }
        let i = self.hit(t, s, x, y).flatten()?;
        let (it, a) = &self.items[i];
        a.filter(|_| it.enabled)
    }

    /// Pfeile wandern, Enter führt aus. `Err(())`: Esc, das Menü schließt.
    pub fn key(&mut self, k: Key) -> Result<Option<Action>, ()> {
        let ok = |i: usize| !self.items[i].0.separator && self.items[i].0.enabled;
        let n = self.items.len();
        match k {
            Key::Escape => Err(()),
            Key::Enter => Ok(self.sel.and_then(|i| self.items[i].1)),
            Key::Other(0x28) | Key::Other(0x26) => {
                let down = k == Key::Other(0x28);
                let mut i = self.sel.unwrap_or(if down { n - 1 } else { 0 });
                for _ in 0..n {
                    i = if down { (i + 1) % n } else { (i + n - 1) % n };
                    if ok(i) {
                        self.sel = Some(i);
                        self.hover = Some(i);
                        break;
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Bild samt Schatten und seine Lage im Fenster.
    pub fn paint(&self, t: &Theme, fonts: &Fonts, s: f32) -> (Canvas, i32, i32) {
        let r = self.rect(t, s);
        let m = (t.size.panel_shadow * s).round();
        let mut c = Canvas::new((r.w + 2.0 * m) as usize, (r.h + 2.0 * m) as usize);
        widgets::panel_filled(&mut c, Rect::new(m, m, r.w, r.h), s, t, t.ui.menu_bg);
        let mut y = m + menu::PAD * s;
        for (i, (it, _)) in self.items.iter().enumerate() {
            let h = self.row_h(it, t, s);
            let row = Rect::new(m, y, r.w, h);
            menu::paint_row(&mut c, t, fonts, s, row, it, self.sel == Some(i));
            y += h;
        }
        (c, (r.x - m) as i32, (r.y - m) as i32)
    }
}

// --- Rückfrage „Gebäude löschen“ -------------------------------------------

/// Antwort der Rückfrage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Keep,
    Delete,
}

/// Knöpfe der Rückfrage: 0 Behalten, 1 Löschen, 2 Schließen (×).
const KEEP: usize = 0;
const DELETE: usize = 1;
const CLOSE: usize = 2;

/// Maße der Rückfrage (dip).
const CONFIRM_W: f32 = 600.0;
const CONFIRM_PAD: f32 = 24.0;
const CONFIRM_WRAP: f32 = 400.0;
const CONFIRM_BUTTON: (f32, f32) = (120.0, 40.0);

/// Text der Rückfrage: welche Bauteile verschwinden, je Art gezählt.
pub fn parts_text(m: &Model, b: BuildingId) -> String {
    let order = [
        (Category::ExteriorWall, "Außenwand", "Außenwände"),
        (Category::InteriorWall, "Innenwand", "Innenwände"),
        (Category::Floor, "Decke", "Decken"),
        (Category::GroundSlab, "Sohlplatte", "Sohlplatten"),
        (Category::StripFooting, "Frostschürze", "Frostschürzen"),
        (
            Category::EdgeInsulation,
            "Randdämmstreifen",
            "Randdämmstreifen",
        ),
    ];
    let parts = m.building_parts(b);
    let count = |c: Category| {
        parts
            .iter()
            .filter(|id| m.element(**id).is_some_and(|e| e.category == c))
            .count()
    };
    let mut names: Vec<String> = Vec::new();
    for (c, one, many) in order {
        match count(c) {
            0 => {}
            // Sohlplatte und Frostschürze gibt es je Gebäude einmal: ohne Zahl
            1 if matches!(c, Category::GroundSlab | Category::StripFooting) => {
                names.push(one.to_string())
            }
            1 => names.push(format!("1 {one}")),
            n => names.push(format!("{n} {many}")),
        }
    }
    let other = parts.len() - order.iter().map(|(c, ..)| count(*c)).sum::<usize>();
    if other > 0 {
        names.push(format!("{other} weitere"));
    }
    if names.is_empty() {
        "Das Gebäude hat noch keine Bauteile.".to_string()
    } else {
        format!(
            "Alle Bauteile dieses Gebäudes werden entfernt: {}.",
            names.join(", ")
        )
    }
}

/// „Gebäude N löschen?“: nennt die Bauteile, „Behalten“ ist vorgewählt
/// (Enter, Esc und × behalten), „Löschen“ und der Rand in `ui.danger`.
#[derive(Clone, Debug)]
pub struct ConfirmCard {
    pub building: BuildingId,
    pub title: String,
    pub text: String,
    /// Bauteile des Gebäudes (leuchten, solange die Karte steht).
    pub parts: Vec<ElementId>,
    focus: usize,
    hover: Option<usize>,
    pressed: Option<usize>,
}

impl ConfirmCard {
    pub fn new(m: &Model, b: BuildingId) -> Option<ConfirmCard> {
        let bd = m.building(b)?;
        // Nummer aus dem Modell (GB-02 → „Gebäude 2“), nie die Listenposition
        let title = match sk_model::building_index(&bd.number) {
            Some(n) => format!("Gebäude {n} löschen?"),
            None => format!("{} löschen?", bd.name),
        };
        Some(ConfirmCard {
            building: b,
            title,
            text: parts_text(m, b),
            parts: m.building_parts(b),
            focus: KEEP,
            hover: None,
            pressed: None,
        })
    }

    fn lines(&self, fonts: &Fonts, t: &Theme, s: f32) -> Vec<String> {
        widgets::wrap(
            fonts.regular.as_ref(),
            &self.text,
            t.size.font * s,
            CONFIRM_WRAP * s,
        )
    }

    /// Größe (Pixel, ohne Schatten).
    fn size(&self, fonts: &Fonts, t: &Theme, s: f32) -> (f32, f32) {
        let n = self.lines(fonts, t, s).len().max(1) as f32;
        let h = 108.0 + (n - 2.0).max(0.0) * 18.0 + CONFIRM_BUTTON.1 + 38.0;
        ((CONFIRM_W * s).round(), (h * s).round())
    }

    /// Lage im Fenster: mittig unten über dem Modell.
    pub fn rect(&self, fonts: &Fonts, t: &Theme, s: f32, win_w: u32, win_h: u32, top: u32) -> Rect {
        let (w, h) = self.size(fonts, t, s);
        let x = ((win_w as f32 - w) * 0.5).round().max(0.0);
        let y = (win_h as f32 - h - 16.0 * s).round().max(top as f32);
        Rect::new(x, y, w, h)
    }

    /// Knöpfe relativ zur Fläche: Behalten, Löschen, ×.
    pub(crate) fn buttons(&self, fonts: &Fonts, t: &Theme, s: f32) -> [Rect; 3] {
        let (w, h) = self.size(fonts, t, s);
        let (bw, bh) = (CONFIRM_BUTTON.0 * s, CONFIRM_BUTTON.1 * s);
        let y = h - (38.0 * s) - bh;
        let del = Rect::new(w - CONFIRM_PAD * s - bw, y, bw, bh);
        let keep = Rect::new(del.x - 12.0 * s - bw, y, bw, bh);
        let x = Rect::new(w - 40.0 * s, 10.0 * s, 28.0 * s, 28.0 * s);
        [keep, del, x]
    }

    fn button_at(
        &self,
        r: Rect,
        fonts: &Fonts,
        t: &Theme,
        s: f32,
        x: f64,
        y: f64,
    ) -> Option<usize> {
        let (lx, ly) = (x - r.x as f64, y - r.y as f64);
        self.buttons(fonts, t, s)
            .iter()
            .position(|b| b.contains(lx, ly))
    }

    /// Enter löst den Knopf mit dem Fokus aus (vorgewählt: Behalten), Esc
    /// behält, Tab und Pfeile wandern.
    pub fn key(&mut self, k: Key) -> Option<Answer> {
        match k {
            Key::Enter => Some(if self.focus == DELETE {
                Answer::Delete
            } else {
                Answer::Keep
            }),
            Key::Escape => Some(Answer::Keep),
            Key::Tab | Key::Left | Key::Right => {
                self.focus = 1 - self.focus;
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
        let h = self.button_at(r, fonts, t, s, x, y);
        let changed = h != self.hover;
        self.hover = h;
        changed
    }

    pub fn press(&mut self, r: Rect, fonts: &Fonts, t: &Theme, s: f32, x: f64, y: f64) {
        self.pressed = self.button_at(r, fonts, t, s, x, y);
    }

    /// Losgelassen auf demselben Knopf: seine Antwort (× behält).
    pub fn release(
        &mut self,
        r: Rect,
        fonts: &Fonts,
        t: &Theme,
        s: f32,
        x: f64,
        y: f64,
    ) -> Option<Answer> {
        let p = self.pressed.take()?;
        if self.button_at(r, fonts, t, s, x, y) != Some(p) {
            return None;
        }
        Some(if p == DELETE {
            Answer::Delete
        } else {
            Answer::Keep
        })
    }

    /// Bild samt Schatten; Lage links oben = `rect` minus Schatten.
    pub fn paint(&self, t: &Theme, fonts: &Fonts, s: f32) -> Canvas {
        let (w, h) = self.size(fonts, t, s);
        let m = (t.size.panel_shadow * s).round();
        let mut c = Canvas::new((w + 2.0 * m) as usize, (h + 2.0 * m) as usize);
        widgets::panel(&mut c, Rect::new(m, m, w, h), s, t);
        let u = &t.ui;
        // Rand in ui.danger
        let (rad, b) = (t.size.corner_radius * s, s.round().max(1.0));
        let mut p = Path::new();
        p.rounded_rect(m, m, w, h, rad);
        p.rounded_rect_hole(m + b, m + b, w - 2.0 * b, h - 2.0 * b, rad - b);
        c.fill(&p, u.danger);
        let (regular, bold) = (fonts.regular.as_ref(), fonts.bold.as_ref());
        let cap = |px: f32| regular.map_or(px * 0.7, |f| f.cap_height(px));
        let x = m + CONFIRM_PAD * s;
        let px = t.size.font_title * s;
        let base = (m + 36.0 * s + cap(px) * 0.5).round();
        widgets::text(&mut c, bold.or(regular), &self.title, px, x, base, u.text);
        let px2 = t.size.font * s;
        let mut b2 = (m + 64.0 * s + cap(px2) * 0.5).round();
        for l in self.lines(fonts, t, s) {
            widgets::text(&mut c, regular, &l, px2, x, b2, u.text_dim);
            b2 += 18.0 * s;
        }
        let [keep, del, close] = self.buttons(fonts, t, s);
        let at = |r: Rect| Rect::new(r.x + m, r.y + m, r.w, r.h);
        let st = |i: usize| ButtonState {
            hover: self.hover == Some(i),
            pressed: self.pressed == Some(i) && self.hover == Some(i),
            active: false,
            disabled: false,
        };
        widgets::button(&mut c, fonts, at(keep), "Behalten", st(KEEP), s, t);
        if self.focus == KEEP {
            focus_ring(&mut c, at(keep), s, u.text_dim);
        }
        danger_button(&mut c, fonts, at(del), "Löschen", st(DELETE), s, t);
        if self.focus == DELETE {
            focus_ring(&mut c, at(del), s, u.text);
        }
        // ×
        let cr = at(close);
        if self.hover == Some(CLOSE) {
            let mut p = Path::new();
            p.rounded_rect(cr.x, cr.y, cr.w, cr.h, 6.0 * s);
            c.fill(&p, u.hover);
        }
        let (cx, cy, d) = (cr.x + cr.w * 0.5, cr.y + cr.h * 0.5, 5.0 * s);
        let mut p = Path::new();
        p.segment((cx - d, cy - d), (cx + d, cy + d), 1.4 * s);
        p.segment((cx - d, cy + d), (cx + d, cy - d), 1.4 * s);
        c.fill(&p, u.text_dim);
        let small = t.size.font_small * s;
        let fb = (m + h - 16.0 * s).round();
        widgets::text(
            &mut c,
            regular,
            "Rückgängig mit Strg+Z, ein Schritt.",
            small,
            x,
            fb,
            u.text_dim,
        );
        c
    }
}

/// Feiner Ring um den vorgewählten Knopf.
fn focus_ring(c: &mut Canvas, r: Rect, s: f32, col: Rgba) {
    let (d, b) = (3.0 * s, s.round().max(1.0));
    let mut p = Path::new();
    p.rounded_rect(r.x - d, r.y - d, r.w + 2.0 * d, r.h + 2.0 * d, 6.0 * s + d);
    p.rounded_rect_hole(
        r.x - d + b,
        r.y - d + b,
        r.w + 2.0 * d - 2.0 * b,
        r.h + 2.0 * d - 2.0 * b,
        6.0 * s + d - b,
    );
    c.fill(&p, Rgba(col.0, col.1, col.2, 160));
}

/// Knopf in `ui.danger` („Löschen“ der Rückfrage).
fn danger_button(
    c: &mut Canvas,
    fonts: &Fonts,
    r: Rect,
    label: &str,
    st: ButtonState,
    s: f32,
    t: &Theme,
) {
    let u = &t.ui;
    let fill = if st.pressed {
        sk_ui::theme::lighten(u.danger)
    } else if st.hover {
        mix(u.danger, sk_ui::theme::lighten(u.danger), 0.5)
    } else {
        u.danger
    };
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, 6.0 * s);
    c.fill(&p, fill);
    if let Some(f) = fonts.bold.as_ref().or(fonts.regular.as_ref()) {
        let px = t.size.font * s;
        let x = r.x + (r.w - f.width(label, px)) * 0.5;
        let y = r.y + (r.h + f.cap_height(px)) * 0.5;
        f.draw(c, label, px, x.round(), y.round(), u.text);
    }
}

fn mix(a: Rgba, b: Rgba, k: f32) -> Rgba {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    Rgba(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2), m(a.3, b.3))
}

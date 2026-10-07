//! Typ-Liste am Chip (K3, Bild soll-katalog-1): beim Zeichnen neben dem
//! Paneel „Werkzeuge“ mit „Bauteilkatalog öffnen …“, in den Eigenschaften
//! unter dem Chip. Dort zeigt das Überfahren den neuen Typ als Vorschau im
//! Plan, ein Klick übernimmt ihn als einen Rückgängig-Schritt.

use crate::type_look::{cm_text, paint_thumb, type_look, TypeLook};
use crate::ui::Id;
use sk_model::{LayerSetId, Model, RunId, TypeCategory};
use sk_paint::{Canvas, Path};
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, Fonts, Rect};

/// Maße in dip.
const PAD: f32 = 8.0;
const HEAD: f32 = 26.0;
const ITEM_H: f32 = 52.0;
const ITEM_GAP: f32 = 6.0;
const LINK_H: f32 = 34.0;
const HINT_H: f32 = 24.0;
const TOOL_W: f32 = 300.0;
const PROPS_W: f32 = 292.0;
/// Abstand zum Paneel bzw. Chip.
const GAP: f32 = 10.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: LayerSetId,
    pub name: String,
    pub detail: String,
    pub look: TypeLook,
    pub standard: bool,
}

/// Was unter der Maus liegt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Item(usize),
    /// „Bauteilkatalog öffnen …“
    Catalog,
    Inside,
    Outside,
}

pub struct TypeMenu {
    /// Chip, an dem die Liste hängt.
    pub chip: Id,
    pub category: TypeCategory,
    pub items: Vec<Item>,
    /// Typ, den das Werkzeug zeichnet bzw. die Auswahl hat.
    pub current: Option<LayerSetId>,
    /// Züge der Auswahl (nur in den Eigenschaften).
    pub runs: Vec<RunId>,
    pub hover: Option<usize>,
    pub link_hover: bool,
    /// Links oben im Fenster (Pixel), ohne Schatten.
    pub x: f32,
    pub y: f32,
    pub scale: f32,
}

impl TypeMenu {
    /// Liste für den Chip `chip` an der Stelle `anchor` (Chip im Fenster);
    /// `panel` ist das Paneel, in dem er steht.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        chip: Id,
        m: &Model,
        theme: &Theme,
        category: TypeCategory,
        current: Option<LayerSetId>,
        runs: Vec<RunId>,
        anchor: Rect,
        panel: Rect,
        scale: f32,
        win: (f32, f32),
    ) -> TypeMenu {
        let standard = m.default_type(category);
        let items = m
            .layer_sets()
            .iter()
            .filter(|(_, t)| t.category == category)
            .map(|(id, t)| Item {
                id,
                name: t.name.clone(),
                detail: format!("{} · {}", t.code, cm_text(t.thickness())),
                look: type_look(m, theme, t),
                standard: id == standard,
            })
            .collect();
        let mut menu = TypeMenu {
            chip,
            category,
            items,
            current,
            runs,
            hover: None,
            link_hover: false,
            x: 0.0,
            y: 0.0,
            scale,
        };
        let (w, h) = menu.size();
        let s = scale;
        let (x, y) = if chip == Id::ToolType {
            // Rechts neben dem Paneel, erster Eintrag auf Höhe des Chips
            (panel.x + panel.w + GAP * s, anchor.y - (PAD + HEAD) * s)
        } else {
            // Unter dem Chip, rechtsbündig mit dem Paneel
            (panel.x + panel.w - w, anchor.y + anchor.h + 6.0 * s)
        };
        let top = 0.0;
        menu.x = x.clamp(0.0, (win.0 - w).max(0.0)).round();
        menu.y = y.clamp(top, (win.1 - h).max(top)).round();
        menu
    }

    fn tool(&self) -> bool {
        self.chip == Id::ToolType
    }

    /// Breite und Höhe in Pixeln.
    pub fn size(&self) -> (f32, f32) {
        let s = self.scale;
        let w = if self.tool() { TOOL_W } else { PROPS_W };
        let foot = if self.tool() {
            ITEM_GAP + 1.0 + LINK_H + HINT_H
        } else {
            HINT_H + 2.0
        };
        let n = self.items.len() as f32;
        let h = PAD + self.head() + n * ITEM_H + (n - 1.0).max(0.0) * ITEM_GAP + foot + PAD;
        ((w * s).round(), (h * s).round())
    }

    fn head(&self) -> f32 {
        if self.tool() {
            HEAD
        } else {
            0.0
        }
    }

    pub fn rect(&self) -> Rect {
        let (w, h) = self.size();
        Rect::new(self.x, self.y, w, h)
    }

    /// Eintrag `i` im Fenster.
    fn item_rect(&self, i: usize) -> Rect {
        let s = self.scale;
        let (w, _) = self.size();
        let y = self.y + (PAD + self.head() + i as f32 * (ITEM_H + ITEM_GAP)) * s;
        Rect::new(self.x + PAD * s, y, w - 2.0 * PAD * s, ITEM_H * s)
    }

    fn link_rect(&self) -> Option<Rect> {
        if !self.tool() || self.items.is_empty() {
            return None;
        }
        let s = self.scale;
        let last = self.item_rect(self.items.len() - 1);
        let y = last.y + last.h + (ITEM_GAP + 1.0) * s;
        Some(Rect::new(last.x, y, last.w, LINK_H * s))
    }

    pub fn hit(&self, x: f64, y: f64) -> Hit {
        if !self.rect().contains(x, y) {
            return Hit::Outside;
        }
        if let Some(i) = (0..self.items.len()).find(|&i| self.item_rect(i).contains(x, y)) {
            return Hit::Item(i);
        }
        if self.link_rect().is_some_and(|r| r.contains(x, y)) {
            return Hit::Catalog;
        }
        Hit::Inside
    }

    /// Pfeiltaste: nächster bzw. voriger Eintrag (vom aktuellen Typ aus).
    pub fn step(&mut self, down: bool) -> Option<usize> {
        let n = self.items.len();
        if n == 0 {
            return None;
        }
        let from = self
            .hover
            .or_else(|| self.items.iter().position(|i| Some(i.id) == self.current));
        let i = match (from, down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        self.hover = Some(i);
        Some(i)
    }

    /// Bild der Liste samt Schatten und seine Lage im Fenster.
    pub fn paint(&self, t: &Theme, fonts: &Fonts) -> (Canvas, i32, i32) {
        let s = self.scale;
        let u = &t.ui;
        let m = (t.size.panel_shadow * s).round();
        let (w, h) = self.size();
        let mut c = Canvas::new((w + 2.0 * m) as usize, (h + 2.0 * m) as usize);
        c.set_origin(self.x - m, self.y - m);
        widgets::panel_filled(&mut c, Rect::new(self.x, self.y, w, h), s, t, u.menu_bg);
        // Rand in Akzent wie der offene Chip
        let b = s.round().max(1.0);
        let rad = t.size.corner_radius * s;
        let mut p = Path::new();
        p.rounded_rect(self.x, self.y, w, h, rad);
        p.rounded_rect_hole(self.x + b, self.y + b, w - 2.0 * b, h - 2.0 * b, rad - b);
        c.fill(&p, u.accent);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let x0 = self.x + (PAD + 6.0) * s;
        if self.tool() {
            let label = sk_model::kinds::spec(self.category.category()).heading();
            let label = label.as_str();
            let y = self.y + (PAD + 14.0) * s;
            widgets::text(
                &mut c,
                bold,
                label,
                t.size.font_detail * s,
                x0,
                y.round(),
                u.text_dim,
            );
        }
        for (i, it) in self.items.iter().enumerate() {
            let r = self.item_rect(i);
            let cur = Some(it.id) == self.current;
            let hov = self.hover == Some(i);
            if cur || hov {
                let mut p = Path::new();
                p.rounded_rect(r.x, r.y, r.w, r.h, 6.0 * s);
                c.fill(&p, if hov { u.hover } else { u.pressed });
            }
            if cur {
                c.fill_rect(
                    r.x,
                    r.y + 4.0 * s,
                    (3.0 * s).round(),
                    r.h - 8.0 * s,
                    u.accent,
                );
            }
            let (tw, th) = (
                (t.size.catalog_thumb_w * s).round(),
                (t.size.catalog_thumb_h * s).round(),
            );
            let ty = (r.y + (r.h - th) * 0.5).round();
            paint_thumb(
                &mut c,
                Rect::new((r.x + 10.0 * s).round(), ty, tw, th),
                &it.look,
                s,
            );
            let tx = r.x + 10.0 * s + tw + 12.0 * s;
            let font = if cur { bold } else { regular };
            let px = t.size.font * s;
            let max_w = r.x + r.w - tx - 8.0 * s;
            let name = widgets::ellipsize(font, &it.name, px, max_w);
            widgets::text(
                &mut c,
                font,
                &name,
                px,
                tx,
                (r.y + 22.0 * s).round(),
                u.text,
            );
            let pd = t.size.font_detail * s;
            let yd = (r.y + 40.0 * s).round();
            widgets::text(&mut c, regular, &it.detail, pd, tx, yd, u.text_dim);
            if it.standard && self.tool() {
                let label = "Standard";
                let lw = bold.map_or(0.0, |f| f.width(label, pd));
                widgets::text(
                    &mut c,
                    bold,
                    label,
                    pd,
                    r.x + r.w - 10.0 * s - lw,
                    yd,
                    u.accent,
                );
            }
        }
        let pd = t.size.font_detail * s;
        if let Some(l) = self.link_rect() {
            widgets::separator(&mut c, l.x + 6.0 * s, l.y - 1.0 * s, l.w - 12.0 * s, s, t);
            let col = if self.link_hover {
                u.accent_hover
            } else {
                u.accent
            };
            let y = (l.y + 24.0 * s).round();
            widgets::text(
                &mut c,
                bold,
                "Bauteilkatalog öffnen …",
                t.size.font * s,
                x0,
                y,
                col,
            );
            let hint = match self.category {
                TypeCategory::ExteriorWall => "Innenwände wählt das Werkzeug „Innenwand“.",
                TypeCategory::InteriorWall => "Außenwände wählt das Werkzeug „Gebäude“.",
                _ => "",
            };
            let y = (l.y + l.h + 16.0 * s).round();
            widgets::text(&mut c, regular, hint, pd, x0, y, u.text_dim);
        } else {
            let y = (self.y + h - (PAD + 8.0) * s).round();
            let hint = "Klick übernimmt, ein Rückgängig-Schritt";
            widgets::text(&mut c, regular, hint, pd, x0, y, u.text_dim);
        }
        (c, (self.x - m) as i32, (self.y - m) as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liste_trifft_eintraege_und_katalog() {
        let m = Model::new();
        let t = Theme::dark();
        let anchor = Rect::new(46.0, 236.0, 178.0, 46.0);
        let panel = Rect::new(32.0, 84.0, 206.0, 300.0);
        let cur = Some(m.default_type(TypeCategory::ExteriorWall));
        let menu = TypeMenu::new(
            Id::ToolType,
            &m,
            &t,
            TypeCategory::ExteriorWall,
            cur,
            Vec::new(),
            anchor,
            panel,
            1.0,
            (1600.0, 1000.0),
        );
        assert!(!menu.items.is_empty());
        assert!(menu
            .items
            .iter()
            .all(|i| m.layer_set(i.id).unwrap().category == TypeCategory::ExteriorWall));
        assert_eq!(menu.items.iter().filter(|i| i.standard).count(), 1);
        // Rechts neben dem Paneel
        assert_eq!(menu.x, 248.0);
        let r = menu.item_rect(0);
        assert_eq!(
            menu.hit((r.x + 20.0) as f64, (r.y + 10.0) as f64),
            Hit::Item(0)
        );
        let l = menu.link_rect().unwrap();
        assert_eq!(
            menu.hit((l.x + 20.0) as f64, (l.y + 10.0) as f64),
            Hit::Catalog
        );
        assert_eq!(menu.hit(5.0, 5.0), Hit::Outside);
        let mut menu = menu;
        let n = menu.items.len();
        let start = menu.items.iter().position(|i| Some(i.id) == cur).unwrap();
        assert_eq!(menu.step(true), Some((start + 1) % n));
        let (c, ..) = menu.paint(
            &t,
            &Fonts {
                regular: None,
                bold: None,
                italic: None,
            },
        );
        assert!(c.width > 0);
    }
}

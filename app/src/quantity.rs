//! Mengenfenster (F2): eigenes Programmfenster mit derselben Titelleiste wie
//! das Hauptfenster. Bis B7 kommt, zeigt es nur eine schlichte Liste der
//! Bauteile, an der sich die Fensterschicht und der gemeinsame Hover- und
//! Auswahlzustand prüfen lassen; geöffnet wird es nur mit `--mengenfenster`.

use crate::picking::Picking;
use crate::scene::Scene;
use sk_model::ElementId;
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{CaptionArea, Event, Key, MouseButton, WindowCommand};
use sk_ui::theme::Theme;
use sk_ui::titlebar::{Button, TitleBar};
use sk_ui::widgets::Fonts;

/// Zeilenhöhe, Rand und Schriftgröße der Liste (dip).
const ROW: f32 = 26.0;
const PAD: f32 = 12.0;
const TEXT: f32 = 13.0;

/// Was ein Ereignis im Mengenfenster für die App bedeutet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    /// Bauteil unter der Maus (oder keines).
    Hover(Option<ElementId>),
    /// Klick auf eine Zeile; mit Strg.
    Click(ElementId, bool),
    /// Esc: Auswahl aufheben.
    Clear,
    Command(WindowCommand),
    Close,
}

pub struct QuantityWindow {
    /// Fenster offen (aus Sicht der App).
    pub open: bool,
    pub w: u32,
    pub h: u32,
    pub title: TitleBar,
    /// Bauteile in Modellreihenfolge: Kennung und Text der Zeile.
    rows: Vec<(ElementId, String)>,
    rows_rev: Option<u64>,
    /// Gerollt um so viele Pixel.
    scroll: f32,
    /// Zuletzt sichtbar gemachte Auswahl (rollt nur bei einer neuen).
    shown_primary: Option<ElementId>,
    /// Muss neu gezeichnet und gezeigt werden.
    pub dirty: bool,
}

impl QuantityWindow {
    pub fn new() -> QuantityWindow {
        let mut title = TitleBar::new(1.0);
        title.side = true;
        QuantityWindow {
            open: false,
            w: 0,
            h: 0,
            title,
            rows: Vec::new(),
            rows_rev: None,
            scroll: 0.0,
            shown_primary: None,
            dirty: false,
        }
    }

    pub fn caption_area(&self) -> CaptionArea {
        CaptionArea {
            height: self.title.height(),
            buttons_width: self.title.buttons_width(),
            left_width: 0,
        }
    }

    fn row_h(&self) -> f32 {
        (ROW * self.title.scale).round()
    }

    fn list_top(&self) -> f32 {
        self.title.height() as f32 + (PAD * self.title.scale).round()
    }

    /// Zeilen an den Modellstand angleichen. Neu gezeichnet wird nur, wenn sich
    /// eine Zeile ändert: Beim Ziehen steigt die Revision je Bild, die Liste
    /// bleibt gleich, und ein ganzes Fensterbild kostet mehrere Millisekunden.
    pub fn sync_rows(&mut self, scene: &Scene) {
        let m = scene.model();
        if self.rows_rev == Some(m.revision()) {
            return;
        }
        self.rows_rev = Some(m.revision());
        let rows: Vec<(ElementId, String)> = m
            .elements()
            .iter()
            .map(|(id, e)| {
                let storey = m.storey(e.storey).map_or("–".into(), |s| s.short.clone());
                (
                    id,
                    format!("{}   {}   {storey}", e.number, e.category.name()),
                )
            })
            .collect();
        if rows == self.rows {
            return;
        }
        self.rows = rows;
        self.clamp_scroll();
        self.dirty = true;
    }

    fn content_h(&self) -> f32 {
        self.rows.len() as f32 * self.row_h()
    }

    fn clamp_scroll(&mut self) {
        let view = (self.h as f32 - self.list_top()).max(0.0);
        self.scroll = self.scroll.clamp(0.0, (self.content_h() - view).max(0.0));
    }

    /// Rollt die neu gewählte Zeile in Sicht (Auswahl im Modell).
    pub fn reveal(&mut self, picking: &Picking) {
        let p = picking.primary();
        if p == self.shown_primary {
            return;
        }
        self.shown_primary = p;
        let Some(i) = p.and_then(|id| self.rows.iter().position(|r| r.0 == id)) else {
            return;
        };
        let (rh, view) = (self.row_h(), self.h as f32 - self.list_top());
        let y = i as f32 * rh;
        if y < self.scroll {
            self.scroll = y;
        } else if y + rh > self.scroll + view {
            self.scroll = y + rh - view;
        }
        self.clamp_scroll();
        self.dirty = true;
    }

    fn row_at(&self, x: f64, y: f64) -> Option<ElementId> {
        let top = self.list_top() as f64;
        if y < top || x < 0.0 || x >= self.w as f64 {
            return None;
        }
        let i = ((y - top + self.scroll as f64) / self.row_h() as f64).floor() as usize;
        self.rows.get(i).map(|r| r.0)
    }

    /// Ereignis des Mengenfensters.
    pub fn handle(&mut self, e: &Event) -> Option<Out> {
        match *e {
            Event::Resized { width, height } => {
                (self.w, self.h) = (width, height);
                self.clamp_scroll();
                self.dirty = true;
                None
            }
            Event::ScaleChanged(s) => {
                self.title.scale = s;
                self.dirty = true;
                None
            }
            Event::Maximized(m) => {
                self.title.maximized = m;
                self.dirty = true;
                None
            }
            Event::Focus(f) => {
                self.title.active = f;
                self.dirty = true;
                None
            }
            Event::Redraw => {
                self.dirty = true;
                None
            }
            Event::CloseRequested { .. } => Some(Out::Close),
            Event::MouseMove { x, y, .. } => {
                let b = self.title.button_at(x, y, self.w);
                if b != self.title.hover {
                    self.title.hover = b;
                    self.dirty = true;
                }
                Some(Out::Hover(self.row_at(x, y)))
            }
            Event::MouseLeave => {
                if self.title.hover.take().is_some() {
                    self.dirty = true;
                }
                Some(Out::Hover(None))
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            } => {
                if let Some(b) = self.title.button_at(x, y, self.w) {
                    self.title.pressed = Some(b);
                    self.dirty = true;
                    return None;
                }
                self.row_at(x, y).map(|id| Out::Click(id, mods.ctrl))
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let pressed = self.title.pressed.take()?;
                self.dirty = true;
                if self.title.button_at(x, y, self.w) != Some(pressed) {
                    return None;
                }
                Some(match pressed {
                    Button::Minimize => Out::Command(WindowCommand::Minimize),
                    Button::Maximize => Out::Command(WindowCommand::ToggleMaximize),
                    _ => Out::Close,
                })
            }
            Event::Wheel { delta, .. } => {
                self.scroll -= delta as f32 * 3.0 * self.row_h();
                self.clamp_scroll();
                self.dirty = true;
                None
            }
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } => Some(Out::Clear),
            _ => None,
        }
    }

    /// Ganzes Fensterbild; Hover und Auswahl in denselben Farben wie im Modell.
    pub fn paint(&self, t: &Theme, fonts: &Fonts, picking: &Picking) -> Canvas {
        let (w, h) = (self.w as usize, self.h as usize);
        let mut c = Canvas::new(w, h);
        c.clear(t.ui.bg);
        let s = self.title.scale;
        let (top, rh) = (self.list_top(), self.row_h());
        let font = fonts.regular.as_ref();
        let px = (TEXT * s).round();
        let pad = (PAD * s).round();
        let hover = Rgba::from_f32(t.interact.hover_element);
        let first = (self.scroll / rh).floor().max(0.0) as usize;
        for (i, (id, text)) in self.rows.iter().enumerate().skip(first) {
            let y = top + i as f32 * rh - self.scroll;
            if y >= h as f32 {
                break;
            }
            let selected = picking.is_selected(*id);
            let band = if selected {
                Some(t.ui.accent)
            } else if picking.hover == Some(*id) {
                Some(hover)
            } else {
                None
            };
            if let Some(b) = band {
                let mut p = Path::new();
                let r = t.size.corner_radius * s;
                p.rounded_rect(pad * 0.5, y + 1.0, w as f32 - pad, rh - 2.0, r);
                c.fill(&p, b);
            }
            if let Some(f) = font {
                let col = if selected { t.ui.on_accent } else { t.ui.text };
                let ty = (y + (rh + f.cap_height(px)) * 0.5).round();
                f.draw(&mut c, text, px, pad, ty, col);
            }
        }
        // Titelleiste zuletzt: Zeilen rollen darunter weg
        let bar = self.title.paint(t, font, self.w);
        c.blit(&bar, 0, 0);
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{Model, RefSide, WallChain};

    /// Ziehen ändert die Revision je Bild, aber keine Zeile: kein neues
    /// Fensterbild. Ein neues Bauteil zeichnet neu.
    #[test]
    fn ziehen_zeichnet_die_liste_nicht_neu() {
        let mut s = Scene::with_model(Model::with_seed(1));
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let eg = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(eg);
        let wand = |x: f64| WallChain {
            base: 0.0,
            points: vec![vec3(x, 0.0, 0.0), vec3(x + 5000.0, 0.0, 0.0)],
            closed: false,
            ref_side: RefSide::Left,
            layers: Vec::new(),
            height: 3500.0,
            joints: Default::default(),
        };
        s.add_wall(&wand(0.0)).unwrap();
        let mut q = QuantityWindow::new();
        q.sync_rows(&s);
        assert!(q.dirty && !q.rows.is_empty());
        q.dirty = false;

        s.begin("Geschoss ziehen");
        for top in [2800.0, 2900.0] {
            let rev = s.model().revision();
            s.drag_storey_top(eg, top);
            assert_ne!(s.model().revision(), rev, "Ziehen ändert die Revision");
            q.sync_rows(&s);
            assert!(!q.dirty, "gleiche Zeilen: kein neues Fensterbild");
        }
        s.commit();

        let n = q.rows.len();
        s.add_wall(&wand(10000.0)).unwrap();
        q.sync_rows(&s);
        assert!(q.dirty && q.rows.len() > n, "neues Bauteil: neue Zeile");
    }
}

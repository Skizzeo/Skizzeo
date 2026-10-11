//! Geschossfenster (Jörn 11.10.): eigenes Programmfenster links neben dem
//! Hauptfenster, gebaut wie das Mengenfenster rechts ([`crate::quantity`]):
//! dieselbe Titelleiste, angedockt mit einer Fuge, lösbar auf einen zweiten
//! Bildschirm. Darunter das Diagramm der Geschosse ([`Ui::fenster`]); was
//! dort gezogen, getippt oder gewählt wird, gibt [`Out::Ui`] an die App.

use crate::ui::{Panel, Ui, UiOut, LEVELS_TITEL};
use sk_paint::Canvas;
use sk_platform::{CaptionArea, Event, Key, Modifiers, MouseButton, WindowCommand};
use sk_ui::theme::Theme;
use sk_ui::titlebar::{Button, TitleBar};

/// Fuge zum Hauptfenster (dip), wie beim Mengenfenster.
const SEAM: f32 = 2.0;

/// Was ein Ereignis im Geschossfenster für die App bedeutet.
pub enum Out {
    /// Ergebnis der Oberfläche: Eingabe, Ziehen, Klick auf ein Geschoss.
    Ui(UiOut),
    /// Taste ohne Eingabe in einer Zahl (Esc beim Ziehen, Rückgängig).
    Key(Key, Modifiers),
    Command(WindowCommand),
    Close,
}

pub struct GeschossFenster {
    /// Fenster offen (aus Sicht der App).
    pub open: bool,
    /// Knopf „Geschossmanager“ geklickt (wird nach den Ereignissen geöffnet).
    pub wanted: bool,
    pub w: u32,
    pub h: u32,
    pub title: TitleBar,
    pub ui: Ui,
    /// Muss neu gezeichnet und gezeigt werden.
    pub dirty: bool,
    /// Angedockt: Fuge am rechten Rand.
    pub docked: bool,
    /// Stand des Farbschemas im gezeigten Bild.
    theme_rev: u64,
    canvas: Canvas,
    shown: Vec<u8>,
}

impl GeschossFenster {
    pub fn new(theme: &Theme) -> GeschossFenster {
        let mut title = TitleBar::new(1.0);
        title.side = true;
        title.caption = LEVELS_TITEL.into();
        GeschossFenster {
            open: false,
            wanted: false,
            w: 0,
            h: 0,
            title,
            ui: Ui::fenster(1.0, theme),
            dirty: false,
            docked: true,
            theme_rev: theme.rev,
            canvas: Canvas::new(0, 0),
            shown: Vec::new(),
        }
    }

    pub fn caption_area(&self) -> CaptionArea {
        CaptionArea {
            height: self.title.height(),
            buttons_width: self.title.buttons_width(),
            left_width: 0,
        }
    }

    /// Oberfläche an Fenstergröße und Skalierung angleichen.
    fn fit(&mut self) {
        let top = self.title.height();
        if self.ui.fit_fenster(self.title.scale, self.w, self.h, top) {
            self.dirty = true;
        }
    }

    /// Ereignis im Fenster (Fensterpixel).
    pub fn handle(&mut self, e: &Event) -> Option<Out> {
        match *e {
            Event::Resized { width, height } => {
                (self.w, self.h) = (width, height);
                self.fit();
                self.dirty = true;
                None
            }
            Event::ScaleChanged(s) => {
                self.title.scale = s;
                self.fit();
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
            Event::Key {
                key, down, mods, ..
            } => match self.ui.key(key, down, mods) {
                Some(out) => Some(Out::Ui(out)),
                None => down.then_some(Out::Key(key, mods)),
            },
            Event::MouseMove { x, y, .. } => {
                let b = self.title.button_at(x, y, self.w);
                if b != self.title.hover {
                    self.title.hover = b;
                    self.dirty = true;
                }
                self.ui_event(e)
            }
            Event::MouseLeave => {
                if self.title.hover.take().is_some() {
                    self.dirty = true;
                }
                self.ui_event(e)
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } if self.ui.level_dragging().is_none() => {
                if let Some(b) = self.title.button_at(x, y, self.w) {
                    self.title.pressed = Some(b);
                    self.dirty = true;
                    return None;
                }
                self.ui_event(e)
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } if self.title.pressed.is_some() => {
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
            Event::MouseDown { .. } | Event::MouseUp { .. } => self.ui_event(e),
            Event::Text(_) | Event::Wheel { .. } => None,
        }
    }

    /// Mausereignis für das Diagramm.
    fn ui_event(&mut self, e: &Event) -> Option<Out> {
        let out = self.ui.handle(e, self.w, self.title.height());
        if !out.changed.is_empty() || out.relayout {
            self.dirty = true;
        }
        Some(Out::Ui(out))
    }

    /// Fensterbild (vormultipliziert), wenn es neu gezeigt werden muss.
    pub fn frame(&mut self, t: &Theme) -> Option<&[u8]> {
        if t.rev != self.theme_rev {
            self.theme_rev = t.rev;
            self.dirty = true;
        }
        if self.w == 0 || self.h == 0 || !self.dirty {
            return None;
        }
        self.dirty = false;
        let (w, h) = (self.w as usize, self.h as usize);
        let mut c = std::mem::replace(&mut self.canvas, Canvas::new(0, 0));
        c.reuse(w, h);
        c.clear(t.ui.bg);
        let top = self.title.height();
        let (img, x, y) = self.ui.paint(t, Panel::Levels, self.w, top);
        c.blit(img, x, y);
        let bar = self.title.paint(t, self.ui.fonts.regular.as_ref(), self.w);
        c.blit(&bar, 0, 0);
        if self.docked {
            let sw = (SEAM * self.title.scale).max(1.0);
            c.fill_rect(w as f32 - sw, 0.0, sw, h as f32, t.title.bg);
        }
        c.premul_rgba8_into(&mut self.shown);
        self.canvas = c;
        Some(&self.shown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

    const M: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    fn fenster() -> GeschossFenster {
        let th = Theme::dark();
        let mut f = GeschossFenster::new(&th);
        f.handle(&Event::ScaleChanged(1.0));
        f.handle(&Event::Resized {
            width: 460,
            height: 900,
        });
        f.ui.set_levels(Scene::new().levels());
        f
    }

    /// Größe und Titelleiste gehen an die Oberfläche; das Bild hat
    /// Fenstergröße, oben die Titelleiste, rechts die Fuge.
    #[test]
    fn bild_mit_titelleiste_und_fuge() {
        let th = Theme::dark();
        let mut f = fenster();
        let px = f.frame(&th).expect("erstes Bild").to_vec();
        assert_eq!(px.len(), 460 * 900 * 4);
        assert!(f.frame(&th).is_none(), "unverändert kein neues Bild");
        let at = |x: usize, y: usize| px[(y * 460 + x) * 4..][..3].to_vec();
        let bg = th.ui.bg;
        assert_eq!(at(230, 880), [bg.0, bg.1, bg.2], "Fläche des Diagramms");
        let fuge = th.title.bg;
        assert_eq!(at(459, 500), [fuge.0, fuge.1, fuge.2]);
        f.docked = false;
        f.dirty = true;
        let px = f.frame(&th).unwrap();
        assert_eq!(&px[(500 * 460 + 459) * 4..][..3], [bg.0, bg.1, bg.2]);
    }

    /// Titelleiste: Schließen meldet `Close`, Minimieren den Befehl; im
    /// Diagramm gehen Maus und Tasten an die Oberfläche, ohne Eingabe meldet
    /// eine Taste sich selbst.
    #[test]
    fn titelleiste_und_diagramm() {
        let mut f = fenster();
        let klick = |f: &mut GeschossFenster, x: f64, y: f64| {
            let mut last = None;
            for e in [
                Event::MouseMove { x, y, mods: M },
                Event::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                    mods: M,
                },
                Event::MouseUp {
                    button: MouseButton::Left,
                    x,
                    y,
                    mods: M,
                },
            ] {
                last = f.handle(&e);
            }
            last
        };
        let bw = f.title.button_width() as f64;
        assert!(matches!(
            klick(&mut f, 460.0 - bw * 0.5, 10.0),
            Some(Out::Close)
        ));
        assert!(matches!(
            klick(&mut f, 460.0 - bw * 2.5, 10.0),
            Some(Out::Command(WindowCommand::Minimize))
        ));
        assert!(matches!(
            f.handle(&Event::Key {
                key: Key::Char('Z'),
                down: true,
                repeat: false,
                mods: Modifiers { ctrl: true, ..M },
            }),
            Some(Out::Key(Key::Char('Z'), _))
        ));
        assert!(matches!(klick(&mut f, 230.0, 600.0), Some(Out::Ui(_))));
    }
}

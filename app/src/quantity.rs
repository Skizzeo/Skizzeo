//! Mengenfenster (F2, B7): eigenes Programmfenster mit derselben Titelleiste
//! wie das Hauptfenster; darunter das Blatt der Mengenermittlung
//! ([`ListView`]). Hover und Auswahl laufen über den gemeinsamen Zustand.

use crate::picking::Picking;
use crate::scene::Scene;
use crate::schedule_view::{ListOut, ListView};
use sk_model::ElementId;
use sk_paint::Canvas;
use sk_platform::{CaptionArea, Event, Key, MouseButton, WindowCommand};
use sk_ui::theme::Theme;
use sk_ui::titlebar::{Button, TitleBar};
use sk_ui::widgets::Fonts;
use std::time::{Duration, Instant};

/// Fuge zum Hauptfenster (dip) und Dauer ihres Aufblinkens beim Einrasten.
const SEAM: f32 = 2.0;
const SEAM_FLASH: Duration = Duration::from_millis(150);

/// Fensterbild zum Zeigen: Bytes und, falls nur ein Teil neu ist, dessen
/// Zeilen (von, bis).
pub type Frame<'a> = (&'a [u8], Option<(u32, u32)>);

/// Was ein Ereignis im Mengenfenster für die App bedeutet.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// Hover oder Auswahl im gemeinsamen Zustand geändert (`selection`:
    /// die Auswahl, sonst nur der Hover).
    Picking {
        selection: bool,
    },
    /// Doppelklick: die aktive Ansicht holt die Bauteile ins Bild.
    Zoom(Vec<ElementId>),
    SaveCsv,
    Command(WindowCommand),
    Close,
}

pub struct QuantityWindow {
    /// Fenster offen (aus Sicht der App).
    pub open: bool,
    pub w: u32,
    pub h: u32,
    pub title: TitleBar,
    pub list: Option<ListView>,
    /// Muss neu gezeichnet und gezeigt werden.
    pub dirty: bool,
    /// Angedockt (für die Fuge) und seit wann die Fuge aufblinkt.
    pub docked: bool,
    seam_flash: Option<Instant>,
    /// Im letzten Bild lief eine Animation (dann noch ein Schlussbild).
    was_busy: bool,
    /// Zuletzt gezeigtes Fensterbild (vormultipliziert) und was die Pille
    /// „wird aktualisiert“ darin zeigt: Beim Ziehen im Modell wird nur die
    /// Pille neu gezeichnet, nicht das ganze Fenster (Review 1g).
    shown: Vec<u8>,
    pill_shown: Option<(u8, u8)>,
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
            list: None,
            dirty: false,
            docked: true,
            seam_flash: None,
            was_busy: false,
            shown: Vec::new(),
            pill_shown: None,
        }
    }

    pub fn caption_area(&self) -> CaptionArea {
        CaptionArea {
            height: self.title.height(),
            buttons_width: self.title.buttons_width(),
            left_width: 0,
        }
    }

    /// Andockzustand aus der Fensterschicht; beim Einrasten blinkt die Fuge.
    pub fn set_docked(&mut self, docked: bool, animate: bool) {
        if docked != self.docked {
            if docked && animate {
                self.seam_flash = Some(Instant::now());
            }
            self.docked = docked;
            self.dirty = true;
        }
    }

    /// An Modell und gemeinsamen Zustand angleichen.
    pub fn sync(&mut self, s: &mut Scene, p: &Picking, animate: bool) {
        let list = self.list.get_or_insert_with(|| ListView::new(s));
        list.scale = self.title.scale;
        (list.w, list.h) = (self.w, self.h);
        if list.sync(s, animate) | list.follow(s, p) {
            self.dirty = true;
        }
    }

    /// Animationen weiterführen; `true`, solange weitere Bilder nötig sind.
    pub fn tick(&mut self, t: &Theme, now: Instant) -> bool {
        let pill = t.size.anim_ms > 0.0
            && self
                .list
                .as_ref()
                .is_some_and(|l| l.pill_key(t, now).is_some());
        let mut busy = self.list.as_mut().is_some_and(|l| l.tick(t, now));
        if let Some(at) = self.seam_flash {
            if now.duration_since(at) < SEAM_FLASH && t.size.anim_ms > 0.0 {
                busy = true;
            } else {
                self.seam_flash = None;
                self.dirty = true;
            }
        }
        if busy || self.was_busy {
            self.dirty = true;
        }
        self.was_busy = busy;
        busy || pill
    }

    fn list_out(&mut self, o: Option<ListOut>) -> Option<Out> {
        match o? {
            ListOut::Repaint => {
                self.dirty = true;
                None
            }
            ListOut::Picking { selection } => {
                self.dirty = true;
                Some(Out::Picking { selection })
            }
            ListOut::Zoom(v) => Some(Out::Zoom(v)),
            ListOut::SaveCsv => {
                self.dirty = true;
                Some(Out::SaveCsv)
            }
        }
    }

    /// Ereignis des Mengenfensters.
    pub fn handle(&mut self, e: &Event, t: &Theme, fonts: &Fonts, p: &mut Picking) -> Option<Out> {
        match *e {
            Event::Resized { width, height } => {
                (self.w, self.h) = (width, height);
                if let Some(l) = self.list.as_mut() {
                    (l.w, l.h) = (width, height);
                }
                self.dirty = true;
                None
            }
            Event::ScaleChanged(s) => {
                self.title.scale = s;
                if let Some(l) = self.list.as_mut() {
                    l.scale = s;
                }
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
                let o = self.list.as_mut()?.mouse_move(t, fonts, p, x, y);
                self.list_out(o)
            }
            Event::MouseLeave => {
                if self.title.hover.take().is_some() {
                    self.dirty = true;
                }
                let o = self.list.as_mut()?.mouse_leave(p);
                self.list_out(o)
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
                let o = self.list.as_mut()?.mouse_down(t, fonts, p, (x, y), mods);
                self.list_out(o)
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Some(pressed) = self.title.pressed.take() {
                    self.dirty = true;
                    if self.title.button_at(x, y, self.w) != Some(pressed) {
                        return None;
                    }
                    return Some(match pressed {
                        Button::Minimize => Out::Command(WindowCommand::Minimize),
                        Button::Maximize => Out::Command(WindowCommand::ToggleMaximize),
                        _ => Out::Close,
                    });
                }
                let o = self.list.as_mut()?.mouse_up(t, fonts, x, y);
                self.list_out(o)
            }
            Event::Wheel { delta, .. } => {
                let anim = t.size.anim_ms > 0.0;
                let o = self.list.as_mut()?.wheel(delta, t, anim);
                self.list_out(o)
            }
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } => {
                let l = self.list.as_mut()?;
                let had = !p.selected.is_empty();
                l.clear_selection(p);
                self.dirty = true;
                had.then_some(Out::Picking { selection: true })
            }
            _ => None,
        }
    }

    /// Neues Fensterbild, falls nötig: das ganze Bild oder, wenn sich nur die
    /// Pille „wird aktualisiert“ geändert hat, nur ihre Zeilen (von, bis).
    pub fn frame(&mut self, t: &Theme, fonts: &Fonts, now: Instant) -> Option<Frame<'_>> {
        if self.w == 0 || self.h == 0 {
            return None;
        }
        let key = self.list.as_ref().and_then(|l| l.pill_key(t, now));
        let full = self.dirty || self.shown.len() != self.w as usize * self.h as usize * 4;
        if full {
            self.shown = self.paint(t, fonts, now).to_premul_rgba8();
            self.dirty = false;
            self.pill_shown = key;
            return Some((&self.shown, None));
        }
        if key == self.pill_shown {
            return None;
        }
        self.pill_shown = key;
        let l = self.list.as_ref()?;
        let (rx, ry, rw, rh) = l.pill_rect(t, fonts)?;
        let mut c = Canvas::new(rw.max(1) as usize, rh.max(1) as usize);
        c.clear(t.ui.sheet_bg);
        l.paint_pill(&mut c, t, fonts, now, (rx as f32, ry as f32));
        let px = c.to_premul_rgba8();
        let (w, h) = (self.w as i32, self.h as i32);
        for y in ry.max(0)..(ry + rh).min(h) {
            let (x1, x2) = (rx.max(0), (rx + rw).min(w));
            if x1 >= x2 {
                continue;
            }
            let src = (((y - ry) * rw + (x1 - rx)) * 4) as usize;
            let dst = ((y * w + x1) * 4) as usize;
            let n = ((x2 - x1) * 4) as usize;
            self.shown[dst..dst + n].copy_from_slice(&px[src..src + n]);
        }
        let rows = (ry.clamp(0, h) as u32, (ry + rh).clamp(0, h) as u32);
        Some((&self.shown, Some(rows)))
    }

    /// Ganzes Fensterbild: Blatt, Fuge zum Hauptfenster, Titelleiste.
    pub fn paint(&self, t: &Theme, fonts: &Fonts, now: Instant) -> Canvas {
        let (w, h) = (self.w as usize, self.h as usize);
        let mut c = Canvas::new(w, h);
        c.clear(t.ui.sheet_bg);
        if let Some(l) = &self.list {
            l.paint(&mut c, t, fonts, now);
        }
        let bar = self.title.paint(t, fonts.regular.as_ref(), self.w);
        c.blit(&bar, 0, 0);
        if self.docked {
            let s = self.title.scale;
            let col = match self.seam_flash {
                Some(at) if now.duration_since(at) < SEAM_FLASH => t.ui.accent,
                _ => t.title.bg,
            };
            c.fill_rect(0.0, 0.0, (SEAM * s).max(1.0), h as f32, col);
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{Model, RefSide, WallChain};

    /// Review 1g (PR #18 auf B7 übertragen): Ziehen ändert die Revision je
    /// Bild, die Liste wartet aber auf das Loslassen. Nach dem ersten Bild
    /// mit der Pille „wird aktualisiert“ entsteht kein ganzes Fensterbild
    /// mehr, höchstens die Zeilen der Pille. Loslassen zeichnet neu.
    #[test]
    fn ziehen_zeichnet_die_liste_nicht_neu() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let mut s = Scene::with_model(Model::with_seed(1));
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let eg = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(eg);
        s.add_wall(&WallChain {
            base: 0.0,
            points: vec![vec3(0.0, 0.0, 0.0), vec3(5000.0, 0.0, 0.0)],
            closed: false,
            ref_side: RefSide::Left,
            layers: Vec::new(),
            height: 3500.0,
            joints: Default::default(),
        })
        .unwrap();
        let p = Picking::default();
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (520, 1000);
        let now = Instant::now();
        q.sync(&mut s, &p, true);
        q.tick(&t, now);
        assert!(
            matches!(q.frame(&t, &fonts, now), Some((_, None))),
            "erstes Bild ganz"
        );
        assert!(q.frame(&t, &fonts, now).is_none(), "nichts geändert");

        s.begin("Geschoss ziehen");
        let mut full = 0;
        for (i, top) in [2800.0, 2900.0, 2850.0, 2950.0].into_iter().enumerate() {
            let rev = s.model().revision();
            s.drag_storey_top(eg, top);
            assert_ne!(s.model().revision(), rev, "Ziehen ändert die Revision");
            let at = now + Duration::from_millis(100 * i as u64);
            q.sync(&mut s, &p, true);
            q.tick(&t, at);
            if let Some((_, None)) = q.frame(&t, &fonts, at) {
                full += 1;
            }
        }
        assert_eq!(
            full, 1,
            "nur das Bild, in dem die Pille erscheint, ist ganz"
        );
        s.commit();
        q.sync(&mut s, &p, true);
        q.tick(&t, now + Duration::from_millis(500));
        assert!(
            matches!(q.frame(&t, &fonts, now), Some((_, None))),
            "Loslassen: neue Mengen, ganzes Bild"
        );
    }
}

//! Mengenfenster (F2, B7): eigenes Programmfenster mit derselben Titelleiste
//! wie das Hauptfenster; darunter das Blatt der Mengenermittlung
//! ([`ListView`]). Hover und Auswahl laufen über den gemeinsamen Zustand.

use crate::picking::Picking;
use crate::scene::Scene;
use crate::schedule_view::{ListOut, ListView, RowBand};
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
    /// Hover oder Auswahl haben sich geändert: nur die Zeilen neu zeichnen,
    /// deren Band anders aussieht (Review 1h, U5). Dazu Bänder und Knopf,
    /// wie sie im gezeigten Bild stehen.
    bands_dirty: bool,
    bands_shown: Vec<RowBand>,
    button_shown: (bool, bool),
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
            bands_dirty: false,
            bands_shown: Vec::new(),
            button_shown: (false, false),
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
        if list.sync(s, animate) {
            self.dirty = true;
        }
        // Liegen die Zeilen danach anders (aufgeklappt, gerollt), zeichnet
        // `frame` doch das ganze Bild
        if list.follow(s, p) {
            self.bands_dirty = true;
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
                self.bands_dirty = true;
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
                self.bands_dirty = true;
                had.then_some(Out::Picking { selection: true })
            }
            _ => None,
        }
    }

    /// Neues Fensterbild, falls nötig: das ganze Bild oder, wenn sich nur
    /// Bänder (Hover, Auswahl) oder die Pille „wird aktualisiert“ geändert
    /// haben, nur deren Zeilen (von, bis).
    pub fn frame(&mut self, t: &Theme, fonts: &Fonts, now: Instant) -> Option<Frame<'_>> {
        if self.w == 0 || self.h == 0 {
            return None;
        }
        let key = self.list.as_ref().and_then(|l| l.pill_key(t, now));
        let mut rows: Option<(i32, i32)> = None;
        let mut grow = |a: i32, b: i32| {
            rows = Some(rows.map_or((a, b), |(x, y)| (x.min(a), y.max(b))));
        };
        let mut full = self.dirty || self.shown.len() != self.w as usize * self.h as usize * 4;
        if !full && std::mem::take(&mut self.bands_dirty) {
            if let Some(l) = &self.list {
                let bands = l.row_bands(t);
                let same_rows = bands.len() == self.bands_shown.len()
                    && bands
                        .iter()
                        .zip(&self.bands_shown)
                        .all(|(a, b)| (a.0, a.1) == (b.0, b.1));
                if !same_rows || l.button_look() != self.button_shown {
                    full = true;
                } else {
                    for (a, b) in bands.iter().zip(&self.bands_shown) {
                        if a.2 != b.2 {
                            grow(a.0, a.1);
                        }
                    }
                    self.bands_shown = bands;
                }
            }
        }
        if full {
            self.shown = self.paint(t, fonts, now).to_premul_rgba8();
            self.dirty = false;
            self.bands_dirty = false;
            self.pill_shown = key;
            if let Some(l) = &self.list {
                self.bands_shown = l.row_bands(t);
                self.button_shown = l.button_look();
            }
            return Some((&self.shown, None));
        }
        if key != self.pill_shown {
            self.pill_shown = key;
            if let Some((_, y, _, h)) = self.list.as_ref().and_then(|l| l.pill_rect(t, fonts)) {
                grow(y, y + h);
            }
        }
        let (y0, y1) = rows?;
        let (y0, y1) = (y0.clamp(0, self.h as i32), y1.clamp(0, self.h as i32));
        if y1 <= y0 {
            return None;
        }
        // Nur diese Zeilen zeichnen: Pixel für Pixel wie im ganzen Bild
        let band = self
            .paint_rows(t, fonts, now, y0 as u32, y1 as u32)
            .to_premul_rgba8();
        let at = y0 as usize * self.w as usize * 4;
        self.shown[at..at + band.len()].copy_from_slice(&band);
        Some((&self.shown, Some((y0 as u32, y1 as u32))))
    }

    /// Ganzes Fensterbild: Blatt, Fuge zum Hauptfenster, Titelleiste.
    pub fn paint(&self, t: &Theme, fonts: &Fonts, now: Instant) -> Canvas {
        self.paint_rows(t, fonts, now, 0, self.h)
    }

    /// Ausschnitt des Fensterbilds, Zeilen `y0..y1`.
    fn paint_rows(&self, t: &Theme, fonts: &Fonts, now: Instant, y0: u32, y1: u32) -> Canvas {
        let (w, h) = (self.w as usize, y1.saturating_sub(y0) as usize);
        let mut c = Canvas::new(w, h);
        c.clear(t.ui.sheet_bg);
        c.set_origin(0.0, y0 as f32);
        if let Some(l) = &self.list {
            l.paint(&mut c, t, fonts, now);
        }
        // Titelleiste nur, wenn der Ausschnitt sie berührt
        if y0 < self.title.height() {
            c.set_origin(0.0, 0.0);
            let bar = self.title.paint(t, fonts.regular.as_ref(), self.w);
            c.blit(&bar, 0, -(y0 as i32));
            c.set_origin(0.0, y0 as f32);
        }
        if self.docked {
            let s = self.title.scale;
            let col = match self.seam_flash {
                Some(at) if now.duration_since(at) < SEAM_FLASH => t.ui.accent,
                _ => t.title.bg,
            };
            c.fill_rect(0.0, 0.0, (SEAM * s).max(1.0), self.h as f32, col);
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

    /// Review 1h (U5): Hover aus dem Hauptfenster zeichnet nur die Zeilen,
    /// deren Band sich ändert, und das Ergebnis gleicht dem ganzen Bild.
    #[test]
    fn hover_zeichnet_nur_die_betroffenen_zeilen() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let mut s = Scene::with_model(Model::with_seed(1));
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let eg = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(eg);
        let run = s
            .add_wall(&WallChain {
                base: 0.0,
                points: vec![vec3(0.0, 0.0, 0.0), vec3(5000.0, 0.0, 0.0)],
                closed: false,
                ref_side: RefSide::Left,
                layers: Vec::new(),
                height: 3500.0,
                joints: Default::default(),
            })
            .unwrap();
        let wall = s.model().wall_at(run, 0).unwrap();
        let mut p = Picking::default();
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (520, 1000);
        let now = Instant::now();
        q.sync(&mut s, &p, false);
        assert!(matches!(q.frame(&t, &fonts, now), Some((_, None))));

        for (one, what) in [(Some(wall), "Hover an"), (None, "Hover aus")] {
            assert!(p.set_hover(one, Vec::new()));
            q.sync(&mut s, &p, false);
            let (_, rows) = q.frame(&t, &fonts, now).expect(what);
            let (y0, y1) = rows.expect("nur Zeilen, nicht das ganze Bild");
            assert!(y1 > y0 && y1 - y0 < 60, "{what}: eine Zeile, {y0}..{y1}");
            let whole = q.paint(&t, &fonts, now).to_premul_rgba8();
            assert!(q.shown == whole, "{what}: gleich dem ganzen Bild");
            assert!(
                q.frame(&t, &fonts, now).is_none(),
                "{what}: danach nichts mehr"
            );
        }

        // Auswahl: ebenfalls nur Zeilen; auf- oder zugeklappt wird nichts
        p.selected = vec![wall];
        q.sync(&mut s, &p, false);
        let f = q.frame(&t, &fonts, now).map(|(_, r)| r);
        assert!(f.is_some());
        let whole = q.paint(&t, &fonts, now).to_premul_rgba8();
        assert!(q.shown == whole, "Auswahl: gleich dem ganzen Bild");
    }

    /// Review 1i: Ein Streifen lässt Zeilen, Kopf und Titelleiste außerhalb
    /// weg und gleicht trotzdem dem ganzen Bild, auch bei 150 % und mit
    /// Hover und Auswahl (mit Schrift nur unter Windows).
    #[test]
    fn jeder_streifen_gleicht_dem_ganzen_bild() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let mut s = Scene::with_model(Model::with_seed(1));
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let eg = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(eg);
        let mut walls = Vec::new();
        for y in [0.0, 3000.0, 6000.0] {
            let run = s
                .add_wall(&WallChain {
                    base: 0.0,
                    points: vec![vec3(0.0, y, 0.0), vec3(5000.0, y, 0.0)],
                    closed: false,
                    ref_side: RefSide::Left,
                    layers: Vec::new(),
                    height: 3500.0,
                    joints: Default::default(),
                })
                .unwrap();
            walls.push(s.model().wall_at(run, 0).unwrap());
        }
        let mut p = Picking {
            selected: vec![walls[0]],
            ..Default::default()
        };
        p.set_hover(Some(walls[2]), Vec::new());
        let mut q = QuantityWindow::new();
        q.title.scale = 1.5;
        (q.w, q.h) = (780, 600);
        let now = Instant::now();
        q.sync(&mut s, &p, false);
        let whole = q.paint(&t, &fonts, now).to_premul_rgba8();
        let row = q.w as usize * 4;
        for y0 in (0..q.h - 9).step_by(13) {
            let y1 = (y0 + 9 + y0 % 31).min(q.h);
            let part = q.paint_rows(&t, &fonts, now, y0, y1).to_premul_rgba8();
            // Verschobene Pfade runden in Gleitkomma minimal anders: höchstens
            // eine Stufe (von 255) Unterschied, unsichtbar
            let w = &whole[y0 as usize * row..y1 as usize * row];
            let worst = part.iter().zip(w).map(|(a, b)| a.abs_diff(*b)).max();
            assert!(
                part.len() == w.len() && worst <= Some(1),
                "Streifen {y0}..{y1} weicht ab: {worst:?}"
            );
        }
    }
}

//! Mengenfenster (F2, B7): eigenes Programmfenster mit derselben Titelleiste
//! wie das Hauptfenster; darunter das Blatt der Mengenermittlung
//! ([`ListView`]). Hover und Auswahl laufen über den gemeinsamen Zustand.
//! Entf, Kontextmenü und Hinweis nach dem Löschen wie im Hauptfenster (H119).

use crate::delete::{Action, ContextMenu, HintCard, Link};
use crate::picking::Picking;
use crate::scene::Scene;
use crate::schedule_view::{ListOut, ListView, RowBand};
use sk_model::{Deleted, ElementId};
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
    /// Entf: die gemeinsame Auswahl löschen.
    Delete,
    /// Rechtsklick (Fensterpixel): Menü an der Zeile öffnen.
    OpenContext {
        x: f64,
        y: f64,
    },
    /// Befehl aus dem Menü: Bauteil des Menüs, Bauteile der Zeile.
    Action(Action, ElementId, Vec<ElementId>),
    /// Verweis im Hinweis.
    Link(Link),
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
    /// Im letzten Bild leuchtete etwas auf (dann noch ein Schlussbild).
    was_busy: bool,
    /// Zuletzt gezeigtes Fensterbild (vormultipliziert) und was die Pille
    /// „wird aktualisiert“ darin zeigt: Beim Ziehen im Modell wird nur die
    /// Pille neu gezeichnet, nicht das ganze Fenster (Review 1g).
    shown: Vec<u8>,
    /// Leinwand der Bilder, behält ihren Speicher ([`Canvas::reuse`]).
    canvas: Canvas,
    pill_shown: Option<(u8, u8)>,
    /// Hover oder Auswahl haben sich geändert: nur die Zeilen neu zeichnen,
    /// deren Band anders aussieht (Review 1h, U5). Dazu Bänder und Knopf,
    /// wie sie im gezeigten Bild stehen.
    bands_dirty: bool,
    bands_shown: Vec<RowBand>,
    button_shown: (bool, bool),
    /// Rollstand des gezeigten Bildes (px): Rollen verschiebt das Bild und
    /// zeichnet nur die frei werdenden Zeilen (U6b).
    scroll_shown: i32,
    /// Hinweis nach Entf unter der Zeile (H119).
    pub hint: Option<HintCard>,
    /// Kontextmenü an einer Zeile und die Bauteile der Zeile.
    pub context: Option<(ContextMenu, Vec<ElementId>)>,
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
            canvas: Canvas::new(0, 0),
            pill_shown: None,
            bands_dirty: false,
            bands_shown: Vec::new(),
            button_shown: (false, false),
            scroll_shown: 0,
            hint: None,
            context: None,
        }
    }

    /// Ein- und Ausblenden des Hinweises (ms).
    fn fade_ms(t: &Theme) -> f32 {
        if t.size.anim_ms > 0.0 {
            t.size.fade_ms
        } else {
            0.0
        }
    }

    /// Hinweis unter der Zeile zeigen (leer: keiner).
    pub fn show_hint(
        &mut self,
        lines: Vec<String>,
        link: Option<(&'static str, Link)>,
        now: Instant,
    ) {
        self.hint = (!lines.is_empty()).then(|| HintCard::new(lines, link, Vec::new(), now));
        self.dirty = true;
    }

    /// Nach dem Löschen aus der Liste (Entf oder Menü): Hinweis, Aufleuchten
    /// der abgelehnten Zeilen, bereinigte Auswahl.
    pub fn erased(&mut self, s: &Scene, d: &Deleted, p: &mut Picking, now: Instant) {
        let Some(l) = self.list.as_mut() else { return };
        let (lines, link) = l.erased(s, d, p, now);
        self.show_hint(lines, link, now);
    }

    /// Entf löscht hier etwas (sonst „Hier ist kein Bauteil gewählt.“).
    pub fn part_selected(&self, p: &Picking) -> bool {
        self.list.as_ref().is_some_and(|l| l.part_selected(p))
    }

    /// Rechtsklick auf eine Zeile: wählt sie und öffnet das Menü. `false`
    /// auf Geschoss- und Summenzeilen.
    #[allow(clippy::too_many_arguments)]
    pub fn open_context(
        &mut self,
        s: &Scene,
        p: &mut Picking,
        x: f64,
        y: f64,
        t: &Theme,
        fonts: &Fonts,
    ) -> bool {
        let Some(l) = self.list.as_mut() else {
            return false;
        };
        let Some((target, ids)) = l.context_at(t, fonts, p, x, y) else {
            return false;
        };
        let menu = ContextMenu::for_list(
            s.model(),
            target,
            &p.selected,
            x,
            y,
            (self.w, self.h, self.title.height()),
            t,
            self.title.scale,
        );
        self.context = Some((menu, ids));
        self.hint = None;
        self.bands_dirty = true;
        self.dirty = true;
        true
    }

    /// Wann sich der Hinweis wieder ändert (für die Ereignisschleife).
    pub fn wait(&self, t: &Theme, now: Instant) -> Option<Duration> {
        self.hint.as_ref().map(|h| h.wait(now, Self::fade_ms(t)))
    }

    /// Menü und Hinweis schließen.
    pub fn close_popups(&mut self) {
        if self.context.take().is_some() || self.hint.take().is_some() {
            self.dirty = true;
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
        // Rollen braucht weitere Bilder, aber kein ganzes: `frame` verschiebt
        let scrolling = self.list.as_mut().is_some_and(|l| l.tick(t, now));
        let mut flashing = self.list.as_ref().is_some_and(|l| l.flashing());
        if let Some(at) = self.seam_flash {
            if now.duration_since(at) < SEAM_FLASH && t.size.anim_ms > 0.0 {
                flashing = true;
            } else {
                self.seam_flash = None;
                self.dirty = true;
            }
        }
        if let Some(h) = &self.hint {
            match h.alpha(now, Self::fade_ms(t)) {
                None => {
                    self.hint = None;
                    self.dirty = true;
                }
                Some(a) if a < 1.0 => flashing = true,
                Some(_) => {}
            }
        }
        if flashing || self.was_busy {
            self.dirty = true;
        }
        self.was_busy = flashing;
        scrolling || flashing || pill
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
        if let Some(o) = self.handle_popups(e, t) {
            return o;
        }
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
                // Kein ganzes Bild: `frame` verschiebt um den neuen Rollstand
                let anim = t.size.anim_ms > 0.0;
                self.list.as_mut()?.wheel(delta, t, anim);
                None
            }
            Event::MouseDown {
                button: MouseButton::Right,
                x,
                y,
                ..
            } if y >= self.title.height() as f64 => Some(Out::OpenContext { x, y }),
            Event::Key {
                key: Key::Delete,
                down: true,
                ..
            } => Some(Out::Delete),
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

    /// Menü und Hinweis nehmen Ereignisse zuerst. `Some(out)`: verbraucht.
    fn handle_popups(&mut self, e: &Event, t: &Theme) -> Option<Option<Out>> {
        let s = self.title.scale;
        if let Some((c, ids)) = self.context.as_mut() {
            match *e {
                Event::MouseMove { x, y, .. } => {
                    if c.mouse_move(t, s, x, y) {
                        self.dirty = true;
                    }
                    return Some(None);
                }
                Event::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    if !c.press(t, s, x, y) {
                        self.context = None;
                        self.dirty = true;
                    }
                    return Some(None);
                }
                Event::MouseUp {
                    button: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    let a = c.release(t, s, x, y);
                    let out = a.map(|a| Out::Action(a, c.target, std::mem::take(ids)));
                    if out.is_some() {
                        self.context = None;
                        self.dirty = true;
                    }
                    return Some(out);
                }
                Event::MouseDown {
                    button: MouseButton::Right,
                    ..
                } => {
                    // Schließt und öffnet an der neuen Stelle
                    self.context = None;
                    self.dirty = true;
                }
                Event::Key {
                    key: Key::Delete,
                    down: true,
                    ..
                } => {
                    self.context = None;
                    self.dirty = true;
                    return Some(Some(Out::Delete));
                }
                Event::Key {
                    key, down: true, ..
                } => {
                    self.dirty = true;
                    return Some(match c.key(key) {
                        Err(()) => {
                            self.context = None;
                            None
                        }
                        Ok(None) => None,
                        Ok(Some(a)) => {
                            let out = Out::Action(a, c.target, std::mem::take(ids));
                            self.context = None;
                            Some(out)
                        }
                    });
                }
                Event::MouseLeave | Event::Focus(false) => {}
                _ => {}
            }
        }
        if let Some(h) = self.hint.as_mut() {
            match *e {
                Event::MouseMove { x, y, .. } => {
                    if h.mouse_move(x, y, s, t, Instant::now()) {
                        self.dirty = true;
                    }
                }
                Event::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                    ..
                } => match h.click(x, y, s, t) {
                    None => {}
                    Some(None) => return Some(None),
                    Some(Some(l)) => {
                        self.hint = None;
                        self.dirty = true;
                        return Some(Some(Out::Link(l)));
                    }
                },
                _ => {}
            }
        }
        None
    }

    /// Neues Fensterbild, falls nötig: das ganze Bild oder nur die Zeilen
    /// (von, bis), die sich geändert haben. Hover und Auswahl zeichnen die
    /// Zeilen mit anderem Band, die Pille ihre Zeilen, Rollen verschiebt die
    /// Liste und zeichnet die frei werdenden Zeilen und die Laufleiste.
    pub fn frame(&mut self, t: &Theme, fonts: &Fonts, now: Instant) -> Option<Frame<'_>> {
        if self.w == 0 || self.h == 0 {
            return None;
        }
        let (w, h) = (self.w as i32, self.h as i32);
        self.place_hint(t, fonts);
        let key = self.list.as_ref().and_then(|l| l.pill_key(t, now));
        // Neu zu zeichnende Zeilenbereiche und was davon gezeigt werden muss
        let mut paint: Vec<(i32, i32)> = Vec::new();
        let mut shown: Option<(i32, i32)> = None;
        let mut show = |a: i32, b: i32| {
            shown = Some(shown.map_or((a, b), |(x, y)| (x.min(a), y.max(b))));
        };
        let mut full = self.dirty || self.shown.len() != (w * h * 4) as usize;
        let scroll = self.list.as_ref().map_or(0, |l| l.scroll_px());
        let scrolled = scroll != self.scroll_shown;
        let bands_dirty = std::mem::take(&mut self.bands_dirty);
        if !full {
            if let Some(l) = &self.list {
                if scrolled && (bands_dirty || l.scrollbar_x(t).is_none()) {
                    full = true;
                } else if bands_dirty {
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
                                paint.push((a.0, a.1));
                                show(a.0, a.1);
                            }
                        }
                        self.bands_shown = bands;
                    }
                }
            }
        }
        if !full && scrolled {
            let l = self.list.as_ref()?;
            let (top, d) = (l.list_y().clamp(0, h), scroll - self.scroll_shown);
            if d.abs() >= h - top {
                full = true;
            } else {
                // Liste verschieben, frei werdende Zeilen neu zeichnen
                let row = (w * 4) as usize;
                let (from, to, fresh) = if d > 0 {
                    (top + d, top, (h - d, h))
                } else {
                    (top, top - d, (top, top - d))
                };
                let n = (h - top - d.abs()) as usize * row;
                let src = from as usize * row;
                self.shown.copy_within(src..src + n, to as usize * row);
                paint.push(fresh);
                show(top, h);
                // Laufleiste: eigene Spalte rechts, über die ganze Liste
                if let Some(x) = l.scrollbar_x(t) {
                    let cw = (w - x).max(1) as usize;
                    let mut c = Canvas::new(cw, (h - top) as usize);
                    c.clear(t.ui.sheet_bg);
                    c.set_origin(x as f32, top as f32);
                    l.paint_scrollbar(&mut c, t);
                    let px = c.to_premul_rgba8();
                    for (k, line) in px.chunks_exact(cw * 4).enumerate() {
                        let at = (top as usize + k) * row + x as usize * 4;
                        self.shown[at..at + cw * 4].copy_from_slice(line);
                    }
                }
                self.scroll_shown = scroll;
                self.bands_shown = l.row_bands(t);
            }
        }
        if full {
            let mut c = std::mem::replace(&mut self.canvas, Canvas::new(0, 0));
            self.paint_rows_into(&mut c, t, fonts, now, 0, self.h);
            c.premul_rgba8_into(&mut self.shown);
            self.canvas = c;
            self.dirty = false;
            self.pill_shown = key;
            self.scroll_shown = scroll;
            if let Some(l) = &self.list {
                self.bands_shown = l.row_bands(t);
                self.button_shown = l.button_look();
            }
            return Some((&self.shown, None));
        }
        if key != self.pill_shown {
            self.pill_shown = key;
            if let Some((_, y, _, ph)) = self.list.as_ref().and_then(|l| l.pill_rect(t, fonts)) {
                paint.push((y, y + ph));
                show(y, y + ph);
            }
        }
        // Nur diese Zeilen zeichnen: wie im ganzen Bild (Schrift höchstens
        // eine Stufe anders gerundet)
        for (y0, y1) in paint {
            let (y0, y1) = (y0.clamp(0, h), y1.clamp(0, h));
            if y1 <= y0 {
                continue;
            }
            let mut c = std::mem::replace(&mut self.canvas, Canvas::new(0, 0));
            self.paint_rows_into(&mut c, t, fonts, now, y0 as u32, y1 as u32);
            let band = c.to_premul_rgba8();
            self.canvas = c;
            let at = (y0 * w * 4) as usize;
            self.shown[at..at + band.len()].copy_from_slice(&band);
        }
        let (y0, y1) = shown?;
        let (y0, y1) = (y0.clamp(0, h), y1.clamp(0, h));
        (y1 > y0).then_some((&self.shown[..], Some((y0 as u32, y1 as u32))))
    }

    /// Hinweis unter seine Zeile legen (folgt dem Rollen).
    fn place_hint(&mut self, t: &Theme, fonts: &Fonts) {
        let (Some(h), Some(l)) = (self.hint.as_mut(), self.list.as_ref()) else {
            return;
        };
        let s = self.title.scale;
        let size = h.size(t, fonts, s);
        let before = h.rect;
        let top = l.list_y() as f32;
        h.place(size, l.hint_rect(t), (self.w as f32, self.h as f32, top), s);
        if h.rect != before {
            self.dirty = true;
        }
    }

    /// Ganzes Fensterbild: Blatt, Fuge zum Hauptfenster, Titelleiste.
    #[cfg(test)]
    pub fn paint(&self, t: &Theme, fonts: &Fonts, now: Instant) -> Canvas {
        self.paint_rows(t, fonts, now, 0, self.h)
    }

    /// Ausschnitt des Fensterbilds, Zeilen `y0..y1`.
    #[cfg(test)]
    fn paint_rows(&self, t: &Theme, fonts: &Fonts, now: Instant, y0: u32, y1: u32) -> Canvas {
        let mut c = Canvas::new(0, 0);
        self.paint_rows_into(&mut c, t, fonts, now, y0, y1);
        c
    }

    /// Wie [`QuantityWindow::paint_rows`] auf eine vorhandene Leinwand.
    fn paint_rows_into(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        now: Instant,
        y0: u32,
        y1: u32,
    ) {
        let (w, h) = (self.w as usize, y1.saturating_sub(y0) as usize);
        c.reuse(w, h);
        c.clear(t.ui.sheet_bg);
        c.set_origin(0.0, y0 as f32);
        if let Some(l) = &self.list {
            l.paint(c, t, fonts, now);
        }
        let s = self.title.scale;
        if let Some(hc) = &self.hint {
            if let (Some(r), Some(a)) = (hc.rect, hc.alpha(now, Self::fade_ms(t))) {
                let img = hc.paint(t, fonts, s);
                let m = (t.size.panel_shadow * s).round();
                c.blit_scaled(&img, r.x - m, r.y - m, 1.0, a);
            }
        }
        if let Some((menu, _)) = &self.context {
            let (img, x, y) = menu.paint(t, fonts, s);
            c.blit(&img, x, y);
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{Model, RefSide, WallChain};

    /// Gleich bis auf eine Stufe (von 255): verschobene Schriftpfade runden
    /// in Gleitkomma minimal anders, unsichtbar.
    fn fast_gleich(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= 1)
    }

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
            assert!(
                fast_gleich(&q.shown, &whole),
                "{what}: gleich dem ganzen Bild"
            );
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
        assert!(
            fast_gleich(&q.shown, &whole),
            "Auswahl: gleich dem ganzen Bild"
        );
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

    /// U6b: Rollen verschiebt das gezeigte Bild und zeichnet nur die frei
    /// werdenden Zeilen und die Laufleiste; das Ergebnis gleicht dem ganzen
    /// Bild, nach unten wie nach oben, auch bei 150 %.
    #[test]
    fn rollen_verschiebt_das_bild() {
        let fonts = Fonts::system();
        let mut t = Theme::dark();
        t.size.anim_ms = 0.0;
        let mut s = Scene::with_model(Model::with_seed(1));
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let eg = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(eg);
        for (k, y) in [0.0, 3000.0, 6000.0, 9000.0].into_iter().enumerate() {
            let cat = if k % 2 == 0 {
                sk_model::Category::ExteriorWall
            } else {
                sk_model::Category::InteriorWall
            };
            s.add_wall_as(
                &WallChain {
                    base: 0.0,
                    points: vec![vec3(0.0, y, 0.0), vec3(5000.0, y, 0.0)],
                    closed: false,
                    ref_side: RefSide::Left,
                    layers: Vec::new(),
                    height: 3500.0,
                    joints: Default::default(),
                },
                cat,
            )
            .unwrap();
        }
        for scale in [1.0f32, 1.5] {
            let p = Picking::default();
            let mut q = QuantityWindow::new();
            q.title.scale = scale;
            (q.w, q.h) = ((520.0 * scale) as u32, (220.0 * scale) as u32);
            let now = Instant::now();
            q.sync(&mut s, &p, false);
            assert!(matches!(q.frame(&t, &fonts, now), Some((_, None))));
            let mut p = p;
            let mut shifted = 0;
            // Bruchteile einer Raste: der Rollstand liegt auch zwischen Pixeln
            for delta in [-0.25, -0.13, 0.2, -0.4, 0.31, -0.07] {
                let before = q.list.as_ref().unwrap().scroll_px();
                q.handle(
                    &Event::Wheel {
                        delta,
                        x: 100.0,
                        y: 200.0,
                        mods: Default::default(),
                    },
                    &t,
                    &fonts,
                    &mut p,
                );
                q.tick(&t, now);
                let after = q.list.as_ref().unwrap().scroll_px();
                let f = q.frame(&t, &fonts, now).map(|(_, r)| r);
                if after == before {
                    assert_eq!(f, None, "nichts gerollt");
                    continue;
                }
                let list_y = q.list.as_ref().unwrap().list_y() as u32;
                assert_eq!(f, Some(Some((list_y, q.h))), "{scale}: nur die Liste");
                shifted += 1;
                let whole = q.paint(&t, &fonts, now).to_premul_rgba8();
                assert!(
                    fast_gleich(&q.shown, &whole),
                    "{scale}: Rollen {before} → {after} gleicht dem ganzen Bild"
                );
            }
            assert!(shifted >= 4, "{scale}: {shifted} Mal verschoben");
        }
    }

    /// Gebäude mit einer Außenwand und drei Innenwänden im EG.
    fn haus_h119() -> (Scene, Vec<ElementId>) {
        let mut s = Scene::with_model(Model::with_seed(1));
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let eg = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(eg);
        let mut walls = Vec::new();
        for (k, y) in [0.0, 3000.0, 6000.0, 9000.0].into_iter().enumerate() {
            // Zuerst der geschlossene Umriss (Außenwände: abgelehnt)
            let (cat, points, closed) = if k == 0 {
                let r = [
                    (0.0, 0.0),
                    (10000.0, 0.0),
                    (10000.0, 12000.0),
                    (0.0, 12000.0),
                ];
                let pts = r.iter().map(|&(x, y)| vec3(x, y, 0.0)).collect();
                (sk_model::Category::ExteriorWall, pts, true)
            } else {
                let pts = vec![vec3(0.0, y, 0.0), vec3(10000.0, y, 0.0)];
                (sk_model::Category::InteriorWall, pts, false)
            };
            let run = s
                .add_wall_as(
                    &WallChain {
                        base: 0.0,
                        points,
                        closed,
                        ref_side: RefSide::Left,
                        layers: Vec::new(),
                        height: 3500.0,
                        joints: Default::default(),
                    },
                    cat,
                )
                .unwrap();
            walls.push(s.model().wall_at(run, 0).unwrap());
        }
        (s, walls)
    }

    fn taste(k: Key) -> Event {
        Event::Key {
            key: k,
            down: true,
            repeat: false,
            mods: Default::default(),
        }
    }

    /// Band der gewählten Zeile (Fensterpixel von, bis).
    fn gewaehlte_zeile(q: &QuantityWindow, t: &Theme) -> (i32, i32) {
        let bands = q.list.as_ref().unwrap().row_bands(t);
        let b = bands
            .iter()
            .find(|b| b.2 == Some((t.ui.sheet_select, true)))
            .expect("gewählte Zeile sichtbar");
        (b.0, b.1)
    }

    /// H119: Entf im Mengenfenster meldet sich bei der App; gelöschte Zeilen
    /// blenden aus, die übrigen rücken nach, danach ist Ruhe. Ohne Hinweis,
    /// wenn alles gelöscht wurde.
    #[test]
    fn entf_blendet_aus_und_rueckt_nach() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let (mut s, walls) = haus_h119();
        let mut p = Picking {
            selected: vec![walls[1]],
            ..Default::default()
        };
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (520, 800);
        let now = Instant::now();
        q.sync(&mut s, &p, true);
        q.frame(&t, &fonts, now);
        assert_eq!(
            q.handle(&taste(Key::Delete), &t, &fonts, &mut p),
            Some(Out::Delete)
        );
        assert!(q.part_selected(&p));
        let d = s.delete_elements(&p.selected.clone());
        assert_eq!(d.removed.len(), 1);
        q.erased(&s, &d, &mut p, now);
        assert!(p.selected.is_empty(), "Auswahl bereinigt");
        assert!(q.hint.is_none(), "alles gelöscht: kein Hinweis");
        q.sync(&mut s, &p, true);
        let l = q.list.as_ref().unwrap();
        assert!(l.flashing(), "Zeilen rücken nach");
        let fade = Duration::from_millis(t.size.fade_ms as u64);
        let anim = Duration::from_millis(t.size.anim_ms as u64);
        assert!(q.tick(&t, now + fade / 2), "blendet aus");
        let half = q.paint(&t, &fonts, now + fade / 2).to_premul_rgba8();
        let moved = q.paint(&t, &fonts, now + fade + anim / 2).to_premul_rgba8();
        assert_ne!(half, moved, "erst ausblenden, dann nachrücken");
        let end = now + fade + anim + Duration::from_millis(t.size.flash_ms as u64 + 50);
        q.tick(&t, end);
        assert!(!q.tick(&t, end), "danach Ruhe");
        assert!(!q.list.as_ref().unwrap().flashing());

        // Ohne Übergänge: sofort fertig
        let mut t0 = Theme::dark();
        t0.size.anim_ms = 0.0;
        p.selected = vec![walls[2]];
        q.sync(&mut s, &p, false);
        let d = s.delete_elements(&p.selected.clone());
        q.erased(&s, &d, &mut p, now);
        q.sync(&mut s, &p, true);
        assert!(!q.tick(&t0, now), "anim_ms = 0: nichts läuft");
    }

    /// H119: Abgelehnt (Außenwand) leuchtet die Zeile, der Hinweis steht
    /// darunter. Nach einem Klick auf die Geschosszeile löscht Entf nichts.
    #[test]
    fn ablehnung_hinweis_unter_der_zeile() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let (mut s, walls) = haus_h119();
        let mut p = Picking {
            selected: vec![walls[0]],
            ..Default::default()
        };
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (520, 800);
        let now = Instant::now();
        q.sync(&mut s, &p, true);
        q.frame(&t, &fonts, now);
        let (_, row_bottom) = gewaehlte_zeile(&q, &t);
        let rev = s.model().revision();
        let d = s.delete_elements(&p.selected.clone());
        assert!(d.removed.is_empty());
        q.erased(&s, &d, &mut p, now);
        assert_eq!(s.model().revision(), rev);
        let h = q.hint.as_ref().expect("Hinweis");
        assert_eq!(h.lines[0], "Außenwände gehören zum Gebäudeumriss.");
        assert!(q.list.as_ref().unwrap().flashing(), "Zeile leuchtet");
        q.sync(&mut s, &p, true);
        q.tick(&t, now + Duration::from_millis(10));
        q.frame(&t, &fonts, now + Duration::from_millis(10));
        let r = q.hint.as_ref().unwrap().rect.expect("gelegt");
        assert!(
            r.y >= row_bottom as f32,
            "unter der Zeile: {} < {row_bottom}",
            r.y
        );
        assert!(q.wait(&t, now).is_some());

        // Klick auf die Geschosszeile: kein Bauteil
        let bands = q.list.as_ref().unwrap().row_bands(&t);
        let (y0, y1, _) = bands[0];
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x: 80.0,
            y: (y0 + y1) as f64 * 0.5,
            mods: Default::default(),
        };
        q.handle(&down, &t, &fonts, &mut p);
        assert!(!q.part_selected(&p), "Geschosszeile angeklickt");
        p.selected = vec![walls[1]];
        q.list.as_mut().unwrap().follow(&mut s, &p);
        assert!(q.part_selected(&p), "Auswahl von außen: wieder ein Bauteil");
        // Summe nach Baustoff (letzte Zeile): ebenfalls kein Bauteil
        let bands = q.list.as_ref().unwrap().row_bands(&t);
        let (y0, y1, _) = *bands.last().unwrap();
        let sum = Event::MouseDown {
            button: MouseButton::Left,
            x: 80.0,
            y: (y0 + y1) as f64 * 0.5,
            mods: Default::default(),
        };
        q.handle(&sum, &t, &fonts, &mut p);
        assert!(!q.part_selected(&p), "Summenzeile angeklickt");
    }

    /// H119: Rechtsklick auf eine Zeile öffnet das Menü des Modells mit „Im
    /// Modell zeigen“ oben und ohne „Eigenschaften“; Enter führt aus.
    #[test]
    fn menue_an_der_zeile() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let (mut s, walls) = haus_h119();
        let mut p = Picking {
            selected: vec![walls[1]],
            ..Default::default()
        };
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (520, 800);
        let now = Instant::now();
        q.sync(&mut s, &p, false);
        q.frame(&t, &fonts, now);
        let (y0, y1) = gewaehlte_zeile(&q, &t);
        let (x, y) = (120.0, (y0 + y1) as f64 * 0.5);
        let right = Event::MouseDown {
            button: MouseButton::Right,
            x,
            y,
            mods: Default::default(),
        };
        assert_eq!(
            q.handle(&right, &t, &fonts, &mut p),
            Some(Out::OpenContext { x, y })
        );
        assert!(q.open_context(&s, &mut p, x, y, &t, &fonts));
        let labels: Vec<String> = q
            .context
            .as_ref()
            .unwrap()
            .0
            .actions()
            .into_iter()
            .map(|a| a.0)
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(labels[0], "Im Modell zeigen");
        assert!(labels.contains(&"Löschen".to_string()));
        assert!(!labels.contains(&"Eigenschaften".to_string()));
        assert!(matches!(q.frame(&t, &fonts, now), Some((_, None))));
        q.handle(&taste(Key::Other(0x28)), &t, &fonts, &mut p);
        let out = q.handle(&taste(Key::Enter), &t, &fonts, &mut p);
        assert_eq!(
            out,
            Some(Out::Action(Action::ShowInModel, walls[1], vec![walls[1]]))
        );
        assert!(q.context.is_none());
        // Auf der Geschosszeile kein Menü
        let bands = q.list.as_ref().unwrap().row_bands(&t);
        let gy = (bands[0].0 + bands[0].1) as f64 * 0.5;
        assert!(!q.open_context(&s, &mut p, 80.0, gy, &t, &fonts));
    }
}

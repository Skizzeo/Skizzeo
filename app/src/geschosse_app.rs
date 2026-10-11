//! Paneel „Geschosse“ in der App: das Abdunkeln hinter dem Dialog „Gebäude
//! erstellen“ (E16) und die schwebende Großansicht (Jörn 11.10., Lage und
//! Treffer in `ui::gross`). Beide liegen auf denselben Plätzen über
//! „Werkzeuge“ und „Ansichten“; sie sind nie zugleich offen.

use crate::ui::{Id, Panel};
use crate::{App, OVERLAY_DIALOG, OVERLAY_SCRIM};
use sk_paint::Rgba;
use sk_platform::{Event, Key, Surface};

/// Vormultipliziert für `set_overlay_fill`.
fn premul(k: Rgba) -> [u8; 4] {
    let a = k.3 as u32;
    let pm = |v: u8| ((v as u32 * a + 127) / 255) as u8;
    [pm(k.0), pm(k.1), pm(k.2), k.3]
}

impl App {
    /// Dialog „Gebäude erstellen“ samt Abdunkeln des Modellfensters, sonst
    /// die Leiste der Großansicht, sonst nichts.
    pub(crate) fn paint_dialog(&mut self) {
        let th = self.title.height();
        let h = self.h.saturating_sub(th);
        if self.ui.dialog {
            let (c, x, y) = self.ui.paint(&self.theme, Panel::Dialog, self.w, th);
            c.premul_rgba8_into(&mut self.panel_px);
            let (w, ch) = (c.width as u32, c.height as u32);
            self.renderer
                .set_overlay(OVERLAY_DIALOG, x, y, w, ch, &self.panel_px);
            let scrim = premul(self.theme.env.scrim);
            self.renderer
                .set_overlay_fill(OVERLAY_SCRIM, 0, th as i32, self.w, h, scrim);
        } else if self.ui.levels_gross() {
            let c = self.ui.paint_gross_leiste(self.w);
            let (w, ch) = (c.width as u32, c.height as u32);
            let px = c.to_premul_rgba8();
            self.renderer
                .set_overlay(OVERLAY_DIALOG, 0, th as i32, w, ch, &px);
            // Ohne Abdunkeln: das Modell zeigt jede Änderung sofort (Jörn 11.10.)
            self.renderer.set_overlay(OVERLAY_SCRIM, 0, 0, 0, 0, &[]);
        } else {
            self.renderer.set_overlay(OVERLAY_DIALOG, 0, 0, 0, 0, &[]);
            self.renderer.set_overlay(OVERLAY_SCRIM, 0, 0, 0, 0, &[]);
        }
    }

    /// Klick auf das kleine Paneel öffnet die Großansicht, „Schließen ×“
    /// oder ein Klick daneben schließt sie.
    pub(crate) fn gross_klick(&mut self, id: Id) {
        if self.ui.set_levels_gross(id == Id::LevelsOpen) {
            self.overlay_dirty = true;
            self.refresh_cursor();
        }
    }

    /// Ereignis bei offener Großansicht: Maus und Rad unter der Titelleiste
    /// gehören ihr, Esc schließt (eine Eingabe bricht Esc zuerst ab, das
    /// Ziehen einer Ebene ist vorher dran). Übrige Tasten wie Rückgängig
    /// gelten weiter. `true`, wenn das Ereignis damit erledigt ist.
    pub(crate) fn gross_event(&mut self, e: &Event, surface: &Surface) -> bool {
        if !self.ui.levels_gross() {
            return false;
        }
        let th = self.top() as f64;
        match *e {
            Event::MouseMove { y, .. } | Event::MouseDown { y, .. } | Event::MouseUp { y, .. }
                if y >= th =>
            {
                let out = self.ui.handle(e, self.w, self.top());
                self.apply_ui(&out);
                if let Some(id) = out.clicked {
                    self.click(id);
                }
                self.sync_levels();
                self.sync_props();
                self.sync_caption(surface);
                true
            }
            Event::Wheel { .. } => true,
            Event::Key {
                key: Key::Escape,
                down,
                ..
            } if self.ui.edit.is_none() => {
                if down {
                    self.gross_klick(Id::LevelsClose);
                }
                true
            }
            _ => false,
        }
    }
}

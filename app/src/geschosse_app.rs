//! Geschosse in der App: das Abdunkeln hinter dem Dialog „Gebäude
//! erstellen“ (E16) und das Geschossfenster links neben dem Hauptfenster
//! (Jörn 11.10., [`crate::geschossfenster`]), das der Knopf
//! „Geschossmanager“ unten links öffnet.

use crate::geschossfenster::Out;
use crate::menu::Command;
use crate::ui::{Grip, Id, Panel, LEVELS_TITEL};
use crate::{App, OVERLAY_DIALOG, OVERLAY_SCRIM};
use sk_paint::Rgba;
use sk_platform::layout::WindowId;
use sk_platform::{Event, Key, Surface};

/// Vormultipliziert für `set_overlay_fill`.
fn premul(k: Rgba) -> [u8; 4] {
    let a = k.3 as u32;
    let pm = |v: u8| ((v as u32 * a + 127) / 255) as u8;
    [pm(k.0), pm(k.1), pm(k.2), k.3]
}

impl App {
    /// Ziehen einer Ebene oder Linie im Geschossmanager beginnt einen
    /// Schritt; die Schnitthöhe ist Ansichtszustand und hinterlässt keinen.
    pub(crate) fn level_begin(&mut self, g: Grip) {
        self.geschoss.plan_cut_start =
            (g == Grip::PlanCut).then(|| self.scene.model().plan_cut_height());
        self.scene.begin(match g {
            Grip::Sill(_) | Grip::Lintel(_) => "Höhenlinie ziehen",
            _ => "Geschoss ziehen",
        });
    }

    /// Ebene oder Linie auf die Kote `z` gezogen: das Modell folgt in jedem
    /// Bild.
    pub(crate) fn level_move(&mut self, g: Grip, z: f64) {
        match g {
            Grip::FoundationBottom => self.scene.drag_foundation_bottom(z),
            Grip::Top(id) => self.scene.drag_storey_top(id, z),
            Grip::Terrain => self.scene.drag_terrain(z),
            Grip::Sill(id) => self.scene.drag_height_line(id, false, z),
            Grip::Lintel(id) => self.scene.drag_height_line(id, true, z),
            Grip::PlanCut => {
                // Höchstens bis UK Decke: darüber fiele die Linie aus ihrem
                // Band und wäre nicht mehr zu greifen (Review #92 H4)
                let st = self.scene.active_storey();
                let m = self.scene.model();
                let el = m.storey(st).map_or(0.0, |s| s.elevation);
                let h = (z - el).min(m.clear_height(st) - 1.0);
                self.scene.set_plan_cut_height(h);
            }
        }
        self.upload_model();
        self.sync_levels();
    }

    /// Ende des Ziehens im Geschossmanager: der Schritt geht in den Verlauf.
    pub(crate) fn level_end(&mut self) {
        self.geschoss.plan_cut_start = None;
        self.scene.commit();
        self.upload_model();
    }

    /// Dialog „Gebäude erstellen“ samt Abdunkeln des Modellfensters.
    pub(crate) fn paint_dialog(&mut self) {
        if !self.ui.dialog {
            self.renderer.set_overlay(OVERLAY_DIALOG, 0, 0, 0, 0, &[]);
            self.renderer.set_overlay(OVERLAY_SCRIM, 0, 0, 0, 0, &[]);
            return;
        }
        let th = self.title.height();
        let h = self.h.saturating_sub(th);
        let (c, x, y) = self.ui.paint(&self.theme, Panel::Dialog, self.w, th);
        c.premul_rgba8_into(&mut self.panel_px);
        let (w, ch) = (c.width as u32, c.height as u32);
        self.renderer
            .set_overlay(OVERLAY_DIALOG, x, y, w, ch, &self.panel_px);
        let scrim = premul(self.theme.env.scrim);
        self.renderer
            .set_overlay_fill(OVERLAY_SCRIM, 0, th as i32, self.w, h, scrim);
    }

    /// Eine Ebene wird im Geschossfenster gezogen.
    pub(crate) fn level_dragging(&self) -> Option<Grip> {
        self.geschoss.ui.level_dragging()
    }

    /// Öffnet das Geschossfenster oder holt es nach vorn.
    fn open_geschoss(&mut self, surface: &Surface) {
        self.geschoss.open = true;
        self.geschoss.dirty = true;
        surface.open_side(WindowId::Levels, LEVELS_TITEL);
        if !self.ui.levels_open {
            self.ui.levels_open = true;
            self.dirty_buttons.push(Id::LevelsOpen);
        }
    }

    /// Geschossfenster schließen (✕ oder Taskleiste); ein laufendes Ziehen
    /// gilt nicht, eine Eingabe verfällt.
    fn close_geschoss(&mut self, surface: &Surface) {
        self.cancel_level_drag();
        surface.close_side(WindowId::Levels);
        self.geschoss.open = false;
        self.geschoss.ui.edit = None;
        self.geschoss.pending_move = None;
        if self.ui.levels_open {
            self.ui.levels_open = false;
            self.dirty_buttons.push(Id::LevelsOpen);
        }
    }

    /// Bricht das Ziehen einer Ebene ab (Esc): das Modell wie vorher.
    fn cancel_level_drag(&mut self) {
        if self.geschoss.ui.cancel_level_drag() {
            self.scene.rollback();
            if let Some(h) = self.geschoss.plan_cut_start.take() {
                self.scene.set_plan_cut_height(h);
            }
            self.upload_model();
            self.geschoss.dirty = true;
            self.sync_levels();
        }
    }

    /// Ereignis aus dem Geschossfenster. Mausbewegungen werden gesammelt:
    /// verarbeitet wird die letzte vor dem nächsten anderen Ereignis bzw.
    /// beim Abgleich, wie `coalesce_moves` im Hauptfenster.
    pub(crate) fn handle_geschoss(&mut self, e: Event, surface: &Surface) {
        if !self.geschoss.open {
            return;
        }
        self.geschoss.gesperrt = self.ui.dialog;
        if let Event::MouseMove { .. } = e {
            self.geschoss.pending_move = Some(e);
            return;
        }
        self.flush_geschoss_move(surface);
        self.geschoss_event(e, surface);
    }

    fn flush_geschoss_move(&mut self, surface: &Surface) {
        if let Some(e) = self.geschoss.pending_move.take() {
            self.geschoss_event(e, surface);
        }
    }

    /// Ziehen, Zahlen und die Wahl eines Geschosses wirken auf das Modell
    /// wie im Hauptfenster.
    fn geschoss_event(&mut self, e: Event, surface: &Surface) {
        let Some(out) = self.geschoss.handle(&e) else {
            return;
        };
        match out {
            Out::Ui(mut out) => {
                // Neu gezeichnet wird dieses Fenster, nicht die Paneele
                out.changed.clear();
                out.relayout = false;
                self.apply_ui(&out);
                if let Some(id) = out.clicked {
                    self.click(id);
                }
                self.sync_levels();
                self.sync_props();
                self.sync_caption(surface);
            }
            Out::Key(Key::Escape, _) => self.cancel_level_drag(),
            // Rückgängig, Wiederholen und Speichern auch von hier
            Out::Key(key, mods) if mods.ctrl && !mods.alt && self.level_dragging().is_none() => {
                let c = match key {
                    Key::Char('Z') if mods.shift => Command::Redo,
                    Key::Char('Z') => Command::Undo,
                    Key::Char('Y') => Command::Redo,
                    Key::Char('S') if !mods.shift => Command::Save,
                    _ => return,
                };
                self.run_command(c, surface);
            }
            Out::Key(..) => {}
            Out::Command(c) => surface.side_command(WindowId::Levels, c),
            Out::Close => self.close_geschoss(surface),
        }
    }

    /// Geschossfenster öffnen, wenn gewünscht, und neu zeigen.
    pub(crate) fn sync_geschoss(&mut self, surface: &Surface) {
        if std::mem::take(&mut self.geschoss.wanted) {
            self.open_geschoss(surface);
        }
        if !self.geschoss.open {
            return;
        }
        self.flush_geschoss_move(surface);
        let docked = surface.layout().levels_docked();
        if docked != self.geschoss.docked {
            self.geschoss.docked = docked;
            self.geschoss.dirty = true;
        }
        surface.set_side_cursor(WindowId::Levels, self.geschoss.ui.cursor());
        let (w, h, area) = (
            self.geschoss.w,
            self.geschoss.h,
            self.geschoss.caption_area(),
        );
        if let Some(px) = self.geschoss.frame(&self.theme) {
            surface.set_side_caption_area(WindowId::Levels, area);
            surface.present_side(WindowId::Levels, w, h, px, None);
        }
    }
}

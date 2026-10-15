//! Lage der App für die Themenwahl der Hilfe (§1.2), aus `main.rs`.

use crate::App;
use sk_model::Category;

impl App {
    /// Lage der App für die Themenwahl (§1.2).
    pub(crate) fn help_ctx(&self) -> crate::help::HelpCtx {
        use crate::help::Window;
        let window = if self.card.is_some() {
            Some(Window::Backups)
        } else if let Some(p) = &self.prefs {
            Some(if p.pattern_open() {
                Window::Patterns
            } else {
                Window::Settings(p.tab_index())
            })
        } else if self.catalog.is_some() {
            Some(Window::Catalog)
        } else if self.materials.is_some() {
            Some(Window::Materials)
        } else if self.verwaltung.is_some() {
            Some(Window::Verwaltung)
        } else {
            None
        };
        let selection = self
            .sel
            .id
            .and_then(|id| crate::help::sel_kind(self.scene.model(), id));
        crate::help::HelpCtx {
            window,
            dialog: self.ui.dialog,
            flush_pick: self.pick.is_some(),
            dragging: self.edit.is_dragging()
                || self.edit.input().is_some()
                || self.edit.eigenes_thema(),
            tool: self
                .tool
                .enabled
                .then_some(self.tool.category == Category::InteriorWall),
            opening: self.tool.oeffnung.is_some(),
            isolating: self.scene.isolating().is_some(),
            gelaende: crate::help::gelaende_gilt(
                self.platte.leiste.offen,
                self.platte.leiste.werkzeug.aktiv
                    || self.picking.plate
                    || self.picking.flaeche.is_some(),
                selection.is_some(),
            ),
            selection,
            view: self.ui.view,
        }
    }
}

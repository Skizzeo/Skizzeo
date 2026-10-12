//! Bemaßung in der App: beim Zeichnen die Linien und, in Teilen je Geschoss,
//! die Tinte für den Renderer (Review W30). Aus `main.rs` ausgelagert.

use crate::bemassung::Blick;
use crate::App;
use sk_render::Helper;

impl App {
    /// Hilfslinien der Bemaßung; die Tinte geht gleich an den Renderer, der
    /// nur neue Teile hochlädt.
    pub(crate) fn mass_zeichnen(&mut self, scale: f32) -> Vec<Helper> {
        let (w, h, _) = self.view_size();
        let blick = Blick {
            cam: &self.cam,
            w,
            h,
        };
        let (v, sel, f) = (
            self.ui.view,
            &self.picking.selected,
            self.ui.fonts.regular.as_ref(),
        );
        let (linien, tinte) =
            self.massketten
                .zeichnen(&self.scene, v, sel, f, &blick, scale, &self.theme);
        self.renderer.set_ink_teile(&tinte);
        linien
    }
}

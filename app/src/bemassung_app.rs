//! Bemaßung in der App (Konzept `bemassung/konzept.md` §4, §5): vor jedem
//! Ereignis die Ketten der Ansicht für das Bedienen
//! ([`crate::wall_edit::mass_edit`]), beim Zeichnen die Linien mit der
//! Hervorhebung und, in Teilen je Geschoss, die Tinte für den Renderer
//! (Review W30). Aus `main.rs` ausgelagert.

use crate::bemassung::Blick;
use crate::App;
use sk_render::Helper;

impl App {
    /// Stand der Ansicht vor jedem Ereignis: Schnittebene, Höhe und gewählte
    /// Wand für das Band, die gezeigten Ketten für die Bemaßung.
    pub(crate) fn edit_stand(&mut self) {
        self.edit.stand(self.plane(), self.plan_z(), self.sel.id);
        let f = self.ui.fonts.regular.as_ref();
        let (v, sel) = (self.ui.view, &self.picking.selected);
        let stand = self.massketten.gezeigt(&self.scene, v, sel, f);
        self.edit.masse.setze(stand, &mut self.scene);
    }

    /// Hilfslinien der Bemaßung mit Wahl, Maus und Eingabe; die Tinte geht
    /// gleich an den Renderer, der nur neue Teile hochlädt.
    pub(crate) fn mass_zeichnen(&mut self, scale: f32) -> Vec<Helper> {
        let h = self.edit.mass_hervor(&self.scene);
        let (w, hh, _) = self.view_size();
        let blick = Blick {
            cam: &self.cam,
            w,
            h: hh,
        };
        let (v, sel, f) = (
            self.ui.view,
            &self.picking.selected,
            self.ui.fonts.regular.as_ref(),
        );
        let (linien, tinte) =
            self.massketten
                .zeichnen(&self.scene, v, sel, f, &blick, scale, &self.theme, &h);
        self.renderer.set_ink_teile(&tinte);
        linien
    }
}

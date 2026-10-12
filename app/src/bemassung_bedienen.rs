//! Bemaßung bedienen (Stufe 2, Konzept §4 und §5).

use super::Stand;
use sk_render::{Helper, Ink};
use sk_ui::theme::Theme;

/// Zustand beim Bedienen der Ketten.
#[derive(Default)]
pub struct Bedienen {
    /// Die Ansicht zeigt gerade Ketten.
    pub sichtbar: bool,
}

impl Bedienen {
    /// Hervorhebungen über dem Bild der Ketten.
    #[allow(clippy::too_many_arguments)]
    pub fn zeichnen(
        &self,
        _s: &Stand,
        _stil: &sk_model::bemassung::Stil,
        _linien: &mut Vec<Helper>,
        _tinte: &mut Vec<Ink>,
        _scale: f32,
        _theme: &Theme,
        _occlude: bool,
    ) {
    }
}

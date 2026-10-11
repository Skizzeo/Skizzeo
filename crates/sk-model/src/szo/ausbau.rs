//! Ausbau des Raumbuchs in der `.szo` und `.szk` (Raumbuch RB1): die
//! Ausbautypen stehen in `[layerset]` mit ihrem Wort für die Typart
//! ([`crate::TypeCategory::word`]); die Sockelleiste trägt dazu ihre Höhe
//! `h=` in mm. Ohne Sockelleiste bleibt die Datei bytegleich.

use super::{Line, LoadError, Record};
use crate::library::LayerSet;

/// `h=` nur, wenn der Typ eine Höhe hat (Sockelleiste).
pub(super) fn write_height(line: Line, s: &LayerSet) -> Line {
    match s.height {
        Some(h) => line.num("h", h),
        None => line,
    }
}

/// Höhe des Profils aus `h=`; ob sie zur Typart passt, prüft
/// [`LayerSet::problems`].
pub(super) fn read_height(r: &Record) -> Result<Option<f64>, LoadError> {
    match r.opt("h") {
        None => Ok(None),
        Some(_) => r.f64("h").map(Some),
    }
}

//! Öffnungen in Außenwänden (Paket O2, Jörn 11.10.): Wandöffnung (Fenster,
//! Terrassentür) und Türöffnung (Haustür) sind dasselbe Bauteil; die Art
//! bleibt für Nummern, Mengen, Kosten und AVA gespeichert.
//!
//! Die Öffnung liegt als Pfad auf der Außenfläche der Wand: Anfang, ggf. die
//! Außenecke, Ende. Sie trifft jede Außenwand, deren Außenfläche auf dem
//! Pfad liegt und deren Höhe sie überdeckt. So schneidet eine
//! geschossübergreifende Öffnung EG- und OG-Wand, eine Ecköffnung beide
//! Wände an der Ecke, und Teilen oder Vereinen einer Wand ändert nichts an
//! ihr. Unter- und Oberkante hängen an den Höhenlinien eines Geschosses
//! (Brüstung, Sturz) oder stehen frei über OK Rohdecke.

use crate::{LevelRef, StoreyId};
use sk_math::Vec3;

/// Art der Öffnung (Jörn 14:04).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpeningKind {
    /// Wandöffnung: Fenster, Terrassentür.
    Window,
    /// Türöffnung: Haustür.
    Door,
}

impl OpeningKind {
    pub const ALL: [OpeningKind; 2] = [OpeningKind::Window, OpeningKind::Door];

    /// Schlüsselwort in der Datei.
    pub fn word(self) -> &'static str {
        match self {
            OpeningKind::Window => "window",
            OpeningKind::Door => "door",
        }
    }

    /// Name für Menschen.
    pub fn name(self) -> &'static str {
        match self {
            OpeningKind::Window => "Wandöffnung",
            OpeningKind::Door => "Türöffnung",
        }
    }
}

/// Höhenlinie eines Geschosses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeightLine {
    Sill,
    Lintel,
}

/// Höhe einer Öffnungskante: an eine Höhenlinie gebunden (sie wandert mit,
/// wenn die Linie im Geschossmanager gezogen wird) oder frei.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeightRef {
    Line(StoreyId, HeightLine),
    Level(LevelRef),
}

impl HeightRef {
    /// Geschoss, auf das sich die Höhe bezieht.
    pub fn storey(&self) -> StoreyId {
        match self {
            HeightRef::Line(s, _) => *s,
            HeightRef::Level(r) => r.storey,
        }
    }
}

/// Kleinste Breite und Höhe einer Öffnung (mm).
pub const MIN_OPENING: f64 = 100.0;
/// Größter Anschlag je Seite (mm).
pub const MAX_REVEAL: f64 = 250.0;
/// Anschlag vor einem Verblender (links, rechts, oben, unten; mm): Rahmen
/// im Anschlag der Innenschale, der Verblender deckt ihn (Recherche 14:15).
pub const REVEAL_FACING: [f64; 4] = [60.0, 60.0, 60.0, 0.0];
/// Anschlag hinter einem WDVS (mm): die Laibungsdämmung überdeckt den
/// Rahmen um etwa 3 cm (DIN 4108 Bbl. 2).
pub const REVEAL_ETICS: [f64; 4] = [30.0, 30.0, 30.0, 0.0];

/// Eine Öffnung.
#[derive(Clone, Debug, PartialEq)]
pub struct Opening {
    pub kind: OpeningKind,
    /// 2 oder 3 Punkte (x, y) auf der Außenfläche: Anfang, ggf. Außenecke,
    /// Ende. Die von außen sichtbare Breite.
    pub path: Vec<Vec3>,
    pub bottom: HeightRef,
    pub top: HeightRef,
    /// Anschlag links, rechts, oben, unten (mm), von außen gesehen; `None`:
    /// nach Wandtyp ([`REVEAL_FACING`], [`REVEAL_ETICS`], monolithisch
    /// keiner).
    pub reveal: Option<[f64; 4]>,
}

impl Opening {
    /// Sichtbare Breite längs des Pfads (mm).
    pub fn width(&self) -> f64 {
        self.path.windows(2).map(|w| (w[1] - w[0]).length()).sum()
    }
}

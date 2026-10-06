//! Farbwerte. Himmel, Boden und Paneele sind aus Jörns Vorlage ausgelesen.

use sk_paint::Rgba;

/// Boden (Vorlage, Bildbereich unterhalb des Horizonts).
pub const GROUND: Rgba = Rgba::rgb(59, 66, 54);

/// Himmelsverlauf: (Abstand über dem Horizont / Höhe der 3D-Ansicht, Farbe).
/// Zeilenmittel aus der Vorlage; oberhalb 0,532 fortgeschrieben.
pub const SKY: [(f32, Rgba); 16] = [
    (0.0000, Rgba::rgb(113, 132, 154)),
    (0.0013, Rgba::rgb(113, 132, 154)),
    (0.0048, Rgba::rgb(110, 128, 150)),
    (0.0180, Rgba::rgb(106, 125, 146)),
    (0.0312, Rgba::rgb(104, 122, 143)),
    (0.0488, Rgba::rgb(101, 119, 140)),
    (0.0751, Rgba::rgb(98, 116, 137)),
    (0.1103, Rgba::rgb(95, 112, 133)),
    (0.1454, Rgba::rgb(91, 109, 129)),
    (0.1806, Rgba::rgb(89, 106, 126)),
    (0.2245, Rgba::rgb(86, 103, 123)),
    (0.2685, Rgba::rgb(83, 100, 119)),
    (0.3563, Rgba::rgb(78, 95, 114)),
    (0.4442, Rgba::rgb(74, 90, 109)),
    (0.5321, Rgba::rgb(71, 87, 105)),
    (1.0000, Rgba::rgb(54, 69, 87)),
];

/// Weicher Übergang Himmel→Boden: Anteil Boden = 1 - exp(-k · Pixel unter dem Horizont).
pub const HORIZON_SOFTNESS: f32 = 1.03;

/// Flächen „altweiß, neutral“ und Kanten schwarz.
pub const FACE: Rgba = Rgba::rgb(242, 240, 234);
pub const EDGE: Rgba = Rgba::rgb(0, 0, 0);

/// Paneel-Design aus der Vorlage, für spätere Paneele.
pub mod panel {
    use sk_paint::Rgba;
    pub const BACKGROUND: Rgba = Rgba::rgb(31, 37, 45);
    pub const BORDER: Rgba = Rgba::rgb(56, 65, 76);
    pub const FIELD: Rgba = Rgba::rgb(20, 25, 32);
    pub const ACCENT: Rgba = Rgba::rgb(242, 179, 61);
    pub const TEXT: Rgba = Rgba::rgb(231, 229, 222);
    /// Gedämpfte Schrift für Hinweise.
    pub const TEXT_DIM: Rgba = Rgba::rgb(160, 165, 172);
    /// Schrift auf Akzentflächen.
    pub const ON_ACCENT: Rgba = Rgba::rgb(31, 37, 45);
    pub const ACCENT_HOVER: Rgba = Rgba::rgb(248, 196, 96);
    pub const BUTTON_HOVER: Rgba = Rgba::rgb(42, 50, 61);
    pub const BUTTON_PRESSED: Rgba = Rgba::rgb(50, 59, 71);
    pub const CORNER_RADIUS: f32 = 10.0;
}

/// Bauzeichnung (Grundriss, Schnitt, Ansichten).
pub mod drawing {
    use sk_paint::Rgba;
    /// Papiergrund, altweiß neutral.
    pub const PAPER: Rgba = Rgba::rgb(245, 244, 239);
    /// Füllung aller Flächen und Schnittflächen.
    pub const FILL: Rgba = Rgba::rgb(255, 255, 255);
    pub const INK: Rgba = Rgba::rgb(0, 0, 0);
    /// Strichbreiten als Vielfaches der Kantenbreite (1,25 px bei 96 dpi).
    pub const CUT_WIDTH: f32 = 2.2;
    pub const VIEW_WIDTH: f32 = 1.35;
    /// Schnittkontur nicht tragender Schichten (Dämmung), mitteldick.
    pub const LAYER_CUT_WIDTH: f32 = 1.35;
    pub const FINE_WIDTH: f32 = 0.55;
}

/// Baustofffarben in der 3D-Ansicht; Schnittflächen kräftiger.
pub mod material {
    use sk_paint::Rgba;
    pub const AERATED_CONCRETE: Rgba = Rgba::rgb(238, 237, 232);
    pub const INSULATION: Rgba = Rgba::rgb(244, 239, 220);
    pub const AERATED_CONCRETE_CUT: Rgba = Rgba::rgb(176, 177, 174);
    pub const INSULATION_CUT: Rgba = Rgba::rgb(232, 196, 92);
}

/// Eigene Titelleiste (hell, damit das schwarze Logo trägt).
pub mod titlebar {
    use sk_paint::Rgba;
    pub const BACKGROUND: Rgba = Rgba::rgb(243, 243, 243);
    pub const GLYPH: Rgba = Rgba::rgb(0, 0, 0);
    pub const GLYPH_INACTIVE: Rgba = Rgba::rgb(150, 150, 150);
    pub const HOVER: Rgba = Rgba::rgb(229, 229, 229);
    pub const PRESSED: Rgba = Rgba::rgb(204, 204, 204);
    pub const CLOSE_HOVER: Rgba = Rgba::rgb(196, 43, 28);
    pub const CLOSE_PRESSED: Rgba = Rgba::rgb(200, 64, 49);
    pub const CLOSE_GLYPH_HOVER: Rgba = Rgba::rgb(255, 255, 255);
    pub const LOGO: Rgba = Rgba::rgb(0, 0, 0);
}

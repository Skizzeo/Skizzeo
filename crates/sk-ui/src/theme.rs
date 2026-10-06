//! Farbschema der Oberfläche: jede UI-Farbe genau einmal, als benannte Rolle.
//! Himmel, Boden und Paneele sind aus Jörns Vorlage ausgelesen.
//!
//! Die App hält das Schema; alles, was malt, bekommt es als `&Theme`.
//! Zeichnungsfarben (Stifte, Schraffuren, Oberflächen) stehen nicht hier,
//! sondern in den Attributtabellen des Modells.

use sk_paint::Rgba;

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    /// Steigt bei jeder Änderung; Paneelbilder und Zeichentabelle werden dann
    /// neu aufgebaut.
    pub rev: u64,
    pub ui: Ui,
    pub title: Title,
    pub interact: Interact,
    pub env: Environment,
    pub size: Sizes,
    /// Bildpunkte je Millimeter Strichbreite auf dem Papier (bei 96 dpi).
    pub px_per_mm: f32,
}

/// Paneele und Knöpfe.
#[derive(Clone, Debug, PartialEq)]
pub struct Ui {
    pub bg: Rgba,
    pub border: Rgba,
    pub field: Rgba,
    pub text: Rgba,
    /// Gedämpfte Schrift für Hinweise.
    pub text_dim: Rgba,
    /// Schrift auf Akzentflächen.
    pub on_accent: Rgba,
    pub accent: Rgba,
    pub accent_hover: Rgba,
    pub hover: Rgba,
    pub pressed: Rgba,
    /// Je Ring des weichen Paneelschattens.
    pub shadow: Rgba,
}

/// Eigene Titelleiste.
#[derive(Clone, Debug, PartialEq)]
pub struct Title {
    pub bg: Rgba,
    pub glyph: Rgba,
    pub glyph_inactive: Rgba,
    pub hover: Rgba,
    pub pressed: Rgba,
    pub close_hover: Rgba,
    pub close_pressed: Rgba,
    pub close_glyph_hover: Rgba,
    pub logo: Rgba,
}

/// Hilfslinien und Punkte beim Bearbeiten (Farben 0..1 für die Grafikkarte).
#[derive(Clone, Debug, PartialEq)]
pub struct Interact {
    /// Auswahlumriss.
    pub select: [f32; 4],
    /// Live-Wand und freier Fangpunkt.
    pub draw: [f32; 4],
    /// Spurlinie vom Startpunkt, Fangpunkt auf Linie oder Kreuzung.
    pub track: [f32; 4],
    /// Übrige Spurlinien.
    pub guide: [f32; 4],
    /// Startpunkt.
    pub start: [f32; 4],
    /// Ziehen und aktiv (Schnittlinie, Gummiband).
    pub drag: [f32; 4],
    /// Gummiband unter der Maus.
    pub drag_hot: [f32; 4],
    /// Gummiband ruhend (halb durchsichtig).
    pub drag_ghost: [f32; 4],
    /// Dunkler Grund unter Fangpunkten.
    pub shadow_tool: [f32; 4],
    /// Dunkler Grund unter dem Gummiband.
    pub shadow_band: [f32; 4],
}

/// 3D-Umgebung.
#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    /// Himmelsverlauf: (Abstand über dem Horizont / Höhe der 3D-Ansicht, Farbe).
    pub sky: Vec<(f32, Rgba)>,
    pub ground: Rgba,
    /// Weicher Übergang Himmel→Boden: Anteil Boden = 1 - exp(-k · Pixel unter dem Horizont).
    pub horizon_softness: f32,
    /// Flächen ohne Baustoff und Kanten.
    pub face: Rgba,
    pub edge: Rgba,
    /// Papier und Füllung, wenn eine Attributangabe fehlt.
    pub paper_fallback: Rgba,
    pub fill_fallback: Rgba,
}

/// Maße in Bildpunkten bei Skalierung 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sizes {
    pub corner_radius: f32,
    /// Knöpfe und Bezeichnungen.
    pub font: f32,
    /// Hinweise, Werte und Schichten.
    pub font_small: f32,
    /// Feinste Zeilen (Mengen unter einer Schicht).
    pub font_detail: f32,
    /// Paneeltitel.
    pub font_title: f32,
    /// Kennbuchstabe der Schnittlinie.
    pub font_mark: f32,
    /// Strichbreite des Auswahlumrisses.
    pub outline: f32,
    /// Abstand der Paneele vom Rand.
    pub panel_margin: f32,
    /// Innenabstand der Paneele.
    pub panel_pad: f32,
    pub panel_width: f32,
    /// Platz für den Schatten rund um ein Paneel.
    pub panel_shadow: f32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    Rgba::rgb(r, g, b)
}

impl Theme {
    /// Dunkles Schema nach Jörns Vorlage.
    pub fn dark() -> Theme {
        let bg = rgb(31, 37, 45);
        let accent = rgb(242, 179, 61);
        let hover = rgb(42, 50, 61);
        let pressed = rgb(50, 59, 71);
        let text = rgb(231, 229, 222);
        Theme {
            name: "Dunkel".into(),
            rev: 0,
            ui: Ui {
                bg,
                border: rgb(56, 65, 76),
                field: rgb(20, 25, 32),
                text,
                text_dim: rgb(160, 165, 172),
                on_accent: bg,
                accent,
                accent_hover: rgb(248, 196, 96),
                hover,
                pressed,
                shadow: Rgba(0, 0, 0, 14),
            },
            title: Title {
                // Dunkel wie die Paneele, damit sie sich auch über dem Papier abhebt
                bg,
                glyph: text,
                glyph_inactive: rgb(120, 127, 136),
                hover,
                pressed,
                close_hover: rgb(196, 43, 28),
                close_pressed: rgb(200, 64, 49),
                close_glyph_hover: rgb(255, 255, 255),
                logo: rgb(255, 255, 255),
            },
            interact: Interact {
                select: accent.to_f32(),
                draw: accent.to_f32(),
                track: [0.85, 0.15, 0.85, 1.0],
                guide: [0.9, 0.3, 0.2, 1.0],
                start: [0.15, 0.75, 0.25, 1.0],
                drag: [0.56, 0.27, 0.86, 1.0],
                drag_hot: [0.74, 0.50, 1.0, 1.0],
                drag_ghost: [0.56, 0.27, 0.86, 0.7],
                shadow_tool: [0.0, 0.0, 0.0, 0.85],
                shadow_band: [0.0, 0.0, 0.0, 0.55],
            },
            env: Environment {
                // Zeilenmittel aus der Vorlage; oberhalb 0,532 fortgeschrieben
                sky: vec![
                    (0.0000, rgb(113, 132, 154)),
                    (0.0013, rgb(113, 132, 154)),
                    (0.0048, rgb(110, 128, 150)),
                    (0.0180, rgb(106, 125, 146)),
                    (0.0312, rgb(104, 122, 143)),
                    (0.0488, rgb(101, 119, 140)),
                    (0.0751, rgb(98, 116, 137)),
                    (0.1103, rgb(95, 112, 133)),
                    (0.1454, rgb(91, 109, 129)),
                    (0.1806, rgb(89, 106, 126)),
                    (0.2245, rgb(86, 103, 123)),
                    (0.2685, rgb(83, 100, 119)),
                    (0.3563, rgb(78, 95, 114)),
                    (0.4442, rgb(74, 90, 109)),
                    (0.5321, rgb(71, 87, 105)),
                    (1.0000, rgb(54, 69, 87)),
                ],
                ground: rgb(59, 66, 54),
                horizon_softness: 1.03,
                face: rgb(242, 240, 234),
                edge: rgb(0, 0, 0),
                paper_fallback: rgb(245, 244, 239),
                fill_fallback: rgb(255, 255, 255),
            },
            size: Sizes {
                corner_radius: 10.0,
                font: 14.0,
                font_small: 13.0,
                font_detail: 12.5,
                font_title: 17.0,
                font_mark: 17.0,
                outline: 2.5,
                panel_margin: 12.0,
                panel_pad: 14.0,
                panel_width: 196.0,
                panel_shadow: 10.0,
            },
            px_per_mm: 5.5,
        }
    }

    /// Setzt den Akzent für Paneele, Auswahl und Wandeingabe.
    pub fn set_accent(&mut self, c: Rgba) {
        self.ui.accent = c;
        self.interact.select = c.to_f32();
        self.interact.draw = c.to_f32();
        self.rev += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dunkel_hat_die_bisherigen_paneelwerte() {
        let t = Theme::dark();
        assert_eq!(t.ui.bg, Rgba::rgb(31, 37, 45));
        assert_eq!(t.ui.accent, Rgba::rgb(242, 179, 61));
        assert_eq!(t.ui.border, Rgba::rgb(56, 65, 76));
        assert_eq!(t.ui.text, Rgba::rgb(231, 229, 222));
        assert_eq!(t.title.bg, t.ui.bg);
        assert_eq!(t.title.hover, t.ui.hover);
        assert_eq!(t.env.sky.len(), 16);
        assert_eq!(t.px_per_mm, 5.5);
    }

    /// Die früheren Werte als 0..1 und ihre Rollen weichen höchstens 0,5/255 ab.
    #[test]
    fn rollen_treffen_die_frueheren_werte() {
        let i = Theme::dark().interact;
        let old: [([f32; 4], [f32; 4]); 10] = [
            (i.select, [242.0 / 255.0, 179.0 / 255.0, 61.0 / 255.0, 1.0]),
            (i.draw, [242.0 / 255.0, 179.0 / 255.0, 61.0 / 255.0, 1.0]),
            (i.track, [0.85, 0.15, 0.85, 1.0]),
            (i.guide, [0.9, 0.3, 0.2, 1.0]),
            (i.start, [0.15, 0.75, 0.25, 1.0]),
            (i.drag, [0.56, 0.27, 0.86, 1.0]),
            (i.drag_hot, [0.74, 0.50, 1.0, 1.0]),
            (i.drag_ghost, [0.56, 0.27, 0.86, 0.7]),
            (i.shadow_tool, [0.0, 0.0, 0.0, 0.85]),
            (i.shadow_band, [0.0, 0.0, 0.0, 0.55]),
        ];
        for (role, was) in old {
            for k in 0..4 {
                assert!((role[k] - was[k]).abs() <= 0.5 / 255.0, "{role:?} {was:?}");
            }
        }
        // Endsymbole der Schnittlinie beim Ziehen: bisher 143, 69, 219
        assert_eq!(Rgba::from_f32(i.drag), Rgba::rgb(143, 69, 219));
    }
}

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
    /// Zahlenfeld: Rahmen, Grund unter der Maus, Rahmen beim Eingeben und bei
    /// ungültiger Eingabe (auch der Hinweistext darunter).
    pub field_border: Rgba,
    pub field_hover: Rgba,
    pub field_focus: Rgba,
    pub field_invalid: Rgba,
    /// Unumkehrbar Großes (Paket „Löschen“): „Gebäude löschen …“, Rand und
    /// Knopf der Rückfrage. Nur dafür.
    pub danger: Rgba,
    /// Zahl, Einheit dahinter, Schreibmarke, markierter Text.
    pub field_text: Rgba,
    pub field_unit: Rgba,
    pub caret: Rgba,
    pub text_select: Rgba,
    /// Berechnete Werte, nicht änderbar.
    pub field_readonly: Rgba,
    /// Paneel „Geschosse“ (E14): Ebenenlinie, Linie des aktiven Geschosses,
    /// Griff normal, unter der Maus und beim Ziehen.
    pub level_line: Rgba,
    pub level_line_active: Rgba,
    pub level_handle: Rgba,
    pub level_handle_hover: Rgba,
    pub level_handle_drag: Rgba,
    /// Maßkette, Maßzahl und Kote, Maßzahl unter der Maus (klickbar).
    pub dim_line: Rgba,
    pub dim_text: Rgba,
    pub dim_text_hover: Rgba,
    /// Gesperrte Knöpfe und Menüzeilen: Schrift (`text_dim` halb deckend).
    pub text_disabled: Rgba,
    /// Hinweis an der Maus: Grund und Schrift.
    pub tooltip_bg: Rgba,
    pub tooltip_text: Rgba,
    /// Dateimenü (E17): Fläche.
    pub menu_bg: Rgba,
    /// Schwebende Bedienelemente über dem Modell (Geschossbogen, E18): Fläche
    /// und Leuchten (die Abstufungen rechnet der Code als Deckkraft).
    pub hud_bg: Rgba,
    pub hud_glow: Rgba,
    /// Kettensymbol je Wandsegment (OG Phase 2): Glieder gekoppelt bzw.
    /// gelöst, Plättchen darunter (Fläche der Bogen-Beschriftungen).
    pub link_on: Rgba,
    pub link_off: Rgba,
    pub link_chip: Rgba,
    /// Mengenfenster (B7): Blatt, Schrift, gedämpfte Schrift, Kontrollzeilen
    /// (kursiv), Haarlinien, Kacheln; Bänder für Hover, Aufleuchten, Auswahl
    /// und die Gruppe über der Auswahl (folgen dem Akzent).
    pub sheet_bg: Rgba,
    pub sheet_text: Rgba,
    pub sheet_text_dim: Rgba,
    pub sheet_hint: Rgba,
    pub sheet_rule: Rgba,
    pub sheet_tile: Rgba,
    pub sheet_hover: Rgba,
    pub sheet_flash: Rgba,
    pub sheet_select: Rgba,
    pub sheet_select_group: Rgba,
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
    /// Bauteil unter der Maus, auch vom Mengenfenster aus (F2): zarter als
    /// die Auswahl.
    pub hover_element: [f32; 4],
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
    /// Deckkraft des Bodens über Modellteilen unter z = 0 in 3D (E11):
    /// 1 = deckend, 0 = Boden nur als Hintergrund.
    pub ground_opacity: f32,
    /// Flächen ohne Baustoff und Kanten.
    pub face: Rgba,
    pub edge: Rgba,
    /// Papier und Füllung, wenn eine Attributangabe fehlt.
    pub paper_fallback: Rgba,
    pub fill_fallback: Rgba,
    /// Abdunkeln des Modellfensters hinter einem Dialog (E16).
    pub scrim: Rgba,
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
    /// Höhe und Innenabstand eines Zahlenfelds.
    pub field_height: f32,
    pub field_pad: f32,
    /// Paneel „Geschosse“: Maßstab des Diagramms (px je m) und kleinster
    /// Maßstab im kleinen Fenster, Griffdurchmesser, Fangabstand um Griff und
    /// Linie, Mindestabstand zweier Linien.
    pub level_px_per_m: f32,
    pub level_px_per_m_min: f32,
    pub level_handle: f32,
    pub level_hit: f32,
    pub level_row_min: f32,
    /// Abstand des Geschossnamens vom Griff und Grundlinie der ersten
    /// Listenzeile unter dem Titel.
    pub level_label_gap: f32,
    pub level_row_base: f32,
    /// Hilfslinie der gezogenen Ebene in 3D, Schnitt und Ansichten (px).
    pub level_guide: f32,
    /// Maßketten: halbe Länge des Schrägstrichs und dessen Strichstärke.
    pub dim_tick: f32,
    pub dim_line: f32,
    /// Dialog „Gebäude erstellen“ (E16): Breite und Höhe.
    pub dialog_w: f32,
    pub dialog_h: f32,
    /// Zeilenabstand der Zeilen im Dialog „Gebäude erstellen“ (dip).
    pub dialog_row: f32,
    /// Dateimenü (E17): Zeilenhöhe, Breite des Menüs und des Untermenüs.
    pub menu_row: f32,
    pub menu_w: f32,
    pub menu_sub_w: f32,
    /// Einstellungsfenster (E5): Breite, Höhe, Reiterleiste links; Zeile der
    /// Tabellen, Bildlaufleiste, Farbwähler, Farbfeld, Kontrollkästchen.
    pub settings_w: f32,
    pub settings_h: f32,
    pub settings_tabs_w: f32,
    pub table_row: f32,
    pub scrollbar: f32,
    pub picker_w: f32,
    pub picker_h: f32,
    pub swatch_w: f32,
    pub swatch_h: f32,
    pub checkbox: f32,
    /// Reiter Schraffuren usw. (E6): Kachel in der Liste, Vorschaubild.
    pub list_thumb_w: f32,
    pub list_thumb_h: f32,
    pub preview_w: f32,
    pub preview_h: f32,
    /// Geschossbogen (E18): Radius bis zur Bandmitte, halber Öffnungswinkel
    /// (Grad), Bandbreite, Länge und Breite der Pfeilspitzen, Schriftgröße
    /// des aktiven Geschosses und der Nachbarn.
    pub arc_r: f32,
    pub arc_span_deg: f32,
    pub arc_band: f32,
    pub arc_head_l: f32,
    pub arc_head_w: f32,
    pub arc_label: f32,
    pub arc_label_small: f32,
    /// Dauer der Übergänge in ms (0 = keine Animation) und Verzögerung des
    /// Hinweises an schwebenden Bedienelementen in s.
    pub anim_ms: f32,
    pub hover_delay_hud: f32,
    /// Mengenfenster (B7): Zeilenhöhe, Einzug je Stufe, Innenabstand des
    /// Blatts, größte Inhaltsbreite, Breite beim Andocken (dip), Dauer des
    /// Aufleuchtens geänderter Werte (ms).
    pub qto_row: f32,
    pub qto_indent: f32,
    pub sheet_pad: f32,
    pub qto_max_w: f32,
    /// Schmalste Kachel der Summe nach Baustoff (dip); passen nicht alle in
    /// eine Zeile, brechen sie um.
    pub sheet_tile_min_w: f32,
    pub qto_window_w: f32,
    pub flash_ms: f32,
    /// Aus- und Einblenden gelöschter Bauteile und des Hinweises am Bauteil
    /// (Paket „Löschen“), ms.
    pub fade_ms: f32,
    /// Kettensymbol (OG Phase 2): Glied breit und hoch, Strichstärke; Breite
    /// der Fußlinie des Partners darunter und des Ziehgeists, Strichlänge
    /// beider gestrichelt (dip).
    pub link_icon_w: f32,
    pub link_icon_h: f32,
    pub link_icon_stroke: f32,
    pub link_line: f32,
    pub link_dash: f32,
    /// Schein um ein hervorgehobenes Bauteil (Hover aus der Mengenliste,
    /// Zielwand beim „Bündig setzen“), dip.
    pub glow_w: f32,
    /// Maßzahl im Bild (Weg beim „Bündig setzen“): Rand, Höhe, Eckradius.
    pub dim_label_pad: f32,
    pub dim_label_h: f32,
    pub dim_label_radius: f32,
    /// Ziehpunkt am Wandfuß in Parallelansichten: ruhend, unter der Maus,
    /// Schatten darunter (Durchmesser, dip).
    pub drag_dot: f32,
    pub drag_dot_hot: f32,
    pub drag_dot_shadow: f32,
    /// Bauteilkatalog (K3): Dialog, Liste links, Kachelzeile, Schnittbild-Kachel.
    pub catalog_w: f32,
    pub catalog_h: f32,
    pub catalog_list_w: f32,
    pub catalog_tile_h: f32,
    pub catalog_thumb_w: f32,
    pub catalog_thumb_h: f32,
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
                field_border: rgb(56, 65, 76),
                field_hover: rgb(26, 32, 40),
                field_focus: accent,
                field_invalid: rgb(214, 84, 64),
                danger: rgb(214, 84, 64),
                field_text: text,
                field_unit: rgb(160, 165, 172),
                caret: text,
                text_select: Rgba(accent.0, accent.1, accent.2, 90),
                field_readonly: rgb(160, 165, 172),
                level_line: rgb(160, 165, 172),
                level_line_active: accent,
                level_handle: text,
                level_handle_hover: rgb(248, 196, 96),
                level_handle_drag: rgb(143, 69, 219),
                dim_line: rgb(160, 165, 172),
                dim_text: text,
                dim_text_hover: accent,
                text_disabled: Rgba(160, 165, 172, 128),
                tooltip_bg: rgb(20, 25, 32),
                tooltip_text: text,
                menu_bg: bg,
                hud_bg: Rgba(bg.0, bg.1, bg.2, 199),
                hud_glow: accent,
                link_on: accent,
                link_off: rgb(160, 165, 172),
                link_chip: Rgba(bg.0, bg.1, bg.2, 199),
                sheet_bg: rgb(245, 244, 239),
                sheet_text: rgb(34, 36, 40),
                sheet_text_dim: rgb(112, 116, 122),
                sheet_hint: rgb(138, 142, 148),
                sheet_rule: rgb(214, 212, 204),
                sheet_tile: rgb(234, 232, 224),
                sheet_hover: Rgba(accent.0, accent.1, accent.2, 36),
                sheet_flash: Rgba(accent.0, accent.1, accent.2, 120),
                sheet_select: Rgba(accent.0, accent.1, accent.2, 84),
                sheet_select_group: Rgba(accent.0, accent.1, accent.2, 26),
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
                hover_element: hover_of(accent),
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
                ground_opacity: 0.6,
                face: rgb(242, 240, 234),
                edge: rgb(0, 0, 0),
                paper_fallback: rgb(245, 244, 239),
                fill_fallback: rgb(255, 255, 255),
                scrim: Rgba(0, 0, 0, 89),
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
                field_height: 26.0,
                field_pad: 6.0,
                level_px_per_m: 40.0,
                level_px_per_m_min: 20.0,
                level_handle: 10.0,
                level_hit: 8.0,
                level_row_min: 22.0,
                level_label_gap: 6.0,
                level_row_base: 15.0,
                level_guide: 1.5,
                dim_tick: 3.5,
                dim_line: 1.2,
                dialog_w: 300.0,
                dialog_h: 290.0,
                dialog_row: 30.0,
                menu_row: 30.0,
                menu_w: 260.0,
                menu_sub_w: 320.0,
                settings_w: 860.0,
                settings_h: 620.0,
                settings_tabs_w: 170.0,
                table_row: 28.0,
                scrollbar: 8.0,
                picker_w: 300.0,
                picker_h: 320.0,
                swatch_w: 36.0,
                swatch_h: 20.0,
                checkbox: 16.0,
                list_thumb_w: 40.0,
                list_thumb_h: 24.0,
                preview_w: 240.0,
                preview_h: 160.0,
                arc_r: 80.0,
                arc_span_deg: 58.0,
                arc_band: 14.0,
                arc_head_l: 24.0,
                arc_head_w: 34.0,
                arc_label: 26.0,
                arc_label_small: 13.0,
                anim_ms: 280.0,
                qto_row: 22.0,
                qto_indent: 18.0,
                sheet_pad: 28.0,
                qto_max_w: 900.0,
                sheet_tile_min_w: 120.0,
                qto_window_w: 520.0,
                flash_ms: 600.0,
                fade_ms: 150.0,
                link_icon_w: 9.0,
                link_icon_h: 5.0,
                link_icon_stroke: 1.5,
                link_line: 1.5,
                link_dash: 6.0,
                glow_w: 8.0,
                dim_label_pad: 6.0,
                dim_label_h: 20.0,
                dim_label_radius: 4.0,
                drag_dot: 8.0,
                drag_dot_hot: 12.0,
                drag_dot_shadow: 15.0,
                catalog_w: 1080.0,
                catalog_h: 720.0,
                catalog_list_w: 270.0,
                catalog_tile_h: 48.0,
                catalog_thumb_w: 30.0,
                catalog_thumb_h: 34.0,
                hover_delay_hud: 0.25,
            },
            px_per_mm: 5.5,
        }
    }

    /// Setzt den Akzent für Paneele, Auswahl und Wandeingabe.
    pub fn set_accent(&mut self, c: Rgba) {
        // Feldrollen, die dem Akzent folgen, gehen mit
        let old = self.ui.accent;
        if self.ui.field_focus == old {
            self.ui.field_focus = c;
        }
        let sel = self.ui.text_select;
        if (sel.0, sel.1, sel.2) == (old.0, old.1, old.2) {
            self.ui.text_select = Rgba(c.0, c.1, c.2, sel.3);
        }
        for role in [
            &mut self.ui.level_line_active,
            &mut self.ui.dim_text_hover,
            &mut self.ui.hud_glow,
            &mut self.ui.link_on,
        ] {
            if *role == old {
                *role = c;
            }
        }
        // Bänder im Mengenfenster: Akzent mit eigener Deckkraft
        for role in [
            &mut self.ui.sheet_hover,
            &mut self.ui.sheet_flash,
            &mut self.ui.sheet_select,
            &mut self.ui.sheet_select_group,
        ] {
            if (role.0, role.1, role.2) == (old.0, old.1, old.2) {
                *role = Rgba(c.0, c.1, c.2, role.3);
            }
        }
        // Akzent unter der Maus: folgt, solange er die Vorbelegung zum alten
        // Akzent ist (aufgehellt; beim dunklen Schema dessen eigener Wert)
        let dark = Theme::dark().ui;
        let old_hover = self.ui.accent_hover;
        let derived =
            old_hover == lighten(old) || (old == dark.accent && old_hover == dark.accent_hover);
        if derived {
            let h = lighten(c);
            self.ui.accent_hover = h;
            if self.ui.level_handle_hover == old_hover {
                self.ui.level_handle_hover = h;
            }
        }
        self.ui.accent = c;
        if self.interact.hover_element == hover_of(old) {
            self.interact.hover_element = hover_of(c);
        }
        self.interact.select = c.to_f32();
        self.interact.draw = c.to_f32();
        self.rev += 1;
    }
}

/// Umriss des Bauteils unter der Maus zum Akzent: fast deckend, damit er
/// sich von der Auswahl unterscheidet (B7; der weiche Schein dahinter nimmt
/// davon 43 %).
fn hover_of(accent: Rgba) -> [f32; 4] {
    let [r, g, b, _] = accent.to_f32();
    [r, g, b, 0.9]
}

/// Akzent unter der Maus zu einem Akzent: 20 % in Richtung Weiß.
pub fn lighten(c: Rgba) -> Rgba {
    let up = |v: u8| (v as f32 + (255.0 - v as f32) * 0.2).round() as u8;
    Rgba(up(c.0), up(c.1), up(c.2), c.3)
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

    /// E12: Rollen der Zahlenfelder.
    #[test]
    fn feldrollen() {
        let mut t = Theme::dark();
        let u = &t.ui;
        assert_eq!(u.field, Rgba::rgb(20, 25, 32));
        assert_eq!(u.field_border, u.border);
        assert_eq!(u.field_hover, Rgba::rgb(26, 32, 40));
        assert_eq!(u.field_focus, u.accent);
        assert_eq!(u.field_invalid, Rgba::rgb(214, 84, 64));
        assert_eq!((u.field_text, u.caret), (u.text, u.text));
        assert_eq!((u.field_unit, u.field_readonly), (u.text_dim, u.text_dim));
        assert_eq!(u.text_select, Rgba(242, 179, 61, 90));
        assert_eq!((t.size.field_height, t.size.field_pad), (26.0, 6.0));
        let blue = Rgba::rgb(40, 120, 220);
        t.set_accent(blue);
        assert_eq!(t.ui.field_focus, blue);
        assert_eq!(t.ui.text_select, Rgba(40, 120, 220, 90));
        // Eigene Feldfarbe folgt dem Akzent nicht mehr
        t.ui.field_focus = Rgba::rgb(1, 2, 3);
        t.set_accent(Rgba::rgb(9, 9, 9));
        assert_eq!(t.ui.field_focus, Rgba::rgb(1, 2, 3));
    }

    /// E14: Rollen und Maße des Paneels „Geschosse“; aktive Linie und
    /// Maßzahl unter der Maus folgen dem Akzent.
    #[test]
    fn geschossrollen() {
        let mut t = Theme::dark();
        let u = &t.ui;
        assert_eq!((u.level_line, u.dim_line), (u.text_dim, u.text_dim));
        assert_eq!(
            (u.level_line_active, u.dim_text_hover),
            (u.accent, u.accent)
        );
        assert_eq!((u.level_handle, u.dim_text), (u.text, u.text));
        assert_eq!(u.level_handle_hover, u.accent_hover);
        assert_eq!(u.level_handle_drag, Rgba::from_f32(t.interact.drag));
        let z = &t.size;
        assert_eq!(
            (z.level_px_per_m, z.level_px_per_m_min, z.level_handle),
            (40.0, 20.0, 10.0)
        );
        assert_eq!((z.level_hit, z.level_row_min), (8.0, 22.0));
        assert_eq!(
            (z.level_label_gap, z.level_row_base, z.level_guide),
            (6.0, 15.0, 1.5)
        );
        assert_eq!((z.dim_tick, z.dim_line), (3.5, 1.2));
        let blue = Rgba::rgb(40, 120, 220);
        t.set_accent(blue);
        assert_eq!((t.ui.level_line_active, t.ui.dim_text_hover), (blue, blue));
    }

    /// Akzent unter der Maus folgt dem Akzent (aufgehellt), solange er nicht
    /// eigens gewählt ist.
    #[test]
    fn akzent_unter_der_maus_folgt() {
        let mut t = Theme::dark();
        let blue = Rgba::rgb(40, 120, 220);
        t.set_accent(blue);
        assert_eq!(t.ui.accent_hover, Rgba::rgb(83, 147, 227));
        assert_eq!(t.ui.level_handle_hover, t.ui.accent_hover);
        assert_eq!(t.ui.hud_glow, blue);
        let green = Rgba::rgb(20, 160, 60);
        t.set_accent(green);
        assert_eq!(t.ui.accent_hover, lighten(green));
        // eigens gewählt: bleibt
        t.ui.accent_hover = Rgba::rgb(1, 2, 3);
        t.set_accent(blue);
        assert_eq!(t.ui.accent_hover, Rgba::rgb(1, 2, 3));
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

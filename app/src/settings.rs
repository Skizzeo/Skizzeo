//! Programmeinstellungen des Nutzers: `%APPDATA%\Skizzeo\einstellungen.txt`.
//!
//! Gleiche Zeilensyntax wie `.szo` ([`sk_model::szo`]), eigener Kopf.
//! Gespeichert werden nur Abweichungen vom Grundschema, damit Verbesserungen
//! am Standardschema auch bei bestehenden Nutzern ankommen. Lesen bricht nie
//! ab: Unbekanntes und Kaputtes wird übersprungen und als Hinweis gesammelt.

use crate::menu::Recent;
use sk_model::szo::{check_header, hex, parse_hex, Line, Record};
use sk_paint::Rgba;
use sk_ui::theme::Theme;
use std::path::PathBuf;

mod fremd;
pub mod vorgaben;

const HEAD: &str = "SKIZZEO-EINSTELLUNGEN";
const VERSION: u32 = 1;

/// Rolle: Schlüssel in der Datei, deutsche Bezeichnung (Einstellungsfenster),
/// Zugriff.
pub type RgbaRole = (&'static str, &'static str, fn(&mut Theme) -> &mut Rgba);
pub type F4Role = (&'static str, &'static str, fn(&mut Theme) -> &mut [f32; 4]);
pub type SizeRole = (&'static str, &'static str, fn(&mut Theme) -> &mut f32);

pub const RGBA_ROLES: [RgbaRole; 68] = [
    ("ui.bg", "Fläche", |t| &mut t.ui.bg),
    ("ui.border", "Rahmen", |t| &mut t.ui.border),
    ("ui.field", "Feld", |t| &mut t.ui.field),
    ("ui.text", "Schrift", |t| &mut t.ui.text),
    ("ui.text_dim", "Schrift gedämpft", |t| &mut t.ui.text_dim),
    ("ui.on_accent", "Schrift auf Akzent", |t| {
        &mut t.ui.on_accent
    }),
    ("ui.accent", "Akzent", |t| &mut t.ui.accent),
    ("ui.accent_hover", "Akzent unter der Maus", |t| {
        &mut t.ui.accent_hover
    }),
    ("ui.hover", "Unter der Maus", |t| &mut t.ui.hover),
    ("ui.pressed", "Gedrückt", |t| &mut t.ui.pressed),
    ("ui.shadow", "Schatten", |t| &mut t.ui.shadow),
    ("ui.field_border", "Feld Rahmen", |t| &mut t.ui.field_border),
    ("ui.field_hover", "Feld unter der Maus", |t| {
        &mut t.ui.field_hover
    }),
    ("ui.field_focus", "Feld Fokus", |t| &mut t.ui.field_focus),
    ("ui.field_invalid", "Feld ungültig", |t| {
        &mut t.ui.field_invalid
    }),
    ("ui.danger", "Löschen (Gebäude)", |t| &mut t.ui.danger),
    ("ui.field_text", "Feld Text", |t| &mut t.ui.field_text),
    ("ui.field_unit", "Einheit", |t| &mut t.ui.field_unit),
    ("ui.caret", "Schreibmarke", |t| &mut t.ui.caret),
    ("ui.text_select", "Markierung", |t| &mut t.ui.text_select),
    ("ui.field_readonly", "Feld berechnet", |t| {
        &mut t.ui.field_readonly
    }),
    ("ui.level_line", "Ebenenlinie", |t| &mut t.ui.level_line),
    ("ui.level_line_active", "Ebenenlinie aktiv", |t| {
        &mut t.ui.level_line_active
    }),
    ("ui.level_handle", "Griff", |t| &mut t.ui.level_handle),
    ("ui.level_handle_hover", "Griff unter der Maus", |t| {
        &mut t.ui.level_handle_hover
    }),
    ("ui.level_handle_drag", "Griff beim Ziehen", |t| {
        &mut t.ui.level_handle_drag
    }),
    ("ui.dim_line", "Maßkette", |t| &mut t.ui.dim_line),
    ("ui.dim_text", "Maßzahl", |t| &mut t.ui.dim_text),
    ("ui.dim_text_hover", "Maßzahl unter der Maus", |t| {
        &mut t.ui.dim_text_hover
    }),
    ("ui.text_disabled", "Schrift gesperrt", |t| {
        &mut t.ui.text_disabled
    }),
    ("ui.tooltip_bg", "Hinweis Grund", |t| &mut t.ui.tooltip_bg),
    ("ui.tooltip_text", "Hinweis Schrift", |t| {
        &mut t.ui.tooltip_text
    }),
    ("ui.menu_bg", "Menü Fläche", |t| &mut t.ui.menu_bg),
    ("ui.hud_bg", "Schwebende Elemente", |t| &mut t.ui.hud_bg),
    ("ui.hud_glow", "Leuchten", |t| &mut t.ui.hud_glow),
    ("ui.link_on", "Kette gekoppelt", |t| &mut t.ui.link_on),
    ("ui.link_off", "Kette gelöst", |t| &mut t.ui.link_off),
    ("ui.link_chip", "Kette Plättchen", |t| &mut t.ui.link_chip),
    ("title.bg", "Titelleiste", |t| &mut t.title.bg),
    ("title.glyph", "Symbole", |t| &mut t.title.glyph),
    ("title.glyph_inactive", "Symbole inaktiv", |t| {
        &mut t.title.glyph_inactive
    }),
    ("title.hover", "Knopf unter der Maus", |t| {
        &mut t.title.hover
    }),
    ("title.pressed", "Knopf gedrückt", |t| &mut t.title.pressed),
    ("title.close_hover", "Schließen unter der Maus", |t| {
        &mut t.title.close_hover
    }),
    ("title.close_pressed", "Schließen gedrückt", |t| {
        &mut t.title.close_pressed
    }),
    (
        "title.close_glyph_hover",
        "Schließen-Symbol unter der Maus",
        |t| &mut t.title.close_glyph_hover,
    ),
    ("title.logo", "Logo", |t| &mut t.title.logo),
    ("env.ground", "Boden", |t| &mut t.env.ground),
    ("env.face", "Flächen ohne Baustoff", |t| &mut t.env.face),
    ("env.edge", "Kanten ohne Stift", |t| &mut t.env.edge),
    ("env.paper_fallback", "Papier (Ersatz)", |t| {
        &mut t.env.paper_fallback
    }),
    ("env.fill_fallback", "Füllung (Ersatz)", |t| {
        &mut t.env.fill_fallback
    }),
    ("env.scrim", "Abdunkeln hinter Dialogen", |t| {
        &mut t.env.scrim
    }),
    ("ui.sheet_bg", "Blatt", |t| &mut t.ui.sheet_bg),
    ("ui.sheet_text", "Schrift auf dem Blatt", |t| {
        &mut t.ui.sheet_text
    }),
    ("ui.sheet_text_dim", "Schrift gedämpft (Blatt)", |t| {
        &mut t.ui.sheet_text_dim
    }),
    ("ui.sheet_hint", "Kontrollzeilen", |t| &mut t.ui.sheet_hint),
    ("ui.sheet_rule", "Linien auf dem Blatt", |t| {
        &mut t.ui.sheet_rule
    }),
    ("ui.sheet_tile", "Kacheln", |t| &mut t.ui.sheet_tile),
    ("ui.sheet_card", "Karten und Felder auf dem Blatt", |t| {
        &mut t.ui.sheet_card
    }),
    ("ui.sheet_success", "Grün auf dem Blatt: erfüllt", |t| {
        &mut t.ui.sheet_success
    }),
    ("ui.sheet_hover", "Zeile unter der Maus", |t| {
        &mut t.ui.sheet_hover
    }),
    ("ui.sheet_flash", "Aufleuchten geänderter Werte", |t| {
        &mut t.ui.sheet_flash
    }),
    ("ui.sheet_select", "Gewählte Zeile", |t| {
        &mut t.ui.sheet_select
    }),
    ("ui.text_same", "Grün: wie im Projekt, erledigt", |t| {
        &mut t.ui.text_same
    }),
    ("ui.sheet_select_group", "Gruppe der Auswahl", |t| {
        &mut t.ui.sheet_select_group
    }),
    ("ui.tree_hover", "Baum Zeile unter der Maus", |t| {
        &mut t.ui.tree_hover
    }),
    ("ui.isolate_band", "Baum Band beim Isolieren", |t| {
        &mut t.ui.isolate_band
    }),
];

pub const F4_ROLES: [F4Role; 11] = [
    ("interact.select", "Auswahl", |t| &mut t.interact.select),
    ("interact.hover_element", "Bauteil unter der Maus", |t| {
        &mut t.interact.hover_element
    }),
    ("interact.draw", "Wand zeichnen", |t| &mut t.interact.draw),
    ("interact.track", "Spurlinie vom Startpunkt", |t| {
        &mut t.interact.track
    }),
    ("interact.guide", "Spurlinien", |t| &mut t.interact.guide),
    ("interact.start", "Startpunkt", |t| &mut t.interact.start),
    ("interact.drag", "Ziehen", |t| &mut t.interact.drag),
    ("interact.drag_hot", "Gummiband unter der Maus", |t| {
        &mut t.interact.drag_hot
    }),
    ("interact.drag_ghost", "Gummiband ruhend", |t| {
        &mut t.interact.drag_ghost
    }),
    ("interact.shadow_tool", "Grund unter Fangpunkten", |t| {
        &mut t.interact.shadow_tool
    }),
    ("interact.shadow_band", "Grund unter dem Gummiband", |t| {
        &mut t.interact.shadow_band
    }),
];

pub const SIZE_ROLES: [SizeRole; 96] = [
    ("corner_radius", "Eckenradius", |t| {
        &mut t.size.corner_radius
    }),
    ("font", "Schrift", |t| &mut t.size.font),
    ("font_small", "Schrift klein", |t| &mut t.size.font_small),
    ("font_detail", "Schrift Details", |t| {
        &mut t.size.font_detail
    }),
    ("font_title", "Schrift Titel", |t| &mut t.size.font_title),
    ("font_mark", "Kennbuchstabe Schnitt", |t| {
        &mut t.size.font_mark
    }),
    ("outline", "Auswahlumriss", |t| &mut t.size.outline),
    ("panel_margin", "Paneel Randabstand", |t| {
        &mut t.size.panel_margin
    }),
    ("panel_pad", "Paneel Innenabstand", |t| {
        &mut t.size.panel_pad
    }),
    ("panel_width", "Paneel Breite", |t| &mut t.size.panel_width),
    ("panel_shadow", "Paneel Schatten", |t| {
        &mut t.size.panel_shadow
    }),
    ("field_height", "Feld Höhe", |t| &mut t.size.field_height),
    ("field_pad", "Feld Innenabstand", |t| &mut t.size.field_pad),
    ("level_px_per_m", "Geschosse Maßstab", |t| {
        &mut t.size.level_px_per_m
    }),
    ("level_px_per_m_min", "Geschosse kleinster Maßstab", |t| {
        &mut t.size.level_px_per_m_min
    }),
    ("level_handle", "Geschosse Griff", |t| {
        &mut t.size.level_handle
    }),
    ("level_hit", "Geschosse Fangabstand", |t| {
        &mut t.size.level_hit
    }),
    ("level_row_min", "Geschosse Zeilenabstand", |t| {
        &mut t.size.level_row_min
    }),
    ("level_label_gap", "Geschosse Abstand Name", |t| {
        &mut t.size.level_label_gap
    }),
    ("level_row_base", "Geschosse erste Zeile", |t| {
        &mut t.size.level_row_base
    }),
    ("level_guide", "Hilfslinie Ebene", |t| {
        &mut t.size.level_guide
    }),
    ("dim_tick", "Maßkette Schrägstrich", |t| {
        &mut t.size.dim_tick
    }),
    ("dim_line", "Maßkette Strich", |t| &mut t.size.dim_line),
    ("dialog_w", "Dialog Breite", |t| &mut t.size.dialog_w),
    ("dialog_h", "Dialog Höhe", |t| &mut t.size.dialog_h),
    ("dialog_row", "Dialog Zeile", |t| &mut t.size.dialog_row),
    ("menu_row", "Menü Zeile", |t| &mut t.size.menu_row),
    ("menu_w", "Menü Breite", |t| &mut t.size.menu_w),
    ("menu_sub_w", "Untermenü Breite", |t| {
        &mut t.size.menu_sub_w
    }),
    ("settings_w", "Einstellungen Breite", |t| {
        &mut t.size.settings_w
    }),
    ("settings_h", "Einstellungen Höhe", |t| {
        &mut t.size.settings_h
    }),
    ("settings_tabs_w", "Einstellungen Reiter", |t| {
        &mut t.size.settings_tabs_w
    }),
    ("table_row", "Tabellenzeile", |t| &mut t.size.table_row),
    ("scrollbar", "Bildlaufleiste", |t| &mut t.size.scrollbar),
    ("picker_w", "Farbwähler Breite", |t| &mut t.size.picker_w),
    ("picker_h", "Farbwähler Höhe", |t| &mut t.size.picker_h),
    ("swatch_w", "Farbfeld Breite", |t| &mut t.size.swatch_w),
    ("swatch_h", "Farbfeld Höhe", |t| &mut t.size.swatch_h),
    ("checkbox", "Kontrollkästchen", |t| &mut t.size.checkbox),
    ("list_thumb_w", "Listenkachel Breite", |t| {
        &mut t.size.list_thumb_w
    }),
    ("list_thumb_h", "Listenkachel Höhe", |t| {
        &mut t.size.list_thumb_h
    }),
    ("preview_w", "Vorschau Breite", |t| &mut t.size.preview_w),
    ("preview_h", "Vorschau Höhe", |t| &mut t.size.preview_h),
    ("arc_r", "Geschossbogen Radius", |t| &mut t.size.arc_r),
    ("arc_span_deg", "Geschossbogen Öffnung (°)", |t| {
        &mut t.size.arc_span_deg
    }),
    ("arc_band", "Geschossbogen Band", |t| &mut t.size.arc_band),
    ("arc_head_l", "Pfeilspitze Länge", |t| {
        &mut t.size.arc_head_l
    }),
    ("arc_head_w", "Pfeilspitze Breite", |t| {
        &mut t.size.arc_head_w
    }),
    ("arc_label", "Geschossname groß", |t| &mut t.size.arc_label),
    ("arc_label_small", "Geschossname klein", |t| {
        &mut t.size.arc_label_small
    }),
    ("anim_ms", "Animationen (ms)", |t| &mut t.size.anim_ms),
    ("hover_delay_hud", "Hinweis am Bogen (s)", |t| {
        &mut t.size.hover_delay_hud
    }),
    ("qto_row", "Mengenliste: Zeile", |t| &mut t.size.qto_row),
    ("qto_indent", "Mengenliste: Einzug", |t| {
        &mut t.size.qto_indent
    }),
    ("sheet_pad", "Mengenliste: Rand", |t| &mut t.size.sheet_pad),
    ("qto_max_w", "Mengenliste: größte Breite", |t| {
        &mut t.size.qto_max_w
    }),
    ("sheet_tile_min_w", "Mengenliste: schmalste Kachel", |t| {
        &mut t.size.sheet_tile_min_w
    }),
    ("qto_window_w", "Mengenfenster: Breite", |t| {
        &mut t.size.qto_window_w
    }),
    ("mat_mark", "Baustoffliste: Kachel", |t| {
        &mut t.size.mat_mark
    }),
    ("flash_ms", "Aufleuchten (ms)", |t| &mut t.size.flash_ms),
    ("fade_ms", "Ausblenden (ms)", |t| &mut t.size.fade_ms),
    ("ghost_alpha_3d", "Isolieren: Deckkraft 3D", |t| {
        &mut t.size.ghost_alpha_3d
    }),
    ("ghost_alpha_paper", "Isolieren: Deckkraft Zeichnung", |t| {
        &mut t.size.ghost_alpha_paper
    }),
    ("right_width", "Rechte Spalte: Breite", |t| {
        &mut t.size.right_width
    }),
    ("tree_row_h", "Baum: Zeilenhöhe", |t| {
        &mut t.size.tree_row_h
    }),
    ("tree_indent", "Baum: Einzug", |t| &mut t.size.tree_indent),
    ("tree_icon", "Baum: Symbol", |t| &mut t.size.tree_icon),
    ("tree_icon_gap", "Baum: Abstand der Symbole", |t| {
        &mut t.size.tree_icon_gap
    }),
    ("tree_small", "Baum: Kleinangabe rechts", |t| {
        &mut t.size.tree_small
    }),
    ("link_icon_w", "Kettenglied: Breite", |t| {
        &mut t.size.link_icon_w
    }),
    ("link_icon_h", "Kettenglied: Höhe", |t| {
        &mut t.size.link_icon_h
    }),
    ("link_icon_stroke", "Kettenglied: Strich", |t| {
        &mut t.size.link_icon_stroke
    }),
    ("link_line", "Fußlinie des Partners", |t| {
        &mut t.size.link_line
    }),
    ("link_dash", "Fußlinie: Strichlänge", |t| {
        &mut t.size.link_dash
    }),
    ("glow_w", "Schein um Bauteil", |t| &mut t.size.glow_w),
    ("dim_label_pad", "Maßzahl im Bild: Rand", |t| {
        &mut t.size.dim_label_pad
    }),
    ("dim_label_h", "Maßzahl im Bild: Höhe", |t| {
        &mut t.size.dim_label_h
    }),
    ("dim_label_radius", "Maßzahl im Bild: Ecken", |t| {
        &mut t.size.dim_label_radius
    }),
    ("drag_dot", "Ziehpunkt", |t| &mut t.size.drag_dot),
    ("drag_dot_hot", "Ziehpunkt unter der Maus", |t| {
        &mut t.size.drag_dot_hot
    }),
    ("drag_dot_shadow", "Ziehpunkt: Schatten", |t| {
        &mut t.size.drag_dot_shadow
    }),
    ("catalog_w", "Bauteilkatalog: Breite", |t| {
        &mut t.size.catalog_w
    }),
    ("catalog_h", "Bauteilkatalog: Höhe", |t| {
        &mut t.size.catalog_h
    }),
    ("catalog_list_w", "Bauteilkatalog: Liste", |t| {
        &mut t.size.catalog_list_w
    }),
    ("catalog_tile_h", "Bauteilkatalog: Zeile", |t| {
        &mut t.size.catalog_tile_h
    }),
    ("catalog_thumb_w", "Schnittbild-Kachel: Breite", |t| {
        &mut t.size.catalog_thumb_w
    }),
    ("catalog_thumb_h", "Schnittbild-Kachel: Höhe", |t| {
        &mut t.size.catalog_thumb_h
    }),
    ("mat_w", "Baustoffe: Breite", |t| &mut t.size.mat_w),
    ("mat_h", "Baustoffe: Höhe", |t| &mut t.size.mat_h),
    ("mat_list_w", "Baustoffe: Liste", |t| &mut t.size.mat_list_w),
    ("mat_row", "Baustoffe: Zeile", |t| &mut t.size.mat_row),
    ("mat_preview", "Baustoffe: Vorschau", |t| {
        &mut t.size.mat_preview
    }),
    ("mat_label_w", "Baustoffe: Beschriftung", |t| {
        &mut t.size.mat_label_w
    }),
    ("mat_num_w", "Baustoffe: Zahlfeld", |t| {
        &mut t.size.mat_num_w
    }),
    ("mat_name_font", "Baustoffe: Schrift Name", |t| {
        &mut t.size.mat_name_font
    }),
    ("mat_caption", "Baustoffe: Schrift Vorschau", |t| {
        &mut t.size.mat_caption
    }),
];

/// Grundschema zu einem Namen.
fn base(name: &str) -> Option<Theme> {
    let t = Theme::dark();
    (t.name == name).then_some(t)
}

fn rgba_hex(c: Rgba) -> String {
    let s = hex([c.0, c.1, c.2]);
    if c.3 == 255 {
        s
    } else {
        format!("{s}{:02x}", c.3)
    }
}

fn parse_rgba(s: &str) -> Option<Rgba> {
    match s.len() {
        6 => parse_hex(s).map(Rgba::from_rgb8),
        8 => {
            let [r, g, b] = parse_hex(&s[..6])?;
            let a = u8::from_str_radix(s.get(6..)?, 16).ok()?;
            Some(Rgba(r, g, b, a))
        }
        _ => None,
    }
}

fn parse_f4(s: &str) -> Option<[f32; 4]> {
    let v: Vec<f32> = s
        .split(',')
        .map(|x| x.parse().ok().filter(|v: &f32| v.is_finite()))
        .collect::<Option<_>>()?;
    v.try_into().ok()
}

/// Das Schema als Einstellungsdatei: nur die Abweichungen von seinem Grundschema.
pub fn write(theme: &Theme) -> String {
    let mut out = format!("{HEAD} {VERSION}\n");
    let mut b = base(&theme.name).unwrap_or_else(Theme::dark);
    let mut t = theme.clone();
    Line::new("theme").text("base", &b.name).finish(&mut out);
    // Der Akzent zuerst: beim Lesen ziehen die Rollen, die ihm folgen, mit
    // (`Theme::set_accent`); geschrieben wird nur, was davon noch abweicht.
    if t.ui.accent != b.ui.accent {
        Line::new("color")
            .word("role", "ui.accent")
            .word("value", &rgba_hex(t.ui.accent))
            .finish(&mut out);
        b.set_accent(t.ui.accent);
    }
    for (name, _, f) in RGBA_ROLES {
        let v = *f(&mut t);
        if v != *f(&mut b) {
            Line::new("color")
                .word("role", name)
                .word("value", &rgba_hex(v))
                .finish(&mut out);
        }
    }
    for (name, _, f) in F4_ROLES {
        let v = *f(&mut t);
        if v != *f(&mut b) {
            let s = v.map(|x| x.to_string()).join(",");
            Line::new("color")
                .word("role", name)
                .word("value", &s)
                .finish(&mut out);
        }
    }
    for (name, _, f) in SIZE_ROLES {
        let v = *f(&mut t);
        if v != *f(&mut b) {
            Line::new("size")
                .word("key", name)
                .num("value", v)
                .finish(&mut out);
        }
    }
    if t.px_per_mm != b.px_per_mm {
        Line::new("screen")
            .num("px_per_mm", t.px_per_mm)
            .finish(&mut out);
    }
    if t.env.horizon_softness != b.env.horizon_softness {
        Line::new("env")
            .num("horizon_softness", t.env.horizon_softness)
            .finish(&mut out);
    }
    if t.env.ground_opacity != b.env.ground_opacity {
        Line::new("env")
            .num("ground_opacity", t.env.ground_opacity)
            .finish(&mut out);
    }
    if t.env.patterns_3d != b.env.patterns_3d {
        Line::new("env")
            .word("patterns_3d", if t.env.patterns_3d { "1" } else { "0" })
            .finish(&mut out);
    }
    if t.env.sky != b.env.sky {
        for (k, c) in &t.env.sky {
            Line::new("sky")
                .num("t", k)
                .word("value", &rgba_hex(*c))
                .finish(&mut out);
        }
    }
    out
}

/// Schema und Liste „Zuletzt geöffnet“ (E17, Abschnitt `[zuletzt]`, neueste
/// oben) als Einstellungsdatei.
pub fn write_all(theme: &Theme, recent: &Recent) -> String {
    let mut out = write(theme);
    for p in recent.paths() {
        Line::new("zuletzt")
            .text("datei", &p.to_string_lossy())
            .finish(&mut out);
    }
    out
}

/// Liest die Einstellungen. Nie ein Abbruch: was nicht passt, wird übersprungen
/// und als Hinweis gemeldet.
#[cfg(test)]
pub fn read(text: &str) -> (Theme, Vec<String>) {
    let (t, _, hints) = read_all(text);
    (t, hints)
}

/// Wie [`read`], dazu die Liste „Zuletzt geöffnet“.
pub fn read_all(text: &str) -> (Theme, Recent, Vec<String>) {
    let mut hints = Vec::new();
    let mut recent = Recent::default();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if let Err(e) = check_header(text.lines().next(), HEAD, VERSION) {
        hints.push(format!("Einstellungen nicht gelesen: {e}"));
        return (Theme::dark(), recent, hints);
    }
    let mut files = Vec::new();
    let mut t = Theme::dark();
    let mut sky = Vec::new();
    let mut changed = false;
    for (i, l) in text.lines().enumerate().skip(1) {
        let r = match Record::parse(i + 1, l) {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(e) => {
                hints.push(e.to_string());
                continue;
            }
        };
        let skip = |hints: &mut Vec<String>, what: &str| {
            hints.push(format!("Zeile {}: {what} übersprungen", r.line));
        };
        match r.section.as_str() {
            "theme" => match r.opt("base").map(|n| (n, base(n))) {
                Some((_, Some(b))) => t = b,
                Some((n, None)) => skip(&mut hints, &format!("unbekanntes Grundschema „{n}“")),
                None => skip(&mut hints, "[theme] ohne base"),
            },
            "color" => {
                let (role, value) = (r.opt("role").unwrap_or(""), r.opt("value").unwrap_or(""));
                if let Some((_, _, f)) = RGBA_ROLES.iter().find(|x| x.0 == role) {
                    match parse_rgba(value) {
                        Some(c) if role == "ui.accent" => t.set_accent(c),
                        Some(c) => *f(&mut t) = c,
                        None => skip(&mut hints, &format!("Farbe „{value}“ für {role}")),
                    }
                } else if let Some((_, _, f)) = F4_ROLES.iter().find(|x| x.0 == role) {
                    match parse_f4(value) {
                        Some(c) => *f(&mut t) = c,
                        None => skip(&mut hints, &format!("Farbe „{value}“ für {role}")),
                    }
                } else {
                    skip(&mut hints, &format!("unbekannte Rolle „{role}“"));
                }
            }
            "size" => {
                let key = r.opt("key").unwrap_or("");
                match (SIZE_ROLES.iter().find(|x| x.0 == key), r.f32("value")) {
                    (Some((_, _, f)), Ok(v)) => *f(&mut t) = v,
                    (None, _) => skip(&mut hints, &format!("unbekannte Größe „{key}“")),
                    (_, Err(_)) => skip(&mut hints, &format!("Wert für Größe „{key}“")),
                }
            }
            "screen" => match r.f32("px_per_mm") {
                Ok(v) if v > 0.0 => t.px_per_mm = v,
                _ => skip(&mut hints, "px_per_mm"),
            },
            "env" => {
                let mut any = false;
                if r.opt("horizon_softness").is_some() {
                    any = true;
                    match r.f32("horizon_softness") {
                        Ok(v) => t.env.horizon_softness = v,
                        Err(_) => skip(&mut hints, "horizon_softness"),
                    }
                }
                if r.opt("ground_opacity").is_some() {
                    any = true;
                    match r.f32("ground_opacity") {
                        Ok(v) if (0.0..=1.0).contains(&v) => t.env.ground_opacity = v,
                        _ => skip(&mut hints, "ground_opacity (0 bis 1)"),
                    }
                }
                if let Some(v) = r.opt("patterns_3d") {
                    any = true;
                    match v {
                        "1" => t.env.patterns_3d = true,
                        "0" => t.env.patterns_3d = false,
                        _ => skip(&mut hints, "patterns_3d (0 oder 1)"),
                    }
                }
                if !any {
                    skip(&mut hints, "[env] ohne Schlüssel");
                }
            }
            "sky" => match (r.f32("t"), r.opt("value").and_then(parse_rgba)) {
                (Ok(k), Some(c)) => sky.push((k, c)),
                _ => skip(&mut hints, "Stützstelle des Himmels"),
            },
            "zuletzt" => {
                match r.opt("datei").filter(|d| !d.is_empty()) {
                    Some(d) => files.push(PathBuf::from(d)),
                    None => skip(&mut hints, "[zuletzt] ohne Datei"),
                }
                r.unused(&mut hints);
                continue;
            }
            // Lage des Mengenfensters (F2): liest die App mit den Bildschirmen;
            // Ort des Firmenkatalogs (K2): liest [`Settings::load`]
            // Planung des letzten Projekts (Paket PD): liest [`Settings::load`]
            // Firmenvorgaben (Sonnenstand S8): liest [`Settings::load`]
            "mengenfenster" | "firmenkatalog" | "planung" | "lvblatt" | "ansichtsschatten"
            | "standardort" => continue,
            s => {
                // Bleibt beim Speichern erhalten ([`fremd`])
                skip(&mut hints, &format!("unbekannter Abschnitt [{s}]"));
                continue;
            }
        }
        changed = true;
        r.unused(&mut hints);
    }
    if !sky.is_empty() {
        sky.sort_by(|a, b| a.0.total_cmp(&b.0));
        t.env.sky = sky;
    }
    // Ein vom Standard abweichendes Schema bekommt einen eigenen Stand
    if changed && t != Theme::dark() {
        t.rev = 1;
    }
    // Neueste oben: von unten her eintragen
    for f in files.into_iter().rev() {
        recent.push(f);
    }
    (t, recent, hints)
}

/// Ob ein Satz eines bekannten Abschnitts gelesen wird: eine Farbrolle,
/// Größe oder Vorgabe aus einer neueren Fassung nicht, sie bleibt als
/// Fremdes.
fn satz_bekannt(abschnitt: &str, zeile: &str) -> bool {
    let Ok(Some(r)) = Record::parse(1, zeile) else {
        return true;
    };
    match abschnitt {
        "color" => {
            let role = r.opt("role").unwrap_or("");
            RGBA_ROLES.iter().any(|x| x.0 == role) || F4_ROLES.iter().any(|x| x.0 == role)
        }
        "size" => SIZE_ROLES.iter().any(|x| x.0 == r.opt("key").unwrap_or("")),
        "ansichtsschatten" | "standardort" => vorgaben::Vorgaben::satz_lesbar(zeile),
        _ => true,
    }
}

/// Ort der Einstellungsdatei und Stand beim Laden.
pub struct Settings {
    /// `None` mit `--ohne-einstellungen` oder `--screenshot` oder ohne `APPDATA`.
    pub path: Option<PathBuf>,
    loaded_rev: u64,
    /// Übersprungenes beim Lesen (später im Einstellungsfenster).
    pub hints: Vec<String>,
    /// Liste „Zuletzt geöffnet“ (E17) und ihr Stand beim Laden.
    pub recent: Recent,
    loaded_recent: Recent,
    /// Abschnitt `[mengenfenster]` (F2, [`crate::windows`]) und sein Stand beim Laden.
    pub windows: String,
    loaded_windows: String,
    /// Abschnitte `[baum]` und `[hinweise]` (Paket 4) und ihr Stand beim
    /// Laden.
    pub panel: String,
    loaded_panel: String,
    /// Abschnitt `[firmenkatalog] datei=…` (K2), unverändert weitergeschrieben.
    company_line: String,
    /// Abschnitt `[planung] name=… anschrift=…` (Paket PD §2): Planung des
    /// zuletzt gesetzten Projekts, für die Maske bei „Neu“; Stand beim Laden.
    planung: String,
    loaded_planung: String,
    /// Abschnitt `[lvblatt] titelblatt verzeichnis` (paket-projektdaten
    /// §5): Häkchen der AVA-Druckvorschau; leer, wenn beide aus sind.
    lvblatt: String,
    loaded_lvblatt: String,
    /// Ort des Firmenkatalogs aus der Datei; ohne ihn gilt der Vorgabeort.
    company: Option<PathBuf>,
    /// Firmenvorgaben (Sonnenstand S8) und ihr Stand beim Laden.
    pub vorgaben: vorgaben::Vorgaben,
    loaded_vorgaben: vorgaben::Vorgaben,
    /// Abschnitte und Schlüssel einer neueren Fassung, beim Speichern
    /// unverändert weitergeschrieben ([`fremd`]).
    fremd: fremd::Fremd,
}

impl Settings {
    pub fn new(args: impl Iterator<Item = String>, appdata: Option<PathBuf>) -> Settings {
        // Bildschirmfotos (Abnahme) hängen nie von der Datei des Nutzers ab
        let off = args
            .skip(1)
            .any(|a| a == "--ohne-einstellungen" || a == "--screenshot");
        Settings {
            path: appdata
                .filter(|_| !off)
                .map(|d| d.join("Skizzeo").join("einstellungen.txt")),
            loaded_rev: 0,
            hints: Vec::new(),
            recent: Recent::default(),
            loaded_recent: Recent::default(),
            windows: String::new(),
            loaded_windows: String::new(),
            panel: String::new(),
            loaded_panel: String::new(),
            company_line: String::new(),
            company: None,
            vorgaben: vorgaben::Vorgaben::WERK,
            loaded_vorgaben: vorgaben::Vorgaben::WERK,
            planung: String::new(),
            loaded_planung: String::new(),
            lvblatt: String::new(),
            loaded_lvblatt: String::new(),
            fremd: fremd::Fremd::default(),
        }
    }

    /// Planung (Name, Anschrift) des zuletzt gesetzten Projekts.
    pub fn planung(&self) -> Option<(String, String)> {
        let r = Record::parse(1, self.planung.trim_end()).ok().flatten()?;
        let name = r.opt("name").unwrap_or("").to_string();
        let anschrift = r.opt("anschrift").unwrap_or("").to_string();
        (!name.is_empty()).then_some((name, anschrift))
    }

    /// Merkt sich die Planung eines gesetzten Projekts; ohne Namen bleibt
    /// die vorige. Steht ab dem nächsten Speichern in der Datei.
    pub fn set_planung(&mut self, name: &str, anschrift: &str) {
        if name.is_empty() {
            return;
        }
        let mut line = String::new();
        Line::new("planung")
            .text("name", name)
            .text("anschrift", anschrift)
            .finish(&mut line);
        self.planung = line;
    }

    /// Druckvorschau: Titelblatt und Inhaltsverzeichnis.
    pub fn lv_blatt(&self) -> (bool, bool) {
        Record::parse(1, self.lvblatt.trim_end())
            .ok()
            .flatten()
            .map_or((false, false), |r| {
                let an = |k: &str| r.flag(k).unwrap_or(false);
                (an("titelblatt"), an("verzeichnis"))
            })
    }

    /// Merkt sich die Häkchen der Druckvorschau; ab dem nächsten Speichern
    /// in der Datei.
    pub fn set_lv_blatt(&mut self, (titelblatt, verzeichnis): (bool, bool)) {
        self.lvblatt.clear();
        if titelblatt || verzeichnis {
            Line::new("lvblatt")
                .flag("titelblatt", titelblatt)
                .flag("verzeichnis", verzeichnis)
                .finish(&mut self.lvblatt);
        }
    }

    /// Ort des Firmenkatalogs und ob es der Vorgabeort neben den
    /// Einstellungen ist (`%APPDATA%\Skizzeo\firmenkatalog.szk`). `None`
    /// ohne Einstellungen (Tests, Bildschirmfotos): dann gilt der eingebaute
    /// Startbestand.
    pub fn company_place(&self) -> Option<(PathBuf, bool)> {
        let dir = self.path.as_ref()?.parent()?;
        Some(match &self.company {
            Some(p) => (p.clone(), false),
            None => (dir.join(crate::catalog::FILE_NAME), true),
        })
    }

    /// Neuer Ort des Firmenkatalogs (K3, „ändern …“ im Bauteilkatalog, F1:
    /// frei wählbar, auch ein Netzlaufwerk). Steht ab dem nächsten
    /// Speichern der Einstellungen in der Datei.
    pub fn set_company_path(&mut self, p: PathBuf) {
        let mut line = String::new();
        Line::new("firmenkatalog")
            .text("datei", &p.to_string_lossy())
            .finish(&mut line);
        self.company_line = line;
        self.company = Some(p);
        // Erzwingt das Schreiben, auch wenn Schema und Liste gleich sind
        self.loaded_rev = u64::MAX;
    }

    /// Liest das Schema; fehlt die Datei, gilt das dunkle Standardschema.
    pub fn load(&mut self) -> Theme {
        let text = self
            .path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok());
        let theme = match text {
            Some(text) => {
                let (t, recent, mut hints) = read_all(&text);
                let (v, h) = vorgaben::Vorgaben::lesen(&text);
                hints.extend(h);
                self.hints = hints;
                self.vorgaben = v;
                self.fremd = fremd::Fremd::sammeln(&text, satz_bekannt);
                self.recent = recent;
                self.windows = text
                    .lines()
                    .filter(|l| l.starts_with("[mengenfenster]"))
                    .map(|l| format!("{l}\n"))
                    .collect();
                self.panel = text
                    .lines()
                    .filter(|l| l.starts_with("[baum]") || l.starts_with("[hinweise]"))
                    .map(|l| format!("{l}\n"))
                    .collect();
                self.company_line = text
                    .lines()
                    .filter(|l| l.starts_with("[firmenkatalog]"))
                    .map(|l| format!("{l}\n"))
                    .collect();
                self.planung = text
                    .lines()
                    .filter(|l| l.starts_with("[planung]"))
                    .take(1)
                    .map(|l| format!("{l}\n"))
                    .collect();
                self.lvblatt = text
                    .lines()
                    .filter(|l| l.starts_with("[lvblatt]"))
                    .take(1)
                    .map(|l| format!("{l}\n"))
                    .collect();
                self.company = text
                    .lines()
                    .enumerate()
                    .filter_map(|(i, l)| Record::parse(i + 1, l).ok().flatten())
                    .filter(|r| r.section == "firmenkatalog")
                    .find_map(|r| r.opt("datei").filter(|d| !d.is_empty()).map(PathBuf::from));
                t
            }
            None => Theme::dark(),
        };
        self.loaded_rev = theme.rev;
        self.loaded_recent = self.recent.clone();
        self.loaded_windows = self.windows.clone();
        self.loaded_panel = self.panel.clone();
        self.loaded_planung = self.planung.clone();
        self.loaded_lvblatt = self.lvblatt.clone();
        self.loaded_vorgaben = self.vorgaben;
        theme
    }

    /// Schreibt atomar, wenn sich Schema oder Liste „Zuletzt geöffnet“ seit
    /// dem Laden geändert haben.
    pub fn save_if_changed(&mut self, theme: &Theme) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if theme.rev == self.loaded_rev
            && self.recent == self.loaded_recent
            && self.windows == self.loaded_windows
            && self.panel == self.loaded_panel
            && self.planung == self.loaded_planung
            && self.lvblatt == self.loaded_lvblatt
            && self.vorgaben == self.loaded_vorgaben
        {
            return Ok(());
        }
        let tmp = path.with_extension("txt.tmp");
        let res = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| {
                let text = self.fremd.einsetzen(
                    &(write_all(theme, &self.recent)
                        + &self.windows
                        + &self.panel
                        + &self.company_line
                        + &self.planung
                        + &self.lvblatt
                        + &self.vorgaben.schreiben()),
                );
                crate::document::write_synced(&tmp, text.as_bytes())
            })
            .and_then(|_| std::fs::rename(&tmp, path));
        match res {
            Ok(()) => {
                self.loaded_rev = theme.rev;
                self.loaded_recent = self.recent.clone();
                self.loaded_windows = self.windows.clone();
                self.loaded_panel = self.panel.clone();
                self.loaded_planung = self.planung.clone();
                self.loaded_lvblatt = self.lvblatt.clone();
                self.loaded_vorgaben = self.vorgaben;
                Ok(())
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(format!("Einstellungen nicht gespeichert: {e}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("skizzeo-einst-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn args(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Abnahme S8: einstellungen.txt ab Werk bytegleich; eine vorhandene
    /// Datei ohne Vorgaben bleibt beim Speichern ohne Änderung, nach einer
    /// anderen Änderung kommen keine Vorgabezeilen dazu.
    #[test]
    fn test_abnahme_s8_einstellungen_bytegleich() {
        let d = dir("abn-s8");
        let p = d.join("Skizzeo");
        std::fs::create_dir_all(&p).unwrap();
        let path = p.join("einstellungen.txt");
        let alt = "SKIZZEO-EINSTELLUNGEN 1\n[theme] base=\"Dunkel\"\n[zuletzt] datei=\"b.szo\"\n";
        std::fs::write(&path, alt).unwrap();
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let t = s.load();
        assert!(s.hints.is_empty(), "{:?}", s.hints);
        assert_eq!(s.vorgaben, vorgaben::Vorgaben::WERK);
        s.save_if_changed(&t).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), alt);
        s.recent.push(PathBuf::from("a.szo"));
        s.save_if_changed(&t).unwrap();
        let neu = std::fs::read_to_string(&path).unwrap();
        assert!(
            !neu.contains("[ansichtsschatten]") && !neu.contains("[standardort]"),
            "{neu}"
        );
        // Ohne Datei: nichts geschrieben
        let d2 = dir("abn-s8-ohne");
        let mut s2 = Settings::new(args(&["skizzeo.exe"]), Some(d2.clone()));
        let t2 = s2.load();
        s2.save_if_changed(&t2).unwrap();
        assert!(!d2.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn standard_ohne_datei() {
        let d = dir("ohne");
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        assert_eq!(s.load(), Theme::dark());
        assert!(s.hints.is_empty());
        // Unverändert: nichts geschrieben
        s.save_if_changed(&Theme::dark()).unwrap();
        assert!(!d.exists());
        assert_eq!(
            write(&Theme::dark()),
            "SKIZZEO-EINSTELLUNGEN 1\n[theme] base=\"Dunkel\"\n"
        );
    }

    /// Sonnenstand S8: Firmenvorgaben im Rundlauf, einmal geschrieben,
    /// ab Werk keine Zeile.
    #[test]
    fn firmenvorgaben_im_rundlauf() {
        let d = dir("vorgaben");
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let t = s.load();
        s.vorgaben.schatten.hatch = true;
        s.vorgaben.ort.breite = 48.137;
        s.save_if_changed(&t).unwrap();
        let path = d.join("Skizzeo").join("einstellungen.txt");
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "SKIZZEO-EINSTELLUNGEN 1\n[theme] base=\"Dunkel\"\n\
             [ansichtsschatten] on=1 fill=hatch light=front-left\n\
             [standardort] lat=48.137 lon=8.591\n"
        );
        let mut s2 = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let t2 = s2.load();
        assert!(s2.hints.is_empty(), "{:?}", s2.hints);
        assert_eq!(s2.vorgaben, s.vorgaben);
        // Unverändert: nichts geschrieben; geändert: kein Abschnitt doppelt
        s2.recent.push(PathBuf::from("a.szo"));
        s2.save_if_changed(&t2).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("[ansichtsschatten]").count(), 1, "{text}");
        assert_eq!(text.matches("[standardort]").count(), 1, "{text}");
        // Zurück auf Werk: die Zeilen entfallen
        s2.vorgaben = vorgaben::Vorgaben::WERK;
        s2.save_if_changed(&t2).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("[ansichtsschatten]") && !text.contains("[standardort]"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Review 3cd: Eine Vorgabe einer späteren Fassung (Licht „west“)
    /// gilt als Werk, bleibt aber beim Speichern stehen, hinter der eigenen.
    #[test]
    fn unlesbare_vorgabe_bleibt_stehen() {
        let d = dir("vorgabe-fremd");
        let path = d.join("Skizzeo").join("einstellungen.txt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let fremd = "[ansichtsschatten] on=1 fill=area light=west";
        std::fs::write(
            &path,
            format!("SKIZZEO-EINSTELLUNGEN 1\n[theme] base=\"Dunkel\"\n{fremd}\n"),
        )
        .unwrap();
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let t = s.load();
        assert_eq!(s.vorgaben, vorgaben::Vorgaben::WERK);
        assert_eq!(s.hints.len(), 1, "{:?}", s.hints);
        s.vorgaben.schatten.hatch = true;
        s.save_if_changed(&t).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            format!(
                "SKIZZEO-EINSTELLUNGEN 1\n[theme] base=\"Dunkel\"\n\
                 [ansichtsschatten] on=1 fill=hatch light=front-left\n{fremd}\n"
            )
        );
        let mut s2 = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        s2.load();
        assert_eq!(s2.vorgaben, s.vorgaben);
        assert_eq!(
            s2.hints,
            ["Zeile 4: [ansichtsschatten] unlesbar, es gilt Zeile 3"]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nur_die_abweichung_wird_gespeichert() {
        let d = dir("abweichung");
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let mut t = s.load();
        t.ui.border = Rgba::rgb(40, 120, 220);
        t.rev += 1;
        s.save_if_changed(&t).unwrap();
        let path = d.join("Skizzeo").join("einstellungen.txt");
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "SKIZZEO-EINSTELLUNGEN 1\n[theme] base=\"Dunkel\"\n[color] role=ui.border value=2878dc\n"
        );
        assert!(!path.with_extension("txt.tmp").exists());
        let mut s2 = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let back = s2.load();
        assert!(s2.hints.is_empty(), "{:?}", s2.hints);
        assert_eq!(Theme { rev: t.rev, ..back }, t);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn alle_arten_im_rundlauf() {
        let mut t = Theme::dark();
        t.set_accent(Rgba::rgb(40, 120, 220));
        t.ui.shadow = Rgba(0, 0, 0, 30);
        t.interact.track = [0.1, 0.2, 0.3, 0.75];
        t.size.font = 15.5;
        t.size.field_height = 28.0;
        t.ui.field_invalid = Rgba::rgb(200, 10, 10);
        t.px_per_mm = 6.0;
        t.env.horizon_softness = 2.0;
        t.env.ground_opacity = 0.3;
        t.env.sky = vec![(0.0, Rgba::rgb(1, 2, 3)), (1.0, Rgba(4, 5, 6, 7))];
        let text = write(&t);
        assert!(
            text.contains("[color] role=ui.shadow value=0000001e\n"),
            "{text}"
        );
        assert!(
            text.contains("[color] role=interact.track value=0.1,0.2,0.3,0.75\n"),
            "{text}"
        );
        assert!(
            text.contains("[color] role=ui.field_invalid value=c80a0a\n"),
            "{text}"
        );
        // E11: Deckkraft des Bodens; außerhalb 0 … 1 übersprungen
        assert!(text.contains("[env] ground_opacity=0.3\n"), "{text}");
        let (back, hints) = read(&text);
        assert!(hints.is_empty(), "{hints:?}");
        assert_eq!(write(&back), text);
        assert_eq!(Theme { rev: t.rev, ..back }, t);
        let (bad, hints) = read(&format!("{HEAD} {VERSION}\n[env] ground_opacity=2\n"));
        assert_eq!(bad.env.ground_opacity, 0.6);
        assert_eq!(hints.len(), 1, "{hints:?}");
    }

    #[test]
    fn kaputtes_wird_uebersprungen() {
        let text = "SKIZZEO-EINSTELLUNGEN 1\n\
            [theme] base=\"Dunkel\"\n\
            [color] role=ui.gibtsnicht value=ffffff\n\
            [color] role=ui.accent value=zz00zz\n\
            [color] role=ui.border value=2878dc\n";
        let (t, hints) = read(text);
        assert_eq!(hints.len(), 2, "{hints:?}");
        assert!(hints[0].contains("Zeile 3"), "{hints:?}");
        assert!(hints[1].contains("Zeile 4"), "{hints:?}");
        assert_eq!(t.ui.border, Rgba::rgb(40, 120, 220));
        assert_eq!(t.ui.accent, Theme::dark().ui.accent);
        // Fremde Datei oder neuere Version: Standardschema und ein Hinweis
        let (t, hints) = read("SKIZZEO-EINSTELLUNGEN 9\n[color] role=ui.border value=000000\n");
        assert_eq!(t, Theme::dark());
        assert_eq!(hints.len(), 1);
        let (t, hints) = read("");
        assert_eq!((t, hints.len()), (Theme::dark(), 1));
    }

    #[test]
    fn ohne_einstellungen_wird_nichts_gelesen_oder_geschrieben() {
        let d = dir("aus");
        let path = d.join("Skizzeo").join("einstellungen.txt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "SKIZZEO-EINSTELLUNGEN 1\n[size] key=font value=20\n").unwrap();
        let mut s = Settings::new(
            args(&["skizzeo.exe", "--ohne-einstellungen"]),
            Some(d.clone()),
        );
        assert!(s.path.is_none());
        let shot = Settings::new(
            args(&["skizzeo.exe", "--screenshot", "a.png"]),
            Some(d.clone()),
        );
        assert!(shot.path.is_none());
        let mut t = s.load();
        assert_eq!(t, Theme::dark());
        t.set_accent(Rgba::rgb(1, 2, 3));
        s.save_if_changed(&t).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, "SKIZZEO-EINSTELLUNGEN 1\n[size] key=font value=20\n");
        // Ohne den Schalter wird die Datei gelesen
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        assert_eq!(s.load().size.font, 20.0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn firmenkatalog_ort_bleibt_erhalten() {
        let d = dir("firma");
        let path = d.join("Skizzeo").join("einstellungen.txt");
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        assert_eq!(
            s.company_place(),
            Some((d.join("Skizzeo").join("firmenkatalog.szk"), true))
        );
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "SKIZZEO-EINSTELLUNGEN 1\n[firmenkatalog] datei=\"N:\\\\Buero\\\\firma.szk\"\n",
        )
        .unwrap();
        let mut t = s.load();
        assert!(s.hints.is_empty(), "{:?}", s.hints);
        assert_eq!(
            s.company_place(),
            Some((PathBuf::from("N:\\Buero\\firma.szk"), false))
        );
        t.set_accent(Rgba::rgb(1, 2, 3));
        s.save_if_changed(&t).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[firmenkatalog] datei=\"N:"), "{text}");
        // K3: „ändern …“ im Bauteilkatalog; geschrieben wird auch ohne
        // geändertes Schema, gelesen kommt der neue Ort zurück
        s.set_company_path(PathBuf::from("M:\\Vorlagen\\büro.szk"));
        s.save_if_changed(&t).unwrap();
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        s.load();
        assert_eq!(
            s.company_place(),
            Some((PathBuf::from("M:\\Vorlagen\\büro.szk"), false))
        );
        assert!(Settings::new(
            args(&["skizzeo.exe", "--ohne-einstellungen"]),
            Some(d.clone())
        )
        .company_place()
        .is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Abnahme 79e7123 (Test, Koordinator 00:28): Eine Einstellungsdatei
    /// von vor der Rolle `ui.sheet_success` lädt ohne Hinweis, die neue
    /// Rolle hat ihren Grundwert, alle gespeicherten Abweichungen bleiben,
    /// und Speichern schreibt die Datei bytegleich zurück.
    #[test]
    fn abnahme_alte_datei_ohne_sheet_success() {
        let alt = "SKIZZEO-EINSTELLUNGEN 1\n\
            [theme] base=\"Dunkel\"\n\
            [color] role=ui.accent value=2878dc\n\
            [color] role=ui.border value=102030\n\
            [color] role=ui.sheet_card value=fafafa\n\
            [color] role=ui.text_same value=00aa00\n\
            [size] key=font value=15\n\
            [zuletzt] datei=\"C:\\\\Haus.szo\"\n";
        let (t, recent, hints) = read_all(alt);
        assert!(hints.is_empty(), "{hints:?}");
        assert_eq!(t.ui.sheet_success, Theme::dark().ui.sheet_success);
        assert_eq!(t.ui.border, Rgba::rgb(16, 32, 48));
        assert_eq!(t.ui.text_same, Rgba::rgb(0, 170, 0));
        assert_eq!(t.ui.sheet_card, Rgba::rgb(250, 250, 250));
        assert_eq!(t.size.font, 15.0);
        assert_eq!(write_all(&t, &recent), alt, "bytegleich zurück");
        // Auch über die Datei wie beim Start
        let d = dir("alt-sheet-success");
        let path = d.join("Skizzeo").join("einstellungen.txt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, alt).unwrap();
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let geladen = s.load();
        assert!(s.hints.is_empty(), "{:?}", s.hints);
        assert_eq!(geladen.ui.sheet_success, Theme::dark().ui.sheet_success);
        s.save_if_changed(&geladen).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), alt);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Häkchen der AVA-Druckvorschau (paket-projektdaten §5): Arbeitsplatz,
    /// über das Speichern und Laden; beide aus schreibt keine Zeile.
    #[test]
    fn lv_blatt_haekchen() {
        let d = dir("lvblatt");
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let t = s.load();
        assert_eq!(s.lv_blatt(), (false, false));
        s.set_lv_blatt((true, false));
        assert_eq!(s.lv_blatt(), (true, false));
        s.save_if_changed(&t).unwrap();
        let path = d.join("Skizzeo").join("einstellungen.txt");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("[lvblatt] titelblatt=1 verzeichnis=0"),
            "{text}"
        );
        let mut neu = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let t = neu.load();
        assert!(neu.hints.is_empty(), "{:?}", neu.hints);
        assert_eq!(neu.lv_blatt(), (true, false));
        neu.set_lv_blatt((false, false));
        neu.save_if_changed(&t).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("lvblatt"), "{text}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Datenverlust-Verdacht (Koordinator 09.10.): Eine neuere Fassung
    /// schreibt Abschnitte und Schlüssel, die diese nicht kennt. Nach dem
    /// Laden, einer Änderung und dem Speichern stehen sie im Wortlaut und
    /// in ihrer Reihenfolge da; ändert die App die Zeile selbst, bleiben
    /// die unbekannten Schlüssel an ihr.
    #[test]
    fn fremdes_bleibt_beim_speichern() {
        let d = dir("fremd");
        let path = d.join("Skizzeo").join("einstellungen.txt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let fremd = [
            "[zukunft] vorgabe=\"Firma \\\"Muster\\\" GmbH\" stufe=3",
            "[color] role=ui.zukunft value=ff0000",
            "[zukunft] zweite=1",
        ];
        let alt = format!(
            "{HEAD} {VERSION}\n[theme] base=dark\n{}\n[env] ground_opacity=0.3 nebel=0.5\n\
             [zuletzt] datei=\"C:\\\\a.szo\" stern=1\n\
             [lvblatt] titelblatt=1 verzeichnis=0 farbe=blau\n{}\n{}\n",
            fremd[0], fremd[1], fremd[2]
        );
        std::fs::write(&path, &alt).unwrap();
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let t = s.load();
        assert_eq!(s.lv_blatt(), (true, false));
        assert_eq!(t.env.ground_opacity, 0.3);
        // Eine andere Einstellung ändern: Planung merken
        s.set_planung("Planer", "Weg 1");
        s.save_if_changed(&t).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let zeilen: Vec<&str> = text.lines().collect();
        let lage: Vec<usize> = fremd
            .iter()
            .map(|f| {
                zeilen
                    .iter()
                    .position(|z| z == f)
                    .unwrap_or_else(|| panic!("{f} fehlt:\n{text}"))
            })
            .collect();
        assert!(lage.windows(2).all(|w| w[0] < w[1]), "Reihenfolge: {text}");
        for z in [
            "[env] ground_opacity=0.3 nebel=0.5",
            "[zuletzt] datei=\"C:\\\\a.szo\" stern=1",
            "[lvblatt] titelblatt=1 verzeichnis=0 farbe=blau",
        ] {
            assert_eq!(
                zeilen.iter().filter(|x| **x == z).count(),
                1,
                "{z}:\n{text}"
            );
        }
        assert!(text.contains("[planung] name=\"Planer\""), "{text}");
        // Die App ändert die Zeile selbst: die unbekannten Schlüssel bleiben
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let t = s.load();
        s.set_lv_blatt((false, true));
        s.save_if_changed(&t).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("[lvblatt] titelblatt=0 verzeichnis=1 farbe=blau\n"),
            "{text}"
        );
        // Beide aus: keine Häkchen, der fremde Schlüssel bleibt
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let t = s.load();
        assert_eq!(s.lv_blatt(), (false, true));
        s.set_lv_blatt((false, false));
        s.save_if_changed(&t).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[lvblatt] farbe=blau\n"), "{text}");
        for f in fremd {
            assert_eq!(text.matches(f).count(), 1, "{f}:\n{text}");
        }
        // Nur der fremde Schlüssel: bleibt einmal, auch nach dem nächsten Speichern
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let t = s.load();
        assert_eq!(s.lv_blatt(), (false, false));
        s.recent.push(PathBuf::from("C:\\b.szo"));
        s.save_if_changed(&t).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("[lvblatt]").count(), 1, "{text}");
        assert!(text.contains("[lvblatt] farbe=blau\n"), "{text}");
        let _ = std::fs::remove_dir_all(&d);
    }
}

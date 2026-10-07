//! Bausteine der Paneele: Schriften, Paneelfläche, Knöpfe, Text.
//! Gestaltung nach Jörns Vorlage (dunkles Paneel, gelber Akzent).

use crate::theme::Theme;
use sk_paint::{font::Font, Canvas, Path, Rgba};

/// Rechteck in Pixeln.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        let (x, y) = (x as f32, y as f32);
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Schriften aus dem System (eigener TrueType-Leser). Fehlt eine, bleibt Text weg.
pub struct Fonts {
    pub regular: Option<Font>,
    pub bold: Option<Font>,
    /// Kursiv (Kontrollzeilen der Mengenliste); fehlt die Datei, gilt `regular`.
    pub italic: Option<Font>,
}

impl Fonts {
    pub fn system() -> Fonts {
        Fonts {
            regular: Font::system(&["segoeui.ttf", "arial.ttf", "tahoma.ttf"]),
            bold: Font::system(&["seguisb.ttf", "segoeuib.ttf", "arialbd.ttf", "tahomabd.ttf"]),
            italic: Font::system(&["segoeuii.ttf", "ariali.ttf"]),
        }
    }
}

/// Paneelfläche mit weichem Schatten und feinem Rand. `r` ist die Fläche ohne Schatten.
///
/// Schatten und Rand werden nur als Ringe gefüllt: Ihr Inneres wird ohnehin von
/// der deckenden Fläche darüber verdeckt. Das Loch liegt [`HIDDEN_INSET`] Pixel
/// innerhalb dieser Fläche, damit die geglätteten Kanten genau gleich aussehen.
pub fn panel(c: &mut Canvas, r: Rect, s: f32, t: &Theme) {
    panel_filled(c, r, s, t, t.ui.bg);
}

/// Wie [`panel`] mit eigener Fläche (etwa `menu_bg`).
pub fn panel_filled(c: &mut Canvas, r: Rect, s: f32, t: &Theme, fill: Rgba) {
    let rad = t.size.corner_radius * s;
    let b = s.round().max(1.0);
    // Deckend überdeckte Bereiche: unter dem Rand bzw. unter der Füllung
    let hole = |p: &mut Path, x: f32, y: f32, w: f32, h: f32, r: f32| {
        let i = HIDDEN_INSET;
        if w > 2.0 * i && h > 2.0 * i {
            p.rounded_rect_hole(x + i, y + i, w - 2.0 * i, h - 2.0 * i, (r - i).max(0.0));
        }
    };
    for i in 1..=4 {
        let d = i as f32 * 2.0 * s;
        let mut p = Path::new();
        p.rounded_rect(r.x - d * 0.5, r.y - d * 0.25, r.w + d, r.h + d, rad + d);
        hole(&mut p, r.x, r.y, r.w, r.h, rad);
        c.fill(&p, t.ui.shadow);
    }
    let (ix, iy, iw, ih, ir) = (r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    hole(&mut p, ix, iy, iw, ih, ir);
    c.fill(&p, t.ui.border);
    let mut p = Path::new();
    p.rounded_rect(ix, iy, iw, ih, ir);
    c.fill(&p, fill);
}

/// Hinweis an der Maus: Text auf dunklem Grund mit Rand. Liefert das Bild
/// (Pixel).
pub fn tooltip(fonts: &Fonts, text: &str, s: f32, t: &Theme) -> Canvas {
    let px = t.size.font_small * s;
    let f = fonts.regular.as_ref();
    // Mehrzeilig mit „\n“; je Zeile 18 dip mehr. Dann ist Zeile 1 die
    // fette Überschrift, die übrigen sind gedämpft (wie E18).
    let lines: Vec<&str> = text.split('\n').collect();
    let multi = lines.len() > 1;
    let font = |i: usize| {
        if multi && i == 0 {
            fonts.bold.as_ref().or(f)
        } else {
            f
        }
    };
    let tw = lines
        .iter()
        .enumerate()
        .map(|(i, l)| font(i).map_or(0.0, |f| f.width(l, px)))
        .fold(0.0, f32::max);
    let line = (18.0 * s).round();
    let (pad, h) = (
        (8.0 * s).round(),
        (24.0 * s).round() + line * (lines.len() - 1) as f32,
    );
    let (w, b) = ((tw + 2.0 * pad).ceil(), s.round().max(1.0));
    let mut c = Canvas::new(w as usize, h as usize);
    let rad = 4.0 * s;
    let mut p = Path::new();
    p.rounded_rect(0.0, 0.0, w, h, rad);
    c.fill(&p, t.ui.border);
    let mut p = Path::new();
    p.rounded_rect(b, b, w - 2.0 * b, h - 2.0 * b, rad - b);
    c.fill(&p, t.ui.tooltip_bg);
    if let Some(f0) = f {
        let y0 = ((24.0 * s + f0.cap_height(px)) * 0.5).round();
        for (i, l) in lines.iter().enumerate() {
            let y = y0 + line * i as f32;
            let col = match (multi, i) {
                (false, _) => t.ui.tooltip_text,
                (true, 0) => t.ui.text,
                _ => t.ui.text_dim,
            };
            font(i).unwrap_or(f0).draw(&mut c, l, px, pad, y, col);
        }
    }
    c
}

/// Unaufdringlicher Hinweis in der Statuszeile (F-17): gedämpfte Schrift
/// mit einem Akzentpunkt davor, auf Paneelgrund mit feinem Rand.
pub fn notice(fonts: &Fonts, text: &str, s: f32, t: &Theme) -> Canvas {
    let px = t.size.font_small * s;
    let f = fonts.regular.as_ref();
    let tw = f.map_or(0.0, |f| f.width(text, px));
    let (pad, h, dot) = ((10.0 * s).round(), (26.0 * s).round(), 6.0 * s);
    let (w, b) = ((tw + 2.0 * pad + dot + 8.0 * s).ceil(), s.round().max(1.0));
    let mut c = Canvas::new(w as usize, h as usize);
    let rad = h * 0.5;
    let mut p = Path::new();
    p.rounded_rect(0.0, 0.0, w, h, rad);
    c.fill(&p, t.ui.border);
    let mut p = Path::new();
    p.rounded_rect(b, b, w - 2.0 * b, h - 2.0 * b, rad - b);
    c.fill(&p, t.ui.bg);
    let mut p = Path::new();
    p.rounded_rect(pad, (h - dot) * 0.5, dot, dot, dot * 0.5);
    c.fill(&p, t.ui.accent);
    if let Some(f) = f {
        let y = ((h + f.cap_height(px)) * 0.5).round();
        f.draw(&mut c, text, px, pad + dot + 8.0 * s, y, t.ui.text_dim);
    }
    c
}

/// Abstand des ausgesparten Lochs vom Rand der deckenden Fläche darüber (Pixel).
const HIDDEN_INSET: f32 = 2.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct ButtonState {
    pub hover: bool,
    pub pressed: bool,
    /// Eingeschaltet / ausgewählt: gelb gefüllt.
    pub active: bool,
    /// Gesperrt: blasse Schrift, keine Reaktion auf die Maus.
    pub disabled: bool,
}

/// Knopf mit zentrierter Beschriftung.
pub fn button(
    c: &mut Canvas,
    fonts: &Fonts,
    r: Rect,
    label: &str,
    st: ButtonState,
    s: f32,
    t: &Theme,
) {
    let u = &t.ui;
    let rad = 6.0 * s;
    let b = s.round().max(1.0);
    let (fill, border, text) = if st.disabled {
        let b = u.border;
        (u.bg, Rgba(b.0, b.1, b.2, b.3 / 2), u.text_disabled)
    } else if st.active {
        let f = if st.hover { u.accent_hover } else { u.accent };
        (f, f, u.on_accent)
    } else if st.pressed {
        (u.pressed, u.border, u.text)
    } else if st.hover {
        (u.hover, u.border, u.text)
    } else {
        (u.bg, u.border, u.text)
    };
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    c.fill(&p, border);
    let mut p = Path::new();
    p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
    c.fill(&p, fill);
    if let Some(f) = fonts.bold.as_ref().or(fonts.regular.as_ref()) {
        let px = t.size.font * s;
        let x = r.x + (r.w - f.width(label, px)) * 0.5;
        let y = r.y + (r.h + f.cap_height(px)) * 0.5;
        f.draw(c, label, px, x.round(), y.round(), text);
    }
}

/// Zustand eines Zahlenfelds beim Zeichnen.
#[derive(Clone, Copy, Debug, Default)]
pub struct FieldState<'a> {
    /// Zahl, wie sie im Feld steht (beim Eingeben der getippte Text).
    pub text: &'a str,
    /// Einheit rechtsbündig hinter der Zahl, z. B. „cm“.
    pub unit: &'a str,
    pub hover: bool,
    /// Eingabemodus: Rahmen in `field_focus`, Schreibmarke und Markierung.
    pub focus: bool,
    pub invalid: bool,
    /// Schreibmarke als Byte-Stelle in `text`.
    pub caret: Option<usize>,
    /// Markierter Bereich (Byte-Stellen, von < bis).
    pub select: Option<(usize, usize)>,
}

/// Zahlenfeld: Grund, Rahmen, Zahl rechtsbündig vor der Einheit.
pub fn field(c: &mut Canvas, fonts: &Fonts, r: Rect, st: &FieldState, s: f32, t: &Theme) {
    field_frame(c, r, st, s, t);
    let Some(f) = fonts.regular.as_ref() else {
        return;
    };
    let px = t.size.font_small * s;
    let pad = t.size.field_pad * s;
    let unit_w = f.width(st.unit, px);
    let unit_x = r.x + r.w - pad - unit_w;
    let base = (r.y + (r.h + f.cap_height(px)) * 0.5).round();
    f.draw(c, st.unit, px, unit_x.round(), base, t.ui.field_unit);
    let gap = if st.unit.is_empty() { 0.0 } else { 4.0 * s };
    let num_x = unit_x - gap - f.width(st.text, px);
    field_text(c, f, r, st, num_x, s, t);
}

/// Textfeld (E5): wie das Zahlenfeld, Text linksbündig, ohne Einheit.
pub fn text_field(c: &mut Canvas, fonts: &Fonts, r: Rect, st: &FieldState, s: f32, t: &Theme) {
    field_frame(c, r, st, s, t);
    if let Some(f) = fonts.regular.as_ref() {
        let x = r.x + t.size.field_pad * s;
        field_text(c, f, r, st, x, s, t);
    }
}

/// Stelle im Text (Byte) unter `x` für ein Feld, dessen Text bei `text_x`
/// beginnt (Klick setzt die Schreibmarke).
pub fn caret_at(font: Option<&Font>, text: &str, px: f32, text_x: f32, x: f32) -> usize {
    let Some(f) = font else {
        return text.len();
    };
    let mut best = (0, f32::MAX);
    for (i, _) in text.char_indices().chain([(text.len(), ' ')]) {
        let d = (text_x + f.width(&text[..i], px) - x).abs();
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

/// Anfang des Texts in einem linksbündigen Textfeld (für [`caret_at`]).
pub fn text_field_x(r: Rect, s: f32, t: &Theme) -> f32 {
    r.x + t.size.field_pad * s
}

/// Anfang der Zahl in einem Zahlenfeld (für [`caret_at`]).
pub fn field_x(fonts: &Fonts, r: Rect, text: &str, unit: &str, s: f32, t: &Theme) -> f32 {
    let Some(f) = fonts.regular.as_ref() else {
        return r.x;
    };
    let px = t.size.font_small * s;
    let unit_x = r.x + r.w - t.size.field_pad * s - f.width(unit, px);
    let gap = if unit.is_empty() { 0.0 } else { 4.0 * s };
    unit_x - gap - f.width(text, px)
}

fn field_frame(c: &mut Canvas, r: Rect, st: &FieldState, s: f32, t: &Theme) {
    let u = &t.ui;
    let rad = 4.0 * s;
    let b = s.round().max(1.0);
    let border = if st.invalid {
        u.field_invalid
    } else if st.focus {
        u.field_focus
    } else {
        u.field_border
    };
    let fill = if st.hover && !st.focus {
        u.field_hover
    } else {
        u.field
    };
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    c.fill(&p, border);
    let mut p = Path::new();
    p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
    c.fill(&p, fill);
}

/// Text, Markierung und Schreibmarke eines Felds ab `x`.
fn field_text(c: &mut Canvas, f: &Font, r: Rect, st: &FieldState, x: f32, s: f32, t: &Theme) {
    let u = &t.ui;
    let px = t.size.font_small * s;
    let b = s.round().max(1.0);
    let base = (r.y + (r.h + f.cap_height(px)) * 0.5).round();
    let at = |i: usize| x + f.width(&st.text[..i.min(st.text.len())], px);
    if let Some((a, z)) = st.select.filter(|(a, z)| a < z) {
        let (x0, x1) = (at(a), at(z));
        c.fill_rect(x0, r.y + 4.0 * s, x1 - x0, r.h - 8.0 * s, u.text_select);
    }
    f.draw(c, st.text, px, x.round(), base, u.field_text);
    if let Some(i) = st.caret.filter(|_| st.focus) {
        c.fill_rect(at(i).round(), r.y + 5.0 * s, b, r.h - 10.0 * s, u.caret);
    }
}

/// Text auf `max_w` Pixel gekürzt, mit „…“ am Ende.
pub fn ellipsize(font: Option<&Font>, text: &str, px: f32, max_w: f32) -> String {
    let Some(f) = font else {
        return text.into();
    };
    if f.width(text, px) <= max_w {
        return text.into();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let mut probe = out.clone();
        probe.push(ch);
        probe.push('…');
        if f.width(&probe, px) > max_w {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

/// Text in Zeilen von höchstens `max_w` Pixeln, umbrochen an Leerzeichen;
/// ein einzelnes zu langes Wort wird gekürzt.
pub fn wrap(font: Option<&Font>, text: &str, px: f32, max_w: f32) -> Vec<String> {
    let Some(f) = font else {
        return vec![text.into()];
    };
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let probe = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if f.width(&probe, px) <= max_w || line.is_empty() {
            line = probe;
        } else {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
        .into_iter()
        .map(|l| ellipsize(Some(f), &l, px, max_w))
        .collect()
}

/// Farbfeld: Rechteck in der Farbe mit feinem Rand.
pub fn swatch(c: &mut Canvas, r: Rect, color: Rgba, hover: bool, s: f32, t: &Theme) {
    let b = s.round().max(1.0);
    let edge = if hover { t.ui.text_dim } else { t.ui.border };
    c.fill_rect(r.x, r.y, r.w, r.h, edge);
    c.fill_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, color);
}

/// Kontrollkästchen: Rahmen, eingeschaltet Akzentfläche mit Haken.
pub fn checkbox(c: &mut Canvas, r: Rect, on: bool, hover: bool, s: f32, t: &Theme) {
    let u = &t.ui;
    let b = s.round().max(1.0);
    let rad = 3.0 * s;
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    let edge = if on {
        u.accent
    } else if hover {
        u.field_focus
    } else {
        u.field_border
    };
    c.fill(&p, edge);
    let mut p = Path::new();
    p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
    c.fill(&p, if on { u.accent } else { u.field });
    if on {
        let w = 2.0 * s;
        let (x, y, k) = (r.x, r.y, r.w / 16.0);
        let mut p = Path::new();
        p.segment((x + 3.5 * k, y + 8.5 * k), (x + 6.5 * k, y + 11.5 * k), w);
        p.segment((x + 6.5 * k, y + 11.5 * k), (x + 12.5 * k, y + 4.5 * k), w);
        c.fill(&p, u.on_accent);
    }
}

/// Auswahlliste (zu): Feld mit Text links und ▾ rechts.
#[allow(clippy::too_many_arguments)]
pub fn combo(
    c: &mut Canvas,
    fonts: &Fonts,
    r: Rect,
    text: &str,
    hover: bool,
    open: bool,
    s: f32,
    t: &Theme,
) {
    combo_icon(c, fonts, r, text, None, hover, open, s, t);
}

/// Auswahlliste mit Bildchen (Farbfeld, Kachel) vor dem Text.
#[allow(clippy::too_many_arguments)]
pub fn combo_icon(
    c: &mut Canvas,
    fonts: &Fonts,
    r: Rect,
    text: &str,
    icon: Option<&Canvas>,
    hover: bool,
    open: bool,
    s: f32,
    t: &Theme,
) {
    let st = FieldState {
        hover,
        focus: open,
        ..FieldState::default()
    };
    field_frame(c, r, &st, s, t);
    let f = fonts.regular.as_ref();
    let px = t.size.font_small * s;
    let cap = f.map_or(px * 0.7, |f| f.cap_height(px));
    let base = r.y + (r.h + cap) * 0.5;
    let mut x = r.x + t.size.field_pad * s;
    if let Some(icon) = icon {
        let y = r.y + (r.h - icon.height as f32) * 0.5;
        c.blit(icon, x as i32, y as i32);
        x += icon.width as f32 + 8.0 * s;
    }
    let max = r.x + r.w - 24.0 * s - x;
    let text = ellipsize(f, text, px, max);
    text_at(c, f, &text, px, x, base, t.ui.field_text);
    let (cx, cy, d) = (r.x + r.w - 12.0 * s, r.y + r.h * 0.5, 3.5 * s);
    let mut p = Path::new();
    p.move_to(cx - d, cy - d * 0.5)
        .line_to(cx + d, cy - d * 0.5)
        .line_to(cx, cy + d * 0.6)
        .close();
    c.fill(&p, t.ui.text_dim);
}

fn text_at(c: &mut Canvas, f: Option<&Font>, s: &str, px: f32, x: f32, y: f32, col: Rgba) {
    if let Some(f) = f {
        f.draw(c, s, px, x.round(), y.round(), col);
    }
}

/// Feld nur zum Ablesen (berechnet oder anderswo gepflegt): ohne Rahmen in
/// Feldfarbe, Text rechtsbündig in `field_readonly`.
pub fn field_readonly(c: &mut Canvas, fonts: &Fonts, r: Rect, text: &str, s: f32, t: &Theme) {
    let u = &t.ui;
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, 4.0 * s);
    c.fill(&p, u.border);
    let b = s.round().max(1.0);
    let mut p = Path::new();
    p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, 4.0 * s - b);
    c.fill(&p, u.bg);
    let Some(f) = fonts.regular.as_ref() else {
        return;
    };
    let px = t.size.font_small * s;
    let pad = t.size.field_pad * s;
    let base = (r.y + (r.h + f.cap_height(px)) * 0.5).round();
    let x = r.x + r.w - pad - f.width(text, px);
    f.draw(c, text, px, x.round(), base, u.field_readonly);
}

/// Dreieck zum Auf- und Zuklappen (▸ zu, ▾ offen), Mitte bei `(x, y)`.
pub fn disclosure(c: &mut Canvas, x: f32, y: f32, open: bool, color: Rgba, s: f32) {
    let d = 3.5 * s;
    let mut p = Path::new();
    if open {
        p.move_to(x - d, y - d * 0.55)
            .line_to(x + d, y - d * 0.55)
            .line_to(x, y + d * 0.65);
    } else {
        p.move_to(x - d * 0.55, y - d)
            .line_to(x + d * 0.65, y)
            .line_to(x - d * 0.55, y + d);
    }
    p.close();
    c.fill(&p, color);
}

/// Eintrag einer senkrechten Reiterliste: aktiv mit Fläche `pressed` und
/// Strich links im Akzent, unter der Maus `hover`, gesperrt blass.
#[allow(clippy::too_many_arguments)]
pub fn tab_item(
    c: &mut Canvas,
    fonts: &Fonts,
    r: Rect,
    label: &str,
    active: bool,
    hover: bool,
    disabled: bool,
    s: f32,
    t: &Theme,
) {
    let u = &t.ui;
    if active || (hover && !disabled) {
        let mut p = Path::new();
        p.rounded_rect(r.x, r.y, r.w, r.h, 4.0 * s);
        c.fill(&p, if active { u.pressed } else { u.hover });
    }
    if active {
        c.fill_rect(r.x, r.y, (3.0 * s).round(), r.h, u.accent);
    }
    let (font, col) = if disabled {
        (fonts.regular.as_ref(), u.text_disabled)
    } else if active {
        (fonts.bold.as_ref().or(fonts.regular.as_ref()), u.text)
    } else {
        (fonts.regular.as_ref(), u.text)
    };
    let px = t.size.font * s;
    let cap = font.map_or(px * 0.7, |f| f.cap_height(px));
    text_at(
        c,
        font,
        label,
        px,
        r.x + 14.0 * s,
        r.y + (r.h + cap) * 0.5,
        col,
    );
}

/// Senkrechte Bildlaufleiste in `r`: Schieber von `start` bis `start + len`
/// (Anteile 0..1 des Inhalts).
pub fn scrollbar(c: &mut Canvas, r: Rect, start: f32, len: f32, hover: bool, s: f32, t: &Theme) {
    let rad = r.w * 0.5;
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    c.fill(&p, t.ui.field);
    let (y0, h) = scroll_thumb(r, start, len, s);
    let mut p = Path::new();
    p.rounded_rect(r.x, y0, r.w, h, rad);
    c.fill(&p, if hover { t.ui.text_dim } else { t.ui.border });
}

/// Lage und Höhe des Schiebers einer Bildlaufleiste (Pixel).
pub fn scroll_thumb(r: Rect, start: f32, len: f32, s: f32) -> (f32, f32) {
    let h = (r.h * len.clamp(0.0, 1.0)).max(20.0 * s).min(r.h);
    let y = r.y + (r.h - h) * (start / (1.0 - len).max(1e-6)).clamp(0.0, 1.0);
    (y, h)
}

/// Sättigung/Helligkeit-Feld des Farbwählers zum Farbton `hue` (Grad):
/// x = Sättigung, y = Helligkeit (oben hell).
pub fn sv_field(w: usize, h: usize, hue: f32) -> Canvas {
    let (fw, fh) = ((w.max(2) - 1) as f32, (h.max(2) - 1) as f32);
    Canvas::from_fn(w, h, |x, y| {
        sk_paint::hsv_to_rgb(hue, x as f32 / fw, 1.0 - y as f32 / fh)
    })
}

/// Farbtonleiste des Farbwählers: oben 360°, unten 0° (beides Rot).
pub fn hue_bar(w: usize, h: usize) -> Canvas {
    let fh = (h.max(2) - 1) as f32;
    Canvas::from_fn(w, h, |_, y| {
        sk_paint::hsv_to_rgb(360.0 * (1.0 - y as f32 / fh), 1.0, 1.0)
    })
}

/// Kreisring (Marke im Farbwähler), Mitte `(x, y)`.
pub fn ring(c: &mut Canvas, x: f32, y: f32, r: f32, w: f32, color: Rgba) {
    let mut p = Path::new();
    p.rounded_rect(x - r, y - r, 2.0 * r, 2.0 * r, r);
    let ri = r - w;
    if ri > 0.0 {
        p.rounded_rect_hole(x - ri, y - ri, 2.0 * ri, 2.0 * ri, ri);
    }
    c.fill(&p, color);
}

/// Text mit Grundlinie bei `y`.
pub fn text(c: &mut Canvas, font: Option<&Font>, t: &str, px: f32, x: f32, y: f32, color: Rgba) {
    if let Some(f) = font {
        f.draw(c, t, px, x.round(), y.round(), color);
    }
}

/// Feine waagerechte Trennlinie.
pub fn separator(c: &mut Canvas, x: f32, y: f32, w: f32, s: f32, t: &Theme) {
    c.fill_rect(x, y.round(), w, s.round().max(1.0), t.ui.border);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paneel wie vor der Beschleunigung: sechs Flächen über das ganze Paneel.
    fn panel_reference(c: &mut Canvas, r: Rect, s: f32, t: &Theme) {
        let rad = t.size.corner_radius * s;
        for i in 1..=4 {
            let d = i as f32 * 2.0 * s;
            let mut p = Path::new();
            p.rounded_rect(r.x - d * 0.5, r.y - d * 0.25, r.w + d, r.h + d, rad + d);
            c.fill(&p, Rgba(0, 0, 0, 14));
        }
        let mut p = Path::new();
        p.rounded_rect(r.x, r.y, r.w, r.h, rad);
        c.fill(&p, Rgba::rgb(56, 65, 76));
        let b = s.round().max(1.0);
        let mut p = Path::new();
        p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
        c.fill(&p, Rgba::rgb(31, 37, 45));
    }

    fn paint(s: f32, f: fn(&mut Canvas, Rect, f32, &Theme)) -> Vec<u8> {
        let m = (10.0 * s).round();
        let (w, h) = (196.0 * s, 640.0 * s);
        let mut c = Canvas::new((w + 2.0 * m) as usize, (h + 2.0 * m) as usize);
        f(&mut c, Rect::new(m, m, w, h), s, &Theme::dark());
        c.to_premul_rgba8()
    }

    #[test]
    fn paneel_bild_bleibt_gleich() {
        for s in [1.0, 1.25, 1.5, 2.0] {
            let (a, b) = (paint(s, panel), paint(s, panel_reference));
            let diff = a.iter().zip(&b).map(|(x, y)| x.abs_diff(*y)).max();
            assert_eq!(diff, Some(0), "Skalierung {s}");
        }
    }

    /// `cargo test --release -p sk-ui paneel_zeit -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn paneel_zeit() {
        for s in [1.0f32, 1.5, 2.0] {
            let t = |f: fn(&mut Canvas, Rect, f32, &Theme)| {
                let n = 20;
                let start = std::time::Instant::now();
                for _ in 0..n {
                    std::hint::black_box(paint(s, f));
                }
                start.elapsed().as_secs_f64() * 1000.0 / n as f64
            };
            println!(
                "Skalierung {s}: vorher {:.2} ms, jetzt {:.2} ms",
                t(panel_reference),
                t(panel)
            );
        }
    }
}

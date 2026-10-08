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

/// Breite eines Tooltips höchstens (dip); darüber bricht der Text um.
pub const TOOLTIP_MAX_W: f32 = 300.0;

/// Hinweis an der Maus: Text auf dunklem Grund mit Rand. Liefert das Bild
/// (Pixel).
pub fn tooltip(fonts: &Fonts, text: &str, s: f32, t: &Theme) -> Canvas {
    let px = t.size.font_small * s;
    let f = fonts.regular.as_ref();
    // Mehrzeilig mit „\n“; je Zeile 18 dip mehr. Dann ist Zeile 1 die
    // fette Überschrift, die übrigen sind gedämpft (wie E18).
    let src: Vec<&str> = text.split('\n').collect();
    let multi = src.len() > 1;
    let font = |i: usize| {
        if multi && i == 0 {
            fonts.bold.as_ref().or(f)
        } else {
            f
        }
    };
    let pad = (8.0 * s).round();
    // Umbruch ab 300 dip (Paket 9 §3.1); jede Teilzeile behält die Art
    // ihrer Zeile
    let max_w = (TOOLTIP_MAX_W * s).round() - 2.0 * pad;
    let lines: Vec<(String, usize)> = src
        .iter()
        .enumerate()
        .flat_map(|(i, l)| wrap(font(i), l, px, max_w).into_iter().map(move |x| (x, i)))
        .collect();
    let tw = lines
        .iter()
        .map(|(l, i)| font(*i).map_or(0.0, |f| f.width(l, px)))
        .fold(0.0, f32::max);
    let line = (18.0 * s).round();
    let h = (24.0 * s).round() + line * (lines.len() - 1) as f32;
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
        for (k, (l, i)) in lines.iter().enumerate() {
            let y = y0 + line * k as f32;
            let col = match (multi, i) {
                (false, _) => t.ui.tooltip_text,
                (true, 0) => t.ui.text,
                _ => t.ui.text_dim,
            };
            font(*i).unwrap_or(f0).draw(&mut c, l, px, pad, y, col);
        }
    }
    c
}

/// Unaufdringlicher Hinweis in der Statuszeile (F-17): gedämpfte Schrift
/// mit einem Akzentpunkt davor, auf Paneelgrund mit feinem Rand.
pub fn notice(fonts: &Fonts, text: &str, s: f32, t: &Theme) -> Canvas {
    notice_dot(fonts, text, s, t, t.ui.accent)
}

/// Wie [`notice`] mit eigener Farbe des Punkts (Fehler: `field_invalid`).
pub fn notice_dot(fonts: &Fonts, text: &str, s: f32, t: &Theme, dot_color: Rgba) -> Canvas {
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
    c.fill(&p, dot_color);
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
    /// Gesperrt (Paket 4): Schrift blass, kein Hover.
    pub disabled: bool,
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
    let unit_col = if st.disabled {
        t.ui.text_disabled
    } else {
        t.ui.field_unit
    };
    f.draw(c, st.unit, px, unit_x.round(), base, unit_col);
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
    let fill = if st.hover && !st.focus && !st.disabled {
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
    let col = if st.disabled {
        u.text_disabled
    } else {
        u.field_text
    };
    f.draw(c, st.text, px, x.round(), base, col);
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

/// Kürzt so, dass der Text ab dem Zeichen `ab` möglichst stehen bleibt:
/// „AW Porenbeton-P… d=24cm Dünnbettmörtel“. Für Einträge, die sich erst
/// hinten unterscheiden; `ab` = 0 kürzt wie [`ellipsize`] am Ende.
pub fn ellipsize_ab(font: Option<&Font>, text: &str, px: f32, max_w: f32, ab: usize) -> String {
    let Some(f) = font else {
        return text.into();
    };
    if f.width(text, px) <= max_w {
        return text.into();
    }
    let zeichen: Vec<char> = text.chars().collect();
    if ab == 0 || ab >= zeichen.len() {
        return ellipsize(font, text, px, max_w);
    }
    let platz = max_w - f.width("… ", px);
    let ende: String = zeichen[ab..].iter().collect();
    let ende = ellipsize(font, ende.trim_start(), px, platz * 0.7);
    let rest = platz - f.width(&ende, px);
    let mut vorn = String::new();
    for ch in &zeichen[..ab] {
        let mut probe = vorn.clone();
        probe.push(*ch);
        if f.width(&probe, px) > rest {
            break;
        }
        vorn.push(*ch);
    }
    // Passt der gemeinsame Anfang ganz, reicht das Kürzen am Ende
    if vorn.chars().count() == ab {
        return ellipsize(font, text, px, max_w);
    }
    // Nach einem Bindestrich ohne Lücke: „Porenbeton-…Planbauplatte“
    let luecke = if zeichen[ab - 1] == '-' { "" } else { " " };
    format!("{}…{luecke}{ende}", vorn.trim_end())
}

/// Erstes Zeichen des Wort(teil)s, ab dem sich `text` von allen `andere`
/// unterscheidet; 0, wenn keiner gleich beginnt.
pub fn eigener_teil<'a>(text: &str, andere: impl IntoIterator<Item = &'a str>) -> usize {
    let a: Vec<char> = text.chars().collect();
    let gleich = andere
        .into_iter()
        .map(|o| {
            a.iter()
                .zip(o.chars())
                .take_while(|(x, y)| **x == *y)
                .count()
        })
        .max()
        .unwrap_or(0);
    // zurück auf den Wortanfang, auch hinter einem Bindestrich
    a[..gleich.min(a.len())]
        .iter()
        .rposition(|c| *c == ' ' || *c == '-')
        .map_or(0, |i| i + 1)
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

/// Zustand eines Baumsymbols (Paket 4): ganz, teilweise oder gar nicht
/// (Auge: sichtbar, teils, ausgeblendet; Schloss: zu, teils, offen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    Full,
    Partial,
    None,
}

/// Strichstärke der Baumsymbole (Pixel).
fn icon_stroke(s: f32) -> f32 {
    (1.3 * s).max(1.0)
}

/// Mandelform des Auges um `(x, y)`, Halbbreite `w`, Kontrollhöhe `h`.
fn almond(p: &mut Path, x: f32, y: f32, w: f32, h: f32) {
    p.move_to(x - w, y)
        .quad_to((x, y - h), (x + w, y))
        .quad_to((x, y + h), (x - w, y))
        .close();
}

/// Auge (16 dip), Mitte `(x, y)`: offen mit Pupille, teilweise mit hohler
/// Pupille, ausgeblendet durchgestrichen.
pub fn eye_icon(c: &mut Canvas, x: f32, y: f32, state: Fill, color: Rgba, s: f32) {
    let k = icon_stroke(s);
    let (w, h) = (6.5 * s, 7.0 * s);
    let mut p = Path::new();
    almond(&mut p, x, y, w, h);
    // Inneres gegenläufig: Loch
    let (wi, hi) = (w - 1.6 * k, h - 2.2 * k);
    p.move_to(x + wi, y)
        .quad_to((x, y - hi), (x - wi, y))
        .quad_to((x, y + hi), (x + wi, y))
        .close();
    match state {
        Fill::Full => {
            p.rounded_rect(x - 2.2 * s, y - 2.2 * s, 4.4 * s, 4.4 * s, 2.2 * s);
        }
        Fill::Partial => {
            p.rounded_rect(x - 2.4 * s, y - 2.4 * s, 4.8 * s, 4.8 * s, 2.4 * s);
            let r = 2.4 * s - k;
            p.rounded_rect_hole(x - r, y - r, 2.0 * r, 2.0 * r, r);
        }
        Fill::None => {
            p.segment((x - 6.0 * s, y + 5.0 * s), (x + 6.0 * s, y - 5.0 * s), k);
        }
    }
    c.fill(&p, color);
}

/// Bogen als Strich (Mitte `(x, y)`, Halbmesser `r`, von `a0` bis `a1`
/// Bogenmaß, y nach unten).
fn arc(p: &mut Path, x: f32, y: f32, r: f32, a0: f32, a1: f32, k: f32) {
    let n = 8;
    let pt = |a: f32| (x + r * a.cos(), y + r * a.sin());
    for i in 0..n {
        let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        p.segment(pt(a0 + (a1 - a0) * t0), pt(a0 + (a1 - a0) * t1), k);
    }
}

/// Schloss (16 dip), Mitte `(x, y)`: zu mit gefülltem Körper, offen mit
/// angehobenem Bügel und hohlem Körper, teilweise halb gefüllt.
pub fn lock_icon(c: &mut Canvas, x: f32, y: f32, state: Fill, color: Rgba, s: f32) {
    let k = icon_stroke(s);
    let (bx, by, bw, bh) = (x - 4.5 * s, y - 0.5 * s, 9.0 * s, 6.5 * s);
    let rad = 1.2 * s;
    let mut p = Path::new();
    p.rounded_rect(bx, by, bw, bh, rad);
    match state {
        Fill::Full => {}
        Fill::None => {
            p.rounded_rect_hole(
                bx + k,
                by + k,
                bw - 2.0 * k,
                bh - 2.0 * k,
                (rad - k).max(0.0),
            );
        }
        Fill::Partial => {
            let hh = (bh - 2.0 * k) * 0.5;
            p.rounded_rect_hole(bx + k, by + k, bw - 2.0 * k, hh, (rad - k).max(0.0));
        }
    }
    // Bügel: Halbkreis über zwei Schenkeln; offen angehoben, nur der linke
    // reicht in den Körper
    let r = 3.0 * s;
    let lift = if state == Fill::None { 2.0 * s } else { 0.0 };
    let top = y - 3.5 * s - lift;
    let pi = std::f32::consts::PI;
    arc(&mut p, x, top, r, pi, 2.0 * pi, k);
    p.segment((x - r, top), (x - r, by), k);
    let right_end = if state == Fill::None {
        top + 1.0 * s
    } else {
        by
    };
    p.segment((x + r, top), (x + r, right_end), k);
    c.fill(&p, color);
}

/// Isolieren (16 dip): Fadenkreuz mit Ring und Punkt.
pub fn isolate_icon(c: &mut Canvas, x: f32, y: f32, color: Rgba, s: f32) {
    let k = icon_stroke(s);
    let r = 4.5 * s;
    let mut p = Path::new();
    p.rounded_rect(x - r, y - r, 2.0 * r, 2.0 * r, r);
    let ri = r - k;
    p.rounded_rect_hole(x - ri, y - ri, 2.0 * ri, 2.0 * ri, ri);
    let (a, b) = (r, 7.0 * s);
    p.segment((x, y - a), (x, y - b), k)
        .segment((x, y + a), (x, y + b), k)
        .segment((x - a, y), (x - b, y), k)
        .segment((x + a, y), (x + b, y), k);
    let d = 1.2 * s;
    p.rounded_rect(x - d, y - d, 2.0 * d, 2.0 * d, d);
    c.fill(&p, color);
}

/// Löschen (16 dip): Papierkorb mit Deckel.
pub fn trash_icon(c: &mut Canvas, x: f32, y: f32, color: Rgba, s: f32) {
    let k = icon_stroke(s);
    let mut p = Path::new();
    p.segment((x - 5.5 * s, y - 4.5 * s), (x + 5.5 * s, y - 4.5 * s), k)
        .segment((x - 2.0 * s, y - 6.0 * s), (x + 2.0 * s, y - 6.0 * s), k);
    let (bx, by, bw, bh) = (x - 4.0 * s, y - 3.5 * s, 8.0 * s, 9.5 * s);
    p.rounded_rect(bx, by, bw, bh, 1.0 * s);
    p.rounded_rect_hole(bx + k, by, bw - 2.0 * k, bh - k, 0.5 * s);
    p.segment(
        (x - 1.4 * s, y - 1.5 * s),
        (x - 1.4 * s, y + 4.0 * s),
        k * 0.85,
    )
    .segment(
        (x + 1.4 * s, y - 1.5 * s),
        (x + 1.4 * s, y + 4.0 * s),
        k * 0.85,
    );
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

    /// Bedienbarkeit 14.1: gekürzt wird vor dem Teil, der den Eintrag von
    /// den Geschwistern unterscheidet; das Ergebnis passt in die Breite.
    #[test]
    fn ellipse_vor_dem_eigenen_teil() {
        let fonts = Fonts::system();
        let Some(f) = fonts.regular.as_ref() else {
            return;
        };
        let lang = "AW Porenbeton-Planstein PP2-0,35 d=24cm Dünnbettmörtel";
        let andere = [
            "AW Porenbeton-Planstein PP2-0,35 d=17,5cm Dünnbettmörtel",
            "IW Porenbeton-Planstein PP2-0,35 d=24cm Dünnbettmörtel",
        ];
        let ab = eigener_teil(lang, andere);
        assert_eq!(&lang[ab..], "d=24cm Dünnbettmörtel");
        assert_eq!(eigener_teil("Randdämmstreifen", andere), 0);
        let platte = "IW Porenbeton-Planbauplatte d=11,5cm Dünnbettmörtel";
        let ab = eigener_teil(platte, andere);
        assert_eq!(&platte[ab..], "Planbauplatte d=11,5cm Dünnbettmörtel");
        // Kurzer gemeinsamer Anfang: kein „…“ direkt dahinter
        let mw = "MW-Dämmplatte 035 d=120mm Steinwolle";
        let ab_mw = eigener_teil(mw, ["MW-Lamellenplatte 035 d=120mm"]);
        let k = ellipsize_ab(Some(f), mw, 13.0, f.width(mw, 13.0) * 0.7, ab_mw);
        assert!(k.starts_with("MW-Dämm") && k.ends_with('…'), "{k}");
        let ab = eigener_teil(lang, andere);
        assert_eq!(ellipsize_ab(Some(f), "kurz", 13.0, 200.0, 2), "kurz");
        let max = f.width(lang, 13.0) * 0.6;
        let k = ellipsize_ab(Some(f), lang, 13.0, max, ab);
        assert!(k.starts_with("AW Poren"), "{k}");
        // Der eigene Teil steht, gekürzt höchstens an seinem Ende
        assert!(k.contains("… d=24cm"), "{k}");
        assert!(f.width(&k, 13.0) <= max, "{k}");
    }

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

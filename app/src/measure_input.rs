//! Maßeingabe per Tastatur (Paket 8): die Pille am Gummiband bzw. am
//! gezogenen Wandsegment wird zum Eingabefeld. Rein, ohne App: Tasten
//! hinein, Ergebnis heraus; das Werkzeug entscheidet, was Enter bewirkt.

use crate::ui::{parse_measure, MeasureKind};
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{Key, Modifiers};
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;

/// Höchstens so viele Zeichen je Feld.
pub const MAX_CHARS: usize = 9;

/// Offene Eingabe: ein oder zwei Felder (Länge und Winkel bzw. Versatz).
#[derive(Clone, Debug, PartialEq)]
pub struct MeasureInput {
    pub fields: [String; 2],
    pub active: usize,
    /// Grund, warum der Wert nicht gilt (für die Statuszeile); Rand rot.
    pub error: Option<String>,
    kinds: [Option<MeasureKind>; 2],
}

/// Was eine Taste in der Eingabe bewirkt hat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputOutcome {
    /// Nicht für die Eingabe (geht ans Werkzeug).
    Ignored,
    /// Inhalt oder Feld geändert.
    Changed,
    /// Rücktaste hat das letzte Zeichen gelöscht: die Eingabe ist leer und
    /// schließt (eine weitere Rücktaste wirkt wie ohne Eingabe).
    Emptied,
    /// Enter mit gültigem Inhalt.
    Enter,
    /// Enter bei ganz leerer Eingabe.
    EnterEmpty,
    /// Enter mit falschem Inhalt: nichts geschieht.
    Refused,
    /// Esc: die Eingabe schließt.
    Escape,
}

/// Öffnet ein Zeichen eine Eingabe in einem Feld der Art `kind`?
pub fn opens(ch: char, kind: MeasureKind, mods: Modifiers) -> bool {
    !mods.ctrl && !mods.alt && accepts(ch, kind)
}

fn accepts(ch: char, kind: MeasureKind) -> bool {
    ch.is_ascii_digit() || ch == ',' || ch == '.' || (ch == '-' && kind != MeasureKind::Length)
}

impl MeasureInput {
    /// Eingabe mit einem Feld (`second` = None) oder zweien.
    pub fn new(first: MeasureKind, second: Option<MeasureKind>) -> MeasureInput {
        MeasureInput {
            fields: [String::new(), String::new()],
            active: 0,
            error: None,
            kinds: [Some(first), second],
        }
    }

    fn kind(&self) -> MeasureKind {
        self.kinds[self.active].unwrap_or(MeasureKind::Length)
    }

    /// Wert eines Felds; `None` bei leerem Feld, `Err` bei falschem Inhalt.
    pub fn value(&self, i: usize) -> Option<Result<f64, String>> {
        let k = self.kinds[i]?;
        let t = &self.fields[i];
        (!t.trim().is_empty()).then(|| parse_measure(t, k).map_err(|e| e.message().to_string()))
    }

    pub fn is_empty(&self) -> bool {
        self.fields.iter().all(|f| f.is_empty())
    }

    fn check(&mut self) {
        self.error = (0..2)
            .filter_map(|i| self.value(i).and_then(|r| r.err()))
            .next();
        // Ein Winkel ohne Länge gilt nicht
        if self.error.is_none() && self.fields[0].trim().is_empty() && !self.is_empty() {
            if let Some(k) = self.kinds[0] {
                self.error = Some(crate::ui::MeasureError(k).message().into());
            }
        }
    }

    /// Ein Zeichen anhängen (ohne Strg und Alt).
    pub fn push(&mut self, ch: char) -> bool {
        let kind = self.kind();
        let f = &mut self.fields[self.active];
        if !accepts(ch, kind) || f.chars().count() >= MAX_CHARS {
            return false;
        }
        f.push(ch);
        self.check();
        true
    }

    /// Taste in der offenen Eingabe.
    pub fn key(&mut self, key: Key, mods: Modifiers) -> InputOutcome {
        match key {
            Key::Char(ch) if !mods.ctrl && !mods.alt && accepts(ch, self.kind()) => {
                self.push(ch);
                InputOutcome::Changed
            }
            Key::Tab if self.kinds[1].is_some() => {
                self.active = 1 - self.active;
                InputOutcome::Changed
            }
            Key::Backspace => {
                if self.fields[self.active].pop().is_none() && self.is_empty() {
                    return InputOutcome::Ignored;
                }
                self.check();
                if self.is_empty() {
                    InputOutcome::Emptied
                } else {
                    InputOutcome::Changed
                }
            }
            Key::Enter => {
                if self.is_empty() {
                    InputOutcome::EnterEmpty
                } else if self.error.is_some() {
                    InputOutcome::Refused
                } else {
                    InputOutcome::Enter
                }
            }
            Key::Escape => InputOutcome::Escape,
            _ => InputOutcome::Ignored,
        }
    }

    /// Text der Pille: „Länge 4,50 m  Winkel 90°“ (zweites Feld nur, wenn
    /// es aktiv ist oder etwas enthält).
    pub fn text(&self, labels: [&str; 2]) -> String {
        let mut parts = Vec::new();
        for (i, label) in labels.iter().enumerate() {
            let Some(k) = self.kinds[i] else {
                continue;
            };
            if i == 1 && self.active != 1 && self.fields[1].is_empty() {
                continue;
            }
            let unit = if k == MeasureKind::Angle { "°" } else { " m" };
            parts.push(format!("{label} {}{unit}", self.fields[i]));
        }
        parts.join("  ")
    }

    /// Pille als Bild: wie die Maßzahl beim Bündig-Setzen, mit Rand 1 dip
    /// (Akzent bzw. Fehlerfarbe) und einem ruhigen Cursor hinter dem
    /// aktiven Feld.
    pub fn paint(&self, fonts: &Fonts, labels: [&str; 2], s: f32, t: &Theme) -> Canvas {
        let px = t.size.font_small * s;
        let f = fonts.regular.as_ref();
        let width = |x: &str| f.map_or(x.len() as f32 * px * 0.5, |f| f.width(x, px));
        let text = self.text(labels);
        // Cursor hinter dem Inhalt des aktiven Felds
        let head = {
            let mut probe = self.clone();
            if self.active == 0 {
                probe.fields[1].clear();
                probe.active = 0;
            }
            let full = probe.text(labels);
            let unit = if self.kinds[self.active] == Some(MeasureKind::Angle) {
                "°"
            } else {
                " m"
            };
            full[..full.len() - unit.len()].to_string()
        };
        let (pad, h) = (
            (t.size.dim_label_pad * s).round(),
            (t.size.dim_label_h * s).round(),
        );
        let w = (width(&text) + 2.0 * pad + 2.0 * s).ceil();
        let mut c = Canvas::new(w as usize, h as usize);
        let r = t.size.dim_label_radius * s;
        let border = if self.error.is_some() {
            t.ui.field_invalid
        } else {
            t.ui.field_focus
        };
        let b = s.max(1.0);
        let mut p = Path::new();
        p.rounded_rect(0.0, 0.0, w, h, r);
        c.fill(&p, border);
        let mut p = Path::new();
        p.rounded_rect(b, b, w - 2.0 * b, h - 2.0 * b, (r - b).max(0.0));
        c.fill(&p, Rgba::from_f32(t.interact.shadow_band));
        let cap = f.map_or(px * 0.7, |f| f.cap_height(px));
        let base = ((h + cap) * 0.5).round();
        sk_ui::widgets::text(&mut c, f, &text, px, pad, base, t.ui.dim_text);
        let cx = (pad + width(&head)).round() + 0.5 * s;
        c.fill_rect(
            cx,
            base - cap - 2.0 * s,
            s.max(1.0),
            cap + 4.0 * s,
            t.ui.caret,
        );
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OHNE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    #[test]
    fn tippen_tab_ruecktaste_enter() {
        let mut m = MeasureInput::new(MeasureKind::Length, Some(MeasureKind::Angle));
        for ch in "4,5".chars() {
            assert_eq!(m.key(Key::Char(ch), OHNE), InputOutcome::Changed);
        }
        assert_eq!(
            m.key(Key::Char('-'), OHNE),
            InputOutcome::Ignored,
            "Minus nur im Winkel"
        );
        assert_eq!(m.key(Key::Tab, OHNE), InputOutcome::Changed);
        for ch in "-90".chars() {
            m.key(Key::Char(ch), OHNE);
        }
        assert_eq!(m.fields, ["4,5".to_string(), "-90".to_string()]);
        assert_eq!(m.value(0), Some(Ok(4500.0)));
        assert_eq!(m.value(1), Some(Ok(-90.0)));
        assert_eq!(m.text(["Länge", "Winkel"]), "Länge 4,5 m  Winkel -90°");
        assert_eq!(m.key(Key::Enter, OHNE), InputOutcome::Enter);
        let strg = Modifiers { ctrl: true, ..OHNE };
        assert_eq!(m.key(Key::Char('5'), strg), InputOutcome::Ignored);
        m.key(Key::Char('5'), OHNE);
        assert!(m.error.is_some(), "−905° gilt nicht");
        assert_eq!(m.key(Key::Enter, OHNE), InputOutcome::Refused);
        for _ in 0..4 {
            m.key(Key::Backspace, OHNE);
        }
        assert_eq!(m.fields[1], "");
        assert!(m.error.is_none());
        m.key(Key::Tab, OHNE);
        for _ in 0..2 {
            assert_eq!(m.key(Key::Backspace, OHNE), InputOutcome::Changed);
        }
        assert_eq!(m.key(Key::Backspace, OHNE), InputOutcome::Emptied);
        assert_eq!(m.key(Key::Enter, OHNE), InputOutcome::EnterEmpty);
    }

    #[test]
    fn hoechstens_neun_zeichen() {
        let mut m = MeasureInput::new(MeasureKind::Offset, None);
        for ch in "-1234567890".chars() {
            m.key(Key::Char(ch), OHNE);
        }
        assert_eq!(m.fields[0], "-12345678");
        assert_eq!(m.key(Key::Tab, OHNE), InputOutcome::Ignored, "ein Feld");
    }
}

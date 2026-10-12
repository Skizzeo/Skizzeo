//! Schnitthöhe des Grundrisses (Jörn 11.10. 14:05): ein Zahnrad oben rechts
//! im Grundriss, an derselben Stelle wie das Zahnrad der Ansichten, öffnet
//! ein kleines Feld mit der Schnitthöhe in m über OK Rohdecke. Vorgabe 1,50 m;
//! sie gilt in jedem Geschoss, das Fundament schneidet weiter in der Mitte der
//! Frostschürze. Die Wahl steht im Modell als Ansichtszustand ohne
//! Rückgängig-Schritt ([`sk_model::hoehen`]).
//!
//! Eingabe: Klick ins Feld, Zahl tippen, Enter. „1,2“ heißt 1,20 m, ab 10
//! gilt die Zahl als cm („120“ ist 1,20 m). Esc bricht die Eingabe ab und
//! schließt danach das Feld.
//!
//! Darunter die Schnelloption „Dachentwässerung ausblenden“ des Grundrisses
//! mit „Auf alle Ansichten übertragen“ ([`crate::sicht_zahnrad`]).

use sk_model::{MAX_PLAN_CUT, MIN_PLAN_CUT, PLAN_CUT};
use sk_paint::Canvas;
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, FieldState, Fonts, Rect};

/// Tooltip am Zahnrad.
pub const TIP: &str = "Schnitthöhe und Sichtbarkeit des Grundrisses";
/// Name der Zeile im Feld.
pub const NAME: &str = "Schnitthöhe";
/// Zeile unter dem Wert: worauf sie sich bezieht.
pub const BEZUG: &str = "über OK Rohdecke, in allen Geschossen";
/// Zeile bei einer Eingabe außerhalb des Bereichs.
pub const FALSCH: &str = "0,10 bis 5,00 m";

const RAD: f32 = 28.0;
const PAD: f32 = 8.0;
const GAP: f32 = 6.0;
const NAME_W: f32 = 92.0;
const FELD_W: f32 = 92.0;
const VORGABE_W: f32 = 120.0;
const BREITE: f32 = 2.0 * PAD + NAME_W + FELD_W + GAP + VORGABE_W;

// ===== Reine Funktionen =====

/// Schnitthöhe (mm) als Text in m mit zwei Stellen, mit drei bei ganzen
/// Millimetern: „1,50“, „1,005“.
pub fn text_m(mm: f64) -> String {
    let t = format!("{:.3}", mm / 1000.0);
    let t = t.strip_suffix('0').unwrap_or(&t);
    t.replace('.', ",")
}

/// Getippte Schnitthöhe in mm: m mit Komma oder Punkt, ab 10 cm; `None`
/// außerhalb von 0,10 … 5,00 m oder unlesbar.
pub fn lesen(t: &str) -> Option<f64> {
    let t = t.trim().trim_end_matches('m').trim().replace(',', ".");
    let v: f64 = t.parse().ok()?;
    let mm = if v >= 10.0 { v * 10.0 } else { v * 1000.0 };
    (mm.is_finite() && (MIN_PLAN_CUT..=MAX_PLAN_CUT).contains(&mm.round())).then(|| mm.round())
}

/// Teil von Zahnrad und Feld.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Teil {
    Zahnrad,
    Wert,
    Vorgabe,
    /// Häkchen „Dachentwässerung ausblenden“ und „Auf alle Ansichten
    /// übertragen“ ([`crate::sicht_zahnrad`]).
    Haken,
    Alle,
}

/// Lage von Zahnrad und Feld in der Ansicht (Pixel).
#[derive(Clone, Debug, PartialEq)]
pub struct Lage {
    pub zahnrad: Rect,
    pub feld: Rect,
    pub name: Rect,
    pub wert: Rect,
    pub vorgabe: Rect,
    /// Zeile unter dem Wert ([`BEZUG`] oder [`FALSCH`]).
    pub bezug: Rect,
    /// Zeile „Sichtbarkeit“: Name, Häkchen, „Auf alle Ansichten übertragen“.
    pub sicht: Rect,
    pub haken: Rect,
    pub alle: Rect,
}

/// Zahnrad oben links neben dem Paneel „Ansichten“ (`rechts`: dessen linker
/// Rand in Ansichtspixeln), das Feld darunter, rechtsbündig; wie das Zahnrad
/// der Ansichten.
pub fn lage(rechts: f32, s: f32, t: &Theme) -> Lage {
    let r = |x: f32, y: f32, w: f32, h: f32| {
        Rect::new(
            (x * s).round(),
            (y * s).round(),
            (w * s).round(),
            (h * s).round(),
        )
    };
    let m = t.size.panel_margin;
    let h = t.size.field_height;
    let x1 = rechts / s - m;
    let x0 = x1 - BREITE;
    let y0 = m + RAD + 4.0;
    let y = y0 + PAD;
    let kx = x0 + PAD + NAME_W;
    let ys = y + 2.0 * h + GAP * 1.5;
    let sw = FELD_W + GAP + VORGABE_W;
    Lage {
        zahnrad: r(x1 - RAD, m, RAD, RAD),
        feld: r(x0, y0, BREITE, 2.0 * PAD + 4.0 * h + GAP * 2.5),
        name: r(x0 + PAD, y, NAME_W, h),
        wert: r(kx, y, FELD_W, h),
        vorgabe: r(kx + FELD_W + GAP, y, VORGABE_W, h),
        bezug: r(x0 + PAD, y + h + GAP * 0.5, BREITE - 2.0 * PAD, h),
        sicht: r(x0 + PAD, ys, NAME_W, h),
        haken: r(kx, ys, sw, h),
        alle: r(kx, ys + h + GAP, sw, h),
    }
}

impl Lage {
    /// Treffbare Teile; geschlossen nur das Zahnrad, „Vorgabe“ nur mit
    /// eigener Höhe.
    fn treffer(&self, m: (f64, f64), offen: bool, eigen: bool) -> Option<Teil> {
        let mut teile = vec![(Teil::Zahnrad, self.zahnrad)];
        if offen {
            teile.push((Teil::Wert, self.wert));
            teile.extend([(Teil::Haken, self.haken), (Teil::Alle, self.alle)]);
            if eigen {
                teile.push((Teil::Vorgabe, self.vorgabe));
            }
        }
        teile
            .into_iter()
            .find(|(_, q)| q.contains(m.0, m.1))
            .map(|p| p.0)
    }
}

/// Was Zahnrad und Feld zeigen; gleich: kein neues Bild.
#[derive(Clone, Debug, PartialEq)]
pub struct Bild {
    /// Schnitthöhe (mm, Bits) und ob sie die eigene des Projekts ist.
    pub mm: u64,
    pub eigen: bool,
    pub offen: bool,
    pub hover: Option<Teil>,
    pub eingabe: Option<TextEdit>,
    pub falsch: bool,
    /// Der Grundriss blendet die Dachentwässerung aus.
    pub entw: bool,
    /// Linker Rand des Paneels „Ansichten“ (Bits) und Skalierung (Bits).
    pub rechts: u32,
    pub scale: u32,
}

/// Zahnrad und (offen) Feld zeichnen: Bild und Lage in der Ansicht.
pub fn malen(b: &Bild, fonts: &Fonts, t: &Theme) -> (Canvas, i32, i32) {
    let s = f32::from_bits(b.scale);
    let l = lage(f32::from_bits(b.rechts), s, t);
    let sh = (t.size.panel_shadow * s).ceil();
    let alles = if b.offen {
        let x0 = l.feld.x.min(l.zahnrad.x);
        let x1 = (l.feld.x + l.feld.w).max(l.zahnrad.x + l.zahnrad.w);
        Rect::new(x0, l.zahnrad.y, x1 - x0, l.feld.y + l.feld.h - l.zahnrad.y)
    } else {
        l.zahnrad
    };
    let (w, h) = ((alles.w + 2.0 * sh) as usize, (alles.h + 2.0 * sh) as usize);
    let mut c = Canvas::new(w, h);
    let (dx, dy) = (alles.x - sh, alles.y - sh);
    let ab = |q: Rect| Rect::new(q.x - dx, q.y - dy, q.w, q.h);
    let st = ButtonState {
        hover: b.hover == Some(Teil::Zahnrad),
        active: b.offen,
        ..Default::default()
    };
    widgets::button(&mut c, fonts, ab(l.zahnrad), "", st, s, t);
    let farbe = if b.offen { t.ui.on_accent } else { t.ui.text };
    crate::ansicht_schatten::zahnrad(&mut c, ab(l.zahnrad), farbe);
    if !b.offen {
        return (c, dx as i32, dy as i32);
    }
    widgets::panel(&mut c, ab(l.feld), s, t);
    let f = fonts.regular.as_ref();
    let zeile = |c: &mut Canvas, text: &str, q: Rect, px: f32, farbe| {
        if let Some(f) = f {
            let q = ab(q);
            let y = q.y + (q.h + f.cap_height(px)) * 0.5;
            f.draw(c, text, px, q.x.round(), y.round(), farbe);
        }
    };
    zeile(&mut c, NAME, l.name, t.size.font * s, t.ui.text);
    let wert = text_m(f64::from_bits(b.mm));
    let ein = b.eingabe.as_ref();
    let fs = FieldState {
        text: ein.map_or(wert.as_str(), |e| e.text.as_str()),
        unit: "m",
        hover: b.hover == Some(Teil::Wert),
        focus: ein.is_some(),
        invalid: b.falsch,
        caret: ein.map(|e| e.caret),
        select: ein.map(|e| e.selection()),
        disabled: false,
    };
    widgets::field(&mut c, fonts, ab(l.wert), &fs, s, t);
    if b.eigen {
        let st = ButtonState {
            hover: b.hover == Some(Teil::Vorgabe),
            ..Default::default()
        };
        let text = format!("Vorgabe {} m", text_m(PLAN_CUT));
        widgets::button(&mut c, fonts, ab(l.vorgabe), &text, st, s, t);
    }
    let (hinweis, farbe) = if b.falsch {
        (FALSCH, t.ui.field_invalid)
    } else {
        (BEZUG, t.ui.text_dim)
    };
    zeile(&mut c, hinweis, l.bezug, t.size.font_small * s, farbe);
    let sicht = crate::sicht_zahnrad::NAME;
    zeile(&mut c, sicht, l.sicht, t.size.font * s, t.ui.text);
    let hover = b.hover == Some(Teil::Haken);
    crate::sicht_zahnrad::haken(&mut c, fonts, ab(l.haken), b.entw, hover, s, t);
    let st = ButtonState {
        hover: b.hover == Some(Teil::Alle),
        ..Default::default()
    };
    let alle = crate::sicht_zahnrad::ALLE;
    widgets::button(&mut c, fonts, ab(l.alle), alle, st, s, t);
    (c, dx as i32, dy as i32)
}

/// Bedienzustand: Feld offen, Teil unter der Maus, getippter Wert.
#[derive(Clone, Debug, Default)]
pub struct Schalter {
    pub offen: bool,
    pub hover: Option<Teil>,
    pub eingabe: Option<TextEdit>,
    pub falsch: bool,
}

/// Ergebnis eines Ereignisses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ausgang {
    pub consumed: bool,
    pub redraw: bool,
    /// Neue Schnitthöhe (mm).
    pub neu: Option<f64>,
    /// Klick aufs Häkchen „Dachentwässerung ausblenden“ bzw. auf „Auf alle
    /// Ansichten übertragen“.
    pub haken: bool,
    pub alle: bool,
}

impl Schalter {
    pub fn reset(&mut self) {
        *self = Schalter::default();
    }

    /// Mausereignis in der Ansicht (ohne Titelleiste); `mm`: die heutige
    /// Schnitthöhe, `eigen`: das Projekt hat eine eigene.
    pub fn handle(&mut self, e: &Event, l: &Lage, mm: f64, eigen: bool) -> Ausgang {
        let mut out = Ausgang::default();
        let im_feld = |offen: bool, m: (f64, f64)| offen && l.feld.contains(m.0, m.1);
        match *e {
            Event::MouseMove { x, y, .. } => {
                let t = l.treffer((x, y), self.offen, eigen);
                out.redraw = t != self.hover;
                self.hover = t;
                out.consumed = t.is_some() || im_feld(self.offen, (x, y));
            }
            Event::MouseLeave => out.redraw = self.hover.take().is_some(),
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let t = l.treffer((x, y), self.offen, eigen);
                out.consumed = t.is_some() || im_feld(self.offen, (x, y));
                out.redraw = out.consumed;
                match t {
                    Some(Teil::Zahnrad) => {
                        self.offen = !self.offen;
                        self.eingabe = None;
                        self.falsch = false;
                    }
                    Some(Teil::Wert) => {
                        if self.eingabe.is_none() {
                            self.eingabe = Some(TextEdit::new(&text_m(mm)));
                        }
                    }
                    Some(Teil::Vorgabe) => {
                        self.eingabe = None;
                        self.falsch = false;
                        out.neu = Some(PLAN_CUT);
                    }
                    Some(t @ (Teil::Haken | Teil::Alle)) => {
                        self.eingabe = None;
                        self.falsch = false;
                        out.haken = t == Teil::Haken;
                        out.alle = t == Teil::Alle;
                    }
                    None if out.consumed => {
                        self.eingabe = None;
                        self.falsch = false;
                    }
                    // Klick daneben schließt das Feld und gehört der Ansicht
                    None => {
                        out.redraw = std::mem::take(&mut self.offen);
                        self.eingabe = None;
                        self.falsch = false;
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// Ob die laufende Eingabe die Taste `key` bearbeitet (siehe
    /// [`Schalter::key`]); andere Tasten wie Tab gehören der App.
    pub fn nimmt(&self, key: Key) -> bool {
        self.eingabe.is_some()
            && matches!(
                key,
                Key::Char(_)
                    | Key::Backspace
                    | Key::Delete
                    | Key::Left
                    | Key::Right
                    | Key::Home
                    | Key::End
                    | Key::Escape
                    | Key::Enter
            )
    }

    /// Taste bei offenem Feld; `true`, wenn verbraucht. Ohne Eingabe nimmt
    /// es nur Esc (schließt).
    pub fn key(&mut self, key: Key, mods: Modifiers, out: &mut Ausgang) -> bool {
        let Some(ed) = self.eingabe.as_mut() else {
            if key == Key::Escape && self.offen {
                self.offen = false;
                out.redraw = true;
                return true;
            }
            return false;
        };
        match key {
            Key::Char(c) if !mods.ctrl && (c.is_ascii_digit() || matches!(c, '.' | ',')) => {
                ed.insert(&c.to_string());
                self.falsch = false;
            }
            Key::Backspace => ed.backspace(),
            Key::Delete => ed.delete(),
            Key::Left => ed.left(mods.shift),
            Key::Right => ed.right(mods.shift),
            Key::Home => ed.home(mods.shift),
            Key::End => ed.end(mods.shift),
            Key::Escape => {
                self.eingabe = None;
                self.falsch = false;
            }
            Key::Enter => match lesen(&ed.text) {
                Some(mm) => {
                    out.neu = Some(mm);
                    self.eingabe = None;
                    self.falsch = false;
                }
                None => self.falsch = true,
            },
            // Strg+Z und anderes bleiben im Feld, ohne Wirkung auf das Modell
            Key::Char(_) => {}
            _ => return false,
        }
        out.redraw = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eingaben_lesen() {
        assert_eq!(lesen("1,5"), Some(1500.0));
        assert_eq!(lesen("1.20"), Some(1200.0));
        assert_eq!(lesen(" 1,05 m"), Some(1050.0));
        assert_eq!(lesen("120"), Some(1200.0), "ab 10 in cm");
        assert_eq!(lesen("0,1"), Some(100.0));
        assert_eq!(lesen("5"), Some(5000.0));
        assert_eq!(lesen("0,05"), None);
        assert_eq!(lesen("5,01"), None);
        assert_eq!(lesen("600"), None);
        assert_eq!(lesen(""), None);
        assert_eq!(lesen("x"), None);
        assert_eq!(text_m(1500.0), "1,50");
        assert_eq!(text_m(1234.0), "1,234");
        assert_eq!(text_m(1230.0), "1,23");
        assert_eq!(text_m(1005.0), "1,005");
        assert_eq!(lesen(&text_m(1005.0)), Some(1005.0), "ohne Änderung gleich");
    }

    fn klick(x: f32, y: f32) -> Event {
        Event::MouseDown {
            button: MouseButton::Left,
            x: x as f64,
            y: y as f64,
            mods: Modifiers::default(),
        }
    }

    fn mitte(r: Rect) -> Event {
        klick(r.x + r.w * 0.5, r.y + r.h * 0.5)
    }

    /// Zahnrad öffnet, Klick ins Feld beginnt die Eingabe mit dem heutigen
    /// Wert, Enter liefert die neue Höhe, Esc schließt; Klick daneben schließt
    /// und gehört der Ansicht.
    #[test]
    fn zahnrad_feld_und_eingabe() {
        let t = Theme::dark();
        let l = lage(1200.0, 1.0, &t);
        let mut s = Schalter::default();
        assert!(!l.feld.contains(0.0, 0.0));
        assert!(s.handle(&mitte(l.zahnrad), &l, 1500.0, false).consumed);
        assert!(s.offen);
        // „Vorgabe“ nur mit eigener Höhe
        let v = s.handle(&mitte(l.vorgabe), &l, 1500.0, false);
        assert_eq!(v.neu, None);
        assert!(v.consumed, "im Feld");
        s.handle(&mitte(l.wert), &l, 1500.0, false);
        assert_eq!(s.eingabe.as_ref().unwrap().text, "1,50");
        let mut out = Ausgang::default();
        s.key(Key::Char('1'), Modifiers::default(), &mut out);
        s.key(Key::Char('2'), Modifiers::default(), &mut out);
        s.key(Key::Char('0'), Modifiers::default(), &mut out);
        let strg = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        assert!(
            s.key(Key::Char('5'), strg, &mut out),
            "Strg+Ziffer bleibt im Feld"
        );
        assert!(s.nimmt(Key::Enter) && !s.nimmt(Key::Tab));
        s.key(Key::Enter, Modifiers::default(), &mut out);
        assert_eq!(out.neu, Some(1200.0));
        assert!(s.eingabe.is_none() && s.offen);
        // Falsche Zahl: Feld bleibt in Eingabe und meldet den Bereich
        s.handle(&mitte(l.wert), &l, 1200.0, true);
        let mut out = Ausgang::default();
        s.key(Key::Char('9'), Modifiers::default(), &mut out);
        s.key(Key::Enter, Modifiers::default(), &mut out);
        assert!(s.falsch && s.eingabe.is_some() && out.neu.is_none());
        s.key(Key::Escape, Modifiers::default(), &mut out);
        assert!(s.eingabe.is_none() && s.offen);
        let v = s.handle(&mitte(l.vorgabe), &l, 1200.0, true);
        assert_eq!(v.neu, Some(PLAN_CUT));
        let mut out = Ausgang::default();
        assert!(s.key(Key::Escape, Modifiers::default(), &mut out));
        assert!(!s.offen);
        s.handle(&mitte(l.zahnrad), &l, 1500.0, false);
        // Dachentwässerung (Jörn 12.10.): Häkchen und „auf alle“ unter der
        // Höhe, im Feld; ein Klick dort beendet die Eingabe
        for q in [l.sicht, l.haken, l.alle] {
            assert!(q.y > l.bezug.y && q.y + q.h <= l.feld.y + l.feld.h);
        }
        s.handle(&mitte(l.wert), &l, 1500.0, false);
        let h = s.handle(&mitte(l.haken), &l, 1500.0, false);
        assert!(h.consumed && h.haken && !h.alle && s.eingabe.is_none());
        let a = s.handle(&mitte(l.alle), &l, 1500.0, false);
        assert!(a.consumed && a.alle && !a.haken && a.neu.is_none());
        let d = s.handle(&klick(5.0, 400.0), &l, 1500.0, false);
        assert!(!d.consumed && !s.offen, "Klick daneben");
    }
}

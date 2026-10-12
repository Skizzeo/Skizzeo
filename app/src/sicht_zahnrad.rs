//! Schnelloption „Dachentwässerung ausblenden“ am Zahnrad (Jörn 12.10.): je
//! Ansicht ein Häkchen, das Regenfallrohr, Wasserfangkasten, Schellen,
//! Standrohr und Notentwässerung ausblendet wie die Art im Baum, nur in
//! dieser Ansicht ([`crate::scene::sicht`]). Mengen, Kosten und IFC bleiben.
//!
//! Grundriss und Ansichten tragen die Zeile im Feld ihres Zahnrads
//! ([`crate::grundriss_schnitt`], [`crate::ansicht_schatten`]); 3D und
//! Schnitt bekommen hier ein eigenes Zahnrad an derselben Stelle. „Auf alle
//! Ansichten übertragen“ gibt die Wahl an alle sieben Ansichten.

use crate::ui::ViewKind;
use sk_paint::Canvas;
use sk_platform::{Event, MouseButton};
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, Fonts, Rect};

/// Name der Zeile im Feld.
pub const NAME: &str = "Sichtbarkeit";
/// Text am Häkchen.
pub const HAKEN: &str = "Dachentwässerung ausblenden";
/// Knopf unter dem Häkchen.
pub const ALLE: &str = "Auf alle Ansichten übertragen";
/// Breite der Zeile mit dem Häkchen (dip): Kästchen, Abstand, Text.
pub const HAKEN_W: f32 = 218.0;

/// Ob die Ansicht das Zahnrad von hier zeigt: 3D und Schnitt (Grundriss
/// und Ansichten haben schon eins).
pub fn zeigt(v: ViewKind) -> bool {
    matches!(v, ViewKind::Persp | ViewKind::Section)
}

/// Häkchen mit [`HAKEN`] in der Zeile `q`: Kästchen links, mittig in der
/// Höhe, der Text daneben.
pub fn haken(c: &mut Canvas, fonts: &Fonts, q: Rect, an: bool, hover: bool, s: f32, t: &Theme) {
    let k = (t.size.checkbox * s).round();
    let kr = Rect::new(q.x, q.y + ((q.h - k) * 0.5).round(), k, k);
    widgets::checkbox(c, kr, an, hover, s, t);
    if let Some(f) = fonts.regular.as_ref() {
        let px = t.size.font * s;
        let y = q.y + (q.h + f.cap_height(px)) * 0.5;
        f.draw(
            c,
            HAKEN,
            px,
            (kr.x + k + 8.0 * s).round(),
            y.round(),
            t.ui.text,
        );
    }
}

// ===== Zahnrad in 3D und Schnitt =====

/// Teil von Zahnrad und Feld.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Teil {
    Zahnrad,
    Haken,
    Alle,
}

const RAD: f32 = 28.0;
const PAD: f32 = 8.0;
const GAP: f32 = 6.0;
const NAME_W: f32 = 92.0;
const BREITE: f32 = 2.0 * PAD + NAME_W + HAKEN_W;

/// Lage von Zahnrad und Feld in der Ansicht (Pixel).
#[derive(Clone, Debug, PartialEq)]
pub struct Lage {
    pub zahnrad: Rect,
    pub feld: Rect,
    pub name: Rect,
    pub haken: Rect,
    pub alle: Rect,
}

/// Zahnrad oben links neben dem Paneel „Ansichten“ (`rechts`: dessen linker
/// Rand in Ansichtspixeln), das Feld darunter, rechtsbündig; wie die
/// Zahnräder von Grundriss und Ansichten.
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
    Lage {
        zahnrad: r(x1 - RAD, m, RAD, RAD),
        feld: r(x0, y0, BREITE, 2.0 * PAD + 2.0 * h + GAP),
        name: r(x0 + PAD, y, NAME_W, h),
        haken: r(kx, y, HAKEN_W, h),
        alle: r(kx, y + h + GAP, HAKEN_W, h),
    }
}

impl Lage {
    /// Treffbare Teile; geschlossen nur das Zahnrad.
    fn treffer(&self, m: (f64, f64), offen: bool) -> Option<Teil> {
        let mut teile = vec![(Teil::Zahnrad, self.zahnrad)];
        if offen {
            teile.extend([(Teil::Haken, self.haken), (Teil::Alle, self.alle)]);
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
    /// Die Ansicht blendet die Dachentwässerung aus.
    pub an: bool,
    pub offen: bool,
    pub hover: Option<Teil>,
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
    if let Some(f) = fonts.regular.as_ref() {
        let q = ab(l.name);
        let px = t.size.font * s;
        let y = q.y + (q.h + f.cap_height(px)) * 0.5;
        f.draw(&mut c, NAME, px, q.x.round(), y.round(), t.ui.text);
    }
    let hover = b.hover == Some(Teil::Haken);
    haken(&mut c, fonts, ab(l.haken), b.an, hover, s, t);
    let st = ButtonState {
        hover: b.hover == Some(Teil::Alle),
        ..Default::default()
    };
    widgets::button(&mut c, fonts, ab(l.alle), ALLE, st, s, t);
    (c, dx as i32, dy as i32)
}

/// Bedienzustand: Feld offen, Teil unter der Maus.
#[derive(Clone, Debug, Default)]
pub struct Schalter {
    pub offen: bool,
    pub hover: Option<Teil>,
}

/// Ergebnis eines Ereignisses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ausgang {
    pub consumed: bool,
    pub redraw: bool,
    /// Klick aufs Häkchen: die Wahl dieser Ansicht umschalten.
    pub haken: bool,
    /// Die Wahl dieser Ansicht in alle sieben übertragen.
    pub alle: bool,
}

impl Schalter {
    pub fn reset(&mut self) {
        *self = Schalter::default();
    }

    /// Mausereignis in der Ansicht (ohne Titelleiste).
    pub fn handle(&mut self, e: &Event, l: &Lage) -> Ausgang {
        let mut out = Ausgang::default();
        let im_feld = |offen: bool, m: (f64, f64)| offen && l.feld.contains(m.0, m.1);
        match *e {
            Event::MouseMove { x, y, .. } => {
                let t = l.treffer((x, y), self.offen);
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
                let t = l.treffer((x, y), self.offen);
                out.consumed = t.is_some() || im_feld(self.offen, (x, y));
                out.redraw = out.consumed;
                match t {
                    Some(Teil::Zahnrad) => self.offen = !self.offen,
                    Some(Teil::Haken) => out.haken = true,
                    Some(Teil::Alle) => out.alle = true,
                    None if out.consumed => {}
                    // Klick daneben schließt das Feld und gehört der Ansicht
                    None => out.redraw = std::mem::take(&mut self.offen),
                }
            }
            _ => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_platform::Modifiers;

    fn klick(r: Rect) -> Event {
        Event::MouseDown {
            button: MouseButton::Left,
            x: (r.x + r.w * 0.5) as f64,
            y: (r.y + r.h * 0.5) as f64,
            mods: Modifiers::default(),
        }
    }

    /// Zahnrad öffnet das Feld, Häkchen und „auf alle“ melden sich, Klick
    /// daneben schließt und gehört der Ansicht; nur 3D und Schnitt.
    #[test]
    fn zahnrad_und_feld() {
        let t = Theme::dark();
        let l = lage(900.0, 1.0, &t);
        assert!(l.zahnrad.x + l.zahnrad.w <= 900.0);
        assert!(l.feld.y > l.zahnrad.y + l.zahnrad.h);
        for q in [l.name, l.haken, l.alle] {
            assert!(q.x >= l.feld.x && q.x + q.w <= l.feld.x + l.feld.w);
            assert!(q.y >= l.feld.y && q.y + q.h <= l.feld.y + l.feld.h);
        }
        // Feld und Häkchen stehen wie im Grundriss
        let g = crate::grundriss_schnitt::lage(900.0, 1.0, &t);
        assert_eq!((l.feld.x, l.feld.w), (g.feld.x, g.feld.w));
        assert_eq!(
            (l.haken.x, l.haken.w, l.alle.w),
            (g.haken.x, g.haken.w, g.alle.w)
        );
        let mut s = Schalter::default();
        let o = s.handle(&klick(l.haken), &l);
        assert!(!o.consumed && !o.haken, "zu: nur das Zahnrad");
        assert!(s.handle(&klick(l.zahnrad), &l).consumed && s.offen);
        let o = s.handle(&klick(l.haken), &l);
        assert!(o.consumed && o.haken && !o.alle);
        let o = s.handle(&klick(l.alle), &l);
        assert!(o.consumed && o.alle && !o.haken);
        let o = s.handle(&klick(l.name), &l);
        assert!(o.consumed && !o.haken && s.offen, "im Feld");
        let o = s.handle(&klick(Rect::new(5.0, 400.0, 1.0, 1.0)), &l);
        assert!(!o.consumed && o.redraw && !s.offen);
        let zeigt: Vec<ViewKind> = ViewKind::ALL.into_iter().filter(|v| zeigt(*v)).collect();
        assert_eq!(zeigt, [ViewKind::Persp, ViewKind::Section]);
        // Bild: offen größer als zu
        let fonts = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        let mut b = Bild {
            an: true,
            offen: false,
            hover: None,
            rechts: 900f32.to_bits(),
            scale: 1f32.to_bits(),
        };
        let (zu, _, _) = malen(&b, &fonts, &t);
        b.offen = true;
        let (auf, x, y) = malen(&b, &fonts, &t);
        assert!(auf.width > zu.width && auf.height > zu.height);
        assert!(x as f32 <= l.feld.x && y as f32 <= l.zahnrad.y);
    }
}

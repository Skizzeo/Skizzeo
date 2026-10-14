//! Schnelloption „Dachentwässerung ausblenden“ am Zahnrad (Jörn 12.10.): je
//! Ansicht ein Häkchen, das Regenfallrohr, Wasserfangkasten, Schellen,
//! Standrohr und Notentwässerung ausblendet wie die Art im Baum, nur in
//! dieser Ansicht ([`crate::scene::sicht`]). Mengen, Kosten und IFC bleiben.
//!
//! Grundriss und Ansichten tragen die Zeile im Feld ihres Zahnrads
//! ([`crate::grundriss_schnitt`], [`crate::ansicht_schatten`]); 3D und
//! Schnitt bekommen hier ein eigenes Zahnrad an derselben Stelle. „Auf alle
//! Ansichten übertragen“ gibt die Wahl an alle sieben Ansichten.
//!
//! Darunter steht in Grundriss, Schnitt und 3D der Schalter „Ausbau innen“
//! (Raumbuch RB4, `raumbuch/plan.md` §8) mit An und Aus je Ansicht
//! ([`sk_model::FINISH_VIEWS`]); „Auf alle Ansichten übertragen“ gibt ihn
//! an diese drei, die leise Zeile [`WOHIN`] sagt es.
//!
//! Zuletzt in 3D die Zeile „Bemaßung“ (Konzept `bemassung/konzept.md` §7):
//! „Maßketten zeigen“ und „Maßbezugspunkte zeigen“, je Ansicht
//! ([`crate::bemassung`]); der Grundriss trägt sie in seinem Feld.

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
/// Zeile „Bemaßung“ mit ihren zwei Häkchen.
pub const MASS: &str = "Bemaßung";
pub const KETTEN: &str = "Maßketten zeigen";
pub const PUNKTE: &str = "Maßbezugspunkte zeigen";
/// Name der Zeile des Schalters „Ausbau innen“ und sein Tooltip an An und
/// Aus.
pub const AUSBAU: &str = "Ausbau innen";
pub const AUSBAU_TIP: &str =
    "Putz, Wandbeläge, Bodenbelag und Sockelleisten, in 3D auch der Estrich; der Schnitt zeigt ihn immer";
/// Leise Zeile unter „Auf alle Ansichten übertragen“: die Dachentwässerung
/// geht an alle sieben Ansichten, der Schalter nur an die drei, die ihn
/// haben (wie Einstellungen H1).
pub const WOHIN: &str = "Ausbau innen nur an Grundriss, Schnitt und 3D";

/// Ob die Ansicht das Zahnrad von hier zeigt: 3D und Schnitt (Grundriss
/// und Ansichten haben schon eins).
pub fn zeigt(v: ViewKind) -> bool {
    matches!(v, ViewKind::Persp | ViewKind::Section)
}

/// Häkchen mit [`HAKEN`] in der Zeile `q`: Kästchen links, mittig in der
/// Höhe, der Text daneben.
pub fn haken(c: &mut Canvas, fonts: &Fonts, q: Rect, an: bool, hover: bool, s: f32, t: &Theme) {
    haken_mit(c, fonts, q, HAKEN, an, hover, s, t);
}

/// Häkchen mit dem Text `text` in der Zeile `q`.
#[allow(clippy::too_many_arguments)]
pub fn haken_mit(
    c: &mut Canvas,
    fonts: &Fonts,
    q: Rect,
    text: &str,
    an: bool,
    hover: bool,
    s: f32,
    t: &Theme,
) {
    let k = (t.size.checkbox * s).round();
    let kr = Rect::new(q.x, q.y + ((q.h - k) * 0.5).round(), k, k);
    widgets::checkbox(c, kr, an, hover, s, t);
    if let Some(f) = fonts.regular.as_ref() {
        let px = t.size.font * s;
        let y = q.y + (q.h + f.cap_height(px)) * 0.5;
        f.draw(
            c,
            text,
            px,
            (kr.x + k + 8.0 * s).round(),
            y.round(),
            t.ui.text,
        );
    }
}

/// Zeile „Ausbau innen“ in `q`: Name, An, Aus und die leise Zeile
/// [`WOHIN`]; der gewählte Knopf ist aktiv, `hover`: der Knopf unter der
/// Maus.
pub fn ausbau(
    c: &mut Canvas,
    fonts: &Fonts,
    q: [Rect; 4],
    an: bool,
    hover: Option<bool>,
    s: f32,
    t: &Theme,
) {
    let [name, ja, nein, wohin] = q;
    for (k, r, text) in [(true, ja, "An"), (false, nein, "Aus")] {
        let st = ButtonState {
            hover: hover == Some(k),
            active: k == an,
            ..Default::default()
        };
        widgets::button(c, fonts, r, text, st, s, t);
    }
    if let Some(f) = fonts.regular.as_ref() {
        for (r, text, px, farbe) in [
            (name, AUSBAU, t.size.font, t.ui.text),
            (wohin, WOHIN, t.size.font_small, t.ui.text_dim),
        ] {
            let px = px * s;
            let y = r.y + (r.h + f.cap_height(px)) * 0.5;
            f.draw(c, text, px, r.x.round(), y.round(), farbe);
        }
    }
}

// ===== Zahnrad in 3D und Schnitt =====

/// Teil von Zahnrad und Feld.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Teil {
    Zahnrad,
    Haken,
    /// „Ausbau innen“ an (`true`) oder aus.
    Ausbau(bool),
    Alle,
    /// Häkchen „Maßketten zeigen“ und „Maßbezugspunkte zeigen“.
    Ketten,
    Punkte,
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
    /// Zeile „Ausbau innen“: Name, An, Aus; „auf alle“ und leise, wohin.
    pub ausbau: Rect,
    pub an: Rect,
    pub aus: Rect,
    pub alle: Rect,
    pub wohin: Rect,
    /// Zeile „Bemaßung“ (Name und zwei Häkchen); leer ohne Bemaßung.
    pub mass: Rect,
    pub ketten: Rect,
    pub punkte: Rect,
}

/// Zahnrad oben links neben dem Paneel „Ansichten“ (`rechts`: dessen linker
/// Rand in Ansichtspixeln), das Feld darunter, rechtsbündig; wie die
/// Zahnräder von Grundriss und Ansichten. `mass`: mit der Zeile „Bemaßung“.
pub fn lage(rechts: f32, s: f32, t: &Theme, mass: bool) -> Lage {
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
    let ya = y + h + GAP;
    let kw = (HAKEN_W - GAP) * 0.5;
    let yl = ya + 2.0 * (h + GAP) - GAP * 0.5;
    let leise = h * 0.8;
    let ym = yl + leise + GAP * 1.5;
    let leer = Rect::new(0.0, 0.0, 0.0, 0.0);
    let (unten, mass, ketten, punkte) = if mass {
        let w = HAKEN_W;
        let q = (r(x0 + PAD, ym, NAME_W, h), r(kx, ym, w, h));
        (ym + 2.0 * h + GAP, q.0, q.1, r(kx, ym + h + GAP, w, h))
    } else {
        (yl + leise, leer, leer, leer)
    };
    Lage {
        zahnrad: r(x1 - RAD, m, RAD, RAD),
        feld: r(x0, y0, BREITE, unten + PAD - y0),
        name: r(x0 + PAD, y, NAME_W, h),
        haken: r(kx, y, HAKEN_W, h),
        ausbau: r(x0 + PAD, ya, NAME_W, h),
        an: r(kx, ya, kw, h),
        aus: r(kx + kw + GAP, ya, kw, h),
        alle: r(kx, ya + h + GAP, HAKEN_W, h),
        wohin: r(x0 + PAD, yl, BREITE - 2.0 * PAD, leise),
        mass,
        ketten,
        punkte,
    }
}

impl Lage {
    /// Treffbare Teile; geschlossen nur das Zahnrad.
    fn treffer(&self, m: (f64, f64), offen: bool) -> Option<Teil> {
        let mut teile = vec![(Teil::Zahnrad, self.zahnrad)];
        if offen {
            teile.extend([
                (Teil::Haken, self.haken),
                (Teil::Ausbau(true), self.an),
                (Teil::Ausbau(false), self.aus),
                (Teil::Alle, self.alle),
            ]);
            teile.extend([(Teil::Ketten, self.ketten), (Teil::Punkte, self.punkte)]);
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
    /// Die Ansicht blendet die Dachentwässerung aus bzw. zeigt den Ausbau
    /// innen.
    pub an: bool,
    pub ausbau: bool,
    pub offen: bool,
    pub hover: Option<Teil>,
    /// Bemaßung der Ansicht: Maßketten und Maßbezugspunkte zeigen; `None`
    /// in einer Ansicht ohne Bemaßung.
    pub mass: Option<(bool, bool)>,
    /// Linker Rand des Paneels „Ansichten“ (Bits) und Skalierung (Bits).
    pub rechts: u32,
    pub scale: u32,
}

/// Zahnrad und (offen) Feld zeichnen: Bild und Lage in der Ansicht.
pub fn malen(b: &Bild, fonts: &Fonts, t: &Theme) -> (Canvas, i32, i32) {
    let s = f32::from_bits(b.scale);
    let l = lage(f32::from_bits(b.rechts), s, t, b.mass.is_some());
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
    let name = |c: &mut Canvas, text: &str, q: Rect| {
        if let Some(f) = fonts.regular.as_ref() {
            let q = ab(q);
            let px = t.size.font * s;
            let y = q.y + (q.h + f.cap_height(px)) * 0.5;
            f.draw(c, text, px, q.x.round(), y.round(), t.ui.text);
        }
    };
    name(&mut c, NAME, l.name);
    let hover = b.hover == Some(Teil::Haken);
    haken(&mut c, fonts, ab(l.haken), b.an, hover, s, t);
    let st = ButtonState {
        hover: b.hover == Some(Teil::Alle),
        ..Default::default()
    };
    widgets::button(&mut c, fonts, ab(l.alle), ALLE, st, s, t);
    let hover = match b.hover {
        Some(Teil::Ausbau(an)) => Some(an),
        _ => None,
    };
    let q = [l.ausbau, l.an, l.aus, l.wohin].map(ab);
    ausbau(&mut c, fonts, q, b.ausbau, hover, s, t);
    if let Some((ketten, punkte)) = b.mass {
        name(&mut c, MASS, l.mass);
        let h = |teil| b.hover == Some(teil);
        haken_mit(
            &mut c,
            fonts,
            ab(l.ketten),
            KETTEN,
            ketten,
            h(Teil::Ketten),
            s,
            t,
        );
        haken_mit(
            &mut c,
            fonts,
            ab(l.punkte),
            PUNKTE,
            punkte,
            h(Teil::Punkte),
            s,
            t,
        );
    }
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
    /// Klick auf An oder Aus von „Ausbau innen“.
    pub ausbau: Option<bool>,
    /// Die Wahl dieser Ansicht in alle sieben übertragen, den Ausbau innen
    /// in die drei mit dem Schalter.
    pub alle: bool,
    /// Klick auf „Maßketten zeigen“ bzw. „Maßbezugspunkte zeigen“.
    pub ketten: bool,
    pub punkte: bool,
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
                    Some(Teil::Ausbau(an)) => out.ausbau = Some(an),
                    Some(Teil::Alle) => out.alle = true,
                    Some(Teil::Ketten) => out.ketten = true,
                    Some(Teil::Punkte) => out.punkte = true,
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
        let l = lage(900.0, 1.0, &t, false);
        assert!(l.zahnrad.x + l.zahnrad.w <= 900.0);
        assert!(l.feld.y > l.zahnrad.y + l.zahnrad.h);
        for q in [l.name, l.haken, l.ausbau, l.an, l.aus, l.alle, l.wohin] {
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
        assert_eq!((l.an.x, l.an.w, l.aus.x), (g.an.x, g.an.w, g.aus.x));
        assert_eq!((l.wohin.x, l.wohin.w), (g.wohin.x, g.wohin.w));
        let mut s = Schalter::default();
        let o = s.handle(&klick(l.haken), &l);
        assert!(!o.consumed && !o.haken, "zu: nur das Zahnrad");
        assert!(s.handle(&klick(l.zahnrad), &l).consumed && s.offen);
        let o = s.handle(&klick(l.haken), &l);
        assert!(o.consumed && o.haken && !o.alle);
        let o = s.handle(&klick(l.alle), &l);
        assert!(o.consumed && o.alle && !o.haken && o.ausbau.is_none());
        let o = s.handle(&klick(l.an), &l);
        assert!(o.consumed && o.ausbau == Some(true) && !o.haken && !o.alle);
        assert_eq!(s.handle(&klick(l.aus), &l).ausbau, Some(false));
        let o = s.handle(&klick(l.wohin), &l);
        assert!(o.consumed && o.ausbau.is_none() && !o.alle, "leise Zeile");
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
            ausbau: false,
            offen: false,
            hover: None,
            mass: None,
            rechts: 900f32.to_bits(),
            scale: 1f32.to_bits(),
        };
        let (zu, _, _) = malen(&b, &fonts, &t);
        b.offen = true;
        let (auf, x, y) = malen(&b, &fonts, &t);
        assert!(auf.width > zu.width && auf.height > zu.height);
        assert!(x as f32 <= l.feld.x && y as f32 <= l.zahnrad.y);
        // Bemaßung (Konzept §7): ohne die Zeile trifft dort nichts, mit ihr
        // melden sich beide Häkchen, das Feld wird höher
        assert!(!s.offen);
        s.handle(&klick(l.zahnrad), &l);
        let o = s.handle(
            &klick(Rect::new(l.alle.x, l.feld.y + l.feld.h + 30.0, 1.0, 1.0)),
            &l,
        );
        assert!(!o.ketten && !o.punkte && !s.offen);
        let m = lage(900.0, 1.0, &t, true);
        assert!(m.feld.h > l.feld.h && (m.alle, m.haken) == (l.alle, l.haken));
        for q in [m.mass, m.ketten, m.punkte] {
            assert!(q.y > m.alle.y + m.alle.h && q.y + q.h <= m.feld.y + m.feld.h);
        }
        s.handle(&klick(m.zahnrad), &m);
        let o = s.handle(&klick(m.ketten), &m);
        assert!(o.consumed && o.ketten && !o.punkte && !o.haken);
        let o = s.handle(&klick(m.punkte), &m);
        assert!(o.consumed && o.punkte && !o.ketten);
        b.mass = Some((true, false));
        let (mit, _, _) = malen(&b, &fonts, &t);
        assert!(mit.height > auf.height);
    }

    /// Die Beschriftungen von „Ausbau innen“ passen in ihre Knöpfe und
    /// Zeilen, auch die leise Zeile (mit Liberation, wenn sie da ist).
    #[test]
    fn beschriftung_passt() {
        let t = Theme::dark();
        let l = lage(900.0, 1.0, &t, false);
        let lib = |n: &str| {
            let p = format!("/usr/share/fonts/truetype/liberation/LiberationSans-{n}.ttf");
            std::fs::read(p).ok().and_then(sk_paint::font::Font::parse)
        };
        if let Some(f) = lib("Bold") {
            for (text, q) in [("An", l.an), ("Aus", l.aus), (ALLE, l.alle)] {
                assert!(f.width(text, t.size.font) + 12.0 <= q.w, "{text}");
            }
        }
        if let Some(f) = lib("Regular") {
            for text in [NAME, AUSBAU, crate::grundriss_schnitt::NAME] {
                assert!(f.width(text, t.size.font) + 4.0 <= l.ausbau.w, "{text}");
            }
            assert!(f.width(WOHIN, t.size.font_small) < l.wohin.w);
        }
    }
}

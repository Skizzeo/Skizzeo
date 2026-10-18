//! Phasenleiste in 3D und Schnitt (Jörn 18.10., „zu 3: a“): oben in der
//! Ansicht links neben dem Zahnrad acht kleine Felder „1“ bis „8“, davor der
//! Name der gezeigten Phase ([`Bauphase`]). Ein Klick springt in die Phase,
//! das Mausrad über der Leiste geht eine weiter oder zurück, Esc oder ein
//! Klick auf „8“ führt zurück zum fertigen Stand. Außerhalb von „Fertig“ ist
//! die Leiste farbig hinterlegt, darunter steht leise die Menge des Schritts
//! aus dem LV. Kein Eingabefeld, keine Einstellung: Die Phase ist nur
//! Ansicht (Jörn 18.10.: den Nutzer nicht mit Einstellungen erschlagen).
//!
//! Module: die Leiste gehört zum Geländemodul, die Zeile mit der Menge
//! braucht dazu die Mengen ([`Teil`], `freischaltung`).

use super::bauphase::Bauphase;
use sk_paint::{Canvas, Rgba};
use sk_platform::{Event, MouseButton};
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, Fonts, Rect};

/// Name vor den Feldern im fertigen Stand.
pub const NAME: &str = "Bauphase";
/// Tooltip am Namen und am Rand der Leiste.
pub const TIP: &str = "Bauphase: Klick springt, Mausrad blättert, Esc zurück zu „Fertig“";

const FELD: f32 = 22.0;
const LUECKE: f32 = 2.0;
const PAD: f32 = 4.0;
const NAME_W: f32 = 128.0;
/// Höhe und Abstand des Zahnrads ([`crate::sicht_zahnrad`]): Die Leiste
/// steht mittig in seiner Zeile, mit Abstand links davon.
const RAD: f32 = 28.0;
const ABSTAND: f32 = 6.0;
const ZEILE: f32 = 18.0;

/// Lage der Leiste in der Ansicht (Pixel).
#[derive(Clone, Debug, PartialEq)]
pub struct Lage {
    /// Die ganze Leiste, darin Name und je Phase ein Feld.
    pub leiste: Rect,
    pub name: Rect,
    pub felder: [Rect; 8],
    /// Leise Zeile mit der Menge, rechtsbündig unter der Leiste.
    pub menge: Rect,
}

/// Leiste links neben dem Zahnrad oben rechts in der Ansicht (`rechts`:
/// linker Rand des Paneels „Ansichten“ in Ansichtspixeln), wie
/// [`crate::sicht_zahnrad::lage`].
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
    let x1 = rechts / s - m - RAD - ABSTAND;
    let breit = 2.0 * PAD + NAME_W + 8.0 * FELD + 7.0 * LUECKE;
    let x0 = x1 - breit;
    let y = m + (RAD - FELD) * 0.5;
    let fx = x0 + PAD + NAME_W;
    Lage {
        leiste: r(x0, m, breit, RAD),
        name: r(x0 + PAD + 4.0, y, NAME_W - 4.0, FELD),
        felder: std::array::from_fn(|i| r(fx + i as f32 * (FELD + LUECKE), y, FELD, FELD)),
        menge: r(x0 - 80.0, m + RAD + 2.0, breit + 80.0, ZEILE),
    }
}

/// Teil der Leiste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Teil {
    /// Das Feld einer Phase.
    Feld(Bauphase),
    /// Der Name und der Rand: nimmt die Maus, tut nichts.
    Leiste,
    /// Die leise Zeile mit der Menge (nicht zu treffen).
    Menge,
}

impl Lage {
    fn treffer(&self, m: (f64, f64)) -> Option<Teil> {
        let feld = Bauphase::ALLE
            .into_iter()
            .zip(&self.felder)
            .find(|(_, q)| q.contains(m.0, m.1))
            .map(|(p, _)| Teil::Feld(p));
        feld.or(self.leiste.contains(m.0, m.1).then_some(Teil::Leiste))
    }
}

/// Was die Leiste zeigt; gleich: kein neues Bild.
#[derive(Clone, Debug, PartialEq)]
pub struct Bild {
    pub phase: Bauphase,
    pub hover: Option<Teil>,
    /// Leise Zeile darunter (Menge des Schritts); `None` ohne.
    pub menge: Option<String>,
    /// Linker Rand des Paneels „Ansichten“ (Bits) und Skalierung (Bits).
    pub rechts: u32,
    pub scale: u32,
}

/// Fläche der Leiste außerhalb von „Fertig“: Paneel, leicht in der
/// Akzentfarbe.
fn getoent(t: &Theme) -> Rgba {
    let (a, b) = (t.ui.accent, t.ui.bg);
    let mix = |x: u8, y: u8| ((x as f32) * 0.3 + (y as f32) * 0.7).round() as u8;
    Rgba(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2), 255)
}

/// Leiste zeichnen: Bild und Lage in der Ansicht.
pub fn malen(b: &Bild, fonts: &Fonts, t: &Theme) -> (Canvas, i32, i32) {
    let s = f32::from_bits(b.scale);
    let l = lage(f32::from_bits(b.rechts), s, t);
    let sh = (t.size.panel_shadow * s).ceil();
    let alles = match b.menge {
        Some(_) => Rect::new(
            l.menge.x,
            l.leiste.y,
            l.leiste.x + l.leiste.w - l.menge.x,
            l.menge.y + l.menge.h - l.leiste.y,
        ),
        None => l.leiste,
    };
    let (w, h) = ((alles.w + 2.0 * sh) as usize, (alles.h + 2.0 * sh) as usize);
    let mut c = Canvas::new(w, h);
    let (dx, dy) = (alles.x - sh, alles.y - sh);
    let ab = |q: Rect| Rect::new(q.x - dx, q.y - dy, q.w, q.h);
    let offen = b.phase.offen();
    let fill = if offen { getoent(t) } else { t.ui.bg };
    widgets::panel_filled(&mut c, ab(l.leiste), s, t, fill);
    let schreib = |c: &mut Canvas, text: &str, q: Rect, px: f32, farbe: Rgba, rechts: bool| {
        if let Some(f) = fonts.regular.as_ref() {
            let px = px * s;
            let y = q.y + (q.h + f.cap_height(px)) * 0.5;
            let x = if rechts {
                q.x + q.w - f.width(text, px)
            } else {
                q.x
            };
            f.draw(c, text, px, x.round(), y.round(), farbe);
        }
    };
    let (name, farbe) = if offen {
        (format!("{} {}", b.phase.nr(), b.phase.name()), t.ui.text)
    } else {
        (NAME.to_string(), t.ui.text_dim)
    };
    schreib(&mut c, &name, ab(l.name), t.size.font, farbe, false);
    for (p, q) in Bauphase::ALLE.into_iter().zip(l.felder) {
        let st = ButtonState {
            hover: b.hover == Some(Teil::Feld(p)),
            active: p == b.phase,
            ..Default::default()
        };
        widgets::button(&mut c, fonts, ab(q), &p.nr().to_string(), st, s, t);
    }
    if let Some(m) = &b.menge {
        schreib(
            &mut c,
            m,
            ab(l.menge),
            t.size.font_small,
            t.ui.text_dim,
            true,
        );
    }
    (c, dx as i32, dy as i32)
}

/// Bedienzustand: Teil unter der Maus.
#[derive(Clone, Debug, Default)]
pub struct Schalter {
    pub hover: Option<Teil>,
}

/// Ergebnis eines Ereignisses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ausgang {
    pub consumed: bool,
    pub redraw: bool,
    /// In diese Phase springen.
    pub phase: Option<Bauphase>,
}

impl Schalter {
    /// Mausereignis in der Ansicht (ohne Titelleiste); `jetzt`: die
    /// gezeigte Phase.
    pub fn handle(&mut self, e: &Event, l: &Lage, jetzt: Bauphase) -> Ausgang {
        let mut out = Ausgang::default();
        match *e {
            Event::MouseMove { x, y, .. } => {
                let t = l.treffer((x, y));
                out.redraw = t != self.hover;
                self.hover = t;
                out.consumed = t.is_some();
            }
            Event::MouseLeave => out.redraw = self.hover.take().is_some(),
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let t = l.treffer((x, y));
                out.consumed = t.is_some();
                if let Some(Teil::Feld(p)) = t {
                    out.phase = (p != jetzt).then_some(p);
                    out.redraw = true;
                }
            }
            Event::Wheel { x, y, delta, .. } if l.leiste.contains(x, y) => {
                out.consumed = true;
                // Rad nach unten (negativ): eine Phase weiter
                let d = if delta < 0.0 { 1 } else { -1 };
                let p = jetzt.schritt(d);
                out.phase = (p != jetzt).then_some(p);
                out.redraw = out.phase.is_some();
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

    fn rad(r: Rect, delta: f64) -> Event {
        Event::Wheel {
            x: (r.x + r.w * 0.5) as f64,
            y: (r.y + r.h * 0.5) as f64,
            delta,
            mods: Modifiers::default(),
        }
    }

    /// Die Leiste steht links neben dem Zahnrad in seiner Zeile, ohne es
    /// zu berühren; ein Klick springt, das Rad schaltet weiter und bleibt in
    /// 1 bis 8, daneben nimmt sie nichts.
    #[test]
    fn klick_und_rad() {
        let t = Theme::dark();
        let l = lage(900.0, 1.0, &t);
        let z = crate::sicht_zahnrad::lage(900.0, 1.0, &t, false);
        assert!(l.leiste.x + l.leiste.w < z.zahnrad.x);
        assert_eq!(l.leiste.y, z.zahnrad.y);
        assert_eq!(l.leiste.h, z.zahnrad.h);
        for q in l.felder.iter().chain([&l.name]) {
            assert!(q.x >= l.leiste.x && q.x + q.w <= l.leiste.x + l.leiste.w);
            assert!(q.y >= l.leiste.y && q.y + q.h <= l.leiste.y + l.leiste.h);
        }
        assert!(l.menge.y >= l.leiste.y + l.leiste.h);
        let mut s = Schalter::default();
        let o = s.handle(&klick(l.felder[3]), &l, Bauphase::Fertig);
        assert!(o.consumed && o.phase == Some(Bauphase::Fuellsand));
        let o = s.handle(&klick(l.felder[7]), &l, Bauphase::Fertig);
        assert!(o.consumed && o.phase.is_none(), "schon da");
        let o = s.handle(&klick(l.name), &l, Bauphase::Fertig);
        assert!(o.consumed && o.phase.is_none());
        let o = s.handle(
            &klick(Rect::new(5.0, 400.0, 1.0, 1.0)),
            &l,
            Bauphase::Fertig,
        );
        assert!(!o.consumed);
        let o = s.handle(&rad(l.name, -1.0), &l, Bauphase::Graeben);
        assert_eq!(o.phase, Some(Bauphase::Gruendung));
        let o = s.handle(&rad(l.felder[0], 1.0), &l, Bauphase::Graeben);
        assert_eq!(o.phase, Some(Bauphase::Fuellsand));
        let o = s.handle(&rad(l.felder[0], -1.0), &l, Bauphase::Fertig);
        assert!(o.consumed && o.phase.is_none(), "bleibt bei 8");
        let o = s.handle(
            &rad(Rect::new(5.0, 400.0, 1.0, 1.0), -1.0),
            &l,
            Bauphase::Graeben,
        );
        assert!(
            !o.consumed && o.phase.is_none(),
            "daneben zoomt die Ansicht"
        );
        // Hover auf einem Feld nimmt die Maus
        let m = Event::MouseMove {
            x: (l.felder[1].x + 2.0) as f64,
            y: (l.felder[1].y + 2.0) as f64,
            mods: Modifiers::default(),
        };
        let o = s.handle(&m, &l, Bauphase::Fertig);
        assert!(o.consumed && o.redraw && s.hover == Some(Teil::Feld(Bauphase::Mutterboden)));
    }

    /// Mit Menge wird das Bild höher; der Name passt in sein Feld (mit
    /// Liberation, wenn sie da ist).
    #[test]
    fn bild_und_namen() {
        let t = Theme::dark();
        let fonts = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        let mut b = Bild {
            phase: Bauphase::Fertig,
            hover: None,
            menge: None,
            rechts: 900f32.to_bits(),
            scale: 1f32.to_bits(),
        };
        let (ohne, _, _) = malen(&b, &fonts, &t);
        b.phase = Bauphase::Fuellsand;
        b.menge = Some("Füllsand einbauen 64,1 m³".into());
        let (mit, _, _) = malen(&b, &fonts, &t);
        assert!(mit.height > ohne.height);
        let l = lage(900.0, 1.0, &t);
        let p = "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf";
        if let Some(f) = std::fs::read(p).ok().and_then(sk_paint::font::Font::parse) {
            for ph in Bauphase::ALLE {
                let name = format!("{} {}", ph.nr(), ph.name());
                assert!(f.width(&name, t.size.font) <= l.name.w, "{name}");
            }
            let lang = "Gräben ausheben 120,5 m³ · Kies 30,2 m³";
            assert!(f.width(lang, t.size.font_small) <= l.menge.w);
        }
    }
}

//! Schrift als Dreiecke (Konzept §8): Die Umrisse der Glyphen werden einmal
//! je Zeichen in Dreiecke zerlegt ([`sk_paint::Path::triangles`]) und
//! gemerkt. So liegt eine Maßzahl in 3D flach auf der Geschossebene und ist
//! im Grundriss in jeder Zoomstufe scharf.

use sk_paint::font::Font;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Zerlegt wird in dieser Größe (Geviert in Einheiten), mit dieser
/// Toleranz der Kurvenunterteilung ([`sk_paint::Path::flatten`]): Die Umrisse
/// weichen höchstens etwa 3 Tausendstel des Gevierts ab (bei 2,5 mm Schrift
/// rund 0,01 mm auf dem Papier), eine Ziffer hat 10 bis 100 Dreiecke
/// (Review W30).
const GEVIERT: f32 = 1000.0;
const TOLERANZ: f32 = 20.0;

/// Dreiecke einer Glyphe in Gevierten, Grundlinie bei y = 0, y nach oben.
struct Glyphe {
    dreiecke: Vec<[[f64; 2]; 3]>,
    vorschub: f64,
}

/// Eine Schrift mit gemerkten Glyphen.
pub struct Schrift {
    font: Font,
    glyphen: RefCell<HashMap<char, Rc<Glyphe>>>,
    /// Höhe der Ziffern in Gevierten (Oberkante der „8“).
    ziffer: f64,
}

/// Dreiecke eines Textes in der Ebene: x längs, y nach oben (mm).
pub type Dreiecke = Vec<[[f64; 2]; 3]>;

impl Schrift {
    pub fn new(font: Font) -> Schrift {
        let mut s = Schrift {
            font,
            glyphen: RefCell::default(),
            ziffer: 0.7,
        };
        let acht = s.glyphe('8');
        let oben = acht
            .dreiecke
            .iter()
            .flat_map(|d| d.iter().map(|p| p[1]))
            .fold(0.0, f64::max);
        if oben > 0.3 && oben < 1.2 {
            s.ziffer = oben;
        }
        s
    }

    fn glyphe(&self, c: char) -> Rc<Glyphe> {
        if let Some(g) = self.glyphen.borrow().get(&c) {
            return g.clone();
        }
        let mut b = [0u8; 4];
        let text = c.encode_utf8(&mut b);
        let k = 1.0 / f64::from(GEVIERT);
        let dreiecke = self
            .font
            .text_path(text, GEVIERT, 0.0, 0.0)
            .triangles(TOLERANZ)
            .into_iter()
            .map(|d| d.map(|p| [f64::from(p.x) * k, -f64::from(p.y) * k]))
            .collect();
        let g = Rc::new(Glyphe {
            dreiecke,
            vorschub: f64::from(self.font.width(text, GEVIERT)) * k,
        });
        self.glyphen.borrow_mut().insert(c, g.clone());
        g
    }

    /// Breite von `text` bei der Ziffernhöhe `h` (mm).
    pub fn breite(&self, text: &str, h: f64) -> f64 {
        let em = h / self.ziffer;
        text.chars().map(|c| self.glyphe(c).vorschub).sum::<f64>() * em
    }

    /// `text` als Dreiecke bei der Ziffernhöhe `h`, Grundlinie bei `(x, 0)`;
    /// hängt sie an `out` und gibt die Breite zurück.
    pub fn zeile(&self, text: &str, h: f64, x: f64, out: &mut Dreiecke) -> f64 {
        let em = h / self.ziffer;
        let mut stift = x;
        for c in text.chars() {
            let g = self.glyphe(c);
            out.extend(
                g.dreiecke
                    .iter()
                    .map(|d| d.map(|p| [stift + p[0] * em, p[1] * em])),
            );
            stift += g.vorschub * em;
        }
        stift - x
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn schrift() -> Option<Schrift> {
        let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
        std::fs::read(lib.join("LiberationSans-Regular.ttf"))
            .ok()
            .and_then(Font::parse)
            .map(Schrift::new)
    }

    fn flaeche(v: &Dreiecke) -> f64 {
        v.iter()
            .map(|[a, b, c]| {
                0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs()
            })
            .sum()
    }

    #[test]
    fn ziffern_stehen_in_der_gewuenschten_hoehe() {
        let Some(s) = schrift() else {
            eprintln!("Liberation Sans fehlt, Test übersprungen");
            return;
        };
        let mut v = Vec::new();
        let w = s.zeile("1,25", 250.0, 100.0, &mut v);
        assert!(!v.is_empty());
        assert!((w - s.breite("1,25", 250.0)).abs() < 1e-9);
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        let (mut x0, mut x1) = (f64::MAX, f64::MIN);
        for p in v.iter().flatten() {
            lo = lo.min(p[1]);
            hi = hi.max(p[1]);
            x0 = x0.min(p[0]);
            x1 = x1.max(p[0]);
        }
        // Ziffern von der Grundlinie bis 250 mm, das Komma darunter
        assert!((hi - 250.0).abs() < 8.0, "Oberkante {hi}");
        assert!(lo < 0.0 && lo > -80.0, "Unterkante {lo}");
        assert!(x0 >= 100.0 && x1 <= 100.0 + w + 1e-6);
        // Gegenformen bleiben frei: die „0“ hat weniger Fläche als ihr Rahmen
        let mut o = Vec::new();
        let b = s.zeile("0", 250.0, 0.0, &mut o);
        assert!(flaeche(&o) < 0.6 * b * 250.0);
    }
}

//! Auswahlliste neben dem Paneel „Werkzeuge“ (E6): die Erweiterungen nach
//! Gruppe › Name am Knopf „Erweiterungen“ und die Typen am Typ-Chip des
//! Werkzeugs. Aussehen wie die Typ-Liste ([`crate::type_menu`]), ohne
//! Vorschaubild.

use crate::ui::Id;
use sk_paint::{Canvas, Path};
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, Fonts, Rect};

/// Maße in dip.
const PAD: f32 = 8.0;
const KOPF: f32 = 26.0;
const GRUPPE: f32 = 24.0;
const EINTRAG: f32 = 40.0;
const FUSS: f32 = 24.0;
const BREITE: f32 = 300.0;
const ABSTAND: f32 = 10.0;

/// Eine Zeile der Liste.
#[derive(Clone, Debug, PartialEq)]
pub enum Zeile {
    /// Überschrift einer Gruppe.
    Gruppe(String),
    /// Wählbarer Eintrag: Name, blasse Angabe darunter.
    Eintrag { name: String, detail: String },
}

/// Was unter der Maus liegt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// Eintrag `i` (gezählt nur über Einträge).
    Eintrag(usize),
    Inside,
    Outside,
}

pub struct Auswahl {
    /// Knopf, an dem die Liste hängt.
    pub knopf: Id,
    pub titel: String,
    pub zeilen: Vec<Zeile>,
    /// Gewählter Eintrag (Akzentstrich).
    pub aktuell: Option<usize>,
    pub hover: Option<usize>,
    /// Blasser Satz unten; leer ohne Fuß.
    pub fuss: String,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
}

impl Auswahl {
    /// Liste rechts neben dem Paneel `panel`, oben auf Höhe von `anchor`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        knopf: Id,
        titel: String,
        zeilen: Vec<Zeile>,
        aktuell: Option<usize>,
        fuss: String,
        anchor: Rect,
        panel: Rect,
        scale: f32,
        win: (f32, f32),
    ) -> Auswahl {
        let mut a = Auswahl {
            knopf,
            titel,
            zeilen,
            aktuell,
            hover: None,
            fuss,
            x: 0.0,
            y: 0.0,
            scale,
        };
        let (w, h) = a.size();
        let s = scale;
        // Neben dem Paneel, beim Paneel rechts im Fenster links davon
        let x = if panel.x + panel.w * 0.5 > win.0 * 0.5 {
            panel.x - w - ABSTAND * s
        } else {
            panel.x + panel.w + ABSTAND * s
        };
        let y = anchor.y - (PAD + KOPF) * s;
        a.x = x.clamp(0.0, (win.0 - w).max(0.0)).round();
        a.y = y.clamp(0.0, (win.1 - h).max(0.0)).round();
        a
    }

    fn hoehe(z: &Zeile) -> f32 {
        match z {
            Zeile::Gruppe(_) => GRUPPE,
            Zeile::Eintrag { .. } => EINTRAG,
        }
    }

    /// Zahl der Einträge.
    pub fn len(&self) -> usize {
        self.zeilen
            .iter()
            .filter(|z| matches!(z, Zeile::Eintrag { .. }))
            .count()
    }

    pub fn size(&self) -> (f32, f32) {
        let s = self.scale;
        let fuss = if self.fuss.is_empty() { 0.0 } else { FUSS };
        let h = PAD + KOPF + self.zeilen.iter().map(Auswahl::hoehe).sum::<f32>() + fuss + PAD;
        ((BREITE * s).round(), (h * s).round())
    }

    pub fn rect(&self) -> Rect {
        let (w, h) = self.size();
        Rect::new(self.x, self.y, w, h)
    }

    /// Zeilen mit ihrer Lage im Fenster und der Nummer des Eintrags.
    fn lagen(&self) -> Vec<(Rect, &Zeile, Option<usize>)> {
        let s = self.scale;
        let (w, _) = self.size();
        let mut y = self.y + (PAD + KOPF) * s;
        let mut n = 0;
        let mut out = Vec::new();
        for z in &self.zeilen {
            let h = Auswahl::hoehe(z) * s;
            let i = matches!(z, Zeile::Eintrag { .. }).then(|| {
                n += 1;
                n - 1
            });
            out.push((Rect::new(self.x + PAD * s, y, w - 2.0 * PAD * s, h), z, i));
            y += h;
        }
        out
    }

    pub fn hit(&self, x: f64, y: f64) -> Hit {
        if !self.rect().contains(x, y) {
            return Hit::Outside;
        }
        self.lagen()
            .into_iter()
            .find(|(r, _, i)| i.is_some() && r.contains(x, y))
            .and_then(|(_, _, i)| i)
            .map_or(Hit::Inside, Hit::Eintrag)
    }

    /// Pfeiltaste: nächster bzw. voriger Eintrag.
    pub fn step(&mut self, down: bool) -> Option<usize> {
        let n = self.len();
        if n == 0 {
            return None;
        }
        let i = match (self.hover.or(self.aktuell), down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        self.hover = Some(i);
        Some(i)
    }

    /// Bild samt Schatten und seine Lage im Fenster.
    pub fn paint(&self, t: &Theme, fonts: &Fonts) -> (Canvas, i32, i32) {
        let s = self.scale;
        let u = &t.ui;
        let m = (t.size.panel_shadow * s).round();
        let (w, h) = self.size();
        let mut c = Canvas::new((w + 2.0 * m) as usize, (h + 2.0 * m) as usize);
        c.set_origin(self.x - m, self.y - m);
        widgets::panel_filled(&mut c, Rect::new(self.x, self.y, w, h), s, t, u.menu_bg);
        let b = s.round().max(1.0);
        let rad = t.size.corner_radius * s;
        let mut p = Path::new();
        p.rounded_rect(self.x, self.y, w, h, rad);
        p.rounded_rect_hole(self.x + b, self.y + b, w - 2.0 * b, h - 2.0 * b, rad - b);
        c.fill(&p, u.accent);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let x0 = self.x + (PAD + 6.0) * s;
        let pd = t.size.font_detail * s;
        widgets::text(
            &mut c,
            bold,
            &self.titel,
            pd,
            x0,
            (self.y + (PAD + 14.0) * s).round(),
            u.text_dim,
        );
        for (r, z, i) in self.lagen() {
            match z {
                Zeile::Gruppe(g) => {
                    let y = (r.y + 16.0 * s).round();
                    widgets::text(&mut c, bold, g, pd, r.x + 6.0 * s, y, u.text_dim);
                }
                Zeile::Eintrag { name, detail } => {
                    let cur = i.is_some() && i == self.aktuell;
                    let hov = i.is_some() && i == self.hover;
                    if cur || hov {
                        let mut p = Path::new();
                        p.rounded_rect(r.x, r.y, r.w, r.h, 6.0 * s);
                        c.fill(&p, if hov { u.hover } else { u.pressed });
                    }
                    if cur {
                        let bar = (3.0 * s).round();
                        c.fill_rect(r.x, r.y + 4.0 * s, bar, r.h - 8.0 * s, u.accent);
                    }
                    let tx = r.x + 14.0 * s;
                    let max_w = r.x + r.w - tx - 8.0 * s;
                    let font = if cur { bold } else { regular };
                    let px = t.size.font * s;
                    let n = widgets::ellipsize(font, name, px, max_w);
                    widgets::text(&mut c, font, &n, px, tx, (r.y + 18.0 * s).round(), u.text);
                    let d = widgets::ellipsize(regular, detail, pd, max_w);
                    let y = (r.y + 33.0 * s).round();
                    widgets::text(&mut c, regular, &d, pd, tx, y, u.text_dim);
                }
            }
        }
        if !self.fuss.is_empty() {
            let y = (self.y + h - (PAD + 8.0) * s).round();
            widgets::text(&mut c, regular, &self.fuss, pd, x0, y, u.text_dim);
        }
        (c, (self.x - m) as i32, (self.y - m) as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eintraege_ohne_ueberschriften() {
        let e = |n: &str| Zeile::Eintrag {
            name: n.into(),
            detail: "x".into(),
        };
        let zeilen = vec![
            Zeile::Gruppe("Tragwerk".into()),
            e("Bodenplatte"),
            e("Stütze"),
            Zeile::Gruppe("Treppen und Geländer".into()),
            e("Treppe"),
        ];
        let anchor = Rect::new(46.0, 236.0, 178.0, 40.0);
        let panel = Rect::new(32.0, 84.0, 206.0, 300.0);
        let mut a = Auswahl::new(
            Id::Ext,
            "Erweiterungen".into(),
            zeilen,
            Some(1),
            String::new(),
            anchor,
            panel,
            1.0,
            (1600.0, 1000.0),
        );
        assert_eq!(a.len(), 3);
        assert_eq!(a.x, 248.0);
        let l = a.lagen();
        // Überschrift trifft nichts, der Eintrag danach ist 0
        let (r, ..) = l[0];
        assert_eq!(a.hit((r.x + 5.0) as f64, (r.y + 5.0) as f64), Hit::Inside);
        let (r, ..) = l[4];
        assert_eq!(
            a.hit((r.x + 5.0) as f64, (r.y + 5.0) as f64),
            Hit::Eintrag(2)
        );
        assert_eq!(a.hit(1.0, 1.0), Hit::Outside);
        assert_eq!(a.step(true), Some(2));
        assert_eq!(a.step(true), Some(0));
        let f = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        let (c, ..) = a.paint(&Theme::dark(), &f);
        assert!(c.width > 0);
    }
}

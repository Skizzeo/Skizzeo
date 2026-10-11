//! Schwebende Großansicht des Paneels „Geschosse“ (Jörn 11.10.): Im linken
//! Rand zeigt das Paneel nur an; ein Klick irgendwo darauf öffnet es groß,
//! mit derselben Leiste wie die große LV-Vorschau ([`sk_ui::schwebend`]),
//! aber ohne Abdunkeln: das Modell daneben zeigt jedes Ziehen sofort.
//! Dort wird gezogen, getippt und ein Geschoss gewählt. Esc,
//! „Schließen ×“ oder ein Klick daneben schließt.
//!
//! Platz für später: Jedes Geschoss bekommt zwei untergeordnete Linien,
//! Brüstung und Sturz von Öffnungen. Die Großansicht ist dafür breiter als
//! das Paneel und zeichnet die Geschosse im größten Maßstab, der ins Fenster
//! passt (bis [`PPM`]); bei 60 dip je m ist ein Geschoss von 2,50 m 150 dip
//! hoch, gut drei Zeilen für Linie und Beschriftung.

use super::*;
use sk_ui::schwebend::{self, Klick, Schwebend};

/// Breite der Großansicht (dip): Namen, Koten und Ketten wie im Paneel,
/// dazwischen Luft für die Beschriftung von Brüstung und Sturz.
pub(super) const BREITE: f32 = 460.0;
/// Größter Maßstab der Großansicht (dip je m); das Paneel nimmt 40.
const PPM: f32 = 120.0;
/// Titel in der Leiste.
pub const TITEL: &str = "Geschosse und Höhen";

impl Ui {
    /// Ist die Großansicht offen?
    pub fn levels_gross(&self) -> bool {
        self.levels_gross
    }

    /// Öffnet bzw. schließt die Großansicht; `true`, wenn sich etwas
    /// ändert. Neben dem Dialog „Gebäude erstellen“ öffnet sie nicht, der
    /// zeigt das Paneel selbst.
    pub fn set_levels_gross(&mut self, on: bool) -> bool {
        if on == self.levels_gross || (on && self.dialog) || self.level_drag.is_some() {
            return false;
        }
        self.levels_gross = on;
        self.hover = None;
        self.pressed = None;
        if self.edit.as_ref().is_some_and(|e| e.field.is_level()) {
            self.edit = None;
        }
        // Andere Größe: das Bild entsteht neu
        self.images[panel_index(Panel::Levels)] = None;
        true
    }

    /// Abstand der Großansicht von der Titelleiste (Pixel): Leiste und Rand.
    pub(super) fn gross_oben(&self) -> f32 {
        let s = self.scale;
        (schwebend::LEISTE * s).round() + (self.size.panel_margin * s).round()
    }

    /// Aufteilung der Großansicht: größter Maßstab bis [`PPM`], bei dem sie
    /// über dem Fensterrand endet.
    pub(super) fn levels_layout_gross(&self) -> LevelsLayout {
        self.levels_layout_at(self.top as f32 + self.gross_oben(), PPM, false)
    }

    /// Rahmen der Großansicht: das Fenster unter der Titelleiste.
    pub fn gross_rahmen(&self, win_w: u32, top: u32) -> Schwebend {
        let t = top as f32;
        let flaeche = (0.0, t, win_w as f32, (self.win_h - t).max(0.0));
        Schwebend::neu(flaeche, self.scale, win_w as f32, &self.fonts)
    }

    /// Treffer bei offener Großansicht: alles unter der Titelleiste gehört
    /// ihr. Im Paneel wie sonst, in der Leiste „Schließen ×“, daneben nichts.
    pub(super) fn hit_gross(
        &self,
        x: f64,
        y: f64,
        win_w: u32,
        top: u32,
    ) -> Option<(Panel, Option<Id>)> {
        if y < top as f64 {
            return None;
        }
        let r = self.rect(Panel::Levels, win_w, top);
        if r.contains(x, y) {
            let (lx, ly) = (x - r.x as f64, y - r.y as f64);
            let id = self
                .buttons(Panel::Levels)
                .into_iter()
                .find(|(_, b, _)| b.contains(lx, ly))
                .map(|b| b.0)
                .or_else(|| self.grip_at(lx, ly).map(Id::Grip));
            return Some((Panel::Levels, id));
        }
        let zu = self
            .gross_rahmen(win_w, top)
            .klick(x as f32, y as f32, false)
            == Klick::Zu;
        Some((Panel::Levels, zu.then_some(Id::LevelsClose)))
    }

    /// Liegt `(x, y)` neben der Großansicht (nicht auf „Schließen ×“)?
    pub(super) fn gross_daneben(&self, x: f64, y: f64, win_w: u32, top: u32) -> bool {
        y >= top as f64
            && !self.rect(Panel::Levels, win_w, top).contains(x, y)
            && self
                .gross_rahmen(win_w, top)
                .klick(x as f32, y as f32, false)
                == Klick::Daneben
    }

    /// Leiste der Großansicht über die Fensterbreite, Titel bündig mit dem
    /// Inhalt des Paneels; liegt direkt unter der Titelleiste.
    pub fn paint_gross_leiste(&self, win_w: u32) -> Canvas {
        let s = self.scale;
        let h = (schwebend::LEISTE * s).round();
        let mut c = Canvas::new(win_w as usize, h as usize);
        let r = Schwebend::neu((0.0, 0.0, win_w as f32, h), s, win_w as f32, &self.fonts);
        let x = (self.size.panel_margin * s).round() + self.size.panel_pad * s;
        let hot = self.hover == Some(Id::LevelsClose);
        r.leiste(&mut c, &self.fonts, (TITEL, x), schwebend::HINWEIS, hot);
        c
    }

    /// Rand in Akzent um das kleine Paneel unter der Maus: ein Klick öffnet
    /// die Großansicht.
    pub(super) fn paint_open_ring(&self, t: &Theme, c: &mut Canvas, panel_h: f32) {
        let (s, z) = (self.scale, &self.size);
        let m = (z.panel_shadow * s).round();
        let w = (self.width(Panel::Levels) * s).round();
        let rad = z.corner_radius * s;
        let b = (1.5 * s).max(1.0);
        let mut p = Path::new();
        p.rounded_rect(m, m, w, panel_h, rad);
        p.rounded_rect_hole(
            m + b,
            m + b,
            w - 2.0 * b,
            panel_h - 2.0 * b,
            (rad - b).max(0.0),
        );
        c.fill(&p, t.ui.accent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

    const M: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    fn ui() -> (Ui, Scene) {
        let s = Scene::new();
        let mut ui = Ui::new(1.0, &Theme::dark());
        ui.fit(1.0, 1440, 900);
        ui.set_levels(s.levels());
        (ui, s)
    }

    fn down(x: f64, y: f64) -> Event {
        Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        }
    }

    fn up(x: f64, y: f64) -> Event {
        Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        }
    }

    /// Klick mit Bewegung dorthin; was das Paneel als geklickt meldet.
    fn klick(ui: &mut Ui, x: f64, y: f64) -> Option<Id> {
        let mut out = None;
        for e in [Event::MouseMove { x, y, mods: M }, down(x, y), up(x, y)] {
            out = out.or(ui.handle(&e, 1440, 32).clicked);
        }
        out
    }

    fn mitte(r: Rect) -> (f64, f64) {
        ((r.x + r.w * 0.5) as f64, (r.y + r.h * 0.5) as f64)
    }

    /// Klein meldet jede Stelle des Paneels „öffnen“, auch Griffe, Zahlen
    /// und Namen; der Zeiger ist die Hand.
    #[test]
    fn klein_zeigt_nur_an() {
        let (mut ui, _) = ui();
        let r = ui.rect(Panel::Levels, 1440, 32);
        let mut stellen = vec![mitte(r), (r.x as f64 + 6.0, r.y as f64 + 6.0)];
        for (_, b, _) in ui.level_buttons() {
            stellen.push((mitte(b).0 + r.x as f64, mitte(b).1 + r.y as f64));
        }
        for (x, y) in stellen {
            assert_eq!(klick(&mut ui, x, y), Some(Id::LevelsOpen), "({x}, {y})");
            assert_eq!(ui.cursor(), Cursor::Hand);
            assert!(ui.edit.is_none() && ui.level_dragging().is_none());
        }
    }

    /// Groß: breiter, unter der Leiste, größerer Maßstab; im
    /// Paneel greifen Zahlen und Griffe wie bisher.
    #[test]
    fn gross_ist_groesser_und_bedienbar() {
        let (mut ui, _) = ui();
        let klein = ui.rect(Panel::Levels, 1440, 32);
        let ppm = ui.levels_layout().ppm;
        assert!(ui.set_levels_gross(true));
        assert!(!ui.set_levels_gross(true));
        let r = ui.rect(Panel::Levels, 1440, 32);
        let l = ui.levels_layout();
        assert!(r.w > 2.0 * klein.w && r.h > klein.h, "{r:?} {klein:?}");
        assert_eq!(r.y, 32.0 + schwebend::LEISTE + 12.0);
        assert!(r.y + r.h <= 900.0 && l.ppm > 2.0 * ppm, "{l:?}");
        let og = ui.levels.bands.iter().find(|b| b.name == "OG").unwrap().id;
        let f = Field::StoreyHeight(og);
        let b = ui.field_rect(f).unwrap();
        let (x, y) = (mitte(b).0 + r.x as f64, mitte(b).1 + r.y as f64);
        ui.handle(&Event::MouseMove { x, y, mods: M }, 1440, 32);
        ui.handle(&down(x, y), 1440, 32);
        assert_eq!(ui.edit.as_ref().map(|e| e.field), Some(f));
        ui.key(Key::Escape, true, M);
        assert!(ui.edit.is_none() && ui.levels_gross());
        let n = ui
            .level_buttons()
            .into_iter()
            .find(|b| b.0 == Id::Storey(og))
            .unwrap();
        let (x, y) = (mitte(n.1).0 + r.x as f64, mitte(n.1).1 + r.y as f64);
        assert_eq!(klick(&mut ui, x, y), Some(Id::Storey(og)));
    }

    /// Daneben schließt beim Drücken, eine Eingabe gilt dabei; in der
    /// Leiste schließt „Schließen ×“, die Titelleiste gehört nicht dazu.
    #[test]
    fn klick_daneben_und_schliessen() {
        let (mut ui, s) = ui();
        ui.set_levels_gross(true);
        let r = ui.rect(Panel::Levels, 1440, 32);
        let og = s.levels().bands.iter().find(|b| b.name == "OG").unwrap().id;
        let f = Field::StoreyHeight(og);
        assert!(ui.focus_field(f).changed.contains(&Id::Field(f)));
        if let Some(e) = ui.edit.as_mut() {
            e.text = "3,10".into();
        }
        let x = (r.x + r.w + 200.0) as f64;
        let out = ui.handle(&down(x, 500.0), 1440, 32);
        assert_eq!(out.submit, Some((f, 3100.0)));
        assert!(out.consumed && out.clicked.is_none() && ui.edit.is_none());
        // Erst das Loslassen schließt, damit es nicht in die Ansicht fällt
        let out = ui.handle(&up(x + 30.0, 520.0), 1440, 32);
        assert_eq!(out.clicked, Some(Id::LevelsClose));
        // „Schließen ×“: Hand und Klick beim Loslassen
        let z = ui.gross_rahmen(1440, 32).zu;
        let (x, y) = ((z.0 + z.2 * 0.5) as f64, (z.1 + z.3 * 0.5) as f64);
        assert_eq!(klick(&mut ui, x, y), Some(Id::LevelsClose));
        assert_eq!(ui.cursor(), Cursor::Hand);
        // Titelleiste und leere Stellen im Paneel schließen nicht
        assert!(ui.hit(400.0, 10.0, 1440, 32).is_none());
        assert_eq!(
            klick(&mut ui, (r.x + 30.0) as f64, (r.y + 6.0) as f64),
            None
        );
        assert!(ui.over(1000.0, 600.0, 1440, 32), "modal: nichts dahinter");
        assert!(ui.set_levels_gross(false));
        assert!(!ui.over(1000.0, 600.0, 1440, 32));
    }

    /// Neben dem Dialog „Gebäude erstellen“ öffnet die Großansicht nicht.
    #[test]
    fn nicht_neben_dem_dialog() {
        let (mut ui, _) = ui();
        ui.dialog = true;
        assert!(!ui.set_levels_gross(true));
    }

    /// Leiste: Fensterbreite, dunkel; das kleine Paneel bekommt unter der
    /// Maus einen Rand in Akzent.
    #[test]
    fn leiste_und_rand() {
        let th = Theme::dark();
        let (mut ui, _) = ui();
        let c = ui.paint_gross_leiste(1440);
        assert_eq!((c.width, c.height), (1440, schwebend::LEISTE as usize));
        let px = c.to_rgba8();
        assert_eq!(&px[(4 * 1440 + 2) * 4..][..3], &[34, 34, 36]);
        let r = ui.rect(Panel::Levels, 1440, 32);
        let ohne = ui.paint(&th, Panel::Levels, 1440, 32).0.to_rgba8();
        ui.handle(
            &Event::MouseMove {
                x: mitte(r).0,
                y: mitte(r).1,
                mods: M,
            },
            1440,
            32,
        );
        let patch = ui.repaint_button(&th, Id::LevelsOpen).expect("Paneelbild");
        let mit = ui.paint(&th, Panel::Levels, 1440, 32).0.to_premul_rgba8();
        assert!(patch.px == mit);
        assert!(ohne != ui.paint(&th, Panel::Levels, 1440, 32).0.to_rgba8());
    }
}

//! Geschossmanager im eigenen Fenster (Jörn 11.10.): Das Diagramm des
//! Paneels „Geschosse“ steht links neben dem Hauptfenster wie das
//! Mengenfenster rechts, auch auf einem zweiten Bildschirm; das Modell
//! bleibt ganz zu sehen. Im Hauptfenster bleibt unten links nur der Knopf
//! „Geschossmanager“, der es öffnet.
//!
//! Das Fenster hat seine eigene [`Ui`] im Modus `fenster`: nur das Paneel
//! „Geschosse“, über die ganze Fläche unter der Titelleiste, ohne Rahmen.
//! Platz für später: Jedes Geschoss bekommt zwei untergeordnete Linien,
//! Brüstung und Sturz von Öffnungen; dafür zeichnet das Fenster im größten
//! Maßstab, der hineinpasst (bis [`PPM`]).

use super::*;

/// Größter Maßstab im Fenster (dip je m).
pub(super) const PPM: f32 = 120.0;
/// Titel des Fensters.
pub const TITEL: &str = "Geschossmanager";
/// Beschriftung des Knopfs im Hauptfenster.
pub const KNOPF: &str = "Geschossmanager";

impl Ui {
    /// Oberfläche des Geschossfensters.
    pub fn fenster(scale: f32, theme: &Theme) -> Ui {
        let mut ui = Ui::new(scale, theme);
        ui.fenster = true;
        ui
    }

    /// Größe des Geschossfensters (Pixel), Bildschirmskalierung und Höhe der
    /// Titelleiste; `true`, wenn sich etwas ändert. Anders als im
    /// Hauptfenster schrumpft nichts mit: das Fenster selbst ist schmal.
    pub fn fit_fenster(&mut self, dpi: f32, w: u32, h: u32, top: u32) -> bool {
        let fw = w as f32 / dpi;
        let neu = (dpi, fw, h as f32, top);
        if neu == (self.scale, self.fenster_w, self.win_h, self.top) {
            return false;
        }
        (self.dpi, self.scale, self.fenster_w, self.win_h, self.top) =
            (dpi, dpi, fw, h as f32, top);
        true
    }

    /// Beendet eine Eingabe wie ein Klick daneben: ein gültiger Wert gilt.
    pub fn end_edit(&mut self) -> UiOut {
        let mut out = UiOut::default();
        self.finish_edit(&mut out);
        out
    }

    /// Zeigt das Paneel „Geschosse“ das Diagramm: nur im eigenen Fenster;
    /// im Hauptfenster ist es immer der Knopf „Geschossmanager“, auch neben
    /// dem Dialog „Gebäude erstellen“ (Jörn 12.10.).
    pub(super) fn levels_diagramm(&self) -> bool {
        self.fenster
    }

    /// Lage des Paneels im Fenster: die ganze Fläche unter der Titelleiste.
    pub(super) fn fenster_rect(&self, top: u32) -> Rect {
        let t = top as f32;
        let w = (self.fenster_w * self.scale).round();
        Rect::new(0.0, t, w, (self.win_h - t).max(0.0))
    }

    /// Aufteilung im Fenster: größter Maßstab bis [`PPM`], der über dem
    /// unteren Rand endet; das Paneel füllt das Fenster, der Fuß mit dem
    /// Gelände steht unten.
    pub(super) fn levels_layout_fenster(&self) -> LevelsLayout {
        let m = (self.size.panel_margin * self.scale).round();
        let t = self.top as f32;
        let mut l = self.levels_layout_at(t - m, PPM);
        l.height = l.height.max(self.win_h - t);
        l
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

    fn fenster() -> (Ui, Scene) {
        let s = Scene::new();
        let mut ui = Ui::fenster(1.0, &Theme::dark());
        assert!(ui.fit_fenster(1.0, 460, 900, 32));
        assert!(!ui.fit_fenster(1.0, 460, 900, 32));
        ui.set_levels(s.levels());
        (ui, s)
    }

    fn mitte(r: Rect) -> (f64, f64) {
        ((r.x + r.w * 0.5) as f64, (r.y + r.h * 0.5) as f64)
    }

    /// Klick mit Bewegung dorthin; was die Oberfläche als geklickt meldet.
    fn klick(ui: &mut Ui, (x, y): (f64, f64), win_w: u32) -> Option<Id> {
        let mut out = None;
        for e in [
            Event::MouseMove { x, y, mods: M },
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods: M,
            },
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                mods: M,
            },
        ] {
            out = out.or(ui.handle(&e, win_w, 32).clicked);
        }
        out
    }

    /// Im Fenster: das Paneel füllt die Fläche unter der Titelleiste, im
    /// großen Maßstab; Zahlen und Namen greifen.
    #[test]
    fn fenster_fuellt_und_ist_bedienbar() {
        let (mut ui, _) = fenster();
        let r = ui.rect(Panel::Levels, 0, 32);
        assert_eq!((r.x, r.y, r.w, r.h), (0.0, 32.0, 460.0, 868.0));
        assert_eq!(ui.panels(), [Panel::Levels]);
        assert!(ui.levels_layout().ppm > 2.0 * ui.size.level_px_per_m);
        let og = ui.levels.bands.iter().find(|b| b.name == "OG").unwrap().id;
        let f = Field::StoreyHeight(og);
        let b = ui.field_rect(f).unwrap();
        let at = (mitte(b).0 + r.x as f64, mitte(b).1 + r.y as f64);
        klick(&mut ui, at, 460);
        assert_eq!(ui.edit.as_ref().map(|e| e.field), Some(f));
        ui.key(Key::Escape, true, M);
        let n = ui
            .level_buttons()
            .into_iter()
            .find(|b| b.0 == Id::Storey(og))
            .unwrap();
        let at = (mitte(n.1).0 + r.x as f64, mitte(n.1).1 + r.y as f64);
        assert_eq!(klick(&mut ui, at, 460), Some(Id::Storey(og)));
        assert!(ui.hit(200.0, 10.0, 460, 32).is_none(), "Titelleiste");
        // Ein schmales, niedriges Fenster behält Namen und Fuß im Bild
        ui.fit_fenster(1.5, 360, 420, 48);
        let l = ui.levels_layout();
        assert_eq!(l.height, 420.0 - 48.0);
        assert!(ui.level_buttons().iter().all(|b| b.1.y + b.1.h <= l.height));
    }

    /// Hauptfenster: unten links nur der Knopf, bündig unter „Werkzeuge“
    /// und so breit; aktiv, solange das Fenster offen ist. Auch neben dem
    /// Dialog steht dort nur der Knopf.
    #[test]
    fn knopf_unten_links() {
        let th = Theme::dark();
        let s = Scene::new();
        let mut ui = Ui::new(1.0, &th);
        ui.fit(1.0, 1440, 900);
        ui.set_levels(s.levels());
        let (t, r) = (
            ui.rect(Panel::Tools, 1440, 32),
            ui.rect(Panel::Levels, 1440, 32),
        );
        assert_eq!((r.x, r.w), (t.x, t.w));
        assert!(r.y > t.y + t.h && r.y + r.h == 900.0 - 12.0, "{r:?}");
        assert!(r.h < 80.0, "nur ein Knopf: {r:?}");
        assert_eq!(ui.laid_out_buttons(Panel::Levels).len(), 1);
        assert_eq!(klick(&mut ui, mitte(r), 1440), Some(Id::LevelsOpen));
        assert!(!ui.is_on(Id::LevelsOpen));
        ui.levels_open = true;
        assert!(ui.is_on(Id::LevelsOpen));
        let patch = ui.repaint_button(&th, Id::LevelsOpen);
        assert!(patch.is_none() || patch.is_some_and(|p| p.panel == Panel::Levels));
        // Neben dem Dialog „Gebäude erstellen“ derselbe Knopf, kein
        // Diagramm (Jörn 12.10.)
        ui.dialog = true;
        assert_eq!(ui.rect(Panel::Levels, 1440, 32), r);
        assert_eq!(ui.laid_out_buttons(Panel::Levels).len(), 1);
        // Ein kleines Fenster: der Knopf rückt nie über „Werkzeuge“
        ui.dialog = false;
        ui.fit(1.0, 900, 380);
        let (t, r) = (
            ui.rect(Panel::Tools, 900, 32),
            ui.rect(Panel::Levels, 900, 32),
        );
        assert!(r.y >= t.y + t.h, "{t:?} {r:?}");
    }
}

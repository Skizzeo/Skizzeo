//! Bilder des Geschossbogens (E18) als Oberflächenbilder: Bogen, Aufleuchten,
//! je Geschoss ein großes und ein kleines Schild und der Hinweis an der
//! Spitze. Gezeichnet wird ein Bild nur, wenn sich sein Inhalt ändert; je
//! Animationsbild ändern sich nur Lage, Größe und Deckkraft.

use crate::scene::Scene;
use crate::ui::Ui;
use crate::wheel::{self, Geo, Part, Wheel, FADE_MS};
use sk_model::StoreyId;
use sk_paint::Canvas;
use sk_render::Renderer;
use sk_ui::theme::Theme;
use sk_ui::widgets::{Fonts, Rect};
use std::collections::HashMap;

/// Höchstens so viele Geschosse bekommen Schilder (Fundament bis Dach).
pub const MAX_LEVELS: usize = 8;
/// Belegte Plätze ab dem ersten: Bogen, Aufleuchten, Schilder, Hinweis.
pub const SLOTS: usize = 3 + 2 * MAX_LEVELS;

/// Stand eines Bildes: was darin steht und wofür es gezeichnet wurde.
type ArcKey = (u64, [u32; 4], Option<Part>, (bool, bool));
type LabelKey = (StoreyId, String, String, bool, u64, [u32; 2]);
type HintKey = (String, String, u64, u32);
/// Breite und Höhe eines Bildes (Pixel).
type Size = (f32, f32);

/// Lage eines Bildes im letzten Bild: Platz, x, y, Breite, Höhe, Deckkraft.
type Placed = (usize, i32, i32, i32, i32, u32);

pub struct WheelView {
    base: usize,
    arc: Option<ArcKey>,
    /// Lage des Bogenbildes (links oben, Pixel).
    arc_at: (i32, i32),
    flash: Option<ArcKey>,
    big: Vec<Option<LabelKey>>,
    small: Vec<Option<LabelKey>>,
    hint: Option<HintKey>,
    placed: Vec<Placed>,
    /// Trefferflächen der Schilder (Fenster-Pixel) und ihr Teil.
    hits: Vec<(Rect, Part)>,
    /// Deckkraft beim Ein- und Ausblenden und Zeit des letzten Schritts.
    fade: f32,
    fade_at: u64,
}

/// Was die App zeigen will: nicht im Grundriss (sofort weg), verdeckt von
/// Dialog oder Menü (sanft weg) oder sichtbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    Off,
    Blocked,
    On,
}

impl WheelView {
    /// Die Bilder liegen auf den Plätzen `base` bis `base + SLOTS - 1`.
    pub fn new(base: usize) -> WheelView {
        WheelView {
            base,
            arc: None,
            arc_at: (0, 0),
            flash: None,
            big: vec![None; MAX_LEVELS],
            small: vec![None; MAX_LEVELS],
            hint: None,
            placed: Vec::new(),
            hits: Vec::new(),
            fade: 0.0,
            fade_at: 0,
        }
    }

    /// Schema oder Skalierung geändert: alle Bilder neu.
    pub fn forget(&mut self) {
        self.arc = None;
        self.flash = None;
        self.big.iter_mut().for_each(|k| *k = None);
        self.small.iter_mut().for_each(|k| *k = None);
        self.hint = None;
    }

    /// Wird gerade ein- oder ausgeblendet (die Schleife zeichnet weiter)?
    pub fn fading(&self, show: Show) -> bool {
        match show {
            Show::On => self.fade < 1.0,
            Show::Blocked => self.fade > 0.0,
            Show::Off => false,
        }
    }

    /// Teil des Bogens unter der Maus (Fenster-Pixel), samt Schildern.
    pub fn hit(&self, wheel: &Wheel, ui: &Ui, w: u32, h: u32, x: f64, y: f64) -> Option<Part> {
        if self.fade <= 0.0 {
            return None;
        }
        let (x, y) = (x as f32, y as f32);
        wheel.geo(ui, w, h).hit(x, y).or_else(|| {
            self.hits
                .iter()
                .find(|(r, _)| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
                .map(|(_, p)| *p)
        })
    }

    /// Bilder an den Stand zur Zeit `t` angleichen; `true`, wenn sich am
    /// Bild etwas geändert hat.
    #[allow(clippy::too_many_arguments)]
    pub fn sync(
        &mut self,
        r: &mut Renderer,
        wheel: &Wheel,
        s: &Scene,
        ui: &Ui,
        th: &Theme,
        (w, h): (u32, u32),
        t: u64,
        show: Show,
        input: bool,
    ) -> bool {
        self.step_fade(wheel.instant(), t, show);
        let mut placed: Vec<Placed> = Vec::new();
        self.hits.clear();
        if self.fade > 0.0 {
            let g = wheel.geo(ui, w, h);
            self.sync_arc(r, wheel, s, &g, th, input, &mut placed, t);
            self.sync_labels(r, wheel, s, &g, th, &ui.fonts, input, &mut placed, t);
            self.sync_hint(r, wheel, s, &g, th, &ui.fonts, input, &mut placed, t);
        }
        // Alles, was dieses Mal nicht liegt, ausblenden
        for slot in self.base..self.base + SLOTS {
            let lies = |v: &[Placed]| v.iter().any(|p| p.0 == slot);
            if !lies(&placed) && lies(&self.placed) {
                r.place_overlay(slot, 0, 0, 0, 0, 0.0);
            }
        }
        let changed = placed != self.placed;
        self.placed = placed;
        changed
    }

    fn step_fade(&mut self, instant: bool, t: u64, show: Show) {
        let dt = t.saturating_sub(self.fade_at);
        self.fade_at = t;
        self.fade = match show {
            Show::Off => 0.0,
            Show::On if instant => 1.0,
            Show::Blocked if instant => 0.0,
            Show::On => (self.fade + dt as f32 / FADE_MS as f32).min(1.0),
            Show::Blocked => (self.fade - dt as f32 / FADE_MS as f32).max(0.0),
        };
    }

    /// Bild hochladen (ohne es schon zu zeigen) und seine Größe.
    fn upload(r: &mut Renderer, slot: usize, c: &Canvas) -> (f32, f32) {
        r.set_overlay(
            slot,
            0,
            0,
            c.width as u32,
            c.height as u32,
            &c.to_premul_rgba8(),
        );
        r.place_overlay(slot, 0, 0, 0, 0, 0.0);
        (c.width as f32, c.height as f32)
    }

    fn place(
        r: &mut Renderer,
        placed: &mut Vec<Placed>,
        slot: usize,
        (x, y, w, h): (f32, f32, f32, f32),
        alpha: f32,
    ) {
        if alpha <= 0.0 || w <= 0.0 || h <= 0.0 {
            return;
        }
        let (x, y) = (x.round() as i32, y.round() as i32);
        let (w, h) = (w.round() as i32, h.round() as i32);
        r.place_overlay(slot, x, y, w, h, alpha);
        placed.push((slot, x, y, w, h, alpha.to_bits()));
    }

    #[allow(clippy::too_many_arguments)]
    fn sync_arc(
        &mut self,
        r: &mut Renderer,
        wheel: &Wheel,
        s: &Scene,
        g: &Geo,
        th: &Theme,
        input: bool,
        placed: &mut Vec<Placed>,
        t: u64,
    ) {
        let enabled = (
            wheel.arrow_enabled(s, true, input),
            wheel.arrow_enabled(s, false, input),
        );
        let geo = [g.cx.to_bits(), g.cy.to_bits(), g.r.to_bits(), g.s.to_bits()];
        let key = (th.rev, geo, wheel.hover, enabled);
        if self.arc.as_ref() != Some(&key) {
            let glow = if wheel.hover.is_some() { 1.5 } else { 1.0 };
            let (c, x, y) = wheel.paint_arc(g, th, enabled, glow);
            Self::upload(r, self.base, &c);
            self.arc = Some(key);
            self.arc_at = (x, y);
        }
        let (tw, tht) = r.overlay_size(self.base);
        let (x, y) = self.arc_at;
        Self::place(
            r,
            placed,
            self.base,
            (x as f32, y as f32, tw as f32, tht as f32),
            self.fade,
        );
        // Aufleuchten beim Wechsel: 2-fach → 1-fach
        let flash_key = (th.rev, geo, None, (false, false));
        if let Some((e, _)) = wheel.progress(t) {
            if self.flash.as_ref() != Some(&flash_key) {
                let (c, _, _) = wheel.paint_flash(g, th);
                Self::upload(r, self.base + 1, &c);
                self.flash = Some(flash_key);
            }
            Self::place(
                r,
                placed,
                self.base + 1,
                (x as f32, y as f32, tw as f32, tht as f32),
                (1.0 - e) * self.fade,
            );
        }
    }

    /// Schilder: je Geschoss groß (aktiv) und klein (Nachbar), beim Wechsel
    /// gleiten sie, wachsen bzw. schrumpfen und blenden über.
    #[allow(clippy::too_many_arguments)]
    fn sync_labels(
        &mut self,
        r: &mut Renderer,
        wheel: &Wheel,
        s: &Scene,
        g: &Geo,
        th: &Theme,
        fonts: &Fonts,
        input: bool,
        placed: &mut Vec<Placed>,
        t: u64,
    ) {
        let levels = wheel::levels(s);
        let active = levels.iter().position(|x| *x == s.active_storey());
        let fit = [g.s.to_bits(), (g.right - g.label_x).to_bits()];
        let mut sizes: HashMap<StoreyId, (Size, Size)> = HashMap::new();
        for (i, &id) in levels.iter().enumerate().take(MAX_LEVELS) {
            let name = wheel::short_name(s, id);
            let kote = wheel::kote(s, id);
            // Kleines Schild an der Spitze unter der Maus in Akzentfarbe
            let hovered = !input
                && active.is_some_and(|a| match wheel.hover {
                    Some(Part::Up) => i == a + 1,
                    Some(Part::Down) => i + 1 == a,
                    _ => false,
                });
            let (bs, ss) = (self.base + 2 + 2 * i, self.base + 3 + 2 * i);
            let bkey = (id, name.clone(), kote.clone(), false, th.rev, fit);
            if self.big[i].as_ref() != Some(&bkey) {
                Self::upload(r, bs, &wheel::paint_big(fonts, &name, &kote, g, th));
                self.big[i] = Some(bkey);
            }
            let skey = (id, name.clone(), String::new(), hovered, th.rev, fit);
            if self.small[i].as_ref() != Some(&skey) {
                let col = if hovered {
                    th.ui.accent
                } else {
                    th.ui.text_dim
                };
                Self::upload(r, ss, &wheel::paint_small(fonts, &name, g, th, col));
                self.small[i] = Some(skey);
            }
            let size = |slot| {
                let (w, h) = r.overlay_size(slot);
                (w as f32, h as f32)
            };
            sizes.insert(id, (size(bs), size(ss)));
        }
        let ratio = (th.size.arc_label_small / th.size.arc_label.max(1.0)).clamp(0.1, 1.0);
        let places = wheel.labels(s, g, t, |id| sizes.get(&id).copied().unwrap_or_default());
        for p in places {
            let Some(i) = levels
                .iter()
                .position(|x| *x == p.storey)
                .filter(|i| *i < MAX_LEVELS)
            else {
                continue;
            };
            let ((bw, bh), (sw, sh)) = sizes[&p.storey];
            let b = p.big;
            let fb = ratio + (1.0 - ratio) * b;
            let fs = 1.0 + (1.0 / ratio - 1.0) * b;
            let rect =
                |w: f32, h: f32, f: f32| (p.x - w * f * 0.5, p.y - h * f * 0.5, w * f, h * f);
            let big = rect(bw, bh, fb);
            let small = rect(sw, sh, fs);
            Self::place(
                r,
                placed,
                self.base + 2 + 2 * i,
                big,
                p.alpha * b * self.fade,
            );
            Self::place(
                r,
                placed,
                self.base + 3 + 2 * i,
                small,
                p.alpha * (1.0 - b) * self.fade,
            );
            // Trefferflächen: das aktive Schild gehört zum Band, die
            // Nachbarn zu ihrer Spitze
            let (x, y, w, h) = if b >= 0.5 { big } else { small };
            let part = if b >= 0.5 {
                Part::Band
            } else if p.y < g.cy {
                Part::Up
            } else {
                Part::Down
            };
            self.hits.push((Rect::new(x, y, w, h), part));
        }
    }

    /// Hinweis links neben der Spitze unter der Maus, nach kurzer Ruhe.
    #[allow(clippy::too_many_arguments)]
    fn sync_hint(
        &mut self,
        r: &mut Renderer,
        wheel: &Wheel,
        s: &Scene,
        g: &Geo,
        th: &Theme,
        fonts: &Fonts,
        input: bool,
        placed: &mut Vec<Placed>,
        t: u64,
    ) {
        let up = match wheel.hover {
            Some(Part::Up) => true,
            Some(Part::Down) => false,
            _ => return,
        };
        if !wheel.hint_due(t) || wheel.animating(t) {
            return;
        }
        let Some((line, kote)) = wheel.arrow_hint(s, up, input) else {
            return;
        };
        let slot = self.base + SLOTS - 1;
        let key = (line.clone(), kote.clone(), th.rev, g.s.to_bits());
        if self.hint.as_ref() != Some(&key) {
            Self::upload(r, slot, &wheel::paint_hint(fonts, &line, &kote, g.s, th));
            self.hint = Some(key);
        }
        let (w, h) = r.overlay_size(slot);
        let (w, h) = (w as f32, h as f32);
        let tip = g.tip(up);
        let x = (tip.0 - g.head_w * 0.5 - 8.0 * g.s - w).max(0.0);
        let y = tip.1 - h * 0.5;
        Self::place(r, placed, slot, (x, y, w, h), self.fade);
    }
}

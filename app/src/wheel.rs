//! Geschossbogen im Grundriss (E18): ein gebogener Doppelpfeil „)“ rechts im
//! Modellfenster. Die Spitzen wechseln ein Geschoss höher oder tiefer, rechts
//! neben der Bogenmitte steht das aktive Geschoss groß, an den Spitzen klein
//! die Nachbarn. Ein Wechsel rollt die Beschriftungen um einen Platz weiter
//! und blendet den Grundriss über (Dauer `anim_ms`, 0 = sofort).
//!
//! Der Bogen wählt nur das aktive Geschoss (Sitzungszustand): kein
//! Rückgängig-Eintrag, keine Änderung an der Datei. Zeiten in Millisekunden
//! seit einem beliebigen Anfang, damit sich alles ohne Warten prüfen lässt.

use crate::scene::{Scene, FOUNDATION_NAME};
use crate::ui::{kote_text, Panel, Ui, ViewKind};
use sk_model::StoreyId;
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::Key;
use sk_ui::theme::{Sizes, Theme};
use sk_ui::widgets::{Fonts, Rect};

/// Bild↑ und Bild↓ (`VK_PRIOR`, `VK_NEXT`).
pub const KEY_PAGE_UP: Key = Key::Other(0x21);
pub const KEY_PAGE_DOWN: Key = Key::Other(0x22);

/// Abstand des Schilds vom Band, Breite der Spalte für das aktive Geschoss
/// (Vielfache der großen Schrift), kleinste Schrift für lange Namen (dip).
const LABEL_GAP: f32 = 20.0;
const LABEL_COL: f32 = 5.0;
const LABEL_MIN: f32 = 16.0;
/// Schrift der Kote unter dem aktiven Geschoss (dip).
const KOTE_PX: f32 = 10.5;
/// Trefferrand um die Spitzen (dip).
const HIT_PAD: f32 = 6.0;
/// Höhenversatz des Grundrisses beim Wechsel (dip).
pub const SLIDE: f32 = 24.0;
/// Ein- und Ausblenden, solange ein Dialog offen ist (ms).
pub const FADE_MS: u64 = 150;

/// Teil des Bogens unter der Maus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Up,
    Down,
    Band,
}

/// Ein Wechsel: Richtung in Plätzen (+ = nach oben).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Switch {
    pub from: StoreyId,
    pub to: StoreyId,
    pub steps: i32,
}

#[derive(Clone, Copy, Debug)]
struct Anim {
    switch: Switch,
    start: u64,
    dur: u64,
    /// Noch kein Bild gezeigt: der Start rückt auf das erste Bild (falls der
    /// Grundriss erst gerechnet werden musste).
    fresh: bool,
}

/// Vorgemerkte Eingabe während einer Animation.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    By(i32),
    To(StoreyId),
}

pub struct Wheel {
    size: Sizes,
    screenshot: bool,
    anim: Option<Anim>,
    queued: Option<Step>,
    /// Zuletzt begonnener Wechsel, bis die App ihn abholt (Bild festhalten).
    started: Option<Switch>,
    pub hover: Option<Part>,
    /// Seit wann die Maus über der Spitze steht (ms).
    hover_since: u64,
}

/// Dauer eines Wechsels: `anim_ms` aus dem Schema, beim Bildschirmfoto 0.
#[cfg(test)]
pub fn anim_duration(th: &Theme, screenshot: bool) -> u64 {
    if screenshot {
        0
    } else {
        ms(th.size.anim_ms)
    }
}

/// `anim_ms` im erlaubten Bereich 0–600.
fn ms(anim_ms: f32) -> u64 {
    anim_ms.clamp(0.0, 600.0).round() as u64
}

/// Weich ein, weich aus.
pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Halbe Breite des breitesten kleinen Schilds an einer Spitze
/// („Fundament“, dip), für den Platz links vom Bogen.
const TIP_LABEL_HALF: f32 = 48.0;

/// Ebenen, durch die der Bogen blättert, von unten nach oben: dieselben,
/// die das Paneel „Geschosse“ zeigt ([`Scene::level_ids`]), auch die der
/// Vorlage ohne Gebäude.
pub fn levels(s: &Scene) -> Vec<StoreyId> {
    s.level_ids()
}

/// Kurzname eines Geschosses im Bogen („EG“, „OG“, „Fundament“).
pub fn short_name(s: &Scene, id: StoreyId) -> String {
    match s.model().storey(id) {
        Some(st) if st.kind == sk_model::LevelKind::Foundation => FOUNDATION_NAME.into(),
        Some(st) => st.short.clone(),
        None => String::new(),
    }
}

/// Voller Name („Erdgeschoss“, „Obergeschoss“, „Fundament“).
fn full_name(s: &Scene, id: StoreyId) -> String {
    match s.model().storey(id) {
        Some(st) if st.kind == sk_model::LevelKind::Foundation => FOUNDATION_NAME.into(),
        Some(st) => st.name.clone(),
        None => String::new(),
    }
}

/// Kote der Unterkante.
pub fn kote(s: &Scene, id: StoreyId) -> String {
    kote_text(s.model().storey(id).map_or(0.0, |st| st.elevation))
}

/// Lage des Bogens im Fenster (Pixel, y nach unten).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geo {
    /// Kreismittelpunkt und Radius bis zur Bandmitte.
    pub cx: f32,
    pub cy: f32,
    pub r: f32,
    /// Halber Öffnungswinkel (Bogenmaß).
    pub span: f32,
    pub band: f32,
    pub head_l: f32,
    pub head_w: f32,
    /// Bildschirmskalierung.
    pub s: f32,
    /// Linke Kante des Schilds für das aktive Geschoss und rechter Rand.
    pub label_x: f32,
    pub right: f32,
}

type Pt = (f32, f32);

impl Geo {
    fn at(&self, rad: f32, a: f32) -> Pt {
        (self.cx + rad * a.cos(), self.cy + rad * a.sin())
    }

    /// Ende des Bandes, Richtung nach außen (Pfeilrichtung) und radiale
    /// Richtung an der oberen (`up`) bzw. unteren Spitze.
    fn end(&self, up: bool) -> (Pt, Pt, Pt) {
        let a = if up { -self.span } else { self.span };
        let e = self.at(self.r, a);
        let n = (a.cos(), a.sin());
        let t = if up {
            (a.sin(), -a.cos())
        } else {
            (-a.sin(), a.cos())
        };
        (e, t, n)
    }

    /// Spitze des Pfeils oben bzw. unten.
    pub fn tip(&self, up: bool) -> Pt {
        let (e, t, _) = self.end(up);
        (e.0 + t.0 * self.head_l, e.1 + t.1 * self.head_l)
    }

    /// Pfeilspitze als Dreieck (um `d` vergrößert): außen, Spitze, innen.
    fn head(&self, up: bool, d: f32) -> [Pt; 3] {
        let (e, t, n) = self.end(up);
        let w = self.head_w * 0.5 + d;
        let back = (e.0 - t.0 * d, e.1 - t.1 * d);
        let l = self.head_l + d * 1.6;
        [
            (back.0 + n.0 * w, back.1 + n.1 * w),
            (e.0 + t.0 * l, e.1 + t.1 * l),
            (back.0 - n.0 * w, back.1 - n.1 * w),
        ]
    }

    /// Umriss von Band und beiden Spitzen, um `d` vergrößert.
    fn outline(&self, d: f32) -> Vec<Pt> {
        let (ro, ri) = (self.r + self.band * 0.5 + d, self.r - self.band * 0.5 - d);
        let n = 40;
        let mut v = Vec::new();
        // außen von unten nach oben, Spitze oben, innen zurück, Spitze unten
        for i in 0..=n {
            let a = self.span - 2.0 * self.span * i as f32 / n as f32;
            v.push(self.at(ro, a));
        }
        v.extend(self.head(true, d));
        for i in 0..=n {
            let a = -self.span + 2.0 * self.span * i as f32 / n as f32;
            v.push(self.at(ri, a));
        }
        let [o, t, i] = self.head(false, d);
        v.extend([i, t, o]);
        v
    }

    /// Umschließendes Rechteck mit Rand `pad`.
    fn bounds(&self, pad: f32) -> Rect {
        let pts = self.outline(pad);
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in pts {
            (x0, y0, x1, y1) = (x0.min(p.0), y0.min(p.1), x1.max(p.0), y1.max(p.1));
        }
        Rect::new(
            x0.floor(),
            y0.floor(),
            (x1 - x0).ceil() + 1.0,
            (y1 - y0).ceil() + 1.0,
        )
    }

    /// Teil des Bogens an der Stelle (Pixel).
    pub fn hit(&self, x: f32, y: f32) -> Option<Part> {
        let pad = HIT_PAD * self.s;
        for (up, part) in [(true, Part::Up), (false, Part::Down)] {
            if in_poly(&self.head(up, pad), (x, y)) {
                return Some(part);
            }
        }
        in_poly(&self.outline(pad * 0.5), (x, y)).then_some(Part::Band)
    }
}

/// Punkt im Polygon (gerade Anzahl Schnitte).
fn in_poly(p: &[Pt], q: Pt) -> bool {
    let mut inside = false;
    let n = p.len();
    for i in 0..n {
        let (a, b) = (p[i], p[(i + n - 1) % n]);
        if (a.1 > q.1) != (b.1 > q.1) && q.0 < (b.0 - a.0) * (q.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
    }
    inside
}

fn poly_path(p: &mut Path, pts: &[Pt]) {
    if let Some(&(x, y)) = pts.first() {
        p.move_to(x, y);
        for &(x, y) in &pts[1..] {
            p.line_to(x, y);
        }
        p.close();
    }
}

/// Beschriftung an ihrem Platz: Mitte (Pixel), wie groß (1 = aktives
/// Geschoss, 0 = Nachbar) und Deckkraft.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabelPlace {
    pub storey: StoreyId,
    pub x: f32,
    pub y: f32,
    pub big: f32,
    pub alpha: f32,
}

impl Wheel {
    pub fn new(th: &Theme, screenshot: bool) -> Wheel {
        Wheel {
            size: th.size,
            screenshot,
            anim: None,
            queued: None,
            started: None,
            hover: None,
            hover_since: 0,
        }
    }

    /// Übernimmt Maße und Dauer aus dem Schema.
    pub fn set_theme(&mut self, th: &Theme) {
        self.size = th.size;
    }

    /// Animationen aus (`anim_ms` = 0 oder Bildschirmfoto): alles sofort.
    pub fn instant(&self) -> bool {
        self.duration() == 0
    }

    fn duration(&self) -> u64 {
        if self.screenshot {
            0
        } else {
            ms(self.size.anim_ms)
        }
    }

    /// Nur im Grundriss und nicht, solange ein Dialog, das Dateimenü oder das
    /// Einstellungsfenster offen ist (`blocked`).
    pub fn visible(&self, view: ViewKind, blocked: bool) -> bool {
        view == ViewKind::Plan && !blocked
    }

    /// Lage im Fenster `w` × `h` (Pixel, mit Titelleiste): rechts am Rand,
    /// senkrecht mittig unter „Ansichten“, nie höher als die Fenstermitte.
    /// Ist ein Bauteil gewählt, rückt er links neben „Eigenschaften“, damit
    /// das Paneel ihn nicht verdeckt.
    pub fn geo(&self, ui: &Ui, w: u32, h: u32) -> Geo {
        self.geo_at(ui, w, h, ui.has_props())
    }

    /// Linke Kante des Bogens samt Beschriftung an den Spitzen, wenn er
    /// neben „Eigenschaften“ steht (Pixel): Bis hierhin darf der Grundriss.
    pub fn left_beside_props(&self, ui: &Ui, w: u32, h: u32) -> f32 {
        let g = self.geo_at(ui, w, h, true);
        let tip = g.tip(true).0.min(g.tip(false).0);
        g.bounds(0.0).x.min(tip - TIP_LABEL_HALF * g.s)
    }

    fn geo_at(&self, ui: &Ui, w: u32, h: u32, props: bool) -> Geo {
        let z = &self.size;
        let s = ui.dpi();
        let views = ui.rect(Panel::Views, w, ui.top);
        let below = views.y + views.h;
        let cy = ((below + h as f32) / 2.0).max(h as f32 / 2.0);
        let right = if props {
            ui.rect(Panel::Props, w, ui.top).x - z.panel_margin * s
        } else {
            w as f32 - z.panel_margin * s
        };
        let label_x = right - LABEL_COL * z.arc_label * s;
        let r = z.arc_r * s;
        let mid = label_x - LABEL_GAP * s - z.arc_band * 0.5 * s;
        Geo {
            cx: mid - r,
            cy,
            r,
            span: z.arc_span_deg.to_radians(),
            band: z.arc_band * s,
            head_l: z.arc_head_l * s,
            head_w: z.arc_head_w * s,
            s,
            label_x,
            right,
        }
    }

    #[cfg(test)]
    /// Kreismittelpunkt und Radius (Pixel).
    pub fn placement(&self, ui: &Ui, w: u32, h: u32) -> (f32, f32, f32) {
        let g = self.geo(ui, w, h);
        (g.cx, g.cy, g.r)
    }

    #[cfg(test)]
    /// Rechte Kante des Bogens samt Schild des aktiven Geschosses.
    pub fn right_edge(&self, _s: &Scene, ui: &Ui, w: u32, h: u32) -> f32 {
        self.geo(ui, w, h).right
    }

    /// Geschoss über (`up`) bzw. unter dem aktiven.
    fn neighbor_id(&self, s: &Scene, up: bool) -> Option<StoreyId> {
        let l = levels(s);
        let i = l.iter().position(|x| *x == s.active_storey())?;
        if up {
            l.get(i + 1).copied()
        } else {
            l.get(i.checked_sub(1)?).copied()
        }
    }

    pub fn arrow_enabled(&self, s: &Scene, up: bool, input: bool) -> bool {
        !input && self.neighbor_id(s, up).is_some()
    }

    #[cfg(test)]
    /// Aktives Geschoss: Name und Kote.
    pub fn center(&self, s: &Scene) -> (String, String) {
        let a = s.active_storey();
        (short_name(s, a), kote(s, a))
    }

    #[cfg(test)]
    /// Beschriftung an der Spitze (`None`: ausgegraut, ohne Beschriftung).
    pub fn neighbor(&self, s: &Scene, up: bool) -> Option<String> {
        self.neighbor_id(s, up).map(|id| short_name(s, id))
    }

    /// Hinweis an der Spitze: „Obergeschoss ↑“ und Kote, beim angefangenen
    /// Wandzug der Sperrhinweis.
    pub fn arrow_hint(&self, s: &Scene, up: bool, input: bool) -> Option<(String, String)> {
        if input {
            return Some((
                "Erst die Wand fertig zeichnen oder Esc".into(),
                String::new(),
            ));
        }
        let id = self.neighbor_id(s, up)?;
        let arrow = if up { "↑" } else { "↓" };
        Some((format!("{} {arrow}", full_name(s, id)), kote(s, id)))
    }

    /// Läuft zur Zeit `t` ein Wechsel?
    pub fn animating(&self, t: u64) -> bool {
        self.anim.is_some_and(|a| a.fresh || t < a.start + a.dur)
    }

    #[cfg(test)]
    /// Um wie viele Plätze die Beschriftungen gerade rollen (0 in Ruhe).
    pub fn anim_steps(&self, t: u64) -> i32 {
        match self.anim {
            Some(a) if self.animating(t) => a.switch.steps,
            _ => 0,
        }
    }

    /// Fortschritt 0…1 (geglättet) und Wechsel, solange er läuft.
    pub fn progress(&self, t: u64) -> Option<(f32, Switch)> {
        let a = self.anim.filter(|_| self.animating(t))?;
        let p = if a.fresh || a.dur == 0 {
            0.0
        } else {
            (t.saturating_sub(a.start)) as f32 / a.dur as f32
        };
        Some((ease_in_out_cubic(p), a.switch))
    }

    /// Das erste Bild des Wechsels ist gezeichnet: ab hier läuft die Zeit.
    pub fn shown(&mut self, t: u64) {
        if let Some(a) = self.anim.as_mut().filter(|a| a.fresh) {
            a.fresh = false;
            a.start = t;
        }
    }

    /// Begonnener Wechsel (einmal abzuholen).
    pub fn take_started(&mut self) -> Option<Switch> {
        self.started.take()
    }

    /// Zeit schreitet fort: eine beendete Animation räumt auf, eine
    /// vorgemerkte Eingabe läuft danach.
    pub fn tick(&mut self, s: &mut Scene, t: u64) {
        if self.anim.is_some() && !self.animating(t) {
            self.anim = None;
        }
        if self.anim.is_none() {
            if let Some(step) = self.queued.take() {
                self.run(s, step, t);
            }
        }
    }

    /// Eingabe: sofort, oder während der Animation vorgemerkt (höchstens
    /// eine).
    fn request(&mut self, s: &mut Scene, step: Step, t: u64) {
        self.tick(s, t);
        if self.animating(t) {
            if self.queued.is_none() {
                self.queued = Some(step);
            }
            return;
        }
        self.run(s, step, t);
    }

    fn run(&mut self, s: &mut Scene, step: Step, t: u64) {
        let from = s.active_storey();
        let levels = levels(s);
        let index = |id: StoreyId| levels.iter().position(|x| *x == id);
        let Some(i) = index(from) else {
            return;
        };
        let (to, steps) = match step {
            Step::By(n) => {
                let j = i as i64 + n as i64;
                match usize::try_from(j).ok().and_then(|j| levels.get(j)) {
                    Some(&id) => (id, n),
                    None => return,
                }
            }
            Step::To(id) => match index(id) {
                Some(j) => (id, j as i32 - i as i32),
                None => return,
            },
        };
        if steps == 0 || !s.set_active_storey(to) {
            return;
        }
        let switch = Switch { from, to, steps };
        self.started = Some(switch);
        let dur = self.duration();
        self.anim = (dur > 0).then_some(Anim {
            switch,
            start: t,
            dur,
            fresh: false,
        });
    }

    /// Klick auf die Spitze oben bzw. unten (oder ihre Beschriftung).
    pub fn click_arrow(&mut self, s: &mut Scene, up: bool, input: bool, t: u64) {
        if input || self.neighbor_id(s, up).is_none() && !self.animating(t) {
            return;
        }
        self.request(s, Step::By(if up { 1 } else { -1 }), t);
    }

    /// Klick auf Band oder Schild: nichts.
    pub fn click_band(&mut self, _s: &mut Scene, _t: u64) {}

    /// Mausrad über dem Bogen: eine Raste = ein Geschoss (hoch = höher).
    pub fn scroll(&mut self, s: &mut Scene, notches: i32, t: u64) {
        if notches != 0 {
            self.request(s, Step::By(notches.signum()), t);
        }
    }

    /// Bild↑/Bild↓ im Grundriss; `true`, wenn der Bogen die Taste nimmt.
    pub fn key(&mut self, s: &mut Scene, view: ViewKind, key: Key, input: bool, t: u64) -> bool {
        if view != ViewKind::Plan || input {
            return false;
        }
        let n = match key {
            KEY_PAGE_UP => 1,
            KEY_PAGE_DOWN => -1,
            _ => return false,
        };
        self.request(s, Step::By(n), t);
        true
    }

    /// Klick auf einen Geschossnamen im Paneel: derselbe Wechsel.
    pub fn select(&mut self, s: &mut Scene, id: StoreyId, t: u64) {
        self.request(s, Step::To(id), t);
    }

    /// Der Grundriss des Ziels war nicht vorbereitet: die Animation beginnt
    /// erst mit dem ersten fertigen Bild.
    pub fn wait_for_first_frame(&mut self) {
        if let Some(a) = self.anim.as_mut() {
            a.fresh = true;
        }
    }

    /// Maus über einem Teil (oder nichts); `true`, wenn sich das ändert.
    pub fn set_hover(&mut self, part: Option<Part>, t: u64) -> bool {
        if self.hover == part {
            return false;
        }
        self.hover = part;
        self.hover_since = t;
        true
    }

    /// Ist der Hinweis an der Spitze unter der Maus fällig?
    pub fn hint_due(&self, t: u64) -> bool {
        matches!(self.hover, Some(Part::Up | Part::Down))
            && t >= self.hover_since + (self.size.hover_delay_hud * 1000.0) as u64
    }

    /// Wann der Hinweis fällig wird (ms ab `t`), solange er noch aussteht.
    pub fn hint_wait(&self, t: u64) -> Option<u64> {
        if !matches!(self.hover, Some(Part::Up | Part::Down)) {
            return None;
        }
        let due = self.hover_since + (self.size.hover_delay_hud * 1000.0) as u64;
        (t < due).then(|| due - t)
    }

    /// Plätze der Beschriftungen zur Zeit `t`. `size(id)` liefert die Größe
    /// (Breite, Höhe) des großen Schilds und des kleinen.
    pub fn labels(
        &self,
        s: &Scene,
        g: &Geo,
        t: u64,
        size: impl Fn(StoreyId) -> ((f32, f32), (f32, f32)),
    ) -> Vec<LabelPlace> {
        let a = s.active_storey();
        let levels = levels(s);
        let Some(ia) = levels.iter().position(|x| *x == a) else {
            return Vec::new();
        };
        // Bruchteil des aktiven Platzes: rollt vom alten zum neuen
        let pos = match self.progress(t) {
            Some((e, sw)) => ia as f32 - sw.steps as f32 * (1.0 - e),
            None => ia as f32,
        };
        let gap = 6.0 * g.s;
        levels
            .iter()
            .enumerate()
            .filter_map(|(i, &id)| {
                let k = i as f32 - pos;
                if k.abs() >= 2.0 {
                    return None;
                }
                let ((bw, bh), (_, sh)) = size(id);
                let (tu, td) = (g.tip(true), g.tip(false));
                let center = (g.label_x + bw * 0.5, g.cy);
                let top = (tu.0, tu.1 - gap - sh * 0.5);
                let bottom = (td.0, td.1 + gap + sh * 0.5);
                let _ = bh;
                let lerp = |p: Pt, q: Pt, f: f32| (p.0 + (q.0 - p.0) * f, p.1 + (q.1 - p.1) * f);
                let (x, y) = if k >= 1.0 {
                    let beyond = (top.0, top.1 - (center.1 - top.1) * 0.5);
                    lerp(top, beyond, k - 1.0)
                } else if k >= 0.0 {
                    lerp(center, top, k)
                } else if k >= -1.0 {
                    lerp(center, bottom, -k)
                } else {
                    let beyond = (bottom.0, bottom.1 + (bottom.1 - center.1) * 0.5);
                    lerp(bottom, beyond, -k - 1.0)
                };
                Some(LabelPlace {
                    storey: id,
                    x,
                    y,
                    big: (1.0 - k.abs()).max(0.0),
                    alpha: (2.0 - k.abs()).clamp(0.0, 1.0),
                })
            })
            .collect()
    }

    /// Bild des Bogens (Band, Spitzen, Leuchten) mit seiner Lage im Fenster.
    /// `glow`: Stärke des Leuchtens (1 in Ruhe, 1,5 unter der Maus).
    pub fn paint_arc(
        &self,
        g: &Geo,
        t: &Theme,
        enabled: (bool, bool),
        glow: f32,
    ) -> (Canvas, i32, i32) {
        let (mut c, local, x, y) = arc_canvas(g);
        let s = g.s;
        let border = 1.5 * s;
        // weicher Schatten wie die Paneele
        for i in 1..=3 {
            let d = border + i as f32 * 2.0 * s;
            ring(&mut c, &local, border, d, t.ui.shadow);
        }
        paint_glow(&mut c, &local, t, glow);
        // Fläche und Rand
        let mut body = Path::new();
        poly_path(&mut body, &local.outline(0.0));
        c.fill(&body, t.ui.hud_bg);
        ring(&mut c, &local, 0.0, border, t.ui.accent);
        // Spitzen: unter der Maus gefüllt, gesperrt grau
        for (up, on) in [(true, enabled.0), (false, enabled.1)] {
            let part = if up { Part::Up } else { Part::Down };
            let mut p = Path::new();
            poly_path(&mut p, &local.head(up, -border));
            if !on {
                let d = t.ui.text_disabled;
                c.fill(&p, Rgba(d.0, d.1, d.2, (d.3 as f32 * 0.75) as u8));
            } else if self.hover == Some(part) {
                let a = t.ui.accent;
                c.fill(&p, Rgba(a.0, a.1, a.2, (a.3 as f32 * 0.9) as u8));
            }
        }
        (c, x, y)
    }

    /// Nur das Leuchten (einfach), an derselben Stelle wie [`Wheel::paint_arc`]:
    /// liegt beim Wechsel über dem Bogen und blendet aus (2-fach → 1-fach).
    pub fn paint_flash(&self, g: &Geo, t: &Theme) -> (Canvas, i32, i32) {
        let (mut c, local, x, y) = arc_canvas(g);
        paint_glow(&mut c, &local, t, 1.0);
        (c, x, y)
    }
}

/// Leeres Bild um den Bogen (mit Rand für Schatten und Leuchten), der Bogen
/// in seinen Koordinaten und die Lage im Fenster.
fn arc_canvas(g: &Geo) -> (Canvas, Geo, i32, i32) {
    let b = g.bounds(12.0 * g.s);
    let local = Geo {
        cx: g.cx - b.x,
        cy: g.cy - b.y,
        ..*g
    };
    (
        Canvas::new(b.w as usize, b.h as usize),
        local,
        b.x as i32,
        b.y as i32,
    )
}

/// Ring zwischen zwei Vergrößerungen der Kontur: außen herum, innen
/// gegenläufig.
fn ring(c: &mut Canvas, g: &Geo, d0: f32, d1: f32, col: Rgba) {
    let mut p = Path::new();
    poly_path(&mut p, &g.outline(d1));
    let mut inner = g.outline(d0);
    inner.reverse();
    poly_path(&mut p, &inner);
    c.fill(&p, col);
}

/// Leuchten: drei Konturen in der Leuchtfarbe (30 %, 15 %, 6 % × `glow`).
fn paint_glow(c: &mut Canvas, g: &Geo, t: &Theme, glow: f32) {
    let s = g.s;
    let border = 1.5 * s;
    let h = t.ui.hud_glow;
    for (k, a) in [0.30f32, 0.15, 0.06].iter().enumerate().rev() {
        let alpha = (h.3 as f32 * (a * glow).min(1.0)).round() as u8;
        let d0 = border + k as f32 * 2.0 * s;
        ring(c, g, d0, d0 + 2.0 * s, Rgba(h.0, h.1, h.2, alpha));
    }
}

/// Schild des aktiven Geschosses: Name groß und fett, darunter die Kote.
pub fn paint_big(fonts: &Fonts, name: &str, kote: &str, g: &Geo, t: &Theme) -> Canvas {
    let s = g.s;
    let z = &t.size;
    let bold = fonts.bold.as_ref().or(fonts.regular.as_ref());
    let reg = fonts.regular.as_ref();
    let (padx, pady) = (9.0 * s, 7.0 * s);
    let max_w = g.right - g.label_x - 2.0 * padx;
    let mut px = z.arc_label * s;
    let width = |px: f32| {
        bold.map_or(px * 0.6 * name.chars().count() as f32, |f| {
            f.width(name, px)
        })
    };
    if width(px) > max_w {
        px = (px * max_w / width(px)).max(LABEL_MIN * s);
    }
    let kp = KOTE_PX * s;
    let cap = bold.map_or(px * 0.7, |f| f.cap_height(px));
    let kcap = reg.map_or(kp * 0.7, |f| f.cap_height(kp));
    let kw = reg.map_or(0.0, |f| f.width(kote, kp));
    let w = (width(px).max(kw) + 2.0 * padx).ceil();
    let h = (pady + cap + 6.0 * s + kcap + pady).ceil();
    let mut c = Canvas::new(w as usize, h as usize);
    let mut p = Path::new();
    p.rounded_rect(0.0, 0.0, w, h, z.corner_radius * s * 0.6);
    c.fill(&p, t.ui.hud_bg);
    sk_ui::widgets::text(&mut c, bold, name, px, padx, pady + cap, t.ui.text);
    sk_ui::widgets::text(&mut c, reg, kote, kp, padx, h - pady, t.ui.text_dim);
    c
}

/// Kleines Schild eines Nachbargeschosses an der Spitze.
pub fn paint_small(fonts: &Fonts, name: &str, g: &Geo, t: &Theme, color: Rgba) -> Canvas {
    let s = g.s;
    let z = &t.size;
    let f = fonts.regular.as_ref();
    let px = z.arc_label_small * s;
    let (padx, pady) = (6.0 * s, 4.5 * s);
    let cap = f.map_or(px * 0.7, |f| f.cap_height(px));
    let tw = f.map_or(px * 0.6 * name.chars().count() as f32, |f| {
        f.width(name, px)
    });
    let (w, h) = ((tw + 2.0 * padx).ceil(), (cap + 2.0 * pady).ceil());
    let mut c = Canvas::new(w as usize, h as usize);
    let mut p = Path::new();
    p.rounded_rect(0.0, 0.0, w, h, 4.0 * s);
    c.fill(&p, t.ui.hud_bg);
    sk_ui::widgets::text(&mut c, f, name, px, padx, pady + cap, color);
    c
}

/// Hinweis an einer Spitze: Zeile und Kote darunter.
pub fn paint_hint(fonts: &Fonts, line: &str, kote: &str, s: f32, t: &Theme) -> Canvas {
    let bold = fonts.bold.as_ref().or(fonts.regular.as_ref());
    let reg = fonts.regular.as_ref();
    let (px, kp) = (t.size.font_small * s, KOTE_PX * s);
    let (pad, b) = ((8.0 * s).round(), s.round().max(1.0));
    let cap = bold.map_or(px * 0.7, |f| f.cap_height(px));
    let kcap = reg.map_or(kp * 0.7, |f| f.cap_height(kp));
    let lw = bold.map_or(0.0, |f| f.width(line, px));
    let kw = reg.map_or(0.0, |f| f.width(kote, kp));
    let two = !kote.is_empty();
    let w = (lw.max(kw) + 2.0 * pad).ceil();
    let h = if two {
        (pad + cap + 6.0 * s + kcap + pad).ceil()
    } else {
        (cap + 2.0 * pad).ceil()
    };
    let mut c = Canvas::new(w as usize, h as usize);
    let rad = 4.0 * s;
    let mut p = Path::new();
    p.rounded_rect(0.0, 0.0, w, h, rad);
    c.fill(&p, t.ui.border);
    let mut p = Path::new();
    p.rounded_rect(b, b, w - 2.0 * b, h - 2.0 * b, rad - b);
    c.fill(&p, t.ui.tooltip_bg);
    sk_ui::widgets::text(&mut c, bold, line, px, pad, pad + cap, t.ui.tooltip_text);
    if two {
        sk_ui::widgets::text(&mut c, reg, kote, kp, pad, h - pad, t.ui.text_dim);
    }
    c
}

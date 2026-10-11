//! Fenster der IFC-Vorschau (ifc/plan.md §6): eigenes Programmfenster wie
//! der Geschossmanager, links der Baum, in der Mitte das 3D-Bild, rechts die
//! Eigenschaften des gewählten Bauteils, unten eine Fußzeile mit
//! „Geschosse einzeln“ und „Ganzes Modell“. Alles in Software gezeichnet.

use super::baum::{self, Sicht, Zeile, Ziel};
use super::bild::{self, Bild, Szene};
use super::eigenschaften::{self, Eintrag};
use crate::camera::Camera;
use sk_ifc::lesen::Vorschau;
use sk_math::{vec3, Vec3};
use sk_paint::font::Font;
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{CaptionArea, Event, Key, MouseButton, WindowCommand};
use sk_ui::theme::Theme;
use sk_ui::titlebar::{Button, TitleBar};
use sk_ui::widgets::Fonts;
use std::collections::BTreeSet;

pub const TITEL: &str = "IFC-Vorschau";

/// Breiten und Höhen in dip.
const BAUM_B: f32 = 290.0;
const EIG_B: f32 = 330.0;
const ZEILE: f32 = 22.0;
const FUSS: f32 = 30.0;
const EINZUG: f32 = 14.0;
const RAND: f32 = 8.0;
/// Ab so viel Bewegung (px) ist ein Klick ein Ziehen.
const ZIEHEN_AB: f64 = 3.0;

/// Was ein Ereignis für die App bedeutet.
#[derive(Debug, PartialEq)]
pub enum Out {
    Command(WindowCommand),
    Close,
}

/// Bereich des Fensters unter der Maus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bereich {
    Titel,
    Baum,
    Bild,
    Eigenschaften,
    Fuss,
}

/// Knöpfe der Fußzeile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Knopf {
    Einzeln,
    Ganz,
}

struct Ziehen {
    knopf: MouseButton,
    /// Gegriffener Punkt im Modell: Drehpunkt, beim Schieben seine Tiefe.
    griff: Vec3,
    x: f64,
    y: f64,
    bewegt: bool,
}

pub struct IfcFenster {
    pub open: bool,
    pub w: u32,
    pub h: u32,
    pub title: TitleBar,
    pub dirty: bool,
    pub v: Vorschau,
    kanten: Vec<Vec<(Vec3, Vec3)>>,
    pub kamera: Camera,
    pub sichtbar: Vec<bool>,
    pub gewaehlt: Option<usize>,
    /// Klappzustand des Baums (siehe [`baum::zeilen`]).
    umgeklappt: BTreeSet<String>,
    /// „Geschosse einzeln“: Klick auf ein Geschoss zeigt nur dieses.
    pub einzeln: bool,
    /// Bildlauf von Baum und Eigenschaften (px).
    baum_y: f32,
    eig_y: f32,
    ziehen: Option<Ziehen>,
    /// Knöpfe der Fußzeile, wie zuletzt gezeichnet: (Knopf, x, Breite).
    knopf_lage: Vec<(Knopf, f32, f32)>,
    /// Zuletzt gezeichnetes 3D-Bild; `None` = neu zeichnen.
    bild: Option<Bild>,
    theme_rev: u64,
    canvas: Canvas,
    shown: Vec<u8>,
}

/// Lage der Bereiche in Fensterpixeln.
#[derive(Clone, Copy, Debug)]
struct Lage {
    oben: f32,
    fuss: f32,
    baum_r: f32,
    eig_l: f32,
}

/// Kamera auf alles Sichtbare; ohne Form ein Kasten um den Ursprung.
fn passend(v: &Vorschau, sichtbar: &[bool]) -> Camera {
    let (lo, hi) = bild::grenzen(&v.teile, sichtbar)
        .or_else(|| bild::grenzen(&v.teile, &[]))
        .unwrap_or((vec3(0.0, 0.0, 0.0), vec3(10000.0, 10000.0, 3000.0)));
    bild::passend(lo, hi)
}

impl IfcFenster {
    /// Vorschau der gelesenen Datei; `datei` steht in der Titelleiste.
    pub fn new(v: Vorschau, datei: &str) -> IfcFenster {
        let mut title = TitleBar::new(1.0);
        title.side = true;
        title.caption = if datei.is_empty() {
            TITEL.into()
        } else {
            format!("{TITEL} – {datei}")
        };
        let sichtbar = vec![true; v.teile.len()];
        let kanten = v.teile.iter().map(bild::kanten).collect();
        IfcFenster {
            open: false,
            w: 0,
            h: 0,
            title,
            dirty: true,
            kamera: passend(&v, &sichtbar),
            kanten,
            sichtbar,
            gewaehlt: None,
            umgeklappt: BTreeSet::new(),
            einzeln: false,
            baum_y: 0.0,
            eig_y: 0.0,
            ziehen: None,
            knopf_lage: Vec::new(),
            bild: None,
            theme_rev: 0,
            canvas: Canvas::new(0, 0),
            shown: Vec::new(),
            v,
        }
    }

    pub fn caption_area(&self) -> CaptionArea {
        CaptionArea {
            height: self.title.height(),
            buttons_width: self.title.buttons_width(),
            left_width: 0,
        }
    }

    fn s(&self) -> f32 {
        self.title.scale
    }

    fn lage(&self) -> Lage {
        let (w, h, s) = (self.w as f32, self.h as f32, self.s());
        let oben = self.title.height() as f32;
        let baum_r = (BAUM_B * s).min(w * 0.3).floor();
        let eig_l = (w - (EIG_B * s).min(w * 0.32)).floor().max(baum_r);
        Lage {
            oben,
            fuss: (h - FUSS * s).floor().max(oben),
            baum_r,
            eig_l,
        }
    }

    fn bereich(&self, x: f64, y: f64) -> Bereich {
        let l = self.lage();
        let (x, y) = (x as f32, y as f32);
        if y < l.oben {
            Bereich::Titel
        } else if y >= l.fuss {
            Bereich::Fuss
        } else if x < l.baum_r {
            Bereich::Baum
        } else if x >= l.eig_l {
            Bereich::Eigenschaften
        } else {
            Bereich::Bild
        }
    }

    /// Sichtbarkeit, Wahl oder Kamera geändert: Bild neu.
    fn neu(&mut self) {
        self.bild = None;
        self.dirty = true;
    }

    fn zeilen(&self) -> Vec<Zeile> {
        baum::zeilen(&self.v, &self.umgeklappt)
    }

    fn eintraege(&self) -> Vec<Eintrag> {
        match self.gewaehlt.and_then(|i| self.v.teile.get(i)) {
            Some(t) => eigenschaften::bauteil(t),
            None => eigenschaften::uebersicht(&self.v),
        }
    }

    /// Bauteil wählen (oder abwählen) und im Baum zeigen.
    pub fn waehlen(&mut self, t: Option<usize>) {
        if self.gewaehlt == t {
            return;
        }
        self.gewaehlt = t;
        self.eig_y = 0.0;
        if let Some(t) = t {
            baum::aufdecken(&self.v, t, &mut self.umgeklappt);
            // Zeile in den sichtbaren Teil des Baums holen
            let z = self.zeilen();
            if let Some(i) = z.iter().position(|z| z.ziel == Ziel::Teil(t)) {
                let l = self.lage();
                let zh = ZEILE * self.s();
                let y = RAND * self.s() + i as f32 * zh;
                let hoch = l.fuss - l.oben;
                if y < self.baum_y || y + zh > self.baum_y + hoch {
                    self.baum_y = (y - hoch * 0.4).max(0.0);
                }
            }
        }
        self.neu();
    }

    /// Ereignis im Fenster (Fensterpixel).
    pub fn handle(&mut self, e: &Event) -> Option<Out> {
        match *e {
            Event::Resized { width, height } => {
                (self.w, self.h) = (width, height);
                self.neu();
            }
            Event::ScaleChanged(s) => {
                self.title.scale = s;
                self.neu();
            }
            Event::Maximized(m) => {
                self.title.maximized = m;
                self.dirty = true;
            }
            Event::Focus(f) => {
                self.title.active = f;
                if !f {
                    self.ziehen = None;
                }
                self.dirty = true;
            }
            Event::Redraw => self.dirty = true,
            Event::CloseRequested { .. } => return Some(Out::Close),
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } => self.waehlen(None),
            Event::Key { .. } | Event::Text(_) => {}
            Event::MouseLeave => {
                if self.title.hover.take().is_some() {
                    self.dirty = true;
                }
            }
            Event::MouseMove { x, y, .. } => {
                let b = self.title.button_at(x, y, self.w);
                if b != self.title.hover {
                    self.title.hover = b;
                    self.dirty = true;
                }
                self.bewegen(x, y);
            }
            Event::MouseDown { button, x, y, .. } => match self.bereich(x, y) {
                Bereich::Titel if button == MouseButton::Left => {
                    if let Some(b) = self.title.button_at(x, y, self.w) {
                        self.title.pressed = Some(b);
                        self.dirty = true;
                    }
                }
                Bereich::Bild => {
                    self.ziehen = Some(Ziehen {
                        knopf: button,
                        griff: self.griff(x, y),
                        x,
                        y,
                        bewegt: false,
                    })
                }
                Bereich::Baum if button == MouseButton::Left => self.baum_klick(x, y),
                Bereich::Fuss if button == MouseButton::Left => match self.knopf_bei(x, y) {
                    Some(Knopf::Einzeln) => {
                        self.einzeln = !self.einzeln;
                        if !self.einzeln {
                            self.sichtbar.iter_mut().for_each(|s| *s = true);
                        }
                        self.neu();
                    }
                    Some(Knopf::Ganz) => {
                        self.kamera = passend(&self.v, &self.sichtbar);
                        self.neu();
                    }
                    None => {}
                },
                _ => {}
            },
            Event::MouseUp { button, x, y, .. } => {
                if button == MouseButton::Left {
                    if let Some(p) = self.title.pressed.take() {
                        self.dirty = true;
                        if self.title.button_at(x, y, self.w) == Some(p) {
                            return Some(match p {
                                Button::Minimize => Out::Command(WindowCommand::Minimize),
                                Button::Maximize => Out::Command(WindowCommand::ToggleMaximize),
                                _ => Out::Close,
                            });
                        }
                        return None;
                    }
                }
                if let Some(z) = self.ziehen.take() {
                    if z.bewegt {
                        // Wieder fein zeichnen
                        self.neu();
                    } else if z.knopf == button && button == MouseButton::Left {
                        let l = self.lage();
                        let t = self
                            .bild
                            .as_ref()
                            .and_then(|b| b.teil_bei(x - l.baum_r as f64, y - l.oben as f64));
                        // Nochmal auf dasselbe Teil: abwählen
                        self.waehlen(if t == self.gewaehlt { None } else { t });
                    }
                }
            }
            Event::Wheel { delta, x, y, .. } => {
                let schritt = 3.0 * ZEILE * self.s() * delta as f32;
                match self.bereich(x, y) {
                    Bereich::Bild => {
                        let p = self.griff(x, y);
                        self.kamera.zoom(p, delta);
                        self.neu();
                    }
                    Bereich::Baum => {
                        self.baum_y -= schritt;
                        self.dirty = true;
                    }
                    Bereich::Eigenschaften => {
                        self.eig_y -= schritt;
                        self.dirty = true;
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Punkt im Modell unter der Maus wie in der Ansicht; ins Leere in der
    /// Tiefe des Blickpunkts.
    fn griff(&self, x: f64, y: f64) -> Vec3 {
        let l = self.lage();
        let (bx, by) = (x - l.baum_r as f64, y - l.oben as f64);
        let (w, h) = ((l.eig_l - l.baum_r) as f64, (l.fuss - l.oben) as f64);
        self.bild
            .as_ref()
            .and_then(|b| b.punkt_bei(&self.kamera, bx, by))
            .unwrap_or_else(|| {
                let (o, r) = self.kamera.ray(bx, by, w.max(1.0), h.max(1.0));
                o + r * self.kamera.focus
            })
    }

    fn bewegen(&mut self, x: f64, y: f64) {
        let h = (self.lage().fuss - self.lage().oben) as f64;
        let Some(z) = self.ziehen.as_mut() else {
            return;
        };
        let (dx, dy) = (x - z.x, y - z.y);
        if !z.bewegt && dx.hypot(dy) < ZIEHEN_AB {
            return;
        }
        z.bewegt = true;
        (z.x, z.y) = (x, y);
        match z.knopf {
            MouseButton::Left => self.kamera.orbit(z.griff, dx, dy),
            _ => {
                let tiefe = (z.griff - self.kamera.eye).dot(self.kamera.forward());
                self.kamera.pan(tiefe, dx, dy, h);
            }
        }
        self.neu();
    }

    /// Klick in den Baum: Dreieck klappt, Kästchen blendet ein und aus, der
    /// Text wählt ein Bauteil oder klappt; mit „Geschosse einzeln“ zeigt ein
    /// Geschoss nur sich.
    fn baum_klick(&mut self, x: f64, y: f64) {
        let l = self.lage();
        let s = self.s();
        let zh = ZEILE * s;
        let i = ((y as f32 - l.oben - RAND * s + self.baum_y) / zh).floor();
        let z = self.zeilen();
        let Some(zeile) = (i >= 0.0).then(|| z.get(i as usize)).flatten() else {
            return;
        };
        let x0 = RAND * s + zeile.tiefe as f32 * EINZUG * s;
        let x = x as f32;
        let klappen = |f: &mut IfcFenster| {
            if zeile.offen.is_some() && !f.umgeklappt.remove(&zeile.schluessel) {
                f.umgeklappt.insert(zeile.schluessel.clone());
            }
            f.dirty = true;
        };
        if x < x0 + 14.0 * s {
            klappen(self);
        } else if x < x0 + 32.0 * s {
            baum::umschalten(&zeile.teile, &mut self.sichtbar);
            self.neu();
        } else {
            match &zeile.ziel {
                Ziel::Teil(t) => {
                    let t = *t;
                    self.waehlen(if self.gewaehlt == Some(t) {
                        None
                    } else {
                        Some(t)
                    });
                }
                Ziel::Knoten { klasse } if self.einzeln && klasse == "IfcBuildingStorey" => {
                    baum::allein(&zeile.teile, &mut self.sichtbar);
                    self.neu();
                }
                _ => klappen(self),
            }
        }
    }

    /// Knöpfe der Fußzeile: (Knopf, Text, x, Breite), rechts ausgerichtet.
    fn knoepfe(&self, font: Option<&Font>, px: f32) -> Vec<(Knopf, &'static str, f32, f32)> {
        let s = self.s();
        let mut x = self.w as f32 - RAND * s;
        let mut out = Vec::new();
        for (k, t) in [
            (Knopf::Ganz, "Ganzes Modell"),
            (Knopf::Einzeln, "Geschosse einzeln"),
        ] {
            let b = font.map_or(t.len() as f32 * px * 0.55, |f| f.width(t, px)) + 20.0 * s;
            x -= b;
            out.push((k, t, x, b));
            x -= 6.0 * s;
        }
        out
    }

    fn knopf_bei(&self, x: f64, y: f64) -> Option<Knopf> {
        let l = self.lage();
        let s = self.s();
        let (x, y) = (x as f32, y as f32);
        if y < l.fuss + 4.0 * s || y > self.h as f32 - 4.0 * s {
            return None;
        }
        self.knopf_lage
            .iter()
            .copied()
            .find(|&(_, bx, bw)| x >= bx && x < bx + bw)
            .map(|(k, ..)| k)
    }
}

/// Text auf `max` Pixel kürzen, mit „…“ am Ende.
fn kuerzen(f: &Font, t: &str, px: f32, max: f32) -> String {
    if f.width(t, px) <= max {
        return t.into();
    }
    let mut s: String = t.into();
    while !s.is_empty() && f.width(&s, px) + f.width("…", px) > max {
        s.pop();
    }
    format!("{s}…")
}

fn rgb(c: Rgba) -> [u8; 3] {
    [c.0, c.1, c.2]
}

/// Höhe der Eigenschaften (px), wie [`IfcFenster::eig_bild`] sie zeichnet.
fn eig_hoehe(e: &[Eintrag], s: f32) -> f32 {
    let kopf = e.iter().filter(|e| matches!(e, Eintrag::Kopf(_))).count();
    2.0 * RAND * s + e.len() as f32 * ZEILE * s + kopf.saturating_sub(1) as f32 * 6.0 * s
}

impl IfcFenster {
    /// Fensterbild (vormultipliziert), wenn es neu gezeigt werden muss.
    pub fn frame(&mut self, t: &Theme, fonts: &Fonts) -> Option<&[u8]> {
        if t.rev != self.theme_rev {
            self.theme_rev = t.rev;
            self.neu();
        }
        if self.w == 0 || self.h == 0 || !self.dirty {
            return None;
        }
        self.dirty = false;
        let (w, h) = (self.w as usize, self.h as usize);
        let l = self.lage();
        let s = self.s();
        let mut c = std::mem::replace(&mut self.canvas, Canvas::new(0, 0));
        c.reuse(w, h);
        c.clear(t.ui.bg);
        // 3D-Bild in der Mitte
        let (bw, bh) = ((l.eig_l - l.baum_r) as usize, (l.fuss - l.oben) as usize);
        if bw > 0 && bh > 0 {
            let alt = match &self.bild {
                Some(b) => b.w != bw || b.h != bh,
                None => true,
            };
            if alt {
                let szene = Szene {
                    teile: &self.v.teile,
                    sichtbar: &self.sichtbar,
                    gewaehlt: self.gewaehlt,
                    hinter: rgb(t.ui.field),
                    kante: rgb(t.env.edge),
                    wahl: rgb(t.ui.accent),
                };
                // Beim Ziehen gröber, damit das Bild der Maus folgt
                let ss = if self.ziehen.as_ref().is_some_and(|z| z.bewegt) {
                    1
                } else {
                    2
                };
                self.bild = Some(bild::zeichnen(
                    &szene,
                    &self.kanten,
                    &self.kamera,
                    (bw, bh),
                    ss,
                ));
            }
            if let Some(b) = &self.bild {
                let img = Canvas::from_fn(bw, bh, |x, y| {
                    let o = (y * bw + x) * 4;
                    Rgba(b.rgba[o], b.rgba[o + 1], b.rgba[o + 2], 255)
                });
                c.blit(&img, l.baum_r as i32, l.oben as i32);
            }
        }
        let hoch = l.fuss - l.oben;
        if l.baum_r >= 1.0 && hoch >= 1.0 {
            let b = self.baum_bild(t, fonts, l.baum_r as usize, hoch as usize);
            c.blit(&b, 0, l.oben as i32);
        }
        let ew = self.w as f32 - l.eig_l;
        if ew >= 1.0 && hoch >= 1.0 {
            let e = self.eig_bild(t, fonts, ew as usize, hoch as usize);
            c.blit(&e, l.eig_l as i32, l.oben as i32);
        }
        let linie = s.max(1.0).round();
        c.fill_rect(l.baum_r - linie, l.oben, linie, hoch, t.ui.border);
        c.fill_rect(l.eig_l, l.oben, linie, hoch, t.ui.border);
        c.fill_rect(0.0, l.fuss, w as f32, linie, t.ui.border);
        self.fuss_zeichnen(&mut c, t, fonts, l);
        let bar = self.title.paint(t, fonts.regular.as_ref(), self.w);
        c.blit(&bar, 0, 0);
        c.premul_rgba8_into(&mut self.shown);
        self.canvas = c;
        Some(&self.shown)
    }

    fn baum_bild(&mut self, t: &Theme, fonts: &Fonts, w: usize, h: usize) -> Canvas {
        let s = self.s();
        let zh = ZEILE * s;
        let px = t.size.font_small * s;
        let zeilen = self.zeilen();
        let inhalt = 2.0 * RAND * s + zeilen.len() as f32 * zh;
        self.baum_y = self.baum_y.clamp(0.0, (inhalt - h as f32).max(0.0));
        let mut c = Canvas::new(w, h);
        let wf = w as f32;
        for (i, z) in zeilen.iter().enumerate() {
            let y = RAND * s + i as f32 * zh - self.baum_y;
            if y + zh < 0.0 || y > h as f32 {
                continue;
            }
            let x0 = RAND * s + z.tiefe as f32 * EINZUG * s;
            let gew = matches!(z.ziel, Ziel::Teil(n) if Some(n) == self.gewaehlt);
            if gew {
                c.fill_rect(0.0, y, wf, zh, t.ui.accent);
            }
            let (farbe, dim) = if gew {
                (t.ui.on_accent, t.ui.on_accent)
            } else {
                (t.ui.text, t.ui.text_dim)
            };
            let cy = y + zh * 0.5;
            if let Some(offen) = z.offen {
                let (cx, mut p) = (x0 + 5.0 * s, Path::new());
                if offen {
                    p.move_to(cx - 4.0 * s, cy - 2.0 * s)
                        .line_to(cx + 4.0 * s, cy - 2.0 * s)
                        .line_to(cx, cy + 3.0 * s);
                } else {
                    p.move_to(cx - 2.0 * s, cy - 4.0 * s)
                        .line_to(cx + 3.0 * s, cy)
                        .line_to(cx - 2.0 * s, cy + 4.0 * s);
                }
                p.close();
                c.fill(&p, dim);
            }
            // Kästchen: voll = sichtbar, halb = teils, leer = aus
            let (bx, bs) = (x0 + 16.0 * s, 11.0 * s);
            let by = cy - bs * 0.5;
            let rand = s.max(1.0);
            match baum::sicht(&z.teile, &self.sichtbar) {
                _ if z.teile.is_empty() => {}
                Sicht::Alle => c.fill_rect(
                    bx,
                    by,
                    bs,
                    bs,
                    if gew { t.ui.on_accent } else { t.ui.accent },
                ),
                Sicht::Teils | Sicht::Keine => {
                    c.fill_rect(bx, by, bs, bs, dim);
                    let innen = if gew { t.ui.accent } else { t.ui.bg };
                    c.fill_rect(
                        bx + rand,
                        by + rand,
                        bs - 2.0 * rand,
                        bs - 2.0 * rand,
                        innen,
                    );
                    if baum::sicht(&z.teile, &self.sichtbar) == Sicht::Teils {
                        c.fill_rect(bx + 3.0 * rand, cy - rand, bs - 6.0 * rand, 2.0 * rand, dim);
                    }
                }
            }
            let Some(f) = fonts.regular.as_ref() else {
                continue;
            };
            let base = cy + f.cap_height(px) * 0.5;
            let tx = x0 + 32.0 * s;
            // Der Name hat Vorrang, der Zusatz bekommt höchstens 40 %
            let frei = (wf - tx - RAND * s).max(0.0);
            let zw = if z.zusatz.is_empty() {
                0.0
            } else {
                f.width(&z.zusatz, px).min(frei * 0.4)
            };
            let text = kuerzen(f, &z.text, px, (frei - zw - 8.0 * s).max(0.0));
            f.draw(&mut c, &text, px, tx, base, farbe);
            if zw > 0.0 {
                let zusatz = kuerzen(f, &z.zusatz, px, zw);
                let zx = wf - RAND * s - f.width(&zusatz, px);
                f.draw(&mut c, &zusatz, px, zx, base, dim);
            }
        }
        c
    }

    fn eig_bild(&mut self, t: &Theme, fonts: &Fonts, w: usize, h: usize) -> Canvas {
        let s = self.s();
        let zh = ZEILE * s;
        let px = t.size.font_small * s;
        let e = self.eintraege();
        self.eig_y = self
            .eig_y
            .clamp(0.0, (eig_hoehe(&e, s) - h as f32).max(0.0));
        let mut c = Canvas::new(w, h);
        let (Some(f), wf) = (fonts.regular.as_ref(), w as f32) else {
            return c;
        };
        let fett = fonts.bold.as_ref().unwrap_or(f);
        let (kx, vx) = (RAND * s + 4.0 * s, (wf * 0.45).round());
        let mut y = RAND * s - self.eig_y;
        for (i, e) in e.iter().enumerate() {
            if matches!(e, Eintrag::Kopf(_)) && i > 0 {
                y += 6.0 * s;
            }
            let base = y + zh * 0.5 + f.cap_height(px) * 0.5;
            if y + zh >= 0.0 && y <= h as f32 {
                match e {
                    Eintrag::Kopf(k) => {
                        let k = kuerzen(fett, k, px, wf - kx - RAND * s);
                        fett.draw(&mut c, &k, px, kx, base, t.ui.text);
                    }
                    Eintrag::Wert(k, v) if v.is_empty() => {
                        let k = kuerzen(f, k, px, wf - kx - RAND * s);
                        f.draw(&mut c, &k, px, kx, base, t.ui.text);
                    }
                    Eintrag::Wert(k, v) => {
                        let k = kuerzen(f, k, px, vx - kx - 6.0 * s);
                        f.draw(&mut c, &k, px, kx, base, t.ui.text_dim);
                        let v = kuerzen(f, v, px, wf - vx - RAND * s);
                        f.draw(&mut c, &v, px, vx, base, t.ui.text);
                    }
                }
            }
            y += zh;
        }
        c
    }

    fn fuss_zeichnen(&mut self, c: &mut Canvas, t: &Theme, fonts: &Fonts, l: Lage) {
        let s = self.s();
        let px = t.size.font_small * s;
        let f = fonts.regular.as_ref();
        self.knopf_lage = self
            .knoepfe(f, px)
            .into_iter()
            .map(|(k, _, x, b)| (k, x, b))
            .collect();
        let Some(f) = f else {
            return;
        };
        let (y, kh) = (l.fuss + 4.0 * s, FUSS * s - 8.0 * s);
        let base = y + kh * 0.5 + f.cap_height(px) * 0.5;
        let mut links = self.w as f32;
        for (k, text, x, b) in self.knoepfe(Some(f), px) {
            let an = k == Knopf::Einzeln && self.einzeln;
            let mut p = Path::new();
            p.rounded_rect(x, y, b, kh, t.size.corner_radius * s);
            c.fill(&p, if an { t.ui.accent } else { t.ui.field });
            let farbe = if an { t.ui.on_accent } else { t.ui.text };
            f.draw(c, text, px, x + 10.0 * s, base, farbe);
            links = links.min(x);
        }
        let n = self.v.teile.len();
        let mut info = format!(
            "{} · {} {}",
            self.v.schema,
            n,
            if n == 1 { "Bauteil" } else { "Bauteile" }
        );
        if self.v.ohne_form > 0 {
            info.push_str(&format!(", {} ohne Form", self.v.ohne_form));
        }
        info.push_str(" · Ziehen dreht, rechte Taste schiebt, Rad nähert");
        let info = kuerzen(f, &info, px, (links - 3.0 * RAND * s).max(0.0));
        f.draw(c, &info, px, RAND * s + 4.0 * s, base, t.ui.text_dim);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_platform::Modifiers;

    const M: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    /// Haus aus dem Baumtest, jedes Bauteil ein Würfel nebeneinander, das
    /// OG darüber.
    fn fenster() -> IfcFenster {
        let mut v = baum::tests::beispiel();
        for (i, t) in v.teile.iter_mut().enumerate() {
            let o = if i == 3 {
                vec3(0.0, 0.0, 3000.0)
            } else {
                vec3(i as f64 * 1500.0, 0.0, 0.0)
            };
            t.netze = bild::tests::wuerfel([0.8, 0.7, 0.6], o).netze;
        }
        v.ohne_form = 0;
        let mut f = IfcFenster::new(v, "haus.ifc");
        f.handle(&Event::ScaleChanged(1.0));
        f.handle(&Event::Resized {
            width: 1200,
            height: 800,
        });
        f
    }

    fn klick(f: &mut IfcFenster, x: f64, y: f64) -> Option<Out> {
        f.handle(&Event::MouseMove { x, y, mods: M });
        f.handle(&Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        });
        f.handle(&Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        })
    }

    /// Fensterpunkt, an dem das Bild das Bauteil `t` zeigt.
    fn punkt_von(f: &IfcFenster, t: usize) -> (f64, f64) {
        let l = f.lage();
        let b = f.bild.as_ref().expect("Bild gezeichnet");
        let i = b
            .teil
            .iter()
            .position(|&n| n == t as u32)
            .expect("Teil im Bild");
        (
            l.baum_r as f64 + (i % b.w) as f64 + 0.5,
            l.oben as f64 + (i / b.w) as f64 + 0.5,
        )
    }

    /// Drei Bereiche: Baum links, Bild in der Mitte, Eigenschaften rechts;
    /// unverändert kein neues Bild.
    #[test]
    fn bild_in_drei_bereichen() {
        let (th, fonts) = (Theme::dark(), Fonts::system());
        let mut f = fenster();
        let px = f.frame(&th, &fonts).expect("erstes Bild").to_vec();
        assert_eq!(px.len(), 1200 * 800 * 4);
        assert!(f.frame(&th, &fonts).is_none());
        let l = f.lage();
        assert_eq!((l.baum_r, l.eig_l), (290.0, 870.0));
        let b = f.bild.as_ref().unwrap();
        assert_eq!((b.w, b.h), (580, (l.fuss - l.oben) as usize));
        for t in 0..4 {
            punkt_von(&f, t);
        }
        // Die Ecke des Bildes ist Hintergrund (Feldfarbe)
        let at = |x: usize, y: usize| px[(y * 1200 + x) * 4..][..3].to_vec();
        let feld = th.ui.field;
        assert_eq!(at(300, l.oben as usize + 3), [feld.0, feld.1, feld.2]);
        assert_eq!(f.title.caption, "IFC-Vorschau – haus.ifc");
    }

    /// Klick ins Bild wählt, zeigt die Eigenschaften und klappt den Baum
    /// bis zum Bauteil auf; nochmal oder Esc wählt ab. Ziehen dreht ohne
    /// Wahl.
    #[test]
    fn waehlen_und_drehen() {
        let (th, fonts) = (Theme::dark(), Fonts::system());
        let mut f = fenster();
        f.frame(&th, &fonts);
        let (x, y) = punkt_von(&f, 2);
        assert_eq!(klick(&mut f, x, y), None);
        assert_eq!(f.gewaehlt, Some(2));
        assert_eq!(
            f.eintraege()[1],
            Eintrag::Wert("Klasse".into(), "IfcSlab".into())
        );
        assert!(f.zeilen().iter().any(|z| z.ziel == Ziel::Teil(2)));
        assert!(f.bild.is_none(), "Wahl zeichnet das Bild neu");
        f.frame(&th, &fonts);
        klick(&mut f, x, y);
        assert_eq!(f.gewaehlt, None);
        f.waehlen(Some(1));
        f.handle(&Event::Key {
            key: Key::Escape,
            down: true,
            repeat: false,
            mods: M,
        });
        assert_eq!(f.gewaehlt, None);
        // Ziehen
        let dreh = f.kamera.yaw;
        f.handle(&Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        });
        f.handle(&Event::MouseMove {
            x: x + 40.0,
            y,
            mods: M,
        });
        f.handle(&Event::MouseUp {
            button: MouseButton::Left,
            x: x + 40.0,
            y,
            mods: M,
        });
        assert!(f.kamera.yaw != dreh);
        assert_eq!(f.gewaehlt, None);
        // Rad nähert, im Baum blättert es
        let a = f.kamera.focus;
        f.handle(&Event::Wheel {
            delta: 1.0,
            x,
            y,
            mods: M,
        });
        assert!(f.kamera.focus < a);
    }

    /// Kästchen im Baum blendet aus; „Geschosse einzeln“ zeigt ein Geschoss
    /// allein; „Ganzes Modell“ richtet die Kamera neu.
    #[test]
    fn baum_und_fusszeile() {
        let (th, fonts) = (Theme::dark(), Fonts::system());
        let mut f = fenster();
        f.frame(&th, &fonts);
        let l = f.lage();
        let zeile_y = |i: usize| (l.oben + RAND + (i as f32 + 0.5) * ZEILE) as f64;
        let z = f.zeilen();
        let og = z.iter().position(|z| z.text == "OG").unwrap();
        let eg = z.iter().position(|z| z.text == "EG").unwrap();
        // Kästchen des OG: x0 = Rand + Tiefe 3 · Einzug, Kästchen ab +16
        let kx = (RAND + 3.0 * EINZUG + 20.0) as f64;
        klick(&mut f, kx, zeile_y(og));
        assert_eq!(f.sichtbar, [true, true, true, false]);
        klick(&mut f, kx, zeile_y(og));
        assert_eq!(f.sichtbar, [true; 4]);
        // Dreieck klappt das EG zu
        klick(&mut f, (RAND + 3.0 * EINZUG + 4.0) as f64, zeile_y(eg));
        assert_eq!(f.zeilen()[eg + 1].text, "OG");
        // Geschosse einzeln
        assert!(f.knopf_lage.len() == 2 || fonts.regular.is_none());
        if let Some(&(_, bx, bw)) = f.knopf_lage.iter().find(|k| k.0 == Knopf::Einzeln) {
            let fy = (l.fuss + FUSS * 0.5) as f64;
            klick(&mut f, (bx + bw * 0.5) as f64, fy);
            assert!(f.einzeln);
            let og = f.zeilen().iter().position(|z| z.text == "OG").unwrap();
            klick(&mut f, 150.0, zeile_y(og));
            assert_eq!(f.sichtbar, [false, false, false, true]);
            let k = f.kamera.clone();
            let (gx, gw) = f
                .knopf_lage
                .iter()
                .find(|k| k.0 == Knopf::Ganz)
                .map(|k| (k.1, k.2))
                .unwrap();
            klick(&mut f, (gx + gw * 0.5) as f64, fy);
            assert!(f.kamera != k, "Kamera auf das OG");
            f.frame(&th, &fonts);
            assert!(f
                .bild
                .as_ref()
                .unwrap()
                .teil
                .iter()
                .all(|&t| t == 3 || t == u32::MAX));
            klick(&mut f, (bx + bw * 0.5) as f64, fy);
            assert!(!f.einzeln);
            assert_eq!(f.sichtbar, [true; 4], "aus: wieder alles");
        }
    }

    /// Titelleiste: Schließen und Minimieren.
    #[test]
    fn titelleiste() {
        let (th, fonts) = (Theme::dark(), Fonts::system());
        let mut f = fenster();
        f.frame(&th, &fonts);
        let bw = f.title.button_width() as f64;
        assert_eq!(klick(&mut f, 1200.0 - bw * 0.5, 10.0), Some(Out::Close));
        assert_eq!(
            klick(&mut f, 1200.0 - bw * 2.5, 10.0),
            Some(Out::Command(WindowCommand::Minimize))
        );
    }

    /// Bild einer Datei zum Ansehen: `IFC_DATEI=… IFC_PNG=… cargo test
    /// bild_einer_datei -- --ignored`.
    #[test]
    #[ignore]
    fn bild_einer_datei() {
        let (Ok(ein), Ok(aus)) = (std::env::var("IFC_DATEI"), std::env::var("IFC_PNG")) else {
            return;
        };
        let text = std::fs::read_to_string(&ein).unwrap();
        let v = sk_ifc::lesen::vorschau(&sk_ifc::step::read(&text).unwrap());
        let mut f = IfcFenster::new(v, "datei.ifc");
        f.handle(&Event::Resized {
            width: 1400,
            height: 860,
        });
        let (th, fonts) = (Theme::dark(), Fonts::system());
        let t0 = std::time::Instant::now();
        f.frame(&th, &fonts);
        eprintln!("erstes Bild {:?}", t0.elapsed());
        if let Ok(n) = std::env::var("IFC_WAHL") {
            f.waehlen(n.parse().ok());
            f.frame(&th, &fonts);
        }
        f.dirty = true;
        f.frame(&th, &fonts);
        let mut c = Canvas::new(0, 0);
        std::mem::swap(&mut c, &mut f.canvas);
        std::fs::write(aus, c.to_png()).unwrap();
    }
}

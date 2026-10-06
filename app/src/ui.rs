//! Paneele über der 3D-Ansicht: links „Werkzeuge“ mit dem Knopf „Gebäude“,
//! rechts „Ansichten“ (3D, Grundriss, Schnitt und vier Ansichten).

use sk_model::RefSide;
use sk_paint::{Canvas, Rgba};
use sk_platform::{Event, MouseButton};
use sk_ui::theme::panel as col;
use sk_ui::widgets::{self, ButtonState, Fonts, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewKind {
    Persp,
    Plan,
    Section,
    Front,
    Back,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Building,
    Ref(RefSide),
    Ortho,
    View(ViewKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Tools,
    Views,
}

/// Abstand der Paneele vom Rand und Innenabstand (dip).
const MARGIN: f32 = 12.0;
const PAD: f32 = 14.0;
const WIDTH: f32 = 196.0;
/// Platz für den Schatten rund um ein Paneel (dip).
const SHADOW: f32 = 10.0;

/// Fenstergröße (dip), ab der die Paneele in voller Größe erscheinen.
const FULL_W: f32 = 1440.0;
const FULL_H: f32 = 810.0;
/// Kleinster Verkleinerungsfaktor, damit die Schrift lesbar bleibt.
const MIN_FIT: f32 = 0.6;

pub struct Ui {
    /// Wirksame Skalierung der Paneele: Bildschirmskalierung × Fensterfaktor.
    pub scale: f32,
    /// Bildschirmskalierung (dpi / 96).
    dpi: f32,
    pub fonts: Fonts,
    pub hover: Option<Id>,
    pressed: Option<Id>,
    pub view: ViewKind,
    pub building: bool,
    pub ref_side: RefSide,
    pub ortho: bool,
    /// Schichten der Außenwand: Farbfeld und Text (aus der Bibliothek).
    pub wall_layers: Vec<(Rgba, String)>,
}

/// Ergebnis eines Ereignisses.
#[derive(Default)]
pub struct UiOut {
    /// Diese Paneele neu zeichnen.
    pub repaint: bool,
    /// Angeklickter Knopf.
    pub clicked: Option<Id>,
    /// Die Maus steht über einem Paneel: das Ereignis gehört der Oberfläche.
    pub consumed: bool,
}

/// Zeilen des Werkzeug-Paneels (für Zeichnen und Treffertest gleich).
enum Row {
    Title(&'static str),
    Button(Id, &'static str),
    Label(&'static str),
    Layer(Rgba, String),
    Segments([(Id, &'static str); 3]),
    Pair([(Id, &'static str); 2]),
    Separator,
    Hint(&'static str),
}

fn tool_rows(layers: &[(Rgba, String)]) -> Vec<Row> {
    let mut rows = vec![
        Row::Title("Werkzeuge"),
        Row::Button(Id::Building, "Gebäude"),
        Row::Label("Außenwand"),
    ];
    rows.extend(layers.iter().map(|(c, t)| Row::Layer(*c, t.clone())));
    rows.extend([
        Row::Label("Bezugsseite"),
        Row::Segments([
            (Id::Ref(RefSide::Left), "Außen"),
            (Id::Ref(RefSide::Center), "Achse"),
            (Id::Ref(RefSide::Right), "Innen"),
        ]),
        Row::Button(Id::Ortho, "90°-Sprung"),
        Row::Separator,
        Row::Hint("Klick setzt Punkte, Klick auf"),
        Row::Hint("den Startpunkt schließt"),
        Row::Hint("Tab: Bezugsseite wechseln"),
        Row::Hint("R: 90°-Sprung"),
        Row::Hint("Esc: Eingabe beenden"),
        Row::Hint("Violettes Band ziehen:"),
        Row::Hint("Wand verschieben"),
    ]);
    rows
}

fn view_rows() -> Vec<Row> {
    vec![
        Row::Title("Ansichten"),
        Row::Button(Id::View(ViewKind::Persp), "3D"),
        Row::Button(Id::View(ViewKind::Plan), "Grundriss"),
        Row::Button(Id::View(ViewKind::Section), "Schnitt"),
        Row::Separator,
        Row::Pair([
            (Id::View(ViewKind::Front), "Vorne"),
            (Id::View(ViewKind::Back), "Hinten"),
        ]),
        Row::Pair([
            (Id::View(ViewKind::Left), "Links"),
            (Id::View(ViewKind::Right), "Rechts"),
        ]),
    ]
}

/// Höhe einer Zeile in dip und Abstand danach.
fn row_height(r: &Row) -> (f32, f32) {
    match r {
        Row::Title(_) => (22.0, 12.0),
        Row::Button(..) | Row::Segments(_) | Row::Pair(_) => (34.0, 8.0),
        Row::Label(_) => (18.0, 6.0),
        Row::Layer(..) => (18.0, 4.0),
        Row::Separator => (1.0, 10.0),
        Row::Hint(_) => (17.0, 0.0),
    }
}

impl Ui {
    pub fn new(scale: f32) -> Ui {
        Ui {
            scale,
            dpi: scale,
            fonts: Fonts::system(),
            hover: None,
            pressed: None,
            view: ViewKind::Persp,
            building: false,
            ref_side: RefSide::Left,
            ortho: true,
            wall_layers: Vec::new(),
        }
    }

    /// Passt die Paneelgröße an Fenster (Pixel) und Bildschirmskalierung an: in
    /// kleineren Fenstern schrumpfen Paneele und Knöpfe mit. `true`, wenn sich die
    /// Größe geändert hat.
    pub fn fit(&mut self, dpi: f32, win_w: u32, win_h: u32) -> bool {
        let (w, h) = (win_w as f32 / dpi, win_h as f32 / dpi);
        let f = (w / FULL_W).min(h / FULL_H).clamp(MIN_FIT, 1.0);
        // In Schritten von 1/40, damit nicht jedes Pixel beim Ziehen neu zeichnet
        let f = (f * 40.0).round() / 40.0;
        let scale = dpi * f;
        let changed = scale != self.scale;
        (self.dpi, self.scale) = (dpi, scale);
        changed
    }

    fn rows(&self, p: Panel) -> Vec<Row> {
        match p {
            Panel::Tools => tool_rows(&self.wall_layers),
            Panel::Views => view_rows(),
        }
    }

    fn panel_height(&self, p: Panel) -> f32 {
        let inner: f32 = self
            .rows(p)
            .iter()
            .map(|r| {
                let (h, g) = row_height(r);
                h + g
            })
            .sum();
        ((inner + 2.0 * PAD) * self.scale).round()
    }

    /// Lage eines Paneels im Fenster (ohne Schatten).
    pub fn rect(&self, p: Panel, win_w: u32, top: u32) -> Rect {
        let s = self.scale;
        let w = (WIDTH * s).round();
        let m = (MARGIN * s).round();
        let x = match p {
            Panel::Tools => m,
            Panel::Views => win_w as f32 - m - w,
        };
        Rect::new(x, top as f32 + m, w, self.panel_height(p))
    }

    /// Knöpfe eines Paneels in Paneelkoordinaten.
    fn buttons(&self, p: Panel) -> Vec<(Id, Rect, &'static str)> {
        let s = self.scale;
        let inner_w = (WIDTH - 2.0 * PAD) * s;
        let mut y = PAD * s;
        let x = PAD * s;
        let mut out = Vec::new();
        for r in self.rows(p) {
            let (h, g) = row_height(&r);
            let (h, g) = (h * s, g * s);
            match r {
                Row::Button(id, label) => out.push((id, Rect::new(x, y, inner_w, h), label)),
                Row::Segments(items) => {
                    let gap = 4.0 * s;
                    let bw = (inner_w - 2.0 * gap) / 3.0;
                    for (i, (id, label)) in items.into_iter().enumerate() {
                        out.push((id, Rect::new(x + i as f32 * (bw + gap), y, bw, h), label));
                    }
                }
                Row::Pair(items) => {
                    let gap = 6.0 * s;
                    let bw = (inner_w - gap) / 2.0;
                    for (i, (id, label)) in items.into_iter().enumerate() {
                        out.push((id, Rect::new(x + i as f32 * (bw + gap), y, bw, h), label));
                    }
                }
                _ => {}
            }
            y += h + g;
        }
        out
    }

    fn is_on(&self, id: Id) -> bool {
        match id {
            Id::Building => self.building,
            Id::Ref(r) => self.ref_side == r,
            Id::Ortho => self.ortho,
            Id::View(v) => self.view == v,
        }
    }

    /// Paneel und Knopf unter der Maus (Fensterkoordinaten).
    fn hit(&self, x: f64, y: f64, win_w: u32, top: u32) -> Option<(Panel, Option<Id>)> {
        for p in [Panel::Tools, Panel::Views] {
            let r = self.rect(p, win_w, top);
            if r.contains(x, y) {
                let (lx, ly) = (x - r.x as f64, y - r.y as f64);
                let id = self
                    .buttons(p)
                    .into_iter()
                    .find(|(_, b, _)| b.contains(lx, ly))
                    .map(|b| b.0);
                return Some((p, id));
            }
        }
        None
    }

    /// Verarbeitet Mausereignisse in Fensterkoordinaten.
    pub fn handle(&mut self, e: &Event, win_w: u32, top: u32) -> UiOut {
        let mut out = UiOut::default();
        match *e {
            Event::MouseMove { x, y, .. } => {
                let hit = self.hit(x, y, win_w, top);
                let hover = hit.and_then(|h| h.1);
                out.repaint = hover != self.hover;
                self.hover = hover;
                out.consumed = hit.is_some();
            }
            Event::MouseLeave => {
                out.repaint = self.hover.take().is_some();
            }
            Event::MouseDown { button, x, y, .. } => {
                if let Some((_, id)) = self.hit(x, y, win_w, top) {
                    out.consumed = true;
                    if button == MouseButton::Left {
                        self.pressed = id;
                        out.repaint = id.is_some();
                    }
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Some(p) = self.pressed.take() {
                    out.repaint = true;
                    out.consumed = true;
                    if self.hit(x, y, win_w, top).and_then(|h| h.1) == Some(p) {
                        out.clicked = Some(p);
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// Lage (links oben) des Paneelbildes samt Schatten im Fenster, wie bei
    /// [`Ui::paint`].
    pub fn origin(&self, p: Panel, win_w: u32, top: u32) -> (i32, i32) {
        let r = self.rect(p, win_w, top);
        let m = (SHADOW * self.scale).round();
        ((r.x - m) as i32, (r.y - m) as i32)
    }

    /// Zeichnet ein Paneel. Liefert das Bild und seine Lage (links oben) im Fenster.
    pub fn paint(&self, p: Panel, win_w: u32, top: u32) -> (Canvas, i32, i32) {
        let s = self.scale;
        let r = self.rect(p, win_w, top);
        let m = (SHADOW * s).round();
        let mut c = Canvas::new((r.w + 2.0 * m) as usize, (r.h + 2.0 * m) as usize);
        widgets::panel(&mut c, Rect::new(m, m, r.w, r.h), s);

        let (regular, bold) = (self.fonts.regular.as_ref(), self.fonts.bold.as_ref());
        let x = m + PAD * s;
        let inner_w = (WIDTH - 2.0 * PAD) * s;
        let mut y = m + PAD * s;
        for row in self.rows(p) {
            let (h, g) = row_height(&row);
            let (h, g) = (h * s, g * s);
            match row {
                Row::Title(t) => widgets::text(
                    &mut c,
                    bold.or(regular),
                    t,
                    17.0 * s,
                    x,
                    y + 16.0 * s,
                    col::TEXT,
                ),
                Row::Label(t) => {
                    widgets::text(&mut c, regular, t, 14.0 * s, x, y + 14.0 * s, col::TEXT)
                }
                Row::Layer(color, t) => {
                    let sw = 12.0 * s;
                    c.fill_rect(x, y + 3.0 * s, sw, sw, color);
                    let tx = x + sw + 8.0 * s;
                    widgets::text(
                        &mut c,
                        regular,
                        &t,
                        13.0 * s,
                        tx,
                        y + 13.5 * s,
                        col::TEXT_DIM,
                    );
                }
                Row::Separator => widgets::separator(&mut c, x, y, inner_w, s),
                Row::Hint(t) => {
                    widgets::text(&mut c, regular, t, 13.0 * s, x, y + 13.0 * s, col::TEXT_DIM)
                }
                _ => {}
            }
            y += h + g;
        }
        for (id, b, label) in self.buttons(p) {
            let st = ButtonState {
                hover: self.hover == Some(id),
                pressed: self.pressed == Some(id) && self.hover == Some(id),
                active: self.is_on(id),
            };
            let b = Rect::new(b.x + m, b.y + m, b.w, b.h);
            widgets::button(&mut c, &self.fonts, b, label, st, s);
        }
        let (x, y) = self.origin(p, win_w, top);
        (c, x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_platform::Modifiers;

    fn click(ui: &mut Ui, x: f64, y: f64) -> Option<Id> {
        let m = Modifiers::default();
        ui.handle(&Event::MouseMove { x, y, mods: m }, 1280, 32);
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: m,
        };
        ui.handle(&down, 1280, 32);
        let up = Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods: m,
        };
        ui.handle(&up, 1280, 32).clicked
    }

    #[test]
    fn knoepfe_werden_getroffen() {
        let mut ui = Ui::new(1.0);
        let r = ui.rect(Panel::Tools, 1280, 32);
        let (id, b, _) = ui.buttons(Panel::Tools)[0];
        assert_eq!(id, Id::Building);
        let hit = click(&mut ui, (r.x + b.x + 5.0) as f64, (r.y + b.y + 5.0) as f64);
        assert_eq!(hit, Some(Id::Building));

        let r = ui.rect(Panel::Views, 1280, 32);
        let grundriss = ui
            .buttons(Panel::Views)
            .into_iter()
            .find(|b| b.0 == Id::View(ViewKind::Plan))
            .unwrap()
            .1;
        let hit = click(
            &mut ui,
            (r.x + grundriss.x + 20.0) as f64,
            (r.y + grundriss.y + 10.0) as f64,
        );
        assert_eq!(hit, Some(Id::View(ViewKind::Plan)));
    }

    #[test]
    fn neben_den_paneelen_gehoert_die_maus_der_3d_ansicht() {
        let mut ui = Ui::new(1.0);
        let out = ui.handle(
            &Event::MouseMove {
                x: 640.0,
                y: 400.0,
                mods: Modifiers::default(),
            },
            1280,
            32,
        );
        assert!(!out.consumed);
    }
}

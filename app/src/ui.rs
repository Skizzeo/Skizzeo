//! Paneele über der 3D-Ansicht: links „Werkzeuge“ mit dem Knopf „Gebäude“,
//! rechts „Ansichten“ (3D, Grundriss, Schnitt und vier Ansichten) und darunter,
//! solange ein Bauteil gewählt ist, „Eigenschaften“.

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
    Props,
}

/// Inhalt des Paneels „Eigenschaften“ (nur lesend).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Props {
    /// Zeilen aus Bezeichnung und Wert, z. B. („Länge“, „10,00 m“).
    pub values: Vec<(&'static str, String)>,
    /// Name des Aufbaus.
    pub layer_set: String,
    /// Je Schicht: Farbfeld, „14 cm Dämmung (WDVS)“ und „3,796 m³ · 76 kg“.
    pub layers: Vec<(Rgba, String, String)>,
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
    /// Eigenschaften des gewählten Bauteils; ohne Auswahl kein Paneel.
    pub props: Option<Props>,
    /// Zuletzt gezeichnete Paneelbilder (Tools, Views, Props) für das
    /// Neuzeichnen einzelner Knöpfe.
    images: [Option<PanelImage>; 3],
}

/// Paneelbild ohne Knöpfe und mit Knöpfen, in Paneelkoordinaten.
struct PanelImage {
    scale: f32,
    base: Canvas,
    cur: Canvas,
}

/// Neu gezeichneter Ausschnitt eines Paneelbildes.
pub struct Patch {
    pub panel: Panel,
    /// Links oben im Paneelbild.
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    /// Vormultipliziertes RGBA8.
    pub px: Vec<u8>,
}

fn panel_index(p: Panel) -> usize {
    match p {
        Panel::Tools => 0,
        Panel::Views => 1,
        Panel::Props => 2,
    }
}

/// Ergebnis eines Ereignisses.
#[derive(Default)]
pub struct UiOut {
    /// Knöpfe, deren Aussehen sich geändert hat (Hover, Drücken).
    pub changed: Vec<Id>,
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
    /// Bezeichnung links, Wert rechtsbündig.
    Value(&'static str, String),
    /// Blasser Text, eingerückt wie der Text einer Schichtzeile.
    Detail(String),
    /// Blasser Text über die ganze Breite.
    Text(String),
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

fn props_rows(p: &Props) -> Vec<Row> {
    let mut rows = vec![Row::Title("Eigenschaften")];
    rows.extend(p.values.iter().map(|(k, v)| Row::Value(k, v.clone())));
    rows.extend([
        Row::Separator,
        Row::Label("Aufbau"),
        Row::Text(p.layer_set.clone()),
    ]);
    for (c, name, amount) in &p.layers {
        rows.push(Row::Layer(*c, name.clone()));
        if !amount.is_empty() {
            rows.push(Row::Detail(amount.clone()));
        }
    }
    rows
}

/// Höhe einer Zeile in dip und Abstand danach.
fn row_height(r: &Row) -> (f32, f32) {
    match r {
        Row::Title(_) => (22.0, 12.0),
        Row::Button(..) | Row::Segments(_) | Row::Pair(_) => (34.0, 8.0),
        Row::Label(_) => (18.0, 6.0),
        Row::Layer(..) => (18.0, 4.0),
        Row::Value(..) => (18.0, 4.0),
        Row::Detail(_) => (17.0, 6.0),
        Row::Text(_) => (17.0, 6.0),
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
            props: None,
            images: [None, None, None],
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
            Panel::Props => self.props.as_ref().map_or(Vec::new(), props_rows),
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
            Panel::Views | Panel::Props => win_w as f32 - m - w,
        };
        let y = match p {
            // Unter „Ansichten“
            Panel::Props => top as f32 + 2.0 * m + self.panel_height(Panel::Views),
            _ => top as f32 + m,
        };
        Rect::new(x, y, w, self.panel_height(p))
    }

    /// Sichtbare Paneele.
    fn panels(&self) -> Vec<Panel> {
        let mut v = vec![Panel::Tools, Panel::Views];
        if self.props.is_some() {
            v.push(Panel::Props);
        }
        v
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
        for p in self.panels() {
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
                if hover != self.hover {
                    out.changed.extend(self.hover.into_iter().chain(hover));
                }
                self.hover = hover;
                out.consumed = hit.is_some();
            }
            Event::MouseLeave => {
                out.changed.extend(self.hover.take());
            }
            Event::MouseDown { button, x, y, .. } => {
                if let Some((_, id)) = self.hit(x, y, win_w, top) {
                    out.consumed = true;
                    if button == MouseButton::Left {
                        self.pressed = id;
                        out.changed.extend(id);
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
                    out.changed.push(p);
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
    pub fn paint(&mut self, p: Panel, win_w: u32, top: u32) -> (&Canvas, i32, i32) {
        let base = self.paint_base(p, win_w, top);
        let mut cur = base.clone();
        for (id, b, label) in self.buttons(p) {
            self.paint_button(&mut cur, id, b, label);
        }
        let (x, y) = self.origin(p, win_w, top);
        let img = self.images[panel_index(p)].insert(PanelImage {
            scale: self.scale,
            base,
            cur,
        });
        (&img.cur, x, y)
    }

    /// Zeichnet einen Knopf auf dem zuletzt gezeichneten Paneelbild neu,
    /// pixelgleich zum vollen Neuzeichnen. `None`, wenn es kein passendes Bild
    /// gibt; dann muss das ganze Paneel neu gezeichnet werden.
    pub fn repaint_button(&mut self, id: Id) -> Option<Patch> {
        let (panel, b, label) = self.panels().into_iter().find_map(|p| {
            self.buttons(p)
                .into_iter()
                .find(|b| b.0 == id)
                .map(|(_, b, label)| (p, b, label))
        })?;
        let mut img = self.images[panel_index(panel)].take()?;
        if img.scale != self.scale {
            return None;
        }
        // Ausschnitt samt geglätteter Kanten; Knöpfe liegen mindestens
        // 2,4 Pixel auseinander, der Rand von 1 Pixel trifft keinen Nachbarn.
        let m = (SHADOW * self.scale).round();
        let x0 = ((b.x + m).floor() as usize).saturating_sub(1);
        let y0 = ((b.y + m).floor() as usize).saturating_sub(1);
        let x1 = (b.x + m + b.w).ceil() as usize + 1;
        let y1 = (b.y + m + b.h).ceil() as usize + 1;
        img.cur.copy_region(&img.base, x0, y0, x1 - x0, y1 - y0);
        self.paint_button(&mut img.cur, id, b, label);
        let (x, y, w, h, px) = img.cur.region_premul_rgba8(x0, y0, x1 - x0, y1 - y0);
        self.images[panel_index(panel)] = Some(img);
        Some(Patch {
            panel,
            x,
            y,
            w,
            h,
            px,
        })
    }

    fn paint_button(&self, c: &mut Canvas, id: Id, b: Rect, label: &str) {
        let s = self.scale;
        let m = (SHADOW * s).round();
        let st = ButtonState {
            hover: self.hover == Some(id),
            pressed: self.pressed == Some(id) && self.hover == Some(id),
            active: self.is_on(id),
        };
        let b = Rect::new(b.x + m, b.y + m, b.w, b.h);
        widgets::button(c, &self.fonts, b, label, st, s);
    }

    /// Paneel ohne Knöpfe.
    fn paint_base(&self, p: Panel, win_w: u32, top: u32) -> Canvas {
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
                Row::Value(k, v) => {
                    let px = 13.0 * s;
                    let base = y + 13.5 * s;
                    widgets::text(&mut c, regular, k, px, x, base, col::TEXT_DIM);
                    let vw = regular.map_or(0.0, |f| f.width(&v, px));
                    widgets::text(&mut c, regular, &v, px, x + inner_w - vw, base, col::TEXT);
                }
                Row::Detail(t) => {
                    let tx = x + 20.0 * s;
                    widgets::text(
                        &mut c,
                        regular,
                        &t,
                        12.5 * s,
                        tx,
                        y + 13.0 * s,
                        col::TEXT_DIM,
                    )
                }
                Row::Text(t) => widgets::text(
                    &mut c,
                    regular,
                    &t,
                    13.0 * s,
                    x,
                    y + 13.0 * s,
                    col::TEXT_DIM,
                ),
                Row::Separator => widgets::separator(&mut c, x, y, inner_w, s),
                Row::Hint(t) => {
                    widgets::text(&mut c, regular, t, 13.0 * s, x, y + 13.0 * s, col::TEXT_DIM)
                }
                _ => {}
            }
            y += h + g;
        }
        c
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

    /// Hover und Drücken zeichnen nur den Knopf neu; das Ergebnis gleicht dem
    /// vollen Neuzeichnen aufs Pixel.
    #[test]
    fn knopf_einzeln_neu_gleicht_dem_ganzen_paneel() {
        for scale in [0.6f32, 0.875, 1.25, 2.0] {
            let mut ui = Ui::new(scale);
            ui.wall_layers = vec![(Rgba::rgb(240, 190, 60), "14 cm Dämmung".into())];
            ui.building = true;
            for p in [Panel::Tools, Panel::Views] {
                let ids: Vec<Id> = ui.buttons(p).into_iter().map(|b| b.0).collect();
                for id in ids {
                    for (hover, pressed) in [(Some(id), None), (Some(id), Some(id)), (None, None)] {
                        let mut img = ui.paint(p, 1280, 32).0.to_premul_rgba8();
                        let w = ui.images[panel_index(p)].as_ref().unwrap().cur.width;
                        (ui.hover, ui.pressed) = (hover, pressed);
                        let patch = ui.repaint_button(id).expect("Paneelbild vorhanden");
                        assert_eq!(patch.panel, p);
                        for row in 0..patch.h {
                            let dst = ((patch.y + row) * w + patch.x) * 4;
                            img[dst..dst + patch.w * 4]
                                .copy_from_slice(&patch.px[row * patch.w * 4..][..patch.w * 4]);
                        }
                        let full = ui.paint(p, 1280, 32).0.to_premul_rgba8();
                        assert!(img == full, "{scale} {id:?} {hover:?} {pressed:?}");
                        (ui.hover, ui.pressed) = (None, None);
                    }
                }
            }
        }
    }

    #[test]
    fn hover_meldet_alten_und_neuen_knopf() {
        let mut ui = Ui::new(1.0);
        let m = Modifiers::default();
        let r = ui.rect(Panel::Views, 1280, 32);
        let bs = ui.buttons(Panel::Views);
        let at = |b: &(Id, Rect, &str)| ((r.x + b.1.x + 5.0) as f64, (r.y + b.1.y + 5.0) as f64);
        let (x, y) = at(&bs[0]);
        let out = ui.handle(&Event::MouseMove { x, y, mods: m }, 1280, 32);
        assert_eq!(out.changed, vec![bs[0].0]);
        let (x, y) = at(&bs[1]);
        let out = ui.handle(&Event::MouseMove { x, y, mods: m }, 1280, 32);
        assert_eq!(out.changed, vec![bs[0].0, bs[1].0]);
        let out = ui.handle(&Event::MouseMove { x, y, mods: m }, 1280, 32);
        assert!(out.changed.is_empty());
        let out = ui.handle(&Event::MouseLeave, 1280, 32);
        assert_eq!(out.changed, vec![bs[1].0]);
    }

    /// `cargo test --release -p skizzeo knopf_zeit -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn knopf_zeit() {
        for scale in [1.0f32, 1.5, 2.0] {
            let mut ui = Ui::new(scale);
            let n = 50;
            let t = std::time::Instant::now();
            for _ in 0..n {
                std::hint::black_box(ui.paint(Panel::Views, 1920, 32).0.to_premul_rgba8());
            }
            let full = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
            let id = Id::View(ViewKind::Front);
            let t = std::time::Instant::now();
            for i in 0..n {
                ui.hover = (i % 2 == 0).then_some(id);
                std::hint::black_box(ui.repaint_button(id).unwrap());
            }
            let part = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
            println!("Skalierung {scale}: Paneel {full:.3} ms, ein Knopf {part:.3} ms");
        }
    }
}

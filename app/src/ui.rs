//! Paneele über der 3D-Ansicht: links „Werkzeuge“ mit dem Knopf „Gebäude“ und
//! darunter „Geschosse“ (E14), rechts „Ansichten“ (3D, Grundriss, Schnitt und vier Ansichten) und darunter,
//! solange ein Bauteil gewählt ist, „Eigenschaften“.

use sk_model::{RefSide, StoreyId};
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_ui::theme::{Sizes, Theme};
use sk_ui::widgets::{self, ButtonState, FieldState, Fonts, Rect};

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

impl ViewKind {
    pub const ALL: [ViewKind; 7] = [
        ViewKind::Persp,
        ViewKind::Plan,
        ViewKind::Section,
        ViewKind::Front,
        ViewKind::Back,
        ViewKind::Left,
        ViewKind::Right,
    ];

    /// Name für die Befehlszeile (`--ansicht grundriss`).
    pub fn arg(self) -> &'static str {
        match self {
            ViewKind::Persp => "3d",
            ViewKind::Plan => "grundriss",
            ViewKind::Section => "schnitt",
            ViewKind::Front => "vorne",
            ViewKind::Back => "hinten",
            ViewKind::Left => "links",
            ViewKind::Right => "rechts",
        }
    }

    pub fn from_arg(s: &str) -> Option<ViewKind> {
        let s = s.to_lowercase();
        ViewKind::ALL.into_iter().find(|v| v.arg() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Building,
    Interior,
    Ref(RefSide),
    Ortho,
    View(ViewKind),
    Field(Field),
    /// Griff einer Ebene im Paneel „Geschosse“.
    Grip(Grip),
}

/// Ziehbare Ebene im Paneel „Geschosse“. ±0,00 liegt fest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grip {
    /// Unterkante der Gründung.
    FoundationBottom,
    /// Oberkante eines Geschosses.
    Top(StoreyId),
}

/// Zahlenfelder im Paneel „Eigenschaften“.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    SlabThickness,
    Recess,
    FootingWidth,
    FootingDepth,
    FloorThickness,
    /// Paneel „Geschosse“ (in m): Kote der Gründungsunterkante, Kote der
    /// Oberkante eines Geschosses, Geschosshöhe (bei der Gründung die
    /// Gründungstiefe) und lichte Höhe.
    LevelBottom,
    LevelTop(StoreyId),
    StoreyHeight(StoreyId),
    ClearHeight(StoreyId),
}

impl Field {
    /// Zahl im Paneel „Geschosse“, in Metern.
    pub fn is_level(self) -> bool {
        matches!(
            self,
            Field::LevelBottom
                | Field::LevelTop(_)
                | Field::StoreyHeight(_)
                | Field::ClearHeight(_)
        )
    }

    fn unit(self) -> &'static str {
        if self.is_level() {
            "m"
        } else {
            "cm"
        }
    }

    /// Kote (mit Vorzeichen) statt Länge.
    fn is_kote(self) -> bool {
        matches!(self, Field::LevelBottom | Field::LevelTop(_))
    }

    /// Wert, wie er im Feld steht (ohne Einheit).
    fn text(self, mm: f64) -> String {
        if !self.is_level() {
            cm_text(mm)
        } else if mm.round() < 0.0 {
            format!("-{}", m_text(mm))
        } else {
            m_text(mm)
        }
    }

    /// Wert mit Einheit für Hinweise.
    fn show(self, mm: f64) -> String {
        if !self.is_level() {
            format!("{} cm", cm_text(mm))
        } else if self.is_kote() {
            format!("{} m", kote_text(mm))
        } else {
            format!("{} m", m_text(mm))
        }
    }
}

/// Ein Zahlenfeld: Wert und erlaubter Bereich in mm, angezeigt in cm.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldRow {
    pub field: Field,
    pub label: &'static str,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    /// 0 ist zusätzlich erlaubt (Sockelrücksprung: bündig).
    pub zero: bool,
}

impl FieldRow {
    /// Prüft eine Eingabe in cm (Paneel „Geschosse“: in m); `Ok` mit dem Wert
    /// in mm (auf 1 mm gerundet).
    pub fn parse(&self, text: &str) -> Result<f64, String> {
        let t = text.trim().replace(',', ".");
        if t.is_empty() {
            return Err("Zahl fehlt".into());
        }
        let n: f64 = t.parse().map_err(|_| "keine Zahl".to_string())?;
        if !n.is_finite() {
            return Err("keine Zahl".into());
        }
        let per = if self.field.is_level() { 1000.0 } else { 10.0 };
        let mm = (n * per).round() + 0.0;
        if self.zero && mm == 0.0 {
            return Ok(0.0);
        }
        let f = self.field;
        if mm < self.min - 1e-6 {
            return Err(if self.zero {
                format!("0 oder mindestens {}", f.show(self.min))
            } else {
                format!("mindestens {}", f.show(self.min))
            });
        }
        if mm > self.max + 1e-6 {
            return Err(format!("höchstens {}", f.show(self.max)));
        }
        Ok(mm)
    }
}

/// mm als Zentimeter mit höchstens einer Nachkommastelle, Dezimalkomma.
pub fn cm_text(mm: f64) -> String {
    let t = (mm.round() / 10.0).to_string();
    t.replace('.', ",")
}

/// mm als Meter ohne Vorzeichen: zwei Nachkommastellen, eine dritte nur,
/// wenn sie nötig ist (2,855).
pub fn m_text(mm: f64) -> String {
    let mm = mm.round().abs();
    let d = if mm % 10.0 == 0.0 { 2 } else { 3 };
    format!("{:.*}", d, mm / 1000.0).replace('.', ",")
}

/// Höhenkote wie in der Bauzeichnung: ±0,00, +2,855, −0,80.
pub fn kote_text(mm: f64) -> String {
    let r = mm.round();
    if r == 0.0 {
        "±0,00".into()
    } else if r > 0.0 {
        format!("+{}", m_text(r))
    } else {
        format!("\u{2212}{}", m_text(r))
    }
}

/// Laufende Eingabe in einem Zahlenfeld. Text nur aus ASCII-Zeichen.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub field: Field,
    pub text: String,
    caret: usize,
    anchor: usize,
    /// Grund, warum die Eingabe nicht gilt (unter dem Feld).
    pub error: Option<String>,
}

impl Edit {
    fn new(row: &FieldRow) -> Edit {
        let text = row.field.text(row.value);
        Edit {
            field: row.field,
            caret: text.len(),
            anchor: 0,
            text,
            error: None,
        }
    }

    fn selection(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    fn replace_selection(&mut self, with: &str) {
        let (a, z) = self.selection();
        self.text.replace_range(a..z, with);
        self.caret = a + with.len();
        self.anchor = self.caret;
    }

    fn move_to(&mut self, i: usize, extend: bool) {
        self.caret = i.min(self.text.len());
        if !extend {
            self.anchor = self.caret;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Tools,
    Views,
    Props,
    Levels,
}

/// Ein Geschossband im Paneel „Geschosse“.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub id: StoreyId,
    /// Anzeigename: „Gründung“, „EG“, „OG“.
    pub name: String,
    /// Unter- und Oberkante (mm).
    pub bottom: f64,
    pub top: f64,
    /// Gründungsband: Unterkante ziehbar, Oberkante (±0,00) fest.
    pub foundation: bool,
    pub active: bool,
}

/// Inhalt des Paneels „Geschosse“.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Levels {
    /// Bänder von unten nach oben.
    pub bands: Vec<Band>,
    /// Lichte Höhe (mm) eines Geschosses (heute des EG).
    pub clear: Option<(StoreyId, f64)>,
    /// Zahlen, die sich eingeben lassen, mit erlaubtem Bereich.
    pub fields: Vec<FieldRow>,
}

/// Eine Ebene (Linie) im Diagramm.
struct LevelLine {
    z: f64,
    name: String,
    /// Kote als Zahl änderbar bzw. Ebene ziehbar; ±0,00 weder noch.
    field: Option<Field>,
    grip: Option<Grip>,
    active: bool,
}

/// Linien von unten nach oben: Unterkante jedes Bandes und Oberkante des obersten.
fn level_lines(l: &Levels) -> Vec<LevelLine> {
    let mut out = Vec::new();
    for (i, b) in l.bands.iter().enumerate() {
        let below = i.checked_sub(1).map(|j| &l.bands[j]);
        let (field, grip) = match below {
            _ if b.foundation => (Some(Field::LevelBottom), Some(Grip::FoundationBottom)),
            Some(u) if !u.foundation => (Some(Field::LevelTop(u.id)), Some(Grip::Top(u.id))),
            _ => (None, None),
        };
        out.push(LevelLine {
            z: b.bottom,
            name: b.name.clone(),
            field,
            grip,
            active: b.active,
        });
    }
    if let Some(b) = l.bands.last() {
        out.push(LevelLine {
            z: b.top,
            name: format!("OK {}", b.name),
            field: Some(Field::LevelTop(b.id)),
            grip: Some(Grip::Top(b.id)),
            active: false,
        });
    }
    out
}

/// Aufteilung des Paneels „Geschosse“, beim Ziehen eingefroren.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LevelsLayout {
    /// Maßstab (dip je m); ohne Platz eine Liste statt des Diagramms.
    ppm: f32,
    list: bool,
    /// Lage von ±0,00 im Paneel und Paneelhöhe (Pixel).
    anchor: f32,
    height: f32,
}

/// Ziehen einer Ebene im Paneel „Geschosse“.
#[derive(Clone, Copy, Debug)]
struct LevelDrag {
    grip: Grip,
    /// Maus und Höhe beim Greifen.
    y0: f64,
    z0: f64,
    layout: LevelsLayout,
}

/// Was das Paneel „Geschosse“ beim Ziehen meldet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LevelEvent {
    Begin(Grip),
    /// Neue Höhe (mm, gefangen) der gezogenen Ebene.
    Move(Grip, f64),
    End,
}

/// Abstände im Diagramm (dip): Titel, Luft über der obersten und unter der
/// untersten Linie, Zeilenhöhe der Liste.
const LEVEL_HEAD: f32 = 34.0;
const LEVEL_TOP: f32 = 20.0;
const LEVEL_BOTTOM: f32 = 8.0;
const LEVEL_LIST_ROW: f32 = 22.0;
/// Rechter Rand: Kette der Geschosshöhen und davor die der lichten Höhe.
const CHAIN_OUTER: f32 = 2.0;
const CHAIN_GAP: f32 = 40.0;

/// Inhalt des Paneels „Eigenschaften“.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Props {
    /// Zeilen aus Bezeichnung und Wert, z. B. („Länge“, „10,00 m“).
    pub values: Vec<(&'static str, String)>,
    /// Name des Aufbaus.
    pub layer_set: String,
    /// Je Schicht: Farbfeld, „14 cm Dämmung (WDVS)“ und „3,796 m³ · 76 kg“.
    pub layers: Vec<(Rgba, String, String)>,
    /// Überschrift über `layer_set`; leer heißt „Aufbau“.
    pub set_label: &'static str,
    /// Zahlenfelder (Parameter des Bauteils), zwischen Werten und Aufbau.
    pub fields: Vec<FieldRow>,
    /// Hinweise (Warnungen der Prüfung).
    pub notes: Vec<String>,
}

/// Breite eines Zahlenfelds (dip).
const FIELD_W: f32 = 60.0;

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
    /// Das Werkzeug zeichnet Innenwände (sonst Außenwände).
    pub interior: bool,
    pub ref_side: RefSide,
    pub ortho: bool,
    /// Schichten der Wand, die das Werkzeug zeichnet: Farbfeld und Text (aus der Bibliothek).
    pub wall_layers: Vec<(Rgba, String)>,
    /// Eigenschaften des gewählten Bauteils; ohne Auswahl kein Paneel.
    props: Option<Props>,
    /// Eingabe in einem Zahlenfeld.
    pub edit: Option<Edit>,
    /// Maße aus dem Farbschema (für Lage und Treffertest) und dessen Stand.
    size: Sizes,
    theme_rev: u64,
    /// Zuletzt gezeichnete Paneelbilder (Tools, Views, Props, Levels) für das
    /// Neuzeichnen einzelner Knöpfe.
    images: [Option<PanelImage>; 4],
    /// Paneel „Geschosse“: Inhalt und laufendes Ziehen.
    levels: Levels,
    level_drag: Option<LevelDrag>,
    /// Fensterhöhe und Höhe der Titelleiste (Pixel), für die Höhe des
    /// Paneels „Geschosse“.
    win_h: f32,
    pub top: u32,
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
        Panel::Levels => 3,
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
    /// Gültige Eingabe in einem Zahlenfeld (Wert in mm).
    pub submit: Option<(Field, f64)>,
    /// Die Höhe eines Paneels hat sich geändert (Hinweis unter einem Feld).
    pub relayout: bool,
    /// Ziehen einer Ebene im Paneel „Geschosse“.
    pub level: Option<LevelEvent>,
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
    /// Bezeichnung links, Zahlenfeld rechts.
    Field(Field, &'static str),
    /// Grund einer ungültigen Eingabe, in `field_invalid`.
    Error(String),
    Separator,
    Hint(&'static str),
}

fn tool_rows(interior: bool, layers: &[(Rgba, String)]) -> Vec<Row> {
    let mut rows = vec![
        Row::Title("Werkzeuge"),
        Row::Button(Id::Building, "Gebäude"),
        Row::Button(Id::Interior, "Innenwand"),
        Row::Label(if interior { "Innenwand" } else { "Außenwand" }),
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

fn props_rows(p: &Props, edit: Option<&Edit>) -> Vec<Row> {
    let mut rows = vec![Row::Title("Eigenschaften")];
    rows.extend(p.values.iter().map(|(k, v)| Row::Value(k, v.clone())));
    if !p.fields.is_empty() {
        rows.push(Row::Separator);
    }
    for f in &p.fields {
        rows.push(Row::Field(f.field, f.label));
        if let Some(e) = edit.filter(|e| e.field == f.field) {
            rows.extend(e.error.clone().map(Row::Error));
        }
    }
    let label = if p.set_label.is_empty() {
        "Aufbau"
    } else {
        p.set_label
    };
    rows.extend([
        Row::Separator,
        Row::Label(label),
        Row::Text(p.layer_set.clone()),
    ]);
    for (c, name, amount) in &p.layers {
        rows.push(Row::Layer(*c, name.clone()));
        if !amount.is_empty() {
            rows.push(Row::Detail(amount.clone()));
        }
    }
    if !p.notes.is_empty() {
        rows.push(Row::Separator);
        rows.extend(p.notes.iter().map(|n| Row::Text(n.clone())));
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
        Row::Field(..) => (26.0, 6.0),
        Row::Error(_) => (15.0, 6.0),
        Row::Detail(_) => (17.0, 6.0),
        Row::Text(_) => (17.0, 6.0),
        Row::Separator => (1.0, 10.0),
        Row::Hint(_) => (17.0, 0.0),
    }
}

impl Ui {
    pub fn new(scale: f32, theme: &Theme) -> Ui {
        Ui {
            size: theme.size,
            theme_rev: theme.rev,
            scale,
            dpi: scale,
            fonts: Fonts::system(),
            hover: None,
            pressed: None,
            view: ViewKind::Persp,
            building: false,
            interior: false,
            ref_side: RefSide::Left,
            ortho: true,
            wall_layers: Vec::new(),
            props: None,
            edit: None,
            images: [None, None, None, None],
            levels: Levels::default(),
            level_drag: None,
            win_h: 1e6,
            top: 32,
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
        let changed = scale != self.scale || win_h as f32 != self.win_h;
        (self.dpi, self.scale, self.win_h) = (dpi, scale, win_h as f32);
        changed
    }

    fn rows(&self, p: Panel) -> Vec<Row> {
        match p {
            Panel::Tools => tool_rows(self.interior, &self.wall_layers),
            Panel::Views => view_rows(),
            Panel::Props => self
                .props
                .as_ref()
                .map_or(Vec::new(), |p| props_rows(p, self.edit.as_ref())),
            Panel::Levels => vec![Row::Title("Geschosse")],
        }
    }

    pub fn props(&self) -> Option<&Props> {
        self.props.as_ref()
    }

    /// Neuer Inhalt des Paneels „Eigenschaften“. Eine Eingabe in einem Feld,
    /// das es nicht mehr gibt, verfällt.
    pub fn set_props(&mut self, p: Option<Props>) {
        let keep = self.edit.as_ref().is_some_and(|e| {
            p.as_ref()
                .is_some_and(|p| p.fields.iter().any(|f| f.field == e.field))
        });
        if !keep {
            self.edit = None;
        }
        self.props = p;
    }

    fn field_row(&self, f: Field) -> Option<&FieldRow> {
        if f.is_level() {
            return self.levels.fields.iter().find(|r| r.field == f);
        }
        self.props.as_ref()?.fields.iter().find(|r| r.field == f)
    }

    fn field_panel(f: Field) -> Panel {
        if f.is_level() {
            Panel::Levels
        } else {
            Panel::Props
        }
    }

    /// Feld in Paneelkoordinaten.
    fn field_rect(&self, f: Field) -> Option<Rect> {
        self.buttons(Ui::field_panel(f))
            .into_iter()
            .find(|b| b.0 == Id::Field(f))
            .map(|b| b.1)
    }

    /// Neuer Inhalt des Paneels „Geschosse“; `true`, wenn er sich geändert
    /// hat. Eine Eingabe in einer Zahl, die es nicht mehr gibt, verfällt.
    pub fn set_levels(&mut self, l: Levels) -> bool {
        if l == self.levels {
            return false;
        }
        if self
            .edit
            .as_ref()
            .is_some_and(|e| e.field.is_level() && !l.fields.iter().any(|f| f.field == e.field))
        {
            self.edit = None;
        }
        self.levels = l;
        true
    }

    /// Eine Ebene wird gerade gezogen.
    pub fn level_dragging(&self) -> Option<Grip> {
        self.level_drag.map(|d| d.grip)
    }

    /// Bricht das Ziehen einer Ebene ab (Esc); `true`, wenn eines lief.
    pub fn cancel_level_drag(&mut self) -> bool {
        self.level_drag.take().is_some()
    }

    /// Höhe (mm) einer Ebene.
    fn grip_z(&self, g: Grip) -> Option<f64> {
        let b = &self.levels.bands;
        match g {
            Grip::FoundationBottom => b.iter().find(|b| b.foundation).map(|b| b.bottom),
            Grip::Top(id) => b.iter().find(|b| b.id == id).map(|b| b.top),
        }
    }

    /// Beginnt die Eingabe in einem Feld (alles markiert).
    fn begin_edit(&mut self, f: Field, out: &mut UiOut) {
        if let Some(row) = self.field_row(f) {
            self.edit = Some(Edit::new(row));
            out.changed.push(Id::Field(f));
            // Das Feld ist breiter als die Maßzahl
            out.relayout |= f.is_level();
        }
    }

    /// Beendet die Eingabe: gültig wird übernommen, sonst bleibt der alte Wert.
    fn finish_edit(&mut self, out: &mut UiOut) {
        if let Some(e) = self.edit.take() {
            if let Some(Ok(mm)) = self.field_row(e.field).map(|r| r.parse(&e.text)) {
                out.submit = Some((e.field, mm));
            }
            out.changed.push(Id::Field(e.field));
            out.relayout |= e.error.is_some() || e.field.is_level();
        }
    }

    /// Tastendruck. `None`, wenn kein Feld in Eingabe ist; dann gehört die
    /// Taste der übrigen App. Sonst nimmt das Feld jede Taste.
    pub fn key(&mut self, key: Key, down: bool, mods: Modifiers) -> Option<UiOut> {
        let mut out = UiOut {
            consumed: true,
            ..UiOut::default()
        };
        let e = self.edit.as_mut()?;
        if !down {
            return Some(out);
        }
        let field = e.field;
        let had_error = e.error.is_some();
        let end = e.text.len();
        match key {
            Key::Enter | Key::Tab => {
                let row = self.field_row(field).cloned();
                let e = self.edit.as_mut()?;
                match row.map(|r| r.parse(&e.text)) {
                    Some(Ok(mm)) => {
                        self.edit = None;
                        out.submit = Some((field, mm));
                        out.relayout = had_error || field.is_level();
                    }
                    Some(Err(why)) => {
                        out.relayout = e.error.as_ref() != Some(&why);
                        e.error = Some(why);
                    }
                    None => self.edit = None,
                }
            }
            Key::Escape => {
                self.edit = None;
                out.relayout = had_error || field.is_level();
            }
            Key::Backspace | Key::Delete => {
                let (a, z) = e.selection();
                if a == z {
                    let r = if key == Key::Backspace {
                        a.saturating_sub(1)..a
                    } else {
                        a..(a + 1).min(end)
                    };
                    e.anchor = r.start;
                    e.caret = r.end;
                }
                e.replace_selection("");
            }
            Key::Left | Key::Right if !mods.shift && e.caret != e.anchor => {
                let (a, z) = e.selection();
                e.move_to(if key == Key::Left { a } else { z }, false);
            }
            Key::Left => e.move_to(e.caret.saturating_sub(1), mods.shift),
            Key::Right => e.move_to(e.caret + 1, mods.shift),
            Key::Home => e.move_to(0, mods.shift),
            Key::End => e.move_to(end, mods.shift),
            Key::Char('A') if mods.ctrl => {
                e.anchor = 0;
                e.caret = end;
            }
            Key::Char(c @ ('0'..='9' | ',' | '.' | '-'))
                if !mods.ctrl
                    && !mods.alt
                    && e.text.len() - (e.selection().1 - e.selection().0) < 12 =>
            {
                e.replace_selection(&c.to_string());
            }
            _ => {}
        }
        out.changed.push(Id::Field(field));
        Some(out)
    }

    /// Schreibmarke an der Stelle `x` (Paneelkoordinaten) im Feld.
    fn caret_at(&self, f: Field, x: f64) -> Option<usize> {
        let (e, r) = (self.edit.as_ref()?, self.field_rect(f)?);
        let font = self.fonts.regular.as_ref()?;
        let s = self.scale;
        let px = self.size.font_small * s;
        let unit_x = r.x + r.w - self.size.field_pad * s - font.width(f.unit(), px);
        let num_x = unit_x - 4.0 * s - font.width(&e.text, px);
        let x = x as f32 - num_x;
        (0..=e.text.len()).min_by(|&a, &b| {
            let da = (font.width(&e.text[..a], px) - x).abs();
            let db = (font.width(&e.text[..b], px) - x).abs();
            da.total_cmp(&db)
        })
    }

    fn panel_height(&self, p: Panel) -> f32 {
        if p == Panel::Levels {
            return self.levels_layout().height;
        }
        let inner: f32 = self
            .rows(p)
            .iter()
            .map(|r| {
                let (h, g) = row_height(r);
                h + g
            })
            .sum();
        ((inner + 2.0 * self.size.panel_pad) * self.scale).round()
    }

    /// Lage eines Paneels im Fenster (ohne Schatten).
    pub fn rect(&self, p: Panel, win_w: u32, top: u32) -> Rect {
        let s = self.scale;
        let w = (self.size.panel_width * s).round();
        let m = (self.size.panel_margin * s).round();
        let x = match p {
            Panel::Tools | Panel::Levels => m,
            Panel::Views | Panel::Props => win_w as f32 - m - w,
        };
        let y = match p {
            // Unter „Ansichten“ bzw. „Werkzeuge“
            Panel::Props => top as f32 + 2.0 * m + self.panel_height(Panel::Views),
            Panel::Levels => top as f32 + 2.0 * m + self.panel_height(Panel::Tools),
            _ => top as f32 + m,
        };
        Rect::new(x, y, w, self.panel_height(p))
    }

    /// Sichtbare Paneele.
    fn panels(&self) -> Vec<Panel> {
        let mut v = vec![Panel::Tools, Panel::Levels, Panel::Views];
        if self.props.is_some() {
            v.push(Panel::Props);
        }
        v
    }

    /// Knöpfe eines Paneels in Paneelkoordinaten.
    fn buttons(&self, p: Panel) -> Vec<(Id, Rect, &'static str)> {
        if p == Panel::Levels {
            return self.level_buttons();
        }
        let s = self.scale;
        let pad = self.size.panel_pad;
        let inner_w = (self.size.panel_width - 2.0 * pad) * s;
        let mut y = pad * s;
        let x = pad * s;
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
                Row::Field(f, _) => {
                    let fw = FIELD_W * s;
                    out.push((Id::Field(f), Rect::new(x + inner_w - fw, y, fw, h), ""));
                }
                _ => {}
            }
            y += h + g;
        }
        out
    }

    fn is_on(&self, id: Id) -> bool {
        match id {
            Id::Building => self.building && !self.interior,
            Id::Interior => self.building && self.interior,
            Id::Ref(r) => self.ref_side == r,
            Id::Ortho => self.ortho,
            Id::View(v) => self.view == v,
            Id::Field(_) => false,
            Id::Grip(_) => false,
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
                    .map(|b| b.0)
                    .or_else(|| match p {
                        Panel::Levels => self.grip_at(lx, ly).map(Id::Grip),
                        _ => None,
                    });
                return Some((p, id));
            }
        }
        None
    }

    /// Verarbeitet Mausereignisse in Fensterkoordinaten.
    pub fn handle(&mut self, e: &Event, win_w: u32, top: u32) -> UiOut {
        let mut out = UiOut::default();
        if let Some(d) = self.level_drag {
            return self.handle_level_drag(e, d);
        }
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
                let hit = self.hit(x, y, win_w, top);
                let field = match hit {
                    Some((_, Some(Id::Field(f)))) if button == MouseButton::Left => Some(f),
                    _ => None,
                };
                // Klick neben das Feld in Eingabe beendet sie
                if self.edit.as_ref().is_some_and(|e| Some(e.field) != field) {
                    self.finish_edit(&mut out);
                }
                if let Some(f) = field {
                    out.consumed = true;
                    if self.edit.is_none() {
                        self.begin_edit(f, &mut out);
                    } else {
                        let r = self.rect(Ui::field_panel(f), win_w, top);
                        let lx = x - r.x as f64;
                        if let Some(i) = self.caret_at(f, lx) {
                            if let Some(e) = self.edit.as_mut() {
                                e.move_to(i, false);
                            }
                        }
                        out.changed.push(Id::Field(f));
                    }
                } else if let (Some((_, Some(Id::Grip(g)))), MouseButton::Left) = (hit, button) {
                    out.consumed = true;
                    if let Some(z0) = self.grip_z(g) {
                        self.level_drag = Some(LevelDrag {
                            grip: g,
                            y0: y,
                            z0,
                            layout: self.levels_layout(),
                        });
                        out.level = Some(LevelEvent::Begin(g));
                        out.changed.push(Id::Grip(g));
                    }
                } else if let Some((_, id)) = hit {
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
        let m = (self.size.panel_shadow * self.scale).round();
        ((r.x - m) as i32, (r.y - m) as i32)
    }

    /// Zeichnet ein Paneel. Liefert das Bild und seine Lage (links oben) im Fenster.
    pub fn paint(&mut self, t: &Theme, p: Panel, win_w: u32, top: u32) -> (&Canvas, i32, i32) {
        self.use_theme(t);
        let base = self.paint_base(t, p, win_w, top);
        let mut cur = base.clone();
        for (id, b, label) in self.buttons(p) {
            self.paint_button(t, &mut cur, id, b, label);
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
    pub fn repaint_button(&mut self, t: &Theme, id: Id) -> Option<Patch> {
        if t.rev != self.theme_rev {
            return None;
        }
        if matches!(id, Id::Grip(_)) || matches!(id, Id::Field(f) if f.is_level()) {
            return self.repaint_levels(t);
        }
        let (panel, b, label) = self.panels().into_iter().find_map(|p| {
            self.buttons(p)
                .into_iter()
                .find(|b| b.0 == id)
                .map(|(_, b, label)| (p, b, label))
        })?;
        let slot = &mut self.images[panel_index(panel)];
        if slot.as_ref()?.scale != self.scale {
            return None;
        }
        let mut img = slot.take()?;
        // Ausschnitt samt geglätteter Kanten; Knöpfe liegen mindestens
        // 2,4 Pixel auseinander, der Rand von 1 Pixel trifft keinen Nachbarn.
        let m = (self.size.panel_shadow * self.scale).round();
        let x0 = ((b.x + m).floor() as usize).saturating_sub(1);
        let y0 = ((b.y + m).floor() as usize).saturating_sub(1);
        let x1 = (b.x + m + b.w).ceil() as usize + 1;
        let y1 = (b.y + m + b.h).ceil() as usize + 1;
        img.cur.copy_region(&img.base, x0, y0, x1 - x0, y1 - y0);
        self.paint_button(t, &mut img.cur, id, b, label);
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

    /// Übernimmt Maße und Stand des Farbschemas; ein neuer Stand verwirft die
    /// Paneelbilder.
    pub fn use_theme(&mut self, t: &Theme) {
        if t.rev != self.theme_rev {
            self.theme_rev = t.rev;
            self.size = t.size;
            self.images = [None, None, None, None];
        }
    }

    fn paint_button(&self, t: &Theme, c: &mut Canvas, id: Id, b: Rect, label: &str) {
        let s = self.scale;
        let m = (self.size.panel_shadow * s).round();
        if let Id::Grip(_) = id {
            return;
        }
        if let Id::Field(f) = id {
            let b = Rect::new(b.x + m, b.y + m, b.w, b.h);
            let edit = self.edit.as_ref().filter(|e| e.field == f);
            if f.is_level() && edit.is_none() {
                self.paint_dim_text(t, c, f, b);
                return;
            }
            let value = self.field_row(f).map_or(String::new(), |r| f.text(r.value));
            let st = FieldState {
                text: edit.map_or(&value, |e| &e.text),
                unit: f.unit(),
                hover: self.hover == Some(id),
                focus: edit.is_some(),
                invalid: edit.is_some_and(|e| e.error.is_some()),
                caret: edit.map(|e| e.caret),
                select: edit.map(|e| e.selection()),
            };
            widgets::field(c, &self.fonts, b, &st, s, t);
            return;
        }
        let st = ButtonState {
            hover: self.hover == Some(id),
            pressed: self.pressed == Some(id) && self.hover == Some(id),
            active: self.is_on(id),
        };
        let b = Rect::new(b.x + m, b.y + m, b.w, b.h);
        widgets::button(c, &self.fonts, b, label, st, s, t);
    }

    /// Paneel ohne Knöpfe.
    fn paint_base(&self, t: &Theme, p: Panel, win_w: u32, top: u32) -> Canvas {
        let (col, size) = (&t.ui, &t.size);
        let s = self.scale;
        let r = self.rect(p, win_w, top);
        let m = (size.panel_shadow * s).round();
        let mut c = Canvas::new((r.w + 2.0 * m) as usize, (r.h + 2.0 * m) as usize);
        widgets::panel(&mut c, Rect::new(m, m, r.w, r.h), s, t);
        if p == Panel::Levels {
            self.paint_levels(t, &mut c, r.h);
        }

        let (regular, bold) = (self.fonts.regular.as_ref(), self.fonts.bold.as_ref());
        let x = m + size.panel_pad * s;
        let inner_w = (size.panel_width - 2.0 * size.panel_pad) * s;
        let mut y = m + size.panel_pad * s;
        for row in self.rows(p) {
            let (h, g) = row_height(&row);
            let (h, g) = (h * s, g * s);
            match row {
                Row::Title(t) => widgets::text(
                    &mut c,
                    bold.or(regular),
                    t,
                    size.font_title * s,
                    x,
                    y + 16.0 * s,
                    col.text,
                ),
                Row::Label(t) => {
                    widgets::text(&mut c, regular, t, size.font * s, x, y + 14.0 * s, col.text)
                }
                Row::Layer(color, t) => {
                    let sw = 12.0 * s;
                    c.fill_rect(x, y + 3.0 * s, sw, sw, color);
                    let tx = x + sw + 8.0 * s;
                    widgets::text(
                        &mut c,
                        regular,
                        &t,
                        size.font_small * s,
                        tx,
                        y + 13.5 * s,
                        col.text_dim,
                    );
                }
                Row::Field(_, k) => {
                    let px = size.font_small * s;
                    widgets::text(&mut c, regular, k, px, x, y + 17.5 * s, col.text_dim);
                }
                Row::Error(t) => widgets::text(
                    &mut c,
                    regular,
                    &t,
                    size.font_detail * s,
                    x,
                    y + 12.0 * s,
                    col.field_invalid,
                ),
                Row::Value(k, v) => {
                    let px = size.font_small * s;
                    let base = y + 13.5 * s;
                    widgets::text(&mut c, regular, k, px, x, base, col.text_dim);
                    let vw = regular.map_or(0.0, |f| f.width(&v, px));
                    widgets::text(&mut c, regular, &v, px, x + inner_w - vw, base, col.text);
                }
                Row::Detail(t) => {
                    let tx = x + 20.0 * s;
                    widgets::text(
                        &mut c,
                        regular,
                        &t,
                        size.font_detail * s,
                        tx,
                        y + 13.0 * s,
                        col.text_dim,
                    )
                }
                Row::Text(t) => widgets::text(
                    &mut c,
                    regular,
                    &t,
                    size.font_small * s,
                    x,
                    y + 13.0 * s,
                    col.text_dim,
                ),
                Row::Separator => widgets::separator(&mut c, x, y, inner_w, s, t),
                Row::Hint(t) => widgets::text(
                    &mut c,
                    regular,
                    t,
                    size.font_small * s,
                    x,
                    y + 13.0 * s,
                    col.text_dim,
                ),
                _ => {}
            }
            y += h + g;
        }
        c
    }
}

/// Paneel „Geschosse“ (E14): Diagramm der Ebenen mit Griffen, Koten und
/// Maßketten, im kleinen Fenster eine Liste.
impl Ui {
    /// Aufteilung: größter Maßstab, bei dem das Paneel über dem Fensterrand
    /// endet; sonst die Liste. Beim Ziehen bleibt sie stehen.
    fn levels_layout(&self) -> LevelsLayout {
        if let Some(d) = self.level_drag {
            return d.layout;
        }
        let (s, z) = (self.scale, &self.size);
        let m = (z.panel_margin * s).round();
        let y = self.top as f32 + 2.0 * m + self.panel_height(Panel::Tools);
        let avail = (self.win_h - y - m).max(0.0);
        let lines = level_lines(&self.levels);
        let head = z.panel_pad + LEVEL_HEAD + LEVEL_TOP;
        let mut ppm = z.level_px_per_m;
        loop {
            let (above, below) = self.level_spans(&lines, ppm);
            let h = ((head + above + below + LEVEL_BOTTOM + z.panel_pad) * s).round();
            if h <= avail {
                return LevelsLayout {
                    ppm,
                    list: false,
                    anchor: ((head + above) * s).round(),
                    height: h,
                };
            }
            if ppm <= z.level_px_per_m_min {
                break;
            }
            ppm = (ppm - 1.0).max(z.level_px_per_m_min);
        }
        let rows = self.levels.bands.len() + usize::from(self.levels.clear.is_some());
        let h = (2.0 * z.panel_pad + LEVEL_HEAD + rows as f32 * LEVEL_LIST_ROW) * s;
        let least = ((2.0 * z.panel_pad + LEVEL_HEAD) * s).round();
        LevelsLayout {
            ppm: z.level_px_per_m_min,
            list: true,
            anchor: 0.0,
            height: h.round().min(avail).max(least),
        }
    }

    /// Abstand zweier Linien (dip), für die Anzeige auf `level_row_min` gespreizt.
    fn level_gap(&self, dz: f64, ppm: f32) -> f32 {
        (dz as f32 / 1000.0 * ppm).max(self.size.level_row_min)
    }

    /// Strecke von ±0,00 bis zur obersten und zur untersten Linie (dip).
    fn level_spans(&self, lines: &[LevelLine], ppm: f32) -> (f32, f32) {
        let a = anchor_index(lines);
        let gap = |w: &[LevelLine]| self.level_gap(w[1].z - w[0].z, ppm);
        let above = lines[a.min(lines.len())..].windows(2).map(gap).sum();
        let below = lines[..(a + 1).min(lines.len())].windows(2).map(gap).sum();
        (above, below)
    }

    /// Lage der Linien im Paneel (Pixel, ohne Schatten), von unten nach oben.
    fn line_ys(&self, lines: &[LevelLine], l: &LevelsLayout) -> Vec<f32> {
        let s = self.scale;
        let mut ys = vec![l.anchor; lines.len()];
        let a = anchor_index(lines);
        for i in a + 1..lines.len() {
            ys[i] = ys[i - 1] - self.level_gap(lines[i].z - lines[i - 1].z, l.ppm) * s;
        }
        for i in (0..a.min(lines.len())).rev() {
            ys[i] = ys[i + 1] + self.level_gap(lines[i + 1].z - lines[i].z, l.ppm) * s;
        }
        ys
    }

    /// Spalten (Pixel, ohne Schatten): Innenkante links, Kette der lichten
    /// Höhe, Kette der Geschosshöhen.
    fn level_columns(&self) -> (f32, f32, f32) {
        let (s, z) = (self.scale, &self.size);
        let x0 = z.panel_pad * s;
        let xo = (z.panel_width - z.panel_pad - CHAIN_OUTER) * s;
        (x0, xo - CHAIN_GAP * s, xo)
    }

    /// Text einer Zahl im Paneel aus dem Stand der Geschosse.
    fn level_text(&self, f: Field) -> String {
        let b = &self.levels.bands;
        let band = |id: StoreyId| b.iter().find(|b| b.id == id);
        match f {
            Field::LevelBottom => b
                .iter()
                .find(|b| b.foundation)
                .map_or(String::new(), |b| kote_text(b.bottom)),
            Field::LevelTop(id) => band(id).map_or(String::new(), |b| kote_text(b.top)),
            Field::StoreyHeight(id) => band(id).map_or(String::new(), |b| m_text(b.top - b.bottom)),
            Field::ClearHeight(_) => self.levels.clear.map_or(String::new(), |c| m_text(c.1)),
            _ => String::new(),
        }
    }

    /// Fläche einer Maßzahl oder Kote: rechtsbündig an `right`, Grundlinie
    /// `base`. In Eingabe ein Zahlenfeld an derselben Stelle.
    fn level_text_rect(&self, f: Field, right: f32, base: f32) -> Rect {
        let s = self.scale;
        let px = self.level_px(f);
        let font = self.fonts.regular.as_ref();
        let text = self.level_text(f);
        let w = font.map_or(0.0, |ft| ft.width(&text, px));
        let cap = font.map_or(px * 0.7, |ft| ft.cap_height(px));
        let r = Rect::new(
            right - w - 2.0 * s,
            base - cap - 3.0 * s,
            w + 4.0 * s,
            cap + 6.0 * s,
        );
        if self.edit.as_ref().is_none_or(|e| e.field != f) {
            return r;
        }
        let (fw, fh) = (FIELD_W * s, self.size.field_height * s);
        let x = (r.x + r.w + 2.0 * s - fw).max(self.size.panel_pad * s * 0.5);
        Rect::new(x, r.y + r.h * 0.5 - fh * 0.5, fw, fh)
    }

    /// Schriftgröße: Koten wie Namen, Maßzahlen etwas kleiner.
    fn level_px(&self, f: Field) -> f32 {
        let z = &self.size;
        if f.is_kote() {
            z.font_small * self.scale
        } else {
            z.font_detail * self.scale
        }
    }

    /// Grundlinie eines Textes, der mittig zwischen `a` und `b` steht.
    fn mid_base(&self, a: f32, b: f32, px: f32) -> f32 {
        let cap = self
            .fonts
            .regular
            .as_ref()
            .map_or(px * 0.7, |f| f.cap_height(px));
        (a + b) * 0.5 + cap * 0.5
    }

    /// Klickbare Zahlen des Paneels „Geschosse“ in Paneelkoordinaten.
    fn level_buttons(&self) -> Vec<(Id, Rect, &'static str)> {
        let s = self.scale;
        let l = self.levels_layout();
        let lines = level_lines(&self.levels);
        let (x0, xi, xo) = self.level_columns();
        let bands = &self.levels.bands;
        let mut out = Vec::new();
        let mut push = |f: Field, right: f32, base: f32| {
            out.push((Id::Field(f), self.level_text_rect(f, right, base), ""));
        };
        if l.list {
            let top = (self.size.panel_pad + LEVEL_HEAD) * s;
            let row = LEVEL_LIST_ROW * s;
            for (k, (i, b)) in bands.iter().enumerate().rev().enumerate() {
                let base = top + k as f32 * row + 15.0 * s;
                if let Some(f) = lines[i].field {
                    push(f, xi - 6.0 * s, base);
                }
                push(Field::StoreyHeight(b.id), xo, base);
            }
            if let Some((id, _)) = self.levels.clear {
                let base = top + bands.len() as f32 * row + 15.0 * s;
                push(Field::ClearHeight(id), xo, base);
            }
            let _ = x0;
            return out;
        }
        let ys = self.line_ys(&lines, &l);
        let clamp = |y: f32| y.clamp((self.size.panel_pad + LEVEL_HEAD) * s, l.height);
        for (line, &y) in lines.iter().zip(&ys) {
            if let Some(f) = line.field {
                push(f, xi - 6.0 * s, clamp(y) - 4.0 * s);
            }
        }
        for (i, b) in bands.iter().enumerate() {
            let f = Field::StoreyHeight(b.id);
            let base = self.mid_base(clamp(ys[i]), clamp(ys[i + 1]), self.level_px(f));
            push(f, xo - 5.0 * s, base);
        }
        if let Some((y0, yc, id)) = self.clear_span(&lines, &ys, &l) {
            let f = Field::ClearHeight(id);
            push(
                f,
                xi - 5.0 * s,
                self.mid_base(clamp(y0), clamp(yc), self.level_px(f)),
            );
        }
        out
    }

    /// Kette der lichten Höhe: Unterkante des Bandes, Unterkante der Decke.
    fn clear_span(
        &self,
        lines: &[LevelLine],
        ys: &[f32],
        l: &LevelsLayout,
    ) -> Option<(f32, f32, StoreyId)> {
        let (id, clear) = self.levels.clear?;
        let i = self.levels.bands.iter().position(|b| b.id == id)?;
        let y0 = *ys.get(i)?;
        // Decke unter der Oberkante: in der gespreizten Darstellung anteilig
        let (z0, z1, y1) = (lines[i].z, lines.get(i + 1)?.z, *ys.get(i + 1)?);
        let t = if z1 > z0 {
            (clear / (z1 - z0)) as f32
        } else {
            1.0
        };
        let _ = l;
        Some((y0, y0 + (y1 - y0) * t.clamp(0.0, 1.0), id))
    }

    /// Griff unter der Maus (Paneelkoordinaten), nur im Diagramm.
    fn grip_at(&self, x: f64, y: f64) -> Option<Grip> {
        let l = self.levels_layout();
        if l.list {
            return None;
        }
        let s = self.scale;
        let (x0, _, xo) = self.level_columns();
        if (x as f32) < x0 - 4.0 * s || (x as f32) > xo + 4.0 * s {
            return None;
        }
        let lines = level_lines(&self.levels);
        let ys = self.line_ys(&lines, &l);
        let hit = self.size.level_hit * s;
        lines
            .iter()
            .zip(ys)
            .filter_map(|(line, ly)| Some((line.grip?, (ly - y as f32).abs())))
            .filter(|(_, d)| *d <= hit)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(g, _)| g)
    }

    /// Ziehen einer Ebene: jede Bewegung meldet die gefangene Höhe (1 cm,
    /// mit Umschalt 5 cm), Loslassen beendet.
    fn handle_level_drag(&mut self, e: &Event, d: LevelDrag) -> UiOut {
        let mut out = UiOut {
            consumed: true,
            ..UiOut::default()
        };
        match *e {
            Event::MouseMove { y, mods, .. } => {
                let px_per_mm = (d.layout.ppm * self.scale) as f64 / 1000.0;
                let z = d.z0 + (d.y0 - y) / px_per_mm;
                let step = if mods.shift { 50.0 } else { 10.0 };
                out.level = Some(LevelEvent::Move(d.grip, (z / step).round() * step));
                out.changed.push(Id::Grip(d.grip));
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                self.level_drag = None;
                out.level = Some(LevelEvent::End);
                // Erst jetzt passt sich das Diagramm an
                out.relayout = true;
            }
            _ => {}
        }
        out
    }

    /// Zeichnet das ganze Paneel „Geschosse“ neu und liefert es als
    /// Ausschnitt; `None`, wenn sich seine Größe geändert hat.
    fn repaint_levels(&mut self, t: &Theme) -> Option<Patch> {
        let i = panel_index(Panel::Levels);
        let old = self.images[i].as_ref()?;
        if old.scale != self.scale {
            return None;
        }
        let (w, h) = (old.cur.width, old.cur.height);
        let base = self.paint_base(t, Panel::Levels, 0, self.top);
        if (base.width, base.height) != (w, h) {
            return None;
        }
        let mut cur = base.clone();
        for (id, b, label) in self.buttons(Panel::Levels) {
            self.paint_button(t, &mut cur, id, b, label);
        }
        let (x, y, w, h, px) = cur.region_premul_rgba8(0, 0, w, h);
        self.images[i] = Some(PanelImage {
            scale: self.scale,
            base,
            cur,
        });
        Some(Patch {
            panel: Panel::Levels,
            x,
            y,
            w,
            h,
            px,
        })
    }

    /// Maßzahl oder Kote (nicht in Eingabe), `b` im Bild.
    fn paint_dim_text(&self, t: &Theme, c: &mut Canvas, f: Field, b: Rect) {
        let s = self.scale;
        let color = if self.hover == Some(Id::Field(f)) {
            t.ui.dim_text_hover
        } else {
            t.ui.dim_text
        };
        let text = self.level_text(f);
        widgets::text(
            c,
            self.fonts.regular.as_ref(),
            &text,
            self.level_px(f),
            b.x + 2.0 * s,
            b.y + b.h - 3.0 * s,
            color,
        );
    }

    /// Linien, Griffe, Ketten und feste Texte (ohne die klickbaren Zahlen).
    fn paint_levels(&self, t: &Theme, c: &mut Canvas, panel_h: f32) {
        let (u, z, s) = (&t.ui, &self.size, self.scale);
        let m = (z.panel_shadow * s).round();
        let l = self.levels_layout();
        let lines = level_lines(&self.levels);
        let (x0, xi, xo) = self.level_columns();
        let (regular, bold) = (self.fonts.regular.as_ref(), self.fonts.bold.as_ref());
        let px = z.font_small * s;
        let px_d = z.font_detail * s;
        let w1 = s.round().max(1.0);
        let right_text = |c: &mut Canvas,
                          f: Option<&sk_paint::font::Font>,
                          txt: &str,
                          px: f32,
                          right: f32,
                          base: f32,
                          col: Rgba| {
            let w = f.map_or(0.0, |f| f.width(txt, px));
            widgets::text(c, f, txt, px, right - w + m, base + m, col);
        };
        if l.list {
            let top = (z.panel_pad + LEVEL_HEAD) * s;
            let row = LEVEL_LIST_ROW * s;
            for (k, (i, b)) in self.levels.bands.iter().enumerate().rev().enumerate() {
                let base = top + k as f32 * row + 15.0 * s;
                let font = if b.active { bold.or(regular) } else { regular };
                let col = if b.active {
                    u.level_line_active
                } else {
                    u.text
                };
                widgets::text(c, font, &b.name, px, x0 + m, base + m, col);
                if lines[i].field.is_none() {
                    right_text(
                        c,
                        regular,
                        &kote_text(lines[i].z),
                        px,
                        xi - 4.0 * s,
                        base,
                        u.dim_text,
                    );
                }
            }
            if let Some((id, _)) = self.levels.clear {
                let name = self
                    .levels
                    .bands
                    .iter()
                    .find(|b| b.id == id)
                    .map_or("", |b| b.name.as_str());
                let base = top + self.levels.bands.len() as f32 * row + 15.0 * s;
                let txt = format!("lichte Höhe {name}");
                widgets::text(c, regular, &txt, px_d, x0 + m, base + m, u.text_dim);
            }
            self.paint_level_error(t, c);
            return;
        }
        let ys = self.line_ys(&lines, &l);
        let (lo, hi) = (
            (z.panel_pad + LEVEL_HEAD) * s,
            panel_h - z.panel_pad * s * 0.5,
        );
        let clamp = |y: f32| y.clamp(lo, hi);
        // Kette der Geschosshöhen
        if let (Some(&a), Some(&b)) = (ys.first(), ys.last()) {
            c.fill_rect(
                xo + m - w1 * 0.5,
                clamp(b) + m,
                w1,
                clamp(a) - clamp(b),
                u.dim_line,
            );
        }
        let tick = |c: &mut Canvas, x: f32, y: f32| {
            let mut p = Path::new();
            let d = 3.5 * s;
            p.segment((x - d + m, y + d + m), (x + d + m, y - d + m), 1.2 * s);
            c.fill(&p, u.dim_line);
        };
        let drag = self.level_drag.map(|d| d.grip);
        for (line, &y) in lines.iter().zip(&ys) {
            let y = clamp(y);
            let dragged = line.grip.is_some() && line.grip == drag;
            let (col, th) = if dragged {
                (u.level_handle_drag, 2.0 * w1)
            } else if line.active {
                (u.level_line_active, 2.0 * w1)
            } else {
                (u.level_line, w1)
            };
            let yl = (y + m - th * 0.5).round();
            c.fill_rect(x0 + m, yl, xo + 4.0 * s - x0, th, col);
            tick(c, xo, y);
            let font = if line.active {
                bold.or(regular)
            } else {
                regular
            };
            let name_col = if line.active {
                u.level_line_active
            } else {
                u.text
            };
            let nx = x0 + (z.level_handle + 6.0) * s;
            widgets::text(c, font, &line.name, px, nx + m, y - 4.0 * s + m, name_col);
            if line.field.is_none() {
                right_text(
                    c,
                    regular,
                    &kote_text(line.z),
                    px,
                    xi - 4.0 * s,
                    y - 4.0 * s,
                    u.dim_text,
                );
            }
            if let Some(g) = line.grip {
                let col = if dragged {
                    u.level_handle_drag
                } else if self.hover == Some(Id::Grip(g)) {
                    u.level_handle_hover
                } else {
                    u.level_handle
                };
                let d = z.level_handle * s;
                let mut p = Path::new();
                p.rounded_rect(x0 + m, y + m - d * 0.5, d, d, d * 0.5);
                c.fill(&p, col);
            }
        }
        // Kette der lichten Höhe mit Unterkante der Decke
        if let Some((y0, yc, id)) = self.clear_span(&lines, &ys, &l) {
            let (y0, yc) = (clamp(y0), clamp(yc));
            c.fill_rect(xi + m - w1 * 0.5, yc + m, w1, y0 - yc, u.dim_line);
            c.fill_rect(
                xi - 10.0 * s + m,
                (yc + m - w1 * 0.5).round(),
                20.0 * s,
                w1,
                u.dim_line,
            );
            tick(c, xi, y0);
            tick(c, xi, yc);
            let f = Field::ClearHeight(id);
            let r = self.level_text_rect(f, xi - 5.0 * s, self.mid_base(y0, yc, px_d));
            let base = self.mid_base(y0, yc, px_d);
            right_text(c, regular, "lichte", px_d, r.x - 2.0 * s, base, u.text_dim);
        }
        self.paint_level_error(t, c);
    }

    /// Grund einer ungültigen Eingabe unter dem Feld.
    fn paint_level_error(&self, t: &Theme, c: &mut Canvas) {
        let Some((f, why)) = self
            .edit
            .as_ref()
            .filter(|e| e.field.is_level())
            .and_then(|e| Some((e.field, e.error.clone()?)))
        else {
            return;
        };
        let Some(r) = self.field_rect(f) else {
            return;
        };
        let (s, z) = (self.scale, &self.size);
        let m = (z.panel_shadow * s).round();
        let px = z.font_detail * s;
        let font = self.fonts.regular.as_ref();
        let w = font.map_or(0.0, |f| f.width(&why, px)) + 8.0 * s;
        let x = (r.x + r.w - w).max(z.panel_pad * s * 0.5);
        let y = r.y + r.h + 2.0 * s;
        let mut p = Path::new();
        p.rounded_rect(x + m, y + m, w, px + 6.0 * s, 3.0 * s);
        c.fill(&p, t.ui.bg);
        widgets::text(
            c,
            font,
            &why,
            px,
            x + 4.0 * s + m,
            y + px + m,
            t.ui.field_invalid,
        );
    }
}

/// Linie bei ±0,00 (fest): an ihr hängt das Diagramm.
fn anchor_index(lines: &[LevelLine]) -> usize {
    lines.iter().position(|l| l.field.is_none()).unwrap_or(0)
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
        let mut ui = Ui::new(1.0, &Theme::dark());
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
        let mut ui = Ui::new(1.0, &Theme::dark());
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
        let th = Theme::dark();
        for scale in [0.6f32, 0.875, 1.25, 2.0] {
            let mut ui = Ui::new(scale, &th);
            ui.wall_layers = vec![(Rgba::rgb(240, 190, 60), "14 cm Dämmung".into())];
            ui.building = true;
            for p in [Panel::Tools, Panel::Views] {
                let ids: Vec<Id> = ui.buttons(p).into_iter().map(|b| b.0).collect();
                for id in ids {
                    for (hover, pressed) in [(Some(id), None), (Some(id), Some(id)), (None, None)] {
                        let mut img = ui.paint(&th, p, 1280, 32).0.to_premul_rgba8();
                        let w = ui.images[panel_index(p)].as_ref().unwrap().cur.width;
                        (ui.hover, ui.pressed) = (hover, pressed);
                        let patch = ui.repaint_button(&th, id).expect("Paneelbild vorhanden");
                        assert_eq!(patch.panel, p);
                        for row in 0..patch.h {
                            let dst = ((patch.y + row) * w + patch.x) * 4;
                            img[dst..dst + patch.w * 4]
                                .copy_from_slice(&patch.px[row * patch.w * 4..][..patch.w * 4]);
                        }
                        let full = ui.paint(&th, p, 1280, 32).0.to_premul_rgba8();
                        assert!(img == full, "{scale} {id:?} {hover:?} {pressed:?}");
                        (ui.hover, ui.pressed) = (None, None);
                    }
                }
            }
        }
    }

    #[test]
    fn hover_meldet_alten_und_neuen_knopf() {
        let mut ui = Ui::new(1.0, &Theme::dark());
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
        let th = Theme::dark();
        for scale in [1.0f32, 1.5, 2.0] {
            let mut ui = Ui::new(scale, &th);
            let n = 50;
            let t = std::time::Instant::now();
            for _ in 0..n {
                std::hint::black_box(ui.paint(&th, Panel::Views, 1920, 32).0.to_premul_rgba8());
            }
            let full = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
            let id = Id::View(ViewKind::Front);
            let t = std::time::Instant::now();
            for i in 0..n {
                ui.hover = (i % 2 == 0).then_some(id);
                std::hint::black_box(ui.repaint_button(&th, id).unwrap());
            }
            let part = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
            println!("Skalierung {scale}: Paneel {full:.3} ms, ein Knopf {part:.3} ms");
        }
    }

    /// Ein neuer Stand des Farbschemas verwirft die Paneelbilder; das nächste
    /// Zeichnen nimmt den neuen Akzent.
    #[test]
    fn neuer_akzent_zeichnet_die_paneele_neu() {
        let mut th = Theme::dark();
        let mut ui = Ui::new(1.0, &th);
        let p = Panel::Views;
        // „3D“ ist eingeschaltet und damit in Akzentfarbe
        let (id, b, _) = ui.buttons(p)[0];
        assert!(ui.is_on(id));
        let m = th.size.panel_shadow;
        let at = |c: &Canvas| {
            let px = c.to_rgba8();
            let i = (((b.y + m + b.h * 0.5) as usize) * c.width + (b.x + m + 6.0) as usize) * 4;
            Rgba(px[i], px[i + 1], px[i + 2], px[i + 3])
        };
        assert_eq!(at(ui.paint(&th, p, 1280, 32).0), th.ui.accent);
        let blue = Rgba::rgb(40, 120, 220);
        th.set_accent(blue);
        assert!(ui.repaint_button(&th, id).is_none(), "altes Bild verworfen");
        assert_eq!(at(ui.paint(&th, p, 1280, 32).0), blue);
        assert!(ui.repaint_button(&th, id).is_some());
    }

    fn props_mit_feldern() -> Props {
        Props {
            values: vec![("Nummer", "SP-001".into())],
            fields: vec![
                FieldRow {
                    field: Field::SlabThickness,
                    label: "Dicke",
                    value: 200.0,
                    min: 100.0,
                    max: 1000.0,
                    zero: false,
                },
                FieldRow {
                    field: Field::Recess,
                    label: "Sockelrücksprung",
                    value: 0.0,
                    min: 20.0,
                    max: 500.0,
                    zero: true,
                },
            ],
            ..Default::default()
        }
    }

    fn feld_mitte(ui: &Ui, f: Field) -> (f64, f64) {
        let r = ui.rect(Panel::Props, 1280, 32);
        let b = ui.field_rect(f).unwrap();
        (
            (r.x + b.x + b.w * 0.5) as f64,
            (r.y + b.y + b.h * 0.5) as f64,
        )
    }

    fn taste(ui: &mut Ui, k: Key) -> UiOut {
        ui.key(k, true, Modifiers::default())
            .expect("Feld in Eingabe")
    }

    #[test]
    fn zahlenfeld_tippen_pruefen_abbrechen() {
        let mut ui = Ui::new(1.0, &Theme::dark());
        ui.set_props(Some(props_mit_feldern()));
        assert!(ui.key(Key::Char('1'), true, Modifiers::default()).is_none());
        let (x, y) = feld_mitte(&ui, Field::SlabThickness);
        click(&mut ui, x, y);
        // Alles markiert: Tippen ersetzt den Wert
        assert_eq!(ui.edit.as_ref().unwrap().text, "20");
        taste(&mut ui, Key::Char('3'));
        taste(&mut ui, Key::Char('5'));
        assert_eq!(ui.edit.as_ref().unwrap().text, "35");
        taste(&mut ui, Key::Char('A'));
        assert_eq!(
            ui.edit.as_ref().unwrap().text,
            "35",
            "Buchstaben zählen nicht"
        );
        // Schreibmarke: Pos1, Entf, Ende, Rücktaste
        taste(&mut ui, Key::Home);
        taste(&mut ui, Key::Delete);
        taste(&mut ui, Key::End);
        taste(&mut ui, Key::Char(','));
        taste(&mut ui, Key::Char('5'));
        assert_eq!(ui.edit.as_ref().unwrap().text, "5,5");
        let out = taste(&mut ui, Key::Enter);
        assert_eq!(out.submit, None, "unter 10 cm");
        assert!(out.relayout);
        assert_eq!(
            ui.edit.as_ref().unwrap().error.as_deref(),
            Some("mindestens 10 cm")
        );
        let h = ui.panel_height(Panel::Props);
        taste(&mut ui, Key::Backspace);
        taste(&mut ui, Key::Backspace);
        taste(&mut ui, Key::Backspace);
        taste(&mut ui, Key::Char('2'));
        taste(&mut ui, Key::Char('2'));
        let out = taste(&mut ui, Key::Enter);
        assert_eq!(out.submit, Some((Field::SlabThickness, 220.0)));
        assert!(out.relayout && ui.edit.is_none());
        assert!(ui.panel_height(Panel::Props) < h, "Hinweis weg");
        // Rücksprung: 0 erlaubt, 1 cm nicht, Esc verwirft
        let (x, y) = feld_mitte(&ui, Field::Recess);
        click(&mut ui, x, y);
        assert_eq!(ui.edit.as_ref().unwrap().text, "0");
        taste(&mut ui, Key::Char('1'));
        taste(&mut ui, Key::Enter);
        assert_eq!(
            ui.edit.as_ref().unwrap().error.as_deref(),
            Some("0 oder mindestens 2 cm")
        );
        let out = taste(&mut ui, Key::Escape);
        assert!(ui.edit.is_none() && out.submit.is_none());
        // Klick neben das Feld übernimmt eine gültige Eingabe
        click(&mut ui, x, y);
        taste(&mut ui, Key::Char('4'));
        let out = ui.handle(
            &Event::MouseDown {
                button: MouseButton::Left,
                x: 600.0,
                y: 400.0,
                mods: Modifiers::default(),
            },
            1280,
            32,
        );
        assert_eq!(out.submit, Some((Field::Recess, 40.0)));
        assert!(!out.consumed, "der Klick gehört weiter der 3D-Ansicht");
        // Felder, die es nicht mehr gibt, beenden die Eingabe
        click(&mut ui, x, y);
        ui.set_props(Some(Props::default()));
        assert!(ui.edit.is_none());
    }

    #[test]
    fn zahl_in_zentimetern() {
        let f = &props_mit_feldern().fields[1];
        assert_eq!(cm_text(200.0), "20");
        assert_eq!(cm_text(25.0), "2,5");
        assert_eq!(f.parse("2,5"), Ok(25.0));
        assert_eq!(f.parse(" 3.0 "), Ok(30.0));
        assert_eq!(f.parse("0"), Ok(0.0));
        assert_eq!(f.parse("-2").unwrap_err(), "0 oder mindestens 2 cm");
        assert_eq!(f.parse("51").unwrap_err(), "höchstens 50 cm");
        assert_eq!(f.parse("").unwrap_err(), "Zahl fehlt");
        assert_eq!(f.parse("2,,5").unwrap_err(), "keine Zahl");
    }

    /// Ein Feld unter der Maus oder in Eingabe zeichnet nur sich neu, gleich
    /// dem vollen Paneel.
    #[test]
    fn feld_einzeln_neu_gleicht_dem_ganzen_paneel() {
        let th = Theme::dark();
        for scale in [1.0f32, 1.5] {
            let mut ui = Ui::new(scale, &th);
            ui.set_props(Some(props_mit_feldern()));
            let id = Id::Field(Field::SlabThickness);
            for step in 0..3 {
                let mut img = ui.paint(&th, Panel::Props, 1280, 32).0.to_premul_rgba8();
                let w = ui.images[panel_index(Panel::Props)]
                    .as_ref()
                    .unwrap()
                    .cur
                    .width;
                match step {
                    0 => ui.hover = Some(id),
                    1 => ui.edit = Some(Edit::new(&props_mit_feldern().fields[0])),
                    _ => {
                        ui.key(Key::Left, true, Modifiers::default());
                    }
                }
                let patch = ui.repaint_button(&th, id).expect("Paneelbild vorhanden");
                for row in 0..patch.h {
                    let dst = ((patch.y + row) * w + patch.x) * 4;
                    img[dst..dst + patch.w * 4]
                        .copy_from_slice(&patch.px[row * patch.w * 4..][..patch.w * 4]);
                }
                let full = ui.paint(&th, Panel::Props, 1280, 32).0.to_premul_rgba8();
                assert!(img == full, "{scale} Schritt {step}");
            }
        }
    }
}

/// E14: Paneel „Geschosse“.
#[cfg(test)]
mod levels_tests {
    use super::*;
    use crate::scene::Scene;

    const M: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    fn ui_mit_geschossen() -> (Ui, Scene) {
        let s = Scene::new();
        let mut ui = Ui::new(1.0, &Theme::dark());
        ui.fit(1.0, 1440, 900);
        ui.set_levels(s.levels());
        (ui, s)
    }

    fn band(ui: &Ui, name: &str) -> Band {
        ui.levels
            .bands
            .iter()
            .find(|b| b.name == name)
            .cloned()
            .unwrap()
    }

    /// Fensterlage einer Linie (am Griff) für die Höhe `z`.
    fn griff(ui: &Ui, z: f64) -> (f64, f64) {
        let r = ui.rect(Panel::Levels, 1440, 32);
        let lines = level_lines(&ui.levels);
        let ys = ui.line_ys(&lines, &ui.levels_layout());
        let i = lines.iter().position(|l| l.z == z).unwrap();
        let (x0, _, _) = ui.level_columns();
        ((r.x + x0 + 5.0) as f64, (r.y + ys[i]) as f64)
    }

    #[test]
    fn drei_baender_und_vier_linien() {
        let (ui, _) = ui_mit_geschossen();
        let names: Vec<_> = ui.levels.bands.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["Gründung", "EG", "OG"]);
        let lines = level_lines(&ui.levels);
        let z: Vec<_> = lines.iter().map(|l| l.z).collect();
        assert_eq!(z, [-800.0, 0.0, 2855.0, 5710.0]);
        assert!(
            lines[1].grip.is_none() && lines[1].field.is_none(),
            "±0,00 fest"
        );
        assert!(lines[1].active, "EG aktiv");
        assert_eq!(kote_text(-800.0), "\u{2212}0,80");
        assert_eq!(kote_text(0.0), "±0,00");
        assert_eq!(kote_text(2855.0), "+2,855");
        assert_eq!(m_text(2635.0), "2,635");
        let l = ui.levels_layout();
        assert!(!l.list && l.ppm == 32.0, "{l:?}");
        // Linien von unten nach oben im Maßstab 32 px/m
        let ys = ui.line_ys(&lines, &l);
        assert!((ys[1] - ys[2] - 2.855 * 32.0).abs() < 1e-3);
        assert!((ys[0] - ys[1] - 0.8 * 32.0).abs() < 1e-3);
    }

    /// Test 2 und 4 aus E14: OG-Griff 30 px hoch, auf 1 cm gefangen; Esc
    /// meldet die App über `cancel_level_drag`.
    #[test]
    fn griff_ziehen_meldet_gefangene_hoehe() {
        let (mut ui, _) = ui_mit_geschossen();
        let og = band(&ui, "OG");
        let (x, y) = griff(&ui, og.top);
        let out = ui.handle(&Event::MouseMove { x, y, mods: M }, 1440, 32);
        assert_eq!(ui.hover, Some(Id::Grip(Grip::Top(og.id))));
        assert!(out.consumed);
        let h = ui.rect(Panel::Levels, 1440, 32).h;
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        };
        let out = ui.handle(&down, 1440, 32);
        assert_eq!(out.level, Some(LevelEvent::Begin(Grip::Top(og.id))));
        let out = ui.handle(
            &Event::MouseMove {
                x,
                y: y - 30.0,
                mods: M,
            },
            1440,
            32,
        );
        assert_eq!(
            out.level,
            Some(LevelEvent::Move(Grip::Top(og.id), 5710.0 + 940.0))
        );
        // Das Paneel bleibt beim Ziehen stehen, auch wenn die Werte wachsen
        let mut l = ui.levels.clone();
        l.bands[2].top += 940.0;
        ui.set_levels(l);
        assert_eq!(ui.rect(Panel::Levels, 1440, 32).h, h);
        let shift = Modifiers { shift: true, ..M };
        let out = ui.handle(
            &Event::MouseMove {
                x,
                y: y - 30.0,
                mods: shift,
            },
            1440,
            32,
        );
        assert_eq!(
            out.level,
            Some(LevelEvent::Move(Grip::Top(og.id), 5710.0 + 940.0))
        );
        let up = Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        };
        let out = ui.handle(&up, 1440, 32);
        assert_eq!(out.level, Some(LevelEvent::End));
        assert!(out.relayout && ui.level_dragging().is_none());
        // ±0,00 hat keinen Griff
        let (x, y) = griff(&ui, 0.0);
        let out = ui.handle(&down_at(x, y), 1440, 32);
        assert_eq!(out.level, None);
        assert!(!ui.cancel_level_drag());
    }

    fn down_at(x: f64, y: f64) -> Event {
        Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        }
    }

    fn klick_auf(ui: &mut Ui, f: Field) {
        let r = ui.rect(Panel::Levels, 1440, 32);
        let b = ui.field_rect(f).unwrap();
        let (x, y) = (
            (r.x + b.x + b.w * 0.5) as f64,
            (r.y + b.y + b.h * 0.5) as f64,
        );
        ui.handle(&Event::MouseMove { x, y, mods: M }, 1440, 32);
        ui.handle(&down_at(x, y), 1440, 32);
    }

    fn tippe(ui: &mut Ui, text: &str) -> UiOut {
        for c in text.chars() {
            ui.key(Key::Char(c), true, M);
        }
        ui.key(Key::Enter, true, M).unwrap()
    }

    /// Test 3 aus E14: Klick auf die lichte Höhe, „2,60“ und Enter; „0“ wird
    /// mit Grund abgelehnt.
    #[test]
    fn masszahl_als_zahl_in_metern() {
        let (mut ui, _) = ui_mit_geschossen();
        let eg = band(&ui, "EG").id;
        let f = Field::ClearHeight(eg);
        klick_auf(&mut ui, f);
        let e = ui.edit.as_ref().expect("Eingabe offen");
        assert_eq!((e.field, e.text.as_str()), (f, "2,635"));
        let out = tippe(&mut ui, "0");
        assert_eq!(out.submit, None);
        assert_eq!(
            ui.edit.as_ref().unwrap().error.as_deref(),
            Some("mindestens 1,00 m")
        );
        ui.key(Key::Backspace, true, M);
        let out = tippe(&mut ui, "2,60");
        assert_eq!(out.submit, Some((f, 2600.0)));
        assert!(out.relayout && ui.edit.is_none());
        // Kote der Gründung mit Vorzeichen, Punkt oder Komma
        klick_auf(&mut ui, Field::LevelBottom);
        assert_eq!(ui.edit.as_ref().unwrap().text, "-0,80");
        let out = tippe(&mut ui, "-0.9");
        assert_eq!(out.submit, Some((Field::LevelBottom, -900.0)));
        // Esc verwirft
        klick_auf(&mut ui, Field::StoreyHeight(eg));
        ui.key(Key::Char('9'), true, M);
        let out = ui.key(Key::Escape, true, M).unwrap();
        assert!(out.submit.is_none() && ui.edit.is_none());
    }

    /// A47 und Test 5 aus E14: auch im kleinen Fenster nichts abgeschnitten;
    /// reicht der Platz nicht, wird es eine Liste mit denselben Zahlen.
    #[test]
    fn kleines_fenster_diagramm_oder_liste() {
        let (mut ui, _) = ui_mit_geschossen();
        for (w, h, list) in [(900u32, 600u32, false), (900, 420, false), (900, 400, true)] {
            ui.fit(1.0, w, h);
            let r = ui.rect(Panel::Levels, w, 32);
            let t = ui.rect(Panel::Tools, w, 32);
            assert!(r.y >= t.y + t.h && r.y + r.h <= h as f32, "{w}×{h}: {r:?}");
            assert_eq!(ui.levels_layout().list, list, "{w}×{h}");
            let n = ui.level_buttons().len();
            assert_eq!(n, if list { 6 } else { 7 }, "{w}×{h}");
            for (_, b, _) in ui.level_buttons() {
                assert!(b.y >= 0.0 && b.y + b.h <= r.h, "{w}×{h}: {b:?}");
            }
        }
    }

    /// Hover über Griff und Maßzahl zeichnet das Paneel neu, gleich dem
    /// vollen Zeichnen; Test 6: aktive Linie folgt dem Akzent.
    #[test]
    fn neu_zeichnen_gleicht_dem_ganzen_paneel() {
        let mut th = Theme::dark();
        let (mut ui, _) = ui_mit_geschossen();
        let og = band(&ui, "OG").id;
        for id in [Id::Grip(Grip::Top(og)), Id::Field(Field::StoreyHeight(og))] {
            ui.paint(&th, Panel::Levels, 1440, 32);
            ui.hover = Some(id);
            let patch = ui.repaint_button(&th, id).expect("Paneelbild vorhanden");
            let full = ui.paint(&th, Panel::Levels, 1440, 32).0.to_premul_rgba8();
            assert!(patch.px == full, "{id:?}");
        }
        let blue = Rgba::rgb(40, 120, 220);
        th.set_accent(blue);
        assert!(ui.repaint_button(&th, Id::Grip(Grip::Top(og))).is_none());
        ui.hover = None;
        let lines = level_lines(&ui.levels);
        let ys = ui.line_ys(&lines, &ui.levels_layout());
        let c = ui.paint(&th, Panel::Levels, 1440, 32).0;
        let m = th.size.panel_shadow;
        let px = c.to_rgba8();
        let (x, y) = ((m + 120.0) as usize, (m + ys[1]).round() as usize);
        let i = (y * c.width + x) * 4;
        assert_eq!(Rgba(px[i], px[i + 1], px[i + 2], px[i + 3]), blue);
    }
}

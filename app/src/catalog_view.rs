//! Dialog „Bauteilkatalog“ (K3, Bilder soll-katalog-2 bis -5): links die
//! Typen nach Außen- und Innenwänden, rechts Kopf, Schnittbild, Kennwerte
//! und Schichten. Reiter „Projekt“ bearbeitet die Typen des Projekts,
//! „Firma“ vergleicht mit dem Firmenkatalog; je Reiter eine Aktion unten
//! links.
//!
//! Bearbeitet wird eine Kopie des Modells. OK übernimmt alles als einen
//! Rückgängig-Schritt „Bauteilkatalog geändert“; ändern sich die Schichten
//! eines verbauten Typs, fragt vorher eine Karte (F2): „Ändern“ oder „Als
//! neuen Typ speichern“. Abbrechen verwirft. Das Zurückspeichern in den
//! Firmenkatalog hat kein Rückgängig und fragt deshalb vorher.

use crate::catalog::{Company, SaveResult};
use crate::prefs::Win;
use crate::scene::Scene;
use crate::type_look::{
    cm_text, paint_section, paint_thumb, section_layer_at, type_look, SectionMarks, TypeLook,
};
use sk_model::{
    compare, import_type, type_code, ElementId, Guid, LayerFunction, LayerSet, LayerSetId, Library,
    MatCategory, Material, MaterialLayer, Model, PropValue, TypeCategory, TypeState, TYPE_PROPS,
};
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{Cursor, Event, Key, Modifiers, MouseButton};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, FieldState, Fonts, Rect};
use std::time::{Duration, Instant};

/// Kopf, Fuß, Rand (dip).
const HEAD: f32 = 64.0;
const FOOT: f32 = 56.0;
const PAD: f32 = 16.0;
/// Kleinste Fenstergröße (dip).
const MIN_W: f32 = 900.0;
const MIN_H: f32 = 600.0;
/// Inhalt rechts: linke Kante (ab Fenster) und rechte Spalte.
const CONTENT_X: f32 = 302.0;
const RIGHT_X: f32 = 752.0;
const PROPS_X: f32 = 880.0;
/// Schichtzeilen.
const ROW_H: f32 = 34.0;
const ROWS_Y: f32 = 442.0;
/// Grün für „wie im Projekt“ (Marke im Firmenreiter).
const SAME: Rgba = Rgba::rgb(126, 196, 140);
/// Funktionen in der Auswahl (Luftschicht kommt mit K4).
const FUNCTIONS: [(LayerFunction, &str); 4] = [
    (LayerFunction::Structure, "tragend"),
    (LayerFunction::Insulation, "Dämmung"),
    (LayerFunction::Finish, "Bekleidung"),
    (LayerFunction::Membrane, "Abdichtung"),
];

fn function_name(f: LayerFunction) -> &'static str {
    match f {
        LayerFunction::AirGap => "Luftschicht",
        f => FUNCTIONS.iter().find(|x| x.0 == f).map_or("", |x| x.1),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Project,
    Company,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldId {
    Name,
    Code,
    Thick(usize),
    Prop(usize),
    MatName,
    MatDensity,
    MatLambda,
}

impl FieldId {
    fn numeric(self) -> bool {
        matches!(
            self,
            FieldId::Thick(_) | FieldId::MatDensity | FieldId::MatLambda
        )
    }

    fn unit(self) -> &'static str {
        match self {
            FieldId::Thick(_) => "cm",
            FieldId::MatDensity => "kg/m³",
            FieldId::MatLambda => "W/mK",
            _ => "",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComboId {
    Category,
    Material(usize),
    Function(usize),
}

/// Knöpfe unter der Liste, unten und in den Karten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Btn {
    New,
    Duplicate,
    Delete,
    Action,
    Cancel,
    Ok,
    /// Karten: Abbrechen, bestätigen.
    PopCancel,
    PopOk,
    /// Rückfrage: als neuen Typ, ändern, zurück.
    AskNew,
    AskChange,
    AskClose,
}

/// Eintrag der Liste links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Type(LayerSetId),
    /// Neuer, noch unvollständiger Typ.
    Draft,
    Company(Guid),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Close,
    Tab(Tab),
    PathLink,
    Tile(Item),
    Btn(Btn),
    Field(FieldId),
    Combo(ComboId),
    Grip(usize),
    Remove(usize),
    AddLayer,
    Standard,
    /// Schicht im Schnittbild.
    Section(usize),
    /// Eintrag einer Auswahlliste.
    Choice(usize),
    /// Karte „Neuer Bauteiltyp“: Art und Ausgangstyp (`None`: leer).
    NewCat(TypeCategory),
    NewFrom(Option<Guid>),
    /// Karte „Neuer Baustoff“: Kategorie.
    MatCat(MatCategory),
    /// Fläche einer Karte ohne eigenes Ziel.
    Card,
}

#[derive(Clone, Debug)]
struct Edit {
    field: FieldId,
    text: TextEdit,
    orig: String,
    invalid: Option<String>,
}

#[derive(Clone, Debug)]
struct Combo {
    id: ComboId,
    items: Vec<(String, Option<TypeLook>)>,
    sel: Option<usize>,
    anchor: Rect,
}

#[derive(Clone, Debug)]
struct NewMat {
    row: usize,
    name: String,
    density: String,
    lambda: String,
    cat: MatCategory,
}

#[derive(Clone, Debug)]
struct Ask {
    /// Verbaute Typen mit geänderten Schichten, der Reihe nach.
    queue: Vec<Guid>,
    at: usize,
    /// Antwort je Typ: `true` = als neuen Typ speichern.
    answers: Vec<(Guid, bool)>,
}

#[derive(Clone, Debug)]
enum Popup {
    Combo(Combo),
    NewType {
        cat: TypeCategory,
        from: Option<Guid>,
    },
    NewMat(NewMat),
    /// „In den Firmenkatalog speichern?“ bzw. die Datei wurde inzwischen
    /// geändert (`true`).
    Export(Guid, bool),
    Ask(Ask),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Window(f32, f32),
    Row(usize),
    Select(FieldId),
}

/// Was die App nach einem Ereignis tun muss.
#[derive(Clone, Debug, Default)]
pub struct Out {
    pub repaint: bool,
    pub popup: bool,
    pub moved: bool,
    pub closed: bool,
    /// Das Modell hat sich geändert (OK): Netze, Paneele, Mengen neu.
    pub applied: bool,
    /// Wände im Modell hervorheben (Rückfrage); leer = keine.
    pub highlight: Option<Vec<ElementId>>,
    /// Ort des Firmenkatalogs wählen („ändern …“).
    pub pick_company: bool,
}

impl Out {
    fn all() -> Out {
        Out {
            repaint: true,
            popup: true,
            ..Out::default()
        }
    }
}

pub struct Ctx<'a> {
    pub scene: &'a mut Scene,
    pub theme: &'a Theme,
    pub fonts: &'a Fonts,
    pub win: Win,
    pub company: Option<&'a mut Company>,
    /// Der Firmenkatalog liegt am Vorgabeort (für das Neuladen).
    pub company_standard: bool,
}

/// Aufleuchten nach einer Änderung: Name, Dicke, U-Wert.
#[derive(Clone, Copy, Debug)]
enum Flash {
    Name = 0,
    Thick = 1,
    U = 2,
}

pub struct Catalog {
    work: Model,
    pub tab: Tab,
    /// Gewählter Projekttyp; `None` mit `draft_new`: neuer Typ.
    sel: Option<LayerSetId>,
    draft: LayerSet,
    draft_new: bool,
    /// Gewählter Typ im Firmenreiter.
    csel: Option<Guid>,
    company: Option<(Library, String)>,
    edit: Option<Edit>,
    hover: Option<Target>,
    pressed: Option<Target>,
    popup: Option<Popup>,
    drag: Option<Drag>,
    pos: Option<(f32, f32)>,
    scroll: f32,
    message: Option<String>,
    /// „Der Name folgt der Dicke.“
    name_hint: bool,
    flashes: [Option<Instant>; 3],
    /// Übergang der Schichtgrenzen: Dicken vorher und Beginn.
    anim: Option<(Vec<f64>, Instant)>,
    ghost: Option<(f64, String)>,
    /// Ein Fehlversuch (ungültiger Typ): Meldung leuchtet.
    blink: Option<Instant>,
}

fn prop_text(v: &PropValue) -> String {
    match v {
        PropValue::Text(t) => t.clone(),
        PropValue::Number(n) => format!("{n}").replace('.', ","),
        PropValue::Bool(b) => if *b { "ja" } else { "nein" }.into(),
    }
}

/// Zahl mit Komma.
fn de(v: f64, dec: usize) -> String {
    format!("{v:.dec$}").replace('.', ",")
}

/// Dicke in cm wie im Feld: „24“, „36,5“.
fn cm_field(mm: f64) -> String {
    let cm = mm / 10.0;
    if (cm - cm.round()).abs() < 1e-9 {
        format!("{cm:.0}")
    } else {
        de(cm, 1)
    }
}

fn parse_num(t: &str) -> Option<f64> {
    let v: f64 = t.trim().replace(',', ".").parse().ok()?;
    v.is_finite().then_some(v)
}

/// Dicke aus dem Feld (cm, 0,5 bis 100, Schritt 0,5) in mm.
fn parse_thick(t: &str) -> Result<f64, String> {
    match parse_num(t) {
        Some(cm) if (0.5..=100.0).contains(&cm) => Ok((cm * 2.0).round() * 5.0),
        _ => Err("Dicke 0,5 bis 100 cm".into()),
    }
}

/// Steht die alte Dicke als eigenes Wort im Namen, zieht sie mit
/// („AW mit WDVS 36“ → „… 40“, „AW 31,5 Gasbeton“ → „AW 33,5 Gasbeton“).
fn follow_name(name: &str, old: f64, new: f64) -> Option<String> {
    let (o, n) = (cm_field(old), cm_field(new));
    let word = |c: Option<char>| c.is_some_and(|c| c.is_ascii_digit() || c == ',');
    let at = name.rmatch_indices(&o).map(|(i, _)| i).find(|&i| {
        !word(name[..i].chars().next_back()) && !word(name[i + o.len()..].chars().next())
    })?;
    Some(format!("{}{n}{}", &name[..at], &name[at + o.len()..]))
}

/// Kacheln, Gruppenköpfe (Text, y) und Gesamthöhe der Liste.
type ListLayout = (Vec<(Item, Rect)>, Vec<(&'static str, f32)>, f32);

impl Catalog {
    pub fn open(scene: &Scene, company: Option<&Company>) -> Catalog {
        let mut work = scene.model().clone();
        work.allow_unstepped();
        work.fork_guids();
        let sel = Some(work.default_type(TypeCategory::ExteriorWall));
        let draft = sel
            .and_then(|id| work.layer_set(id))
            .cloned()
            .unwrap_or_else(|| empty_type(&mut work, TypeCategory::ExteriorWall));
        let mut c = Catalog {
            work,
            tab: Tab::Project,
            sel,
            draft,
            draft_new: false,
            csel: None,
            company: None,
            edit: None,
            hover: None,
            pressed: None,
            popup: None,
            drag: None,
            pos: None,
            scroll: 0.0,
            message: None,
            name_hint: false,
            flashes: [None; 3],
            anim: None,
            ghost: None,
            blink: None,
        };
        c.set_company(company);
        c
    }

    /// Neuer Stand des Firmenkatalogs (nach Speichern, Neuladen, neuem Ort).
    pub fn set_company(&mut self, company: Option<&Company>) {
        self.company = company.map(|c| (c.library().clone(), c.path().display().to_string()));
        if self.csel.is_none_or(|g| {
            self.company_lib()
                .is_none_or(|l| l.type_by_guid(g).is_none())
        }) {
            self.csel = self.company_items().first().map(|(_, g)| *g);
        }
    }

    fn company_lib(&self) -> Option<&Library> {
        self.company.as_ref().map(|c| &c.0)
    }

    /// Läuft die Rückfrage? Dann tritt das Fenster zurück.
    pub fn asking(&self) -> bool {
        matches!(self.popup, Some(Popup::Ask(_)))
    }

    // --- Lage -----------------------------------------------------------------

    fn size(&self, t: &Theme, w: &Win) -> (f32, f32) {
        let s = w.scale;
        let m = t.size.panel_margin * s;
        let avail_w = w.w as f32 - 2.0 * m;
        let avail_h = w.h as f32 - w.top as f32 - 2.0 * m;
        let ww = (t.size.catalog_w * s).min(avail_w).max(MIN_W * s);
        let hh = (t.size.catalog_h * s).min(avail_h).max(MIN_H * s);
        (ww.round(), hh.round())
    }

    /// Fensterfläche im Programmfenster, anfangs mittig.
    fn frame(&self, t: &Theme, w: &Win) -> Rect {
        let (ww, hh) = self.size(t, w);
        let (x, y) = self.pos.unwrap_or((
            (w.w as f32 - ww) * 0.5,
            w.top as f32 + (w.h as f32 - w.top as f32 - hh) * 0.5,
        ));
        let x = x.min(w.w as f32 - ww).max(0.0);
        let y = y.min(w.h as f32 - hh).max(w.top as f32);
        Rect::new(x.round(), y.round(), ww, hh)
    }

    pub fn origin(&self, t: &Theme, w: &Win) -> (i32, i32) {
        let f = self.frame(t, w);
        let m = (t.size.panel_shadow * w.scale).round();
        ((f.x - m) as i32, (f.y - m) as i32)
    }

    /// Rechteck in dip ab der linken oberen Fensterecke.
    fn r(&self, t: &Theme, w: &Win, x: f32, y: f32, ww: f32, hh: f32) -> Rect {
        let f = self.frame(t, w);
        let s = w.scale;
        Rect::new(
            (f.x + x * s).round(),
            (f.y + y * s).round(),
            (ww * s).round(),
            (hh * s).round(),
        )
    }

    fn close_rect(&self, t: &Theme, w: &Win) -> Rect {
        let f = self.frame(t, w);
        let s = w.scale;
        Rect::new(f.x + f.w - 46.0 * s, f.y + 18.0 * s, 28.0 * s, 28.0 * s)
    }

    fn tab_rect(&self, t: &Theme, w: &Win, tab: Tab) -> Rect {
        let i = if tab == Tab::Project { 0.0 } else { 1.0 };
        self.r(t, w, 204.0 + i * 108.0, 18.0, 106.0, 28.0)
    }

    fn path_link(&self, t: &Theme, w: &Win, fonts: &Fonts) -> Option<Rect> {
        if self.tab != Tab::Company {
            return None;
        }
        let path = &self.company.as_ref()?.1;
        let s = w.scale;
        let px = t.size.font_small * s;
        let pw = fonts.regular.as_ref().map_or(0.0, |f| f.width(path, px));
        let pw = pw.min(360.0 * s);
        let lw = fonts
            .regular
            .as_ref()
            .map_or(60.0, |f| f.width("ändern …", px));
        let base = self.r(t, w, 436.0, 18.0, 0.0, 28.0);
        Some(Rect::new(
            base.x + pw + 14.0 * s,
            base.y,
            lw + 4.0 * s,
            base.h,
        ))
    }

    /// Liste links: sichtbarer Bereich.
    fn list_body(&self, t: &Theme, w: &Win) -> Rect {
        let (_, hh) = self.size(t, w);
        let s = w.scale;
        let h = hh / s - HEAD - FOOT - 60.0 - 8.0;
        self.r(t, w, 0.0, HEAD + 8.0, t.size.catalog_list_w + 12.0, h)
    }

    /// Kacheln der Liste mit Gruppenköpfen (Fensterkoordinaten, ohne
    /// Bildlauf).
    fn list_layout(&self, t: &Theme, w: &Win) -> ListLayout {
        let s = w.scale;
        let body = self.list_body(t, w);
        let tile_h = t.size.catalog_tile_h * s;
        let mut y = body.y - self.scroll;
        let (mut tiles, mut heads) = (Vec::new(), Vec::new());
        for cat in TypeCategory::ALL {
            let items = self.items_of(cat);
            if items.is_empty() {
                continue;
            }
            let head = match cat {
                TypeCategory::ExteriorWall => "AUSSENWÄNDE",
                TypeCategory::InteriorWall => "INNENWÄNDE",
            };
            heads.push((head, y));
            y += 24.0 * s;
            for it in items {
                tiles.push((
                    it,
                    Rect::new(
                        body.x + 10.0 * s,
                        y.round(),
                        (t.size.catalog_list_w - 8.0) * s,
                        tile_h,
                    ),
                ));
                y += tile_h + 4.0 * s;
            }
            y += 10.0 * s;
        }
        let content = y + self.scroll - body.y;
        (tiles, heads, content)
    }

    fn items_of(&self, cat: TypeCategory) -> Vec<Item> {
        match self.tab {
            Tab::Project => {
                let mut v: Vec<Item> = self
                    .work
                    .layer_sets()
                    .iter()
                    .filter(|(_, t)| t.category == cat)
                    .map(|(id, _)| Item::Type(id))
                    .collect();
                if self.draft_new && self.draft.category == cat {
                    v.push(Item::Draft);
                }
                v
            }
            Tab::Company => self
                .company_items()
                .into_iter()
                .filter(|(c, _)| *c == cat)
                .map(|(_, g)| Item::Company(g))
                .collect(),
        }
    }

    fn company_items(&self) -> Vec<(TypeCategory, Guid)> {
        let Some(lib) = self.company_lib() else {
            return Vec::new();
        };
        let mut v: Vec<(TypeCategory, Guid)> = TypeCategory::ALL
            .iter()
            .flat_map(|cat| {
                lib.types
                    .iter()
                    .filter(move |(_, t)| t.category == *cat)
                    .map(|(_, t)| (t.category, t.guid))
            })
            .collect();
        v.dedup();
        v
    }

    fn list_buttons(&self, t: &Theme, w: &Win) -> [(Btn, Rect, &'static str); 3] {
        let (_, hh) = self.size(t, w);
        let y = hh / w.scale - FOOT - 50.0;
        [
            (Btn::New, self.r(t, w, 10.0, y, 70.0, 30.0), "Neu"),
            (
                Btn::Duplicate,
                self.r(t, w, 88.0, y, 110.0, 30.0),
                "Duplizieren",
            ),
            (Btn::Delete, self.r(t, w, 206.0, y, 66.0, 30.0), "Löschen"),
        ]
    }

    fn foot_buttons(&self, t: &Theme, w: &Win) -> [(Btn, Rect, &'static str); 3] {
        let (ww, hh) = self.size(t, w);
        let (ww, hh) = (ww / w.scale, hh / w.scale);
        let y = hh - FOOT + 13.0;
        let action = match self.tab {
            Tab::Project => "In Firmenkatalog speichern",
            Tab::Company => "Ins Projekt übernehmen",
        };
        [
            (Btn::Action, self.r(t, w, PAD, y, 230.0, 30.0), action),
            (
                Btn::Cancel,
                self.r(t, w, ww - PAD - 112.0 - 8.0 - 100.0, y, 100.0, 30.0),
                "Abbrechen",
            ),
            (
                Btn::Ok,
                self.r(t, w, ww - PAD - 112.0, y, 112.0, 30.0),
                "OK",
            ),
        ]
    }

    fn section_rect(&self, t: &Theme, w: &Win) -> Rect {
        self.r(t, w, CONTENT_X, 120.0, 430.0, 262.0)
    }

    fn field_rect(&self, t: &Theme, w: &Win, f: FieldId) -> Option<Rect> {
        match f {
            FieldId::Name => Some(self.r(t, w, CONTENT_X, 72.0, 330.0, 32.0)),
            FieldId::Code => Some(self.r(t, w, CONTENT_X + 340.0, 72.0, 80.0, 32.0)),
            FieldId::Thick(i) => Some(self.r(t, w, 584.0, ROWS_Y + i as f32 * ROW_H, 86.0, 26.0)),
            FieldId::Prop(i) => {
                Some(self.r(t, w, PROPS_X, ROWS_Y + 4.0 + i as f32 * 48.0, 172.0, 26.0))
            }
            FieldId::MatName | FieldId::MatDensity | FieldId::MatLambda => {
                let card = self.new_mat_card(t, w)?;
                let s = w.scale;
                let i = match f {
                    FieldId::MatName => 0.0,
                    FieldId::MatDensity => 1.0,
                    _ => 2.0,
                };
                Some(Rect::new(
                    card.x + 150.0 * s,
                    card.y + (52.0 + i * 36.0) * s,
                    card.w - 170.0 * s,
                    28.0 * s,
                ))
            }
        }
    }

    fn combo_rect(&self, t: &Theme, w: &Win, id: ComboId) -> Rect {
        match id {
            ComboId::Category => self.r(t, w, CONTENT_X + 430.0, 72.0, 130.0, 32.0),
            ComboId::Material(i) => self.r(t, w, 324.0, ROWS_Y + i as f32 * ROW_H, 250.0, 26.0),
            ComboId::Function(i) => self.r(t, w, 680.0, ROWS_Y + i as f32 * ROW_H, 150.0, 26.0),
        }
    }

    fn standard_rect(&self, t: &Theme, w: &Win) -> Rect {
        self.r(t, w, RIGHT_X, 282.0, 16.0, 16.0)
    }

    fn add_layer_rect(&self, t: &Theme, w: &Win) -> Rect {
        let n = self.draft.layers.len() as f32;
        self.r(t, w, CONTENT_X, ROWS_Y + n * ROW_H + 4.0, 90.0, 22.0)
    }

    // --- Karten ---------------------------------------------------------------

    fn new_type_card(&self, t: &Theme, w: &Win) -> Rect {
        let n = self.new_from_items().len() as f32 + 1.0;
        let h = 174.0 + n * 36.0 + 64.0;
        let (_, hh) = self.size(t, w);
        let y = hh / w.scale - FOOT - 60.0 - h + 20.0;
        self.r(t, w, 20.0, y.max(HEAD), 470.0, h)
    }

    fn new_from_items(&self) -> Vec<(Guid, String, f64)> {
        let cat = match &self.popup {
            Some(Popup::NewType { cat, .. }) => *cat,
            _ => TypeCategory::ExteriorWall,
        };
        let Some(lib) = self.company_lib() else {
            return Vec::new();
        };
        lib.types
            .iter()
            .filter(|(_, t)| t.category == cat)
            .map(|(_, t)| (t.guid, t.name.clone(), t.thickness()))
            .collect()
    }

    fn new_mat_card(&self, t: &Theme, w: &Win) -> Option<Rect> {
        let Some(Popup::NewMat(nm)) = &self.popup else {
            return None;
        };
        let anchor = self.combo_rect(t, w, ComboId::Material(nm.row));
        let s = w.scale;
        let (cw, ch) = (420.0 * s, 250.0 * s);
        let y = (anchor.y - ch - 8.0 * s).max(w.top as f32);
        Some(Rect::new(
            anchor.x.round(),
            y.round(),
            cw.round(),
            ch.round(),
        ))
    }

    fn export_card(&self, t: &Theme, w: &Win) -> Rect {
        let (ww, hh) = self.size(t, w);
        let (ww, hh) = (ww / w.scale, hh / w.scale);
        self.r(t, w, (ww - 460.0) * 0.5, hh - FOOT - 190.0, 460.0, 170.0)
    }

    /// Rückfrage unten mittig im Programmfenster.
    fn ask_card(&self, w: &Win) -> Rect {
        let s = w.scale;
        let (cw, ch) = (580.0 * s, 236.0 * s);
        let x = (w.w as f32 - cw) * 0.5;
        let y = w.h as f32 - ch - 64.0 * s;
        Rect::new(x.round(), y.round(), cw.round(), ch.round())
    }

    /// Knöpfe einer Karte.
    fn card_buttons(&self, t: &Theme, w: &Win) -> Vec<(Btn, Rect, &'static str)> {
        let s = w.scale;
        match &self.popup {
            Some(Popup::NewType { .. }) => {
                let c = self.new_type_card(t, w);
                let y = c.y + c.h - 46.0 * s;
                vec![
                    (
                        Btn::PopCancel,
                        Rect::new(c.x + c.w - 236.0 * s, y, 104.0 * s, 30.0 * s),
                        "Abbrechen",
                    ),
                    (
                        Btn::PopOk,
                        Rect::new(c.x + c.w - 122.0 * s, y, 104.0 * s, 30.0 * s),
                        "Anlegen",
                    ),
                ]
            }
            Some(Popup::NewMat(_)) => {
                let Some(c) = self.new_mat_card(t, w) else {
                    return Vec::new();
                };
                let y = c.y + c.h - 46.0 * s;
                vec![
                    (
                        Btn::PopCancel,
                        Rect::new(c.x + c.w - 236.0 * s, y, 104.0 * s, 30.0 * s),
                        "Abbrechen",
                    ),
                    (
                        Btn::PopOk,
                        Rect::new(c.x + c.w - 122.0 * s, y, 104.0 * s, 30.0 * s),
                        "Anlegen",
                    ),
                ]
            }
            Some(Popup::Export(_, changed)) => {
                let c = self.export_card(t, w);
                let y = c.y + c.h - 46.0 * s;
                let ok = if *changed {
                    "Neu laden und speichern"
                } else {
                    "Speichern"
                };
                let ow = if *changed { 200.0 } else { 110.0 } * s;
                vec![
                    (
                        Btn::PopCancel,
                        Rect::new(c.x + c.w - ow - 122.0 * s, y, 104.0 * s, 30.0 * s),
                        "Abbrechen",
                    ),
                    (
                        Btn::PopOk,
                        Rect::new(c.x + c.w - ow - 18.0 * s, y, ow, 30.0 * s),
                        ok,
                    ),
                ]
            }
            Some(Popup::Ask(_)) => {
                let c = self.ask_card(w);
                let y = c.y + 112.0 * s;
                let bw = (c.w - 60.0 * s) * 0.5;
                vec![
                    (
                        Btn::AskNew,
                        Rect::new(c.x + 22.0 * s, y, bw, 70.0 * s),
                        "Als neuen Typ speichern",
                    ),
                    (
                        Btn::AskChange,
                        Rect::new(c.x + 38.0 * s + bw, y, bw, 70.0 * s),
                        "Ändern",
                    ),
                    (
                        Btn::AskClose,
                        Rect::new(c.x + c.w - 36.0 * s, c.y + 10.0 * s, 24.0 * s, 24.0 * s),
                        "",
                    ),
                ]
            }
            _ => Vec::new(),
        }
    }

    fn combo_rows(&self, cb: &Combo, w: &Win) -> (Rect, Vec<Rect>) {
        let s = w.scale;
        let row = 30.0 * s;
        let n = cb.items.len() as f32;
        let h = n * row + 8.0 * s;
        let mut y = cb.anchor.y + cb.anchor.h + 2.0 * s;
        if y + h > w.h as f32 {
            y = (cb.anchor.y - h - 2.0 * s).max(w.top as f32);
        }
        let r = Rect::new(
            cb.anchor.x,
            y.round(),
            cb.anchor.w.max(220.0 * s),
            h.round(),
        );
        let rows = (0..cb.items.len())
            .map(|i| Rect::new(r.x, r.y + 4.0 * s + i as f32 * row, r.w, row))
            .collect();
        (r, rows)
    }

    // --- Treffer --------------------------------------------------------------

    fn hit(&self, t: &Theme, w: &Win, fonts: &Fonts, x: f64, y: f64) -> Option<Target> {
        if let Some(p) = &self.popup {
            return self.popup_hit(p, t, w, x, y);
        }
        let f = self.frame(t, w);
        if !f.contains(x, y) {
            return None;
        }
        if self.close_rect(t, w).contains(x, y) {
            return Some(Target::Close);
        }
        for tab in [Tab::Project, Tab::Company] {
            if self.tab_rect(t, w, tab).contains(x, y) {
                return Some(Target::Tab(tab));
            }
        }
        if self
            .path_link(t, w, fonts)
            .is_some_and(|r| r.contains(x, y))
        {
            return Some(Target::PathLink);
        }
        let body = self.list_body(t, w);
        if body.contains(x, y) {
            let (tiles, ..) = self.list_layout(t, w);
            return tiles
                .into_iter()
                .find(|(_, r)| r.contains(x, y))
                .map(|(it, _)| Target::Tile(it));
        }
        for (b, r, _) in self.foot_buttons(t, w) {
            if r.contains(x, y) {
                return Some(Target::Btn(b));
            }
        }
        if self.tab == Tab::Project {
            for (b, r, _) in self.list_buttons(t, w) {
                if r.contains(x, y) {
                    return Some(Target::Btn(b));
                }
            }
            return self.project_hit(t, w, x, y);
        }
        None
    }

    fn project_hit(&self, t: &Theme, w: &Win, x: f64, y: f64) -> Option<Target> {
        let s = w.scale;
        for fi in [FieldId::Name, FieldId::Code] {
            if self.field_rect(t, w, fi).is_some_and(|r| r.contains(x, y)) {
                return Some(Target::Field(fi));
            }
        }
        if self.combo_rect(t, w, ComboId::Category).contains(x, y) {
            return Some(Target::Combo(ComboId::Category));
        }
        let sec = self.section_rect(t, w);
        if sec.contains(x, y) {
            let look = type_look(&self.work, t, &self.draft);
            let th = self.anim_thick(t);
            return section_layer_at(sec, &look, &th, x as f32, y as f32).map(Target::Section);
        }
        let st = self.standard_rect(t, w);
        let label = Rect::new(st.x, st.y, 260.0 * s, st.h);
        if label.contains(x, y) {
            return Some(Target::Standard);
        }
        for i in 0..self.draft.layers.len() {
            let row_y = self.r(t, w, 0.0, ROWS_Y + i as f32 * ROW_H, 0.0, 26.0);
            if (y as f32) < row_y.y || (y as f32) >= row_y.y + row_y.h {
                continue;
            }
            let grip = self.r(t, w, CONTENT_X, ROWS_Y + i as f32 * ROW_H, 20.0, 26.0);
            if grip.contains(x, y) {
                return Some(Target::Grip(i));
            }
            for id in [ComboId::Material(i), ComboId::Function(i)] {
                if self.combo_rect(t, w, id).contains(x, y) {
                    return Some(Target::Combo(id));
                }
            }
            if self
                .field_rect(t, w, FieldId::Thick(i))
                .is_some_and(|r| r.contains(x, y))
            {
                return Some(Target::Field(FieldId::Thick(i)));
            }
            let rm = self.r(t, w, 836.0, ROWS_Y + i as f32 * ROW_H, 26.0, 26.0);
            if rm.contains(x, y) {
                return Some(Target::Remove(i));
            }
        }
        if self.add_layer_rect(t, w).contains(x, y) {
            return Some(Target::AddLayer);
        }
        for i in 0..TYPE_PROPS.len() {
            if self
                .field_rect(t, w, FieldId::Prop(i))
                .is_some_and(|r| r.contains(x, y))
            {
                return Some(Target::Field(FieldId::Prop(i)));
            }
        }
        None
    }

    fn popup_hit(&self, p: &Popup, t: &Theme, w: &Win, x: f64, y: f64) -> Option<Target> {
        let s = w.scale;
        for (b, r, _) in self.card_buttons(t, w) {
            if r.contains(x, y) {
                return Some(Target::Btn(b));
            }
        }
        match p {
            Popup::Combo(cb) => {
                let (_, rows) = self.combo_rows(cb, w);
                rows.iter()
                    .position(|r| r.contains(x, y))
                    .map(Target::Choice)
            }
            Popup::NewType { .. } => {
                let c = self.new_type_card(t, w);
                if !c.contains(x, y) {
                    return None;
                }
                for (i, cat) in TypeCategory::ALL.into_iter().enumerate() {
                    let r = Rect::new(
                        c.x + (18.0 + i as f32 * 222.0) * s,
                        c.y + 64.0 * s,
                        212.0 * s,
                        70.0 * s,
                    );
                    if r.contains(x, y) {
                        return Some(Target::NewCat(cat));
                    }
                }
                let items = self.new_from_items();
                for i in 0..=items.len() {
                    let r = Rect::new(
                        c.x + 18.0 * s,
                        c.y + (174.0 + i as f32 * 36.0) * s,
                        c.w - 36.0 * s,
                        32.0 * s,
                    );
                    if r.contains(x, y) {
                        return Some(Target::NewFrom(items.get(i).map(|it| it.0)));
                    }
                }
                Some(Target::Card)
            }
            Popup::NewMat(_) => {
                let c = self.new_mat_card(t, w)?;
                if !c.contains(x, y) {
                    return None;
                }
                for f in [FieldId::MatName, FieldId::MatDensity, FieldId::MatLambda] {
                    if self.field_rect(t, w, f).is_some_and(|r| r.contains(x, y)) {
                        return Some(Target::Field(f));
                    }
                }
                for (i, cat) in MatCategory::ALL.into_iter().enumerate() {
                    if mat_cat_rect(c, i, s).contains(x, y) {
                        return Some(Target::MatCat(cat));
                    }
                }
                Some(Target::Card)
            }
            Popup::Export(..) => self
                .export_card(t, w)
                .contains(x, y)
                .then_some(Target::Card),
            Popup::Ask(_) => self.ask_card(w).contains(x, y).then_some(Target::Card),
        }
    }

    // --- Ereignisse -----------------------------------------------------------

    pub fn handle(&mut self, e: &Event, cx: &mut Ctx) -> Out {
        let mut out = Out::default();
        match *e {
            Event::MouseMove { x, y, .. } => self.mouse_move(x, y, cx, &mut out),
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            } => self.mouse_down(x, y, mods, cx, &mut out),
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => self.mouse_up(x, y, cx, &mut out),
            Event::Wheel { delta, x, y, .. } => {
                let (t, w) = (cx.theme, cx.win);
                if self.popup.is_none() && self.list_body(t, &w).contains(x, y) {
                    let (_, _, content) = self.list_layout(t, &w);
                    let max = (content - self.list_body(t, &w).h).max(0.0);
                    let step = -(delta as f32) * 3.0 * 30.0 * w.scale;
                    self.scroll = (self.scroll + step).clamp(0.0, max);
                    out.repaint = true;
                }
            }
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } => self.key(key, mods, cx, &mut out),
            Event::Text(ch) => self.text(ch, &mut out),
            Event::MouseLeave if self.hover.is_some() => {
                self.hover = None;
                out = Out::all();
            }
            _ => {}
        }
        out
    }

    fn mouse_move(&mut self, x: f64, y: f64, cx: &mut Ctx, out: &mut Out) {
        let (t, w) = (cx.theme, cx.win);
        match self.drag {
            Some(Drag::Window(dx, dy)) => {
                let before = self.frame(t, &w);
                self.pos = Some((x as f32 - dx, y as f32 - dy));
                let after = self.frame(t, &w);
                self.pos = Some((after.x, after.y));
                out.moved = (before.x, before.y) != (after.x, after.y);
                out.popup = out.moved && self.popup.is_some();
                return;
            }
            Some(Drag::Row(from)) => {
                let top = self.r(t, &w, 0.0, ROWS_Y, 0.0, 0.0).y;
                let n = self.draft.layers.len();
                let to =
                    (((y as f32 - top) / (ROW_H * w.scale)).floor().max(0.0) as usize).min(n - 1);
                if to != from {
                    let l = self.draft.layers.remove(from);
                    self.draft.layers.insert(to, l);
                    self.drag = Some(Drag::Row(to));
                    self.edit = None;
                    self.commit_draft();
                    out.repaint = true;
                }
                return;
            }
            Some(Drag::Select(f)) => {
                if let Some(i) = self.caret_from_mouse(f, x, cx) {
                    if let Some(e) = self.edit.as_mut() {
                        e.text.place(i, true);
                    }
                    *out = Out::all();
                }
                return;
            }
            None => {}
        }
        let h = self.hit(t, &w, cx.fonts, x, y);
        if h != self.hover {
            self.hover = h;
            out.repaint = true;
            out.popup = self.popup.is_some();
        }
    }

    fn mouse_down(&mut self, x: f64, y: f64, mods: Modifiers, cx: &mut Ctx, out: &mut Out) {
        let (t, w) = (cx.theme, cx.win);
        let hit = self.hit(t, &w, cx.fonts, x, y);
        *out = Out::all();
        // Auswahlliste: Klick daneben schließt sie
        if let Some(Popup::Combo(_)) = self.popup {
            if hit.is_none() {
                self.popup = None;
                self.hover = self.hit(t, &w, cx.fonts, x, y);
                return;
            }
        }
        // Klick neben das Feld in Eingabe beendet sie
        if let Some(e) = &self.edit {
            if hit != Some(Target::Field(e.field)) {
                self.end_edit(true);
            }
        }
        match hit {
            Some(Target::Field(f)) => {
                if self.edit.as_ref().is_none_or(|e| e.field != f) {
                    self.begin_edit(f);
                } else if let Some(i) = self.caret_from_mouse(f, x, cx) {
                    if let Some(e) = self.edit.as_mut() {
                        e.text.place(i, mods.shift);
                    }
                    self.drag = Some(Drag::Select(f));
                }
            }
            Some(Target::Grip(i)) => self.drag = Some(Drag::Row(i)),
            Some(p) => self.pressed = Some(p),
            None => {
                // Am Kopf ziehen verschiebt das Fenster
                let f = self.frame(t, &w);
                let head = Rect::new(f.x, f.y, f.w, HEAD * w.scale);
                if self.popup.is_none() && head.contains(x, y) {
                    self.drag = Some(Drag::Window(x as f32 - f.x, y as f32 - f.y));
                }
            }
        }
    }

    fn mouse_up(&mut self, x: f64, y: f64, cx: &mut Ctx, out: &mut Out) {
        if let Some(d) = self.drag.take() {
            if !matches!(d, Drag::Window(..)) {
                *out = Out::all();
            }
            return;
        }
        let Some(p) = self.pressed.take() else {
            return;
        };
        *out = Out::all();
        if self.hit(cx.theme, &cx.win, cx.fonts, x, y) == Some(p) {
            self.click(p, cx, out);
        }
    }

    fn click(&mut self, p: Target, cx: &mut Ctx, out: &mut Out) {
        match p {
            Target::Close | Target::Btn(Btn::Cancel) => out.closed = true,
            Target::Btn(Btn::Ok) => self.ok(cx, out),
            Target::Tab(tab) if tab != self.tab => {
                if self.blocked() {
                    return;
                }
                self.tab = tab;
                self.scroll = 0.0;
                self.edit = None;
            }
            Target::PathLink => out.pick_company = true,
            Target::Tile(it) => self.select(it),
            Target::Btn(Btn::New) => {
                if self.blocked() {
                    return;
                }
                let cat = self.draft.category;
                let from = self
                    .company_lib()
                    .and_then(|l| l.default_type(cat))
                    .and_then(|id| self.company_lib()?.types.get(id).map(|t| t.guid));
                self.popup = Some(Popup::NewType { cat, from });
            }
            Target::Btn(Btn::Duplicate) => {
                if self.blocked() {
                    return;
                }
                if let Some(id) = self.sel.and_then(|id| self.work.duplicate_type(id)) {
                    self.select(Item::Type(id));
                }
            }
            Target::Btn(Btn::Delete) => self.delete(),
            Target::Btn(Btn::Action) => match self.tab {
                Tab::Project => {
                    if self.blocked() || cx.company.is_none() || self.draft_new {
                        return;
                    }
                    self.popup = Some(Popup::Export(self.draft.guid, false));
                }
                Tab::Company => self.import_selected(),
            },
            Target::Combo(id) => self.open_combo(id, cx),
            Target::Choice(i) => self.choose(i, cx),
            Target::Remove(i) => {
                if i < self.draft.layers.len() {
                    self.edit = None;
                    let before = self.draft.thickness();
                    self.draft.layers.remove(i);
                    self.thickness_changed(before, None, None);
                    self.commit_draft();
                }
            }
            Target::AddLayer => self.add_layer(),
            Target::Standard => {
                if let Some(id) = self.sel {
                    self.work.set_default_type(self.draft.category, id);
                }
            }
            Target::NewCat(cat) => {
                if let Some(Popup::NewType { cat: c, from }) = &mut self.popup {
                    if *c != cat {
                        *c = cat;
                        *from = None;
                    }
                }
                let first = self.new_from_items().first().map(|x| x.0);
                if let Some(Popup::NewType { from, .. }) = &mut self.popup {
                    if from.is_none() {
                        *from = first;
                    }
                }
            }
            Target::NewFrom(g) => {
                if let Some(Popup::NewType { from, .. }) = &mut self.popup {
                    *from = g;
                }
                // Leer: eigenes Feld, damit „Anlegen“ sichtbar die Wahl trägt
                if g.is_none() {
                    if let Some(Popup::NewType { from, .. }) = &mut self.popup {
                        *from = None;
                    }
                }
            }
            Target::MatCat(cat) => {
                if let Some(Popup::NewMat(nm)) = &mut self.popup {
                    nm.cat = cat;
                }
            }
            Target::Btn(Btn::PopCancel) => {
                self.popup = None;
                self.edit = None;
            }
            Target::Btn(Btn::PopOk) => self.confirm_popup(cx, out),
            Target::Btn(Btn::AskNew) => self.answer(true, cx, out),
            Target::Btn(Btn::AskChange) => self.answer(false, cx, out),
            Target::Btn(Btn::AskClose) => {
                self.popup = None;
                out.highlight = Some(Vec::new());
            }
            _ => {}
        }
        self.hover = None;
    }

    fn key(&mut self, key: Key, mods: Modifiers, cx: &mut Ctx, out: &mut Out) {
        *out = Out::all();
        if self.edit.is_some() {
            self.edit_key(key, mods);
            return;
        }
        match key {
            Key::Escape => match &self.popup {
                Some(Popup::Ask(_)) => {
                    self.popup = None;
                    out.highlight = Some(Vec::new());
                }
                Some(_) => self.popup = None,
                None => out.closed = true,
            },
            Key::Enter => match &self.popup {
                Some(Popup::Combo(cb)) => {
                    if let Some(i) = cb.sel {
                        self.choose(i, cx);
                    }
                }
                Some(Popup::Ask(_)) => self.answer(false, cx, out),
                Some(_) => self.confirm_popup(cx, out),
                None => self.ok(cx, out),
            },
            Key::Other(k @ (0x26 | 0x28)) => {
                if let Some(Popup::Combo(cb)) = &mut self.popup {
                    let n = cb.items.len();
                    if n > 0 {
                        let i = cb.sel.unwrap_or(0);
                        cb.sel = Some(if k == 0x28 {
                            (i + 1) % n
                        } else {
                            (i + n - 1) % n
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn edit_key(&mut self, key: Key, mods: Modifiers) {
        let Some(e) = self.edit.as_mut() else {
            return;
        };
        let f = e.field;
        let sh = mods.shift;
        let changed = match key {
            Key::Escape => {
                let orig = e.orig.clone();
                e.text = TextEdit::new(&orig);
                self.apply_text(f);
                self.edit = None;
                return;
            }
            Key::Enter | Key::Tab => {
                self.end_edit(true);
                return;
            }
            Key::Backspace => {
                e.text.backspace();
                true
            }
            Key::Delete => {
                e.text.delete();
                true
            }
            Key::Left => {
                e.text.left(sh);
                false
            }
            Key::Right => {
                e.text.right(sh);
                false
            }
            Key::Home => {
                e.text.home(sh);
                false
            }
            Key::End => {
                e.text.end(sh);
                false
            }
            Key::Char('A') if mods.ctrl => {
                e.text.select_all();
                false
            }
            Key::Char('C') if mods.ctrl => {
                sk_platform::set_clipboard_text(e.text.selected());
                false
            }
            Key::Char('X') if mods.ctrl => {
                let cut = e.text.cut();
                sk_platform::set_clipboard_text(&cut);
                true
            }
            Key::Char('V') if mods.ctrl => {
                let paste = sk_platform::clipboard_text().unwrap_or_default();
                e.text.insert(paste.lines().next().unwrap_or(""));
                true
            }
            Key::Char('Z') if mods.ctrl => e.text.undo(),
            _ => false,
        };
        if changed {
            self.apply_text(f);
        }
    }

    fn text(&mut self, ch: char, out: &mut Out) {
        let Some(e) = self.edit.as_mut() else {
            return;
        };
        let f = e.field;
        if ch.is_control() || (f.numeric() && !(ch.is_ascii_digit() || matches!(ch, ',' | '.'))) {
            return;
        }
        e.text.insert(ch.encode_utf8(&mut [0; 4]));
        *out = Out::all();
        self.apply_text(f);
    }

    fn caret_from_mouse(&self, f: FieldId, x: f64, cx: &Ctx) -> Option<usize> {
        let r = self.field_rect(cx.theme, &cx.win, f)?;
        let e = self.edit.as_ref().filter(|e| e.field == f)?;
        let s = cx.win.scale;
        let px = cx.theme.size.font_small * s;
        let tx = if f.numeric() {
            widgets::field_x(cx.fonts, r, &e.text.text, f.unit(), s, cx.theme)
        } else {
            widgets::text_field_x(r, s, cx.theme)
        };
        Some(widgets::caret_at(
            cx.fonts.regular.as_ref(),
            &e.text.text,
            px,
            tx,
            x as f32,
        ))
    }

    // --- Felder ---------------------------------------------------------------

    fn field_value(&self, f: FieldId) -> String {
        match f {
            FieldId::Name => self.draft.name.clone(),
            FieldId::Code => self.draft.code.clone(),
            FieldId::Thick(i) => self
                .draft
                .layers
                .get(i)
                .map_or(String::new(), |l| cm_field(l.thickness)),
            FieldId::Prop(i) => self
                .draft
                .props
                .get(TYPE_PROPS[i])
                .map_or(String::new(), prop_text),
            FieldId::MatName | FieldId::MatDensity | FieldId::MatLambda => match &self.popup {
                Some(Popup::NewMat(nm)) => match f {
                    FieldId::MatName => nm.name.clone(),
                    FieldId::MatDensity => nm.density.clone(),
                    _ => nm.lambda.clone(),
                },
                _ => String::new(),
            },
        }
    }

    fn begin_edit(&mut self, f: FieldId) {
        let v = self.field_value(f);
        let mut text = TextEdit::new(&v);
        text.select_all();
        self.edit = Some(Edit {
            field: f,
            text,
            orig: v,
            invalid: None,
        });
    }

    /// Feld verlassen: gültiger Inhalt bleibt (er galt schon), ungültiger
    /// fällt zurück.
    fn end_edit(&mut self, keep: bool) {
        let Some(e) = self.edit.take() else {
            return;
        };
        if !keep || e.invalid.is_some() {
            let mut back = e.clone();
            back.text = TextEdit::new(&e.orig);
            self.edit = Some(back);
            self.apply_text(e.field);
            self.edit = None;
        }
    }

    /// Prüft den Feldinhalt und lässt ihn gültig sofort wirken.
    fn apply_text(&mut self, f: FieldId) {
        let Some(text) = self.edit.as_ref().map(|e| e.text.text.clone()) else {
            return;
        };
        let res = self.apply_value(f, &text);
        if let Some(e) = self.edit.as_mut() {
            e.invalid = res.err();
        }
    }

    fn apply_value(&mut self, f: FieldId, text: &str) -> Result<(), String> {
        match f {
            FieldId::Name => {
                if text.trim().is_empty() {
                    return Err("Name fehlt".into());
                }
                self.draft.name = text.into();
                self.name_hint = false;
                self.commit_draft();
            }
            FieldId::Code => {
                let code = text.trim();
                if code.is_empty() {
                    return Err("Kurzzeichen fehlt".into());
                }
                if self.code_taken(code) {
                    return Err(format!("Kurzzeichen {code} ist schon vergeben"));
                }
                self.draft.code = code.into();
                self.commit_draft();
            }
            FieldId::Thick(i) => {
                let mm = parse_thick(text)?;
                let Some(old) = self.draft.layers.get(i).map(|l| l.thickness) else {
                    return Ok(());
                };
                if (old - mm).abs() < 1e-9 {
                    return Ok(());
                }
                let before = self.draft.thickness();
                let from: Vec<f64> = self.draft.layers.iter().map(|l| l.thickness).collect();
                self.draft.layers[i].thickness = mm;
                // Alte Grenze und Unterschied gegen den Wert beim Betreten des
                // Felds, nicht gegen den letzten Tastendruck
                let start = self
                    .edit
                    .as_ref()
                    .and_then(|e| parse_thick(&e.orig).ok())
                    .unwrap_or(old);
                let edge = self.draft.layers[..i]
                    .iter()
                    .map(|l| l.thickness)
                    .sum::<f64>()
                    + start;
                let delta = mm - start;
                let ghost = (delta.abs() > 1e-9).then(|| {
                    (
                        edge,
                        format!(
                            "{}{}",
                            if delta > 0.0 { "+" } else { "−" },
                            cm_field(delta.abs())
                        ),
                    )
                });
                self.thickness_changed(before, Some(from), ghost);
                self.commit_draft();
            }
            FieldId::Prop(i) => {
                let key = TYPE_PROPS[i].to_string();
                if text.trim().is_empty() {
                    self.draft.props.remove(&key);
                } else {
                    self.draft.props.insert(key, PropValue::Text(text.into()));
                }
                self.commit_draft();
            }
            FieldId::MatName => {
                if let Some(Popup::NewMat(nm)) = &mut self.popup {
                    nm.name = text.into();
                }
                if text.trim().is_empty() {
                    return Err("Name fehlt".into());
                }
            }
            FieldId::MatDensity => {
                if let Some(Popup::NewMat(nm)) = &mut self.popup {
                    nm.density = text.into();
                }
                if !parse_num(text).is_some_and(|v| v > 0.0) {
                    return Err("Rohdichte in kg/m³".into());
                }
            }
            FieldId::MatLambda => {
                if let Some(Popup::NewMat(nm)) = &mut self.popup {
                    nm.lambda = text.into();
                }
                if !text.trim().is_empty() && !parse_num(text).is_some_and(|v| v > 0.0) {
                    return Err("λ in W/(mK) oder leer".into());
                }
            }
        }
        Ok(())
    }

    fn code_taken(&self, code: &str) -> bool {
        self.work
            .layer_sets()
            .iter()
            .any(|(id, t)| Some(id) != self.sel && t.code == code)
    }

    /// Nach einer Dickenänderung: Name und Kurzzeichen ziehen mit, die
    /// Grenzen gleiten (von `from`, gleich viele Schichten), Dicke und U-Wert
    /// leuchten.
    fn thickness_changed(
        &mut self,
        before: f64,
        from: Option<Vec<f64>>,
        ghost: Option<(f64, String)>,
    ) {
        let after = self.draft.thickness();
        if (after - before).abs() < 1e-9 {
            return;
        }
        if let Some(n) = follow_name(&self.draft.name, before, after) {
            self.draft.name = n;
            self.name_hint = true;
            self.flash(Flash::Name);
        }
        // Auch „AW-31,5-2“ (Kopie) zieht mit
        let cat = self.draft.category;
        let old = type_code(cat, before);
        let rest = self.draft.code.strip_prefix(&old);
        if rest.is_some_and(|r| {
            r.is_empty()
                || r.strip_prefix('-')
                    .is_some_and(|n| n.parse::<u32>().is_ok())
        }) {
            self.draft.code = sk_model_free_code(&type_code(cat, after), |c| self.code_taken(c));
        }
        self.anim = from.map(|f| (f, Instant::now()));
        self.ghost = ghost;
        self.flash(Flash::Thick);
        self.flash(Flash::U);
    }

    fn anim_thick(&self, t: &Theme) -> Vec<f64> {
        let to: Vec<f64> = self.draft.layers.iter().map(|l| l.thickness).collect();
        let Some((from, start)) = &self.anim else {
            return to;
        };
        if from.len() != to.len() || t.size.anim_ms <= 0.0 {
            return to;
        }
        let k = (start.elapsed().as_secs_f32() * 1000.0 / t.size.anim_ms).clamp(0.0, 1.0) as f64;
        let e = 1.0 - (1.0 - k).powi(3);
        from.iter().zip(&to).map(|(a, b)| a + (b - a) * e).collect()
    }

    fn flash(&mut self, f: Flash) {
        self.flashes[f as usize] = Some(Instant::now());
    }

    /// Deckkraft des Aufleuchtens (1 → 0).
    fn flash_k(&self, f: Flash, t: &Theme) -> f32 {
        self.flashes[f as usize].map_or(0.0, |at| {
            (1.0 - at.elapsed().as_secs_f32() * 1000.0 / t.size.flash_ms.max(1.0)).max(0.0)
        })
    }

    fn draft_problems(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.draft.name.trim().is_empty() {
            v.push("Name fehlt".into());
        }
        if self.draft.layers.is_empty() {
            v.push("Mindestens eine Schicht anlegen".into());
        }
        v.extend(self.draft.problems());
        if self.code_taken(&self.draft.code) {
            v.push(format!(
                "Kurzzeichen {} ist schon vergeben",
                self.draft.code
            ));
        }
        v
    }

    /// Gültigen Entwurf in die Arbeitskopie schreiben.
    fn commit_draft(&mut self) -> bool {
        let problems = self.draft_problems();
        if let Some(p) = problems.first() {
            self.message = Some(p.clone());
            return false;
        }
        self.message = None;
        match self.sel {
            Some(id) if !self.draft_new => {
                let ok = self.work.set_layer_set(id, self.draft.clone());
                if ok {
                    if let Some(t) = self.work.layer_set(id) {
                        self.draft.changed = t.changed;
                    }
                }
                ok
            }
            _ => match self.work.add_layer_set(self.draft.clone()) {
                Some(id) => {
                    self.sel = Some(id);
                    self.draft_new = false;
                    true
                }
                None => false,
            },
        }
    }

    /// Ein unvollständiger Typ hält alles andere an.
    fn blocked(&mut self) -> bool {
        self.end_edit(true);
        if self.tab != Tab::Project {
            return false;
        }
        match self.draft_problems().first() {
            Some(p) => {
                self.message = Some(p.clone());
                self.blink = Some(Instant::now());
                true
            }
            None => false,
        }
    }

    fn select(&mut self, it: Item) {
        match it {
            Item::Company(g) => {
                self.csel = Some(g);
            }
            Item::Draft => {}
            Item::Type(id) => {
                if self.sel == Some(id) && !self.draft_new {
                    return;
                }
                if self.blocked() {
                    return;
                }
                self.show(id);
            }
        }
    }

    /// Zeigt den Projekttyp `id` (ohne Prüfung des bisherigen Entwurfs).
    fn show(&mut self, id: LayerSetId) {
        if let Some(t) = self.work.layer_set(id) {
            self.sel = Some(id);
            self.draft = t.clone();
            self.draft_new = false;
            self.reset_marks();
        }
    }

    fn reset_marks(&mut self) {
        self.edit = None;
        self.anim = None;
        self.ghost = None;
        self.name_hint = false;
        self.flashes = [None; 3];
        self.message = None;
    }

    fn delete(&mut self) {
        let Some(id) = self.sel else {
            return;
        };
        if self.draft_new {
            // Unvollständiger neuer Typ: einfach verwerfen
            self.draft_new = false;
            let cat = self.draft.category;
            self.sel = Some(self.work.default_type(cat));
            if let Some(t) = self.sel.and_then(|i| self.work.layer_set(i)) {
                self.draft = t.clone();
            }
            self.reset_marks();
            return;
        }
        let cat = self.draft.category;
        if self.work.remove_type(id).is_ok() {
            let next = self.work.default_type(cat);
            self.sel = Some(next);
            if let Some(t) = self.work.layer_set(next) {
                self.draft = t.clone();
            }
            self.reset_marks();
        }
    }

    fn can_delete(&self) -> bool {
        self.draft_new
            || self.sel.is_some_and(|id| {
                !self.work.is_default_type(id) && self.work.type_users(id).is_empty()
            })
    }

    fn add_layer(&mut self) {
        self.end_edit(true);
        let structure = self
            .draft
            .layers
            .iter()
            .any(|l| l.function == LayerFunction::Structure);
        let mat = self
            .work
            .materials()
            .iter()
            .find(|(_, m)| m.category == MatCategory::Masonry)
            .or_else(|| self.work.materials().iter().next())
            .map(|(id, _)| id);
        let Some(material) = mat else {
            return;
        };
        let before = self.draft.thickness();
        // Erste Schicht trägt, weitere sind innen Putz-artig dünn
        let layer = if structure {
            MaterialLayer {
                material,
                thickness: 15.0,
                function: LayerFunction::Finish,
                core: false,
            }
        } else {
            MaterialLayer {
                material,
                thickness: 175.0,
                function: LayerFunction::Structure,
                core: true,
            }
        };
        self.draft.layers.push(layer);
        self.thickness_changed(before, None, None);
        self.commit_draft();
    }

    fn open_combo(&mut self, id: ComboId, cx: &Ctx) {
        let t = cx.theme;
        let anchor = self.combo_rect(t, &cx.win, id);
        let (items, sel) = match id {
            ComboId::Category => {
                if self.category_locked() {
                    return;
                }
                let sel = TypeCategory::ALL
                    .iter()
                    .position(|c| *c == self.draft.category);
                (
                    TypeCategory::ALL
                        .iter()
                        .map(|c| (c.name().to_string(), None))
                        .collect(),
                    sel,
                )
            }
            ComboId::Material(i) => {
                let cur = self.draft.layers.get(i).map(|l| l.material);
                let mut items: Vec<(String, Option<TypeLook>)> = Vec::new();
                let mut sel = None;
                for (k, (mid, m)) in self.work.materials().iter().enumerate() {
                    if Some(mid) == cur {
                        sel = Some(k);
                    }
                    items.push((m.name.clone(), Some(mat_look(&self.work, t, mid))));
                }
                items.push(("Neuer Baustoff …".into(), None));
                (items, sel)
            }
            ComboId::Function(i) => {
                let cur = self.draft.layers.get(i).map(|l| l.function);
                let sel = FUNCTIONS.iter().position(|f| Some(f.0) == cur);
                (
                    FUNCTIONS.iter().map(|f| (f.1.to_string(), None)).collect(),
                    sel,
                )
            }
        };
        self.end_edit(true);
        self.popup = Some(Popup::Combo(Combo {
            id,
            items,
            sel,
            anchor,
        }));
    }

    fn category_locked(&self) -> bool {
        !self.draft_new
            && self.sel.is_some_and(|id| {
                self.work.is_default_type(id) || !self.work.type_users(id).is_empty()
            })
    }

    fn choose(&mut self, i: usize, cx: &Ctx) {
        let Some(Popup::Combo(cb)) = self.popup.take() else {
            return;
        };
        match cb.id {
            ComboId::Category => {
                if let Some(cat) = TypeCategory::ALL.get(i) {
                    if *cat != self.draft.category {
                        let t = self.draft.thickness();
                        if self.draft.code == type_code(self.draft.category, t) {
                            let code = type_code(*cat, t);
                            if !self.code_taken(&code) {
                                self.draft.code = code;
                            }
                        }
                        self.draft.category = *cat;
                        self.commit_draft();
                    }
                }
            }
            ComboId::Material(row) => {
                let mats: Vec<_> = self.work.materials().iter().map(|(id, _)| id).collect();
                match mats.get(i) {
                    Some(mid) => {
                        if let Some(l) = self.draft.layers.get_mut(row) {
                            l.material = *mid;
                            self.commit_draft();
                        }
                    }
                    None => {
                        // „Neuer Baustoff …“
                        let cat = self
                            .draft
                            .layers
                            .get(row)
                            .and_then(|l| self.work.material(l.material))
                            .map_or(MatCategory::Masonry, |m| m.category);
                        self.popup = Some(Popup::NewMat(NewMat {
                            row,
                            name: String::new(),
                            density: String::new(),
                            lambda: String::new(),
                            cat,
                        }));
                        self.begin_edit(FieldId::MatName);
                        let _ = cx;
                    }
                }
            }
            ComboId::Function(row) => {
                if let (Some(f), Some(l)) = (FUNCTIONS.get(i), self.draft.layers.get_mut(row)) {
                    l.function = f.0;
                    // Tragend heißt Kern (IFC LoadBearing)
                    l.core = f.0 == LayerFunction::Structure;
                    self.commit_draft();
                }
            }
        }
    }

    /// „Anlegen“ bzw. „Speichern“ einer Karte.
    fn confirm_popup(&mut self, cx: &mut Ctx, out: &mut Out) {
        self.end_edit(true);
        match self.popup.clone() {
            Some(Popup::NewType { cat, from }) => {
                self.popup = None;
                self.create_type(cat, from);
            }
            Some(Popup::NewMat(nm)) => {
                let (Some(density), lambda) = (parse_num(&nm.density), parse_num(&nm.lambda))
                else {
                    self.begin_edit(FieldId::MatDensity);
                    return;
                };
                if nm.name.trim().is_empty() {
                    self.begin_edit(FieldId::MatName);
                    return;
                }
                let template = self
                    .work
                    .materials()
                    .iter()
                    .find(|(_, m)| m.category == nm.cat)
                    .or_else(|| self.work.materials().iter().next())
                    .map(|(_, m)| m.clone());
                let Some(tpl) = template else {
                    return;
                };
                let guid = self.work.new_guid();
                let id = self.work.add_material(Material {
                    guid,
                    name: nm.name.trim().into(),
                    category: nm.cat,
                    density,
                    lambda: lambda.filter(|v| *v > 0.0),
                    ..tpl
                });
                if let Some(l) = self.draft.layers.get_mut(nm.row) {
                    l.material = id;
                }
                self.popup = None;
                self.commit_draft();
            }
            Some(Popup::Export(g, changed)) => {
                let Some(company) = cx.company.as_deref_mut() else {
                    self.popup = None;
                    return;
                };
                if changed {
                    company.reload(cx.company_standard);
                }
                match company.save_type(&self.work, g) {
                    SaveResult::Saved => {
                        self.popup = None;
                        self.message = Some("In den Firmenkatalog gespeichert".into());
                    }
                    SaveResult::Changed => self.popup = Some(Popup::Export(g, true)),
                    SaveResult::Failed(e) => {
                        self.popup = None;
                        self.message = Some(e);
                    }
                }
                let c: Option<&Company> = cx.company.as_deref();
                self.set_company(c);
            }
            _ => {}
        }
        let _ = out;
    }

    /// Neuer Typ aus einem Typ des Firmenkatalogs oder leer.
    fn create_type(&mut self, cat: TypeCategory, from: Option<Guid>) {
        // Immer der Stand der Firma, als eigener Typ mit neuer Guid (auch
        // wenn das Projekt den Ausgangstyp schon hat)
        let created = match (from, self.company_lib()) {
            (Some(g), Some(lib)) => {
                let mut lib = lib.clone();
                let guid = self.work.new_guid();
                match lib.type_by_guid(g).and_then(|id| lib.types.get_mut(id)) {
                    Some(t) => {
                        t.guid = guid;
                        t.name = format!("{} (Kopie)", t.name);
                        t.changed = 1;
                        import_type(&mut self.work, &lib, guid)
                    }
                    None => None,
                }
            }
            _ => None,
        };
        match created {
            Some(id) => {
                self.tab = Tab::Project;
                self.show(id);
            }
            None => {
                self.draft = empty_type(&mut self.work, cat);
                self.draft_new = true;
                self.sel = None;
                self.tab = Tab::Project;
                self.reset_marks();
                self.message = Some("Mindestens eine Schicht anlegen".into());
            }
        }
    }

    fn import_selected(&mut self) {
        let (Some(g), Some(lib)) = (self.csel, self.company_lib().cloned()) else {
            return;
        };
        match import_type(&mut self.work, &lib, g) {
            Some(id) => {
                self.tab = Tab::Project;
                self.show(id);
                self.flash(Flash::Name);
            }
            None => {
                self.message = Some("Übernehmen nicht möglich (Art eines verbauten Typs)".into());
            }
        }
    }

    // --- OK und Rückfrage -----------------------------------------------------

    /// Verbaute Typen, deren Schichten sich ändern.
    fn asked_types(&self, real: &Model) -> Vec<Guid> {
        let shape = |t: &LayerSet| -> Vec<(f64, Option<Guid>, LayerFunction, bool)> {
            t.layers
                .iter()
                .map(|l| (l.thickness, None, l.function, l.core))
                .collect()
        };
        let mut v = Vec::new();
        for (_, w) in self.work.layer_sets().iter() {
            let Some(rid) = real.type_by_guid(w.guid) else {
                continue;
            };
            let Some(r) = real.layer_set(rid) else {
                continue;
            };
            if real.type_users(rid).is_empty() {
                continue;
            }
            let mats_w: Vec<Option<Guid>> = w
                .layers
                .iter()
                .map(|l| self.work.material(l.material).map(|m| m.guid))
                .collect();
            let mats_r: Vec<Option<Guid>> = r
                .layers
                .iter()
                .map(|l| real.material(l.material).map(|m| m.guid))
                .collect();
            if shape(w) != shape(r) || mats_w != mats_r || w.category != r.category {
                v.push(w.guid);
            }
        }
        v
    }

    fn ok(&mut self, cx: &mut Ctx, out: &mut Out) {
        if self.blocked() {
            return;
        }
        let queue = self.asked_types(cx.scene.model());
        if queue.is_empty() {
            self.apply(cx, &[], out);
            return;
        }
        self.popup = Some(Popup::Ask(Ask {
            queue,
            at: 0,
            answers: Vec::new(),
        }));
        out.highlight = Some(self.ask_users(cx.scene.model()));
    }

    fn ask_users(&self, real: &Model) -> Vec<ElementId> {
        let Some(Popup::Ask(a)) = &self.popup else {
            return Vec::new();
        };
        a.queue
            .get(a.at)
            .and_then(|g| real.type_by_guid(*g))
            .map_or(Vec::new(), |id| real.type_users(id))
    }

    fn answer(&mut self, new: bool, cx: &mut Ctx, out: &mut Out) {
        let Some(Popup::Ask(a)) = &mut self.popup else {
            return;
        };
        let g = a.queue[a.at];
        a.answers.push((g, new));
        a.at += 1;
        if a.at < a.queue.len() {
            out.highlight = Some(self.ask_users(cx.scene.model()));
            return;
        }
        let answers = a.answers.clone();
        self.popup = None;
        out.highlight = Some(Vec::new());
        self.apply(cx, &answers, out);
    }

    /// Name für „Als neuen Typ speichern“: der geänderte Name, sonst „… (neu)“.
    fn new_name(&self, real: &Model, g: Guid) -> String {
        let w = self
            .work
            .type_by_guid(g)
            .and_then(|id| self.work.layer_set(id));
        let r = real.type_by_guid(g).and_then(|id| real.layer_set(id));
        match (w, r) {
            (Some(w), Some(r)) if w.name != r.name => w.name.clone(),
            (Some(w), _) => format!("{} (neu)", w.name),
            _ => String::new(),
        }
    }

    /// Übernimmt die Arbeitskopie als einen Schritt. `answers`: verbaute
    /// Typen, die als neuer Typ gespeichert werden (`true`).
    fn apply(&mut self, cx: &mut Ctx, answers: &[(Guid, bool)], out: &mut Out) {
        let real = cx.scene.model();
        let mut work = self.work.clone();
        // Als neuen Typ: eigene Guid und Kurzzeichen, die Wände behalten den alten
        let mut renamed = Vec::new();
        for (g, new) in answers {
            if !new {
                continue;
            }
            let name = self.new_name(real, *g);
            let Some(id) = work.type_by_guid(*g) else {
                continue;
            };
            let Some(mut t) = work.layer_set(id).cloned() else {
                continue;
            };
            let guid = work.new_guid();
            let wanted = type_code(t.category, t.thickness());
            let code = sk_model_free_code(&wanted, |c| {
                real.type_by_code(c).is_some() || work.type_by_code(c).is_some_and(|x| x != id)
            });
            t.guid = guid;
            t.code = code;
            t.name = name;
            t.changed = 1;
            renamed.push((*g, t));
        }
        let mut lib = Library::from_model(&work);
        for (old, t) in &renamed {
            // Im Katalog ersetzt der neue Typ den geänderten; der alte bleibt im Projekt
            if let Some(id) = lib.type_by_guid(*old) {
                let mats: Vec<_> = lib
                    .types
                    .get(id)
                    .map(|x| x.layers.clone())
                    .unwrap_or_default();
                lib.types.remove(id);
                lib.types.insert(LayerSet {
                    layers: mats,
                    ..t.clone()
                });
            }
            let new_id = lib.type_by_guid(t.guid);
            for d in [&mut lib.default_exterior, &mut lib.default_interior] {
                if d.is_some_and(|id| lib.types.get(id).is_none()) {
                    *d = new_id;
                }
            }
        }
        let keep_old: Vec<Guid> = renamed.iter().map(|x| x.0).collect();
        let work_guids: Vec<Guid> = lib.types.iter().map(|(_, t)| t.guid).collect();
        let defaults: Vec<(TypeCategory, Guid)> = TypeCategory::ALL
            .iter()
            .filter_map(|c| {
                let id = lib.default_type(*c)?;
                Some((*c, lib.types.get(id)?.guid))
            })
            .collect();
        let real_guids: Vec<Guid> = real.layer_sets().iter().map(|(_, t)| t.guid).collect();
        let changed = cx.scene.edit_types("Bauteilkatalog geändert", |m| {
            for g in &work_guids {
                import_type(m, &lib, *g);
            }
            for (cat, g) in &defaults {
                if let Some(id) = m.type_by_guid(*g) {
                    m.set_default_type(*cat, id);
                }
            }
            for g in &real_guids {
                if !work_guids.contains(g) && !keep_old.contains(g) {
                    if let Some(id) = m.type_by_guid(*g) {
                        let _ = m.remove_type(id);
                    }
                }
            }
            true
        });
        out.applied = changed;
        out.closed = true;
    }

    // --- Zeit -----------------------------------------------------------------

    /// Läuft ein Übergang oder ein Aufleuchten?
    fn animating(&self, t: &Theme) -> bool {
        let anim = self
            .anim
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed().as_secs_f32() * 1000.0 < t.size.anim_ms);
        let blink = self
            .blink
            .is_some_and(|at| at.elapsed().as_secs_f32() * 1000.0 < t.size.flash_ms);
        anim || blink
            || (0..3).any(|i| {
                self.flashes[i]
                    .is_some_and(|at| at.elapsed().as_secs_f32() * 1000.0 < t.size.flash_ms)
            })
    }

    pub fn wait(&self, t: &Theme) -> Option<Duration> {
        self.animating(t).then_some(Duration::from_millis(16))
    }

    /// Ein Bild weiter: `true`, wenn neu zu zeichnen ist.
    pub fn tick(&mut self, t: &Theme) -> bool {
        if self.animating(t) {
            return true;
        }
        // Ein letztes Bild ohne Übergang
        let mut done = self.anim.take().is_some() | self.blink.take().is_some();
        for f in &mut self.flashes {
            done |= f.take().is_some();
        }
        done
    }

    pub fn cursor(&self) -> Cursor {
        match (self.drag, self.hover) {
            (Some(Drag::Select(_)), _) | (_, Some(Target::Field(_))) => Cursor::IBeam,
            (Some(Drag::Row(_)), _) | (_, Some(Target::Grip(_))) => Cursor::SizeNS,
            (_, Some(Target::PathLink | Target::AddLayer)) => Cursor::Hand,
            _ => Cursor::Arrow,
        }
    }

    pub fn tip(&self) -> Option<String> {
        match self.hover? {
            Target::Btn(Btn::Delete) if !self.can_delete() => {
                let id = self.sel?;
                let n = self.work.type_users(id).len();
                Some(if n > 0 {
                    format!("Verbaut in {n} Wänden, Löschen erst ohne Verwendung")
                } else {
                    "Standardtyp, erst einen anderen zum Standard machen".into()
                })
            }
            Target::Combo(ComboId::Category) if self.category_locked() => {
                Some("Art gesperrt: Typ ist verbaut oder Standard".into())
            }
            Target::Btn(Btn::Action) if self.tab == Tab::Project && self.company.is_none() => {
                Some("Ohne Einstellungen gibt es keinen Firmenkatalog".into())
            }
            Target::Grip(_) => Some("Ziehen sortiert um".into()),
            _ => None,
        }
    }
}

/// Neuer leerer Typ der Art.
fn empty_type(m: &mut Model, cat: TypeCategory) -> LayerSet {
    let guid = m.new_guid();
    let code = m.free_code(&format!("{}-neu", cat.prefix()));
    LayerSet {
        guid,
        name: format!("Neue {}", cat.name()),
        code,
        category: cat,
        layers: Vec::new(),
        props: Default::default(),
        note: String::new(),
        changed: 1,
    }
}

/// Freies Kurzzeichen gegen eine eigene Prüfung („AW-40“, „AW-40-2“ …).
fn sk_model_free_code(code: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(code) {
        return code.into();
    }
    (2..)
        .map(|i| format!("{code}-{i}"))
        .find(|c| !taken(c))
        .unwrap_or_default()
}

/// Kachel eines Baustoffs (eine Schicht).
fn mat_look(m: &Model, t: &Theme, id: sk_model::MaterialId) -> TypeLook {
    let set = LayerSet {
        guid: Guid(0),
        name: String::new(),
        code: String::new(),
        category: TypeCategory::InteriorWall,
        layers: vec![MaterialLayer {
            material: id,
            thickness: 100.0,
            function: LayerFunction::Structure,
            core: false,
        }],
        props: Default::default(),
        note: String::new(),
        changed: 0,
    };
    type_look(m, t, &set)
}

/// Kategorie-Knopf in der Karte „Neuer Baustoff“.
fn mat_cat_rect(card: Rect, i: usize, s: f32) -> Rect {
    let w = (card.w - 36.0 * s) / MatCategory::ALL.len() as f32;
    Rect::new(
        card.x + 18.0 * s + i as f32 * w,
        card.y + 160.0 * s,
        w - 4.0 * s,
        26.0 * s,
    )
}

// --- Zeichnen -------------------------------------------------------------------

fn label(
    c: &mut Canvas,
    f: Option<&sk_paint::font::Font>,
    t: &str,
    px: f32,
    x: f32,
    y: f32,
    col: Rgba,
) {
    widgets::text(c, f, t, px, x.round(), y.round(), col);
}

fn rounded(c: &mut Canvas, r: Rect, rad: f32, col: Rgba) {
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    c.fill(&p, col);
}

fn outline(c: &mut Canvas, r: Rect, rad: f32, b: f32, col: Rgba) {
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    p.rounded_rect_hole(
        r.x + b,
        r.y + b,
        r.w - 2.0 * b,
        r.h - 2.0 * b,
        (rad - b).max(0.0),
    );
    c.fill(&p, col);
}

fn with_alpha(c: Rgba, a: f32) -> Rgba {
    Rgba(
        c.0,
        c.1,
        c.2,
        (c.3 as f32 * a.clamp(0.0, 1.0)).round() as u8,
    )
}

/// Marke an der Kachel.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mark {
    Standard,
    OnlyProject,
    CompanyNewer,
    ProjectNewer,
    Same,
    NotInProject,
}

impl Mark {
    fn text(self) -> &'static str {
        match self {
            Mark::Standard => "Standard",
            Mark::OnlyProject => "nur im Projekt",
            Mark::CompanyNewer => "Firma neuer",
            Mark::ProjectNewer => "Projekt neuer",
            Mark::Same => "wie im Projekt",
            Mark::NotInProject => "nicht im Projekt",
        }
    }
}

fn pill_font(fonts: &Fonts, mark: Mark) -> Option<&sk_paint::font::Font> {
    let bold = fonts.bold.as_ref().or(fonts.regular.as_ref());
    if mark == Mark::Standard {
        bold
    } else {
        fonts.regular.as_ref()
    }
}

fn pill_w(fonts: &Fonts, mark: Mark, s: f32, t: &Theme) -> f32 {
    let px = t.size.font_detail * s;
    let tw = pill_font(fonts, mark).map_or(40.0, |f| f.width(mark.text(), px));
    (tw + 14.0 * s).round()
}

fn paint_pill(c: &mut Canvas, fonts: &Fonts, right: f32, y: f32, mark: Mark, s: f32, t: &Theme) {
    let u = &t.ui;
    let font = pill_font(fonts, mark);
    let px = t.size.font_detail * s;
    let (w, h) = (pill_w(fonts, mark, s, t), (16.0 * s).round());
    let r = Rect::new((right - w).round(), y.round(), w, h);
    let rad = h * 0.5;
    let (fill, edge, text) = match mark {
        Mark::Standard => (Some(u.accent), u.accent, u.on_accent),
        Mark::OnlyProject | Mark::CompanyNewer | Mark::ProjectNewer => (None, u.accent, u.accent),
        Mark::Same => (None, SAME, SAME),
        Mark::NotInProject => (None, u.text_dim, u.text_dim),
    };
    match fill {
        Some(f) => rounded(c, r, rad, f),
        None => outline(c, r, rad, s.round().max(1.0), edge),
    }
    label(
        c,
        font,
        mark.text(),
        px,
        r.x + 7.0 * s,
        r.y + 12.0 * s,
        text,
    );
}

impl Catalog {
    fn mark_of(&self, it: Item) -> Option<Mark> {
        let lib = self.company_lib();
        match it {
            Item::Type(id) => {
                if self.work.is_default_type(id) {
                    return Some(Mark::Standard);
                }
                let g = self.work.layer_set(id)?.guid;
                (lib.is_some_and(|l| l.type_by_guid(g).is_none())).then_some(Mark::OnlyProject)
            }
            Item::Draft => Some(Mark::OnlyProject),
            Item::Company(g) => {
                let lib = lib?;
                let ct = lib.types.get(lib.type_by_guid(g)?)?;
                let state = compare(&self.work, lib)
                    .into_iter()
                    .find(|x| x.0 == g)
                    .map(|x| x.1);
                Some(match state {
                    Some(TypeState::Same) => Mark::Same,
                    Some(TypeState::Differs) => {
                        let pt = self
                            .work
                            .type_by_guid(g)
                            .and_then(|id| self.work.layer_set(id));
                        if pt.is_some_and(|p| p.changed > ct.changed) {
                            Mark::ProjectNewer
                        } else {
                            Mark::CompanyNewer
                        }
                    }
                    _ => {
                        if lib.default_type(ct.category) == lib.type_by_guid(g) {
                            Mark::Standard
                        } else {
                            Mark::NotInProject
                        }
                    }
                })
            }
        }
    }

    /// Name, Kurzzeichen · Dicke, Verwendung und Aussehen einer Kachel.
    fn tile_info(&self, it: Item, t: &Theme) -> Option<(String, String, usize, TypeLook)> {
        match it {
            Item::Type(id) => {
                // Der gewählte Typ zeigt den Entwurf (auch ungültig)
                let ts = if Some(id) == self.sel && !self.draft_new {
                    &self.draft
                } else {
                    self.work.layer_set(id)?
                };
                Some((
                    ts.name.clone(),
                    format!("{} · {}", ts.code, cm_text(ts.thickness())),
                    self.work.type_users(id).len(),
                    type_look(&self.work, t, ts),
                ))
            }
            Item::Draft => Some((
                self.draft.name.clone(),
                format!("{} · {}", self.draft.code, cm_text(self.draft.thickness())),
                0,
                type_look(&self.work, t, &self.draft),
            )),
            Item::Company(g) => {
                let lib = self.company_lib()?;
                let ct = lib.types.get(lib.type_by_guid(g)?)?;
                Some((
                    ct.name.clone(),
                    format!("{} · {}", ct.code, cm_text(ct.thickness())),
                    0,
                    lib_look(lib, t, ct),
                ))
            }
        }
    }

    fn is_selected(&self, it: Item) -> bool {
        match it {
            Item::Type(id) => self.tab == Tab::Project && !self.draft_new && self.sel == Some(id),
            Item::Draft => self.draft_new,
            Item::Company(g) => self.tab == Tab::Company && self.csel == Some(g),
        }
    }

    pub fn paint(&mut self, t: &Theme, fonts: &Fonts, w: &Win) -> (Canvas, i32, i32) {
        let f = self.frame(t, w);
        let s = w.scale;
        let m = (t.size.panel_shadow * s).round();
        let mut c = Canvas::new((f.w + 2.0 * m) as usize, (f.h + 2.0 * m) as usize);
        // Zeichnen in Fensterkoordinaten
        c.set_origin(f.x - m, f.y - m);
        let u = &t.ui;
        widgets::panel(&mut c, f, s, t);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let line = s.round().max(1.0);
        // Kopf
        label(
            &mut c,
            bold,
            "Bauteilkatalog",
            t.size.font_title * s,
            f.x + 20.0 * s,
            f.y + 40.0 * s,
            u.text,
        );
        let seg = Rect::new(
            self.tab_rect(t, w, Tab::Project).x - 2.0 * s,
            self.tab_rect(t, w, Tab::Project).y - 2.0 * s,
            218.0 * s,
            32.0 * s,
        );
        rounded(&mut c, seg, 7.0 * s, u.field);
        for tab in [Tab::Project, Tab::Company] {
            let r = self.tab_rect(t, w, tab);
            let on = self.tab == tab;
            let hov = self.hover == Some(Target::Tab(tab));
            if on {
                rounded(&mut c, r, 6.0 * s, u.pressed);
                outline(&mut c, r, 6.0 * s, line, u.accent);
            } else if hov {
                rounded(&mut c, r, 6.0 * s, u.hover);
            }
            let text = if tab == Tab::Project {
                "Projekt"
            } else {
                "Firma"
            };
            let font = if on { bold } else { regular };
            let px = t.size.font * s;
            let tw = font.map_or(0.0, |ft| ft.width(text, px));
            label(
                &mut c,
                font,
                text,
                px,
                r.x + (r.w - tw) * 0.5,
                r.y + 19.0 * s,
                if on { u.text } else { u.text_dim },
            );
        }
        if let (Tab::Company, Some((_, path))) = (self.tab, &self.company) {
            let base = self.r(t, w, 436.0, 18.0, 0.0, 28.0);
            let px = t.size.font_small * s;
            let p = widgets::ellipsize(regular, path, px, 360.0 * s);
            label(
                &mut c,
                regular,
                &p,
                px,
                base.x,
                base.y + 19.0 * s,
                u.text_dim,
            );
            if let Some(l) = self.path_link(t, w, fonts) {
                let col = if self.hover == Some(Target::PathLink) {
                    u.accent_hover
                } else {
                    u.accent
                };
                label(
                    &mut c,
                    regular,
                    "ändern …",
                    px,
                    l.x + 2.0 * s,
                    l.y + 19.0 * s,
                    col,
                );
            }
        }
        let cr = self.close_rect(t, w);
        if self.hover == Some(Target::Close) {
            rounded(&mut c, cr, 6.0 * s, u.hover);
        }
        let (cx0, cy0, d) = (cr.x + cr.w * 0.5, cr.y + cr.h * 0.5, 5.5 * s);
        let mut p = Path::new();
        p.segment((cx0 - d, cy0 - d), (cx0 + d, cy0 + d), 1.4 * s);
        p.segment((cx0 - d, cy0 + d), (cx0 + d, cy0 - d), 1.4 * s);
        c.fill(&p, u.text_dim);
        c.fill_rect(f.x, f.y + HEAD * s, f.w, line, u.border);
        c.fill_rect(f.x, f.y + f.h - FOOT * s, f.w, line, u.border);
        let sep_x = f.x + (t.size.catalog_list_w + 12.0) * s;
        c.fill_rect(
            sep_x.round(),
            f.y + HEAD * s,
            line,
            f.h - (HEAD + FOOT) * s,
            u.border,
        );
        self.paint_list(&mut c, t, fonts, w);
        if self.tab == Tab::Project {
            for (b, r, text) in self.list_buttons(t, w) {
                let disabled = b == Btn::Delete && !self.can_delete();
                widgets::button(
                    &mut c,
                    fonts,
                    r,
                    text,
                    self.btn_state(b, false, disabled),
                    s,
                    t,
                );
            }
            self.paint_project(&mut c, t, fonts, w);
        } else {
            self.paint_company(&mut c, t, fonts, w);
        }
        // Fuß
        let no_company = self.company.is_none();
        for (b, r, text) in self.foot_buttons(t, w) {
            let disabled = b == Btn::Action
                && (no_company
                    || (self.tab == Tab::Project && self.draft_new)
                    || (self.tab == Tab::Company && self.csel.is_none()));
            widgets::button(
                &mut c,
                fonts,
                r,
                text,
                self.btn_state(b, b == Btn::Ok, disabled),
                s,
                t,
            );
        }
        let msg = self
            .edit
            .as_ref()
            .and_then(|e| e.invalid.clone())
            .or_else(|| self.message.clone());
        if let Some(msg) = msg {
            let [action, cancel, _] = self.foot_buttons(t, w);
            let x0 = action.1.x + action.1.w + 14.0 * s;
            let max = cancel.1.x - 14.0 * s - x0;
            let px = t.size.font_small * s;
            let text = widgets::ellipsize(regular, &msg, px, max);
            let blink = self
                .blink
                .map_or(0.0, |at| {
                    1.0 - at.elapsed().as_secs_f32() * 1000.0 / t.size.flash_ms.max(1.0)
                })
                .max(0.0);
            if blink > 0.0 {
                let tw = regular.map_or(0.0, |ft| ft.width(&text, px));
                let r = Rect::new(x0 - 6.0 * s, action.1.y + 3.0 * s, tw + 12.0 * s, 24.0 * s);
                rounded(
                    &mut c,
                    r,
                    4.0 * s,
                    with_alpha(u.field_invalid, 0.35 * blink),
                );
            }
            let col = if self.edit.as_ref().is_some_and(|e| e.invalid.is_some())
                || self.draft_problems().first() == Some(&msg)
            {
                u.field_invalid
            } else {
                u.text_dim
            };
            label(&mut c, regular, &text, px, x0, action.1.y + 20.0 * s, col);
        }
        let (x, y) = self.origin(t, w);
        (c, x, y)
    }

    fn btn_state(&self, b: Btn, active: bool, disabled: bool) -> ButtonState {
        ButtonState {
            hover: self.hover == Some(Target::Btn(b)) && !disabled,
            pressed: self.pressed == Some(Target::Btn(b)) && self.hover == Some(Target::Btn(b)),
            active,
            disabled,
        }
    }

    fn paint_list(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let body = self.list_body(t, w);
        let (tiles, heads, _) = self.list_layout(t, w);
        // Eigene Leinwand: was über den Rand ragt, fällt weg
        let body = Rect::new(
            body.x.round(),
            body.y.round(),
            body.w.round(),
            body.h.round(),
        );
        let mut sub = Canvas::new(body.w as usize, body.h as usize);
        sub.set_origin(body.x, body.y);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        for (h, y) in heads {
            label(
                &mut sub,
                bold,
                h,
                t.size.font_detail * s,
                body.x + 18.0 * s,
                y + 16.0 * s,
                u.text_dim,
            );
        }
        for (it, r) in tiles {
            if r.y + r.h < body.y || r.y > body.y + body.h {
                continue;
            }
            let Some((name, detail, uses, look)) = self.tile_info(it, t) else {
                continue;
            };
            let sel = self.is_selected(it);
            if sel {
                rounded(&mut sub, r, 6.0 * s, u.pressed);
                sub.fill_rect(
                    r.x,
                    r.y + 4.0 * s,
                    (3.0 * s).round(),
                    r.h - 8.0 * s,
                    u.accent,
                );
            } else if self.hover == Some(Target::Tile(it)) {
                rounded(&mut sub, r, 6.0 * s, u.hover);
            }
            let (tw, th) = (
                (t.size.catalog_thumb_w * s).round(),
                (t.size.catalog_thumb_h * s).round(),
            );
            let ty = (r.y + (r.h - th) * 0.5).round();
            paint_thumb(
                &mut sub,
                Rect::new((r.x + 10.0 * s).round(), ty, tw, th),
                &look,
                s,
            );
            let tx = r.x + 10.0 * s + tw + 10.0 * s;
            let mark = self.mark_of(it);
            let right = r.x + r.w - 10.0 * s;
            let px = t.size.font * s;
            let font = if sel { bold } else { regular };
            let max = right - tx - if uses > 0 { 30.0 * s } else { 0.0 };
            let name = widgets::ellipsize(font, &name, px, max);
            label(&mut sub, font, &name, px, tx, r.y + 20.0 * s, u.text);
            let pd = t.size.font_detail * s;
            let room = right - tx - mark.map_or(0.0, |mk| pill_w(fonts, mk, s, t) + 6.0 * s);
            let detail = widgets::ellipsize(regular, &detail, pd, room);
            label(
                &mut sub,
                regular,
                &detail,
                pd,
                tx,
                r.y + 38.0 * s,
                u.text_dim,
            );
            if uses > 0 {
                let text = format!("{uses}×");
                let uw = regular.map_or(0.0, |ft| ft.width(&text, pd));
                label(
                    &mut sub,
                    regular,
                    &text,
                    pd,
                    right - uw,
                    r.y + 18.0 * s,
                    u.text_dim,
                );
            }
            if let Some(mk) = mark {
                paint_pill(&mut sub, fonts, right, r.y + 25.0 * s, mk, s, t);
            }
        }
        c.blit(&sub, body.x as i32, body.y as i32);
    }

    fn paint_project(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let small = t.size.font_small * s;
        // Kopf: Name, Kurzzeichen, Art
        for f in [FieldId::Name, FieldId::Code] {
            let Some(r) = self.field_rect(t, w, f) else {
                continue;
            };
            let v = self.field_value(f);
            let st = self.field_state(f, &v);
            widgets::text_field(c, fonts, r, &st, s, t);
            let k = self.flash_k(Flash::Name, t);
            if f == FieldId::Name && k > 0.0 && self.edit.as_ref().is_none_or(|e| e.field != f) {
                rounded(c, r, 4.0 * s, with_alpha(u.accent, 0.25 * k));
            }
        }
        let cr = self.combo_rect(t, w, ComboId::Category);
        let locked = self.category_locked();
        let open = matches!(&self.popup, Some(Popup::Combo(cb)) if cb.id == ComboId::Category);
        widgets::combo(
            c,
            fonts,
            cr,
            self.draft.category.name(),
            !locked && self.hover == Some(Target::Combo(ComboId::Category)),
            open,
            s,
            t,
        );
        if locked {
            rounded(c, cr, 4.0 * s, with_alpha(u.bg, 0.45));
        }
        if self.name_hint {
            let r = self.r(t, w, CONTENT_X + 4.0, 104.0, 0.0, 14.0);
            label(
                c,
                regular,
                "Der Name folgt der Dicke.",
                t.size.font_detail * s,
                r.x,
                r.y + 11.0 * s,
                u.accent,
            );
        }
        // Schnittbild
        let sec = self.section_rect(t, w);
        let look = type_look(&self.work, t, &self.draft);
        let th = self.anim_thick(t);
        let hover_layer = match (self.hover, &self.edit, self.drag) {
            (_, _, Some(Drag::Row(i))) => Some(i),
            (Some(Target::Section(i) | Target::Grip(i) | Target::Remove(i)), ..) => Some(i),
            (Some(Target::Field(FieldId::Thick(i))), ..) => Some(i),
            (Some(Target::Combo(ComboId::Material(i) | ComboId::Function(i))), ..) => Some(i),
            (_, Some(e), _) => match e.field {
                FieldId::Thick(i) => Some(i),
                _ => None,
            },
            _ => None,
        };
        let marks = SectionMarks {
            hover: hover_layer,
            ghost: self.ghost.clone(),
        };
        paint_section(c, regular, sec, &look, &th, &marks, s, t);
        // Kennwerte
        let users = self
            .sel
            .filter(|_| !self.draft_new)
            .map_or(0, |id| self.work.type_users(id).len());
        let u_value = self.work.u_value_of(&self.draft);
        let structure: Vec<String> = self
            .draft
            .layers
            .iter()
            .filter(|l| l.core && l.function == LayerFunction::Structure)
            .map(|l| {
                let name = self
                    .work
                    .material(l.material)
                    .map_or("", |m| m.name.as_str());
                format!("{name} {}", cm_field(l.thickness))
            })
            .collect();
        let rows: [(&str, String, Option<Flash>); 4] = [
            ("Dicke", cm_text(self.draft.thickness()), Some(Flash::Thick)),
            (
                "U-Wert",
                match u_value {
                    Some(v) => format!("≈ {} W/m²K", de(v, 2)),
                    None => "–".into(),
                },
                Some(Flash::U),
            ),
            (
                "Tragend",
                if structure.is_empty() {
                    "nein".into()
                } else {
                    structure.join(", ")
                },
                None,
            ),
            (
                "Verbaut",
                match users {
                    0 => "noch nicht".into(),
                    1 => "1 Wand".into(),
                    n => format!("{n} Wände"),
                },
                None,
            ),
        ];
        for (i, (k, v, fl)) in rows.iter().enumerate() {
            let r = self.r(t, w, RIGHT_X, 124.0 + i as f32 * 30.0, 300.0, 26.0);
            let kf = fl.map_or(0.0, |f| self.flash_k(f, t));
            if kf > 0.0 {
                let vr = Rect::new(r.x + 118.0 * s, r.y, r.w - 118.0 * s, r.h);
                rounded(c, vr, 4.0 * s, with_alpha(u.accent, 0.3 * kf));
            }
            label(c, regular, k, small, r.x, r.y + 18.0 * s, u.text_dim);
            let vw = bold.map_or(0.0, |ft| ft.width(v, small));
            label(
                c,
                bold,
                v,
                small,
                r.x + r.w - 10.0 * s - vw,
                r.y + 18.0 * s,
                u.text,
            );
        }
        // Standard
        let sr = self.standard_rect(t, w);
        let on = self
            .sel
            .is_some_and(|id| !self.draft_new && self.work.is_default_type(id));
        widgets::checkbox(c, sr, on, self.hover == Some(Target::Standard), s, t);
        let text = match self.draft.category {
            TypeCategory::ExteriorWall => "Standard für neue Gebäude",
            TypeCategory::InteriorWall => "Standard für neue Innenwände",
        };
        label(
            c,
            regular,
            text,
            small,
            sr.x + sr.w + 10.0 * s,
            sr.y + 12.5 * s,
            u.text,
        );
        // Schichten
        let hy = self.r(t, w, CONTENT_X, ROWS_Y - 36.0, 0.0, 0.0).y;
        label(
            c,
            bold,
            "Schichten",
            t.size.font * s,
            self.r(t, w, CONTENT_X, 0.0, 0.0, 0.0).x,
            hy,
            u.text,
        );
        let hw = bold.map_or(70.0 * s, |ft| ft.width("Schichten", t.size.font * s));
        let x_after = self.r(t, w, CONTENT_X, 0.0, 0.0, 0.0).x + hw + 10.0 * s;
        let sub = if self.draft.category == TypeCategory::ExteriorWall {
            "von außen nach innen"
        } else {
            "von einer Seite zur anderen"
        };
        label(
            c,
            regular,
            sub,
            t.size.font_detail * s,
            x_after,
            hy,
            u.text_dim,
        );
        let col_y = self.r(t, w, 0.0, ROWS_Y - 12.0, 0.0, 0.0).y;
        for (x, text) in [(330.0, "Baustoff"), (588.0, "Dicke"), (684.0, "Funktion")] {
            let r = self.r(t, w, x, 0.0, 0.0, 0.0);
            label(
                c,
                bold,
                text,
                t.size.font_detail * s,
                r.x,
                col_y,
                u.text_dim,
            );
        }
        for (i, l) in self.draft.layers.iter().enumerate() {
            let row = self.r(
                t,
                w,
                CONTENT_X - 4.0,
                ROWS_Y + i as f32 * ROW_H - 4.0,
                562.0,
                ROW_H,
            );
            let dragging = self.drag == Some(Drag::Row(i));
            if dragging {
                rounded(
                    c,
                    Rect::new(row.x + 2.0 * s, row.y + 3.0 * s, row.w, row.h),
                    6.0 * s,
                    u.shadow,
                );
                rounded(c, row, 6.0 * s, u.pressed);
            } else if hover_layer == Some(i) {
                rounded(c, row, 6.0 * s, u.hover);
            }
            // Griff ⠿
            let g = self.r(
                t,
                w,
                CONTENT_X + 6.0,
                ROWS_Y + i as f32 * ROW_H + 7.0,
                0.0,
                0.0,
            );
            for (dx, dy) in [
                (0.0, 0.0),
                (5.0, 0.0),
                (0.0, 5.0),
                (5.0, 5.0),
                (0.0, 10.0),
                (5.0, 10.0),
            ] {
                let d = (2.0 * s).round().max(1.0);
                rounded(
                    c,
                    Rect::new((g.x + dx * s).round(), (g.y + dy * s).round(), d, d),
                    d * 0.5,
                    u.text_dim,
                );
            }
            let mr = self.combo_rect(t, w, ComboId::Material(i));
            let mat = self.work.material(l.material);
            let icon = mat.map(|_| {
                let look = mat_look(&self.work, t, l.material);
                let (iw, ih) = ((22.0 * s).round(), (14.0 * s).round());
                let mut ic = Canvas::new(iw as usize, ih as usize);
                paint_thumb(&mut ic, Rect::new(0.0, 0.0, iw, ih), &look, s);
                ic
            });
            let open = |id| matches!(&self.popup, Some(Popup::Combo(cb)) if cb.id == id);
            widgets::combo_icon(
                c,
                fonts,
                mr,
                mat.map_or("–", |m| m.name.as_str()),
                icon.as_ref(),
                self.hover == Some(Target::Combo(ComboId::Material(i))),
                open(ComboId::Material(i)),
                s,
                t,
            );
            if let Some(r) = self.field_rect(t, w, FieldId::Thick(i)) {
                let v = self.field_value(FieldId::Thick(i));
                let st = self.field_state(FieldId::Thick(i), &v);
                widgets::field(c, fonts, r, &st, s, t);
            }
            let fr = self.combo_rect(t, w, ComboId::Function(i));
            widgets::combo(
                c,
                fonts,
                fr,
                function_name(l.function),
                self.hover == Some(Target::Combo(ComboId::Function(i))),
                open(ComboId::Function(i)),
                s,
                t,
            );
            let rm = self.r(t, w, 836.0, ROWS_Y + i as f32 * ROW_H, 26.0, 26.0);
            let col = if self.hover == Some(Target::Remove(i)) {
                u.text
            } else {
                u.text_dim
            };
            let (cx0, cy0, d) = (rm.x + rm.w * 0.5, rm.y + rm.h * 0.5, 4.0 * s);
            let mut p = Path::new();
            p.segment((cx0 - d, cy0 - d), (cx0 + d, cy0 + d), 1.2 * s);
            p.segment((cx0 - d, cy0 + d), (cx0 + d, cy0 - d), 1.2 * s);
            c.fill(&p, col);
        }
        let ar = self.add_layer_rect(t, w);
        let col = if self.hover == Some(Target::AddLayer) {
            u.accent_hover
        } else {
            u.accent
        };
        label(
            c,
            bold,
            "+ Schicht",
            t.size.font * s,
            ar.x + 4.0 * s,
            ar.y + 16.0 * s,
            col,
        );
        // Merkmale
        let mr = self.r(t, w, PROPS_X, ROWS_Y - 36.0, 0.0, 0.0);
        label(c, bold, "Merkmale", t.size.font * s, mr.x, mr.y, u.text);
        for (i, key) in TYPE_PROPS.iter().enumerate() {
            let Some(r) = self.field_rect(t, w, FieldId::Prop(i)) else {
                continue;
            };
            label(
                c,
                regular,
                key,
                t.size.font_detail * s,
                r.x,
                r.y - 5.0 * s,
                u.text_dim,
            );
            let v = self.field_value(FieldId::Prop(i));
            let st = self.field_state(FieldId::Prop(i), &v);
            widgets::text_field(c, fonts, r, &st, s, t);
        }
    }

    fn field_state<'a>(&'a self, f: FieldId, value: &'a str) -> FieldState<'a> {
        let e = self.edit.as_ref().filter(|e| e.field == f);
        FieldState {
            text: e.map_or(value, |e| e.text.text.as_str()),
            unit: f.unit(),
            hover: self.hover == Some(Target::Field(f)),
            focus: e.is_some(),
            invalid: e.is_some_and(|e| e.invalid.is_some()),
            caret: e.map(|e| e.text.caret),
            select: e.map(|e| e.text.selection()),
        }
    }

    fn paint_company(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let small = t.size.font_small * s;
        let x0 = self.r(t, w, CONTENT_X, 0.0, 0.0, 0.0).x;
        let Some(lib) = self.company_lib() else {
            let y = self.r(t, w, 0.0, 100.0, 0.0, 0.0).y;
            label(
                c,
                bold,
                "Kein Firmenkatalog",
                t.size.font_title * s,
                x0,
                y,
                u.text,
            );
            let text = "Ohne Einstellungsdatei (Bildschirmfotos, Tests) arbeitet Skizzeo mit dem eingebauten Startbestand.";
            label(c, regular, text, small, x0, y + 26.0 * s, u.text_dim);
            return;
        };
        let Some(g) = self.csel else {
            return;
        };
        let Some(ct) = lib.type_by_guid(g).and_then(|id| lib.types.get(id)) else {
            return;
        };
        let pt = self
            .work
            .type_by_guid(g)
            .and_then(|id| self.work.layer_set(id));
        let y = self.r(t, w, 0.0, 100.0, 0.0, 0.0).y;
        label(c, bold, &ct.name, t.size.font_title * s, x0, y, u.text);
        let users = self
            .work
            .type_by_guid(g)
            .map_or(0, |id| self.work.type_users(id).len());
        let state = match self.mark_of(Item::Company(g)) {
            Some(Mark::Same) => "Gleich wie im Projekt.",
            Some(Mark::CompanyNewer) => "Im Firmenkatalog neuer.",
            Some(Mark::ProjectNewer) => "Im Projekt neuer.",
            _ => "Noch nicht im Projekt.",
        };
        let used = match (pt.is_some(), users) {
            (false, _) => String::new(),
            (true, 0) => " Im Projekt nicht verbaut.".into(),
            (true, n) => format!(" Im Projekt {n}-mal verbaut."),
        };
        label(
            c,
            regular,
            &format!("{state}{used}"),
            small,
            x0,
            y + 24.0 * s,
            u.text_dim,
        );
        // Zwei Karten mit Schnittbild
        let newer = self.mark_of(Item::Company(g)) == Some(Mark::CompanyNewer);
        for (i, title) in ["Im Projekt", "Im Firmenkatalog"].iter().enumerate() {
            let r = self.r(t, w, CONTENT_X + i as f32 * 368.0, 128.0, 356.0, 300.0);
            rounded(c, r, 6.0 * s, u.sheet_bg);
            if i == 1 && newer {
                outline(c, r, 6.0 * s, (2.0 * s).round(), u.accent);
            }
            label(
                c,
                bold,
                title,
                small,
                r.x + 14.0 * s,
                r.y + 22.0 * s,
                u.sheet_text,
            );
            let inner = Rect::new(
                r.x + 6.0 * s,
                r.y + 34.0 * s,
                r.w - 12.0 * s,
                r.h - 40.0 * s,
            );
            let look_t = if i == 0 {
                pt.map(|p| {
                    (
                        type_look(&self.work, t, p),
                        p.layers.iter().map(|l| l.thickness).collect::<Vec<_>>(),
                    )
                })
            } else {
                Some((
                    lib_look(lib, t, ct),
                    ct.layers.iter().map(|l| l.thickness).collect(),
                ))
            };
            match look_t {
                Some((look, th)) => paint_section(
                    c,
                    regular,
                    inner,
                    &look,
                    &th,
                    &SectionMarks::default(),
                    s,
                    t,
                ),
                None => {
                    let text = "nicht im Projekt";
                    let tw = regular.map_or(0.0, |f| f.width(text, small));
                    label(
                        c,
                        regular,
                        text,
                        small,
                        r.x + (r.w - tw) * 0.5,
                        r.y + r.h * 0.5,
                        u.sheet_text_dim,
                    );
                }
            }
        }
        // Unterschiede
        let diffs = differences(&self.work, pt, lib, ct);
        let hy = self.r(t, w, 0.0, 462.0, 0.0, 0.0).y;
        if !diffs.is_empty() {
            label(c, bold, "Unterschied", t.size.font * s, x0, hy, u.text);
        }
        for (i, (k, a, b)) in diffs.iter().take(5).enumerate() {
            let y = hy + (28.0 + i as f32 * 28.0) * s;
            label(c, regular, k, small, x0, y, u.text_dim);
            let ax = self.r(t, w, 560.0, 0.0, 0.0, 0.0).x;
            let aw = regular.map_or(0.0, |f| f.width(a, small));
            label(c, regular, a, small, ax - aw, y, u.text);
            label(c, regular, "→", small, ax + 22.0 * s, y, u.text_dim);
            label(c, bold, b, small, ax + 52.0 * s, y, u.accent);
        }
        let ny = hy + (40.0 + diffs.len().min(5) as f32 * 28.0) * s;
        for (i, l) in [
            "„Ins Projekt übernehmen“ setzt den Projekttyp auf den Stand der Firma.",
            "Bei verbauten Typen fragt Skizzeo vorher, wie beim Ändern.",
        ]
        .iter()
        .enumerate()
        {
            label(
                c,
                regular,
                l,
                small,
                x0,
                ny + i as f32 * 20.0 * s,
                u.text_dim,
            );
        }
    }

    /// Aufklapper und Karten über dem Fenster.
    pub fn paint_popup(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        real: &Model,
    ) -> Option<(Canvas, i32, i32)> {
        let s = w.scale;
        let m = (t.size.panel_shadow * s).round();
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let small = t.size.font_small * s;
        let popup = self.popup.as_ref()?;
        let r = match popup {
            Popup::Combo(cb) => self.combo_rows(cb, w).0,
            Popup::NewType { .. } => self.new_type_card(t, w),
            Popup::NewMat(_) => self.new_mat_card(t, w)?,
            Popup::Export(..) => self.export_card(t, w),
            Popup::Ask(_) => self.ask_card(w),
        };
        let mut c = Canvas::new((r.w + 2.0 * m) as usize, (r.h + 2.0 * m) as usize);
        c.set_origin(r.x - m, r.y - m);
        widgets::panel_filled(&mut c, r, s, t, u.menu_bg);
        if !matches!(popup, Popup::Combo(_)) {
            outline(
                &mut c,
                r,
                t.size.corner_radius * s,
                s.round().max(1.0),
                u.accent,
            );
        }
        match popup {
            Popup::Combo(cb) => {
                let (_, rows) = self.combo_rows(cb, w);
                for (i, (row, (text, look))) in rows.iter().zip(&cb.items).enumerate() {
                    let hot = self.hover == Some(Target::Choice(i))
                        || (self.hover.is_none() && cb.sel == Some(i));
                    if hot {
                        rounded(
                            &mut c,
                            Rect::new(row.x + 4.0 * s, row.y + s, row.w - 8.0 * s, row.h - 2.0 * s),
                            4.0 * s,
                            u.hover,
                        );
                    }
                    let mut x = row.x + t.size.field_pad * s;
                    if let Some(look) = look {
                        let (iw, ih) = ((22.0 * s).round(), (14.0 * s).round());
                        paint_thumb(
                            &mut c,
                            Rect::new(x.round(), (row.y + (row.h - ih) * 0.5).round(), iw, ih),
                            look,
                            s,
                        );
                        x += iw + 8.0 * s;
                    }
                    let col = if look.is_none() && matches!(cb.id, ComboId::Material(_)) {
                        u.accent
                    } else {
                        u.text
                    };
                    label(
                        &mut c,
                        regular,
                        text,
                        small,
                        x,
                        row.y + row.h * 0.5 + 5.0 * s,
                        col,
                    );
                }
            }
            Popup::NewType { cat, from } => {
                label(
                    &mut c,
                    bold,
                    "Neuer Bauteiltyp",
                    t.size.font_title * s,
                    r.x + 18.0 * s,
                    r.y + 30.0 * s,
                    u.text,
                );
                label(
                    &mut c,
                    regular,
                    "Was soll es werden?",
                    small,
                    r.x + 18.0 * s,
                    r.y + 54.0 * s,
                    u.text_dim,
                );
                for (i, kind) in TypeCategory::ALL.into_iter().enumerate() {
                    let b = Rect::new(
                        r.x + (18.0 + i as f32 * 222.0) * s,
                        r.y + 64.0 * s,
                        212.0 * s,
                        70.0 * s,
                    );
                    let on = *cat == kind;
                    rounded(&mut c, b, 6.0 * s, if on { u.pressed } else { u.bg });
                    outline(
                        &mut c,
                        b,
                        6.0 * s,
                        s.round().max(1.0),
                        if on { u.accent } else { u.border },
                    );
                    let look = self.sample_look(kind, t);
                    if let Some(look) = look {
                        paint_thumb(
                            &mut c,
                            Rect::new(b.x + 12.0 * s, b.y + 14.0 * s, 30.0 * s, 42.0 * s),
                            &look,
                            s,
                        );
                    }
                    let (title, sub) = match kind {
                        TypeCategory::ExteriorWall => ("Außenwand", "von außen nach innen"),
                        TypeCategory::InteriorWall => ("Innenwand", "eine oder mehr Schichten"),
                    };
                    label(
                        &mut c,
                        bold,
                        title,
                        t.size.font * s,
                        b.x + 54.0 * s,
                        b.y + 30.0 * s,
                        u.text,
                    );
                    label(
                        &mut c,
                        regular,
                        sub,
                        t.size.font_detail * s,
                        b.x + 54.0 * s,
                        b.y + 50.0 * s,
                        u.text_dim,
                    );
                }
                label(
                    &mut c,
                    regular,
                    "Ausgehen von (Bürostandard)",
                    small,
                    r.x + 18.0 * s,
                    r.y + 164.0 * s,
                    u.text_dim,
                );
                let items = self.new_from_items();
                for i in 0..=items.len() {
                    let row = Rect::new(
                        r.x + 18.0 * s,
                        r.y + (174.0 + i as f32 * 36.0) * s,
                        r.w - 36.0 * s,
                        32.0 * s,
                    );
                    let g = items.get(i).map(|x| x.0);
                    let on = *from == g;
                    if on {
                        rounded(&mut c, row, 6.0 * s, u.pressed);
                        outline(&mut c, row, 6.0 * s, s.round().max(1.0), u.accent);
                    } else if self.hover == Some(Target::NewFrom(g)) {
                        rounded(&mut c, row, 6.0 * s, u.hover);
                    }
                    let font = if on { bold } else { regular };
                    match items.get(i) {
                        Some((g, name, th)) => {
                            if let Some(look) = self.company_lib().and_then(|l| {
                                l.types
                                    .get(l.type_by_guid(*g)?)
                                    .map(|ct| lib_look(l, t, ct))
                            }) {
                                paint_thumb(
                                    &mut c,
                                    Rect::new(
                                        row.x + 10.0 * s,
                                        row.y + 6.0 * s,
                                        20.0 * s,
                                        20.0 * s,
                                    ),
                                    &look,
                                    s,
                                );
                            }
                            label(
                                &mut c,
                                font,
                                name,
                                t.size.font * s,
                                row.x + 40.0 * s,
                                row.y + 21.0 * s,
                                u.text,
                            );
                            let tt = cm_text(*th);
                            let tw = regular.map_or(0.0, |f| f.width(&tt, small));
                            label(
                                &mut c,
                                regular,
                                &tt,
                                small,
                                row.x + row.w - 12.0 * s - tw,
                                row.y + 21.0 * s,
                                u.text_dim,
                            );
                        }
                        None => label(
                            &mut c,
                            font,
                            "leer, Schichten selbst anlegen",
                            t.size.font * s,
                            row.x + 40.0 * s,
                            row.y + 21.0 * s,
                            u.text_dim,
                        ),
                    }
                }
            }
            Popup::NewMat(nm) => {
                label(
                    &mut c,
                    bold,
                    "Neuer Baustoff",
                    t.size.font_title * s,
                    r.x + 18.0 * s,
                    r.y + 30.0 * s,
                    u.text,
                );
                for (f, k) in [
                    (FieldId::MatName, "Name"),
                    (FieldId::MatDensity, "Rohdichte"),
                    (FieldId::MatLambda, "λ (für den U-Wert)"),
                ] {
                    if let Some(fr) = self.field_rect(t, w, f) {
                        label(
                            &mut c,
                            regular,
                            k,
                            small,
                            r.x + 18.0 * s,
                            fr.y + 19.0 * s,
                            u.text_dim,
                        );
                        let v = self.field_value(f);
                        let st = self.field_state(f, &v);
                        if f.numeric() {
                            widgets::field(&mut c, fonts, fr, &st, s, t);
                        } else {
                            widgets::text_field(&mut c, fonts, fr, &st, s, t);
                        }
                    }
                }
                for (i, cat) in MatCategory::ALL.into_iter().enumerate() {
                    let b = mat_cat_rect(r, i, s);
                    let st = ButtonState {
                        hover: self.hover == Some(Target::MatCat(cat)),
                        active: nm.cat == cat,
                        ..ButtonState::default()
                    };
                    widgets::button(&mut c, fonts, b, cat.name(), st, s, t);
                }
                let hint = "Priorität und Darstellung kommen von der Kategorie.";
                label(
                    &mut c,
                    regular,
                    hint,
                    t.size.font_detail * s,
                    r.x + 18.0 * s,
                    r.y + 204.0 * s,
                    u.text_dim,
                );
            }
            Popup::Export(g, changed) => {
                let name = self
                    .work
                    .type_by_guid(*g)
                    .and_then(|id| self.work.layer_set(id))
                    .map_or("", |t| t.name.as_str());
                let (title, body) = if *changed {
                    (
                        "Der Firmenkatalog wurde inzwischen geändert.".to_string(),
                        "Neu laden übernimmt die Änderungen der anderen und speichert den Typ dann zurück.".to_string(),
                    )
                } else {
                    (
                        format!("„{name}“ in den Firmenkatalog speichern?"),
                        format!(
                            "{}. Das lässt sich nicht rückgängig machen.",
                            self.company.as_ref().map_or("", |c| c.1.as_str())
                        ),
                    )
                };
                label(
                    &mut c,
                    bold,
                    &title,
                    t.size.font * s,
                    r.x + 18.0 * s,
                    r.y + 34.0 * s,
                    u.text,
                );
                for (i, l) in widgets::wrap(regular, &body, small, r.w - 36.0 * s)
                    .iter()
                    .enumerate()
                {
                    label(
                        &mut c,
                        regular,
                        l,
                        small,
                        r.x + 18.0 * s,
                        r.y + (62.0 + i as f32 * 20.0) * s,
                        u.text_dim,
                    );
                }
            }
            Popup::Ask(a) => self.paint_ask(&mut c, t, fonts, w, real, a, r),
        }
        for (b, br, text) in self.card_buttons(t, w) {
            match b {
                Btn::AskNew | Btn::AskChange | Btn::AskClose => {}
                _ => {
                    let st = self.btn_state(b, b == Btn::PopOk, false);
                    widgets::button(&mut c, fonts, br, text, st, s, t);
                }
            }
        }
        Some((c, (r.x - m) as i32, (r.y - m) as i32))
    }

    fn sample_look(&self, cat: TypeCategory, t: &Theme) -> Option<TypeLook> {
        let id = self.work.default_type(cat);
        self.work
            .layer_set(id)
            .map(|ts| type_look(&self.work, t, ts))
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_ask(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        real: &Model,
        a: &Ask,
        r: Rect,
    ) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let small = t.size.font_small * s;
        let Some(&g) = a.queue.get(a.at) else {
            return;
        };
        let Some(old) = real.type_by_guid(g).and_then(|id| real.layer_set(id)) else {
            return;
        };
        let Some(new) = self
            .work
            .type_by_guid(g)
            .and_then(|id| self.work.layer_set(id))
        else {
            return;
        };
        let n = real
            .type_by_guid(g)
            .map_or(0, |id| real.type_users(id).len());
        // Kacheln alt → neu
        let (tw, th) = ((36.0 * s).round(), (58.0 * s).round());
        paint_thumb(
            c,
            Rect::new(r.x + 22.0 * s, r.y + 26.0 * s, tw, th),
            &type_look(real, t, old),
            s,
        );
        label(
            c,
            regular,
            "→",
            small,
            r.x + 62.0 * s,
            r.y + 60.0 * s,
            u.text_dim,
        );
        paint_thumb(
            c,
            Rect::new(r.x + 80.0 * s, r.y + 26.0 * s, tw, th),
            &type_look(&self.work, t, new),
            s,
        );
        let x = r.x + 138.0 * s;
        label(
            c,
            bold,
            &format!("{} ist {n}-mal verbaut.", old.name),
            t.size.font * s,
            x,
            r.y + 42.0 * s,
            u.text,
        );
        let d = new.thickness() - old.thickness();
        let how = if d.abs() < 1e-9 {
            "Die Dicke bleibt.".to_string()
        } else if old.is_external() {
            format!(
                "Sie bleiben außen stehen und werden nach innen {} cm {}.",
                cm_field(d.abs()),
                if d > 0.0 { "dicker" } else { "dünner" }
            )
        } else {
            format!(
                "Sie werden {} cm {}.",
                cm_field(d.abs()),
                if d > 0.0 { "dicker" } else { "dünner" }
            )
        };
        let body = format!("Die markierten Wände ändern sich mit. {how}");
        for (i, l) in widgets::wrap(regular, &body, small, r.x + r.w - 30.0 * s - x)
            .iter()
            .enumerate()
        {
            label(
                c,
                regular,
                l,
                small,
                x,
                r.y + (66.0 + i as f32 * 18.0) * s,
                u.text_dim,
            );
        }
        if a.queue.len() > 1 {
            let text = format!("{} von {}", a.at + 1, a.queue.len());
            label(
                c,
                regular,
                &text,
                t.size.font_detail * s,
                r.x + r.w - 110.0 * s,
                r.y + 27.0 * s,
                u.text_dim,
            );
        }
        let new_name = self.new_name(real, g);
        for (b, br, text) in self.card_buttons(t, w) {
            match b {
                Btn::AskNew | Btn::AskChange => {
                    let accent = b == Btn::AskChange;
                    let hov = self.hover == Some(Target::Btn(b));
                    let (fill, fg, sub) = if accent {
                        (
                            if hov { u.accent_hover } else { u.accent },
                            u.on_accent,
                            u.on_accent,
                        )
                    } else {
                        (if hov { u.hover } else { u.bg }, u.text, u.text_dim)
                    };
                    rounded(c, br, 6.0 * s, fill);
                    if !accent {
                        outline(c, br, 6.0 * s, s.round().max(1.0), u.border);
                    }
                    label(
                        c,
                        bold,
                        text,
                        t.size.font * s,
                        br.x + 16.0 * s,
                        br.y + 28.0 * s,
                        fg,
                    );
                    let detail = if accent {
                        format!("alle {n} Wände bekommen {}", cm_text(new.thickness()))
                    } else {
                        format!("„{new_name}“, Wände bleiben")
                    };
                    let detail = widgets::ellipsize(
                        regular,
                        &detail,
                        t.size.font_detail * s,
                        br.w - 32.0 * s,
                    );
                    label(
                        c,
                        regular,
                        &detail,
                        t.size.font_detail * s,
                        br.x + 16.0 * s,
                        br.y + 50.0 * s,
                        sub,
                    );
                }
                Btn::AskClose => {
                    if self.hover == Some(Target::Btn(b)) {
                        rounded(c, br, 6.0 * s, u.hover);
                    }
                    let (cx0, cy0, d) = (br.x + br.w * 0.5, br.y + br.h * 0.5, 4.5 * s);
                    let mut p = Path::new();
                    p.segment((cx0 - d, cy0 - d), (cx0 + d, cy0 + d), 1.3 * s);
                    p.segment((cx0 - d, cy0 + d), (cx0 + d, cy0 - d), 1.3 * s);
                    c.fill(&p, u.text_dim);
                }
                _ => {}
            }
        }
        let foot = "Beides ist ein Rückgängig-Schritt. × kehrt zum Bearbeiten zurück.";
        let fw = regular.map_or(0.0, |f| f.width(foot, t.size.font_detail * s));
        label(
            c,
            regular,
            foot,
            t.size.font_detail * s,
            r.x + (r.w - fw) * 0.5,
            r.y + r.h - 20.0 * s,
            u.text_dim,
        );
    }
}

/// Aussehen eines Katalogtyps (Baustoffe aus dem Katalog, Darstellung über
/// ein Modell mit dessen Stiften und Schraffuren).
fn lib_look(lib: &Library, t: &Theme, ct: &LayerSet) -> TypeLook {
    let mut m = Model::new();
    m.allow_unstepped();
    let g = ct.guid;
    match import_type(&mut m, lib, g).and_then(|id| m.layer_set(id).cloned()) {
        Some(ts) => type_look(&m, t, &ts),
        None => type_look(&m, t, ct),
    }
}

/// Unterschiede Projekt ↔ Firma: Bezeichnung, vorher, nachher.
fn differences(
    m: &Model,
    pt: Option<&LayerSet>,
    lib: &Library,
    ct: &LayerSet,
) -> Vec<(String, String, String)> {
    let Some(pt) = pt else {
        return Vec::new();
    };
    let mut v = Vec::new();
    if pt.name != ct.name {
        v.push(("Name".into(), pt.name.clone(), ct.name.clone()));
    }
    if pt.code != ct.code {
        v.push(("Kurzzeichen".into(), pt.code.clone(), ct.code.clone()));
    }
    let n = pt.layers.len().max(ct.layers.len());
    for i in 0..n {
        let a = pt.layers.get(i);
        let b = ct.layers.get(i);
        let an = a
            .and_then(|l| m.material(l.material))
            .map(|x| x.name.clone());
        let bn = b
            .and_then(|l| lib.materials.get(l.material))
            .map(|x| x.name.clone());
        let key = format!("Schicht {}", i + 1);
        match (a, b) {
            (Some(a), Some(b)) => {
                if an != bn {
                    v.push((
                        key.clone(),
                        an.clone().unwrap_or_default(),
                        bn.clone().unwrap_or_default(),
                    ));
                }
                if (a.thickness - b.thickness).abs() > 1e-9 {
                    let k = format!("{} ({})", key, bn.clone().unwrap_or_default());
                    v.push((k, cm_text(a.thickness), cm_text(b.thickness)));
                }
                if a.function != b.function {
                    v.push((
                        key,
                        function_name(a.function).into(),
                        function_name(b.function).into(),
                    ));
                }
            }
            (Some(a), None) => v.push((
                key,
                format!("{} {}", an.unwrap_or_default(), cm_text(a.thickness)),
                "–".into(),
            )),
            (None, Some(b)) => v.push((
                key,
                "–".into(),
                format!("{} {}", bn.unwrap_or_default(), cm_text(b.thickness)),
            )),
            _ => {}
        }
    }
    if (pt.thickness() - ct.thickness()).abs() > 1e-9 {
        v.push((
            "Dicke".into(),
            cm_text(pt.thickness()),
            cm_text(ct.thickness()),
        ));
    }
    for k in TYPE_PROPS {
        let a = pt.props.get(k).map(prop_text).unwrap_or_default();
        let b = ct.props.get(k).map(prop_text).unwrap_or_default();
        if a != b {
            v.push((
                k.into(),
                if a.is_empty() { "–".into() } else { a },
                if b.is_empty() { "–".into() } else { b },
            ));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_folgt_der_dicke() {
        assert_eq!(
            follow_name("AW mit WDVS 36", 360.0, 400.0).as_deref(),
            Some("AW mit WDVS 40")
        );
        assert_eq!(
            follow_name("AW monolithisch 36,5", 365.0, 425.0).as_deref(),
            Some("AW monolithisch 42,5")
        );
        assert_eq!(follow_name("AW 136", 36.0, 40.0), None);
        assert_eq!(follow_name("Wand", 360.0, 400.0), None);
        assert_eq!(
            follow_name("AW 31,5 Gasbeton + WDVS", 315.0, 335.0).as_deref(),
            Some("AW 33,5 Gasbeton + WDVS")
        );
        assert_eq!(parse_thick("16"), Ok(160.0));
        // Kurzzeichen einer Kopie zieht mit
        let mut c = Catalog::open(&szene(), None);
        let id = c.work.duplicate_type(c.sel.unwrap()).unwrap();
        c.show(id);
        assert_eq!(c.draft.code, "AW-31,5-2");
        c.apply_value(FieldId::Thick(0), "20").unwrap();
        assert_eq!(c.draft.code, "AW-37,5");
        assert_eq!(parse_thick("12,3"), Ok(125.0));
        assert!(parse_thick("0").is_err());
        assert!(parse_thick("101").is_err());
    }

    use sk_math::vec3;
    use sk_model::{RefSide, WallChain};

    fn szene() -> Scene {
        let mut s = Scene::with_model(Model::with_seed(7));
        s.add_wall(&WallChain {
            base: 0.0,
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 4000.0, 0.0),
                vec3(5000.0, 4000.0, 0.0),
                vec3(5000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            layers: Vec::new(),
            height: 3500.0,
            joints: Default::default(),
        })
        .unwrap();
        s
    }

    fn fonts() -> Fonts {
        Fonts {
            regular: None,
            bold: None,
            italic: None,
        }
    }

    const WIN: Win = Win {
        w: 1280,
        h: 800,
        top: 32,
        scale: 1.0,
    };

    fn aussen(m: &Model) -> Vec<&LayerSet> {
        m.layer_sets()
            .iter()
            .filter(|(_, t)| t.category == TypeCategory::ExteriorWall)
            .map(|(_, t)| t)
            .collect()
    }

    /// Dicke eines verbauten Typs ändern: Rückfrage mit den Wänden, „Ändern“
    /// wirkt auf alle, ein Rückgängig-Schritt; Name und Kurzzeichen folgen.
    #[test]
    fn aendern_fragt_und_ist_ein_schritt() {
        let mut s = szene();
        let theme = Theme::dark();
        let f = fonts();
        let mut c = Catalog::open(&s, None);
        assert_eq!(c.apply_value(FieldId::Thick(0), "16"), Ok(()));
        assert_eq!(c.draft.code, "AW-33,5");
        assert_eq!(c.draft.name, "AW 33,5 Gasbeton + WDVS");
        let mut cx = Ctx {
            scene: &mut s,
            theme: &theme,
            fonts: &f,
            win: WIN,
            company: None,
            company_standard: false,
        };
        let mut out = Out::default();
        c.ok(&mut cx, &mut out);
        assert!(c.asking());
        assert!(
            !out.highlight.as_ref().unwrap().is_empty(),
            "Wände markiert"
        );
        assert!(!out.applied);
        let mut out = Out::default();
        c.answer(false, &mut cx, &mut out);
        assert!(out.applied && out.closed);
        assert_eq!(out.highlight, Some(Vec::new()));
        let t = aussen(s.model());
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].thickness(), 335.0);
        assert_eq!(t[0].code, "AW-33,5");
        assert_eq!(s.undo_label(), Some("Bauteilkatalog geändert"));
        s.undo();
        assert_eq!(aussen(s.model())[0].thickness(), 315.0);
    }

    /// „Als neuen Typ speichern“: die Wände behalten den alten Typ, der neue
    /// bekommt Name und Kurzzeichen der neuen Dicke.
    #[test]
    fn als_neuen_typ_speichern() {
        let mut s = szene();
        let theme = Theme::dark();
        let f = fonts();
        let old = s.model().default_type(TypeCategory::ExteriorWall);
        let users = s.model().type_users(old).len();
        let mut c = Catalog::open(&s, None);
        c.apply_value(FieldId::Thick(0), "16").unwrap();
        let mut cx = Ctx {
            scene: &mut s,
            theme: &theme,
            fonts: &f,
            win: WIN,
            company: None,
            company_standard: false,
        };
        let mut out = Out::default();
        c.ok(&mut cx, &mut out);
        c.answer(true, &mut cx, &mut out);
        assert!(out.applied);
        let m = s.model();
        let t = aussen(m);
        assert_eq!(t.len(), 2);
        let alt = m.layer_set(old).unwrap();
        assert_eq!((alt.thickness(), alt.code.as_str()), (315.0, "AW-31,5"));
        assert_eq!(m.type_users(old).len(), users, "Wände bleiben");
        let neu = t.iter().find(|x| x.guid != alt.guid).unwrap();
        assert_eq!((neu.thickness(), neu.code.as_str()), (335.0, "AW-33,5"));
        assert_eq!(neu.name, "AW 33,5 Gasbeton + WDVS");
    }

    /// Was eine Arbeitskopie vergibt, vergibt die nächste nicht noch einmal
    /// (zweimal öffnen, je duplizieren).
    #[test]
    fn guids_der_arbeitskopie_sind_neu() {
        let mut s = szene();
        let theme = Theme::dark();
        let f = fonts();
        for n in 2..4 {
            let mut c = Catalog::open(&s, None);
            let id = c.work.duplicate_type(c.sel.unwrap()).expect("Kopie");
            c.show(id);
            let mut cx = Ctx {
                scene: &mut s,
                theme: &theme,
                fonts: &f,
                win: WIN,
                company: None,
                company_standard: false,
            };
            let mut out = Out::default();
            c.ok(&mut cx, &mut out);
            assert_eq!(aussen(s.model()).len(), n);
        }
    }

    /// „Neu“ aus dem Firmenkatalog: ein eigener Typ mit freiem Kurzzeichen,
    /// sofort weiter bearbeitbar; OK legt ihn ohne Rückfrage an.
    #[test]
    fn neu_aus_firmenkatalog() {
        let d = std::env::temp_dir().join("skizzeo-katalog-neu");
        let _ = std::fs::remove_dir_all(&d);
        let (mut company, hints) = Company::load(&d.join("firmenkatalog.szk"), true);
        assert!(hints.is_empty(), "{hints:?}");
        let mut s = szene();
        let theme = Theme::dark();
        let f = fonts();
        let mut c = Catalog::open(&s, Some(&company));
        let lib = company.library();
        let g = lib
            .types
            .get(lib.default_type(TypeCategory::ExteriorWall).unwrap())
            .unwrap()
            .guid;
        // Der Projekttyp mit derselben Guid weicht ab: es gilt der Stand der Firma
        c.apply_value(FieldId::Thick(0), "16").unwrap();
        c.create_type(TypeCategory::ExteriorWall, Some(g));
        assert!(c.draft_problems().is_empty(), "{:?}", c.draft_problems());
        assert_eq!(c.draft.thickness(), 315.0);
        assert_ne!(c.draft.guid, g);
        assert_eq!(c.draft.code, "AW-31,5");
        assert!(!c.blocked());
        let mut cx = Ctx {
            scene: &mut s,
            theme: &theme,
            fonts: &f,
            win: WIN,
            company: Some(&mut company),
            company_standard: true,
        };
        let mut out = Out::default();
        c.ok(&mut cx, &mut out);
        // Der geänderte verbaute Typ fragt nach
        assert!(c.asking());
        c.answer(false, &mut cx, &mut out);
        assert!(out.applied);
        assert_eq!(aussen(s.model()).len(), 2);
        let _ = std::fs::remove_dir_all(&d);
    }
}

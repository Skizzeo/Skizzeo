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
    cm_text, paint_section, paint_thumb, section_layer_at, type_look, Patterns, SectionMarks,
    TypeLook,
};
use crate::window_kit::{intersect, label, outline, parse_num, pixel_strips, prop_text, rounded};
use sk_model::{
    compare, import_type, type_code, ElementId, Guid, LayerFunction, LayerSet, LayerSetId, Library,
    MatCategory, Material, MaterialLayer, Model, PropValue, TypeCategory, TypeState, TYPE_PROPS,
};
use sk_paint::{font::Font, Canvas, Path, Rgba};
use sk_platform::{Cursor, Event, Key, Modifiers, MouseButton};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, FieldState, Fonts, Rect};
use std::cell::RefCell;
use std::time::{Duration, Instant};

/// Kopf, Fuß, Rand (dip).
mod ext;
mod raeume;

const HEAD: f32 = 64.0;
const FOOT: f32 = 56.0;
const PAD: f32 = 16.0;
/// Kleinste Fenstergröße (dip).
const MIN_W: f32 = 900.0;
const MIN_H: f32 = 600.0;
/// Inhalt rechts: linke Kante (ab Fenster) und rechte Spalte.
const CONTENT_X: f32 = 302.0;
/// Ort des Firmenkatalogs im Kopf, rechts der vier Reiter.
const PATH_X: f32 = 676.0;
const RIGHT_X: f32 = 752.0;
const PROPS_X: f32 = 880.0;
/// Schichtzeilen.
const ROW_H: f32 = 34.0;
const ROWS_Y: f32 = 442.0;
/// Grün für „wie im Projekt“ (Marke im Firmenreiter).
/// Funktionen in der Auswahl.
const FUNCTIONS: [(LayerFunction, &str); 5] = [
    (LayerFunction::Structure, "tragend"),
    (LayerFunction::Insulation, "Dämmung"),
    (LayerFunction::Finish, "Bekleidung"),
    (LayerFunction::Membrane, "Abdichtung"),
    (LayerFunction::AirGap, "Luftschicht"),
];

/// Kategorien für einen neuen Baustoff; Luft gibt es nur einmal, fest an
/// der Luftschicht (K4).
const NEW_MAT_CATS: [MatCategory; 5] = [
    MatCategory::Masonry,
    MatCategory::Concrete,
    MatCategory::Insulation,
    MatCategory::Plaster,
    MatCategory::Timber,
];

fn function_name(f: LayerFunction) -> &'static str {
    FUNCTIONS.iter().find(|x| x.0 == f).map_or("", |x| x.1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Project,
    Company,
    /// Raumvorlagen (Raumbuch RB2).
    Rooms,
    /// Eingelesene Bauteile (E6).
    Ext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldId {
    Name,
    Code,
    Thick(usize),
    Prop(usize),
    /// Auflagertiefe bei „fest, Rest Randstreifen“.
    Depth,
    MatName,
    MatDensity,
    MatLambda,
    /// Reiter „Räume“: Name der Vorlage, Höhe der Wandfliesen.
    RoomName,
    TilesHeight,
}

impl FieldId {
    fn numeric(self) -> bool {
        matches!(
            self,
            FieldId::Thick(_)
                | FieldId::Depth
                | FieldId::MatDensity
                | FieldId::MatLambda
                | FieldId::TilesHeight
        )
    }

    fn unit(self) -> &'static str {
        match self {
            FieldId::Thick(_) | FieldId::Depth => "cm",
            FieldId::MatDensity => "kg/m³",
            FieldId::MatLambda => "W/mK",
            FieldId::TilesHeight => "m",
            _ => "",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComboId {
    Category,
    Material(usize),
    Function(usize),
    /// Baustoff des Randdämmstreifens.
    Strip,
    /// Rechtsklick auf eine Schicht: „Baustoff bearbeiten …“ (Paket 5).
    Edit(usize),
    /// Reiter „Räume“: Nutzungsgruppe, Wohnfläche, Ausbautyp je Zeile.
    RoomGroup,
    RoomWoflv,
    RoomSlot(usize),
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
    /// Reiter „Erweiterungen“: Bauteil setzen.
    Einfuegen,
}

/// Eintrag der Liste links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Type(LayerSetId),
    /// Neuer, noch unvollständiger Typ.
    Draft,
    Company(Guid),
    /// Bauteil `i` im Reiter „Erweiterungen“.
    Ext(usize),
    /// Raumvorlage im Reiter „Räume“.
    Room(Guid),
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
    /// Deckenauflager: ganze tragende Schicht (`false`) oder fest, Rest
    /// Randstreifen (`true`).
    Bearing(bool),
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
    /// Fenster „Baustoffe …“ mit diesem Baustoff öffnen (der Katalog ist
    /// ohne Änderungen geschlossen).
    pub open_material: Option<Guid>,
    /// „Einfügen“ im Reiter „Erweiterungen“: Werkzeug mit dem Bauteil
    /// dieses Schlüssels (der Katalog ist ohne Änderungen geschlossen).
    pub ext_einfuegen: Option<String>,
    /// „Bauteil einlesen …“ im Reiter „Erweiterungen“ (E5): Datei wählen,
    /// das Fenster „Erweiterungen“ liegt dann über dem Katalog.
    pub ext_einlesen: bool,
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
#[derive(Clone, Copy, Debug, PartialEq)]
enum Flash {
    Name = 0,
    Thick = 1,
    U = 2,
}

/// Hinweis am gesperrten „In den Firmenkatalog …“ (KA-3b1).
pub(crate) const NUR_LESBAR: &str =
    "Nur lesbar: In den Firmenkatalog speichert erst, wer unter „Verwaltung …“ das Kennwort eingibt";

pub struct Catalog {
    work: Model,
    pub tab: Tab,
    /// Verwaltungskennwort gesetzt und hier nicht eingegeben (KA-3b1): die
    /// Firma nur lesen, nicht hineinspeichern.
    nutzer: bool,
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
    /// Was sich seit dem letzten Bild geändert hat (U7); leer heißt: alles
    /// neu, wenn die App ein Bild verlangt.
    damage: Vec<Area>,
    /// Letztes ganzes Bild; Teilbilder erneuern es stellenweise.
    img: Option<Canvas>,
    /// Größe, Skalierung, Schema und Lage, für die `img` gilt.
    img_key: Option<(usize, usize, u32, u64, i32, i32)>,
    /// Bytes des letzten ganzen Bildes nach dem Hochladen zurück
    /// ([`Catalog::give_back`]): ihr Speicher dient dem nächsten.
    bytes: Vec<u8>,
    /// Leinwand für Teilbilder, behält ihren Speicher.
    scratch: Canvas,
    /// Leinwand der Liste (Kacheln mit Bildlauf), behält ihren Speicher.
    list_sub: std::cell::RefCell<Canvas>,
    /// Schraffuren des Schnittbilds (Übergang ohne Rechnen je Bildpunkt).
    patterns: RefCell<Patterns>,
    /// Kachelflug (K3b): Kopie der Kachel fliegt zum Reiter; Beginn.
    fly: Option<(Item, Tab, Instant)>,
    /// Bild der fliegenden Kopie und wo sie zuletzt gezeichnet wurde.
    fly_img: Option<Canvas>,
    fly_drawn: Option<Rect>,
    /// Reiter, der nach Übernehmen oder Speichern einmal pulst.
    pulse: Option<(Tab, Instant)>,
    /// Stand der Arbeitskopie beim Öffnen: gleich heißt „nichts geändert“.
    opened: u64,
    /// Gewählt: „Baustoff bearbeiten …“ (an [`Out::open_material`]).
    edit_material: Option<Guid>,
    /// Marke einer Kachel, die überblendet: vorige Marke und Beginn.
    fade: Option<(Item, Option<Mark>, Instant)>,
    /// Zeitpunkt des Bildes, das gerade entsteht: alle Teilbilder zeigen
    /// denselben Stand der Übergänge.
    paint_now: Option<Instant>,
    /// Reiter „Erweiterungen“: Bauteile und das gewählte.
    ext: Vec<ext::Eintrag>,
    ext_sel: Option<usize>,
    /// Reiter „Räume“: gewählte Raumvorlage (Projekt, Firma oder Werk).
    room_sel: Option<Guid>,
    /// Feste Uhr für Tests: alle Bilder zeigen denselben Stand.
    #[cfg(test)]
    test_clock: Option<Instant>,
}

/// Dauer der Überblendung einer Marke (ms).
const FADE_MS: f32 = 150.0;

/// Bereich des Fensters, der neu gemalt werden muss.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Area {
    Full,
    /// Was ein Ziel unter der Maus hervorhebt (vorher und nachher).
    Hover(Option<Target>),
    /// Schnittbild (Übergang der Schichtgrenzen).
    Section,
    Flash(Flash),
    /// Meldung im Fuß (Aufleuchten).
    Foot,
    /// Fliegende Kachel (vorige und jetzige Lage).
    Fly,
    Pulse(Tab),
    /// Kachel, deren Marke überblendet.
    Tile(Item),
}

/// Was die App hochladen muss: das ganze Bild oder Ausschnitte daraus
/// (vormultipliziertes RGBA8, Lage im Bild).
pub enum Frame {
    Full {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        px: Vec<u8>,
    },
    Parts(Vec<(i32, i32, u32, u32, Vec<u8>)>),
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

/// Dicke aus dem Feld (cm, 0,5 bis 100, Schritt 0,5) in mm.
fn parse_thick(t: &str) -> Result<f64, String> {
    match parse_num(t) {
        Some(cm) if (0.5..=100.0).contains(&cm) => Ok((cm * 2.0).round() * 5.0),
        _ => Err("Dicke 0,5 bis 100 cm".into()),
    }
}

/// Auflagertiefe in cm, Schritte 0,5 cm, im Bereich des Typs (Regel 21);
/// rundet und klemmt nicht.
fn parse_depth(t: &str, range: Option<(f64, f64)>) -> Result<f64, String> {
    let Some((lo, hi)) = range else {
        return Err("Hier kein festes Auflager möglich".into());
    };
    let msg = || format!("Auflagertiefe {} bis {} cm", cm_field(lo), cm_field(hi));
    let cm = parse_num(t).ok_or_else(msg)?;
    if ((cm * 2.0) - (cm * 2.0).round()).abs() > 1e-6 {
        return Err("Auflagertiefe in Schritten von 0,5 cm".into());
    }
    let mm = (cm * 2.0).round() * 5.0;
    if mm < lo - 1e-6 || mm > hi + 1e-6 {
        return Err(msg());
    }
    Ok(mm)
}

/// Steht die alte Dicke als eigenes Wort im Namen, zieht sie mit
/// („AW mit WDVS 36“ → „… 40“, „AW 31,5 Porenbeton“ → „AW 33,5 Porenbeton“).
fn follow_name(name: &str, old: f64, new: f64) -> Option<String> {
    let (o, n) = (cm_field(old), cm_field(new));
    let word = |c: Option<char>| c.is_some_and(|c| c.is_ascii_digit() || c == ',');
    let at = name.rmatch_indices(&o).map(|(i, _)| i).find(|&i| {
        !word(name[..i].chars().next_back()) && !word(name[i + o.len()..].chars().next())
    })?;
    Some(format!("{}{n}{}", &name[..at], &name[at + o.len()..]))
}

/// Kacheln, Gruppenköpfe (Text, y) und Gesamthöhe der Liste.
type ListLayout = (Vec<(Item, Rect)>, Vec<(String, f32)>, f32);

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
            nutzer: false,
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
            damage: Vec::new(),
            img: None,
            img_key: None,
            bytes: Vec::new(),
            scratch: Canvas::new(0, 0),
            list_sub: std::cell::RefCell::new(Canvas::new(0, 0)),
            patterns: RefCell::default(),
            fly: None,
            fly_img: None,
            fly_drawn: None,
            pulse: None,
            opened: 0,
            edit_material: None,
            fade: None,
            paint_now: None,
            ext: Vec::new(),
            ext_sel: None,
            room_sel: None,
            #[cfg(test)]
            test_clock: None,
        };
        c.set_company(company);
        c.room_sel = c.raum_eintraege().first().map(|e| e.0);
        c.opened = c.work.revision();
        c
    }

    /// Hat sich seit dem Öffnen etwas geändert?
    fn pending(&self) -> bool {
        self.draft_new || self.work.revision() != self.opened
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
        // „Erweiterungen“ braucht mehr Platz als „Projekt“ und „Firma“
        let (i, ww) = match tab {
            Tab::Project => (0.0, 106.0),
            Tab::Company => (1.0, 106.0),
            Tab::Rooms => (2.0, 106.0),
            Tab::Ext => (3.0, 130.0),
        };
        self.r(t, w, 204.0 + i * 108.0, 18.0, ww, 28.0)
    }

    /// Platz für den Ort des Firmenkatalogs bis vor „ändern …“ und das
    /// Schließen, höchstens 360 dip.
    fn path_room(&self, t: &Theme, w: &Win) -> f32 {
        let fw = self.frame(t, w).w / w.scale;
        (fw - PATH_X - 46.0 - 90.0).clamp(80.0, 360.0) * w.scale
    }

    fn path_link(&self, t: &Theme, w: &Win, fonts: &Fonts) -> Option<Rect> {
        if self.tab != Tab::Company {
            return None;
        }
        let path = &self.company.as_ref()?.1;
        let s = w.scale;
        let px = t.size.font_small * s;
        let pw = fonts.regular.as_ref().map_or(0.0, |f| f.width(path, px));
        let pw = pw.min(self.path_room(t, w));
        let lw = fonts
            .regular
            .as_ref()
            .map_or(60.0, |f| f.width("ändern …", px));
        let base = self.r(t, w, PATH_X, 18.0, 0.0, 28.0);
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
        if self.tab == Tab::Ext {
            return self.ext_layout(t, w);
        }
        if self.tab == Tab::Rooms {
            return self.rooms_layout(t, w);
        }
        let s = w.scale;
        let body = self.list_body(t, w);
        let tile_h = t.size.catalog_tile_h * s;
        let mut y = body.y - self.scroll;
        let (mut tiles, mut heads) = (Vec::new(), Vec::new());
        for cat in TypeCategory::WALLS {
            let items = self.items_of(cat);
            if items.is_empty() {
                continue;
            }
            let head = sk_model::kinds::spec(cat.category()).heading();
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
            Tab::Ext | Tab::Rooms => Vec::new(),
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
        let mut v: Vec<(TypeCategory, Guid)> = TypeCategory::WALLS
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
            Tab::Project | Tab::Rooms => "In Firmenkatalog speichern",
            Tab::Company => "Ins Projekt übernehmen",
            Tab::Ext => "Bauteil einlesen …",
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
            FieldId::Depth => (self.exterior() && self.fixed())
                .then(|| self.r(t, w, RIGHT_X + 150.0, 308.0, 90.0, 26.0)),
            FieldId::RoomName | FieldId::TilesHeight => self.room_field_rect(t, w, f),
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
            ComboId::Strip => self.r(t, w, RIGHT_X + 150.0, 340.0, 150.0, 26.0),
            ComboId::Edit(i) => self.combo_rect(t, w, ComboId::Material(i)),
            ComboId::RoomGroup | ComboId::RoomWoflv | ComboId::RoomSlot(_) => {
                self.room_combo_rect(t, w, id)
            }
        }
    }

    fn standard_rect(&self, t: &Theme, w: &Win) -> Rect {
        // Unter den Kennwerten, bei Außenwänden unter dem Deckenauflager
        let y = if !self.exterior() {
            252.0
        } else if self.fixed() {
            372.0
        } else {
            310.0
        };
        self.r(t, w, RIGHT_X, y, 16.0, 16.0)
    }

    fn exterior(&self) -> bool {
        self.draft.category == TypeCategory::ExteriorWall
    }

    /// Deckenauflager „fest, Rest Randstreifen“.
    fn fixed(&self) -> bool {
        matches!(self.draft.bearing, sk_model::Bearing::Depth { .. })
    }

    /// Umschalter Deckenauflager und seine beiden Hälften.
    fn bearing_box(&self, t: &Theme, w: &Win) -> Rect {
        self.r(t, w, RIGHT_X, 272.0, 300.0, 30.0)
    }

    fn bearing_rect(&self, t: &Theme, w: &Win, fixed: bool) -> Rect {
        let x = RIGHT_X + 2.0 + if fixed { 149.0 } else { 0.0 };
        self.r(t, w, x, 274.0, 147.0, 26.0)
    }

    /// Warum „fest, Rest Randstreifen“ nicht geht.
    fn fixed_blocked(&self) -> Option<&'static str> {
        if self.fixed() || self.draft.bearing_range().is_some() {
            return None;
        }
        Some(if self.draft.layers.first().is_some_and(|l| l.core) {
            "Tragende Schicht zu dünn: Auflager mindestens 10 cm, Randstreifen mindestens 2 cm"
        } else {
            "Nur bei Wänden ohne Schichten vor der tragenden Schicht."
        })
    }

    /// Dämmstoffe für den Randstreifen, der Werksbaustoff „Randdämmung“ zuerst.
    fn strip_materials(&self) -> Vec<sk_model::MaterialId> {
        let mut v: Vec<(bool, sk_model::MaterialId)> = self
            .work
            .materials()
            .iter()
            .filter(|(_, m)| m.category == MatCategory::Insulation)
            .map(|(id, m)| (m.name != "Randdämmung", id))
            .collect();
        v.sort_by_key(|x| x.0);
        v.into_iter().map(|x| x.1).collect()
    }

    fn set_bearing(&mut self, fixed: bool) {
        if fixed == self.fixed() || (fixed && self.fixed_blocked().is_some()) {
            return;
        }
        self.end_edit(true);
        if fixed {
            let Some((lo, hi)) = self.draft.bearing_range() else {
                return;
            };
            let Some(&strip) = self.strip_materials().first() else {
                self.message = Some("Kein Dämmstoff für den Randstreifen".into());
                return;
            };
            let depth = if (lo - 1e-6..=hi + 1e-6).contains(&240.0) {
                240.0
            } else {
                ((lo + hi) / 10.0).round() * 5.0
            };
            self.draft.bearing = sk_model::Bearing::Depth { depth, strip };
        } else {
            self.draft.bearing = sk_model::Bearing::Core;
        }
        self.commit_draft();
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
        let (cw, ch) = (640.0 * s, 252.0 * s);
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
                        Rect::new(c.x + 22.0 * s, y, bw, 86.0 * s),
                        "Als neuen Typ speichern",
                    ),
                    (
                        Btn::AskChange,
                        Rect::new(c.x + 38.0 * s + bw, y, bw, 86.0 * s),
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
        for tab in [Tab::Project, Tab::Company, Tab::Rooms, Tab::Ext] {
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
        if self.tab == Tab::Ext {
            return self.ext_hit(t, w, x, y);
        }
        if self.tab == Tab::Rooms {
            for (b, r, _) in self.list_buttons(t, w) {
                if r.contains(x, y) {
                    return Some(Target::Btn(b));
                }
            }
            return self.rooms_hit(t, w, x, y);
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
        if self.exterior() {
            for fixed in [false, true] {
                if self.bearing_rect(t, w, fixed).contains(x, y) {
                    return Some(Target::Bearing(fixed));
                }
            }
            if self.fixed() {
                if self
                    .field_rect(t, w, FieldId::Depth)
                    .is_some_and(|r| r.contains(x, y))
                {
                    return Some(Target::Field(FieldId::Depth));
                }
                if self.combo_rect(t, w, ComboId::Strip).contains(x, y) {
                    return Some(Target::Combo(ComboId::Strip));
                }
            }
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
                for (i, cat) in TypeCategory::WALLS.into_iter().enumerate() {
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
                for (i, cat) in NEW_MAT_CATS.into_iter().enumerate() {
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
        let mut out = self.dispatch(e, cx);
        if let Some(g) = self.edit_material.take() {
            out.open_material = Some(g);
            out.closed = true;
        }
        // Nur Hervorhebungen kennen ihren Bereich; alles andere malt ganz
        if out.repaint && self.damage.is_empty() {
            self.damage.push(Area::Full);
        }
        if out.closed {
            out.repaint = true;
        }
        out
    }

    fn dispatch(&mut self, e: &Event, cx: &mut Ctx) -> Out {
        let mut out = Out::default();
        match *e {
            Event::MouseMove { x, y, .. } => self.mouse_move(x, y, cx, &mut out),
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            } => self.mouse_down(x, y, mods, cx, &mut out),
            Event::MouseDown {
                button: MouseButton::Right,
                x,
                y,
                ..
            } => self.right_down(x, y, cx, &mut out),
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
                self.damage.push(Area::Hover(self.hover.take()));
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
                // Nur wenn sich die Markierung ändert, und nur das Feld
                if let (Some(i), Some(e)) = (self.caret_from_mouse(f, x, cx), self.edit.as_mut()) {
                    let before = e.text.caret;
                    e.text.place(i, true);
                    if e.text.caret != before {
                        self.damage.push(Area::Hover(Some(Target::Field(f))));
                        *out = Out::all();
                    }
                }
                return;
            }
            None => {}
        }
        let h = self.hit(t, &w, cx.fonts, x, y);
        if h != self.hover {
            self.damage.push(Area::Hover(self.hover));
            self.damage.push(Area::Hover(h));
            self.hover = h;
            out.repaint = true;
            out.popup = self.popup.is_some();
        }
    }

    /// Rechtsklick auf eine Schicht (Baustofffeld oder Schnittbild):
    /// „Baustoff bearbeiten …“, solange der Katalog nichts Ungespeichertes
    /// hat (sonst ginge es verloren).
    fn right_down(&mut self, x: f64, y: f64, cx: &mut Ctx, out: &mut Out) {
        if self.popup.is_some() || self.tab != Tab::Project || self.pending() {
            return;
        }
        let (t, w) = (cx.theme, cx.win);
        let row = match self.hit(t, &w, cx.fonts, x, y) {
            Some(Target::Combo(ComboId::Material(i)) | Target::Section(i)) => i,
            _ => return,
        };
        if self.is_air(row) {
            return;
        }
        self.open_combo(ComboId::Edit(row), cx);
        *out = Out::all();
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
            Target::Btn(Btn::New) if self.tab == Tab::Rooms => {
                self.end_edit(true);
                self.room_new();
            }
            Target::Btn(Btn::Duplicate) if self.tab == Tab::Rooms => {
                self.end_edit(true);
                self.room_duplicate();
            }
            Target::Btn(Btn::Delete) if self.tab == Tab::Rooms => {
                self.end_edit(true);
                self.room_delete();
            }
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
                    if self.nutzer || self.blocked() || cx.company.is_none() || self.draft_new {
                        return;
                    }
                    self.popup = Some(Popup::Export(self.draft.guid, false));
                }
                Tab::Company => self.import_selected(),
                Tab::Rooms => {
                    if !self.nutzer && cx.company.is_some() {
                        self.end_edit(true);
                        self.room_export();
                    }
                }
                Tab::Ext => out.ext_einlesen = true,
            },
            Target::Btn(Btn::Einfuegen) => self.ext_einfuegen(out),
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
            Target::Bearing(fixed) => self.set_bearing(fixed),
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
            FieldId::Depth => match self.draft.bearing {
                sk_model::Bearing::Depth { depth, .. } => cm_field(depth),
                sk_model::Bearing::Core => String::new(),
            },
            FieldId::MatName | FieldId::MatDensity | FieldId::MatLambda => match &self.popup {
                Some(Popup::NewMat(nm)) => match f {
                    FieldId::MatName => nm.name.clone(),
                    FieldId::MatDensity => nm.density.clone(),
                    _ => nm.lambda.clone(),
                },
                _ => String::new(),
            },
            FieldId::RoomName | FieldId::TilesHeight => self.room_field_value(f),
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
            FieldId::RoomName | FieldId::TilesHeight => return self.room_apply_value(f, text),
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
            FieldId::Depth => {
                let mm = parse_depth(text, self.draft.bearing_range())?;
                if let sk_model::Bearing::Depth { depth, .. } = &mut self.draft.bearing {
                    if (*depth - mm).abs() < 1e-9 {
                        return Ok(());
                    }
                    *depth = mm;
                    self.commit_draft();
                }
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

    /// Kachelflug zum Reiter `to`, der danach pulst; die Marke der Kachel
    /// (vorher `old`) blendet über (K3b §2).
    fn transfer(&mut self, it: Item, to: Tab, old: Option<Mark>) {
        let now = Instant::now();
        self.fly = Some((it, to, now));
        self.fly_img = None;
        self.pulse = Some((to, now));
        self.fade = Some((it, old, now));
    }

    /// Fortschritt eines Übergangs der Dauer `ms` (0 … 1); `None` ohne
    /// Animationen oder danach.
    fn progress(&self, at: Instant, ms: f32, t: &Theme) -> Option<f32> {
        let now = self.paint_now.unwrap_or_else(|| self.clock());
        let u = now.saturating_duration_since(at).as_secs_f32() * 1000.0 / ms.max(1.0);
        (t.size.anim_ms > 0.0 && u < 1.0).then_some(u)
    }

    /// Jetzt; in Tests die feste Uhr, falls gesetzt.
    fn clock(&self) -> Instant {
        #[cfg(test)]
        if let Some(t) = self.test_clock {
            return t;
        }
        Instant::now()
    }

    /// Lage der fliegenden Kopie: Rechteck und Deckkraft.
    fn fly_rect(&self, t: &Theme, w: &Win) -> Option<(Item, Rect, f32)> {
        let (it, to, at) = self.fly?;
        let u = self.progress(at, t.size.anim_ms, t)?;
        let (tiles, ..) = self.list_layout(t, w);
        let from = tiles.into_iter().find(|(x, _)| *x == it)?.1;
        let dest = self.tab_rect(t, w, to);
        let e = 1.0 - (1.0 - u).powi(3);
        let k = 1.0 - 0.4 * e;
        let (fx, fy) = (from.x + from.w * 0.5, from.y + from.h * 0.5);
        let (tx, ty) = (dest.x + dest.w * 0.5, dest.y + dest.h * 0.5);
        // Flacher Bogen nach oben
        let lift = 28.0 * w.scale * (std::f32::consts::PI * e).sin();
        let (cx, cy) = (fx + (tx - fx) * e, fy + (ty - fy) * e - lift);
        let (ww, hh) = (from.w * k, from.h * k);
        Some((it, Rect::new(cx - ww * 0.5, cy - hh * 0.5, ww, hh), 1.0 - u))
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
        v.extend(self.work.bearing_problem(&self.draft));
        v.extend(self.work.finish_problems(&self.draft));
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
            Item::Ext(i) => self.ext_sel = Some(i),
            Item::Room(g) => {
                self.end_edit(true);
                self.room_sel = Some(g);
            }
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

    /// Zeigt den Projekttyp mit der Guid `g` (aus „Verwendet in“ im Fenster
    /// „Baustoffe …“).
    pub fn show_guid(&mut self, g: Guid) {
        let id = self
            .work
            .layer_sets()
            .iter()
            .find(|(_, t)| t.guid == g)
            .map(|(id, _)| id);
        if let Some(id) = id {
            self.show(id);
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
            MaterialLayer::new(material, 15.0, LayerFunction::Finish)
        } else {
            MaterialLayer::new(material, 175.0, LayerFunction::Structure).core()
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
                let sel = TypeCategory::WALLS
                    .iter()
                    .position(|c| *c == self.draft.category);
                (
                    TypeCategory::WALLS
                        .iter()
                        .map(|c| (c.name().to_string(), None))
                        .collect(),
                    sel,
                )
            }
            ComboId::Material(i) => {
                // Die Luftschicht hat fest den Baustoff Luft
                if self.is_air(i) {
                    return;
                }
                let cur = self.draft.layers.get(i).map(|l| l.material);
                let mut items: Vec<(String, Option<TypeLook>)> = Vec::new();
                let mut sel = None;
                for (k, mid) in self.solid_materials().into_iter().enumerate() {
                    if Some(mid) == cur {
                        sel = Some(k);
                    }
                    let name = self
                        .work
                        .material(mid)
                        .map_or(String::new(), |m| m.name.clone());
                    items.push((name, Some(mat_look(&self.work, t, mid))));
                }
                items.push(("Neuer Baustoff …".into(), None));
                (items, sel)
            }
            ComboId::Strip => {
                let cur = self.draft.strip_material();
                let mats = self.strip_materials();
                let sel = mats.iter().position(|m| Some(*m) == cur);
                let items = mats
                    .into_iter()
                    .map(|mid| {
                        let name = self
                            .work
                            .material(mid)
                            .map_or(String::new(), |m| m.name.clone());
                        (name, Some(mat_look(&self.work, t, mid)))
                    })
                    .collect();
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
            ComboId::Edit(_) => (vec![("Baustoff bearbeiten …".to_string(), None)], None),
            ComboId::RoomGroup | ComboId::RoomWoflv | ComboId::RoomSlot(_) => {
                let Some((items, sel)) = self.room_combo_items(id) else {
                    return;
                };
                (items.into_iter().map(|x| (x, None)).collect(), sel)
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

    fn is_air(&self, row: usize) -> bool {
        self.draft
            .layers
            .get(row)
            .is_some_and(|l| l.function == LayerFunction::AirGap)
    }

    /// Baustoffe mit Körper, in der Reihenfolge der Bibliothek (Auswahl).
    fn solid_materials(&self) -> Vec<sk_model::MaterialId> {
        self.work
            .materials()
            .iter()
            .filter(|(_, m)| m.category != MatCategory::Air)
            .map(|(id, _)| id)
            .collect()
    }

    fn first_material(&self, cat: MatCategory) -> Option<sk_model::MaterialId> {
        self.work
            .materials()
            .iter()
            .find(|(_, m)| m.category == cat)
            .map(|(id, _)| id)
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
            ComboId::RoomGroup | ComboId::RoomWoflv | ComboId::RoomSlot(_) => {
                self.room_choose(cb.id, i)
            }
            ComboId::Category => {
                if let Some(cat) = TypeCategory::WALLS.get(i) {
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
                let mats = self.solid_materials();
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
                        let cat = if cat == MatCategory::Air {
                            MatCategory::Masonry
                        } else {
                            cat
                        };
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
            ComboId::Strip => {
                let mats = self.strip_materials();
                if let (Some(&mid), sk_model::Bearing::Depth { strip, .. }) =
                    (mats.get(i), &mut self.draft.bearing)
                {
                    if *strip != mid {
                        *strip = mid;
                        self.commit_draft();
                    }
                }
            }
            ComboId::Function(row) => {
                let Some(&(f, _)) = FUNCTIONS.get(i) else {
                    return;
                };
                // Luftschicht: Baustoff fest Luft; zurück: erster passender Baustoff
                let was_air = self.is_air(row);
                let material = if f == LayerFunction::AirGap {
                    self.first_material(MatCategory::Air)
                } else if was_air {
                    let cat = if f == LayerFunction::Insulation {
                        MatCategory::Insulation
                    } else {
                        MatCategory::Masonry
                    };
                    self.first_material(cat)
                } else {
                    None
                };
                if f == LayerFunction::AirGap && material.is_none() {
                    return;
                }
                if let Some(l) = self.draft.layers.get_mut(row) {
                    l.function = f;
                    // Tragend heißt Kern (IFC LoadBearing)
                    l.core = f == LayerFunction::Structure;
                    if let Some(m) = material {
                        l.material = m;
                    }
                    self.commit_draft();
                }
            }
            ComboId::Edit(row) => {
                let mat = self.draft.layers.get(row).map(|l| l.material);
                self.edit_material = mat.and_then(|m| self.work.material(m)).map(|x| x.guid);
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
                    // Gewerk nach Baustoffart (Paket 1a §3)
                    trade: sk_model::trade::for_category(nm.cat),
                    ..tpl
                });
                if let Some(l) = self.draft.layers.get_mut(nm.row) {
                    l.material = id;
                }
                self.popup = None;
                self.commit_draft();
            }
            Some(Popup::Export(g, changed)) => {
                let Some(company) = cx.company.as_deref_mut().filter(|_| !self.nutzer) else {
                    self.popup = None;
                    return;
                };
                if changed {
                    company.reload(cx.company_standard);
                }
                let it = self.work.type_by_guid(g).map(Item::Type);
                let old = it.and_then(|it| self.mark_of(it));
                let saved = if self.tab == Tab::Rooms {
                    company.save_space_type(&self.work, g)
                } else {
                    company.save_type(&self.work, g)
                };
                match saved {
                    SaveResult::Saved => {
                        self.popup = None;
                        self.message = Some("In den Firmenkatalog gespeichert".into());
                        if let Some(it) = it {
                            self.transfer(it, Tab::Company, old);
                        }
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

    /// Rolle Nutzer (KA-3b1): „In den Firmenkatalog …“ gesperrt.
    pub fn set_nutzer(&mut self, nutzer: bool) {
        self.nutzer = nutzer;
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
        let old = self.mark_of(Item::Company(g));
        match import_type(&mut self.work, &lib, g) {
            Some(id) => {
                // Die Kachel fliegt zum Reiter „Projekt“, der Firmenreiter bleibt
                self.show(id);
                self.transfer(Item::Company(g), Tab::Project, old);
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
        let defaults: Vec<(TypeCategory, Guid)> = TypeCategory::WALLS
            .iter()
            .filter_map(|c| {
                let id = lib.default_type(*c)?;
                Some((*c, lib.types.get(id)?.guid))
            })
            .collect();
        let real_guids: Vec<Guid> = real.layer_sets().iter().map(|(_, t)| t.guid).collect();
        // Raumvorlagen (RB2) nach den Typen, auf die sie verweisen, und vor
        // dem Löschen der Typen, die sie nicht mehr brauchen. Nicht halb
        // übernehmen (Review #85): vorher gegen die Arbeitskopie geprüft, die
        // dieselben Typen trägt; scheitert es im Projekt doch, nimmt der
        // Schritt alles zurück und das Fenster bleibt offen
        let rooms = self.work.space_types().to_vec();
        let why = rooms
            .iter()
            .find_map(|s| self.work.space_type_problems(s).next());
        let mut rooms_ok = why.is_none();
        let changed = rooms_ok
            && cx.scene.edit_types(crate::scene::CATALOG_STEP, |m| {
                for g in &work_guids {
                    import_type(m, &lib, *g);
                }
                for (cat, g) in &defaults {
                    if let Some(id) = m.type_by_guid(*g) {
                        m.set_default_type(*cat, id);
                    }
                }
                if !m.replace_space_types(rooms) {
                    m.rollback();
                    rooms_ok = false;
                    return false;
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
        if !rooms_ok {
            let why = why.unwrap_or_else(|| "Raumvorlagen passen nicht zu den Typen".into());
            self.message = Some(format!("Nicht übernommen: {why}"));
            self.blink = Some(Instant::now());
            return;
        }
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
        let fly = self
            .fly
            .is_some_and(|(_, _, at)| self.progress(at, t.size.anim_ms, t).is_some());
        let pulse = self
            .pulse
            .is_some_and(|(_, at)| self.progress(at, t.size.flash_ms, t).is_some());
        let fade = self
            .fade
            .is_some_and(|(_, _, at)| self.progress(at, FADE_MS, t).is_some());
        anim || blink
            || fly
            || pulse
            || fade
            || (0..3).any(|i| {
                self.flashes[i]
                    .is_some_and(|at| at.elapsed().as_secs_f32() * 1000.0 < t.size.flash_ms)
            })
    }

    /// Klappliste, Inline-Eingabe, Rückfrage oder Ziehen offen: Esc gehört
    /// zuerst ihnen (Paket 9, Nachtrag H9-1).
    pub fn busy(&self) -> bool {
        self.edit.is_some() || self.popup.is_some() || self.drag.is_some()
    }

    pub fn wait(&self, t: &Theme) -> Option<Duration> {
        self.animating(t).then_some(Duration::from_millis(16))
    }

    /// Ein Bild weiter: `true`, wenn neu zu zeichnen ist.
    pub fn tick(&mut self, t: &Theme) -> bool {
        // Was sich bewegt, malt nur seinen Bereich
        if self.anim.is_some() {
            self.damage.push(Area::Section);
        }
        if self.blink.is_some() {
            self.damage.push(Area::Foot);
        }
        for f in [Flash::Name, Flash::Thick, Flash::U] {
            if self.flashes[f as usize].is_some() {
                self.damage.push(Area::Flash(f));
            }
        }
        if self.fly.is_some() || self.fly_drawn.is_some() {
            self.damage.push(Area::Fly);
        }
        if let Some((tab, _)) = self.pulse {
            self.damage.push(Area::Pulse(tab));
        }
        if let Some((it, ..)) = self.fade {
            self.damage.push(Area::Tile(it));
        }
        if self.animating(t) {
            return true;
        }
        // Ein letztes Bild ohne Übergang
        let mut done = self.anim.take().is_some() | self.blink.take().is_some();
        done |= self.fly.take().is_some() | self.fly_drawn.is_some();
        done |= self.pulse.take().is_some() | self.fade.take().is_some();
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
            Target::Btn(Btn::Delete) if self.tab == Tab::Rooms => {
                let n = self.room_users();
                let wer = if n == 1 {
                    "einem Raum".into()
                } else {
                    format!("{n} Räumen")
                };
                (n > 0).then(|| format!("Genutzt von {wer}, Löschen erst ohne Nutzung"))
            }
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
            Target::Btn(Btn::Action) if self.tab == Tab::Project && self.nutzer => {
                Some(NUR_LESBAR.into())
            }
            Target::Grip(_) => Some("Ziehen sortiert um".into()),
            Target::Bearing(true) => self.fixed_blocked().map(Into::into),
            Target::Field(FieldId::Depth) => {
                let (lo, hi) = self.draft.bearing_range()?;
                Some(format!(
                    "Ab Wandinnenseite, {} bis {} cm",
                    cm_field(lo),
                    cm_field(hi)
                ))
            }
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
        bearing: sk_model::Bearing::Core,
        height: None,
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
        layers: vec![MaterialLayer::new(id, 100.0, LayerFunction::Structure)],
        props: Default::default(),
        note: String::new(),
        changed: 0,
        bearing: sk_model::Bearing::Core,
        height: None,
    };
    type_look(m, t, &set)
}

/// Kategorie-Knopf in der Karte „Neuer Baustoff“.
fn mat_cat_rect(card: Rect, i: usize, s: f32) -> Rect {
    let w = (card.w - 36.0 * s) / NEW_MAT_CATS.len() as f32;
    Rect::new(
        card.x + 18.0 * s + i as f32 * w,
        card.y + 160.0 * s,
        w - 4.0 * s,
        26.0 * s,
    )
}

mod zeichnen;
use zeichnen::Mark;

#[cfg(test)]
mod tests;

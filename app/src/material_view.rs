//! Fenster „Baustoffe“ (Paket 5, projektstruktur/paket-5-materialfenster.md,
//! einstellungen/paket-p5-darstellung.md): alle Baustoffe des Projekts und
//! des Firmenkatalogs, nach Baustoffart gegliedert. Auf den ersten Blick
//! schlicht (Name, drei Vorschauen, λ, Rohdichte, Richtpreis, „Verwendet
//! in“); „Mehr“ zeigt die Kennwerte, Herkunft und Darstellung.
//!
//! Bearbeitet wird eine Kopie des Modells; OK übernimmt alles als einen
//! Schritt „Baustoffe geändert“ ([`sk_model::sync_materials`]), Abbrechen
//! verwirft. Die Eingabe selbst ([`input`], [`add_custom`]) ist eine reine
//! Funktion auf dem Modell (Abnahmetests A230–A240).

use crate::attr_pick::{self, Pick, Tiles};
use crate::catalog::{Company, SaveResult};
use crate::catalog_view::Frame;
use crate::prefs::Win;
use crate::scene::Scene;
use crate::window_kit::{
    intersect, label, num_text as de, outline, parse_num, pixel_strips, prop_text, rounded,
};
use sk_model::matprop::{self, MatPropKind, PRICE, PRICE_DATE, PRICE_UNIT};
use sk_model::{
    compare_materials, import_material, Guid, LayerSetId, Library, MatCategory, Material,
    MaterialDisplay, MaterialId, Model, PropValue, TypeCategory, TypeState, Use,
};
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{Cursor, Event, Key, Modifiers, MouseButton};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, FieldState, Fonts, Rect};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Rückgängig-Schritt beim OK.
pub const STEP: &str = "Baustoffe geändert";

/// Reihenfolge der Baustoffarten in der Liste (Sollbild p5-1).
const CATS: [MatCategory; 7] = [
    MatCategory::Masonry,
    MatCategory::Concrete,
    MatCategory::Insulation,
    MatCategory::Plaster,
    MatCategory::Timber,
    MatCategory::Metal,
    MatCategory::Air,
];

/// Kopf und Fuß des Fensters (dip), wie der Bauteilkatalog.
const HEAD: f32 = 52.0;
/// Platz für den Pfeil hinter „Mehr“ bzw. „Weniger“ (dip).
const MORE_ARROW: f32 = 14.0;
const FOOT: f32 = 56.0;

// --- Eingabe (rein) ---------------------------------------------------------

/// Felder, die Eigenschaften des Baustoffs selbst sind (keine Kennwerte).
const OWN_FIELDS: [&str; 6] = [
    "Name",
    "Art",
    "λ",
    "Rohdichte",
    "Verschnittpriorität",
    "Gewerk",
];

/// Eingabe im Fenster wie getippt (A230–A240): `field` ist „Name“, „Art“
/// (Name der Baustoffart), „λ“, „Rohdichte“, „Verschnittpriorität“, ein
/// fester Kennwert aus [`sk_model::MAT_PROPS`] („µ“ gilt als „μ“) oder ein
/// vorhandener eigener Kennwert. Leer löscht einen Kennwert (beim Richtpreis
/// samt Einheit). Der Richtpreis speichert die Einheit der Baustoffart mit
/// (Regel 53). `false`: abgelehnt, nichts geändert.
pub fn input(m: &mut Model, id: MaterialId, field: &str, text: &str) -> bool {
    let Some(mut x) = m.material(id).cloned() else {
        return false;
    };
    let text = text.trim();
    let key = matprop::normalize_key(field);
    match key.as_str() {
        "Name" => {
            if text.is_empty() {
                return false;
            }
            x.name = text.to_string();
        }
        "Art" => match CATS.into_iter().find(|c| c.name() == text) {
            Some(c) => x.category = c,
            None => return false,
        },
        "λ" => {
            if text.is_empty() {
                x.lambda = None;
            } else {
                match parse_num(text) {
                    Some(v) if matprop::check_lambda(x.category, v).is_ok() => x.lambda = Some(v),
                    _ => return false,
                }
            }
        }
        "Rohdichte" => match parse_num(text) {
            Some(v) if matprop::check_density(x.category, v).is_ok() => x.density = v,
            _ => return false,
        },
        "Verschnittpriorität" => match text.parse::<u16>() {
            Ok(v) => x.priority = v,
            Err(_) => return false,
        },
        PRICE_UNIT | "Gewerk" | "" => return false,
        k => {
            let fixed = matprop::mat_prop(k);
            if fixed.is_none() && !x.props.contains_key(k) {
                return false;
            }
            if text.is_empty() {
                x.props.remove(k);
                if k == PRICE {
                    x.props.remove(PRICE_UNIT);
                }
            } else {
                let v = match fixed.map(|p| p.kind) {
                    Some(MatPropKind::Number) => match parse_num(text) {
                        Some(n) => PropValue::Number(n),
                        None => return false,
                    },
                    Some(MatPropKind::Text) => PropValue::Text(text.to_string()),
                    None => custom_value(text),
                };
                if matprop::check_prop(x.category, k, &v).is_err() {
                    return false;
                }
                if k == PRICE {
                    let Some(u) = matprop::price_unit(x.category) else {
                        return false;
                    };
                    x.props.insert(PRICE_UNIT.into(), PropValue::Text(u.into()));
                }
                x.props.insert(k.to_string(), v);
            }
        }
    }
    m.set_material(id, x)
}

/// Wert eines eigenen Kennworts: Zahl, wenn er eine ist, sonst Text.
/// Suchwörter, die der Baustoffname nicht enthält (KA-0a3): fest im
/// Programm, kein Dateifeld.
const SYNONYMS: [(&str, &str); 2] = [("gasbeton", "porenbeton"), ("ytong", "porenbeton")];

/// Name, den ein Suchwort (klein, ab drei Zeichen auch angefangen) meint.
fn synonym(q: &str) -> Option<&'static str> {
    SYNONYMS
        .iter()
        .find(|(alt, _)| *alt == q || (q.chars().count() >= 3 && alt.starts_with(q)))
        .map(|(_, neu)| *neu)
}

fn custom_value(text: &str) -> PropValue {
    match parse_num(text) {
        Some(n) => PropValue::Number(n),
        None => PropValue::Text(text.to_string()),
    }
}

/// „+ Kennwert“: eigener Kennwert `name` mit dem Wert `text` (Zahl oder
/// Text). Die Einheit steht im Namen („Druckfestigkeit [N/mm²]“). `false`,
/// wenn der Name leer, ein fester Schlüssel (auch als „µ“) oder ein Feld des
/// Baustoffs ist oder der Wert fehlt (Regel 50).
pub fn add_custom(m: &mut Model, id: MaterialId, name: &str, text: &str) -> bool {
    let key = matprop::normalize_key(name);
    let text = text.trim();
    if key.is_empty()
        || text.is_empty()
        || matprop::mat_prop(&key).is_some()
        || OWN_FIELDS.contains(&key.as_str())
    {
        return false;
    }
    let Some(mut x) = m.material(id).cloned() else {
        return false;
    };
    x.props.insert(key, custom_value(text));
    m.set_material(id, x)
}

/// Gewerk des Baustoffs setzen (Auswahl unter „Mehr“).
fn set_trade(m: &mut Model, id: MaterialId, trade: Option<sk_model::trade::TradeId>) -> bool {
    let Some(mut x) = m.material(id).cloned() else {
        return false;
    };
    x.trade = trade;
    m.set_material(id, x)
}

/// Darstellungsverweise setzen (Schraffur, Stifte, Oberfläche).
/// Gespeicherte Preiseinheit, falls ein Richtpreis steht.
fn stored_unit(x: &Material) -> Option<&str> {
    x.props.get(PRICE)?;
    match x.props.get(PRICE_UNIT) {
        Some(PropValue::Text(u)) => Some(u.as_str()),
        _ => None,
    }
}

/// Hinweis, wenn die gespeicherte Einheit des Richtpreises nicht mehr zur
/// Baustoffart passt (Regel 53); der Preis wird nie umgedeutet.
pub fn price_hint(m: &Model, id: MaterialId) -> Option<String> {
    let x = m.material(id)?;
    let unit = stored_unit(x)?;
    if matprop::price_unit(x.category) == Some(unit) {
        return None;
    }
    Some(format!(
        "Preis in {} – Einheit passt nicht mehr zur Baustoffart",
        matprop::price_unit_label(unit)
    ))
}

/// Zweite, kleine Zeile des Hinweises.
fn price_hint_detail(x: &Material) -> String {
    match matprop::price_unit(x.category) {
        Some(u) => format!(
            "Der Preis wird nicht umgerechnet. Neu eintragen setzt {}.",
            matprop::price_unit_label(u)
        ),
        None => "Luft hat keinen Preis.".into(),
    }
}

/// Bauteile eines Typs als Wort mit Zahl: „8 Wände“, „1 Dachterrasse“.
fn count_text(c: TypeCategory, n: usize) -> String {
    let (one, many) = match c {
        TypeCategory::ExteriorWall | TypeCategory::InteriorWall => ("Wand", "Wände"),
        TypeCategory::Floor => ("Decke", "Decken"),
        TypeCategory::GroundSlab => ("Sohlplatte", "Sohlplatten"),
        TypeCategory::StripFooting => ("Frostschürze", "Frostschürzen"),
        TypeCategory::RoofTerrace => ("Dachterrasse", "Dachterrassen"),
    };
    match n {
        0 => "nicht verbaut".into(),
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    }
}

/// Warum ein Baustoff nicht gelöscht werden kann (zweite Zeile des
/// Hinweises am blassen Knopf).
fn delete_why(m: &Model, id: MaterialId, uses: &[Use]) -> String {
    let Some(x) = m.material(id) else {
        return String::new();
    };
    if x.category == MatCategory::Air {
        return "Luft bleibt immer im Projekt".into();
    }
    match uses.first() {
        Some(Use::Type(t, _)) => format!(
            "verwendet in {}",
            m.layer_set(*t).map_or("", |t| t.name.as_str())
        ),
        Some(Use::Element(e)) => format!(
            "verwendet in {}",
            m.element(*e).map_or("", |e| e.number.as_str())
        ),
        None => "gehört zu Dachterrasse und Attikablech".into(),
    }
}

// --- Fenster ----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Project,
    Company,
}

/// Eingabefeld.
#[derive(Clone, Debug, PartialEq)]
enum Field {
    Search,
    Name,
    /// Fester Schlüssel: „λ“, „Rohdichte“, „Verschnittpriorität“ oder ein
    /// Kennwert aus `MAT_PROPS`.
    Key(&'static str),
    /// Eigener Kennwert.
    Custom(String),
    /// „+ Kennwert“: erst der Name, dann der Wert.
    NewKey,
    NewValue(String),
}

impl Field {
    fn numeric(&self) -> bool {
        match self {
            Field::Key(k) => {
                matches!(*k, "λ" | "Rohdichte" | "Verschnittpriorität")
                    || matprop::mat_prop(k).is_some_and(|p| p.kind == MatPropKind::Number)
            }
            _ => false,
        }
    }

    /// Schlüssel für [`input`].
    fn key(&self) -> Option<String> {
        match self {
            Field::Name => Some("Name".into()),
            Field::Key(k) => Some((*k).into()),
            Field::Custom(k) => Some(k.clone()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ComboId {
    Category,
    Trade,
    Euro,
    Pick(Pick),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Btn {
    Duplicate,
    Delete,
    Export,
    Import,
    Cancel,
    Ok,
    ConfirmNo,
    ConfirmYes,
}

#[derive(Clone, Debug, PartialEq)]
enum Target {
    Close,
    Tab(Tab),
    PathLink,
    Group(MatCategory),
    Row(Guid),
    Field(Field),
    Combo(ComboId),
    /// Verweis „Verwendet in“ auf einen Typ.
    Use(LayerSetId),
    More,
    AddKey,
    Btn(Btn),
    Item(usize),
    /// Fläche einer Karte oder Liste ohne eigenes Ziel.
    Card,
}

#[derive(Clone, Debug)]
struct Edit {
    field: Field,
    text: TextEdit,
    orig: String,
    invalid: bool,
}

struct List {
    id: ComboId,
    items: Vec<String>,
    icons: Vec<Option<Canvas>>,
    sel: usize,
    first: usize,
}

enum Popup {
    List(List),
    /// „In den Firmenkatalog …“: Rückfrage für diesen Baustoff.
    Confirm(Guid),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Window(f32, f32),
    Select,
}

/// Zeile der Liste links.
#[derive(Clone, Debug, PartialEq)]
enum Row {
    Group(MatCategory),
    Item(Guid),
}

/// Was die App nach einem Ereignis tun muss.
#[derive(Clone, Debug, Default)]
pub struct Out {
    pub repaint: bool,
    pub moved: bool,
    pub closed: bool,
    /// Das Modell hat sich geändert (OK): Netze, Paneele, Mengen neu.
    pub applied: bool,
    /// Ort des Firmenkatalogs wählen („ändern …“).
    pub pick_company: bool,
    /// Bauteilkatalog mit diesem Typ öffnen („Verwendet in“).
    pub open_type: Option<Guid>,
    /// Was beim OK nicht übernommen werden konnte (Review 3n/4).
    pub problems: Vec<String>,
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

/// Stand der Kacheln der Liste: Revision und Attributrevision der Kopie,
/// Skalierung, Schema.
type TileStamp = (u64, u64, u32, u64);

/// Kachel eines Listeneintrags (Baustoff-Guid, Bild).
type Mark = (Guid, Canvas);

/// Ein Eintrag der Zeile „Verwendet in“: ein Typ (anklickbar) oder ein
/// einzelnes Bauteil mit eigenem Aufbau (nur Text).
struct UseLink {
    ty: Option<LayerSetId>,
    name: String,
    tail: String,
    rect: Rect,
    end: f32,
}

pub struct MaterialView {
    work: Model,
    pub tab: Tab,
    sel: Option<Guid>,
    csel: Option<Guid>,
    company: Option<(Library, String)>,
    search: String,
    closed_groups: Vec<MatCategory>,
    /// „Mehr“ offen (gilt für die Sitzung) und Beginn des Aufklappens.
    more: bool,
    more_at: Option<Instant>,
    more_scroll: f32,
    edit: Option<Edit>,
    hover: Option<Target>,
    pressed: Option<Target>,
    popup: Option<Popup>,
    drag: Option<Drag>,
    pos: Option<(f32, f32)>,
    scroll: f32,
    /// Meldung im Fuß (Zurückspeichern).
    message: Option<String>,
    /// Kacheln, Vorschauen und Bildchen der Auswahllisten (M1–M3).
    tiles: RefCell<Tiles>,
    /// Kleine Schraffurfelder der Liste je Baustoff-Guid, gültig für einen
    /// Stand ([`TileStamp`]).
    marks: RefCell<(Option<TileStamp>, Vec<Mark>)>,
    /// „Verwendet in“ je Revision der Kopie und Baustoff (M4).
    uses: RefCell<Option<(u64, MaterialId, Vec<Use>)>>,
    /// „Im Projekt verwendet“ je Revision der Kopie und Baustoff, nur für
    /// gefragte (sichtbare) Baustoffe (Review 3n).
    used: RefCell<(u64, HashMap<MaterialId, bool>)>,
    /// Ziele, deren Hervorhebung sich seit dem letzten Bild geändert hat
    /// (vorher und nachher, B6); leer: alles.
    damage: Vec<Option<Target>>,
    /// Das nächste Bild ganz malen (jede Änderung außer dem Überfahren).
    full_frame: bool,
    /// Letztes ganzes Bild; Teilbilder erneuern es stellenweise.
    img: Option<Canvas>,
    /// Größe, Skalierung, Schema und Lage, für die `img` gilt.
    img_key: Option<(usize, usize, u32, u64, i32, i32)>,
    /// Leinwand für Teilbilder, behält ihren Speicher.
    scratch: Canvas,
    /// Fenstergrund mit Schatten je Größe, Skalierung und Schema (Review 3n,
    /// wie 3k).
    ground: Option<((usize, usize, u32, u64), Canvas)>,
    /// Leinwand für „Mehr“, je Bild wiederverwendet.
    more_img: RefCell<Canvas>,
}

impl MaterialView {
    /// Öffnet das Fenster auf einer Kopie des Modells, der erste Baustoff
    /// der Liste gewählt.
    #[cfg(test)]
    pub fn open(scene: &Scene) -> MaterialView {
        Self::open_with(scene, None, None, false)
    }

    /// Wie [`MaterialView::open`] mit Firmenkatalog, gewähltem Baustoff und
    /// dem Zustand „Mehr“ aus der Sitzung.
    pub fn open_with(
        scene: &Scene,
        company: Option<&Company>,
        select: Option<Guid>,
        more: bool,
    ) -> MaterialView {
        let mut work = scene.model().clone();
        work.allow_unstepped();
        work.fork_guids();
        let mut v = MaterialView {
            work,
            tab: Tab::Project,
            sel: None,
            csel: None,
            company: None,
            search: String::new(),
            closed_groups: Vec::new(),
            more,
            more_at: None,
            more_scroll: 0.0,
            edit: None,
            hover: None,
            pressed: None,
            popup: None,
            drag: None,
            pos: None,
            scroll: 0.0,
            message: None,
            tiles: RefCell::default(),
            marks: RefCell::new((None, Vec::new())),
            uses: RefCell::new(None),
            used: RefCell::new((u64::MAX, HashMap::new())),
            damage: Vec::new(),
            full_frame: true,
            img: None,
            img_key: None,
            scratch: Canvas::new(0, 0),
            ground: None,
            more_img: RefCell::new(Canvas::new(0, 0)),
        };
        v.set_company(company);
        v.sel = select
            .filter(|g| v.work.materials().iter().any(|(_, x)| x.guid == *g))
            .or_else(|| v.first_item());
        v
    }

    /// Reiter „Firma“ zeigen (Bildvergleiche, `--baustoffe-firma`).
    pub fn show_company(&mut self) {
        self.full_frame = true;
        self.tab = Tab::Company;
        self.csel = self.sel;
    }

    /// Neuer Stand des Firmenkatalogs (nach Neuladen oder neuem Ort).
    pub fn set_company(&mut self, company: Option<&Company>) {
        self.company = company.map(|c| (c.library().clone(), c.path().display().to_string()));
        self.full_frame = true;
    }

    /// Die Kopie, die das Fenster bearbeitet.
    #[cfg(test)]
    pub fn model_mut(&mut self) -> &mut Model {
        &mut self.work
    }

    /// Zustand „Mehr“ für die Sitzung.
    pub fn more(&self) -> bool {
        self.more
    }

    /// OK: alle Änderungen als ein Schritt „Baustoffe geändert“. `true`,
    /// wenn sich etwas geändert hat.
    #[cfg(test)]
    pub fn ok(&mut self, s: &mut Scene) -> bool {
        self.ok_report(s, &mut Vec::new())
    }

    /// Wie [`MaterialView::ok`]; was nicht übernommen werden konnte, steht
    /// in `problems` (Review 3n/4).
    pub fn ok_report(&mut self, s: &mut Scene, problems: &mut Vec<String>) -> bool {
        let work = &self.work;
        s.edit_types(STEP, |m| sk_model::sync_materials_report(m, work, problems))
    }

    /// Abbrechen: die Kopie wird verworfen, das Modell bleibt.
    pub fn cancel(&mut self, _s: &mut Scene) {
        self.edit = None;
        self.popup = None;
    }

    // --- Daten ---------------------------------------------------------------

    fn mat_id(&self, g: Guid) -> Option<MaterialId> {
        self.work
            .materials()
            .iter()
            .find(|(_, x)| x.guid == g)
            .map(|(id, _)| id)
    }

    fn sel_id(&self) -> Option<MaterialId> {
        self.sel.and_then(|g| self.mat_id(g))
    }

    fn lib(&self) -> Option<&Library> {
        self.company.as_ref().map(|c| &c.0)
    }

    fn lib_mat(&self, g: Guid) -> Option<&Material> {
        self.lib()?
            .materials
            .iter()
            .find(|(_, x)| x.guid == g)
            .map(|(_, x)| x)
    }

    /// „Verwendet in“ des Baustoffs, einmal je Revision der Kopie (M4).
    fn uses_of(&self, id: MaterialId) -> Vec<Use> {
        let rev = self.work.revision();
        let mut c = self.uses.borrow_mut();
        if let Some((r, i, u)) = c.as_ref() {
            if *r == rev && *i == id {
                return u.clone();
            }
        }
        let u = self.work.material_uses(id);
        *c = Some((rev, id, u.clone()));
        u
    }

    /// Steckt der Baustoff im Projekt? Einmal je Revision und Baustoff
    /// statt in jedem Bild über alle Bauteile.
    fn used(&self, id: MaterialId) -> bool {
        let rev = self.work.revision();
        let mut c = self.used.borrow_mut();
        if c.0 != rev {
            *c = (rev, HashMap::new());
        }
        *c.1.entry(id).or_insert_with(|| self.work.material_used(id))
    }

    fn matches(&self, x: &Material) -> bool {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        let text = |k: &str| match x.props.get(k) {
            Some(PropValue::Text(t)) => t.to_lowercase().contains(&q),
            _ => false,
        };
        let name = x.name.to_lowercase();
        name.contains(&q)
            || synonym(&q).is_some_and(|n| name.contains(n))
            || text(matprop::MAKER)
            || text(matprop::SUBGROUP)
    }

    /// Einträge der Liste: je Art (in [`CATS`]) Kopf und Baustoffe; im
    /// Reiter Firma Projekt und Katalog zusammen, ohne Luft.
    fn entries(&self) -> Vec<(MatCategory, Vec<(Guid, String)>)> {
        let mut all: Vec<(Guid, String, MatCategory)> = Vec::new();
        let mut push = |x: &Material| {
            if !all.iter().any(|a| a.0 == x.guid) && self.matches(x) {
                all.push((x.guid, x.name.clone(), x.category));
            }
        };
        match self.tab {
            Tab::Project => self.work.materials().iter().for_each(|(_, x)| push(x)),
            Tab::Company => {
                let lib = self.lib().map(|l| l.materials.iter().map(|(_, x)| x));
                for x in lib
                    .into_iter()
                    .flatten()
                    .chain(self.work.materials().iter().map(|(_, x)| x))
                {
                    if x.category != MatCategory::Air {
                        push(x);
                    }
                }
            }
        }
        CATS.into_iter()
            .filter_map(|c| {
                let v: Vec<(Guid, String)> = all
                    .iter()
                    .filter(|a| a.2 == c)
                    .map(|a| (a.0, a.1.clone()))
                    .collect();
                (!v.is_empty()).then_some((c, v))
            })
            .collect()
    }

    fn rows(&self) -> Vec<(Row, String)> {
        let mut out = Vec::new();
        for (c, items) in self.entries() {
            out.push((Row::Group(c), c.name().to_string()));
            if self.closed_groups.contains(&c) {
                continue;
            }
            out.extend(items.into_iter().map(|(g, n)| (Row::Item(g), n)));
        }
        out
    }

    fn first_item(&self) -> Option<Guid> {
        self.rows().into_iter().find_map(|(r, _)| match r {
            Row::Item(g) => Some(g),
            Row::Group(_) => None,
        })
    }

    fn can_delete(&self) -> bool {
        self.tab == Tab::Project
            && self
                .sel_id()
                .is_some_and(|id| !self.used(id) && self.work.can_remove_material(id))
    }

    // --- Lage ----------------------------------------------------------------

    fn size(&self, t: &Theme, w: &Win) -> (f32, f32) {
        let s = w.scale;
        let ww = (t.size.mat_w * s).min(w.w as f32);
        let hh = (t.size.mat_h * s).min((w.h - w.top) as f32);
        (ww.round(), hh.round())
    }

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

    /// Fensterbreite und -höhe in dip.
    fn dip(&self, t: &Theme, w: &Win) -> (f32, f32) {
        let (ww, hh) = self.size(t, w);
        (ww / w.scale, hh / w.scale)
    }

    /// Linker Rand des Inhalts rechts der Liste (dip).
    fn rx(&self, t: &Theme) -> f32 {
        t.size.mat_list_w + 24.0
    }

    fn close_rect(&self, t: &Theme, w: &Win) -> Rect {
        let (ww, _) = self.dip(t, w);
        self.r(t, w, ww - 46.0, 12.0, 28.0, 28.0)
    }

    fn tab_rect(&self, t: &Theme, w: &Win, tab: Tab) -> Rect {
        let i = if tab == Tab::Project { 0.0 } else { 1.0 };
        self.r(t, w, 174.0 + i * 108.0, 12.0, 106.0, 28.0)
    }

    fn path_link(&self, t: &Theme, w: &Win, fonts: &Fonts) -> Option<Rect> {
        if self.tab != Tab::Company {
            return None;
        }
        let s = w.scale;
        let px = t.size.font_small * s;
        let pw = match &self.company {
            Some((_, p)) => fonts
                .regular
                .as_ref()
                .map_or(0.0, |f| f.width(p, px))
                .min(360.0 * s),
            None => 0.0,
        };
        let lw = fonts
            .regular
            .as_ref()
            .map_or(60.0 * s, |f| f.width("ändern …", px));
        let base = self.r(t, w, 406.0, 12.0, 0.0, 28.0);
        let gap = if pw > 0.0 { 12.0 * s } else { 0.0 };
        Some(Rect::new(
            base.x + pw + gap - 2.0 * s,
            base.y,
            lw + 4.0 * s,
            base.h,
        ))
    }

    fn search_rect(&self, t: &Theme, w: &Win) -> Rect {
        self.r(t, w, 12.0, HEAD + 12.0, t.size.mat_list_w - 24.0, 28.0)
    }

    /// Bereich der Liste (Zeilen mit Bildlauf).
    fn list_body(&self, t: &Theme, w: &Win) -> Rect {
        let (_, hh) = self.dip(t, w);
        let y0 = HEAD + 52.0;
        self.r(t, w, 0.0, y0, t.size.mat_list_w, hh - FOOT - 8.0 - y0)
    }

    fn row_rect(&self, t: &Theme, w: &Win, i: usize) -> Rect {
        let body = self.list_body(t, w);
        let s = w.scale;
        let rh = (t.size.mat_row * s).round();
        Rect::new(
            body.x + 12.0 * s,
            (body.y + i as f32 * rh - self.scroll).round(),
            body.w - 24.0 * s,
            rh,
        )
    }

    fn name_rect(&self, t: &Theme, w: &Win) -> Rect {
        self.r(t, w, self.rx(t), HEAD + 12.0, 300.0, 34.0)
    }

    fn cat_rect(&self, t: &Theme, w: &Win, fonts: &Fonts) -> Rect {
        let s = w.scale;
        let n = self.name_rect(t, w);
        let text = self
            .sel_id()
            .and_then(|id| self.work.material(id))
            .map_or("", |x| x.category.name());
        let tw = fonts
            .regular
            .as_ref()
            .map_or(60.0 * s, |f| f.width(text, t.size.font_small * s));
        Rect::new(n.x + n.w + 8.0 * s, n.y, tw + 16.0 * s, n.h)
    }

    fn preview_rect(&self, t: &Theme, w: &Win, i: usize) -> Rect {
        let p = t.size.mat_preview;
        self.r(t, w, self.rx(t) + i as f32 * (p + 12.0), HEAD + 64.0, p, p)
    }

    /// Linker Rand der Zeilen rechts der Vorschauen (dip).
    fn rows_x(&self, t: &Theme) -> f32 {
        self.rx(t) + 3.0 * t.size.mat_preview + 2.0 * 12.0 + 28.0
    }

    /// Feld der Zeile `i` rechts der Vorschauen (dip-Zeilen zu 32).
    fn basic_field(&self, t: &Theme, w: &Win, i: usize, x: f32, ww: f32) -> Rect {
        let y = HEAD + 65.0 + i as f32 * 32.0;
        self.r(
            t,
            w,
            self.rows_x(t) + t.size.mat_label_w + x,
            y,
            ww,
            t.size.mat_row,
        )
    }

    /// Zeile von „Verwendet in“ (unter dem Hinweis zur Preiseinheit).
    fn uses_row(&self) -> usize {
        let hint = self
            .sel_id()
            .and_then(|id| price_hint(&self.work, id))
            .is_some();
        if hint {
            4
        } else {
            3
        }
    }

    fn more_link(&self, t: &Theme, w: &Win, fonts: &Fonts) -> Rect {
        let s = w.scale;
        let label = if self.more { "Weniger" } else { "Mehr" };
        // Text und Pfeil (gezeichnet, nicht jede Schrift hat ▸ und ▾)
        let tw = fonts
            .bold
            .as_ref()
            .or(fonts.regular.as_ref())
            .map_or(60.0 * s, |f| f.width(label, t.size.font_small * s))
            + MORE_ARROW * s;
        let extra = if self.uses_row() == 4 { 32.0 } else { 0.0 };
        let r = self.r(t, w, self.rx(t), HEAD + 196.0 + extra, 0.0, 22.0);
        Rect::new(r.x - 2.0 * s, r.y, tw + 4.0 * s, r.h)
    }

    /// Bereich von „Mehr“ (zwei Spalten, mit Bildlauf).
    fn more_body(&self, t: &Theme, w: &Win) -> Rect {
        let (ww, hh) = self.dip(t, w);
        let extra = if self.uses_row() == 4 { 32.0 } else { 0.0 };
        let y0 = HEAD + 220.0 + extra;
        let x0 = self.rx(t);
        self.r(t, w, x0, y0, ww - x0 - 6.0, hh - FOOT - 4.0 - y0)
    }

    fn foot_buttons(&self, t: &Theme, w: &Win) -> Vec<(Btn, Rect, &'static str)> {
        let (ww, hh) = self.dip(t, w);
        let y = hh - FOOT + 12.0;
        let mut v = Vec::new();
        match self.tab {
            Tab::Project => {
                v.push((Btn::Delete, self.r(t, w, 16.0, y, 94.0, 30.0), "Löschen"));
                v.push((
                    Btn::Export,
                    self.r(t, w, 118.0, y, 204.0, 30.0),
                    "In den Firmenkatalog …",
                ));
            }
            Tab::Company => v.push((
                Btn::Import,
                self.r(t, w, 16.0, y, 206.0, 30.0),
                "Ins Projekt übernehmen",
            )),
        }
        v.push((
            Btn::Cancel,
            self.r(t, w, ww - 236.0, y, 100.0, 30.0),
            "Abbrechen",
        ));
        v.push((Btn::Ok, self.r(t, w, ww - 126.0, y, 110.0, 30.0), "OK"));
        v
    }

    fn duplicate_rect(&self, t: &Theme, w: &Win) -> Rect {
        let (ww, _) = self.dip(t, w);
        self.r(t, w, ww - 130.0, HEAD + 14.0, 106.0, 30.0)
    }

    /// Hinweis am blassen „Löschen“.
    fn delete_tip_rect(&self, t: &Theme, w: &Win) -> Rect {
        let (_, hh) = self.dip(t, w);
        self.r(t, w, 16.0, hh - FOOT - 46.0, 240.0, 44.0)
    }

    fn confirm_card(&self, t: &Theme, w: &Win) -> Rect {
        let (ww, hh) = self.dip(t, w);
        self.r(t, w, (ww - 440.0) * 0.5, (hh - 170.0) * 0.5, 440.0, 170.0)
    }

    fn confirm_buttons(&self, t: &Theme, w: &Win) -> [(Btn, Rect, &'static str); 2] {
        let c = self.confirm_card(t, w);
        let s = w.scale;
        let y = c.y + c.h - 46.0 * s;
        [
            (
                Btn::ConfirmNo,
                Rect::new(c.x + c.w - 236.0 * s, y, 104.0 * s, 30.0 * s),
                "Abbrechen",
            ),
            (
                Btn::ConfirmYes,
                Rect::new(c.x + c.w - 122.0 * s, y, 104.0 * s, 30.0 * s),
                "Speichern",
            ),
        ]
    }

    // --- „Mehr“ ----------------------------------------------------------------

    /// Zeilen von „Mehr“ je Spalte: Gruppenkopf (`None`) oder Feld mit
    /// Beschriftung.
    fn more_rows(&self) -> [Vec<(Option<Target>, String)>; 2] {
        let f = |k: &'static str| Some(Target::Field(Field::Key(k)));
        let mut left = vec![
            (None, "EINORDNUNG".to_string()),
            (f(matprop::SUBGROUP), "Untergruppe".into()),
            (Some(Target::Combo(ComboId::Trade)), "Gewerk".into()),
            (f("Verschnittpriorität"), "Verschnittpriorität".into()),
            (None, "BAUPHYSIK".into()),
            (f(matprop::MU), "Diffusion μ".into()),
            (f("c"), "Wärmekapazität c".into()),
            (Some(Target::Combo(ComboId::Euro)), "Baustoffklasse".into()),
            (None, "EIGENE KENNWERTE".into()),
        ];
        if let Some(x) = self.sel_id().and_then(|id| self.work.material(id)) {
            for k in x.props.keys().filter(|k| matprop::mat_prop(k).is_none()) {
                left.push((Some(Target::Field(Field::Custom(k.clone()))), k.clone()));
            }
        }
        match self.edit.as_ref().map(|e| &e.field) {
            Some(Field::NewKey) => left.push((Some(Target::Field(Field::NewKey)), String::new())),
            Some(Field::NewValue(k)) => {
                left.push((Some(Target::Field(Field::NewValue(k.clone()))), k.clone()))
            }
            _ => {}
        }
        left.push((Some(Target::AddKey), "+ Kennwert".into()));
        let right = vec![
            (None, "HERKUNFT".to_string()),
            (f(matprop::MAKER), "Hersteller".into()),
            (f("Produkt"), "Produkt".into()),
            (f("Bemerkung"), "Bemerkung".into()),
            (f("Preisquelle"), "Preisquelle".into()),
            (None, "DARSTELLUNG".into()),
            (
                Some(Target::Combo(ComboId::Pick(Pick::Fill))),
                "Schraffur".into(),
            ),
            (
                Some(Target::Combo(ComboId::Pick(Pick::Fg))),
                "Stift Schraffur".into(),
            ),
            (
                Some(Target::Combo(ComboId::Pick(Pick::Bg))),
                "Stift Grund".into(),
            ),
            (
                Some(Target::Combo(ComboId::Pick(Pick::Surface))),
                "Oberfläche".into(),
            ),
        ];
        [left, right]
    }

    /// Lage der Zeilen von „Mehr“ in Fensterkoordinaten (mit Bildlauf):
    /// Ziel, Beschriftung, Zeile (Beschriftung links, Feld rechts), Kopf?
    fn more_layout(&self, t: &Theme, w: &Win) -> Vec<(Option<Target>, String, Rect, Rect)> {
        let body = self.more_body(t, w);
        let s = w.scale;
        let col_w = ((body.w - 40.0 * s) * 0.5).floor();
        let label_w = 150.0 * s;
        let mut out = Vec::new();
        for (k, col) in self.more_rows().into_iter().enumerate() {
            let x = body.x + k as f32 * (col_w + 40.0 * s);
            let mut y = body.y - self.more_scroll;
            for (i, (tg, label)) in col.into_iter().enumerate() {
                let head = tg.is_none();
                if head && i > 0 {
                    y += 4.0 * s;
                }
                let h = if head { 22.0 * s } else { 27.0 * s };
                let row = Rect::new(x, y.round(), col_w, h);
                let field = if head || tg == Some(Target::AddKey) {
                    row
                } else {
                    Rect::new(
                        x + label_w,
                        y.round(),
                        col_w - label_w,
                        (t.size.mat_row * s - 2.0 * s).round(),
                    )
                };
                out.push((tg, label, row, field));
                y += h;
            }
        }
        out
    }

    fn more_height(&self, t: &Theme, w: &Win) -> f32 {
        let body = self.more_body(t, w);
        self.more_layout(t, w)
            .iter()
            .map(|x| x.2.y + x.2.h + self.more_scroll - body.y)
            .fold(0.0, f32::max)
    }

    /// Fortschritt des Aufklappens (0 zu, 1 offen).
    fn more_progress(&self, t: &Theme) -> f32 {
        let p = match self.more_at {
            Some(at) if t.size.anim_ms > 0.0 => {
                let k = at.elapsed().as_secs_f32() * 1000.0 / t.size.anim_ms;
                // ease-out
                let k = k.clamp(0.0, 1.0);
                1.0 - (1.0 - k) * (1.0 - k)
            }
            _ => 1.0,
        };
        if self.more {
            p
        } else {
            1.0 - p
        }
    }

    // --- Felder ------------------------------------------------------------------

    /// Text eines Felds außerhalb der Eingabe.
    fn field_value(&self, f: &Field) -> String {
        let x = self.sel_id().and_then(|id| self.work.material(id));
        let prop = |k: &str| {
            x.and_then(|x| x.props.get(k))
                .map_or(String::new(), prop_text)
        };
        match f {
            Field::Search => self.search.clone(),
            Field::Name => x.map_or(String::new(), |x| x.name.clone()),
            Field::Key("λ") => x.and_then(|x| x.lambda).map_or(String::new(), de),
            Field::Key("Rohdichte") => x.map_or(String::new(), |x| de(x.density)),
            Field::Key("Verschnittpriorität") => {
                x.map_or(String::new(), |x| x.priority.to_string())
            }
            Field::Key(k) => prop(k),
            Field::Custom(k) => prop(k),
            Field::NewKey | Field::NewValue(_) => String::new(),
        }
    }

    /// Einheit im Feld.
    fn field_unit(&self, f: &Field) -> &'static str {
        let x = self.sel_id().and_then(|id| self.work.material(id));
        match f {
            Field::Key("λ") => "W/(mK)",
            Field::Key("Rohdichte") => "kg/m³",
            Field::Key("c") => "J/(kg·K)",
            Field::Key(PRICE) => {
                let unit = x
                    .and_then(|x| stored_unit(x).or(matprop::price_unit(x.category)))
                    .unwrap_or("");
                matprop::price_unit_label(unit)
            }
            _ => "",
        }
    }

    /// Lage eines Felds in Fensterkoordinaten.
    fn field_rect(&self, t: &Theme, w: &Win, f: &Field) -> Option<Rect> {
        let nw = t.size.mat_num_w;
        Some(match f {
            Field::Search => self.search_rect(t, w),
            Field::Name => self.name_rect(t, w),
            Field::Key("λ") => self.basic_field(t, w, 0, 0.0, nw),
            Field::Key("Rohdichte") => self.basic_field(t, w, 1, 0.0, nw),
            Field::Key(PRICE) => self.basic_field(t, w, 2, 0.0, nw),
            Field::Key(PRICE_DATE) => self.basic_field(t, w, 2, nw + 50.0, 64.0),
            _ => {
                if self.more_progress_now() < 1.0 {
                    return None;
                }
                let tg = Target::Field(f.clone());
                self.more_layout(t, w)
                    .into_iter()
                    .find(|x| x.0.as_ref() == Some(&tg))
                    .map(|x| x.3)?
            }
        })
    }

    /// Ohne Uhr: „Mehr“ ganz offen?
    fn more_progress_now(&self) -> f32 {
        if self.more
            && self
                .more_at
                .is_none_or(|at| at.elapsed() > Duration::from_secs(2))
        {
            1.0
        } else if self.more {
            0.5
        } else {
            0.0
        }
    }

    fn combo_rect(&self, t: &Theme, w: &Win, fonts: &Fonts, id: ComboId) -> Option<Rect> {
        if id == ComboId::Category {
            return Some(self.cat_rect(t, w, fonts));
        }
        let tg = Target::Combo(id);
        self.more_layout(t, w)
            .into_iter()
            .find(|x| x.0.as_ref() == Some(&tg))
            .map(|x| x.3)
    }

    /// Verweise „Verwendet in“: Typ, Lage; dahinter der Rest als Text.
    fn use_links(&self, t: &Theme, w: &Win, fonts: &Fonts) -> (Vec<UseLink>, String) {
        let s = w.scale;
        let px = t.size.font_small * s;
        let f = fonts.regular.as_ref();
        let width = |x: &str| f.map_or(x.len() as f32 * 7.0 * s, |f| f.width(x, px));
        let Some(id) = self.sel_id() else {
            return (Vec::new(), String::new());
        };
        // Verbaute Typen vorn (die meisten Bauteile zuerst), dann Bauteile
        // mit eigenem Aufbau, unverbaute Typen zuletzt
        let mut parts: Vec<(usize, Option<LayerSetId>, String, String)> = Vec::new();
        for u in &self.uses_of(id) {
            match *u {
                Use::Type(tid, n) => {
                    if let Some(ts) = self.work.layer_set(tid) {
                        let tail = format!(" · {}", count_text(ts.category, n));
                        parts.push((n, Some(tid), ts.name.clone(), tail));
                    }
                }
                Use::Element(e) => {
                    if let Some(e) = self.work.element(e) {
                        parts.push((1, None, e.number.clone(), String::new()));
                    }
                }
            }
        }
        parts.sort_by_key(|p| std::cmp::Reverse(p.0));
        if parts.is_empty() {
            return (Vec::new(), "nicht verwendet".into());
        }
        let (ww, _) = self.dip(t, w);
        let r0 = self.basic_field(t, w, self.uses_row(), 0.0, 0.0);
        let right = self.frame(t, w).x + (ww - 16.0) * s;
        let (mut x, n) = (r0.x, parts.len());
        let mut links = Vec::new();
        let mut rest = String::new();
        for (i, (_, ty, name, tail)) in parts.into_iter().enumerate() {
            let sep = if i > 0 { width(", ") } else { 0.0 };
            let more = if i + 1 < n {
                width(&format!(" · +{} weitere", n - i - 1))
            } else {
                0.0
            };
            // Ein zu langer erster Name wird vor „· +N weitere“ gekürzt
            // (Prüfung ae)
            let name = if i == 0 && x + width(&name) + more > right {
                widgets::ellipsize(f, &name, px, (right - x - more).max(0.0))
            } else {
                name
            };
            let wl = width(&name);
            if i > 0 && x + sep + wl + width(&tail) + more > right {
                rest = format!(" · +{} weitere", n - i);
                break;
            }
            // Passt schon der erste samt Rest nicht: ohne Anzahl
            let tail = if x + wl + width(&tail) + more > right {
                String::new()
            } else {
                tail
            };
            x += sep;
            links.push(UseLink {
                ty,
                name,
                tail: tail.clone(),
                rect: Rect::new(x, r0.y, wl, r0.h),
                end: x + wl + width(&tail),
            });
            x += wl + width(&tail);
        }
        (links, rest)
    }

    // --- Ereignisse ----------------------------------------------------------

    /// Ein Ereignis, solange das Fenster offen ist (es nimmt alle Maus- und
    /// Tastenereignisse).
    pub fn handle(&mut self, e: &Event, cx: &mut Ctx) -> Out {
        let hovers = self.damage.len();
        let mut out = self.handle_now(e, cx);
        // Nur das Überfahren malt Teilbilder; alles andere das ganze Fenster
        if out.repaint || out.moved || out.closed {
            self.full_frame = true;
        }
        out.repaint |= self.damage.len() > hovers;
        out
    }

    fn handle_now(&mut self, e: &Event, cx: &mut Ctx) -> Out {
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
            Event::Wheel { delta, x, y, .. } => self.wheel(delta as f32, x, y, cx, &mut out),
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } => self.key(key, mods, cx, &mut out),
            Event::Text(c) => self.text(c, &mut out),
            Event::MouseLeave if self.hover.is_some() => {
                self.set_hover(None);
            }
            _ => {}
        }
        out
    }

    fn full(&mut self, out: &mut Out) {
        self.full_frame = true;
        out.repaint = true;
    }

    /// Ziel unter der Maus wechselt: vorheriges und neues kommen in die
    /// Teilbilder des nächsten Bildes ([`MaterialView::handle`] fordert es an).
    fn set_hover(&mut self, h: Option<Target>) {
        if h == self.hover {
            return;
        }
        let old = self.hover.take();
        self.hover = h;
        self.damage.push(old);
        self.damage.push(self.hover.clone());
    }

    /// Bereiche (Fensterkoordinaten), die die Hervorhebung von `h` ändert;
    /// `None`: unbekannt, ganz malen. Gleiche Lagen wie [`MaterialView::hit`].
    fn hover_area(&self, h: &Target, t: &Theme, w: &Win, fonts: &Fonts) -> Option<Vec<Rect>> {
        let row = |want: Row| {
            let i = self.rows().into_iter().position(|(r, _)| r == want)?;
            let r = intersect(self.row_rect(t, w, i), self.list_body(t, w))?;
            Some(vec![r])
        };
        Some(match h {
            Target::Card => Vec::new(),
            Target::Close => vec![self.close_rect(t, w)],
            Target::Tab(tab) => vec![self.tab_rect(t, w, *tab)],
            Target::PathLink => vec![self.path_link(t, w, fonts)?],
            Target::Group(c) => row(Row::Group(*c))?,
            Target::Row(g) => row(Row::Item(*g))?,
            Target::Field(f) => vec![self.field_rect(t, w, f)?],
            Target::Combo(id) => vec![self.combo_rect(t, w, fonts, *id)?],
            Target::Use(id) => self
                .use_links(t, w, fonts)
                .0
                .into_iter()
                .filter(|l| l.ty == Some(*id))
                .map(|l| l.rect)
                .collect(),
            Target::More => vec![self.more_link(t, w, fonts)],
            Target::AddKey => {
                if self.more_progress_now() < 1.0 {
                    return None;
                }
                let tg = Some(Target::AddKey);
                vec![self.more_layout(t, w).into_iter().find(|x| x.0 == tg)?.2]
            }
            // Der Hinweis am blassen „Löschen“ ist breiter als der Knopf
            Target::Btn(Btn::Delete) => return None,
            Target::Btn(b) => match &self.popup {
                Some(Popup::Confirm(_)) => {
                    let all = self.confirm_buttons(t, w);
                    vec![all.into_iter().find(|x| x.0 == *b)?.1]
                }
                _ if *b == Btn::Duplicate => vec![self.duplicate_rect(t, w)],
                _ => vec![self.foot_buttons(t, w).into_iter().find(|x| x.0 == *b)?.1],
            },
            Target::Item(j) => {
                let Some(Popup::List(l)) = &self.popup else {
                    return None;
                };
                let (r, k) = self.list_rect(t, w, fonts, l);
                let i = j.checked_sub(l.first).filter(|i| *i < k)?;
                let rh = (t.size.mat_row * w.scale).round();
                vec![Rect::new(r.x, r.y + i as f32 * rh, r.w, rh)]
            }
        })
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
                return;
            }
            Some(Drag::Select) => {
                if let Some(i) = self.caret_from_mouse(x, cx) {
                    if let Some(e) = self.edit.as_mut() {
                        e.text.place(i, true);
                    }
                    self.full(out);
                }
                return;
            }
            None => {}
        }
        let h = self.hit(t, &w, cx.fonts, x, y);
        self.set_hover(h);
    }

    fn mouse_down(&mut self, x: f64, y: f64, mods: Modifiers, cx: &mut Ctx, out: &mut Out) {
        let (t, w) = (cx.theme, cx.win);
        let hit = self.hit(t, &w, cx.fonts, x, y);
        self.full(out);
        self.message = None;
        // Offene Auswahlliste: Eintrag wählen oder schließen
        if let Some(Popup::List(_)) = &self.popup {
            match hit {
                Some(Target::Item(i)) => self.choose(i, cx, out),
                _ => self.popup = None,
            }
            return;
        }
        if let Some(Popup::Confirm(_)) = &self.popup {
            if let Some(Target::Btn(b @ (Btn::ConfirmNo | Btn::ConfirmYes))) = hit {
                self.pressed = Some(Target::Btn(b));
            }
            return;
        }
        // Klick ins Feld, das gerade bearbeitet wird: Schreibmarke
        if let (Some(Target::Field(f)), Some(e)) = (&hit, &self.edit) {
            if *f == e.field {
                if let Some(i) = self.caret_from_mouse(x, cx) {
                    if let Some(e) = self.edit.as_mut() {
                        e.text.place(i, mods.shift);
                    }
                }
                self.drag = Some(Drag::Select);
                return;
            }
        }
        // Sonst endet die Eingabe (übernommen)
        if self.edit.is_some() {
            let keep = matches!(
                (&hit, self.edit.as_ref().map(|e| &e.field)),
                (Some(Target::Card), _)
            );
            if !keep {
                self.end_edit(true);
            }
        }
        match hit {
            Some(Target::Field(f)) => {
                self.begin_edit(f);
            }
            Some(Target::Card) | None => {
                // Kopf: Fenster ziehen
                let fr = self.frame(t, &w);
                if fr.contains(x, y) && y < (fr.y + HEAD * w.scale) as f64 {
                    self.drag = Some(Drag::Window(x as f32 - fr.x, y as f32 - fr.y));
                }
            }
            Some(h) => self.pressed = Some(h),
        }
    }

    fn mouse_up(&mut self, x: f64, y: f64, cx: &mut Ctx, out: &mut Out) {
        self.drag = None;
        let Some(p) = self.pressed.take() else {
            return;
        };
        let (t, w) = (cx.theme, cx.win);
        let hit = self.hit(t, &w, cx.fonts, x, y);
        self.full(out);
        if hit.as_ref() == Some(&p) {
            self.click(p, cx, out);
        }
    }

    fn wheel(&mut self, delta: f32, x: f64, y: f64, cx: &mut Ctx, out: &mut Out) {
        let (t, w) = (cx.theme, cx.win);
        let s = w.scale;
        let step = delta * t.size.mat_row * s * 3.0;
        if let Some(Popup::List(l)) = self.popup.as_mut() {
            let n = l.items.len();
            let k = if delta > 0.0 { -1i64 } else { 1 } * 3;
            l.first = (l.first as i64 + k).clamp(0, n.saturating_sub(1) as i64) as usize;
            self.full(out);
            return;
        }
        if self.list_body(t, &w).contains(x, y) {
            let rows = self.rows().len() as f32 * (t.size.mat_row * s).round();
            let max = (rows - self.list_body(t, &w).h).max(0.0);
            self.scroll = (self.scroll - step).clamp(0.0, max);
            self.full(out);
        } else if self.more && self.more_body(t, &w).contains(x, y) {
            let max = (self.more_height(t, &w) - self.more_body(t, &w).h).max(0.0);
            self.more_scroll = (self.more_scroll - step).clamp(0.0, max);
            self.full(out);
        }
    }

    fn key(&mut self, key: Key, mods: Modifiers, cx: &mut Ctx, out: &mut Out) {
        self.full(out);
        if self.edit.is_some() {
            self.edit_key(key, mods, out);
            return;
        }
        if self.popup.is_some() {
            if key == Key::Escape {
                self.popup = None;
            } else if key == Key::Enter {
                if let Some(Popup::Confirm(_)) = self.popup {
                    self.click(Target::Btn(Btn::ConfirmYes), cx, out);
                }
            }
            return;
        }
        match key {
            Key::Char('Z') | Key::Char('Y') if mods.ctrl => {}
            Key::Char('F') if mods.ctrl => self.begin_edit(Field::Search),
            Key::Enter => self.click(Target::Btn(Btn::Ok), cx, out),
            Key::Escape => self.click(Target::Btn(Btn::Cancel), cx, out),
            _ => {}
        }
    }

    fn edit_key(&mut self, key: Key, mods: Modifiers, out: &mut Out) {
        let Some(e) = self.edit.as_mut() else {
            return;
        };
        let sh = mods.shift;
        let mut changed = true;
        match key {
            Key::Escape => {
                if e.field == Field::Search {
                    self.search.clear();
                }
                self.edit = None;
                return;
            }
            Key::Enter | Key::Tab => {
                self.end_edit(true);
                return;
            }
            Key::Backspace => e.text.backspace(),
            Key::Delete => e.text.delete(),
            Key::Left => {
                e.text.left(sh);
                changed = false;
            }
            Key::Right => {
                e.text.right(sh);
                changed = false;
            }
            Key::Home => {
                e.text.home(sh);
                changed = false;
            }
            Key::End => {
                e.text.end(sh);
                changed = false;
            }
            Key::Char('A') if mods.ctrl => {
                e.text.select_all();
                changed = false;
            }
            Key::Char('C') if mods.ctrl => {
                sk_platform::set_clipboard_text(e.text.selected());
                changed = false;
            }
            Key::Char('X') if mods.ctrl => {
                let cut = e.text.cut();
                sk_platform::set_clipboard_text(&cut);
            }
            Key::Char('V') if mods.ctrl => {
                let paste = sk_platform::clipboard_text().unwrap_or_default();
                let line: String = paste.lines().next().unwrap_or("").into();
                e.text.insert(&line);
            }
            Key::Char('Z') if mods.ctrl => changed = e.text.undo(),
            _ => changed = false,
        }
        if changed {
            e.invalid = false;
            self.after_typing(out);
        }
    }

    fn text(&mut self, ch: char, out: &mut Out) {
        let Some(e) = self.edit.as_mut() else {
            return;
        };
        if ch.is_control() {
            return;
        }
        if e.field.numeric() && !(ch.is_ascii_digit() || matches!(ch, ',' | '.' | '-')) {
            return;
        }
        let mut b = [0; 4];
        e.text.insert(ch.encode_utf8(&mut b));
        e.invalid = false;
        self.after_typing(out);
    }

    /// Nach jeder Änderung im Feld: die Suche filtert sofort.
    fn after_typing(&mut self, out: &mut Out) {
        if let Some(e) = &self.edit {
            if e.field == Field::Search {
                self.search = e.text.text.to_string();
                self.scroll = 0.0;
            }
        }
        out.repaint = true;
    }

    fn begin_edit(&mut self, f: Field) {
        if self.tab == Tab::Company && f != Field::Search {
            return;
        }
        let orig = self.field_value(&f);
        let mut text = TextEdit::new(&orig);
        text.select_all();
        self.edit = Some(Edit {
            field: f,
            text,
            orig,
            invalid: false,
        });
    }

    /// Beendet die Eingabe. `commit`: übernehmen (ungültig bleibt das Feld
    /// offen und rot), sonst verwerfen.
    fn end_edit(&mut self, commit: bool) {
        let Some(e) = self.edit.take() else {
            return;
        };
        if !commit {
            return;
        }
        let text = e.text.text.to_string();
        let ok = match &e.field {
            Field::Search => {
                self.search = text;
                true
            }
            Field::NewKey => {
                let k = matprop::normalize_key(&text);
                if k.is_empty() {
                    true
                } else if matprop::mat_prop(&k).is_some()
                    || OWN_FIELDS.contains(&k.as_str())
                    || self
                        .sel_id()
                        .and_then(|id| self.work.material(id))
                        .is_some_and(|x| x.props.contains_key(&k))
                {
                    false
                } else {
                    self.edit = Some(Edit {
                        field: Field::NewValue(k),
                        text: TextEdit::new(""),
                        orig: String::new(),
                        invalid: false,
                    });
                    return;
                }
            }
            Field::NewValue(k) => {
                text.trim().is_empty()
                    || self
                        .sel_id()
                        .is_some_and(|id| add_custom(&mut self.work, id, k, &text))
            }
            f => {
                if text == e.orig {
                    true
                } else {
                    let key = f.key().unwrap_or_default();
                    self.sel_id()
                        .is_some_and(|id| input(&mut self.work, id, &key, &text))
                }
            }
        };
        if !ok {
            self.edit = Some(Edit { invalid: true, ..e });
        }
    }

    fn caret_from_mouse(&self, x: f64, cx: &Ctx) -> Option<usize> {
        let (t, w) = (cx.theme, cx.win);
        let s = w.scale;
        let e = self.edit.as_ref()?;
        let r = self.field_rect(t, &w, &e.field)?;
        let text = e.text.text.as_str();
        let (font, px, tx) = match &e.field {
            Field::Name => (
                cx.fonts.bold.as_ref().or(cx.fonts.regular.as_ref()),
                t.size.mat_name_font * s,
                widgets::text_field_x(r, s, t),
            ),
            f if f.numeric() || *f == Field::Key(PRICE_DATE) => (
                cx.fonts.regular.as_ref(),
                t.size.font_small * s,
                widgets::field_x(cx.fonts, r, text, self.field_unit(f), s, t),
            ),
            _ => (
                cx.fonts.regular.as_ref(),
                t.size.font_small * s,
                widgets::text_field_x(r, s, t),
            ),
        };
        Some(widgets::caret_at(font, text, px, tx, x as f32))
    }

    fn click(&mut self, tg: Target, cx: &mut Ctx, out: &mut Out) {
        let (t, w) = (cx.theme, cx.win);
        match tg {
            Target::Close | Target::Btn(Btn::Cancel) => {
                self.cancel(cx.scene);
                out.closed = true;
            }
            Target::Btn(Btn::Ok) => {
                self.end_edit(true);
                if self.edit.is_some() {
                    return;
                }
                out.applied = self.ok_report(cx.scene, &mut out.problems);
                out.closed = true;
            }
            Target::Tab(tab) => {
                if self.tab != tab {
                    self.end_edit(true);
                    self.edit = None;
                    self.tab = tab;
                    self.scroll = 0.0;
                    if tab == Tab::Company && self.csel.is_none() {
                        self.csel = self.first_item();
                    }
                }
            }
            Target::PathLink => out.pick_company = true,
            Target::Group(c) => {
                if let Some(i) = self.closed_groups.iter().position(|x| *x == c) {
                    self.closed_groups.remove(i);
                } else {
                    self.closed_groups.push(c);
                }
            }
            Target::Row(g) => match self.tab {
                Tab::Project => {
                    self.sel = Some(g);
                    self.more_scroll = 0.0;
                }
                Tab::Company => self.csel = Some(g),
            },
            Target::More => {
                self.more = !self.more;
                self.more_at = Some(Instant::now());
                self.more_scroll = 0.0;
            }
            Target::AddKey => self.begin_edit(Field::NewKey),
            Target::Use(id) => {
                out.open_type = self.work.layer_set(id).map(|x| x.guid);
                out.applied = self.ok_report(cx.scene, &mut out.problems);
                out.closed = true;
            }
            Target::Combo(id) => self.open_list(id, t, &w, cx.fonts),
            Target::Btn(b) if self.btn_disabled(b) => {}
            Target::Btn(Btn::Duplicate) => {
                if let Some(id) = self.sel_id() {
                    if let Some(n) = self.work.duplicate_material(id) {
                        self.sel = self.work.material(n).map(|x| x.guid);
                        self.begin_edit(Field::Name);
                    }
                }
            }
            Target::Btn(Btn::Delete) => {
                if let Some(id) = self.sel_id() {
                    if self.work.remove_material(id) {
                        self.sel = self.first_item();
                    }
                }
            }
            Target::Btn(Btn::Export) => {
                if self.company.is_some() {
                    self.popup = self.sel.map(Popup::Confirm);
                } else {
                    out.pick_company = true;
                }
            }
            Target::Btn(Btn::ConfirmNo) => self.popup = None,
            Target::Btn(Btn::ConfirmYes) => {
                let Some(Popup::Confirm(g)) = self.popup.take() else {
                    return;
                };
                self.export(g, cx);
            }
            Target::Btn(Btn::Import) => {
                let (Some(g), Some((lib, _))) = (self.csel, &self.company) else {
                    return;
                };
                if import_material(&mut self.work, lib, g) {
                    self.message =
                        Some("Übernommen; mit OK wird es ein Rückgängig-Schritt.".into());
                }
            }
            Target::Item(i) => self.choose(i, cx, out),
            Target::Field(_) | Target::Card => {}
        }
    }

    /// Blasser Knopf: reagiert nicht (Review 3n/8).
    fn btn_disabled(&self, b: Btn) -> bool {
        match b {
            Btn::Delete => !self.can_delete(),
            Btn::Export => self.sel_id().is_none_or(|id| {
                self.work
                    .material(id)
                    .is_none_or(|x| x.category == MatCategory::Air)
            }),
            Btn::Import => self.csel.is_none_or(|g| self.lib_mat(g).is_none()),
            _ => false,
        }
    }

    /// „In den Firmenkatalog …“ nach der Rückfrage: schreibt sofort. Hat
    /// sich die Datei inzwischen geändert, wird sie neu geladen und der
    /// Baustoff noch einmal hineingeschrieben.
    fn export(&mut self, g: Guid, cx: &mut Ctx) {
        let Some(id) = self.mat_id(g) else {
            return;
        };
        let Some(company) = cx.company.as_deref_mut() else {
            return;
        };
        let mut result = company.save_material(&self.work, id);
        if result == SaveResult::Changed {
            company.reload(cx.company_standard);
            result = company.save_material(&self.work, id);
        }
        self.message = Some(match result {
            SaveResult::Saved => "Im Firmenkatalog gespeichert.".into(),
            SaveResult::Changed => {
                "Der Firmenkatalog ändert sich gerade; bitte noch einmal.".into()
            }
            SaveResult::Failed(e) => e,
        });
        let c = cx.company.as_deref();
        self.set_company(c);
    }

    fn open_list(&mut self, id: ComboId, t: &Theme, w: &Win, _fonts: &Fonts) {
        let Some(mid) = self.sel_id() else {
            return;
        };
        let Some(x) = self.work.material(mid).cloned() else {
            return;
        };
        let s = w.scale;
        let (items, icons, sel) = match id {
            ComboId::Category => {
                let v: Vec<String> = CATS.iter().map(|c| c.name().to_string()).collect();
                let sel = CATS.iter().position(|c| *c == x.category).unwrap_or(0);
                let n = v.len();
                (v, vec![None; n], sel)
            }
            ComboId::Trade => {
                let tr = self.work.trades();
                let v: Vec<String> = tr
                    .iter()
                    .map(|t| format!("{} {}", t.code, t.name))
                    .collect();
                let sel = tr.iter().position(|t| Some(t.id()) == x.trade).unwrap_or(0);
                let n = v.len();
                (v, vec![None; n], sel)
            }
            ComboId::Euro => {
                let v: Vec<String> = matprop::EUROCLASSES.iter().map(|c| c.to_string()).collect();
                let cur = self.field_value(&Field::Key(matprop::EUROCLASS));
                let sel = v.iter().position(|c| *c == cur).unwrap_or(0);
                let n = v.len();
                (v, vec![None; n], sel)
            }
            ComboId::Pick(p) => {
                let mut tiles = self.tiles.borrow_mut();
                attr_pick::pick_items(&self.work, &mut tiles, t, s, p, &x.display())
            }
        };
        self.popup = Some(Popup::List(List {
            id,
            items,
            icons,
            sel,
            first: 0,
        }));
    }

    fn choose(&mut self, i: usize, _cx: &mut Ctx, _out: &mut Out) {
        let Some(Popup::List(l)) = self.popup.take() else {
            return;
        };
        let Some(mid) = self.sel_id() else {
            return;
        };
        match l.id {
            ComboId::Category => {
                if let Some(c) = CATS.get(i) {
                    let same = self.work.material(mid).is_some_and(|x| x.category == *c);
                    if !same && !input(&mut self.work, mid, "Art", c.name()) {
                        // Review 3n/8: nicht still ablehnen
                        self.message = Some(format!(
                            "„{}“ passt nicht zu den Kennwerten (z. B. Preis oder Rohdichte); erst diese leeren.",
                            c.name()
                        ));
                    }
                }
            }
            ComboId::Trade => {
                let t = self.work.trades().get(i).map(|t| t.id());
                if t.is_some() {
                    set_trade(&mut self.work, mid, t);
                }
            }
            ComboId::Euro => {
                if let Some(c) = matprop::EUROCLASSES.get(i) {
                    input(&mut self.work, mid, matprop::EUROCLASS, c);
                }
            }
            ComboId::Pick(p) => {
                let Some(mut d) = self.work.material(mid).map(|x| x.display()) else {
                    return;
                };
                if attr_pick::pick_apply(&self.work, p, i, &mut d) {
                    self.work.set_material_display(mid, d);
                }
            }
        }
    }

    // --- Treffer ---------------------------------------------------------------

    fn list_rect(&self, t: &Theme, w: &Win, fonts: &Fonts, l: &List) -> (Rect, usize) {
        let s = w.scale;
        let anchor = self
            .combo_rect(t, w, fonts, l.id)
            .unwrap_or_else(|| self.name_rect(t, w));
        let rh = (t.size.mat_row * s).round();
        let f = self.frame(t, w);
        let bottom = f.y + f.h - 8.0 * s;
        let top = f.y + HEAD * s;
        let below = ((bottom - anchor.y - anchor.h - 4.0 * s) / rh)
            .floor()
            .max(0.0) as usize;
        let above = ((anchor.y - 4.0 * s - top) / rh).floor().max(0.0) as usize;
        let n = l.items.len();
        let ww = anchor.w.max(200.0 * s);
        if n <= below || below >= above {
            let k = n.min(below.max(1));
            (
                Rect::new(anchor.x, anchor.y + anchor.h + 4.0 * s, ww, k as f32 * rh),
                k,
            )
        } else {
            let k = n.min(above.max(1));
            let h = k as f32 * rh;
            (Rect::new(anchor.x, anchor.y - 4.0 * s - h, ww, h), k)
        }
    }

    fn hit(&self, t: &Theme, w: &Win, fonts: &Fonts, x: f64, y: f64) -> Option<Target> {
        let s = w.scale;
        match &self.popup {
            Some(Popup::List(l)) => {
                let (r, k) = self.list_rect(t, w, fonts, l);
                if r.contains(x, y) {
                    let rh = (t.size.mat_row * s).round();
                    let i = ((y as f32 - r.y) / rh).floor() as usize;
                    return Some(Target::Item(l.first + i.min(k.saturating_sub(1))));
                }
                return None;
            }
            Some(Popup::Confirm(_)) => {
                for (b, r, _) in self.confirm_buttons(t, w) {
                    if r.contains(x, y) {
                        return Some(Target::Btn(b));
                    }
                }
                return self
                    .confirm_card(t, w)
                    .contains(x, y)
                    .then_some(Target::Card);
            }
            None => {}
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
        for (b, r, _) in self.foot_buttons(t, w) {
            if r.contains(x, y) {
                return Some(Target::Btn(b));
            }
        }
        if self.search_rect(t, w).contains(x, y) {
            return Some(Target::Field(Field::Search));
        }
        let body = self.list_body(t, w);
        if body.contains(x, y) {
            for (i, (row, _)) in self.rows().into_iter().enumerate() {
                if self.row_rect(t, w, i).contains(x, y) {
                    return Some(match row {
                        Row::Group(c) => Target::Group(c),
                        Row::Item(g) => Target::Row(g),
                    });
                }
            }
            return Some(Target::Card);
        }
        if self.tab == Tab::Company {
            return Some(Target::Card);
        }
        self.sel_id()?;
        if self.duplicate_rect(t, w).contains(x, y) {
            return Some(Target::Btn(Btn::Duplicate));
        }
        if self.name_rect(t, w).contains(x, y) {
            return Some(Target::Field(Field::Name));
        }
        if self.cat_rect(t, w, fonts).contains(x, y) {
            return Some(Target::Combo(ComboId::Category));
        }
        for f in [
            Field::Key("λ"),
            Field::Key("Rohdichte"),
            Field::Key(PRICE),
            Field::Key(PRICE_DATE),
        ] {
            if self.field_rect(t, w, &f).is_some_and(|r| r.contains(x, y)) {
                return Some(Target::Field(f));
            }
        }
        for l in self.use_links(t, w, fonts).0 {
            if let Some(id) = l.ty.filter(|_| l.rect.contains(x, y)) {
                return Some(Target::Use(id));
            }
        }
        if self.more_link(t, w, fonts).contains(x, y) {
            return Some(Target::More);
        }
        if self.more && self.more_body(t, w).contains(x, y) {
            for (tg, _, row, field) in self.more_layout(t, w) {
                match tg {
                    Some(Target::AddKey) => {
                        let lw = fonts
                            .bold
                            .as_ref()
                            .or(fonts.regular.as_ref())
                            .map_or(80.0 * s, |f| f.width("+ Kennwert", t.size.font_small * s));
                        let r =
                            Rect::new(field.x + 150.0 * s - 2.0 * s, row.y, lw + 4.0 * s, row.h);
                        if r.contains(x, y) {
                            return Some(Target::AddKey);
                        }
                    }
                    Some(tg) if field.contains(x, y) => return Some(tg),
                    _ => {}
                }
            }
        }
        Some(Target::Card)
    }

    // --- Zeit --------------------------------------------------------------------

    fn animating(&self, t: &Theme) -> bool {
        self.more_at
            .is_some_and(|at| at.elapsed().as_secs_f32() * 1000.0 < t.size.anim_ms)
    }

    /// Klappliste, Inline-Eingabe, Rückfrage oder Ziehen offen: Esc gehört
    /// zuerst ihnen (Paket 9, Nachtrag H9-1).
    pub fn busy(&self) -> bool {
        self.edit.is_some() || self.popup.is_some() || self.drag.is_some()
    }

    pub fn wait(&self, t: &Theme) -> Option<Duration> {
        (self.animating(t) || self.tiles.borrow().busy()).then_some(Duration::from_millis(16))
    }

    /// Ein Bild weiter: `true`, wenn neu zu zeichnen ist (auch wenn eine
    /// Verbandstabelle der Würfelvorschau fertig wurde oder einblendet).
    pub fn tick(&mut self, t: &Theme) -> bool {
        let was = self.more_at.is_some() || self.tiles.borrow().tick();
        if self.more_at.is_some() && !self.animating(t) {
            self.more_at = None;
        }
        if was {
            self.full_frame = true;
        }
        was
    }

    pub fn cursor(&self) -> Cursor {
        match &self.hover {
            Some(Target::Field(_)) if self.popup.is_none() => Cursor::IBeam,
            Some(
                Target::Use(_) | Target::More | Target::AddKey | Target::PathLink | Target::Row(_),
            ) => Cursor::Hand,
            _ => Cursor::Arrow,
        }
    }

    /// Hinweis an der Maus (Hinweise zum Löschen stehen im Fenster).
    pub fn tip(&self) -> Option<String> {
        None
    }
}

// --- Zeichnen -------------------------------------------------------------------

/// Unterstrich unter einem Verweis beim Überfahren.
fn underline(c: &mut Canvas, x: f32, y: f32, w: f32, s: f32, col: Rgba) {
    c.fill_rect(x, (y + 2.0 * s).round(), w, s.round().max(1.0), col);
}

impl MaterialView {
    /// Malt das Fenster: das ganze Bild oder die Ausschnitte, die sich seit
    /// dem letzten geändert haben.
    /// Nach einem Wechsel unter der Maus nur die betroffenen Zeilen, Felder
    /// und Knöpfe (B6, wie U7), pixelgleich zum ganzen Bild; sonst, nach
    /// Theme- oder Skalierungswechsel und beim ersten Bild alles.
    pub fn paint_frame(&mut self, t: &Theme, fonts: &Fonts, w: &Win) -> Frame {
        let f = self.frame(t, w);
        let s = w.scale;
        let m = (t.size.panel_shadow * s).round();
        let (cw, ch) = ((f.w + 2.0 * m) as usize, (f.h + 2.0 * m) as usize);
        let (ox, oy) = (f.x - m, f.y - m);
        let key = (cw, ch, s.to_bits(), t.rev, ox as i32, oy as i32);
        let targets = std::mem::take(&mut self.damage);
        let mut full = std::mem::take(&mut self.full_frame)
            || targets.is_empty()
            || self.img.is_none()
            || self.img_key != Some(key);
        let mut rects = Vec::new();
        for h in targets.iter().flatten() {
            if full {
                break;
            }
            match self.hover_area(h, t, w, fonts) {
                Some(v) => rects.extend(v),
                None => full = true,
            }
        }
        let pad = (4.0 * s).ceil();
        let parts = pixel_strips(&rects, pad, (ox, oy), (cw, ch));
        let area: usize = parts.iter().map(|p| (p.2 - p.0) * (p.3 - p.1)).sum();
        if full || area * 2 > cw * ch {
            let (c, x, y) = self.paint(t, fonts, w);
            let px = c.to_premul_rgba8();
            let (cw, ch) = (c.width as u32, c.height as u32);
            self.img = Some(c);
            self.img_key = Some(key);
            return Frame::Full {
                x,
                y,
                w: cw,
                h: ch,
                px,
            };
        }
        let mut out = Vec::with_capacity(parts.len());
        for (x0, y0, x1, y1) in parts {
            let mut sub = std::mem::replace(&mut self.scratch, Canvas::new(0, 0));
            sub.reuse(x1 - x0, y1 - y0);
            // Grund des Ausschnitts, dann das Fenster darüber wie im ganzen Bild
            if let Some((_, g)) = &self.ground {
                sub.set_origin(x0 as f32, y0 as f32);
                sub.copy_rect_from(g, x0 as f32, y0 as f32, x1 as f32, y1 as f32);
            }
            sub.set_origin(ox + x0 as f32, oy + y0 as f32);
            self.paint_into(&mut sub, t, fonts, w);
            if let Some(img) = self.img.as_mut() {
                img.put(&sub, x0, y0);
            }
            out.push((
                x0 as i32,
                y0 as i32,
                sub.width as u32,
                sub.height as u32,
                sub.to_premul_rgba8(),
            ));
            self.scratch = sub;
        }
        Frame::Parts(out)
    }

    /// Das ganze Fenster samt Schatten; Lage der linken oberen Ecke.
    pub fn paint(&mut self, t: &Theme, fonts: &Fonts, w: &Win) -> (Canvas, i32, i32) {
        let f = self.frame(t, w);
        let s = w.scale;
        let m = (t.size.panel_shadow * s).round();
        let (cw, ch) = ((f.w + 2.0 * m) as usize, (f.h + 2.0 * m) as usize);
        let mut c = self.img.take().unwrap_or_else(|| Canvas::new(0, 0));
        c.reuse(cw, ch);
        // Grund mit Schatten einmal je Größe, dann nur kopiert (Review 3n)
        let key = (cw, ch, s.to_bits(), t.rev);
        if self.ground.as_ref().map(|g| g.0) != Some(key) {
            let mut g = Canvas::new(cw, ch);
            g.set_origin(f.x - m, f.y - m);
            widgets::panel(&mut g, f, s, t);
            g.set_origin(0.0, 0.0);
            self.ground = Some((key, g));
        }
        if let Some((_, g)) = &self.ground {
            c.copy_rows(g, 0, ch);
        }
        c.set_origin(f.x - m, f.y - m);
        self.paint_into(&mut c, t, fonts, w);
        c.set_origin(0.0, 0.0);
        let (x, y) = self.origin(t, w);
        (c, x, y)
    }

    fn paint_into(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let f = self.frame(t, w);
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let line = s.round().max(1.0);
        // Kopf
        label(
            c,
            bold,
            "Baustoffe",
            t.size.font_title * s,
            f.x + 20.0 * s,
            f.y + 32.0 * s,
            u.text,
        );
        let p0 = self.tab_rect(t, w, Tab::Project);
        let seg = Rect::new(p0.x - 2.0 * s, p0.y - 2.0 * s, 218.0 * s, 32.0 * s);
        rounded(c, seg, 7.0 * s, u.field);
        for tab in [Tab::Project, Tab::Company] {
            let r = self.tab_rect(t, w, tab);
            let on = self.tab == tab;
            if on {
                rounded(c, r, 6.0 * s, u.pressed);
                outline(c, r, 6.0 * s, line, u.accent);
            } else if self.hover == Some(Target::Tab(tab)) {
                rounded(c, r, 6.0 * s, u.hover);
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
                c,
                font,
                text,
                px,
                r.x + (r.w - tw) * 0.5,
                r.y + 19.0 * s,
                if on { u.text } else { u.text_dim },
            );
        }
        if self.tab == Tab::Company {
            let base = self.r(t, w, 406.0, 12.0, 0.0, 28.0);
            let px = t.size.font_small * s;
            if let Some((_, path)) = &self.company {
                let p = widgets::ellipsize(regular, path, px, 360.0 * s);
                label(c, regular, &p, px, base.x, base.y + 19.0 * s, u.text_dim);
            }
            if let Some(l) = self.path_link(t, w, fonts) {
                let hov = self.hover == Some(Target::PathLink);
                let col = if hov { u.accent_hover } else { u.accent };
                label(
                    c,
                    regular,
                    "ändern …",
                    px,
                    l.x + 2.0 * s,
                    l.y + 19.0 * s,
                    col,
                );
                if hov {
                    underline(c, l.x + 2.0 * s, l.y + 19.0 * s, l.w - 4.0 * s, s, col);
                }
            }
        }
        let cr = self.close_rect(t, w);
        if self.hover == Some(Target::Close) {
            rounded(c, cr, 6.0 * s, u.hover);
        }
        let (cx0, cy0, d) = (cr.x + cr.w * 0.5, cr.y + cr.h * 0.5, 5.5 * s);
        let mut p = Path::new();
        p.segment((cx0 - d, cy0 - d), (cx0 + d, cy0 + d), 1.4 * s);
        p.segment((cx0 - d, cy0 + d), (cx0 + d, cy0 - d), 1.4 * s);
        c.fill(&p, u.text_dim);
        c.fill_rect(f.x, f.y + HEAD * s, f.w, line, u.border);
        c.fill_rect(f.x, f.y + f.h - FOOT * s, f.w, line, u.border);
        c.fill_rect(
            (f.x + t.size.mat_list_w * s).round(),
            f.y + HEAD * s,
            line,
            f.h - (HEAD + FOOT) * s,
            u.border,
        );
        self.paint_list(c, t, fonts, w);
        match self.tab {
            Tab::Project => self.paint_project(c, t, fonts, w),
            Tab::Company => self.paint_company(c, t, fonts, w),
        }
        // Fuß
        for (b, r, text) in self.foot_buttons(t, w) {
            let disabled = self.btn_disabled(b);
            let st = ButtonState {
                hover: self.hover == Some(Target::Btn(b)) && !disabled,
                pressed: self.pressed == Some(Target::Btn(b)),
                active: matches!(b, Btn::Ok | Btn::Import) && !disabled,
                disabled,
            };
            widgets::button(c, fonts, r, text, st, s, t);
        }
        if let Some(msg) = &self.message {
            let b = self.foot_buttons(t, w);
            let x0 = b
                .iter()
                .find(|x| x.0 == Btn::Export || x.0 == Btn::Import)
                .map_or(f.x, |x| x.1.x + x.1.w);
            label(
                c,
                regular,
                msg,
                t.size.font_small * s,
                x0 + 16.0 * s,
                f.y + f.h - 23.0 * s,
                u.text_dim,
            );
        }
        // Hinweis am blassen „Löschen“
        if self.hover == Some(Target::Btn(Btn::Delete))
            && !self.can_delete()
            && self.tab == Tab::Project
        {
            if let Some(id) = self.sel_id() {
                let why = delete_why(&self.work, id, &self.uses_of(id));
                let r = self.delete_tip_rect(t, w);
                let pxs = t.size.font_small * s;
                let tw = regular
                    .map_or(r.w, |ft| ft.width(&why, t.size.font_detail * s))
                    .max(bold.map_or(0.0, |ft| ft.width("Löschen nicht möglich", pxs)))
                    + 24.0 * s;
                let r = Rect::new(r.x, r.y, tw.max(r.w), r.h);
                rounded(c, r, 6.0 * s, u.border);
                rounded(
                    c,
                    Rect::new(r.x + line, r.y + line, r.w - 2.0 * line, r.h - 2.0 * line),
                    6.0 * s - line,
                    u.tooltip_bg,
                );
                label(
                    c,
                    bold,
                    "Löschen nicht möglich",
                    pxs,
                    r.x + 12.0 * s,
                    r.y + 18.0 * s,
                    u.tooltip_text,
                );
                label(
                    c,
                    regular,
                    &why,
                    t.size.font_detail * s,
                    r.x + 12.0 * s,
                    r.y + 35.0 * s,
                    u.text_dim,
                );
            }
        }
        match &self.popup {
            Some(Popup::List(l)) => self.paint_popup_list(c, t, fonts, w, l),
            Some(Popup::Confirm(g)) => self.paint_confirm(c, t, fonts, w, *g),
            None => {}
        }
    }

    /// Kleines Schraffurfeld eines Baustoffs in der Liste.
    fn mark_tile(&self, t: &Theme, s: f32, g: Guid) -> Option<Canvas> {
        let stamp = (
            self.work.revision(),
            self.work.attr().rev(),
            s.to_bits(),
            t.rev,
        );
        let mut cache = self.marks.borrow_mut();
        if cache.0 != Some(stamp) {
            *cache = (Some(stamp), Vec::new());
        }
        if let Some((_, c)) = cache.1.iter().find(|x| x.0 == g) {
            return Some(c.clone());
        }
        let d = match self.mat_id(g).and_then(|id| self.work.material(id)) {
            Some(x) => x.display(),
            None => self.lib_display(g)?,
        };
        let px = (t.size.mat_mark * s).round();
        let img = attr_pick::tile(&self.work, t, &d, px, px, s);
        cache.1.push((g, img.clone()));
        Some(img)
    }

    /// Darstellung eines Katalog-Baustoffs in den Attributen der Kopie (über
    /// die Guids); fehlt eine, `None`.
    fn lib_display(&self, g: Guid) -> Option<MaterialDisplay> {
        let lib = self.lib()?;
        let x = self.lib_mat(g)?;
        let a = self.work.attr();
        let fill = lib.fills.get(x.cut_fill)?.guid;
        let fg = lib.pens.get(x.cut_fg)?.guid;
        let bg = lib.pens.get(x.cut_bg)?.guid;
        let sf = lib.surfaces.get(x.surface)?.guid;
        Some(MaterialDisplay {
            cut_fill: a.fills().iter().find(|(_, f)| f.guid == fill)?.0,
            cut_fg: a.pens().iter().find(|(_, p)| p.guid == fg)?.0,
            cut_bg: a.pens().iter().find(|(_, p)| p.guid == bg)?.0,
            surface: a.surfaces().iter().find(|(_, o)| o.guid == sf)?.0,
        })
    }

    fn paint_list(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        // Suche
        let r = self.search_rect(t, w);
        let editing = self.edit.as_ref().filter(|e| e.field == Field::Search);
        let text = editing.map_or(self.search.clone(), |e| e.text.text.to_string());
        let st = FieldState {
            text: &text,
            hover: self.hover == Some(Target::Field(Field::Search)),
            focus: editing.is_some(),
            caret: editing.map(|e| e.text.caret),
            select: editing.map(|e| e.text.selection()).filter(|x| x.0 != x.1),
            ..FieldState::default()
        };
        widgets::text_field(c, fonts, r, &st, s, t);
        if text.is_empty() && editing.is_none() {
            label(
                c,
                regular,
                "Suche …",
                t.size.font_small * s,
                widgets::text_field_x(r, s, t),
                r.y + (r.h + regular.map_or(9.0 * s, |f| f.cap_height(t.size.font_small * s)))
                    * 0.5,
                u.text_disabled,
            );
        }
        // Zeilen in eigenem Bild, damit angeschnittene sauber enden
        let body = self.list_body(t, w);
        let mut sub = Canvas::new(body.w.max(1.0) as usize, body.h.max(1.0) as usize);
        sub.set_origin(body.x, body.y);
        sub.clear(u.bg);
        let px = t.size.font * s;
        let cap = regular.map_or(px * 0.7, |f| f.cap_height(px));
        let compare = if self.tab == Tab::Company {
            self.lib().map(|l| compare_materials(&self.work, l))
        } else {
            None
        };
        for (i, (row, name)) in self.rows().into_iter().enumerate() {
            let r = self.row_rect(t, w, i);
            if r.y + r.h < body.y || r.y > body.y + body.h {
                continue;
            }
            let base = (r.y + (r.h + cap) * 0.5).round();
            match row {
                Row::Group(cat) => {
                    let open = !self.closed_groups.contains(&cat);
                    widgets::disclosure(
                        &mut sub,
                        r.x + 8.0 * s,
                        r.y + r.h * 0.5,
                        open,
                        u.text_dim,
                        s,
                    );
                    label(
                        &mut sub,
                        bold,
                        &name,
                        t.size.font_small * s,
                        r.x + 20.0 * s,
                        base,
                        u.text_dim,
                    );
                }
                Row::Item(g) => {
                    let sel = match self.tab {
                        Tab::Project => self.sel == Some(g),
                        Tab::Company => self.csel == Some(g),
                    };
                    if sel {
                        rounded(&mut sub, r, 4.0 * s, u.pressed);
                        sub.fill_rect(r.x, r.y, (3.0 * s).round(), r.h, u.accent);
                    } else if self.hover == Some(Target::Row(g)) {
                        rounded(&mut sub, r, 4.0 * s, u.hover);
                    }
                    let ts = (t.size.mat_mark * s).round();
                    if let Some(img) = self.mark_tile(t, s, g) {
                        sub.blit(
                            &img,
                            (r.x + 20.0 * s) as i32,
                            (r.y + (r.h - ts) * 0.5).round() as i32,
                        );
                    }
                    let font = if sel { bold } else { regular };
                    let nx = r.x + 46.0 * s;
                    let mark = compare
                        .as_ref()
                        .and_then(|v| v.iter().find(|x| x.0 == g).map(|x| x.1));
                    let (mark_text, mark_col) = match mark {
                        Some(TypeState::Same) => ("wie im Projekt", u.text_same),
                        Some(TypeState::Differs) => ("abweichend", u.accent),
                        Some(TypeState::OnlyProject) => ("nicht in Firma", u.text_dim),
                        Some(TypeState::OnlyCompany) => ("nicht im Projekt", u.text_dim),
                        None => ("", u.text_dim),
                    };
                    // Kennzeichen klein wie im Baum (Prüfung af, soll-p5-3)
                    let mpx = t.size.tree_small * s;
                    let mw = regular.map_or(0.0, |f| f.width(mark_text, mpx));
                    let room =
                        r.x + r.w - nx - 8.0 * s - if mw > 0.0 { mw + 8.0 * s } else { 12.0 * s };
                    let shown = widgets::ellipsize(font, &name, px, room);
                    label(&mut sub, font, &shown, px, nx, base, u.text);
                    if mw > 0.0 {
                        label(
                            &mut sub,
                            regular,
                            mark_text,
                            mpx,
                            r.x + r.w - 8.0 * s - mw,
                            base,
                            mark_col,
                        );
                    } else if self.tab == Tab::Project
                        && self.mat_id(g).is_some_and(|id| self.used(id))
                    {
                        // Punkt „im Projekt verwendet“
                        let tw = font.map_or(0.0, |f| f.width(&shown, px));
                        let mut p = Path::new();
                        let rad = 2.5 * s;
                        p.rounded_rect(
                            nx + tw + 6.0 * s,
                            base - cap * 0.5 - rad,
                            2.0 * rad,
                            2.0 * rad,
                            rad,
                        );
                        sub.fill(&p, u.text_dim);
                    }
                }
            }
        }
        sub.set_origin(0.0, 0.0);
        let (ox, oy) = c.origin();
        c.blit(
            &sub,
            (body.x - ox) as i32 + ox as i32,
            (body.y - oy) as i32 + oy as i32,
        );
    }

    /// Eingabefeld (Zahl rechtsbündig mit Einheit, sonst Text links).
    fn paint_field(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win, f: &Field, r: Rect) {
        let s = w.scale;
        let editing = self.edit.as_ref().filter(|e| e.field == *f);
        let value = editing.map_or_else(|| self.field_value(f), |e| e.text.text.to_string());
        let unit = self.field_unit(f);
        let st = FieldState {
            text: &value,
            unit,
            hover: self.hover == Some(Target::Field(f.clone())),
            focus: editing.is_some(),
            invalid: editing.is_some_and(|e| e.invalid),
            caret: editing.map(|e| e.text.caret),
            select: editing.map(|e| e.text.selection()).filter(|x| x.0 != x.1),
            disabled: false,
        };
        if f.numeric() || *f == Field::Key(PRICE_DATE) {
            widgets::field(c, fonts, r, &st, s, t);
        } else {
            widgets::text_field(c, fonts, r, &st, s, t);
        }
    }

    fn paint_project(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let Some(id) = self.sel_id() else {
            return;
        };
        let Some(x) = self.work.material(id) else {
            return;
        };
        // Name (fett, größer) und Art
        let nr = self.name_rect(t, w);
        let editing = self.edit.as_ref().filter(|e| e.field == Field::Name);
        let name = editing.map_or(x.name.clone(), |e| e.text.text.to_string());
        let st = FieldState {
            text: "",
            hover: self.hover == Some(Target::Field(Field::Name)),
            focus: editing.is_some(),
            invalid: editing.is_some_and(|e| e.invalid),
            ..FieldState::default()
        };
        widgets::text_field(c, fonts, nr, &st, s, t);
        let npx = t.size.mat_name_font * s;
        let ncap = bold.map_or(npx * 0.7, |f| f.cap_height(npx));
        let nx = widgets::text_field_x(nr, s, t);
        let nbase = (nr.y + (nr.h + ncap) * 0.5).round();
        if let (Some(e), Some(fb)) = (editing, bold) {
            let (a, b) = e.text.selection();
            if a != b {
                let x0 = nx + fb.width(&name[..a], npx);
                let x1 = nx + fb.width(&name[..b], npx);
                c.fill_rect(x0, nr.y + 6.0 * s, x1 - x0, nr.h - 12.0 * s, u.text_select);
            }
            let cx0 = nx + fb.width(&name[..e.text.caret], npx);
            c.fill_rect(
                cx0.round(),
                nr.y + 7.0 * s,
                s.round().max(1.0),
                nr.h - 14.0 * s,
                u.caret,
            );
        }
        let shown = widgets::ellipsize(bold, &name, npx, nr.w - 16.0 * s);
        label(c, bold, &shown, npx, nx, nbase, u.text);
        let cat = self.cat_rect(t, w, fonts);
        let hov = self.hover == Some(Target::Combo(ComboId::Category));
        let cpx = t.size.font_small * s;
        label(
            c,
            regular,
            x.category.name(),
            cpx,
            cat.x + 8.0 * s,
            nbase,
            if hov { u.text } else { u.text_dim },
        );
        if hov {
            underline(c, cat.x + 8.0 * s, nbase, cat.w - 16.0 * s, s, u.text);
        }
        let dr = self.duplicate_rect(t, w);
        let st = ButtonState {
            hover: self.hover == Some(Target::Btn(Btn::Duplicate)),
            pressed: self.pressed == Some(Target::Btn(Btn::Duplicate)),
            ..ButtonState::default()
        };
        widgets::button(c, fonts, dr, "Duplizieren", st, s, t);
        // Drei Vorschauen
        let d = x.display();
        let paper = Rgba::from_rgb8(self.work.attr().display().paper);
        let cap_px = t.size.mat_caption * s;
        let line = s.round().max(1.0);
        for (i, cap_text) in ["Schnitt", "Ansicht", "3D"].into_iter().enumerate() {
            let r = self.preview_rect(t, w, i);
            let inner = Rect::new(
                r.x + 12.0 * s,
                r.y + 12.0 * s,
                r.w - 24.0 * s,
                r.h - 24.0 * s,
            );
            let key = sk_model::MaterialId::clone(&id);
            let mut tiles = self.tiles.borrow_mut();
            tiles.sync(&self.work, t, s);
            let tk = attr_pick::TileKey::Material(key);
            let cube_ready = self
                .work
                .attr()
                .surface(d.surface)
                .and_then(|o| o.pattern.as_ref())
                .is_none_or(sk_model::proctex::pattern_ready);
            match i {
                0 => tiles.preview(c, 10, tk, r, true, |c| {
                    rounded(c, r, 6.0 * s, u.border);
                    rounded(
                        c,
                        Rect::new(r.x + line, r.y + line, r.w - 2.0 * line, r.h - 2.0 * line),
                        6.0 * s - line,
                        paper,
                    );
                    attr_pick::paint_fill_preview(c, inner, &self.work, t, s, d);
                }),
                1 => tiles.preview(c, 11, tk, r, true, |c| {
                    rounded(c, r, 6.0 * s, u.border);
                    rounded(
                        c,
                        Rect::new(r.x + line, r.y + line, r.w - 2.0 * line, r.h - 2.0 * line),
                        6.0 * s - line,
                        paper,
                    );
                    // Bis Paket 6 die Oberflächenfarbe als Fläche mit Rand
                    let col = self
                        .work
                        .attr()
                        .surface(d.surface)
                        .map_or(paper, |o| Rgba::from_rgb8(o.color));
                    c.fill_rect(inner.x, inner.y, inner.w, inner.h, t.env.edge);
                    c.fill_rect(
                        inner.x + line,
                        inner.y + line,
                        inner.w - 2.0 * line,
                        inner.h - 2.0 * line,
                        col,
                    );
                }),
                _ => tiles.preview(c, 12, tk, r, cube_ready, |c| {
                    rounded(c, r, 6.0 * s, u.border);
                    let ir = Rect::new(r.x + line, r.y + line, r.w - 2.0 * line, r.h - 2.0 * line);
                    if let Some(o) = self.work.attr().surface(d.surface) {
                        attr_pick::paint_cube(c, ir, o, t, s);
                    }
                }),
            }
            let tw = regular.map_or(0.0, |f| f.width(cap_text, cap_px));
            label(
                c,
                regular,
                cap_text,
                cap_px,
                r.x + (r.w - tw) * 0.5,
                r.y + r.h + 18.0 * s,
                u.text_dim,
            );
        }
        // Zeilen rechts
        let lx = self.r(t, w, self.rows_x(t), 0.0, 0.0, 0.0).x;
        let lpx = t.size.font_small * s;
        let lcap = regular.map_or(lpx * 0.7, |f| f.cap_height(lpx));
        let row_base = |r: Rect| (r.y + (r.h + lcap) * 0.5).round();
        for (f, text) in [
            (Field::Key("λ"), "Wärmeleitfähigkeit λ"),
            (Field::Key("Rohdichte"), "Rohdichte"),
            (Field::Key(PRICE), "Richtpreis"),
        ] {
            let Some(r) = self.field_rect(t, w, &f) else {
                continue;
            };
            label(c, regular, text, lpx, lx, row_base(r), u.text_dim);
            self.paint_field(c, t, fonts, w, &f, r);
        }
        if let Some(r) = self.field_rect(t, w, &Field::Key(PRICE_DATE)) {
            let sw = regular.map_or(0.0, |f| f.width("Stand", t.size.font_detail * s));
            label(
                c,
                regular,
                "Stand",
                t.size.font_detail * s,
                r.x - 8.0 * s - sw,
                row_base(r),
                u.text_dim,
            );
            self.paint_field(c, t, fonts, w, &Field::Key(PRICE_DATE), r);
        }
        // Hinweis: Einheit passt nicht mehr zur Art (Regel 53)
        if let Some(h) = price_hint(&self.work, id) {
            let r = self.basic_field(t, w, 3, 0.0, 0.0);
            let lx2 = lx;
            let mut p = Path::new();
            let rad = 3.0 * s;
            p.rounded_rect(lx2, r.y + 6.0 * s, 2.0 * rad, 2.0 * rad, rad);
            c.fill(&p, u.accent);
            label(
                c,
                regular,
                &h,
                t.size.font_detail * s,
                lx2 + 12.0 * s,
                r.y + 12.0 * s,
                u.text_dim,
            );
            label(
                c,
                regular,
                &price_hint_detail(x),
                t.size.font_detail * s * 0.92,
                lx2 + 12.0 * s,
                r.y + 27.0 * s,
                u.text_dim,
            );
        }
        // Verwendet in
        let ur = self.basic_field(t, w, self.uses_row(), 0.0, 0.0);
        label(
            c,
            regular,
            "Verwendet in",
            lpx,
            lx,
            row_base(ur),
            u.text_dim,
        );
        let (links, rest) = self.use_links(t, w, fonts);
        let mut end = ur.x;
        for (i, l) in links.iter().enumerate() {
            let r = l.rect;
            match l.ty {
                Some(tid) => {
                    let hov = self.hover == Some(Target::Use(tid));
                    let col = if hov { u.accent_hover } else { u.accent };
                    label(c, regular, &l.name, lpx, r.x, row_base(ur), col);
                    if hov {
                        underline(c, r.x, row_base(ur), r.w, s, col);
                    }
                }
                None => label(c, regular, &l.name, lpx, r.x, row_base(ur), u.text),
            }
            label(
                c,
                regular,
                &l.tail,
                lpx,
                r.x + r.w,
                row_base(ur),
                u.text_dim,
            );
            end = l.end;
            if i + 1 < links.len() {
                label(c, regular, ",", lpx, end, row_base(ur), u.text_dim);
            }
        }
        if !rest.is_empty() {
            label(c, regular, &rest, lpx, end, row_base(ur), u.text_dim);
        }
        // Mehr ▸ / Weniger ▾
        let ml = self.more_link(t, w, fonts);
        let hov = self.hover == Some(Target::More);
        let col = if hov { u.accent_hover } else { u.accent };
        let text = if self.more { "Weniger" } else { "Mehr" };
        let base = ml.y + 16.0 * s;
        label(c, bold, text, lpx, ml.x + 2.0 * s, base, col);
        // Pfeil: zu ▸, offen ▾
        let tw = bold.map_or(0.0, |f| f.width(text, lpx));
        let (ax, ay, d) = (ml.x + 2.0 * s + tw + 8.0 * s, base - 4.0 * s, 3.5 * s);
        let mut p = Path::new();
        if self.more {
            p.move_to(ax - d, ay - d * 0.6)
                .line_to(ax + d, ay - d * 0.6)
                .line_to(ax, ay + d * 0.6);
        } else {
            p.move_to(ax - d * 0.6, ay - d)
                .line_to(ax + d * 0.6, ay)
                .line_to(ax - d * 0.6, ay + d);
        }
        p.close();
        c.fill(&p, col);
        if hov {
            underline(c, ml.x + 2.0 * s, ml.y + 16.0 * s, ml.w - 4.0 * s, s, col);
        }
        let p = self.more_progress(t);
        if p > 0.0 {
            self.paint_more(c, t, fonts, w, p);
        }
    }

    fn paint_more(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win, p: f32) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let body = self.more_body(t, w);
        // Aufklappen: die Fläche wächst nach unten, der Inhalt blendet ein
        let h = (body.h * p).round().max(1.0);
        // Leinwand wiederverwendet statt je Bild neu (Review 3n)
        let mut sub = self.more_img.replace(Canvas::new(0, 0));
        sub.reuse(body.w.max(1.0) as usize, h as usize);
        sub.set_origin(body.x, body.y);
        sub.clear(u.bg);
        let lpx = t.size.font_small * s;
        let lcap = regular.map_or(lpx * 0.7, |f| f.cap_height(lpx));
        let x = self.sel_id().and_then(|id| self.work.material(id));
        for (tg, text, row, field) in self.more_layout(t, w) {
            if row.y + row.h < body.y || row.y > body.y + h {
                continue;
            }
            let base = (row.y + (row.h + lcap) * 0.5).round();
            match &tg {
                None => {
                    let hb = row.y + 15.0 * s;
                    label(
                        &mut sub,
                        bold,
                        &text,
                        t.size.font_detail * s,
                        row.x,
                        hb,
                        u.text_dim,
                    );
                    sub.fill_rect(
                        row.x,
                        (hb + 5.0 * s).round(),
                        row.w,
                        s.round().max(1.0),
                        u.border,
                    );
                }
                Some(Target::AddKey) => {
                    let hov = self.hover == Some(Target::AddKey);
                    let col = if hov { u.accent_hover } else { u.accent };
                    let lx = row.x + 150.0 * s;
                    label(&mut sub, bold, &text, lpx, lx, base, col);
                    if hov {
                        let tw = bold.map_or(0.0, |f| f.width(&text, lpx));
                        underline(&mut sub, lx, base, tw, s, col);
                    }
                }
                Some(Target::Field(f)) => {
                    let fbase = (field.y + (field.h + lcap) * 0.5).round();
                    if *f == Field::NewKey {
                        // Name des neuen Kennworts im Feld der Beschriftung
                        let nr = Rect::new(row.x, field.y, 140.0 * s, field.h);
                        self.paint_field(&mut sub, t, fonts, w, f, nr);
                    } else {
                        let shown = widgets::ellipsize(regular, &text, lpx, 144.0 * s);
                        label(&mut sub, regular, &shown, lpx, row.x, fbase, u.text_dim);
                        self.paint_field(&mut sub, t, fonts, w, f, field);
                    }
                }
                Some(Target::Combo(id)) => {
                    let fbase = (field.y + (field.h + lcap) * 0.5).round();
                    label(&mut sub, regular, &text, lpx, row.x, fbase, u.text_dim);
                    let open = matches!(&self.popup, Some(Popup::List(l)) if l.id == *id);
                    let hov = self.hover.as_ref() == Some(&Target::Combo(*id));
                    let (shown, icon) = match (*id, x) {
                        (ComboId::Pick(pk), Some(x)) => {
                            let mut tiles = self.tiles.borrow_mut();
                            attr_pick::pick_shown(&self.work, &mut tiles, t, s, pk, &x.display())
                        }
                        (ComboId::Trade, Some(x)) => (
                            x.trade
                                .and_then(|tr| self.work.trade(tr))
                                .map_or("–".into(), |tr| tr.name.clone()),
                            None,
                        ),
                        (ComboId::Euro, _) => {
                            let v = self.field_value(&Field::Key(matprop::EUROCLASS));
                            (if v.is_empty() { "–".into() } else { v }, None)
                        }
                        _ => (String::new(), None),
                    };
                    widgets::combo_icon(
                        &mut sub,
                        fonts,
                        field,
                        &shown,
                        icon.as_ref(),
                        hov,
                        open,
                        s,
                        t,
                    );
                }
                _ => {}
            }
        }
        // Bildlaufleiste, wenn „Mehr“ höher ist als der Platz
        let total = self.more_height(t, w);
        if total > body.h + 1.0 {
            let bw = t.size.scrollbar * s;
            let bar = Rect::new(body.x + body.w - bw, body.y, bw, body.h);
            widgets::scrollbar(
                &mut sub,
                bar,
                self.more_scroll / total,
                body.h / total,
                false,
                s,
                t,
            );
        }
        sub.set_origin(0.0, 0.0);
        let (ox, oy) = c.origin();
        let a = if p < 1.0 { p } else { 1.0 };
        // Ganz offen auf ganzen Pixeln: gerade kopieren statt abtasten
        if a >= 1.0 && body.x.fract() == 0.0 && body.y.fract() == 0.0 {
            c.blit(&sub, body.x as i32, body.y as i32);
        } else {
            c.blit_scaled(&sub, body.x - ox + ox, body.y - oy + oy, 1.0, a);
        }
        self.more_img.replace(sub);
    }

    fn paint_company(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let x0 = self.r(t, w, self.rx(t), 0.0, 0.0, 0.0).x;
        let y0 = self.frame(t, w).y + HEAD * s;
        let px = t.size.font_small * s;
        let Some(lib) = self.lib() else {
            label(
                c,
                regular,
                "Kein Firmenkatalog. Über „ändern …“ wählen.",
                px,
                x0,
                y0 + 36.0 * s,
                u.text_dim,
            );
            return;
        };
        let Some(g) = self.csel else {
            return;
        };
        let mine = self
            .mat_id(g)
            .and_then(|id| self.work.material(id).map(|x| (id, x)));
        let theirs = self.lib_mat(g);
        let Some(name) = mine
            .map(|m| m.1.name.clone())
            .or(theirs.map(|x| x.name.clone()))
        else {
            return;
        };
        label(
            c,
            bold,
            &name,
            t.size.mat_name_font * s,
            x0,
            y0 + 36.0 * s,
            u.text,
        );
        let sentence = match mine {
            Some((id, _)) => {
                let n: usize = self
                    .uses_of(id)
                    .iter()
                    .map(|u| match u {
                        Use::Type(_, n) => *n,
                        Use::Element(_) => 1,
                    })
                    .sum();
                match (theirs.is_some(), n) {
                    (false, _) => "Nur im Projekt; „In den Firmenkatalog …“ im Reiter Projekt speichert ihn dort.".to_string(),
                    (true, 0) => "Im Projekt nicht verbaut.".into(),
                    (true, 1) => "Im Projekt in 1 Bauteil verwendet.".into(),
                    (true, n) => format!("Im Projekt in {n} Bauteilen verwendet."),
                }
            }
            None => "Nicht im Projekt; „Ins Projekt übernehmen“ holt ihn mit OK herein.".into(),
        };
        label(c, regular, &sentence, px, x0, y0 + 58.0 * s, u.text_dim);
        // Abgleich-Tabelle
        let cols = [x0, x0 + 260.0 * s, x0 + 420.0 * s];
        let ty = y0 + 100.0 * s;
        for (i, h) in ["Kennwert", "Im Projekt", "Im Firmenkatalog"]
            .into_iter()
            .enumerate()
        {
            label(c, bold, h, t.size.font_detail * s, cols[i], ty, u.text_dim);
        }
        c.fill_rect(x0, ty + 8.0 * s, 600.0 * s, s.round().max(1.0), u.border);
        let val = |x: Option<&Material>, k: &str| -> String {
            let Some(x) = x else {
                return "–".into();
            };
            match k {
                "λ" => x.lambda.map_or("–".into(), |v| format!("{} W/(mK)", de(v))),
                "Rohdichte" => format!("{} kg/m³", de(x.density)),
                PRICE => match x.props.get(PRICE) {
                    Some(PropValue::Number(n)) => {
                        let unit = stored_unit(x)
                            .or(matprop::price_unit(x.category))
                            .unwrap_or("");
                        format!("{} {}", de(*n), matprop::price_unit_label(unit))
                    }
                    _ => "–".into(),
                },
                k => x.props.get(k).map_or("–".into(), prop_text),
            }
        };
        let mine_m = mine.map(|m| m.1);
        let mut keys: Vec<(String, String)> = vec![
            (PRICE.into(), "Richtpreis".into()),
            (PRICE_DATE.into(), "Stand".into()),
            ("λ".into(), "Wärmeleitfähigkeit λ".into()),
            ("Rohdichte".into(), "Rohdichte".into()),
        ];
        // Weitere Kennwerte, die sich unterscheiden
        for k in mine_m
            .into_iter()
            .chain(theirs)
            .flat_map(|x| x.props.keys().cloned())
            .collect::<std::collections::BTreeSet<String>>()
        {
            if k == PRICE || k == PRICE_DATE || k == PRICE_UNIT {
                continue;
            }
            if val(mine_m, &k) != val(theirs, &k) {
                keys.push((k.clone(), k));
            }
        }
        // Name und Gewerk, wenn sie abweichen: Die Liste nennt den Baustoff
        // dann „abweichend“ (Review 3n/9)
        let trade_text = |x: Option<&Material>, from_lib: bool| -> String {
            let id = x.and_then(|x| x.trade);
            let t = id.and_then(|id| {
                if from_lib {
                    lib.trades.iter().find(|t| t.id() == id).cloned()
                } else {
                    self.work.trade(id).cloned()
                }
            });
            t.map_or("–".into(), |t| format!("{} {}", t.code, t.name))
        };
        if let (Some(a), Some(b)) = (mine_m, theirs) {
            if a.name != b.name {
                keys.insert(0, ("\u{1}name".into(), "Name".into()));
            }
            if trade_text(Some(a), false) != trade_text(Some(b), true) {
                keys.push(("\u{1}trade".into(), "Gewerk".into()));
            }
        }
        let display_same = match (mine_m, theirs) {
            (Some(a), Some(_)) => self.lib_display(g).is_some_and(|d| d == a.display()),
            _ => true,
        };
        let mut y = ty + 32.0 * s;
        for (k, text) in keys {
            let (a, b) = match k.as_str() {
                "\u{1}name" => (
                    mine_m.map_or("–".into(), |x| x.name.clone()),
                    theirs.map_or("–".into(), |x| x.name.clone()),
                ),
                "\u{1}trade" => (trade_text(mine_m, false), trade_text(theirs, true)),
                _ => (val(mine_m, &k), val(theirs, &k)),
            };
            let differs = mine_m.is_some() && theirs.is_some() && a != b;
            label(c, regular, &text, px, cols[0], y, u.text_dim);
            label(c, regular, &a, px, cols[1], y, u.text);
            let (f, col) = if differs {
                (bold, u.accent)
            } else {
                (regular, u.text)
            };
            label(c, f, &b, px, cols[2], y, col);
            y += 30.0 * s;
        }
        let (a, b) = if display_same {
            ("gleich", "gleich")
        } else {
            ("eigene", "abweichend")
        };
        label(c, regular, "Darstellung", px, cols[0], y, u.text_dim);
        label(c, regular, a, px, cols[1], y, u.text);
        label(
            c,
            if display_same { regular } else { bold },
            b,
            px,
            cols[2],
            y,
            if display_same { u.text } else { u.accent },
        );
        y += 44.0 * s;
        let note = "Abweichende Werte in Akzent. „Ins Projekt übernehmen“ setzt den Projektbaustoff auf den Stand der Firma; die Mengen bleiben gleich, nur Preis und Kennwerte ändern sich. Ein Rückgängig-Schritt mit OK.";
        for l in widgets::wrap(regular, note, px, 640.0 * s) {
            label(c, regular, &l, px, x0, y, u.text_dim);
            y += 20.0 * s;
        }
    }

    fn paint_popup_list(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win, l: &List) {
        let s = w.scale;
        let u = &t.ui;
        let (r, k) = self.list_rect(t, w, fonts, l);
        let pad = 4.0 * s;
        let bg = Rect::new(r.x - pad, r.y - pad, r.w + 2.0 * pad, r.h + 2.0 * pad);
        widgets::panel_filled(c, bg, s, t, u.menu_bg);
        let rh = (t.size.mat_row * s).round();
        let px = t.size.font_small * s;
        let regular = fonts.regular.as_ref();
        let cap = regular.map_or(px * 0.7, |f| f.cap_height(px));
        for i in 0..k {
            let j = l.first + i;
            let Some(text) = l.items.get(j) else {
                break;
            };
            let row = Rect::new(r.x, r.y + i as f32 * rh, r.w, rh);
            if self.hover == Some(Target::Item(j)) {
                rounded(c, row, 4.0 * s, u.hover);
            } else if j == l.sel {
                rounded(c, row, 4.0 * s, u.pressed);
            }
            let mut x = row.x + 8.0 * s;
            if let Some(Some(icon)) = l.icons.get(j) {
                c.blit(
                    icon,
                    x as i32,
                    (row.y + (row.h - icon.height as f32) * 0.5) as i32,
                );
                x += icon.width as f32 + 8.0 * s;
            }
            let shown = widgets::ellipsize(regular, text, px, row.x + row.w - x - 8.0 * s);
            label(
                c,
                regular,
                &shown,
                px,
                x,
                (row.y + (row.h + cap) * 0.5).round(),
                u.text,
            );
        }
    }

    fn paint_confirm(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win, g: Guid) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let card = self.confirm_card(t, w);
        widgets::panel_filled(c, card, s, t, u.menu_bg);
        let name = self
            .mat_id(g)
            .and_then(|id| self.work.material(id))
            .map_or(String::new(), |x| x.name.clone());
        let q = format!("„{name}“ in den Firmenkatalog speichern?");
        let mut y = card.y + 34.0 * s;
        for l in widgets::wrap(bold, &q, t.size.font * s, card.w - 40.0 * s) {
            label(c, bold, &l, t.size.font * s, card.x + 20.0 * s, y, u.text);
            y += 20.0 * s;
        }
        let d =
            "Der Firmenkatalog wird sofort geschrieben; das lässt sich nicht rückgängig machen.";
        for l in widgets::wrap(regular, d, t.size.font_small * s, card.w - 40.0 * s) {
            label(
                c,
                regular,
                &l,
                t.size.font_small * s,
                card.x + 20.0 * s,
                y + 4.0 * s,
                u.text_dim,
            );
            y += 18.0 * s;
        }
        for (b, r, text) in self.confirm_buttons(t, w) {
            let st = ButtonState {
                hover: self.hover == Some(Target::Btn(b)),
                pressed: self.pressed == Some(Target::Btn(b)),
                active: b == Btn::ConfirmYes,
                disabled: false,
            };
            widgets::button(c, fonts, r, text, st, s, t);
        }
    }
}

#[cfg(test)]
mod synonym_tests {
    use super::*;

    /// KA-0a3: „gasbeton“ und „ytong“ finden den Werksbaustoff Porenbeton,
    /// auch angefangen; andere Wörter nichts.
    #[test]
    fn gasbeton_und_ytong_finden_porenbeton() {
        let m = Model::new();
        let poren = m
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Porenbeton")
            .map(|(_, x)| x.name.to_lowercase())
            .expect("Werksbaustoff Porenbeton");
        for q in ["gasbeton", "gasb", "ytong", "yto"] {
            assert!(synonym(q).is_some_and(|n| poren.contains(n)), "{q}");
        }
        for q in ["ga", "beton", "putz", ""] {
            assert_eq!(synonym(q), None, "{q}");
        }
    }
}

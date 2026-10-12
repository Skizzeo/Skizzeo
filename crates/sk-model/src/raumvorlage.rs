//! Raumkatalog des Raumbuchs (raumbuch/plan.md §4.3 und §7, Schritt RB2):
//! Raumvorlagen mit Nutzungsgruppe nach DIN 277, Anrechnung zur Wohnfläche
//! und den Ausbautypen für Boden, Estrich, Wand, Wandfliesen, Decke und
//! Sockel. Ein Raum verweist über die Guid auf seine Vorlage (`use=`, Raum
//! des Grundrisses); ändert sich die Vorlage „Bad“, ändern sich alle Bäder
//! ohne eigene Abweichung. Eine Vorlage, die ein Raum nutzt, bleibt im
//! Projekt wie ein Typ, der verbaut ist.
//!
//! Wie die Werks-Ausbautypen haben die 13 Werksvorlagen feste Guids und
//! werden erst angelegt, wenn ein Raum oder der Katalog sie braucht
//! ([`Model::ensure_space_type`]): Ein Projekt ohne Raumvorlage schreibt
//! dieselben Bytes wie vorher. Die Vorbelegung ist Praxis, keine Norm.

use super::*;
use crate::txn::Key;

/// Nutzungsgruppe nach DIN 277:2021 (Nutzungsfläche NUF 1 bis 7,
/// Technikfläche, Verkehrsfläche).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpaceGroup {
    Nuf1,
    Nuf2,
    Nuf3,
    Nuf4,
    Nuf5,
    Nuf6,
    Nuf7,
    Tf,
    Vf,
}

/// Kurzname, Name und Dateiwort der Nutzungsgruppen.
const GROUP_NAMES: [(SpaceGroup, &str, &str, &str); 9] = [
    (SpaceGroup::Nuf1, "NUF 1", "Wohnen und Aufenthalt", "nuf1"),
    (SpaceGroup::Nuf2, "NUF 2", "Büroarbeit", "nuf2"),
    (
        SpaceGroup::Nuf3,
        "NUF 3",
        "Produktion, Hand- und Maschinenarbeit, Forschung und Entwicklung",
        "nuf3",
    ),
    (
        SpaceGroup::Nuf4,
        "NUF 4",
        "Lagern, Verteilen und Verkaufen",
        "nuf4",
    ),
    (
        SpaceGroup::Nuf5,
        "NUF 5",
        "Bildung, Unterricht und Kultur",
        "nuf5",
    ),
    (SpaceGroup::Nuf6, "NUF 6", "Heilen und Pflegen", "nuf6"),
    (SpaceGroup::Nuf7, "NUF 7", "Sonstige Nutzungen", "nuf7"),
    (SpaceGroup::Tf, "TF", "Technikfläche", "tf"),
    (SpaceGroup::Vf, "VF", "Verkehrsfläche", "vf"),
];

impl SpaceGroup {
    pub const ALL: [SpaceGroup; 9] = [
        SpaceGroup::Nuf1,
        SpaceGroup::Nuf2,
        SpaceGroup::Nuf3,
        SpaceGroup::Nuf4,
        SpaceGroup::Nuf5,
        SpaceGroup::Nuf6,
        SpaceGroup::Nuf7,
        SpaceGroup::Tf,
        SpaceGroup::Vf,
    ];

    fn names(self) -> &'static (SpaceGroup, &'static str, &'static str, &'static str) {
        &GROUP_NAMES[self as usize]
    }

    /// „NUF 1“, „TF“, „VF“.
    pub fn short(self) -> &'static str {
        self.names().1
    }

    /// „Wohnen und Aufenthalt“.
    pub fn name(self) -> &'static str {
        self.names().2
    }

    /// Wort in der Datei (`group=`).
    pub fn word(self) -> &'static str {
        self.names().3
    }

    pub fn from_word(w: &str) -> Option<SpaceGroup> {
        Self::ALL.into_iter().find(|g| g.word() == w)
    }
}

/// Anrechnung zur Wohnfläche nach WoFlV in Prozent: voll, halb, ein
/// Viertel (Balkon, Terrasse) oder nicht (Zubehörraum, § 2 Abs. 3).
pub const WOFLV: [u8; 4] = [100, 50, 25, 0];

/// Wandfliesen als Zone ab Oberkante Fertigfußboden bis zur Höhe (mm); eine
/// Höhe ab Raumhöhe heißt raumhoch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallTiles {
    pub set: Guid,
    pub height: f64,
}

/// Höhe der Wandfliesen (mm).
pub const TILES_HEIGHT: (f64, f64) = (100.0, 4000.0);

/// Höhe einer neuen Fliesenzone (mm): 1,20 m wie im Bad.
pub const TILES_DEFAULT: f64 = 1200.0;

/// Eine Raumvorlage. Die Ausbautypen stehen als Guid des Typs; ihre Art
/// muss zum Feld passen ([`Model::space_type_problems`]).
#[derive(Clone, Debug, PartialEq)]
pub struct SpaceType {
    pub guid: Guid,
    /// „Bad“; Vorschlag im Raumstempel, im Projekt eindeutig.
    pub name: String,
    pub group: SpaceGroup,
    /// Anrechnung zur Wohnfläche in Prozent ([`WOFLV`]).
    pub woflv: u8,
    /// Bodenbelag; leer: ohne Belag.
    pub floor: Option<Guid>,
    /// Estrichaufbau; leer: Vorgabe des Geschosses (auf der Sohlplatte
    /// „Estrich 14 auf Sohlplatte“, sonst „auf Decke“, Regel 5.1).
    pub screed: Option<Guid>,
    /// Wandbekleidung (Innenputz mit Beschichtung).
    pub wall: Option<Guid>,
    /// Wandfliesen vor der Wandbekleidung, ab Fußboden.
    pub tiles: Option<WallTiles>,
    /// Deckenbekleidung.
    pub ceiling: Option<Guid>,
    /// Sockelleiste; leer: keine (etwa bei Wandfliesen ab Boden).
    pub skirting: Option<Guid>,
}

impl SpaceType {
    /// Die Felder mit Ausbautyp: Name, Typart, Guid.
    pub fn slots(&self) -> [(&'static str, TypeCategory, Option<Guid>); 6] {
        [
            ("Bodenbelag", TypeCategory::FloorFinish, self.floor),
            ("Estrich", TypeCategory::Screed, self.screed),
            ("Wand", TypeCategory::WallFinish, self.wall),
            (
                "Wandfliesen",
                TypeCategory::WallTiles,
                self.tiles.map(|t| t.set),
            ),
            ("Decke", TypeCategory::CeilingFinish, self.ceiling),
            ("Sockel", TypeCategory::Skirting, self.skirting),
        ]
    }

    /// Verweist die Vorlage auf den Typ?
    pub fn uses(&self, set: Guid) -> bool {
        self.slots().iter().any(|s| s.2 == Some(set))
    }
}

/// Eine Werksvorlage (Vorbelegung nach plan.md §7).
pub struct SpaceTemplate {
    pub guid: Guid,
    pub name: &'static str,
    pub group: SpaceGroup,
    pub woflv: u8,
    pub floor: Guid,
    pub wall: Guid,
    pub tiles: Option<(Guid, f64)>,
    pub ceiling: Guid,
    pub skirting: Option<Guid>,
}

impl SpaceTemplate {
    /// Die Vorlage, wie sie ins Projekt kommt; die Ausbautypen sind die
    /// Werkstypen mit ihrer festen Guid.
    pub fn space_type(&self) -> SpaceType {
        SpaceType {
            guid: self.guid,
            name: self.name.into(),
            group: self.group,
            woflv: self.woflv,
            floor: Some(self.floor),
            screed: None,
            wall: Some(self.wall),
            tiles: self.tiles.map(|(set, height)| WallTiles { set, height }),
            ceiling: Some(self.ceiling),
            skirting: self.skirting,
        }
    }
}

pub const HALL_SPACE_GUID: Guid = Guid(0x6b4c41bb53012c4ea74e4f41656bac1e);
pub const LIVING_SPACE_GUID: Guid = Guid(0xd5034a7735a2f43ac3d4be414ddba684);
pub const DINING_SPACE_GUID: Guid = Guid(0x16bc60407e6653a6453ef389c7096cea);
pub const KITCHEN_SPACE_GUID: Guid = Guid(0xb0ce19da35025ac9435b277cb2879b16);
pub const BEDROOM_SPACE_GUID: Guid = Guid(0xc1f6abcc730d096b31d17609d0ad5bfb);
pub const CHILD_SPACE_GUID: Guid = Guid(0x587ec235b5f9597a2d41d70e5f8cad14);
pub const GUEST_SPACE_GUID: Guid = Guid(0xedecedeaa41ff9e14a47fc1cb3ef8890);
pub const STUDY_SPACE_GUID: Guid = Guid(0xe549b94608b7bcbe087b298815d7c0bd);
pub const BATH_SPACE_GUID: Guid = Guid(0xcc8a64fb0158cc960e2669c2574ad520);
pub const WC_SPACE_GUID: Guid = Guid(0xd09b5f5d7cc2856674059d0a61407e1d);
pub const STORAGE_SPACE_GUID: Guid = Guid(0xe23aa91c50667d8868fdf03c66bccae2);
pub const UTILITY_SPACE_GUID: Guid = Guid(0xdc174da8762ea36b1231194293bee3b9);
pub const TECHNICAL_SPACE_GUID: Guid = Guid(0x879bc17925706812f6558651bfcd7067);

/// Vorlage eines neu erkannten Raums, bis er benannt ist (plan.md §7:
/// Werk wie Wohnzimmer).
pub const DEFAULT_SPACE_GUID: Guid = LIVING_SPACE_GUID;

/// Wohnraum: Parkett, Gipsputz mit Raufaser, Holzsockel.
const fn dwelling(guid: Guid, name: &'static str) -> SpaceTemplate {
    SpaceTemplate {
        guid,
        name,
        group: SpaceGroup::Nuf1,
        woflv: 100,
        floor: PARQUET_TYPE_GUID,
        wall: WOODCHIP_WALL_TYPE_GUID,
        tiles: None,
        ceiling: CEILING_TYPE_GUID,
        skirting: Some(OAK_SKIRTING_TYPE_GUID),
    }
}

/// Nebenraum: Feinsteinzeug mit Fliesensockel.
const fn side(
    guid: Guid,
    name: &'static str,
    group: SpaceGroup,
    woflv: u8,
    wall: Guid,
) -> SpaceTemplate {
    SpaceTemplate {
        guid,
        name,
        group,
        woflv,
        floor: PORCELAIN_TYPE_GUID,
        wall,
        tiles: None,
        ceiling: CEILING_TYPE_GUID,
        skirting: Some(TILE_SKIRTING_TYPE_GUID),
    }
}

/// Sanitärraum: Fliese mit Verbundabdichtung, Wandfliesen 1,20 m auf
/// Kalkzementputz, kein Sockel.
const fn wet(guid: Guid, name: &'static str) -> SpaceTemplate {
    SpaceTemplate {
        guid,
        name,
        group: SpaceGroup::Nuf7,
        woflv: 100,
        floor: WET_FLOOR_TYPE_GUID,
        wall: LIME_WALL_TYPE_GUID,
        tiles: Some((WALL_TILES_TYPE_GUID, 1200.0)),
        ceiling: CEILING_TYPE_GUID,
        skirting: None,
    }
}

/// Die Werksvorlagen in der Reihenfolge der Auswahlliste.
pub const SPACE_TEMPLATES: [SpaceTemplate; 13] = [
    side(
        HALL_SPACE_GUID,
        "Diele, Flur",
        SpaceGroup::Vf,
        100,
        WOODCHIP_WALL_TYPE_GUID,
    ),
    dwelling(LIVING_SPACE_GUID, "Wohnzimmer"),
    dwelling(DINING_SPACE_GUID, "Esszimmer"),
    // Fliesenspiegel kommt mit der Küchenzeile
    side(
        KITCHEN_SPACE_GUID,
        "Küche",
        SpaceGroup::Nuf1,
        100,
        GYPSUM_WALL_TYPE_GUID,
    ),
    dwelling(BEDROOM_SPACE_GUID, "Schlafzimmer"),
    dwelling(CHILD_SPACE_GUID, "Kinderzimmer"),
    dwelling(GUEST_SPACE_GUID, "Gästezimmer"),
    dwelling(STUDY_SPACE_GUID, "Arbeitszimmer"),
    wet(BATH_SPACE_GUID, "Bad"),
    wet(WC_SPACE_GUID, "Gäste-WC"),
    side(
        STORAGE_SPACE_GUID,
        "Abstellraum",
        SpaceGroup::Nuf7,
        100,
        GYPSUM_WALL_TYPE_GUID,
    ),
    side(
        UTILITY_SPACE_GUID,
        "Hauswirtschaftsraum",
        SpaceGroup::Nuf7,
        100,
        LIME_WALL_TYPE_GUID,
    ),
    side(
        TECHNICAL_SPACE_GUID,
        "Technik, Hausanschluss",
        SpaceGroup::Tf,
        0,
        LIME_WALL_TYPE_GUID,
    ),
];

/// Rang einer Vorlage in der Auswahlliste: Werksvorlagen in ihrer
/// Reihenfolge, eigene danach.
pub fn space_template_rank(g: Guid) -> usize {
    SPACE_TEMPLATES
        .iter()
        .position(|t| t.guid == g)
        .unwrap_or(SPACE_TEMPLATES.len())
}

impl Model {
    /// Raumvorlagen des Projekts.
    pub fn space_types(&self) -> &[SpaceType] {
        &self.space_types
    }

    /// Raumvorlage des Projekts mit der Guid `g`.
    pub fn space_type(&self, g: Guid) -> Option<&SpaceType> {
        self.space_types.iter().find(|s| s.guid == g)
    }

    /// Auswahl für den Raumstempel und den Reiter „Räume“: die Vorlagen des
    /// Projekts, dazu die Werksvorlagen, die es noch nicht führt (ohne
    /// Namensgleiche). Werksvorlagen in ihrer Reihenfolge, eigene danach
    /// nach Name.
    pub fn space_type_choices(&self) -> Vec<(Guid, String)> {
        let mut v: Vec<(Guid, String)> = self
            .space_types
            .iter()
            .map(|s| (s.guid, s.name.clone()))
            .collect();
        for t in &SPACE_TEMPLATES {
            if !v.iter().any(|(g, n)| *g == t.guid || n == t.name) {
                v.push((t.guid, t.name.into()));
            }
        }
        v.sort_by(|a, b| (space_template_rank(a.0), &a.1).cmp(&(space_template_rank(b.0), &b.1)));
        v
    }

    /// Raumvorlage mit der Guid `g`; eine Werksvorlage, die das Projekt noch
    /// nicht führt, wird samt ihren Ausbautypen angelegt. Ein belegter Name
    /// weicht auf „Name 2“ aus. `None` bei fremder Guid.
    pub fn ensure_space_type(&mut self, g: Guid) -> Option<Guid> {
        if self.space_type(g).is_some() {
            return Some(g);
        }
        let t = SPACE_TEMPLATES.iter().find(|t| t.guid == g)?;
        let mut s = t.space_type();
        for set in s.slots().into_iter().filter_map(|x| x.2) {
            self.ensure_finish_type(set)?;
        }
        s.name = self.free_space_name(t.name);
        self.put_space_type(s).then_some(g)
    }

    /// `name`, wenn keine Vorlage so heißt, sonst „name 2“, „name 3“ …
    pub fn free_space_name(&self, name: &str) -> String {
        let taken = |n: &str| self.space_types.iter().any(|s| s.name == n);
        if !taken(name) {
            return name.into();
        }
        (2..)
            .map(|i| format!("{name} {i}"))
            .find(|n| !taken(n))
            .unwrap_or_default()
    }

    /// Legt die Vorlage an oder ersetzt die mit derselben Guid. `false`,
    /// wenn sie gegen eine Regel verstößt ([`Model::space_type_problems`])
    /// oder ihr Name schon vergeben ist.
    pub fn put_space_type(&mut self, s: SpaceType) -> bool {
        let at = self.space_types.iter().position(|x| x.guid == s.guid);
        if self.space_type_problems(&s).next().is_some()
            || self
                .space_types
                .iter()
                .any(|x| x.guid != s.guid && x.name == s.name)
        {
            return false;
        }
        if at.is_some_and(|i| self.space_types[i] == s) {
            return true;
        }
        self.note_space_types();
        match at {
            Some(i) => self.space_types[i] = s,
            None => self.space_types.push(s),
        }
        self.touch();
        true
    }

    /// Ersetzt alle Vorlagen auf einmal (OK im Bauteilkatalog): so dürfen
    /// zwei Vorlagen ihre Namen tauschen. `false`, wenn eine gegen eine Regel
    /// verstößt, Name oder Guid doppelt sind oder eine Vorlage fehlt, die ein
    /// Raum nutzt; dann bleibt alles.
    pub fn replace_space_types(&mut self, v: Vec<SpaceType>) -> bool {
        let bad = v.iter().enumerate().any(|(i, s)| {
            self.space_type_problems(s).next().is_some()
                || v[i + 1..]
                    .iter()
                    .any(|x| x.guid == s.guid || x.name == s.name)
        });
        let genutzt_fehlt = self
            .space_types
            .iter()
            .any(|s| !v.iter().any(|x| x.guid == s.guid) && self.space_type_users(s.guid) > 0);
        if bad || genutzt_fehlt {
            return false;
        }
        if v == self.space_types {
            return true;
        }
        self.note_space_types();
        self.space_types = v;
        self.touch();
        true
    }

    /// Entfernt die Vorlage; `false`, wenn das Projekt sie nicht führt oder
    /// ein Raum sie nutzt ([`Model::space_type_users`]).
    pub fn remove_space_type(&mut self, g: Guid) -> bool {
        let Some(i) = self.space_types.iter().position(|s| s.guid == g) else {
            return false;
        };
        if self.space_type_users(g) > 0 {
            return false;
        }
        self.note_space_types();
        self.space_types.remove(i);
        self.touch();
        true
    }

    /// Zahl der Räume, die die Vorlage `g` als Nutzung gewählt haben (auch
    /// ruhende). Ein Raum ohne Wahl nutzt keine.
    pub fn space_type_users(&self, g: Guid) -> usize {
        self.elements
            .iter()
            .filter(|(_, e)| matches!(&e.kind, ElementKind::Space(s) if s.nutzung == Some(g)))
            .count()
    }

    /// Verweist eine Raumvorlage auf den Typ? Dann lässt er sich weder
    /// löschen noch in eine andere Art umstellen.
    pub fn type_in_space_type(&self, id: LayerSetId) -> bool {
        self.layer_set(id)
            .is_some_and(|t| self.space_types.iter().any(|s| s.uses(t.guid)))
    }

    /// Regeln der Raumvorlage: Name, Anrechnung, Fliesenhöhe, und jeder
    /// Ausbautyp lebt im Projekt mit der Art seines Felds.
    pub fn space_type_problems<'a>(
        &'a self,
        s: &'a SpaceType,
    ) -> impl Iterator<Item = String> + 'a {
        self.space_type_rules(s, |g| {
            let t = self.layer_set(self.type_by_guid(g)?)?;
            Some((t.code.clone(), t.category))
        })
        .into_iter()
    }

    /// Erster Befund der Vorlage `s`, bevor etwas ins Projekt kommt: wie
    /// [`Model::space_type_problems`], dazu ein Name, den eine andere
    /// Vorlage trägt. Ein Ausbautyp, den das Projekt noch nicht führt, zählt
    /// mit der Art, die er mitbrächte: aus dem Werk ([`FINISH_TYPES`]), wenn
    /// er sich anlegen lässt, sonst aus dem Firmenkatalog `lib`. So prüft der
    /// Bauteilkatalog eine Änderung, ohne das Modell zu ändern oder zu
    /// klonen (Review #85, W22).
    pub fn space_type_problem_before(
        &self,
        s: &SpaceType,
        lib: Option<&crate::catalog::Library>,
    ) -> Option<String> {
        if self
            .space_types
            .iter()
            .any(|x| x.guid != s.guid && x.name == s.name)
        {
            return Some(format!("Eine Vorlage „{}“ gibt es schon", s.name));
        }
        self.space_type_rules(s, |g| {
            if let Some(t) = self.type_by_guid(g).and_then(|id| self.layer_set(id)) {
                return Some((t.code.clone(), t.category));
            }
            if let Some(t) = FINISH_TYPES
                .iter()
                .find(|t| t.guid == g && self.finish_type_creatable(t))
            {
                return Some((t.code.into(), t.category));
            }
            let lib = lib?;
            let t = lib.types.get(lib.type_by_guid(g)?)?;
            Some((t.code.clone(), t.category))
        })
        .into_iter()
        .next()
    }

    /// Regeln der Vorlage; `typ` nennt Kurzzeichen und Art des Ausbautyps
    /// mit der Guid, `None`: fehlt.
    fn space_type_rules(
        &self,
        s: &SpaceType,
        typ: impl Fn(Guid) -> Option<(String, TypeCategory)>,
    ) -> Vec<String> {
        let who = if s.name.trim().is_empty() {
            "ohne Namen".to_string()
        } else {
            format!("„{}“", s.name)
        };
        let mut out = Vec::new();
        if s.name.trim().is_empty() {
            out.push(format!("Raumvorlage {who}: Name fehlt"));
        }
        if !WOFLV.contains(&s.woflv) {
            out.push(format!(
                "Raumvorlage {who}: Wohnfläche zu 100, 50, 25 oder 0 %"
            ));
        }
        if let Some(t) = s.tiles {
            let (lo, hi) = TILES_HEIGHT;
            if !(lo..=hi).contains(&t.height) {
                out.push(format!(
                    "Raumvorlage {who}: Fliesenhöhe {} bis {} m",
                    m_de(lo),
                    m_de(hi)
                ));
            }
        }
        for (field, cat, g) in s.slots() {
            let Some(g) = g else { continue };
            match typ(g) {
                Some((_, c)) if c == cat => {}
                Some((code, _)) => out.push(format!(
                    "Raumvorlage {who}: {field} {code} ist kein Typ „{}“",
                    cat.name()
                )),
                None => out.push(format!("Raumvorlage {who}: {field} fehlt im Projekt")),
            }
        }
        out
    }

    /// Alle Befunde der Raumvorlagen, mit doppelten Namen und Guids.
    pub(super) fn check_space_types(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .space_types
            .iter()
            .flat_map(|s| self.space_type_problems(s))
            .collect();
        for (i, s) in self.space_types.iter().enumerate() {
            let later = &self.space_types[i + 1..];
            if later.iter().any(|x| x.name == s.name) {
                out.push(format!("Raumvorlage „{}“ doppelt", s.name));
            }
            if later.iter().any(|x| x.guid == s.guid) {
                out.push(format!("Raumvorlage {}: Guid doppelt", s.guid));
            }
        }
        out
    }

    /// Raumvorlagen aus der Datei, ohne Rückgängig.
    /// Übernimmt, was das Raumbuch aus der `.szo` gelesen hat.
    pub(crate) fn load_ausbau(&mut self, d: crate::szo::ausbau::Daten) {
        self.space_types = d.space_types;
    }

    /// Merkt die Vorlagen vor ihrer ersten Änderung im offenen Schritt (als
    /// Ganzes wie die Gewerke).
    fn note_space_types(&mut self) {
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::SpaceTypes) {
                    t.changes.push(Change::SpaceTypes {
                        old: self.space_types.clone(),
                        new: Vec::new(),
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
    }
}

/// „1,20“ (m) aus mm.
fn m_de(mm: f64) -> String {
    format!("{:.2}", mm / 1000.0).replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::txn::Direction;
    use crate::{export_space_type, import_space_type, read_szk, szo, write_szk, GuidGen, Library};

    /// Projekt mit allen Werksvorlagen, in einem Schritt angelegt.
    fn mit_vorlagen() -> (Model, Txn) {
        let mut m = Model::with_seed(73);
        m.require_steps();
        m.begin("Raumkatalog");
        for t in &SPACE_TEMPLATES {
            assert_eq!(m.ensure_space_type(t.guid), Some(t.guid), "{}", t.name);
        }
        let tx = m.commit().expect("Schritt");
        (m, tx)
    }

    /// Jede Werksvorlage entsteht samt Ausbautypen nach der Vorbelegung
    /// (plan.md §7), das Modell bleibt ohne Befund; zweimal anlegen ändert
    /// nichts.
    #[test]
    fn werksvorlagen_anlegen() {
        let (mut m, _) = mit_vorlagen();
        assert!(m.check().is_empty(), "{:?}", m.check());
        assert_eq!(m.space_types().len(), 13);
        let rev = m.revision();
        for t in &SPACE_TEMPLATES {
            m.ensure_space_type(t.guid);
        }
        assert_eq!(m.revision(), rev);
        let bad = m.space_type(BATH_SPACE_GUID).unwrap();
        assert_eq!(bad.name, "Bad");
        assert_eq!(bad.group, SpaceGroup::Nuf7);
        assert_eq!(bad.floor, Some(WET_FLOOR_TYPE_GUID));
        assert_eq!(bad.wall, Some(LIME_WALL_TYPE_GUID));
        assert_eq!(
            bad.tiles,
            Some(WallTiles {
                set: WALL_TILES_TYPE_GUID,
                height: 1200.0
            })
        );
        assert_eq!(bad.skirting, None);
        assert_eq!(bad.screed, None, "Estrich nach Geschoss");
        let wohnen = m.space_type(DEFAULT_SPACE_GUID).unwrap();
        assert_eq!(wohnen.name, "Wohnzimmer");
        assert_eq!(wohnen.floor, Some(PARQUET_TYPE_GUID));
        assert_eq!(wohnen.skirting, Some(OAK_SKIRTING_TYPE_GUID));
        let technik = m.space_type(TECHNICAL_SPACE_GUID).unwrap();
        assert_eq!((technik.group, technik.woflv), (SpaceGroup::Tf, 0));
        let flur = m.space_type(HALL_SPACE_GUID).unwrap();
        assert_eq!(flur.group, SpaceGroup::Vf);
        // Auswahl im Raumstempel: Werksreihenfolge, Diele zuerst
        let names: Vec<String> = m.space_type_choices().into_iter().map(|c| c.1).collect();
        let werk: Vec<&str> = SPACE_TEMPLATES.iter().map(|t| t.name).collect();
        assert_eq!(names, werk);
        assert!(m.ensure_space_type(Guid(0x5151)).is_none());
    }

    /// Die Liste für den Raumstempel kennt die Werksvorlagen auch ohne
    /// angelegte; eine eigene Vorlage gleichen Namens verdrängt die des
    /// Werks, die beim Anlegen „Bad 2“ heißt.
    #[test]
    fn auswahl_und_namen() {
        let mut m = Model::with_seed(73);
        assert_eq!(m.space_type_choices().len(), 13);
        m.ensure_space_type(WC_SPACE_GUID).unwrap();
        let mut eigen = m.space_type(WC_SPACE_GUID).unwrap().clone();
        eigen.guid = Guid(0xbad);
        eigen.name = "Bad".into();
        assert!(m.put_space_type(eigen.clone()));
        let c = m.space_type_choices();
        assert_eq!(c.len(), 13);
        assert!(c.contains(&(Guid(0xbad), "Bad".into())));
        assert!(!c.iter().any(|x| x.0 == BATH_SPACE_GUID));
        // zweiter Name „Bad“ wird nicht angenommen
        eigen.guid = Guid(0xbad2);
        assert!(!m.put_space_type(eigen));
        m.ensure_space_type(BATH_SPACE_GUID).unwrap();
        assert_eq!(m.space_type(BATH_SPACE_GUID).unwrap().name, "Bad 2");
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Rundlauf über die `.szo`: gleiche Bytes, gleiche Vorlagen; ohne
    /// Vorlage keine Zeile.
    #[test]
    fn rundlauf() {
        let leer = szo::write(&Model::with_seed(73));
        assert!(!leer.contains("[spacetype]"));
        let (m, _) = mit_vorlagen();
        let text = szo::write(&m);
        let bad = text
            .lines()
            .find(|l| l.starts_with("[spacetype]") && l.contains("name=\"Bad\""))
            .unwrap();
        assert!(bad.contains(" group=nuf7 woflv=100 floor="), "{bad}");
        assert!(bad.contains(" tiles_h=1200 ceiling="), "{bad}");
        assert!(
            !bad.contains("screed=") && !bad.contains("skirting="),
            "{bad}"
        );
        let l = szo::read(&text, GuidGen::with_seed(5)).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(szo::write(&l.model), text);
        let mut a = m.space_types().to_vec();
        let mut b = l.model.space_types().to_vec();
        a.sort_by_key(|s| s.guid);
        b.sort_by_key(|s| s.guid);
        assert_eq!(a, b);
        assert!(l.model.check().is_empty(), "{:?}", l.model.check());
    }

    /// Rückgängig nimmt Vorlagen und Ausbautypen zurück, Wiederholen bringt
    /// sie wieder.
    #[test]
    fn rueckgaengig() {
        let vorher = szo::write(&Model::with_seed(73));
        let (mut m, tx) = mit_vorlagen();
        let nachher = szo::write(&m);
        m.apply(&tx, Direction::Undo);
        assert_eq!(szo::write(&m), vorher);
        assert!(m.space_types().is_empty());
        m.apply(&tx, Direction::Redo);
        assert_eq!(szo::write(&m), nachher);
        // Ändern und Entfernen je ein Schritt
        m.begin("Vorlage");
        let mut k = m.space_type(KITCHEN_SPACE_GUID).unwrap().clone();
        k.woflv = 50;
        assert!(m.put_space_type(k));
        assert!(m.remove_space_type(STORAGE_SPACE_GUID));
        let tx = m.commit().unwrap();
        m.apply(&tx, Direction::Undo);
        assert_eq!(szo::write(&m), nachher);
    }

    /// Regeln: Anrechnung, Fliesenhöhe, Art und Dasein der Ausbautypen,
    /// Name. Ein Typ, auf den eine Vorlage verweist, bleibt stehen und
    /// behält seine Art.
    #[test]
    fn regeln() {
        let (mut m, _) = mit_vorlagen();
        m.begin("Regeln");
        let gut = m.space_type(BATH_SPACE_GUID).unwrap().clone();
        let befund = |m: &Model, s: &SpaceType| m.space_type_problems(s).collect::<Vec<_>>();
        let mut s = gut.clone();
        s.woflv = 75;
        assert_eq!(
            befund(&m, &s),
            ["Raumvorlage „Bad“: Wohnfläche zu 100, 50, 25 oder 0 %"]
        );
        assert!(!m.put_space_type(s));
        let mut s = gut.clone();
        s.tiles = Some(WallTiles {
            set: WALL_TILES_TYPE_GUID,
            height: 50.0,
        });
        assert_eq!(
            befund(&m, &s),
            ["Raumvorlage „Bad“: Fliesenhöhe 0,10 bis 4,00 m"]
        );
        let mut s = gut.clone();
        s.floor = Some(LIME_WALL_TYPE_GUID);
        assert_eq!(
            befund(&m, &s),
            ["Raumvorlage „Bad“: Bodenbelag WB-KZ ist kein Typ „Bodenbelag“"]
        );
        let mut s = gut.clone();
        s.ceiling = Some(Guid(0x77));
        assert_eq!(
            befund(&m, &s),
            ["Raumvorlage „Bad“: Decke fehlt im Projekt"]
        );
        let mut s = gut.clone();
        s.name = " ".into();
        assert_eq!(befund(&m, &s), ["Raumvorlage ohne Namen: Name fehlt"]);
        // verwendeter Ausbautyp: nicht löschen, Art bleibt
        let id = m.type_by_guid(WALL_TILES_TYPE_GUID).unwrap();
        assert!(m.type_in_space_type(id));
        assert_eq!(m.remove_type(id), Err(0));
        let mut t = m.layer_set(id).unwrap().clone();
        t.category = TypeCategory::FloorFinish;
        assert!(!m.set_layer_set(id, t));
        // ohne Verweis geht beides
        let mut bad = gut.clone();
        bad.tiles = None;
        assert!(m.put_space_type(bad));
        let wc = m.space_type(WC_SPACE_GUID).unwrap().clone();
        assert!(m.put_space_type(SpaceType { tiles: None, ..wc }));
        assert!(!m.type_in_space_type(id));
        assert_eq!(m.remove_type(id), Ok(()));
        assert!(m.commit().is_some());
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Zwei Vorlagen tauschen ihre Namen in einem Zug; doppelte Namen
    /// nimmt der Tausch nicht an.
    #[test]
    fn namen_tauschen() {
        let (mut m, _) = mit_vorlagen();
        let mut v = m.space_types().to_vec();
        let a = v.iter().position(|s| s.guid == BATH_SPACE_GUID).unwrap();
        let b = v.iter().position(|s| s.guid == WC_SPACE_GUID).unwrap();
        let (na, nb) = (v[a].name.clone(), v[b].name.clone());
        v[a].name = nb.clone();
        v[b].name = na.clone();
        let mut doppelt = v.clone();
        doppelt[a].name = na.clone();
        m.begin("Tausch");
        assert!(!m.replace_space_types(doppelt));
        assert!(m.replace_space_types(v));
        assert!(m.commit().is_some());
        assert_eq!(m.space_type(BATH_SPACE_GUID).unwrap().name, nb);
        assert_eq!(m.space_type(WC_SPACE_GUID).unwrap().name, na);
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Eine Vorlage, die ein Raum nutzt, lässt sich weder löschen noch beim
    /// Übernehmen aus dem Katalog weglassen; ohne Nutzung wieder schon.
    #[test]
    fn genutzte_vorlage_bleibt() {
        let (mut m, _) = mit_vorlagen();
        let st = m.defaults().storey;
        m.begin("Raum");
        let mut s = Space::neu(vec3(1000.0, 1000.0, 0.0), "Bad".into());
        s.nutzung = Some(BATH_SPACE_GUID);
        let r = m.add_space(st, s);
        assert!(m.commit().is_some());
        assert_eq!(m.space_type_users(BATH_SPACE_GUID), 1);
        assert_eq!(m.space_type_users(WC_SPACE_GUID), 0);
        let ohne: Vec<SpaceType> = m
            .space_types()
            .iter()
            .filter(|t| t.guid != BATH_SPACE_GUID)
            .cloned()
            .collect();
        m.begin("Löschen");
        assert!(!m.remove_space_type(BATH_SPACE_GUID));
        assert!(!m.replace_space_types(ohne.clone()));
        assert!(m.remove_space_type(WC_SPACE_GUID), "ungenutzt");
        assert!(m.set_space(r, |s| s.nutzung = None));
        assert!(m.replace_space_types(ohne));
        assert!(m.commit().is_some());
        assert!(m.space_type(BATH_SPACE_GUID).is_none());
    }

    /// Unlesbares in `[spacetype]` gibt Hinweise, die Vorlage kommt mit
    /// Ersatzwerten; doppelte Guid bricht ab.
    #[test]
    fn datei_mit_fehlern() {
        let (m, _) = mit_vorlagen();
        let text = szo::write(&m);
        let alt = text
            .lines()
            .find(|l| l.contains("name=\"Küche\""))
            .unwrap()
            .to_string();
        let floor = Guid(0x1).to_ifc();
        let neu = alt
            .replace("group=nuf1", "group=nuf9")
            .replace("woflv=100", "woflv=70")
            .replacen("floor=", &format!("floor={floor} x="), 1);
        let l = szo::read(&text.replace(&alt, &neu), GuidGen::with_seed(5)).unwrap();
        let k = l.model.space_type(KITCHEN_SPACE_GUID).unwrap();
        assert_eq!((k.group, k.woflv, k.floor), (SpaceGroup::Nuf7, 100, None));
        assert_eq!(l.hints.len(), 4, "{:?}", l.hints);
        assert!(l
            .hints
            .iter()
            .any(|h| h.contains("Bodenbelag") && h.contains("entfällt")));
        let doppelt = format!("{text}{alt}\n");
        assert!(szo::read(&doppelt, GuidGen::with_seed(5)).is_err());
    }

    /// Fehlt `tiles_h`, ist ein Verweis unlesbar oder fehlt die Gruppe,
    /// lädt die Datei trotzdem: Ersatz mit Hinweis (Review #85, Hinweis 4).
    #[test]
    fn datei_laedt_trotz_fliesenhoehe() {
        let (m, _) = mit_vorlagen();
        let text = szo::write(&m);
        let alt = text
            .lines()
            .find(|l| l.contains("name=\"Bad\""))
            .unwrap()
            .to_string();
        let neu = alt
            .replace(" tiles_h=1200", "")
            .replace(" group=nuf7", "")
            .replacen("ceiling=", "ceiling=kaputt x=", 1);
        let l = szo::read(&text.replace(&alt, &neu), GuidGen::with_seed(5)).unwrap();
        let b = l.model.space_type(BATH_SPACE_GUID).unwrap();
        assert_eq!(
            b.tiles,
            Some(WallTiles {
                set: WALL_TILES_TYPE_GUID,
                height: TILES_DEFAULT
            })
        );
        assert_eq!((b.group, b.ceiling), (SpaceGroup::Nuf7, None));
        assert!(l.hints.iter().any(|h| h.contains("Fliesenhöhe fehlt")));
        assert!(l.hints.iter().any(|h| h.contains("„kaputt“ unlesbar")));
        let ohne_wert = text.replace(&alt, &alt.replace("tiles_h=1200", "tiles_h=hoch"));
        let l = szo::read(&ohne_wert, GuidGen::with_seed(5)).unwrap();
        let b = l.model.space_type(BATH_SPACE_GUID).unwrap();
        assert_eq!(b.tiles.map(|t| t.height), Some(TILES_DEFAULT));
    }

    /// Prüfen vor dem Anlegen: Ausbautypen des Werks und der Firma zählen
    /// mit ihrer Art, ohne dass etwas ins Projekt kommt (Review #85,
    /// Befund 1).
    #[test]
    fn pruefen_vor_dem_anlegen() {
        use crate::catalog::Library;
        let mut m = Model::with_seed(13);
        let (typen, stoffe) = (m.layer_sets().len(), m.materials().len());
        let bad = SPACE_TEMPLATES[8].space_type();
        assert_eq!(bad.guid, BATH_SPACE_GUID);
        assert_eq!(m.space_type_problem_before(&bad, None), None);
        let falsch = SpaceType {
            floor: Some(WALL_TILES_TYPE_GUID),
            ..bad.clone()
        };
        let why = m.space_type_problem_before(&falsch, None).unwrap();
        assert!(why.contains("ist kein Typ „Bodenbelag“"), "{why}");
        let fremd = SpaceType {
            ceiling: Some(Guid(0x77)),
            ..bad.clone()
        };
        let why = m.space_type_problem_before(&fremd, None).unwrap();
        assert!(why.contains("Decke fehlt im Projekt"), "{why}");
        assert_eq!((m.layer_sets().len(), m.materials().len()), (typen, stoffe));
        // ein Typ nur im Firmenkatalog
        let mut quelle = Model::with_seed(14);
        let id = quelle.ensure_finish_type(PORCELAIN_TYPE_GUID).unwrap();
        let mut t = quelle.layer_set(id).unwrap().clone();
        t.guid = Guid(0x78);
        t.code = "BB-X".into();
        quelle.add_layer_set(t).unwrap();
        let lib = Library::from_model(&quelle);
        let firma = SpaceType {
            floor: Some(Guid(0x78)),
            ..bad.clone()
        };
        assert!(m.space_type_problem_before(&firma, None).is_some());
        assert_eq!(m.space_type_problem_before(&firma, Some(&lib)), None);
        // Name einer anderen Vorlage
        assert_eq!(m.ensure_space_type(BATH_SPACE_GUID), Some(BATH_SPACE_GUID));
        let zweites = SpaceType {
            guid: Guid(0x79),
            ..bad.clone()
        };
        let why = m.space_type_problem_before(&zweites, None).unwrap();
        assert_eq!(why, "Eine Vorlage „Bad“ gibt es schon");
        // Werkstyp, der sich nicht anlegen lässt: Anstrich umgestellt
        let mut m = Model::with_seed(15);
        let a = m.ensure_finish_material(PAINT_GUID).unwrap();
        let x = Material {
            category: MatCategory::Plaster,
            ..m.material(a).unwrap().clone()
        };
        assert!(m.set_material(a, x));
        let why = m.space_type_problem_before(&bad, None).unwrap();
        assert!(why.contains("fehlt im Projekt"), "{why}");
    }

    /// Firmenkatalog: Vorlage samt Ausbautypen hinaus, Rundlauf der `.szk`,
    /// in ein leeres Projekt herein.
    #[test]
    fn firmenkatalog() {
        let (m, _) = mit_vorlagen();
        let mut lib = Library::default();
        assert!(export_space_type(&m, &mut lib, BATH_SPACE_GUID));
        assert!(!export_space_type(&m, &mut lib, Guid(0x99)));
        assert_eq!(lib.space_types.len(), 1);
        // Boden, Wand, Fliesen, Decke
        assert_eq!(lib.types.len(), 4);
        let text = write_szk(&lib);
        assert!(text.contains("[spacetype] "));
        let back = read_szk(&text).unwrap();
        assert_eq!(write_szk(&back), text);
        assert_eq!(back.space_types, lib.space_types);
        let mut neu = Model::with_seed(9);
        assert!(import_space_type(&mut neu, &back, BATH_SPACE_GUID));
        assert_eq!(
            neu.space_type(BATH_SPACE_GUID),
            m.space_type(BATH_SPACE_GUID)
        );
        assert!(neu.check().is_empty(), "{:?}", neu.check());
        assert!(!import_space_type(&mut neu, &back, Guid(0x99)));
    }
}

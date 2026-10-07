//! Eine Tabelle je Bauteilart (R1, projektstruktur/bauteil-integration.md
//! §4): Name, Präfix, IFC-Klasse, Kostengruppe, Rang im Mengenfenster und
//! Dateiwort stehen nur hier. [`Category`], die Mengenliste, die Datei und
//! die Texte der App lesen aus [`spec`]. Eine neue Kategorie ohne Eintrag
//! übersetzt nicht, weil [`spec`] jede Kategorie genau einmal nennt.

use crate::element::Category;
use crate::library::TypeCategory;

/// Grammatisches Geschlecht des Namens, für Sätze wie „Die Decke bleibt,
/// sie gehört …“.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Genus {
    Feminine,
    Masculine,
    Neuter,
}

impl Genus {
    /// Bestimmter Artikel im Nominativ, groß geschrieben.
    pub fn article(self) -> &'static str {
        match self {
            Genus::Feminine => "Die",
            Genus::Masculine => "Der",
            Genus::Neuter => "Das",
        }
    }

    /// Personalpronomen im Nominativ.
    pub fn pronoun(self) -> &'static str {
        match self {
            Genus::Feminine => "sie",
            Genus::Masculine => "er",
            Genus::Neuter => "es",
        }
    }
}

/// Angaben zu einer Bauteilart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindSpec {
    pub category: Category,
    /// Name, z. B. „Geschossdecke“ (Eigenschaften, Mengenliste).
    pub name: &'static str,
    /// Kurzname und Mehrzahl in Sätzen, z. B. „Decke“, „Decken“.
    pub short: &'static str,
    pub plural: &'static str,
    pub genus: Genus,
    /// Präfix der Bauteilnummer, z. B. „AW“ für AW-001.
    pub prefix: &'static str,
    /// IFC-Klasse mit vordefiniertem Typ, falls nötig.
    pub ifc: &'static str,
    /// Kostengruppe nach DIN 276:2018 (Räume und Öffnungen haben keine).
    pub kg: Option<u16>,
    /// Reihenfolge der Gruppen im Mengenfenster nach Bauablauf.
    pub qto_rank: u8,
    /// Wort in der Datei (`cat=`).
    pub szo: &'static str,
    /// IFC-Eigenschaft IsExternal.
    pub external: bool,
    /// Typart, die zu Bauteilen der Kategorie passt; `None`: ohne Typ.
    pub type_category: Option<TypeCategory>,
    /// Ein Typ ist Pflicht (Wände). Sonst gilt ohne Typ der gedachte
    /// Einschicht-Aufbau ([`crate::Model::element_layers`]).
    pub needs_type: bool,
    /// Gibt es je Gebäude genau einmal (Sätze ohne Zahl).
    pub once: bool,
    /// ATV-Nummer des Gewerks, wenn der Baustoff keins vorschlägt
    /// (Paket 1a); bei allen heutigen Arten keins.
    pub default_trade: Option<&'static str>,
}

impl KindSpec {
    /// Überschrift einer Liste, z. B. „AUSSENWÄNDE“.
    pub fn heading(&self) -> String {
        self.plural.to_uppercase()
    }
}

/// Angaben zur Bauteilart `c`.
pub fn spec(c: Category) -> &'static KindSpec {
    match c {
        Category::ExteriorWall => &EXTERIOR_WALL,
        Category::InteriorWall => &INTERIOR_WALL,
        Category::Floor => &FLOOR,
        Category::GroundSlab => &GROUND_SLAB,
        Category::Roof => &ROOF,
        Category::Window => &WINDOW,
        Category::Door => &DOOR,
        Category::Opening => &OPENING,
        Category::Space => &SPACE,
        Category::StripFooting => &STRIP_FOOTING,
        Category::EdgeInsulation => &EDGE_INSULATION,
        Category::SoffitInsulation => &SOFFIT_INSULATION,
    }
}

/// Rang im Mengenfenster für alles, was dort keinen eigenen Platz hat.
const LAST: u8 = 7;

const EXTERIOR_WALL: KindSpec = KindSpec {
    category: Category::ExteriorWall,
    name: "Außenwand",
    short: "Außenwand",
    plural: "Außenwände",
    genus: Genus::Feminine,
    prefix: "AW",
    ifc: "IfcWall",
    kg: Some(330),
    qto_rank: 2,
    szo: "exterior",
    external: true,
    type_category: Some(TypeCategory::ExteriorWall),
    needs_type: true,
    once: false,
    default_trade: None,
};

const INTERIOR_WALL: KindSpec = KindSpec {
    category: Category::InteriorWall,
    name: "Innenwand",
    short: "Innenwand",
    plural: "Innenwände",
    genus: Genus::Feminine,
    prefix: "IW",
    ifc: "IfcWall",
    kg: Some(340),
    qto_rank: 4,
    szo: "interior",
    external: false,
    type_category: Some(TypeCategory::InteriorWall),
    needs_type: true,
    once: false,
    default_trade: None,
};

const FLOOR: KindSpec = KindSpec {
    category: Category::Floor,
    name: "Geschossdecke",
    short: "Decke",
    plural: "Decken",
    genus: Genus::Feminine,
    prefix: "DE",
    ifc: "IfcSlab.FLOOR",
    kg: Some(350),
    qto_rank: 5,
    szo: "floor",
    external: false,
    type_category: Some(TypeCategory::Floor),
    needs_type: false,
    once: false,
    default_trade: None,
};

const GROUND_SLAB: KindSpec = KindSpec {
    category: Category::GroundSlab,
    name: "Sohlplatte",
    short: "Sohlplatte",
    plural: "Sohlplatten",
    genus: Genus::Feminine,
    prefix: "SP",
    ifc: "IfcSlab.BASESLAB",
    kg: Some(322),
    qto_rank: 1,
    szo: "groundslab",
    external: false,
    type_category: Some(TypeCategory::GroundSlab),
    needs_type: false,
    once: true,
    default_trade: None,
};

const ROOF: KindSpec = KindSpec {
    category: Category::Roof,
    name: "Dach",
    short: "Dach",
    plural: "Dächer",
    genus: Genus::Neuter,
    prefix: "DA",
    ifc: "IfcRoof",
    kg: Some(360),
    qto_rank: LAST,
    szo: "roof",
    external: false,
    type_category: None,
    needs_type: false,
    once: false,
    default_trade: None,
};

const WINDOW: KindSpec = KindSpec {
    category: Category::Window,
    name: "Fenster",
    short: "Fenster",
    plural: "Fenster",
    genus: Genus::Neuter,
    prefix: "FE",
    ifc: "IfcWindow",
    kg: Some(330),
    qto_rank: LAST,
    szo: "window",
    external: false,
    type_category: None,
    needs_type: false,
    once: false,
    default_trade: None,
};

const DOOR: KindSpec = KindSpec {
    category: Category::Door,
    name: "Tür",
    short: "Tür",
    plural: "Türen",
    genus: Genus::Feminine,
    prefix: "TU",
    ifc: "IfcDoor",
    kg: Some(340),
    qto_rank: LAST,
    szo: "door",
    external: false,
    type_category: None,
    needs_type: false,
    once: false,
    default_trade: None,
};

const OPENING: KindSpec = KindSpec {
    category: Category::Opening,
    name: "Öffnung",
    short: "Öffnung",
    plural: "Öffnungen",
    genus: Genus::Feminine,
    prefix: "OE",
    ifc: "IfcOpeningElement",
    kg: None,
    qto_rank: LAST,
    szo: "opening",
    external: false,
    type_category: None,
    needs_type: false,
    once: false,
    default_trade: None,
};

const SPACE: KindSpec = KindSpec {
    category: Category::Space,
    name: "Raum",
    short: "Raum",
    plural: "Räume",
    genus: Genus::Masculine,
    prefix: "R",
    ifc: "IfcSpace",
    kg: None,
    qto_rank: LAST,
    szo: "space",
    external: false,
    type_category: None,
    needs_type: false,
    once: false,
    default_trade: None,
};

const STRIP_FOOTING: KindSpec = KindSpec {
    category: Category::StripFooting,
    name: "Frostschürze",
    short: "Frostschürze",
    plural: "Frostschürzen",
    genus: Genus::Feminine,
    prefix: "FS",
    ifc: "IfcFooting.STRIP_FOOTING",
    kg: Some(322),
    qto_rank: 0,
    szo: "stripfooting",
    external: false,
    type_category: Some(TypeCategory::StripFooting),
    needs_type: false,
    once: true,
    default_trade: None,
};

/// Über IfcRelAggregates Teil der Wand; im Mengenfenster neben den
/// Außenwänden (K5).
const EDGE_INSULATION: KindSpec = KindSpec {
    category: Category::EdgeInsulation,
    name: "Randdämmstreifen",
    short: "Randdämmstreifen",
    plural: "Randdämmstreifen",
    genus: Genus::Masculine,
    prefix: "RD",
    ifc: "IfcBuildingElementPart.INSULATION",
    kg: Some(330),
    qto_rank: 3,
    szo: "edgeinsulation",
    external: false,
    type_category: None,
    needs_type: false,
    once: false,
    default_trade: None,
};

/// Deckenbekleidung (DIN 276:2018-12; 353 sind dort Deckenbeläge); im
/// Mengenfenster unter ihrer Decke.
const SOFFIT_INSULATION: KindSpec = KindSpec {
    category: Category::SoffitInsulation,
    name: "Untersichtdämmung",
    short: "Untersichtdämmung",
    plural: "Untersichtdämmungen",
    genus: Genus::Feminine,
    prefix: "UD",
    ifc: "IfcCovering.INSULATION",
    kg: Some(354),
    qto_rank: 6,
    szo: "soffitinsulation",
    external: false,
    type_category: None,
    needs_type: false,
    once: false,
    default_trade: None,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Jede Kategorie hat genau ihren Eintrag; Präfixe und Dateiwörter sind
    /// eindeutig (Nummern und Datei hängen daran).
    #[test]
    fn jede_kategorie_einmal() {
        for c in Category::ALL {
            assert_eq!(spec(c).category, c);
        }
        for (i, a) in Category::ALL.iter().enumerate() {
            for b in &Category::ALL[i + 1..] {
                assert_ne!(spec(*a).prefix, spec(*b).prefix, "{a:?} {b:?}");
                assert_ne!(spec(*a).szo, spec(*b).szo, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn ueberschrift() {
        assert_eq!(spec(Category::ExteriorWall).heading(), "AUSSENWÄNDE");
        assert_eq!(spec(Category::InteriorWall).heading(), "INNENWÄNDE");
    }

    /// Typarten verweisen zurück auf ihre Kategorie; Pflicht ist ein Typ nur
    /// bei Wänden.
    #[test]
    fn typarten_passen() {
        for c in Category::ALL {
            let k = spec(c);
            if let Some(t) = k.type_category {
                assert_eq!(TypeCategory::of(c), Some(t));
                assert_eq!(t.category(), c);
                assert_eq!(t.name(), k.name);
                assert_eq!(t.prefix(), k.prefix);
            }
            assert_eq!(
                k.needs_type,
                k.type_category.is_some_and(TypeCategory::is_wall),
                "{c:?}"
            );
        }
        for t in TypeCategory::ALL {
            assert_eq!(spec(t.category()).type_category, Some(t));
        }
    }
}

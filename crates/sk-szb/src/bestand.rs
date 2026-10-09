//! Was Skizzeo schon kennt (Vertrag §9 und §10): Werks-Baustoffe mit
//! Schlüssel, Gewerke, Kostengruppen, Werks-Leistungen und belegte Präfixe.
//! Die Prüfung braucht das, um Schlüssel und Kennungen einzuordnen. Die App
//! gleicht die Tafel in einem Test mit Modell und werk.szk ab.

/// Ein Werks-Baustoff: Schlüssel, Name, Kategorie, Farbe, Gewerk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WerkBaustoff {
    pub key: &'static str,
    pub name: &'static str,
    pub kategorie: &'static str,
    pub farbe: &'static str,
    pub gewerk: u32,
}

/// Eine Werks-Leistung: Kennung, Einheit, Mengenbezug, Kurztext.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WerkLeistung {
    pub id: &'static str,
    pub einheit: &'static str,
    pub bezug: &'static str,
    pub kurz: &'static str,
}

/// Bestand, gegen den geprüft wird.
#[derive(Clone, Debug, PartialEq)]
pub struct Bestand {
    pub baustoffe: Vec<WerkBaustoff>,
    /// ATV-Nummer und Name.
    pub gewerke: Vec<(u32, &'static str)>,
    pub kgs: Vec<u32>,
    pub leistungen: Vec<WerkLeistung>,
    /// Belegte Präfixe; dazu kommen die anderer installierter Bauteile.
    pub praefixe: Vec<String>,
    /// Bauteilarten, die Skizzeo kennt (Wort für `art=`).
    pub arten: Vec<&'static str>,
}

const fn b(
    key: &'static str,
    name: &'static str,
    kategorie: &'static str,
    farbe: &'static str,
    gewerk: u32,
) -> WerkBaustoff {
    WerkBaustoff {
        key,
        name,
        kategorie,
        farbe,
        gewerk,
    }
}

const fn l(
    id: &'static str,
    einheit: &'static str,
    bezug: &'static str,
    kurz: &'static str,
) -> WerkLeistung {
    WerkLeistung {
        id,
        einheit,
        bezug,
        kurz,
    }
}

/// Werks-Baustoffe nach Vertrag 0.5 §9.
pub const BAUSTOFFE: [WerkBaustoff; 10] = [
    b("stahlbeton", "Stahlbeton", "concrete", "8e8e8d", 18331),
    b("porenbeton", "Porenbeton", "masonry", "eeede8", 18330),
    b(
        "verblender",
        "Verblender (Vormauerziegel)",
        "masonry",
        "b5654a",
        18330,
    ),
    b("putz", "Putz", "plaster", "ececed", 18350),
    b(
        "daemmung_wdvs",
        "Dämmung (WDVS)",
        "insulation",
        "f4efdc",
        18345,
    ),
    b(
        "kerndaemmung",
        "Kerndämmung (Mineralwolle)",
        "insulation",
        "ece2aa",
        18330,
    ),
    b("randdaemmung", "Randdämmung", "insulation", "ece6c4", 18330),
    b(
        "daemmung_hart",
        "Dämmung hart (Terrasse)",
        "insulation",
        "d6e2ec",
        18338,
    ),
    b("titanzink", "Titanzink 0,7", "metal", "969ea4", 18339),
    b(
        "terrassenbelag",
        "Terrassenbelag",
        "concrete",
        "c4beb2",
        18338,
    ),
];

/// Gewerke im Werksbestand (§10).
pub const GEWERKE: [(u32, &str); 6] = [
    (18330, "Mauerarbeiten"),
    (18331, "Betonarbeiten"),
    (18338, "Dachdeckungs- und Dachabdichtungsarbeiten"),
    (18339, "Klempnerarbeiten"),
    (18345, "Wärmedämm-Verbundsysteme"),
    (18350, "Putz- und Stuckarbeiten"),
];

/// Kostengruppen, die der Werksbestand kennt.
pub const KGS: [u32; 12] = [322, 330, 331, 333, 340, 343, 350, 351, 354, 359, 360, 363];

/// In Skizzeo belegte Präfixe (§5).
pub const BELEGT: [&str; 15] = [
    "AW", "IW", "DE", "SP", "DA", "FE", "TU", "OE", "R", "FS", "RD", "UD", "DT", "AB", "GB",
];

/// Bauteilarten in Skizzeo (§10).
pub const ARTEN: [&str; 9] = [
    "exterior",
    "interior",
    "floor",
    "groundslab",
    "stripfooting",
    "edgeinsulation",
    "soffitinsulation",
    "roofterrace",
    "coping",
];

/// Werks-Leistungen (§10, aus werk.szk).
pub const LEISTUNGEN: [WerkLeistung; 22] = [
    l(
        "1S7bUW0010080200000001",
        "m3",
        "volume",
        "Bodenplatte Stb C25/30 XC2 d=18-25cm",
    ),
    l(
        "1S7bUW0010080200000002",
        "m3",
        "volume",
        "Frostschürze Stb C25/30 b=30-45cm erdgeschalt",
    ),
    l(
        "1S7bUW0010080200000003",
        "m3",
        "volume",
        "Stb-Decke Ortbeton C25/30 XC1 d=18-25cm",
    ),
    l(
        "1S7bUW0010080200000004",
        "m2",
        "formwork",
        "Deckenschalung glatt, kein Sichtbeton, Stützhöhe bis 3,0m",
    ),
    l(
        "1S7bUW0010080200000005",
        "m",
        "perimeter",
        "Randschalung Decke/Bodenplatte h bis 25cm",
    ),
    l(
        "1S7bUW0010080200000006",
        "t",
        "steel",
        "Betonstahl B500 liefern, schneiden, biegen, verlegen",
    ),
    l(
        "1S7bUW0010080200000007",
        "m2",
        "area",
        "AW Porenbeton-Planstein PP2-0,35 d=17,5cm Dünnbettmörtel",
    ),
    l(
        "1S7bUW0010080200000008",
        "m2",
        "area",
        "AW Porenbeton-Planstein PP2-0,35 d=24cm Dünnbettmörtel",
    ),
    l(
        "1S7bUW0010080200000009",
        "m2",
        "area",
        "AW Porenbeton-Planstein PP2-0,35 d=36,5cm Dünnbettmörtel",
    ),
    l(
        "1S7bUW001008020000000A",
        "m2",
        "area",
        "IW Porenbeton-Planstein PP2-0,35 d=24cm Dünnbettmörtel",
    ),
    l(
        "1S7bUW001008020000000B",
        "m2",
        "area",
        "IW Porenbeton-Planstein PP2-0,35 d=17,5cm Dünnbettmörtel",
    ),
    l(
        "1S7bUW001008020000000C",
        "m2",
        "area",
        "IW Porenbeton-Planbauplatte d=11,5cm Dünnbettmörtel",
    ),
    l(
        "1S7bUW001008020000000D",
        "m",
        "length",
        "Randdämmstreifen an Deckenrand, MW d=6cm",
    ),
    l(
        "1S7bUW001008020000000E",
        "m2",
        "area",
        "Kerndämmung MW-Platte WLS 035 d=140mm 2-schal. Mauerwerk",
    ),
    l(
        "1S7bUW001008020000000F",
        "m2",
        "area",
        "Verblendschale Klinker NF d=11,5cm Läuferverband verfugt",
    ),
    l(
        "1S7bUW001008020000000G",
        "m",
        "length",
        "Abfangung Verblendschale, Konsolanker Edelstahl",
    ),
    l(
        "1S7bUW001008020000000H",
        "m2",
        "area",
        "WDVS EPS 035 d=120mm, Kleber, Dübel, Armierung, Oberputz",
    ),
    l(
        "1S7bUW001008020000000I",
        "m2",
        "area",
        "WDVS EPS 035 d=140mm, Kleber, Dübel, Armierung, Oberputz",
    ),
    l(
        "1S7bUW001008020000000J",
        "m2",
        "area",
        "Untersichtdämmung Decke EPS d=120mm geklebt, verputzt",
    ),
    l(
        "1S7bUW001008020000000K",
        "m2",
        "area",
        "Innenputz Gips maschinell d=15mm Q2",
    ),
    l(
        "1S7bUW001008020000000L",
        "m2",
        "area",
        "Untersichtdämmung Decke MW-Lamelle d=120mm geklebt, verputzt",
    ),
    l(
        "1S7bUW001008020000000M",
        "m2",
        "area",
        "Untersichtdämmung Decke MW-Platte d=120mm gedübelt, verputzt",
    ),
];

impl Bestand {
    /// Der Werksbestand nach Vertrag 0.5.
    pub fn werk() -> Bestand {
        Bestand {
            baustoffe: BAUSTOFFE.to_vec(),
            gewerke: GEWERKE.to_vec(),
            kgs: KGS.to_vec(),
            leistungen: LEISTUNGEN.to_vec(),
            praefixe: BELEGT.iter().map(|s| s.to_string()).collect(),
            arten: ARTEN.to_vec(),
        }
    }

    pub fn baustoff(&self, key: &str) -> Option<&WerkBaustoff> {
        self.baustoffe.iter().find(|b| b.key == key)
    }

    pub fn leistung(&self, id: &str) -> Option<&WerkLeistung> {
        self.leistungen.iter().find(|l| l.id == id)
    }

    pub fn hat_gewerk(&self, nr: u32) -> bool {
        self.gewerke.iter().any(|g| g.0 == nr)
    }
}

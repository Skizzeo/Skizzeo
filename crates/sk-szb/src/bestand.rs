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

/// Eine Werks-Leistung: Kennung, Einheit, Mengenbezug, Kurztext und
/// Dickenband in mm (werk.szk `tmin`/`tmax`), wenn sie eines hat.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WerkLeistung {
    pub id: &'static str,
    pub einheit: &'static str,
    pub bezug: &'static str,
    pub kurz: &'static str,
    pub band: Option<(u32, u32)>,
}

/// Ein Werks-Artikel für `stoffe=` (§10): Kennung, Einheit, Name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WerkArtikel {
    pub id: &'static str,
    pub einheit: &'static str,
    pub name: &'static str,
}

/// Bestand, gegen den geprüft wird.
#[derive(Clone, Debug, PartialEq)]
pub struct Bestand {
    pub baustoffe: Vec<WerkBaustoff>,
    /// ATV-Nummer und Name.
    pub gewerke: Vec<(u32, &'static str)>,
    pub kgs: Vec<u32>,
    pub leistungen: Vec<WerkLeistung>,
    pub artikel: Vec<WerkArtikel>,
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
        band: None,
    }
}

const fn a(id: &'static str, einheit: &'static str, name: &'static str) -> WerkArtikel {
    WerkArtikel { id, einheit, name }
}

/// Dickenbänder der Werks-Leistungen in mm (§10, werk.szk `tmin`/`tmax`).
pub const BAENDER: [(&str, u32, u32); 12] = [
    ("1S7bUW0010080200000001", 180, 250),
    ("1S7bUW0010080200000003", 180, 250),
    ("1S7bUW0010080200000007", 170, 180),
    ("1S7bUW0010080200000008", 230, 250),
    ("1S7bUW0010080200000009", 355, 375),
    ("1S7bUW001008020000000A", 230, 250),
    ("1S7bUW001008020000000B", 170, 180),
    ("1S7bUW001008020000000C", 110, 120),
    ("1S7bUW001008020000000E", 130, 150),
    ("1S7bUW001008020000000F", 105, 125),
    ("1S7bUW001008020000000H", 100, 130),
    ("1S7bUW001008020000000I", 131, 160),
];

/// Werks-Artikel (§10, aus werk.szk).
pub const ARTIKEL: [WerkArtikel; 67] = [
    a(
        "1S7bUW0010080100000001",
        "m2",
        "Porenbeton-Planbauplatte PP2-0,35 d=11,5cm",
    ),
    a(
        "1S7bUW0010080100000002",
        "m2",
        "Porenbeton-Planstein PP2-0,35 d=17,5cm",
    ),
    a(
        "1S7bUW0010080100000003",
        "m2",
        "Porenbeton-Planstein PP2-0,35 d=24cm",
    ),
    a(
        "1S7bUW0010080100000004",
        "m2",
        "Porenbeton-Planstein PP2-0,35 d=36,5cm",
    ),
    a(
        "1S7bUW0010080100000005",
        "kg",
        "Dünnbettmörtel für Porenbeton",
    ),
    a(
        "1S7bUW0010080100000006",
        "m3",
        "Transportbeton C25/30 XC1-XC2 F3",
    ),
    a(
        "1S7bUW0010080100000007",
        "t",
        "Betonstahl B500, geschnitten und gebogen",
    ),
    a(
        "1S7bUW0010080100000008",
        "m2",
        "Deckenschalung, Vorhaltung und Verschleiß",
    ),
    a(
        "1S7bUW0010080100000009",
        "m",
        "Randschalung, Vorhaltung und Verschleiß",
    ),
    a(
        "1S7bUW001008010000000A",
        "m2",
        "Kerndämmplatte MW 035 d=140mm",
    ),
    a(
        "1S7bUW001008010000000B",
        "m2",
        "Klinker NF inkl. Vormauermörtel",
    ),
    a(
        "1S7bUW001008010000000C",
        "m",
        "Konsolanker Edelstahl, Abfangung",
    ),
    a("1S7bUW001008010000000D", "m", "Randdämmstreifen MW"),
    a(
        "1S7bUW001008010000000E",
        "m2",
        "WDVS-System EPS 035 d=120mm inkl. Kleber, Dübel, Gewebe, Oberputz",
    ),
    a(
        "1S7bUW001008010000000F",
        "m2",
        "WDVS-System EPS 035 d=140mm inkl. Kleber, Dübel, Gewebe, Oberputz",
    ),
    a(
        "1S7bUW001008010000000G",
        "m2",
        "EPS-Dämmplatte 035 d=120mm, zwischen Lattung",
    ),
    a("1S7bUW001008010000000H", "m2", "Gipsputz maschinell 15 mm"),
    a(
        "1S7bUW001008010000000I",
        "m2",
        "MW-Klemmplatte 035 d=120mm A1, zwischen Lattung",
    ),
    a(
        "1S7bUW001008010000000J",
        "m2",
        "MW-Dämmplatte 035 d=120mm A1, zwischen Lattung",
    ),
    a(
        "1S7bUW001008010000000K",
        "m3",
        "Kies 16/32 kapillarbrechend, frei Baustelle",
    ),
    a(
        "1S7bUW001008010000000L",
        "m3",
        "Schotter 0/32 Füllmaterial, frei Baustelle",
    ),
    a(
        "1S7bUW001008010000000M",
        "m3",
        "XPS-Perimeterdämmplatte 035 CS 300 kPa, lastabtragend",
    ),
    a(
        "1S7bUW001008010000000N",
        "m3",
        "EPS-Dachdämmplatte 035 DAA dh, druckfest",
    ),
    a(
        "1S7bUW001008010000000O",
        "m2",
        "Bitumen-Schweißbahn mit Alu-Einlage, Dampfsperre",
    ),
    a("1S7bUW001008010000000P", "kg", "Bitumen-Voranstrich kalt"),
    a(
        "1S7bUW001008010000000Q",
        "m2",
        "Polymerbitumen-Schweißbahn PYE PV 200 S5",
    ),
    a(
        "1S7bUW001008010000000R",
        "m2",
        "Terrassenplatte Beton 40mm, Splittbett und Schutzlage",
    ),
    a(
        "1S7bUW001008010000000S",
        "m",
        "Mauerabdeckung Titanzink 0,7mm Zuschnitt 400, Halter",
    ),
    a(
        "1S7bUW001008010000000T",
        "m3",
        "EPS-Dachdämmplatte 035 DAA dh, Flachdach",
    ),
    a(
        "1S7bUW001008010000000U",
        "m2",
        "Faserzementtafel 8mm inkl. Fugenband und Edelstahlschrauben",
    ),
    a(
        "1S7bUW001008010000000V",
        "st",
        "Attikaablauf DN 100 wärmegedämmt, waagerecht, mit Dampfsperranschluss",
    ),
    a(
        "1S7bUW001008010000000W",
        "st",
        "Notüberlauf rechteckig für Attika, mit Anschlussflansch",
    ),
    a(
        "1S7bUW001008010000000X",
        "m3",
        "EPS-Dachdämmplatte 035 DAA dh, Streifen Attikakrone",
    ),
    a(
        "1S7bUW001008010000000Y",
        "st",
        "Wasserfangkasten Titanzink 0,7mm mit Laubfangsieb, Stutzen DN 100",
    ),
    a(
        "1S7bUW001008010000000Z",
        "m",
        "Fallrohr Titanzink rund DN 100, 0,7mm, DIN EN 612",
    ),
    a(
        "1S7bUW001008010000000a",
        "st",
        "Rohrschelle DN 100 mit Stockschraube, Abstandsmontage WDVS",
    ),
    a(
        "1S7bUW001008010000000b",
        "st",
        "Regenstandrohr Guss DN 100, 1,0m, mit Reinigungsöffnung",
    ),
    a(
        "1S7bUW001008010000000c",
        "st",
        "Wasserfangkasten Notüberlauf Titanzink, Stutzen DN 100",
    ),
    a(
        "1S7bUW001008010000000d",
        "st",
        "Auslaufbogen Titanzink DN 100",
    ),
    a(
        "1S7bUW001008010000000e",
        "st",
        "Wasserspeier Titanzink, 300mm Auskragung, Tropfkante",
    ),
    a(
        "1S7bUW001008010000000f",
        "st",
        "Kontrollablauf DN 50 mit Klebeflansch und Insektenschutz",
    ),
    a(
        "1S7bUW001008010000000g",
        "m3",
        "Gipsputz maschinell, Werktrockenmörtel, innen",
    ),
    a(
        "1S7bUW001008010000000h",
        "m3",
        "Kalkzementputz maschinell CS II, Werktrockenmörtel",
    ),
    a(
        "1S7bUW001008010000000i",
        "m",
        "Eckschutzschiene verzinkt für Innenputz 10–15mm",
    ),
    a(
        "1S7bUW001008010000000j",
        "m3",
        "Zementestrich CT-C25-F4, Mischgut frei Baustelle",
    ),
    a(
        "1S7bUW001008010000000k",
        "m2",
        "PE-Folie 0,2mm als Trennlage unter Estrich",
    ),
    a(
        "1S7bUW001008010000000l",
        "m3",
        "Trittschalldämmplatte EPS-T DES sg",
    ),
    a(
        "1S7bUW001008010000000m",
        "m3",
        "Wärmedämmplatte EPS 035 DEO dm, unter Estrich",
    ),
    a(
        "1S7bUW001008010000000n",
        "m",
        "Randdämmstreifen PE-Schaum 8×100mm mit Folienlasche",
    ),
    a(
        "1S7bUW001008010000000o",
        "m2",
        "Bitumen-Schweißbahn G 200 S4, Abdichtung Bodenplatte",
    ),
    a(
        "1S7bUW001008010000000p",
        "m2",
        "Feinsteinzeug 60×60 R10, Mittelklasse",
    ),
    a(
        "1S7bUW001008010000000q",
        "kg",
        "Flexkleber C2 TE S1 für Dünnbett",
    ),
    a("1S7bUW001008010000000r", "kg", "Fugenmörtel CG2 WA"),
    a(
        "1S7bUW001008010000000s",
        "m2",
        "Wandfliese Steingut 30×60 weiß glasiert",
    ),
    a(
        "1S7bUW001008010000000t",
        "m2",
        "Dichtschlämme flexibel 2-lagig mit Dichtband, DIN 18534",
    ),
    a(
        "1S7bUW001008010000000u",
        "m",
        "Sanitärsilikon mit Hinterfüllprofil",
    ),
    a(
        "1S7bUW001008010000000v",
        "m",
        "Fliesensockel aus Feinsteinzeug geschnitten, h=60mm, mit Kleber",
    ),
    a(
        "1S7bUW001008010000000w",
        "m2",
        "Massivparkett Eiche Stab 20mm, Sortierung Natur",
    ),
    a(
        "1S7bUW001008010000000x",
        "kg",
        "Parkettklebstoff SMP elastisch",
    ),
    a(
        "1S7bUW001008010000000y",
        "m2",
        "Parkettversiegelung, Grundierung und 2 Deckschichten",
    ),
    a(
        "1S7bUW001008010000000z",
        "m",
        "Sockelleiste Eiche massiv 60×15 lackiert, Clips",
    ),
    a(
        "1S7bUW001008010000000_",
        "m2",
        "Innendispersion Klasse 2 mit Tiefgrund, für 2 Anstriche",
    ),
    a(
        "1S7bUW001008010000000$",
        "m2",
        "Raufasertapete mittlere Körnung",
    ),
    a("1S7bUW0010080100000010", "m2", "Tapetenkleister"),
    a(
        "1S7bUW0010080100000011",
        "m",
        "Übergangsprofil Aluminium eloxiert, Höhenausgleich bis 15mm",
    ),
    a(
        "1S7bUW0010080100000012",
        "m",
        "Abschluss- und Übergangsprofil Edelstahl für Fliesen 10–12mm",
    ),
    a(
        "1S7bUW0010080100000013",
        "m3",
        "Füllsand 0/2 verdichtungsfähig, frei Baustelle",
    ),
];

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
pub const GEWERKE: [(u32, &str); 17] = [
    (18300, "Erdarbeiten"),
    (18320, "Landschaftsbauarbeiten"),
    (18330, "Mauerarbeiten"),
    (18331, "Betonarbeiten"),
    (18334, "Zimmer- und Holzbauarbeiten"),
    (18336, "Abdichtungsarbeiten"),
    (18338, "Dachdeckungs- und Dachabdichtungsarbeiten"),
    (18339, "Klempnerarbeiten"),
    (18345, "Wärmedämm-Verbundsysteme"),
    (18350, "Putz- und Stuckarbeiten"),
    (18351, "Vorgehängte hinterlüftete Fassaden"),
    (18352, "Fliesen- und Plattenarbeiten"),
    (18353, "Estricharbeiten"),
    (18356, "Parkett- und Holzpflasterarbeiten"),
    (18363, "Maler- und Lackierarbeiten"),
    (18366, "Tapezierarbeiten"),
    (18451, "Gerüstarbeiten"),
];

/// Kostengruppen, die der Werksbestand kennt.
pub const KGS: [u32; 20] = [
    311, 322, 325, 330, 331, 333, 335, 336, 340, 343, 350, 351, 354, 359, 360, 363, 391, 392, 511,
    571,
];

/// In Skizzeo belegte Präfixe (§5).
pub const BELEGT: [&str; 18] = [
    "AW", "IW", "DE", "SP", "DA", "FE", "TU", "OE", "R", "FS", "RD", "UD", "DT", "AB", "GB", "AK",
    "PD", "RF",
];

/// Bauteilarten in Skizzeo (§10).
pub const ARTEN: [&str; 13] = [
    "exterior",
    "interior",
    "floor",
    "groundslab",
    "stripfooting",
    "edgeinsulation",
    "soffitinsulation",
    "perimeterinsulation",
    "roofterrace",
    "coping",
    "parapet",
    "roof",
    "downpipe",
];

/// Werks-Leistungen (§10, aus werk.szk).
pub const LEISTUNGEN: [WerkLeistung; 123] = [
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
        "Untersichtdämmung EPS 035 d=120mm zwischen Lattung, über Kopf",
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
        "Untersichtdämmung MW 035 d=120mm A1 zwischen Lattung, über Kopf",
    ),
    l(
        "1S7bUW001008020000000M",
        "m2",
        "area",
        "Untersichtdämmung MW-Platte 035 d=120mm zwischen Lattung, über Kopf",
    ),
    l(
        "1S7bUW001008020000000N",
        "m3",
        "auto",
        "Oberboden bis 30cm abtragen, seitlich lagern",
    ),
    l(
        "1S7bUW001008020000000O",
        "m3",
        "auto",
        "Auskofferung ausheben Homogenbereich B1, seitlich lagern",
    ),
    l(
        "1S7bUW001008020000000P",
        "m3",
        "auto",
        "Graben Frostschürze ausheben Homogenbereich B1, Wände senkrecht",
    ),
    l(
        "1S7bUW001008020000000Q",
        "m2",
        "auto",
        "Planum herstellen, ±2cm, verdichten",
    ),
    l(
        "1S7bUW001008020000000R",
        "m3",
        "auto",
        "Kapillarbrechende Schicht Kies 16/32 einbauen, verdichten",
    ),
    l(
        "1S7bUW001008020000000S",
        "m3",
        "auto",
        "Auffüllung Schotter 0/32 lagenweise einbauen, verdichten",
    ),
    l(
        "1S7bUW001008020000000T",
        "m3",
        "auto",
        "Arbeitsraum mit gelagertem Aushub verfüllen, verdichten",
    ),
    l(
        "1S7bUW001008020000000U",
        "m3",
        "auto",
        "Aushub laden, abfahren, entsorgen BM-0 (früher Z0)",
    ),
    l(
        "1S7bUW001008020000000V",
        "m2",
        "auto",
        "Böschung mit Folie abdecken, vorhalten, räumen",
    ),
    l(
        "1S7bUW001008020000000W",
        "psch",
        "auto",
        "Baustelleneinrichtung einrichten und räumen",
    ),
    l(
        "1S7bUW001008020000000X",
        "mon",
        "auto",
        "Baustelleneinrichtung vorhalten",
    ),
    l(
        "1S7bUW001008020000000Y",
        "m",
        "auto",
        "Bauzaun Mobilzaun h=2,0m aufstellen, vorhalten, räumen",
    ),
    l(
        "1S7bUW001008020000000Z",
        "st",
        "auto",
        "Bauschild liefern, aufstellen, vorhalten, räumen",
    ),
    l(
        "1S7bUW001008020000000a",
        "psch",
        "auto",
        "Baustromanschluss und Verteiler einrichten und räumen",
    ),
    l(
        "1S7bUW001008020000000b",
        "mon",
        "auto",
        "Baustromverteiler vorhalten",
    ),
    l(
        "1S7bUW001008020000000c",
        "psch",
        "auto",
        "Bauwasseranschluss Standrohr einrichten und räumen",
    ),
    l(
        "1S7bUW001008020000000d",
        "mon",
        "auto",
        "Bauwasser-Standrohr vorhalten",
    ),
    l(
        "1S7bUW001008020000000e",
        "mon",
        "auto",
        "Toilettenkabine mobil vorhalten, inkl. Reinigung",
    ),
    l(
        "1S7bUW001008020000000f",
        "psch",
        "auto",
        "Schnurgerüst herstellen, vorhalten, beseitigen",
    ),
    l(
        "1S7bUW001008020000000g",
        "m2",
        "auto",
        "Fassadengerüst LK3 W09, 4 Wochen Standzeit, auf-/abbauen",
    ),
    l(
        "1S7bUW001008020000000h",
        "m2",
        "area",
        "Perimeterdämmung XPS 300 unter Bodenplatte, lastabtragend",
    ),
    l(
        "1S7bUW001008020000000i",
        "m2",
        "area",
        "Dampfsperre Bitumen-Alu-Schweißbahn vollflächig, inkl. Voranstrich",
    ),
    l(
        "1S7bUW001008020000000j",
        "m2",
        "area",
        "Terrassendämmung EPS 035 DAA dh druckfest, Dicke nach Aufbau",
    ),
    l(
        "1S7bUW001008020000000k",
        "m2",
        "area",
        "Abdichtung Polymerbitumen 2-lagig, Oberlage beschiefert",
    ),
    l(
        "1S7bUW001008020000000l",
        "m",
        "auto",
        "Abdichtungsanschluss an Attika, über Krone geführt, inkl. Keil",
    ),
    l(
        "1S7bUW001008020000000m",
        "m2",
        "area",
        "Terrassenbelag Betonplatten 40mm auf Splittbett, Schutzlage",
    ),
    l(
        "1S7bUW001008020000000n",
        "m",
        "length",
        "Attikaabdeckung Titanzink 0,7mm, Zuschnitt bis 400mm, Halter",
    ),
    l(
        "1S7bUW001008020000000o",
        "m",
        "auto",
        "Zulage Attikaabdeckung Zuschnitt über 400 bis 500mm",
    ),
    l(
        "1S7bUW001008020000000p",
        "m",
        "auto",
        "Zulage Attikaabdeckung Zuschnitt über 500 bis 667mm",
    ),
    l(
        "1S7bUW001008020000000q",
        "m",
        "auto",
        "Zulage Attikaabdeckung Zuschnitt über 667 bis 1000mm",
    ),
    l(
        "1S7bUW001008020000000r",
        "st",
        "auto",
        "Zulage Attikaabdeckung Ecke 90°, gefalzt oder gelötet",
    ),
    l(
        "1S7bUW001008020000000s",
        "st",
        "auto",
        "Zulage Attikaabdeckung Endabschluss mit Stirnblech",
    ),
    l(
        "1S7bUW001008020000000t",
        "m",
        "auto",
        "Grundlattung KVH 60/80 über Kopf, a=80cm, mit Schraubankern",
    ),
    l(
        "1S7bUW001008020000000u",
        "m",
        "auto",
        "Traglattung 30/50 quer, a=40cm, Hinterlüftung",
    ),
    l(
        "1S7bUW001008020000000v",
        "m",
        "auto",
        "Lüftungsprofil Alu gelocht mit Insektenschutz, Untersichtrand",
    ),
    l(
        "1S7bUW001008020000000w",
        "m",
        "auto",
        "Fußpunkt Verblendschale Schaumglas-Dämmstein 115mm, 1. Lage",
    ),
    l(
        "1S7bUW001008020000000x",
        "m",
        "auto",
        "Zulage Fußpunkt Schaumglas-Dämmstein je weitere Lage",
    ),
    l(
        "1S7bUW001008020000000y",
        "m",
        "auto",
        "Mauersperrbahn unter Fußpunkt, B bis 25cm",
    ),
    l(
        "1S7bUW001008020000000z",
        "m",
        "auto",
        "Z-Folie über Fußpunkt, aus dem Schalenzwischenraum geführt",
    ),
    l(
        "1S7bUW001008020000000_",
        "m2",
        "area",
        "Dampfsperre Bitumen-Alu-Schweißbahn Flachdach, inkl. Voranstrich",
    ),
    l(
        "1S7bUW001008020000000$",
        "m2",
        "area",
        "Dachdämmung EPS 035 DAA dh einlagig, Dicke nach Aufbau",
    ),
    l(
        "1S7bUW0010080200000010",
        "m2",
        "area",
        "Abdichtung Polymerbitumen 2-lagig Flachdach, Oberlage beschiefert",
    ),
    l(
        "1S7bUW0010080200000011",
        "m2",
        "area",
        "Attikadämmung innen Flachdach, EPS, Dicke nach Aufbau",
    ),
    l(
        "1S7bUW0010080200000012",
        "st",
        "auto",
        "Attikaablauf DN 100 wärmegedämmt, waagerecht, Dampfsperranschluss",
    ),
    l(
        "1S7bUW0010080200000013",
        "st",
        "auto",
        "Notüberlauf Attika rechteckig, inkl. Durchbruch und Einbindung",
    ),
    l(
        "1S7bUW0010080200000014",
        "m2",
        "area",
        "Untersichtbekleidung Faserzementtafel 8mm über Kopf, verschraubt",
    ),
    l(
        "1S7bUW0010080200000015",
        "m",
        "auto",
        "Abdichtungsanschluss an Aufkantung Flachdach, über Krone, inkl. Keil",
    ),
    l(
        "1S7bUW0010080200000016",
        "st",
        "auto",
        "Zulage Abdichtungsanschluss Innenecke, Eckverstärkung",
    ),
    l(
        "1S7bUW0010080200000017",
        "m3",
        "auto",
        "Gefälledämmung EPS 035 DAA dh, Keilplatten nach Verlegeplan",
    ),
    l(
        "1S7bUW0010080200000018",
        "m",
        "auto",
        "Zulage Gefälledämmung Kehle, Zuschnitt auf Gehrung",
    ),
    l(
        "1S7bUW0010080200000019",
        "m",
        "auto",
        "Zulage Gefälledämmung Grat, Zuschnitt auf Gehrung",
    ),
    l(
        "1S7bUW001008020000001A",
        "m2",
        "area",
        "Attikadämmung Krone Dachterrasse, EPS, Dicke nach Aufbau, unter Blech",
    ),
    l(
        "1S7bUW001008020000001B",
        "m2",
        "area",
        "Attikadämmung Krone Flachdach, EPS, Dicke nach Aufbau, unter Blech",
    ),
    l(
        "1S7bUW001008020000001C",
        "st",
        "auto",
        "Wasserfangkasten Titanzink 0,7mm mit Laubfangsieb, Stutzen DN 100",
    ),
    l(
        "1S7bUW001008020000001D",
        "m",
        "auto",
        "Regenfallrohr Titanzink rund DN 100, 0,7mm, inkl. Verbindungen",
    ),
    l(
        "1S7bUW001008020000001E",
        "st",
        "auto",
        "Rohrschelle DN 100 mit Stockschraube durch WDVS, Abstand bis 2,0m",
    ),
    l(
        "1S7bUW001008020000001F",
        "st",
        "auto",
        "Regenstandrohr Guss DN 100, 1,0m, Reinigungsöffnung, an Grundleitung",
    ),
    l(
        "1S7bUW001008020000001G",
        "st",
        "auto",
        "Wasserfangkasten Notüberlauf Titanzink, Stutzen DN 100",
    ),
    l(
        "1S7bUW001008020000001H",
        "m",
        "auto",
        "Notfallrohr Titanzink rund DN 100, frei auslaufend",
    ),
    l(
        "1S7bUW001008020000001I",
        "st",
        "auto",
        "Auslaufbogen Titanzink DN 100, Rohrende 20cm über Gelände oder Belag",
    ),
    l(
        "1S7bUW001008020000001J",
        "st",
        "auto",
        "Wasserspeier Titanzink am Notüberlauf, 300mm Auskragung",
    ),
    l(
        "1S7bUW001008020000001K",
        "st",
        "auto",
        "Kontrollablauf Dampfsperrebene DN 50 durch die Aufkantung",
    ),
    l(
        "1S7bUW001008020000001L",
        "m",
        "auto",
        "Laibung WDVS bis 1,0m tief, Eckprofil mit Gewebe, Anputzleiste",
    ),
    l(
        "1S7bUW001008020000001M",
        "st",
        "auto",
        "Zulage Diagonalbewehrung Gewebe an Öffnungsecke",
    ),
    l(
        "1S7bUW001008020000001N",
        "m",
        "auto",
        "Laibung Innenputz Gips bis 1,0m tief, Kantenschutzprofil",
    ),
    l(
        "1S7bUW001008020000001O",
        "m",
        "auto",
        "Laibung Außenputz bis 1,0m tief, Kantenprofil",
    ),
    l(
        "1S7bUW001008020000001P",
        "m",
        "auto",
        "Zulage Laibung Verblendmauerwerk, Läufer und Binder verzahnt",
    ),
    l(
        "1S7bUW001008020000001Q",
        "m",
        "auto",
        "Laibung Innenputz Gips an Deckenaussparung, Kantenschutzprofil",
    ),
    l(
        "1S7bUW001008020000001R",
        "m",
        "auto",
        "Putz Deckenstirn in Aussparung, Kantenschutz oben und unten",
    ),
    l(
        "1S7bUW001008020000001S",
        "m3",
        "auto",
        "Oberboden abtragen auf Geländeflächen, seitlich lagern",
    ),
    l(
        "1S7bUW001008020000001T",
        "m3",
        "auto",
        "Boden abtragen bis Planum, Homogenbereich B1, seitlich lagern",
    ),
    l(
        "1S7bUW001008020000001U",
        "m3",
        "auto",
        "Auftrag mit gelagertem Boden, lagenweise einbauen, verdichten",
    ),
    l(
        "1S7bUW001008020000001V",
        "m3",
        "auto",
        "Boden liefern, lagenweise einbauen, verdichten",
    ),
    l(
        "1S7bUW001008020000001W",
        "m3",
        "auto",
        "Überschuss laden, abfahren, entsorgen BM-0 (früher Z0)",
    ),
    l(
        "1S7bUW001008020000001X",
        "m3",
        "auto",
        "Oberboden laden, abfahren, verwerten",
    ),
    l(
        "1S7bUW001008020000001Y",
        "m3",
        "auto",
        "Oberboden liefern, Bodengruppe nach DIN 18915, abladen",
    ),
    l(
        "1S7bUW001008020000001Z",
        "m2",
        "auto",
        "Oberboden andecken, Dicke nach Vorgabe, Feinplanum",
    ),
    l(
        "1S7bUW001008020000001a",
        "m2",
        "auto",
        "Zulage Andecken an Böschung steiler als 1:4, schräg gemessen",
    ),
    l(
        "1S7bUW001008020000001b",
        "m2",
        "area",
        "Abdichtung Bodenplatte W1-E, Bitumen-Schweißbahn 1-lagig, Voranstrich",
    ),
    l(
        "1S7bUW001008020000001c",
        "m2",
        "area",
        "Innenputz Gipsputz maschinell 1-lagig Q2, Dicke nach Raumbuch",
    ),
    l(
        "1S7bUW001008020000001d",
        "m2",
        "area",
        "Deckenputz Gipsputz maschinell 1-lagig Q2, Dicke nach Raumbuch",
    ),
    l(
        "1S7bUW001008020000001e",
        "m2",
        "area",
        "Innenputz Kalkzementputz maschinell 1-lagig, Dicke nach Raumbuch",
    ),
    l(
        "1S7bUW001008020000001f",
        "m",
        "auto",
        "Eckschutzschiene verzinkt an vorspringenden Ecken, eingeputzt",
    ),
    l(
        "1S7bUW001008020000001g",
        "m2",
        "area",
        "Zementestrich CT-C25-F4 schwimmend, Dicke nach Raumbuch",
    ),
    l(
        "1S7bUW001008020000001h",
        "m2",
        "area",
        "Trittschalldämmung EPS-T unter Estrich, einlagig, dicht gestoßen",
    ),
    l(
        "1S7bUW001008020000001i",
        "m2",
        "area",
        "Wärmedämmung EPS 035 DEO unter Estrich, Dicke nach Raumbuch",
    ),
    l(
        "1S7bUW001008020000001j",
        "m",
        "auto",
        "Randdämmstreifen PE 8mm mit Folienlasche an aufgehenden Bauteilen",
    ),
    l(
        "1S7bUW001008020000001k",
        "m2",
        "area",
        "Bodenfliesen Feinsteinzeug 60×60 im Dünnbett, verfugt",
    ),
    l(
        "1S7bUW001008020000001l",
        "m2",
        "area",
        "Wandfliesen 30×60 im Dünnbett, verfugt",
    ),
    l(
        "1S7bUW001008020000001m",
        "m2",
        "area",
        "Verbundabdichtung DIN 18534 2-lagig, Dichtband in Ecken",
    ),
    l(
        "1S7bUW001008020000001n",
        "m",
        "auto",
        "Fliesensockel Feinsteinzeug h=60mm geschnitten, im Dünnbett verfugt",
    ),
    l(
        "1S7bUW001008020000001o",
        "m",
        "auto",
        "Silikonfuge Sanitär mit Hinterfüllprofil, Boden/Wand und Innenecken",
    ),
    l(
        "1S7bUW001008020000001p",
        "m",
        "auto",
        "Übergangsprofil Edelstahl an Fliesenbelag im Türdurchgang, eingefliest",
    ),
    l(
        "1S7bUW001008020000001q",
        "m2",
        "area",
        "Massivparkett Eiche 20mm verklebt, geschliffen, versiegelt",
    ),
    l(
        "1S7bUW001008020000001r",
        "m",
        "auto",
        "Sockelleiste Eiche massiv 60×15 lackiert, verdeckt befestigt",
    ),
    l(
        "1S7bUW001008020000001s",
        "m",
        "auto",
        "Übergangsprofil Aluminium an Belagwechsel im Türdurchgang, geschraubt",
    ),
    l(
        "1S7bUW001008020000001t",
        "m2",
        "area",
        "Dispersionsanstrich Wand Klasse 2, Grundierung und 2 Anstriche",
    ),
    l(
        "1S7bUW001008020000001u",
        "m2",
        "area",
        "Dispersionsanstrich Decke Klasse 2, Grundierung und 2 Anstriche",
    ),
    l(
        "1S7bUW001008020000001v",
        "m2",
        "area",
        "Raufasertapete mittel kleben, auf Stoß, mit Kleister",
    ),
    l(
        "1S7bUW001008020000001w",
        "m3",
        "auto",
        "Füllsand Sandplatte liefern, in Lagen einbauen, verdichten n. Vorgabe",
    ),
    l(
        "1S7bUW001008020000001x",
        "m3",
        "auto",
        "Graben Frostschürze im eingebauten Füllsand ausheben, Sohle eben",
    ),
];

impl Bestand {
    /// Der Werksbestand nach Vertrag 0.5.
    pub fn werk() -> Bestand {
        Bestand {
            baustoffe: BAUSTOFFE.to_vec(),
            gewerke: GEWERKE.to_vec(),
            kgs: KGS.to_vec(),
            leistungen: LEISTUNGEN
                .iter()
                .map(|l| WerkLeistung {
                    band: BAENDER.iter().find(|b| b.0 == l.id).map(|b| (b.1, b.2)),
                    ..*l
                })
                .collect(),
            artikel: ARTIKEL.to_vec(),
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

    pub fn artikel(&self, id: &str) -> Option<&WerkArtikel> {
        self.artikel.iter().find(|a| a.id == id)
    }

    pub fn hat_gewerk(&self, nr: u32) -> bool {
        self.gewerke.iter().any(|g| g.0 == nr)
    }
}

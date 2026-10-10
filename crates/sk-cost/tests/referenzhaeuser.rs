//! Referenzhäuser RH-1 bis RH-3 und RH-5 (Flachdach) auf den Cent (Abnahme
//! KA-0 „Rechnung“, Sollwerte bim/integration/sollwerte-referenzhaeuser.md
//! und sollwerte-rh5.md, Prüfstand der BIM-Integration). Die Häuser liegen
//! unverändert in `referenz/`.

use sk_cost::{lesen, Cent, Dez, Kostenblatt, Kostenspeicher, Umfang};
use sk_model::{qto, szo, GuidGen, Model};

fn laden(text: &str) -> Model {
    szo::read_with(text, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model
}

/// Kurztext, Menge (3 Stellen in Tausendsteln), EP, GP, Stoff-GP in Cent.
type Zeile = (&'static str, i64, i64, i64, i64);

struct Soll {
    datei: &'static str,
    netto: i64,
    material: i64,
    /// Geschoss, Netto in Cent
    geschosse: &'static [(&'static str, i64)],
    /// Fassadengerüst (Titel 1.01): Höhe bis zur höchsten Wandkrone
    geruest: Zeile,
    ausgleich: i64,
    zeilen: &'static [Zeile],
    /// Bauteil, Menge in Tausendsteln
    ohne: &'static [(&'static str, i64)],
}

/// Erdarbeiten aus der Gründung (Werksbestand Stand 8, Gelände = OK Platte,
/// Bodenkennwerte ab Werk), in allen drei Häusern gleich: 1.864,05 €.
const ERDE: &[Zeile] = &[
    (
        "Oberboden bis 30cm abtragen, seitlich lagern",
        42900,
        1320,
        56628,
        0,
    ),
    (
        "Baugrube ausheben Homogenbereich B1, seitlich lagern",
        4752,
        1280,
        6083,
        0,
    ),
    (
        "Graben Frostschürze ausheben, Wände senkrecht, Sohle eben",
        6055,
        3000,
        18165,
        0,
    ),
    ("Planum herstellen, ±2cm, verdichten", 67890, 300, 20367, 0),
    (
        "Kapillarbrechende Schicht Kies 16/32 einbauen, verdichten",
        10184,
        5200,
        52957,
        32589,
    ),
    (
        "Aushub laden, abfahren, entsorgen BM-0 (früher Z0)",
        10807,
        2980,
        32205,
        0,
    ),
];

/// Baustelleneinrichtung (Titel 1.01, Werksbestand Stand 8) ohne Bauzaun
/// und Gerüst, in allen Häusern gleich: zwei Geschosse, 3 Monate
/// Vorhaltung. Der Bauzaun hängt an der Hülle aller Geschosse und steht je
/// Haus in `zeilen`, das Gerüst an der höchsten Wandkrone in `geruest`.
const BE: &[Zeile] = &[
    (
        "Baustelleneinrichtung einrichten und räumen",
        1000,
        250000,
        250000,
        0,
    ),
    ("Baustelleneinrichtung vorhalten", 3000, 40000, 120000, 0),
    (
        "Bauschild liefern, aufstellen, vorhalten, räumen",
        1000,
        60000,
        60000,
        0,
    ),
    (
        "Baustromanschluss und Verteiler einrichten und räumen",
        1000,
        100000,
        100000,
        0,
    ),
    ("Baustromverteiler vorhalten", 3000, 7000, 21000, 0),
    (
        "Bauwasseranschluss Standrohr einrichten und räumen",
        1000,
        45000,
        45000,
        0,
    ),
    ("Bauwasser-Standrohr vorhalten", 3000, 10000, 30000, 0),
    (
        "Toilettenkabine mobil vorhalten, inkl. Reinigung",
        3000,
        13000,
        39000,
        0,
    ),
    (
        "Schnurgerüst herstellen, vorhalten, beseitigen",
        1000,
        40000,
        40000,
        0,
    ),
];

/// Fassadengerüst zweier Geschosse ohne Flachdach: 46,40 m × 6,71 m.
const GERUEST_2G: Zeile = (
    "Fassadengerüst LK3 W09, 4 Wochen Standzeit, auf-/abbauen",
    311344,
    900,
    280210,
    0,
);

/// Zulagen der Attikaabdeckung an der Dachterrasse (Werksbestand Stand 9,
/// Titel 4.02): ein U-förmiger Blechstrang mit zwei Ecken und zwei
/// Endabschlüssen, in RH-1 und RH-3 gleich.
const DT_ZULAGEN: [Zeile; 2] = [
    (
        "Zulage Attikaabdeckung Ecke 90°, gefalzt oder gelötet",
        2000,
        4800,
        9600,
        0,
    ),
    (
        "Zulage Attikaabdeckung Endabschluss mit Stirnblech",
        2000,
        2500,
        5000,
        0,
    ),
];

const RH1: Soll = Soll {
    datei: include_str!("../referenz/rh1-standardhaus.szo"),
    netto: 7595363,
    material: 3281223,
    geruest: GERUEST_2G,
    geschosse: &[
        ("Gründung", 2242615),
        ("Erdgeschoss", 3001329),
        ("Obergeschoss", 2351416),
    ],
    ausgleich: 3,
    zeilen: &[
        (
            "Bodenplatte Stb C25/30 XC2 d=18-25cm",
            17600,
            22600,
            397760,
            334400,
        ),
        (
            "Frostschürze Stb C25/30 b=30-45cm erdgeschalt",
            7024,
            25000,
            175600,
            133456,
        ),
        (
            "Stb-Decke Ortbeton C25/30 XC1 d=18-25cm",
            29809,
            22600,
            673683,
            566371,
        ),
        (
            "Deckenschalung glatt, kein Sichtbeton, Stützhöhe bis 3,0m",
            124059,
            3800,
            471424,
            99247,
        ),
        (
            "Randschalung Decke/Bodenplatte h bis 25cm",
            102760,
            2900,
            298004,
            51380,
        ),
        (
            "Betonstahl B500 liefern, schneiden, biegen, verlegen",
            4670,
            160000,
            747200,
            467000,
        ),
        (
            "AW Porenbeton-Planstein PP2-0,35 d=17,5cm Dünnbettmörtel",
            172224,
            5400,
            930010,
            465005,
        ),
        (
            "WDVS EPS 035 d=140mm, Kleber, Dübel, Armierung, Oberputz",
            199595,
            11600,
            2315302,
            997975,
        ),
        (
            "Bauzaun Mobilzaun h=2,0m aufstellen, vorhalten, räumen",
            60000,
            2050,
            123000,
            0,
        ),
        (
            "Dampfsperre Bitumen-Alu-Schweißbahn vollflächig, inkl. Voranstrich",
            13219,
            1710,
            22604,
            10707,
        ),
        (
            "Terrassendämmung EPS 035 DAA dh druckfest, Dicke nach Aufbau",
            13219,
            1920,
            25380,
            15863,
        ),
        (
            "Abdichtung Polymerbitumen 2-lagig, Oberlage beschiefert",
            13219,
            3755,
            49637,
            25843,
        ),
        (
            "Abdichtungsanschluss an Attika, über Krone geführt, inkl. Keil",
            13000,
            2910,
            37830,
            6630,
        ),
        (
            "Terrassenbelag Betonplatten 40mm auf Splittbett, Schutzlage",
            13219,
            6000,
            79314,
            39657,
        ),
        (
            "Attikaabdeckung Titanzink 0,7mm, Zuschnitt bis 400mm, Halter",
            13000,
            4800,
            62400,
            35100,
        ),
        // Zulagen am Attikablech (zwei Ecken, zwei Endabschlüsse)
        DT_ZULAGEN[0],
        DT_ZULAGEN[1],
    ],
    ohne: &[],
};

const RH2: Soll = Soll {
    datei: include_str!("../referenz/rh2-mehrschalig.szo"),
    netto: 8291878,
    material: 3694694,
    geruest: GERUEST_2G,
    geschosse: &[
        ("Gründung", 2242615),
        ("Erdgeschoss", 3034024),
        ("Obergeschoss", 3015257),
    ],
    ausgleich: -18,
    zeilen: &[
        (
            "Bodenplatte Stb C25/30 XC2 d=18-25cm",
            17600,
            22600,
            397760,
            334400,
        ),
        (
            "Frostschürze Stb C25/30 b=30-45cm erdgeschalt",
            7024,
            25000,
            175600,
            133456,
        ),
        (
            "Stb-Decke Ortbeton C25/30 XC1 d=18-25cm",
            30385,
            22600,
            686701,
            577315,
        ),
        (
            "Deckenschalung glatt, kein Sichtbeton, Stützhöhe bis 3,0m",
            124605,
            3800,
            473499,
            99684,
        ),
        (
            "Randschalung Decke/Bodenplatte h bis 25cm",
            102960,
            2900,
            298584,
            51480,
        ),
        (
            "Betonstahl B500 liefern, schneiden, biegen, verlegen",
            4727,
            160000,
            756320,
            472700,
        ),
        (
            "AW Porenbeton-Planstein PP2-0,35 d=17,5cm Dünnbettmörtel",
            172751,
            5400,
            932855,
            466428,
        ),
        (
            "IW Porenbeton-Planstein PP2-0,35 d=17,5cm Dünnbettmörtel",
            18498,
            5400,
            99889,
            49945,
        ),
        (
            "IW Porenbeton-Planbauplatte d=11,5cm Dünnbettmörtel",
            18498,
            4299,
            79523,
            35128,
        ),
        (
            "Kerndämmung MW-Platte WLS 035 d=140mm 2-schal. Mauerwerk",
            194368,
            2500,
            485920,
            310989,
        ),
        (
            "Verblendschale Klinker NF d=11,5cm Läuferverband verfugt",
            205560,
            12700,
            2610612,
            1130580,
        ),
        (
            "Bauzaun Mobilzaun h=2,0m aufstellen, vorhalten, räumen",
            60000,
            2050,
            123000,
            0,
        ),
    ],
    ohne: &[],
};

const RH3: Soll = Soll {
    datei: include_str!("../referenz/rh3-versatz-dachterrasse.szo"),
    netto: 8090854,
    material: 3584238,
    geruest: GERUEST_2G,
    geschosse: &[
        ("Gründung", 2243845),
        ("Erdgeschoss", 3037935),
        ("Obergeschoss", 2809049),
    ],
    ausgleich: 25,
    zeilen: &[
        (
            "Bodenplatte Stb C25/30 XC2 d=18-25cm",
            17600,
            22600,
            397760,
            334400,
        ),
        (
            "Frostschürze Stb C25/30 b=30-45cm erdgeschalt",
            7024,
            25000,
            175600,
            133456,
        ),
        (
            "Stb-Decke Ortbeton C25/30 XC1 d=18-25cm",
            33969,
            22600,
            767699,
            645411,
        ),
        (
            "Deckenschalung glatt, kein Sichtbeton, Stützhöhe bis 3,0m",
            136298,
            3800,
            517932,
            109038,
        ),
        (
            "Randschalung Decke/Bodenplatte h bis 25cm",
            106680,
            2900,
            309372,
            53340,
        ),
        (
            "Betonstahl B500 liefern, schneiden, biegen, verlegen",
            5086,
            160000,
            813760,
            508600,
        ),
        (
            "AW Porenbeton-Planstein PP2-0,35 d=24cm Dünnbettmörtel",
            179602,
            6600,
            1185373,
            646567,
        ),
        (
            "IW Porenbeton-Planstein PP2-0,35 d=24cm Dünnbettmörtel",
            19183,
            6600,
            126608,
            69059,
        ),
        (
            "WDVS EPS 035 d=120mm, Kleber, Dübel, Armierung, Oberputz",
            207884,
            11420,
            2374035,
            1002001,
        ),
        // Stand 9, K2: Untersicht zwischen Lattung; die Datei ist älter als
        // die Bekleidung (W3), darum ohne Bekleidung, Lattung und Randprofil
        (
            "Untersichtdämmung EPS 035 d=120mm zwischen Lattung, über Kopf",
            2928,
            2400,
            7027,
            3514,
        ),
        (
            "Bauzaun Mobilzaun h=2,0m aufstellen, vorhalten, räumen",
            60600,
            2050,
            124230,
            0,
        ),
        (
            "Dampfsperre Bitumen-Alu-Schweißbahn vollflächig, inkl. Voranstrich",
            1757,
            1710,
            3004,
            1423,
        ),
        (
            "Terrassendämmung EPS 035 DAA dh druckfest, Dicke nach Aufbau",
            1757,
            1920,
            3373,
            2108,
        ),
        (
            "Abdichtung Polymerbitumen 2-lagig, Oberlage beschiefert",
            1757,
            3755,
            6598,
            3435,
        ),
        (
            "Abdichtungsanschluss an Attika, über Krone geführt, inkl. Keil",
            10600,
            2910,
            30846,
            5406,
        ),
        (
            "Terrassenbelag Betonplatten 40mm auf Splittbett, Schutzlage",
            1757,
            6000,
            10542,
            5271,
        ),
        (
            "Attikaabdeckung Titanzink 0,7mm, Zuschnitt bis 400mm, Halter",
            10600,
            4800,
            50880,
            28620,
        ),
        // Zulagen am Attikablech (zwei Ecken, zwei Endabschlüsse)
        DT_ZULAGEN[0],
        DT_ZULAGEN[1],
    ],
    ohne: &[],
};

/// RH-5 Flachdach (Kosten-Strang 10.10., Tagesplan K4): Prüfhaus 10 × 8 m,
/// AW-49 zweischalig, OG Nord 1,50 m zurück (Dachterrasse mit Fußpunkt aus
/// Schaumglas), OG Süd 0,30 m vor (Untersicht mit Bekleidung Faserzement und
/// Lattung), Flachdach mit
/// Aufkantung 50 cm, Dachaufbau Flachdach 21,5 und Attikablech. Bis zur
/// Oberkante Dach alles mit Bauleistung, keine graue Zeile.
const RH5: Soll = Soll {
    datei: include_str!("../referenz/rh5-flachdach.szo"),
    netto: 9765247,
    material: 4239190,
    geruest: (
        "Fassadengerüst LK3 W09, 4 Wochen Standzeit, auf-/abbauen",
        334544,
        900,
        301090,
        0,
    ),
    geschosse: &[
        ("Gründung", 2264725),
        ("Erdgeschoss", 3311769),
        ("Obergeschoss", 2879023),
        ("Flachdach", 1309724),
    ],
    ausgleich: 6,
    zeilen: &[
        (
            "Bauzaun Mobilzaun h=2,0m aufstellen, vorhalten, räumen",
            60600,
            2050,
            124230,
            0,
        ),
        (
            "Bodenplatte Stb C25/30 XC2 d=18-25cm",
            17600,
            22600,
            397760,
            334400,
        ),
        (
            "Frostschürze Stb C25/30 b=30-45cm erdgeschalt",
            7024,
            25000,
            175600,
            133456,
        ),
        (
            "Stb-Decke Ortbeton C25/30 XC1 d=18-25cm",
            28530,
            22600,
            644778,
            542070,
        ),
        (
            "Deckenschalung glatt, kein Sichtbeton, Stützhöhe bis 3,0m",
            118628,
            3800,
            450786,
            94902,
        ),
        (
            "Randschalung Decke/Bodenplatte h bis 25cm",
            101160,
            2900,
            293364,
            50580,
        ),
        (
            "Betonstahl B500 liefern, schneiden, biegen, verlegen",
            4542,
            160000,
            726720,
            454200,
        ),
        (
            "AW Porenbeton-Planstein PP2-0,35 d=17,5cm Dünnbettmörtel",
            181617,
            5400,
            980732,
            490366,
        ),
        (
            "Kerndämmung MW-Platte WLS 035 d=140mm 2-schal. Mauerwerk",
            205944,
            2500,
            514860,
            329510,
        ),
        (
            "Verblendschale Klinker NF d=11,5cm Läuferverband verfugt",
            217612,
            12700,
            2763672,
            1196866,
        ),
        (
            "Abfangung Verblendschale, Konsolanker Edelstahl",
            10485,
            5300,
            55571,
            36698,
        ),
        (
            "Fußpunkt Verblendschale Schaumglas-Dämmstein 115mm, 1. Lage",
            10000,
            5200,
            52000,
            0,
        ),
        (
            "Zulage Fußpunkt Schaumglas-Dämmstein je weitere Lage",
            10000,
            5200,
            52000,
            0,
        ),
        (
            "Mauersperrbahn unter Fußpunkt, B bis 25cm",
            10000,
            500,
            5000,
            0,
        ),
        (
            "Z-Folie über Fußpunkt, aus dem Schalenzwischenraum geführt",
            10000,
            1200,
            12000,
            0,
        ),
        (
            "Dampfsperre Bitumen-Alu-Schweißbahn vollflächig, inkl. Voranstrich",
            11103,
            1710,
            18986,
            8993,
        ),
        (
            "Terrassendämmung EPS 035 DAA dh druckfest, Dicke nach Aufbau",
            11103,
            1920,
            21318,
            13324,
        ),
        (
            "Abdichtung Polymerbitumen 2-lagig, Oberlage beschiefert",
            11103,
            3755,
            41692,
            21706,
        ),
        (
            "Abdichtungsanschluss an Attika, über Krone geführt, inkl. Keil",
            46600,
            2910,
            135606,
            23766,
        ),
        (
            "Terrassenbelag Betonplatten 40mm auf Splittbett, Schutzlage",
            11103,
            6000,
            66618,
            33309,
        ),
        (
            "Attikaabdeckung Titanzink 0,7mm, Zuschnitt bis 400mm, Halter",
            46600,
            4800,
            223680,
            125820,
        ),
        (
            "Zulage Attikaabdeckung Zuschnitt über 400 bis 500mm",
            13000,
            1200,
            15600,
            0,
        ),
        (
            "Zulage Attikaabdeckung Zuschnitt über 500 bis 667mm",
            33600,
            2600,
            87360,
            0,
        ),
        (
            "Zulage Attikaabdeckung Ecke 90°, gefalzt oder gelötet",
            6000,
            4800,
            28800,
            0,
        ),
        (
            "Zulage Attikaabdeckung Endabschluss mit Stirnblech",
            2000,
            2500,
            5000,
            0,
        ),
        (
            "Dampfsperre Bitumen-Alu-Schweißbahn Flachdach, inkl. Voranstrich",
            52496,
            1710,
            89768,
            42522,
        ),
        (
            "Dachdämmung EPS 035 DAA dh einlagig, Dicke nach Aufbau",
            52496,
            3600,
            188986,
            157488,
        ),
        (
            "Abdichtung Polymerbitumen 2-lagig Flachdach, Oberlage beschiefert",
            52496,
            3755,
            197122,
            102630,
        ),
        (
            "Attikadämmung innen und Krone, EPS d=6-10cm, unter Abdichtung",
            29680,
            2200,
            65296,
            0,
        ),
        (
            "Dachablauf DN 100 wärmegedämmt mit Aufstockelement, Einbau",
            1000,
            40000,
            40000,
            0,
        ),
        (
            "Notüberlauf Attika rechteckig, inkl. Durchbruch und Einbindung",
            1000,
            28000,
            28000,
            0,
        ),
        (
            "Grundlattung KVH 60/80 über Kopf, a=80cm, mit Schraubankern",
            22492,
            975,
            21930,
            0,
        ),
        (
            "Untersichtdämmung MW 035 d=120mm A1 zwischen Lattung, über Kopf",
            2811,
            2600,
            7309,
            3935,
        ),
        (
            "Traglattung 30/50 quer, a=40cm, Hinterlüftung",
            24777,
            440,
            10902,
            0,
        ),
        (
            "Untersichtbekleidung Faserzementtafel 8mm über Kopf, verschraubt",
            1829,
            10900,
            19936,
            10060,
        ),
        (
            "Lüftungsprofil Alu gelocht mit Insektenschutz, Untersichtrand",
            9770,
            1000,
            9770,
            0,
        ),
    ],
    ohne: &[],
};

fn blatt(m: &Model, u: &Umfang) -> Kostenblatt {
    let sched = qto::schedule(m);
    let k = lesen::katalog(m, None);
    lesen::kosten(m, &sched, &k, u)
}

fn pruefen(s: &Soll) {
    let m = laden(s.datei);
    // Stammdaten und Abdeckung ohne Befund (Regeln 71–92, 97, 99)
    let k = lesen::katalog(&m, None);
    assert!(
        lesen::befunde(&m, &k).is_empty(),
        "{:#?}",
        lesen::befunde(&m, &k)
    );
    let b = blatt(&m, &Umfang::projekt());
    let ist: Vec<(String, i64, i64, i64, i64)> = b
        .positionen
        .iter()
        .map(|p| {
            (
                p.kurz.clone(),
                p.menge.0 / 1000,
                p.ep.0,
                p.gp.0,
                p.stoff_gp.0,
            )
        })
        .collect();
    for z in s.zeilen.iter().chain(ERDE).chain(BE).chain([&s.geruest]) {
        assert!(
            ist.iter().any(|i| (i.0.as_str(), i.1, i.2, i.3, i.4) == *z),
            "fehlt {z:?}\nist {ist:#?}"
        );
    }
    assert_eq!(
        ist.len(),
        s.zeilen.len() + ERDE.len() + BE.len() + 1,
        "{ist:#?}"
    );
    assert_eq!(b.netto, Cent(s.netto));
    assert_eq!(b.nur_material, Cent(s.material));
    assert_eq!(b.mwst, Cent((s.netto * 19 + 50) / 100));
    assert_eq!(b.brutto, b.netto + b.mwst);
    // Anteile je für sich auf den Cent: höchstens ein Cent je Position und
    // Anteil Abstand zum Netto
    let anteile = b.lohn + b.stoff + b.geraet + b.sonst + b.nu;
    assert!((anteile - b.netto).0.abs() <= 4 * b.positionen.len() as i64);
    assert_eq!((b.unvollstaendig, b.geschaetzt), (0, 0));
    // je Geschoss mit einem Rundungsausgleich (Regel 96, ka-0-fach §1.7)
    let geschosse: Vec<(String, i64)> = b
        .nach_geschoss
        .iter()
        .map(|(id, c)| (m.storey(*id).unwrap().name.clone(), c.0))
        .collect();
    let soll: Vec<(String, i64)> = s
        .geschosse
        .iter()
        .map(|(n, c)| (n.to_string(), *c))
        .collect();
    assert_eq!(geschosse, soll);
    assert_eq!(b.ausgleich_geschoss, Cent(s.ausgleich));
    assert!(b.befunde.iter().all(|f| f.regel != 96), "{:#?}", b.befunde);
    // Kostengruppen und Gewerke gehen ohne Rest auf
    let kg: Cent = b.nach_kg.iter().map(|x| x.1).sum();
    assert_eq!(kg + b.ausgleich_kg, b.netto);
    let gw: Cent = b.nach_gewerk.iter().map(|x| x.1).sum();
    assert_eq!(gw, b.netto);
    // ohne Bauleistung, grau mit Menge
    let ohne: Vec<(&str, i64)> = b
        .ohne
        .iter()
        .map(|o| (o.nummer.as_str(), o.menge.0 / 1000))
        .collect();
    assert_eq!(ohne, s.ohne);
    // Umfang Gebäude: ein Haus, keine losen Geschosse, gleiches Blatt
    let (bid, _) = m.buildings().iter().next().expect("ein Gebäude");
    assert_eq!(blatt(&m, &Umfang::gebaeude(bid)), b);
    // mit Zwischenspeicher dasselbe Blatt
    let sched = qto::schedule(&m);
    let (b1, sp) = lesen::kosten_mit(
        Kostenspeicher::default(),
        &m,
        &sched,
        &k,
        &Umfang::projekt(),
    );
    assert_eq!(b1, b);
    let (b2, sp) = lesen::kosten_mit(sp, &m, &sched, &k, &Umfang::projekt());
    assert_eq!(b2, b);
    assert_eq!(sp.neu_zugeordnet(), 0);
}

#[test]
fn rh1_standardhaus() {
    pruefen(&RH1);
}

#[test]
fn rh2_mehrschalig() {
    pruefen(&RH2);
}

#[test]
fn rh3_versatz_dachterrasse() {
    pruefen(&RH3);
}

#[test]
fn rh5_flachdach() {
    pruefen(&RH5);
}

/// Die Sollbilder zeigen RH-1 ohne Deckenschalung und Randschalung, ohne
/// die Automatikpositionen (älter als Stand 8) und ohne Los 4 Dach (älter
/// als Stand 9): 52.395,55 € / 29.642,07 €.
#[test]
fn rh1_ohne_schalung() {
    let m = laden(RH1.datei);
    let b = blatt(&m, &Umfang::projekt());
    let ohne = |f: &dyn Fn(&sk_cost::Position) -> Cent| -> Cent {
        b.positionen
            .iter()
            .filter(|p| {
                !p.kurz.starts_with("Deckenschalung")
                    && !p.kurz.starts_with("Randschalung")
                    && p.ansatz.iter().all(|a| a.formel.is_none())
                    && !p.oz.starts_with("4.")
            })
            .map(f)
            .sum()
    };
    assert_eq!(ohne(&|p| p.gp), Cent(5_239_555));
    assert_eq!(ohne(&|p| p.stoff_gp), Cent(2_964_207));
    let _ = Dez::NULL;
}

/// Abnahme 28: `lesen::kosten` je Haus unter 1 ms (nur mit Optimierung
/// gemessen), Median aus fünf Läufen.
#[test]
fn zeit_unter_einer_millisekunde() {
    for s in [&RH1, &RH2, &RH3] {
        let m = laden(s.datei);
        let sched = qto::schedule(&m);
        let k = lesen::katalog(&m, None);
        let mut t: Vec<f64> = (0..5)
            .map(|_| {
                let a = std::time::Instant::now();
                std::hint::black_box(lesen::kosten(&m, &sched, &k, &Umfang::projekt()));
                a.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        t.sort_by(f64::total_cmp);
        eprintln!("kosten {:.3} ms", t[2]);
        if !cfg!(debug_assertions) {
            assert!(t[2] < 1.0, "{:.3} ms", t[2]);
        }
    }
}

/// Abnahme 29 (Teil): Wand verschieben, Typ tauschen, Preis im Projekt.
/// Nach jeder Änderung ist `kosten_mit` gleich `kosten`; nach einer
/// verschobenen Wand ordnet der Speicher nichts neu zu, nach einem Preis im
/// Projekt alles.
#[test]
fn kostenspeicher_gleich_kosten() {
    for s in [&RH1, &RH2] {
        let mut m = laden(s.datei);
        let u = Umfang::projekt();
        let sched = qto::schedule(&m);
        let k = lesen::katalog(&m, None);
        let (_, sp) = lesen::kosten_mit(Kostenspeicher::default(), &m, &sched, &k, &u);
        let typen = sp.neu_zugeordnet();
        assert!(typen > 0);
        // Wand verschieben: die erste Außenwand, die sich um 250 mm
        // versetzen lässt. Der erste Versatz kann ein neues Bauteil ergeben
        // (Untersicht unter dem Überstand); danach ordnet ein weiterer
        // Versatz nichts mehr neu zu
        let waende: Vec<_> = m
            .elements()
            .iter()
            .filter(|(_, e)| e.category == sk_model::element::Category::ExteriorWall)
            .map(|(id, _)| id)
            .collect();
        let mut sp = sp;
        let mut verschoben = 0;
        for w in waende {
            for d in [250.0, -100.0] {
                m.begin("Wand");
                if m.move_segment(w, d).is_none() {
                    m.rollback();
                    continue;
                }
                m.commit();
                let sched = qto::schedule(&m);
                let soll = lesen::kosten(&m, &sched, &k, &u);
                let (ist, neu) = lesen::kosten_mit(sp, &m, &sched, &k, &u);
                assert_eq!(ist, soll);
                if verschoben > 0 {
                    assert_eq!(neu.neu_zugeordnet(), 0, "Wand verschoben: kein Typ neu");
                }
                verschoben += 1;
                sp = neu;
            }
            if verschoben >= 2 {
                break;
            }
        }
        assert!(verschoben >= 2, "keine Wand verschiebbar");
        let sched = qto::schedule(&m);
        // Preis im Projekt: alles neu
        m.begin("Preis");
        sk_cost::ausfuehren(
            &mut m,
            None,
            sk_cost::Rolle::Admin,
            &sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "10:00"),
            sk_cost::Op::FirmenwertSetzen {
                schluessel: "wage".into(),
                wert: Dez::ganz(65),
            },
        )
        .unwrap();
        m.commit();
        let k = lesen::katalog(&m, None);
        let soll = lesen::kosten(&m, &sched, &k, &u);
        let (ist, sp) = lesen::kosten_mit(sp, &m, &sched, &k, &u);
        assert_eq!(ist, soll);
        assert_ne!(ist.netto, Cent(s.netto));
        assert!(sp.neu_zugeordnet() >= typen, "Preis im Projekt: alle neu");
        let (_, leer) = lesen::kosten_mit(Kostenspeicher::default(), &m, &sched, &k, &u);
        assert_eq!(sp.neu_zugeordnet(), leer.neu_zugeordnet());
    }
}

/// Haus 10 × 8 m aus der Werksmatrix des Prüfstands (BIM-Integration):
/// Außenwandtyp `aw`, je eine Innenwand bei x = 5 m, OG-Segmente gelöst und
/// versetzt (Segment, mm, + außen; 0 West, 1 Nord, 2 Ost, 3 Süd).
fn matrixhaus(aw: &str, iw: &[(usize, &str)], versatz: &[(usize, f64)]) -> Model {
    use sk_math::vec3;
    use sk_model::{element::Category, RefSide};
    let mut m = Model::new();
    m.begin("Gebäude");
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.build_from_polygon(b, &pts).expect("Gebäude");
    m.commit();
    let t = m.type_by_code(aw).expect("Werkstyp");
    m.begin("Typ");
    assert!(m.set_run_type(eg, t), "Typwechsel {aw}");
    m.commit();
    let og = m.runs_above(eg)[0];
    for &(seg, off) in versatz {
        let w = m.wall_at(og, seg).expect("Wand");
        m.begin("Versatz");
        assert!(m.set_linked(w, false));
        assert!(m.set_offset(w, off).is_some(), "Versatz {off}");
        m.commit();
    }
    for &(lvl, code) in iw {
        let run = if lvl == 0 { eg } else { og };
        let st = m.run(run).expect("Zug").storey;
        let t = m.type_by_code(code).expect("IW-Typ");
        m.begin("Innenwand");
        m.add_wall_run(
            &[vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)],
            false,
            RefSide::Center,
            st,
            t,
            Category::InteriorWall,
        )
        .expect("Innenwand");
        m.commit();
    }
    m
}

/// Menge der Abfangung V20 (Folge des Verblenders) in Tausendsteln m.
fn abfangung(b: &Kostenblatt) -> Option<i64> {
    b.positionen
        .iter()
        .find(|p| p.kurz.starts_with("Abfangung Verblendschale"))
        .map(|p| p.menge.0 / 1000)
}

/// Abnahme 23a (VK-01, Abfangung ohne Rückfall): RH-2 (AW-49 ohne Versatz)
/// hat keine Position V20; die Werksmatrix AW-49 mit Vor- und Rücksprung
/// hat V20 = 10,485 m (Nord +0,30: 9,885 + West und Ost je 0,300).
#[test]
fn abfangung_ohne_rueckfall() {
    let rh2 = laden(RH2.datei);
    assert_eq!(abfangung(&blatt(&rh2, &Umfang::projekt())), None);
    let m = matrixhaus("AW-49", &[(0, "IW-11,5")], &[(1, 300.0), (3, -300.0)]);
    let b = blatt(&m, &Umfang::projekt());
    assert_eq!(abfangung(&b), Some(10_485), "{:#?}", b.positionen);
    // Nur der Vorsprung braucht eine Abfangung, der Rücksprung nicht
    let m = matrixhaus("AW-49", &[], &[(3, -300.0)]);
    assert_eq!(abfangung(&blatt(&m, &Umfang::projekt())), None);
}

/// Abnahme 12 (Vorschau): Die Vorschau nennt das Netto vorher und nachher
/// selbst, gleich `kosten` vor und nach der Ausführung; Modell, `revision`,
/// `ext_revision` bleiben. Lohn 65 €/h auf RH-1: 60.089,83 → 62.501,62
/// (Einstellungen §3 KA-2 Punkt 8), mit Stand 9 75.953,63 → 78.889,33; eine
/// Zuordnung rechnet auf einer Kopie.
#[test]
fn vorschau_mit_summe_vorher_nachher() {
    use sk_cost::{Herkunft, HerkunftArt, Op, Rolle};
    let mut m = laden(RH1.datei);
    let sched = qto::schedule(&m);
    let u = Umfang::projekt();
    let lohn = Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(65),
    };
    let (rev, ext) = (m.revision(), m.ext_revision());
    let p = sk_cost::vorschau_kosten(
        &m,
        &sched,
        None,
        Rolle::Admin,
        std::slice::from_ref(&lohn),
        &u,
    )
    .expect("Vorschau");
    assert_eq!(p.netto, Some((Cent(7_595_363), Cent(7_888_933))));
    assert_eq!((m.revision(), m.ext_revision()), (rev, ext));
    // gleich der Ausführung
    let h = Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "10:40");
    m.begin("Lohn");
    sk_cost::ausfuehren(&mut m, None, Rolle::Admin, &h, lohn).expect("Lohn");
    m.commit();
    assert_eq!(blatt(&m, &u).netto, Cent(7_888_933));
    // Zuordnung: eine Außenwandschicht auf eine andere Bauleistung, nur in
    // der Kopie
    let k = lesen::katalog(&m, None);
    let (typ, schicht, alt) = m
        .layer_sets()
        .iter()
        .filter(|(id, _)| !m.type_users(*id).is_empty())
        .find_map(|(_, t)| {
            let z = lesen::zuordnung(&m, &k, t.guid, 0)?;
            Some((t.guid, 0usize, z.leistung?))
        })
        .expect("Schicht mit Bauleistung");
    let andere = k
        .leistungen
        .iter()
        .find(|l| l.guid != alt && !l.retired && !l.kategorien.is_empty())
        .expect("andere Bauleistung")
        .guid;
    let op = Op::BauleistungZuordnen {
        typ,
        schicht,
        bauleistung: Some(andere),
    };
    let rev = m.revision();
    let p = sk_cost::vorschau_kosten(
        &m,
        &sched,
        None,
        Rolle::Admin,
        std::slice::from_ref(&op),
        &u,
    )
    .expect("Vorschau Zuordnung");
    assert_eq!(m.revision(), rev);
    let (vorher, nachher) = p.netto.expect("Summen");
    assert_eq!(vorher, Cent(7_888_933));
    m.begin("Zuordnen");
    sk_cost::ausfuehren(&mut m, None, Rolle::Admin, &h, op).expect("Zuordnen");
    m.commit();
    assert_eq!(blatt(&m, &u).netto, nachher);
    assert_ne!(nachher, vorher);
}

/// Review 3ag: Typen und Baustoffe umbenennen ändert nur Befundsätze. Der
/// Speicher liefert danach keine Sätze mit alten Namen; `kosten_mit` bleibt
/// gleich `kosten`.
#[test]
fn kostenspeicher_nach_umbenennen() {
    for s in [&RH1, &RH3] {
        // eigene Dämmung der Dachterrasse: eine Zeile ohne Bauleistung mit
        // Befund (die eingebaute hat seit Stand 9 eine Werksleistung)
        let mut m = laden(
            &s.datei
                .replace("3NKChAqkL3uwFZN5n$FOr1", "3NKChAqkL3uwFZN5n$FOr9"),
        );
        let u = Umfang::projekt();
        let k = lesen::katalog(&m, None);
        let sched = qto::schedule(&m);
        let (vorher, sp) = lesen::kosten_mit(Kostenspeicher::default(), &m, &sched, &k, &u);
        assert!(vorher.befunde.iter().any(|b| b.regel == 81));
        m.begin("Namen");
        let mats: Vec<_> = m
            .materials()
            .iter()
            .map(|(id, x)| (id, x.clone()))
            .collect();
        for (id, mut x) in mats {
            x.name = format!("{} neu", x.name);
            assert!(m.set_material(id, x));
        }
        let typen: Vec<_> = m
            .layer_sets()
            .iter()
            .map(|(id, t)| (id, t.clone()))
            .collect();
        for (id, mut t) in typen {
            t.name = format!("{} neu", t.name);
            m.set_layer_set(id, t);
        }
        m.commit();
        let sched = qto::schedule(&m);
        let soll = lesen::kosten(&m, &sched, &k, &u);
        let (ist, _) = lesen::kosten_mit(sp, &m, &sched, &k, &u);
        assert_eq!(ist, soll);
    }
}

//! Die eine Feldtabelle der Kostenabschnitte (BIM §3.1–§3.8, §3.11, §3.13).
//! Lesen, Schreiben, Prüfen der Feldwerte (Regeln 72, 76, 79) und
//! [`crate::schema`] benutzen nur sie (K6).

use crate::geld::Dez;
use crate::zeile::{self, Zeile};
use sk_model::Guid;

/// Art eines Felds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// Guid in IFC-Form (22 Zeichen).
    Guid,
    /// Text in Anführungszeichen; `max` Zeichen, `zeile`: ohne Zeilenumbruch.
    Text { max: usize, zeile: bool },
    /// Text aus 1 bis `max` Ziffern (`[lot] nr`).
    Ziffern { max: usize },
    /// Ganzzahl von–bis.
    Ganz { min: i64, max: i64 },
    /// Zahl mit Punkt, höchstens `stellen` Nachkommastellen; `min`/`max` in
    /// [`Dez`]-Einheiten, `min_offen`: echt größer als `min`.
    Zahl {
        min: Dez,
        max: Dez,
        min_offen: bool,
        stellen: u32,
    },
    /// Ein Wort aus der Liste.
    Wort(&'static [&'static str]),
    /// Wörter aus der Liste, durch `,` getrennt.
    Woerter(&'static [&'static str]),
    /// Kennzeichen `=1`; 0 wird nicht geschrieben.
    Flag,
    /// Datum `JJJJ-MM-TT`.
    Tag,
    /// Preisstand `MM/JJJJ` (Regel 51).
    Monat,
    /// Zeitpunkt `JJJJ-MM-TTThh:mm`.
    Zeit,
    /// Prüfwert des Verwaltungskennworts
    /// `pbkdf2-sha256$runden$salz-hex$hash-hex` (KA-3b1, Review 3at).
    Pw,
    /// Kennung ohne Leerzeichen und Anführungszeichen (Guid oder Wort).
    Schluessel,
}

impl Art {
    /// Kurzbeschreibung für das Schema (K6).
    pub fn beschreibung(&self) -> String {
        match self {
            Art::Guid => "Guid".into(),
            Art::Text { max, zeile } => {
                let mut s = "Text".to_string();
                if *max < usize::MAX {
                    s += &format!(" ≤ {max}");
                }
                if *zeile {
                    s += ", eine Zeile";
                }
                s
            }
            Art::Ziffern { max } => format!("Text aus 1–{max} Ziffern"),
            Art::Ganz { min, max } => {
                if *max == i64::MAX {
                    format!("Ganzzahl ≥ {min}")
                } else {
                    format!("Ganzzahl {min}–{max}")
                }
            }
            Art::Zahl {
                min,
                max,
                min_offen,
                stellen,
            } => format!(
                "Zahl {}{} bis {}, ≤ {stellen} Nachkommastellen",
                if *min_offen { "> " } else { "≥ " },
                min.text(),
                max.text()
            ),
            Art::Wort(w) => format!("Wort: {}", w.join(" | ")),
            Art::Woerter(w) => format!("Wörter mit „,“: {}", w.join(" | ")),
            Art::Flag => "Flag =1".into(),
            Art::Tag => "Datum JJJJ-MM-TT".into(),
            Art::Monat => "Stand MM/JJJJ".into(),
            Art::Zeit => "Zeit JJJJ-MM-TTThh:mm".into(),
            Art::Pw => "pbkdf2-sha256$Runden$Salz$Prüfwert".into(),
            Art::Schluessel => "Kennung (Wort oder Guid)".into(),
        }
    }
}

/// Ein Feld eines Abschnitts.
#[derive(Clone, Copy, Debug)]
pub struct Feld {
    pub name: &'static str,
    pub art: Art,
    pub pflicht: bool,
    pub bedeutung: &'static str,
    /// Ein ungültiger Wert macht den Satz nicht ungültig: Er gilt nicht und
    /// bleibt roh stehen (Ausnahme zu Regel 72, etwa `conv`).
    pub weich: bool,
}

/// Womit eine Zeile gekennzeichnet ist (BIM §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kennung {
    Guid,
    Key,
}

/// Ein Kostenabschnitt mit seinen Feldern.
#[derive(Debug)]
pub struct Abschnitt {
    pub name: &'static str,
    pub kennung: Kennung,
    /// Steht im Projekt (`.szo`) bzw. im Firmenkatalog (`.szk`).
    pub szo: bool,
    pub szk: bool,
    pub bedeutung: &'static str,
    pub felder: &'static [Feld],
}

/// Abschnitte sind gleich, wenn sie gleich heißen (es gibt jeden einmal).
impl PartialEq for Abschnitt {
    fn eq(&self, o: &Abschnitt) -> bool {
        self.name == o.name
    }
}

impl Eq for Abschnitt {}

impl Abschnitt {
    pub fn feld(&self, name: &str) -> Option<&'static Feld> {
        self.felder.iter().find(|f| f.name == name)
    }
}

pub const EINHEITEN: &[&str] = &["m2", "m3", "m", "t", "kg", "st"];
pub const BEZUEGE: &[&str] = &["area", "volume", "length", "perimeter", "formwork", "steel"];
/// Bauteilarten einer Regel (`kinds.rs` `szo`), BIM §3.3.
pub const KATEGORIEN: &[&str] = &[
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
pub const FUNKTIONEN: &[&str] = &["loadbearing", "insulation", "finish", "membrane"];
pub const REC: &[&str] = &["article", "service", "svcpart", "svcfollow", "rate", "lot"];

/// `[log] rec`: dazu der Kopf, für `op=werk_uebernommen` (Bausteingrenze §6).
pub const LOG_REC: &[&str] = &[
    "article",
    "service",
    "svcpart",
    "svcfollow",
    "rate",
    "lot",
    "catalog",
];

const fn f(name: &'static str, art: Art, pflicht: bool, bedeutung: &'static str) -> Feld {
    Feld {
        name,
        art,
        pflicht,
        bedeutung,
        weich: false,
    }
}

/// Feld, dessen ungültiger Wert nur nicht gilt (siehe [`Feld::weich`]).
const fn weich(name: &'static str, art: Art, bedeutung: &'static str) -> Feld {
    Feld {
        name,
        art,
        pflicht: false,
        bedeutung,
        weich: true,
    }
}

const TEXT: Art = Art::Text {
    max: usize::MAX,
    zeile: false,
};
const NAME: Art = Art::Text {
    max: 70,
    zeile: true,
};
/// Kurztext einer Bauleistung: einzeilig, Überlänge ist kein Lesefehler
/// (Regel 79 „Überlänge“: Die Bauleistung rechnet und steht im LV, das
/// LV-Prüfen meldet die Zeichenzahl; die Verwaltung nimmt höchstens 70).
const KURZ: Art = Art::Text {
    max: usize::MAX,
    zeile: true,
};
const fn zahl(min: i64, max: i64, min_offen: bool, stellen: u32) -> Art {
    Art::Zahl {
        min: Dez::ganz(min),
        max: Dez::ganz(max),
        min_offen,
        stellen,
    }
}
/// Dicke in mm (Zehntel und Hundertstel erlaubt).
const MM: Art = zahl(0, 2000, false, 3);
const MM_POS: Art = zahl(0, 2000, true, 3);
const BETRAG: Art = zahl(0, 100_000, false, 4);

pub const CATALOG: Abschnitt = Abschnitt {
    name: "catalog",
    kennung: Kennung::Guid,
    szo: false,
    szk: true,
    bedeutung: "Firmenkatalog, genau eine Zeile",
    felder: &[
        f(
            "guid",
            Art::Guid,
            true,
            "Kennung des Firmenkatalogs über alle Stände",
        ),
        f(
            "name",
            Art::Text {
                max: 70,
                zeile: true,
            },
            true,
            "Firmenname, nicht leer",
        ),
        f(
            "stand",
            Art::Ganz {
                min: 0,
                max: i64::MAX,
            },
            true,
            "freigegebener Stand; 0 = nie freigegeben",
        ),
        f("date", Art::Tag, false, "Datum der Freigabe"),
        f(
            "status",
            Art::Wort(&["released", "draft"]),
            true,
            "freigegeben oder Entwurf",
        ),
        // Abweichend von Regel 72: eine andere Form gilt nicht, bleibt
        // bytegleich und sperrt die Verwaltung (BIM §3.1 `pw`)
        weich(
            "pw",
            Art::Pw,
            "Prüfwert des Verwaltungskennworts (PBKDF2-HMAC-SHA256)",
        ),
    ],
};

pub const ARTICLE: Abschnitt = Abschnitt {
    name: "article",
    kennung: Kennung::Guid,
    szo: true,
    szk: true,
    bedeutung: "Artikel (Baustoffpreis); ohne mat ein Hilfsstoff",
    felder: &[
        f("guid", Art::Guid, true, "Kennung"),
        f("name", NAME, true, "Bezeichnung"),
        f(
            "mat",
            Art::Guid,
            false,
            "Baustoff; leer = Hilfsstoff, nur in Stoffanteilen mit art=",
        ),
        f("t", MM_POS, false, "Dicke mm; leer = jede Dicke"),
        f("grade", TEXT, false, "Güte"),
        f("format", TEXT, false, "Format"),
        f("unit", Art::Wort(EINHEITEN), true, "Einheit"),
        f(
            "price",
            zahl(0, 1_000_000, false, 4),
            false,
            "netto €/Einheit; fehlt = Preis fehlt",
        ),
        f("date", Art::Monat, false, "Preisstand"),
        f("source", TEXT, false, "Quelle"),
        f("supplier", TEXT, false, "Lieferant"),
        weich(
            "conv",
            zahl(0, 10_000, true, 4),
            "Stück je Einheit (nicht bei st); ungültig: gilt nicht, bleibt stehen",
        ),
        f(
            "std",
            Art::Flag,
            false,
            "Standardartikel des Baustoffs für diese Dicke",
        ),
        f("retired", Art::Flag, false, "ausgemustert"),
    ],
};

pub const SERVICE: Abschnitt = Abschnitt {
    name: "service",
    kennung: Kennung::Guid,
    szo: true,
    szk: true,
    bedeutung: "Bauleistung mit Aufwandswert und Zuordnungsregel",
    felder: &[
        f("guid", Art::Guid, true, "Kennung"),
        f("short", KURZ, true, "Kurztext"),
        f("trade", Art::Guid, true, "Gewerk"),
        f("title", Art::Guid, true, "Titel im LV ([lot] mit parent)"),
        f(
            "pos",
            Art::Ganz { min: 1, max: 9999 },
            true,
            "Positionsnummer im Titel",
        ),
        f("unit", Art::Wort(EINHEITEN), true, "Einheit"),
        f("basis", Art::Wort(BEZUEGE), true, "Mengenbezug"),
        f("hours", BETRAG, false, "Zeitansatz h je Einheit"),
        f("equip", BETRAG, false, "Gerät € je Einheit"),
        f("other", BETRAG, false, "Sonstiges € je Einheit"),
        f("nu", BETRAG, false, "Pauschal-EP Nachunternehmer"),
        f(
            "kind",
            Art::Wort(&["normal", "need", "alt"]),
            false,
            "Positionsart",
        ),
        f(
            "kg",
            Art::Ganz { min: 100, max: 999 },
            false,
            "Kostengruppe DIN 276; leer = die der Schicht",
        ),
        f(
            "cats",
            Art::Woerter(KATEGORIEN),
            false,
            "Regel: Bauteilarten; leer = nur Folge- oder Wahlleistung",
        ),
        f("mat", Art::Guid, false, "Regel: Baustoff der Schicht"),
        f("tmin", MM, false, "Regel: Schichtdicke von (mm)"),
        f("tmax", MM, false, "Regel: Schichtdicke bis (mm)"),
        f("fn", Art::Wort(FUNKTIONEN), false, "Regel: Schichtfunktion"),
        f("retired", Art::Flag, false, "ausgemustert"),
    ],
};

pub const SVCPART: Abschnitt = Abschnitt {
    name: "svcpart",
    kennung: Kennung::Guid,
    szo: true,
    szk: true,
    bedeutung: "Stoffanteil einer Bauleistung: art= oder layer=1",
    felder: &[
        f("guid", Art::Guid, true, "Kennung"),
        f("service", Art::Guid, true, "Bauleistung"),
        f(
            "nr",
            Art::Ganz {
                min: 1,
                max: i64::MAX,
            },
            true,
            "Reihenfolge in der Bauleistung",
        ),
        f(
            "art",
            Art::Guid,
            false,
            "fester Artikel (eins von art und layer)",
        ),
        f(
            "layer",
            Art::Flag,
            false,
            "Artikel der Schicht nach Regel 82 (eins von art und layer)",
        ),
        f(
            "qty",
            zahl(0, 1_000_000, true, 6),
            true,
            "Artikelmenge je Einheit der Bauleistung",
        ),
    ],
};

pub const SVCFOLLOW: Abschnitt = Abschnitt {
    name: "svcfollow",
    kennung: Kennung::Guid,
    szo: true,
    szk: true,
    bedeutung: "Folgeposition einer Bauleistung",
    felder: &[
        f("guid", Art::Guid, true, "Kennung"),
        f("service", Art::Guid, true, "auslösende Bauleistung"),
        f(
            "nr",
            Art::Ganz {
                min: 1,
                max: i64::MAX,
            },
            true,
            "Reihenfolge in der Bauleistung",
        ),
        f("follow", Art::Guid, true, "Folge-Bauleistung"),
        f(
            "factor",
            zahl(0, 1_000_000, true, 6),
            false,
            "Faktor auf die Folgemenge (1)",
        ),
    ],
};

pub const RATE: Abschnitt = Abschnitt {
    name: "rate",
    kennung: Kennung::Key,
    szo: true,
    szk: true,
    bedeutung: "Firmenwert: wage, surcharge, vat, steel.<Bauteilart>",
    felder: &[
        f("key", Art::Schluessel, true, "Name des Firmenwerts"),
        f("num", zahl(-1_000_000, 1_000_000, false, 6), true, "Wert"),
    ],
};

pub const LOT: Abschnitt = Abschnitt {
    name: "lot",
    kennung: Kennung::Guid,
    szo: true,
    szk: true,
    bedeutung: "Los; mit parent= ein Titel dieses Loses",
    felder: &[
        f("guid", Art::Guid, true, "Kennung"),
        f("name", NAME, true, "Name"),
        f("nr", Art::Ziffern { max: 4 }, true, "OZ-Teil"),
        f("parent", Art::Guid, false, "Los des Titels; leer = Los"),
        f(
            "pre",
            Art::Text {
                max: 2000,
                zeile: false,
            },
            false,
            "Vorbemerkungen des Loses",
        ),
        f("retired", Art::Flag, false, "ausgemustert"),
    ],
};

pub const ORIGIN: Abschnitt = Abschnitt {
    name: "origin",
    kennung: Kennung::Key,
    szo: true,
    szk: true,
    bedeutung: "Herkunft eines Stammdatensatzes",
    felder: &[
        f(
            "key",
            Art::Schluessel,
            true,
            "Kennung des beschriebenen Satzes",
        ),
        f("rec", Art::Wort(REC), true, "Abschnitt des Satzes"),
        f(
            "kind",
            Art::Wort(&["factory", "manual", "import", "ai"]),
            true,
            "Werk, Hand, Import, KI",
        ),
        f(
            "status",
            Art::Wort(&["open", "confirmed"]),
            true,
            "unbestätigt oder bestätigt",
        ),
        f("date", Art::Tag, true, "Datum"),
        f("source", TEXT, false, "Quelle"),
        f("url", TEXT, false, "Fundstelle"),
        f("region", TEXT, false, "Preisregion"),
        f(
            "proj",
            Art::Flag,
            false,
            "nur .szo: im Projekt entstanden, Marke der Abweichung (Regel 89)",
        ),
        f(
            "conf",
            Art::Wort(&["sure", "mid", "rough"]),
            false,
            "Sicherheit des Werts",
        ),
    ],
};

pub const COSTPROJECT: Abschnitt = Abschnitt {
    name: "costproject",
    kennung: Kennung::Key,
    szo: true,
    szk: false,
    bedeutung: "Katalogstand der Projektkopie, höchstens eine Zeile",
    felder: &[
        f("key", Art::Wort(&["project"]), true, "Kennung"),
        f("catalog", Art::Guid, false, "Firmenkatalog der Kopie"),
        f(
            "stand",
            Art::Ganz {
                min: 0,
                max: i64::MAX,
            },
            false,
            "Stand der Kopie",
        ),
        f(
            "lvstorey",
            Art::Flag,
            false,
            "Geschosse als Untertitel im LV",
        ),
        f(
            "keep",
            Art::Ganz {
                min: 0,
                max: i64::MAX,
            },
            false,
            "Firmenstand, den der Nutzer so gelassen hat",
        ),
    ],
};

pub const LOG: Abschnitt = Abschnitt {
    name: "log",
    kennung: Kennung::Key,
    szo: false,
    szk: true,
    bedeutung: "Protokoll des Firmenkatalogs, nur anhängen",
    felder: &[
        f(
            "key",
            Art::Ganz {
                min: 1,
                max: i64::MAX,
            },
            true,
            "laufende Nummer",
        ),
        f(
            "stand",
            Art::Ganz {
                min: 0,
                max: i64::MAX,
            },
            true,
            "Stand der Freigabe",
        ),
        f("time", Art::Zeit, true, "Zeitpunkt"),
        f("role", Art::Wort(&["admin", "user", "ai"]), true, "Rolle"),
        f("op", Art::Schluessel, true, "Name der Operation"),
        f("rec", Art::Wort(LOG_REC), true, "Abschnitt des Satzes"),
        f("of", Art::Schluessel, true, "Kennung des Satzes"),
        f(
            "old",
            Art::Text {
                max: 120,
                zeile: false,
            },
            false,
            "Kurzform alt",
        ),
        f(
            "new",
            Art::Text {
                max: 120,
                zeile: false,
            },
            false,
            "Kurzform neu",
        ),
    ],
};

/// Alle Kostenabschnitte in der Schreibordnung (BIM §1).
pub const ABSCHNITTE: [&Abschnitt; 10] = [
    &CATALOG,
    &ARTICLE,
    &SERVICE,
    &SVCPART,
    &SVCFOLLOW,
    &RATE,
    &LOT,
    &ORIGIN,
    &COSTPROJECT,
    &LOG,
];

/// Abschnittslisten für `szo::read_with` und `catalog::read_szk_with`
/// (Bausteingrenze §4.1).
pub const ABSCHNITTE_SZO: [&str; 8] = [
    "article",
    "service",
    "svcpart",
    "svcfollow",
    "rate",
    "lot",
    "origin",
    "costproject",
];
pub const ABSCHNITTE_SZK: [&str; 9] = [
    "catalog",
    "article",
    "service",
    "svcpart",
    "svcfollow",
    "rate",
    "lot",
    "origin",
    "log",
];

pub fn abschnitt(name: &str) -> Option<&'static Abschnitt> {
    ABSCHNITTE.iter().copied().find(|a| a.name == name)
}

/// Gelesener Feldwert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wert {
    Guid(Guid),
    Text(String),
    Ganz(i64),
    Zahl(Dez),
    Wort(String),
    Woerter(Vec<String>),
    Flag(bool),
    /// Ungültiger Wert eines weichen Felds, unverändert.
    Roh(String),
}

impl Wert {
    /// Der Wert, wie er in der Zeile steht.
    pub fn schreiben(&self, art: Art) -> Option<String> {
        Some(match self {
            Wert::Guid(g) => g.to_ifc(),
            Wert::Text(t) => match art {
                Art::Tag | Art::Monat | Art::Zeit | Art::Pw | Art::Schluessel => t.clone(),
                _ => zeile::text(t),
            },
            Wert::Ganz(v) => v.to_string(),
            Wert::Zahl(d) => d.text(),
            Wert::Wort(w) => w.clone(),
            Wert::Woerter(w) => w.join(","),
            Wert::Flag(true) => "1".into(),
            Wert::Flag(false) => return None,
            Wert::Roh(t) if zeile::braucht_text(t) => zeile::text(t),
            Wert::Roh(t) => t.clone(),
        })
    }
}

/// Ein gelesener, gültiger Satz eines Kostenabschnitts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Satz {
    pub abschnitt: &'static Abschnitt,
    /// Eigene Felder in Tabellenordnung.
    pub werte: Vec<(&'static str, Wert)>,
    /// Fremde Schlüssel (neuere Fassung) in Dateireihenfolge.
    pub fremd: Vec<(String, String)>,
}

/// Warum eine Zeile nicht zählt (Regel 72).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ungueltig {
    pub feld: Option<&'static str>,
    pub grund: String,
}

fn ungueltig(feld: &'static str, grund: impl Into<String>) -> Ungueltig {
    Ungueltig {
        feld: Some(feld),
        grund: grund.into(),
    }
}

fn ziffern(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

fn tag_gueltig(s: &str) -> bool {
    let b: Vec<&str> = s.split('-').collect();
    b.len() == 3
        && ziffern(b[0], 4)
        && ziffern(b[1], 2)
        && ziffern(b[2], 2)
        && (1..=12).contains(&b[1].parse::<u32>().unwrap_or(0))
        && (1..=31).contains(&b[2].parse::<u32>().unwrap_or(0))
}

/// Liest einen Wert nach seiner Art.
pub fn wert_lesen(feld: &'static Feld, v: &str) -> Result<Wert, Ungueltig> {
    wert_lesen_in(None, feld, v)
}

/// Wie [`wert_lesen`]; der Grund nennt das Feld mit seinem Wort im
/// Abschnitt `abschnitt` (Sätze für Menschen, nie der Dateischlüssel).
fn wert_lesen_in(abschnitt: Option<&str>, feld: &'static Feld, v: &str) -> Result<Wert, Ungueltig> {
    let n = feld.name;
    let w = crate::wort::feld(abschnitt, n);
    let bad = |was: &str| ungueltig(n, format!("{w} ist ungültig ({v}; erwartet {was})"));
    Ok(match feld.art {
        Art::Guid => Wert::Guid(Guid::from_ifc(v).ok_or_else(|| bad("Guid"))?),
        Art::Text { max, zeile } => {
            if v.chars().count() > max {
                return Err(ungueltig(n, format!("{w} ist länger als {max} Zeichen")));
            }
            if zeile && v.contains('\n') {
                return Err(ungueltig(n, format!("{w} hat einen Zeilenumbruch")));
            }
            Wert::Text(v.to_string())
        }
        Art::Ziffern { max } => {
            if v.is_empty() || v.len() > max || !v.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad(&format!("1–{max} Ziffern")));
            }
            Wert::Text(v.to_string())
        }
        Art::Ganz { min, max } => {
            let x: i64 = v.parse().map_err(|_| bad("ganze Zahl"))?;
            if x < min || x > max {
                return Err(bad(&feld.art.beschreibung()));
            }
            Wert::Ganz(x)
        }
        Art::Zahl {
            min,
            max,
            min_offen,
            stellen,
        } => {
            let x = Dez::lesen(v, stellen).ok_or_else(|| bad("Zahl mit Punkt"))?;
            if x < min || (min_offen && x == min) || x > max {
                return Err(bad(&feld.art.beschreibung()));
            }
            Wert::Zahl(x)
        }
        Art::Wort(w) => {
            if !w.contains(&v) {
                return Err(bad("ein bekanntes Wort"));
            }
            Wert::Wort(v.to_string())
        }
        Art::Woerter(w) => {
            let l: Vec<String> = v.split(',').map(str::to_string).collect();
            if l.iter().any(|x| !w.contains(&x.as_str())) {
                return Err(bad("bekannte Wörter"));
            }
            Wert::Woerter(l)
        }
        Art::Flag => match v {
            "0" => Wert::Flag(false),
            "1" => Wert::Flag(true),
            _ => return Err(bad("0 oder 1")),
        },
        Art::Tag => {
            if !tag_gueltig(v) {
                return Err(bad("JJJJ-MM-TT"));
            }
            Wert::Text(v.to_string())
        }
        Art::Monat => {
            let ok = v.split_once('/').is_some_and(|(m, j)| {
                ziffern(m, 2) && ziffern(j, 4) && (1..=12).contains(&m.parse::<u32>().unwrap_or(0))
            });
            if !ok {
                return Err(bad("MM/JJJJ"));
            }
            Wert::Text(v.to_string())
        }
        Art::Zeit => {
            let ok = v.split_once('T').is_some_and(|(d, t)| {
                tag_gueltig(d)
                    && t.split_once(':').is_some_and(|(h, m)| {
                        ziffern(h, 2)
                            && ziffern(m, 2)
                            && h.parse::<u32>().unwrap_or(99) < 24
                            && m.parse::<u32>().unwrap_or(99) < 60
                    })
            });
            if !ok {
                return Err(bad("JJJJ-MM-TTThh:mm"));
            }
            Wert::Text(v.to_string())
        }
        Art::Pw => {
            if crate::verwaltung::Pruefwert::lesen(v).is_none() {
                return Err(bad("einen Prüfwert"));
            }
            Wert::Text(v.to_string())
        }
        Art::Schluessel => {
            if v.is_empty() || zeile::braucht_text(v) {
                return Err(bad("ein Wort ohne Leerzeichen"));
            }
            Wert::Text(v.to_string())
        }
    })
}

impl Satz {
    /// Liest eine Zeile des Abschnitts `a` (Regel 72: Pflichtfeld, Art,
    /// Bereich, Wort). Doppelte Schlüssel: der erste gilt, die weiteren
    /// bleiben als fremde erhalten.
    pub fn lesen(a: &'static Abschnitt, z: &Zeile) -> Result<Satz, Ungueltig> {
        let mut werte: Vec<(&'static str, Wert)> = Vec::new();
        let mut fremd = Vec::new();
        for (k, v) in &z.paare {
            match a.feld(k) {
                Some(f) if !werte.iter().any(|(n, _)| *n == f.name) => {
                    let w = match wert_lesen_in(Some(a.name), f, v) {
                        Err(_) if f.weich => Wert::Roh(v.clone()),
                        w => w?,
                    };
                    werte.push((f.name, w));
                }
                _ => fremd.push((k.clone(), v.clone())),
            }
        }
        for f in a.felder.iter().filter(|f| f.pflicht) {
            if !werte.iter().any(|(n, _)| *n == f.name) {
                let w = crate::wort::feld(Some(a.name), f.name);
                return Err(ungueltig(f.name, format!("{w} fehlt")));
            }
        }
        werte.sort_by_key(|(n, _)| a.felder.iter().position(|f| f.name == *n));
        Ok(Satz {
            abschnitt: a,
            werte,
            fremd,
        })
    }

    /// Leerer Satz für eine neue Zeile.
    pub fn neu(a: &'static Abschnitt) -> Satz {
        Satz {
            abschnitt: a,
            werte: Vec::new(),
            fremd: Vec::new(),
        }
    }

    pub fn wert(&self, name: &str) -> Option<&Wert> {
        self.werte.iter().find(|(n, _)| *n == name).map(|(_, w)| w)
    }

    /// Setzt oder entfernt ein eigenes Feld (`None`, leerer Text, Flag 0).
    pub fn setzen(&mut self, name: &str, w: Option<Wert>) {
        let Some(f) = self.abschnitt.feld(name) else {
            debug_assert!(
                false,
                "Feld {name} gibt es in [{}] nicht",
                self.abschnitt.name
            );
            return;
        };
        self.werte.retain(|(n, _)| *n != f.name);
        if let Some(w) = w.filter(|w| *w != Wert::Flag(false)) {
            self.werte.push((f.name, w));
            let a = self.abschnitt;
            self.werte
                .sort_by_key(|(n, _)| a.felder.iter().position(|f| f.name == *n));
        }
    }

    pub fn guid(&self, name: &str) -> Option<Guid> {
        match self.wert(name)? {
            Wert::Guid(g) => Some(*g),
            _ => None,
        }
    }

    pub fn text(&self, name: &str) -> Option<&str> {
        match self.wert(name)? {
            Wert::Text(t) | Wert::Wort(t) => Some(t),
            _ => None,
        }
    }

    pub fn ganz(&self, name: &str) -> Option<i64> {
        match self.wert(name)? {
            Wert::Ganz(v) => Some(*v),
            _ => None,
        }
    }

    pub fn zahl(&self, name: &str) -> Option<Dez> {
        match self.wert(name)? {
            Wert::Zahl(v) => Some(*v),
            _ => None,
        }
    }

    pub fn flag(&self, name: &str) -> bool {
        matches!(self.wert(name), Some(Wert::Flag(true)))
    }

    pub fn woerter(&self, name: &str) -> &[String] {
        match self.wert(name) {
            Some(Wert::Woerter(w)) => w,
            _ => &[],
        }
    }

    /// Kennung der Zeile (`guid=` oder `key=`) als Text.
    pub fn kennung(&self) -> Option<String> {
        match self.abschnitt.kennung {
            Kennung::Guid => self.guid("guid").map(|g| g.to_ifc()),
            Kennung::Key => self.wert("key").and_then(|w| match w {
                Wert::Ganz(v) => Some(v.to_string()),
                Wert::Text(t) | Wert::Wort(t) => Some(t.clone()),
                _ => None,
            }),
        }
    }

    /// Die Zeile: eigene Felder in Tabellenordnung, dahinter die fremden
    /// (Bausteingrenze §4.1).
    pub fn zeile(&self) -> String {
        let mut s = format!("[{}]", self.abschnitt.name);
        for f in self.abschnitt.felder {
            if let Some(v) = self.wert(f.name).and_then(|w| w.schreiben(f.art)) {
                s.push(' ');
                s.push_str(f.name);
                s.push('=');
                s.push_str(&v);
            }
        }
        for (k, v) in &self.fremd {
            s.push(' ');
            s.push_str(k);
            s.push('=');
            if zeile::braucht_text(v) {
                s.push_str(&zeile::text(v));
            } else {
                s.push_str(v);
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lies(text: &str) -> Result<Satz, Ungueltig> {
        let z = zeile::zerlegen(text).unwrap();
        Satz::lesen(abschnitt(&z.abschnitt).unwrap(), &z)
    }

    /// Jede Zeile der Werksdatei liest sich und schreibt sich bytegleich:
    /// Die Feldtabelle trifft die Schreibweise von Stammdaten.
    #[test]
    fn werkszeilen_bytegleich() {
        let mut n = 0;
        for l in crate::WERK.lines() {
            let Some(z) = zeile::zerlegen(l) else {
                continue;
            };
            let Some(a) = abschnitt(&z.abschnitt) else {
                continue;
            };
            let s = Satz::lesen(a, &z).unwrap_or_else(|e| panic!("{l}: {e:?}"));
            assert!(s.fremd.is_empty(), "{l}");
            assert_eq!(s.zeile(), l);
            n += 1;
        }
        assert!(n > 100, "{n}");
    }

    #[test]
    fn ungueltig_und_fremd() {
        let e = lies("[article] guid=1S7bUW0010080100000001 name=\"x\" unit=Banane").unwrap_err();
        assert_eq!(e.feld, Some("unit"));
        let e = lies("[article] guid=1S7bUW0010080100000001 unit=m2").unwrap_err();
        assert_eq!(e.grund, "Name fehlt", "{e:?}");
        let e = lies("[article] guid=1S7bUW0010080100000001 name=\"x\" unit=m2 price=1.23456")
            .unwrap_err();
        assert_eq!(e.feld, Some("price"));
        assert!(lies("[service] guid=1S7bUW0010080200000001 short=\"a\nb\" trade=1S7bUW0010080200000001 title=1S7bUW0010080200000001 pos=1 unit=m2 basis=area").is_err());
        let mut s = lies("[rate] zukunft=\"a b\" key=wage num=60 neu=1 key=doppelt").unwrap();
        assert_eq!(s.kennung().as_deref(), Some("wage"));
        assert_eq!(
            s.zeile(),
            "[rate] key=wage num=60 zukunft=\"a b\" neu=1 key=doppelt"
        );
        s.setzen("num", Some(Wert::Zahl(Dez::ganz(65))));
        assert_eq!(
            s.zeile(),
            "[rate] key=wage num=65 zukunft=\"a b\" neu=1 key=doppelt"
        );
        assert!(
            lies("[origin] key=x rec=article kind=manual status=open date=2026-13-01").is_err()
        );
        assert!(
            lies("[log] key=1 stand=1 time=2026-10-08T07:61 role=admin op=x rec=rate of=wage")
                .is_err()
        );
        assert!(
            lies("[log] key=1 stand=1 time=2026-10-08T07:59 role=admin op=x rec=rate of=wage")
                .is_ok()
        );
    }

    #[test]
    fn listen_passen_zur_tabelle() {
        for n in ABSCHNITTE_SZO {
            assert!(abschnitt(n).unwrap().szo, "{n}");
        }
        for n in ABSCHNITTE_SZK {
            assert!(abschnitt(n).unwrap().szk, "{n}");
        }
        let szo = ABSCHNITTE.iter().filter(|a| a.szo).count();
        let szk = ABSCHNITTE.iter().filter(|a| a.szk).count();
        assert_eq!((szo, szk), (ABSCHNITTE_SZO.len(), ABSCHNITTE_SZK.len()));
    }
}

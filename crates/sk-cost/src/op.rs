//! Operationen: die einzige Tür zum Schreiben (Bausteingrenze §5, Regel 93).
//!
//! Jede Operation wird zuerst an einer Arbeitskopie der Kostenzeilen geplant
//! und geprüft (Rolle, Feldwerte, dann die Regeln 72–92 am ganzen Bestand).
//! Erst ein fehlerfreier Plan ändert das Projekt: Zeile für Zeile über den
//! Erweiterungsspeicher im offenen Schritt, also mit genau einem
//! Rückgängig-Schritt. Für den Firmenkatalog rechnet [`firma_anwenden`] rein
//! von Text zu Text.

use crate::befund::{self, satz_ort, Befund, Schwere};
use crate::geld::{Cent, Dez};
use crate::katalog::{self, rate_bereich, Bezug, Einheit, Katalog, Quelle, Umfeld};
use crate::satz::{self, Abschnitt, Satz, Wert};
use crate::zeile;
use sk_model::{ExtStore, Guid, GuidGen, Library, Model};
use std::collections::HashSet;

/// Wer ändert (Regel 2). Bis KA-3 ist jeder Nutzer Administrator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rolle {
    Admin,
    Nutzer,
    Ki,
}

impl Rolle {
    /// Wort in `[log] role`.
    pub fn wort(self) -> &'static str {
        match self {
            Rolle::Admin => "admin",
            Rolle::Nutzer => "user",
            Rolle::Ki => "ai",
        }
    }
}

/// Art der Herkunft (`[origin] kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HerkunftArt {
    Factory,
    Manual,
    Import,
    Ai,
}

impl HerkunftArt {
    pub fn wort(self) -> &'static str {
        match self {
            HerkunftArt::Factory => "factory",
            HerkunftArt::Manual => "manual",
            HerkunftArt::Import => "import",
            HerkunftArt::Ai => "ai",
        }
    }
}

/// Sicherheit eines Werts (`[origin] conf`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sicherheit {
    Sure,
    Mid,
    Rough,
}

impl Sicherheit {
    pub fn wort(self) -> &'static str {
        match self {
            Sicherheit::Sure => "sure",
            Sicherheit::Mid => "mid",
            Sicherheit::Rough => "rough",
        }
    }
}

/// Herkunft einer Änderung, Pflichtparameter jeder Operation (Regel 88).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Herkunft {
    pub art: HerkunftArt,
    pub quelle: String,
    pub url: String,
    pub region: String,
    /// `JJJJ-MM-TT`
    pub datum: String,
    /// `hh:mm` (für `[log] time`)
    pub zeit: String,
    pub sicher: Option<Sicherheit>,
}

impl Herkunft {
    pub fn neu(art: HerkunftArt, datum: &str, zeit: &str) -> Herkunft {
        Herkunft {
            art,
            quelle: String::new(),
            url: String::new(),
            region: String::new(),
            datum: datum.into(),
            zeit: zeit.into(),
            sicher: None,
        }
    }

    /// Mit Datum und Uhrzeit von jetzt (UTC).
    pub fn jetzt(art: HerkunftArt) -> Herkunft {
        let s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()) as i64;
        let (j, m, t) = tag_aus_tagen(s.div_euclid(86_400));
        let min = s.rem_euclid(86_400) / 60;
        Herkunft::neu(
            art,
            &format!("{j:04}-{m:02}-{t:02}"),
            &format!("{:02}:{:02}", min / 60, min % 60),
        )
    }
}

/// Kalenderdatum aus Tagen seit 1970-01-01 (H. Hinnant, civil_from_days).
fn tag_aus_tagen(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Ein Stammdatensatz: Abschnitt und Kennung.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SatzId {
    pub abschnitt: &'static str,
    pub kennung: String,
}

impl SatzId {
    pub fn neu(abschnitt: &'static str, kennung: impl Into<String>) -> SatzId {
        SatzId {
            abschnitt,
            kennung: kennung.into(),
        }
    }
}

/// Felder einer Bauleistung zum Anlegen und Ändern.
#[derive(Clone, Debug, PartialEq)]
pub struct Bauleistung {
    pub kurz: String,
    pub gewerk: Guid,
    pub titel: Guid,
    pub pos: u16,
    pub einheit: Einheit,
    pub bezug: Bezug,
    pub stunden: Dez,
    pub geraet: Dez,
    pub sonst: Dez,
    pub nu: Option<Dez>,
    pub kg: Option<u16>,
    pub kategorien: Vec<String>,
    pub mat: Option<Guid>,
    pub tmin: Option<Dez>,
    pub tmax: Option<Dez>,
    pub funktion: Option<String>,
}

/// Inhalt eines Stoffanteils.
#[derive(Clone, Debug, PartialEq)]
pub enum Stoff {
    /// Fester Artikel mit Menge je Einheit der Bauleistung.
    Artikel { artikel: Guid, menge: Dez },
    /// Artikel der Schicht (Regel 82) mit Faktor.
    Schicht { faktor: Dez },
}

/// Eine benannte Änderung an Stammdaten (Bausteingrenze §5).
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    ArtikelAnlegen {
        /// `None`: Hilfsstoff (E2).
        baustoff: Option<Guid>,
        name: String,
        dicke: Option<Dez>,
        guete: String,
        format: String,
        einheit: Einheit,
        preis: Option<Dez>,
        stand: String,
        quelle: String,
        lieferant: String,
        standard: bool,
    },
    PreisSetzen {
        artikel: Guid,
        preis: Option<Dez>,
        stand: String,
        quelle: String,
    },
    BauleistungAnlegen(Bauleistung),
    BauleistungAendern {
        bauleistung: Guid,
        daten: Bauleistung,
    },
    StoffanteilSetzen {
        bauleistung: Guid,
        nr: u32,
        anteil: Option<Stoff>,
    },
    FolgeSetzen {
        bauleistung: Guid,
        nr: u32,
        folge: Option<(Guid, Dez)>,
    },
    /// Setzt `svc=` an Schicht `schicht` des Typs `typ` (nur Projekt).
    BauleistungZuordnen {
        typ: Guid,
        schicht: usize,
        bauleistung: Option<Guid>,
    },
    FirmenwertSetzen {
        schluessel: String,
        wert: Dez,
    },
    /// `los`: gesetzt = Titel dieses Loses.
    LosAnlegen {
        name: String,
        nr: String,
        los: Option<Guid>,
    },
    Ausmustern {
        satz: SatzId,
    },
    Wiederherstellen {
        satz: SatzId,
    },
    HerkunftBestaetigen {
        satz: SatzId,
    },
    /// Projektsatz wieder wie Firma bzw. Werk, Marke weg (KA-2).
    AbweichungZuruecknehmen {
        saetze: Vec<SatzId>,
    },
    /// Regel 92 „Übernehmen“: Sätze aus dem Firmenkatalog ins Projekt.
    StandUebernehmen {
        saetze: Vec<SatzId>,
    },
    /// `[costproject] keep`: Abgleichzeile aus bis zum nächsten Firmenstand.
    AbgleichLassen {
        stand: u32,
    },
    /// `[costproject] lvstorey` (KA-4).
    LvGliederungSetzen {
        untertitel: bool,
    },
}

/// Alle Operationen mit festem Namen (für Schema, `[log] op`, Abläufe):
/// Name, Angaben, nur in der Verwaltung.
pub const NAMEN: [(&str, &str, bool); 16] = [
    ("artikel_anlegen", "baustoff name dicke guete format einheit preis stand quelle lieferant standard", true),
    ("preis_setzen", "artikel preis stand quelle", false),
    ("bauleistung_anlegen", "kurz gewerk titel pos einheit bezug stunden geraet sonst nu kg kategorien mat tmin tmax funktion", true),
    ("bauleistung_aendern", "bauleistung + Felder wie bauleistung_anlegen", true),
    ("stoffanteil_setzen", "bauleistung nr anteil (artikel menge | schicht faktor | leer = entfernen)", true),
    ("folge_setzen", "bauleistung nr folge faktor (leer = entfernen)", true),
    ("bauleistung_zuordnen", "typ schicht bauleistung (leer = nach Regel)", false),
    ("firmenwert_setzen", "schluessel wert", false),
    ("los_anlegen", "name nr los (gesetzt = Titel)", true),
    ("ausmustern", "satz", true),
    ("wiederherstellen", "satz", true),
    ("herkunft_bestaetigen", "satz", false),
    ("abweichung_zuruecknehmen", "saetze", false),
    ("stand_uebernehmen", "saetze", false),
    ("abgleich_lassen", "stand", false),
    ("lv_gliederung_setzen", "untertitel", false),
];

/// „65,5“ statt „65.5“.
fn komma(d: Dez) -> String {
    d.text().replace('.', ",")
}

impl Op {
    /// Stelle in [`NAMEN`].
    fn nr(&self) -> usize {
        match self {
            Op::ArtikelAnlegen { .. } => 0,
            Op::PreisSetzen { .. } => 1,
            Op::BauleistungAnlegen(_) => 2,
            Op::BauleistungAendern { .. } => 3,
            Op::StoffanteilSetzen { .. } => 4,
            Op::FolgeSetzen { .. } => 5,
            Op::BauleistungZuordnen { .. } => 6,
            Op::FirmenwertSetzen { .. } => 7,
            Op::LosAnlegen { .. } => 8,
            Op::Ausmustern { .. } => 9,
            Op::Wiederherstellen { .. } => 10,
            Op::HerkunftBestaetigen { .. } => 11,
            Op::AbweichungZuruecknehmen { .. } => 12,
            Op::StandUebernehmen { .. } => 13,
            Op::AbgleichLassen { .. } => 14,
            Op::LvGliederungSetzen { .. } => 15,
        }
    }

    /// Fester Name für Verlauf, `[log] op` und Abläufe.
    pub fn name(&self) -> &'static str {
        NAMEN[self.nr()].0
    }

    /// Text des Rückgängig-Schritts.
    pub fn bezeichnung(&self) -> String {
        match self {
            Op::ArtikelAnlegen { name, .. } => format!("Artikel angelegt: {name}"),
            Op::PreisSetzen { preis: Some(p), .. } => {
                format!("Preis gesetzt: {} €", p.cent().deutsch())
            }
            Op::PreisSetzen { preis: None, .. } => "Preis entfernt".into(),
            Op::BauleistungAnlegen(d) => format!("Bauleistung angelegt: {}", d.kurz),
            Op::BauleistungAendern { daten, .. } => format!("Bauleistung geändert: {}", daten.kurz),
            Op::StoffanteilSetzen { nr, anteil, .. } => match anteil {
                Some(_) => format!("Stoffanteil {nr} gesetzt"),
                None => format!("Stoffanteil {nr} entfernt"),
            },
            Op::FolgeSetzen { nr, folge, .. } => match folge {
                Some(_) => format!("Folgeposition {nr} gesetzt"),
                None => format!("Folgeposition {nr} entfernt"),
            },
            Op::BauleistungZuordnen {
                bauleistung: Some(_),
                ..
            } => "Bauleistung gewählt".into(),
            Op::BauleistungZuordnen { .. } => "Bauleistung nach Regel".into(),
            Op::FirmenwertSetzen { schluessel, wert } => match schluessel.as_str() {
                "wage" => format!("Lohn {} €/h", wert.cent().deutsch()),
                "surcharge" => format!("Zuschlag Stoff {} %", komma(*wert)),
                "vat" => format!("MwSt. {} %", komma(*wert)),
                k => format!(
                    "Bewehrungsgrad {} {} kg/m³",
                    k.strip_prefix("steel.").unwrap_or(k),
                    komma(*wert)
                ),
            },
            Op::LosAnlegen {
                name, los: None, ..
            } => format!("Los angelegt: {name}"),
            Op::LosAnlegen { name, .. } => format!("Titel angelegt: {name}"),
            Op::Ausmustern { .. } => "Ausgemustert".into(),
            Op::Wiederherstellen { .. } => "Wiederhergestellt".into(),
            Op::HerkunftBestaetigen { .. } => "Herkunft bestätigt".into(),
            Op::AbweichungZuruecknehmen { saetze } => {
                format!("Abweichung zurückgenommen ({})", saetze.len())
            }
            Op::StandUebernehmen { saetze } => format!("Firmenstand übernommen ({})", saetze.len()),
            Op::AbgleichLassen { stand } => format!("Firmenstand {stand} so gelassen"),
            Op::LvGliederungSetzen { untertitel: true } => "Geschosse als Untertitel".into(),
            Op::LvGliederungSetzen { untertitel: false } => "Geschosse nicht als Untertitel".into(),
        }
    }

    /// Nur in der Verwaltung erlaubt (Regel 104: `kind=user` ruft sie nie).
    pub fn nur_admin(&self) -> bool {
        NAMEN[self.nr()].2
    }
}

/// Eine geplante Zeilenänderung.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aenderung {
    pub satz: SatzId,
    pub alt: Option<String>,
    pub neu: Option<String>,
    /// Kennung, vor die eine neue Zeile kommt (BIM §3: sortiert).
    pub vor: Option<String>,
    /// Index der auslösenden Operation (0, wenn Kopie).
    pub op: usize,
}

/// Abschnitte, die eine Projektkopie bekommt (BIM §3.2–§3.7, dazu Herkunft).
const KOPIE: [&str; 7] = [
    "article",
    "service",
    "svcpart",
    "svcfollow",
    "rate",
    "lot",
    "origin",
];

fn abgelehnt(op: &Op, grund: &str) -> Vec<Befund> {
    vec![Befund::fehler(
        93,
        befund::r93(op.name(), grund),
        befund::Ort::Datei,
    )]
}

/// Ordnung einer Zeile im Abschnitt (BIM §3): Guid bzw. (service, nr);
/// `None`: hinten anhängen.
fn ordnung(abschnitt: &str, line: &str) -> Option<(u128, i64)> {
    let z = zeile::zerlegen(line)?;
    let wert = |k: &str| {
        z.paare
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    };
    match abschnitt {
        "svcpart" | "svcfollow" => Some((
            Guid::from_ifc(wert("service")?)?.0,
            wert("nr")?.parse().ok()?,
        )),
        "article" | "service" | "lot" => Some((Guid::from_ifc(wert("guid")?)?.0, 0)),
        "origin" => Some((Guid::from_ifc(wert("key")?)?.0, 0)),
        _ => None,
    }
}

/// Arbeitskopie der Kostenzeilen mit dem Plan der Änderungen.
struct Arbeit<'a> {
    zeilen: ExtStore,
    umfeld: &'a Umfeld,
    quelle: Quelle,
    herkunft: &'a Herkunft,
    guids: GuidGen,
    aend: Vec<Aenderung>,
    op: usize,
    /// Firma oder Werk, gegen die Abweichungen gelten (Regeln 89, 92).
    bezug: Option<(Katalog, ExtStore)>,
    /// Stand des Firmenkatalogs für `StandUebernehmen`.
    firma: Option<(Guid, u32)>,
    /// Im Projekt (`.szo`): jede geschriebene Herkunft trägt `proj=1`
    /// (Regel 89, Nachtrag BIM 09:40).
    projekt: bool,
}

impl Arbeit<'_> {
    fn katalog(&self) -> Katalog {
        katalog::lesen(
            self.zeilen
                .recs()
                .iter()
                .map(|r| (r.section.as_str(), r.line.as_str())),
            self.umfeld,
            self.quelle.clone(),
        )
    }

    fn vor(&self, abschnitt: &str, id: &str, line: &str) -> Option<String> {
        if self
            .zeilen
            .section(abschnitt)
            .any(|r| r.id.as_deref() == Some(id))
        {
            return None;
        }
        let k = ordnung(abschnitt, line)?;
        self.zeilen
            .section(abschnitt)
            .find(|r| ordnung(abschnitt, &r.line).is_some_and(|o| o > k))
            .and_then(|r| r.id.clone())
    }

    fn put(&mut self, abschnitt: &'static str, id: &str, line: String) {
        let vor = self.vor(abschnitt, id, &line);
        let (_, alt) = self.zeilen.put(abschnitt, id, line.clone(), vor.as_deref());
        if alt.as_deref() != Some(line.as_str()) {
            self.aend.push(Aenderung {
                satz: SatzId::neu(abschnitt, id),
                alt,
                neu: Some(line),
                vor,
                op: self.op,
            });
        }
    }

    fn remove(&mut self, abschnitt: &'static str, id: &str) {
        if let Some((_, alt)) = self.zeilen.remove(abschnitt, id) {
            self.aend.push(Aenderung {
                satz: SatzId::neu(abschnitt, id),
                alt: Some(alt),
                neu: None,
                vor: None,
                op: self.op,
            });
        }
    }

    /// `[origin]` zum geänderten Satz (Regel 88): `manual` bestätigt,
    /// `import` und `ai` offen, Werk ohne Zeile.
    fn herkunft_setzen(&mut self, rec: &'static str, id: &str) {
        let h = self.herkunft;
        if h.art == HerkunftArt::Factory {
            return;
        }
        let mut s = Satz::neu(&satz::ORIGIN);
        s.setzen("key", Some(Wert::Text(id.into())));
        s.setzen("rec", Some(Wert::Wort(rec.into())));
        s.setzen("kind", Some(Wert::Wort(h.art.wort().into())));
        let status = if h.art == HerkunftArt::Manual {
            "confirmed"
        } else {
            "open"
        };
        s.setzen("status", Some(Wert::Wort(status.into())));
        s.setzen("date", Some(Wert::Text(h.datum.clone())));
        for (f, v) in [
            ("source", &h.quelle),
            ("url", &h.url),
            ("region", &h.region),
        ] {
            if !v.is_empty() {
                s.setzen(f, Some(Wert::Text(v.clone())));
            }
        }
        if let Some(c) = h.sicher {
            s.setzen("conf", Some(Wert::Wort(c.wort().into())));
        }
        if self.projekt {
            s.setzen("proj", Some(Wert::Flag(true)));
        }
        let alt = self
            .zeilen
            .section("origin")
            .find(|r| r.id.as_deref() == Some(id))
            .and_then(|r| zeile::zerlegen(&r.line))
            .and_then(|z| Satz::lesen(&satz::ORIGIN, &z).ok());
        if let Some(alt) = alt {
            s.fremd = alt.fremd;
        }
        self.put("origin", id, s.zeile());
    }

    /// Prüft die Feldwerte eines neuen Satzes an der Feldtabelle (Regeln 72,
    /// 76, 79) und schreibt ihn.
    fn satz_schreiben(&mut self, s: &Satz, op: &Op) -> Result<String, Vec<Befund>> {
        let line = s.zeile();
        let z = zeile::zerlegen(&line).ok_or_else(|| abgelehnt(op, "Zeile nicht lesbar"))?;
        if let Err(e) = Satz::lesen(s.abschnitt, &z) {
            let feld = e.feld.unwrap_or("");
            let wert = z
                .paare
                .iter()
                .find(|(k, _)| k == feld)
                .map_or(e.grund.clone(), |(_, v)| v.clone());
            let id = s.kennung().unwrap_or_default();
            let ort = satz_ort(s.abschnitt.name, id);
            let b = match s.abschnitt.name {
                "article" => Befund::fehler(
                    76,
                    befund::r76(s.text("name").unwrap_or(""), feld, &wert),
                    ort,
                ),
                "service" => Befund::fehler(
                    79,
                    befund::r79(s.text("short").unwrap_or(""), feld, &wert),
                    ort,
                ),
                a => Befund::fehler(72, befund::r72(0, a, &e.grund), ort),
            };
            return Err(vec![b]);
        }
        let id = s.kennung().ok_or_else(|| abgelehnt(op, "Kennung fehlt"))?;
        self.put(s.abschnitt.name, &id, line);
        Ok(id)
    }

    fn neue_guid(&mut self) -> Guid {
        self.guids.next_guid()
    }

    /// `[costproject]`-Satz der Kopie (oder ein neuer).
    fn kopie_satz(&self) -> Satz {
        self.zeilen
            .section("costproject")
            .next()
            .and_then(|r| zeile::zerlegen(&r.line))
            .and_then(|z| Satz::lesen(&satz::COSTPROJECT, &z).ok())
            .unwrap_or_else(|| {
                let mut s = Satz::neu(&satz::COSTPROJECT);
                s.setzen("key", Some(Wert::Wort("project".into())));
                s
            })
    }

    fn anwenden(&mut self, op: &Op, ziel: Ziel) -> Result<(), Vec<Befund>> {
        let k = self.katalog();
        let opt_text = |v: &str| (!v.is_empty()).then(|| Wert::Text(v.to_string()));
        match op {
            Op::ArtikelAnlegen {
                baustoff,
                name,
                dicke,
                guete,
                format,
                einheit,
                preis,
                stand,
                quelle,
                lieferant,
                standard,
            } => {
                let g = self.neue_guid();
                let mut s = Satz::neu(&satz::ARTICLE);
                s.setzen("guid", Some(Wert::Guid(g)));
                s.setzen("name", Some(Wert::Text(name.clone())));
                s.setzen("mat", baustoff.map(Wert::Guid));
                s.setzen("t", dicke.map(Wert::Zahl));
                s.setzen("grade", opt_text(guete));
                s.setzen("format", opt_text(format));
                s.setzen("unit", Some(Wert::Wort(einheit.wort().into())));
                s.setzen("price", preis.map(Wert::Zahl));
                s.setzen("date", opt_text(stand));
                s.setzen("source", opt_text(quelle));
                s.setzen("supplier", opt_text(lieferant));
                s.setzen("std", Some(Wert::Flag(*standard)));
                let id = self.satz_schreiben(&s, op)?;
                self.herkunft_setzen("article", &id);
            }
            Op::PreisSetzen {
                artikel,
                preis,
                stand,
                quelle,
            } => {
                let a = k
                    .artikel(*artikel)
                    .ok_or_else(|| abgelehnt(op, "den Artikel gibt es nicht"))?;
                let mut s = a.satz.clone();
                s.setzen("price", preis.map(Wert::Zahl));
                if !stand.is_empty() {
                    s.setzen("date", Some(Wert::Text(stand.clone())));
                }
                if !quelle.is_empty() {
                    s.setzen("source", Some(Wert::Text(quelle.clone())));
                }
                let id = self.satz_schreiben(&s, op)?;
                self.herkunft_setzen("article", &id);
            }
            Op::BauleistungAnlegen(d) => {
                let g = self.neue_guid();
                let mut s = Satz::neu(&satz::SERVICE);
                s.setzen("guid", Some(Wert::Guid(g)));
                leistung_setzen(&mut s, d);
                let id = self.satz_schreiben(&s, op)?;
                self.herkunft_setzen("service", &id);
            }
            Op::BauleistungAendern { bauleistung, daten } => {
                let l = k
                    .leistung(*bauleistung)
                    .ok_or_else(|| abgelehnt(op, "die Bauleistung gibt es nicht"))?;
                let mut s = l.satz.clone();
                leistung_setzen(&mut s, daten);
                let id = self.satz_schreiben(&s, op)?;
                self.herkunft_setzen("service", &id);
            }
            Op::StoffanteilSetzen {
                bauleistung,
                nr,
                anteil,
            } => {
                if k.leistung(*bauleistung).is_none() {
                    return Err(abgelehnt(op, "die Bauleistung gibt es nicht"));
                }
                let alt = k
                    .anteile
                    .iter()
                    .find(|a| a.leistung == *bauleistung && a.nr == *nr);
                match anteil {
                    None => {
                        let a =
                            alt.ok_or_else(|| abgelehnt(op, "den Stoffanteil gibt es nicht"))?;
                        let id = a.guid.to_ifc();
                        self.remove("svcpart", &id);
                        self.remove("origin", &id);
                    }
                    Some(st) => {
                        let mut s = match alt {
                            Some(a) => a.satz.clone(),
                            None => {
                                let mut s = Satz::neu(&satz::SVCPART);
                                s.setzen("guid", Some(Wert::Guid(self.neue_guid())));
                                s
                            }
                        };
                        s.setzen("service", Some(Wert::Guid(*bauleistung)));
                        s.setzen("nr", Some(Wert::Ganz(i64::from(*nr))));
                        match st {
                            Stoff::Artikel { artikel, menge } => {
                                s.setzen("art", Some(Wert::Guid(*artikel)));
                                s.setzen("layer", None);
                                s.setzen("qty", Some(Wert::Zahl(*menge)));
                            }
                            Stoff::Schicht { faktor } => {
                                s.setzen("art", None);
                                s.setzen("layer", Some(Wert::Flag(true)));
                                s.setzen("qty", Some(Wert::Zahl(*faktor)));
                            }
                        }
                        let id = self.satz_schreiben(&s, op)?;
                        self.herkunft_setzen("svcpart", &id);
                    }
                }
            }
            Op::FolgeSetzen {
                bauleistung,
                nr,
                folge,
            } => {
                if k.leistung(*bauleistung).is_none() {
                    return Err(abgelehnt(op, "die Bauleistung gibt es nicht"));
                }
                let alt = k
                    .folgen
                    .iter()
                    .find(|f| f.leistung == *bauleistung && f.nr == *nr);
                match folge {
                    None => {
                        let f =
                            alt.ok_or_else(|| abgelehnt(op, "die Folgeposition gibt es nicht"))?;
                        let id = f.guid.to_ifc();
                        self.remove("svcfollow", &id);
                        self.remove("origin", &id);
                    }
                    Some((fg, faktor)) => {
                        let mut s = match alt {
                            Some(f) => f.satz.clone(),
                            None => {
                                let mut s = Satz::neu(&satz::SVCFOLLOW);
                                s.setzen("guid", Some(Wert::Guid(self.neue_guid())));
                                s
                            }
                        };
                        s.setzen("service", Some(Wert::Guid(*bauleistung)));
                        s.setzen("nr", Some(Wert::Ganz(i64::from(*nr))));
                        s.setzen("follow", Some(Wert::Guid(*fg)));
                        s.setzen(
                            "factor",
                            (*faktor != Dez::EINS).then_some(Wert::Zahl(*faktor)),
                        );
                        let id = self.satz_schreiben(&s, op)?;
                        self.herkunft_setzen("svcfollow", &id);
                    }
                }
            }
            Op::BauleistungZuordnen { bauleistung, .. } => {
                // Die Schicht ändert der Aufrufer im Modell; hier nur die
                // Prüfung des Ziels (Regel 99)
                if ziel != Ziel::Projekt {
                    return Err(abgelehnt(op, "nur im Projekt"));
                }
                if let Some(g) = bauleistung {
                    match k.leistung(*g) {
                        None => return Err(abgelehnt(op, "die Bauleistung gibt es nicht")),
                        Some(l) if l.retired => {
                            return Err(abgelehnt(op, "die Bauleistung ist ausgemustert"))
                        }
                        Some(_) => {}
                    }
                }
            }
            Op::FirmenwertSetzen { schluessel, wert } => {
                let Some((lo, hi)) = rate_bereich(schluessel) else {
                    return Err(abgelehnt(
                        op,
                        &format!("unbekannter Firmenwert {schluessel}"),
                    ));
                };
                if *wert < Dez::ganz(lo) || *wert > Dez::ganz(hi) {
                    let g = format!("{} liegt nicht in {lo}–{hi}", komma(*wert));
                    return Err(abgelehnt(op, &g));
                }
                let alt = self
                    .zeilen
                    .section("rate")
                    .find(|r| r.id.as_deref() == Some(schluessel))
                    .and_then(|r| zeile::zerlegen(&r.line))
                    .and_then(|z| Satz::lesen(&satz::RATE, &z).ok());
                let mut s = alt.unwrap_or_else(|| {
                    let mut s = Satz::neu(&satz::RATE);
                    s.setzen("key", Some(Wert::Text(schluessel.clone())));
                    s
                });
                s.setzen("num", Some(Wert::Zahl(*wert)));
                let id = self.satz_schreiben(&s, op)?;
                self.herkunft_setzen("rate", &id);
            }
            Op::LosAnlegen { name, nr, los } => {
                let mut s = Satz::neu(&satz::LOT);
                s.setzen("guid", Some(Wert::Guid(self.neue_guid())));
                s.setzen("name", Some(Wert::Text(name.clone())));
                s.setzen("nr", Some(Wert::Text(nr.clone())));
                s.setzen("parent", los.map(Wert::Guid));
                let id = self.satz_schreiben(&s, op)?;
                self.herkunft_setzen("lot", &id);
            }
            Op::Ausmustern { satz: id } | Op::Wiederherstellen { satz: id } => {
                let aus = matches!(op, Op::Ausmustern { .. });
                let g = Guid::from_ifc(&id.kennung);
                let s = match id.abschnitt {
                    "article" => g.and_then(|g| k.artikel(g)).map(|a| a.satz.clone()),
                    "service" => g.and_then(|g| k.leistung(g)).map(|l| l.satz.clone()),
                    "lot" => g.and_then(|g| k.los(g)).map(|l| l.satz.clone()),
                    _ => return Err(abgelehnt(op, "nur Artikel, Bauleistung oder Los")),
                };
                let mut s = s.ok_or_else(|| abgelehnt(op, "den Satz gibt es nicht"))?;
                s.setzen("retired", Some(Wert::Flag(aus)));
                let rec = s.abschnitt.name;
                let id = self.satz_schreiben(&s, op)?;
                self.herkunft_setzen(rec, &id);
            }
            Op::HerkunftBestaetigen { satz: id } => {
                let u = k
                    .herkunft_von(id.abschnitt, &id.kennung)
                    .ok_or_else(|| abgelehnt(op, "keine Herkunftsangabe"))?;
                let mut s = u.satz.clone();
                s.setzen("status", Some(Wert::Wort("confirmed".into())));
                self.satz_schreiben(&s, op)?;
            }
            Op::AbweichungZuruecknehmen { saetze } | Op::StandUebernehmen { saetze } => {
                let uebernehmen = matches!(op, Op::StandUebernehmen { .. });
                if uebernehmen && self.firma.is_none() {
                    return Err(abgelehnt(op, "kein Firmenkatalog"));
                }
                let Some((_, bezug)) = &self.bezug else {
                    return Err(abgelehnt(op, "keine Vorlage"));
                };
                let bezug = bezug.clone();
                for id in saetze {
                    let neu = bezug
                        .section(id.abschnitt)
                        .find(|r| r.id.as_deref() == Some(id.kennung.as_str()))
                        .map(|r| r.line.clone());
                    let sec = satz::abschnitt(id.abschnitt)
                        .map(|a| a.name)
                        .ok_or_else(|| abgelehnt(op, "unbekannter Abschnitt"))?;
                    match neu {
                        Some(l) => self.put(sec, &id.kennung, l),
                        None => self.remove(sec, &id.kennung),
                    }
                    // Marke weg; die Herkunft der Vorlage kommt mit, ohne
                    // `proj` (Regel 89)
                    match bezug
                        .section("origin")
                        .find(|r| r.id.as_deref() == Some(id.kennung.as_str()))
                        .filter(|r| !projektherkunft(&r.line))
                    {
                        Some(r) => self.put("origin", &id.kennung, r.line.clone()),
                        None => self.remove("origin", &id.kennung),
                    }
                }
                if uebernehmen && self.gleich_firma() {
                    let (g, stand) = self.firma.unwrap();
                    let mut s = self.kopie_satz();
                    s.setzen("catalog", Some(Wert::Guid(g)));
                    s.setzen("stand", Some(Wert::Ganz(i64::from(stand))));
                    if s.ganz("keep").is_some_and(|v| v < i64::from(stand)) {
                        s.setzen("keep", None);
                    }
                    self.satz_schreiben(&s, op)?;
                }
            }
            Op::AbgleichLassen { stand } => {
                let mut s = self.kopie_satz();
                s.setzen("keep", Some(Wert::Ganz(i64::from(*stand))));
                self.satz_schreiben(&s, op)?;
            }
            Op::LvGliederungSetzen { untertitel } => {
                let mut s = self.kopie_satz();
                s.setzen("lvstorey", Some(Wert::Flag(*untertitel)));
                self.satz_schreiben(&s, op)?;
            }
        }
        Ok(())
    }

    /// Stimmen alle Stammsätze ohne Projektmarke mit der Firma überein?
    fn gleich_firma(&self) -> bool {
        let Some((_, bezug)) = &self.bezug else {
            return false;
        };
        let markiert: HashSet<&str> = self
            .zeilen
            .section("origin")
            .filter(|r| projektherkunft(&r.line))
            .filter_map(|r| r.id.as_deref())
            .collect();
        let zeilen = |s: &ExtStore| -> HashSet<(String, String)> {
            ["article", "service", "svcpart", "svcfollow", "rate", "lot"]
                .iter()
                .flat_map(|a| s.section(a))
                .filter(|r| !r.id.as_deref().is_some_and(|i| markiert.contains(i)))
                .map(|r| (r.section.clone(), r.line.clone()))
                .collect()
        };
        zeilen(&self.zeilen) == zeilen(bezug)
    }
}

fn leistung_setzen(s: &mut Satz, d: &Bauleistung) {
    let zahl = |v: Dez| (v != Dez::NULL).then_some(Wert::Zahl(v));
    s.setzen("short", Some(Wert::Text(d.kurz.clone())));
    s.setzen("trade", Some(Wert::Guid(d.gewerk)));
    s.setzen("title", Some(Wert::Guid(d.titel)));
    s.setzen("pos", Some(Wert::Ganz(i64::from(d.pos))));
    s.setzen("unit", Some(Wert::Wort(d.einheit.wort().into())));
    s.setzen("basis", Some(Wert::Wort(d.bezug.wort().into())));
    s.setzen("hours", zahl(d.stunden));
    s.setzen("equip", zahl(d.geraet));
    s.setzen("other", zahl(d.sonst));
    s.setzen("nu", d.nu.map(Wert::Zahl));
    s.setzen("kg", d.kg.map(|v| Wert::Ganz(i64::from(v))));
    s.setzen(
        "cats",
        (!d.kategorien.is_empty()).then(|| Wert::Woerter(d.kategorien.clone())),
    );
    s.setzen("mat", d.mat.map(Wert::Guid));
    s.setzen("tmin", d.tmin.map(Wert::Zahl));
    s.setzen("tmax", d.tmax.map(Wert::Zahl));
    s.setzen("fn", d.funktion.clone().map(Wert::Wort));
}

/// Wohin eine Operation schreibt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ziel {
    Projekt,
    Firma,
    /// Ab KA-3b: eigene `.szk` mit `status=draft`.
    FirmaEntwurf,
}

/// Fehler, die es nach dem Plan gibt und vorher nicht gab.
fn neue_fehler(vorher: &[Befund], nachher: &[Befund]) -> Vec<Befund> {
    nachher
        .iter()
        .filter(|b| b.schwere == Schwere::Fehler && !vorher.contains(b))
        .cloned()
        .collect()
}

/// Kostenzeilen der geltenden Quelle als Kopie fürs Projekt (Regel 92):
/// die gültigen, nicht ausgemusterten Stammsätze und ihre Herkunft, dazu
/// `[costproject] catalog/stand`. Werk, wenn keine Firma gilt.
fn kopie(m: &Model, firma: Option<&Library>) -> ExtStore {
    let k = crate::lesen::firma_oder_werk(m, firma);
    let (zeilen, kopf) = quelle_zeilen(firma, &k.quelle);
    // nicht Ausgemustertes, dazu Ausgemustertes, das davon noch benutzt
    // wird (Regel 87: Hinweis, gerechnet wird weiter; ohne den Satz stünde
    // im Projekt ein Verweis ins Leere, Regel 73)
    let mut benutzt: Vec<&crate::katalog::Leistung> =
        k.leistungen.iter().filter(|l| !l.retired).collect();
    for f in benutzt.clone().iter().flat_map(|l| k.folgen_von(l.guid)) {
        if let Some(x) = k.leistung(f.folge).filter(|x| x.retired) {
            benutzt.push(x);
        }
    }
    let mut gueltig: HashSet<String> = HashSet::new();
    for l in benutzt {
        gueltig.insert(l.guid.to_ifc());
        gueltig.insert(l.titel.to_ifc());
        for a in k.anteile_von(l.guid) {
            gueltig.insert(a.guid.to_ifc());
            if let Some(x) = a.artikel {
                gueltig.insert(x.to_ifc());
            }
        }
        for f in k.folgen_von(l.guid) {
            gueltig.insert(f.guid.to_ifc());
            gueltig.insert(f.folge.to_ifc());
        }
    }
    gueltig.extend(
        k.artikel
            .iter()
            .filter(|a| !a.retired)
            .map(|a| a.guid.to_ifc()),
    );
    gueltig.extend(
        k.lose
            .iter()
            .filter(|l| !l.retired)
            .map(|l| l.guid.to_ifc()),
    );
    let raten: HashSet<String> = zeilen
        .section("rate")
        .filter_map(|r| r.id.clone())
        .collect();
    let mut out = ExtStore::default();
    out.declare(&satz::ABSCHNITTE_SZO);
    for a in KOPIE {
        for r in zeilen.section(a) {
            let Some(id) = &r.id else { continue };
            let ok = match a {
                "rate" => raten.contains(id),
                // Herkunft aller `kind` unverändert und ohne `proj`; die
                // Marke der Abweichung hängt an `proj` (Regeln 89, 92)
                "origin" => {
                    (gueltig.contains(id) || raten.contains(id)) && !projektherkunft(&r.line)
                }
                _ => gueltig.contains(id),
            };
            if ok {
                out.push_read(a, &r.line);
            }
        }
    }
    let mut s = Satz::neu(&satz::COSTPROJECT);
    s.setzen("key", Some(Wert::Wort("project".into())));
    if let Some((g, stand)) = kopf {
        s.setzen("catalog", Some(Wert::Guid(g)));
        s.setzen("stand", Some(Wert::Ganz(i64::from(stand))));
    }
    out.push_read("costproject", &s.zeile());
    out
}

/// Entstand die `[origin]`-Zeile im Projekt (`proj=1`, Regel 89)?
fn projektherkunft(zeile: &str) -> bool {
    crate::zeile::zerlegen(zeile)
        .is_some_and(|z| z.paare.iter().any(|(k, v)| k == "proj" && v == "1"))
}

/// Zeilen der Quelle (Firma oder Werk) und ihr Kopf (Guid, Stand).
fn quelle_zeilen(firma: Option<&Library>, quelle: &Quelle) -> (ExtStore, Option<(Guid, u32)>) {
    let mut out = ExtStore::default();
    match (quelle, firma) {
        (Quelle::Firma { .. }, Some(f)) => {
            for r in f.ext.recs() {
                out.push_read(&r.section, &r.line);
            }
        }
        _ => {
            for (a, l) in crate::werk_zeilen() {
                out.push_read(a, l);
            }
        }
    }
    let kopf = out
        .section("catalog")
        .next()
        .and_then(|r| zeile::zerlegen(&r.line))
        .and_then(|z| Satz::lesen(&satz::CATALOG, &z).ok())
        .and_then(|s| Some((s.guid("guid")?, s.ganz("stand")? as u32)));
    (out, kopf)
}

/// Hat das Projekt eine Kopie (Bausteingrenze §6)?
fn hat_kopie(m: &Model) -> bool {
    [
        "costproject",
        "article",
        "service",
        "svcpart",
        "svcfollow",
        "rate",
        "lot",
    ]
    .iter()
    .any(|s| m.ext(s).next().is_some())
}

/// Firma, die als Bezug gilt: freigegeben und mit Kostensätzen.
fn firma_bezug(m: &Model, firma: Option<&Library>) -> Option<(Katalog, ExtStore, Guid, u32)> {
    let f = firma?;
    let k = crate::lesen::firma_oder_werk(m, Some(f));
    if !matches!(k.quelle, Quelle::Firma { .. }) {
        return None;
    }
    let (zeilen, kopf) = quelle_zeilen(Some(f), &k.quelle);
    let (g, stand) = kopf?;
    Some((k, zeilen, g, stand))
}

/// Ergebnis eines Plans.
pub struct Plan {
    /// Kopie, die vor den Änderungen ins Projekt kommt (erste Operation in
    /// einem Projekt ohne Kopie).
    pub kopie: Option<ExtStore>,
    pub aenderungen: Vec<Aenderung>,
    /// Wirksamer Katalog nach dem Plan.
    pub katalog: Katalog,
    /// Netto im Umfang vorher und nachher; nur aus [`vorschau_kosten`].
    pub netto: Option<(Cent, Cent)>,
}

/// Plant `ops` für das Projekt und prüft sie (Regel 93), ohne etwas zu
/// ändern.
pub fn planen(
    m: &Model,
    firma: Option<&Library>,
    rolle: Rolle,
    herkunft: &Herkunft,
    ops: &[Op],
    guids: GuidGen,
) -> Result<Plan, Vec<Befund>> {
    for op in ops {
        if op.nur_admin() && rolle != Rolle::Admin {
            return Err(abgelehnt(op, "nur in der Verwaltung"));
        }
    }
    let umfeld = Umfeld::aus_modell(m);
    let (zeilen, kopie) = if hat_kopie(m) {
        (m.ext_store().clone(), None)
    } else {
        let k = kopie(m, firma);
        (k.clone(), Some(k))
    };
    let start = katalog::lesen(
        zeilen
            .recs()
            .iter()
            .map(|r| (r.section.as_str(), r.line.as_str())),
        &umfeld,
        Quelle::Projekt {
            katalog: None,
            stand: None,
        },
    );
    let fb = firma_bezug(m, firma);
    let bezug = match &fb {
        Some((k, z, _, _)) => Some((k.clone(), z.clone())),
        None => {
            let (z, _) = quelle_zeilen(
                None,
                &Quelle::Werk {
                    stand: String::new(),
                },
            );
            Some((crate::lesen::werk(m), z))
        }
    };
    let mut a = Arbeit {
        zeilen,
        umfeld: &umfeld,
        quelle: start.quelle.clone(),
        herkunft,
        guids,
        aend: Vec::new(),
        op: 0,
        bezug,
        firma: fb.map(|f| (f.2, f.3)),
        projekt: true,
    };
    for (i, op) in ops.iter().enumerate() {
        a.op = i;
        a.anwenden(op, Ziel::Projekt)?;
        if let Op::BauleistungZuordnen { typ, schicht, .. } = op {
            let ok = m
                .layer_sets()
                .iter()
                .find(|(_, t)| t.guid == *typ)
                .is_some_and(|(_, t)| *schicht < t.layers.len());
            if !ok {
                return Err(abgelehnt(op, "die Schicht gibt es nicht"));
            }
        }
    }
    let nachher = a.katalog();
    let neu = neue_fehler(&start.befunde, &nachher.befunde);
    if !neu.is_empty() {
        let name = ops.first().map_or("", |o| o.name());
        let mut b = vec![Befund::fehler(
            93,
            befund::r93(name, &neu[0].satz),
            befund::Ort::Datei,
        )];
        b.extend(neu);
        return Err(b);
    }
    Ok(Plan {
        kopie,
        aenderungen: a.aend,
        katalog: nachher,
        netto: None,
    })
}

/// Vorschau (Abnahme 12, Regel 94): Änderungen alt/neu und der wirksame
/// Katalog mit den noch nicht ausgeführten Operationen, rein im Speicher.
/// Modell, `revision` und Verlauf bleiben unberührt. Neue Guids der Vorschau
/// sind nur Platzhalter; die Ausführung vergibt eigene.
pub fn vorschau(
    m: &Model,
    firma: Option<&Library>,
    rolle: Rolle,
    ops: &[Op],
) -> Result<Plan, Vec<Befund>> {
    let h = Herkunft::neu(HerkunftArt::Manual, "2000-01-01", "00:00");
    planen(m, firma, rolle, &h, ops, GuidGen::with_seed(0))
}

/// Vorschau mit Kosten (Abnahme 12): wie [`vorschau`], dazu das Netto im
/// Umfang `u` vorher und nachher, beide mit `lesen::kosten` gerechnet, auf
/// dem wirksamen Katalog und auf dem des Plans. Eine Zuordnung
/// (`BauleistungZuordnen`) rechnet auf einer Kopie des Modells; `m` bleibt
/// unberührt (Regel 94). `sched` ist die Mengenliste des Aufrufers.
pub fn vorschau_kosten(
    m: &Model,
    sched: &sk_model::qto::Schedule,
    firma: Option<&Library>,
    rolle: Rolle,
    ops: &[Op],
    u: &crate::Umfang,
) -> Result<Plan, Vec<Befund>> {
    let mut plan = vorschau(m, firma, rolle, ops)?;
    let vorher = crate::lesen::kosten(m, sched, &crate::lesen::katalog(m, firma), u).netto;
    let zuordnungen: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            Op::BauleistungZuordnen {
                typ,
                schicht,
                bauleistung,
            } => Some((*typ, *schicht, *bauleistung)),
            _ => None,
        })
        .collect();
    let nachher = if zuordnungen.is_empty() {
        crate::lesen::kosten(m, sched, &plan.katalog, u).netto
    } else {
        let mut k = m.clone();
        k.begin("Vorschau");
        for (typ, schicht, svc) in zuordnungen {
            let id = k
                .layer_sets()
                .iter()
                .find(|(_, t)| t.guid == typ)
                .map(|(id, _)| id);
            if let Some(id) = id {
                k.set_layer_svc(id, schicht, svc);
            }
        }
        k.commit();
        crate::lesen::kosten(&k, sched, &plan.katalog, u).netto
    };
    plan.netto = Some((vorher, nachher));
    Ok(plan)
}

/// Prüft eine Operation, ohne etwas zu ändern (Regeln 2, 72–92).
pub fn pruefen(
    m: &Model,
    firma: Option<&Library>,
    rolle: Rolle,
    op: &Op,
) -> Result<(), Vec<Befund>> {
    let h = Herkunft::neu(HerkunftArt::Manual, "2000-01-01", "00:00");
    planen(
        m,
        firma,
        rolle,
        &h,
        std::slice::from_ref(op),
        GuidGen::with_seed(0),
    )
    .map(|_| ())
}

/// Führt Operationen im offenen Schritt des Modells aus (Ziel Projekt).
/// Ein Fehler ändert nichts. Fehlt dem Projekt die Kopie, kommt sie im
/// selben Schritt dazu (Bausteingrenze §6).
pub fn ausfuehren_folge(
    m: &mut Model,
    firma: Option<&Library>,
    rolle: Rolle,
    herkunft: &Herkunft,
    ops: &[Op],
) -> Result<(), Vec<Befund>> {
    debug_assert!(m.in_step(), "Kostenoperation ohne offenen Schritt");
    let seed = m.new_guid().0 as u64;
    let plan = planen(m, firma, rolle, herkunft, ops, GuidGen::with_seed(seed))?;
    m.ext_declare(&satz::ABSCHNITTE_SZO);
    if let Some(k) = plan.kopie {
        for r in k.recs() {
            if let Some(id) = &r.id {
                m.ext_put(&r.section, id, r.line.clone(), None);
            }
        }
    }
    for a in plan.aenderungen {
        match a.neu {
            Some(l) => m.ext_put(a.satz.abschnitt, &a.satz.kennung, l, a.vor.as_deref()),
            None => m.ext_remove(a.satz.abschnitt, &a.satz.kennung),
        }
    }
    for op in ops {
        if let Op::BauleistungZuordnen {
            typ,
            schicht,
            bauleistung,
        } = op
        {
            // `planen` hat Typ und Schicht geprüft; ein Typ mit gemeldetem
            // Problem (Regel 21) bekommt die Bauleistung trotzdem
            let id = m
                .layer_sets()
                .iter()
                .find(|(_, t)| t.guid == *typ)
                .map(|(id, _)| id);
            let ok = id.is_some_and(|id| m.set_layer_svc(id, *schicht, *bauleistung));
            debug_assert!(ok, "Schicht nach der Prüfung nicht gefunden");
        }
    }
    Ok(())
}

/// Eine Operation im offenen Schritt (Ziel Projekt).
pub fn ausfuehren(
    m: &mut Model,
    firma: Option<&Library>,
    rolle: Rolle,
    herkunft: &Herkunft,
    op: Op,
) -> Result<(), Vec<Befund>> {
    ausfuehren_folge(m, firma, rolle, herkunft, std::slice::from_ref(&op))
}

/// Datei › Neu (Regel 92, E3): Mit freigegebenem Firmenkatalog bekommt das
/// neue Projekt die Kopie seiner nicht ausgemusterten Sätze und
/// `[costproject] catalog/stand`; ohne bleibt es ohne Kostenzeile. Für ein
/// frisches Modell vor dem ersten Schritt (kein Rückgängig).
pub fn neues_projekt(m: &mut Model, firma: Option<&Library>) {
    if firma_bezug(m, firma).is_none() || hat_kopie(m) {
        return;
    }
    let k = kopie(m, firma);
    m.ext_declare(&satz::ABSCHNITTE_SZO);
    for r in k.recs() {
        if let Some(id) = &r.id {
            m.ext_put(&r.section, id, r.line.clone(), None);
        }
    }
}

/// Ergebnis von [`firma_anwenden`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirmaNeu {
    pub text: String,
    /// Dateistand vor der Änderung, aus `text`.
    pub stand_vorher: u32,
    pub stand: u32,
    /// Geänderte Stammsätze (für `StandUebernehmen`).
    pub saetze: Vec<SatzId>,
}

/// Kurzform alt → neu für `[log]` (≤ 120 Zeichen): nur die geänderten
/// Felder.
fn kurzform(alt: Option<&str>, neu: Option<&str>) -> (String, String) {
    let paare = |l: Option<&str>| -> Vec<(String, String)> {
        l.and_then(zeile::zerlegen).map_or(Vec::new(), |z| z.paare)
    };
    let (a, n) = (paare(alt), paare(neu));
    let mut sa = Vec::new();
    let mut sn = Vec::new();
    let mut keys: Vec<&String> = a.iter().map(|p| &p.0).collect();
    for (k, _) in &n {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    for k in keys {
        let va = a.iter().find(|p| &p.0 == k).map(|p| p.1.as_str());
        let vn = n.iter().find(|p| &p.0 == k).map(|p| p.1.as_str());
        if va != vn {
            if let Some(v) = va {
                sa.push(format!("{k}={v}"));
            }
            if let Some(v) = vn {
                sn.push(format!("{k}={v}"));
            }
        }
    }
    // Ein Feld: nur die Werte („old=60 new=65“), sonst Schlüssel=Wert
    if sa.len() <= 1 && sn.len() <= 1 {
        let wert = |v: &[String]| {
            v.first()
                .and_then(|x| x.split_once('='))
                .map_or(String::new(), |(_, w)| w.to_string())
        };
        sa = vec![wert(&sa)];
        sn = vec![wert(&sn)];
    }
    let kurz = |v: Vec<String>| v.join(" ").chars().take(120).collect::<String>();
    (kurz(sa), kurz(sn))
}

/// Ziel Firma, rein von Text zu Text (Bausteingrenze §5, VK-05): `text`
/// ist die Datei unter Sperre, `geladen` der Stand, den der Nutzer gesehen
/// hat. Weicht ein betroffener Satz ab, gibt es nur den Befund; andere
/// Sätze dürfen sich geändert haben und bleiben. Stand und nächster
/// `[log]`-Schlüssel kommen aus `text` (N3). Ein Katalog ohne Kostensätze
/// bekommt vorher die Werkssätze, damit eine einzelne Änderung den
/// Werksbestand nicht verdeckt.
pub fn firma_anwenden(
    text: &str,
    geladen: &str,
    rolle: Rolle,
    herkunft: &Herkunft,
    ops: &[Op],
) -> Result<FirmaNeu, Vec<Befund>> {
    let lies = |t: &str| sk_model::read_szk_with(t, &satz::ABSCHNITTE_SZK);
    let fehler = |g: &str| {
        vec![Befund::fehler(
            93,
            befund::r93(ops.first().map_or("", |o| o.name()), g),
            befund::Ort::Datei,
        )]
    };
    let mut lib = lies(text).map_err(|e| fehler(&format!("Firmenkatalog nicht lesbar ({e})")))?;
    let alt = lies(geladen).map_err(|e| fehler(&format!("geladener Stand nicht lesbar ({e})")))?;
    for op in ops {
        if op.nur_admin() && rolle != Rolle::Admin {
            return Err(abgelehnt(op, "nur in der Verwaltung"));
        }
    }
    let umfeld = Umfeld::aus_bibliothek(&lib);
    let mut zeilen = lib.ext.clone();
    zeilen.declare(&satz::ABSCHNITTE_SZK);
    let stamm = ["article", "service", "svcpart", "svcfollow", "rate", "lot"];
    let mut werk_zeilen = Vec::new();
    if !stamm.iter().any(|s| zeilen.section(s).next().is_some()) {
        for (a, l) in crate::werk_zeilen() {
            if a != "catalog" && a != "log" {
                zeilen.push_read(a, l);
                werk_zeilen.push((a, l.to_string()));
            }
        }
    }
    let werk_kopie = !werk_zeilen.is_empty();
    let quelle = Quelle::Firma {
        name: String::new(),
        stand: 0,
    };
    let start = katalog::lesen(
        zeilen
            .recs()
            .iter()
            .map(|r| (r.section.as_str(), r.line.as_str())),
        &umfeld,
        quelle.clone(),
    );
    let mut a = Arbeit {
        zeilen,
        umfeld: &umfeld,
        quelle,
        herkunft,
        guids: GuidGen::from_time(),
        aend: Vec::new(),
        op: 0,
        bezug: None,
        firma: None,
        projekt: false,
    };
    for (i, op) in ops.iter().enumerate() {
        a.op = i;
        a.anwenden(op, Ziel::Firma)?;
    }
    let nachher = a.katalog();
    let neu = neue_fehler(&start.befunde, &nachher.befunde);
    if !neu.is_empty() {
        let mut b = fehler(&neu[0].satz);
        b.extend(neu);
        return Err(b);
    }
    // Hat sich ein betroffener Satz seit dem Laden geändert?
    let zeile_in = |l: &Library, s: &SatzId| {
        l.ext(s.abschnitt)
            .find(|r| r.id.as_deref() == Some(s.kennung.as_str()))
            .map(|r| r.line.clone())
    };
    for x in &a.aend {
        if zeile_in(&lib, &x.satz) != zeile_in(&alt, &x.satz) {
            return Err(fehler(
                "Firmenkatalog wurde inzwischen geändert · neu laden",
            ));
        }
    }
    // Kopf: Stand + 1 und Datum aus `text`; fehlt er, ganz neu (N2)
    let kopf = lib
        .ext("catalog")
        .next()
        .and_then(|r| zeile::zerlegen(&r.line))
        .and_then(|z| Satz::lesen(&satz::CATALOG, &z).ok());
    let stand_vorher = kopf.as_ref().and_then(|s| s.ganz("stand")).unwrap_or(0) as u32;
    let stand = stand_vorher + 1;
    let mut kopf = kopf.unwrap_or_else(|| {
        let mut s = Satz::neu(&satz::CATALOG);
        s.setzen("guid", Some(Wert::Guid(GuidGen::from_time().next_guid())));
        s.setzen("name", Some(Wert::Text("Firmenkatalog".into())));
        s.setzen("status", Some(Wert::Wort("released".into())));
        s
    });
    kopf.setzen("stand", Some(Wert::Ganz(i64::from(stand))));
    kopf.setzen("date", Some(Wert::Text(herkunft.datum.clone())));
    let kopf_id = kopf.kennung().unwrap_or_default();
    lib.ext_declare(&satz::ABSCHNITTE_SZK);
    lib.ext_put("catalog", &kopf_id, kopf.zeile(), None);
    for (sec, l) in werk_zeilen {
        if let Some(id) = sk_model::ext::rec_id(&l) {
            lib.ext_put(sec, &id, l, None);
        }
    }
    let mut saetze: Vec<SatzId> = Vec::new();
    let mut key = lib
        .ext("log")
        .filter_map(|r| r.id.as_deref().and_then(|k| k.parse::<u32>().ok()))
        .max()
        .unwrap_or(0);
    let mut log = |lib: &mut Library, op: &str, rec: &str, of: &str, o: String, n: String| {
        key += 1;
        let mut s = Satz::neu(&satz::LOG);
        s.setzen("key", Some(Wert::Ganz(i64::from(key))));
        s.setzen("stand", Some(Wert::Ganz(i64::from(stand))));
        s.setzen(
            "time",
            Some(Wert::Text(format!("{}T{}", herkunft.datum, herkunft.zeit))),
        );
        s.setzen("role", Some(Wert::Wort(rolle.wort().into())));
        s.setzen("op", Some(Wert::Text(op.into())));
        s.setzen("rec", Some(Wert::Wort(rec.into())));
        s.setzen("of", Some(Wert::Text(of.into())));
        if !o.is_empty() {
            s.setzen("old", Some(Wert::Text(o)));
        }
        if !n.is_empty() {
            s.setzen("new", Some(Wert::Text(n)));
        }
        lib.ext_put("log", &key.to_string(), s.zeile(), None);
    };
    // Werksbestand übernommen: eine Zeile vor der Änderung (Regel 90)
    if werk_kopie {
        let werk = crate::werk_zeilen()
            .into_iter()
            .find(|(a, _)| *a == "catalog")
            .and_then(|(_, l)| zeile::zerlegen(l))
            .and_then(|z| Satz::lesen(&satz::CATALOG, &z).ok());
        let of = werk.as_ref().and_then(|w| w.kennung()).unwrap_or_default();
        let n = werk
            .and_then(|w| w.ganz("stand"))
            .map_or(String::new(), |st| format!("Werksbestand Stand {st}"));
        log(
            &mut lib,
            "werk_uebernommen",
            "catalog",
            &of,
            String::new(),
            n,
        );
    }
    for x in &a.aend {
        match &x.neu {
            Some(l) => lib.ext_put(
                x.satz.abschnitt,
                &x.satz.kennung,
                l.clone(),
                x.vor.as_deref(),
            ),
            None => lib.ext_remove(x.satz.abschnitt, &x.satz.kennung),
        }
        if x.satz.abschnitt == "origin" {
            continue;
        }
        if !saetze.contains(&x.satz) {
            saetze.push(x.satz.clone());
        }
        let (o, n) = kurzform(x.alt.as_deref(), x.neu.as_deref());
        log(
            &mut lib,
            ops[x.op].name(),
            x.satz.abschnitt,
            &x.satz.kennung,
            o,
            n,
        );
    }
    Ok(FirmaNeu {
        text: sk_model::write_szk(&lib),
        stand_vorher,
        stand,
        saetze,
    })
}

/// Satzabschnitt nach Namen (für Aufrufer mit Text).
pub fn abschnitt(name: &str) -> Option<&'static Abschnitt> {
    satz::abschnitt(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lesen;
    use sk_model::{szo, Direction};

    fn hand() -> Herkunft {
        Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "09:00")
    }

    fn g(s: &str) -> Guid {
        Guid::from_ifc(s).unwrap()
    }

    const M10: &str = "1S7bUW0010080200000007";
    const STEIN175: &str = "1S7bUW0010080100000002";

    fn projekt() -> Model {
        let mut m = Model::new();
        m.require_steps();
        m
    }

    fn schritt(
        m: &mut Model,
        firma: Option<&Library>,
        h: &Herkunft,
        ops: &[Op],
    ) -> Result<sk_model::Txn, Vec<Befund>> {
        m.begin("Kosten");
        match ausfuehren_folge(m, firma, Rolle::Admin, h, ops) {
            Ok(()) => Ok(m.commit().expect("ein Schritt")),
            Err(b) => {
                m.rollback();
                Err(b)
            }
        }
    }

    /// Abnahme 10 (Teil sk-cost) und 11: Erste Operation in einem Projekt
    /// ohne Kopie legt Kopie und Änderung in einem Schritt an; Rückgängig
    /// gibt das Projekt bytegleich zurück; `[origin]` nach Regel 88.
    #[test]
    fn erste_operation_kopie_und_aenderung_ein_schritt() {
        let mut m = projekt();
        let vorher = szo::write(&m);
        let t = schritt(
            &mut m,
            None,
            &hand(),
            &[Op::FirmenwertSetzen {
                schluessel: "wage".into(),
                wert: Dez::ganz(65),
            }],
        )
        .unwrap();
        let k = lesen::katalog(&m, None);
        assert!(
            matches!(k.quelle, Quelle::Projekt { stand: Some(7), .. }),
            "{:?}",
            k.quelle
        );
        assert_eq!(k.werte.lohn, Dez::ganz(65));
        assert_eq!(k.leistungen.len(), 22, "Kopie des Werks");
        assert!(k.befunde.is_empty(), "{:#?}", k.befunde);
        let u = k.herkunft_von("rate", "wage").unwrap();
        assert_eq!((u.kind.as_str(), u.bestaetigt), ("manual", true));
        assert!(u.satz.zeile().contains(" status=confirmed"));
        // Regel 89: Marke an der Projektabweichung
        assert!(lesen::befunde(&m, &k).iter().any(|b| b.regel == 89));
        m.apply(&t, Direction::Undo);
        assert_eq!(szo::write(&m), vorher);
        m.apply(&t, Direction::Redo);
        assert_eq!(lesen::katalog(&m, None).werte.lohn, Dez::ganz(65));
    }

    /// Abnahme 11: Fehler ändert nichts und nennt Regel und Satz; Import und
    /// KI schreiben `status=open`.
    #[test]
    fn fehler_aendert_nichts_import_ist_offen() {
        let mut m = projekt();
        let vorher = (szo::write(&m), m.revision(), m.ext_revision());
        let e = schritt(
            &mut m,
            None,
            &hand(),
            &[Op::FirmenwertSetzen {
                schluessel: "wage".into(),
                wert: Dez::ganz(900),
            }],
        )
        .unwrap_err();
        assert_eq!(e[0].regel, 93);
        assert!(
            e[0].satz
                .starts_with("Änderung firmenwert_setzen abgelehnt:"),
            "{e:?}"
        );
        assert_eq!(vorher.0, szo::write(&m));
        // Preis außerhalb des Bereichs: Regel 76 mit dem Artikel
        let e = pruefen(
            &m,
            None,
            Rolle::Admin,
            &Op::PreisSetzen {
                artikel: g(STEIN175),
                preis: Some(Dez::ganz(2_000_000)),
                stand: String::new(),
                quelle: String::new(),
            },
        )
        .unwrap_err();
        assert_eq!(e[0].regel, 76, "{e:?}");
        assert!(e[0].satz.contains("price ist ungültig"), "{e:?}");
        // Nutzer darf keine Bauleistung anlegen
        let e = pruefen(
            &m,
            None,
            Rolle::Nutzer,
            &Op::Ausmustern {
                satz: SatzId::neu("service", M10),
            },
        )
        .unwrap_err();
        assert!(e[0].satz.contains("nur in der Verwaltung"));
        // Import: offen
        let mut h = hand();
        h.art = HerkunftArt::Import;
        h.quelle = "Händler Muster".into();
        h.sicher = Some(Sicherheit::Mid);
        schritt(
            &mut m,
            None,
            &h,
            &[Op::PreisSetzen {
                artikel: g(STEIN175),
                preis: Some(Dez::lesen("23.5", 4).unwrap()),
                stand: "10/2026".into(),
                quelle: "Angebot 4711".into(),
            }],
        )
        .unwrap();
        let k = lesen::katalog(&m, None);
        assert_eq!(k.artikel(g(STEIN175)).unwrap().preis, Dez::lesen("23.5", 4));
        let u = k.herkunft_von("article", STEIN175).unwrap();
        assert!(!u.bestaetigt);
        assert!(u.satz.zeile().contains("kind=import status=open"));
        assert!(u.satz.zeile().contains(" conf=mid"));
        // Bestätigen setzt confirmed, ein Schritt
        schritt(
            &mut m,
            None,
            &hand(),
            &[Op::HerkunftBestaetigen {
                satz: SatzId::neu("article", STEIN175),
            }],
        )
        .unwrap();
        let k = lesen::katalog(&m, None);
        assert!(k.herkunft_von("article", STEIN175).unwrap().bestaetigt);
        assert_ne!(vorher.2, m.ext_revision());
    }

    /// Abnahme 13: drei Operationen ein Schritt; scheitert die dritte,
    /// bleibt nichts.
    #[test]
    fn folge_ist_ein_schritt_oder_nichts() {
        let mut m = projekt();
        let vorher = szo::write(&m);
        let lohn = Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(65),
        };
        let mwst = Op::FirmenwertSetzen {
            schluessel: "vat".into(),
            wert: Dez::ganz(7),
        };
        let kaputt = Op::PreisSetzen {
            artikel: g("0000000000000000000099"),
            preis: None,
            stand: String::new(),
            quelle: String::new(),
        };
        assert!(schritt(&mut m, None, &hand(), &[lohn.clone(), mwst.clone(), kaputt]).is_err());
        assert_eq!(szo::write(&m), vorher);
        let lv = Op::LvGliederungSetzen { untertitel: true };
        let t = schritt(&mut m, None, &hand(), &[lohn, mwst, lv]).unwrap();
        let k = lesen::katalog(&m, None);
        assert_eq!((k.werte.lohn, k.werte.mwst), (Dez::ganz(65), Dez::ganz(7)));
        assert!(k.kopie.as_ref().unwrap().lvstorey);
        m.apply(&t, Direction::Undo);
        assert_eq!(szo::write(&m), vorher);
    }

    /// Neue Sätze kommen an ihre Stelle (BIM §3: nach Guid, Stoffanteile
    /// nach (service, nr)); Fremde Schlüssel bleiben beim Neuschreiben
    /// (Abnahme 4, Teil sk-cost).
    #[test]
    fn neue_zeilen_sortiert_fremde_schluessel_bleiben() {
        let mut m = projekt();
        schritt(
            &mut m,
            None,
            &hand(),
            &[
                Op::StoffanteilSetzen {
                    bauleistung: g(M10),
                    nr: 3,
                    anteil: Some(Stoff::Schicht {
                        faktor: Dez::lesen("1.03", 6).unwrap(),
                    }),
                },
                Op::StoffanteilSetzen {
                    bauleistung: g(M10),
                    nr: 1,
                    anteil: Some(Stoff::Artikel {
                        artikel: g(STEIN175),
                        menge: Dez::ganz(1),
                    }),
                },
            ],
        )
        .unwrap();
        let k = lesen::katalog(&m, None);
        let nrs: Vec<u32> = k.anteile_von(g(M10)).map(|a| a.nr).collect();
        assert_eq!(nrs, [1, 2, 3]);
        let zeilen: Vec<String> = m.ext("svcpart").map(|r| r.line.clone()).collect();
        let i = zeilen.iter().position(|l| l.contains(" nr=3 ")).unwrap();
        assert!(
            zeilen[i - 1].contains(&format!("service={M10} nr=2")),
            "{zeilen:#?}"
        );
        // fremder Schlüssel an einem Artikel bleibt beim Preis setzen
        let alt = m
            .ext("article")
            .find(|r| r.id.as_deref() == Some(STEIN175))
            .unwrap()
            .line
            .clone();
        m.begin("fremd");
        m.ext_put("article", STEIN175, format!("{alt} supno=\"A 17\""), None);
        m.commit();
        schritt(
            &mut m,
            None,
            &hand(),
            &[Op::PreisSetzen {
                artikel: g(STEIN175),
                preis: Some(Dez::ganz(24)),
                stand: String::new(),
                quelle: String::new(),
            }],
        )
        .unwrap();
        let neu = &m
            .ext("article")
            .find(|r| r.id.as_deref() == Some(STEIN175))
            .unwrap()
            .line;
        assert!(neu.ends_with(" supno=\"A 17\""), "{neu}");
        assert!(neu.contains(" price=24 "), "{neu}");
    }

    /// Abnahme 10: Datei › Neu ohne Firmenkatalog schreibt nichts; mit
    /// Firmenkatalog die Kopie der nicht ausgemusterten Sätze und
    /// `[costproject] catalog/stand`.
    #[test]
    fn neues_projekt_mit_und_ohne_firma() {
        let mut m = Model::new();
        let leer = szo::write(&m);
        neues_projekt(&mut m, None);
        assert_eq!(szo::write(&m), leer);
        neues_projekt(&mut m, Some(&Library::standard()));
        assert_eq!(szo::write(&m), leer, "Firma ohne Kostensätze");
        // Firma: Werk plus eigener Lohn, ein Artikel ausgemustert
        let text = sk_model::write_szk(&Library::standard());
        let f = firma_anwenden(
            &text,
            &text,
            Rolle::Admin,
            &hand(),
            &[
                Op::FirmenwertSetzen {
                    schluessel: "wage".into(),
                    wert: Dez::ganz(62),
                },
                Op::Ausmustern {
                    satz: SatzId::neu("article", "1S7bUW001008010000000G"),
                },
            ],
        )
        .unwrap();
        // Werksbestand übernommen: eine Zeile vor den Änderungen (Regel 90)
        let logs: Vec<&str> = f.text.lines().filter(|l| l.starts_with("[log]")).collect();
        assert_eq!(logs.len(), 3, "{logs:#?}");
        assert!(logs[0].starts_with("[log] key=1 stand=1 "), "{}", logs[0]);
        assert!(logs[0].contains(" op=werk_uebernommen rec=catalog of=1S7bUW0010080600000001 "));
        assert!(logs[1].starts_with("[log] key=2 "), "{}", logs[1]);
        assert!(
            f.text.lines().any(|l| l.starts_with("[origin] key=1S7bUW")),
            "Werksherkunft mit"
        );
        // dazu ein unbenutzter ausgemusterter Artikel
        let alt =
            "[article] guid=0000000000000000000A99 name=\"Altartikel\" unit=m2 price=1 retired=1";
        let i = f
            .text
            .find("[article] guid=1S7bUW001008010000000G")
            .unwrap();
        let text = format!("{}{alt}\n{}", &f.text[..i], &f.text[i..]);
        let lib = sk_model::read_szk_with(&text, &satz::ABSCHNITTE_SZK).unwrap();
        let mut m = Model::from_library(&lib);
        neues_projekt(&mut m, Some(&lib));
        let k = lesen::katalog(&m, Some(&lib));
        assert_eq!(k.werte.lohn, Dez::ganz(62));
        // ausgemustert, aber von einer Bauleistung benutzt: kommt mit
        // (Regel 87), unbenutzt Ausgemustertes nicht
        assert!(k
            .artikel(g("1S7bUW001008010000000G"))
            .is_some_and(|a| a.retired));
        assert!(
            k.artikel(g("0000000000000000000A99")).is_none(),
            "ausgemustert nicht kopiert"
        );
        assert!(
            lesen::firma_oder_werk(&Model::from_library(&lib), Some(&lib))
                .artikel(g("0000000000000000000A99"))
                .is_some()
        );
        assert_eq!(k.artikel.len(), 19);
        assert!(k.befunde.iter().all(|b| b.regel == 87), "{:#?}", k.befunde);
        let c = k.kopie.as_ref().unwrap();
        assert_eq!(c.stand, Some(1));
        assert!(c.katalog.is_some());
        assert!(lesen::befunde(&m, &k).iter().all(|b| b.regel != 92));
    }

    fn firmentext(stand: u32) -> String {
        let mut lib = Library::standard();
        lib.ext_declare(&satz::ABSCHNITTE_SZK);
        lib.ext_put(
            "catalog",
            "0000000000000000000F01",
            format!("[catalog] guid=0000000000000000000F01 name=\"Muster Bau\" stand={stand} date=2026-10-01 status=released"),
            None,
        );
        for (a, l) in crate::werk_zeilen() {
            if a != "catalog" {
                lib.ext_put(a, &sk_model::ext::rec_id(l).unwrap(), l.to_string(), None);
            }
        }
        sk_model::write_szk(&lib)
    }

    fn lohn(v: i64) -> Op {
        Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(v),
        }
    }

    /// Abnahme 15a (VK-05): Firma rein auf Text.
    #[test]
    fn firma_anwenden_lohn() {
        let text = firmentext(3);
        let f = firma_anwenden(&text, &text, Rolle::Admin, &hand(), &[lohn(65)]).unwrap();
        assert_eq!((f.stand_vorher, f.stand), (3, 4));
        assert_eq!(f.saetze, [SatzId::neu("rate", "wage")]);
        let alt: Vec<&str> = text.lines().collect();
        let neu: Vec<&str> = f.text.lines().collect();
        let weg: Vec<&&str> = alt.iter().filter(|l| !neu.contains(l)).collect();
        let dazu: Vec<&&str> = neu.iter().filter(|l| !alt.contains(l)).collect();
        assert_eq!(weg.len(), 2, "Lohn und Kopf: {weg:#?}");
        assert!(weg.iter().any(|l| **l == "[rate] key=wage num=60"));
        assert!(dazu.iter().any(|l| **l == "[rate] key=wage num=65"));
        assert!(dazu
            .iter()
            .any(|l| l.starts_with("[catalog] ") && l.contains(" stand=4 date=2026-10-08 ")));
        assert!(dazu.contains(&&"[log] key=1 stand=4 time=2026-10-08T09:00 role=admin op=firmenwert_setzen rec=rate of=wage old=\"60\" new=\"65\""), "{dazu:#?}");
        assert!(dazu
            .iter()
            .any(|l| l.starts_with("[origin] key=wage rec=rate kind=manual status=confirmed")));
        assert_eq!(dazu.len(), 4, "{dazu:#?}");
        // liest sich ohne Befund
        let lib = sk_model::read_szk_with(&f.text, &satz::ABSCHNITTE_SZK).unwrap();
        let k = lesen::katalog(&Model::from_library(&lib), Some(&lib));
        assert!(k.befunde.is_empty(), "{:#?}", k.befunde);
        assert_eq!(k.werte.lohn, Dez::ganz(65));
    }

    /// Abnahme 15a: ohne `[catalog]` entsteht die Zeile vollständig (N2);
    /// eine fremde Änderung an einem anderen Satz bleibt, an diesem Satz
    /// gibt es den Befund; Stand und `[log]` aus der Datei (N3, N4).
    #[test]
    fn firma_anwenden_kopf_und_fremde_aenderungen() {
        let text = sk_model::write_szk(&Library::standard());
        let f = firma_anwenden(&text, &text, Rolle::Admin, &hand(), &[lohn(65)]).unwrap();
        assert_eq!((f.stand_vorher, f.stand), (0, 1));
        let kopf = f.text.lines().find(|l| l.starts_with("[catalog]")).unwrap();
        assert!(
            kopf.contains(" name=\"Firmenkatalog\" stand=1 date=2026-10-08 status=released"),
            "{kopf}"
        );
        let lib = sk_model::read_szk_with(&f.text, &satz::ABSCHNITTE_SZK).unwrap();
        let k = lesen::katalog(&Model::from_library(&lib), Some(&lib));
        assert!(k.befunde.is_empty(), "{:#?}", k.befunde);
        assert_eq!(k.leistungen.len(), 22, "Werkssätze kopiert");

        // Anderer Satz in der Datei geändert: gelingt, fremde Änderung bleibt
        let geladen = firmentext(3);
        let datei = geladen.replace("[rate] key=vat num=19", "[rate] key=vat num=7");
        let f = firma_anwenden(&datei, &geladen, Rolle::Admin, &hand(), &[lohn(65)]).unwrap();
        assert!(f.text.contains("[rate] key=vat num=7\n"));
        assert!(f.text.contains("[rate] key=wage num=65\n"));
        // Dieser Satz geändert: Befund, kein Text
        let datei = geladen.replace("[rate] key=wage num=60", "[rate] key=wage num=61");
        let e = firma_anwenden(&datei, &geladen, Rolle::Admin, &hand(), &[lohn(65)]).unwrap_err();
        assert!(
            e[0].satz
                .contains("Firmenkatalog wurde inzwischen geändert"),
            "{e:?}"
        );
        // Fremdes [log] und höherer Stand nur in der Datei
        let datei = firmentext(5)
            + "[log] key=1 stand=5 time=2026-10-08T08:00 role=admin op=preis_setzen rec=rate of=vat\n";
        let f = firma_anwenden(&datei, &geladen, Rolle::Admin, &hand(), &[lohn(65)]).unwrap();
        assert_eq!((f.stand_vorher, f.stand), (5, 6));
        assert!(f.text.contains("[log] key=2 stand=6 "), "{}", f.text);
        let lib = sk_model::read_szk_with(&f.text, &satz::ABSCHNITTE_SZK).unwrap();
        let k = lesen::katalog(&Model::from_library(&lib), Some(&lib));
        assert!(k.befunde.iter().all(|b| b.regel != 90), "{:#?}", k.befunde);
    }

    /// Regel 92: Übernehmen bringt die Firmenzeile ins Projekt; sind danach
    /// alle Sätze gleich, rückt der Stand mit.
    #[test]
    fn stand_uebernehmen() {
        let text = firmentext(3);
        let lib = sk_model::read_szk_with(&text, &satz::ABSCHNITTE_SZK).unwrap();
        let mut m = Model::from_library(&lib);
        neues_projekt(&mut m, Some(&lib));
        m.require_steps();
        let neu = firma_anwenden(&text, &text, Rolle::Admin, &hand(), &[lohn(65)]).unwrap();
        let lib2 = sk_model::read_szk_with(&neu.text, &satz::ABSCHNITTE_SZK).unwrap();
        let k = lesen::katalog(&m, Some(&lib2));
        assert!(lesen::befunde(&m, &k).iter().any(|b| b.regel == 92));
        assert_eq!(
            k.werte.lohn,
            Dez::ganz(60),
            "Projekt bleibt auf seinem Stand"
        );
        schritt(
            &mut m,
            Some(&lib2),
            &hand(),
            &[Op::StandUebernehmen {
                saetze: neu.saetze.clone(),
            }],
        )
        .unwrap();
        let k = lesen::katalog(&m, Some(&lib2));
        assert_eq!(k.werte.lohn, Dez::ganz(65));
        assert_eq!(k.kopie.as_ref().unwrap().stand, Some(4));
        assert!(lesen::befunde(&m, &k)
            .iter()
            .all(|b| b.regel != 92 && b.regel != 89));
    }

    /// Abnahme 10 und 24 (Regel 89, Nachtrag BIM 09:40): Die Kopie nimmt
    /// die Herkunft aller `kind` unverändert und ohne `proj` mit; die Marke
    /// hängt an `proj=1`, das jede Operation im Projekt schreibt;
    /// `AbweichungZuruecknehmen` stellt Zeile und Firmenherkunft wieder her.
    #[test]
    fn herkunft_proj_und_marke() {
        let text = firmentext(3);
        let mut imp = Herkunft::neu(HerkunftArt::Import, "2026-10-01", "08:00");
        imp.quelle = "Händlerliste".into();
        imp.sicher = Some(Sicherheit::Rough);
        let f = firma_anwenden(&text, &text, Rolle::Admin, &imp, &[lohn(63)]).unwrap();
        let firma_origin = f
            .text
            .lines()
            .find(|l| l.starts_with("[origin] key=wage "))
            .unwrap()
            .to_string();
        assert!(firma_origin.contains(" kind=import status=open "));
        assert!(!firma_origin.contains("proj"), "{firma_origin}");
        let lib = sk_model::read_szk_with(&f.text, &satz::ABSCHNITTE_SZK).unwrap();
        let mut m = Model::from_library(&lib);
        neues_projekt(&mut m, Some(&lib));
        m.require_steps();
        let origin = |m: &Model| {
            m.ext("origin")
                .find(|r| r.id.as_deref() == Some("wage"))
                .map(|r| r.line.clone())
        };
        assert_eq!(origin(&m).as_deref(), Some(firma_origin.as_str()));
        let k = lesen::katalog(&m, Some(&lib));
        let u = k.herkunft_von("rate", "wage").unwrap();
        assert_eq!(
            (u.kind.as_str(), u.bestaetigt, u.proj),
            ("import", false, false)
        );
        assert!(lesen::befunde(&m, &k).iter().all(|b| b.regel != 89));
        // Operation im Projekt: proj=1 und Marke
        schritt(&mut m, Some(&lib), &hand(), &[lohn(70)]).unwrap();
        let l = origin(&m).unwrap();
        assert!(l.contains(" kind=manual ") && l.contains(" proj=1"), "{l}");
        let k = lesen::katalog(&m, Some(&lib));
        assert!(k.herkunft_von("rate", "wage").unwrap().proj);
        assert!(lesen::befunde(&m, &k).iter().any(|b| b.regel == 89));
        // zurücknehmen: Zeile und Firmenherkunft ohne proj
        schritt(
            &mut m,
            Some(&lib),
            &hand(),
            &[Op::AbweichungZuruecknehmen {
                saetze: vec![SatzId::neu("rate", "wage")],
            }],
        )
        .unwrap();
        assert_eq!(origin(&m).as_deref(), Some(firma_origin.as_str()));
        let k = lesen::katalog(&m, Some(&lib));
        assert_eq!(k.werte.lohn, Dez::ganz(63));
        assert!(lesen::befunde(&m, &k).iter().all(|b| b.regel != 89));
    }

    #[test]
    fn namen_und_bezeichnungen() {
        let names: HashSet<&str> = NAMEN.iter().map(|n| n.0).collect();
        assert_eq!(names.len(), NAMEN.len());
        assert_eq!(lohn(65).name(), "firmenwert_setzen");
        assert_eq!(lohn(65).bezeichnung(), "Lohn 65,00 €/h");
        // die Werksabläufe rufen nur bekannte Operationen
        for l in crate::WERK.lines().filter(|l| l.contains(" step=op ")) {
            let op = l.split(" op=").nth(1).unwrap().split(' ').next().unwrap();
            assert!(names.contains(op), "{op}");
        }
        assert_eq!(tag_aus_tagen(0), (1970, 1, 1));
        assert_eq!(tag_aus_tagen(20_734), (2026, 10, 8));
    }

    #[test]
    fn bauleistung_auch_am_typ_mit_problem() {
        // Review 3ae: AW-36,5 mit Streifen aus Porenbeton (Regel 21). Die
        // Zuordnung kommt an, ein Schritt, Rückgängig gibt die Datei zurück
        let t = szo::write(&Model::new()).replace(
            "bearing=240 strip=2dv8rsAYH3ovLj1KIg1$Mo",
            "bearing=240 strip=2wuC33GkTD9Qack6WJ4EsM",
        );
        let mut m = szo::read(&t, GuidGen::with_seed(7)).unwrap().model;
        let (id, typ) = m
            .layer_sets()
            .iter()
            .find(|(_, t)| t.code == "AW-36,5")
            .map(|(id, t)| (id, t.guid))
            .unwrap();
        assert!(m.bearing_problem(m.layer_set(id).unwrap()).is_some());
        let g = lesen::werk(&m).leistungen[0].guid;
        let h = Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "09:00");
        let vorher = szo::write(&m);
        m.begin("Bauleistung");
        let op = Op::BauleistungZuordnen {
            typ,
            schicht: 0,
            bauleistung: Some(g),
        };
        ausfuehren(&mut m, None, Rolle::Admin, &h, op).unwrap();
        let t = m.commit().unwrap();
        assert_eq!(m.layer_set(id).unwrap().layers[0].svc, Some(g));
        m.apply(&t, Direction::Undo);
        assert_eq!(szo::write(&m), vorher);
        m.apply(&t, Direction::Redo);
        assert_eq!(m.layer_set(id).unwrap().layers[0].svc, Some(g));
    }
}

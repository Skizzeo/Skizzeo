//! Wirksame Stammdaten: Kostenzeilen lesen, nach den Regeln 72–80 und
//! 85–91 prüfen und als Datensätze bereitstellen (Bausteingrenze §6).
//! Projekt, Firmenkatalog und Werksbestand gehen durch denselben Leser.

use crate::befund::{self, satz_ort, Befund, Ort};
use crate::geld::Dez;
use crate::satz::{self, Abschnitt, Satz, Wert};
use crate::zeile;
use sk_model::Guid;
use std::collections::{HashMap, HashSet};

/// Einheit eines Artikels oder einer Bauleistung.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Einheit {
    M2,
    M3,
    M,
    T,
    Kg,
    St,
}

impl Einheit {
    pub fn aus(w: &str) -> Option<Einheit> {
        Some(match w {
            "m2" => Einheit::M2,
            "m3" => Einheit::M3,
            "m" => Einheit::M,
            "t" => Einheit::T,
            "kg" => Einheit::Kg,
            "st" => Einheit::St,
            _ => return None,
        })
    }

    pub fn wort(self) -> &'static str {
        match self {
            Einheit::M2 => "m2",
            Einheit::M3 => "m3",
            Einheit::M => "m",
            Einheit::T => "t",
            Einheit::Kg => "kg",
            Einheit::St => "st",
        }
    }

    /// Anzeige: „m²“.
    pub fn zeichen(self) -> &'static str {
        match self {
            Einheit::M2 => "m²",
            Einheit::M3 => "m³",
            Einheit::M => "m",
            Einheit::T => "t",
            Einheit::Kg => "kg",
            Einheit::St => "St",
        }
    }
}

/// Mengenbezug einer Bauleistung (BIM §3.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bezug {
    Flaeche,
    Volumen,
    Laenge,
    Umfang,
    Schalung,
    Stahl,
}

impl Bezug {
    pub fn aus(w: &str) -> Option<Bezug> {
        Some(match w {
            "area" => Bezug::Flaeche,
            "volume" => Bezug::Volumen,
            "length" => Bezug::Laenge,
            "perimeter" => Bezug::Umfang,
            "formwork" => Bezug::Schalung,
            "steel" => Bezug::Stahl,
            _ => return None,
        })
    }

    pub fn wort(self) -> &'static str {
        match self {
            Bezug::Flaeche => "area",
            Bezug::Volumen => "volume",
            Bezug::Laenge => "length",
            Bezug::Umfang => "perimeter",
            Bezug::Schalung => "formwork",
            Bezug::Stahl => "steel",
        }
    }

    /// Regel 80: Einheit passt zum Mengenbezug (Ausnahme `steel` mit `kg`).
    pub fn passt(self, e: Einheit) -> bool {
        match self {
            Bezug::Flaeche | Bezug::Schalung => e == Einheit::M2,
            Bezug::Volumen => e == Einheit::M3,
            Bezug::Laenge | Bezug::Umfang => e == Einheit::M,
            Bezug::Stahl => matches!(e, Einheit::T | Einheit::Kg),
        }
    }
}

/// `[article]`
#[derive(Clone, Debug, PartialEq)]
pub struct Artikel {
    pub guid: Guid,
    pub name: String,
    /// `None`: Hilfsstoff (E2).
    pub mat: Option<Guid>,
    /// Art des Baustoffs `mat` (Fuge im Vorschlag `conv`, Regel 108).
    pub kategorie: Option<sk_model::library::MatCategory>,
    /// Dicke in mm; `None`: jede Dicke.
    pub t: Option<Dez>,
    pub einheit: Einheit,
    /// netto €/Einheit; `None`: Preis fehlt.
    pub preis: Option<Dez>,
    /// Stück je Einheit (nur Preiseingabe je Stück, Regel 108); `None`
    /// auch, wenn `conv` ungültig ist (Regel 76).
    pub conv: Option<Dez>,
    /// Standardartikel (nach Regel 77 bereinigt).
    pub std: bool,
    pub retired: bool,
    pub satz: Satz,
}

/// `[service]`
#[derive(Clone, Debug, PartialEq)]
pub struct Leistung {
    pub guid: Guid,
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
    /// Regel: Bauteilarten (`kinds.rs`-Wörter); leer = nur als Folge.
    pub kategorien: Vec<String>,
    pub mat: Option<Guid>,
    pub tmin: Option<Dez>,
    pub tmax: Option<Dez>,
    pub funktion: Option<String>,
    pub retired: bool,
    pub satz: Satz,
}

impl Leistung {
    /// Zahl der gesetzten Regelfelder (Regel 81 Stufe 2: spezifischer).
    pub fn regelfelder(&self) -> usize {
        usize::from(!self.kategorien.is_empty())
            + usize::from(self.mat.is_some())
            + usize::from(self.tmin.is_some())
            + usize::from(self.tmax.is_some())
            + usize::from(self.funktion.is_some())
    }
}

/// `[svcpart]`
#[derive(Clone, Debug, PartialEq)]
pub struct Anteil {
    pub guid: Guid,
    pub leistung: Guid,
    pub nr: u32,
    /// Fester Artikel; `None`: Artikel der Schicht (Regel 82).
    pub artikel: Option<Guid>,
    pub menge: Dez,
    pub satz: Satz,
}

/// `[svcfollow]`
#[derive(Clone, Debug, PartialEq)]
pub struct Folge {
    pub guid: Guid,
    pub leistung: Guid,
    pub nr: u32,
    pub folge: Guid,
    pub faktor: Dez,
    pub satz: Satz,
}

/// `[lot]`: Los oder (mit `parent`) Titel.
#[derive(Clone, Debug, PartialEq)]
pub struct Los {
    pub guid: Guid,
    pub name: String,
    pub nr: String,
    pub parent: Option<Guid>,
    pub pre: Option<String>,
    pub retired: bool,
    pub satz: Satz,
}

/// `[origin]`
#[derive(Clone, Debug, PartialEq)]
pub struct Ursprung {
    pub key: String,
    pub rec: String,
    pub kind: String,
    /// `confirmed` nach Regel 88 auch bei `manual` + `open`.
    pub bestaetigt: bool,
    /// `proj=1`: im Projekt entstanden, Marke der Abweichung (Regel 89).
    pub proj: bool,
    pub satz: Satz,
}

/// `[catalog]`
#[derive(Clone, Debug, PartialEq)]
pub struct Kopf {
    pub guid: Guid,
    pub name: String,
    pub stand: u32,
    pub entwurf: bool,
    pub satz: Satz,
}

/// `[costproject]`
#[derive(Clone, Debug, PartialEq)]
pub struct Kopie {
    pub katalog: Option<Guid>,
    pub stand: Option<u32>,
    pub lvstorey: bool,
    pub keep: Option<u32>,
    pub satz: Satz,
}

/// `[log]`
#[derive(Clone, Debug, PartialEq)]
pub struct Protokoll {
    pub key: u32,
    pub stand: u32,
    pub satz: Satz,
}

/// `[proposal]`: Vorschlag aus einem Projekt, nur im Entwurf (BIM §3.16,
/// Regel 105). Werte wie in der Datei (Punkt).
#[derive(Clone, Debug, PartialEq)]
pub struct Vorschlag {
    pub key: u32,
    pub projekt: Guid,
    /// Projektname zur Anzeige; leer, wenn keiner.
    pub name: String,
    pub rec: String,
    pub of: String,
    pub feld: String,
    pub alt: Option<String>,
    pub neu: String,
    pub datum: String,
    pub satz: Satz,
}

/// Firmenwerte (`[rate]`) mit den Werkswerten als Rückfall (BIM §3.6).
#[derive(Clone, Debug, PartialEq)]
pub struct Firmenwerte {
    /// Verrechnungslohn €/h.
    pub lohn: Dez,
    /// Zuschlag auf Stoff in %.
    pub zuschlag: Dez,
    /// MwSt. in %.
    pub mwst: Dez,
    /// Bewehrungsgrad kg/m³ je Bauteilart (`kinds.rs`-Wort).
    pub stahl: Vec<(String, Dez)>,
}

/// Bekannte Firmenwerte: Schlüssel, Bereich, Werkswert.
pub const RATEN: [(&str, i64, i64, i64); 6] = [
    ("wage", 0, 500, 60),
    ("surcharge", 0, 100, 0),
    ("vat", 0, 100, 19),
    ("steel.floor", 0, 400, 100),
    ("steel.groundslab", 0, 400, 80),
    ("steel.stripfooting", 0, 400, 40),
];

impl Firmenwerte {
    pub fn werk() -> Firmenwerte {
        let mut w = Firmenwerte {
            lohn: Dez::NULL,
            zuschlag: Dez::NULL,
            mwst: Dez::NULL,
            stahl: Vec::new(),
        };
        for (k, _, _, v) in RATEN {
            w.setzen(k, Dez::ganz(v));
        }
        w
    }

    fn setzen(&mut self, key: &str, v: Dez) {
        match key {
            "wage" => self.lohn = v,
            "surcharge" => self.zuschlag = v,
            "vat" => self.mwst = v,
            k => {
                if let Some(cat) = k.strip_prefix("steel.") {
                    match self.stahl.iter_mut().find(|(c, _)| c == cat) {
                        Some(s) => s.1 = v,
                        None => self.stahl.push((cat.to_string(), v)),
                    }
                }
            }
        }
    }

    /// Bewehrungsgrad kg/m³ der Bauteilart, wenn es einen gibt.
    pub fn stahl(&self, cat: &str) -> Option<Dez> {
        self.stahl.iter().find(|(c, _)| c == cat).map(|(_, v)| *v)
    }
}

/// Bereich eines Firmenwerts; `None`: unbekannter Schlüssel (neuere
/// Fassung, bleibt roh).
pub fn rate_bereich(key: &str) -> Option<(i64, i64)> {
    if let Some((_, lo, hi, _)) = RATEN.iter().find(|r| r.0 == key) {
        return Some((*lo, *hi));
    }
    let cat = key.strip_prefix("steel.")?;
    satz::KATEGORIEN.contains(&cat).then_some((0, 400))
}

/// Woher die wirksamen Stammdaten kommen (Bausteingrenze §6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Quelle {
    /// Projektkopie (`[costproject]` oder Kostenzeilen im Projekt).
    Projekt {
        katalog: Option<Guid>,
        stand: Option<u32>,
    },
    /// Freigegebener Firmenkatalog.
    Firma { name: String, stand: u32 },
    /// Werksbestand des Programms; `stand` „MM/JJJJ“.
    Werk { stand: String },
}

impl Quelle {
    /// Kopf des Kostenreiters (E3): „Werkspreise 10/2026“.
    pub fn text(&self) -> String {
        match self {
            Quelle::Projekt { stand: Some(s), .. } => format!("Projektstand {s}"),
            Quelle::Projekt { stand: None, .. } => "Projektstand".into(),
            Quelle::Firma { name, stand } => format!("{name}, Stand {stand}"),
            Quelle::Werk { stand } => format!("Werkspreise {stand}"),
        }
    }
}

/// Wirksame Stammdaten mit den Befunden beim Lesen.
#[derive(Clone, Debug, PartialEq)]
pub struct Katalog {
    pub quelle: Quelle,
    pub artikel: Vec<Artikel>,
    pub leistungen: Vec<Leistung>,
    pub anteile: Vec<Anteil>,
    pub folgen: Vec<Folge>,
    pub lose: Vec<Los>,
    pub werte: Firmenwerte,
    pub herkunft: Vec<Ursprung>,
    pub kopf: Option<Kopf>,
    pub kopie: Option<Kopie>,
    pub protokoll: Vec<Protokoll>,
    /// Offene Vorschläge, nur aus einem Entwurf (Regel 105).
    pub vorschlaege: Vec<Vorschlag>,
    /// Stand des freigegebenen Firmenkatalogs, wenn einer da ist (Regel 92).
    pub firma_stand: Option<u32>,
    pub befunde: Vec<Befund>,
    /// FNV-1a über die gelesenen Satzzeilen (Bausteingrenze §5): ändert sich
    /// ein Satz, ordnet der Kostenspeicher alles neu zu.
    pub stempel: u64,
}

/// FNV-1a, 64 Bit, eigene Umsetzung.
pub(crate) fn fnv(h: u64, bytes: &[u8]) -> u64 {
    let mut h = h;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Startwert von FNV-1a.
pub(crate) const FNV_START: u64 = 0xcbf2_9ce4_8422_2325;

impl Katalog {
    pub fn artikel(&self, g: Guid) -> Option<&Artikel> {
        self.artikel.iter().find(|a| a.guid == g)
    }

    pub fn leistung(&self, g: Guid) -> Option<&Leistung> {
        self.leistungen.iter().find(|l| l.guid == g)
    }

    pub fn los(&self, g: Guid) -> Option<&Los> {
        self.lose.iter().find(|l| l.guid == g)
    }

    /// Stoffanteile einer Bauleistung nach `nr`.
    pub fn anteile_von(&self, g: Guid) -> impl Iterator<Item = &Anteil> {
        self.anteile.iter().filter(move |a| a.leistung == g)
    }

    /// Folgepositionen einer Bauleistung nach `nr`.
    pub fn folgen_von(&self, g: Guid) -> impl Iterator<Item = &Folge> {
        self.folgen.iter().filter(move |f| f.leistung == g)
    }

    /// Herkunft eines Satzes, wenn eine Zeile sie nennt.
    pub fn herkunft_von(&self, rec: &str, key: &str) -> Option<&Ursprung> {
        self.herkunft.iter().find(|u| u.rec == rec && u.key == key)
    }

    /// OZ einer Bauleistung: Titel-`nr` + „.“ + `pos` vierstellig (Regel 86).
    pub fn oz(&self, l: &Leistung) -> String {
        let t = self.los(l.titel).map_or("?", |t| t.nr.as_str());
        format!("{t}.{:04}", l.pos)
    }

    /// OZ mit Los davor („1.01.0020“): eindeutig über alle Lose. Für alles,
    /// was Positionen mehrerer Lose zeigt (Kostenblatt, CSV, Befunde); im LV
    /// eines Loses gilt [`Katalog::oz`].
    pub fn oz_voll(&self, l: &Leistung) -> String {
        match self
            .los(l.titel)
            .and_then(|t| t.parent)
            .and_then(|p| self.los(p))
        {
            Some(los) => format!("{}.{}", los.nr, self.oz(l)),
            None => self.oz(l),
        }
    }
}

/// Was der Leser außerhalb der Kostenzeilen kennen muss (Regel 73).
pub struct Umfeld {
    /// Baustoffe mit Namen (für Verweise und Befundsätze): die der Datei und
    /// die Werksbaustoffe, die sie nicht führt (gelöscht, kein Fehler).
    pub materialien: HashMap<Guid, String>,
    /// Art der Baustoffe der Datei.
    pub kategorien: HashMap<Guid, sk_model::library::MatCategory>,
    /// Gewerke der Datei und des Startbestands (`trade::merge`).
    pub gewerke: HashSet<Guid>,
    /// Werksbaustoffe in älteren Dateien (R73-W, Bausteingrenze §6):
    /// Werks-Guid → Guid und Name des einen Baustoffs der Datei mit gleicher
    /// Kategorie und gleichem oder altem Namen.
    pub uebersetzung: HashMap<Guid, (Guid, String)>,
}

impl Umfeld {
    pub fn aus_modell(m: &sk_model::Model) -> Umfeld {
        Umfeld::neu(
            m.materials().iter().map(|(_, x)| x),
            m.trades().iter().map(|t| t.guid),
        )
    }

    pub fn aus_bibliothek(lib: &sk_model::Library) -> Umfeld {
        Umfeld::neu(
            lib.materials.iter().map(|(_, x)| x),
            lib.trades.iter().map(|t| t.guid),
        )
    }

    fn neu<'a>(
        mats: impl Iterator<Item = &'a sk_model::library::Material>,
        gewerke: impl Iterator<Item = Guid>,
    ) -> Umfeld {
        let mut g: HashSet<Guid> = gewerke.collect();
        g.extend(sk_model::trade::start_trades().iter().map(|t| t.guid));
        let mats: Vec<_> = mats.collect();
        let mut materialien: HashMap<Guid, String> =
            mats.iter().map(|x| (x.guid, x.name.clone())).collect();
        let kategorien = mats.iter().map(|x| (x.guid, x.category)).collect();
        let mut uebersetzung = HashMap::new();
        let start = sk_model::Model::new();
        for (_, w) in start.materials().iter() {
            if materialien.contains_key(&w.guid) {
                continue;
            }
            let mut treffer = mats
                .iter()
                .filter(|x| x.category == w.category && sk_model::szo::werksname(&x.name, &w.name));
            match (treffer.next(), treffer.next()) {
                // genau einer: übersetzen
                (Some(x), None) => {
                    uebersetzung.insert(w.guid, (x.guid, x.name.clone()));
                }
                // keiner: gelöscht, die Werkssätze greifen an keiner Schicht
                (None, _) => {
                    materialien.insert(w.guid, w.name.clone());
                }
                // mehrere: bleibt ein toter Verweis (Regel 73)
                _ => {}
            }
        }
        Umfeld {
            materialien,
            kategorien,
            gewerke: g,
            uebersetzung,
        }
    }

    /// Kennt der Leser den Baustoff `g` (Regel 73)?
    fn kennt(&self, g: Guid) -> bool {
        self.materialien.contains_key(&g) || self.uebersetzung.contains_key(&g)
    }

    /// Baustoff der Datei zu `mat=` (R73-W übersetzt Werks-Guids).
    fn mat(&self, g: Option<Guid>) -> Option<Guid> {
        g.map(|g| self.uebersetzung.get(&g).map_or(g, |x| x.0))
    }

    fn baustoff(&self, g: Guid) -> String {
        self.materialien
            .get(&g)
            .cloned()
            .unwrap_or_else(|| crate::wort::EIN_EINTRAG.to_string())
    }
}

/// Name eines Satzes für Befundsätze: Name, Kurztext, Firmenwert oder „ein
/// Eintrag“, nie die Kennung.
fn satz_name(s: &Satz) -> String {
    if let Some(n) = s.text("name").or(s.text("short")).filter(|n| !n.is_empty()) {
        return n.to_string();
    }
    match (s.abschnitt.name, s.text("key")) {
        ("rate", Some(k)) => crate::wort::firmenwert(k),
        _ => crate::wort::EIN_EINTRAG.to_string(),
    }
}

/// Kurztext einer Bauleistung, sonst „einem Eintrag“ (steht hinter „von“).
fn kurz_von(k: &Katalog, g: Guid) -> String {
    k.leistung(g)
        .map_or_else(|| "einem Eintrag".to_string(), |l| l.kurz.clone())
}

/// Name des Eintrags hinter einer Herkunftsangabe (`rec`, `key`).
fn eintrag_name(k: &Katalog, rec: &str, key: &str) -> String {
    let g = Guid::from_ifc(key);
    let name = match rec {
        "rate" => Some(crate::wort::firmenwert(key)),
        "article" => g.and_then(|g| k.artikel(g)).map(|a| a.name.clone()),
        "service" => g.and_then(|g| k.leistung(g)).map(|l| l.kurz.clone()),
        "lot" => g.and_then(|g| k.los(g)).map(|l| l.name.clone()),
        "svcpart" => g
            .and_then(|g| k.anteile.iter().find(|a| a.guid == g))
            .map(|a| format!("Stoffanteil von {}", kurz_von(k, a.leistung))),
        "svcfollow" => g
            .and_then(|g| k.folgen.iter().find(|f| f.guid == g))
            .map(|f| format!("Folgeposition von {}", kurz_von(k, f.leistung))),
        _ => None,
    };
    name.unwrap_or_else(|| crate::wort::EIN_EINTRAG.to_string())
}

/// Gelesene, für sich gültige Zeile mit ihrer Stelle im Abschnitt.
struct Roh {
    n: usize,
    satz: Satz,
}

fn mm(d: Dez) -> String {
    format!("{} mm", d.text())
}

/// Liest Kostenzeilen `(abschnitt, zeile)` (Regeln 72–80, 85–91). Fremde
/// Abschnitte übergeht der Leser; `quelle` setzt der Aufrufer.
pub fn lesen<'a>(
    zeilen: impl IntoIterator<Item = (&'a str, &'a str)>,
    u: &Umfeld,
    quelle: Quelle,
) -> Katalog {
    let mut bf: Vec<Befund> = Vec::new();
    let mut roh: HashMap<&'static str, Vec<Roh>> = HashMap::new();
    let mut zaehler: HashMap<&'static str, usize> = HashMap::new();
    let mut fremd = false;
    let mut stempel = FNV_START;
    for (sec, line) in zeilen {
        let Some(a) = satz::abschnitt(sec) else {
            continue;
        };
        stempel = fnv(fnv(stempel, line.as_bytes()), b"\n");
        let n = {
            let c = zaehler.entry(a.name).or_default();
            *c += 1;
            *c
        };
        let ort = || {
            satz_ort(
                a.name,
                sk_model::ext::rec_id(line).unwrap_or(format!("Zeile {n}")),
            )
        };
        let Some(z) = zeile::zerlegen(line) else {
            bf.push(Befund::fehler(
                72,
                befund::r72(n, a.name, "nicht lesbar"),
                ort(),
            ));
            continue;
        };
        match Satz::lesen(a, &z) {
            Ok(s) => {
                fremd |= !s.fremd.is_empty();
                roh.entry(a.name).or_default().push(Roh { n, satz: s });
            }
            Err(e) => bf.push(Befund::fehler(72, befund::r72(n, a.name, &e.grund), ort())),
        }
    }
    if fremd {
        bf.push(Befund::hinweis(71, befund::r71(), Ort::Datei));
    }
    // Regel 74: erste Zeile je Kennung; [catalog] und [costproject] einmal
    for (name, liste) in roh.iter_mut() {
        let einmal = matches!(*name, "catalog" | "costproject");
        let mut seen = HashSet::new();
        liste.retain(|r| {
            let k = r.satz.kennung().unwrap_or_default();
            let neu = if einmal {
                seen.is_empty()
            } else {
                !seen.contains(&k)
            };
            if neu {
                seen.insert(k);
            } else {
                bf.push(Befund::fehler(
                    74,
                    befund::r74(name, &satz_name(&r.satz)),
                    satz_ort(name, k.clone()),
                ));
            }
            neu
        });
    }
    let mut take = |name: &str| roh.remove(name).unwrap_or_default();
    let mut k = Katalog {
        quelle,
        artikel: Vec::new(),
        leistungen: Vec::new(),
        anteile: Vec::new(),
        folgen: Vec::new(),
        lose: Vec::new(),
        werte: Firmenwerte::werk(),
        herkunft: Vec::new(),
        kopf: None,
        kopie: None,
        protokoll: Vec::new(),
        vorschlaege: Vec::new(),
        firma_stand: None,
        befunde: Vec::new(),
        stempel: {
            // R73-W: die Übersetzung gehört zum Stempel (Bausteingrenze §6)
            let mut paare: Vec<(Guid, Guid)> =
                u.uebersetzung.iter().map(|(w, x)| (*w, x.0)).collect();
            paare.sort();
            paare.iter().fold(stempel, |h, (w, x)| {
                fnv(fnv(h, w.to_ifc().as_bytes()), x.to_ifc().as_bytes())
            })
        },
    };
    let skip = |bf: &mut Vec<Befund>, a: &Abschnitt, r: &Roh, grund: String| {
        let id = r.satz.kennung().unwrap_or_default();
        bf.push(Befund::fehler(
            72,
            befund::r72(r.n, a.name, &grund),
            satz_ort(a.name, id),
        ));
    };

    // Lose und Titel: parent zeigt auf ein Los (nicht auf einen Titel)
    let lose = take("lot");
    let parents: HashMap<Guid, bool> = lose
        .iter()
        .filter_map(|r| Some((r.satz.guid("guid")?, r.satz.guid("parent").is_some())))
        .collect();
    for r in lose {
        let s = &r.satz;
        let guid = s.guid("guid").unwrap();
        let name = s.text("name").unwrap_or_default().to_string();
        if let Some(p) = s.guid("parent") {
            match parents.get(&p) {
                None => {
                    let t = befund::r73(&format!("Titel {name}"), "ein Los, das");
                    bf.push(Befund::fehler(73, t, satz_ort("lot", guid.to_ifc())));
                    skip(&mut bf, &satz::LOT, &r, "das Los fehlt".into());
                    continue;
                }
                Some(true) => {
                    skip(&mut bf, &satz::LOT, &r, "das Los ist ein Titel".into());
                    continue;
                }
                Some(false) => {}
            }
        }
        k.lose.push(Los {
            guid,
            name,
            nr: s.text("nr").unwrap_or_default().to_string(),
            parent: s.guid("parent"),
            pre: s.text("pre").map(str::to_string),
            retired: s.flag("retired"),
            satz: r.satz,
        });
    }

    // Artikel
    for r in take("article") {
        let s = &r.satz;
        let guid = s.guid("guid").unwrap();
        let name = s.text("name").unwrap_or_default().to_string();
        if s.guid("mat").is_some_and(|m| !u.kennt(m)) {
            let t = befund::r73(&format!("Artikel {name}"), "einen Baustoff, den");
            bf.push(Befund::fehler(73, t, satz_ort("article", guid.to_ifc())));
            continue;
        }
        if name.trim().is_empty() {
            let t = befund::r76(&name, "name", "leer");
            bf.push(Befund::fehler(76, t, satz_ort("article", guid.to_ifc())));
            continue;
        }
        let einheit = Einheit::aus(s.text("unit").unwrap_or_default()).unwrap();
        // Regel 76 (conv): ungültig oder bei st gilt es nicht, der Artikel
        // schon; der Wert bleibt stehen
        let conv = match s.wert("conv") {
            Some(Wert::Zahl(d)) if einheit != Einheit::St => Some(*d),
            Some(w) => {
                let roh = w.schreiben(satz::ARTICLE.feld("conv").unwrap().art);
                let t = befund::r76(&name, "conv", &roh.unwrap_or_default());
                bf.push(Befund::warnung(76, t, satz_ort("article", guid.to_ifc())));
                None
            }
            None => None,
        };
        k.artikel.push(Artikel {
            guid,
            name,
            mat: u.mat(s.guid("mat")),
            kategorie: u
                .mat(s.guid("mat"))
                .and_then(|g| u.kategorien.get(&g).copied()),
            t: s.zahl("t"),
            einheit,
            preis: s.zahl("price"),
            conv,
            std: s.flag("std"),
            retired: s.flag("retired"),
            satz: r.satz,
        });
    }
    // Regel 76: Name eindeutig unter den nicht ausgemusterten
    let mut namen: HashMap<String, Guid> = HashMap::new();
    for a in k.artikel.iter().filter(|a| !a.retired) {
        if namen.insert(a.name.clone(), a.guid).is_some() {
            let t = befund::r76(&a.name, "name", "doppelt");
            bf.push(Befund::warnung(76, t, satz_ort("article", a.guid.to_ifc())));
        }
    }
    // Regel 77: ein Standardartikel je (Baustoff, Dicke), die kleinere Guid
    let mut std: HashMap<(Option<Guid>, Option<Dez>), Guid> = HashMap::new();
    for a in k.artikel.iter().filter(|a| a.std) {
        let e = std.entry((a.mat, a.t)).or_insert(a.guid);
        if a.guid < *e {
            *e = a.guid;
        }
    }
    let namen: HashMap<Guid, String> = k.artikel.iter().map(|a| (a.guid, a.name.clone())).collect();
    for a in k.artikel.iter_mut().filter(|a| a.std) {
        let gilt = std[&(a.mat, a.t)];
        if gilt != a.guid {
            a.std = false;
            let b = a.mat.map_or("Hilfsstoff".to_string(), |m| u.baustoff(m));
            let d = a.t.map_or("ohne Dicke".to_string(), mm);
            let name = namen
                .get(&gilt)
                .cloned()
                .unwrap_or_else(|| crate::wort::EIN_EINTRAG.to_string());
            let t = befund::r77(&b, &d, &name);
            bf.push(Befund::warnung(77, t, satz_ort("article", a.guid.to_ifc())));
        }
    }
    let artikel: HashMap<Guid, bool> = k.artikel.iter().map(|a| (a.guid, a.retired)).collect();

    // Bauleistungen
    for r in take("service") {
        let s = &r.satz;
        let guid = s.guid("guid").unwrap();
        let kurz = s.text("short").unwrap_or_default().to_string();
        let ort = || satz_ort("service", guid.to_ifc());
        let wer = format!("Bauleistung {kurz}");
        let trade = s.guid("trade").unwrap();
        if !u.gewerke.contains(&trade) {
            bf.push(Befund::fehler(
                73,
                befund::r73(&wer, "ein Gewerk, das"),
                ort(),
            ));
            continue;
        }
        let title = s.guid("title").unwrap();
        match k.lose.iter().find(|l| l.guid == title) {
            None => {
                bf.push(Befund::fehler(
                    73,
                    befund::r73(&wer, "einen Titel, den"),
                    ort(),
                ));
                continue;
            }
            Some(l) if l.parent.is_none() => {
                let t = befund::r79(&kurz, "title", "Los statt Titel");
                bf.push(Befund::fehler(79, t, ort()));
                continue;
            }
            Some(_) => {}
        }
        if s.guid("mat").is_some_and(|m| !u.kennt(m)) {
            bf.push(Befund::fehler(
                73,
                befund::r73(&wer, "einen Baustoff, den"),
                ort(),
            ));
            continue;
        }
        if kurz.trim().is_empty() {
            bf.push(Befund::fehler(
                79,
                befund::r79(&kurz, "short", "leer"),
                ort(),
            ));
            continue;
        }
        let einheit = Einheit::aus(s.text("unit").unwrap_or_default()).unwrap();
        let bezug = Bezug::aus(s.text("basis").unwrap_or_default()).unwrap();
        if !bezug.passt(einheit) {
            let t = befund::r80(&kurz, einheit.zeichen(), crate::wort::bezug(bezug));
            bf.push(Befund::fehler(80, t, ort()));
            continue;
        }
        let (tmin, tmax) = (s.zahl("tmin"), s.zahl("tmax"));
        if let (Some(a), Some(b)) = (tmin, tmax) {
            if a > b {
                let t = befund::r79(&kurz, "tmin", &format!("{} > {}", a.text(), b.text()));
                bf.push(Befund::fehler(79, t, ort()));
                continue;
            }
        }
        let kg = s.ganz("kg").map(|v| v as u16);
        if let Some(v) = kg.filter(|v| !sk_model::trade::valid_kg(*v)) {
            bf.push(Befund::fehler(
                79,
                befund::r79(&kurz, "kg", &v.to_string()),
                ort(),
            ));
            continue;
        }
        k.leistungen.push(Leistung {
            guid,
            kurz,
            gewerk: trade,
            titel: title,
            pos: s.ganz("pos").unwrap_or(1) as u16,
            einheit,
            bezug,
            stunden: s.zahl("hours").unwrap_or_default(),
            geraet: s.zahl("equip").unwrap_or_default(),
            sonst: s.zahl("other").unwrap_or_default(),
            nu: s.zahl("nu"),
            kg,
            kategorien: s.woerter("cats").to_vec(),
            mat: u.mat(s.guid("mat")),
            tmin,
            tmax,
            funktion: s.text("fn").map(str::to_string),
            retired: s.flag("retired"),
            satz: r.satz,
        });
    }
    let leistungen: HashMap<Guid, bool> =
        k.leistungen.iter().map(|l| (l.guid, l.retired)).collect();

    // Stoffanteile und Folgepositionen: (service, nr) eindeutig
    let mut nrs: HashSet<(&str, Guid, i64)> = HashSet::new();
    for r in take("svcpart") {
        let s = &r.satz;
        let guid = s.guid("guid").unwrap();
        let svc = s.guid("service").unwrap();
        let ort = || satz_ort("svcpart", guid.to_ifc());
        let wer = format!(
            "Stoffanteil {} von {}",
            s.ganz("nr").unwrap_or(0),
            kurz_von(&k, svc)
        );
        if !leistungen.contains_key(&svc) {
            bf.push(Befund::fehler(
                73,
                befund::r73(&wer, "eine Bauleistung, die"),
                ort(),
            ));
            continue;
        }
        let (art, layer) = (s.guid("art"), s.flag("layer"));
        match (art, layer) {
            (Some(_), true) => {
                skip(
                    &mut bf,
                    &satz::SVCPART,
                    &r,
                    "Artikel und Stoff aus der Schicht zugleich".into(),
                );
                continue;
            }
            (None, false) => {
                skip(
                    &mut bf,
                    &satz::SVCPART,
                    &r,
                    "Artikel oder Stoff aus der Schicht fehlt".into(),
                );
                continue;
            }
            (Some(a), false) if !artikel.contains_key(&a) => {
                bf.push(Befund::fehler(
                    73,
                    befund::r73(&wer, "einen Artikel, den"),
                    ort(),
                ));
                continue;
            }
            _ => {}
        }
        let nr = s.ganz("nr").unwrap();
        if !nrs.insert(("svcpart", svc, nr)) {
            let id = format!("{nr} von {}", kurz_von(&k, svc));
            bf.push(Befund::fehler(74, befund::r74("svcpart", &id), ort()));
            continue;
        }
        k.anteile.push(Anteil {
            guid,
            leistung: svc,
            nr: nr as u32,
            artikel: art,
            menge: s.zahl("qty").unwrap(),
            satz: r.satz,
        });
    }
    k.anteile.sort_by_key(|a| (a.leistung, a.nr));
    for r in take("svcfollow") {
        let s = &r.satz;
        let guid = s.guid("guid").unwrap();
        let svc = s.guid("service").unwrap();
        let fol = s.guid("follow").unwrap();
        let ort = || satz_ort("svcfollow", guid.to_ifc());
        let wer = format!("Folgeposition von {}", kurz_von(&k, svc));
        if [svc, fol].iter().any(|g| !leistungen.contains_key(g)) {
            bf.push(Befund::fehler(
                73,
                befund::r73(&wer, "eine Bauleistung, die"),
                ort(),
            ));
            continue;
        }
        let nr = s.ganz("nr").unwrap();
        if !nrs.insert(("svcfollow", svc, nr)) {
            let id = format!("{nr} von {}", kurz_von(&k, svc));
            bf.push(Befund::fehler(74, befund::r74("svcfollow", &id), ort()));
            continue;
        }
        k.folgen.push(Folge {
            guid,
            leistung: svc,
            nr: nr as u32,
            folge: fol,
            faktor: s.zahl("factor").unwrap_or(Dez::EINS),
            satz: r.satz,
        });
    }
    k.folgen.sort_by_key(|f| (f.leistung, f.nr));
    // Regel 85: eine Ebene, keine Kette, kein Zyklus
    let ausloeser: HashSet<Guid> = k.folgen.iter().map(|f| f.leistung).collect();
    for f in &k.folgen {
        if ausloeser.contains(&f.folge) || f.folge == f.leistung {
            let kurz = k.leistung(f.leistung).map_or("", |l| l.kurz.as_str());
            let folge = k.leistung(f.folge).map_or("", |l| l.kurz.as_str());
            let t = befund::r85_kette(kurz, folge);
            bf.push(Befund::fehler(
                85,
                t,
                satz_ort("svcfollow", f.guid.to_ifc()),
            ));
        }
    }

    // Firmenwerte
    let mut raten: HashSet<String> = HashSet::new();
    for r in take("rate") {
        let key = r.satz.text("key").unwrap_or_default().to_string();
        let num = r.satz.zahl("num").unwrap();
        match rate_bereich(&key) {
            Some((lo, hi)) if num < Dez::ganz(lo) || num > Dez::ganz(hi) => {
                skip(
                    &mut bf,
                    &satz::RATE,
                    &r,
                    format!(
                        "{} {} liegt nicht zwischen {lo} und {hi}",
                        crate::wort::firmenwert(&key),
                        num.text().replace('.', ",")
                    ),
                );
                continue;
            }
            Some(_) => k.werte.setzen(&key, num),
            None => {}
        }
        raten.insert(key);
    }

    // Herkunft (Regel 88)
    for r in take("origin") {
        let s = &r.satz;
        let key = s.text("key").unwrap_or_default().to_string();
        let rec = s.text("rec").unwrap_or_default().to_string();
        let da = match rec.as_str() {
            "rate" => raten.contains(&key),
            _ => Guid::from_ifc(&key).is_some_and(|g| match rec.as_str() {
                "article" => artikel.contains_key(&g),
                "service" => leistungen.contains_key(&g),
                "svcpart" => k.anteile.iter().any(|a| a.guid == g),
                "svcfollow" => k.folgen.iter().any(|f| f.guid == g),
                "lot" => k.lose.iter().any(|l| l.guid == g),
                _ => false,
            }),
        };
        if !da {
            bf.push(Befund::fehler(88, befund::r88(), satz_ort("origin", key)));
            continue;
        }
        let kind = s.text("kind").unwrap_or_default().to_string();
        let offen = s.text("status") == Some("open");
        if offen && kind == "manual" {
            bf.push(Befund::hinweis(
                88,
                befund::r88_hand(&eintrag_name(&k, &rec, &key)),
                satz_ort("origin", key.clone()),
            ));
        }
        k.herkunft.push(Ursprung {
            key,
            rec,
            bestaetigt: !offen || kind == "manual" || kind == "factory",
            kind,
            proj: s.flag("proj"),
            satz: r.satz,
        });
    }

    // Kopf, Kopie, Protokoll
    if let Some(r) = take("catalog").into_iter().next() {
        let s = &r.satz;
        let kopf = Kopf {
            guid: s.guid("guid").unwrap(),
            name: s.text("name").unwrap_or_default().to_string(),
            stand: s.ganz("stand").unwrap_or(0) as u32,
            entwurf: s.text("status") == Some("draft"),
            satz: r.satz,
        };
        if kopf.entwurf {
            bf.push(Befund::warnung(91, befund::r91(), Ort::Datei));
        }
        k.kopf = Some(kopf);
    }
    if let Some(r) = take("costproject").into_iter().next() {
        let s = &r.satz;
        k.kopie = Some(Kopie {
            katalog: s.guid("catalog"),
            stand: s.ganz("stand").map(|v| v as u32),
            lvstorey: s.flag("lvstorey"),
            keep: s.ganz("keep").map(|v| v as u32),
            satz: r.satz,
        });
    }
    let stand = k.kopf.as_ref().map_or(0, |c| c.stand);
    for (i, r) in take("log").into_iter().enumerate() {
        let key = r.satz.ganz("key").unwrap() as u32;
        let st = r.satz.ganz("stand").unwrap() as u32;
        if key as usize != i + 1 || st > stand {
            let t = befund::r90(&key.to_string());
            bf.push(Befund::fehler(90, t, satz_ort("log", key.to_string())));
        }
        k.protokoll.push(Protokoll {
            key,
            stand: st,
            satz: r.satz,
        });
    }

    // Regel 105: Vorschläge nur im Entwurf, sonst übergangen
    let vorschlaege = take("proposal");
    if k.kopf.as_ref().is_some_and(|c| c.entwurf) {
        for r in vorschlaege {
            let s = r.satz;
            let text = |f: &str| s.text(f).unwrap_or_default().to_string();
            k.vorschlaege.push(Vorschlag {
                key: s.ganz("key").unwrap_or(0) as u32,
                projekt: s.guid("project").unwrap_or(Guid(0)),
                name: text("name"),
                rec: text("rec"),
                of: text("of"),
                feld: text("field"),
                alt: s.text("old").map(str::to_string),
                neu: text("new"),
                datum: text("date"),
                satz: s,
            });
        }
    } else if !vorschlaege.is_empty() {
        bf.push(Befund::warnung(
            105,
            befund::r105(vorschlaege.len()),
            Ort::Datei,
        ));
    }

    // Regel 86: OZ fest und eindeutig
    let mut los_nr: HashSet<(Option<Guid>, &str)> = HashSet::new();
    for l in k.lose.iter().filter(|l| !l.retired) {
        if !los_nr.insert((l.parent, l.nr.as_str())) {
            let oz = match l.parent.and_then(|p| k.los(p)) {
                Some(p) => format!("{}.{}", p.nr, l.nr),
                None => l.nr.clone(),
            };
            bf.push(Befund::fehler(
                86,
                befund::r86(&oz),
                satz_ort("lot", l.guid.to_ifc()),
            ));
        }
    }
    let mut oz: HashSet<(Guid, u16)> = HashSet::new();
    for l in k.leistungen.iter().filter(|l| !l.retired) {
        if !oz.insert((l.titel, l.pos)) {
            let t = befund::r86(&k.oz_voll(l));
            bf.push(Befund::fehler(86, t, satz_ort("service", l.guid.to_ifc())));
        }
    }
    // Regel 87: Ausgemustertes, auf das Gültiges verweist
    let mut alt: Vec<(String, &'static str, Guid)> = Vec::new();
    for l in k.leistungen.iter().filter(|l| !l.retired) {
        if k.los(l.titel).is_some_and(|t| t.retired) {
            alt.push((k.los(l.titel).unwrap().name.clone(), "lot", l.titel));
        }
        for a in k.anteile_von(l.guid) {
            if let Some(x) = a.artikel.and_then(|g| k.artikel(g)).filter(|x| x.retired) {
                alt.push((x.name.clone(), "article", x.guid));
            }
        }
        for f in k.folgen_von(l.guid) {
            if let Some(x) = k.leistung(f.folge).filter(|x| x.retired) {
                alt.push((x.kurz.clone(), "service", x.guid));
            }
        }
    }
    alt.sort_by_key(|a| a.2);
    alt.dedup_by_key(|a| a.2);
    for (name, rec, g) in alt {
        bf.push(Befund::hinweis(
            87,
            befund::r87(&name),
            satz_ort(rec, g.to_ifc()),
        ));
    }
    // R73-W: je übersetztem Baustoff ein leiser Hinweis
    let mut genutzt: Vec<&(Guid, String)> = u
        .uebersetzung
        .values()
        .filter(|(g, _)| {
            let m = Some(*g);
            k.artikel.iter().any(|a| a.mat == m) || k.leistungen.iter().any(|l| l.mat == m)
        })
        .collect();
    genutzt.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    for (_, name) in genutzt {
        bf.push(Befund::hinweis(73, befund::r73w(name), Ort::Datei));
    }
    k.befunde = bf;
    k
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::Model;

    fn werk() -> Katalog {
        let m = Model::new();
        let z = crate::werk_zeilen();
        lesen(
            z.iter().map(|(a, l)| (*a, *l)),
            &Umfeld::aus_modell(&m),
            Quelle::Werk {
                stand: "10/2026".into(),
            },
        )
    }

    /// Abnahme 8 (Teil Lesen): Der Werksbestand liest sich ohne Befund.
    #[test]
    fn werk_ohne_befund() {
        let k = werk();
        assert!(k.befunde.is_empty(), "{:#?}", k.befunde);
        assert_eq!(k.leistungen.len(), 22);
        assert_eq!(k.artikel.len(), 19);
        assert_eq!(k.lose.len(), 9);
        assert_eq!(k.anteile.len(), 28);
        assert_eq!(k.folgen.len(), 7);
        assert_eq!(k.werte, Firmenwerte::werk());
        assert_eq!(k.kopf.as_ref().map(|c| c.stand), Some(7));
        assert!(k.herkunft.iter().all(|h| h.bestaetigt));
        let m10 = k
            .leistungen
            .iter()
            .find(|l| {
                l.kurz
                    .starts_with("AW Porenbeton-Planstein PP2-0,35 d=17,5cm")
            })
            .unwrap();
        assert_eq!(k.oz(m10), "02.0010");
        assert_eq!(k.oz_voll(m10), "1.02.0010");
        assert_eq!(m10.stunden, Dez(450_000));
        assert_eq!(k.anteile_von(m10.guid).count(), 2);
        // Stand 7: Stück je Einheit an den Steinen
        let stein = k
            .artikel(Guid::from_ifc("1S7bUW0010080100000002").unwrap())
            .unwrap();
        assert_eq!(stein.conv, Some(Dez(6_670_000)));
        let nf = k
            .artikel(Guid::from_ifc("1S7bUW001008010000000B").unwrap())
            .unwrap();
        assert_eq!(nf.conv, Some(Dez::ganz(48)));
    }

    /// Regel 76 (conv, BIM §3.2): Ein ungültiges `conv` macht den Artikel
    /// nicht ungültig. Es gilt nicht, gibt Befund 76 und bleibt stehen.
    #[test]
    fn conv_ungueltig_gilt_nicht() {
        let g = "0000000000000000000A01";
        for (unit, roh) in [
            ("m2", "abc"),
            ("m2", "0"),
            ("m2", "-1"),
            ("m2", "10000.1"),
            ("m2", "1.23456"),
            ("m2", "\"\""),
            ("st", "2"),
        ] {
            let l = format!("[article] guid={g} name=\"Probe\" unit={unit} price=2 conv={roh}");
            let k = mit(&[("article", &l)]);
            let a = k.artikel(Guid::from_ifc(g).unwrap()).expect(roh);
            assert_eq!(a.conv, None, "{roh}");
            let b: Vec<_> = k.befunde.iter().filter(|b| b.regel == 76).collect();
            assert_eq!(b.len(), 1, "{roh}: {:#?}", k.befunde);
            assert_eq!(b[0].schwere, crate::Schwere::Warnung);
            // der Wert wie in der Datei (leer als "")
            assert_eq!(
                b[0].satz,
                format!("Artikel Probe: Umrechnung ist ungültig ({roh}).")
            );
            // geschrieben wie gelesen
            assert_eq!(a.satz.zeile(), l, "{roh}");
        }
        let l = format!("[article] guid={g} name=\"Probe\" unit=m2 price=2 conv=0.5");
        let k = mit(&[("article", &l)]);
        assert_eq!(
            k.artikel(Guid::from_ifc(g).unwrap()).unwrap().conv,
            Some(Dez(500_000))
        );
        assert!(k.befunde.is_empty(), "{:#?}", k.befunde);
    }

    fn mit(zeilen: &[(&str, &str)]) -> Katalog {
        let m = Model::new();
        let mut z = crate::werk_zeilen();
        z.extend(zeilen.iter().copied());
        lesen(
            z.iter().map(|(a, l)| (*a, *l)),
            &Umfeld::aus_modell(&m),
            Quelle::Werk {
                stand: "10/2026".into(),
            },
        )
    }

    fn regeln(k: &Katalog) -> Vec<u16> {
        k.befunde.iter().map(|b| b.regel).collect()
    }

    #[test]
    fn regeln_beim_lesen() {
        // 72 ungültig, 74 doppelt, 73 tot, 80 Einheit, 88 ohne Satz
        let k = mit(&[
            ("article", "[article] guid=0000000000000000000001 name=\"x\" unit=Banane"),
            ("rate", "[rate] key=wage num=65"),
            ("rate", "[rate] key=wage num=70"),
            ("rate", "[rate] key=zukunft num=1"),
            ("rate", "[rate] key=vat num=101"),
            ("article", "[article] guid=0000000000000000000002 name=\"y\" mat=0000000000000000000009 unit=m2"),
            ("service", "[service] guid=0000000000000000000003 short=\"s\" trade=1S7Wf_00100800000004UR title=1S7bUW0010080300000002 pos=99 unit=m2 basis=volume"),
            ("origin", "[origin] key=0000000000000000000002 rec=article kind=manual status=open date=2026-10-08"),
            ("svcpart", "[svcpart] guid=0000000000000000000004 service=1S7bUW0010080200000001 nr=1 layer=1 qty=1"),
        ]);
        let r = regeln(&k);
        for n in [72, 73, 74, 80, 88] {
            assert!(r.contains(&n), "{n}: {:#?}", k.befunde);
        }
        // der erste Lohn (Werk) gilt, unbekannter Schlüssel ohne Befund
        assert_eq!(k.werte.lohn, Dez::ganz(60));
        assert_eq!(k.werte.mwst, Dez::ganz(19));
        // 74 für den doppelten Stoffanteil (service, nr)
        assert!(k
            .befunde
            .iter()
            .any(|b| b.regel == 74 && b.satz.starts_with("Stoffanteil 1 von ")));
        // Sätze tragen die Wörter aus BIM §4
        assert!(k.befunde.iter().any(|b| b.satz
            == "Firmenwert Verrechnungslohn kommt doppelt vor; es gilt der erste Eintrag."));
    }

    #[test]
    fn standardartikel_und_oz() {
        let k = mit(&[
            ("article", "[article] guid=0000000000000000000001 name=\"zweiter\" mat=2wuC33GkTD9Qack6WJ4EsM t=175 unit=m2 price=1 std=1"),
            ("service", "[service] guid=0000000000000000000003 short=\"s\" trade=1S7Wf_00100800000004UR title=1S7bUW0010080300000002 pos=10 unit=m3 basis=volume"),
            ("lot", "[lot] guid=0000000000000000000005 name=\"x\" nr=\"01\" parent=1S7bUW0010080300000001"),
        ]);
        let r = regeln(&k);
        assert_eq!(
            r.iter().filter(|n| **n == 77).count(),
            1,
            "{:#?}",
            k.befunde
        );
        // die kleinere Guid gilt
        assert!(
            k.artikel(Guid::from_ifc("0000000000000000000001").unwrap())
                .unwrap()
                .std
        );
        assert!(
            !k.artikel(Guid::from_ifc("1S7bUW0010080100000002").unwrap())
                .unwrap()
                .std
        );
        assert_eq!(
            r.iter().filter(|n| **n == 86).count(),
            2,
            "{:#?}",
            k.befunde
        );
    }

    #[test]
    fn protokoll_und_entwurf() {
        let k = mit(&[
            ("log", "[log] key=1 stand=1 time=2026-10-08T07:00 role=admin op=preis_setzen rec=rate of=wage"),
            ("log", "[log] key=3 stand=9 time=2026-10-08T07:00 role=admin op=preis_setzen rec=rate of=wage"),
        ]);
        assert_eq!(regeln(&k), vec![90]);
        let z = "[catalog] guid=0000000000000000000001 name=\"F\" stand=0 status=draft";
        let k = lesen(
            [("catalog", z)],
            &Umfeld::aus_modell(&Model::new()),
            Quelle::Werk {
                stand: String::new(),
            },
        );
        assert_eq!(regeln(&k), vec![91]);
    }
}

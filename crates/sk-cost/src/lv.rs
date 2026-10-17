//! Leistungsverzeichnis eines Loses (KA-4a, architektur/paket-ka4.md §3,
//! kosten/ka-4-fach.md §3.1–§3.5). Grundlage ist das Kostenblatt desselben
//! Umfangs: Das LV rechnet keine Menge und keinen Preis neu, es ordnet nur
//! um. Geschätzte Zeilen und Zeilen ohne Bauleistung stehen in keinem LV;
//! sie erscheinen im Prüfen, die geschätzten auch in der Zusammenstellung.

use crate::befund::{Befund, Ort, Schwere};
use crate::geld::{runden, Cent, Dez};
use crate::katalog::{Einheit, Katalog, Leistung};
use crate::rechnung::OhneHerkunft;
use crate::rechnung::{drei, gp, skala, Ansatz, Kostenblatt, Position, Quelle};
use sk_model::{ElementId, Guid, Model, StoreyId};
use std::collections::{HashMap, HashSet};

/// Was der Nutzer für das LV wählt: das Los, „Geschosse als Untertitel“
/// (`[costproject] lvstorey=1`) und „Mit Preisen“ statt „Für Anfrage
/// (leer)“. `heute` (Jahr, Monat) ist der Tag, gegen den „Preisstand älter
/// als 12 Monate“ prüft; `None` prüft das nicht (Tests setzen ihn fest).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LvWahl {
    pub los: Guid,
    pub untertitel: bool,
    pub preise: bool,
    pub heute: Option<(u16, u8)>,
}

/// Positionsart (ka-4-fach §3.1): jetzt nur Normalposition; Bedarfs- und
/// Wahlpositionen sind vorgemerkt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Positionsart {
    Normal,
}

impl Positionsart {
    pub fn name(self) -> &'static str {
        match self {
            Positionsart::Normal => "Normalposition",
        }
    }
}

/// Vorbemerkung, wenn Erdmengen aus dem Geländemodell kommen
/// (vob/pruefung-gelaende-g2.md Frage 1): Die Vordersätze sind ermittelt,
/// abgerechnet wird nach Aufmaß.
pub const VORBEMERKUNG_NETZ: &str = "Die Vordersätze der Erdarbeiten sind aus dem digitalen \
    Geländemodell des Höhenaufmaßes ermittelt (Dreiecksprismen, REB-VB 22.013). Abgerechnet wird \
    nach DIN 18300 Abschnitt 5 auf Grundlage einer gemeinsamen Geländeaufnahme vor Beginn der \
    Arbeiten; ein DGM aus dieser Aufnahme ist als Ermittlungsverfahren vereinbart.";

/// Vorbemerkung beim angenommenen Gelände (I1, VOB-Spruch zu I1): Die
/// Höhen sind nicht vermessen, das gemeinsame Aufmaß vor Beginn gilt. `{}`
/// ist die Dreieckszahl.
pub const VORBEMERKUNG_ANGENOMMEN: &str = "Die Mengen der Erdarbeiten sind aus einem \
    angenommenen Gelände ermittelt (Höhen nicht vermessen, Dreiecksprismen nach REB-VB 22.013, \
    {} Dreiecke). Die Höhen des Urgeländes werden vor Beginn der Arbeiten gemeinsam \
    aufgenommen; abgerechnet wird nach diesem Aufmaß.";

/// [`VORBEMERKUNG_ANGENOMMEN`] mit der Dreieckszahl `n` („1.834“).
pub fn vorbemerkung_angenommen(n: &str) -> String {
    VORBEMERKUNG_ANGENOMMEN.replacen("{}", n, 1)
}

/// [`VORBEMERKUNG_NETZ`] mit der Herkunft des Netzes in der Klammer
/// („Höhenaufmaß x.csv vom 12.10.2026, 1.834 Dreiecke“,
/// [`sk_model::qto_earth::netz_herkunft`]): Die Ansätze nennen die
/// Dreieckszahl nicht mehr je Zeile, der Prüfer des AG sieht sie hier
/// einmal (VOB-Spruch zu A2 vom 15.10.).
pub fn vorbemerkung_netz(herkunft: Option<&str>) -> String {
    match herkunft {
        Some(h) => VORBEMERKUNG_NETZ.replacen(
            "(Dreiecksprismen, REB-VB 22.013)",
            &format!("(Dreiecksprismen, REB-VB 22.013; {h})"),
            1,
        ),
        None => VORBEMERKUNG_NETZ.to_string(),
    }
}

/// Kopf des LV (ka-4-fach §3.2). Umfang und Datum setzt die Ansicht (Uhr
/// und Kopfzeile aus KA-1), ebenso den Dateinamen, wenn `bauvorhaben` leer
/// ist ([`bauvorhaben_aus_datei`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LvKopf {
    /// `Project.site`.
    pub bauvorhaben: Option<String>,
    /// `Project.kind` („Neubau Einfamilienhaus“, Paket PD-3).
    pub projektart: Option<String>,
    /// `Project.place`, Zeilen mit `\n` (Paket PD-3).
    pub bauort: Option<String>,
    /// `Project.number` („01/26“, Paket PD-3).
    pub projektnummer: Option<String>,
    /// `Project.client`.
    pub bauherr: Option<String>,
    /// `Project.client_addr`, Zeilen mit `\n` (Paket PD-3).
    pub bauherr_anschrift: Option<String>,
    /// `Project.author`, sonst der Name des Firmenkatalogs.
    pub aufsteller: Option<String>,
    /// `Project.author_addr`, nur zum Verfasser im Projekt (Paket PD-3).
    pub aufsteller_anschrift: Option<String>,
    pub los: String,
    pub los_nr: String,
    /// „Anfrage ohne Preise“ oder „mit Preisen“.
    pub art: &'static str,
    pub waehrung: &'static str,
    pub netto: &'static str,
    /// `[lot] pre=` des Loses; `None` druckt keine Vorbemerkungen.
    pub vorbemerkungen: Option<String>,
}

/// Eine Zeile im Mengenansatz einer LV-Position: Geschoss, Bauteilnummern,
/// Herkunft und Menge auf 3 Stellen. Die Zeilen einer Position ergeben ihre
/// Menge (größter Rest, ka-4-fach §3.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ansatzzeile {
    pub geschoss: StoreyId,
    pub nummern: Vec<String>,
    pub elemente: Vec<ElementId>,
    /// „aus Modell“, „aus 01.0030 Stb-Decke …“ oder „− Auflager“.
    pub herkunft: String,
    pub menge: Dez,
}

/// Preisanteile einer Position (Detail „Preisanteile“, nur mit Preisen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anteile {
    pub lohn: Cent,
    pub stoff: Cent,
    pub geraet: Cent,
    pub sonst: Cent,
    pub nu: Option<Cent>,
    /// Zeitansatz (h je Einheit).
    pub stunden: Dez,
}

/// Eine Position im LV.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LvPosition {
    /// OZ im LV des Loses: `TT.PPPP`, mit Untertitel `TT.UU.PPPP`.
    pub oz: String,
    pub kurztext: String,
    pub menge: Dez,
    pub einheit: Einheit,
    /// `None` bei „Für Anfrage (leer)“ und bei mehreren Preisen (K12).
    pub ep: Option<Cent>,
    pub gp: Option<Cent>,
    pub ansatz: Vec<Ansatzzeile>,
    /// Die Bauleistung.
    pub quelle: Guid,
    /// Untertitel (`UU`) bei „Geschosse als Untertitel“.
    pub untertitel: Option<u32>,
    pub art: Positionsart,
    pub gewerk: Guid,
    pub kg: Option<u16>,
    /// Bauteile mit verschiedenen KG (B60 an Sohlplatte und Decke): Menge
    /// je KG, nach KG; sonst leer.
    pub kg_teile: Vec<(u16, Dez)>,
    pub anteile: Option<Anteile>,
    /// Ein Artikel ohne Preis (Punkt in Akzent).
    pub preis_fehlt: bool,
    /// Mehrere Stoff-EP (K12, Regel 101).
    pub mehrere_preise: bool,
    /// Zeilen des Kostenblatts hinter der Position (Preisblatt).
    pub blatt: Vec<usize>,
    /// Teilposition je Dicke (R1-B3): Index an der OZ und Dicke.
    pub dicke: Option<(u32, Dez)>,
}

/// Ein Untertitel (Geschoss) in einem Titel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LvUntertitel {
    pub nr: u32,
    /// „01.02“.
    pub oz: String,
    pub name: String,
    pub summe: Option<Cent>,
}

/// Ein Titel des Loses. Leere Titel bleiben in der Liste (Baum grau mit 0);
/// Tabelle, Zusammenstellung und CSV lassen sie weg.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LvTitel {
    pub guid: Guid,
    pub nr: String,
    pub name: String,
    pub positionen: Vec<LvPosition>,
    pub untertitel: Vec<LvUntertitel>,
    /// Nur mit Preisen; bei `unvollstaendig` die Summe der bekannten GP.
    pub summe: Option<Cent>,
    /// Eine Position ohne Preis oder mit mehreren Preisen.
    pub unvollstaendig: bool,
}

/// Zusammenstellung (ka-4-fach §3.4): eine Zeile je Titel mit Positionen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zusammenstellung {
    /// OZ des Titels, Name, Summe (`None` ohne Preise).
    pub zeilen: Vec<(String, String, Option<Cent>)>,
    pub netto: Option<Cent>,
    pub mwst_satz: Dez,
    pub mwst: Option<Cent>,
    pub brutto: Option<Cent>,
    /// „nicht ausgeschrieben (geschätzt): x €“; nur mit Preisen, zählt
    /// nicht in die Summe.
    pub geschaetzt: Option<Cent>,
    /// „ohne Los, nicht in der Summe: x €“: Positionen aus Erweiterungen,
    /// deren Gewerk in keinem Titel steht (A8); nur mit Preisen. Mit den
    /// Losen ergibt sie das Netto des Kostenblatts.
    pub ohne_los: Option<Cent>,
    pub unvollstaendig: bool,
}

/// Das LV eines Loses im Umfang.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lv {
    pub kopf: LvKopf,
    pub titel: Vec<LvTitel>,
    pub zusammenstellung: Zusammenstellung,
    pub befunde: Vec<Befund>,
}

impl Lv {
    /// Positionen im Umfang (Karte „LV Rohbau · 7 Pos.“).
    pub fn anzahl(&self) -> usize {
        self.titel.iter().map(|t| t.positionen.len()).sum()
    }

    /// Befunde im Prüfen: (Fehler, Hinweise); Warnungen zählen als Fehler.
    pub fn zaehlen(&self) -> (usize, usize) {
        let h = self
            .befunde
            .iter()
            .filter(|b| b.schwere == Schwere::Hinweis)
            .count();
        (self.befunde.len() - h, h)
    }
}

/// „haus.szo“ → „Haus“: Bauvorhaben ohne `Project.site`.
pub fn bauvorhaben_aus_datei(datei: &str) -> String {
    let name = datei.rsplit(['/', '\\']).next().unwrap_or(datei);
    let stamm = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let mut z = stamm.chars();
    match z.next() {
        Some(c) => c.to_uppercase().chain(z).collect(),
        None => String::new(),
    }
}

/// Menge deutsch mit 3 Stellen: „1.234,500“.
fn menge_deutsch(d: Dez) -> String {
    let milli = runden(d.0 as i128, (Dez::SKALA / 1000) as i128);
    let (vz, milli) = if milli < 0 {
        ("-", -milli)
    } else {
        ("", milli)
    };
    let ganz = (milli / 1000).to_string();
    let mut g = String::new();
    for (i, c) in ganz.chars().enumerate() {
        if i > 0 && (ganz.len() - i) % 3 == 0 {
            g.push('.');
        }
        g.push(c);
    }
    format!("{vz}{g},{:03}", milli % 1000)
}

/// Dicke in mm ohne Einheit: „120“, „17,5“.
fn mm(t: Dez) -> String {
    t.text().replace('.', ",")
}

/// Los, zu dem die Bauleistungen eines Gewerks gehören (Richtpreis und
/// Zeilen ohne Bauleistung haben nur ein Gewerk).
fn los_vom_gewerk(k: &Katalog, gewerk: Option<Guid>) -> Option<Guid> {
    let g = gewerk?;
    k.leistungen
        .iter()
        .filter(|l| !l.retired && l.gewerk == g)
        .find_map(|l| k.los(l.titel).and_then(|t| t.parent))
}

/// Los einer Bauleistung.
fn los_der_leistung(k: &Katalog, l: &Leistung) -> Option<Guid> {
    k.los(l.titel).and_then(|t| t.parent)
}

/// Untertitel eines Geschosses: Stelle im Geschossbogen seines Gebäudes
/// (von unten, ab 1) und Name. Damit ist `UU` in jedem Umfang gleich.
fn untertitel_von(m: &Model, st: StoreyId) -> (u32, String) {
    let Some(s) = m.storey(st) else {
        return (0, String::new());
    };
    let mut bogen: Vec<(i64, StoreyId)> = m
        .storeys()
        .iter()
        .filter(|(_, x)| x.building == s.building)
        .map(|(id, x)| ((x.elevation * 1000.0).round() as i64, id))
        .collect();
    bogen.sort_by_key(|(h, _)| *h);
    let nr = bogen
        .iter()
        .position(|(_, id)| *id == st)
        .map_or(0, |i| i + 1);
    // Wie der Chip und der Geschossbogen: „Fundament“ (Bedienbarkeit 1.1,
    // soll-ka-4c), nicht der Name im Modell
    let name = if s.kind == sk_model::LevelKind::Foundation {
        "Fundament".to_string()
    } else {
        s.name.clone()
    };
    (nr as u32, name)
}

/// Zeilen des Mengenansatzes je für sich auf 3 Stellen (ka-0-fach §1.9,
/// Fachprüfung KA-4a P1): Jede Zeile zeigt die Menge, die auch im Mengen-
/// Reiter steht. Weicht ihre Summe von der Positionsmenge ab, gleicht der
/// zweite Wert das aus (sonst 0).
fn einzeln(werte: &[i128], e: Einheit) -> (Vec<Dez>, Dez) {
    let s = skala(e);
    let ziel = runden(werte.iter().sum::<i128>() * 1000, s);
    let milli: Vec<i128> = werte.iter().map(|v| runden(v * 1000, s)).collect();
    let rest = ziel - milli.iter().sum::<i128>();
    let dez = |x: i128| Dez((x * 1000) as i64);
    (milli.into_iter().map(dez).collect(), dez(rest))
}

/// Mengenansatz (ka-4-fach §3.3): je Geschoss und Herkunft die
/// Bauteilnummern; der Abzug des Auflagers als eigene Zeile.
fn mengenansatz(
    k: &Katalog,
    ansatz: &[&Ansatz],
    einheit: Einheit,
    uu: Option<u32>,
) -> Vec<Ansatzzeile> {
    // je Geschoss, Folge-Quelle, Bewehrungsgrad (B16) und Rechenweg einer
    // Automatikmenge
    type Schluessel<'a> = (StoreyId, Option<Guid>, Option<u32>, Option<&'a str>);
    type Gruppe<'a> = (
        StoreyId,
        Option<Guid>,
        Option<u32>,
        Option<&'a str>,
        Vec<&'a Ansatz>,
    );
    // Gruppen in der Reihenfolge ihres ersten Ansatzes; Stelle über den
    // Schlüssel, Nummern und Bauteile über Mengen statt linear (Review
    // #197 H-197i: eine Gruppe je Raum mit eigenem Rechenweg)
    let mut gruppen: Vec<Gruppe> = Vec::new();
    let mut stelle: HashMap<Schluessel, usize> = HashMap::new();
    for a in ansatz {
        let f = a.formel.as_deref();
        let j = *stelle
            .entry((a.geschoss, a.aus, a.grad, f))
            .or_insert_with(|| {
                gruppen.push((a.geschoss, a.aus, a.grad, f, Vec::new()));
                gruppen.len() - 1
            });
        gruppen[j].4.push(a);
    }
    let mut zeilen: Vec<(Ansatzzeile, i128)> = Vec::new();
    for (st, aus, grad, formel, v) in gruppen {
        let mut nummern: Vec<String> = Vec::new();
        let mut elemente: Vec<ElementId> = Vec::new();
        let mut gesehen: (HashSet<&str>, HashSet<ElementId>) = Default::default();
        for a in &v {
            if gesehen.0.insert(&a.nummer) {
                nummern.push(a.nummer.clone());
            }
            if gesehen.1.insert(a.element) {
                elemente.push(a.element);
            }
        }
        let menge: i128 = v.iter().map(|a| a.menge).sum();
        let auflager: i128 = v.iter().map(|a| a.auflager).sum();
        let herkunft = match (aus.and_then(|g| k.leistung(g)), grad, formel) {
            (Some(l), _, _) => format!("aus {} {}", oz_im_los(k, l, uu), l.kurz),
            // Bewehrungsgrad der Erweiterung, nicht der Firmenwerte (B16)
            (None, Some(g), _) => format!("aus Modell ({g} kg/m³ aus der Definition)"),
            (None, None, Some(f)) => f.to_string(),
            (None, None, None) => "aus Modell".to_string(),
        };
        let zeile = |herkunft: String| Ansatzzeile {
            geschoss: st,
            nummern: nummern.clone(),
            elemente: elemente.clone(),
            herkunft,
            menge: Dez::NULL,
        };
        zeilen.push((zeile(herkunft), menge + auflager));
        if auflager != 0 {
            zeilen.push((zeile("− Auflager".to_string()), -auflager));
        }
    }
    let werte: Vec<i128> = zeilen.iter().map(|z| z.1).collect();
    let (mengen, rest) = einzeln(&werte, einheit);
    let mut aus: Vec<Ansatzzeile> = zeilen
        .into_iter()
        .zip(mengen)
        .map(|((z, _), menge)| Ansatzzeile { menge, ..z })
        .collect();
    // Sichtbarer Rundungsausgleich ohne Bauteile, am letzten Geschoss
    if rest != Dez::NULL {
        if let Some(geschoss) = aus.last().map(|z| z.geschoss) {
            aus.push(Ansatzzeile {
                geschoss,
                nummern: Vec::new(),
                elemente: Vec::new(),
                herkunft: "Rundungsausgleich".to_string(),
                menge: rest,
            });
        }
    }
    aus
}

/// OZ im LV eines Loses, mit Untertitel dreistufig.
fn oz_im_los(k: &Katalog, l: &Leistung, uu: Option<u32>) -> String {
    match uu {
        Some(u) => {
            let t = k.los(l.titel).map_or("?", |t| t.nr.as_str());
            format!("{t}.{u:02}.{:04}", l.pos)
        }
        None => k.oz(l),
    }
}

/// Höchster Positionsindex einer Teilposition je Dicke (eine Stelle).
const INDEX_MAX: u32 = 9;

/// Bauleistung, ihre Zeilen im Kostenblatt und, als Teilposition je Dicke,
/// Index und Dicke.
type LvTeil = (Guid, Vec<usize>, Option<(u32, Dez)>);

/// Dicken je Bauleistung an Räumen im ganzen Projekt (R1-B3): Zeilen
/// derselben Bauleistung, die nur an Räumen rechnen und sich nur in der
/// Dicke ihrer Schicht unterscheiden (K12, gleicher Baustoff, jede Zeile
/// eine Dicke, jede Dicke einmal), aufsteigend. Der Index einer
/// Teilposition ist der Rang ihrer Dicke hier, darum in jedem Umfang
/// derselbe (Review #213 B2). Eine neue Dicke dazwischen verschiebt die
/// Indizes dahinter; die OZ nach der Vergabe festzuhalten ist ein eigener
/// Schritt (H-213i).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dickenrang(HashMap<Guid, Vec<Dez>>);

impl Dickenrang {
    /// Aus dem Kostenblatt im Umfang Projekt.
    pub fn aus(m: &Model, projekt: &Kostenblatt) -> Dickenrang {
        // je Bauleistung: Baustoff der ersten Zeile und die Dicken, `None`
        // sobald eine Zeile nicht passt
        let mut je: HashMap<Guid, Option<(Guid, Vec<Dez>)>> = HashMap::new();
        for p in &projekt.positionen {
            let Quelle::Leistung(g) = p.quelle else {
                continue;
            };
            let zeile = match (p.schicht, p.dicken.as_slice()) {
                (Some((bs, _)), [d]) if nur_raeume(m, p) => Some((bs, *d)),
                _ => None,
            };
            let e = je
                .entry(g)
                .or_insert_with(|| zeile.map(|(bs, _)| (bs, Vec::new())));
            match (e.as_mut(), zeile) {
                (Some((bs, ds)), Some((b, d))) if *bs == b && !ds.contains(&d) => ds.push(d),
                _ => *e = None,
            }
        }
        Dickenrang(
            je.into_iter()
                .filter_map(|(g, x)| {
                    let mut ds = x?.1;
                    ds.sort();
                    (ds.len() > 1).then_some((g, ds))
                })
                .collect(),
        )
    }

    /// Dicken der Bauleistung `g` an Räumen, wenn sie sich je Dicke teilt
    /// (auch mehr, als der Index fasst).
    fn dicken(&self, g: Guid) -> Option<&[Dez]> {
        self.0.get(&g).map(|d| d.as_slice())
    }

    /// Index der Dicke `d` von `g` (1 bis [`INDEX_MAX`]), wenn `g` sich
    /// teilt und der Index alle Dicken fasst.
    fn index(&self, g: Guid, d: Dez) -> Option<u32> {
        let ds = self.dicken(g).filter(|ds| ds.len() <= INDEX_MAX as usize)?;
        ds.iter().position(|x| *x == d).map(|i| i as u32 + 1)
    }
}

/// Rechnet die Position nur an Räumen?
fn nur_raeume(m: &Model, p: &Position) -> bool {
    p.ansatz.iter().all(|a| {
        m.element(a.element)
            .is_some_and(|e| matches!(e.kind, sk_model::ElementKind::Space(_)))
    })
}

/// Kostenzeilen je Bauleistung. Teilt sich eine Bauleistung im Projekt je
/// Dicke ([`Dickenrang`]), wird jede ihrer Zeilen eine Teilposition mit
/// dem Index ihrer Dicke, auch wenn der Umfang nur eine Dicke zeigt
/// (DIN 18353 Abschnitt 0.5: Dämmschichten getrennt nach Dicke). Im Raum
/// folgt die Dicke aus dem Raumbuch (die Dämmung gleicht den
/// Fußbodenaufbau aus), eine Bauleistung je Dicke kann niemand anlegen. An
/// Bauteilen ist die Dicke die des Typs; zwei Dicken heißen dort eine zu
/// weite Regel und bleiben Regel 101, ebenso mehr Dicken, als der Index
/// fasst. Eine Dicke im Projekt bleibt ohne Index.
fn je_dicke_teilen(
    positionen: &[Position],
    rang: &Dickenrang,
    je_leistung: Vec<(Guid, Vec<usize>)>,
) -> Vec<LvTeil> {
    let mut out = Vec::new();
    for (g, zeilen) in je_leistung {
        let teile: Option<Vec<(u32, Dez)>> = zeilen
            .iter()
            .map(|i| match positionen[*i].dicken.as_slice() {
                [d] => rang.index(g, *d).map(|n| (n, *d)),
                _ => None,
            })
            .collect();
        match teile {
            Some(t) => {
                let mut je: Vec<((u32, Dez), usize)> = t.into_iter().zip(zeilen).collect();
                je.sort();
                for (t, i) in je {
                    out.push((g, vec![i], Some(t)));
                }
            }
            None => out.push((g, zeilen, None)),
        }
    }
    out
}

/// Kurztext einer Teilposition der Dicke `d` (Review #213 B1): „Dicke nach
/// Raumbuch“ wird „d = 40 mm“, sonst kommt „, d = 40 mm“ dazu.
fn kurz_je_dicke(kurz: &str, d: Dez) -> String {
    let dicke = format!("d = {} mm", mm(d));
    if kurz.contains(DICKE_NACH_RAUMBUCH) {
        kurz.replacen(DICKE_NACH_RAUMBUCH, &dicke, 1)
    } else {
        format!("{kurz}, {dicke}")
    }
}

/// Wendung im Kurztext der Werksleistungen, deren Dicke das Raumbuch gibt.
const DICKE_NACH_RAUMBUCH: &str = "Dicke nach Raumbuch";

/// OZ mit Los davor für Befundsätze („1.01.0020“, paket-ka4 §3).
fn oz_mit_los(los_nr: &str, oz: &str) -> String {
    format!("{los_nr}.{oz}")
}

/// Name des Typs eines Bauteils, sonst der Bauteilart.
fn typname(m: &Model, e: ElementId) -> String {
    m.element(e).map_or(String::new(), |x| {
        x.layer_set.and_then(|t| m.layer_set(t)).map_or_else(
            || sk_model::kinds::spec(x.category).name.to_string(),
            |t| t.name.clone(),
        )
    })
}

fn baustoff_name(m: &Model, g: Guid) -> String {
    m.materials()
        .iter()
        .find(|(_, x)| x.guid == g)
        .map_or(String::new(), |(_, x)| x.name.clone())
}

fn element_guid(m: &Model, e: ElementId) -> Option<Guid> {
    m.element(e).map(|x| x.guid)
}

/// „06/2025“ → (2025, 6).
fn monat_lesen(t: &str) -> Option<(u16, u8)> {
    let (mo, j) = t.split_once('/')?;
    Some((j.parse().ok()?, mo.parse().ok()?))
}

/// Das LV aus dem fertigen Kostenblatt `b` im Umfang Projekt (für die
/// Ansicht, die das Blatt zwischenspeichert; Regel 94: liest nur). Für
/// einen anderen Umfang [`lv_im_umfang`].
pub fn lv_aus(m: &Model, b: &Kostenblatt, k: &Katalog, w: &LvWahl) -> Lv {
    lv_im_umfang(m, b, &Dickenrang::aus(m, b), k, w)
}

/// Das LV aus dem fertigen Kostenblatt `b` eines Umfangs; die Indizes der
/// Teilpositionen je Dicke aus `rang` des ganzen Projekts.
pub fn lv_im_umfang(m: &Model, b: &Kostenblatt, rang: &Dickenrang, k: &Katalog, w: &LvWahl) -> Lv {
    let los = k.los(w.los);
    let los_nr = los.map_or(String::new(), |l| l.nr.clone());
    let mut befunde: Vec<Befund> = Vec::new();

    // Titel des Loses nach Nummer
    let mut titel: Vec<LvTitel> = k
        .lose
        .iter()
        .filter(|t| !t.retired && t.parent == Some(w.los))
        .map(|t| LvTitel {
            guid: t.guid,
            nr: t.nr.clone(),
            name: t.name.clone(),
            positionen: Vec::new(),
            untertitel: Vec::new(),
            summe: None,
            unvollstaendig: false,
        })
        .collect();
    titel.sort_by(|a, b| (a.nr.len(), &a.nr).cmp(&(b.nr.len(), &b.nr)));

    // Kostenzeilen je Bauleistung dieses Loses (K12: mehrere Zeilen, eine
    // Position)
    let mut je_leistung: Vec<(Guid, Vec<usize>)> = Vec::new();
    let mut stelle: HashMap<Guid, usize> = HashMap::new();
    for (i, p) in b.positionen.iter().enumerate() {
        let Quelle::Leistung(g) = p.quelle else {
            continue;
        };
        let Some(l) = k.leistung(g) else { continue };
        if los_der_leistung(k, l) != Some(w.los) {
            continue;
        }
        let j = *stelle.entry(g).or_insert_with(|| {
            je_leistung.push((g, Vec::new()));
            je_leistung.len() - 1
        });
        je_leistung[j].1.push(i);
    }

    // erste Position mit Erdmengen aus dem Höhenaufmaß (Hinweis unten)
    let mut erde_aus_mittel: Option<String> = None;
    // Gelände steiler als 1:10 (Hinweis), Erdmengen aus dem Geländemodell
    // (Vorbemerkung)
    let mut steil: Option<(String, String)> = None;
    // Hinweise der Automatikmengen je Position (VOB-Prüfung G3 D1)
    let mut mengen_hinweise: Vec<(String, String)> = Vec::new();
    let mut hinweis_gesehen: HashSet<(String, String)> = HashSet::new();
    let mut aus_netz = false;
    // Erdmengen aus dem angenommenen Gelände (VOB-Spruch zu I1)
    let mut angenommen: Option<String> = None;
    // die erste Position aus dem Netz, für den Befund zu Punkten von Hand
    // (VOB-Spruch zu I4)
    let mut netz_oz: Option<String> = None;
    for (g, zeilen, je_dicke) in je_dicke_teilen(&b.positionen, rang, je_leistung) {
        let l = k.leistung(g).expect("Leistung");
        let ps: Vec<&Position> = zeilen.iter().map(|i| &b.positionen[*i]).collect();
        let p0 = ps[0];
        let mehrere = ps.len() > 1;
        // Teilposition je Dicke (R1-B3): eigener Index an der OZ, Dicke im
        // Kurztext
        let index = je_dicke.map_or(String::new(), |(i, _)| format!(".{i}"));
        let kurz = match je_dicke {
            Some((_, d)) => kurz_je_dicke(&l.kurz, d),
            None => l.kurz.clone(),
        };
        let preis_fehlt = ps.iter().any(|p| p.preis_fehlt);
        let ansatz: Vec<&Ansatz> = ps.iter().flat_map(|p| p.ansatz.iter()).collect();
        let roh: i128 = ansatz.iter().map(|a| a.menge).sum();
        if roh == 0 {
            continue;
        }
        let Some(ti) = titel.iter().position(|t| t.guid == l.titel) else {
            continue;
        };
        let oz_basis = k.oz(l) + &index;
        let erde = |a: &&Ansatz| {
            a.auto.is_some_and(|x| x.starts_with("earth."))
                && a.formel
                    .as_deref()
                    .is_some_and(|f| f.contains(sk_model::qto_earth::AUS_AUFMASS))
        };
        if erde_aus_mittel.is_none() && ansatz.iter().any(erde) {
            erde_aus_mittel = Some(oz_basis.clone());
        }
        for a in &ansatz {
            for h in a.hinweis.lines().filter(|h| !h.is_empty()) {
                let t = (h.to_string(), oz_basis.clone());
                if hinweis_gesehen.insert(t.clone()) {
                    mengen_hinweise.push(t);
                }
            }
            let Some(f) = a.formel.as_deref() else {
                continue;
            };
            if !a.auto.is_some_and(|x| x.starts_with("earth.")) {
                continue;
            }
            if f.contains(sk_model::qto_earth::DREIECKSPRISMEN) {
                aus_netz = true;
                netz_oz.get_or_insert_with(|| oz_basis.clone());
            }
            if angenommen.is_none() && f.contains(sk_model::qto_earth::ANGENOMMEN) {
                angenommen = Some(oz_basis.clone());
            }
            if steil.is_none() {
                steil = sk_model::qto_earth::steil_hinweis(f).map(|h| (h, oz_basis.clone()));
            }
        }
        if mehrere {
            // Zeilen aus Erweiterungen nennen das Bauteil statt der Dicke
            // (Vorprüfung E8-3)
            let ext = |p: &Position| {
                p.ansatz.iter().find_map(|a| {
                    let d = m.ext_def_of(a.element)?;
                    Some(format!(
                        "{} {}",
                        sk_model::erweiterung::anzeige(d.name(), 60),
                        a.nummer
                    ))
                })
            };
            let mut dicken: Vec<Dez> = ps
                .iter()
                .filter(|p| ext(p).is_none())
                .flat_map(|p| p.dicken.iter().copied())
                .collect();
            dicken.sort();
            dicken.dedup();
            let mut d: Vec<String> = dicken.iter().map(|d| format!("{} mm", mm(*d))).collect();
            let mut bauteile: Vec<String> = ps.iter().filter_map(|p| ext(p)).collect();
            bauteile.dedup();
            let je = if bauteile.is_empty() {
                "je Dicke"
            } else {
                "je Dicke und Erweiterung"
            };
            d.extend(bauteile);
            // Räume: die Dicke kommt aus dem Raumbuch, eine Bauleistung je
            // Dicke kann niemand anlegen (Review #213 H-213f)
            let raum = ps.iter().all(|p| nur_raeume(m, p));
            let bitte = match rang.dicken(g) {
                Some(ds) if raum && ds.len() > INDEX_MAX as usize => format!(
                    "{} Dicken im Projekt sind mehr, als der Positionsindex fasst (1 bis {INDEX_MAX}); bitte im Raumbuch weniger Dicken vorsehen.",
                    ds.len()
                ),
                _ if raum && ps.iter().any(|p| p.schicht.map(|s| s.0) != p0.schicht.map(|s| s.0)) => {
                    "bitte die Bauleistung je Baustoff anlegen.".to_string()
                }
                _ if raum => "bitte die Artikel je Dicke prüfen.".to_string(),
                _ => "bitte die Bauleistung je Dicke anlegen.".to_string(),
            };
            befunde.push(Befund::fehler(
                101,
                format!(
                    "{} {} hat verschiedene Stoffpreise {je} ({}); {bitte}",
                    oz_mit_los(&los_nr, &oz_basis),
                    l.kurz,
                    d.join(" und ")
                ),
                Ort::Position(oz_basis.clone()),
            ));
        }
        // Kostengruppe wie im Kostenblatt: die gemeinsame `Ansatz.kg` der
        // Zeilen (dort schon die der Bauleistung, sonst die der Schicht);
        // verschiedene ergeben keine
        let kg = {
            let mut kgs = ansatz.iter().map(|a| a.kg);
            let erste = kgs.next().flatten();
            match erste {
                Some(_) if kgs.all(|x| x == erste) => erste,
                Some(_) => None,
                None if ansatz.iter().all(|a| a.kg.is_none()) => l.kg,
                None => None,
            }
        };
        // Verschiedene KG nicht verschweigen (Kosten B2): Menge je KG
        let kg_teile: Vec<(u16, Dez)> = {
            let mut t: Vec<(u16, i128)> = Vec::new();
            for a in ansatz.iter().filter(|_| kg.is_none()) {
                let Some(x) = a.kg else { continue };
                match t.iter_mut().find(|y| y.0 == x) {
                    Some(y) => y.1 += a.menge,
                    None => t.push((x, a.menge)),
                }
            }
            t.sort_by_key(|x| x.0);
            if t.len() > 1 {
                t.into_iter()
                    .map(|(x, v)| (x, drei(v, l.einheit)))
                    .collect()
            } else {
                Vec::new()
            }
        };
        let ep = (w.preise && !mehrere).then_some(p0.ep);
        let anteile = (w.preise && !mehrere).then_some(Anteile {
            lohn: p0.lohn,
            stoff: p0.stoff,
            geraet: p0.geraet,
            sonst: p0.sonst,
            nu: p0.nu,
            stunden: l.stunden,
        });
        let neu = |oz: String,
                   menge: Dez,
                   gp_wert: Option<Cent>,
                   ansatz: Vec<Ansatzzeile>,
                   uu: Option<u32>| LvPosition {
            oz,
            kurztext: kurz.clone(),
            menge,
            einheit: l.einheit,
            ep,
            gp: gp_wert,
            ansatz,
            quelle: g,
            untertitel: uu,
            art: Positionsart::Normal,
            gewerk: l.gewerk,
            kg,
            kg_teile: kg_teile.clone(),
            anteile,
            preis_fehlt,
            mehrere_preise: mehrere,
            blatt: zeilen.clone(),
            dicke: je_dicke,
        };
        if w.untertitel {
            // je Geschoss eine Position mit eigener OZ (Nachtrag 07:05)
            let mut teile: Vec<(u32, String, Vec<&Ansatz>)> = Vec::new();
            for a in &ansatz {
                let (uu, name) = untertitel_von(m, a.geschoss);
                match teile.iter_mut().find(|t| t.0 == uu) {
                    Some(t) => t.2.push(a),
                    None => teile.push((uu, name, vec![a])),
                }
            }
            teile.sort_by_key(|t| t.0);
            for (uu, name, v) in teile {
                let r: i128 = v.iter().map(|a| a.menge).sum();
                if r == 0 {
                    continue;
                }
                let menge = drei(r, l.einheit);
                let t = &mut titel[ti];
                if !t.untertitel.iter().any(|u| u.nr == uu) {
                    t.untertitel.push(LvUntertitel {
                        nr: uu,
                        oz: format!("{}.{uu:02}", t.nr),
                        name,
                        summe: None,
                    });
                }
                let ma = mengenansatz(k, &v, l.einheit, Some(uu));
                let oz = oz_im_los(k, l, Some(uu)) + &index;
                titel[ti]
                    .positionen
                    .push(neu(oz, menge, ep.map(|e| gp(menge, e)), ma, Some(uu)));
            }
        } else {
            let menge = drei(roh, l.einheit);
            // ohne K12 genau die Zeile des Kostenblatts (Abnahme 7)
            let gp_wert = ep.map(|_| p0.gp);
            let ma = mengenansatz(k, &ansatz, l.einheit, None);
            titel[ti]
                .positionen
                .push(neu(oz_basis, menge, gp_wert, ma, None));
        }
    }

    // Ordnung im Titel: Untertitel, dann pos (OZ fest, Lücken gewollt)
    for t in &mut titel {
        t.untertitel.sort_by_key(|u| u.nr);
        t.positionen.sort_by_key(|p| {
            let pos = k.leistung(p.quelle).map_or(0, |l| l.pos);
            (p.untertitel, pos, p.dicke.map(|x| x.0))
        });
        t.unvollstaendig = t
            .positionen
            .iter()
            .any(|p| p.preis_fehlt || p.mehrere_preise);
        if w.preise {
            t.summe = Some(t.positionen.iter().filter_map(|p| p.gp).sum());
            for u in &mut t.untertitel {
                u.summe = Some(
                    t.positionen
                        .iter()
                        .filter(|p| p.untertitel == Some(u.nr))
                        .filter_map(|p| p.gp)
                        .sum(),
                );
            }
        }
    }

    if let Some(oz) = erde_aus_mittel {
        befunde.push(Befund::hinweis(
            0,
            crate::rechnung::ERDE_AUS_MITTEL,
            Ort::Position(oz),
        ));
    }
    if let Some(oz) = angenommen {
        befunde.push(Befund::hinweis(
            0,
            crate::rechnung::NICHT_VERMESSEN,
            Ort::Position(oz),
        ));
    }
    if let (Some(oz), Some(g)) = (netz_oz, sk_model::qto_earth::netz_geaendert(m)) {
        befunde.push(Befund::hinweis(
            0,
            sk_model::qto_earth::hand_geaendert(g),
            Ort::Position(oz),
        ));
    }
    if let Some((h, oz)) = steil {
        befunde.push(Befund::hinweis(0, h, Ort::Position(oz)));
    }
    for (h, oz) in mengen_hinweise {
        befunde.push(Befund::hinweis(0, h, Ort::Position(oz)));
    }

    // Regel 86 im LV: Der Katalog meldet eine doppelte OZ nur und behält
    // beide Bauleistungen (auch eine ausgemusterte, die noch verwendet
    // wird); im LV des Loses wäre die OZ dann zweimal da
    // (Mengen statt Listen, Review #213 H-213h)
    let mut oz_gesehen: HashSet<&str> = HashSet::new();
    let mut oz_doppelt: Vec<&str> = Vec::new();
    let mut doppelt_gesehen: HashSet<&str> = HashSet::new();
    for p in titel.iter().flat_map(|t| &t.positionen) {
        if !oz_gesehen.insert(&p.oz) && doppelt_gesehen.insert(&p.oz) {
            oz_doppelt.push(&p.oz);
        }
    }
    for oz in oz_doppelt {
        befunde.push(Befund::fehler(
            86,
            crate::befund::r86(&oz_mit_los(&los_nr, oz)),
            Ort::Position(oz.to_string()),
        ));
    }

    // Befunde je Position: Preis fehlt, Kurztext, Preisstand, eigener Preis;
    // die übrigen nach dem Kurztext nur einmal je Bauleistung und Dicke,
    // mit Untertiteln an der ersten (Review #213 H-213e, H-213h)
    let mut gemeldet: HashSet<(Guid, Option<u32>)> = HashSet::new();
    for t in &titel {
        for p in &t.positionen {
            let voll = oz_mit_los(&los_nr, &p.oz);
            let ort = Ort::Position(p.oz.clone());
            let zeichen = p.kurztext.chars().count();
            if zeichen > 70 {
                befunde.push(Befund::fehler(
                    79,
                    crate::befund::r79_laenge(&voll, zeichen),
                    ort.clone(),
                ));
            }
            if !gemeldet.insert((p.quelle, p.dicke.map(|x| x.0))) {
                continue;
            }
            if w.preise && p.preis_fehlt {
                befunde.push(Befund::fehler(
                    82,
                    format!("Preis fehlt: {voll} {}", p.kurztext),
                    ort.clone(),
                ));
            }
            let mut gesehen: Vec<Guid> = Vec::new();
            let mut eigen = false;
            for i in &p.blatt {
                let Some(a) = crate::preis::aufbau(m, k, &b.positionen[*i]) else {
                    continue;
                };
                eigen |= !crate::preis::abweichend(k, &a).is_empty();
                for s in &a.stoffe {
                    let Some(art) = s.artikel.and_then(|g| k.artikel(g)) else {
                        continue;
                    };
                    if gesehen.contains(&art.guid) {
                        continue;
                    }
                    gesehen.push(art.guid);
                    let stand = art.satz.text("date").and_then(monat_lesen);
                    if let (Some((jahr, monat)), Some((hj, hm))) = (stand, w.heute) {
                        let alter =
                            (hj as i32 * 12 + hm as i32) - (jahr as i32 * 12 + monat as i32);
                        if alter > 12 {
                            befunde.push(Befund::hinweis(
                                82,
                                format!("Preisstand {monat:02}/{jahr}: {}", art.name),
                                ort.clone(),
                            ));
                        }
                    }
                }
            }
            if eigen {
                befunde.push(Befund::hinweis(
                    89,
                    format!("eigener Preis: {voll} {}", p.kurztext),
                    ort.clone(),
                ));
            }
        }
    }

    // Geschätzte Zeilen dieses Loses: nicht ausgeschrieben (K13)
    let mut geschaetzt = Cent::NULL;
    for p in &b.positionen {
        let (nach, los_p) = match &p.quelle {
            Quelle::Leistung(_) => continue,
            Quelle::Geschaetzt(g) => match k.leistung(*g) {
                Some(l) => (
                    format!("{} {}", k.oz_voll(l), l.kurz),
                    los_der_leistung(k, l),
                ),
                None => continue,
            },
            Quelle::Richtpreis(_) => ("Richtpreis".to_string(), los_vom_gewerk(k, p.gewerk)),
        };
        if los_p != Some(w.los) {
            continue;
        }
        geschaetzt += p.gp;
        let mut nummern: Vec<&str> = Vec::new();
        for a in &p.ansatz {
            if !nummern.contains(&a.nummer.as_str()) {
                nummern.push(&a.nummer);
            }
        }
        let was = match p.schicht {
            Some((bs, d)) if d.0 > 0 => {
                format!("{} d={}cm", baustoff_name(m, bs), mm(Dez(d.0 / 10)))
            }
            _ => match &p.quelle {
                Quelle::Richtpreis(bs) => baustoff_name(m, *bs),
                _ => p
                    .ansatz
                    .first()
                    .map_or(String::new(), |a| typname(m, a.element)),
            },
        };
        let ort = p
            .ansatz
            .first()
            .and_then(|a| element_guid(m, a.element))
            .map_or(Ort::Datei, Ort::Bauteil);
        befunde.push(Befund::fehler(
            81,
            format!(
                "Nicht ausgeschrieben: {was} ({}), {} {}, in den Kosten geschätzt nach {nach}",
                nummern.join(", "),
                menge_deutsch(p.menge),
                p.einheit.zeichen()
            ),
            ort,
        ));
    }

    // Zeilen ohne Bauleistung: im Los ihres Gewerks, ohne Los in jedem
    for o in &b.ohne {
        match los_vom_gewerk(k, o.gewerk) {
            Some(x) if x != w.los => continue,
            _ => {}
        }
        let satz = match &o.herkunft {
            OhneHerkunft::Schicht { baustoff, .. } => {
                let art = m.element(o.element).map_or(String::new(), |e| {
                    sk_model::kinds::spec(e.category).name.to_string()
                });
                format!(
                    "Ohne Bauleistung: {art} · {} ({})",
                    baustoff_name(m, *baustoff),
                    o.nummer
                )
            }
            OhneHerkunft::Erweiterung { .. } => crate::rechnung::ohne_ext_text(m, o),
        };
        befunde.push(Befund::fehler(
            81,
            satz,
            element_guid(m, o.element).map_or(Ort::Datei, Ort::Bauteil),
        ));
    }

    // Kopf und Vorbemerkungen (Regel 107)
    let pr = m.project();
    let text = |s: &str| (!s.trim().is_empty()).then(|| s.to_string());
    // Aufsteller: Verfasser im Projekt, sonst der Name eines echten
    // Firmenkatalogs; der Werksbestand ist eine Preisquelle, kein
    // Aufsteller (Kosten A1, Bedienbarkeit 12.1)
    let firma = match &k.quelle {
        crate::katalog::Quelle::Firma { name, .. } => text(name),
        _ => None,
    };
    let mut kopf = LvKopf {
        bauvorhaben: text(&pr.site),
        projektart: text(&pr.kind),
        bauort: text(&pr.place),
        projektnummer: text(&pr.number),
        bauherr: text(&pr.client),
        bauherr_anschrift: text(&pr.client_addr),
        aufsteller_anschrift: text(&pr.author).and_then(|_| text(&pr.author_addr)),
        aufsteller: text(&pr.author).or(firma),
        los: los.map_or(String::new(), |l| l.name.clone()),
        los_nr: los_nr.clone(),
        art: if w.preise {
            "mit Preisen"
        } else {
            "Anfrage ohne Preise"
        },
        waehrung: "EUR",
        netto: "Preise netto zzgl. MwSt.",
        vorbemerkungen: los.and_then(|l| l.pre.clone()).filter(|p| !p.is_empty()),
    };
    if kopf.bauherr.is_none() {
        befunde.push(Befund::hinweis(0, "Bauherr fehlt", Ort::Kopf));
    }
    if kopf.aufsteller.is_none() {
        befunde.push(Befund::hinweis(0, "Aufsteller fehlt", Ort::Kopf));
    }
    if kopf.vorbemerkungen.is_none() {
        befunde.push(Befund::hinweis(
            107,
            format!("Los {} hat keine allgemeinen Vorbemerkungen.", kopf.los),
            Ort::Kopf,
        ));
    }
    // Erdmengen aus dem Geländemodell: das Verfahren am Ende der
    // Vorbemerkungen des Loses (VOB-Prüfung G2 Frage 1, Regel 107)
    if aus_netz {
        let v = kopf.vorbemerkungen.get_or_insert_with(String::new);
        if !v.is_empty() {
            v.push_str("\n\n");
        }
        // ein angenommenes Gelände hat keinen Höhenaufmaß (VOB-Spruch zu I1)
        v.push_str(&match sk_model::qto_earth::netz_angenommen(m) {
            Some(n) => vorbemerkung_angenommen(&n),
            None => vorbemerkung_netz(sk_model::qto_earth::netz_herkunft(m).as_deref()),
        });
    }
    for t in k
        .lose
        .iter()
        .filter(|t| t.parent == Some(w.los) && t.pre.is_some())
    {
        befunde.push(Befund::hinweis(
            107,
            format!(
                "Titel {} hat Vorbemerkungen; gedruckt werden nur die des Loses.",
                t.name
            ),
            Ort::Kopf,
        ));
    }

    // Zusammenstellung: Titel mit Positionen, MwSt. wie im Kostenblatt
    let mit: Vec<&LvTitel> = titel.iter().filter(|t| !t.positionen.is_empty()).collect();
    let netto = w
        .preise
        .then(|| mit.iter().filter_map(|t| t.summe).sum::<Cent>());
    let mwst = netto.map(|n| {
        Cent(runden(
            n.0 as i128 * b.mwst_satz.0 as i128,
            100 * Dez::SKALA as i128,
        ) as i64)
    });
    let zusammenstellung = Zusammenstellung {
        zeilen: mit
            .iter()
            .map(|t| (t.nr.clone(), t.name.clone(), t.summe))
            .collect(),
        netto,
        mwst_satz: b.mwst_satz,
        mwst,
        brutto: netto.zip(mwst).map(|(n, s)| n + s),
        geschaetzt: (w.preise && geschaetzt != Cent::NULL).then_some(geschaetzt),
        ohne_los: {
            let c: Cent = b
                .positionen
                .iter()
                .filter(|p| match p.quelle {
                    Quelle::Leistung(g) => k.leistung(g).is_some_and(|l| k.los(l.titel).is_none()),
                    _ => false,
                })
                .map(|p| p.gp)
                .sum();
            (w.preise && c != Cent::NULL).then_some(c)
        },
        unvollstaendig: mit.iter().any(|t| t.unvollstaendig),
    };
    Lv {
        kopf,
        titel,
        zusammenstellung,
        befunde,
    }
}

#[cfg(test)]
mod tests;

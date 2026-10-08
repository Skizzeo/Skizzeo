//! Reiter Kosten (KA-2b, paket-ka2 §3 und §5, Einstellungen §3 KA-2,
//! ka-2-fach §2): Kopf mit Umfang und Preisquelle, Chips mit ihrer Summe,
//! Schalter „Preise“ und „Gliedern“, die Liste, drei Kacheln und die
//! Fußzeile.
//!
//! Die Ansicht rechnet keine Kosten und kein Geld. Sie formatiert das
//! Kostenblatt (`Scene::kostenblatt`) in der Gliederung von
//! `Kostenblatt::aufteilung` (Review 3ai): Teilzeilen, Gruppensummen als
//! Summe der Zeilen darunter (Bedienbarkeit 2.1, Abnahme 6) und am Ende der
//! „Rundungsausgleich“, wo Teilzeilen das Netto nicht genau treffen; die
//! Chip-Summen aus `Kostenblatt::summe_geschosse`. Das Preisblatt
//! (`preis_blatt.rs`) zeigt beim Tippen das Blatt auf dem Katalog mit den
//! getippten Werten.

use crate::picking::Picking;
use crate::preis_blatt::{self, Gilt, PreisBlatt};
use crate::scene::Scene;
use crate::schedule_view::ListOut;
use crate::umfang_view::{self, Leiste};
use crate::wahl_blatt::{self, WahlBlatt};
use sk_cost::abgleich::Abgleich;
use sk_cost::gliederung::{Gruppe, Schluessel, Teilung};
use sk_cost::katalog::{Einheit, Katalog};
use sk_cost::rechnung::{Ansatz, Position, Quelle};
use sk_cost::{Cent, Dez, Kostenblatt, Op, SatzId};
use sk_model::qto::Umfang;
use sk_model::trade::TradeId;
use sk_model::{ElementId, Guid, Model, StoreyId};
use sk_paint::{Canvas, Path, Rgba};
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;

/// Preise der Liste (ka-2-fach §2.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Modus {
    /// Stoff-EP und Stoff-GP; NU-Positionen zählen nicht.
    Material,
    /// EP und GP voll (Standard).
    #[default]
    Voll,
}

impl Modus {
    const ALLE: [Modus; 2] = [Modus::Material, Modus::Voll];

    fn label(self) -> &'static str {
        match self {
            Modus::Material => "Material",
            Modus::Voll => "Material + Lohn",
        }
    }

    fn nur_material(self) -> bool {
        self == Modus::Material
    }
}

/// Gliederung der Liste; Kostengruppe steht zuletzt (Bedienbarkeit 2.3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gliederung {
    #[default]
    Gewerk,
    Geschoss,
    Kostengruppe,
}

impl Gliederung {
    const ALLE: [Gliederung; 3] = [
        Gliederung::Gewerk,
        Gliederung::Geschoss,
        Gliederung::Kostengruppe,
    ];

    fn label(self) -> &'static str {
        match self {
            Gliederung::Gewerk => "Gewerk",
            Gliederung::Geschoss => "Geschoss",
            Gliederung::Kostengruppe => "Kostengruppe",
        }
    }
}

/// Art einer Zeile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// Gewerk, Geschoss oder Kostengruppe mit Summe; `ebene` 0 oder 1.
    Gruppe,
    /// Position oder Teil einer Position; `offen` mit Mengenansatz darunter.
    Position { offen: bool },
    /// Eine Zeile des Mengenansatzes.
    Ansatz,
    /// Mengenzeile ohne Bauleistung, grau, ohne Preis.
    Ohne,
    /// „Rundungsausgleich“ am Ende (ka-0-fach §1.7).
    Ausgleich,
}

/// Eine Zeile der Liste, fertig zum Zeichnen und für die CSV.
#[derive(Clone, Debug, PartialEq)]
pub struct Zeile {
    pub art: Art,
    pub ebene: u8,
    pub text: String,
    /// Leise hinter dem Text: DIN-Nummer, „geschätzt nach …“, Herkunft.
    pub leise: String,
    /// Menge mit Einheit („172,224 m²“).
    pub menge: String,
    pub ep: String,
    pub gp: String,
    /// Betrag hinter `gp`; `None`, wo nichts zählt (NU im Modus Material,
    /// Zeilen ohne Bauleistung).
    pub betrag: Option<Cent>,
    /// Position im Kostenblatt und angezeigte Menge (für die CSV).
    pub pos: Option<(usize, Dez)>,
    pub geschaetzt: bool,
    pub elements: Vec<ElementId>,
    /// Schlüssel zum Aufklappen (Positionen).
    pub key: u64,
    /// Gliederung, unter der die Zeile steht (CSV-Spalte).
    pub gruppe: String,
    /// Zeile ohne Bauleistung im Kostenblatt („Bauleistung wählen …“).
    pub ohne: Option<usize>,
}

impl Zeile {
    fn neu(art: Art, ebene: u8, text: String) -> Zeile {
        Zeile {
            art,
            ebene,
            text,
            leise: String::new(),
            menge: String::new(),
            ep: String::new(),
            gp: String::new(),
            betrag: None,
            pos: None,
            geschaetzt: false,
            elements: Vec::new(),
            key: 0,
            gruppe: String::new(),
            ohne: None,
        }
    }
}

// --- Zahlen ------------------------------------------------------------------

/// Tausenderpunkte vor eine Ziffernfolge.
fn tausender(ziffern: &str) -> String {
    let mut s = String::new();
    for (i, c) in ziffern.chars().enumerate() {
        if i > 0 && (ziffern.len() - i).is_multiple_of(3) {
            s.push('.');
        }
        s.push(c);
    }
    s
}

/// Menge auf 3 Stellen mit Einheit: „1.172,224 m²“.
pub fn menge_text(d: Dez, e: Einheit) -> String {
    let milli = d.0 / 1000;
    let neg = milli < 0;
    let a = milli.unsigned_abs();
    format!(
        "{}{},{:03} {}",
        if neg { "−" } else { "" },
        tausender(&(a / 1000).to_string()),
        a % 1000,
        e.zeichen()
    )
}

/// Betrag mit zwei Stellen: „60.089,83“.
fn euro(c: Cent) -> String {
    c.deutsch()
}

/// Betrag auf ganze Euro mit Zeichen: „60.090 €“ (Karten, Chips).
pub fn euro_ganz(c: Cent) -> String {
    let e = (c.0 + if c.0 < 0 { -50 } else { 50 }) / 100;
    let neg = e < 0;
    format!(
        "{}{} €",
        if neg { "−" } else { "" },
        tausender(&e.unsigned_abs().to_string())
    )
}

/// Prozent ganzzahlig, kaufmännisch: `teil / ganz`.
fn prozent(teil: Cent, ganz: Cent) -> Option<i64> {
    (ganz.0 > 0).then(|| (teil.0 * 200 + ganz.0) / (2 * ganz.0))
}

// --- Zeilen bauen ------------------------------------------------------------

/// Name und DIN-Nummer eines Gewerks; ohne Gewerk „Ohne Gewerk“.
fn gewerk_name(m: &Model, g: Option<Guid>) -> (String, String, u16) {
    match g.and_then(|g| m.trade(TradeId(g))) {
        Some(t) => (t.name.clone(), format!("DIN {}", t.code), t.order),
        None => ("Ohne Gewerk".into(), String::new(), u16::MAX),
    }
}

/// Name eines Geschosses in der Gliederung („Gründung“, „Erdgeschoss“).
fn geschoss_name(m: &Model, s: StoreyId) -> String {
    m.storey(s).map_or_else(String::new, |x| x.name.clone())
}

/// Kostengruppe mit Namen: „322 Flachgründungen und Bodenplatten“.
fn kg_name(kg: Option<u16>) -> String {
    match kg {
        Some(k) => match sk_cost::din276::name(k) {
            Some(n) => format!("{k} {n}"),
            None => k.to_string(),
        },
        None => "Ohne Kostengruppe".into(),
    }
}

/// Kurzname einer Bauleistung für „geschätzt nach …“ (Namen statt
/// Kennungen, Bedienbarkeit 4.9).
fn leistung_name(k: &Katalog, g: Guid) -> String {
    k.leistung(g).map_or_else(String::new, |l| l.kurz.clone())
}

/// Schlüssel einer Position zum Aufklappen (über Gliederungen gleich).
fn pos_key(p: &Position, teil: u64) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let quelle = match &p.quelle {
        Quelle::Leistung(g) => (1u8, g.0),
        Quelle::Geschaetzt(g) => (2, g.0),
        Quelle::Richtpreis(g) => (3, g.0),
    };
    for b in quelle
        .0
        .to_le_bytes()
        .iter()
        .chain(&quelle.1.to_le_bytes())
        .chain(p.kurz.as_bytes())
        .chain(&teil.to_le_bytes())
    {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Alles, was die Zeilen brauchen.
struct Bau<'a> {
    m: &'a Model,
    k: &'a Katalog,
    b: &'a Kostenblatt,
    modus: Modus,
    offen: &'a HashSet<u64>,
}

impl Bau<'_> {
    /// Zeile einer Position mit der Menge `menge` (ganz oder Teil) aus den
    /// Ansatzzeilen `ansatz`; darunter, wenn offen, der Mengenansatz.
    fn position(
        &self,
        out: &mut Vec<Zeile>,
        t: &sk_cost::gliederung::Zeile,
        ansatz: &[&Ansatz],
        teil: u64,
        ebene: u8,
        gruppe: &str,
    ) {
        let (i, menge) = (t.pos, t.menge);
        let p = &self.b.positionen[i];
        let key = pos_key(p, teil);
        let offen = self.offen.contains(&key);
        let mut z = Zeile::neu(Art::Position { offen }, ebene, p.kurz.clone());
        z.key = key;
        z.gruppe = gruppe.to_string();
        z.menge = menge_text(menge, p.einheit);
        z.pos = Some((i, menge));
        z.geschaetzt = !matches!(p.quelle, Quelle::Leistung(_));
        z.leise = match &p.quelle {
            Quelle::Geschaetzt(g) => format!("geschätzt nach {}", leistung_name(self.k, *g)),
            Quelle::Richtpreis(_) => "geschätzt, nur Material".into(),
            Quelle::Leistung(_) => String::new(),
        };
        let mut els: Vec<ElementId> = Vec::new();
        for a in ansatz {
            if !els.contains(&a.element) {
                els.push(a.element);
            }
        }
        z.elements = els;
        // Betrag aus dem Kostenblatt; ohne Betrag (NU bei „nur Material“)
        // steht „NU-Preis“
        match t.betrag {
            Some(gp) => {
                let ep = if self.modus.nur_material() {
                    p.stoff
                } else {
                    p.ep
                };
                z.ep = euro(ep);
                z.gp = euro(gp);
            }
            None => z.ep = "NU-Preis".into(),
        }
        z.betrag = t.betrag;
        out.push(z);
        if offen {
            self.ansatz(out, p, ansatz, ebene + 1, gruppe);
        }
    }

    /// Mengenansatz: je Geschoss und Herkunft die Bauteilnummern und die
    /// Menge, Herkunft „aus Modell“ bzw. „aus {Bauleistung}“.
    fn ansatz(
        &self,
        out: &mut Vec<Zeile>,
        p: &Position,
        ansatz: &[&Ansatz],
        ebene: u8,
        gruppe: &str,
    ) {
        let mut teile: Vec<(StoreyId, Option<Guid>, Vec<&Ansatz>)> = Vec::new();
        for a in ansatz {
            match teile.iter_mut().find(|t| t.0 == a.geschoss && t.1 == a.aus) {
                Some(t) => t.2.push(a),
                None => teile.push((a.geschoss, a.aus, vec![a])),
            }
        }
        for (st, aus, v) in teile {
            let mut nummern: Vec<&str> = Vec::new();
            for a in &v {
                if !nummern.contains(&a.nummer.as_str()) {
                    nummern.push(&a.nummer);
                }
            }
            let kurz = self.m.storey(st).map_or(String::new(), |s| s.short.clone());
            let mut z = Zeile::neu(
                Art::Ansatz,
                ebene,
                format!("{kurz} · {}", nummern.join(", ")),
            );
            z.gruppe = gruppe.to_string();
            z.leise = match aus {
                Some(g) => format!("aus {}", leistung_name(self.k, g)),
                None => "aus Modell".into(),
            };
            let teil = Position {
                ansatz: v.iter().map(|a| (*a).clone()).collect(),
                ..p.clone()
            };
            let menge = teil.teile(|_| ()).first().map_or(Dez::NULL, |t| t.1);
            z.menge = menge_text(menge, p.einheit);
            z.elements = v.iter().map(|a| a.element).collect();
            out.push(z);
        }
    }

    /// Zeilen ohne Bauleistung (grau, Menge, kein Preis).
    fn ohne(&self, out: &mut Vec<Zeile>, welche: &[usize], ebene: u8, gruppe: &str) {
        for (&j, o) in welche.iter().map(|j| (j, &self.b.ohne[*j])) {
            let name = self.m.element(o.element).map_or(String::new(), |e| {
                sk_model::kinds::spec(e.category).name.to_string()
            });
            let baustoff = self
                .m
                .materials()
                .iter()
                .find(|(_, x)| x.guid == o.baustoff)
                .map_or(String::new(), |(_, x)| x.name.clone());
            let mut z = Zeile::neu(
                Art::Ohne,
                ebene,
                format!("{} {}", o.nummer, baustoff).trim().to_string(),
            );
            z.gruppe = gruppe.to_string();
            z.leise = format!("{name} · ohne Bauleistung");
            z.menge = menge_text(o.menge, o.einheit);
            z.elements = vec![o.element];
            z.ohne = Some(j);
            out.push(z);
        }
    }

    /// Name und leiser Zusatz einer Gruppe.
    fn gruppe_name(&self, k: Schluessel) -> (String, String) {
        match k {
            Schluessel::Gewerk(g) => {
                let (name, din, _) = gewerk_name(self.m, g);
                (name, din)
            }
            Schluessel::Geschoss(st) => (geschoss_name(self.m, st), String::new()),
            Schluessel::Kg2(kg) | Schluessel::Kg(kg) => (kg_name(kg), String::new()),
        }
    }

    /// Gruppenzeile mit der Summe aus dem Kostenblatt, darunter Untergruppen,
    /// (Teil-)Zeilen und Zeilen ohne Bauleistung. `teil`: Schlüssel der
    /// Teilung zum Aufklappen; `csv`: Gliederung für die CSV-Spalte, wenn
    /// eine Gruppe darüber sie vorgibt (Geschoss).
    fn gruppe(&self, out: &mut Vec<Zeile>, g: &Gruppe, ebene: u8, teil: u64, csv: Option<&str>) {
        let (text, leise) = self.gruppe_name(g.schluessel);
        let teil = match g.schluessel {
            Schluessel::Geschoss(st) => st_key(st),
            Schluessel::Kg(kg) => 1 << 32 | u64::from(kg.unwrap_or(0)),
            _ => teil,
        };
        let name = csv.unwrap_or(&text).to_string();
        let csv_kinder = match g.schluessel {
            Schluessel::Geschoss(_) => Some(name.as_str()),
            _ => csv,
        };
        let mut kinder = Vec::new();
        for u in &g.gruppen {
            self.gruppe(&mut kinder, u, ebene + 1, teil, csv_kinder);
        }
        for z in &g.zeilen {
            let p = &self.b.positionen[z.pos];
            let a: Vec<&Ansatz> = z.ansatz.iter().map(|&j| &p.ansatz[j]).collect();
            self.position(&mut kinder, z, &a, teil, ebene + 1, &name);
        }
        self.ohne(&mut kinder, &g.ohne, ebene + 1, &name);
        let mut z = Zeile::neu(Art::Gruppe, ebene, text);
        z.leise = leise;
        z.betrag = g.summe;
        z.gp = match g.summe {
            Some(c) => euro(c),
            None if g.zeilen.is_empty() && g.gruppen.is_empty() => "ohne Bauleistung".into(),
            None => String::new(),
        };
        let mut els: Vec<ElementId> = Vec::new();
        for k in &kinder {
            for e in &k.elements {
                if !els.contains(e) {
                    els.push(*e);
                }
            }
        }
        z.elements = els;
        out.push(z);
        out.extend(kinder);
    }

    /// Alle Zeilen der Gliederung, am Ende der „Rundungsausgleich“, wenn
    /// das Kostenblatt einen hat.
    fn alle(&self, t: Teilung) -> Vec<Zeile> {
        let a = self.b.aufteilung(self.m, t, self.modus.nur_material());
        let mut out = Vec::new();
        for g in &a.gruppen {
            self.gruppe(&mut out, g, 0, 0, None);
        }
        if a.ausgleich != Cent(0) {
            let mut z = Zeile::neu(Art::Ausgleich, 0, "Rundungsausgleich".into());
            z.leise = "Teilmengen je auf 3 Stellen gerundet".into();
            z.gp = euro(a.ausgleich);
            z.betrag = Some(a.ausgleich);
            out.push(z);
        }
        out
    }
}

/// Schlüssel eines Geschosses für das Aufklappen von Teilzeilen.
fn st_key(st: StoreyId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    st.hash(&mut h);
    h.finish()
}

/// Die Zeilen der Liste: rein aus Modell (Namen), Katalog (Namen),
/// Kostenblatt, Gliederung, Modus und aufgeklappten Positionen.
pub fn zeilen(
    m: &Model,
    k: &Katalog,
    b: &Kostenblatt,
    g: Gliederung,
    modus: Modus,
    offen: &HashSet<u64>,
) -> Vec<Zeile> {
    let bau = Bau {
        m,
        k,
        b,
        modus,
        offen,
    };
    bau.alle(match g {
        Gliederung::Gewerk => Teilung::Gewerk,
        Gliederung::Geschoss => Teilung::Geschoss,
        Gliederung::Kostengruppe => Teilung::Kostengruppe,
    })
}

/// Summe je Chip (Reiter Kosten, auch abgewählt) aus dem Kostenblatt ohne
/// Abwahl, im Modus.
pub fn chip_summen(b: &Kostenblatt, chips: &[umfang_view::Chip], modus: Modus) -> Vec<Cent> {
    chips
        .iter()
        .map(|c| b.summe_geschosse(&c.geschosse, modus.nur_material()))
        .collect()
}

// --- Ansicht -----------------------------------------------------------------

/// Kopf (dip ab Unterkante der Kartenleiste): Titel, Unterzeile, Chips,
/// Schalter, Spaltenköpfe und Linie.
const TITLE_Y: f32 = 34.0;
const SUB_Y: f32 = 52.0;
const CHIP_TOP: f32 = 62.0;
const SWITCH_TOP: f32 = CHIP_TOP + umfang_view::ROW + 4.0;
const SWITCH_H: f32 = 24.0;
const HEAD: f32 = SWITCH_TOP + SWITCH_H + 34.0;
/// Abgleichzeile unter der Unterzeile (Grundlinie) und was sie Chips,
/// Schalter und Liste nach unten schiebt.
const ABGLEICH_Y: f32 = 72.0;
const ABGLEICH_H: f32 = 20.0;
/// Abstand des Schalters „Gliedern“ vom linken Rand (Einstellungen §3 KA-1
/// Punkt 9: wie im Mengenblatt bei links + 300 dip).
const GLIEDERN_X: f32 = 300.0;
/// Zeilenhöhen (dip).
const ROW_GROUP0: f32 = 30.0;
const ROW_GROUP1: f32 = 26.0;
const ROW_POS: f32 = 22.0;
const ROW_ANSATZ: f32 = 20.0;
/// Kacheln und Fußzeile (dip).
const TILE_H: f32 = 62.0;
const TILE_GAP: f32 = 10.0;
const FOOT_LINE: f32 = 18.0;
const BOTTOM_PAD: f32 = 12.0;
/// Einzug je Ebene (dip).
const INDENT: f32 = 16.0;
/// Knopf „Als Tabelle speichern“.
const BUTTON_H: f32 = 26.0;
const BUTTON_PAD: f32 = 12.0;

type Rect = (f32, f32, f32, f32);

fn inside((rx, ry, rw, rh): Rect, x: f32, y: f32) -> bool {
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

/// Teil des Blatts unter der Maus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hot {
    Umfang(umfang_view::Hot),
    Modus(Modus),
    Gliederung(Gliederung),
    Button,
    /// Zeile; `true` auf dem Dreieck zum Aufklappen.
    Zeile(usize, bool),
    /// Verweis in der Fußzeile: geschätzte bzw. graue Zeilen.
    Geschaetzt,
    OhnePreis,
    /// Abgleichzeile: Text (Liste im Tooltip), „übernehmen“, „so lassen“.
    Abgleich,
    Uebernehmen,
    Lassen,
    /// „Bauleistung wählen …“ an der grauen Zeile.
    Waehlen(usize),
}

/// Was das Preisblatt schreiben lässt (die App führt es über
/// `Scene::kosten_folge` aus, paket-ka2 §4).
#[derive(Clone, Debug, PartialEq)]
pub enum Schreiben {
    /// `text`: Bezeichnung mit Wert für „Auch für neue Häuser“
    /// („Planstein 20,50 €/m² für dieses und neue Häuser“).
    Preis {
        ops: Vec<Op>,
        gilt: Gilt,
        text: String,
    },
    Zurueck(Vec<SatzId>),
    /// Abgleichzeile: Werte für neue Häuser übernehmen bzw. so lassen.
    Uebernehmen(Vec<SatzId>),
    Lassen(u32),
    /// „Bauleistung gewählt“ (`BauleistungZuordnen`).
    Bauleistung(Box<Op>),
}

/// Kosten beim Tippen im Preisblatt: Operationen und die Blätter darauf.
struct Live {
    ops: Vec<Op>,
    blatt: Rc<Kostenblatt>,
    ganz: Rc<Kostenblatt>,
}

/// Verweis an grauen Zeilen (Einstellungen §3 KA-2 Punkt 4).
const WAEHLEN: &str = "Bauleistung wählen …";

/// Doppelklick (ms), wie im Mengenblatt.
const DOUBLE_MS: u128 = 450;

/// Fußzeile: Text, Verweistext (Teil nach dem Doppelpunkt) und Ziel.
struct Fuss {
    text: String,
    verweis: String,
    ziel: Option<Hot>,
}

pub struct KostenView {
    pub leiste: Leiste,
    pub modus: Modus,
    pub gliederung: Gliederung,
    pub w: u32,
    pub h: u32,
    pub scale: f32,
    /// Oberkante des Blatts unter Titelleiste und Karten (dip).
    pub top: f32,
    blatt: Option<Rc<Kostenblatt>>,
    katalog: Option<Rc<Katalog>>,
    /// Kostenblatt aller Geschosse des Umfangs (Chip-Summen).
    ganz: Option<Rc<Kostenblatt>>,
    preisquelle: String,
    lohnsatz: Dez,
    zeilen: Vec<Zeile>,
    offen: HashSet<u64>,
    /// Woraus die Zeilen gebaut sind.
    gebaut: Option<(*const Kostenblatt, Modus, Gliederung, u64)>,
    offen_stand: u64,
    pub subtitle: String,
    stale: bool,
    scroll: f32,
    hot: Option<Hot>,
    button_down: bool,
    hover: Vec<ElementId>,
    selected: Vec<ElementId>,
    /// Breite des Fußes aus dem letzten Bild (Zeilenzahl der Fußzeile).
    fuss_zeilen: Cell<usize>,
    /// Preisblatt am EP (KA-2c), sein Öffnen beim nächsten `sync` und die
    /// Kosten beim Tippen.
    preis: Option<PreisBlatt>,
    preis_wunsch: Option<(usize, u64)>,
    live: Option<Live>,
    live_neu: bool,
    /// Letzter Klick auf einen EP (Zeile, Zeit) für den Doppelklick.
    klick: Option<(usize, Instant)>,
    /// Im Projekt geänderte Positionen mit dem EP der Firma (Punkt am EP,
    /// CSV-Spalte Projektabweichung) und woraus sie bestimmt sind.
    eigen: HashMap<usize, Cent>,
    eigen_von: Option<(*const Kostenblatt, *const Katalog)>,
    /// Abgleich mit dem Firmenkatalog (Regel 92) mit netto vorher und
    /// nachher für den Tooltip an „übernehmen“, und woraus er bestimmt ist.
    abgleich: Option<(Abgleich, Option<(Cent, Cent)>)>,
    abgleich_von: Option<(*const Kostenblatt, *const Katalog)>,
    /// Blatt „Bauleistung wählen …“, sein Öffnen beim nächsten `sync`
    /// (Zeile ohne Bauleistung und Bauteil) und die grauen Zeilen, für die
    /// es etwas zu wählen gibt.
    wahl: Option<WahlBlatt>,
    wahl_wunsch: Option<(usize, ElementId)>,
    waehlbar: HashSet<usize>,
    waehlbar_von: Option<(*const Kostenblatt, *const Katalog)>,
}

impl Default for KostenView {
    fn default() -> Self {
        KostenView::new()
    }
}

impl KostenView {
    pub fn new() -> KostenView {
        KostenView {
            leiste: Leiste::default(),
            modus: Modus::default(),
            gliederung: Gliederung::default(),
            w: 0,
            h: 0,
            scale: 1.0,
            top: 32.0,
            blatt: None,
            katalog: None,
            ganz: None,
            preisquelle: String::new(),
            lohnsatz: Dez::NULL,
            zeilen: Vec::new(),
            offen: HashSet::new(),
            gebaut: None,
            offen_stand: 0,
            subtitle: String::new(),
            stale: false,
            scroll: 0.0,
            hot: None,
            button_down: false,
            hover: Vec::new(),
            selected: Vec::new(),
            fuss_zeilen: Cell::new(0),
            preis: None,
            preis_wunsch: None,
            live: None,
            live_neu: false,
            klick: None,
            eigen: HashMap::new(),
            eigen_von: None,
            abgleich: None,
            abgleich_von: None,
            wahl: None,
            wahl_wunsch: None,
            waehlbar: HashSet::new(),
            waehlbar_von: None,
        }
    }

    /// Das angezeigte Kostenblatt (Karten, CSV, Prüfung).
    pub fn blatt(&self) -> Option<&Kostenblatt> {
        self.blatt.as_deref()
    }

    #[cfg(test)]
    pub fn zeilen(&self) -> &[Zeile] {
        &self.zeilen
    }

    /// Gesamtbetrag im Modus (Kachel netto).
    #[cfg(test)]
    pub fn netto(&self) -> Option<Cent> {
        let b = self.blatt.as_deref()?;
        Some(if self.modus.nur_material() {
            b.nur_material
        } else {
            b.netto
        })
    }

    /// An Modell und Katalog angleichen. `true`, wenn neu gezeichnet werden
    /// muss.
    pub fn sync(&mut self, s: &mut Scene, firma: Option<(&sk_model::Library, u64)>) -> bool {
        let mut changed = self.leiste.sync(s.model());
        let kat = s.katalog(firma);
        let blatt = s.kostenblatt(firma, &self.leiste.umfang);
        let alle = Umfang {
            gebaeude: self.leiste.umfang.gebaeude,
            ohne: Vec::new(),
        };
        let ganz = s.kostenblatt(firma, &alle);
        changed |= self.sync_eigen(s, firma, &kat, &blatt);
        changed |= self.sync_preis(s, firma, &kat, &blatt, &alle);
        changed |= self.sync_abgleich(s, firma, &kat, &blatt);
        changed |= self.sync_wahl(s, &kat, &blatt);
        // Beim Tippen im Preisblatt zeigen Zeilen, Summen und Chips die
        // Vorschau; Namen, Preisquelle und Lohn bleiben aus dem Katalog
        let (blatt, ganz) = match &self.live {
            Some(l) => (l.blatt.clone(), l.ganz.clone()),
            None => (blatt, ganz),
        };
        let stale = s.schedule_stale();
        if stale != self.stale {
            self.stale = stale;
            changed = true;
        }
        let neu_katalog = self.katalog.as_ref().is_none_or(|k| !Rc::ptr_eq(k, &kat));
        if neu_katalog {
            self.preisquelle = sk_cost::lesen::preisquelle(&kat);
            self.lohnsatz = kat.werte.lohn;
            self.katalog = Some(kat);
            changed = true;
        }
        let key = (
            Rc::as_ptr(&blatt),
            self.modus,
            self.gliederung,
            self.offen_stand,
        );
        if self.gebaut != Some(key) || neu_katalog {
            let k = self.katalog.as_deref().expect("eben gesetzt");
            self.zeilen = zeilen(
                s.model(),
                k,
                &blatt,
                self.gliederung,
                self.modus,
                &self.offen,
            );
            self.gebaut = Some(key);
            changed = true;
        }
        if self.ganz.as_ref().is_none_or(|g| !Rc::ptr_eq(g, &ganz)) || changed {
            self.leiste.summen = chip_summen(&ganz, self.leiste.chips(), self.modus)
                .into_iter()
                .map(euro_ganz)
                .collect();
            self.ganz = Some(ganz);
            changed = true;
        }
        if self.blatt.as_ref().is_none_or(|b| !Rc::ptr_eq(b, &blatt)) {
            self.blatt = Some(blatt);
            changed = true;
        }
        let text = format!(
            "{} · {}",
            umfang_view::umfang_text(
                s.model(),
                &self.leiste.umfang,
                sk_platform::local_date_time()
            ),
            self.preisquelle
        );
        if changed {
            self.subtitle = text;
        }
        self.clamp();
        changed
    }

    /// Punkte „im Projekt geändert“: Positionen, deren Bauleistung oder
    /// Artikel im Projekt vom Firmenkatalog abweichen, mit dem EP der Firma.
    fn sync_eigen(
        &mut self,
        s: &mut Scene,
        firma: Option<(&sk_model::Library, u64)>,
        kat: &Rc<Katalog>,
        blatt: &Rc<Kostenblatt>,
    ) -> bool {
        let von = (Rc::as_ptr(blatt), Rc::as_ptr(kat));
        if self.eigen_von == Some(von) {
            return false;
        }
        self.eigen_von = Some(von);
        let fk = s.firmenkatalog(firma);
        let m = s.model();
        let neu: HashMap<usize, Cent> = blatt
            .positionen
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let a = sk_cost::preis::aufbau(m, kat, p)?;
                if sk_cost::preis::abweichend(kat, &a).is_empty() {
                    return None;
                }
                Some((i, sk_cost::preis::aufbau(m, &fk, p).map_or(a.ep, |f| f.ep)))
            })
            .collect();
        let changed = neu != self.eigen;
        self.eigen = neu;
        changed
    }

    /// Abgleichzeile: Unterschiede zur Firma und was „übernehmen“ am
    /// Netto ändert (`Scene::kosten_live`).
    fn sync_abgleich(
        &mut self,
        s: &mut Scene,
        firma: Option<(&sk_model::Library, u64)>,
        kat: &Rc<Katalog>,
        blatt: &Rc<Kostenblatt>,
    ) -> bool {
        let von = (Rc::as_ptr(blatt), Rc::as_ptr(kat));
        if self.abgleich_von == Some(von) {
            return false;
        }
        self.abgleich_von = Some(von);
        let neu = sk_cost::abgleich::abgleich(s.model(), firma.map(|f| f.0)).map(|a| {
            let op = Op::StandUebernehmen {
                saetze: a.saetze.clone(),
            };
            let netto = s
                .kosten_live(firma, &[op], &[&self.leiste.umfang])
                .ok()
                .and_then(|(_, b)| Some((blatt.netto, b.first()?.netto)));
            (a, netto)
        });
        let changed = neu != self.abgleich;
        self.abgleich = neu;
        if changed {
            self.clamp();
        }
        changed
    }

    /// Graue Zeilen mit Wahl und das Blatt „Bauleistung wählen …“ öffnen.
    fn sync_wahl(&mut self, s: &Scene, kat: &Rc<Katalog>, blatt: &Rc<Kostenblatt>) -> bool {
        let mut changed = false;
        let von = (Rc::as_ptr(blatt), Rc::as_ptr(kat));
        if self.waehlbar_von != Some(von) {
            self.waehlbar_von = Some(von);
            self.waehlbar = blatt
                .ohne
                .iter()
                .enumerate()
                .filter(|(_, z)| sk_cost::wahl::waehlbar(kat, z))
                .map(|(j, _)| j)
                .collect();
            // Das Blatt gehört zu einer Zeile, die es so nicht mehr gibt
            if self.wahl.as_ref().is_some_and(|w| {
                blatt
                    .ohne
                    .get(w.ohne)
                    .is_none_or(|z| z.element != w.element)
            }) {
                self.wahl = None;
            }
            changed = true;
        }
        if let Some((j, el)) = self.wahl_wunsch.take() {
            let z = blatt.ohne.get(j).filter(|z| z.element == el);
            let a = z.and_then(|z| sk_cost::wahl::auswahl(s.model(), kat, z));
            let titel = self
                .zeilen
                .iter()
                .find(|x| x.ohne == Some(j))
                .map_or_else(String::new, |x| x.text.clone());
            if let (Some(z), Some(a)) = (z, a) {
                let mut w = WahlBlatt::neu((j, el), &titel, menge_text(z.menge, z.einheit), a);
                w.scale = self.scale;
                self.wahl = Some(w);
                changed = true;
            }
        }
        changed
    }

    /// Text der Abgleichzeile, wenn sie steht.
    #[cfg(test)]
    pub fn abgleich_zeile(&self) -> Option<String> {
        self.abgleich.as_ref().map(|(a, _)| a.zeile())
    }

    /// Preisblatt öffnen (Doppelklick auf den EP) und beim Tippen die
    /// Vorschau rechnen (`Scene::kosten_live`).
    fn sync_preis(
        &mut self,
        s: &mut Scene,
        firma: Option<(&sk_model::Library, u64)>,
        kat: &Rc<Katalog>,
        blatt: &Rc<Kostenblatt>,
        alle: &Umfang,
    ) -> bool {
        let mut changed = false;
        if let Some((pos, key)) = self.preis_wunsch.take() {
            let fk = s.firmenkatalog(firma);
            let m = s.model();
            let a = blatt
                .positionen
                .get(pos)
                .and_then(|p| sk_cost::preis::aufbau(m, kat, p));
            if let Some(a) = a {
                let f = sk_cost::preis::aufbau(m, &fk, &blatt.positionen[pos]);
                let mut pb =
                    PreisBlatt::neu(kat.clone(), a, f, (pos, key), preis_blatt::stand_jetzt());
                pb.scale = self.scale;
                self.preis = Some(pb);
                self.live = None;
                changed = true;
            }
        }
        if !std::mem::take(&mut self.live_neu) {
            return changed;
        }
        let Some(pb) = self.preis.as_mut() else {
            return changed;
        };
        let ops = pb.ops();
        if self.live.as_ref().is_some_and(|l| l.ops == ops) {
            return changed;
        }
        if ops.is_empty() {
            self.live = None;
            pb.set_live(Ok(None));
            return true;
        }
        match s.kosten_live(firma, &ops, &[&self.leiste.umfang, alle]) {
            Ok((k, mut b)) => {
                let ganz = b.pop().expect("zwei Umfänge");
                let blatt = b.pop().expect("zwei Umfänge");
                let a = blatt
                    .positionen
                    .get(pb.pos)
                    .and_then(|p| sk_cost::preis::aufbau(s.model(), &k, p));
                pb.set_live(Ok(a));
                self.live = Some(Live { ops, blatt, ganz });
            }
            Err(b) => {
                pb.set_live(Err(b.first().map_or_else(String::new, |x| x.satz.clone())));
                self.live = None;
            }
        }
        true
    }

    /// Ist das Preisblatt offen?
    #[cfg(test)]
    pub fn preis_offen(&self) -> bool {
        self.preis.is_some()
    }

    /// Ist ein Blatt offen (Preis oder Bauleistung; Tasten gehen dorthin)?
    pub fn blatt_offen(&self) -> bool {
        self.preis.is_some() || self.wahl.is_some()
    }

    /// Beide Blätter schließen, ohne zu schreiben.
    pub fn blaetter_schliessen(&mut self) -> bool {
        self.wahl.take().is_some() | self.preis_schliessen()
    }

    /// Ergebnis des Blatts „Bauleistung wählen …“.
    fn wahl_aus(&mut self, aus: wahl_blatt::Aus) -> Option<ListOut> {
        use wahl_blatt::Aus;
        Some(match aus {
            Aus::Repaint => ListOut::Repaint,
            Aus::Schliessen => {
                self.wahl = None;
                ListOut::Repaint
            }
            Aus::Waehlen(g) => {
                let w = self.wahl.take()?;
                let z = self.blatt.as_deref()?.ohne.get(w.ohne)?;
                ListOut::Kosten(Schreiben::Bauleistung(Box::new(sk_cost::wahl::zuordnen(
                    z, g,
                )?)))
            }
        })
    }

    /// Verweis „Bauleistung wählen …“ rechts in einer grauen Zeile (px).
    fn waehlen_rect(&self, t: &Theme, fonts: &Fonts, y: f32, h: f32) -> Rect {
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let px = 11.0 * s;
        let w = fonts
            .bold
            .as_ref()
            .or(fonts.regular.as_ref())
            .map_or(120.0 * s, |f| f.width(WAEHLEN, px));
        (x0 + cw - w, y, w, h)
    }

    /// Zeile `i` zeigt beim Überfahren „Bauleistung wählen …“.
    fn zeigt_waehlen(&self, i: usize) -> bool {
        self.zeilen
            .get(i)
            .and_then(|z| z.ohne)
            .is_some_and(|j| self.waehlbar.contains(&j))
    }

    /// Preisblatt schließen, ohne zu schreiben.
    pub fn preis_schliessen(&mut self) -> bool {
        self.live = None;
        self.preis.take().is_some()
    }

    /// Ergebnis des Preisblatts an die Ansicht und das Mengenfenster.
    fn preis_aus(&mut self, aus: preis_blatt::Aus) -> Option<ListOut> {
        use preis_blatt::Aus;
        Some(match aus {
            Aus::Repaint => ListOut::Repaint,
            Aus::Live => {
                self.live_neu = true;
                ListOut::Repaint
            }
            Aus::Verwerfen => {
                self.preis_schliessen();
                ListOut::Repaint
            }
            Aus::Anwenden { ops, gilt, text } => {
                self.preis_schliessen();
                if ops.is_empty() {
                    ListOut::Repaint
                } else {
                    ListOut::Kosten(Schreiben::Preis { ops, gilt, text })
                }
            }
            Aus::Zuruecknehmen(saetze) => {
                self.preis_schliessen();
                ListOut::Kosten(Schreiben::Zurueck(saetze))
            }
        })
    }

    /// EP-Zelle (px) der Zeile mit dem Schlüssel `key`, wenn sie zu sehen ist.
    fn ep_zelle(&self, t: &Theme, key: u64) -> Option<Rect> {
        let (x0, cw) = self.content_x(t);
        let s = self.scale;
        let r = col_ep(x0, cw);
        self.sichtbar()
            .into_iter()
            .find(|(i, _, _)| {
                let z = &self.zeilen[*i];
                z.key == key && matches!(z.art, Art::Position { .. })
            })
            .map(|(_, y, h)| (r - 70.0 * s, y, r + 4.0 * s, y + h))
    }

    /// Blatt „Bauleistung wählen …“ an den Verweis der Zeile legen.
    fn lege_wahl(&mut self, t: &Theme) {
        let Some(j) = self.wahl.as_ref().map(|w| w.ohne) else {
            return;
        };
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let anker = self
            .sichtbar()
            .into_iter()
            .find(|(i, _, _)| self.zeilen[*i].ohne == Some(j))
            .map(|(_, y, h)| (x0 + cw - 120.0 * s, y, x0 + cw, y + h));
        let (w, h) = (self.w as f32, self.h as f32);
        let wb = self.wahl.as_mut().expect("eben gesehen");
        if let Some(r) = anker {
            wb.set_anker(r);
        }
        wb.fenster = (w, h);
        wb.scale = s;
    }

    /// Preisblatt an die EP-Zelle und die Fenstergröße legen.
    fn lege_preis(&mut self, t: &Theme) {
        self.lege_wahl(t);
        let Some(key) = self.preis.as_ref().map(|p| p.key) else {
            return;
        };
        let anker = self.ep_zelle(t, key);
        let (w, h, s) = (self.w as f32, self.h as f32, self.scale);
        let pb = self.preis.as_mut().expect("eben gesehen");
        if let Some(r) = anker {
            pb.set_anker(r);
        }
        pb.fenster = (w, h);
        pb.scale = s;
    }

    /// Taste fürs Preisblatt; `None`, wenn es nicht offen ist.
    pub fn key(
        &mut self,
        t: &Theme,
        key: sk_platform::Key,
        mods: sk_platform::Modifiers,
    ) -> Option<Option<ListOut>> {
        self.lege_preis(t);
        if let Some(w) = self.wahl.as_mut() {
            let aus = w.key(key, mods);
            return Some(aus.and_then(|a| self.wahl_aus(a)));
        }
        let aus = self.preis.as_mut()?.key(key, mods);
        Some(aus.and_then(|a| self.preis_aus(a)))
    }

    /// Getipptes Zeichen fürs Preisblatt.
    pub fn text(&mut self, ch: char) -> Option<ListOut> {
        if let Some(w) = self.wahl.as_mut() {
            let aus = w.text(ch)?;
            return self.wahl_aus(aus);
        }
        let aus = self.preis.as_mut()?.text(ch)?;
        self.preis_aus(aus)
    }

    /// Auswahl aus dem gemeinsamen Zustand übernehmen; `true`, wenn sich
    /// das Bild ändert.
    pub fn follow(&mut self, p: &Picking) -> bool {
        let hover: Vec<ElementId> = p.hovered().collect();
        let changed = hover != self.hover || p.selected != self.selected;
        self.hover = hover;
        self.selected = p.selected.clone();
        changed
    }

    // --- Lage ----------------------------------------------------------------

    fn content_x(&self, t: &Theme) -> (f32, f32) {
        let s = self.scale;
        let pad = t.size.sheet_pad * s;
        let w = (self.w as f32 - 2.0 * pad)
            .min(t.size.qto_max_w * s)
            .max(0.0);
        (pad, w)
    }

    fn top_px(&self) -> f32 {
        self.top * self.scale
    }

    /// Versatz unter der Abgleichzeile (dip).
    fn ab(&self) -> f32 {
        if self.abgleich.is_some() {
            ABGLEICH_H
        } else {
            0.0
        }
    }

    /// Höhe des Kopfs (dip).
    fn head(&self) -> f32 {
        HEAD + self.ab()
    }

    /// Erste Zeile der Liste (px).
    fn list_top(&self) -> f32 {
        self.top_px() + self.head() * self.scale
    }

    /// Abgleichzeile: x des Texts, Grundlinie, Text (gekürzt), Textfläche,
    /// „übernehmen“ und „so lassen“ (px).
    fn abgleich_lage(
        &self,
        t: &Theme,
        fonts: &Fonts,
    ) -> Option<(f32, f32, String, Rect, Rect, Rect)> {
        let (a, _) = self.abgleich.as_ref()?;
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let px = 11.0 * s;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let breite = |f: Option<&sk_paint::font::Font>, text: &str| {
            f.map_or(text.chars().count() as f32 * px * 0.55, |f| {
                f.width(text, px)
            })
        };
        let base = self.top_px() + ABGLEICH_Y * s;
        let x = x0 + 12.0 * s;
        let sep = breite(regular, " · ");
        let (wu, wl) = (breite(bold, "übernehmen"), breite(bold, "so lassen"));
        let platz = (x0 + cw - x - 2.0 * sep - wu - wl).max(0.0);
        let text = sk_ui::widgets::ellipsize(regular, &a.zeile(), px, platz);
        let wt = breite(regular, &text);
        let (y, h) = (base - 14.0 * s, ABGLEICH_H * s);
        let u = x + wt + sep;
        let l = u + wu + sep;
        Some((x, base, text, (x, y, wt, h), (u, y, wu, h), (l, y, wl, h)))
    }

    /// Oberkante der Kacheln (px).
    fn tiles_top(&self) -> f32 {
        let s = self.scale;
        let fuss = self.fuss_zeilen.get().max(1) as f32 * FOOT_LINE;
        self.h as f32 - (BOTTOM_PAD + fuss + TILE_GAP + TILE_H) * s
    }

    fn row_h(z: &Zeile) -> f32 {
        match z.art {
            Art::Gruppe if z.ebene == 0 => ROW_GROUP0,
            Art::Gruppe | Art::Ausgleich => ROW_GROUP1,
            Art::Ansatz => ROW_ANSATZ,
            _ => ROW_POS,
        }
    }

    fn content_h(&self) -> f32 {
        self.zeilen.iter().map(Self::row_h).sum::<f32>() + 8.0
    }

    fn view_h(&self) -> f32 {
        ((self.tiles_top() - self.list_top()) / self.scale).max(0.0)
    }

    fn clamp(&mut self) {
        let max = (self.content_h() - self.view_h()).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// Zeilen mit ihrer Oberkante (px) im sichtbaren Bereich.
    fn sichtbar(&self) -> Vec<(usize, f32, f32)> {
        let s = self.scale;
        let (top, bottom) = (self.list_top(), self.tiles_top());
        let mut y = top - self.scroll * s;
        let mut out = Vec::new();
        for (i, z) in self.zeilen.iter().enumerate() {
            let h = Self::row_h(z) * s;
            if y + h > top && y < bottom {
                out.push((i, y, h));
            }
            y += h;
            if y >= bottom {
                break;
            }
        }
        out
    }

    fn leiste_lage(&self, t: &Theme) -> umfang_view::Lage {
        umfang_view::Lage {
            x0: self.content_x(t).0,
            y: self.top_px() + (CHIP_TOP + self.ab()) * self.scale,
            s: self.scale,
        }
    }

    fn button_rect(&self, t: &Theme, fonts: &Fonts) -> Rect {
        let s = self.scale;
        let (x0, w) = self.content_x(t);
        let tw = fonts
            .bold
            .as_ref()
            .or(fonts.regular.as_ref())
            .map_or(130.0 * s, |f| f.width("Als Tabelle speichern", 11.0 * s));
        let bw = tw + 2.0 * BUTTON_PAD * s;
        (x0 + w - bw, self.top_px() + 14.0 * s, bw, BUTTON_H * s)
    }

    /// Segmentschalter: Beschriftung (x, Grundlinie) und Segmente.
    #[allow(clippy::type_complexity)]
    fn schalter(
        &self,
        t: &Theme,
        fonts: &Fonts,
    ) -> ((f32, Vec<(Modus, Rect)>), (f32, Vec<(Gliederung, Rect)>)) {
        let s = self.scale;
        let (x0, _) = self.content_x(t);
        let px = 10.5 * s;
        let y = self.top_px() + (SWITCH_TOP + self.ab()) * s;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let w = |text: &str| {
            bold.map_or(text.chars().count() as f32 * px * 0.6, |f| {
                f.width(text, px)
            }) + 20.0 * s
        };
        let label_w = |text: &str| {
            regular.map_or(text.chars().count() as f32 * px * 0.55, |f| {
                f.width(text, px)
            }) + 8.0 * s
        };
        let inset = 2.0 * s;
        let mut x = x0 + label_w("Preise") + inset;
        let mut preise = Vec::new();
        for m in Modus::ALLE {
            let sw = w(m.label());
            preise.push((m, (x, y + inset, sw, SWITCH_H * s - 2.0 * inset)));
            x += sw;
        }
        let gx = (x0 + GLIEDERN_X * s).max(x + 24.0 * s);
        let mut x = gx + label_w("Gliedern") + inset;
        let mut gl = Vec::new();
        for g in Gliederung::ALLE {
            let sw = w(g.label());
            gl.push((g, (x, y + inset, sw, SWITCH_H * s - 2.0 * inset)));
            x += sw;
        }
        ((x0, preise), (gx, gl))
    }

    fn fuss(&self) -> Vec<Fuss> {
        let Some(b) = self.blatt.as_deref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if b.geschaetzt > 0 {
            out.push(Fuss {
                text: "davon geschätzt: ".into(),
                verweis: format!(
                    "{} {}, {} €",
                    b.geschaetzt,
                    if b.geschaetzt == 1 { "Zeile" } else { "Zeilen" },
                    euro(b.geschaetzt_betrag)
                ),
                ziel: Some(Hot::Geschaetzt),
            });
        }
        if !b.ohne.is_empty() {
            let mut namen: Vec<String> = Vec::new();
            for z in self.zeilen.iter().filter(|z| z.art == Art::Ohne) {
                let n = z.leise.split(" · ").next().unwrap_or_default().to_string();
                if !n.is_empty() && !namen.contains(&n) {
                    namen.push(n);
                }
            }
            out.push(Fuss {
                text: "Ohne Preis: ".into(),
                verweis: format!(
                    "{} ({} {} ohne Bauleistung)",
                    namen.join(", "),
                    b.ohne.len(),
                    if b.ohne.len() == 1 { "Zeile" } else { "Zeilen" }
                ),
                ziel: Some(Hot::OhnePreis),
            });
        }
        if b.unvollstaendig > 0 {
            out.push(Fuss {
                text: format!("unvollständig: {} Positionen ohne Preis", b.unvollstaendig),
                verweis: String::new(),
                ziel: None,
            });
        }
        if self.modus.nur_material() {
            let n = b.positionen.iter().filter(|p| p.nu.is_some()).count();
            if n > 0 {
                out.push(Fuss {
                    text: format!("ohne {n} NU-Positionen (Summe {} €)", euro(b.nu)),
                    verweis: String::new(),
                    ziel: None,
                });
            }
        }
        out
    }

    /// Verweise der Fußzeile (px).
    fn fuss_rects(&self, t: &Theme, fonts: &Fonts) -> Vec<(Hot, Rect)> {
        let s = self.scale;
        let (x0, _) = self.content_x(t);
        let px = 10.5 * s;
        let regular = fonts.regular.as_ref();
        let mut y = self.tiles_top() + (TILE_H + TILE_GAP) * s;
        let mut out = Vec::new();
        for f in self.fuss() {
            if let (Some(z), Some(r)) = (f.ziel, regular) {
                let x = x0 + r.width(&f.text, px);
                out.push((z, (x, y, r.width(&f.verweis, px), FOOT_LINE * s)));
            }
            y += FOOT_LINE * s;
        }
        out
    }

    fn hit(&self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<Hot> {
        let (x, y) = (x as f32, y as f32);
        if let Some(h) = self.leiste.hit(fonts, self.leiste_lage(t), x, y) {
            return Some(Hot::Umfang(h));
        }
        if self.leiste.field_open() {
            return None;
        }
        if inside(self.button_rect(t, fonts), x, y) {
            return Some(Hot::Button);
        }
        if let Some((_, _, _, text, u, l)) = self.abgleich_lage(t, fonts) {
            for (r, h) in [
                (u, Hot::Uebernehmen),
                (l, Hot::Lassen),
                (text, Hot::Abgleich),
            ] {
                if inside(r, x, y) {
                    return Some(h);
                }
            }
        }
        let ((_, preise), (_, gl)) = self.schalter(t, fonts);
        if let Some((m, _)) = preise.iter().find(|(_, r)| inside(*r, x, y)) {
            return Some(Hot::Modus(*m));
        }
        if let Some((g, _)) = gl.iter().find(|(_, r)| inside(*r, x, y)) {
            return Some(Hot::Gliederung(*g));
        }
        if let Some((h, _)) = self
            .fuss_rects(t, fonts)
            .into_iter()
            .find(|(_, r)| inside(*r, x, y))
        {
            return Some(h);
        }
        if y < self.list_top() || y >= self.tiles_top() {
            return None;
        }
        let (x0, _) = self.content_x(t);
        let s = self.scale;
        self.sichtbar()
            .into_iter()
            .find(|(_, ry, rh)| y >= *ry && y < ry + rh)
            .map(|(i, ry, rh)| {
                if self.zeigt_waehlen(i) && inside(self.waehlen_rect(t, fonts, ry, rh), x, y) {
                    return Hot::Waehlen(i);
                }
                let z = &self.zeilen[i];
                let dx = x0 + z.ebene as f32 * INDENT * s;
                let dreieck =
                    matches!(z.art, Art::Position { .. }) && x >= dx - 4.0 * s && x < dx + 12.0 * s;
                Hot::Zeile(i, dreieck)
            })
    }

    /// Hover der Bauteile einer Zeile: einzeln oder als Gruppe.
    fn hover_of(&self, hot: Option<Hot>) -> (Option<ElementId>, Vec<ElementId>) {
        match hot {
            Some(Hot::Zeile(i, _)) => {
                let z = &self.zeilen[i];
                match z.elements.as_slice() {
                    [e] => (Some(*e), Vec::new()),
                    v => (None, v.to_vec()),
                }
            }
            _ => (None, Vec::new()),
        }
    }

    // --- Ereignisse ----------------------------------------------------------

    pub fn mouse_move(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        p: &mut Picking,
        x: f64,
        y: f64,
    ) -> Option<ListOut> {
        self.lege_preis(t);
        if let Some(w) = self.wahl.as_mut() {
            let repaint = w.mouse_move(x as f32, y as f32);
            if w.enthaelt(x as f32, y as f32) {
                let mut out = repaint.then_some(ListOut::Repaint);
                if self.hot.take().is_some() {
                    out = Some(ListOut::Repaint);
                }
                return out;
            }
        }
        if let Some(pb) = self.preis.as_mut() {
            let repaint = pb.mouse_move(fonts, x as f32, y as f32);
            if pb.enthaelt(x as f32, y as f32) {
                let mut out = repaint.then_some(ListOut::Repaint);
                if self.hot.take().is_some() {
                    out = Some(ListOut::Repaint);
                }
                if p.set_hover(None, Vec::new()) {
                    self.hover.clear();
                    out = Some(ListOut::Picking { selection: false });
                }
                return out;
            }
            if repaint {
                self.hot = self.hit(t, fonts, x, y);
                return Some(ListOut::Repaint);
            }
        }
        let hot = self.hit(t, fonts, x, y);
        let look = |h: Option<Hot>| match h {
            Some(Hot::Zeile(i, _)) if self.zeigt_waehlen(i) => h,
            Some(Hot::Zeile(..)) | None => None,
            h => h,
        };
        let repaint = look(hot) != look(self.hot);
        self.hot = hot;
        self.leiste.hot = match hot {
            Some(Hot::Umfang(h)) => Some(h),
            _ => None,
        };
        let (one, group) = self.hover_of(hot);
        if p.set_hover(one, group) {
            self.hover = p.hovered().collect();
            return Some(ListOut::Picking { selection: false });
        }
        repaint.then_some(ListOut::Repaint)
    }

    pub fn mouse_leave(&mut self, p: &mut Picking) -> Option<ListOut> {
        let repaint = self.hot.take().is_some() || self.button_down;
        self.button_down = false;
        self.leiste.hot = None;
        if p.set_hover(None, Vec::new()) {
            self.hover.clear();
            return Some(ListOut::Picking { selection: false });
        }
        repaint.then_some(ListOut::Repaint)
    }

    pub fn mouse_down(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        p: &mut Picking,
        (x, y): (f64, f64),
        mods: sk_platform::Modifiers,
    ) -> Option<ListOut> {
        self.lege_preis(t);
        if let Some(w) = self.wahl.as_mut() {
            // Klick daneben schließt ohne Spur
            let aus = w.mouse_down(fonts, x as f32, y as f32)?;
            return self.wahl_aus(aus);
        }
        if let Some(pb) = self.preis.as_mut() {
            // Klick daneben schreibt und schließt (wie Enter)
            let aus = pb.mouse_down(fonts, x as f32, y as f32)?;
            return self.preis_aus(aus);
        }
        let hot = self.hit(t, fonts, x, y);
        if let Some(Hot::Umfang(h)) = hot {
            self.leiste.click(h, mods.ctrl);
            return Some(ListOut::Repaint);
        }
        if self.leiste.close_field() {
            return Some(ListOut::Repaint);
        }
        match hot? {
            Hot::Umfang(_) => None,
            Hot::Button => {
                self.button_down = true;
                Some(ListOut::Repaint)
            }
            Hot::Modus(m) => (m != self.modus).then(|| {
                self.modus = m;
                self.gebaut = None;
                ListOut::Repaint
            }),
            Hot::Gliederung(g) => (g != self.gliederung).then(|| {
                self.gliederung = g;
                self.gebaut = None;
                self.scroll = 0.0;
                ListOut::Repaint
            }),
            Hot::Zeile(i, dreieck) => {
                let z = &self.zeilen[i];
                if dreieck {
                    let key = z.key;
                    if !self.offen.remove(&key) {
                        self.offen.insert(key);
                    }
                    self.offen_stand += 1;
                    return Some(ListOut::Repaint);
                }
                // Doppelklick auf den EP öffnet das Preisblatt
                let (zpos, zart, zkey) = (z.pos, z.art, z.key);
                if let (Some((pos, _)), Art::Position { .. }) = (zpos, zart) {
                    let (x0, cw) = self.content_x(t);
                    let r = col_ep(x0, cw) as f64;
                    let s = self.scale as f64;
                    if x >= r - 70.0 * s && x < r + 4.0 * s {
                        let now = Instant::now();
                        let doppelt = self.klick.is_some_and(|(j, at)| {
                            j == i && now.duration_since(at).as_millis() < DOUBLE_MS
                        });
                        if doppelt {
                            self.klick = None;
                            self.preis_wunsch = Some((pos, zkey));
                            return Some(ListOut::Repaint);
                        }
                        self.klick = Some((i, now));
                    }
                }
                let els = self.zeilen[i].elements.clone();
                match els.as_slice() {
                    [] => None,
                    [e] => {
                        p.click(*e, mods.ctrl);
                        self.selected = p.selected.clone();
                        Some(ListOut::Picking { selection: true })
                    }
                    v => {
                        p.selected = v.to_vec();
                        self.selected = p.selected.clone();
                        Some(ListOut::Picking { selection: true })
                    }
                }
            }
            Hot::Geschaetzt => self.springe(|z| z.geschaetzt),
            Hot::OhnePreis => self.springe(|z| z.art == Art::Ohne),
            Hot::Abgleich => None,
            Hot::Waehlen(i) => {
                let j = self.zeilen.get(i)?.ohne?;
                let el = self.blatt.as_deref()?.ohne.get(j)?.element;
                self.wahl_wunsch = Some((j, el));
                Some(ListOut::Repaint)
            }
            Hot::Uebernehmen => {
                let (a, _) = self.abgleich.as_ref()?;
                Some(ListOut::Kosten(Schreiben::Uebernehmen(a.saetze.clone())))
            }
            Hot::Lassen => {
                let (a, _) = self.abgleich.as_ref()?;
                Some(ListOut::Kosten(Schreiben::Lassen(a.stand)))
            }
        }
    }

    /// Zur ersten Zeile, die `f` erfüllt, rollen.
    fn springe(&mut self, f: impl Fn(&Zeile) -> bool) -> Option<ListOut> {
        let mut y = 0.0;
        for z in &self.zeilen {
            if f(z) {
                self.scroll = (y - 2.0 * ROW_POS).max(0.0);
                self.clamp();
                return Some(ListOut::Repaint);
            }
            y += Self::row_h(z);
        }
        None
    }

    pub fn mouse_up(&mut self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<ListOut> {
        if !std::mem::take(&mut self.button_down) {
            return None;
        }
        if self.hit(t, fonts, x, y) == Some(Hot::Button) {
            Some(ListOut::SaveCsv)
        } else {
            Some(ListOut::Repaint)
        }
    }

    pub fn wheel(&mut self, delta: f64, t: &Theme) -> Option<ListOut> {
        // Das Blatt hängt am EP: solange es offen ist, rollt die Liste nicht
        if let Some(w) = self.wahl.as_mut() {
            return w.wheel(delta).then_some(ListOut::Repaint);
        }
        if self.preis.is_some() {
            return None;
        }
        self.scroll -= delta as f32 * 3.0 * t.size.qto_row;
        self.clamp();
        Some(ListOut::Repaint)
    }

    pub fn tick(&mut self, t: &Theme, now: Instant) -> bool {
        self.lege_preis(t);
        self.leiste.tick(t, now)
    }

    pub fn overlay_open(&self) -> bool {
        self.leiste.overlay_open()
    }

    pub fn tip_at(&self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<String> {
        if let Some(w) = self.wahl.as_ref() {
            if w.enthaelt(x as f32, y as f32) {
                return w.tip_at(x as f32, y as f32);
            }
        }
        if let Some(pb) = self.preis.as_ref() {
            if pb.enthaelt(x as f32, y as f32) {
                return pb.tip_at(fonts, x as f32, y as f32);
            }
        }
        match self.hit(t, fonts, x, y)? {
            Hot::Umfang(h) => self.leiste.tip(h),
            // Die OZ steht auf dem Bildschirm nur hier (Namen statt
            // Kennungen, Bedienbarkeit 4.9)
            Hot::Zeile(i, _) => {
                let (p, _) = self.zeilen.get(i)?.pos?;
                // Punkt am EP: im Projekt geändert (soll-ka-2c)
                let (x0, cw) = self.content_x(t);
                let r = col_ep(x0, cw) as f64;
                if let Some(f) = self.eigen.get(&p) {
                    if x >= r - 70.0 * self.scale as f64 && x < r + 4.0 * self.scale as f64 {
                        return Some(format!("im Projekt geändert, Firma {}", f.deutsch()));
                    }
                }
                let oz = &self.blatt.as_deref()?.positionen.get(p)?.oz;
                (!oz.is_empty()).then(|| format!("OZ {oz}"))
            }
            Hot::Abgleich => {
                let (a, _) = self.abgleich.as_ref()?;
                (a.texte.len() > 2)
                    .then(|| format!("Für neue Häuser gilt:\n{}", a.texte.join("\n")))
            }
            Hot::Uebernehmen => {
                let (_, netto) = self.abgleich.as_ref()?;
                let (v, n) = (*netto)?;
                Some(format!(
                    "Mit den Werten für neue Häuser: {} € → {} € netto · Eigene Werte dieses Hauses bleiben stehen.",
                    euro(v),
                    euro(n)
                ))
            }
            Hot::Gliederung(Gliederung::Kostengruppe) => Some(
                "Kostengruppe nach DIN 276, wie sie Architekten und Bauherren verwenden".into(),
            ),
            _ => None,
        }
    }

    // --- Zeichnen ------------------------------------------------------------

    pub fn paint(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, now: Instant) {
        let u = &t.ui;
        let s = self.scale;
        let fuss = self.fuss();
        self.fuss_zeilen.set(fuss.len());
        c.fill_rect(0.0, self.top_px(), self.w as f32, self.h as f32, u.sheet_bg);
        self.paint_rows(c, t, fonts);
        // Kopf deckt weggerollte Zeilen ab
        c.fill_rect(
            0.0,
            self.top_px(),
            self.w as f32,
            self.head() * s,
            u.sheet_bg,
        );
        self.paint_head(c, t, fonts, now);
        let tt = self.tiles_top();
        c.fill_rect(
            0.0,
            tt - 4.0 * s,
            self.w as f32,
            self.h as f32 - tt + 4.0 * s,
            u.sheet_bg,
        );
        self.paint_tiles(c, t, fonts);
        self.paint_fuss(c, t, fonts, &fuss);
        self.paint_scrollbar(c, t);
        self.leiste
            .paint_field_list(c, t, fonts, self.leiste_lage(t));
        if let Some(pb) = &self.preis {
            pb.paint(c, t, fonts);
        }
        if let Some(w) = &self.wahl {
            w.paint(c, t, fonts);
        }
    }

    fn paint_head(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, now: Instant) {
        let s = self.scale;
        let u = &t.ui;
        let top = self.top_px();
        let (x0, cw) = self.content_x(t);
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        if let Some(f) = bold {
            f.draw(c, "Kosten", 19.0 * s, x0, top + TITLE_Y * s, u.sheet_text);
            if self.stale {
                if let Some(r) = regular {
                    let x = x0 + f.width("Kosten", 19.0 * s) + 12.0 * s;
                    r.draw(
                        c,
                        "wird aktualisiert",
                        10.5 * s,
                        x,
                        top + TITLE_Y * s,
                        u.sheet_text_dim,
                    );
                }
            }
        }
        if let Some(f) = regular {
            let text = sk_ui::widgets::ellipsize(Some(f), &self.subtitle, 10.5 * s, cw);
            f.draw(c, &text, 10.5 * s, x0, top + SUB_Y * s, u.sheet_text_dim);
        }
        self.paint_abgleich(c, t, fonts);
        // Knopf „Als Tabelle speichern“
        let (bx, by, bw, bh) = self.button_rect(t, fonts);
        let bg = if self.button_down {
            u.pressed
        } else if self.hot == Some(Hot::Button) {
            u.hover
        } else {
            u.bg
        };
        let mut p = Path::new();
        p.rounded_rect(bx, by, bw, bh, t.size.corner_radius * s);
        c.fill(&p, bg);
        if let Some(f) = bold {
            let px = 11.0 * s;
            f.draw(
                c,
                "Als Tabelle speichern",
                px,
                bx + BUTTON_PAD * s,
                by + (bh + f.cap_height(px)) * 0.5,
                u.text,
            );
        }
        self.leiste.paint(c, t, fonts, self.leiste_lage(t), now);
        // Schalter
        let ((px0, preise), (gx, gl)) = self.schalter(t, fonts);
        self.paint_segmente(
            c,
            t,
            fonts,
            (px0, "Preise"),
            &preise,
            |m| m == self.modus,
            |m| self.hot == Some(Hot::Modus(m)),
            Modus::label,
        );
        self.paint_segmente(
            c,
            t,
            fonts,
            (gx, "Gliedern"),
            &gl,
            |g| g == self.gliederung,
            |g| self.hot == Some(Hot::Gliederung(g)),
            Gliederung::label,
        );
        // Spaltenköpfe und Linie
        if let Some(f) = regular {
            let px = 10.0 * s;
            let base = top + (self.head() - 12.0) * s;
            let (ep, gp) = if self.modus.nur_material() {
                ("Stoff-EP", "Stoff-GP")
            } else {
                ("EP", "GP")
            };
            let first = match self.gliederung {
                Gliederung::Gewerk => "Gewerk · Position",
                Gliederung::Geschoss => "Geschoss · Gewerk · Position",
                Gliederung::Kostengruppe => "Kostengruppe · Position",
            };
            f.draw(c, first, px, x0, base, u.sheet_text_dim);
            for (text, right) in [
                ("Menge", col_menge(x0, cw)),
                (ep, col_ep(x0, cw)),
                (gp, x0 + cw),
            ] {
                f.draw(
                    c,
                    text,
                    px,
                    right - f.width(text, px),
                    base,
                    u.sheet_text_dim,
                );
            }
        }
        c.fill_rect(
            x0,
            top + (self.head() - 6.0) * s,
            cw,
            s.max(1.0),
            u.sheet_rule,
        );
    }

    /// „● Für neue Häuser gilt … · übernehmen · so lassen“.
    fn paint_abgleich(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let Some((x, base, text, _, (ux, _, _, _), (lx, _, _, _))) = self.abgleich_lage(t, fonts)
        else {
            return;
        };
        let (s, u) = (self.scale, &t.ui);
        let px = 11.0 * s;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let (x0, _) = self.content_x(t);
        let cap = regular.map_or(8.0 * s, |f| f.cap_height(px));
        let mut p = Path::new();
        let d = 3.0 * s;
        p.rounded_rect(x0, base - cap * 0.5 - d, 2.0 * d, 2.0 * d, d);
        c.fill(&p, u.accent);
        if let Some(f) = regular {
            f.draw(c, &text, px, x, base, u.sheet_text);
            f.draw(
                c,
                " · ",
                px,
                ux - f.width(" · ", px),
                base,
                u.sheet_text_dim,
            );
            f.draw(
                c,
                " · ",
                px,
                lx - f.width(" · ", px),
                base,
                u.sheet_text_dim,
            );
        }
        if let Some(f) = bold {
            let unter = |h| self.hot == Some(h);
            let farbe = |h, c| if unter(h) { u.text } else { c };
            f.draw(
                c,
                "übernehmen",
                px,
                ux,
                base,
                farbe(Hot::Uebernehmen, u.accent),
            );
            f.draw(
                c,
                "so lassen",
                px,
                lx,
                base,
                farbe(Hot::Lassen, u.sheet_text_dim),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_segmente<T: Copy>(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        (lx, label): (f32, &str),
        segs: &[(T, Rect)],
        on: impl Fn(T) -> bool,
        hover: impl Fn(T) -> bool,
        text: impl Fn(T) -> &'static str,
    ) {
        let (Some(&(_, first)), Some(&(_, last))) = (segs.first(), segs.last()) else {
            return;
        };
        let s = self.scale;
        let u = &t.ui;
        let px = 10.5 * s;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let inset = 2.0 * s;
        let (x, y, h) = (first.0 - inset, first.1 - inset, first.3 + 2.0 * inset);
        let w = last.0 + last.2 + inset - x;
        let mut p = Path::new();
        p.rounded_rect(x, y, w, h, h * 0.5);
        c.fill(&p, u.sheet_tile);
        if let Some(f) = regular {
            f.draw(
                c,
                label,
                px,
                lx,
                y + (h + f.cap_height(px)) * 0.5,
                u.sheet_text_dim,
            );
        }
        for &(v, (sx, sy, sw, sh)) in segs {
            let (font, col) = if on(v) {
                let mut p = Path::new();
                p.rounded_rect(sx, sy, sw, sh, sh * 0.5);
                c.fill(&p, u.sheet_card);
                (bold, u.sheet_text)
            } else if hover(v) {
                (regular, u.sheet_text)
            } else {
                (regular, u.sheet_text_dim)
            };
            let Some(f) = font else { continue };
            let tw = f.width(text(v), px);
            f.draw(
                c,
                text(v),
                px,
                sx + (sw - tw) * 0.5,
                sy + (sh + f.cap_height(px)) * 0.5,
                col,
            );
        }
    }

    fn paint_rows(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let s = self.scale;
        let u = &t.ui;
        let (x0, cw) = self.content_x(t);
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let italic = fonts.italic.as_ref().or(regular);
        for (i, y, h) in self.sichtbar() {
            let z = &self.zeilen[i];
            // Band: Auswahl, Hover
            let sel =
                !z.elements.is_empty() && z.elements.iter().all(|e| self.selected.contains(e));
            let hov = !z.elements.is_empty() && z.elements.iter().all(|e| self.hover.contains(e));
            if sel {
                c.fill_rect(x0 - 10.0 * s, y, cw + 20.0 * s, h, u.sheet_select);
                c.fill_rect(x0 - 10.0 * s, y, 3.0 * s, h, u.accent);
            } else if hov
                || self.hot == Some(Hot::Zeile(i, false))
                || self.hot == Some(Hot::Zeile(i, true))
            {
                c.fill_rect(x0 - 10.0 * s, y, cw + 20.0 * s, h, u.sheet_hover);
            }
            let dx = x0 + z.ebene as f32 * INDENT * s;
            let (font, px, col) = match z.art {
                Art::Gruppe if z.ebene == 0 => (bold, 12.0 * s, u.sheet_text),
                Art::Gruppe | Art::Ausgleich => (bold, 11.0 * s, u.sheet_text),
                Art::Position { .. } if z.geschaetzt => (italic, 11.0 * s, u.sheet_text),
                Art::Position { .. } => (regular, 11.0 * s, u.sheet_text),
                Art::Ansatz => (regular, 10.5 * s, u.sheet_text_dim),
                Art::Ohne => (regular, 11.0 * s, u.sheet_hint),
            };
            let Some(f) = font else { continue };
            let base = y + (h + f.cap_height(px)) * 0.5;
            let tx = match z.art {
                Art::Position { offen } => {
                    sk_ui::widgets::disclosure(
                        c,
                        dx + 4.0 * s,
                        y + h * 0.5,
                        offen,
                        u.sheet_text_dim,
                        s,
                    );
                    dx + 14.0 * s
                }
                _ => dx,
            };
            // Text links bis vor die Mengenspalte, leise dahinter
            let menge_x = col_menge(x0, cw);
            let room = menge_x - f.width("0.000,000 m²", px) - 12.0 * s - tx;
            let text = sk_ui::widgets::ellipsize(Some(f), &z.text, px, room.max(0.0));
            f.draw(c, &text, px, tx, base, col);
            if let Some(r) = regular {
                let lx = tx + f.width(&text, px) + 8.0 * s;
                let rest = (tx + room - lx).max(0.0);
                if !z.leise.is_empty() && rest > 20.0 * s {
                    let leise = sk_ui::widgets::ellipsize(Some(r), &z.leise, 10.0 * s, rest);
                    r.draw(c, &leise, 10.0 * s, lx, base, u.sheet_text_dim);
                }
            }
            let zahl =
                |c: &mut Canvas, text: &str, right: f32, f: &sk_paint::font::Font, col: Rgba| {
                    f.draw(c, text, px, right - f.width(text, px), base, col);
                };
            if !z.menge.is_empty() {
                zahl(c, &z.menge, menge_x, f, col);
            }
            if !z.ep.is_empty() {
                let r = col_ep(x0, cw);
                zahl(c, &z.ep, r, f, col);
                // Punkt vor dem EP: im Projekt geändert (ka-2-fach §2.4)
                if z.pos.is_some_and(|(p, _)| self.eigen.contains_key(&p)) {
                    let d = 1.5 * s;
                    let cx = r - f.width(&z.ep, px) - 8.0 * s;
                    let mut p = Path::new();
                    p.rounded_rect(cx - d, y + h * 0.5 - d, 2.0 * d, 2.0 * d, d);
                    c.fill(&p, u.accent);
                }
            }
            if !z.gp.is_empty() {
                let gcol = if z.betrag.is_none() {
                    u.sheet_hint
                } else {
                    col
                };
                zahl(c, &z.gp, x0 + cw, f, gcol);
            }
            let ueber =
                matches!(self.hot, Some(Hot::Zeile(j, _)) | Some(Hot::Waehlen(j)) if j == i);
            if ueber && self.zeigt_waehlen(i) {
                if let Some(b) = bold {
                    let (lx, ..) = self.waehlen_rect(t, fonts, y, h);
                    b.draw(c, WAEHLEN, px, lx, base, u.accent);
                }
            }
            if z.art == Art::Gruppe && z.ebene == 0 {
                c.fill_rect(x0, y + h - s.max(1.0), cw, s.max(1.0), u.sheet_rule);
            }
        }
    }

    fn paint_tiles(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let Some(b) = self.blatt.as_deref() else {
            return;
        };
        let s = self.scale;
        let u = &t.ui;
        let (x0, cw) = self.content_x(t);
        let y = self.tiles_top();
        let gap = TILE_GAP * s;
        let tw = ((cw - 2.0 * gap) / 3.0).max(0.0);
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let material = self.modus.nur_material();
        let (netto, mwst) = if material {
            (b.nur_material, b.mwst_material)
        } else {
            (b.netto, b.mwst)
        };
        let satz = b.mwst_satz.text().replace('.', ",");
        let lohn_text = format!(
            "Lohn {} € ({} €/h) · Material {} €",
            euro(b.lohn),
            euro(self.lohnsatz.cent()),
            euro(b.stoff)
        );
        let anteil = if material {
            "–".to_string()
        } else {
            prozent(b.lohn, b.netto).map_or("–".into(), |p| format!("{p} %"))
        };
        let kacheln = [
            ("Summe netto", format!("{} €", euro(netto)), String::new()),
            (
                "Summe brutto",
                format!("{} €", euro(netto + mwst)),
                format!("MwSt. {satz} % {} €", euro(mwst)),
            ),
            ("Lohnanteil", anteil, lohn_text),
        ];
        for (k, (titel, wert, klein)) in kacheln.iter().enumerate() {
            let x = x0 + k as f32 * (tw + gap);
            let mut p = Path::new();
            p.rounded_rect(x, y, tw, TILE_H * s, t.size.corner_radius * s);
            c.fill(&p, u.sheet_tile);
            if let Some(f) = regular {
                f.draw(
                    c,
                    titel,
                    10.0 * s,
                    x + 12.0 * s,
                    y + 18.0 * s,
                    u.sheet_text_dim,
                );
                if !klein.is_empty() {
                    let k = sk_ui::widgets::ellipsize(Some(f), klein, 10.0 * s, tw - 24.0 * s);
                    f.draw(
                        c,
                        &k,
                        10.0 * s,
                        x + 12.0 * s,
                        y + 54.0 * s,
                        u.sheet_text_dim,
                    );
                }
            }
            if let Some(f) = bold {
                f.draw(c, wert, 17.0 * s, x + 12.0 * s, y + 38.0 * s, u.sheet_text);
            }
        }
    }

    fn paint_fuss(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, fuss: &[Fuss]) {
        let s = self.scale;
        let u = &t.ui;
        let (x0, _) = self.content_x(t);
        let Some(f) = fonts.regular.as_ref() else {
            return;
        };
        let px = 10.5 * s;
        let mut y = self.tiles_top() + (TILE_H + TILE_GAP) * s;
        for z in fuss {
            let base = y + (FOOT_LINE * s + f.cap_height(px)) * 0.5;
            f.draw(c, &z.text, px, x0, base, u.sheet_text_dim);
            if !z.verweis.is_empty() {
                let col = if z.ziel.is_some() && self.hot == z.ziel {
                    u.accent_hover
                } else {
                    u.accent
                };
                f.draw(c, &z.verweis, px, x0 + f.width(&z.text, px), base, col);
            }
            y += FOOT_LINE * s;
        }
    }

    fn paint_scrollbar(&self, c: &mut Canvas, t: &Theme) {
        let s = self.scale;
        let (content, view) = (self.content_h(), self.view_h());
        if content <= view || view <= 0.0 {
            return;
        }
        let r = sk_ui::widgets::Rect::new(
            self.w as f32 - 9.0 * s,
            self.list_top(),
            5.0 * s,
            self.tiles_top() - self.list_top() - 6.0 * s,
        );
        sk_ui::widgets::scrollbar(c, r, self.scroll / content, view / content, false, s, t);
    }

    // --- CSV -----------------------------------------------------------------

    /// „Als Tabelle speichern“ im Reiter Kosten (ka-2-fach §2.5): UTF-8 mit
    /// BOM, „;“, Dezimalkomma; Kopfzeilen Projekt, Umfang, Preise, Datum,
    /// Modus; je Positionszeile die Spalten; Fußzeilen Netto, MwSt., Brutto,
    /// Lohn, Stoff, NU. Werte wie in der Anzeige.
    pub fn csv(&self, projekt: &str, datum: &str) -> Vec<u8> {
        let mut out = String::from("\u{feff}");
        let mut zeile = |cols: &[&str]| {
            let v: Vec<String> = cols.iter().map(|c| csv_feld(c)).collect();
            out.push_str(&v.join(";"));
            out.push_str("\r\n");
        };
        zeile(&["Projekt", projekt]);
        zeile(&[
            "Umfang",
            self.subtitle.split(" · Stand").next().unwrap_or_default(),
        ]);
        zeile(&["Preise", &self.preisquelle]);
        zeile(&["Datum", datum]);
        zeile(&["Modus", self.modus.label()]);
        zeile(&[]);
        zeile(&[
            "Gliederung",
            "Kennung",
            "OZ",
            "Kurztext",
            "Menge",
            "Einheit",
            "Lohn-EP",
            "Stoff-EP",
            "Gerät-EP",
            "Sonst-EP",
            "EP",
            "GP",
            "Stoff-GP",
            "Projektabweichung",
        ]);
        let Some(b) = self.blatt.as_deref() else {
            return out.into_bytes();
        };
        let zahl = |c: Cent| euro(c).replace('.', "");
        let menge = |d: Dez, e: Einheit| {
            menge_text(d, e)
                .trim_end_matches(e.zeichen())
                .trim()
                .replace('.', "")
        };
        for z in &self.zeilen {
            match (z.art, z.pos) {
                (Art::Position { .. }, Some((i, m))) => {
                    let p = &b.positionen[i];
                    let nu_material = self.modus.nur_material() && p.nu.is_some();
                    let gp = if nu_material {
                        String::new()
                    } else {
                        zahl(p.gp_von(m, false))
                    };
                    let kennung = match &p.quelle {
                        Quelle::Leistung(g) | Quelle::Geschaetzt(g) | Quelle::Richtpreis(g) => {
                            g.to_ifc()
                        }
                    };
                    zeile(&[
                        &z.gruppe,
                        &kennung,
                        &p.oz,
                        &p.kurz,
                        &menge(m, p.einheit),
                        p.einheit.zeichen(),
                        &zahl(p.lohn),
                        &zahl(p.stoff),
                        &zahl(p.geraet),
                        &zahl(p.sonst),
                        &zahl(p.ep),
                        &gp,
                        &zahl(p.gp_von(m, true)),
                        if self.eigen.contains_key(&i) {
                            "ja"
                        } else {
                            ""
                        },
                    ]);
                }
                (Art::Ohne, _) => {
                    let (zahl_menge, einheit) = z.menge.split_once(' ').unwrap_or((&z.menge, ""));
                    zeile(&[
                        &z.gruppe,
                        "",
                        "",
                        &z.text,
                        &zahl_menge.replace('.', ""),
                        einheit,
                        "",
                        "",
                        "",
                        "",
                        "",
                        "",
                        "",
                        "",
                    ]);
                }
                (Art::Gruppe, _) => {
                    let summe = z.betrag.map(zahl).unwrap_or_default();
                    zeile(&[
                        &z.text, "", "", "Summe", "", "", "", "", "", "", "", &summe, "", "",
                    ]);
                }
                (Art::Ausgleich, _) => {
                    let summe = z.betrag.map(zahl).unwrap_or_default();
                    zeile(&[
                        "", "", "", &z.text, "", "", "", "", "", "", "", &summe, "", "",
                    ]);
                }
                _ => {}
            }
        }
        zeile(&[]);
        let material = self.modus.nur_material();
        let (netto, mwst) = if material {
            (b.nur_material, b.mwst_material)
        } else {
            (b.netto, b.mwst)
        };
        zeile(&["Netto", &zahl(netto)]);
        zeile(&["MwSt.", &zahl(mwst)]);
        zeile(&["Brutto", &zahl(netto + mwst)]);
        zeile(&["Lohn", &zahl(b.lohn)]);
        zeile(&["Stoff", &zahl(b.stoff)]);
        zeile(&["NU", &zahl(b.nu)]);
        out.into_bytes()
    }
}

/// Rechte Kante der Spalte Menge und EP (px).
fn col_menge(x0: f32, cw: f32) -> f32 {
    x0 + cw * 0.66
}

fn col_ep(x0: f32, cw: f32) -> f32 {
    x0 + cw * 0.82
}

fn csv_feld(s: &str) -> String {
    if s.contains([';', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn haus() -> Scene {
        let m = sk_model::szo::read_with(
            include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
            sk_model::GuidGen::with_seed(1),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .expect("lädt")
        .model;
        Scene::with_model(m)
    }

    /// Betrag aus einem angezeigten Text „1.234,56“.
    fn cent(text: &str) -> i64 {
        let t = text.replace(['.', '−'], "").replace(',', "");
        let v: i64 = t.parse().unwrap_or_else(|_| panic!("{text}"));
        if text.starts_with('−') {
            -v
        } else {
            v
        }
    }

    /// Menge in Tausendsteln aus „172,224 m²“.
    fn milli(text: &str) -> i64 {
        let n = text.split(' ').next().unwrap();
        n.replace(['.', ','], "").parse().unwrap()
    }

    /// Abnahme 6 (Anzeige rechnet nach): In jeder Zeile GP = angezeigte
    /// Menge × angezeigter EP auf den Cent; jede Summe = Summe der
    /// angezeigten Zeilen; das Netto gleich in jeder Gliederung, mit
    /// Rundungsausgleich; beide Modi.
    #[test]
    fn anzeige_rechnet_nach() {
        let mut s = haus();
        let mut v = KostenView::new();
        v.sync(&mut s, None);
        let netto_voll = v.netto().unwrap();
        assert_eq!(netto_voll, Cent(6_008_983));
        for modus in Modus::ALLE {
            for g in Gliederung::ALLE {
                v.modus = modus;
                v.gliederung = g;
                v.sync(&mut s, None);
                let z = v.zeilen();
                let netto = v.netto().unwrap();
                // GP = Menge × EP
                for x in z.iter().filter(|x| matches!(x.art, Art::Position { .. })) {
                    if x.gp.is_empty() {
                        continue;
                    }
                    let soll =
                        (milli(&x.menge) as i128 * cent(&x.ep) as i128 + 500).div_euclid(1000);
                    assert_eq!(cent(&x.gp) as i128, soll, "{modus:?} {g:?} {x:?}");
                }
                // Gruppen = Summe der Zeilen darunter
                for (i, x) in z.iter().enumerate() {
                    if x.art != Art::Gruppe || x.betrag.is_none() {
                        continue;
                    }
                    let kinder: i64 = z[i + 1..]
                        .iter()
                        .take_while(|k| k.ebene > x.ebene || k.art == Art::Ansatz)
                        .filter(|k| k.ebene == x.ebene + 1)
                        .filter_map(|k| k.betrag)
                        .map(|c| c.0)
                        .sum();
                    assert_eq!(cent(&x.gp), kinder, "{modus:?} {g:?} {}", x.text);
                }
                // Netto = Gruppen der obersten Ebene + Ausgleich
                let oben: i64 = z
                    .iter()
                    .filter(|x| x.ebene == 0)
                    .filter_map(|x| x.betrag)
                    .map(|c| c.0)
                    .sum();
                assert_eq!(oben, netto.0, "{modus:?} {g:?}");
            }
        }
        // Gliederung Gewerk ohne Ausgleich; Geschoss mit −0,01 / +0,03 je nach Blatt
        v.modus = Modus::Voll;
        v.gliederung = Gliederung::Geschoss;
        v.sync(&mut s, None);
        let ausgleich = v
            .zeilen()
            .iter()
            .find(|x| x.art == Art::Ausgleich)
            .map(|x| x.betrag);
        assert_eq!(
            ausgleich.flatten(),
            Some(v.blatt().unwrap().ausgleich_geschoss).filter(|c| c.0 != 0)
        );
    }

    /// Abnahme 1 und 2 (Gliederungen): KG mit 322, 331, 335, 351 und den
    /// Zwischensummen der 2. Ebene; Geschoss von unten; Chip-Summe gleich
    /// dem Blatt mit nur diesem Geschoss.
    #[test]
    fn gliederungen_und_chips() {
        let mut s = haus();
        let mut v = KostenView::new();
        v.gliederung = Gliederung::Kostengruppe;
        v.sync(&mut s, None);
        let gruppen: Vec<&str> = v
            .zeilen()
            .iter()
            .filter(|z| z.art == Art::Gruppe)
            .map(|z| z.text.as_str())
            .collect();
        for k in [
            "320 Gründung",
            "322 Flachgründungen und Bodenplatten",
            "331 Tragende Außenwände",
            "335 Außenwandbekleidungen, außen",
            "350 Decken",
            "351 Deckenkonstruktionen",
        ] {
            assert!(gruppen.contains(&k), "{k}: {gruppen:?}");
        }
        v.gliederung = Gliederung::Geschoss;
        v.sync(&mut s, None);
        let oben: Vec<&str> = v
            .zeilen()
            .iter()
            .filter(|z| z.art == Art::Gruppe && z.ebene == 0)
            .map(|z| z.text.as_str())
            .collect();
        assert_eq!(oben, ["Gründung", "Erdgeschoss", "Obergeschoss"]);
        // Chip EG: Summe gleich der Geschosszeile und dem Blatt nur mit EG
        let summen = v.leiste.summen.clone();
        assert_eq!(summen.len(), 3);
        let eg = v
            .zeilen()
            .iter()
            .find(|z| z.text == "Erdgeschoss")
            .unwrap()
            .betrag
            .unwrap();
        assert_eq!(summen[1], euro_ganz(eg));
        let c = v.leiste.chips().to_vec();
        assert!(umfang_view::klick(&mut v.leiste.umfang, &c, 1, true));
        v.sync(&mut s, None);
        assert_eq!(v.netto(), Some(eg));
        assert!(v.subtitle.contains(" · EG · Stand "), "{}", v.subtitle);
        assert!(
            v.subtitle.ends_with(" · Referenzpreise 10/2026"),
            "{}",
            v.subtitle
        );
        // abgewählte Chips behalten ihre Summe
        assert_eq!(v.leiste.summen, summen);
    }

    /// Zeilen ohne Bauleistung stehen grau unter ihrem Gewerk, die
    /// Gewerkzeile sagt „ohne Bauleistung“; die Fußzeile nennt sie.
    #[test]
    fn graue_zeilen_unter_dem_gewerk() {
        let mut s = haus();
        let mut v = KostenView::new();
        v.sync(&mut s, None);
        let z = v.zeilen();
        let i = z
            .iter()
            .position(|x| x.art == Art::Ohne)
            .expect("graue Zeile");
        let g = z[..i].iter().rev().find(|x| x.art == Art::Gruppe).unwrap();
        assert!(g.text.contains("Dach"), "{}", g.text);
        assert_eq!(g.gp, "ohne Bauleistung");
        let f = v.fuss();
        assert!(
            f.iter().any(|x| x.text == "Ohne Preis: "
                && x.verweis == "Dachterrasse, Attikablech (3 Zeilen ohne Bauleistung)"),
            "{:?}",
            f.iter().map(|x| &x.verweis).collect::<Vec<_>>()
        );
    }

    #[test]
    fn zahlen_und_csv() {
        assert_eq!(menge_text(Dez(172_224_000), Einheit::M2), "172,224 m²");
        assert_eq!(menge_text(Dez(1_172_224_000), Einheit::M3), "1.172,224 m³");
        assert_eq!(euro_ganz(Cent(6_008_983)), "60.090 €");
        assert_eq!(prozent(Cent(2_894_149), Cent(6_008_983)), Some(48));
        let mut s = haus();
        let mut v = KostenView::new();
        v.sync(&mut s, None);
        let csv = String::from_utf8(v.csv("Standardhaus", "08.10.2026")).unwrap();
        assert!(csv.starts_with("\u{feff}Projekt;Standardhaus\r\n"), "{csv}");
        assert!(csv.contains("\r\nNetto;60089,83\r\n"), "{csv}");
        assert!(csv.contains("\r\nModus;Material + Lohn\r\n"));
        // OZ mit Los, Frostschürze unter 1.01.0020; Zeilen ohne Bauleistung
        // ohne OZ, Menge und Einheit getrennt
        assert!(csv.contains(";1.01.0020;Frostschürze"), "{csv}");
        let kopf = csv.lines().find(|l| l.starts_with("Gliederung;")).unwrap();
        let spalten = kopf.split(';').count();
        assert_eq!(spalten, 14);
        for l in csv
            .lines()
            .filter(|l| l.contains(';'))
            .skip_while(|l| !l.starts_with("Gliederung;"))
        {
            if l.starts_with("Netto;") {
                break;
            }
            assert_eq!(l.split(';').count(), spalten, "{l}");
        }
    }

    /// Abnahme 11 (soll-ka-2c): Doppelklick auf den EP öffnet das
    /// Preisblatt; „20,5“ getippt zeigt den EP 52,34 live in der Zeile,
    /// ohne das Modell zu ändern; Enter liefert genau ein `PreisSetzen`.
    /// Danach trägt die Zeile den Punkt, der Tooltip nennt den Firmenwert und
    /// die CSV „ja“ in der Spalte Projektabweichung.
    #[test]
    fn preisblatt_live_und_punkt() {
        let t = Theme::dark();
        let fonts = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        let mut s = haus();
        let mut v = KostenView::new();
        (v.w, v.h) = (1200, 900);
        v.sync(&mut s, None);
        let rev = s.model().revision();
        let i = v
            .zeilen()
            .iter()
            .position(|z| z.text.contains("Porenbeton") && z.text.contains("17,5"))
            .expect("Mauerwerk");
        let (_, y, h) = v
            .sichtbar()
            .into_iter()
            .find(|(j, _, _)| *j == i)
            .expect("sichtbar");
        let (x0, cw) = v.content_x(&t);
        let (x, y) = ((col_ep(x0, cw) - 20.0) as f64, (y + h * 0.5) as f64);
        let mut p = Picking::default();
        let mods = sk_platform::Modifiers::default();
        v.mouse_down(&t, &fonts, &mut p, (x, y), mods);
        assert!(!v.preis_offen());
        assert_eq!(
            v.mouse_down(&t, &fonts, &mut p, (x, y), mods),
            Some(ListOut::Repaint)
        );
        v.sync(&mut s, None);
        assert!(v.preis_offen());
        assert_eq!(v.zeilen()[i].ep, "54,00");
        for ch in "20,5".chars() {
            assert_eq!(v.text(ch), Some(ListOut::Repaint));
        }
        v.sync(&mut s, None);
        assert_eq!(v.zeilen()[i].ep, "52,34");
        assert_eq!(v.zeilen()[i].gp, "9.014,20");
        assert_eq!(s.model().revision(), rev, "Vorschau schreibt nichts");
        let out = v.key(&t, sk_platform::Key::Enter, mods);
        let Some(Some(ListOut::Kosten(Schreiben::Preis { ops, gilt, .. }))) = out else {
            panic!("{out:?}");
        };
        assert_eq!(gilt, Gilt::NurHaus);
        assert_eq!(ops.len(), 1);
        assert!(!v.preis_offen());
        let herkunft = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:00");
        s.kosten_folge("Preis im Projekt geändert", None, &herkunft, &ops)
            .unwrap();
        v.sync(&mut s, None);
        assert_eq!(v.zeilen()[i].ep, "52,34");
        let pos = v.zeilen()[i].pos.unwrap().0;
        assert_eq!(v.eigen.get(&pos), Some(&Cent(5_400)));
        assert_eq!(
            v.tip_at(&t, &fonts, x, y).as_deref(),
            Some("im Projekt geändert, Firma 54,00")
        );
        let csv = String::from_utf8(v.csv("Standardhaus", "08.10.2026")).unwrap();
        let zeile = csv
            .lines()
            .find(|l| l.contains("Porenbeton") && l.contains("17,5"))
            .unwrap();
        assert!(zeile.ends_with(";ja"), "{zeile}");
        // Esc verwirft: wieder öffnen, tippen, Esc, nichts geändert
        v.mouse_down(&t, &fonts, &mut p, (x, y), mods);
        v.mouse_down(&t, &fonts, &mut p, (x, y), mods);
        v.sync(&mut s, None);
        v.text('1');
        v.sync(&mut s, None);
        assert_eq!(
            v.key(&t, sk_platform::Key::Escape, mods),
            Some(Some(ListOut::Repaint))
        );
        v.sync(&mut s, None);
        assert_eq!(v.zeilen()[i].ep, "52,34");
    }

    /// KA-2c2 (Abnahme 12): „Auch für neue Häuser“ schreibt die Firma als
    /// neuen Stand. Ohne Projektkopie gibt es keinen Projektschritt; mit
    /// Kopie einen, mit Wert im Namen, und Strg+Z nimmt nur ihn zurück.
    #[test]
    fn auch_fuer_neue_haeuser() {
        let d = std::env::temp_dir().join(format!("skizzeo-kv-firma-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (mut c, _) = crate::catalog::Company::load(&d.join("firmenkatalog.szk"), true);
        let mut s = haus();
        let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:00");
        let ep = |s: &mut Scene, c: &crate::catalog::Company, firma_allein: bool| {
            let f = Some((c.library(), c.stand()));
            let k = if firma_allein {
                s.firmenkatalog(f)
            } else {
                s.katalog(f)
            };
            let b = s.kostenblatt(f, &Umfang::projekt());
            let p = b
                .positionen
                .iter()
                .find(|p| p.kurz.contains("Porenbeton") && p.kurz.contains("17,5"))
                .unwrap();
            sk_cost::preis::aufbau(s.model(), &k, p).unwrap()
        };
        let a = ep(&mut s, &c, false);
        let stein = a.stoffe.iter().find(|t| t.haupt).unwrap().artikel.unwrap();
        let k = s.katalog(Some((c.library(), c.stand())));
        let setze = |x: &str| {
            sk_cost::preis::preis_ops(
                &k,
                &a,
                &sk_cost::preis::Eingabe {
                    stunden: None,
                    preise: vec![(stein, Dez::lesen(x, 4).unwrap())],
                },
                "10/2026",
            )
        };
        // Ohne Projektkopie: nur die Firma, kein Rückgängig-Schritt
        let undo = s.undo_label();
        let label = s.bezeichnung("Planstein 20,50 €/m² für dieses und neue Häuser".into());
        assert_eq!(s.fuer_firma(label, &mut c, &h, &setze("20.5")), Ok(None));
        assert_eq!(s.undo_label(), undo);
        assert_eq!(ep(&mut s, &c, false).ep, Cent(5_234));
        assert_eq!(ep(&mut s, &c, true).ep, Cent(5_234));
        // Nur dieses Haus: 21,00 im Projekt, die Firma bleibt bei 20,50
        let f = Some(c.library());
        s.kosten_folge("Preis im Projekt geändert", f, &h, &setze("21"))
            .unwrap();
        assert_eq!(ep(&mut s, &c, false).ep, Cent(5_284));
        assert_eq!(ep(&mut s, &c, true).ep, Cent(5_234));
        // Auch für neue Häuser mit Kopie: Firma und Projekt 19,00, ein Schritt
        let label = s.bezeichnung("Planstein 19,00 €/m² für dieses und neue Häuser".into());
        assert_eq!(s.fuer_firma(label, &mut c, &h, &setze("19")), Ok(None));
        assert_eq!(s.undo_label(), Some(label));
        assert_eq!(ep(&mut s, &c, false).ep, Cent(5_084));
        assert_eq!(ep(&mut s, &c, true).ep, Cent(5_084));
        // Strg+Z: nur das Projekt zurück, die Firma behält 19,00
        assert!(s.undo());
        assert_eq!(ep(&mut s, &c, false).ep, Cent(5_284));
        assert_eq!(ep(&mut s, &c, true).ep, Cent(5_084));
        // Gleicher Wortlaut, gleiche Bezeichnung
        assert!(std::ptr::eq(
            s.bezeichnung("Planstein 19,00 €/m² für dieses und neue Häuser".into()),
            label
        ));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Abgleichzeile: Firma auf Lohn 65 nach der Projektkopie; „übernehmen“
    /// mit Netto im Tooltip, die Liste schiebt sich unter die Zeile.
    #[test]
    fn abgleichzeile_fuer_neue_haeuser() {
        let t = Theme::dark();
        let fonts = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        let d = std::env::temp_dir().join(format!("skizzeo-kv-abgleich-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (mut c, _) = crate::catalog::Company::load(&d.join("firmenkatalog.szk"), true);
        let mut s = haus();
        let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:00");
        let lohn = |v: i64| sk_cost::Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(v),
        };
        c.fuer_firma(&h, &[lohn(60)]).unwrap();
        let mut v = KostenView::new();
        (v.w, v.h) = (1200, 900);
        v.sync(&mut s, Some((c.library(), c.stand())));
        assert_eq!(v.abgleich_zeile(), None, "ohne Kopie");
        let oben = v.list_top();
        // Projektkopie über einen eigenen Preis, dann Lohn 65 in der Firma
        let k = s.katalog(Some((c.library(), c.stand())));
        let art = k.artikel.iter().find(|a| a.preis.is_some()).unwrap().guid;
        let preis = Op::PreisSetzen {
            artikel: art,
            preis: Some(Dez::ganz(99)),
            stand: "10/2026".into(),
            quelle: "Preisblatt".into(),
        };
        s.kosten_folge("Preis", Some(c.library()), &h, &[preis])
            .unwrap();
        c.fuer_firma(&h, &[lohn(65)]).unwrap();
        v.sync(&mut s, Some((c.library(), c.stand())));
        assert_eq!(
            v.abgleich_zeile().as_deref(),
            Some("Für neue Häuser gilt Lohn 65,00 €/h (hier 60,00)")
        );
        assert_eq!(v.list_top(), oben + ABGLEICH_H * v.scale);
        let (_, _, _, _, (ux, uy, uw, uh), (lx, ..)) = v.abgleich_lage(&t, &fonts).unwrap();
        let (x, y) = ((ux + uw * 0.5) as f64, (uy + uh * 0.5) as f64);
        let tip = v.tip_at(&t, &fonts, x, y).unwrap();
        assert!(tip.starts_with("Mit den Werten für neue Häuser: "), "{tip}");
        assert!(tip.ends_with("netto · Eigene Werte dieses Hauses bleiben stehen."));
        let mut p = Picking::default();
        let mods = sk_platform::Modifiers::default();
        let lassen = v.mouse_down(&t, &fonts, &mut p, ((lx + 2.0) as f64, y), mods);
        assert!(matches!(
            lassen,
            Some(ListOut::Kosten(Schreiben::Lassen(_)))
        ));
        let Some(ListOut::Kosten(Schreiben::Uebernehmen(saetze))) =
            v.mouse_down(&t, &fonts, &mut p, (x, y), mods)
        else {
            panic!("übernehmen");
        };
        let op = Op::StandUebernehmen { saetze };
        s.kosten_folge("Übernommen", Some(c.library()), &h, &[op])
            .unwrap();
        v.sync(&mut s, Some((c.library(), c.stand())));
        assert_eq!(v.abgleich_zeile(), None);
        assert_eq!(v.list_top(), oben);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Graue Zeile: überfahren zeigt „Bauleistung wählen …“, Klick öffnet das
    /// Blatt, Klick auf einen Eintrag schreibt `BauleistungZuordnen`; danach
    /// ist die Zeile eine Position.
    #[test]
    fn bauleistung_waehlen() {
        let t = Theme::dark();
        let fonts = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        let mut s = haus();
        let mut v = KostenView::new();
        (v.w, v.h) = (1200, 1400);
        v.sync(&mut s, None);
        let grau = v.blatt().unwrap().ohne.len();
        let i = v
            .zeilen()
            .iter()
            .position(|z| z.ohne.is_some_and(|j| v.waehlbar.contains(&j)))
            .expect("graue Zeile mit Wahl");
        let (_, y, h) = v
            .sichtbar()
            .into_iter()
            .find(|(j, _, _)| *j == i)
            .expect("sichtbar");
        let (lx, ly, lw, lh) = v.waehlen_rect(&t, &fonts, y, h);
        let (x, y) = ((lx + lw * 0.5) as f64, (ly + lh * 0.5) as f64);
        let mut p = Picking::default();
        let mods = sk_platform::Modifiers::default();
        v.mouse_move(&t, &fonts, &mut p, x, y);
        assert_eq!(v.hot, Some(Hot::Waehlen(i)));
        assert_eq!(
            v.mouse_down(&t, &fonts, &mut p, (x, y), mods),
            Some(ListOut::Repaint)
        );
        v.sync(&mut s, None);
        assert!(v.blatt_offen());
        // Enter mit Suchtext, der genau einen Treffer lässt
        let w = v.wahl.as_ref().unwrap();
        let erste = w_erste(w);
        for ch in erste.chars() {
            v.text(ch);
        }
        let out = v.key(&t, sk_platform::Key::Enter, mods);
        let Some(Some(ListOut::Kosten(Schreiben::Bauleistung(op)))) = out else {
            panic!("{out:?}");
        };
        assert!(!v.blatt_offen());
        let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:00");
        s.kosten_folge("Bauleistung gewählt", None, &h, &[*op])
            .unwrap();
        v.sync(&mut s, None);
        assert!(v.blatt().unwrap().ohne.len() < grau);
        assert!(s.undo());
        v.sync(&mut s, None);
        assert_eq!(v.blatt().unwrap().ohne.len(), grau);
    }

    /// Kurzname des ersten wählbaren Eintrags, der eindeutig ist.
    fn w_erste(w: &WahlBlatt) -> String {
        w.erster_eindeutig().expect("ein eindeutiger Name")
    }
}

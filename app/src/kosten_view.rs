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

use crate::lohn_blatt::{self, LohnBlatt};
use crate::picking::Picking;
use crate::preis_blatt::{self, Gilt, PreisBlatt};
use crate::scene::Scene;
use crate::schedule_view::ListOut;
use crate::umfang_view::{self, Leiste};
use crate::wahl_blatt::{self, WahlBlatt};
use sk_cost::abgleich::Abgleich;
use sk_cost::katalog::{Einheit, Katalog};
use sk_cost::rechnung::Quelle;
use sk_cost::{Cent, Dez, Kostenblatt, Op, SatzId};
use sk_model::qto::Umfang;
use sk_model::ElementId;
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

#[cfg(test)]
mod bild;
#[cfg(test)]
mod tests;
mod zeilen;

pub use zeilen::{chip_summen, euro_ganz, menge_text, zeilen};
pub(crate) use zeilen::{euro, geschoss_name, gewerk_name, kg_name, prozent, tausender};

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
/// Hinweis nach „Bauleistung wählen“ mit einer Bauleistung eines anderen
/// Gewerks (Bedienbarkeit 8.6, 9.3): stimmt in jeder Gliederung.
pub fn gewerk_hinweis(zeile: &str, gewerk: &str) -> crate::meldung::Meldung {
    crate::meldung::Meldung::mit("{} gehört jetzt zum Gewerk {}.", &[zeile, gewerk])
}

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
    /// Stundenlohn in der Kachel Lohnanteil (öffnet das Lohnfeld).
    Lohnsatz,
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
    Bauleistung {
        op: Box<Op>,
        /// „DT-001 PIR-Dämmung steht jetzt unter WDV-Systeme (DIN 18345).“,
        /// wenn die Zeile zu einem anderen Gewerk kommt.
        hinweis: Option<crate::meldung::Meldung>,
    },
    /// Verrechnungslohn für dieses Haus oder auch für neue Häuser.
    Lohn {
        wert: Dez,
        gilt: Gilt,
    },
    /// AVA-Kopf: Bauvorhaben, Bauherr oder Aufsteller (`Model::set_project`,
    /// ein Schritt „Bauherr gesetzt“ usw.).
    Projekt {
        projekt: sk_model::Project,
        label: &'static str,
    },
    /// AVA „Mehr“ › „Geschosse als Untertitel“ (`LvGliederungSetzen`).
    Gliederung(bool),
}

/// Kosten beim Tippen im Preisblatt: Operationen und die Blätter darauf.
struct Live {
    ops: Vec<Op>,
    blatt: Rc<Kostenblatt>,
    ganz: Rc<Kostenblatt>,
}

/// Verweis an grauen Zeilen (Einstellungen §3 KA-2 Punkt 4).
const WAEHLEN: &str = "Bauleistung wählen …";

/// So lange ist die eben zugeordnete Position markiert.
const BLITZ: std::time::Duration = std::time::Duration::from_millis(1500);

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
    /// Nach „Bauleistung wählen“: Bauteil und Bauleistung der neuen
    /// Position; die Liste springt hin und markiert sie kurz (Bedienbarkeit
    /// 8.6). Die Zeit setzt der erste Aufbau, der sie findet.
    blitz: Option<(ElementId, sk_model::Guid, Option<Instant>)>,
    /// Im Projekt geänderte Positionen mit dem EP der Firma (Punkt am EP,
    /// CSV-Spalte Projektabweichung) und woraus sie bestimmt sind.
    eigen: HashMap<usize, Cent>,
    /// Marke „eigener Lohn“ (paket-ka2 §4): der Lohn der Firma, wenn das
    /// Projekt einen eigenen hat, der davon abweicht.
    lohn_firma: Option<Dez>,
    eigen_von: Option<(*const Kostenblatt, *const Katalog)>,
    /// Abgleich mit dem Firmenkatalog (Regel 92) mit netto vorher und
    /// nachher für den Tooltip an „übernehmen“, und woraus er bestimmt ist.
    abgleich: Option<(Abgleich, Option<(Cent, Cent)>)>,
    /// Stempel von Katalog und Firmenkatalog des Abgleichs: Ein Bauschritt
    /// ändert keine Preise und rechnet ihn nicht neu (Review 3ak).
    abgleich_von: Option<(u64, u64)>,
    /// Kostenblatt, zu dem die Netto-Vorschau von „übernehmen“ gehört; sie
    /// wird erst gerechnet, wenn die Maus auf „übernehmen“ steht.
    abgleich_netto_von: Option<*const Kostenblatt>,
    /// Blatt „Bauleistung wählen …“, sein Öffnen beim nächsten `sync`
    /// (Zeile ohne Bauleistung und Bauteil) und die grauen Zeilen, für die
    /// es etwas zu wählen gibt.
    wahl: Option<WahlBlatt>,
    wahl_wunsch: Option<(usize, ElementId)>,
    /// Aus dem Prüfen des AVA: zur grauen Zeile dieses Bauteils und dort
    /// „Bauleistung wählen …“ öffnen, sobald das Blatt steht.
    wahl_nach: Option<ElementId>,
    waehlbar: HashSet<usize>,
    waehlbar_von: Option<(*const Kostenblatt, *const Katalog)>,
    /// Lohnfeld (Hinweiskarte oder Blatt an der Kachel) und sein Öffnen
    /// beim nächsten `sync`.
    lohn: Option<LohnBlatt>,
    lohn_wunsch: Option<lohn_blatt::Form>,
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
            blitz: None,
            eigen: HashMap::new(),
            lohn_firma: None,
            eigen_von: None,
            abgleich: None,
            abgleich_von: None,
            abgleich_netto_von: None,
            wahl: None,
            wahl_wunsch: None,
            wahl_nach: None,
            waehlbar: HashSet::new(),
            waehlbar_von: None,
            lohn: None,
            lohn_wunsch: None,
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
        if let Some(form) = self.lohn_wunsch.take() {
            let firma_lohn = s.firmenkatalog(firma).werte.lohn;
            let titel = match form {
                lohn_blatt::Form::Karte => {
                    lohn_blatt::kartentitel(&sk_cost::lesen::preisquelle(&kat))
                }
                lohn_blatt::Form::Blatt => "Verrechnungslohn".into(),
            };
            let mut l = LohnBlatt::neu(form, titel, kat.werte.lohn, firma_lohn);
            l.scale = self.scale;
            self.preis_schliessen();
            self.wahl = None;
            self.lohn = Some(l);
            changed = true;
        }
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
            self.blitz_suchen(&blatt);
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
        let lohn = (kat.herkunft_von("rate", "wage").is_some_and(|u| u.proj)
            && fk.werte.lohn != kat.werte.lohn)
            .then_some(fk.werte.lohn);
        let changed = neu != self.eigen || lohn != self.lohn_firma;
        self.eigen = neu;
        self.lohn_firma = lohn;
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
        let von = (kat.stempel, firma.map_or(0, |f| f.1));
        let mut changed = false;
        if self.abgleich_von != Some(von) {
            self.abgleich_von = Some(von);
            self.abgleich_netto_von = None;
            let neu = sk_cost::abgleich::abgleich(s.model(), firma.map(|f| f.0));
            let alt = self.abgleich.take();
            changed = neu.as_ref() != alt.as_ref().map(|a| &a.0);
            self.abgleich = neu.map(|a| match alt {
                Some((b, netto)) if b == a => (a, netto),
                _ => (a, None),
            });
            if changed {
                self.clamp();
            }
        }
        // Netto-Vorschau nur für den Tooltip an „übernehmen“
        let ptr = Rc::as_ptr(blatt);
        if self.hot == Some(Hot::Uebernehmen) && self.abgleich_netto_von != Some(ptr) {
            if let Some((a, netto)) = self.abgleich.as_mut() {
                self.abgleich_netto_von = Some(ptr);
                let op = Op::StandUebernehmen {
                    saetze: a.saetze.clone(),
                };
                *netto = s
                    .kosten_live(firma, &[op], &[&self.leiste.umfang])
                    .ok()
                    .and_then(|(_, b)| Some((blatt.netto, b.first()?.netto)));
                changed = true;
            }
        }
        changed
    }

    /// Tooltip an „übernehmen“; ohne Vorschau ohne Beträge.
    pub fn tip_uebernehmen(&self) -> Option<String> {
        let (_, netto) = self.abgleich.as_ref()?;
        Some(match netto {
            Some((v, n)) => format!(
                "Mit den Werten für neue Häuser: {} € → {} € netto · Eigene Werte dieses Hauses bleiben stehen.",
                euro(*v),
                euro(*n)
            ),
            None => "Rechnet mit den Werten für neue Häuser · Eigene Werte dieses Hauses bleiben stehen.".into(),
        })
    }

    /// Steht die Maus auf „übernehmen“?
    pub fn auf_uebernehmen(&self) -> bool {
        self.hot == Some(Hot::Uebernehmen)
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
        if let Some(el) = self.wahl_nach.take() {
            if let Some(j) = blatt.ohne.iter().position(|z| z.element == el) {
                self.springe(|z| z.ohne == Some(j));
                self.wahl_wunsch = Some((j, el));
                changed = true;
            }
        }
        if let Some((j, el)) = self.wahl_wunsch.take() {
            let z = blatt.ohne.get(j).filter(|z| z.element == el);
            let a = z.and_then(|z| sk_cost::wahl::auswahl(s.model(), kat, z));
            // Im Blatt mit Bauteilnummer: „DT-001 PIR-Dämmung“
            let titel = z.map_or_else(String::new, |z| {
                format!(
                    "{} {}",
                    z.nummer,
                    zeilen::baustoff_name(s.model(), z.baustoff)
                )
                .trim()
                .to_string()
            });
            if let (Some(z), Some(a)) = (z, a) {
                let mut w = WahlBlatt::neu((j, el), &titel, menge_text(z.menge, z.einheit), a);
                w.scale = self.scale;
                self.wahl = Some(w);
                changed = true;
            }
        }
        changed
    }

    /// Zur grauen Zeile des Bauteils `el` und dort „Bauleistung wählen …“
    /// öffnen (Prüfen im AVA, Bedienbarkeit 12.2); wirkt beim nächsten
    /// `sync`.
    pub fn waehlen_fuer(&mut self, el: ElementId) {
        self.wahl_nach = Some(el);
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
                pb.set_live(Err(crate::meldung::Meldung::vorschau(
                    &b,
                    "Dieser Wert geht nicht.",
                )));
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
        self.preis.is_some() || self.wahl.is_some() || self.lohn.is_some()
    }

    /// Hinweiskarte mit dem Lohnfeld beim ersten Öffnen der Kosten.
    pub fn lohn_karte(&mut self) {
        self.lohn_wunsch = Some(lohn_blatt::Form::Karte);
    }

    /// Ergebnis des Lohnfelds.
    fn lohn_aus(&mut self, aus: lohn_blatt::Aus) -> Option<ListOut> {
        use lohn_blatt::Aus;
        Some(match aus {
            Aus::Repaint => ListOut::Repaint,
            Aus::Schliessen => {
                self.lohn = None;
                ListOut::Repaint
            }
            Aus::Schreiben { wert, gilt } => {
                self.lohn = None;
                ListOut::Kosten(Schreiben::Lohn { wert, gilt })
            }
        })
    }

    /// Stundenlohn in der Kachel Lohnanteil: Text davor, Verweis, danach und
    /// die Fläche des Verweises (px).
    fn lohnsatz_lage(&self, t: &Theme, fonts: &Fonts) -> Option<(String, String, String, Rect)> {
        let b = self.blatt.as_deref()?;
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let tw = ((cw - 2.0 * TILE_GAP * s) / 3.0).max(0.0);
        // Klein neben dem Betrag, rechtsbündig auf seiner Grundlinie
        // (Einstellungen §3 KA-2 Punkt 3)
        let rechts = x0 + 2.0 * (tw + TILE_GAP * s) + tw - 12.0 * s;
        let vor = format!("Lohn {} € (", euro(b.lohn));
        let link = format!("{} €/h", euro(self.lohnsatz.cent()));
        let nach = format!(") · Material {} €", euro(b.stoff));
        let px = 10.0 * s;
        let w = |f: Option<&sk_paint::font::Font>, t: &str| {
            f.map_or(t.chars().count() as f32 * px * 0.55, |f| f.width(t, px))
        };
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let punkt = if self.lohn_firma.is_some() {
            8.0 * s
        } else {
            0.0
        };
        let lw = w(bold, &link);
        let breit = w(regular, &vor) + lw + punkt + w(regular, &nach);
        // Passt die Zeile nicht neben den Betrag, steht sie darunter wie
        // die zweite Menge der Baustoffkacheln
        let anteil = if self.modus.nur_material() {
            "–".to_string()
        } else {
            prozent(b.lohn, b.netto).map_or("–".into(), |p| format!("{p} %"))
        };
        let links = rechts + 12.0 * s - tw + 12.0 * s;
        let wert = links + bold.map_or(0.0, |f| f.width(&anteil, 17.0 * s));
        let (x, y) = if rechts - breit < wert + 12.0 * s {
            (links, self.tiles_top() + 44.0 * s)
        } else {
            (rechts - breit, self.tiles_top() + 28.0 * s)
        };
        let lx = x + w(regular, &vor);
        Some((vor, link.clone(), nach, (lx, y, lw, 14.0 * s)))
    }

    /// Beide Blätter schließen, ohne zu schreiben.
    pub fn blaetter_schliessen(&mut self) -> bool {
        self.wahl.take().is_some() | self.lohn.take().is_some() | self.preis_schliessen()
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
                let op = Box::new(sk_cost::wahl::zuordnen(z, g)?);
                let hinweis = w
                    .gewaehlt(g)
                    .and_then(|x| x.fremd.as_ref())
                    .map(|gewerk| gewerk_hinweis(w.zeile(), gewerk));
                self.blitz = Some((w.element, g, None));
                ListOut::Kosten(Schreiben::Bauleistung { op, hinweis })
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

    /// Lohnfeld an den Stundenlohn der Kachel bzw. unten rechts legen.
    fn lege_lohn(&mut self, t: &Theme) {
        let Some(l) = self.lohn.as_mut() else {
            return;
        };
        l.fenster = (self.w as f32, self.h as f32);
        l.scale = self.scale;
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let tw = ((cw - 2.0 * TILE_GAP * s) / 3.0).max(0.0);
        let x = x0 + 2.0 * (tw + TILE_GAP * s);
        let y = self.tiles_top();
        let l = self.lohn.as_mut().expect("eben gesehen");
        l.set_anker((x + 12.0 * s, y + 40.0 * s, x + tw * 0.6, y + 58.0 * s));
        // Die Hinweiskarte bleibt über den Kacheln (B-Befund 11.3)
        l.ueber = Some(y - 12.0 * s);
    }

    /// Preisblatt an die EP-Zelle und die Fenstergröße legen.
    fn lege_preis(&mut self, t: &Theme) {
        self.lege_wahl(t);
        self.lege_lohn(t);
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
        if let Some(l) = self.lohn.as_mut() {
            let aus = l.key(key, mods);
            return Some(aus.and_then(|a| self.lohn_aus(a)));
        }
        if let Some(w) = self.wahl.as_mut() {
            let aus = w.key(key, mods);
            return Some(aus.and_then(|a| self.wahl_aus(a)));
        }
        let aus = self.preis.as_mut()?.key(key, mods);
        Some(aus.and_then(|a| self.preis_aus(a)))
    }

    /// Getipptes Zeichen fürs Preisblatt.
    pub fn text(&mut self, ch: char) -> Option<ListOut> {
        if let Some(l) = self.lohn.as_mut() {
            let aus = l.text(ch)?;
            return self.lohn_aus(aus);
        }
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
                let n = z.text.split(" · ").next().unwrap_or_default().to_string();
                if !n.is_empty() && !namen.contains(&n) {
                    namen.push(n);
                }
            }
            out.push(Fuss {
                text: "Ohne Preis: ".into(),
                verweis: format!(
                    "{} ({} {})",
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
        let bold = fonts.bold.as_ref().or(regular);
        let mut y = self.tiles_top() + (TILE_H + TILE_GAP) * s;
        let mut out = Vec::new();
        for f in self.fuss() {
            if let (Some(z), Some(r), Some(b)) = (f.ziel, regular, bold) {
                let x = x0 + r.width(&f.text, px);
                out.push((z, (x, y, b.width(&f.verweis, px), FOOT_LINE * s)));
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
        if self
            .lohnsatz_lage(t, fonts)
            .is_some_and(|(_, _, _, r)| inside(r, x, y))
        {
            return Some(Hot::Lohnsatz);
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
        if let Some(l) = self.lohn.as_mut() {
            let repaint = l.mouse_move(fonts, x as f32, y as f32);
            if l.enthaelt(x as f32, y as f32) || l.form == lohn_blatt::Form::Blatt {
                let mut out = repaint.then_some(ListOut::Repaint);
                if self.hot.take().is_some() {
                    out = Some(ListOut::Repaint);
                }
                return out;
            }
        }
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
        if let Some(l) = self.lohn.as_mut() {
            // Die Karte steht neben der Arbeit; das Blatt schreibt daneben
            if l.enthaelt(x as f32, y as f32) || l.form == lohn_blatt::Form::Blatt {
                let aus = l.mouse_down(fonts, x as f32, y as f32)?;
                return self.lohn_aus(aus);
            }
        }
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
            Hot::Lohnsatz => {
                self.lohn_wunsch = Some(lohn_blatt::Form::Blatt);
                Some(ListOut::Repaint)
            }
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

    /// Nach „Bauleistung wählen“ zur neuen Position springen und die
    /// Markierung starten; findet der Aufbau sie nicht, ist sie vorbei.
    fn blitz_suchen(&mut self, blatt: &Kostenblatt) {
        let Some((el, g, None)) = self.blitz else {
            return;
        };
        let passt = |z: &Zeile| {
            z.pos.is_some_and(|(p, _)| {
                blatt.positionen.get(p).is_some_and(|x| match x.quelle {
                    sk_cost::rechnung::Quelle::Leistung(l)
                    | sk_cost::rechnung::Quelle::Geschaetzt(l) => l == g,
                    _ => false,
                })
            }) && z.elements.contains(&el)
        };
        self.blitz = self.springe(passt).map(|_| (el, g, Some(Instant::now())));
    }

    /// Zeile `i` ist die eben zugeordnete Position.
    fn blitzt(&self, i: usize, now: Instant) -> bool {
        let Some((el, _, Some(at))) = self.blitz else {
            return false;
        };
        let z = &self.zeilen[i];
        now.duration_since(at) < BLITZ && z.pos.is_some() && z.elements.contains(&el)
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
        if self
            .lohn
            .as_ref()
            .is_some_and(|l| l.form == lohn_blatt::Form::Blatt)
        {
            return None;
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
        let mut weiter = self.leiste.tick(t, now);
        if let Some((_, _, Some(at))) = self.blitz {
            if now.duration_since(at) < BLITZ {
                weiter = true;
            } else {
                self.blitz = None;
                weiter = true;
            }
        }
        weiter
    }

    pub fn overlay_open(&self) -> bool {
        self.leiste.overlay_open()
    }

    pub fn tip_at(&self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<String> {
        if let Some(l) = self.lohn.as_ref() {
            if l.enthaelt(x as f32, y as f32) {
                return l.tip_at(fonts, x as f32, y as f32);
            }
        }
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
                Some(a.tooltip())
            }
            Hot::Lohnsatz => Some(match self.lohn_firma {
                Some(f) => format!(
                    "eigener Lohn für dieses Haus, Firma {} €/h · Klick ändert ihn",
                    euro(f.cent())
                ),
                None => "Stundenlohn ändern".into(),
            }),
            Hot::Uebernehmen => self.tip_uebernehmen(),
            Hot::Lassen => Some(
                "Dieses Haus rechnet weiter mit seinen Werten. Die Zeile kommt wieder, wenn sich für neue Häuser erneut etwas ändert.".into(),
            ),
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
        self.paint_rows(c, t, fonts, now);
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
        if let Some(l) = &self.lohn {
            l.paint(c, t, fonts);
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
                crate::cards::verweis(u, unter(Hot::Uebernehmen)),
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

    fn paint_rows(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, now: Instant) {
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
            if sel || self.blitzt(i, now) {
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
                    let hot = self.hot == Some(Hot::Waehlen(i));
                    b.draw(c, WAEHLEN, px, lx, base, crate::cards::verweis(u, hot));
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
                // Klein neben dem Betrag, rechtsbündig auf seiner Grundlinie
                let by = y + 38.0 * s;
                if k == 2 {
                    // „60,00 €/h“ als Verweis aufs Lohnfeld (Bedienbarkeit 4.7)
                    if let Some((vor, link, nach, (lx, ly, ..))) = self.lohnsatz_lage(t, fonts) {
                        let px = 10.0 * s;
                        let by = ly + 10.0 * s;
                        let fb = bold.unwrap_or(f);
                        f.draw(c, &vor, px, lx - f.width(&vor, px), by, u.sheet_text_dim);
                        let col = crate::cards::verweis(u, self.hot == Some(Hot::Lohnsatz));
                        fb.draw(c, &link, px, lx, by, col);
                        let mut nx = lx + fb.width(&link, px);
                        // Punkt hinter dem Stundenlohn: eigener Lohn (paket-ka2 §4)
                        if self.lohn_firma.is_some() {
                            let d = 1.5 * s;
                            let cx = nx + 4.0 * s;
                            let mut p = Path::new();
                            p.rounded_rect(cx - d, by - 3.5 * s - d, 2.0 * d, 2.0 * d, d);
                            c.fill(&p, u.accent);
                            nx += 8.0 * s;
                        }
                        f.draw(c, &nach, px, nx, by, u.sheet_text_dim);
                    }
                } else if !klein.is_empty() {
                    let px = 10.0 * s;
                    let k = sk_ui::widgets::ellipsize(Some(f), klein, px, tw * 0.5);
                    let kw = f.width(&k, px);
                    f.draw(c, &k, px, x + tw - 12.0 * s - kw, by, u.sheet_text_dim);
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
        let b = fonts.bold.as_ref().unwrap_or(f);
        let px = 10.5 * s;
        let mut y = self.tiles_top() + (TILE_H + TILE_GAP) * s;
        for z in fuss {
            let base = y + (FOOT_LINE * s + f.cap_height(px)) * 0.5;
            f.draw(c, &z.text, px, x0, base, u.sheet_text_dim);
            if !z.verweis.is_empty() {
                let col = crate::cards::verweis(u, z.ziel.is_some() && self.hot == z.ziel);
                b.draw(c, &z.verweis, px, x0 + f.width(&z.text, px), base, col);
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
                        &csv_text(&p.oz),
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

/// Zelle, die eine Tabellenkalkulation als Text lesen soll: OZ wie
/// „01.02“ oder „01.0010“ würde das deutsche Excel sonst zum Datum oder zur
/// Zahl machen. `="01.02"` bleibt Text; leer bleibt leer.
pub(crate) fn csv_text(s: &str) -> String {
    if s.is_empty() {
        String::new()
    } else {
        format!("=\"{}\"", s.replace('"', "\"\""))
    }
}

fn csv_feld(s: &str) -> String {
    if s.contains([';', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Abnahme KA-2a/b durch Test (paket-ka2 §6 Nr. 3, 6a, 7, 10).
#[cfg(test)]
mod abnahme_ka2 {
    use super::*;
    use sk_cost::{Herkunft, HerkunftArt, Op, SatzId};

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

    fn h() -> Herkunft {
        Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "11:40")
    }

    fn leistung(s: &Scene, kurz: &str) -> sk_cost::katalog::Leistung {
        let k = sk_cost::lesen::katalog(s.model(), None);
        k.leistungen
            .iter()
            .find(|l| l.kurz.starts_with(kurz))
            .unwrap_or_else(|| panic!("{kurz}"))
            .clone()
    }

    /// Nr. 3: Im Modus Material zählt eine NU-Position nicht, die Fußzeile
    /// nennt sie mit ihrer Summe.
    #[test]
    fn nr3_nu_im_modus_material() {
        let mut s = haus();
        let w20 = leistung(&s, "WDVS EPS 035 d=140mm");
        let mut daten = sk_cost::preis::bauleistung(&w20);
        daten.nu = Some(Dez::ganz(110));
        s.kosten(
            None,
            &h(),
            Op::BauleistungAendern {
                bauleistung: w20.guid,
                daten,
            },
        )
        .unwrap();
        let mut v = KostenView::new();
        v.modus = Modus::Material;
        v.sync(&mut s, None);
        let b = v.blatt().unwrap();
        assert_eq!(b.nu, Cent(2_195_545));
        assert_eq!(v.netto(), Some(Cent(3_114_834 - 997_975)));
        let z = v
            .zeilen()
            .iter()
            .find(|z| z.text.starts_with("WDVS EPS 035"))
            .expect("WDVS-Zeile");
        assert_eq!(z.betrag, None, "{z:?}");
        let f = v.fuss();
        assert!(
            f.iter()
                .any(|x| x.text.contains("NU-Position") && x.text.contains("21.955,45")),
            "{:?}",
            f.iter().map(|x| &x.text).collect::<Vec<_>>()
        );
    }

    /// Nr. 6a: Projekt ohne die Deckenleistung B30 (ausgemustert): die
    /// Decken stehen grau unter ihrem Gewerk, die Fußzeile nennt sie; in
    /// der CSV keine OZ doppelt, WDVS unter 2.01.0020.
    #[test]
    fn nr6a_ohne_preis_und_oz() {
        let mut s = haus();
        let b30 = leistung(&s, "Stb-Decke Ortbeton");
        s.kosten(
            None,
            &h(),
            Op::Ausmustern {
                satz: SatzId {
                    abschnitt: "service",
                    kennung: b30.guid.to_ifc(),
                },
            },
        )
        .unwrap();
        let mut v = KostenView::new();
        v.sync(&mut s, None);
        let z = v.zeilen();
        let grau: Vec<&Zeile> = z.iter().filter(|x| x.art == Art::Ohne).collect();
        assert!(
            grau.iter().any(|x| x.text.starts_with("Geschossdecke")),
            "{grau:?}"
        );
        let i = z
            .iter()
            .position(|x| x.art == Art::Ohne && x.text.starts_with("Geschossdecke"))
            .unwrap();
        let g = z[..i]
            .iter()
            .rev()
            .find(|x| x.art == Art::Gruppe && x.ebene == 0)
            .unwrap();
        assert!(
            g.text.contains("Beton"),
            "Decke unter ihrem Gewerk: {}",
            g.text
        );
        let f = v.fuss();
        assert!(
            f.iter()
                .any(|x| x.text == "Ohne Preis: " && x.verweis.contains("Geschossdecke")),
            "{:?}",
            f.iter().map(|x| (&x.text, &x.verweis)).collect::<Vec<_>>()
        );
        // OZ: einmal je Position, mit Los
        let mut v = KostenView::new();
        let mut s = haus();
        v.sync(&mut s, None);
        let csv = String::from_utf8(v.csv("Standardhaus", "08.10.2026")).unwrap();
        // OZ als Text für Excel: ="2.01.0020"
        assert!(csv.contains(";\"=\"\"2.01.0020\"\"\";WDVS"), "{csv}");
        let mut oz: Vec<&str> = csv
            .lines()
            .skip_while(|l| !l.starts_with("Gliederung;"))
            .skip(1)
            .take_while(|l| !l.starts_with("Netto;"))
            .filter_map(|l| l.split(';').nth(2))
            .filter(|o| !o.is_empty())
            .collect();
        let n = oz.len();
        oz.sort();
        oz.dedup();
        assert_eq!(oz.len(), n, "keine OZ zweimal: {oz:?}");
    }

    /// Nr. 7: AW Porenbeton 20 cm ergibt eine eigene Zeile „geschätzt nach
    /// M10“ mit EP 57,17; die Fußzeile nennt „davon geschätzt“.
    #[test]
    fn nr7_geschaetzt() {
        let mut s = haus();
        let m = s.model();
        let (id, mut t) = m
            .layer_sets()
            .iter()
            .find(|(id, t)| t.code.starts_with("AW") && !m.type_users(*id).is_empty())
            .map(|(id, t)| (id, t.clone()))
            .unwrap();
        let l = t
            .layers
            .iter_mut()
            .find(|l| {
                m.material(l.material)
                    .is_some_and(|x| x.name == "Porenbeton")
            })
            .unwrap();
        l.thickness = 200.0;
        s.edit_types("Dicke", |m| m.set_layer_set(id, t));
        let mut v = KostenView::new();
        v.sync(&mut s, None);
        let z: Vec<&Zeile> = v.zeilen().iter().filter(|x| x.geschaetzt).collect();
        assert!(
            z.iter()
                .any(|x| x.leise.contains("geschätzt nach") && x.ep == "57,17"),
            "{z:?}"
        );
        let f = v.fuss();
        assert!(
            f.iter().any(|x| x.text.starts_with("davon geschätzt")),
            "{:?}",
            f.iter().map(|x| &x.text).collect::<Vec<_>>()
        );
    }

    /// Nr. 10: `[mengenfenster] blatt=kosten` wird geschrieben und gelesen;
    /// ohne Angabe das Mengenblatt.
    #[test]
    fn nr10_blatt_wird_gemerkt() {
        use crate::cards::Blatt;
        use crate::windows::{read_blatt, write_settings_blatt, Windows, WIDTH_DIP};
        let w = Windows::new(WIDTH_DIP);
        let g = crate::schedule_view::Grouping::default();
        let text = write_settings_blatt(&w, g, Blatt::Kosten);
        assert!(text.contains("blatt=kosten"), "{text}");
        assert_eq!(read_blatt(&text), Blatt::Kosten);
        assert_eq!(read_blatt(""), Blatt::Mengen);
        assert_eq!(crate::cards::KNOPF, "Mengen · Kosten · AVA");
    }
    /// Nr. 5 (Teil): Mit offenem Kostenblatt erhöht ein Loslassen nach Wand
    /// verschieben `schedule_runs` um genau 1, die Kosten folgen im selben
    /// Lauf.
    #[test]
    fn nr5_loslassen_ein_lauf() {
        let mut s = haus();
        let mut v = KostenView::new();
        v.sync(&mut s, None);
        let runs = s.schedule_runs();
        let netto = v.netto().unwrap();
        let run = s.model().runs().ids().next().unwrap();
        let moved = s.chain(run).unwrap().with_segment_moved(0, -500.0).unwrap();
        s.begin("Wand verschieben");
        s.set_run_points(run, &moved.points);
        s.commit();
        v.sync(&mut s, None);
        assert_eq!(s.schedule_runs(), runs + 1);
        assert_ne!(v.netto().unwrap(), netto, "Kosten folgen");
        v.sync(&mut s, None);
        assert_eq!(s.schedule_runs(), runs + 1);
    }
    /// Review 3ai: Die Oberfläche addiert keine Gruppensummen selbst. Für
    /// RH-1 bis RH-3 und jede Gliederung sind die Gruppensummen der Ansicht
    /// die des Kostenblatts (`nach_gewerk`, `nach_geschoss`, `nach_kg`), der
    /// „Rundungsausgleich“ der des Blatts (RH-1 Geschoss +0,03, RH-2 KG
    /// −1,60), und die CSV trägt dieselben Summen.
    #[test]
    fn gruppensummen_aus_dem_kostenblatt() {
        for (name, text) in [
            (
                "RH-1",
                include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
            ),
            (
                "RH-2",
                include_str!("../../crates/sk-cost/referenz/rh2-mehrschalig.szo"),
            ),
            (
                "RH-3",
                include_str!("../../crates/sk-cost/referenz/rh3-versatz-dachterrasse.szo"),
            ),
        ] {
            let m = sk_model::szo::read_with(
                text,
                sk_model::GuidGen::with_seed(1),
                &sk_cost::lesen::ABSCHNITTE_SZO,
            )
            .unwrap()
            .model;
            let mut s = Scene::with_model(m);
            let mut v = KostenView::new();
            for g in Gliederung::ALLE {
                v.gliederung = g;
                v.sync(&mut s, None);
                let b = v.blatt().unwrap().clone();
                let (mut soll, ausgleich): (Vec<i64>, Cent) = match g {
                    Gliederung::Gewerk => (b.nach_gewerk.iter().map(|x| x.1 .0).collect(), Cent(0)),
                    Gliederung::Geschoss => (
                        b.nach_geschoss.iter().map(|x| x.1 .0).collect(),
                        b.ausgleich_geschoss,
                    ),
                    Gliederung::Kostengruppe => {
                        (b.nach_kg.iter().map(|x| x.1 .0).collect(), b.ausgleich_kg)
                    }
                };
                soll.retain(|c| *c != 0);
                soll.sort();
                let z = v.zeilen();
                // Gruppen der untersten Teilung (bei KG die dreistelligen)
                let mut ist: Vec<i64> = z
                    .iter()
                    .filter(|x| x.art == Art::Gruppe)
                    .filter(|x| match g {
                        Gliederung::Kostengruppe => x
                            .text
                            .split(' ')
                            .next()
                            .and_then(|n| n.parse::<u16>().ok())
                            .is_some_and(|n| b.nach_kg.iter().any(|k| k.0 == Some(n))),
                        _ => x.ebene == 0,
                    })
                    .filter_map(|x| x.betrag.map(|c| c.0))
                    .filter(|c| *c != 0)
                    .collect();
                ist.sort();
                assert_eq!(ist, soll, "{name} {g:?}");
                let aus: Vec<Cent> = z
                    .iter()
                    .filter(|x| x.art == Art::Ausgleich)
                    .filter_map(|x| x.betrag)
                    .collect();
                if ausgleich == Cent(0) {
                    assert!(aus.is_empty(), "{name} {g:?}: {aus:?}");
                } else {
                    assert_eq!(aus, [ausgleich], "{name} {g:?}");
                }
                assert_eq!(v.netto(), Some(b.netto), "{name} {g:?}");
                // CSV: dieselben Gruppensummen und dasselbe Netto
                let csv = String::from_utf8(v.csv(name, "08.10.2026")).unwrap();
                let zahl = |c: Cent| euro(c).replace('.', "");
                let gruppen: Vec<String> = z
                    .iter()
                    .filter(|x| x.art == Art::Gruppe)
                    .map(|x| x.betrag.map(zahl).unwrap_or_default())
                    .collect();
                let csv_gruppen: Vec<String> = csv
                    .lines()
                    .map(|l| l.split(';').collect::<Vec<_>>())
                    .filter(|c| c.len() == 14 && c[3] == "Summe")
                    .map(|c| c[11].to_string())
                    .collect();
                assert_eq!(csv_gruppen, gruppen, "{name} {g:?}");
                assert!(
                    csv.contains(&format!("\r\nNetto;{}\r\n", zahl(b.netto))),
                    "{name} {g:?}"
                );
            }
        }
    }
}

/// Abnahme KA-2c durch Test (paket-ka2 §6 Nr. 4, 11, 13).
#[cfg(test)]
mod abnahme_ka2c {
    use super::*;
    use sk_cost::{Herkunft, HerkunftArt, Op, SatzId};

    const PB175: &str = "1S7bUW0010080100000002";

    fn rh2() -> Scene {
        let m = sk_model::szo::read_with(
            include_str!("../../crates/sk-cost/referenz/rh2-mehrschalig.szo"),
            sk_model::GuidGen::with_seed(1),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .unwrap()
        .model;
        Scene::with_model(m)
    }

    fn h() -> Herkunft {
        Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "12:30")
    }

    fn pb175_24() -> Op {
        Op::PreisSetzen {
            artikel: sk_model::Guid::from_ifc(PB175).unwrap(),
            preis: Some(Dez::ganz(24)),
            stand: "10/2026".into(),
            quelle: "Abnahme 4".into(),
        }
    }

    /// EP der Zeilen „AW …17,5“ (M10) und „IW …17,5“ (M50) und ob sie den
    /// Punkt tragen.
    fn m10_m50(v: &KostenView) -> Vec<(String, bool)> {
        v.zeilen()
            .iter()
            .filter(|z| matches!(z.art, Art::Position { .. }))
            .filter(|z| z.text.contains("Planstein") && z.text.contains("17,5"))
            .map(|z| {
                (
                    z.ep.clone(),
                    z.pos.is_some_and(|p| v.eigen.contains_key(&p.0)),
                )
            })
            .collect()
    }

    /// Nr. 4 und 11: A-PB175 auf 24,00 im Projekt ändert M10 und M50 (EP
    /// 27,00 + 24,00 + 4,84 = 55,84), beide tragen den Punkt; ein
    /// Rückgängig-Schritt „Preis im Projekt geändert“, `[origin]` mit
    /// `kind=manual` und `proj=1`; Strg+Z nimmt es zurück, Wiederholen
    /// bringt es; Speichern und Laden behält es.
    #[test]
    fn nr4_artikelpreis_im_projekt() {
        let mut s = rh2();
        let mut v = KostenView::new();
        v.sync(&mut s, None);
        assert_eq!(
            m10_m50(&v),
            [("54,00".into(), false), ("54,00".into(), false)]
        );
        s.kosten_folge("Preis im Projekt geändert", None, &h(), &[pb175_24()])
            .unwrap();
        assert_eq!(s.undo_label(), Some("Preis im Projekt geändert"));
        v.sync(&mut s, None);
        assert_eq!(
            m10_m50(&v),
            [("55,84".into(), true), ("55,84".into(), true)]
        );
        let text = sk_model::szo::write(s.model());
        let origin = text
            .lines()
            .find(|l| l.starts_with("[origin]") && l.contains(PB175))
            .expect("[origin] zum Artikel");
        assert!(
            origin.contains("kind=manual") && origin.contains("proj=1"),
            "{origin}"
        );
        // Strg+Z und Wiederholen
        assert!(s.undo());
        v.sync(&mut s, None);
        assert_eq!(
            m10_m50(&v),
            [("54,00".into(), false), ("54,00".into(), false)]
        );
        assert!(s.redo());
        v.sync(&mut s, None);
        assert_eq!(
            m10_m50(&v),
            [("55,84".into(), true), ("55,84".into(), true)]
        );
        // Speichern und Laden
        let m = sk_model::szo::read_with(
            &sk_model::szo::write(s.model()),
            sk_model::GuidGen::with_seed(2),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .unwrap()
        .model;
        let mut s2 = Scene::with_model(m);
        let mut v2 = KostenView::new();
        v2.sync(&mut s2, None);
        assert_eq!(
            m10_m50(&v2),
            [("55,84".into(), true), ("55,84".into(), true)]
        );
        assert_eq!(v2.netto(), v.netto());
    }

    /// Nr. 13: „Firmenpreis zurückholen“ nimmt die Abweichung zurück: Punkt
    /// weg, EP wie Werk, ein Rückgängig-Schritt.
    #[test]
    fn nr13_firmenpreis_zurueckholen() {
        let mut s = rh2();
        let mut v = KostenView::new();
        v.sync(&mut s, None);
        let netto = v.netto();
        s.kosten_folge("Preis im Projekt geändert", None, &h(), &[pb175_24()])
            .unwrap();
        v.sync(&mut s, None);
        assert_ne!(v.netto(), netto);
        let op = Op::AbweichungZuruecknehmen {
            saetze: vec![SatzId {
                abschnitt: "article",
                kennung: PB175.into(),
            }],
        };
        s.kosten_folge("Firmenpreis zurückgeholt", None, &h(), &[op])
            .unwrap();
        assert_eq!(s.undo_label(), Some("Firmenpreis zurückgeholt"));
        v.sync(&mut s, None);
        assert_eq!(
            m10_m50(&v),
            [("54,00".into(), false), ("54,00".into(), false)]
        );
        assert_eq!(v.netto(), netto);
        assert!(s.undo());
        v.sync(&mut s, None);
        assert_eq!(
            m10_m50(&v),
            [("55,84".into(), true), ("55,84".into(), true)]
        );
    }
}

/// Abnahme KA-2c2 Nr. 8 (paket-ka2 §6): Lohnfeld für dieses und neue Häuser,
/// Strg+Z nur für dieses Haus, Abgleichzeile, „Nur dieses Haus“ mit Marke.
#[cfg(test)]
mod abnahme_ka2c2_lohn;

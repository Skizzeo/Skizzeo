//! Erdarbeiten als Automatikmengen (VOB/C ATV DIN 18300, DIN 4124): aus der
//! Gründung eines Gebäudes und den Bodenkennwerten des Projekts. Nur Lesen,
//! keine Körper. Die Kosten rechnen damit über Bauleistungen mit
//! `auto=earth.…` (kosten/erdarbeiten-recherche.md).
//!
//! Höhen relativ zu ±0,00 (OK Sohlplatte), nach oben positiv:
//! OK Gelände → (Oberboden) → gewachsener Boden → UK Platte → (innerhalb
//! der Frostschürze: Perimeterdämmung, kapillarbrechende Schicht) → UK
//! Schürze. Gebaut wird auf einer Sandplatte (Jörn 17./18.10.): ausgekoffert
//! [`AUSKOFFERUNG`] ab OK Gelände, Oberboden eingeschlossen, tiefer, wenn
//! die Gründung tiefer liegt, [`UEBERSTAND`] über die Platte hinaus; darin
//! Füllsand bis UK Dämmung (innerhalb der Schürze bis UK Schicht), darüber
//! Verfüllung. Die Gräben der Schürze werden danach aus dem Füllsand und
//! darunter aus dem gewachsenen Boden gehoben, so breit wie die Schürze.
//! Die Darstellung (`app/src/gelaende_platte/bodenaufbau.rs`) rechnet mit
//! derselben [`Sandplatte`].

use crate::foundation::Foundation;
use crate::qto::AutoMenge;
use sk_math::Vec3;

mod flaechen;
mod netz;
pub mod ueberdeckung;
pub(crate) use flaechen::{gelaende_mengen, planungen, Merker as FlaechenMerker};
pub use flaechen::{
    Gruendung, AUSSERHALB, BEFESTIGT, GANZ_AUSSERHALB, GRUNDSTUECK, GRUNDSTUECK_SCHLUESSEL, KEIL,
    OHNE_AUFMASS, OHNE_TRAEGER, UEBERLAPPT, UNGUELTIG,
};
/// Projekthinweis für zwei Häuser, die ineinander stehen (H-245-geo-a,
/// [`crate::Model::haeuser_ineinander`]), hinter ihren Namen; VOB: im
/// Rechenweg, mit beiden Häusern.
pub const INEINANDER: &str = "stehen ineinander: Wände und Baugruben überdecken sich, \
     Wand-, Gründungs- und Erdmengen dort doppelt, Lage prüfen";

/// Zwei Häuser, die ineinander stehen, mit ihrem Hinweis aus
/// [`ineinander`], wie er in [`crate::qto::Schedule::hinweise`] steht.
#[derive(Clone, Debug, PartialEq)]
pub struct Ineinander {
    pub haeuser: (crate::BuildingId, crate::BuildingId),
    pub satz: String,
}

impl Ineinander {
    /// Gehört das Haus `b` zum Paar?
    pub fn mit(&self, b: crate::BuildingId) -> bool {
        self.haeuser.0 == b || self.haeuser.1 == b
    }
}

/// [`INEINANDER`] je Paar von Häusern, die ineinander stehen, für die
/// Hinweise des Projekts.
pub(crate) fn ineinander(m: &crate::Model) -> Vec<Ineinander> {
    let name = |b| {
        m.building(b)
            .map_or_else(String::new, |x| format!("{} ({})", x.name, x.number))
    };
    let nummer = |b| m.building(b).map(|x| x.number.clone());
    let mut paare = Vec::new();
    for (a, b) in m.haeuser_ineinander() {
        // nach Nummer gelesen: „Gebäude 1 und Gebäude 2“ (Kosten K-248a)
        let (a, b) = if nummer(b) < nummer(a) {
            (b, a)
        } else {
            (a, b)
        };
        paare.push(Ineinander {
            haeuser: (a, b),
            satz: format!("{} und {} {INEINANDER}", name(a), name(b)),
        });
    }
    // mehrere Paare nach Nummer gelesen (VOB H-256a)
    paare.sort_by_cached_key(|x| (nummer(x.haeuser.0), nummer(x.haeuser.1)));
    paare
}

/// Je Gründung in `gr` die Fläche (mm²), die ihr Baufeld an die Baufelder
/// daneben abgibt, und der Rechenweg dazu für den Oberboden der Dicke `o`
/// (mm), je Nachbar eine Zeile „; − Überdeckung mit … (Abstand … m): …
/// m³“ (H-280-geo-a, VOB 18.10. 15:05): Liegen zwei Häuser näher, als ihre
/// Baufelder breit sind, zählt das Projekt die Vereinigung, und jedes Haus
/// gibt die Hälfte der Überdeckung ab ([`ueberdeckung::anteile`]). Häuser,
/// die ineinander stehen ([`crate::Model::haeuser_ineinander`]), werden
/// nicht gekürzt; dort steht der Hinweis [`INEINANDER`].
pub(crate) fn baufelder_teilen(m: &crate::Model, gr: &[&Gruendung], o: f64) -> Vec<(f64, String)> {
    let ineinander = m.haeuser_ineinander();
    let haus = |i: usize| gr[i].vorlage.building;
    let raender: Vec<ueberdeckung::Rand> = gr
        .iter()
        .map(|g| ueberdeckung::Rand {
            umriss: &g.outline,
            versatz: g.zone.baufeld.max(0.0),
        })
        .collect();
    let anteile = ueberdeckung::anteile(&raender, |i, j| match (haus(i), haus(j)) {
        (Some(a), Some(b)) => !ineinander.contains(&(a, b)) && !ineinander.contains(&(b, a)),
        _ => true,
    });
    let name = |i: usize, von: usize| match haus(i) {
        Some(b) if haus(von) != Some(b) => m
            .building(b)
            .map_or_else(String::new, |x| format!("{} ({})", x.name, x.number)),
        _ => format!("Sohlplatte {}", gr[i].vorlage.number),
    };
    anteile
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let mut text = String::new();
            if o > 0.0 {
                for &(j, f) in &a.ab {
                    let d = ueberdeckung::abstand(&gr[i].outline, &gr[j].outline);
                    text += &format!(
                        "; − Überdeckung mit {} (Abstand {} m): {} m³",
                        name(j, i),
                        zahl(d, 1e3),
                        zahl(f * o, 1e9)
                    );
                }
            }
            (a.summe_ab(), text)
        })
        .collect()
}

/// Die Oberbodenzeile `r` um `weniger` (mm³) gekürzt, mit dem Rechenweg
/// `text` aus [`baufelder_teilen`] und der Restmenge am Schluss (VOB
/// V-285a).
pub(crate) fn oberboden_kuerzen(r: &mut AutoMenge, weniger: f64, text: &str) {
    r.value -= weniger;
    r.formula += text;
    r.formula += &format!("; = {} m³", zahl(r.value, 1e9));
}

pub use netz::{
    hand_geaendert, netz_angenommen, netz_geaendert, netz_herkunft, steil_hinweis, versatz,
    ErdNetz, ANGENOMMEN, DREIECKSPRISMEN, HAND_GEAENDERT, STEIL, STEIL_AB,
};

/// Bodenkennwerte des Projekts (mm, Grad); stehen am Projekt
/// ([`crate::Project::soil`]) und in der Datei nur, wenn sie von der
/// Vorgabe abweichen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Boden {
    /// Oberboden: abtragen und seitlich lagern (DIN 18915, ATV DIN 18320).
    pub oberboden: f64,
    /// Abtrag um die Platte: so weit um den Plattenumriss wird der Oberboden abgetragen
    /// (Arbeitsraum, Gerüst, Wege); mindestens bis zum Rand der Baugrube.
    pub rand: f64,
    /// Arbeitsraum um den Plattenumriss (DIN 4124: mindestens 0,50 m).
    pub arbeitsraum: f64,
    /// Kapillarbrechende Schicht unter der Platte innerhalb der Schürze:
    /// Dicke, und ob sie eingebaut wird (Jörn 17.10.: eine Position zum
    /// Zuschalten, in neuen Projekten aus; sonst Füllsand bis UK Dämmung).
    /// Die Dicke bleibt, wenn sie aus ist. Dateien ohne Schalter haben sie.
    pub tragschicht: f64,
    /// Ob die kapillarbrechende Schicht eingebaut wird (`[soil]
    /// capillary_on`); aus heißt keine Kiesposition, die Dicke bleibt.
    pub kies: bool,
    /// Böschungswinkel β, wenn die Baugrube tiefer ist als
    /// [`SENKRECHT_BIS`] (DIN 4124: 45° nichtbindig oder weich bindig, 60°
    /// steif bindig, 80° Fels).
    pub boeschung: f64,
}

impl Default for Boden {
    fn default() -> Boden {
        Boden {
            oberboden: 300.0,
            rand: 1500.0,
            arbeitsraum: 500.0,
            tragschicht: 150.0,
            kies: true,
            boeschung: 45.0,
        }
    }
}

impl Boden {
    /// Schlüssel an `[project]`, Bezeichnung und erlaubter Bereich (mm bzw.
    /// Grad), in der Reihenfolge der Felder.
    pub const FELDER: [(&'static str, &'static str, f64, f64); 5] = [
        ("topsoil", "Oberboden", 0.0, 1000.0),
        ("strip", "Abtrag um die Platte", 0.0, 10000.0),
        ("workspace", "Arbeitsraum", 0.0, 2000.0),
        ("capillary", "Kapillarbrechende Schicht", 0.0, 1000.0),
        ("slope", "Böschungswinkel", 30.0, 90.0),
    ];

    /// Schlüssel des Schalters an `[soil]`: steht nur als `0`, wenn die
    /// kapillarbrechende Schicht aus ist.
    pub const SCHALTER: &'static str = "capillary_on";

    /// Dicke der kapillarbrechenden Schicht, wie sie eingebaut wird (0, wenn
    /// sie aus ist).
    pub fn schicht(&self) -> f64 {
        if self.kies {
            self.tragschicht.max(0.0)
        } else {
            0.0
        }
    }

    /// Die Werte in der Reihenfolge von [`Boden::FELDER`].
    pub fn werte(&self) -> [f64; 5] {
        [
            self.oberboden,
            self.rand,
            self.arbeitsraum,
            self.tragschicht,
            self.boeschung,
        ]
    }

    /// Aus Werten in der Reihenfolge von [`Boden::FELDER`], die Schicht
    /// eingebaut; `None`, wenn einer außerhalb seines Bereichs liegt.
    pub fn aus_werten(w: [f64; 5]) -> Option<Boden> {
        let ok = w
            .iter()
            .zip(Boden::FELDER)
            .all(|(v, f)| v.is_finite() && *v >= f.2 && *v <= f.3);
        ok.then_some(Boden {
            oberboden: w[0],
            rand: w[1],
            arbeitsraum: w[2],
            tragschicht: w[3],
            kies: true,
            boeschung: w[4],
        })
    }
}

/// Bis zu dieser Tiefe (mm ab Gelände) darf die Baugrube ohne Böschung
/// und ohne Verbau senkrecht sein (DIN 4124, 4.2.3).
pub const SENKRECHT_BIS: f64 = 1250.0;

/// Tiefe der Auskofferung unter OK Gelände, Oberboden eingeschlossen (mm;
/// Jörn 18.10.: 0,80 m ab OK Gelände). Fest und ohne Feld, wie
/// [`UEBERSTAND`] (Jörn 18.10.: keine Einstellungsfelder, wo ein fester
/// Wert reicht); der Rechenweg nennt beide.
pub const AUSKOFFERUNG: f64 = 800.0;

/// Überstand der Sandplatte über die Außenkante der Schürze (mm); liegt die
/// Platte im gewachsenen Boden, mindestens der Arbeitsraum.
pub const UEBERSTAND: f64 = 500.0;

/// Reicht die Schürze nicht mindestens so weit (mm) unter die Sohle der
/// Auskofferung, zählt ihr Graben ganz zum Füllsand: keine Position für
/// Kubikzentimeter aus der Rundung der Geländehöhe.
const GRABEN_AB: f64 = 1.0;

/// Sandplatte einer Gründung (mm, z zu ±0,00), wie Mengen und Darstellung
/// sie teilen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sandplatte {
    /// Sohle der Auskofferung: [`AUSKOFFERUNG`] unter OK Gelände, tiefer
    /// bis UK Schicht, nie über dem gewachsenen Boden.
    pub ka: f64,
    /// So weit reicht die Sohle über die Platte.
    pub w: f64,
    /// Rücklauf der Wand der Auskofferung je mm Höhe (0: senkrecht bis
    /// [`SENKRECHT_BIS`]).
    pub k: f64,
    /// Böschung der Sandplatte über dem gewachsenen Boden: Fall je mm
    /// waagerecht.
    pub tan: f64,
}

impl Sandplatte {
    /// Aus OK Gelände `t`, OK gewachsener Boden `n`, UK Dämmung `b` (ohne
    /// Dämmung UK Platte) und UK Schicht `p` (ohne Schicht `b`).
    pub fn neu(t: f64, n: f64, b: f64, p: f64, boden: &Boden) -> Sandplatte {
        let ka = (t - AUSKOFFERUNG).min(p).min(n);
        let tan = boden.boeschung.clamp(1.0, 89.0).to_radians().tan();
        let w = if b < n {
            UEBERSTAND.max(boden.arbeitsraum)
        } else {
            UEBERSTAND
        };
        let k = if t - ka > SENKRECHT_BIS {
            1.0 / tan
        } else {
            0.0
        };
        Sandplatte { ka, w, k, tan }
    }
}

/// Bis wohin im Rechenweg innerhalb der Schürze Füllsand kommt: UK der
/// untersten eingebauten Lage unter der Platte (VOB V-268a).
pub(crate) fn aufgefuellt_bis(daemmung: bool, kies: bool) -> &'static str {
    match (daemmung, kies) {
        (_, true) => "UK Schicht",
        (true, false) => "UK Dämmung",
        (false, false) => "UK Platte",
    }
}

/// Rechenweg des Planums, wenn die kapillarbrechende Schicht aus ist.
pub const NICHT_EINGEBAUT: &str = "; Kapillarbrechende Schicht: nicht eingebaut (Bodenkennwerte)";

/// Kostengruppe der Erdarbeiten (DIN 276: 311 Herstellung der Baugrube).
pub const KG: u16 = 311;
/// Kostengruppen des Geländebaus (G3b, kosten/kg-gelaende-g3.md): 511
/// Erdbau, 571 vegetationstechnische Bodenbearbeitung.
pub const KG_GELAENDE: u16 = 511;
pub const KG_ANDECKEN: u16 = 571;

/// Schlüssel der Erdmengen (`[service] auto=`), Einheit und Bedeutung.
pub const SCHLUESSEL: [(&str, &str, &str); 20] = [
    (TOPSOIL, "m3", "Oberboden abtragen und seitlich lagern"),
    (
        EXCAVATION,
        "m3",
        "Auskofferung im gewachsenen Boden (Homogenbereich B1)",
    ),
    (
        TRENCH,
        "m3",
        "Graben der Frostschürze im gewachsenen Boden (Homogenbereich B1)",
    ),
    (
        TRENCH_SAND,
        "m3",
        "Graben der Frostschürze im eingebauten Füllsand",
    ),
    (
        SUBGRADE,
        "m2",
        "Planum unter der kapillarbrechenden Schicht",
    ),
    (GRAVEL, "m3", "kapillarbrechende Schicht unter der Platte"),
    (
        SAND,
        "m3",
        "Füllsand der Sandplatte liefern, einbauen, verdichten",
    ),
    (
        FILL,
        "m3",
        "Auffüllung Schotter (Bodenaufbau vor der Sandplatte, nicht mehr gerechnet)",
    ),
    (BACKFILL, "m3", "Arbeitsraum verfüllen"),
    (DISPOSAL, "m3", "überschüssigen Aushub laden und abfahren"),
    (
        SLOPE,
        "m2",
        "Böschungsfläche abdecken (nur über 1,25 m Tiefe)",
    ),
    (
        TOPSOIL_SITE,
        "m3",
        "Oberboden abtragen auf Geländeflächen außerhalb des Baufelds",
    ),
    (
        CUT,
        "m3",
        "Gelände abtragen bis Planum (außerhalb der Baugrube)",
    ),
    (EMBANK, "m3", "Gelände auftragen mit Eigenboden bis Planum"),
    (
        EMBANK_SUPPLY,
        "m3",
        "Gelände auftragen mit Lieferboden bis Planum",
    ),
    (
        SURPLUS,
        "m3",
        "überschüssigen Geländeabtrag laden und abfahren",
    ),
    (
        TOPSOIL_SUPPLY,
        "m3",
        "Oberboden liefern (Fehlmenge der Andeckung)",
    ),
    (
        TOPSOIL_REMOVE,
        "m3",
        "überschüssigen Oberboden laden und abfahren",
    ),
    (SPREAD, "m2", "Oberboden andecken, alle Flächen"),
    (
        SPREAD_SLOPE,
        "m2",
        "Zulage Andecken an Böschungen steiler als 1:4",
    ),
];

pub const TOPSOIL: &str = "earth.topsoil";
pub const EXCAVATION: &str = "earth.excavation";
pub const TRENCH: &str = "earth.trench";
pub const TRENCH_SAND: &str = "earth.trench_sand";
pub const SAND: &str = "earth.sand";
pub const SUBGRADE: &str = "earth.subgrade";
pub const GRAVEL: &str = "earth.gravel";
pub const FILL: &str = "earth.fill";
pub const BACKFILL: &str = "earth.backfill";
pub const DISPOSAL: &str = "earth.disposal";
pub const SLOPE: &str = "earth.slope";
pub const TOPSOIL_SITE: &str = "earth.topsoil_site";
pub const CUT: &str = "earth.cut";
pub const EMBANK: &str = "earth.embank";
pub const EMBANK_SUPPLY: &str = "earth.embank_supply";
pub const SURPLUS: &str = "earth.surplus";
pub const TOPSOIL_SUPPLY: &str = "earth.topsoil_supply";
pub const TOPSOIL_REMOVE: &str = "earth.topsoil_remove";
pub const SPREAD: &str = "earth.spread";
pub const SPREAD_SLOPE: &str = "earth.spread_slope";

/// Was die Geländeflächen (G3b) von den Erdmengen einer Gründung brauchen:
/// wie weit um die Platte der Oberboden schon abgetragen ist und wo bis zur
/// Sohle ausgehoben wird.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ErdZone {
    /// Abtrag um die Platte (mm): das Baufeld.
    pub baufeld: f64,
    /// Sohle der Auskofferung (Platte samt Überstand) gegen den
    /// Uhrzeigersinn; leer, wenn nichts ausgehoben wird.
    pub sohle: Vec<Vec3>,
    /// OK Sandplatte um die Platte (UK Dämmung, mm über ±0,00): Darüber
    /// wird bis zum gewachsenen Boden verfüllt, ein Abtrag der
    /// Geländeflächen mindert dort die Verfüllung.
    pub b: f64,
    /// Die Böschung über dem Sohlrand ist mit dabei (dort angenähert).
    pub keil: bool,
}

/// Höhen und Flächen einer Gründung (mm, mm²), wie die Erdmengen sie
/// brauchen.
#[derive(Clone, Debug, PartialEq)]
pub struct ErdBasis {
    /// OK Gelände.
    pub terrain_z: f64,
    /// UK Platte (= OK Schürze).
    pub slab_bottom_z: f64,
    /// UK Perimeterdämmung (innerhalb der Schürze), ohne Dämmung UK Platte.
    pub insulation_bottom_z: f64,
    /// UK Frostschürze.
    pub footing_bottom_z: f64,
    /// Plattenumriss gegen den Uhrzeigersinn.
    pub outline: Vec<Vec3>,
    pub slab_area: f64,
    pub slab_perimeter: f64,
    /// Ring der Frostschürze im Grundriss.
    pub footing_area: f64,
    /// Mit Höhenaufmaß: die mittlere Geländehöhe in m ü. NHN, die
    /// `terrain_z` ist; mit ihr rechnet, was das Netz nicht deckt.
    pub aufmass: Option<f64>,
    /// Mit Höhenaufmaß das Netz (G2): Die Mengen kommen aus seinen
    /// Dreiecken statt aus der Ebene in `terrain_z`.
    pub netz: Option<ErdNetz>,
    /// Oberboden andecken des Grundstücks (mm): Um die Platte reicht der
    /// Füllsand höchstens bis unter den angedeckten Oberboden, wie in der
    /// Darstellung.
    pub andecken: f64,
}

impl ErdBasis {
    /// Aus den Grundlagen der Erdarbeiten des Modells (Gelände, Dämmung,
    /// Gründung; gelaende/schnittstelle.md).
    pub fn aus(g: &crate::GroundBasis) -> ErdBasis {
        ErdBasis {
            terrain_z: g.terrain_z,
            slab_bottom_z: g.slab_bottom_z,
            insulation_bottom_z: g.insulation_bottom_z,
            footing_bottom_z: g.footing_bottom_z,
            outline: g.outline.clone(),
            slab_area: g.slab_area,
            slab_perimeter: g.slab_perimeter,
            footing_area: g.footing_area,
            aufmass: g.terrain_nhn.map(|h| h + g.terrain_z / 1000.0),
            netz: None,
            andecken: crate::model::SiteSoil::default().spread,
        }
    }

    /// Wie [`ErdBasis::aus`], mit Aufmaß samt Netz aus dem Modell `m`.
    pub fn im_modell(m: &crate::Model, g: &crate::GroundBasis) -> ErdBasis {
        let netz = g.terrain_nhn.and_then(|_| ErdNetz::neu(m, g.run));
        ErdBasis {
            netz,
            andecken: m.site_soil().spread,
            ..ErdBasis::aus(g)
        }
    }

    /// Aus der Gründung, mit dem Gelände in Höhe `terrain_z` und ohne
    /// Perimeterdämmung.
    pub fn aus_gruendung(f: &Foundation, terrain_z: f64) -> ErdBasis {
        let p = f.params;
        ErdBasis {
            terrain_z,
            slab_bottom_z: -p.slab_thickness,
            insulation_bottom_z: -p.slab_thickness,
            footing_bottom_z: -p.slab_thickness - p.footing_depth,
            outline: f.outline.clone(),
            slab_area: f.slab_area(),
            slab_perimeter: f.slab_perimeter(),
            footing_area: f.footing_area(),
            aufmass: None,
            netz: None,
            andecken: crate::model::SiteSoil::default().spread,
        }
    }
}

/// Summe von tan(φ/2) über die Außenwinkel φ der Ecken: Ein Umriss, um `x`
/// nach außen versetzt (Ecken auf Gehrung), hat die Fläche
/// `A + U·x + T·x²`. Rechteck: 4.
fn eckzahl(pts: &[Vec3]) -> f64 {
    netz::ecken_tan(pts).iter().sum()
}

/// Zahl mit zwei Nachkommastellen und Komma, aus mm (`teiler` 1e3), mm²
/// (1e6) oder mm³ (1e9).
fn zahl(x: f64, teiler: f64) -> String {
    format!("{:.2}", x / teiler).replace('.', ",")
}

/// Das Netz des Höhenaufmaßes deckt die Baugrube der Gründung `g` nicht;
/// ihre Erdmengen rechnen mit der mittleren Höhe (Bedienbarkeit 42.5).
/// Ohne Aufmaß `false`.
pub(crate) fn baugrube_ohne_netz(m: &crate::Model, g: &crate::GroundBasis, boden: &Boden) -> bool {
    if g.terrain_nhn.is_none() {
        return false;
    }
    let b = ErdBasis::im_modell(m, g);
    b.netz.as_ref().is_none_or(|n| !netz::deckt(&b, n, boden))
}

/// Zusatz im Mengenansatz, wenn die Menge an der Geländehöhe aus dem
/// Aufmaß hängt; Kostenblatt und LV geben ihren Hinweis genau dann.
pub const AUS_AUFMASS: &str = "; Gelände aus Aufmaß";

/// Erdmengen einer Gründung (Bauteil `element`, Nummer `number` der
/// Sohlplatte). Mengen ≤ 0 entstehen nicht. Dazu Baufeld und Baugrube für
/// die Geländeflächen.
pub fn erd_mengen(
    b: &ErdBasis,
    boden: &Boden,
    mut menge: impl FnMut(&'static str, f64, String),
) -> ErdZone {
    // G2: über das Netz, wenn es die Baugrube deckt
    if let Some(z) = b
        .netz
        .as_ref()
        .and_then(|n| netz::mengen(b, n, boden, &mut menge))
    {
        return z;
    }
    // Mit Aufmaß nennt jeder Ansatz, der an der Geländehöhe hängt, die
    // Ebene (plan-gelaendemodul.md §5 G1)
    let gelaende = b.aufmass.map_or(String::new(), |h| {
        format!("{AUS_AUFMASS}, Mittel {}", zahl(h, 1.0))
    });
    let mut menge = |key: &'static str, wert: f64, ansatz: String| {
        if matches!(key, TOPSOIL | SUBGRADE | GRAVEL) {
            menge(key, wert, ansatz)
        } else {
            menge(key, wert, ansatz + &gelaende)
        }
    };
    let o = boden.oberboden.max(0.0);
    let g = boden.schicht();
    let (fl, u) = (b.slab_area, b.slab_perimeter);
    let ring = b.footing_area.min(fl).max(0.0);
    let innen = (fl - ring).max(0.0);
    let t = eckzahl(&b.outline);
    let pr = |x0: f64, k: f64, h: f64| prisma(fl, u, t, x0, k, h);
    // gewachsener Boden unter dem Oberboden
    let z_n = b.terrain_z - o;
    let z_s = b.slab_bottom_z;
    let z_b = b.insulation_bottom_z.min(z_s);
    let z_p = z_b - g;
    let z_f = b.footing_bottom_z;
    let sp = Sandplatte::neu(b.terrain_z, z_n, z_b, z_p, boden);
    let ka = sp.ka;
    // Auskofferung im gewachsenen Boden bis zur Sohle, Überstand um die
    // Platte; geböscht, wenn sie tiefer reicht als 1,25 m
    let h = z_n - ka;
    let grube = pr(sp.w, sp.k, h);
    let boeschung = sp.k > 0.0 && h > 0.0;
    // Sandplatte über dem gewachsenen Boden: läuft ab dem Überstand in der
    // Böschung aus, höchstens bis unter den angedeckten Oberboden
    let auf_h = (z_b - z_n).max(0.0);
    let auf_c = auf_h.min(o - b.andecken.clamp(0.0, o));
    let x_ob = boden
        .rand
        .max(sp.w + sp.k * h)
        .max(UEBERSTAND + auf_h / sp.tan);
    let breite = |x: f64| format!("{} m", zahl(x, 1e3));
    let bis = if z_b < z_s {
        "UK Dämmung"
    } else {
        "UK Platte"
    };

    oberboden(b, x_ob, o, &mut menge);
    let mut aushub = 0.0;
    if grube > 0.0 {
        let wand = if boeschung {
            format!(", Böschung {}°", boden.boeschung.round())
        } else {
            String::new()
        };
        // VOB B-BA-1: der Oberboden steht gesondert, hier nur der Boden
        menge(
            EXCAVATION,
            grube,
            format!(
                "Auskofferung {} ab OK Gelände = Oberboden {} (gesondert) + Boden {}: Platte {} \
                 m² + Überstand {}{wand}",
                breite(b.terrain_z - ka),
                breite(o),
                breite(h),
                zahl(fl, 1e6),
                breite(sp.w)
            ),
        );
        aushub += grube;
    }
    // Füllsand (VOB B-BA-2): innerhalb der Schürze bis UK Schicht, unter
    // der Schürze und um die Platte bis UK Dämmung; die Gräben werden danach
    // aus ihm ausgehoben
    let kern = innen * (z_p - ka).max(0.0);
    if kern > 0.0 {
        menge(
            SAND,
            kern,
            format!(
                "innerhalb der Schürze {} m² × {} bis {}",
                zahl(innen, 1e6),
                breite(z_p - ka),
                aufgefuellt_bis(z_b < z_s, boden.kies)
            ),
        );
    }
    let unter = ring * (z_b - ka).max(0.0);
    if unter > 0.0 {
        menge(
            SAND,
            unter,
            format!(
                "unter der Schürze {} m² × {} bis {bis}",
                zahl(ring, 1e6),
                breite(z_b - ka)
            ),
        );
    }
    let sand_h = z_b.min(z_n) - ka;
    let um = pr(sp.w, sp.k, sand_h) - fl * sand_h.max(0.0);
    if um > 0.0 {
        let oben = if z_b < z_n {
            bis
        } else {
            "OK gewachsener Boden"
        };
        menge(
            SAND,
            um,
            format!(
                "um die Platte, Überstand {}, {} bis {oben}",
                breite(sp.w),
                breite(sand_h)
            ),
        );
    }
    // bis zum Überstand voll, dahinter die Böschung (wie im Netz)
    let auftrag = pr(UEBERSTAND + (auf_h - auf_c) / sp.tan, 1.0 / sp.tan, auf_c) - fl * auf_c;
    let a_ue = pr(UEBERSTAND, 0.0, 1.0) - fl;
    if auftrag > 0.0 {
        let oben = if auf_c < auf_h {
            "unter den angedeckten Oberboden"
        } else {
            bis
        };
        menge(
            SAND,
            a_ue * auf_c,
            format!(
                "über dem gewachsenen Boden um die Platte {} m² (Überstand {}) × {} bis {oben}",
                zahl(a_ue, 1e6),
                breite(UEBERSTAND),
                breite(auf_c)
            ),
        );
        menge(
            SAND,
            auftrag - a_ue * auf_c,
            format!(
                "Böschung {}° der Sandplatte, Höhe bis {}",
                boden.boeschung.round(),
                breite(auf_c)
            ),
        );
    }
    // Gräben der Schürze ab OK Sandplatte (VOB B-BA-3): erst im Füllsand,
    // unter der Sohle der Auskofferung im gewachsenen Boden
    let unten = if ka - z_f > GRABEN_AB { ka } else { z_f };
    let im_sand = ring * (z_b - unten).max(0.0);
    if im_sand > 0.0 {
        menge(
            TRENCH_SAND,
            im_sand,
            format!(
                "Ring {} m² × {} ab OK Sandplatte",
                zahl(ring, 1e6),
                breite(z_b - unten)
            ),
        );
        aushub += im_sand;
    }
    let graben = ring * (unten - z_f);
    if graben > 0.0 {
        menge(
            TRENCH,
            graben,
            format!(
                "Ring {} m² × {} unter der Sohle der Auskofferung",
                zahl(ring, 1e6),
                breite(ka - z_f)
            ),
        );
        aushub += graben;
    }
    planum(innen, g, boden.kies, &mut menge);
    // über der Sandplatte bis OK gewachsener Boden, außerhalb der Platte
    let ueber = z_n - z_b;
    let verfuellen = if ueber > 0.0 {
        pr(sp.w, sp.k, h) - pr(sp.w, sp.k, z_b - ka) - fl * ueber
    } else {
        0.0
    };
    if verfuellen > 0.0 {
        menge(
            BACKFILL,
            verfuellen,
            format!(
                "über der Sandplatte: Auskofferung {} m³ bis {bis} {} m³ − Platte {} m² × {}",
                zahl(grube, 1e9),
                zahl(pr(sp.w, sp.k, z_b - ka), 1e9),
                zahl(fl, 1e6),
                breite(ueber)
            ),
        );
    }
    let verfuellen = verfuellen.max(0.0);
    let abfuhr = aushub - verfuellen;
    if abfuhr > 0.0 {
        menge(
            DISPOSAL,
            abfuhr,
            format!(
                "Aushub {} m³ − Verfüllung {} m³",
                zahl(aushub, 1e9),
                zahl(verfuellen, 1e9)
            ),
        );
    }
    if boeschung {
        // Schräge Fläche: Umfang auf halber Höhe × Böschungslänge
        let x_m = sp.w + sp.k * h / 2.0;
        let lang = h / sp.tan.atan().sin();
        let flaeche = (u + 2.0 * t * x_m) * lang;
        menge(
            SLOPE,
            flaeche,
            format!(
                "Umfang {} × Böschung {}",
                breite(u + 2.0 * t * x_m),
                breite(lang)
            ),
        );
    }
    ErdZone {
        baufeld: x_ob,
        sohle: if grube > 0.0 {
            sohle(&b.outline, sp.w)
        } else {
            Vec::new()
        },
        b: z_b,
        keil: boeschung,
    }
}

/// Sohle der Auskofferung: der Plattenumriss um `w` versetzt.
fn sohle(outline: &[Vec3], w: f64) -> Vec<Vec3> {
    if w > 0.0 {
        sk_math::flaeche::versatz(outline, &vec![-w; outline.len()]).unwrap_or_default()
    } else {
        outline.to_vec()
    }
}

/// Raum über einem Umriss mit Fläche `fl`, Umfang `u` und Eckzahl `t`, um
/// `x0` nach außen versetzt und je mm Höhe um `k` breiter, über die Höhe
/// `h` (mm³): ∫ A(x0 + k·y) dy mit A(x) = fl + u·x + t·x²; 0 für `h` ≤ 0.
fn prisma(fl: f64, u: f64, t: f64, x0: f64, k: f64, h: f64) -> f64 {
    if h <= 0.0 {
        return 0.0;
    }
    fl * h
        + u * (x0 * h + k * h * h / 2.0)
        + t * (x0 * x0 * h + x0 * k * h * h + k * k * h * h * h / 3.0)
}

/// Oberboden über das Baufeld, `x_ob` um die Platte (waagerechte Fläche ×
/// Dicke `o`, auch am Hang: VOB-Prüfung G2 Frage 4).
fn oberboden(b: &ErdBasis, x_ob: f64, o: f64, menge: &mut dyn FnMut(&'static str, f64, String)) {
    let (fl, u, t) = (b.slab_area, b.slab_perimeter, eckzahl(&b.outline));
    let ob_flaeche = fl + u * x_ob + t * x_ob * x_ob;
    if o > 0.0 {
        menge(
            TOPSOIL,
            ob_flaeche * o,
            format!(
                "Platte {} m² + Abtrag um die Platte {} m: {} m² × {} m",
                zahl(fl, 1e6),
                zahl(x_ob, 1e3),
                zahl(ob_flaeche, 1e6),
                zahl(o, 1e3)
            ),
        );
    }
}

/// Planum und kapillarbrechende Schicht innerhalb der Schürze (`innen`
/// mm², Schicht `g` mm). Ist sie nicht eingebaut, gibt es keine
/// Kiesposition, auch keine mit 0; das Planum sagt es (VOB B-BA-5).
fn planum(innen: f64, g: f64, kies: bool, menge: &mut dyn FnMut(&'static str, f64, String)) {
    if innen > 0.0 {
        let ohne = if kies { "" } else { NICHT_EINGEBAUT };
        menge(
            SUBGRADE,
            innen,
            format!("innerhalb der Frostschürze {} m²{ohne}", zahl(innen, 1e6)),
        );
    }
    if g > 0.0 && innen > 0.0 {
        menge(
            GRAVEL,
            innen * g,
            format!("{} m² × {} m", zahl(innen, 1e6), zahl(g, 1e3)),
        );
    }
}

/// Erdmengen der Gründung als Automatikmengen am Bauteil der Sohlplatte,
/// dazu Baufeld und Baugrube.
pub(crate) fn auto_mengen(
    vorlage: &AutoMenge,
    b: &ErdBasis,
    boden: &Boden,
) -> (Vec<AutoMenge>, ErdZone) {
    let mut out = Vec::new();
    let zone = erd_mengen(b, boden, |key, value, formula| {
        let unit = SCHLUESSEL.iter().find(|s| s.0 == key).map_or("m3", |s| s.1);
        out.push(AutoMenge {
            key,
            unit,
            value,
            kg: Some(KG),
            formula,
            ..vorlage.clone()
        });
    });
    (out, zone)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rechteck(w: f64, d: f64) -> Vec<Vec3> {
        vec![
            sk_math::vec3(0.0, 0.0, 0.0),
            sk_math::vec3(w, 0.0, 0.0),
            sk_math::vec3(w, d, 0.0),
            sk_math::vec3(0.0, d, 0.0),
        ]
    }

    /// 10 × 8 m, Platte 200, Schürze 350 breit und 600 tief, Gelände = OK
    /// Platte.
    fn basis(terrain_z: f64) -> ErdBasis {
        let (w, d, s) = (10_000.0, 8_000.0, 350.0);
        let ring = w * d - (w - 2.0 * s) * (d - 2.0 * s);
        ErdBasis {
            terrain_z,
            slab_bottom_z: -200.0,
            insulation_bottom_z: -200.0,
            footing_bottom_z: -800.0,
            outline: rechteck(w, d),
            slab_area: w * d,
            slab_perimeter: 2.0 * (w + d),
            footing_area: ring,
            aufmass: None,
            netz: None,
            andecken: 200.0,
        }
    }

    fn mengen(b: &ErdBasis, boden: &Boden) -> Vec<(&'static str, f64)> {
        let mut v: Vec<(&'static str, f64)> = Vec::new();
        erd_mengen(b, boden, |k, x, _| match v.iter_mut().find(|e| e.0 == k) {
            Some(e) => e.1 += x,
            None => v.push((k, x)),
        });
        v
    }

    fn wert(v: &[(&str, f64)], k: &str) -> f64 {
        v.iter().find(|e| e.0 == k).map_or(0.0, |e| e.1)
    }

    #[test]
    fn eckzahl_rechteck_und_l() {
        assert!((eckzahl(&rechteck(5.0, 3.0)) - 4.0).abs() < 1e-9);
        // L-Form: 5 konvexe, 1 einspringende Ecke
        let l = [
            (0.0, 0.0),
            (6.0, 0.0),
            (6.0, 2.0),
            (2.0, 2.0),
            (2.0, 5.0),
            (0.0, 5.0),
        ]
        .map(|(x, y)| sk_math::vec3(x, y, 0.0));
        assert!((eckzahl(&l) - 4.0).abs() < 1e-9);
    }

    /// Gelände = OK Platte: Auskofferung 0,80 m ab OK Gelände (Oberboden
    /// 0,30 + Boden 0,50) über Platte und Überstand 0,50 m; Füllsand bis UK
    /// Schicht bzw. UK Platte, über dem gewachsenen Boden mit Böschung; der
    /// Graben liegt ganz im Füllsand.
    #[test]
    fn gelaende_auf_ok_platte() {
        let v = mengen(&basis(0.0), &Boden::default());
        let m3 = |k| wert(&v, k) / 1e9;
        // Baufeld 1,50 m um die Platte
        let oben = 80.0 + 36.0 * 1.5 + 4.0 * 2.25; // m²
        assert!((m3(TOPSOIL) - oben * 0.3).abs() < 1e-9);
        // Sohle −0,80, gewachsen −0,30: 0,50 m über Platte + 0,50 m
        let sohle = 80.0 + 36.0 * 0.5 + 4.0 * 0.25;
        assert!((m3(EXCAVATION) - sohle * 0.5).abs() < 1e-9);
        let innen = 9.3 * 7.3;
        let ring = 80.0 - innen;
        // innen bis UK Schicht −0,35, unter der Schürze bis UK Platte −0,20,
        // um die Platte bis N, darüber 0,10 m mit Böschung 45°
        let auf = 36.0 * (0.5 * 0.1 + 0.01 / 2.0) + 4.0 * (0.025 + 0.005 + 0.001 / 3.0);
        let sand = innen * 0.45 + ring * 0.6 + (sohle - 80.0) * 0.5 + auf;
        assert!((m3(SAND) - sand).abs() < 1e-9, "{}", m3(SAND));
        assert!((m3(TRENCH_SAND) - ring * 0.6).abs() < 1e-9);
        assert_eq!(m3(TRENCH), 0.0);
        assert!((m3(GRAVEL) - innen * 0.15).abs() < 1e-9);
        assert_eq!(m3(FILL), 0.0);
        assert_eq!(m3(BACKFILL), 0.0);
        assert!((wert(&v, SUBGRADE) / 1e6 - innen).abs() < 1e-9);
        assert_eq!(m3(SLOPE), 0.0);
        assert!((m3(DISPOSAL) - (sohle * 0.5 + ring * 0.6)).abs() < 1e-9);
    }

    /// Platte 0,50 m unter Gelände: Die Auskofferung reicht bis UK Schicht,
    /// tiefer als 0,80 m; über der Sandplatte Verfüllung, der Graben unter
    /// der Sohle im gewachsenen Boden.
    #[test]
    fn platte_tiefer() {
        let v = mengen(&basis(500.0), &Boden::default());
        let m3 = |k| wert(&v, k) / 1e9;
        // gewachsen +0,20, Sohle −0,35: 0,55 m
        let sohle = 80.0 + 36.0 * 0.5 + 4.0 * 0.25;
        let ring = 80.0 - 9.3 * 7.3;
        assert!((m3(EXCAVATION) - sohle * 0.55).abs() < 1e-9);
        assert!((m3(SAND) - (ring + sohle - 80.0) * 0.15).abs() < 1e-9);
        assert!((m3(TRENCH_SAND) - ring * 0.15).abs() < 1e-9);
        assert!((m3(TRENCH) - ring * 0.45).abs() < 1e-9);
        let verf = sohle * 0.4 - 80.0 * 0.4;
        assert!((m3(BACKFILL) - verf).abs() < 1e-9);
        let aushub = sohle * 0.55 + ring * 0.6;
        assert!((m3(DISPOSAL) - (aushub - verf)).abs() < 1e-9);
    }

    /// Platte 0,40 m über Gelände: ausgekoffert 0,80 m ab OK Gelände, der
    /// Füllsand reicht um die Platte bis unter den angedeckten Oberboden
    /// (0,10 m über dem gewachsenen Boden) und läuft dort in der Böschung
    /// aus; keine Auffüllung Schotter mehr.
    #[test]
    fn platte_hoeher() {
        let v = mengen(&basis(-400.0), &Boden::default());
        let m3 = |k| wert(&v, k) / 1e9;
        let sohle = 80.0 + 36.0 * 0.5 + 4.0 * 0.25;
        let innen = 9.3 * 7.3;
        let ring = 80.0 - innen;
        // Sohle −1,20, gewachsen −0,70, UK Schicht −0,35, UK Platte −0,20
        assert!((m3(EXCAVATION) - sohle * 0.5).abs() < 1e-9);
        // voll bis 0,90 m vor der Platte, dann 0,10 m Böschung
        let auf = 36.0 * (0.9 * 0.1 + 0.01 / 2.0) + 4.0 * (0.81 * 0.1 + 0.9 * 0.01 + 0.001 / 3.0);
        let sand = innen * 0.85 + ring + (sohle - 80.0) * 0.5 + auf;
        assert!((m3(SAND) - sand).abs() < 1e-9, "{}", m3(SAND));
        assert_eq!(m3(FILL), 0.0);
        assert!((m3(TRENCH_SAND) - ring * 0.6).abs() < 1e-9);
        assert_eq!(m3(TRENCH), 0.0);
        // das Baufeld reicht bis zum Fuß der Böschung, 1,00 m
        let oben = 80.0 + 36.0 * 1.5 + 4.0 * 2.25;
        assert!((m3(TOPSOIL) - oben * 0.3).abs() < 1e-9);
    }

    /// Über 1,25 m Tiefe ab OK Gelände: Die Wand der Auskofferung wird
    /// geböscht, die Grube oben breiter.
    #[test]
    fn boeschung_ab_125() {
        let b = basis(1_500.0);
        let v = mengen(&b, &Boden::default());
        // gewachsen +1,20, Sohle −0,35: h = 1,55 m, k = 1 (45°)
        let (h, w, k) = (1.55, 0.5, 1.0);
        let grube = h * 80.0
            + 36.0 * (w * h + k * h * h / 2.0)
            + 4.0 * (w * w * h + w * k * h * h + k * k * h * h * h / 3.0);
        assert!((wert(&v, EXCAVATION) / 1e9 - grube).abs() < 1e-6);
        assert!(wert(&v, SLOPE) > 0.0);
        // Die Auskofferung reicht oben 2,05 m über die Platte, weiter als
        // das Baufeld: Oberboden bis zu ihrem Rand
        let x = w + k * h;
        let oben = 80.0 + 36.0 * x + 4.0 * x * x;
        assert!((wert(&v, TOPSOIL) / 1e9 - oben * 0.3).abs() < 1e-6);
        let senkrecht = mengen(&basis(800.0), &Boden::default());
        assert_eq!(wert(&senkrecht, SLOPE), 0.0);
    }

    /// VOB B-BA-4: Je Haus geht die Bilanz auf, auf 1 ‰: Aushub
    /// (Auskofferung und beide Gräben) = Füllsand + Kies + Verfüllung +
    /// Bauwerk unter dem gewachsenen Boden (innerhalb der Schürze ab UK
    /// Dämmung, die Schürze ab ihrer UK); die Abfuhr ist Aushub minus
    /// Verfüllung. Die Platte liegt dabei im gewachsenen Boden.
    #[test]
    fn mengenbilanz_je_haus() {
        let mut n = 0;
        for t in [100.0, 500.0, 1_200.0, 1_500.0, 3_000.0] {
            for daemmung in [0.0, 120.0] {
                for kies in [true, false] {
                    for arbeitsraum in [500.0, 800.0] {
                        let mut b = basis(t);
                        b.insulation_bottom_z = b.slab_bottom_z - daemmung;
                        let boden = Boden {
                            kies,
                            arbeitsraum,
                            ..Boden::default()
                        };
                        let v = mengen(&b, &boden);
                        let m = |k| wert(&v, k);
                        let z_n = t - boden.oberboden;
                        let ring = b.footing_area;
                        let innen = b.slab_area - ring;
                        let bauwerk = innen * (z_n - b.insulation_bottom_z)
                            + ring * (z_n - b.footing_bottom_z);
                        let aushub = m(EXCAVATION) + m(TRENCH) + m(TRENCH_SAND);
                        let rein = m(SAND) + m(GRAVEL) + m(BACKFILL) + bauwerk;
                        assert!(
                            (aushub - rein).abs() <= 1e-3 * aushub,
                            "{t} {daemmung} {kies} {arbeitsraum}: {aushub} ≠ {rein}"
                        );
                        let abfuhr = aushub - m(BACKFILL);
                        assert!((m(DISPOSAL) - abfuhr).abs() <= 1e-6 * abfuhr);
                        assert_eq!(m(FILL), 0.0);
                        n += 1;
                    }
                }
            }
        }
        assert_eq!(n, 40);
    }

    /// Zwei Häuser 8 × 10 m nebeneinander (Sonde #280, H-280-geo-a): Der
    /// Oberboden zählt die Vereinigung der Baufelder (1,50 m um die
    /// Platte), mit und ohne Netz. Jedes Haus gibt die Hälfte des
    /// Streifens ab und nennt sie im Rechenweg; mit dem Netz andecken die
    /// Baufelder ebenso nur einmal.
    #[test]
    fn oberboden_zweier_haeuser_einmal() {
        use crate::Model;
        use sk_math::vec3;
        fn modell(haeuser: &[f64], netz: bool) -> Model {
            let mut m = Model::with_seed(5);
            for &x in haeuser {
                let b = m.add_building(1);
                let pts = [
                    vec3(x, 0.0, 0.0),
                    vec3(x + 8000.0, 0.0, 0.0),
                    vec3(x + 8000.0, 10000.0, 0.0),
                    vec3(x, 10000.0, 0.0),
                ];
                m.build_from_polygon(b, &pts).unwrap();
                m.begin("G");
                m.set_terrain_offset_of(b, 500.0);
                m.commit();
            }
            if netz {
                m.begin("F");
                assert!(m.seed_flat_terrain(&[]));
                m.commit();
            }
            m
        }
        let ob = |m: &Model| -> Vec<(f64, String)> {
            crate::qto::schedule(m)
                .auto
                .iter()
                .filter(|a| a.key == TOPSOIL)
                .map(|a| (a.value, a.formula.clone()))
                .collect()
        };
        // Plattenumriss als Rechteck [x0, y0, x1, y1]
        let rahmen = |g: &crate::GroundBasis| {
            g.outline
                .iter()
                .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |r, p| {
                    [r[0].min(p.x), r[1].min(p.y), r[2].max(p.x), r[3].max(p.y)]
                })
        };
        let (o, x) = (Boden::default().oberboden, Boden::default().rand);
        for netz in [false, true] {
            let ein = ob(&modell(&[0.0], netz));
            assert_eq!(ein.len(), 1);
            for spalt in [0.0, 315.0, 630.0, 1500.0, 2500.0, 4000.0] {
                let m = modell(&[0.0, 8000.0 + spalt], netz);
                let zwei = ob(&m);
                assert_eq!(zwei.len(), 2);
                let mut r: Vec<[f64; 4]> = m.ground_bases().iter().map(rahmen).collect();
                r.sort_by(|a, b| a[0].total_cmp(&b[0]));
                let d = r[1][0] - r[0][2];
                // ineinander: nichts gekürzt, dort steht der Hinweis
                let streifen = if m.haeuser_ineinander().is_empty() {
                    (2.0 * x - d).max(0.0) * (r[0][3] - r[0][1] + 2.0 * x) * o
                } else {
                    assert!(d <= 0.0, "{spalt}");
                    0.0
                };
                let summe = zwei[0].0 + zwei[1].0;
                assert!(
                    (summe - (2.0 * ein[0].0 - streifen)).abs() < 1e-6 * ein[0].0,
                    "{netz} {spalt}: {summe} {}",
                    2.0 * ein[0].0 - streifen
                );
                for (v, f) in &zwei {
                    assert!((v - (ein[0].0 - streifen / 2.0)).abs() < 1e-6 * ein[0].0);
                    assert_eq!(f.contains("− Überdeckung mit"), streifen > 0.0, "{f}");
                }
                if streifen > 0.0 {
                    let text = format!(
                        "(Abstand {} m): {} m³",
                        zahl(d, 1e3),
                        zahl(streifen / 2.0, 1e9)
                    );
                    assert!(zwei[0].1.contains(&text), "{} / {text}", zwei[0].1);
                    // V-285a: am Schluss die Restmenge
                    let rest = format!("; = {} m³", zahl(zwei[0].0, 1e9));
                    assert!(zwei[0].1.ends_with(&rest), "{} / {rest}", zwei[0].1);
                }
            }
        }
    }

    /// Der Rechenweg der Auskofferung nennt OK Gelände und den Oberboden
    /// (VOB B-BA-1), Füllsand und Gräben nennen, wo sie liegen.
    #[test]
    fn rechenweg_der_sandplatte() {
        let mut v: Vec<(&str, String)> = Vec::new();
        erd_mengen(&basis(0.0), &Boden::default(), |k, _, f| v.push((k, f)));
        let text = |k: &str| {
            v.iter()
                .filter(|x| x.0 == k)
                .map(|x| x.1.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            text(EXCAVATION),
            [
                "Auskofferung 0,80 m ab OK Gelände = Oberboden 0,30 m (gesondert) + Boden 0,50 m: \
                 Platte 80,00 m² + Überstand 0,50 m"
            ]
        );
        assert_eq!(
            text(TRENCH_SAND),
            ["Ring 12,11 m² × 0,60 m ab OK Sandplatte"]
        );
        assert_eq!(
            text(SAND)[0],
            "innerhalb der Schürze 67,89 m² × 0,45 m bis UK Schicht"
        );
    }

    /// Bereiche der Bodenkennwerte; die Vorgabe liegt darin.
    #[test]
    fn boden_bereiche() {
        let d = Boden::default();
        assert_eq!(Boden::aus_werten(d.werte()), Some(d));
        let mut w = d.werte();
        w[4] = 20.0;
        assert_eq!(Boden::aus_werten(w), None);
        w[4] = 60.0;
        w[0] = -1.0;
        assert_eq!(Boden::aus_werten(w), None);
        w[0] = f64::NAN;
        assert_eq!(Boden::aus_werten(w), None);
    }
}

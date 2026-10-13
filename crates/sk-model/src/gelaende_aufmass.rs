//! Höhenaufmaß im Modell (Gelände G1, plan-gelaendemodul.md §4.2, §4.3, §6).
//! Gespeichert werden nur die Messpunkte wie gelesen, die Grenzen aus
//! Grenzpunkten und die Codes, die von der Vorgabe abweichen; Netz, Höhen
//! und Höhenlinien rechnet `sk-gelaende` daraus und merkt sich das Netz am
//! Aufmaß.
//!
//! Die Lage hat genau eine Quelle: `[location]` (UTM-Nullpunkt und
//! Nordrichtung), bei örtlichem System oder Gauß-Krüger der Nullpunkt am
//! Aufmaß. Verschieben und Drehen ändern nur diese Lage; das Haus bleibt,
//! wo es gezeichnet ist (§4.8). ±0,00 bleibt OK Sohlplatte; ihre Höhe über
//! NHN folgt der Höhenlage (§4.3).

use super::*;
use sk_gelaende::abfrage::Abfrage;
use sk_gelaende::codes::{Bedeutung, Codetabelle};
use sk_gelaende::lesen::Punkt;
use sk_gelaende::linien::Hoehenlinie;
use sk_gelaende::{netz, Netz, Rand};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Ein Messpunkt wie gelesen: Rechts- und Hochwert und Höhe in m.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPoint {
    pub nr: String,
    pub e: f64,
    pub n: f64,
    pub h: f64,
    pub code: String,
    /// Vom Nutzer aus dem Gelände genommen.
    pub off: bool,
}

/// Lagesystem der Messpunkte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crs {
    /// ETRS89/UTM, Zone 32 oder 33.
    Utm(u8),
    /// Gauß-Krüger, Streifen 2 bis 5 (gilt als örtliches System, §11 E12).
    GaussKrueger(u8),
    Local,
}

impl Crs {
    pub fn key(self) -> String {
        match self {
            Crs::Utm(z) => format!("utm{z}"),
            Crs::GaussKrueger(s) => format!("gk{s}"),
            Crs::Local => "local".into(),
        }
    }

    pub fn from_key(k: &str) -> Option<Crs> {
        if k == "local" {
            return Some(Crs::Local);
        }
        let zahl = |p: &str| k.strip_prefix(p).and_then(|z| z.parse::<u8>().ok());
        match (zahl("utm"), zahl("gk")) {
            (Some(z), _) if location::UTM_ZONEN.contains(&z) => Some(Crs::Utm(z)),
            (_, Some(s)) if (2..=5).contains(&s) => Some(Crs::GaussKrueger(s)),
            _ => None,
        }
    }
}

/// Höhenbezug; nur vermerkt, nicht umgerechnet (§11 E13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeightSystem {
    #[default]
    Dhhn2016,
    Dhhn92,
    Nn,
    Hn,
    Ellipsoid,
}

impl HeightSystem {
    pub const ALL: [HeightSystem; 5] = [
        HeightSystem::Dhhn2016,
        HeightSystem::Dhhn92,
        HeightSystem::Nn,
        HeightSystem::Hn,
        HeightSystem::Ellipsoid,
    ];

    pub fn key(self) -> &'static str {
        match self {
            HeightSystem::Dhhn2016 => "DHHN2016",
            HeightSystem::Dhhn92 => "DHHN92",
            HeightSystem::Nn => "NN",
            HeightSystem::Hn => "HN",
            HeightSystem::Ellipsoid => "ell",
        }
    }
}

/// Rand des Netzes (E9): beschnitten (Vorgabe) oder wie gemessen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Trim {
    #[default]
    Auto,
    Hull,
}

/// Zuschnitt der Darstellung (§4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    All,
    Fade,
    Parcel,
}

impl Show {
    pub fn key(self) -> &'static str {
        match self {
            Show::All => "all",
            Show::Fade => "fade",
            Show::Parcel => "parcel",
        }
    }
}

/// Art der Höhenlage (§4.3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeightRef {
    /// ±0,00 = höchster Geländepunkt am Plattenrand + Versatz des Gebäudes.
    #[default]
    High,
    /// ±0,00 = `[location] nhn`, fest.
    Abs,
}

/// Lage eines Aufmaßes ohne UTM: Landeswert des Modellnullpunkts (m) und
/// Drehung der Modell-x-Achse gegen Ost (Grad, gegen den Uhrzeigersinn).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalFrame {
    pub e0: f64,
    pub n0: f64,
    pub rot: f64,
}

/// Umrechnung zwischen Modell (mm) und Aufmaß (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurveyFrame {
    pub e0: f64,
    pub n0: f64,
    /// Drehung der Modell-x-Achse gegen Ost im Gitter (rad, gegen den
    /// Uhrzeigersinn): Nordrichtung plus Meridiankonvergenz wie im IFC.
    pub g: f64,
}

impl SurveyFrame {
    /// Landeswert → Modell (mm).
    pub fn to_model(&self, e: f64, n: f64) -> [f64; 2] {
        let (s, c) = self.g.sin_cos();
        let (de, dn) = (e - self.e0, n - self.n0);
        [(de * c + dn * s) * 1000.0, (-de * s + dn * c) * 1000.0]
    }

    /// Modell (mm) → Landeswert (m).
    pub fn to_survey(&self, x: f64, y: f64) -> (f64, f64) {
        let (s, c) = self.g.sin_cos();
        let (x, y) = (x / 1000.0, y / 1000.0);
        (self.e0 + x * c - y * s, self.n0 + x * s + y * c)
    }
}

/// Das Netz eines Aufmaßes, in m relativ zu `bezug`.
#[derive(Debug)]
pub struct TerrainMesh {
    /// Landeswert (Ost, Nord) des Netznullpunkts.
    pub bezug: (f64, f64),
    pub netz: Netz,
    pub abfrage: Abfrage,
    /// Aufmaßpunkt je Netzpunkt.
    pub index: Vec<usize>,
    /// Was beim Prüfen und Vernetzen auffiel.
    pub hinweise: Vec<String>,
    /// Ausreißer (Aufmaßpunkte), zum Abwählen angeboten.
    pub ausreisser: Vec<usize>,
    /// Gerechnete Höhenlinien und Abstände je Stufe (Review #123 W45 H1):
    /// Sie hängen nur am Netz, eine Wand- oder Lageänderung rechnet sie
    /// nicht neu.
    linien: Mutex<Vec<(u64, Arc<Vec<Hoehenlinie>>)>>,
    stufen: Mutex<Vec<(u64, Option<f64>)>>,
}

/// So viele Stufen merkt ein Netz; die älteste geht zuerst.
const GEMERKTE_STUFEN: usize = 4;

/// Wert zu `stufe` aus `liste`, sonst gerechnet und dazugelegt.
fn gemerkt<T: Clone>(liste: &Mutex<Vec<(u64, T)>>, stufe: f64, rechnen: impl FnOnce() -> T) -> T {
    let k = stufe.to_bits();
    if let Some((_, v)) = liste
        .lock()
        .ok()
        .and_then(|l| l.iter().find(|x| x.0 == k).cloned())
    {
        return v;
    }
    let v = rechnen();
    if let Ok(mut l) = liste.lock() {
        if l.len() >= GEMERKTE_STUFEN {
            l.remove(0);
        }
        l.push((k, v.clone()));
    }
    v
}

impl TerrainMesh {
    /// Höhenlinien im Abstand `stufe` (m, relativ zu `bezug`), je Stufe
    /// einmal gerechnet.
    pub fn hoehenlinien(&self, stufe: f64) -> Arc<Vec<Hoehenlinie>> {
        gemerkt(&self.linien, stufe, || {
            Arc::new(sk_gelaende::linien::hoehenlinien(&self.netz, stufe))
        })
    }
}

/// Das Netz, einmal gerechnet, mit dem Schlüssel der Punkte, Codes und
/// des Rands, aus denen es entstand ([`Terrain::netz_schluessel`]); gilt
/// bei jedem Vergleich als gleich.
#[derive(Default)]
struct Merker(OnceLock<(u64, Arc<TerrainMesh>)>);

impl Clone for Merker {
    fn clone(&self) -> Merker {
        let m = Merker::default();
        if let Some(n) = self.0.get() {
            let _ = m.0.set(n.clone());
        }
        m
    }
}

impl std::fmt::Debug for Merker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.get().is_some() { "Netz" } else { "-" })
    }
}

/// Was aus dem Aufmaß für das ganze Modell folgt, einmal je Modellstand
/// gerechnet (Review #123 W45 B1): Vorher rechnete jede Geländeabfrage
/// alle Plattenumrisse neu, und Gänge über alle Gründungen oder Platten
/// wuchsen mit dem Quadrat der Häuser. Versatz und feste Höhe von ±0,00
/// stehen nicht darin, sie kommen beim Lesen dazu ([`Model::terrain_nhn`]):
/// So bleibt die Lage gemerkt, während OK Gelände gezogen wird
/// (W45-Nachprüfung B1).
#[derive(Debug, Default)]
pub(crate) struct Lage {
    /// Aufmaß und Lage waren da: Nur dann gibt es ±0,00 in NHN.
    aufmass: bool,
    /// Das Gebäude, das die Höhenlage bestimmt ([`Model::level_building`]).
    haus: Option<BuildingId>,
    /// Höchster Geländepunkt am Plattenrand des bestimmenden Gebäudes
    /// (m ü. NHN), ohne seinen Versatz.
    hoch: Option<f64>,
    /// Mittlere Punkthöhe des Aufmaßes (m ü. NHN).
    mittel: Option<f64>,
    /// Tiefster Geländepunkt am Plattenrand je Gründung (m ü. NHN).
    tief: HashMap<StoreyId, f64>,
}

/// Platz für die [`Lage`] im Modell mit dem Stand, zu dem sie gehört.
/// Der Stand steigt mit jeder Änderung außer an Geschosshöhen, an Höhe und
/// Einbindung einer Gründung, am Versatz und an der festen Höhe von ±0,00:
/// Davon hängt die Lage nicht ab, und beim Ziehen im Geschossmanager bleibt
/// sie so gemerkt ([`Model::touch_height`]).
#[derive(Clone, Default)]
pub(crate) struct Lagemerker {
    stand: u64,
    lage: RefCell<Option<(u64, Arc<Lage>)>>,
}

impl std::fmt::Debug for Lagemerker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Lagemerker")
    }
}

impl Lagemerker {
    /// Vergessen, wenn der Stand zurückgesetzt wird
    /// ([`Model::restore_revision`]): Dieselbe Nummer kann dann für einen
    /// anderen Stand stehen.
    pub(crate) fn vergessen(&self) {
        self.lage.borrow_mut().take();
    }

    /// Eine Änderung, von der die Lage abhängen kann.
    pub(crate) fn weiter(&mut self) {
        self.stand += 1;
    }
}

/// Ein eingelesenes Höhenaufmaß.
#[derive(Clone, Debug)]
pub struct Terrain {
    /// Dateiname und Lesedatum (JJJJ-MM-TT).
    pub src: String,
    pub read: String,
    pub crs: Crs,
    pub hsys: HeightSystem,
    pub trim: Trim,
    /// Zuschnitt; `None`: Grundstück, sobald eine Grenze da ist, sonst alles.
    pub show: Option<Show>,
    pub href: HeightRef,
    /// Lage ohne UTM (örtlich, Gauß-Krüger).
    pub local: Option<LocalFrame>,
    pub points: Vec<TerrainPoint>,
    /// Grenzen als Punktnummern im Umlauf, je Flurstück eine.
    pub boundaries: Vec<Vec<String>>,
    /// Codes, deren Bedeutung von der Vorgabe abweicht.
    pub codes: Vec<(String, Bedeutung)>,
    /// Zeilen der Datei, die nicht zählen, roh (Regel 72).
    pub raw: Vec<String>,
    /// Die `[terrain]`-Zeile der Datei, roh, wenn ein Wert darin nicht galt
    /// (Regel 72, Review #123 W45 B5); neue Werte im Kopf ersetzen sie.
    pub head_raw: Option<String>,
    mesh: Merker,
}

impl PartialEq for Terrain {
    fn eq(&self, o: &Terrain) -> bool {
        (
            &self.src, &self.read, self.crs, self.hsys, self.trim, self.show, self.href, self.local,
        ) == (
            &o.src, &o.read, o.crs, o.hsys, o.trim, o.show, o.href, o.local,
        ) && (&self.points, &self.boundaries, &self.codes, &self.raw)
            == (&o.points, &o.boundaries, &o.codes, &o.raw)
            && self.head_raw == o.head_raw
    }
}

impl Terrain {
    /// Leeres Aufmaß im System `crs`.
    pub fn new(crs: Crs) -> Terrain {
        Terrain {
            src: String::new(),
            read: String::new(),
            crs,
            hsys: HeightSystem::default(),
            trim: Trim::default(),
            show: None,
            href: HeightRef::default(),
            local: None,
            points: Vec::new(),
            boundaries: Vec::new(),
            codes: Vec::new(),
            raw: Vec::new(),
            head_raw: None,
            mesh: Merker::default(),
        }
    }

    /// Codetabelle: Vorgabe mit den abweichenden Codes.
    pub fn code_table(&self) -> Codetabelle {
        let mut t = Codetabelle::default();
        for (c, b) in &self.codes {
            t.setze(c, *b);
        }
        t
    }

    /// Zuschnitt, wie er gilt.
    pub fn show(&self) -> Show {
        self.show.unwrap_or(if self.boundaries.is_empty() {
            Show::All
        } else {
            Show::Parcel
        })
    }

    /// Das Netz (einmal gerechnet, dann gemerkt).
    pub fn mesh(&self) -> Arc<TerrainMesh> {
        self.mesh
            .0
            .get_or_init(|| (self.netz_schluessel(), Arc::new(self.vernetzen())))
            .1
            .clone()
    }

    /// Prüfsumme über alles, woraus das Netz entsteht (Rand, Codes,
    /// Punkte mit Abwahl), wie [`Terrain::netz_gleich`] vergleicht.
    fn netz_schluessel(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.trim as u8).hash(&mut h);
        for (c, b) in &self.codes {
            (c, *b as u8).hash(&mut h);
        }
        for p in &self.points {
            (
                &p.nr,
                p.e.to_bits(),
                p.n.to_bits(),
                p.h.to_bits(),
                &p.code,
                p.off,
            )
                .hash(&mut h);
        }
        h.finish()
    }

    /// Das gemerkte Netz gehört zu diesen Punkten, Codes und diesem Rand.
    fn netz_passt(&self) -> bool {
        self.mesh
            .0
            .get()
            .is_some_and(|(k, _)| *k == self.netz_schluessel())
    }

    fn vernetzen(&self) -> TerrainMesh {
        let codes = self.code_table();
        let mut punkte: Vec<Punkt> = self
            .points
            .iter()
            .map(|p| Punkt {
                nr: p.nr.clone(),
                e: p.e,
                n: p.n,
                h: p.h,
                code: p.code.clone(),
                text: String::new(),
                zeile: 0,
            })
            .collect();
        let mut pr = sk_gelaende::pruefen::pruefen(&mut punkte, &codes);
        for (d, p) in pr.drin.iter_mut().zip(&self.points) {
            *d &= !p.off && p.e.is_finite() && p.n.is_finite() && p.h.is_finite();
        }
        let index: Vec<usize> = (0..punkte.len()).filter(|&i| pr.drin[i]).collect();
        // Bezug: Median der Lage, damit das Netz mit kleinen Zahlen rechnet
        let median = |f: fn(&Punkt) -> f64| {
            let mut v: Vec<f64> = index.iter().map(|&i| f(&punkte[i])).collect();
            v.sort_by(f64::total_cmp);
            v.get(v.len() / 2).copied().unwrap_or(0.0)
        };
        let bezug = (median(|p| p.e).round(), median(|p| p.n).round());
        let lage: Vec<[f64; 3]> = index
            .iter()
            .map(|&i| [punkte[i].e - bezug.0, punkte[i].n - bezug.1, punkte[i].h])
            .collect();
        let mut nach = vec![usize::MAX; punkte.len()];
        for (k, &i) in index.iter().enumerate() {
            nach[i] = k;
        }
        let bruch: Vec<[usize; 2]> = codes
            .bruchkanten(&punkte, &pr.drin)
            .into_iter()
            .map(|[a, b]| [nach[a], nach[b]])
            .collect();
        let rand = match self.trim {
            Trim::Auto => Rand::Beschnitten,
            Trim::Hull => Rand::Huelle,
        };
        let netz = netz(&lage, &bruch, rand);
        let mut hinweise = pr.hinweise;
        hinweise.extend(netz.hinweise.iter().cloned());
        TerrainMesh {
            bezug,
            abfrage: Abfrage::neu(&netz),
            netz,
            index,
            hinweise,
            ausreisser: pr.ausreisser,
            linien: Mutex::default(),
            stufen: Mutex::default(),
        }
    }

    /// Höhe (m ü. NHN) am Landeswert, aus dem Netz; ohne Netz `None`.
    pub fn height(&self, e: f64, n: f64) -> Option<f64> {
        let m = self.mesh();
        if m.netz.dreiecke.is_empty() {
            return None;
        }
        m.abfrage.hoehe(e - m.bezug.0, n - m.bezug.1)
    }

    /// Höchste Höhe entlang eines geschlossenen Umrisses (Landeswerte):
    /// an jeder Ecke und jedem Schnitt mit einer Netzkante.
    pub fn max_along(&self, ring: &[(f64, f64)]) -> Option<f64> {
        self.along(ring).into_iter().reduce(f64::max)
    }

    /// Tiefste Höhe entlang eines geschlossenen Umrisses (Landeswerte).
    pub fn min_along(&self, ring: &[(f64, f64)]) -> Option<f64> {
        self.along(ring).into_iter().reduce(f64::min)
    }

    /// Höhen entlang eines Umrisses: an jeder Ecke und jedem Schnitt mit
    /// einer Netzkante, dazwischen verläuft das Gelände geradlinig.
    fn along(&self, ring: &[(f64, f64)]) -> Vec<f64> {
        let m = self.mesh();
        if m.netz.dreiecke.is_empty() {
            return Vec::new();
        }
        let (e0, n0) = m.bezug;
        (0..ring.len())
            .flat_map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                m.abfrage.profil([a.0 - e0, a.1 - n0], [b.0 - e0, b.1 - n0])
            })
            .map(|w| w[1])
            .collect()
    }

    /// Flächenmittel der Höhe über einem Polygon (Landeswerte), exakt über
    /// die Dreiecke; liegt es ganz außerhalb des Netzes, die Höhe in seiner
    /// Mitte (nächster Randpunkt).
    pub fn mean_over(&self, ring: &[(f64, f64)]) -> Option<f64> {
        let m = self.mesh();
        if m.netz.dreiecke.is_empty() || ring.is_empty() {
            return None;
        }
        let (e0, n0) = m.bezug;
        let r: Vec<[f64; 2]> = ring.iter().map(|p| [p.0 - e0, p.1 - n0]).collect();
        sk_gelaende::volumen::mittel(&m.netz, &r).or_else(|| {
            let k = r.len() as f64;
            let (x, y) = r
                .iter()
                .fold((0.0, 0.0), |a, p| (a.0 + p[0] / k, a.1 + p[1] / k));
            m.abfrage.hoehe(x, y)
        })
    }

    /// Mittlere Höhe der Punkte im Gelände.
    pub fn mean_height(&self) -> Option<f64> {
        let m = self.mesh();
        let n = m.index.len();
        (n > 0).then(|| m.index.iter().map(|&i| self.points[i].h).sum::<f64>() / n as f64)
    }

    /// Dasselbe Netz: gleiche Punkte (mit „aus“), Codes und Rand.
    fn netz_gleich(&self, o: &Terrain) -> bool {
        self.trim == o.trim && self.codes == o.codes && self.points == o.points
    }

    /// Gleiche Werte in der `[terrain]`-Zeile.
    fn kopf_gleich(&self, o: &Terrain) -> bool {
        (
            &self.src, &self.read, self.crs, self.hsys, self.trim, self.show, self.href, self.local,
        ) == (
            &o.src, &o.read, o.crs, o.hsys, o.trim, o.show, o.href, o.local,
        )
    }
}

impl Model {
    /// Das Höhenaufmaß, wenn eines eingelesen ist.
    pub fn terrain(&self) -> Option<&Terrain> {
        self.project.terrain.as_deref()
    }

    /// Aufmaß setzen, ersetzen oder (`None`) entfernen, im offenen Schritt.
    /// Gleiches ändert nichts. Die Gründungen behalten ihre Einbindetiefe.
    pub fn set_terrain(&mut self, t: Option<Terrain>) -> bool {
        let vorher = self.foundation_terrain();
        let neu = self.put_survey(t);
        self.resettle_foundations(vorher);
        neu
    }

    /// Aufmaß schreiben, ohne die Gründungen nachzuführen. Das gemerkte
    /// Netz geht nur mit, wenn Punkte, Codes und Rand gleich bleiben: Eine
    /// Kopie mit abgewähltem Punkt brachte sonst das alte Netz mit (Review
    /// #123 W45 B2). Ein Netz, das das neue Aufmaß schon aus genau seinen
    /// Punkten gerechnet hat (beim Einlesen), bleibt (H5).
    fn put_survey(&mut self, t: Option<Terrain>) -> bool {
        if t.as_ref() == self.terrain() {
            return false;
        }
        let t = t.map(|mut t| {
            t.mesh = match self.terrain() {
                Some(alt) if alt.netz_gleich(&t) => alt.mesh.clone(),
                _ if t.netz_passt() => std::mem::take(&mut t.mesh),
                _ => Merker::default(),
            };
            // neue Werte im Kopf ersetzen die rohe `[terrain]`-Zeile
            if self.terrain().is_some_and(|alt| !alt.kopf_gleich(&t)) {
                t.head_raw = None;
            }
            Arc::new(t)
        });
        self.note_project();
        self.project.terrain = t;
        self.touch();
        true
    }

    /// Umrechnung Modell ↔ Aufmaß: UTM über `[location]` (Nullpunkt,
    /// Nordrichtung und Meridiankonvergenz, wie die IFC-Georeferenz), sonst
    /// die Lage am Aufmaß. `None`, solange die Lage fehlt.
    pub fn survey_frame(&self) -> Option<SurveyFrame> {
        let t = self.terrain()?;
        match t.crs {
            Crs::Utm(z) => {
                let l = self.location().normalized();
                let u = l.utm.filter(|u| u.zone == z)?;
                let (b, lon) = sk_math::utm::von_utm(u.e, u.n, u.zone);
                let g = (l.north_deg() + sk_math::utm::konvergenz(b, lon, u.zone)).to_radians();
                Some(SurveyFrame {
                    e0: u.e,
                    n0: u.n,
                    g,
                })
            }
            _ => t.local.map(|f| SurveyFrame {
                e0: f.e0,
                n0: f.n0,
                g: f.rot.to_radians(),
            }),
        }
    }

    /// Das Gebäude, das die Höhenlage bestimmt (§4.3): das mit der kleinsten
    /// Nummer unter denen mit Gründung.
    pub fn level_building(&self) -> Option<BuildingId> {
        self.lage().haus
    }

    /// Höhe von ±0,00 in m ü. NHN mit Aufmaß: fest (`abs`, `[location]
    /// nhn`), sonst höchster Geländepunkt am Plattenrand des bestimmenden
    /// Gebäudes plus sein Versatz; ohne Haus die mittlere Punkthöhe. `None`
    /// ohne Aufmaß oder Lage.
    pub fn terrain_nhn(&self) -> Option<f64> {
        self.terrain()?;
        self.nhn_aus(&self.lage())
    }

    /// ±0,00 in m ü. NHN aus der gemerkten [`Lage`], mit fester Höhe oder
    /// Versatz zum jetzigen Stand.
    fn nhn_aus(&self, lage: &Lage) -> Option<f64> {
        let t = self.terrain().filter(|_| lage.aufmass)?;
        match (t.href, self.location().nhn, lage.haus) {
            (HeightRef::Abs, Some(h), _) => Some(h),
            (_, _, None) => lage.mittel,
            (_, _, Some(b)) => {
                let versatz = self.terrain_offset_of(Some(b)) / 1000.0;
                lage.hoch.or(lage.mittel).map(|h| h + versatz)
            }
        }
    }

    /// Die gemerkte [`Lage`] verwerfen, etwa nach dem Lesen einer Datei:
    /// Dessen `load_*` erhöhen den Lagestand nicht, und eine Abfrage
    /// mittendrin hätte eine halbe Lage gemerkt (W45-Nachprüfung H-e).
    pub(crate) fn lage_vergessen(&self) {
        self.lagemerker.vergessen();
    }

    /// Die [`Lage`] zum jetzigen Modellstand, gerechnet beim ersten Bedarf.
    fn lage(&self) -> Arc<Lage> {
        let stand = self.lagemerker.stand;
        if let Some((s, l)) = self.lagemerker.lage.borrow().as_ref() {
            if *s == stand {
                return l.clone();
            }
        }
        let l = Arc::new(self.lesend(|| self.lage_rechnen()));
        *self.lagemerker.lage.borrow_mut() = Some((stand, l.clone()));
        l
    }

    /// Ein Gang über die Wandzüge: Plattenumriss, Gebäude und Gründung je
    /// Zug, mit Aufmaß dazu die Höhen entlang des Umrisses.
    fn lage_rechnen(&self) -> Lage {
        let aufmass = self.terrain().zip(self.survey_frame());
        let mut lage = Lage::default();
        // je Zug mit Gebäude: Gebäude und höchster Punkt am Umriss
        let mut hoch: Vec<(BuildingId, Option<f64>)> = Vec::new();
        for (id, r) in self.runs.iter() {
            let Some(ring) = self.plate_outline(id) else {
                continue;
            };
            let along = aufmass.map_or_else(Vec::new, |(t, f)| {
                let q: Vec<(f64, f64)> = ring.iter().map(|p| f.to_survey(p.x, p.y)).collect();
                t.along(&q)
            });
            let lo = along.iter().copied().reduce(f64::min);
            if let (Some(h), Some(gr)) = (lo, self.foundation_level_of(r.storey)) {
                let x = lage.tief.entry(gr).or_insert(h);
                *x = x.min(h);
            }
            if let Some(b) = self.building_of(r.storey) {
                hoch.push((b, along.into_iter().reduce(f64::max)));
            }
        }
        let nummer = |b: BuildingId| {
            self.building(b)
                .map(|x| x.number.clone())
                .unwrap_or_default()
        };
        lage.haus = hoch
            .iter()
            .map(|x| x.0)
            .reduce(|a, b| if nummer(b) < nummer(a) { b } else { a });
        let Some((t, _)) = aufmass else {
            return lage;
        };
        lage.aufmass = true;
        lage.mittel = t.mean_height();
        lage.hoch = lage.haus.and_then(|b| {
            hoch.iter()
                .filter(|x| x.0 == b)
                .filter_map(|x| x.1)
                .reduce(f64::max)
        });
        lage
    }

    /// OK Gelände aus dem Aufmaß an der Modellstelle (mm, relativ zu
    /// ±0,00); `None` ohne Aufmaß, Lage oder Netz.
    pub fn terrain_z_survey(&self, x: f64, y: f64) -> Option<f64> {
        self.terrain_z_survey_with(self.terrain_nhn()?, x, y)
    }

    /// Wie [`Model::terrain_z_survey`] mit schon bekannter Höhe von ±0,00
    /// (für viele Abfragen hintereinander).
    pub fn terrain_z_survey_with(&self, nhn: f64, x: f64, y: f64) -> Option<f64> {
        let t = self.terrain()?;
        let (e, n) = self.survey_frame()?.to_survey(x, y);
        Some((t.height(e, n)? - nhn) * 1000.0)
    }

    /// Mit Aufmaß: tiefster Geländepunkt am Plattenrand der Gründung `gr`
    /// (mm, relativ zu ±0,00); ab hier zählt ihre Einbindetiefe (§5 G1).
    /// `None` ohne Aufmaß, Lage, Netz oder Platte.
    pub(super) fn terrain_low_survey(&self, gr: StoreyId) -> Option<f64> {
        self.terrain()?;
        let lage = self.lage();
        Some((lage.tief.get(&gr)? - self.nhn_aus(&lage)?) * 1000.0)
    }

    /// Mit Aufmaß: Flächenmittel des Geländes über einem Umriss im Modell
    /// (mm, relativ zu ±0,00) und die Höhe von ±0,00 (m ü. NHN).
    pub(super) fn terrain_mean_survey(&self, outline: &[Vec3]) -> Option<(f64, f64)> {
        let t = self.terrain()?;
        let f = self.survey_frame()?;
        let h0 = self.terrain_nhn()?;
        let q: Vec<(f64, f64)> = outline.iter().map(|p| f.to_survey(p.x, p.y)).collect();
        Some(((t.mean_over(&q)? - h0) * 1000.0, h0))
    }
}

/// Folgen der Höhenlage ([`Model::terrain_folgen`]), mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainFolgen {
    /// OK Sohle über Gelände am Plattenrand, kleinster und größter Wert;
    /// unter 0 steckt das Haus im Gelände.
    pub sockel: (f64, f64),
    /// Gelände am Plattenrand bis UK Gründung.
    pub einbindung: (f64, f64),
    /// Höhe von ±0,00 in m ü. NHN.
    pub nhn: f64,
    /// Die Gründung bindet überall mindestens frostfrei ein.
    pub frostfrei: bool,
}

/// Ecken eines Umrisses gegen den Uhrzeigersinn um ihren Schwerpunkt.
fn im_umlauf(p: &[(f64, f64)]) -> Vec<usize> {
    let n = p.len().max(1) as f64;
    let (cx, cy) = p
        .iter()
        .fold((0.0, 0.0), |(x, y), q| (x + q.0 / n, y + q.1 / n));
    let mut ix: Vec<usize> = (0..p.len()).collect();
    ix.sort_by(|&a, &b| {
        (p[a].1 - cy)
            .atan2(p[a].0 - cx)
            .total_cmp(&(p[b].1 - cy).atan2(p[b].0 - cx))
    });
    ix
}

/// Winkel einer Strecke auf (−90°, 90°] gefaltet (Richtung ohne Sinn), rad.
fn gefaltet(w: f64) -> f64 {
    use std::f64::consts::{FRAC_PI_2, PI};
    let mut w = w.rem_euclid(PI);
    if w > FRAC_PI_2 {
        w -= PI;
    }
    w
}

impl Terrain {
    /// Aufmaß aus einer gelesenen Punktliste. Grenzpunkte (bis acht) werden
    /// im Umlauf zur Grenze verbunden (§4.5); die Lage setzt
    /// [`Model::import_terrain`].
    pub fn from_survey(
        a: &sk_gelaende::lesen::Aufmass,
        src: &str,
        read: &str,
        zone: u8,
    ) -> Terrain {
        use sk_gelaende::lesen::Lage;
        let crs = match a.erkannt.lage {
            Lage::Utm { zone: Some(z) } => Crs::Utm(z),
            Lage::Utm { zone: None } => Crs::Utm(zone),
            Lage::GaussKrueger { streifen } => Crs::GaussKrueger(streifen),
            Lage::Oertlich => Crs::Local,
        };
        let mut t = Terrain::new(crs);
        t.src = src.to_string();
        t.read = read.to_string();
        t.points = a
            .punkte
            .iter()
            .map(|p| TerrainPoint {
                nr: p.nr.clone(),
                e: p.e,
                n: p.n,
                h: p.h,
                code: p.code.clone(),
                off: false,
            })
            .collect();
        let codes = t.code_table();
        let grenze: Vec<usize> = (0..t.points.len())
            .filter(|&i| codes.bedeutung(&t.points[i].code) == Bedeutung::Grenzpunkt)
            .collect();
        if (3..=8).contains(&grenze.len()) {
            let lage: Vec<(f64, f64)> = grenze
                .iter()
                .map(|&i| (t.points[i].e, t.points[i].n))
                .collect();
            t.boundaries.push(
                im_umlauf(&lage)
                    .into_iter()
                    .map(|k| t.points[grenze[k]].nr.clone())
                    .collect(),
            );
        }
        t
    }

    /// Eine Punktliste lesen, wie „Höhen“ in der Werkzeugleiste sie
    /// übernimmt (§5 G1): das Aufmaß, der Satz, was erkannt wurde, und die
    /// Hinweise aus Lesen, Prüfen und Netz. Ohne Zonenkennzahl gilt
    /// `zone`. `Err` mit einem Satz, wenn kein Gelände entsteht.
    pub fn lesen(
        roh: &[u8],
        src: &str,
        read: &str,
        zone: u8,
    ) -> Result<(Terrain, String, Vec<String>), String> {
        let a = sk_gelaende::lesen::lesen(roh)?;
        let t = Terrain::from_survey(&a, src, read, zone);
        let m = t.mesh();
        if m.netz.dreiecke.is_empty() {
            return Err(
                "Aus den Punkten entsteht kein Gelände: Es braucht drei Punkte, die nicht auf einer Linie liegen."
                    .into(),
            );
        }
        let mut hinweise = a.hinweise.clone();
        hinweise.extend(m.hinweise.iter().cloned());
        Ok((t, a.satz(), hinweise))
    }

    /// Grenzen als Landeswerte; Nummern ohne Punkt fehlen, bei doppelter
    /// Nummer gilt der erste Punkt. Die Nummern werden einmal in eine
    /// Tabelle gelegt, nicht je Grenzpunkt gesucht (Review W45 H4).
    pub fn boundary_rings(&self) -> Vec<Vec<(f64, f64)>> {
        if self.boundaries.is_empty() {
            return Vec::new();
        }
        let mut nach_nr: HashMap<&str, (f64, f64)> = HashMap::with_capacity(self.points.len());
        for p in &self.points {
            nach_nr.entry(p.nr.as_str()).or_insert((p.e, p.n));
        }
        self.boundaries
            .iter()
            .map(|b| {
                b.iter()
                    .filter_map(|nr| nach_nr.get(nr.as_str()).copied())
                    .collect::<Vec<_>>()
            })
            .filter(|r| r.len() >= 3)
            .collect()
    }

    /// Fläche aller Grenzen (m²).
    pub fn parcel_area(&self) -> f64 {
        self.boundary_rings()
            .iter()
            .map(|r| {
                let n = r.len();
                ((0..n)
                    .map(|i| r[i].0 * r[(i + 1) % n].1 - r[(i + 1) % n].0 * r[i].1)
                    .sum::<f64>()
                    / 2.0)
                    .abs()
            })
            .sum()
    }
}

impl Model {
    /// Ein gelesenes Aufmaß übernehmen (im offenen Schritt, §5 G1 Schritt 4
    /// und 5). Liegt es in UTM und ist `[location] utm` in derselben Zone
    /// gesetzt, fällt es an seinen Platz. Sonst liegt das Grundstück mittig
    /// unter dem Haus, gedreht so, dass die längste Grenze parallel zur
    /// längsten Hauswand liegt (ohne Grenze Nord nach oben), Nullpunkt auf
    /// volle Meter. Ist `nhn` gesetzt, gilt die Höhenlage absolut, sonst
    /// über dem höchsten Geländepunkt am Plattenrand.
    pub fn import_terrain(&mut self, mut t: Terrain) -> bool {
        let vorher = self.foundation_terrain();
        let loc = self.location().normalized();
        let an_seinem_platz = matches!(t.crs, Crs::Utm(z) if loc.utm.is_some_and(|u| u.zone == z));
        // ein neues Aufmaß behält die Art der Höhenlage des alten
        t.href = match self.terrain() {
            Some(alt) => alt.href,
            None if loc.nhn.is_some() => HeightRef::Abs,
            None => HeightRef::High,
        };
        if !an_seinem_platz {
            let (rahmen, nord) = self.lage_unter_dem_haus(&t);
            match t.crs {
                Crs::Utm(z) => {
                    let l = Location {
                        utm: Some(Utm {
                            e: rahmen.e0,
                            n: rahmen.n0,
                            zone: z,
                        }),
                        north: Some(nord),
                        ..loc
                    };
                    self.put_location(l);
                }
                _ => {
                    t.local = Some(LocalFrame {
                        e0: rahmen.e0,
                        n0: rahmen.n0,
                        rot: rahmen.g.to_degrees(),
                    })
                }
            }
        }
        let neu = self.put_survey(Some(t));
        self.terrain_nhn_merken();
        self.resettle_foundations(vorher);
        neu
    }

    /// Nordrichtung (Grad) zu einer Gitterdrehung `g` am Nullpunkt.
    fn nord_zu(g: f64, e0: f64, n0: f64, zone: u8) -> f64 {
        let (b, l) = sk_math::utm::von_utm(e0, n0, zone);
        (g.to_degrees() - sk_math::utm::konvergenz(b, l, zone)).rem_euclid(360.0)
    }

    /// Lage für ein Aufmaß ohne Platz: Rahmen und Nordrichtung (Grad).
    fn lage_unter_dem_haus(&self, t: &Terrain) -> (SurveyFrame, f64) {
        let zone = match t.crs {
            Crs::Utm(z) => Some(z),
            _ => None,
        };
        let ringe = t.boundary_rings();
        // Mitte des Aufmaßes: Mitte der Grenze, sonst des Punkterechtecks
        let mitte = |q: &mut dyn Iterator<Item = (f64, f64)>| {
            let (mut a, mut b) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
            for p in q {
                a = (a.0.min(p.0), a.1.min(p.1));
                b = (b.0.max(p.0), b.1.max(p.1));
            }
            ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
        };
        let c = if ringe.is_empty() {
            mitte(&mut t.points.iter().map(|p| (p.e, p.n)))
        } else {
            mitte(&mut ringe.iter().flatten().copied())
        };
        // Haus: Mitte und längste Wand der Außenumrisse
        let umrisse = self.plate_outlines();
        let mut h = (0.0, 0.0);
        let mut wand: Option<(f64, f64)> = None;
        if !umrisse.is_empty() {
            h = mitte(&mut umrisse.iter().flat_map(|o| o.1.iter().map(|p| (p.x, p.y))));
            for (_, r) in &umrisse {
                for i in 0..r.len() {
                    let (a, b) = (r[i], r[(i + 1) % r.len()]);
                    let l = (b.x - a.x).hypot(b.y - a.y);
                    if wand.is_none_or(|w| l > w.0) {
                        wand = Some((l, (b.y - a.y).atan2(b.x - a.x)));
                    }
                }
            }
        }
        let grenze = ringe
            .iter()
            .flat_map(|r| (0..r.len()).map(move |i| (r[i], r[(i + 1) % r.len()])))
            .map(|(a, b)| ((b.0 - a.0).hypot(b.1 - a.1), (b.1 - a.1).atan2(b.0 - a.0)))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let g = match (grenze, wand, zone) {
            // Modellwinkel = Gitterwinkel − g: längste Grenze ∥ längste Wand
            (Some((_, wg)), Some((_, wm)), _) => gefaltet(wg - wm),
            // Nord nach oben: geografisch Nord = +y
            (_, _, Some(z)) => {
                let (b, l) = sk_math::utm::von_utm(c.0, c.1, z);
                sk_math::utm::konvergenz(b, l, z).to_radians()
            }
            _ => 0.0,
        };
        let (s, co) = g.sin_cos();
        let (hx, hy) = (h.0 / 1000.0, h.1 / 1000.0);
        let e0 = (c.0 - (hx * co - hy * s)).round();
        let n0 = (c.1 - (hx * s + hy * co)).round();
        let nord = zone.map_or(0.0, |z| Model::nord_zu(g, e0, n0, z));
        (SurveyFrame { e0, n0, g }, nord)
    }

    /// Schreibt die Höhe von ±0,00 nach `[location] nhn`, wenn sie über dem
    /// Gelände liegt (für IFC und Projektdaten; „beide schreiben am Ende
    /// nhn“, §4.3). Läuft auch am Ende jedes Schritts, damit die Höhe
    /// Wandänderungen folgt (Review W45 H2).
    pub(super) fn terrain_nhn_merken(&mut self) {
        if self.terrain().is_some_and(|t| t.href == HeightRef::High) {
            if let Some(h) = self.terrain_nhn() {
                let h = (h * 1000.0).round() / 1000.0;
                let l = Location {
                    nhn: Some(h),
                    ..self.location().normalized()
                };
                self.put_location(l);
            }
        }
    }

    /// Rahmen setzen (UTM: `[location]`, sonst am Aufmaß); die Gründungen
    /// behalten ihre Einbindetiefe.
    fn rahmen_setzen(&mut self, f: SurveyFrame) -> bool {
        let vorher = self.foundation_terrain();
        let neu = self.rahmen_schreiben(f);
        self.resettle_foundations(vorher);
        neu
    }

    fn rahmen_schreiben(&mut self, f: SurveyFrame) -> bool {
        let Some(t) = self.terrain() else {
            return false;
        };
        let geaendert = match t.crs {
            Crs::Utm(z) => {
                let loc = self.location().normalized();
                let l = Location {
                    utm: Some(Utm {
                        e: f.e0,
                        n: f.n0,
                        zone: z,
                    }),
                    north: Some(Model::nord_zu(f.g, f.e0, f.n0, z)),
                    ..loc
                };
                self.put_location(l)
            }
            _ => {
                let mut t = t.clone();
                t.local = Some(LocalFrame {
                    e0: f.e0,
                    n0: f.n0,
                    rot: f.g.to_degrees(),
                });
                self.put_survey(Some(t))
            }
        };
        if geaendert {
            self.terrain_nhn_merken();
        }
        geaendert
    }

    /// Gelände unter dem Haus verschieben (im offenen Schritt, §4.8): jeder
    /// Geländepunkt wandert um `d` (mm, Modell); das Haus bleibt.
    pub fn move_terrain(&mut self, d: [f64; 2]) -> bool {
        let Some(f) = self.survey_frame() else {
            return false;
        };
        if !(d[0].is_finite() && d[1].is_finite()) || (d[0] == 0.0 && d[1] == 0.0) {
            return false;
        }
        let (s, c) = f.g.sin_cos();
        let (dx, dy) = (d[0] / 1000.0, d[1] / 1000.0);
        self.rahmen_setzen(SurveyFrame {
            e0: f.e0 - (dx * c - dy * s),
            n0: f.n0 - (dx * s + dy * c),
            g: f.g,
        })
    }

    /// Gelände unter dem Haus um `um` (rad, gegen den Uhrzeigersinn) um den
    /// Modellpunkt `mitte` (mm) drehen (im offenen Schritt, §4.8).
    pub fn rotate_terrain(&mut self, mitte: [f64; 2], um: f64) -> bool {
        let Some(f) = self.survey_frame() else {
            return false;
        };
        if !um.is_finite() || um == 0.0 || !mitte.iter().all(|v| v.is_finite()) {
            return false;
        }
        // Der Landeswert, der unter `mitte` lag, liegt dort wieder
        let (em, nm) = f.to_survey(mitte[0], mitte[1]);
        let g = f.g - um;
        let (s, c) = g.sin_cos();
        let (mx, my) = (mitte[0] / 1000.0, mitte[1] / 1000.0);
        self.rahmen_setzen(SurveyFrame {
            e0: em - (mx * c - my * s),
            n0: nm - (mx * s + my * c),
            g,
        })
    }

    /// Art der Höhenlage setzen (im offenen Schritt). Bei „absolut“ bleibt
    /// die jetzige Höhe von ±0,00 fest stehen.
    pub fn set_terrain_href(&mut self, href: HeightRef) -> bool {
        let Some(t) = self.terrain() else {
            return false;
        };
        if t.href == href {
            return false;
        }
        let vorher = self.foundation_terrain();
        let jetzt = self.terrain_nhn();
        let mut t = t.clone();
        t.href = href;
        self.put_survey(Some(t));
        if let (HeightRef::Abs, Some(h)) = (href, jetzt) {
            let l = Location {
                nhn: Some((h * 1000.0).round() / 1000.0),
                ..self.location().normalized()
            };
            self.put_location(l);
        }
        self.terrain_nhn_merken();
        self.resettle_foundations(vorher);
        true
    }

    /// Höhe von ±0,00 fest setzen (m ü. NHN, im offenen Schritt): Höhenlage
    /// „absolut“.
    pub fn set_terrain_nhn(&mut self, h: f64) -> bool {
        if !location::im_bereich(h, location::NHN) {
            return false;
        }
        let Some(mut t) = self.terrain().cloned() else {
            return false;
        };
        let vorher = self.foundation_terrain();
        t.href = HeightRef::Abs;
        let a = self.put_survey(Some(t));
        let l = Location {
            nhn: Some(h),
            ..self.location().normalized()
        };
        let b = self.put_location(l);
        self.resettle_foundations(vorher);
        a || b
    }

    /// Zuschnitt setzen (im offenen Schritt, §4.5).
    pub fn set_terrain_show(&mut self, show: Show) -> bool {
        let Some(t) = self.terrain() else {
            return false;
        };
        if t.show() == show {
            return false;
        }
        let mut t = t.clone();
        t.show = Some(show);
        self.put_survey(Some(t))
    }

    /// Gelände um die Hausmitte so drehen, dass die längste Grenze
    /// parallel zur nächstliegenden Hauswand liegt, um den kleinsten Winkel
    /// (§4.8 „An Grenze ausrichten“; im offenen Schritt). `false` ohne
    /// Grenze, Haus oder wenn sie schon parallel liegt.
    pub fn align_terrain(&mut self) -> bool {
        let grenze = self
            .terrain_boundaries()
            .iter()
            .flat_map(|r| (0..r.len()).map(move |i| (r[i], r[(i + 1) % r.len()])))
            .map(|(a, b)| {
                (
                    (b[0] - a[0]).hypot(b[1] - a[1]),
                    (b[1] - a[1]).atan2(b[0] - a[0]),
                )
            })
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let umrisse = self.plate_outlines();
        let Some((_, wg)) = grenze else {
            return false;
        };
        let um = umrisse
            .iter()
            .flat_map(|(_, r)| (0..r.len()).map(move |i| (r[i], r[(i + 1) % r.len()])))
            .filter(|(a, b)| (b.x - a.x).hypot(b.y - a.y) > 1.0)
            .map(|(a, b)| gefaltet((b.y - a.y).atan2(b.x - a.x) - wg))
            .min_by(|a, b| a.abs().total_cmp(&b.abs()));
        let (Some(um), Some(mitte)) = (um.filter(|u| u.abs() > 1e-9), self.house_center()) else {
            return false;
        };
        self.rotate_terrain(mitte, um)
    }

    /// Mitte des Hauses im Grundriss (mm): Mitte des Rechtecks um alle
    /// Plattenumrisse, um sie dreht „Drehen“ (§4.8). `None` ohne Haus.
    pub fn house_center(&self) -> Option<[f64; 2]> {
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for (_, r) in self.plate_outlines() {
            for p in r {
                lo = [lo[0].min(p.x), lo[1].min(p.y)];
                hi = [hi[0].max(p.x), hi[1].max(p.y)];
            }
        }
        (lo[0] <= hi[0]).then(|| [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0])
    }

    /// Folgen der Höhenlage am bestimmenden Gebäude (§4.3): Sockel (OK
    /// Sohle über Gelände) und Einbindung der Schürze (Gelände bis UK
    /// Gründung) am Plattenrand, je kleinster und größter Wert in mm.
    /// `None` ohne Aufmaß oder Haus.
    pub fn terrain_folgen(&self) -> Option<TerrainFolgen> {
        let t = self.terrain()?;
        let f = self.survey_frame()?;
        let h0 = self.terrain_nhn()?;
        let b = self.level_building()?;
        let mut hoehen: Vec<f64> = Vec::new();
        for (_, ring) in self.plate_outlines().into_iter().filter(|o| o.0 == b) {
            let r: Vec<(f64, f64)> = ring.iter().map(|p| f.to_survey(p.x, p.y)).collect();
            hoehen.extend(t.along(&r).into_iter().map(|h| (h - h0) * 1000.0));
        }
        let lo = hoehen.iter().copied().reduce(f64::min)?;
        let hi = hoehen.iter().copied().reduce(f64::max)?;
        let (gr, uk) = self
            .storeys
            .iter()
            .filter(|(_, s)| s.kind == LevelKind::Foundation && s.building == Some(b))
            .map(|(id, s)| (id, s.elevation))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        Some(TerrainFolgen {
            sockel: (-hi, -lo),
            einbindung: (lo - uk, hi - uk),
            nhn: h0,
            frostfrei: self.frost_safe(gr),
        })
    }

    /// Dreiecke des Geländes im Modell (mm, z relativ zu ±0,00), gegen den
    /// Uhrzeigersinn; leer ohne Aufmaß, Lage oder Netz.
    pub fn terrain_triangles(&self) -> Vec<[Vec3; 3]> {
        let (Some(t), Some(f), Some(h0)) =
            (self.terrain(), self.survey_frame(), self.terrain_nhn())
        else {
            return Vec::new();
        };
        let m = t.mesh();
        let p: Vec<Vec3> = m
            .netz
            .punkte
            .iter()
            .map(|q| {
                let [x, y] = f.to_model(q[0] + m.bezug.0, q[1] + m.bezug.1);
                sk_math::vec3(x, y, (q[2] - h0) * 1000.0)
            })
            .collect();
        m.netz
            .dreiecke
            .iter()
            .map(|d| d.map(|i| p[i as usize]))
            .collect()
    }

    /// Abstand der Höhenlinien (m) für den Wunsch `wunsch`: er selbst, oder
    /// der nächste gröbere aus 0,5 / 1 / 2 / 2,5 / 5 / 10 / 25 / 50 / 100 m,
    /// wenn er mehr als [`sk_gelaende::linien::MAX_STRECKEN`] Stücke gäbe
    /// (Review #121, Auflage H1: vorab gezählt, in O(Dreiecke)). Der Wunsch
    /// gilt geklemmt auf [`sk_gelaende::linien::MIN_STUFE`] bis 100 m, und
    /// es passt nur ein Abstand, mit dem Linien entstehen (Review #123 W45
    /// B4 und Nachprüfung: ein ebenes Aufmaß hat Höhen, aber keine Linie).
    /// `None` ohne Aufmaß oder Netz, bei einem Wunsch, der keine Zahl ist,
    /// wenn auch 100 m nicht passt oder bei keinem Abstand eine Linie
    /// entsteht.
    pub fn terrain_contour_step(&self, wunsch: f64) -> Option<f64> {
        use sk_gelaende::linien::{strecken, MAX_STRECKEN, MIN_STUFE};
        const GROEBER: [f64; 9] = [0.5, 1.0, 2.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0];
        if wunsch.is_nan() {
            return None;
        }
        let wunsch = wunsch.clamp(MIN_STUFE, GROEBER[8]);
        let m = self.terrain()?.mesh();
        gemerkt(&m.stufen, wunsch, || {
            std::iter::once(wunsch)
                .chain(GROEBER.into_iter().filter(|&s| s > wunsch))
                .find(|&s| strecken(&m.netz, s).is_some_and(|n| n > 0 && n <= MAX_STRECKEN))
        })
    }

    /// Stücke der Höhenlinien im Abstand `stufe` (m), vorab gezählt wie in
    /// [`Model::terrain_contour_step`]: `Some(0)` bei ebenem Gelände, `None`
    /// ohne Aufmaß oder bei zu vielen Höhen.
    pub fn terrain_contour_pieces(&self, stufe: f64) -> Option<usize> {
        sk_gelaende::linien::strecken(&self.terrain()?.mesh().netz, stufe)
    }

    /// Höhenlinien im Abstand `stufe` (m) im Modell: Höhe (m ü. NHN) und
    /// Punkte (mm), das Höhere links. Mehr als
    /// [`sk_gelaende::linien::MAX_STRECKEN`] Stücke geben keine Linien,
    /// [`Model::terrain_contour_step`] nennt einen Abstand, der passt.
    pub fn terrain_contours(&self, stufe: f64) -> Vec<(f64, Vec<[f64; 2]>, bool)> {
        let (Some(t), Some(f)) = (self.terrain(), self.survey_frame()) else {
            return Vec::new();
        };
        let m = t.mesh();
        m.hoehenlinien(stufe)
            .iter()
            .map(|l| {
                let p = l
                    .punkte
                    .iter()
                    .map(|q| f.to_model(q[0] + m.bezug.0, q[1] + m.bezug.1))
                    .collect();
                (l.z, p, l.geschlossen)
            })
            .collect()
    }

    /// Grenzen im Modell (mm).
    pub fn terrain_boundaries(&self) -> Vec<Vec<[f64; 2]>> {
        let (Some(t), Some(f)) = (self.terrain(), self.survey_frame()) else {
            return Vec::new();
        };
        t.boundary_rings()
            .into_iter()
            .map(|r| r.into_iter().map(|(e, n)| f.to_model(e, n)).collect())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &[u8] =
        include_bytes!("../../sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.csv");

    /// Haus des Beispiels: Außenfläche x 7 bis 17 m, y 6 bis 14 m.
    fn lindenhof_haus() -> (Model, BuildingId) {
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            sk_math::vec3(7000.0, 6000.0, 0.0),
            sk_math::vec3(7000.0, 14000.0, 0.0),
            sk_math::vec3(17000.0, 14000.0, 0.0),
            sk_math::vec3(17000.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        (m, b)
    }

    fn aufmass() -> Terrain {
        let a = sk_gelaende::lesen::lesen(CSV).unwrap();
        Terrain::from_survey(&a, "hoehenaufmass-lindenhof-7.csv", "2026-10-12", 32)
    }

    /// Abnahme G1 im Modell (sollwerte-lindenhof-7.md): Aufmaß an seinem
    /// Platz, Höhenlage über Gelände 24,933, Hausecken, absolut 24,95 mit
    /// Sockel 1,7 bis 42,7 cm, Verschieben und Drehen, Rückgängig.
    #[test]
    fn lindenhof_an_seinem_platz() {
        let (mut m, b) = lindenhof_haus();
        let mut tx = Vec::new();
        let (e0, n0) = (467108.214, 5878741.637);
        let nord = Model::nord_zu(7f64.to_radians(), e0, n0, 32);
        m.begin("Lage");
        m.set_location(Location {
            utm: Some(Utm {
                e: e0,
                n: n0,
                zone: 32,
            }),
            north: Some(nord),
            ..Location::default()
        });
        m.commit();
        m.begin("Aufmaß");
        assert!(m.import_terrain(aufmass()));
        tx.push(m.commit().unwrap());
        let t = m.terrain().unwrap();
        assert_eq!(
            (t.crs, t.href, t.points.len()),
            (Crs::Utm(32), HeightRef::High, 109)
        );
        assert_eq!(t.boundaries.len(), 1);
        let mut g = t.boundaries[0].clone();
        g.sort();
        assert_eq!(g, ["1", "2", "3", "4"]);
        assert!(
            (t.parcel_area() - 822.87).abs() < 0.01,
            "{}",
            t.parcel_area()
        );
        let f = m.survey_frame().unwrap();
        assert!((f.g - 7f64.to_radians()).abs() < 1e-9);
        let h0 = m.terrain_nhn().unwrap();
        assert!((h0 - 24.933).abs() < 0.0005, "{h0}");
        assert_eq!(m.location().nhn, Some((h0 * 1000.0).round() / 1000.0));
        let ecke = |m: &Model, x: f64, y: f64| m.terrain_z_at(Some(b), x, y);
        for (x, y, soll) in [
            (7000.0, 6000.0, 24.523),
            (17000.0, 6000.0, 24.683),
            (17000.0, 14000.0, 24.933),
            (7000.0, 14000.0, 24.796),
        ] {
            let z = ecke(&m, x, y);
            assert!((z - (soll - h0) * 1000.0).abs() < 0.5, "{x} {y}: {z}");
        }
        assert!(!m.terrain_triangles().is_empty());
        assert!(!m.terrain_contours(0.25).is_empty());
        assert_eq!(m.terrain_boundaries()[0].len(), 4);
        // absolut 24,95: Sockel 1,7 bis 42,7 cm
        m.begin("Höhe");
        assert!(m.set_terrain_nhn(24.95));
        tx.push(m.commit().unwrap());
        assert_eq!(m.terrain().unwrap().href, HeightRef::Abs);
        let sockel: Vec<f64> = [(7000.0, 6000.0), (17000.0, 14000.0)]
            .iter()
            .map(|p| -ecke(&m, p.0, p.1))
            .collect();
        assert!(
            (sockel[0] - 427.0).abs() < 0.5 && (sockel[1] - 17.0).abs() < 0.5,
            "{sockel:?}"
        );
        // Verschieben: was unter (7, 6) lag, liegt danach unter (8, 6)
        let vorher = ecke(&m, 7000.0, 6000.0);
        m.begin("Verschieben");
        assert!(m.move_terrain([1000.0, 0.0]));
        tx.push(m.commit().unwrap());
        assert!((ecke(&m, 8000.0, 6000.0) - vorher).abs() < 1e-6);
        // Drehen um die Hausmitte und zurück
        let mitte = [12000.0, 10000.0];
        let z = ecke(&m, 9000.0, 7000.0);
        let z_mitte = ecke(&m, mitte[0], mitte[1]);
        m.begin("Drehen");
        assert!(m.rotate_terrain(mitte, 0.3));
        tx.push(m.commit().unwrap());
        // die Mitte bleibt, wo sie war; daneben dreht sich das Gelände weg
        assert!((ecke(&m, mitte[0], mitte[1]) - z_mitte).abs() < 1e-6);
        assert!((ecke(&m, 9000.0, 7000.0) - z).abs() > 1.0);
        m.begin("Drehen");
        assert!(m.rotate_terrain(mitte, -0.3));
        tx.push(m.commit().unwrap());
        assert!((ecke(&m, 9000.0, 7000.0) - z).abs() < 1e-6);
        // Rückgängig bis vor das Aufmaß
        for t in tx.iter().rev() {
            assert!(m.terrain().is_some());
            m.apply(t, crate::txn::Direction::Undo);
        }
        assert!(m.terrain().is_none());
        assert_eq!(ecke(&m, 7000.0, 6000.0), 0.0);
    }

    /// Über Gelände folgt ±0,00 den Wänden: Steht das Haus nach einem Umbau
    /// tiefer am Hang, schreibt der Schritt die neue Höhe nach `[location]
    /// nhn`, Rückgängig nimmt sie zurück (Review W45 H2).
    #[test]
    fn nhn_folgt_den_waenden() {
        let (mut m, b) = lindenhof_haus();
        let (e0, n0) = (467108.214, 5878741.637);
        m.begin("Lage");
        m.set_location(Location {
            utm: Some(Utm {
                e: e0,
                n: n0,
                zone: 32,
            }),
            north: Some(Model::nord_zu(7f64.to_radians(), e0, n0, 32)),
            ..Location::default()
        });
        m.commit();
        m.begin("Aufmaß");
        assert!(m.import_terrain(aufmass()));
        m.commit();
        let gerundet = |h: f64| (h * 1000.0).round() / 1000.0;
        let vorher = m.location().nhn;
        assert_eq!(vorher, m.terrain_nhn().map(gerundet));
        // dasselbe Haus 3 m weiter südlich, wo der Hang tiefer liegt
        let r = m.runs().ids().next().unwrap();
        m.begin("Umbau");
        assert!(m.remove_run(r));
        let pts = [
            sk_math::vec3(7000.0, 3000.0, 0.0),
            sk_math::vec3(7000.0, 11000.0, 0.0),
            sk_math::vec3(17000.0, 11000.0, 0.0),
            sk_math::vec3(17000.0, 3000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let tx = m.commit().unwrap();
        let jetzt = m.location().nhn;
        assert_eq!(jetzt, m.terrain_nhn().map(gerundet));
        assert!(
            jetzt.unwrap() < vorher.unwrap() - 0.05,
            "{jetzt:?} {vorher:?}"
        );
        m.apply(&tx, crate::txn::Direction::Undo);
        assert_eq!(m.location().nhn, vorher);
        assert_eq!(m.terrain_nhn().map(gerundet), vorher);
    }

    /// Eine Kopie mit abgewähltem Punkt bringt nicht das alte Netz mit,
    /// der Punkt fehlt sofort; Verschieben behält das Netz (Review W45 B2).
    #[test]
    fn abwahl_rechnet_neues_netz() {
        let (mut m, _) = lindenhof_haus();
        m.begin("Aufmaß");
        assert!(m.import_terrain(aufmass()));
        m.commit();
        let netz = m.terrain().unwrap().mesh();
        m.begin("Verschieben");
        assert!(m.move_terrain([1000.0, 0.0]));
        m.commit();
        assert!(Arc::ptr_eq(&netz, &m.terrain().unwrap().mesh()));
        let mut t = m.terrain().unwrap().clone();
        t.points[netz.index[0]].off = true;
        m.begin("Abwahl");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        let neu = m.terrain().unwrap().mesh();
        assert_eq!(neu.index.len() + 1, netz.index.len());
        assert!(!neu.index.contains(&netz.index[0]));
    }

    /// Frostschürze ab dem tiefsten Gelände am Plattenrand, Erdmengen aus
    /// dem Flächenmittel unter der Platte (sollwerte-lindenhof-7.md): bei
    /// ±0,00 = 24,95 liegt UK Schürze auf 23,723, das Mittel auf 24,753.
    /// Ohne Aufmaß liegt die Gründung wieder wie vorher.
    #[test]
    fn lindenhof_schuerze_und_erdmengen() {
        use crate::qto_earth::{TOPSOIL, TRENCH};
        let (mut m, _) = lindenhof_haus();
        let gr = m.foundation_level().unwrap();
        let uk = |m: &Model| m.storey(gr).unwrap().elevation;
        let (uk0, tiefe) = (uk(&m), m.embedment_of(gr).unwrap());
        assert_eq!(tiefe, 800.0, "Vorgabe: frostfrei 0,80 m");
        let (e0, n0) = (467108.214, 5878741.637);
        m.begin("Lage");
        m.set_location(Location {
            utm: Some(Utm {
                e: e0,
                n: n0,
                zone: 32,
            }),
            north: Some(Model::nord_zu(7f64.to_radians(), e0, n0, 32)),
            ..Location::default()
        });
        m.commit();
        assert_eq!(uk(&m), uk0, "ohne Aufmaß bewegt die Lage nichts");
        m.begin("Aufmaß");
        assert!(m.import_terrain(aufmass()));
        m.commit();
        // über dem höchsten Punkt am Rand (24,933): 0,80 unter 24,523
        assert!((uk(&m) - ((24.523 - 24.933) * 1000.0 - 800.0)).abs() < 0.5);
        assert!((m.embedment_of(gr).unwrap() - 800.0).abs() < 1e-6);
        assert!(m.frost_safe(gr));
        m.begin("Höhe");
        assert!(m.set_terrain_nhn(24.95));
        m.commit();
        let uk_nhn = 24.95 + uk(&m) / 1000.0;
        assert!((uk_nhn - 23.723).abs() < 0.0005, "UK Schürze {uk_nhn}");
        let g = m.ground_bases().remove(0);
        assert_eq!(g.terrain_nhn, Some(24.95));
        let mittel = 24.95 + g.terrain_z / 1000.0;
        assert!((mittel - 24.753).abs() < 0.0005, "Mittel {mittel}");
        assert!((g.embedment - 800.0).abs() < 1e-6);
        let s = crate::qto::schedule(&m);
        let ansatz = |k: &str| s.auto.iter().find(|a| a.key == k).unwrap().formula.clone();
        assert!(
            ansatz(TRENCH).ends_with("; Gelände aus Aufmaß, Mittel 24,75"),
            "{}",
            ansatz(TRENCH)
        );
        assert!(!ansatz(TOPSOIL).contains("Aufmaß"));
        // Verschieben: die Schürze folgt dem tiefsten Punkt
        m.begin("Verschieben");
        assert!(m.move_terrain([0.0, -3000.0]));
        m.commit();
        assert!((m.embedment_of(gr).unwrap() - 800.0).abs() < 1e-6);
        // Aufmaß entfernen: wieder die Ebene, die Gründung wie vorher
        m.begin("Ohne Aufmaß");
        assert!(m.set_terrain(None));
        m.commit();
        assert!((uk(&m) - uk0).abs() < 1e-6, "{} {uk0}", uk(&m));
        assert_eq!(m.ground_bases()[0].terrain_nhn, None);
    }

    /// OK Gelände an der Gründung setzen (Geschossmanager): ohne Aufmaß
    /// der Gegenwert des Versatzes, mit Aufmaß wandert ±0,00 gegen das
    /// Gelände, über den Versatz oder absolut über NHN; die Kote stimmt
    /// danach, die Einbindetiefe bleibt (Geometrie zu #129).
    #[test]
    fn ok_gelaende_an_der_gruendung() {
        let (mut m, b) = lindenhof_haus();
        let gr = m.foundation_level().unwrap();
        let setzen = |m: &mut Model, z: f64| {
            m.begin("Gelände");
            let ok = m.set_terrain_z_of_level(gr, z);
            m.commit();
            ok
        };
        assert_eq!(m.terrain_z_of_level_range(gr), (-3000.0, 3000.0));
        assert!(setzen(&mut m, -300.0));
        assert_eq!(m.terrain_offset_of(Some(b)), 300.0);
        assert_eq!(m.terrain_z_of_level(gr), -300.0);
        assert!(!setzen(&mut m, f64::NAN));
        m.begin("Aufmaß");
        assert!(m.import_terrain(aufmass()));
        m.commit();
        for (href, dz) in [(HeightRef::High, -200.0), (HeightRef::Abs, 150.0)] {
            if href == HeightRef::Abs {
                m.begin("Höhe");
                assert!(m.set_terrain_nhn(24.95));
                m.commit();
            }
            let z0 = m.terrain_z_of_level(gr);
            let (lo, hi) = m.terrain_z_of_level_range(gr);
            assert!(lo < z0 && z0 < hi, "{lo} {z0} {hi}");
            assert!(setzen(&mut m, z0 + dz));
            let z1 = m.terrain_z_of_level(gr);
            assert!((z1 - (z0 + dz)).abs() < 0.5, "{href:?}: {z0} + {dz} → {z1}");
            assert_eq!(m.terrain().unwrap().href, href);
            assert!((m.embedment_of(gr).unwrap() - 800.0).abs() < 1e-6);
        }
    }

    /// Ohne Lage liegt das Grundstück mittig unter dem Haus, die längste
    /// Grenze parallel zur längsten Wand, Nullpunkt auf vollen Metern.
    #[test]
    fn lindenhof_unter_dem_haus() {
        let (mut m, _) = lindenhof_haus();
        m.begin("Aufmaß");
        assert!(m.import_terrain(aufmass()));
        m.commit();
        let u = m.location().utm.unwrap();
        assert_eq!((u.e.fract(), u.n.fract(), u.zone), (0.0, 0.0, 32));
        let g = &m.terrain_boundaries()[0];
        let (mut lang, mut w) = (0.0, 0.0);
        let (mut cx, mut cy) = (0.0, 0.0);
        for i in 0..g.len() {
            let (a, b) = (g[i], g[(i + 1) % g.len()]);
            let l = (b[0] - a[0]).hypot(b[1] - a[1]);
            if l > lang {
                (lang, w) = (l, (b[1] - a[1]).atan2(b[0] - a[0]));
            }
            cx += a[0] / g.len() as f64;
            cy += a[1] / g.len() as f64;
        }
        assert!(gefaltet(w).abs() < 1e-9, "längste Grenze ∥ x: {w}");
        // Mitte der Grenze ≈ Hausmitte (Rundung auf volle Meter)
        assert!(
            (cx - 12000.0).abs() < 1500.0 && (cy - 10000.0).abs() < 1500.0,
            "{cx} {cy}"
        );
        assert!(m.terrain_nhn().is_some());
    }

    /// Örtliches System: Lage am Aufmaß, Höhe aus dem Netz.
    #[test]
    fn oertliches_aufmass() {
        let (mut m, b) = lindenhof_haus();
        let a = sk_gelaende::lesen::lesen(b"1 0 0 10\n2 30 0 10.3\n3 30 30 10.6\n4 0 30 10.3\n")
            .unwrap();
        let t = Terrain::from_survey(&a, "x.txt", "", 32);
        assert_eq!(t.crs, Crs::Local);
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        assert!(m.terrain().unwrap().local.is_some());
        assert!(m.location().utm.is_none());
        let z = m.terrain_z_at(Some(b), 7000.0, 6000.0);
        assert!(z.is_finite() && z <= 0.0, "{z}");
    }

    /// Zu viele Stücke für Höhenlinien: vorab gezählt, ein gröberer
    /// Abstand passt (Review #121, Auflage H1).
    #[test]
    fn hoehenlinien_vorab_gezaehlt() {
        let (mut m, _) = lindenhof_haus();
        assert_eq!(m.terrain_contour_step(0.25), None, "ohne Aufmaß");
        // 30 × 30 Punkte, jede zweite Reihe 400 m höher: bei 0,25 m wären
        // es 2,7 Mio. Stücke, bei 0,5 m 1,3 Mio.
        let mut text = String::new();
        for i in 0..30 {
            for j in 0..30 {
                let h = 10.1 + (i % 2) as f64 * 400.0;
                text.push_str(&format!("{} {j} {i} {h}\n", i * 30 + j + 1));
            }
        }
        let a = sk_gelaende::lesen::lesen(text.as_bytes()).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(Terrain::from_survey(&a, "x.txt", "", 32)));
        m.commit();
        let t = std::time::Instant::now();
        assert_eq!(m.terrain_contour_step(0.25), Some(1.0));
        assert!(m.terrain_contours(0.25).is_empty());
        let grenze = if cfg!(debug_assertions) { 5.0 } else { 0.5 };
        assert!(t.elapsed().as_secs_f64() < grenze, "{:?}", t.elapsed());
        assert_eq!(m.terrain_contours(1.0).len(), 400 * 29);
        // das Beispiel bleibt beim Wunsch
        let (mut m, _) = lindenhof_haus();
        m.begin("Aufmaß");
        assert!(m.import_terrain(aufmass()));
        m.commit();
        assert_eq!(m.terrain_contour_step(0.25), Some(0.25));
        // geklemmt, was gerechnet wird; keine Zahl passt nie (W45 B4)
        assert_eq!(m.terrain_contour_step(0.0001), Some(0.01));
        for w in [0.0, -1.0, f64::NEG_INFINITY] {
            assert_eq!(m.terrain_contour_step(w), Some(0.01), "{w}");
        }
        // 100 m schneidet das Beispiel (24 bis 26 m) nicht: keine Linie
        assert_eq!(m.terrain_contour_step(f64::INFINITY), None);
        assert_eq!(m.terrain_contour_step(f64::NAN), None);
        assert_eq!(m.terrain_contour_step(1e9), None);
        assert_eq!(m.terrain_contour_step(50.0), None);
    }

    /// Ein ebenes Aufmaß hat Höhen, aber keine Höhenlinie: kein Abstand
    /// passt (W45-Nachprüfung B4).
    #[test]
    fn ebenes_aufmass_ohne_hoehenlinien() {
        let (mut m, _) = lindenhof_haus();
        let mut text = String::new();
        for i in 0..10 {
            for j in 0..10 {
                text.push_str(&format!("{} {} {} 24.5\n", i * 10 + j + 1, 5 * j, 5 * i));
            }
        }
        let a = sk_gelaende::lesen::lesen(text.as_bytes()).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(Terrain::from_survey(&a, "eben.txt", "", 32)));
        m.commit();
        let netz = &m.terrain().unwrap().mesh().netz;
        assert_eq!(sk_gelaende::linien::strecken(netz, 0.25), Some(0));
        for w in [0.25, 0.5, 1.0, 100.0] {
            assert_eq!(m.terrain_contour_step(w), None, "{w}");
            assert!(m.terrain_contours(w).is_empty(), "{w}");
        }
    }

    #[test]
    fn schluessel_und_rahmen() {
        for c in [Crs::Utm(32), Crs::Utm(33), Crs::GaussKrueger(3), Crs::Local] {
            assert_eq!(Crs::from_key(&c.key()), Some(c));
        }
        assert_eq!(Crs::from_key("utm31"), None);
        assert_eq!(Crs::from_key("gk6"), None);
        let f = SurveyFrame {
            e0: 467108.214,
            n0: 5878741.637,
            g: 7f64.to_radians(),
        };
        let (e, n) = f.to_survey(24000.0, 0.0);
        assert!((e - (467108.214 + 24.0 * 7f64.to_radians().cos())).abs() < 1e-9);
        let p = f.to_model(e, n);
        assert!((p[0] - 24000.0).abs() < 1e-6 && p[1].abs() < 1e-6);
    }

    /// Das Netz aus „Höhen“ bleibt beim Übernehmen (Review #123 W45 H5).
    /// Höhenlinien und Abstand je Stufe hängen am Netz und überstehen eine
    /// neue Lage, gezeichnet an der neuen Stelle (H1). Eine Abwahl rechnet
    /// ein neues Netz (B2).
    #[test]
    fn netz_und_linien_gemerkt() {
        let (mut m, _) = lindenhof_haus();
        let (t, ..) = Terrain::lesen(CSV, "aufmass.csv", "2026-10-12", 32).unwrap();
        let netz = t.mesh();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        assert!(Arc::ptr_eq(&netz, &m.terrain().unwrap().mesh()));
        let s = m.terrain_contour_step(0.25).unwrap();
        let linien = netz.hoehenlinien(s);
        let vorher = m.terrain_contours(s);
        assert!(!vorher.is_empty());
        m.begin("Verschieben");
        assert!(m.move_terrain([1000.0, 0.0]));
        m.commit();
        let netz2 = m.terrain().unwrap().mesh();
        assert!(Arc::ptr_eq(&netz, &netz2));
        assert!(Arc::ptr_eq(&linien, &netz2.hoehenlinien(s)));
        assert_eq!(m.terrain_contour_step(0.25), Some(s));
        let nachher = m.terrain_contours(s);
        assert_eq!(vorher.len(), nachher.len());
        let dx = nachher[0].1[0][0] - vorher[0].1[0][0];
        assert!((dx - 1000.0).abs() < 1e-6, "{dx}");
        let mut t = m.terrain().unwrap().clone();
        t.points[netz.index[0]].off = true;
        m.begin("Abwahl");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        let netz3 = m.terrain().unwrap().mesh();
        assert!(!Arc::ptr_eq(&netz, &netz3));
        assert_eq!(netz3.index.len() + 1, netz.index.len());
    }
}

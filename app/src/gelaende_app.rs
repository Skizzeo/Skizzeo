//! Leiste „Gelände“ in der App (plan-gelaendemodul.md §4.7, §4.8 und §5
//! G1, G1c): Knöpfe und Felder der Leiste, das Einlesen von Lageplan und
//! Höhenaufmaß, die Höhenlage, der Zuschnitt und das Verschieben und Drehen
//! des Geländes unter dem Haus. Jede Handlung ist ein Rückgängig-Schritt. Was die
//! Leiste zeigt, kommt aus dem Modell ([`leiste`]); die Zeilen baut
//! [`crate::ui`].

use crate::meldung::Meldung;
use crate::selection::de;
use crate::ui::{
    Field, FieldRow, GelaendeFeld, GelaendeKnopf, GelaendeLeiste, NHN, OBEN, RECHTS, UEBER,
};
use crate::App;
use sk_model::{Crs, HeightRef, Model, SitePlan, Terrain, TerrainFolgen};
use sk_platform::Surface;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

pub(crate) mod unterlage;
pub(crate) use unterlage::Unterlage;

/// Größtes Höhenaufmaß, das gelesen wird (Bytes); eins mit 100.000
/// Punkten hat als Punktliste etwa 5 MB, als DXF mit Nummerntexten etwa
/// 15 MB.
const MAX_DATEI: u64 = 64 << 20;

/// Größtes PDF, das als Lageplan gelesen wird (Bytes): so viel, wie der
/// PDF-Leser höchstens hält (#128, Review #139 H5).
const MAX_PDF: u64 = 256 << 20;

/// Dateiarten im Dialog „Lageplan einlesen“.
const FILTER_PDF: [(&str, &str); 2] = [
    ("Lageplan als PDF (*.pdf)", "*.pdf"),
    ("Alle Dateien (*.*)", "*.*"),
];

/// Dateiarten im Dialog; die Endung zählt nicht, Skizzeo liest den Inhalt.
const FILTER: [(&str, &str); 3] = [
    (
        "Punktliste oder DXF (*.csv, *.txt, *.dxf, …)",
        "*.csv;*.txt;*.xyz;*.asc;*.pkt;*.dat;*.koo;*.kor;*.coo;*.dxf",
    ),
    ("DXF des Vermessers (*.dxf)", "*.dxf"),
    ("Alle Dateien (*.*)", "*.*"),
];

/// So viele Hinweise des Einlesens stehen in der Leiste, der Rest als
/// Anzahl.
const HINWEISE: usize = 3;

/// Bereich der Felder für Verschieben (m) und Drehen (Grad).
const WEG: f64 = 500.0;
const WINKEL: f64 = 360.0;

/// Zustand der Leiste in der App.
#[derive(Default)]
pub(crate) struct Zustand {
    pub(crate) offen: bool,
    /// Verschieben oder Drehen zeigt seine Felder.
    tippen: Option<GelaendeKnopf>,
    /// Ergebnis des letzten Einlesens.
    meldung: Vec<String>,
    /// Hinweise über die ersten [`HINWEISE`] hinaus; `alle` zeigt sie
    /// (Bedienbarkeit 43.5).
    weitere: Vec<String>,
    alle: bool,
    fehler: Option<Meldung>,
    /// „Höhen“ geklickt: der Dateidialog öffnet nach dem Ereignis.
    pub(crate) hoehen_wanted: bool,
    /// „Lageplan“ geklickt: ebenso.
    pub(crate) lageplan_wanted: bool,
    /// Gelesener Lageplan und seine Datei, für „Flurstück übernehmen“;
    /// bleibt nach dem Übernehmen, so dass es nach Rückgängig wieder geht.
    plan: Option<(String, Arc<SitePlan>)>,
    /// Ein Lageplan wird im eigenen Faden gelesen (Review #139 B2).
    lesen: Option<(PathBuf, mpsc::Receiver<Result<SitePlan, Meldung>>)>,
    /// Ein Höhenaufmaß wird samt Netz im eigenen Faden gelesen (Review
    /// #152 B1), mit der Datei und der Zone, die dabei gilt.
    hoehen: Option<(PathBuf, u8, mpsc::Receiver<Ergebnis>)>,
    /// Wie es um die Unterlage im Modell steht (gelesen, fehlt).
    pub(crate) unterlage: unterlage::Stand,
    /// Modellstand der Leiste im Paneel; `neu` erzwingt einen Abgleich.
    stand: Option<u64>,
    pub(crate) neu: bool,
    /// Werkzeug „Fläche zeichnen“ (G3d).
    pub(crate) werkzeug: crate::gelaende_flaeche_app::Werkzeug,
    /// Gewählte Geländefläche und Modellstand, für die das Paneel
    /// „Eigenschaften“ steht.
    pub(crate) props: Option<(usize, u64)>,
    /// Baugruben, die das Netz nicht deckt (rote Zeile), aus dem eigenen
    /// Faden: Zahl, Modellstand dazu und der laufende Faden (Review #196
    /// B1: im UI-Faden je Loslassen bis 179 ms).
    ohne_netz: usize,
    geprueft: Option<u64>,
    pruefung: Option<(u64, mpsc::Receiver<Pruefung>)>,
    /// Sockel und Einbindung aus demselben Faden (H-196h: bei dichtem
    /// Aufmaß 8 ms je Leiste); `None`, solange er nichts geliefert hat,
    /// dann rechnet die Leiste selbst.
    folgen: Option<Option<TerrainFolgen>>,
}

/// Ergebnis des Fadens: Baugruben ohne Netz und die Folgen der Höhenlage.
type Pruefung = (usize, Option<TerrainFolgen>);

impl Zustand {
    /// Bleibt die Leiste, wie sie ist? Beim selben Modellstand, und im
    /// offenen Schritt, wenn es sie schon gibt; `neu` erzwingt den Abgleich.
    fn steht(&self, m: &Model) -> bool {
        !self.neu && (self.stand == Some(m.revision()) || (self.stand.is_some() && m.in_step()))
    }

    /// Leiste zu: die Eingabe und das Werkzeug „Fläche zeichnen“ fallen
    /// weg. Ein gelesener Lageplan, eine Datei, die gerade gelesen wird,
    /// und die Meldungen bleiben, bis ein neuer Plan kommt; ein Aufmaß
    /// wird zu Ende gelesen und übernommen (Bedienbarkeit 43.4).
    fn zu(&mut self) {
        self.offen = false;
        self.tippen = None;
        self.werkzeug.set_aktiv(false);
        self.neu = true;
    }

    /// Meldungen und Hinweise leeren.
    fn leeren(&mut self) {
        self.meldung.clear();
        self.weitere.clear();
        self.alle = false;
    }

    /// Bis zu [`HINWEISE`] Hinweise in die Meldung, den Rest hinter den
    /// Knopf „Alle Hinweise“.
    fn hinweise(&mut self, hinweise: &[String]) {
        let n = hinweise.len().min(HINWEISE);
        self.meldung.extend(hinweise[..n].iter().cloned());
        self.weitere = hinweise[n..].to_vec();
    }

    /// Baugruben ohne Netz bei offener Leiste mit Aufmaß im eigenen Faden
    /// zählen, je Modellstand einmal, nicht im offenen Schritt (Review #196
    /// B1); holt ein fertiges Ergebnis ab, auch das erst nach dem Schritt,
    /// sonst baute es die Leiste mitten im Zug neu (H-196f). Derselbe Faden
    /// rechnet Sockel und Einbindung (H-196h). `true`, wenn die Leiste sich
    /// damit ändern kann.
    pub(crate) fn baugruben(&mut self, m: &Model) -> bool {
        if m.in_step() {
            return false;
        }
        if let Some((rev, rx)) = &self.pruefung {
            let (n, folgen) = match rx.try_recv() {
                Ok((n, f)) => (n, Some(f)),
                Err(mpsc::TryRecvError::Empty) => return false,
                // abgebrochen: bis zum nächsten Schritt ohne rote Zeile
                Err(mpsc::TryRecvError::Disconnected) => (0, None),
            };
            self.geprueft = Some(*rev);
            self.pruefung = None;
            let anders = (n > 0) != (self.ohne_netz > 0) || folgen != self.folgen;
            self.ohne_netz = n;
            self.folgen = folgen;
            return anders;
        }
        let rev = m.revision();
        if !self.offen || self.geprueft == Some(rev) {
            return false;
        }
        if m.terrain().is_none_or(|t| t.points.is_empty()) {
            // ohne Höhen keine rote Zeile, nichts zu rechnen
            self.geprueft = Some(rev);
            let anders = self.ohne_netz > 0 || self.folgen.is_some();
            self.ohne_netz = 0;
            self.folgen = None;
            return anders;
        }
        let kopie = m.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // fällt weg, wenn die Leiste inzwischen neu begonnen hat
            let _ = tx.send((kopie.terrain_baugruben_ohne_netz(), kopie.terrain_folgen()));
        });
        self.pruefung = Some((rev, rx));
        false
    }

    /// Prüft gerade ein Faden die Baugruben?
    pub(crate) fn prueft(&self) -> bool {
        self.pruefung.is_some()
    }

    /// Ein Lageplan oder ein Höhenaufmaß wird gelesen.
    pub(crate) fn liest(&self) -> bool {
        self.lesen.is_some() || self.hoehen.is_some()
    }
}

/// Statuszeile, wenn ein Lageplan oder ein Aufmaß bei geschlossener Leiste
/// fertig gelesen ist (43.4).
pub const GELESEN_PLAN: &str = "Lageplan gelesen; mehr unter „Gelände“";
pub const GELESEN_HOEHEN: &str = "Höhenaufmaß eingelesen; mehr unter „Gelände“";

/// Meldung nach „Gelände entfernen“; „ruhen“ las sich unklar (Bedienbarkeit,
/// später zu #196).
const ENTFERNT: &str = "Gelände entfernt, das Haus steht wieder auf der Platte; \
    Geländeflächen bleiben gespeichert und gelten wieder mit dem nächsten Aufmaß. \
    Strg+Z holt es zurück.";

/// Name eines Knopfs im Tooltip (Zeile in `hilfe.txt`, `[knopf]`).
pub(crate) fn knopf_name(k: GelaendeKnopf) -> &'static str {
    use sk_model::Show;
    match k {
        GelaendeKnopf::Leiste => "Gelände",
        GelaendeKnopf::Lageplan => "Lageplan",
        GelaendeKnopf::Uebernehmen => "Flurstück übernehmen",
        GelaendeKnopf::Hoehen => "Höhen",
        GelaendeKnopf::Verschieben => "Gelände verschieben",
        GelaendeKnopf::Drehen => "Gelände drehen",
        GelaendeKnopf::Ausrichten => "An Grenze ausrichten",
        GelaendeKnopf::Ueber => "Höhenlage über Gelände",
        GelaendeKnopf::Absolut => "Höhenlage absolut",
        GelaendeKnopf::Zuschnitt(Show::All) => "Umgebung ganz",
        GelaendeKnopf::Zuschnitt(Show::Fade) => "Umgebung blass",
        GelaendeKnopf::Zuschnitt(Show::Parcel) => "Umgebung nicht",
        GelaendeKnopf::AmHaus => "Gelände ans Haus",
        GelaendeKnopf::Flaeche => "Fläche zeichnen",
        GelaendeKnopf::Befestigt => "Fläche befestigt",
        GelaendeKnopf::Loeschen => "Fläche löschen",
        GelaendeKnopf::Hinweise => "Alle Hinweise",
        GelaendeKnopf::Entfernen => "Gelände entfernen",
    }
}

fn feld(
    f: GelaendeFeld,
    label: &'static str,
    value: f64,
    r: (f64, f64),
    einheit: Option<&'static str>,
) -> FieldRow {
    FieldRow {
        field: Field::Gelaende(f),
        label,
        value,
        min: r.0,
        max: r.1,
        zero: false,
        einheit,
    }
}

/// Inhalt der Leiste zum Modell.
pub(crate) fn leiste(m: &Model, z: &Zustand) -> GelaendeLeiste {
    use unterlage::Stand;
    // schon übernommen: kein Knopf, nach Rückgängig wieder (Review #139 H3)
    let da = |src: &String| {
        let u = m.terrain().and_then(|t| t.underlay.as_ref());
        u.is_some_and(|u| u.src == *src)
    };
    let mut l = GelaendeLeiste {
        meldung: z.meldung.clone(),
        weitere: z.weitere.len(),
        alle: z.alle,
        fehler: z.fehler.clone(),
        uebernehmen: z
            .plan
            .as_ref()
            .is_some_and(|(src, p)| p.parcel.is_some() && !da(src)),
        ..Default::default()
    };
    if z.alle {
        l.meldung.extend(z.weitere.iter().cloned());
    }
    if z.lesen.is_some() {
        l.meldung.push("Lageplan wird gelesen …".into());
    }
    if z.hoehen.is_some() {
        l.meldung.push("Höhenaufmaß wird gelesen …".into());
    }
    match &z.unterlage {
        Stand::Gut => {}
        Stand::Liest => l.meldung.push("Unterlage: Lageplan wird gelesen …".into()),
        Stand::Fehlt(n) => l.meldung.push(format!(
            "Unterlage: „{n}“ nicht gefunden, weder am alten Ort noch beim Projekt; die Grenze bleibt."
        )),
        Stand::Kaputt(n) => l.meldung.push(format!(
            "Unterlage: „{n}“ ist kein lesbarer Lageplan mehr; die Grenze bleibt."
        )),
    }
    // das Werkzeug geht auch ohne Gelände (I1)
    l.zeichnen = z.werkzeug.aktiv;
    l.ecken = z.werkzeug.ecken().len();
    let Some(t) = m.terrain() else {
        return l;
    };
    // nach Strg+Z ist das Aufmaß wieder da (H-196c)
    l.meldung.retain(|s| s != ENTFERNT);
    let ringe = t.boundary_rings();
    l.aufmass = true;
    l.hoehen = !t.points.is_empty();
    l.angenommen = t.assumed();
    l.grenze = !ringe.is_empty();
    l.absolut = t.href == HeightRef::Abs;
    l.zuschnitt = Some(t.show());
    l.tippen = z.tippen;
    // aus dem Faden; bis er nach einem Schritt fertig ist, der Stand davor
    l.deckt_nicht = l.hoehen && z.ohne_netz > 0;
    match z.tippen {
        Some(GelaendeKnopf::Verschieben) => {
            let r = (-WEG, WEG);
            l.felder
                .push(feld(GelaendeFeld::Rechts, RECHTS, 0.0, r, Some("m")));
            l.felder
                .push(feld(GelaendeFeld::Oben, OBEN, 0.0, r, Some("m")));
        }
        Some(GelaendeKnopf::Drehen) => {
            let r = (-WINKEL, WINKEL);
            l.felder
                .push(feld(GelaendeFeld::Winkel, "Winkel", 0.0, r, Some("°")));
        }
        _ => {}
    }
    let nhn = m.terrain_nhn();
    if l.absolut {
        let h = nhn.map_or(0.0, |h| (h * 1000.0).round() / 1000.0);
        let r = sk_model::NHN;
        l.felder.push(feld(GelaendeFeld::Nhn, NHN, h, r, Some("m")));
    } else {
        let v = m.terrain_offset_of(m.level_building());
        let r = sk_model::MAX_TERRAIN_OFFSET;
        l.felder
            .push(feld(GelaendeFeld::Versatz, UEBER, v, (-r, r), None));
        if let Some(h) = nhn {
            l.hoehenlage.push(format!("±0,00 = {} m ü. NHN", de(h, 2)));
        }
    }
    // aus dem Faden wie die rote Zeile; vor dem ersten Ergebnis hier
    if let Some(f) = z.folgen.unwrap_or_else(|| m.terrain_folgen()) {
        let cm = |v: f64| de(v / 10.0, 1);
        l.hoehenlage.push(format!(
            "Sockel {} bis {} cm",
            cm(f.sockel.0),
            cm(f.sockel.1)
        ));
        let m_ = |v: f64| de(v / 1000.0, 2);
        let frost = if f.frostfrei { ", frostfrei" } else { "" };
        l.hoehenlage.push(format!(
            "Einbindung {} bis {} m{frost}",
            m_(f.einbindung.0),
            m_(f.einbindung.1)
        ));
        if f.sockel.0 < -0.5 {
            l.steckt = Some(Meldung::mit(
                "Das Haus steckt bis {} cm im Gelände.",
                &[&cm(-f.sockel.0)],
            ));
        }
    }
    if l.grenze {
        l.grundstueck = Some(format!("Grundstück {} m²", de(t.parcel_area(), 2)));
    }
    // zu viele Höhenlinien: gröber oder keine, mit Hinweis (Auflage H1)
    let wunsch = crate::gelaende_netz::STUFE;
    let meter = |s: f64| {
        let t = de(s, 2);
        t.trim_end_matches('0').trim_end_matches(',').to_string()
    };
    l.hoehenlinien = match m.terrain_contour_step(wunsch) {
        Some(s) if s == wunsch => None,
        Some(s) => Some(format!(
            "Höhenlinien alle {} m, bei {} m wären es zu viele",
            meter(s),
            meter(wunsch)
        )),
        None if m.terrain_contour_pieces(wunsch) == Some(0) => {
            Some("Höhenlinien: keine, das Gelände ist eben".into())
        }
        None => Some("Höhenlinien: zu viele, keine gezeigt".into()),
    };
    l
}

/// Wie ein neues Aufmaß zum Lageplan passt.
enum Einpassen {
    /// Kein Lageplan allein oder schon im selben System.
    Nicht,
    /// Neben einem Aufmaß mit Lageplan: das neue ersetzt beide.
    Ohne,
    /// Eingepasst, mit der größten Abweichung (m).
    Mit(sk_model::Affine, f64),
    /// Passt nicht; der Satz sagt, was mit der Grenze geschieht.
    Geht(&'static str),
}

/// Was die Leiste nach dem Lesen eines Lageplans sagt (ohne seine
/// Hinweise, die [`Zustand::hinweise`] anhängt).
fn lageplan_saetze(p: &SitePlan) -> Vec<String> {
    let mut v = Vec::new();
    let mst = |s: f64| format!("1:{}", s.round());
    match (p.fit, p.scale, p.residual) {
        (Some(_), Some(s), Some(r)) => v.push(format!(
            "Lageplan {}, Lage aus den Gitterkreuzen (Restfehler {} cm)",
            mst(s),
            de(r * 100.0, 1)
        )),
        (Some(_), Some(s), None) => v.push(format!("Lageplan {}, Norden oben", mst(s))),
        _ => v.push("Maßstab und Lage nicht erkannt".into()),
    }
    match (&p.parcel, &p.number) {
        // gemessen, nicht amtlich (Bedienbarkeit, später zu Durchgang 43)
        (Some((nr, _, Some(a))), _) => v.push(format!(
            "Flurstück {nr}, {} m² (aus der Zeichnung)",
            de(*a, 2)
        )),
        (Some((nr, _, None)), _) => v.push(format!("Flurstück {nr}")),
        (None, Some(nr)) => v.push(format!(
            "Flurstück {nr}: keine geschlossene Grenze um die Nummer gefunden"
        )),
        (None, None) => v.push("Keine Flurstücksnummer im Kopf gefunden".into()),
    }
    v
}

/// Heutiges Datum (UTC) als `JJJJ-MM-TT` (Vermerk `[terrain] read=`):
/// ohne Zeitzonendaten zählt der Tag nach UTC, kurz nach Mitternacht steht
/// also noch der Vortag da.
fn heute() -> String {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = crate::autosave::civil_from_days(s.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Datei lesen, höchstens `max` Bytes, vor dem Lesen geprüft.
fn datei(p: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    let gross = || std::io::Error::new(std::io::ErrorKind::InvalidData, "zu groß");
    if std::fs::metadata(p)?.len() > max {
        return Err(gross());
    }
    let b = std::fs::read(p)?;
    if b.len() as u64 > max {
        return Err(gross());
    }
    Ok(b)
}

/// Satz, wenn das Flurstück aus dem Lageplan die Grenze aus der Datei
/// (`eigene`, z. B. aus der DXF) ersetzt hat (Review #152 H-152a).
fn grenze_ersetzt(eigene: &[Vec<(f64, f64)>], m: &Model) -> Option<&'static str> {
    let t = m.terrain()?;
    (!eigene.is_empty() && t.underlay.is_some() && t.parcels != eigene).then_some(
        "Grundstück bleibt das Flurstück aus dem Lageplan; die Grenze aus der Datei ist nicht übernommen.",
    )
}

/// Ein gelesenes Höhenaufmaß: Gelände mit Netz, Satz und Hinweise.
type Gelesen = (Terrain, String, Vec<String>);

/// Was der Faden von [`App::hoehen_einlesen`] schickt.
type Ergebnis = Result<Gelesen, Meldung>;

/// Punktliste oder DXF `p` lesen und vernetzen (im Faden von
/// [`App::hoehen_einlesen`]); das Netz bleibt am Gelände gemerkt.
fn hoehen_lesen(p: &Path, zone: u8) -> Ergebnis {
    let name = p
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let roh = datei(p, MAX_DATEI)
        .map_err(|e| Meldung::aus_io("Höhenaufmaß nicht eingelesen", "Einlesen", p, &e))?;
    Terrain::lesen(&roh, &name, &heute(), zone)
        .map_err(|e| Meldung::mit("Höhenaufmaß nicht eingelesen: {}", &[&e]))
}

/// PDF `p` als Lageplan lesen (im Faden von [`App::lageplan_einlesen`]).
fn lageplan_lesen(p: &Path, zone: u8) -> Result<SitePlan, Meldung> {
    let roh = datei(p, MAX_PDF)
        .map_err(|e| Meldung::aus_io("Lageplan nicht eingelesen", "Einlesen", p, &e))?;
    SitePlan::read(&roh, zone).map_err(|e| Meldung::mit("Lageplan nicht eingelesen: {}", &[&e]))
}

impl App {
    /// Knopf „Gelände“ oder einer seiner Leiste.
    pub(crate) fn gelaende_klick(&mut self, k: GelaendeKnopf) {
        use sk_model::HeightRef as H;
        use GelaendeKnopf as K;
        let z = &mut self.platte.leiste;
        z.neu = true;
        match k {
            K::Leiste if z.offen => z.zu(),
            K::Leiste => {
                z.offen = true;
                // Die Leiste steht, wo sonst die Wandvorgaben stehen
                self.tool.set_enabled(false);
                self.nord.set_aktiv(false);
                self.refresh_cursor();
            }
            K::Lageplan => z.lageplan_wanted = true,
            K::Uebernehmen => self.flurstueck_uebernehmen(),
            K::Hoehen => z.hoehen_wanted = true,
            K::Hinweise => z.alle = !z.alle,
            K::Verschieben | K::Drehen => z.tippen = (z.tippen != Some(k)).then_some(k),
            K::Ausrichten => {
                self.gelaende_schritt("Gelände an Grenze ausrichten", |m| m.align_terrain());
            }
            K::Ueber => {
                self.gelaende_schritt("Höhenlage über Gelände", |m| m.set_terrain_href(H::High));
            }
            K::Absolut => {
                self.gelaende_schritt("Höhenlage absolut", |m| m.set_terrain_href(H::Abs));
            }
            K::Zuschnitt(s) => {
                self.gelaende_schritt("Umgebung zeigen", |m| m.set_terrain_show(s));
            }
            K::AmHaus | K::Flaeche | K::Befestigt | K::Loeschen => self.flaeche_knopf(k),
            K::Entfernen => self.aufmass_entfernen(),
        }
        self.gelaende_sync();
        self.redraw = true;
    }

    /// Aufmaß samt Lageplan und Grundstück entfernen, ein eigener Schritt
    /// (Bedienbarkeit 42.6); die Leiste vergisst das letzte Einlesen.
    fn aufmass_entfernen(&mut self) {
        // nimmt auch Lageplan und Grundstück: „Gelände entfernen“ (Bedienbarkeit,
        // später zu #196)
        if !self.gelaende_schritt("Gelände entfernen", |m| m.set_terrain(None)) {
            return;
        }
        let z = &mut self.platte.leiste;
        z.leeren();
        z.fehler = None;
        z.tippen = None;
        z.werkzeug.set_aktiv(false);
        z.meldung.push(ENTFERNT.into());
        // ohne Aufmaß ruhen die Flächen (H-196a)
        self.flaeche_waehlen(None);
        self.quantity.dirty = true;
    }

    /// Baugruben ohne Netz im eigenen Faden zählen ([`Zustand::baugruben`]);
    /// ändert sich die rote Zeile, folgt die Leiste und ein Bild (`true`).
    pub(crate) fn baugruben_pruefen(&mut self) -> bool {
        let anders = self.platte.leiste.baugruben(self.scene.model());
        if anders {
            self.platte.leiste.neu = true;
            self.gelaende_sync();
        }
        anders
    }

    /// Prüft gerade ein Faden die Baugruben? Im offenen Schritt wartet sein
    /// Ergebnis aufs Loslassen, die Hauptschleife muss nicht nachfragen.
    pub(crate) fn baugruben_rechnen(&self) -> bool {
        self.platte.leiste.prueft() && !self.scene.model().in_step()
    }

    /// Feld der Leiste übernommen (ein Schritt).
    pub(crate) fn gelaende_feld(&mut self, f: GelaendeFeld, v: f64) {
        if self.flaeche_feld(f, v) {
            return;
        }
        match f {
            GelaendeFeld::Versatz => {
                if let Some(b) = self.scene.model().level_building() {
                    self.gelaende_schritt("Höhenlage", |m| m.set_terrain_offset_of(b, v));
                }
            }
            GelaendeFeld::Nhn => {
                let h = (v * 1000.0).round() / 1000.0;
                self.gelaende_schritt("Höhenlage", |m| m.set_terrain_nhn(h));
            }
            GelaendeFeld::Rechts | GelaendeFeld::Oben => {
                let d = if f == GelaendeFeld::Rechts {
                    [v * 1000.0, 0.0]
                } else {
                    [0.0, v * 1000.0]
                };
                self.gelaende_schritt("Gelände verschieben", |m| m.move_terrain(d));
            }
            GelaendeFeld::Winkel => {
                if let Some(c) = self.scene.model().house_center() {
                    let um = v.to_radians();
                    self.gelaende_schritt("Gelände drehen", |m| m.rotate_terrain(c, um));
                }
            }
            GelaendeFeld::Hoehe
            | GelaendeFeld::Boeschung
            | GelaendeFeld::Breite
            | GelaendeFeld::Gefaelle
            | GelaendeFeld::Andecken
            | GelaendeFeld::Faktor => {}
        }
        self.platte.leiste.neu = true;
        self.gelaende_sync();
    }

    /// Eine Änderung am Gelände als ein Schritt; Darstellung und
    /// Geschossmanager folgen. `true`, wenn sich etwas geändert hat.
    fn gelaende_schritt(
        &mut self,
        label: &'static str,
        f: impl FnOnce(&mut Model) -> bool,
    ) -> bool {
        let neu = self.scene.edit_model(label, f);
        if neu {
            self.upload_model();
            self.sync_levels();
        }
        neu
    }

    /// Dialoge, die nach einem Ereignis öffnen (sie brauchen das Fenster):
    /// Mengenfenster, „Lageplan einlesen“ und „Höhenaufmaß einlesen“.
    pub(crate) fn offene_dialoge(&mut self, surface: &Surface) {
        if std::mem::take(&mut self.quantity_wanted) {
            self.open_quantity(surface);
        }
        if std::mem::take(&mut self.platte.leiste.lageplan_wanted) {
            if let Some(p) = surface.open_dialog("Lageplan einlesen", &FILTER_PDF) {
                self.lageplan_einlesen(&p);
            }
        }
        if std::mem::take(&mut self.platte.leiste.hoehen_wanted) {
            if let Some(p) = surface.open_dialog("Höhenaufmaß einlesen", &FILTER) {
                self.hoehen_einlesen(&p);
            }
        }
    }

    /// Punktliste oder DXF `p` im eigenen Faden lesen und vernetzen (Review
    /// #152: 100.000 Punkte hielten sonst die Oberfläche an); übernommen
    /// wird beim Abholen ([`App::hoehen_abholen`]).
    pub(crate) fn hoehen_einlesen(&mut self, p: &Path) {
        let zone = self.scene.model().location().utm.map_or(32, |u| u.zone);
        let pfad = p.to_path_buf();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // fällt weg, wenn die Leiste inzwischen zu ist
            let _ = tx.send(hoehen_lesen(&pfad, zone));
        });
        let z = &mut self.platte.leiste;
        z.neu = true;
        z.leeren();
        z.fehler = None;
        z.hoehen = Some((p.to_path_buf(), zone, rx));
        self.gelaende_sync();
        self.redraw = true;
    }

    /// Ein fertig gelesenes Höhenaufmaß übernehmen; `true`, wenn eins kam
    /// (dann folgt ein Bild). Es wartet, solange ein Zug läuft (siehe
    /// [`darf_abholen`]). Ein Ergebnis, das nicht mehr gilt, fällt weg: Ein
    /// neues Einlesen verwirft den Empfänger, die Leiste zu nicht (sie
    /// liest fertig); hat das Modell inzwischen eine andere UTM-Zone (die
    /// Vorgabe für Dateien ohne Zonenkennzahl), wird neu gelesen. Sonst
    /// hängt das Gelesene nicht am Modell.
    pub(crate) fn hoehen_abholen(&mut self) -> bool {
        if !darf_abholen(self.im_zug(), self.scene.model()) {
            return false;
        }
        let Some((_, zone, rx)) = &self.platte.leiste.hoehen else {
            return false;
        };
        let r = match rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => Err(Meldung::satz(
                "Höhenaufmaß nicht eingelesen: Das Lesen brach ab.",
            )),
        };
        let jetzt = self.scene.model().location().utm.map_or(32, |u| u.zone);
        let zone = *zone;
        let (p, _, _) = self.platte.leiste.hoehen.take().expect("Lesen");
        if r.is_ok() && jetzt != zone {
            self.hoehen_einlesen(&p);
            return false;
        }
        self.hoehen_gelesen(r);
        true
    }

    /// Das gelesene Aufmaß als Schritt übernehmen. Was erkannt wurde, steht
    /// in der Leiste, ebenso ein Fehler.
    fn hoehen_gelesen(&mut self, gelesen: Ergebnis) {
        self.gelesen_bei_zu(gelesen.as_ref().map(|_| GELESEN_HOEHEN));
        let m = self.scene.model();
        let mit_unterlage = m.terrain().is_some_and(|t| t.underlay.is_some());
        let z = &mut self.platte.leiste;
        z.neu = true;
        match gelesen {
            Err(e) => z.fehler = Some(e),
            Ok((t, satz, hinweise)) => {
                z.meldung.push(satz);
                z.hinweise(&hinweise);
                let eigene = t.parcels.clone();
                match self.lageplan_einpassen(&t) {
                    Einpassen::Nicht => {
                        self.gelaende_schritt("Höhenaufmaß eingelesen", |m| m.import_terrain(t));
                    }
                    Einpassen::Mit(fit, fehler) => {
                        self.platte.leiste.meldung.push(format!(
                            "Lageplan über die Grenzpunkte eingepasst, Abweichung bis {} cm",
                            de(fehler * 100.0, 1)
                        ));
                        self.gelaende_schritt("Höhenaufmaß eingelesen", |m| {
                            m.import_terrain_fitted(t, fit)
                        });
                    }
                    Einpassen::Geht(warum) => {
                        self.platte.leiste.meldung.push(warum.into());
                        self.gelaende_schritt("Höhenaufmaß eingelesen", |m| m.import_terrain(t));
                    }
                    Einpassen::Ohne => {
                        self.gelaende_schritt("Höhenaufmaß eingelesen", |m| m.import_terrain(t));
                        // nicht stillschweigend (Review #139 H4)
                        let m = self.scene.model();
                        if mit_unterlage && m.terrain().is_some_and(|t| t.underlay.is_none()) {
                            self.platte.leiste.meldung.push(
                                "Die Unterlage aus dem Lageplan entfällt mit dem neuen Aufmaß; „Lageplan“ und „Flurstück übernehmen“ legen sie neu an.".into(),
                            );
                        }
                    }
                }
                if let Some(satz) = grenze_ersetzt(&eigene, self.scene.model()) {
                    self.platte.leiste.meldung.push(satz.into());
                }
            }
        }
        self.gelaende_sync();
        self.redraw = true;
    }

    /// Wie ein neues Aufmaß zum Grundstück aus dem Lageplan passt: Liegt
    /// nur ein Lageplan vor, in einem anderen System, wird er über die
    /// Grenzpunkte mit gleicher Nummer eingepasst.
    fn lageplan_einpassen(&mut self, t: &Terrain) -> Einpassen {
        let m = self.scene.model();
        let Some(alt) = m.terrain() else {
            return Einpassen::Nicht;
        };
        if alt.underlay.is_none()
            || matches!((alt.crs, t.crs), (Crs::Utm(a), Crs::Utm(b)) if a == b)
        {
            return Einpassen::Nicht;
        }
        if !alt.points.is_empty() {
            return Einpassen::Ohne;
        }
        // Der Plan ist meist längst gelesen; sonst wird hier auf ihn
        // gewartet, denn ohne ihn passt nichts ein
        let u = &mut self.platte.unterlage;
        let plan = u.plan(m).or_else(|| {
            u.warten();
            u.plan(m)
        });
        match plan.and_then(|p| p.fit_to(&t.points)) {
            Some((fit, fehler)) => Einpassen::Mit(fit, fehler),
            None => Einpassen::Geht(
                "Lageplan und Aufmaß haben keine zwei Grenzpunkte mit gleicher Nummer; die Grenze aus dem Lageplan entfällt.",
            ),
        }
    }

    /// PDF `p` als Lageplan lesen, im eigenen Faden (Review #139 B2: im
    /// UI-Faden stand das Bild bis zu 250 ms). Ist er gelesen
    /// ([`App::lageplan_abholen`]), stehen Maßstab, Lage und Flurstück in
    /// der Leiste, „Flurstück übernehmen“ macht es zur Grenze.
    pub(crate) fn lageplan_einlesen(&mut self, p: &Path) {
        let zone = self.scene.model().location().utm.map_or(32, |u| u.zone);
        let pfad = p.to_path_buf();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // fällt weg, wenn die Leiste inzwischen zu ist
            let _ = tx.send(lageplan_lesen(&pfad, zone));
        });
        let z = &mut self.platte.leiste;
        z.neu = true;
        z.leeren();
        z.fehler = None;
        z.plan = None;
        z.lesen = Some((p.to_path_buf(), rx));
        self.gelaende_sync();
        self.redraw = true;
    }

    /// Einen fertig gelesenen Lageplan in die Leiste übernehmen; `true`,
    /// wenn einer kam (dann folgt ein Bild).
    pub(crate) fn lageplan_abholen(&mut self) -> bool {
        let z = &mut self.platte.leiste;
        let Some((_, rx)) = &z.lesen else {
            return false;
        };
        let r = match rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => Err(Meldung::satz(
                "Lageplan nicht eingelesen: Das Lesen brach ab.",
            )),
        };
        let (p, _) = z.lesen.take().expect("Lesen");
        self.lageplan_gelesen(&p, r);
        true
    }

    fn lageplan_gelesen(&mut self, p: &Path, r: Result<SitePlan, Meldung>) {
        self.gelesen_bei_zu(r.as_ref().map(|_| GELESEN_PLAN));
        let z = &mut self.platte.leiste;
        z.neu = true;
        match r {
            Err(e) => z.fehler = Some(e),
            Ok(plan) => {
                z.meldung = lageplan_saetze(&plan);
                z.hinweise(&plan.hints);
                z.plan = Some((p.to_string_lossy().into_owned(), Arc::new(plan)));
            }
        }
        self.gelaende_sync();
        self.redraw = true;
    }

    /// Kam das Ergebnis bei geschlossener Leiste, sagt es die Statuszeile;
    /// der Rest steht in der Leiste, wenn sie wieder aufgeht (43.4).
    fn gelesen_bei_zu(&mut self, r: Result<&'static str, &Meldung>) {
        if self.platte.leiste.offen {
            return;
        }
        let m = match r {
            Ok(satz) => Meldung::satz(satz),
            Err(e) => e.clone(),
        };
        self.status(m, crate::NOTICE_TIME);
    }

    /// Das Flurstück des gelesenen Lageplans übernehmen (ein Schritt). Mit
    /// Aufmaß in einem anderen System wird über die Grenzpunkte mit
    /// gleicher Nummer eingepasst. Der Plan bleibt in der Leiste, so dass
    /// es nach Rückgängig wieder geht; die Unterlage merkt ihn erst, wenn
    /// das Übernehmen gelang (Review #139 H3).
    fn flurstueck_uebernehmen(&mut self) {
        let Some((src, plan)) = self.platte.leiste.plan.clone() else {
            return;
        };
        let m = self.scene.model();
        let mut fit = None;
        let mut satz = None;
        let gleich = |t: &Terrain| matches!((plan.fit, t.crs), (Some((Crs::Utm(a), _)), Crs::Utm(b)) if a == b);
        let z = &mut self.platte.leiste;
        z.neu = true;
        z.fehler = None;
        if let Some(t) = m.terrain().filter(|t| !t.points.is_empty() && !gleich(t)) {
            match plan.fit_to(&t.points) {
                Some((f, fehler)) => {
                    fit = Some((t.crs, f));
                    satz = Some(format!(
                        "Über die Grenzpunkte des Aufmaßes eingepasst, Abweichung bis {} cm",
                        de(fehler * 100.0, 1)
                    ));
                }
                None => {
                    z.fehler = Some(Meldung::satz(
                        "Lageplan und Aufmaß liegen in verschiedenen Systemen und haben keine zwei Grenzpunkte mit gleicher Nummer.",
                    ));
                    return;
                }
            }
        }
        if plan.fit.is_none() && fit.is_none() {
            z.fehler = Some(Meldung::satz(
                "Maßstab und Lage des Lageplans sind nicht erkannt. Beim Vermessungsamt einen Auszug als Vektor-PDF anfordern, Maßstab 1:500 oder 1:1000.",
            ));
            return;
        }
        let ok = self.gelaende_schritt("Flurstück übernommen", |m| {
            m.import_parcel(&plan, &src, fit)
        });
        let z = &mut self.platte.leiste;
        z.leeren();
        if ok {
            self.platte
                .unterlage
                .merken(self.scene.model(), plan.clone());
            let nr = plan.parcel.as_ref().map_or("", |p| p.0.as_str());
            z.meldung.push(format!("Flurstück {nr} übernommen"));
            z.meldung.extend(satz);
        } else {
            z.fehler = Some(Meldung::satz(
                "Flurstück nicht übernommen: Seine Grenze ergibt keine Fläche, oder Grenze und Unterlage sind schon dieselben.",
            ));
        }
    }

    /// Die Leiste sagt, ob die Unterlage gelesen wird oder fehlt; neu nur,
    /// wenn sich das ändert.
    pub(crate) fn unterlage_stand(&mut self) {
        let s = self.platte.unterlage.stand(self.scene.model());
        let z = &mut self.platte.leiste;
        if z.unterlage != s {
            z.unterlage = s;
            z.neu = true;
            self.gelaende_sync();
            self.redraw = true;
        }
    }

    /// Paneel „Werkzeuge“ an die Leiste angleichen; ein Zeichenwerkzeug
    /// oder der Nordpfeil schließt sie. Im offenen Schritt (Wand, Ebene,
    /// Gelände ziehen) bleibt sie auf dem Stand davor und folgt beim
    /// Loslassen, wie das Paneel „Eigenschaften“ (Review #129 B2: sonst je
    /// Mausbewegung 8,2 ms bei 1000 Häusern, 1 s bei 4000).
    pub(crate) fn gelaende_sync(&mut self) {
        self.flaeche_zeile_ende();
        let z = &mut self.platte.leiste;
        let zeichnet = self.tool.enabled || self.tool.oeffnung.is_some() || self.nord.aktiv;
        if z.offen && zeichnet {
            z.zu();
        }
        let m = self.scene.model();
        if z.steht(m) {
            return;
        }
        z.neu = false;
        z.stand = Some(m.revision());
        let soll = z.offen.then(|| leiste(m, z));
        if soll != self.ui.gelaende {
            self.ui.gelaende = soll;
            self.overlay_dirty = true;
        }
    }
}

/// Ob ein fertig gelesenes Aufmaß jetzt ins Modell darf: nicht während
/// eines Zugs der Werkzeuge und nicht in einem offenen Schritt eines
/// anderen Zugs (Wand, Raumstempel, …). Bei geschlossener Leiste wird
/// weitergezeichnet, das Aufmaß käme sonst mitten in dessen Schritt und
/// Rückgängig-Verlauf (43.4, Review #200 B1).
fn darf_abholen(im_zug: bool, m: &Model) -> bool {
    !im_zug && !m.in_step()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    const CSV: &[u8] =
        include_bytes!("../../crates/sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.csv");

    /// Haus des Beispiels „Am Lindenhof 7“: x 7 bis 17 m, y 6 bis 14 m.
    fn lindenhof() -> Model {
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            vec3(7000.0, 6000.0, 0.0),
            vec3(7000.0, 14000.0, 0.0),
            vec3(17000.0, 14000.0, 0.0),
            vec3(17000.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        m
    }

    /// Zu viele Höhenlinien bei 0,25 m: gröber, und die Leiste sagt es
    /// (Review #121, Auflage H1).
    #[test]
    fn zu_viele_hoehenlinien() {
        let mut m = lindenhof();
        // jede zweite Reihe 400 m höher: bei 0,25 m 2,7 Mio. Stücke
        let mut text = String::new();
        for i in 0..30 {
            for j in 0..30 {
                let h = 10.1 + (i % 2) as f64 * 400.0;
                text.push_str(&format!("{} {j} {i} {h}\n", i * 30 + j + 1));
            }
        }
        let (t, ..) = Terrain::lesen(text.as_bytes(), "x.txt", "2026-10-13", 32).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        let l = leiste(&m, &Zustand::default());
        assert_eq!(
            l.hoehenlinien.as_deref(),
            Some("Höhenlinien alle 1 m, bei 0,25 m wären es zu viele")
        );
        assert!(crate::gelaende_netz::Gelaende::aus(&m).is_some());
    }

    /// Leiste zu: eine Datei, die gerade gelesen wird, und die Meldungen
    /// bleiben; Eingabe und Werkzeug fallen weg (Bedienbarkeit 43.4).
    #[test]
    fn abholen_wartet_auf_den_schritt() {
        let mut m = lindenhof();
        assert!(darf_abholen(false, &m));
        assert!(!darf_abholen(true, &m), "im Zug");
        m.begin("Raum stempeln");
        assert!(!darf_abholen(false, &m), "offener Schritt");
        m.commit();
        assert!(darf_abholen(false, &m), "nach dem Schritt");
    }

    #[test]
    fn zu_behaelt_das_gelesene() {
        let mut z = Zustand {
            offen: true,
            ..Default::default()
        };
        let (_tx, rx) = mpsc::channel();
        z.lesen = Some((PathBuf::from("lageplan.pdf"), rx));
        let (_tx2, rx2) = mpsc::channel();
        z.hoehen = Some((PathBuf::from("punkte.txt"), 32, rx2));
        z.meldung.push("Maßstab 1:500 erkannt".into());
        z.fehler = Some(Meldung::satz("Fehler"));
        z.werkzeug.set_aktiv(true);
        z.zu();
        assert!(!z.offen && z.neu && !z.werkzeug.aktiv && z.tippen.is_none());
        assert!(z.liest() && z.lesen.is_some() && z.hoehen.is_some());
        assert_eq!(z.meldung, ["Maßstab 1:500 erkannt"]);
        assert!(z.fehler.is_some());
    }

    /// Im offenen Schritt bleibt die Leiste stehen und folgt beim
    /// Loslassen (Review #129 B2); „neu“ erzwingt den Abgleich.
    #[test]
    fn leiste_steht_im_schritt() {
        let mut m = lindenhof();
        let mut z = Zustand::default();
        assert!(!z.steht(&m), "noch keine");
        z.stand = Some(m.revision());
        assert!(z.steht(&m));
        let run = m.runs().ids().next().unwrap();
        let pts = m.runs().get(run).unwrap().points.clone();
        let moved: Vec<_> = pts.iter().map(|p| *p + vec3(500.0, 0.0, 0.0)).collect();
        m.begin("Wand verschieben");
        assert!(m.set_run_points(run, &moved).is_some());
        assert_ne!(z.stand, Some(m.revision()));
        assert!(z.steht(&m), "im Schritt");
        z.neu = true;
        assert!(!z.steht(&m), "neu");
        z.neu = false;
        m.commit();
        assert!(!z.steht(&m), "nach dem Loslassen");
    }

    /// Ebenes Aufmaß: keine Höhenlinien, aber nicht „zu viele“
    /// (W45-Nachprüfung B4).
    #[test]
    fn ebenes_aufmass_ohne_hoehenlinien() {
        let mut m = lindenhof();
        let mut text = String::new();
        for i in 0..30 {
            for j in 0..30 {
                text.push_str(&format!("{} {j} {i} 10.1\n", i * 30 + j + 1));
            }
        }
        let (t, ..) = Terrain::lesen(text.as_bytes(), "x.txt", "2026-10-13", 32).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        assert_eq!(m.terrain_contour_step(crate::gelaende_netz::STUFE), None);
        let l = leiste(&m, &Zustand::default());
        assert_eq!(
            l.hoehenlinien.as_deref(),
            Some("Höhenlinien: keine, das Gelände ist eben")
        );
    }

    /// Ohne Aufmaß nur das Einlesen; mit Aufmaß Höhenlage, Folgen und
    /// Grundstück wie in der Abnahme (§12).
    #[test]
    fn leiste_zum_modell() {
        let mut m = lindenhof();
        let z = Zustand::default();
        let l = leiste(&m, &z);
        assert!(!l.aufmass && l.felder.is_empty() && l.hoehenlage.is_empty());
        // Fläche zeichnen geht ohne Gelände; die Leiste zeigt das (I1)
        let mut zeichnen = Zustand::default();
        zeichnen.werkzeug.set_aktiv(true);
        assert!(m.terrain().is_none() && leiste(&m, &zeichnen).zeichnen);
        let (t, satz, _) = Terrain::lesen(CSV, "aufmass.csv", "2026-10-12", 32).unwrap();
        assert!(satz.contains("Punkte"), "{satz}");
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        let l = leiste(&m, &z);
        assert!(l.aufmass && l.grenze && !l.absolut);
        assert_eq!(l.felder.len(), 1);
        assert_eq!(l.felder[0].field, Field::Gelaende(GelaendeFeld::Versatz));
        // Ohne Lage liegt das Grundstück mittig unter dem Haus; ±0,00 liegt
        // auf dem höchsten Punkt am Plattenrand
        assert!(
            l.hoehenlage[0].starts_with("±0,00 = 25,"),
            "{:?}",
            l.hoehenlage
        );
        assert!(
            l.hoehenlage[1].starts_with("Sockel 0,0 bis"),
            "{:?}",
            l.hoehenlage
        );
        assert!(l.hoehenlage[2].ends_with("frostfrei"), "{:?}", l.hoehenlage);
        assert_eq!(l.steckt, None);
        // 822,865 m² (Abnahme: 822,87 ± 0,01)
        assert_eq!(l.grundstueck.as_deref(), Some("Grundstück 822,86 m²"));
        assert_eq!(l.hoehenlinien, None, "Höhenlinien wie gewünscht");
        assert!(!l.deckt_nicht, "das Aufmaß deckt die Baugrube");
        let mut offen = Zustand {
            offen: true,
            ..Default::default()
        };
        pruefen(&mut offen, &m);
        assert_eq!(offen.ohne_netz, 0, "deckt");
        // Sockel und Einbindung aus dem Faden, wie sonst hier gerechnet
        // (H-196h)
        assert_eq!(offen.folgen, Some(m.terrain_folgen()));
        assert_eq!(leiste(&m, &offen).hoehenlage, l.hoehenlage);
        // Absolut: das Feld trägt die Höhe; 0,30 m tiefer steckt das Haus
        m.begin("absolut");
        assert!(m.set_terrain_href(HeightRef::Abs));
        let h = m.terrain_nhn().unwrap();
        assert!(m.set_terrain_nhn(h - 0.3));
        m.commit();
        let l = leiste(&m, &z);
        assert!(l.absolut);
        assert_eq!(l.felder[0].field, Field::Gelaende(GelaendeFeld::Nhn));
        assert!(l.steckt.is_some(), "{:?}", l.hoehenlage);
        // Verschieben zeigt zwei Felder vor der Höhenlage
        let z = Zustand {
            tippen: Some(GelaendeKnopf::Verschieben),
            ..Default::default()
        };
        let l = leiste(&m, &z);
        let f: Vec<Field> = l.felder.iter().map(|r| r.field).collect();
        assert_eq!(
            f,
            [
                Field::Gelaende(GelaendeFeld::Rechts),
                Field::Gelaende(GelaendeFeld::Oben),
                Field::Gelaende(GelaendeFeld::Nhn)
            ]
        );
        // weit weg verschoben: die Erdmengen rechnen mit der mittleren Höhe
        // (Bedienbarkeit 42.5)
        m.begin("Verschieben");
        assert!(m.move_terrain([200_000.0, 0.0]));
        assert!(!offen.baugruben(&m), "nicht im offenen Schritt");
        assert!(offen.pruefung.is_none());
        m.commit();
        assert!(!leiste(&m, &offen).deckt_nicht, "erst aus dem Faden");
        assert!(pruefen(&mut offen, &m), "rote Zeile");
        assert!(leiste(&m, &offen).deckt_nicht);
        assert!(!offen.baugruben(&m), "derselbe Stand rechnet nicht neu");
        assert!(offen.pruefung.is_none());
        // zurück unters Haus: fertig wird der Faden mitten im nächsten Zug,
        // abgeholt erst nach dem Loslassen (H-196f)
        m.begin("zurück");
        assert!(m.move_terrain([-200_000.0, 0.0]));
        m.commit();
        assert!(!offen.baugruben(&m) && offen.prueft());
        m.begin("Zug");
        let bis = std::time::Instant::now() + std::time::Duration::from_millis(300);
        while std::time::Instant::now() < bis {
            assert!(!offen.baugruben(&m), "nicht im offenen Schritt");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(offen.prueft() && offen.ohne_netz > 0);
        m.commit();
        assert!(offen.baugruben(&m), "nach dem Loslassen");
        assert_eq!(offen.ohne_netz, 0);
        m.begin("wieder weg");
        assert!(m.move_terrain([200_000.0, 0.0]));
        m.commit();
        assert!(pruefen(&mut offen, &m));
        assert!(leiste(&m, &offen).deckt_nicht);
        // ohne Aufmaß keine Leiste mehr dafür (42.6)
        let vorher = m.terrain().cloned();
        m.begin("Aufmaß entfernen");
        assert!(m.set_terrain(None));
        m.commit();
        offen.meldung.push(ENTFERNT.into());
        let l = leiste(&m, &offen);
        assert!(!l.aufmass && !l.deckt_nicht);
        assert_eq!(l.meldung, [ENTFERNT]);
        assert!(
            offen.baugruben(&m) && offen.pruefung.is_none(),
            "ohne Faden"
        );
        assert_eq!(offen.ohne_netz, 0);
        // wie nach Strg+Z: das Aufmaß ist wieder da, die Meldung nicht (H-196c)
        m.begin("zurück");
        assert!(m.set_terrain(vorher));
        m.commit();
        assert!(leiste(&m, &offen).meldung.is_empty());
    }

    /// [`Zustand::baugruben`] bis der Faden fertig ist; `true`, wenn sich
    /// die rote Zeile ändert.
    fn pruefen(z: &mut Zustand, m: &Model) -> bool {
        let mut anders = z.baugruben(m);
        while z.pruefung.is_some() {
            std::thread::sleep(std::time::Duration::from_millis(1));
            anders |= z.baugruben(m);
        }
        anders
    }

    /// 100.000 Punkte lesen und vernetzen im Faden; im UI-Faden bleibt nur
    /// das Übernehmen ins Modell (Review #152).
    #[test]
    fn grosses_aufmass_im_faden() {
        let ordner = std::env::temp_dir().join(format!("sk-hoehen-{}", std::process::id()));
        std::fs::create_dir_all(&ordner).unwrap();
        let p = ordner.join("gross.csv");
        let mut s = String::with_capacity(4 << 20);
        for i in 0..317 {
            for j in 0..317 {
                let (x, y) = (i as f64 * 0.5, j as f64 * 0.5);
                s.push_str(&format!(
                    "{},{:.3},{:.3},{:.3}\n",
                    i * 317 + j + 1,
                    467_000.0 + x,
                    5_878_000.0 + y,
                    24.0 + 0.02 * x + 0.3 * (y / 7.0).sin()
                ));
            }
        }
        std::fs::write(&p, s).unwrap();
        let t0 = std::time::Instant::now();
        let gelesen = std::thread::spawn({
            let p = p.clone();
            move || hoehen_lesen(&p, 32)
        })
        .join()
        .unwrap();
        let faden = t0.elapsed().as_secs_f64();
        std::fs::remove_dir_all(&ordner).ok();
        let (t, satz, _) = gelesen.unwrap();
        assert!(satz.starts_with("100489 Punkte"), "{satz}");
        let im_faden = t.mesh();
        let mut m = lindenhof();
        let t1 = std::time::Instant::now();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        let netz = m.terrain().unwrap().mesh();
        let ui = t1.elapsed().as_secs_f64();
        // das Netz aus dem Faden, nicht neu trianguliert
        assert!(Arc::ptr_eq(&im_faden, &netz));
        assert!(netz.netz.dreiecke.len() > 190_000);
        eprintln!("Höhenaufmaß 100.489 Punkte: Faden {faden:.3} s, UI {ui:.3} s");
        assert!(ui < 0.25 * faden + 0.05, "Faden {faden} s, UI {ui} s");
    }

    /// „Höhen“ nimmt auch die DXF des Vermessers (G1b): Der Dialog bietet
    /// sie an, und die Leiste zeigt dasselbe Grundstück wie mit der CSV.
    #[test]
    fn dxf_in_der_leiste() {
        const DXF: &[u8] =
            include_bytes!("../../crates/sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.dxf");
        assert!(FILTER[0].1.split(';').any(|f| f == "*.dxf"));
        let mut m = lindenhof();
        let (t, satz, hinweise) = Terrain::lesen(DXF, "aufmass.dxf", "2026-10-13", 32).unwrap();
        assert_eq!(
            satz,
            "109 Punkte · DXF R12 · 4 Bruchkanten · 1 Grenze · ETRS89/UTM · Höhen 24,15 bis 26,04 m"
        );
        assert!(hinweise.is_empty(), "{hinweise:?}");
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        let l = leiste(&m, &Zustand::default());
        assert!(l.aufmass && l.grenze);
        assert_eq!(l.grundstueck.as_deref(), Some("Grundstück 822,86 m²"));
    }

    /// Ersetzt das Flurstück aus dem Lageplan die Grenze der DXF, sagt es
    /// die Leiste (Review #152 H-152a).
    #[test]
    fn dxf_grenze_weicht_dem_flurstueck() {
        const DXF: &[u8] =
            include_bytes!("../../crates/sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.dxf");
        let plan = SitePlan::read(PLAN, 32).unwrap();
        let mut m = lindenhof();
        m.begin("Lageplan");
        assert!(m.import_parcel(&plan, "lageplan.pdf", None));
        m.commit();
        let (t, _, _) = Terrain::lesen(DXF, "aufmass.dxf", "2026-10-14", 32).unwrap();
        let eigene = t.parcels.clone();
        assert_eq!(eigene.len(), 1);
        // der Plan liegt nur im Maßstab vor: eingepasst wie in der App
        let (fit, _) = plan.fit_to(&t.points).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain_fitted(t, fit));
        m.commit();
        assert!(m.terrain().unwrap().underlay.is_some());
        assert!(grenze_ersetzt(&eigene, &m).is_some());
        // ohne Lageplan bleibt die Grenze der DXF, kein Satz
        let mut n = lindenhof();
        let (t, _, _) = Terrain::lesen(DXF, "aufmass.dxf", "2026-10-14", 32).unwrap();
        n.begin("Aufmaß");
        assert!(n.import_terrain(t));
        n.commit();
        assert_eq!(n.terrain().unwrap().parcels, eigene);
        assert!(grenze_ersetzt(&eigene, &n).is_none());
    }

    /// Jeder Knopf hat einen Namen mit Satz in der Hilfe.
    #[test]
    fn knoepfe_haben_hilfe() {
        use sk_model::Show;
        use GelaendeKnopf as K;
        for k in [
            K::Leiste,
            K::Lageplan,
            K::Uebernehmen,
            K::Hoehen,
            K::Verschieben,
            K::Drehen,
            K::Ausrichten,
            K::Ueber,
            K::Absolut,
            K::Zuschnitt(Show::All),
            K::Zuschnitt(Show::Fade),
            K::Zuschnitt(Show::Parcel),
            K::AmHaus,
            K::Flaeche,
            K::Befestigt,
            K::Loeschen,
            K::Hinweise,
            K::Entfernen,
        ] {
            let z = crate::help::tooltip_lines(knopf_name(k));
            assert_eq!(z.len(), 3, "{k:?}");
        }
    }

    const PLAN: &[u8] =
        include_bytes!("../../crates/sk-gelaende/tests/daten/lageplan-lindenhof-7.pdf");

    /// Fehler beim Lesen sagen, was nicht eingelesen wurde (H5).
    #[test]
    fn lageplan_fehler_mit_vorsatz() {
        let d = std::env::temp_dir().join(format!("skizzeo-lageplan-h5-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("kein.pdf");
        std::fs::write(&p, b"Hallo").unwrap();
        let e = lageplan_lesen(&p, 32).unwrap_err();
        assert!(
            e.as_str().starts_with("Lageplan nicht eingelesen: "),
            "{}",
            e.as_str()
        );
        let e = lageplan_lesen(&d.join("fehlt.pdf"), 32).unwrap_err();
        assert!(
            e.as_str().starts_with("Lageplan nicht eingelesen"),
            "{}",
            e.as_str()
        );
        let p = d.join("lageplan.pdf");
        std::fs::write(&p, PLAN).unwrap();
        assert!(lageplan_lesen(&p, 32).unwrap().parcel.is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Bedienbarkeit 43.5: Drei Hinweise stehen da, die übrigen hinter
    /// „Alle Hinweise“; ein zweiter Klick nimmt sie wieder weg, ein neues
    /// Einlesen leert beides.
    #[test]
    fn alle_hinweise() {
        let h: Vec<String> = (1..=5).map(|i| format!("Hinweis {i}")).collect();
        let mut z = Zustand::default();
        z.hinweise(&h);
        let m = Model::with_seed(7);
        let l = leiste(&m, &z);
        assert_eq!(l.meldung, h[..3]);
        assert_eq!((l.weitere, l.alle), (2, false));
        z.alle = true;
        assert_eq!(leiste(&m, &z).meldung, h);
        z.leeren();
        let l = leiste(&m, &z);
        assert!(l.meldung.is_empty() && l.weitere == 0 && !l.alle);
        z.hinweise(&h[..2]);
        assert_eq!(leiste(&m, &z).weitere, 0);
    }

    /// Der Beispiel-Lageplan: Maßstab und Flurstück in der Leiste, dann
    /// übernommen ein ebenes Grundstück ohne Höhenlage.
    #[test]
    fn lageplan_in_der_leiste() {
        let plan = SitePlan::read(PLAN, 32).unwrap();
        assert_eq!(
            lageplan_saetze(&plan),
            // vom Blatt (Lage auf 0,01 pt, 1,8 mm bei 1:500); das Aufmaß
            // ergibt 822,87 m²
            [
                "Lageplan 1:500, Norden oben",
                "Flurstück 123/4, 822,90 m² (aus der Zeichnung)"
            ]
        );
        let mut s = crate::scene::Scene::with_model(lindenhof());
        let z = Zustand {
            plan: Some(("lageplan.pdf".into(), Arc::new(plan.clone()))),
            ..Default::default()
        };
        let l = leiste(s.model(), &z);
        assert!(l.uebernehmen && !l.aufmass);
        assert!(s.edit_model("Lageplan", |m| m.import_parcel(&plan, "lageplan.pdf", None)));
        let l = leiste(s.model(), &Zustand::default());
        assert!(l.aufmass && !l.hoehen && l.grenze && !l.uebernehmen);
        assert!(l.hoehenlage.is_empty() && l.steckt.is_none());
        assert_eq!(l.grundstueck.as_deref(), Some("Grundstück 822,90 m²"));
        // übernommen: kein Knopf mehr, nach Rückgängig wieder (H3)
        assert!(!leiste(s.model(), &z).uebernehmen);
        assert!(s.undo());
        assert!(leiste(s.model(), &z).uebernehmen);
        assert!(s.redo());
        // ohne Datei keine Unterlage, die Grenze bleibt, die Leiste sagt es
        let m = s.model();
        let mut u = Unterlage::default();
        assert!(u.linien(m).is_empty());
        assert!(u.liest() && u.stand(m) == unterlage::Stand::Liest);
        u.warten();
        assert!(u.linien(m).is_empty() && !u.liest());
        let fehlt = unterlage::Stand::Fehlt("lageplan.pdf".into());
        assert_eq!(u.stand(m), fehlt);
        let z = Zustand {
            unterlage: fehlt,
            ..Default::default()
        };
        let l = leiste(m, &z);
        assert!(
            l.meldung[0].contains("„lageplan.pdf“ nicht gefunden"),
            "{:?}",
            l.meldung
        );
        u.merken(m, Arc::new(plan));
        assert!(u.linien(m).len() > 4);
        // gemerkt: dieselben Linien ohne neues Abbilden (B3)
        assert!(Arc::ptr_eq(&u.linien(m), &u.linien(m)));
    }
}

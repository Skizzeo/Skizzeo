//! Leiste „Gelände“ in der App (plan-gelaendemodul.md §4.7, §4.8 und §5
//! G1, G1c): Knöpfe und Felder der Leiste, das Einlesen von Lageplan und
//! Höhenaufmaß, die Höhenlage, der Zuschnitt und das Verschieben und Drehen
//! des Geländes unter dem Haus. Jede Handlung ist ein Rückgängig-Schritt. Was die
//! Leiste zeigt, kommt aus dem Modell ([`leiste`]); die Zeilen baut
//! [`crate::ui`].

use crate::meldung::Meldung;
use crate::selection::de;
use crate::ui::{Field, FieldRow, GelaendeFeld, GelaendeKnopf, GelaendeLeiste};
use crate::App;
use sk_model::{Crs, HeightRef, Model, SitePlan, Terrain, UnderlayKind};
use sk_platform::Surface;
use std::path::Path;

/// Größte Punktliste, die gelesen wird (Bytes); ein Aufmaß mit 100.000
/// Punkten hat etwa 5 MB.
const MAX_DATEI: u64 = 32 << 20;

/// Dateiarten im Dialog „Lageplan einlesen“.
const FILTER_PDF: [(&str, &str); 2] = [
    ("Lageplan als PDF (*.pdf)", "*.pdf"),
    ("Alle Dateien (*.*)", "*.*"),
];

/// Dateiarten im Dialog; die Endung zählt nicht, Skizzeo liest den Inhalt.
const FILTER: [(&str, &str); 2] = [
    (
        "Punktliste (*.csv, *.txt, *.xyz, …)",
        "*.csv;*.txt;*.xyz;*.asc;*.pkt;*.dat;*.koo;*.kor;*.coo",
    ),
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
    fehler: Option<Meldung>,
    /// „Höhen“ geklickt: der Dateidialog öffnet nach dem Ereignis.
    pub(crate) hoehen_wanted: bool,
    /// „Lageplan“ geklickt: ebenso.
    pub(crate) lageplan_wanted: bool,
    /// Gelesener Lageplan und seine Datei, bis „Flurstück übernehmen“.
    plan: Option<(String, SitePlan)>,
    /// Modellstand der Leiste im Paneel; `neu` erzwingt einen Abgleich.
    stand: Option<u64>,
    neu: bool,
}

impl Zustand {
    /// Leiste zu: Eingaben und Meldungen fallen weg.
    fn zu(&mut self) {
        self.offen = false;
        self.tippen = None;
        self.meldung.clear();
        self.fehler = None;
        self.plan = None;
        self.neu = true;
    }
}

/// Der Lageplan der Unterlage im Modell, einmal gelesen je Datei. Fehlt
/// die Datei, bleibt die Grenze und die Unterlage fehlt.
#[derive(Default)]
pub(crate) struct Unterlage {
    src: Option<String>,
    plan: Option<SitePlan>,
}

impl Unterlage {
    /// Der Lageplan zur Unterlage des Modells; gelesen, wenn sich die
    /// Datei geändert hat.
    fn plan(&mut self, m: &Model) -> Option<&SitePlan> {
        let src = &m.terrain()?.underlay.as_ref()?.src;
        if self.src.as_ref() != Some(src) {
            let zone = m.location().utm.map_or(32, |u| u.zone);
            self.plan = datei(Path::new(src))
                .ok()
                .and_then(|roh| SitePlan::read(&roh, zone).ok());
            self.src = Some(src.clone());
        }
        self.plan.as_ref()
    }

    /// Linien der Unterlage im Modell (mm); leer ohne Lageplan.
    pub(crate) fn linien(&mut self, m: &Model) -> Vec<(Vec<[f64; 2]>, bool, UnderlayKind)> {
        self.plan(m).map_or(Vec::new(), |p| m.underlay_lines(p))
    }

    /// Eben gelesen: kein zweites Lesen derselben Datei.
    fn merken(&mut self, src: &str, plan: &SitePlan) {
        self.src = Some(src.to_string());
        self.plan = Some(plan.clone());
    }
}

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
        GelaendeKnopf::Zuschnitt(Show::All) => "Zuschnitt alles",
        GelaendeKnopf::Zuschnitt(Show::Fade) => "Umgebung blass",
        GelaendeKnopf::Zuschnitt(Show::Parcel) => "Nur Grundstück",
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
    let mut l = GelaendeLeiste {
        meldung: z.meldung.clone(),
        fehler: z.fehler.clone(),
        uebernehmen: z.plan.as_ref().is_some_and(|(_, p)| p.parcel.is_some()),
        ..Default::default()
    };
    let Some(t) = m.terrain() else {
        return l;
    };
    let ringe = t.boundary_rings();
    l.aufmass = true;
    l.hoehen = !t.points.is_empty();
    l.grenze = !ringe.is_empty();
    l.absolut = t.href == HeightRef::Abs;
    l.zuschnitt = Some(t.show());
    l.tippen = z.tippen;
    match z.tippen {
        Some(GelaendeKnopf::Verschieben) => {
            let r = (-WEG, WEG);
            l.felder
                .push(feld(GelaendeFeld::Rechts, "nach rechts", 0.0, r, Some("m")));
            l.felder
                .push(feld(GelaendeFeld::Oben, "nach oben", 0.0, r, Some("m")));
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
        l.felder
            .push(feld(GelaendeFeld::Nhn, "±0,00 ü. NHN", h, r, Some("m")));
    } else {
        let v = m.terrain_offset_of(m.level_building());
        let r = sk_model::MAX_TERRAIN_OFFSET;
        l.felder
            .push(feld(GelaendeFeld::Versatz, "Versatz", v, (-r, r), None));
        if let Some(h) = nhn {
            l.hoehenlage.push(format!("±0,00 = {} m ü. NHN", de(h, 2)));
        }
    }
    if let Some(f) = m.terrain_folgen() {
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
    l
}

/// Wie ein neues Aufmaß zum Lageplan passt.
enum Einpassen {
    /// Kein Lageplan allein oder schon im selben System.
    Nicht,
    /// Eingepasst, mit der größten Abweichung (m).
    Mit(sk_model::Affine, f64),
    /// Passt nicht; der Satz sagt, was mit der Grenze geschieht.
    Geht(&'static str),
}

/// Bis zu [`HINWEISE`] Hinweise, der Rest als Anzahl.
fn hinweise_anhaengen(meldung: &mut Vec<String>, hinweise: &[String]) {
    meldung.extend(hinweise.iter().take(HINWEISE).cloned());
    if hinweise.len() > HINWEISE {
        let n = hinweise.len() - HINWEISE;
        meldung.push(format!("und {n} weitere Hinweise"));
    }
}

/// Was die Leiste nach dem Lesen eines Lageplans sagt.
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
        (Some((nr, _, Some(a))), _) => v.push(format!("Flurstück {nr}, {} m²", de(*a, 2))),
        (Some((nr, _, None)), _) => v.push(format!("Flurstück {nr}")),
        (None, Some(nr)) => v.push(format!(
            "Flurstück {nr}: keine geschlossene Grenze um die Nummer gefunden"
        )),
        (None, None) => v.push("Keine Flurstücksnummer im Kopf gefunden".into()),
    }
    hinweise_anhaengen(&mut v, &p.hints);
    v
}

/// Heutiges Datum als `JJJJ-MM-TT` (Vermerk `[terrain] read=`).
fn heute() -> String {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = crate::autosave::civil_from_days(s.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Datei lesen, höchstens [`MAX_DATEI`], vor dem Lesen geprüft.
fn datei(p: &Path) -> std::io::Result<Vec<u8>> {
    let gross = || std::io::Error::new(std::io::ErrorKind::InvalidData, "zu groß");
    if std::fs::metadata(p)?.len() > MAX_DATEI {
        return Err(gross());
    }
    let b = std::fs::read(p)?;
    if b.len() as u64 > MAX_DATEI {
        return Err(gross());
    }
    Ok(b)
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
                self.gelaende_schritt("Zuschnitt", |m| m.set_terrain_show(s));
            }
        }
        self.gelaende_sync();
        self.redraw = true;
    }

    /// Feld der Leiste übernommen (ein Schritt).
    pub(crate) fn gelaende_feld(&mut self, f: GelaendeFeld, v: f64) {
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

    /// Punktliste `p` lesen und als Aufmaß übernehmen (ein Schritt). Was
    /// erkannt wurde, steht in der Leiste, ebenso ein Fehler.
    pub(crate) fn hoehen_einlesen(&mut self, p: &Path) {
        let name = p
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let m = self.scene.model();
        let zone = m.location().utm.map_or(32, |u| u.zone);
        let gelesen = datei(p)
            .map_err(|e| Meldung::aus_io("Höhenaufmaß nicht eingelesen", "Einlesen", p, &e))
            .and_then(|roh| {
                Terrain::lesen(&roh, &name, &heute(), zone).map_err(|e| Meldung::mit("{}", &[&e]))
            });
        let z = &mut self.platte.leiste;
        z.neu = true;
        z.meldung.clear();
        z.fehler = None;
        match gelesen {
            Err(e) => z.fehler = Some(e),
            Ok((t, satz, hinweise)) => {
                z.meldung.push(satz);
                hinweise_anhaengen(&mut z.meldung, &hinweise);
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
        let Some(alt) = m.terrain().filter(|a| a.points.is_empty()) else {
            return Einpassen::Nicht;
        };
        if alt.underlay.is_none()
            || matches!((alt.crs, t.crs), (Crs::Utm(a), Crs::Utm(b)) if a == b)
        {
            return Einpassen::Nicht;
        }
        let plan = self.platte.unterlage.plan(m);
        match plan.and_then(|p| p.fit_to(&t.points)) {
            Some((fit, fehler)) => Einpassen::Mit(fit, fehler),
            None => Einpassen::Geht(
                "Lageplan und Aufmaß haben keine zwei Grenzpunkte mit gleicher Nummer; die Grenze aus dem Lageplan entfällt.",
            ),
        }
    }

    /// PDF `p` als Lageplan lesen: Maßstab, Lage und Flurstück stehen in
    /// der Leiste, „Flurstück übernehmen“ macht es zur Grenze.
    pub(crate) fn lageplan_einlesen(&mut self, p: &Path) {
        let zone = self.scene.model().location().utm.map_or(32, |u| u.zone);
        let gelesen = datei(p)
            .map_err(|e| Meldung::aus_io("Lageplan nicht eingelesen", "Einlesen", p, &e))
            .and_then(|roh| SitePlan::read(&roh, zone).map_err(|e| Meldung::mit("{}", &[&e])));
        let z = &mut self.platte.leiste;
        z.neu = true;
        z.meldung.clear();
        z.fehler = None;
        z.plan = None;
        match gelesen {
            Err(e) => z.fehler = Some(e),
            Ok(plan) => {
                z.meldung = lageplan_saetze(&plan);
                z.plan = Some((p.to_string_lossy().into_owned(), plan));
            }
        }
        self.gelaende_sync();
        self.redraw = true;
    }

    /// Das Flurstück des gelesenen Lageplans übernehmen (ein Schritt). Mit
    /// Aufmaß in einem anderen System wird über die Grenzpunkte mit
    /// gleicher Nummer eingepasst.
    fn flurstueck_uebernehmen(&mut self) {
        let Some((src, plan)) = self.platte.leiste.plan.take() else {
            return;
        };
        let m = self.scene.model();
        let mut fit = None;
        let mut satz = None;
        let gleich = |t: &Terrain| matches!((plan.fit, t.crs), (Some((Crs::Utm(a), _)), Crs::Utm(b)) if a == b);
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
                    let z = &mut self.platte.leiste;
                    z.fehler = Some(Meldung::satz(
                        "Lageplan und Aufmaß liegen in verschiedenen Systemen und haben keine zwei Grenzpunkte mit gleicher Nummer.",
                    ));
                    z.plan = Some((src, plan));
                    return;
                }
            }
        }
        if plan.fit.is_none() && fit.is_none() {
            let z = &mut self.platte.leiste;
            z.fehler = Some(Meldung::satz(
                "Maßstab und Lage des Lageplans sind nicht erkannt; Einpassen über zwei Punkte folgt.",
            ));
            z.plan = Some((src, plan));
            return;
        }
        self.platte.unterlage.merken(&src, &plan);
        let ok = self.gelaende_schritt("Flurstück übernommen", |m| {
            m.import_parcel(&plan, &src, fit)
        });
        let z = &mut self.platte.leiste;
        z.meldung.clear();
        if ok {
            let nr = plan.parcel.as_ref().map_or("", |p| p.0.as_str());
            z.meldung.push(format!("Flurstück {nr} übernommen"));
            z.meldung.extend(satz);
        }
    }

    /// Paneel „Werkzeuge“ an die Leiste angleichen; ein Zeichenwerkzeug
    /// oder der Nordpfeil schließt sie.
    pub(crate) fn gelaende_sync(&mut self) {
        let z = &mut self.platte.leiste;
        let zeichnet = self.tool.enabled || self.tool.oeffnung.is_some() || self.nord.aktiv;
        if z.offen && zeichnet {
            z.zu();
        }
        let rev = self.scene.model().revision();
        if !z.neu && z.stand == Some(rev) {
            return;
        }
        z.neu = false;
        z.stand = Some(rev);
        let soll = z.offen.then(|| leiste(self.scene.model(), z));
        if soll != self.ui.gelaende {
            self.ui.gelaende = soll;
            self.overlay_dirty = true;
        }
    }
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

    /// Ohne Aufmaß nur das Einlesen; mit Aufmaß Höhenlage, Folgen und
    /// Grundstück wie in der Abnahme (§12).
    #[test]
    fn leiste_zum_modell() {
        let mut m = lindenhof();
        let z = Zustand::default();
        let l = leiste(&m, &z);
        assert!(!l.aufmass && l.felder.is_empty() && l.hoehenlage.is_empty());
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
        ] {
            let z = crate::help::tooltip_lines(knopf_name(k));
            assert_eq!(z.len(), 3, "{k:?}");
        }
    }

    const PLAN: &[u8] =
        include_bytes!("../../crates/sk-gelaende/tests/daten/lageplan-lindenhof-7.pdf");

    /// Der Beispiel-Lageplan: Maßstab und Flurstück in der Leiste, dann
    /// übernommen ein ebenes Grundstück ohne Höhenlage.
    #[test]
    fn lageplan_in_der_leiste() {
        let plan = SitePlan::read(PLAN, 32).unwrap();
        assert_eq!(
            lageplan_saetze(&plan),
            // vom Blatt (Lage auf 0,01 pt, 1,8 mm bei 1:500); das Aufmaß
            // ergibt 822,87 m²
            ["Lageplan 1:500, Norden oben", "Flurstück 123/4, 822,90 m²"]
        );
        let mut m = lindenhof();
        let z = Zustand {
            plan: Some(("lageplan.pdf".into(), plan.clone())),
            ..Default::default()
        };
        let l = leiste(&m, &z);
        assert!(l.uebernehmen && !l.aufmass);
        m.begin("Lageplan");
        assert!(m.import_parcel(&plan, "lageplan.pdf", None));
        m.commit();
        let l = leiste(&m, &Zustand::default());
        assert!(l.aufmass && !l.hoehen && l.grenze && !l.uebernehmen);
        assert!(l.hoehenlage.is_empty() && l.steckt.is_none());
        assert_eq!(l.grundstueck.as_deref(), Some("Grundstück 822,90 m²"));
        // ohne Datei keine Unterlage, die Grenze bleibt
        let mut u = Unterlage::default();
        assert!(u.linien(&m).is_empty());
        u.merken("lageplan.pdf", &plan);
        assert!(u.linien(&m).len() > 4);
    }
}

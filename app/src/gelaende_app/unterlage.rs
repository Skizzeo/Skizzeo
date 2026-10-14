//! Der Lageplan unter dem Gelände (G1c): gelesen im eigenen Faden, die
//! Linien im Modell gemerkt (Review #139 B2 und B3). Der UI-Faden liest
//! und zerlegt kein PDF; bis der Plan da ist, steht das Gelände ohne
//! Unterlage da, und die Leiste sagt, dass er gelesen wird.

use super::{datei, MAX_PDF};
use sk_model::{Model, SitePlan, UnderlayKind};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::SystemTime;

/// Linien der Unterlage im Modell (mm): Punkte, geschlossen, Art.
pub(crate) type Striche = Vec<(Vec<[f64; 2]>, bool, UnderlayKind)>;

/// Wozu abgebildete Linien gehören: Fassung des Plans, Einpassung des
/// Blatts und Lage des Aufmaßes (Bits).
pub(crate) type Schluessel = (u64, [u64; 6], [u64; 3]);

/// Datei der Unterlage, wie sie im Modell steht, und die UTM-Zone.
type Quelle = (String, u8);

/// Was der Faden liefert: der Plan oder, wenn das Lesen scheiterte, ob
/// die Datei fehlt, und ihr Zeitstempel.
type Gelesen = Result<SitePlan, (bool, Option<SystemTime>)>;

/// Wie es um die Unterlage steht (für die Leiste).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Stand {
    /// Keine Unterlage im Modell oder gelesen.
    #[default]
    Gut,
    /// Wird gelesen.
    Liest,
    /// Datei nicht gefunden; der Name.
    Fehlt(String),
    /// Datei da, aber kein lesbarer Lageplan; der Name.
    Kaputt(String),
}

/// Der Lageplan der Unterlage im Modell, einmal gelesen je Datei. Fehlt
/// die Datei, bleibt die Grenze und die Unterlage fehlt.
#[derive(Default)]
pub(crate) struct Unterlage {
    /// Datei und Zone, zu denen `plan` und `fehler` gehören.
    quelle: Option<Quelle>,
    plan: Option<Arc<SitePlan>>,
    /// Zählt je neuem Plan (Schlüssel der Linien).
    fassung: u64,
    /// Das Lesen scheiterte: fehlte die Datei, und wann sie zuletzt
    /// geändert war. Ändert sie sich, wird neu gelesen (Review #139 H2).
    fehler: Option<(bool, Option<SystemTime>)>,
    /// Lesen im eigenen Faden, zur Quelle.
    job: Option<(Quelle, mpsc::Receiver<Gelesen>)>,
    /// Abgebildete Linien zu ihrem Schlüssel.
    linien: Option<(Schluessel, Arc<Striche>)>,
    /// Ordner des Projekts: Fehlt die Datei unter ihrem Pfad, wird sie dort
    /// unter ihrem Namen gesucht (Review #139 H1).
    ordner: Option<PathBuf>,
}

/// Plan und, wenn schon abgebildet, seine Linien, für den Faden der
/// Platte.
pub(crate) struct Abbild {
    pub(crate) key: Schluessel,
    plan: Arc<SitePlan>,
    linien: Option<Arc<Striche>>,
}

impl Abbild {
    /// Die Linien waren schon gemerkt (der Faden bildet nichts ab).
    #[cfg(test)]
    pub(crate) fn gemerkt(&self) -> bool {
        self.linien.is_some()
    }

    /// Die Linien im Modell `m`: gemerkt oder neu abgebildet.
    pub(crate) fn linien(&self, m: &Model) -> Arc<Striche> {
        self.linien
            .clone()
            .unwrap_or_else(|| Arc::new(m.underlay_lines(&self.plan)))
    }
}

/// Datei und Zone der Unterlage im Modell, geliehen.
fn soll_ref(m: &Model) -> Option<(&str, u8)> {
    let src = &m.terrain()?.underlay.as_ref()?.src;
    Some((src, m.location().utm.map_or(32, |u| u.zone)))
}

/// Datei und Zone der Unterlage im Modell.
fn soll(m: &Model) -> Option<Quelle> {
    soll_ref(m).map(|(s, z)| (s.to_string(), z))
}

/// Gehört `q` zu `s`?
fn ist(q: Option<&Quelle>, s: (&str, u8)) -> bool {
    q.is_some_and(|q| q.0 == s.0 && q.1 == s.1)
}

/// Zeitstempel der Datei, wenn es sie gibt.
fn zeit(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|d| d.modified()).ok()
}

/// Name der Datei zum Pfad im Modell.
fn name(src: &str) -> String {
    Path::new(src)
        .file_name()
        .map_or(src.to_string(), |n| n.to_string_lossy().into_owned())
}

impl Unterlage {
    /// Ordner des Projekts setzen (je Durchlauf, ohne Kopie, wenn gleich).
    /// Ein anderer Ordner lässt eine fehlende Datei neu suchen; `true`,
    /// wenn deshalb neu gelesen werden soll.
    pub(crate) fn ordner(&mut self, o: Option<&Path>) -> bool {
        if self.ordner.as_deref() == o {
            return false;
        }
        self.ordner = o.map(Path::to_path_buf);
        let suchen = self.fehler.is_some_and(|f| f.0);
        if suchen {
            self.quelle = None;
            self.fehler = None;
        }
        suchen
    }

    /// Wo die Datei liegt: unter ihrem Pfad, sonst unter ihrem Namen im
    /// Ordner des Projekts.
    fn pfad(&self, src: &str) -> PathBuf {
        let p = Path::new(src);
        if p.exists() {
            return p.to_path_buf();
        }
        let neben = self
            .ordner
            .as_ref()
            .zip(p.file_name())
            .map(|(o, n)| o.join(n));
        neben
            .filter(|q| q.exists())
            .unwrap_or_else(|| p.to_path_buf())
    }

    /// Der Lageplan zur Unterlage des Modells, wenn er gelesen ist. Sonst
    /// beginnt das Lesen im eigenen Faden (einmal je Datei; nach einem
    /// Fehler neu, wenn sich die Datei ändert), und bis dahin `None`.
    pub(crate) fn plan(&mut self, m: &Model) -> Option<Arc<SitePlan>> {
        let soll = soll(m)?;
        if self.quelle.as_ref() == Some(&soll) {
            match self.fehler {
                None => return self.plan.clone(),
                Some((_, z)) if zeit(&self.pfad(&soll.0)) == z => return None,
                Some(_) => {}
            }
        }
        if self.job.as_ref().is_none_or(|(q, _)| *q != soll) {
            let p = self.pfad(&soll.0);
            let zone = soll.1;
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let r = match datei(&p, MAX_PDF) {
                    Ok(roh) => SitePlan::read(&roh, zone).map_err(|_| (false, zeit(&p))),
                    Err(e) => Err((e.kind() == std::io::ErrorKind::NotFound, zeit(&p))),
                };
                // fällt weg, wenn inzwischen eine andere Datei gelesen wird
                let _ = tx.send(r);
            });
            self.job = Some((soll, rx));
        }
        None
    }

    /// Einen fertig gelesenen Plan übernehmen; `true`, wenn einer kam (dann
    /// zieht die Platte mit der Unterlage nach).
    pub(crate) fn abholen(&mut self) -> bool {
        let Some((_, rx)) = &self.job else {
            return false;
        };
        let r = match rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => Err((false, None)),
        };
        let (q, _) = self.job.take().expect("Lesen");
        self.einsetzen(q, r);
        true
    }

    fn einsetzen(&mut self, q: Quelle, r: Gelesen) {
        self.quelle = Some(q);
        match r {
            Ok(p) => {
                self.plan = Some(Arc::new(p));
                self.fehler = None;
                self.fassung += 1;
            }
            Err(f) => {
                self.plan = None;
                self.fehler = Some(f);
            }
        }
    }

    /// Auf das Lesen warten: nur, wo ohne den Plan nichts geht (ein neues
    /// Aufmaß wird an ihm eingepasst), und in Tests.
    pub(crate) fn warten(&mut self) {
        if let Some((q, rx)) = self.job.take() {
            let r = rx.recv().unwrap_or(Err((false, None)));
            self.einsetzen(q, r);
        }
    }

    /// Es wird gelesen.
    pub(crate) fn liest(&self) -> bool {
        self.job.is_some()
    }

    /// Wie es um die Unterlage des Modells steht (je Durchlauf, ohne
    /// Kopie).
    pub(crate) fn stand(&self, m: &Model) -> Stand {
        let Some(soll) = soll_ref(m) else {
            return Stand::Gut;
        };
        if ist(self.job.as_ref().map(|j| &j.0), soll) {
            return Stand::Liest;
        }
        match self.fehler {
            Some((fehlt, _)) if ist(self.quelle.as_ref(), soll) => {
                let n = name(soll.0);
                if fehlt {
                    Stand::Fehlt(n)
                } else {
                    Stand::Kaputt(n)
                }
            }
            _ => Stand::Gut,
        }
    }

    /// Schlüssel der Linien im Modell `m` zum Plan; `None` ohne Lage.
    fn schluessel(&self, m: &Model) -> Option<Schluessel> {
        let u = m.terrain()?.underlay.as_ref()?;
        let f = m.survey_frame()?;
        let b = |v: f64| v.to_bits();
        Some((self.fassung, u.fit.map(b), [f.e0, f.n0, f.g].map(b)))
    }

    /// Plan und gemerkte Linien für den Faden der Platte; `None` ohne
    /// gelesenen Plan. Abgebildet wird im Faden (Review #139 B3).
    pub(crate) fn abbild(&mut self, m: &Model) -> Option<Abbild> {
        let plan = self.plan(m)?;
        let key = self.schluessel(m)?;
        let linien = self
            .linien
            .as_ref()
            .filter(|l| l.0 == key)
            .map(|l| l.1.clone());
        Some(Abbild { key, plan, linien })
    }

    /// Im Faden abgebildete Linien merken.
    pub(crate) fn merke_linien(&mut self, key: Schluessel, l: Arc<Striche>) {
        if key.0 == self.fassung {
            self.linien = Some((key, l));
        }
    }

    /// Linien der Unterlage im Modell (mm), gemerkt; leer ohne Lageplan.
    pub(crate) fn linien(&mut self, m: &Model) -> Arc<Striche> {
        let Some(a) = self.abbild(m) else {
            return Arc::default();
        };
        let l = a.linien(m);
        self.merke_linien(a.key, l.clone());
        l
    }

    /// Eben übernommen: kein zweites Lesen derselben Datei.
    pub(crate) fn merken(&mut self, m: &Model, plan: Arc<SitePlan>) {
        let Some(q) = soll(m) else {
            return;
        };
        self.job = None;
        self.quelle = Some(q);
        self.plan = Some(plan);
        self.fehler = None;
        self.fassung += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    const PLAN: &[u8] =
        include_bytes!("../../../crates/sk-gelaende/tests/daten/lageplan-lindenhof-7.pdf");

    /// Das Beispiel „Am Lindenhof 7“ mit dem Flurstück aus der Datei `src`.
    fn mit_lageplan(src: &Path) -> Model {
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            vec3(7000.0, 6000.0, 0.0),
            vec3(7000.0, 14000.0, 0.0),
            vec3(17000.0, 14000.0, 0.0),
            vec3(17000.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let plan = SitePlan::read(PLAN, 32).unwrap();
        m.begin("Lageplan");
        assert!(m.import_parcel(&plan, &src.to_string_lossy(), None));
        m.commit();
        m
    }

    fn ordner(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("skizzeo-unterlage-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Gelesen wird im eigenen Faden, einmal je Datei; die Linien bildet
    /// der erste Aufruf ab, danach sind sie gemerkt (Review #139 B2, B3).
    #[test]
    fn liest_im_faden_einmal() {
        let d = ordner("faden");
        let p = d.join("lageplan.pdf");
        std::fs::write(&p, PLAN).unwrap();
        let m = mit_lageplan(&p);
        let mut u = Unterlage::default();
        assert!(u.plan(&m).is_none() && u.liest());
        assert_eq!(u.stand(&m), Stand::Liest);
        // ein zweiter Aufruf beginnt kein zweites Lesen
        assert!(u.plan(&m).is_none());
        while !u.abholen() {
            std::thread::yield_now();
        }
        assert!(!u.liest() && u.stand(&m) == Stand::Gut);
        let plan = u.plan(&m).unwrap();
        assert!(Arc::ptr_eq(&plan, &u.plan(&m).unwrap()));
        let a = u.abbild(&m).unwrap();
        assert!(a.linien.is_none());
        let l = a.linien(&m);
        assert!(l.len() > 4);
        u.merke_linien(a.key, l.clone());
        let b = u.abbild(&m).unwrap();
        assert!(Arc::ptr_eq(&b.linien(&m), &l));
        // eine neue Lage des Aufmaßes bildet neu ab
        let mut n = m.clone();
        n.begin("verschieben");
        assert!(n.move_terrain([1000.0, 0.0]));
        n.commit();
        assert!(u.abbild(&n).unwrap().linien.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Fehlt die Datei am alten Ort, sucht die Unterlage sie unter ihrem
    /// Namen im Ordner des Projekts (H1). Ein Fehler bleibt nicht bis zum
    /// Neustart: Ändert sich die Datei, wird neu gelesen (H2).
    #[test]
    fn fehlt_und_kommt_wieder() {
        let d = ordner("fehlt");
        let alt = d.join("weg").join("lageplan.pdf");
        let m = mit_lageplan(&alt);
        let mut u = Unterlage::default();
        assert!(u.plan(&m).is_none());
        u.warten();
        assert_eq!(u.stand(&m), Stand::Fehlt("lageplan.pdf".into()));
        // der Ordner des Projekts allein findet nichts
        let projekt = d.join("projekt");
        std::fs::create_dir_all(&projekt).unwrap();
        assert!(u.ordner(Some(&projekt)));
        assert!(u.plan(&m).is_none());
        u.warten();
        assert_eq!(u.stand(&m), Stand::Fehlt("lageplan.pdf".into()));
        // kaputt neben dem Projekt
        let neben = projekt.join("lageplan.pdf");
        std::fs::write(&neben, b"%PDF-1.4 kaputt").unwrap();
        let f = std::fs::File::options().write(true).open(&neben).unwrap();
        f.set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000))
            .unwrap();
        drop(f);
        assert!(u.plan(&m).is_none());
        u.warten();
        assert_eq!(u.stand(&m), Stand::Kaputt("lageplan.pdf".into()));
        // unverändert: kein neues Lesen
        assert!(u.plan(&m).is_none() && !u.liest());
        // die Datei wird ersetzt: neu gelesen
        std::fs::write(&neben, PLAN).unwrap();
        let f = std::fs::File::options().write(true).open(&neben).unwrap();
        f.set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2_000_000))
            .unwrap();
        drop(f);
        assert!(u.plan(&m).is_none() && u.liest());
        u.warten();
        assert!(u.plan(&m).is_some() && u.stand(&m) == Stand::Gut);
        let _ = std::fs::remove_dir_all(&d);
    }
}

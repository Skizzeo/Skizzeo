//! Automatisch sichern (F-13): alle 5 Minuten still eine Sicherung, nur wenn
//! sich seit der letzten Sicherung (bzw. seit Öffnen oder Speichern) etwas
//! geändert hat. In die eigene `.szo` wird nie automatisch geschrieben.
//!
//! - Sicherungen liegen im Ordner „Sicherungen“ (`%APPDATA%\Skizzeo\Sicherungen`)
//!   als `<Name> <JJJJ-MM-TT HH-MM>.szo`, je Projekt nur die jüngste; nach
//!   7 Tagen entfernt sie der Start still.
//! - `sicherungen.txt` im Ordner merkt sich je Sicherung, zu welcher Datei
//!   sie gehört und ob Skizzeo danach normal beendet (oder gespeichert)
//!   wurde. Steht dort noch „offen“, war es ein Absturz: Die Startkarte
//!   bietet die Sicherung an, wenn sie neuer ist als die gespeicherte Datei.
//!
//! Die Zeit kommt von außen (Tests); in der App schreibt ein eigener Faden,
//! damit das Bild nicht ruckelt.

use crate::document::Document;
use crate::scene::Scene;
use sk_model::{szo, Model};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Takt des Sicherns.
pub const INTERVAL: Duration = Duration::from_secs(5 * 60);
/// So lange bleiben Sicherungen im Ordner „Sicherungen“.
pub const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);
/// Herkunft und Zustand der Sicherungen, eine Zeile je Sicherung.
const INDEX: &str = "sicherungen.txt";
const OPEN: &str = "offen";
const CLOSED: &str = "beendet";

/// Ordner „Sicherungen“ der App: `%APPDATA%\Skizzeo\Sicherungen`.
pub fn folder() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Skizzeo").join("Sicherungen"))
}

fn modified(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

// --- Verzeichnis der Sicherungen ---------------------------------------------

/// Zeile in `sicherungen.txt`: Dateiname der Sicherung, offen (kein
/// normales Beenden danach), Datei, zu der sie gehört.
#[derive(Clone, Debug, PartialEq)]
struct Line {
    name: String,
    open: bool,
    original: Option<PathBuf>,
}

fn read_index(dir: &Path) -> Vec<Line> {
    std::fs::read_to_string(dir.join(INDEX))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let name = f.next()?.to_string();
            let open = f.next()? == OPEN;
            let original = f.next().filter(|p| !p.is_empty()).map(PathBuf::from);
            Some(Line {
                name,
                open,
                original,
            })
        })
        .collect()
}

/// Schreibt das Verzeichnis (atomar: ein Absturz dabei verliert kein
/// „offen“); Zeilen ohne Datei fallen weg.
fn write_index(dir: &Path, lines: &[Line]) {
    let text: String = lines
        .iter()
        .filter(|l| dir.join(&l.name).is_file())
        .map(|l| {
            let state = if l.open { OPEN } else { CLOSED };
            let orig = l
                .original
                .as_ref()
                .map_or(String::new(), |p| p.display().to_string());
            format!("{}\t{state}\t{orig}\n", l.name)
        })
        .collect();
    write_atomic(&dir.join(INDEX), &text);
}

/// Sicherung als „normal beendet“ vermerken.
fn mark_closed(dir: &Path, name: &str) {
    let mut lines = read_index(dir);
    for l in lines.iter_mut().filter(|l| l.name == name) {
        l.open = false;
    }
    write_index(dir, &lines);
}

/// Schreibt atomar (Zwischendatei, dann umbenennen): Eine halbe Sicherung
/// gibt es nie, auch wenn der Rechner dabei ausgeht.
fn write_atomic(path: &Path, text: &str) -> bool {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let ok = crate::document::write_synced(&tmp, text.as_bytes())
        .and_then(|_| std::fs::rename(&tmp, path))
        .is_ok();
    if !ok {
        let _ = std::fs::remove_file(&tmp);
    }
    ok
}

/// Name der Sicherung zur Ortszeit `at`: „haus 2026-10-07 03-41.szo“.
fn backup_name(original: Option<&Path>, at: (u16, u8, u8, u8, u8)) -> String {
    let stem = original
        .and_then(Path::file_stem)
        .map_or("Unbenannt".to_string(), |s| {
            s.to_string_lossy().into_owned()
        });
    let (y, mo, d, h, mi) = at;
    format!("{stem} {y:04}-{mo:02}-{d:02} {h:02}-{mi:02}.szo")
}

// --- Zeitgeber ---------------------------------------------------------------

/// Zeitgeber des Sicherns für das offene Projekt.
pub struct AutoSave {
    dir: PathBuf,
    /// Beginn des Takts: letzte Sicherung, Öffnen oder Speichern.
    since: Option<Duration>,
    /// Modellstand der letzten Sicherung.
    covered: Option<u64>,
    /// Name der zuletzt geschriebenen Sicherung dieses Projekts.
    written: Option<String>,
    /// Auf einem eigenen Faden schreiben (App); sonst sofort (Tests).
    background: bool,
    /// Laufende Sicherung; `false`, wenn sie nicht auf die Platte kam.
    job: Option<std::thread::JoinHandle<bool>>,
    /// Ergebnis des zuletzt beendeten Versuchs, noch nicht abgeholt.
    outcome: Option<bool>,
}

/// Abstand, in dem die Ereignisschleife nach einem schreibenden Faden sieht.
const JOB_POLL: Duration = Duration::from_millis(50);

impl AutoSave {
    /// Sichert in den Ordner `dir`.
    pub fn new(dir: PathBuf) -> AutoSave {
        AutoSave {
            dir,
            since: None,
            covered: None,
            written: None,
            background: false,
            job: None,
            outcome: None,
        }
    }

    /// Schreibt auf einem eigenen Faden: Das Bild wartet nicht auf die Platte.
    pub fn in_background(mut self) -> AutoSave {
        self.background = true;
        self
    }

    /// Anderes Projekt (Öffnen, Neu): Die Sicherung des alten gilt als
    /// beendet, der Takt beginnt jetzt.
    pub fn reset(&mut self, now: Duration) {
        self.finish();
        if let Some(n) = self.written.take() {
            mark_closed(&self.dir, &n);
        }
        self.since = Some(now);
        self.covered = None;
    }

    /// Geändert seit Öffnen, Speichern oder der letzten Sicherung.
    fn changed(&self, m: &Model, doc: &Document) -> bool {
        doc.is_dirty(m) && self.covered != Some(m.revision())
    }

    /// Zur Zeit `now` (seit Programmstart): sichert, wenn geändert und seit
    /// Öffnen, Speichern oder der letzten Sicherung [`INTERVAL`] vergangen
    /// ist. `Some(pfad)`, wenn jetzt eine Sicherung geschrieben wird.
    pub fn tick(&mut self, m: &Model, doc: &Document, now: Duration) -> Option<PathBuf> {
        // Ging die letzte Sicherung schief (Platte voll, Ordner gesperrt),
        // gilt der Stand als ungesichert: nächster Versuch nach dem Takt
        if self.job.as_ref().is_some_and(|j| j.is_finished()) && !self.collect() {
            self.covered = None;
        }
        let since = *self.since.get_or_insert(now);
        if !self.changed(m, doc) || now.saturating_sub(since) < INTERVAL {
            return None;
        }
        let name = backup_name(doc.path.as_deref(), sk_platform::local_date_time());
        let path = self.dir.join(&name);
        let text = szo::write(m);
        self.collect();
        // Je Projekt nur die jüngste: die älteren erst entfernen, wenn die
        // neue auf der Platte ist
        let mut lines = read_index(&self.dir);
        let same = |l: &Line| match &doc.path {
            Some(p) => l.original.as_deref() == Some(p.as_path()),
            None => self.written.as_deref() == Some(l.name.as_str()),
        };
        let stale: Vec<String> = lines
            .iter()
            .filter(|l| l.name != name && same(l))
            .map(|l| l.name.clone())
            .collect();
        lines.retain(|l| l.name != name && !same(l));
        lines.push(Line {
            name: name.clone(),
            open: true,
            original: doc.path.clone(),
        });
        self.since = Some(now);
        self.covered = Some(m.revision());
        let (dir, p) = (self.dir.clone(), path.clone());
        let write = move || {
            if !write_atomic(&p, &text) {
                return false;
            }
            for n in &stale {
                let _ = std::fs::remove_file(dir.join(n));
            }
            write_index(&dir, &lines);
            true
        };
        if self.background {
            self.job = Some(std::thread::spawn(write));
        } else if !write() {
            self.outcome = Some(false);
            return None;
        } else {
            self.outcome = Some(true);
        }
        self.written = Some(name);
        Some(path)
    }

    /// Wie lange bis zur nächsten Sicherung (Ereignisschleife); `None`, wenn
    /// nichts zu sichern ist. Schreibt ein Faden noch, kurz: Sein Ergebnis
    /// (Hinweiskarte bei Fehlschlag, §8) soll nicht auf die nächste Eingabe
    /// warten.
    pub fn wait(&self, m: &Model, doc: &Document, now: Duration) -> Option<Duration> {
        if self.job.is_some() {
            return Some(JOB_POLL);
        }
        if !self.changed(m, doc) {
            return None;
        }
        let since = self.since.unwrap_or(now);
        Some((since + INTERVAL).saturating_sub(now))
    }

    /// Gespeichert: Die Datei ist aktuell, die Sicherung bleibt (beendet) im
    /// Ordner, der Takt beginnt neu.
    pub fn saved(&mut self, _doc: &Document, now: Duration) {
        self.finish();
        if let Some(n) = self.written.take() {
            mark_closed(&self.dir, &n);
        }
        self.since = Some(now);
        self.covered = None;
    }

    /// Normal beendet (auch „Nicht speichern“): Die Sicherung bleibt im
    /// Ordner, beim nächsten Start kommt keine Karte.
    pub fn closed(&mut self, _doc: &Document) {
        self.finish();
        if let Some(n) = self.written.take() {
            mark_closed(&self.dir, &n);
        }
        self.since = None;
        self.covered = None;
    }

    /// Ergebnis des letzten beendeten Versuchs (einmal): `false`, wenn er
    /// nicht auf die Platte kam (Hinweis F-13 §8).
    pub fn take_outcome(&mut self) -> Option<bool> {
        self.outcome.take()
    }

    /// Wie [`AutoSave::finish`], merkt sich das Ergebnis für die App.
    fn collect(&mut self) -> bool {
        if self.job.is_none() {
            return true;
        }
        let ok = self.finish();
        self.outcome = Some(ok);
        ok
    }

    /// Wartet, bis eine laufende Sicherung auf der Platte ist; `false`, wenn
    /// sie nicht geschrieben werden konnte.
    fn finish(&mut self) -> bool {
        self.job.take().is_none_or(|j| j.join().unwrap_or(false))
    }
}

impl Drop for AutoSave {
    fn drop(&mut self) {
        self.finish();
    }
}

// --- Hinweis beim Scheitern (F-13 §8) -----------------------------------------

/// Frühestens so lange nach der letzten Karte erscheint sie wieder.
const FAIL_AGAIN: Duration = Duration::from_secs(30 * 60);

/// Was mit der Hinweiskarte „Automatisches Sichern klappt gerade nicht.“
/// geschieht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeStep {
    Show,
    Hide,
    None,
}

/// Zähler der Fehlschläge in Folge und Sperre für die Karte: Ein einzelner
/// Fehlschlag bleibt still, ab dem zweiten in Folge erscheint die Karte,
/// danach höchstens alle 30 Minuten. Gelungenes Sichern oder Speichern
/// setzt alles zurück; eine gezeigte Karte blendet dann aus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FailNotice {
    fails: u32,
    last: Option<Duration>,
    shown: bool,
}

impl FailNotice {
    /// Ein Versuch im Takt zur Zeit `now` (seit Programmstart).
    pub fn attempt(&mut self, ok: bool, now: Duration) -> NoticeStep {
        if ok {
            return self.reset();
        }
        self.fails += 1;
        let free = self
            .last
            .is_none_or(|t| now.saturating_sub(t) >= FAIL_AGAIN);
        if self.fails >= 2 && free {
            self.last = Some(now);
            self.shown = true;
            return NoticeStep::Show;
        }
        NoticeStep::None
    }

    /// Die Datei wurde gespeichert.
    pub fn saved(&mut self, _now: Duration) -> NoticeStep {
        self.reset()
    }

    fn reset(&mut self) -> NoticeStep {
        let shown = self.shown;
        *self = FailNotice::default();
        if shown {
            NoticeStep::Hide
        } else {
            NoticeStep::None
        }
    }
}

/// Zeilen der Karte: fett, gedimmt, Verweis.
pub fn fail_notice_lines() -> [String; 3] {
    [
        "Automatisches Sichern klappt gerade nicht.".into(),
        "Der Ordner „Sicherungen“ ist voll oder gesperrt. Bitte die Datei speichern.".into(),
        "Jetzt speichern".into(),
    ]
}

/// Befehl hinter „Jetzt speichern“: wie Strg+S, bei Unbenannt „Speichern
/// unter …“.
pub fn fail_notice_command(untitled: bool) -> crate::menu::Command {
    if untitled {
        crate::menu::Command::SaveAs
    } else {
        crate::menu::Command::Save
    }
}

// --- Start -------------------------------------------------------------------

/// Sicherung für die Startkarte.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub backup: PathBuf,
    /// Gespeicherte Datei (`None`: Unbenannt).
    pub original: Option<PathBuf>,
}

/// Beim Start zur Zeit `now`: entfernt Sicherungen älter als [`KEEP`] und
/// meldet die jüngste, nach der Skizzeo nicht normal beendet wurde und die
/// neuer ist als ihre gespeicherte Datei. `None`: keine Karte.
pub fn start(dir: &Path, now: SystemTime) -> Option<Found> {
    clean_in(dir, now);
    let mut open: Vec<(SystemTime, Line)> = read_index(dir)
        .into_iter()
        .filter(|l| l.open)
        .filter_map(|l| Some((modified(&dir.join(&l.name))?, l)))
        .collect();
    open.sort_by_key(|x| std::cmp::Reverse(x.0));
    let (t, l) = open.into_iter().next()?;
    // Danach gespeichert (etwa mit einem anderen Programmstand): keine Karte
    if let Some(p) = &l.original {
        if modified(p).is_some_and(|mp| mp > t) {
            return None;
        }
    }
    Some(Found {
        backup: dir.join(&l.name),
        original: l.original,
    })
}

/// Nach der Startkarte (Wiederherstellen oder Verwerfen): Diese Sicherung
/// führt nicht noch einmal zur Karte. Sie bleibt im Ordner.
pub fn answered(backup: &Path) {
    if let (Some(dir), Some(name)) = (backup.parent(), backup.file_name()) {
        mark_closed(dir, &name.to_string_lossy());
    }
}

/// „Wiederherstellen“: der Stand der Sicherung als Projekt mit dem
/// Originalpfad, ungespeichert, mit leerem Rückgängig-Verlauf.
pub fn restore(backup: &Path, original: Option<&Path>) -> Result<(Scene, Document), String> {
    let l = crate::document::load(backup)?;
    let s = Scene::with_model(l.model);
    let doc = Document::restored(original.map(Path::to_path_buf));
    Ok((s, doc))
}

/// Titel der Startkarte.
pub fn card_title(original: Option<&Path>) -> String {
    let name = original
        .and_then(Path::file_name)
        .map_or("Unbenannt".to_string(), |n| {
            n.to_string_lossy().into_owned()
        });
    format!("Sicherung von {name} gefunden")
}

// --- Ordner „Sicherungen“ im Dateimenü ---------------------------------------

/// Sicherung im Ordner „Sicherungen“.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    /// Datei, zu der sie gehört (`None`: Unbenannt).
    pub original: Option<PathBuf>,
    pub modified: SystemTime,
}

/// Sicherungen in `dir`, die jüngste zuerst.
pub fn list_in(dir: &Path) -> Vec<Entry> {
    let index = read_index(dir);
    let mut v: Vec<Entry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            if !name.to_lowercase().ends_with(".szo") {
                return None;
            }
            let original = index
                .iter()
                .find(|x| x.name == name)
                .and_then(|x| x.original.clone());
            Some(Entry {
                modified: modified(&path)?,
                path,
                original,
            })
        })
        .collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.modified));
    v
}

/// Sicherungen älter als [`KEEP`] still entfernen, ebenso liegen
/// gebliebene Zwischendateien (`.tmp`, Absturz beim Schreiben).
pub fn clean_in(dir: &Path, now: SystemTime) {
    let mut gone = false;
    for e in list_in(dir) {
        if now.duration_since(e.modified).is_ok_and(|d| d > KEEP) {
            gone |= std::fs::remove_file(&e.path).is_ok();
        }
    }
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        let tmp = p.extension().is_some_and(|x| x.eq_ignore_ascii_case("tmp"));
        if tmp && modified(&p).is_some_and(|t| now.duration_since(t).is_ok_and(|d| d > KEEP)) {
            let _ = std::fs::remove_file(&p);
        }
    }
    if gone {
        write_index(dir, &read_index(dir));
    }
}

// --- Zeitangaben -------------------------------------------------------------

/// Tage seit 1970-01-01 für ein bürgerliches Datum.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Datum zu Tagen seit 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Ortszeit des Zeitpunkts `t`, zurückgerechnet von der Ortszeit
/// `local_now` zur Zeit `now`.
pub fn local_at(
    t: SystemTime,
    now: SystemTime,
    local_now: (u16, u8, u8, u8, u8),
) -> (u16, u8, u8, u8, u8) {
    let (y, mo, d, h, mi) = local_now;
    let now_min = days_from_civil(y as i64, mo as i64, d as i64) * 1440 + h as i64 * 60 + mi as i64;
    let back = now.duration_since(t).map_or(0, |x| x.as_secs() / 60) as i64;
    let at = now_min - back;
    let (yy, mm, dd) = civil_from_days(at.div_euclid(1440));
    let hm = at.rem_euclid(1440);
    (
        yy as u16,
        mm as u8,
        dd as u8,
        (hm / 60) as u8,
        (hm % 60) as u8,
    )
}

/// Zeitangabe einer Kachel: „heute, 03:41“, „gestern, 23:05“, sonst
/// „01.10., 09:00“; beide Zeiten als Ortszeit (Jahr, Monat, Tag, Stunde,
/// Minute).
pub fn when_text(at: (u16, u8, u8, u8, u8), now: (u16, u8, u8, u8, u8)) -> String {
    let day = |t: (u16, u8, u8, u8, u8)| days_from_civil(t.0 as i64, t.1 as i64, t.2 as i64);
    let clock = format!("{:02}:{:02}", at.3, at.4);
    match day(now) - day(at) {
        0 => format!("heute, {clock}"),
        1 => format!("gestern, {clock}"),
        _ => format!("{:02}.{:02}., {clock}", at.2, at.1),
    }
}

/// Abstand der Sicherung zur gespeicherten Datei: „4 Minuten neuer“.
pub fn age_text(min: u64) -> String {
    match min {
        0 | 1 => "1 Minute neuer".into(),
        n if n < 60 => format!("{n} Minuten neuer"),
        n if n < 120 => "1 Stunde neuer".into(),
        n => format!("{} Stunden neuer", n / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ortszeit_zurueckgerechnet() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        let m = |n: u64| now - Duration::from_secs(n * 60);
        assert_eq!(
            local_at(m(4), now, (2026, 10, 7, 3, 45)),
            (2026, 10, 7, 3, 41)
        );
        assert_eq!(
            local_at(m(50), now, (2026, 1, 1, 0, 10)),
            (2025, 12, 31, 23, 20)
        );
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
    }

    /// Ungültige oder fremde Zeilen im Verzeichnis stören nicht; Zeilen
    /// ohne Datei fallen beim Schreiben weg.
    #[test]
    fn verzeichnis() {
        let d = std::env::temp_dir().join(format!("skizzeo-sicherungen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("haus 2026-10-07 03-41.szo"), "x").unwrap();
        std::fs::write(
            d.join(INDEX),
            "haus 2026-10-07 03-41.szo\toffen\tD:\\Projekte\\haus.szo\nMüll\nweg.szo\toffen\t\n",
        )
        .unwrap();
        let l = list_in(&d);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].original, Some(PathBuf::from("D:\\Projekte\\haus.szo")));
        answered(&l[0].path);
        assert_eq!(
            std::fs::read_to_string(d.join(INDEX)).unwrap(),
            "haus 2026-10-07 03-41.szo\tbeendet\tD:\\Projekte\\haus.szo\n"
        );
        assert_eq!(start(&d, SystemTime::now()), None);
        let _ = std::fs::remove_dir_all(&d);
    }
    /// Kam eine Sicherung im Hintergrund nicht auf die Platte (hier: der
    /// Ordner ist eine Datei), versucht der Takt es wieder, auch ohne neue
    /// Änderung. Liegen gebliebene Zwischendateien räumt der Start nach
    /// [`KEEP`] weg; das Verzeichnis selbst hinterlässt keine.
    #[test]
    fn fehlschlag_wird_wiederholt_und_tmp_geraeumt() {
        let d = std::env::temp_dir().join(format!("skizzeo-sicherungen-f-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let gesperrt = d.join("gesperrt");
        std::fs::write(&gesperrt, "keine Ordner").unwrap();
        let m = Model::with_seed(1);
        // Ungespeichert geändert: anderer Stand als beim Öffnen
        let doc = Document::opened(d.join("haus.szo"), m.revision() + 1);
        let mut a = AutoSave::new(gesperrt.clone()).in_background();
        let t = |min: u64| Duration::from_secs(min * 60);
        assert_eq!(a.tick(&m, &doc, t(0)), None);
        assert!(a.tick(&m, &doc, t(5)).is_some(), "fällig, Faden startet");
        // Die Schleife sieht bald wieder nach, nicht erst bei der nächsten
        // Eingabe (Hinweiskarte erscheint auch bei stiller Maus)
        assert_eq!(a.wait(&m, &doc, t(5)), Some(JOB_POLL));
        while !a.job.as_ref().unwrap().is_finished() {
            std::thread::yield_now();
        }
        assert_eq!(a.tick(&m, &doc, t(6)), None, "erst nach dem Takt");
        assert_eq!(a.take_outcome(), Some(false), "Fehlschlag gemeldet");
        assert_eq!(a.take_outcome(), None, "nur einmal");
        assert!(
            a.tick(&m, &doc, t(10)).is_some(),
            "ohne neue Änderung wiederholt"
        );
        drop(a);

        // Zwischendateien: alte weg, frische (vielleicht ein zweites
        // Skizzeo beim Schreiben) bleiben
        let alt = d.join("haus 2026-09-01 10-00.szo.tmp");
        std::fs::write(&alt, "halb").unwrap();
        let spaeter = SystemTime::now() + KEEP + Duration::from_secs(60);
        clean_in(&d, SystemTime::now());
        assert!(alt.is_file());
        clean_in(&d, spaeter);
        assert!(!alt.is_file());
        write_index(&d, &[]);
        assert!(!d.join("sicherungen.txt.tmp").exists());
        assert!(d.join(INDEX).is_file());
        let _ = std::fs::remove_dir_all(&d);
    }
}

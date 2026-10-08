//! Firmenkatalog als Datei (K2): `%APPDATA%\Skizzeo\firmenkatalog.szk` oder
//! ein anderer Ort aus den Einstellungen (`[firmenkatalog] datei=…`, auch
//! ein Netzlaufwerk). Fehlt die Datei am Vorgabeort, entsteht sie mit dem
//! Startbestand. Was nicht lesbar ist, wird zum Hinweis; Skizzeo arbeitet
//! dann mit dem eingebauten Startbestand weiter und blockiert nie.

use crate::meldung::Meldung;
use sk_model::{export_material, export_type, Guid, MaterialId, Model};
use sk_model::{read_szk_with, write_szk, Library};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

/// Name der Datei am Vorgabeort.
pub const FILE_NAME: &str = "firmenkatalog.szk";

/// Ergebnis von [`Company::save_type`].
#[derive(Clone, Debug, PartialEq)]
pub enum SaveResult {
    Saved,
    /// Ein anderer hat die Datei seit dem Laden geändert: nichts geschrieben,
    /// die Oberfläche fragt „Neu laden und Typ erneut zurückspeichern?“.
    Changed,
    Failed(String),
}

/// Der geladene Firmenkatalog und der Stand seiner Datei.
pub struct Company {
    path: PathBuf,
    lib: Library,
    /// Stempel der Datei beim Laden bzw. letzten Schreiben.
    stamp: Option<Stempel>,
    /// Inhalt der Datei beim Laden bzw. letzten Schreiben: der Stand, den
    /// der Nutzer gesehen hat (`firma_anwenden`, Bausteingrenze §5).
    geladen: String,
    /// Die Datei ließ sich nicht lesen: nie darüber schreiben.
    broken: bool,
    /// Zählt jede Änderung des Katalogs im Speicher (Laden, Schreiben).
    gen: u64,
    /// Die Sätze, die das letzte „Auch für neue Häuser“ geändert hat.
    zuletzt: Vec<sk_cost::SatzId>,
}

/// Schluss des Hinweises auf Fremdes aus einer neueren Fassung; er gehört
/// in die Statuszeile, nicht in einen Dialog ([`quiet`]).
const UNKNOWN_TAIL: &str = "unbekannte Angaben übersprungen, alles andere ist geladen.";

/// Schluss des Hinweises auf Typen mit ungültigem Deckenauflager (Regel 21),
/// ebenfalls für die Statuszeile.
const BEARING_TAIL: &str = "gebaut wie „ganze tragende Schicht“.";

/// Hinweis für die Statuszeile statt eines Dialogs?
pub fn quiet(hint: &str) -> bool {
    hint.ends_with(UNKNOWN_TAIL) || hint.ends_with(BEARING_TAIL)
}

/// Wo sich Skizzeo merkt, welche Fassungen eines Firmenkatalogs schon einen
/// Hinweis bekamen: beim Nutzer, sonst neben der Datei.
fn seen_list(path: &Path) -> PathBuf {
    #[cfg(not(test))]
    if let Some(a) = std::env::var_os("APPDATA") {
        return PathBuf::from(a)
            .join("Skizzeo")
            .join("firmenkatalog-hinweise.txt");
    }
    path.with_extension("szk-hinweise")
}

/// FNV-1a über den Inhalt: erkennt eine neue Fassung der Datei.
fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}

/// Ob gesehene Fassungen gemerkt werden. Der Bildvergleich (--screenshot)
/// schaltet es ab, damit er Jörns Hinweis nicht vorwegnimmt.
static REMEMBER: AtomicBool = AtomicBool::new(true);

/// Fassungen des Firmenkatalogs merken (Normalfall) oder nur prüfen.
pub fn remember_hints(on: bool) {
    REMEMBER.store(on, Ordering::Relaxed);
}

/// Erster Blick auf diese Fassung der Datei (Pfad und Inhalt)? Dann wird
/// sie gemerkt. Ohne lesbare Liste gilt: ja.
fn first_time(path: &Path) -> bool {
    first_seen(path, REMEMBER.load(Ordering::Relaxed))
}

fn first_seen(path: &Path, remember: bool) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return true;
    };
    let key = format!("{:016x} {}", fingerprint(&bytes), path.display());
    let list = seen_list(path);
    let old = std::fs::read_to_string(&list).unwrap_or_default();
    if old.lines().any(|l| l == key) {
        return false;
    }
    if !remember {
        return true;
    }
    // Die letzten Einträge genügen
    let mut lines: Vec<&str> = old.lines().collect();
    let skip = lines.len().saturating_sub(99);
    lines.drain(..skip);
    lines.push(&key);
    if let Some(dir) = list.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&list, lines.join("\n") + "\n");
    true
}

fn modified(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Stempel einer Datei: Änderungszeit, Länge und FNV-1a des Inhalts
/// (KA-2c2). Erkennt auch eine fremde Änderung in derselben Sekunde.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stempel {
    zeit: Option<SystemTime>,
    laenge: u64,
    inhalt: u64,
}

fn stempel(p: &Path) -> Option<Stempel> {
    lesen_mit_stempel(p).ok().map(|(_, s)| s)
}

/// Inhalt und Stempel aus einem Lesen. Die Zeit kommt vor dem Lesen: Ändert
/// ein anderer Platz die Datei dazwischen, passt der Stempel später nicht
/// mehr, und es wird nichts Veraltetes zurückgeschrieben.
fn lesen_mit_stempel(p: &Path) -> std::io::Result<(Vec<u8>, Stempel)> {
    let zeit = modified(p);
    let bytes = std::fs::read(p)?;
    let s = Stempel {
        zeit,
        laenge: bytes.len() as u64,
        inhalt: fingerprint(&bytes),
    };
    Ok((bytes, s))
}

/// Eindeutig je Prozess und Aufruf (Sperre, temporäre Datei).
fn kennung() -> String {
    let n = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{}-{n}", std::process::id())
}

/// Alter der Datei `p` nach der Uhr des Ablageorts: Auf einem Netzlaufwerk
/// setzt der Server die Änderungszeit, die eigene Uhr kann abweichen. Eine
/// frisch angelegte Datei daneben gibt dessen „jetzt“; geht das nicht, gilt
/// die eigene Uhr.
fn alter(p: &Path) -> Option<Duration> {
    let m = modified(p)?;
    let uhr = p.with_extension(format!("szk.uhr-{}", kennung()));
    let jetzt = std::fs::write(&uhr, b"")
        .ok()
        .and_then(|_| modified(&uhr))
        .unwrap_or_else(SystemTime::now);
    let _ = std::fs::remove_file(&uhr);
    jetzt.duration_since(m).ok()
}

/// Ab diesem Alter gilt eine Sperrdatei als liegen geblieben.
const SPERRE_ALT: Duration = Duration::from_secs(30);

/// Meldung, wenn ein anderer Platz gerade schreibt.
pub const GESPERRT: &str =
    "Firmenkatalog wird gerade an einem anderen Platz gespeichert. Nochmal versuchen.";

/// Vorsatz und Handlung, wenn das Speichern scheitert ([`Meldung::aus_io`]).
const NICHT_GESPEICHERT: (&str, &str) =
    ("Firmenkatalog nicht gespeichert", "Firmenkatalog speichern");

/// Schluss der Hinweise, wenn der Katalog nicht geladen ist.
const STARTBESTAND: &str = "Skizzeo arbeitet mit dem eingebauten Startbestand.";

/// Sperrdatei `firmenkatalog.szk.lock` neben dem Katalog (Bausteingrenze
/// §5): mit `create_new` angelegt, Inhalt Platz, Uhrzeit und eine Kennung;
/// gelöscht, wenn sie fallen gelassen wird und noch die eigene ist.
struct Sperre {
    path: PathBuf,
    inhalt: String,
}

impl Sperre {
    /// Liegt noch die eigene Sperre? Übernehmen zwei Plätze gleichzeitig
    /// eine liegen gebliebene, löscht der zweite die frische des ersten;
    /// vor dem Schreiben merkt es so der, dessen Sperre weg ist.
    fn gilt(&self) -> bool {
        std::fs::read_to_string(&self.path).is_ok_and(|t| t == self.inhalt)
    }
}

impl Drop for Sperre {
    fn drop(&mut self) {
        if self.gilt() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Sperrt den Katalog `path`. Eine Sperre älter als 30 s wird still
/// übernommen; das steht nur im Fehlerprotokoll (Bausteingrenze §5).
fn sperren(path: &Path) -> Result<Sperre, Meldung> {
    use std::io::Write;
    let lock = path.with_extension("szk.lock");
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let platz = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unbekannter Platz".into());
    let (j, mo, t, h, mi) = sk_platform::local_date_time();
    let inhalt = format!("{platz} {t:02}.{mo:02}.{j} {h:02}:{mi:02} {}\n", kennung());
    let anlegen = || {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
    };
    let (ergebnis, was) = NICHT_GESPEICHERT;
    let mut f = match anlegen() {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let alt = alter(&lock).is_some_and(|d| d > SPERRE_ALT);
            if !alt {
                return Err(Meldung::satz(GESPERRT));
            }
            let wer = std::fs::read_to_string(&lock).unwrap_or_default();
            let _ = std::fs::remove_file(&lock);
            crate::meldung::protokoll(&format!(
                "Liegen gebliebene Sperre {} übernommen ({})",
                lock.display(),
                wer.trim()
            ));
            anlegen().map_err(|_| Meldung::satz(GESPERRT))?
        }
        Err(e) => return Err(Meldung::aus_io(ergebnis, was, path, &e)),
    };
    f.write_all(inhalt.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|e| Meldung::aus_io(ergebnis, was, path, &e))?;
    Ok(Sperre { path: lock, inhalt })
}

#[cfg(test)]
thread_local! {
    /// Test: Schreiben scheitert nach der temporären Datei, vor dem
    /// Umbenennen (die Abnahme prüft, dass die Firmendatei heil bleibt).
    static SCHREIBFEHLER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Test: welcher Fehler beim Schreiben (sonst `ErrorKind::Other`),
    /// etwa schreibgeschützt oder von einem anderen Programm geöffnet.
    static SCHREIBFEHLER_ART: std::cell::Cell<Option<fn() -> std::io::Error>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn schreibfehler_im_test() -> std::io::Result<()> {
    if SCHREIBFEHLER.with(|f| f.get()) {
        return Err(SCHREIBFEHLER_ART
            .with(|a| a.get())
            .map_or_else(|| std::io::Error::other("Schreibfehler (Test)"), |f| f()));
    }
    Ok(())
}

#[cfg(not(test))]
fn schreibfehler_im_test() -> std::io::Result<()> {
    Ok(())
}

/// Schreibt atomar: erst eine temporäre Datei daneben, dann umbenennen
/// (unter Windows `MoveFileExW` mit Ersetzen). Die temporäre Datei ist je
/// Aufruf eine eigene: Schreiben zwei Plätze zugleich, benennt keiner die
/// halb geschriebene des anderen um.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("szk.{}.tmp", kennung()));
    let res = crate::document::write_synced(&tmp, text.as_bytes())
        .and_then(|_| schreibfehler_im_test())
        .and_then(|_| std::fs::rename(&tmp, path));
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

impl Company {
    /// Lädt den Katalog. `standard_place`: `path` ist der Vorgabeort; fehlt
    /// die Datei dort, wird sie mit dem Startbestand angelegt. Liefert die
    /// Hinweise für den Nutzer (leer, wenn alles geklappt hat).
    pub fn laden(path: &Path, standard_place: bool) -> (Company, Vec<Meldung>) {
        let mut c = Company {
            path: path.to_path_buf(),
            lib: Library::standard(),
            stamp: None,
            geladen: String::new(),
            broken: false,
            gen: 0,
            zuletzt: Vec::new(),
        };
        let hints = c.reload(standard_place);
        (c, hints)
    }

    /// [`Company::laden`] mit den Hinweisen als Text (für die Abnahme).
    #[cfg(test)]
    pub fn load(path: &Path, standard_place: bool) -> (Company, Vec<String>) {
        let (c, h) = Company::laden(path, standard_place);
        (c, h.into_iter().map(|m| m.to_string()).collect())
    }

    /// Liest die Datei neu (z. B. nach [`SaveResult::Changed`]).
    pub fn reload(&mut self, standard_place: bool) -> Vec<Meldung> {
        self.broken = false;
        self.gen += 1;
        let gelesen = lesen_mit_stempel(&self.path).and_then(|(b, s)| {
            String::from_utf8(b)
                .map(|t| (t, s))
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        });
        match gelesen {
            Ok((text, s)) => {
                self.stamp = Some(s);
                match read_szk_with(&text, &sk_cost::lesen::ABSCHNITTE_SZK) {
                    Ok(lib) => {
                        self.geladen = text.clone();
                        let unknown = lib.foreign.unknown;
                        let bearings = lib.invalid_bearings();
                        self.lib = lib;
                        let mut hints = self.add_stock();
                        // Fremdes aus einer neueren Fassung und ungültige
                        // Auflager: ein Hinweis, einmal je Fassung der Datei
                        if (unknown > 0 || !bearings.is_empty()) && first_time(&self.path) {
                            let auflager = (!bearings.is_empty()).then(|| {
                                Meldung::mit(
                                    " Deckenauflager von {} ungültig, gebaut wie „ganze tragende Schicht“.",
                                    &[&bearings.join(", ")],
                                )
                            });
                            let fremd = (unknown > 0).then(|| {
                                Meldung::mit(
                                    " {} unbekannte Angaben übersprungen, alles andere ist geladen.",
                                    &[&unknown.to_string()],
                                )
                            });
                            hints.push(Meldung::mit(
                                "Firmenkatalog:{}{}",
                                &[
                                    auflager.as_deref().unwrap_or(""),
                                    fremd.as_deref().unwrap_or(""),
                                ],
                            ));
                        }
                        hints
                    }
                    Err(e) => {
                        self.lib = Library::standard();
                        self.broken = true;
                        crate::meldung::protokoll(&format!(
                            "Firmenkatalog {} nicht lesbar: {e}",
                            self.path.display()
                        ));
                        let m = if e.line > 0 {
                            Meldung::mit(
                                "Der Firmenkatalog ist ab Zeile {} nicht lesbar.",
                                &[&e.line.to_string()],
                            )
                        } else {
                            Meldung::satz("Der Firmenkatalog ist nicht lesbar.")
                        };
                        vec![m.dazu(STARTBESTAND)]
                    }
                }
            }
            Err(_) if standard_place => {
                self.lib = Library::standard();
                let text = write_szk(&self.lib);
                match write_atomic(&self.path, &text) {
                    Ok(()) => {
                        self.stamp = stempel(&self.path);
                        self.geladen = text;
                        Vec::new()
                    }
                    Err(e) => vec![Meldung::aus_io(
                        "Firmenkatalog nicht angelegt",
                        "Firmenkatalog anlegen",
                        &self.path,
                        &e,
                    )
                    .dazu(STARTBESTAND)],
                }
            }
            Err(e) => {
                self.lib = Library::standard();
                self.stamp = None;
                self.geladen = String::new();
                vec![Meldung::aus_io(
                    "Firmenkatalog nicht geladen",
                    "Firmenkatalog laden",
                    &self.path,
                    &e,
                )
                .dazu(STARTBESTAND)]
            }
        }
    }

    /// Ergänzt neue Werkstypen (K4) und schreibt den Katalog zurück, damit
    /// der Hinweis nur einmal kommt.
    fn add_stock(&mut self) -> Vec<Meldung> {
        let before = write_szk(&self.lib);
        let (added, switched) = self.lib.add_stock();
        if self.lib.stock.is_empty() {
            return Vec::new();
        }
        let mut hints = Vec::new();
        if !added.is_empty() {
            hints.push(Meldung::mit(
                "Firmenkatalog: neue Werkstypen ergänzt ({}).",
                &[&added.join(", ")],
            ));
        }
        if switched {
            hints.push(Meldung::satz(
                "Firmenkatalog: Standard-Außenwand ist jetzt AW-36.",
            ));
        }
        // Nur bei echter Ergänzung schreiben: die Datei einer anderen Fassung
        // bleibt sonst bytegleich (Reihenfolge, fremde Zeilen)
        let text = write_szk(&self.lib);
        if text != before {
            // Unter der Sperre und nur über den gelesenen Stand; sonst bleibt
            // die Ergänzung im Speicher und kommt beim nächsten Laden wieder
            let Ok(sperre) = sperren(&self.path) else {
                return hints;
            };
            if stempel(&self.path) != self.stamp || !sperre.gilt() {
                return hints;
            }
            match write_atomic(&self.path, &text) {
                Ok(()) => {
                    self.stamp = stempel(&self.path);
                    self.geladen = text;
                }
                Err(e) => hints.push(Meldung::aus_io(
                    "Firmenkatalog nicht ergänzt",
                    "Firmenkatalog ergänzen",
                    &self.path,
                    &e,
                )),
            }
        }
        hints
    }

    /// Die Sätze, die das letzte [`Company::fuer_firma`] geändert hat
    /// (Nachfrage beim Speichern, Bedienbarkeit 9.2).
    pub fn zuletzt_geaendert(&self) -> &[sk_cost::SatzId] {
        &self.zuletzt
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn library(&self) -> &Library {
        &self.lib
    }

    /// Inhalt der Datei beim Laden bzw. letzten Schreiben (Vorschau der
    /// Verwaltung, KA-3a2): leer, wenn es die Datei noch nicht gibt.
    pub fn geladen(&self) -> &str {
        &self.geladen
    }

    /// Stand des Katalogs im Speicher als Schlüssel für Zwischenspeicher:
    /// ändert sich mit jedem Laden und Schreiben und mit dem Ort.
    pub fn stand(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.path.hash(&mut h);
        self.gen.hash(&mut h);
        h.finish()
    }

    /// Speichert einen Projekttyp in den Katalog zurück. Hat sich die Datei
    /// seit dem Laden geändert, wird nichts geschrieben ([`SaveResult::Changed`]);
    /// es gibt keine Sperren.
    pub fn save_type(&mut self, m: &Model, g: Guid) -> SaveResult {
        self.save_with(|lib| export_type(m, lib, g), "Typ nicht im Projekt")
    }

    /// Speichert einen Baustoff in den Katalog zurück, sonst wie
    /// [`Company::save_type`].
    pub fn save_material(&mut self, m: &Model, id: MaterialId) -> SaveResult {
        self.save_with(
            |lib| export_material(m, lib, id),
            "Baustoff nicht im Projekt",
        )
    }

    /// Speichert eine Mustervorlage („Als Vorlage speichern …“ im Fenster
    /// „Muster“), sonst wie [`Company::save_type`]; ein ungültiger Name
    /// kommt als [`SaveResult::Failed`] mit dem Grund zurück.
    pub fn save_preset(
        &mut self,
        name: &str,
        pattern: &sk_model::proctex::Pattern,
        base: [u8; 3],
    ) -> SaveResult {
        let mut why = String::new();
        let r = self.save_with(
            |lib| match sk_model::save_preset(lib, name, pattern, base) {
                Ok(()) => true,
                Err(e) => {
                    why = e;
                    false
                }
            },
            "",
        );
        match r {
            SaveResult::Failed(e) if e.is_empty() => SaveResult::Failed(why),
            r => r,
        }
    }

    /// Kostensätze für neue Häuser schreiben (Bausteingrenze §5, KA-2c2):
    /// unter Sperre die Datei neu lesen, `firma_anwenden` gegen den
    /// gesehenen Stand, den Stand davor unverändert nach
    /// `firmenkatalog-staende/stand-000n.szk` legen, atomar schreiben und
    /// neu laden. Scheitert etwas, ist nichts geschrieben; der Fehler ist
    /// der Satz für die Meldung. Hat ein anderer Platz die Datei
    /// inzwischen geändert, wird sie neu geladen, damit der nächste Versuch
    /// auf ihrem Stand aufsetzt.
    pub fn fuer_firma(
        &mut self,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
    ) -> Result<sk_cost::FirmaNeu, Meldung> {
        if self.broken {
            return Err(Meldung::satz(
                "Der Firmenkatalog ist nicht lesbar und wird nicht überschrieben.",
            ));
        }
        let (ergebnis, was) = NICHT_GESPEICHERT;
        let sperre = sperren(&self.path)?;
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(Meldung::aus_io(ergebnis, was, &self.path, &e)),
        };
        let neu = match sk_cost::firma_anwenden(
            &text,
            &self.geladen,
            sk_cost::Rolle::Admin,
            herkunft,
            ops,
        ) {
            Ok(n) => n,
            Err(b) => {
                let m = Meldung::aus_befunden(&b, "Firmenkatalog nicht geändert.");
                if text != self.geladen {
                    drop(sperre);
                    self.reload(false);
                }
                return Err(m);
            }
        };
        if !text.is_empty() {
            let dir = self
                .path
                .parent()
                .unwrap_or(Path::new("."))
                .join("firmenkatalog-staende");
            let alt = dir.join(format!("stand-{:04}.szk", neu.stand_vorher));
            if !alt.exists() {
                std::fs::create_dir_all(&dir)
                    .and_then(|_| crate::document::write_synced(&alt, text.as_bytes()))
                    .map_err(|e| Meldung::aus_io(ergebnis, was, &dir, &e))?;
            }
        }
        if !sperre.gilt() {
            return Err(Meldung::satz(GESPERRT));
        }
        write_atomic(&self.path, &neu.text)
            .map_err(|e| Meldung::aus_io(ergebnis, was, &self.path, &e))?;
        drop(sperre);
        self.reload(false);
        self.zuletzt = neu.saetze.clone();
        Ok(neu)
    }

    fn save_with(
        &mut self,
        export: impl FnOnce(&mut Library) -> bool,
        missing: &str,
    ) -> SaveResult {
        if self.broken {
            return SaveResult::Failed(
                "Der Firmenkatalog ist nicht lesbar und wird nicht überschrieben.".into(),
            );
        }
        // Unter derselben Sperre wie die Kostensätze (KA-2c2): sonst
        // schriebe es veraltete Sätze aus dem Speicher zurück
        let sperre = match sperren(&self.path) {
            Ok(s) => s,
            Err(e) => return SaveResult::Failed(e.to_string()),
        };
        if stempel(&self.path) != self.stamp {
            return SaveResult::Changed;
        }
        let mut lib = self.lib.clone();
        if !export(&mut lib) {
            return SaveResult::Failed(missing.into());
        }
        let text = write_szk(&lib);
        if !sperre.gilt() {
            return SaveResult::Failed(GESPERRT.into());
        }
        match write_atomic(&self.path, &text) {
            Ok(()) => {
                self.lib = lib;
                self.gen += 1;
                self.stamp = stempel(&self.path);
                self.geladen = text;
                SaveResult::Saved
            }
            Err(e) => {
                let (ergebnis, was) = NICHT_GESPEICHERT;
                SaveResult::Failed(Meldung::aus_io(ergebnis, was, &self.path, &e).to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::{compare, TypeState};

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("skizzeo-katalog-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Ein Firmentyp mit ungültigem Deckenauflager (Regel 21) kommt mit ins
    /// Projekt, behält seine Tiefe und wird wie „ganze tragende Schicht“
    /// gebaut; die Statuszeile nennt ihn einmal je Fassung.
    #[test]
    fn firmentyp_mit_ungueltigem_auflager_kommt_mit() {
        let d = dir("auflager");
        let p = d.join(FILE_NAME);
        let mut lib = Library::standard();
        let mono = lib.type_by_guid(sk_model::MONO_TYPE_GUID).unwrap();
        let mut t = lib.types.get(mono).unwrap().clone();
        t.guid = Guid(0x77);
        t.code = "AW-F".into();
        t.name = "Firmentyp".into();
        lib.types.insert(t);
        let n = lib.types.len();
        assert_eq!(n, 8);
        let text = write_szk(&lib);
        let i = text.find("code=\"AW-F\"").unwrap();
        let j = i + text[i..].find("bearing=240").unwrap();
        let text = format!("{}bearing=400{}", &text[..j], &text[j + 11..]);
        std::fs::write(&p, &text).unwrap();
        let (c, h) = Company::load(&p, false);
        assert_eq!(h.len(), 1, "{h:?}");
        assert!(quiet(&h[0]) && h[0].contains("AW-F"), "{h:?}");
        let m = Model::from_library(&c.lib);
        assert_eq!(m.layer_sets().len(), n, "der 8. Typ kommt mit");
        let id = m.type_by_guid(Guid(0x77)).unwrap();
        let ft = m.layer_set(id).unwrap();
        assert!(matches!(ft.bearing, sk_model::Bearing::Depth { depth, .. } if depth == 400.0));
        assert!(m.bearing_problem(ft).is_some());
        assert!(m.check().iter().any(|x| x.contains("Auflagertiefe")));
        // Zweites Laden derselben Fassung: kein Hinweis mehr
        let (_, h) = Company::load(&p, false);
        assert!(h.is_empty(), "{h:?}");
    }

    #[test]
    fn bildvergleich_merkt_keine_fassung() {
        let d = dir("bildvergleich");
        let p = d.join(FILE_NAME);
        std::fs::write(&p, "[skizzeo-katalog 1]\n").unwrap();
        // Ohne Merken bleibt jede Fassung neu, die Liste entsteht nicht
        assert!(first_seen(&p, false));
        assert!(first_seen(&p, false));
        assert!(!seen_list(&p).exists());
        // Mit Merken nur beim ersten Blick
        assert!(first_seen(&p, true));
        assert!(!first_seen(&p, true));
        assert!(!first_seen(&p, false));
    }

    #[test]
    fn vorgabeort_wird_angelegt_fremder_nicht() {
        let d = dir("ort");
        let p = d.join(FILE_NAME);
        let (c, h) = Company::load(&p, true);
        assert!(h.is_empty(), "{h:?}");
        assert!(p.exists());
        assert_eq!(c.library(), &Library::standard());
        let q = d.join("netz").join("firma.szk");
        let (c, h) = Company::load(&q, false);
        assert!(!q.exists() && h.len() == 1, "{h:?}");
        assert_eq!(c.library(), &Library::standard());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Jörns Firmenkatalog aus K4 (mit „Luft“, cat=air) wird ohne Verlust
    /// gelesen: alle Typen und Baustoffe bleiben, nur AW-36,5 kommt dazu.
    #[test]
    fn katalog_aus_k4_ohne_verlust() {
        let d = dir("k4");
        let p = d.join(FILE_NAME);
        let alt = include_str!("firmenkatalog_k4.szk");
        std::fs::write(&p, alt).unwrap();
        let (c, h) = Company::load(&p, true);
        assert_eq!(
            h,
            ["Firmenkatalog: neue Werkstypen ergänzt (AW-36,5).".to_string()]
        );
        let vorher = sk_model::read_szk(alt).unwrap();
        let lib = c.library();
        for (_, t) in vorher.types.iter() {
            let n = lib.types.iter().find(|(_, x)| x.guid == t.guid).unwrap().1;
            assert_eq!((&n.code, &n.layers.len()), (&t.code, &t.layers.len()));
        }
        for (_, m) in vorher.materials.iter() {
            assert!(lib.materials.iter().any(|(_, x)| x == m), "{}", m.name);
        }
        assert_eq!(lib.types.len(), vorher.types.len() + 1);
        // Die Datei ist ergänzt, das zweite Laden gibt keinen Hinweis
        let (c2, h) = Company::load(&p, true);
        assert!(h.is_empty(), "{h:?}");
        assert_eq!(c2.library(), lib);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Angaben einer neueren Fassung bleiben bytegleich stehen, auch in
    /// Sätzen ohne Guid ([layer], [default], [typeprop]): beim Ergänzen der
    /// Werkstypen und beim Zurückspeichern eines anderen Typs.
    #[test]
    fn fremde_angaben_ueberleben_das_zurueckschreiben() {
        let d = dir("fremd");
        let p = d.join(FILE_NAME);
        let mut lines: Vec<String> = include_str!("firmenkatalog_k4.szk")
            .lines()
            .map(String::from)
            .collect();
        let at = |lines: &[String], head: &str, n: usize| {
            lines
                .iter()
                .enumerate()
                .filter(|(_, l)| l.starts_with(head))
                .nth(n)
                .unwrap()
                .0
        };
        // Zweite Schicht des ersten Typs, eine Standardangabe, ein Baustoff
        for (head, n, extra) in [
            ("[layer] ", 1, " neu=1"),
            ("[default] ", 1, " neu=2"),
            ("[material] ", 0, " neu=3"),
        ] {
            let i = at(&lines, head, n);
            lines[i] += extra;
        }
        // Ein Merkmal mit fremder Angabe hinter den Schichten des ersten Typs
        let set = lines[at(&lines, "[layerset] ", 0)]
            .split(' ')
            .find_map(|w| w.strip_prefix("guid="))
            .unwrap()
            .to_string();
        let last = lines
            .iter()
            .rposition(|l| l.starts_with("[layer] ") && l.contains(&set))
            .unwrap();
        lines.insert(
            last + 1,
            format!("[typeprop] set={set} key=\"Brandschutz\" value=\"F90\" neu=4"),
        );
        let fremd: Vec<String> = lines
            .iter()
            .filter(|l| l.contains(" neu="))
            .cloned()
            .collect();
        assert_eq!(fremd.len(), 4);
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        let steht = |what: &str| {
            let text = std::fs::read_to_string(&p).unwrap();
            for l in &fremd {
                assert!(text.lines().any(|x| x == l), "{what}: fehlt {l}");
            }
        };
        // Laden ergänzt AW-36,5 und schreibt zurück
        let (mut c, h) = Company::load(&p, true);
        assert!(h.iter().any(|h| h.contains("4 unbekannte")), "{h:?}");
        assert!(h.iter().any(|h| h.contains("Werkstypen ergänzt")), "{h:?}");
        steht("nach dem Ergänzen");
        // Einen anderen Typ ändern und zurückspeichern
        let mut m = Model::from_library(c.library());
        let (id, g) = m
            .layer_sets()
            .iter()
            .find(|(_, t)| t.guid.to_string() != set)
            .map(|(id, t)| (id, t.guid))
            .unwrap();
        let mut t = m.layer_set(id).unwrap().clone();
        t.layers[0].thickness += 10.0;
        assert!(m.set_layer_set(id, t));
        assert_eq!(c.save_type(&m, g), SaveResult::Saved);
        steht("nach dem Zurückspeichern");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn kaputt_gibt_hinweis_und_bleibt_unberuehrt() {
        let d = dir("kaputt");
        let p = d.join("k.szk");
        std::fs::write(&p, "SZK 1\n[layerset] guid=x\n").unwrap();
        let (mut c, h) = Company::load(&p, false);
        assert!(h[0].contains("Zeile 2"), "{h:?}");
        let m = Model::new();
        let g = m.layer_set(m.defaults().exterior_wall).unwrap().guid;
        assert!(matches!(c.save_type(&m, g), SaveResult::Failed(_)));
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "SZK 1\n[layerset] guid=x\n"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn zurueckspeichern_fragt_bei_fremder_aenderung() {
        let d = dir("zurueck");
        let p = d.join(FILE_NAME);
        let (mut c, _) = Company::load(&p, true);
        let mut m = Model::new();
        let id = m.defaults().exterior_wall;
        let mut t = m.layer_set(id).unwrap().clone();
        t.layers[0].thickness = 120.0;
        assert!(m.set_layer_set(id, t));
        let g = m.layer_set(id).unwrap().guid;
        // Ein anderer schreibt dazwischen
        let fremd = std::fs::read_to_string(&p).unwrap() + "# fremd\n";
        std::fs::write(&p, &fremd).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_modified(SystemTime::now() + std::time::Duration::from_secs(10))
            .unwrap();
        assert_eq!(c.save_type(&m, g), SaveResult::Changed);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), fremd);
        assert!(c.reload(true).is_empty());
        assert_eq!(c.save_type(&m, g), SaveResult::Saved);
        let lib = sk_model::read_szk(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert!(compare(&m, &lib).iter().all(|x| x.1 == TypeState::Same));
        let names: Vec<String> = std::fs::read_dir(&d)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [FILE_NAME]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// KA-0c: Kostenzeilen im Firmenkatalog gehen in den
    /// Erweiterungsspeicher (kein Hinweis „unbekannte Angaben“) und bleiben
    /// beim Speichern eines Typs bytegleich stehen.
    #[test]
    fn kostenzeilen_im_firmenkatalog_bleiben() {
        let d = dir("kosten");
        let p = d.join(FILE_NAME);
        let kosten =
            "[catalog] guid=0000000000000000000F01 name=\"Muster Bau\" stand=1 status=released\n\
                      [rate] key=wage num=70\n";
        let text = write_szk(&Library::standard()) + kosten;
        std::fs::write(&p, &text).unwrap();
        let (mut c, h) = Company::load(&p, false);
        assert!(h.is_empty(), "{h:?}");
        assert_eq!(c.library().ext("rate").count(), 1);
        let mut m = Model::from_library(c.library());
        let id = m.defaults().exterior_wall;
        let mut t = m.layer_set(id).unwrap().clone();
        t.layers[0].thickness = 130.0;
        assert!(m.set_layer_set(id, t));
        let g = m.layer_set(id).unwrap().guid;
        assert_eq!(c.save_type(&m, g), SaveResult::Saved);
        let neu = std::fs::read_to_string(&p).unwrap();
        assert!(neu != text, "Typ nicht gespeichert");
        assert!(neu.ends_with(kosten), "{neu}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// KA-2c2 (Bausteingrenze §5): „Auch für neue Häuser“ schreibt unter
    /// Sperre einen neuen Stand mit `[log]`, legt den Stand davor ab und
    /// lädt neu. Eine frische Sperre hält es auf, eine liegen gebliebene
    /// wird übernommen; eine fremde Änderung desselben Satzes lehnt es ab.
    /// Danach schreibt auch `save_type` unter derselben Sperre.
    #[test]
    fn fuer_firma_neuer_stand_unter_sperre() {
        let d = dir("fuer-firma");
        let p = d.join(FILE_NAME);
        let (mut c, h) = Company::load(&p, true);
        assert!(h.is_empty(), "{h:?}");
        let m = Model::from_library(c.library());
        let k = sk_cost::lesen::firma_oder_werk(&m, Some(c.library()));
        let a = k.artikel.iter().find(|a| a.preis.is_some()).unwrap();
        let alt = a.preis.unwrap();
        let preis = |c: &Company| {
            sk_cost::lesen::firma_oder_werk(&m, Some(c.library()))
                .artikel(a.guid)
                .and_then(|x| x.preis)
        };
        let op = |cent: i64| sk_cost::Op::PreisSetzen {
            artikel: a.guid,
            preis: Some(sk_cost::Dez(alt.0 + cent * 10_000)),
            stand: "10/2026".into(),
            quelle: "Preisblatt".into(),
        };
        let herkunft = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:00");
        let lock = p.with_extension("szk.lock");
        // Frische Sperre eines anderen Platzes: nichts geschieht
        std::fs::write(&lock, "anderer Platz").unwrap();
        let vorher = std::fs::read_to_string(&p).unwrap();
        assert_eq!(c.fuer_firma(&herkunft, &[op(100)]).unwrap_err(), GESPERRT);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), vorher);
        // Liegen geblieben (älter als 30 s): still übernommen
        std::fs::File::options()
            .write(true)
            .open(&lock)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(60))
            .unwrap();
        crate::meldung::protokoll_im_test();
        let neu = c.fuer_firma(&herkunft, &[op(100)]).unwrap();
        // still übernommen: nur im Fehlerprotokoll (Bausteingrenze §5)
        let log = crate::meldung::protokoll_im_test();
        assert!(log.iter().any(|l| l.contains("anderer Platz")), "{log:?}");
        assert!(!lock.exists(), "Sperre gelöscht");
        assert_eq!(neu.stand, neu.stand_vorher + 1);
        assert_eq!(neu.saetze.len(), 1);
        assert_eq!(preis(&c), Some(sk_cost::Dez(alt.0 + 1_000_000)));
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("[log]"), "{text}");
        let abgelegt = d
            .join("firmenkatalog-staende")
            .join(format!("stand-{:04}.szk", neu.stand_vorher));
        assert_eq!(std::fs::read_to_string(abgelegt).unwrap(), vorher);
        // Ein anderer Platz ändert denselben Preis: abgelehnt, Datei bleibt
        let (mut c2, _) = Company::load(&p, false);
        c2.fuer_firma(&herkunft, &[op(200)]).unwrap();
        let fremd = std::fs::read_to_string(&p).unwrap();
        let e = c.fuer_firma(&herkunft, &[op(300)]).unwrap_err();
        assert!(e.contains("geändert"), "{e}");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), fremd);
        // Nach dem Neuladen geht es, und save_type schreibt unter der Sperre
        c.reload(false);
        c.fuer_firma(&herkunft, &[op(300)]).unwrap();
        assert_eq!(preis(&c), Some(sk_cost::Dez(alt.0 + 3_000_000)));
        std::fs::write(&lock, "anderer Platz").unwrap();
        let mut m2 = Model::from_library(c.library());
        let id = m2.defaults().exterior_wall;
        let mut t = m2.layer_set(id).unwrap().clone();
        t.layers[0].thickness = 130.0;
        assert!(m2.set_layer_set(id, t));
        let g = m2.layer_set(id).unwrap().guid;
        assert_eq!(c.save_type(&m2, g), SaveResult::Failed(GESPERRT.into()));
        std::fs::remove_file(&lock).unwrap();
        assert_eq!(c.save_type(&m2, g), SaveResult::Saved);
        assert_eq!(
            preis(&c),
            Some(sk_cost::Dez(alt.0 + 3_000_000)),
            "Kostensätze bleiben"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Review 3aj: Übernimmt ein anderer Platz die Sperre, merkt es der
    /// erste vor dem Schreiben und löscht die fremde Sperre nicht. Die
    /// temporäre Datei ist je Aufruf eine eigene; eine fremde bleibt.
    #[test]
    fn sperre_nur_eigene_und_eigene_temporaere_datei() {
        let d = dir("sperre-eigen");
        let p = d.join(FILE_NAME);
        let lock = p.with_extension("szk.lock");
        let s = sperren(&p).unwrap();
        assert!(s.gilt());
        std::fs::write(&lock, "anderer Platz").unwrap();
        assert!(!s.gilt());
        drop(s);
        assert_eq!(std::fs::read_to_string(&lock).unwrap(), "anderer Platz");
        std::fs::remove_file(&lock).unwrap();
        // fremde halb geschriebene temporäre Datei wird nicht umbenannt
        let fremd = p.with_extension("szk.tmp");
        std::fs::write(&fremd, "halb").unwrap();
        write_atomic(&p, "ganz").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "ganz");
        assert_eq!(std::fs::read_to_string(&fremd).unwrap(), "halb");
        let reste = std::fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .count();
        assert_eq!(reste, 1, "nur die fremde");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// KA-2d1 (paket-ka2 §6 „Sätze für Menschen“): schreibgeschützt bzw.
    /// in einem anderen Programm geöffnet ergibt den Satz ohne Pfad und
    /// Systemtext; der steht im Fehlerprotokoll. Ein anderer Fehler nur
    /// „Firmenkatalog speichern hat nicht geklappt.“.
    #[test]
    fn schreibfehler_als_satz() {
        let d = dir("schreibfehler-satz");
        let p = d.join(FILE_NAME);
        let (mut c, _) = Company::laden(&p, true);
        let lohn = |w| sk_cost::Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: sk_cost::Dez::ganz(w),
        };
        let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "15:30");
        type Fall = (fn() -> std::io::Error, &'static str);
        let faelle: [Fall; 3] = [
            (
                || std::io::Error::from(std::io::ErrorKind::PermissionDenied),
                "Firmenkatalog nicht gespeichert: Keine Schreibrechte für firmenkatalog.szk.",
            ),
            (
                || std::io::Error::from_raw_os_error(32),
                "Firmenkatalog nicht gespeichert: firmenkatalog.szk ist gerade in einem anderen Programm geöffnet. Dort schließen, dann nochmal versuchen.",
            ),
            (
                || std::io::Error::other("Access is denied. (os error 5)"),
                "Firmenkatalog speichern hat nicht geklappt.",
            ),
        ];
        for (art, satz) in faelle {
            crate::meldung::protokoll_im_test();
            SCHREIBFEHLER.with(|f| f.set(true));
            SCHREIBFEHLER_ART.with(|a| a.set(Some(art)));
            let e = c.fuer_firma(&h, &[lohn(70)]).unwrap_err();
            SCHREIBFEHLER.with(|f| f.set(false));
            SCHREIBFEHLER_ART.with(|a| a.set(None));
            assert_eq!(e, satz);
            let log = crate::meldung::protokoll_im_test();
            assert!(
                log.iter().any(|l| l.contains(&d.display().to_string())),
                "Pfad im Protokoll: {log:?}"
            );
        }
        c.fuer_firma(&h, &[lohn(70)]).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// Abnahme KA-2c2 durch Test (paket-ka2 §6 Nr. 8a, Koordinator 12:07):
/// fremder Stand mit anderem Satz, gleiche Änderungszeit, Schreibfehler
/// mittendrin, `[log]`.
#[cfg(test)]
mod abnahme_ka2c2 {
    use super::*;
    use std::time::SystemTime;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("skizzeo-abnahme-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn h() -> sk_cost::Herkunft {
        sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:15")
    }

    fn lohn(w: i64) -> sk_cost::Op {
        sk_cost::Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: sk_cost::Dez::ganz(w),
        }
    }

    /// (Artikel, alter Preis) des ersten Artikels mit Preis.
    fn artikel(c: &Company) -> (sk_model::Guid, sk_cost::Dez) {
        let m = Model::from_library(c.library());
        let k = sk_cost::lesen::firma_oder_werk(&m, Some(c.library()));
        let a = k.artikel.iter().find(|a| a.preis.is_some()).unwrap();
        (a.guid, a.preis.unwrap())
    }

    fn preis(g: sk_model::Guid, p: sk_cost::Dez) -> sk_cost::Op {
        sk_cost::Op::PreisSetzen {
            artikel: g,
            preis: Some(p),
            stand: "10/2026".into(),
            quelle: "Abnahme".into(),
        }
    }

    fn werte(c: &Company, g: sk_model::Guid) -> (Option<sk_cost::Dez>, Option<sk_cost::Dez>) {
        let m = Model::from_library(c.library());
        let k = sk_cost::lesen::firma_oder_werk(&m, Some(c.library()));
        (k.artikel(g).and_then(|a| a.preis), Some(k.werte.lohn))
    }

    /// F5/F6: Ein anderer Platz ändert den Lohn, dieser danach (mit altem
    /// Stand im Speicher) einen Preis: beide Werte stehen in der Datei, Stand
    /// zweimal + 1, `[log]` lückenlos, beide alten Stände abgelegt. Dasselbe,
    /// wenn die Änderungszeit der Datei zurückgesetzt wurde.
    #[test]
    fn fremder_stand_anderer_satz() {
        fremd(&[(true, false), (true, true)]);
    }

    /// Befund (d7472ce): Wie oben, aber die Firma hat noch keine eigenen
    /// Kostensätze (Startbestand, der erste Stand übernimmt den Werksbestand).
    /// Der zweite Platz wird mit „inzwischen geändert“ abgelehnt, obwohl ein
    /// anderer Satz geändert wurde: Die Werkszeilen fehlen in seinem
    /// geladenen Text und gelten deshalb als fremd geändert.
    #[test]
    fn fremder_stand_erster_firmenstand() {
        fremd(&[(false, false)]);
    }

    fn fremd(faelle: &[(bool, bool)]) {
        for &(vorbelegt, gleiche_zeit) in faelle {
            let d = dir(&format!("fremd-{vorbelegt}-{gleiche_zeit}"));
            let p = d.join(FILE_NAME);
            let (mut c, _) = Company::load(&p, true);
            if vorbelegt {
                // Die Firma hat schon eigene Kostensätze (erster Stand geschrieben)
                c.fuer_firma(&h(), &[lohn(61)]).unwrap();
                c.fuer_firma(&h(), &[lohn(60)]).unwrap();
            }
            let (g, alt) = artikel(&c);
            let mtime = std::fs::metadata(&p).unwrap().modified().unwrap();
            let (mut c2, _) = Company::load(&p, false);
            let n1 = c2.fuer_firma(&h(), &[lohn(65)]).unwrap();
            if gleiche_zeit {
                std::fs::File::options()
                    .write(true)
                    .open(&p)
                    .unwrap()
                    .set_modified(mtime)
                    .unwrap();
            }
            let neu_preis = sk_cost::Dez(alt.0 + 1_000_000);
            let n2 = c
                .fuer_firma(&h(), &[preis(g, neu_preis)])
                .unwrap_or_else(|e| {
                    panic!("vorbelegt={vorbelegt} gleiche_zeit={gleiche_zeit}: {e}")
                });
            assert_eq!(n2.stand_vorher, n1.stand, "{gleiche_zeit}");
            assert_eq!(n2.stand, n1.stand_vorher + 2);
            let (p_ist, l_ist) = werte(&c, g);
            assert_eq!(p_ist, Some(neu_preis));
            assert_eq!(l_ist, Some(sk_cost::Dez::ganz(65)), "fremder Lohn bleibt");
            let text = std::fs::read_to_string(&p).unwrap();
            let logs: Vec<&str> = text.lines().filter(|l| l.starts_with("[log]")).collect();
            assert!(
                logs.iter()
                    .any(|l| l.contains(r#"old="60""#) && l.contains(r#"new="65""#)),
                "{logs:#?}"
            );
            assert!(
                logs.iter()
                    .any(|l| l.contains(&format!("stand={}", n2.stand))),
                "{logs:#?}"
            );
            for s in [n1.stand_vorher, n2.stand_vorher] {
                let f = d
                    .join("firmenkatalog-staende")
                    .join(format!("stand-{s:04}.szk"));
                assert!(f.exists(), "{}", f.display());
            }
            assert!(!p.with_extension("szk.lock").exists());
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    /// Schreibfehler mittendrin (nach der temporären Datei, vor dem
    /// Umbenennen; die temporäre Datei ist seit Review 3aj je Aufruf eine
    /// eigene und lässt sich nicht mehr von außen blockieren): Fehlermeldung,
    /// die Firmendatei ist bytegleich, keine Sperre und keine temporäre
    /// Datei bleiben; danach geht es wieder.
    #[test]
    fn schreibfehler_laesst_die_datei_heil() {
        let d = dir("schreibfehler");
        let p = d.join(FILE_NAME);
        let (mut c, _) = Company::load(&p, true);
        let vorher = std::fs::read(&p).unwrap();
        let tmp_da = || {
            std::fs::read_dir(&d)
                .unwrap()
                .any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".tmp"))
        };
        SCHREIBFEHLER.with(|f| f.set(true));
        let e = c.fuer_firma(&h(), &[lohn(70)]).unwrap_err();
        SCHREIBFEHLER.with(|f| f.set(false));
        assert!(!tmp_da(), "keine temporäre Datei");
        // KA-2d1: ein anderer Fehler nur als Handlung, ohne Vorsatz
        assert_eq!(e, "Firmenkatalog speichern hat nicht geklappt.");
        assert_eq!(std::fs::read(&p).unwrap(), vorher, "Datei heil");
        assert!(!p.with_extension("szk.lock").exists(), "Sperre weg");
        assert_eq!(werte(&c, artikel(&c).0).1, Some(sk_cost::Dez::ganz(60)));
        let n = c.fuer_firma(&h(), &[lohn(70)]).unwrap();
        assert_eq!(n.stand, n.stand_vorher + 1);
        assert!(!tmp_da());
        assert_eq!(werte(&c, artikel(&c).0).1, Some(sk_cost::Dez::ganz(70)));
        let _ = SystemTime::now();
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// Abnahme KA-2c2 am Haus (paket-ka2 §6 Nr. 8 Abgleich, 8a F1/F7/F8, 12):
/// „Auch für neue Häuser“ über `Scene::fuer_firma`, neue und schon angelegte
/// Projekte, Abgleichzeile mit „übernehmen“ und „so lassen“.
#[cfg(test)]
mod abnahme_ka2c2_haus {
    use super::*;
    use crate::scene::Scene;
    use sk_cost::abgleich::abgleich;
    use sk_cost::{Dez, Op};

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("skizzeo-abnahme-haus-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn h() -> sk_cost::Herkunft {
        sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:40")
    }

    fn lohn(w: i64) -> Op {
        Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(w),
        }
    }

    fn preis(g: sk_model::Guid, p: Dez) -> Op {
        Op::PreisSetzen {
            artikel: g,
            preis: Some(p),
            stand: "10/2026".into(),
            quelle: "Abnahme".into(),
        }
    }

    /// Neues Haus wie `main::new_model`: Typen und Kostenkopie der Firma.
    fn neues_haus(c: &Company) -> Scene {
        let mut m = Model::from_library(c.library());
        sk_cost::neues_projekt(&mut m, Some(c.library()));
        Scene::with_model(m)
    }

    /// Die ersten `n` Artikel mit Preis.
    fn artikel(c: &Company, n: usize) -> Vec<(sk_model::Guid, Dez)> {
        let m = Model::from_library(c.library());
        sk_cost::lesen::firma_oder_werk(&m, Some(c.library()))
            .artikel
            .iter()
            .filter_map(|a| Some((a.guid, a.preis?)))
            .take(n)
            .collect()
    }

    fn werte(s: &Scene, c: &Company, g: sk_model::Guid) -> (Option<Dez>, Dez) {
        let k = sk_cost::lesen::katalog(s.model(), Some(c.library()));
        (k.artikel(g).and_then(|a| a.preis), k.werte.lohn)
    }

    /// `stand=` aus der Kopfzeile `[catalog]` der Datei.
    fn datei_stand(p: &Path) -> u32 {
        let t = std::fs::read_to_string(p).unwrap();
        let l = t.lines().find(|l| l.starts_with("[catalog]")).unwrap();
        l.split_whitespace()
            .find_map(|w| w.strip_prefix("stand="))
            .unwrap()
            .parse()
            .unwrap()
    }

    fn szo_neu(s: &Scene) -> Scene {
        let m = sk_model::szo::read_with(
            &sk_model::szo::write(s.model()),
            sk_model::GuidGen::with_seed(3),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .unwrap()
        .model;
        Scene::with_model(m)
    }

    /// Nr. 12, Nr. 8 (Abgleich) und F1: Ein schon angelegtes Haus B mit
    /// eigenem Preis; Haus A setzt den Lohn 65 „für neue Häuser“. Die Firma
    /// hat einen neuen Stand mit `[log]`, ein neues Haus rechnet mit 65, B
    /// nicht und zeigt die Abgleichzeile. „übernehmen“ in B ist ein Schritt
    /// und lässt den eigenen Preis stehen; Strg+Z bringt die Zeile zurück.
    /// „so lassen“ blendet sie aus (auch nach Speichern und Laden), bis die
    /// Firma einen neuen Stand hat.
    #[test]
    fn nr12_neue_und_angelegte_haeuser() {
        let d = dir("nr12");
        let p = d.join(FILE_NAME);
        let (mut c, _) = Company::load(&p, true);
        c.fuer_firma(&h(), &[lohn(60)]).unwrap();
        let [(g, alt)] = artikel(&c, 1)[..] else {
            panic!("Artikel")
        };
        // B: schon angelegt, mit eigenem Preis 99
        let mut b = neues_haus(&c);
        assert!(sk_cost::op::hat_kopie(b.model()));
        b.kosten_folge(
            "Preis im Projekt geändert",
            Some(c.library()),
            &h(),
            &[preis(g, Dez::ganz(99))],
        )
        .unwrap();
        assert_eq!(
            abgleich(b.model(), Some(c.library())),
            None,
            "gleicher Stand"
        );
        // A setzt den Lohn für neue Häuser
        let mut a = neues_haus(&c);
        let stand = datei_stand(&p);
        assert_eq!(
            a.fuer_firma(
                "Lohn 65,00 €/h für dieses und neue Häuser",
                &mut c,
                &h(),
                &[lohn(65)]
            ),
            Ok(None)
        );
        assert_eq!(datei_stand(&p), stand + 1);
        assert_eq!(
            a.undo_label(),
            Some("Lohn 65,00 €/h für dieses und neue Häuser")
        );
        assert_eq!(werte(&a, &c, g), (Some(alt), Dez::ganz(65)));
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(
            text.lines()
                .any(|l| l.starts_with("[log]") && l.contains(r#"new="65""#)),
            "{text}"
        );
        assert!(d
            .join("firmenkatalog-staende")
            .join(format!("stand-{stand:04}.szk"))
            .exists());
        // Neues Haus trägt den Lohn (Regel 92)
        let n = neues_haus(&c);
        assert_eq!(werte(&n, &c, g), (Some(alt), Dez::ganz(65)));
        assert_eq!(abgleich(n.model(), Some(c.library())), None);
        // B nicht, mit Abgleichzeile (F1)
        assert_eq!(werte(&b, &c, g), (Some(Dez::ganz(99)), Dez::ganz(60)));
        let ab = abgleich(b.model(), Some(c.library())).expect("Abgleichzeile in B");
        assert_eq!(
            ab.zeile(),
            "Für neue Häuser gilt Lohn 65,00 €/h (hier 60,00)"
        );
        assert_eq!(ab.stand, datei_stand(&p));
        // „übernehmen“: ein Schritt, eigener Preis bleibt
        let rev = b.model().revision();
        b.kosten_folge(
            "Werte für neue Häuser übernommen",
            Some(c.library()),
            &h(),
            &[Op::StandUebernehmen {
                saetze: ab.saetze.clone(),
            }],
        )
        .unwrap();
        assert_eq!(b.undo_label(), Some("Werte für neue Häuser übernommen"));
        assert_ne!(b.model().revision(), rev);
        assert_eq!(werte(&b, &c, g), (Some(Dez::ganz(99)), Dez::ganz(65)));
        assert_eq!(abgleich(b.model(), Some(c.library())), None);
        assert!(b.undo());
        assert_eq!(b.undo_label(), Some("Preis im Projekt geändert"));
        assert_eq!(werte(&b, &c, g), (Some(Dez::ganz(99)), Dez::ganz(60)));
        assert_eq!(abgleich(b.model(), Some(c.library())), Some(ab.clone()));
        // „so lassen“: weg, auch nach Speichern und Laden
        b.kosten_folge(
            "Werte für neue Häuser nicht übernommen",
            Some(c.library()),
            &h(),
            &[Op::AbgleichLassen { stand: ab.stand }],
        )
        .unwrap();
        assert_eq!(abgleich(b.model(), Some(c.library())), None);
        let mut b = szo_neu(&b);
        assert_eq!(abgleich(b.model(), Some(c.library())), None, "nach Laden");
        assert_eq!(werte(&b, &c, g), (Some(Dez::ganz(99)), Dez::ganz(60)));
        // Neuer Firmenstand: die Zeile kommt wieder, mit beiden Werten
        let mut x = neues_haus(&c);
        x.fuer_firma(
            "Zuschlag",
            &mut c,
            &h(),
            &[Op::FirmenwertSetzen {
                schluessel: "surcharge".into(),
                wert: Dez::ganz(12),
            }],
        )
        .unwrap();
        let ab2 = abgleich(b.model(), Some(c.library())).expect("neuer Stand");
        assert_eq!(ab2.texte.len(), 2, "{:?}", ab2.texte);
        assert!(
            ab2.zeile().contains("Lohn 65,00 €/h (hier 60,00)"),
            "{}",
            ab2.zeile()
        );
        assert!(
            ab2.zeile().contains("Zuschlag Stoff 12,00 %"),
            "{}",
            ab2.zeile()
        );
        // „übernehmen“ nach dem Laden: weiterhin ein Schritt
        b.kosten_folge(
            "Werte für neue Häuser übernommen",
            Some(c.library()),
            &h(),
            &[Op::StandUebernehmen { saetze: ab2.saetze }],
        )
        .unwrap();
        assert_eq!(werte(&b, &c, g), (Some(Dez::ganz(99)), Dez::ganz(65)));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Abgleich ab drei Unterschieden: „3 Änderungen für neue Häuser“,
    /// die Liste trägt alle drei.
    #[test]
    fn abgleich_ab_drei_werten() {
        let d = dir("drei");
        let p = d.join(FILE_NAME);
        let (mut c, _) = Company::load(&p, true);
        c.fuer_firma(&h(), &[lohn(60)]).unwrap();
        let b = neues_haus(&c);
        let ar = artikel(&c, 2);
        c.fuer_firma(
            &h(),
            &[
                lohn(65),
                preis(ar[0].0, Dez(ar[0].1 .0 + 10_000)),
                preis(ar[1].0, Dez(ar[1].1 .0 + 20_000)),
            ],
        )
        .unwrap();
        let ab = abgleich(b.model(), Some(c.library())).unwrap();
        assert_eq!(ab.texte.len(), 3, "{:?}", ab.texte);
        assert_eq!(ab.zeile(), "3 Änderungen für neue Häuser");
        assert_eq!(ab.saetze.len(), 3);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// F8: Haus B auf älterem Stand (Lohn 60) setzt einen Preis „für neue
    /// Häuser“, während die Firma schon Lohn 65 hat: Nur der Preis geht in die
    /// Firma (Lohn bleibt 65), B bekommt den Preis und behält Lohn 60; die
    /// Abgleichzeile nennt weiterhin den Lohn.
    #[test]
    fn f8_aelterer_stand_nur_der_eine_satz() {
        let d = dir("f8");
        let p = d.join(FILE_NAME);
        let (mut c, _) = Company::load(&p, true);
        c.fuer_firma(&h(), &[lohn(60)]).unwrap();
        let mut b = neues_haus(&c);
        c.fuer_firma(&h(), &[lohn(65)]).unwrap();
        let [(g, alt)] = artikel(&c, 1)[..] else {
            panic!()
        };
        let neu = Dez(alt.0 + 50_000);
        assert_eq!(
            b.fuer_firma("Preis für neue Häuser", &mut c, &h(), &[preis(g, neu)]),
            Ok(None)
        );
        assert_eq!(werte(&b, &c, g), (Some(neu), Dez::ganz(60)));
        let n = neues_haus(&c);
        assert_eq!(werte(&n, &c, g), (Some(neu), Dez::ganz(65)), "Firma");
        let ab = abgleich(b.model(), Some(c.library())).expect("Lohn bleibt anders");
        assert_eq!(
            ab.zeile(),
            "Für neue Häuser gilt Lohn 65,00 €/h (hier 60,00)"
        );
        // Strg+Z: B wieder alter Preis, Firma behält den neuen
        assert!(b.undo());
        assert_eq!(werte(&b, &c, g), (Some(alt), Dez::ganz(60)));
        assert_eq!(werte(&neues_haus(&c), &c, g), (Some(neu), Dez::ganz(65)));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// F7: Scheitert das Schreiben der Firma, ändert sich nichts: Datei
    /// bytegleich, Haus ohne Schritt; die Meldung ist der Satz für die
    /// Statuszeile, „Nur dieses Haus“ geht danach.
    #[test]
    fn f7_firma_nicht_schreibbar() {
        let d = dir("f7");
        let p = d.join(FILE_NAME);
        let (mut c, _) = Company::load(&p, true);
        c.fuer_firma(&h(), &[lohn(60)]).unwrap();
        for mut s in [neues_haus(&c), {
            // ohne Kopie (rechnet mit der Firma)
            Scene::with_model(Model::from_library(c.library()))
        }] {
            let kopie = sk_cost::op::hat_kopie(s.model());
            let vorher = std::fs::read(&p).unwrap();
            let (rev, label) = (s.model().revision(), s.undo_label());
            let text = sk_model::szo::write(s.model());
            SCHREIBFEHLER.with(|f| f.set(true));
            let e = s
                .fuer_firma("Lohn für neue Häuser", &mut c, &h(), &[lohn(65)])
                .unwrap_err();
            SCHREIBFEHLER.with(|f| f.set(false));
            assert_eq!(e, "Firmenkatalog speichern hat nicht geklappt.");
            assert_eq!(std::fs::read(&p).unwrap(), vorher, "kopie={kopie}");
            assert_eq!((s.model().revision(), s.undo_label()), (rev, label));
            assert_eq!(sk_model::szo::write(s.model()), text, "kopie={kopie}");
            assert_eq!(sk_cost::op::hat_kopie(s.model()), kopie);
            // „Nur dieses Haus“ geht
            s.kosten_folge(
                "Preis im Projekt geändert",
                Some(c.library()),
                &h(),
                &[lohn(63)],
            )
            .unwrap();
            assert_eq!(
                sk_cost::lesen::katalog(s.model(), Some(c.library()))
                    .werte
                    .lohn,
                Dez::ganz(63)
            );
            assert_eq!(std::fs::read(&p).unwrap(), vorher);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// F1 zuletzt: Nach Firmenänderungen „Typ in den Firmenkatalog“:
    /// Kostensätze und alle `[log]`-Zeilen bleiben, der Stand auch.
    #[test]
    fn f1_typ_in_den_firmenkatalog_behaelt_kosten_und_log() {
        let d = dir("f1-typ");
        let p = d.join(FILE_NAME);
        let (mut c, _) = Company::load(&p, true);
        c.fuer_firma(&h(), &[lohn(60)]).unwrap();
        c.fuer_firma(&h(), &[lohn(65)]).unwrap();
        let logs = |p: &Path| -> Vec<String> {
            std::fs::read_to_string(p)
                .unwrap()
                .lines()
                .filter(|l| l.starts_with("[log]"))
                .map(String::from)
                .collect()
        };
        let vorher = logs(&p);
        assert!(vorher.len() >= 2, "{vorher:?}");
        let stand = datei_stand(&p);
        let mut m = Model::from_library(c.library());
        let id = m.defaults().exterior_wall;
        let mut t = m.layer_set(id).unwrap().clone();
        t.layers[0].thickness += 10.0;
        assert!(m.set_layer_set(id, t));
        let g = m.layer_set(id).unwrap().guid;
        assert_eq!(c.save_type(&m, g), SaveResult::Saved);
        assert_eq!(logs(&p), vorher);
        assert_eq!(datei_stand(&p), stand);
        let n = Model::from_library(c.library());
        assert_eq!(
            sk_cost::lesen::firma_oder_werk(&n, Some(c.library()))
                .werte
                .lohn,
            Dez::ganz(65)
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// Abnahme KA-2d1 „Sätze für Menschen“ (paket-ka2 §6) am Haus: der ganze
/// Satz der Statuszeile, wenn „Auch für neue Häuser“ scheitert, Systemtext
/// nur im Fehlerprotokoll, stille Übernahme der Sperre, Abgleichzeile mit
/// Unterschied ohne Wert und Bewehrungsgrad.
#[cfg(test)]
mod abnahme_ka2d1 {
    use super::*;
    use crate::scene::Scene;
    use sk_cost::abgleich::abgleich;
    use sk_cost::{Dez, Op};

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("skizzeo-abnahme-ka2d1-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn h() -> sk_cost::Herkunft {
        sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "13:50")
    }

    fn wert(k: &str, w: i64) -> Op {
        Op::FirmenwertSetzen {
            schluessel: k.into(),
            wert: Dez::ganz(w),
        }
    }

    fn neues_haus(c: &Company) -> Scene {
        let mut m = Model::from_library(c.library());
        sk_cost::neues_projekt(&mut m, Some(c.library()));
        Scene::with_model(m)
    }

    /// Scheitert „Auch für neue Häuser“, steht in der Statuszeile genau ein
    /// Satz ohne Pfad und Systemtext, dahinter „Nichts geändert; …“; der
    /// Systemtext steht im Fehlerprotokoll. Das Haus bleibt unverändert.
    #[test]
    fn scheitern_als_ganzer_satz() {
        let d = dir("scheitern");
        let p = d.join(FILE_NAME);
        let (mut c, _) = Company::laden(&p, true);
        c.fuer_firma(&h(), &[wert("wage", 60)]).unwrap();
        type Fall = (fn() -> std::io::Error, &'static str);
        let faelle: [Fall; 3] = [
            (
                || std::io::Error::from(std::io::ErrorKind::PermissionDenied),
                "Firmenkatalog nicht gespeichert: Keine Schreibrechte für firmenkatalog.szk. Nichts geändert; „Nur dieses Haus“ geht weiterhin.",
            ),
            (
                || std::io::Error::from_raw_os_error(32),
                "Firmenkatalog nicht gespeichert: firmenkatalog.szk ist gerade in einem anderen Programm geöffnet. Dort schließen, dann nochmal versuchen. Nichts geändert; „Nur dieses Haus“ geht weiterhin.",
            ),
            (
                || std::io::Error::other("Access is denied. (os error 5)"),
                "Firmenkatalog speichern hat nicht geklappt. Nichts geändert; „Nur dieses Haus“ geht weiterhin.",
            ),
        ];
        for (art, satz) in faelle {
            let mut s = neues_haus(&c);
            let text = sk_model::szo::write(s.model());
            let vorher = std::fs::read(&p).unwrap();
            crate::meldung::protokoll_im_test();
            SCHREIBFEHLER.with(|f| f.set(true));
            SCHREIBFEHLER_ART.with(|a| a.set(Some(art)));
            let e = s
                .fuer_firma(
                    "Lohn 65,00 €/h für dieses und neue Häuser",
                    &mut c,
                    &h(),
                    &[wert("wage", 65)],
                )
                .unwrap_err();
            SCHREIBFEHLER.with(|f| f.set(false));
            SCHREIBFEHLER_ART.with(|a| a.set(None));
            let ganz = e.dazu(crate::NICHTS_GEAENDERT);
            assert_eq!(ganz, satz);
            assert!(!ganz.contains(&*d.to_string_lossy()));
            let log = crate::meldung::protokoll_im_test().join("\n");
            let system = art().to_string();
            assert!(
                log.contains(&system),
                "Systemtext „{system}“ im Protokoll: {log}"
            );
            assert_eq!(std::fs::read(&p).unwrap(), vorher);
            assert_eq!(sk_model::szo::write(s.model()), text, "Haus unverändert");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Eine liegen gebliebene Sperre wird ohne Meldung übernommen.
    #[test]
    fn liegen_gebliebene_sperre_still() {
        let d = dir("sperre");
        let p = d.join(FILE_NAME);
        let (mut c, _) = Company::laden(&p, true);
        c.fuer_firma(&h(), &[wert("wage", 60)]).unwrap();
        let lock = p.with_extension("szk.lock");
        std::fs::write(&lock, "anderer Platz").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&lock)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(60))
            .unwrap();
        let mut s = neues_haus(&c);
        assert_eq!(
            s.fuer_firma(
                "Lohn 65,00 €/h für dieses und neue Häuser",
                &mut c,
                &h(),
                &[wert("wage", 65)]
            ),
            Ok(None),
            "keine Meldung in der Statuszeile"
        );
        assert!(!lock.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Abgleichzeile über die Firma: ein Stoffanteil geändert ergibt „Für
    /// neue Häuser geändert: Stoffanteile von …“; mit Lohn und Zuschlag „3
    /// Änderungen für neue Häuser“, Tooltip-Kopf „Für neue Häuser
    /// geändert:“. Bewehrungsgrad mit dem Namen der Bauteilart, im Abgleich
    /// und im Rückgängig-Text.
    #[test]
    fn abgleich_ohne_wert_und_bewehrungsgrad() {
        let d = dir("abgleich");
        let p = d.join(FILE_NAME);
        let (mut c, _) = Company::laden(&p, true);
        c.fuer_firma(&h(), &[wert("wage", 60)]).unwrap();
        let b = neues_haus(&c);
        let m = Model::from_library(c.library());
        let k = sk_cost::lesen::firma_oder_werk(&m, Some(c.library()));
        let a = k
            .anteile
            .iter()
            .find(|a| {
                k.leistung(a.leistung)
                    .is_some_and(|l| l.kurz.contains("Porenbeton") && l.kurz.contains("17,5"))
            })
            .or_else(|| k.anteile.first())
            .expect("ein Stoffanteil");
        let kurz = k.leistung(a.leistung).unwrap().kurz.clone();
        let op = Op::StoffanteilSetzen {
            bauleistung: a.leistung,
            nr: a.nr,
            anteil: Some(match a.artikel {
                Some(g) => sk_cost::op::Stoff::Artikel {
                    artikel: g,
                    menge: Dez(a.menge.0 + 1_000),
                },
                None => sk_cost::op::Stoff::Schicht {
                    faktor: Dez(a.menge.0 + 1_000),
                },
            }),
        };
        c.fuer_firma(&h(), &[op]).unwrap();
        let ab = abgleich(b.model(), Some(c.library())).expect("Abgleichzeile");
        assert_eq!(
            ab.zeile(),
            format!("Für neue Häuser geändert: Stoffanteile von {kurz}")
        );
        c.fuer_firma(&h(), &[wert("wage", 65), wert("surcharge", 12)])
            .unwrap();
        let ab = abgleich(b.model(), Some(c.library())).unwrap();
        assert_eq!(ab.zeile(), "3 Änderungen für neue Häuser", "{:?}", ab.texte);
        assert!(
            ab.tooltip().starts_with("Für neue Häuser geändert:\n"),
            "{}",
            ab.tooltip()
        );
        // Bewehrungsgrad der Sohlplatte (Wort des Fensters, Hilfe „Sohlplatte“)
        let b2 = neues_haus(&c);
        let alt = sk_cost::lesen::katalog(b2.model(), Some(c.library()))
            .werte
            .stahl
            .iter()
            .find(|(w, _)| w == "groundslab")
            .map(|x| x.1)
            .expect("Bewehrungsgrad Sohlplatte");
        assert_eq!(alt, Dez::ganz(80), "Werk");
        let op = wert("steel.groundslab", 95);
        let bez = op.bezeichnung();
        assert!(bez.contains("Bewehrungsgrad Sohlplatte"), "{bez}");
        assert!(
            !bez.contains("groundslab") && !bez.contains("steel"),
            "{bez}"
        );
        c.fuer_firma(&h(), &[op]).unwrap();
        let ab = abgleich(b2.model(), Some(c.library())).unwrap();
        let hier = |d: Dez| d.text().replace('.', ",");
        assert_eq!(
            ab.zeile(),
            format!(
                "Für neue Häuser gilt Bewehrungsgrad Sohlplatte 95 kg/m³ (hier {})",
                hier(alt)
            )
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// Abnahme Bedienbarkeit 9.2: Die Nachfrage beim Speichern führt die Werte
/// für neue Häuser nach Satz. Zwei Artikel mit ähnlichem Namen und zwei
/// Aufwandswerte bleiben getrennt, ein späterer Wert desselben Satzes
/// ersetzt den früheren.
#[cfg(test)]
mod abnahme_bb9 {
    use super::*;
    use sk_cost::{Dez, Op};

    #[test]
    fn nachfrage_nach_satz() {
        let d = std::env::temp_dir().join("skizzeo-abnahme-bb9");
        let _ = std::fs::remove_dir_all(&d);
        let (mut c, _) = Company::laden(&d.join(FILE_NAME), true);
        let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "14:10");
        let m = Model::from_library(c.library());
        let k = sk_cost::lesen::firma_oder_werk(&m, Some(c.library()));
        let steine: Vec<_> = k
            .artikel
            .iter()
            .filter(|a| a.preis.is_some() && a.name.contains("Planstein"))
            .take(2)
            .map(|a| (a.guid, a.name.clone(), a.preis.unwrap()))
            .collect();
        assert_eq!(steine.len(), 2, "zwei Plansteine");
        let leist: Vec<_> = k
            .leistungen
            .iter()
            .filter(|l| l.stunden != Dez::NULL)
            .take(2)
            .cloned()
            .collect();
        let mut doc = crate::document::Document::new(0);
        let mut setze = |c: &mut Company, op: Op, text: String| {
            c.fuer_firma(&h, &[op]).unwrap();
            doc.fuer_neue_merken(c.zuletzt_geaendert(), &text);
            doc.fuer_neue.len()
        };
        for (i, (g, name, p)) in steine.iter().enumerate() {
            let op = Op::PreisSetzen {
                artikel: *g,
                preis: Some(Dez(p.0 + 10_000)),
                stand: "10/2026".into(),
                quelle: "Abnahme".into(),
            };
            assert_eq!(setze(&mut c, op, format!("{name} neu")), i + 1);
        }
        for (i, l) in leist.iter().enumerate() {
            let mut b = sk_cost::preis::bauleistung(l);
            b.stunden = Dez(b.stunden.0 + 100);
            let op = Op::BauleistungAendern {
                bauleistung: l.guid,
                daten: b,
            };
            let text = format!("Aufwandswert {}: neu", l.kurz);
            assert_eq!(setze(&mut c, op, text), 3 + i);
        }
        // Derselbe Stein noch einmal: ersetzt, zählt nicht dazu
        let (g, name, p) = &steine[0];
        let op = Op::PreisSetzen {
            artikel: *g,
            preis: Some(Dez(p.0 + 20_000)),
            stand: "10/2026".into(),
            quelle: "Abnahme".into(),
        };
        assert_eq!(setze(&mut c, op, format!("{name} noch neuer")), 4);
        let (frage, rest) = crate::menu::save_question(&doc, None);
        let frage = format!("{frage}\n{}", rest.unwrap_or_default());
        assert!(
            frage.contains("4 Änderungen für neue Häuser sind schon gespeichert und bleiben."),
            "{frage}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// Abnahme KA-2 Nr. 14 (Schluss): „Bauleistung wählen …“ an der grauen
/// Zeile der Dachterrasse setzt `svc=` an der Schicht des Typs, die Zeile
/// wird eine Position; Strg+Z nimmt es zurück (Datei bytegleich).
#[cfg(test)]
mod abnahme_ka2_nr14 {
    use crate::scene::Scene;
    use sk_cost::Op;

    #[test]
    fn bauleistung_waehlen_setzt_svc() {
        let m = sk_model::szo::read_with(
            include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
            sk_model::GuidGen::with_seed(1),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .unwrap()
        .model;
        let mut s = Scene::with_model(m);
        let u = sk_cost::Umfang::projekt();
        let b = s.kostenblatt(None, &u);
        let k = s.katalog(None);
        let vorher = sk_model::szo::write(s.model());
        let (z, wahl) = b
            .ohne
            .iter()
            .find_map(|z| {
                let a = sk_cost::wahl::auswahl(s.model(), &k, z)?;
                let w = a
                    .eigene
                    .first()
                    .or(a.aehnlich.first())
                    .or(a.weitere.first())?
                    .clone();
                Some((z.clone(), w))
            })
            .expect("graue Zeile mit Wahl");
        let typ = z.typ.unwrap();
        let op = Op::BauleistungZuordnen {
            typ,
            schicht: z.schicht,
            bauleistung: Some(wahl.leistung),
        };
        s.kosten_folge(
            "Bauleistung gewählt",
            None,
            &sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "14:20"),
            &[op],
        )
        .unwrap();
        assert_eq!(s.undo_label(), Some("Bauleistung gewählt"));
        let text = sk_model::szo::write(s.model());
        let svc = format!("svc={}", wahl.leistung.to_ifc());
        assert!(text.contains(&svc), "svc= an der Schicht fehlt");
        let b2 = s.kostenblatt(None, &u);
        assert!(b2
            .ohne
            .iter()
            .all(|x| x.element != z.element || x.schicht != z.schicht));
        assert!(b2.positionen.len() > b.positionen.len());
        assert!(s.undo());
        assert_eq!(sk_model::szo::write(s.model()), vorher, "Strg+Z bytegleich");
    }
}

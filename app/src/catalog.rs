//! Firmenkatalog als Datei (K2): `%APPDATA%\Skizzeo\firmenkatalog.szk` oder
//! ein anderer Ort aus den Einstellungen (`[firmenkatalog] datei=…`, auch
//! ein Netzlaufwerk). Fehlt die Datei am Vorgabeort, entsteht sie mit dem
//! Startbestand. Was nicht lesbar ist, wird zum Hinweis; Skizzeo arbeitet
//! dann mit dem eingebauten Startbestand weiter und blockiert nie.

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
    "Firmenkatalog wird gerade an einem anderen Platz gespeichert · nochmal versuchen";

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

/// Sperrt den Katalog `path`. Eine Sperre älter als 30 s wird übernommen;
/// dann kommt der Hinweis dazu.
fn sperren(path: &Path) -> Result<(Sperre, Option<String>), String> {
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
    let mut hinweis = None;
    let mut f = match anlegen() {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let alt = alter(&lock).is_some_and(|d| d > SPERRE_ALT);
            if !alt {
                return Err(GESPERRT.into());
            }
            let wer = std::fs::read_to_string(&lock).unwrap_or_default();
            let _ = std::fs::remove_file(&lock);
            hinweis = Some(format!(
                "Liegen gebliebene Sperre des Firmenkatalogs übernommen ({}).",
                wer.trim()
            ));
            anlegen().map_err(|_| GESPERRT.to_string())?
        }
        Err(e) => {
            return Err(format!(
                "Firmenkatalog {} nicht gesperrt: {e}",
                path.display()
            ))
        }
    };
    f.write_all(inhalt.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|e| format!("Firmenkatalog {} nicht gesperrt: {e}", path.display()))?;
    Ok((Sperre { path: lock, inhalt }, hinweis))
}

#[cfg(test)]
thread_local! {
    /// Test: Schreiben scheitert nach der temporären Datei, vor dem
    /// Umbenennen (die Abnahme prüft, dass die Firmendatei heil bleibt).
    static SCHREIBFEHLER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn schreibfehler_im_test() -> std::io::Result<()> {
    if SCHREIBFEHLER.with(|f| f.get()) {
        return Err(std::io::Error::other("Schreibfehler (Test)"));
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
    pub fn load(path: &Path, standard_place: bool) -> (Company, Vec<String>) {
        let mut c = Company {
            path: path.to_path_buf(),
            lib: Library::standard(),
            stamp: None,
            geladen: String::new(),
            broken: false,
            gen: 0,
        };
        let hints = c.reload(standard_place);
        (c, hints)
    }

    /// Liest die Datei neu (z. B. nach [`SaveResult::Changed`]).
    pub fn reload(&mut self, standard_place: bool) -> Vec<String> {
        let shown = self.path.display();
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
                            let mut text = String::from("Firmenkatalog:");
                            if !bearings.is_empty() {
                                text += &format!(
                                    " Deckenauflager von {} ungültig, {BEARING_TAIL}",
                                    bearings.join(", ")
                                );
                            }
                            if unknown > 0 {
                                text += &format!(" {unknown} {UNKNOWN_TAIL}");
                            }
                            hints.push(text);
                        }
                        hints
                    }
                    Err(e) => {
                        self.lib = Library::standard();
                        self.broken = true;
                        vec![format!(
                            "Firmenkatalog {shown} nicht lesbar ({e}). Skizzeo arbeitet mit dem eingebauten Startbestand."
                        )]
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
                    Err(e) => vec![format!(
                        "Firmenkatalog {shown} nicht angelegt: {e}. Skizzeo arbeitet mit dem eingebauten Startbestand."
                    )],
                }
            }
            Err(_) => {
                self.lib = Library::standard();
                self.stamp = None;
                self.geladen = String::new();
                vec![format!(
                    "Firmenkatalog {shown} nicht gefunden. Skizzeo arbeitet mit dem eingebauten Startbestand."
                )]
            }
        }
    }

    /// Ergänzt neue Werkstypen (K4) und schreibt den Katalog zurück, damit
    /// der Hinweis nur einmal kommt.
    fn add_stock(&mut self) -> Vec<String> {
        let before = write_szk(&self.lib);
        let (added, switched) = self.lib.add_stock();
        if self.lib.stock.is_empty() {
            return Vec::new();
        }
        let mut hints = Vec::new();
        if !added.is_empty() {
            hints.push(format!(
                "Firmenkatalog: neue Werkstypen ergänzt ({}).",
                added.join(", ")
            ));
        }
        if switched {
            hints.push("Firmenkatalog: Standard-Außenwand ist jetzt AW-36.".into());
        }
        // Nur bei echter Ergänzung schreiben: die Datei einer anderen Fassung
        // bleibt sonst bytegleich (Reihenfolge, fremde Zeilen)
        let text = write_szk(&self.lib);
        if text != before {
            // Unter der Sperre und nur über den gelesenen Stand; sonst bleibt
            // die Ergänzung im Speicher und kommt beim nächsten Laden wieder
            let Ok((sperre, _)) = sperren(&self.path) else {
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
                Err(e) => hints.push(format!(
                    "Firmenkatalog {} nicht ergänzt: {e}",
                    self.path.display()
                )),
            }
        }
        hints
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn library(&self) -> &Library {
        &self.lib
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
    /// der Satz für die Meldung. Liefert den neuen Stand und einen Hinweis
    /// (übernommene Sperre).
    pub fn fuer_firma(
        &mut self,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
    ) -> Result<(sk_cost::FirmaNeu, Option<String>), String> {
        if self.broken {
            return Err(format!(
                "Firmenkatalog {} ist nicht lesbar und wird nicht überschrieben",
                self.path.display()
            ));
        }
        let (sperre, hinweis) = sperren(&self.path)?;
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("Firmenkatalog nicht lesbar: {e}")),
        };
        let neu =
            sk_cost::firma_anwenden(&text, &self.geladen, sk_cost::Rolle::Admin, herkunft, ops)
                .map_err(|b| {
                    b.first()
                        .map_or_else(|| "Firmenkatalog nicht geändert".into(), |x| x.satz.clone())
                })?;
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
                    .map_err(|e| format!("Voriger Stand nicht abgelegt: {e}"))?;
            }
        }
        if !sperre.gilt() {
            return Err(GESPERRT.into());
        }
        write_atomic(&self.path, &neu.text).map_err(|e| {
            format!(
                "Firmenkatalog {} nicht gespeichert: {e}",
                self.path.display()
            )
        })?;
        drop(sperre);
        self.reload(false);
        Ok((neu, hinweis))
    }

    fn save_with(
        &mut self,
        export: impl FnOnce(&mut Library) -> bool,
        missing: &str,
    ) -> SaveResult {
        if self.broken {
            return SaveResult::Failed(format!(
                "Firmenkatalog {} ist nicht lesbar und wird nicht überschrieben",
                self.path.display()
            ));
        }
        // Unter derselben Sperre wie die Kostensätze (KA-2c2): sonst
        // schriebe es veraltete Sätze aus dem Speicher zurück
        let sperre = match sperren(&self.path) {
            Ok((s, _)) => s,
            Err(e) => return SaveResult::Failed(e),
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
            Err(e) => SaveResult::Failed(format!(
                "Firmenkatalog {} nicht gespeichert: {e}",
                self.path.display()
            )),
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
        // Liegen geblieben (älter als 30 s): übernommen, mit Hinweis
        std::fs::File::options()
            .write(true)
            .open(&lock)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(60))
            .unwrap();
        let (neu, hinweis) = c.fuer_firma(&herkunft, &[op(100)]).unwrap();
        assert!(hinweis.unwrap().contains("anderer Platz"));
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
        let (s, h) = sperren(&p).unwrap();
        assert!(h.is_none() && s.gilt());
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
            let (n1, _) = c2.fuer_firma(&h(), &[lohn(65)]).unwrap();
            if gleiche_zeit {
                std::fs::File::options()
                    .write(true)
                    .open(&p)
                    .unwrap()
                    .set_modified(mtime)
                    .unwrap();
            }
            let neu_preis = sk_cost::Dez(alt.0 + 1_000_000);
            let (n2, _) = c
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
        assert!(e.contains("nicht gespeichert"), "{e}");
        assert_eq!(std::fs::read(&p).unwrap(), vorher, "Datei heil");
        assert!(!p.with_extension("szk.lock").exists(), "Sperre weg");
        assert_eq!(werte(&c, artikel(&c).0).1, Some(sk_cost::Dez::ganz(60)));
        let (n, _) = c.fuer_firma(&h(), &[lohn(70)]).unwrap();
        assert_eq!(n.stand, n.stand_vorher + 1);
        assert!(!tmp_da());
        assert_eq!(werte(&c, artikel(&c).0).1, Some(sk_cost::Dez::ganz(70)));
        let _ = SystemTime::now();
        let _ = std::fs::remove_dir_all(&d);
    }
}

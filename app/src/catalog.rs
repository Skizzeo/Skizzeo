//! Firmenkatalog als Datei (K2): `%APPDATA%\Skizzeo\firmenkatalog.szk` oder
//! ein anderer Ort aus den Einstellungen (`[firmenkatalog] datei=…`, auch
//! ein Netzlaufwerk). Fehlt die Datei am Vorgabeort, entsteht sie mit dem
//! Startbestand. Was nicht lesbar ist, wird zum Hinweis; Skizzeo arbeitet
//! dann mit dem eingebauten Startbestand weiter und blockiert nie.

use sk_model::{export_type, Guid, Model};
use sk_model::{read_szk, write_szk, Library};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

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
    /// Änderungszeitpunkt der Datei beim Laden bzw. letzten Schreiben.
    stamp: Option<SystemTime>,
    /// Die Datei ließ sich nicht lesen: nie darüber schreiben.
    broken: bool,
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

/// Schreibt atomar: erst eine temporäre Datei daneben, dann umbenennen.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("szk.tmp");
    let res = crate::document::write_synced(&tmp, text.as_bytes())
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
            broken: false,
        };
        let hints = c.reload(standard_place);
        (c, hints)
    }

    /// Liest die Datei neu (z. B. nach [`SaveResult::Changed`]).
    pub fn reload(&mut self, standard_place: bool) -> Vec<String> {
        let shown = self.path.display();
        self.broken = false;
        match std::fs::read_to_string(&self.path) {
            Ok(text) => {
                self.stamp = modified(&self.path);
                match read_szk(&text) {
                    Ok(lib) => {
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
                match write_atomic(&self.path, &write_szk(&self.lib)) {
                    Ok(()) => {
                        self.stamp = modified(&self.path);
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
            match write_atomic(&self.path, &text) {
                Ok(()) => self.stamp = modified(&self.path),
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

    /// Speichert einen Projekttyp in den Katalog zurück. Hat sich die Datei
    /// seit dem Laden geändert, wird nichts geschrieben ([`SaveResult::Changed`]);
    /// es gibt keine Sperren.
    pub fn save_type(&mut self, m: &Model, g: Guid) -> SaveResult {
        if self.broken {
            return SaveResult::Failed(format!(
                "Firmenkatalog {} ist nicht lesbar und wird nicht überschrieben",
                self.path.display()
            ));
        }
        if modified(&self.path) != self.stamp {
            return SaveResult::Changed;
        }
        let mut lib = self.lib.clone();
        if !export_type(m, &mut lib, g) {
            return SaveResult::Failed("Typ nicht im Projekt".into());
        }
        match write_atomic(&self.path, &write_szk(&lib)) {
            Ok(()) => {
                self.lib = lib;
                self.stamp = modified(&self.path);
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
        let vorher = read_szk(alt).unwrap();
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
        let lib = read_szk(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert!(compare(&m, &lib).iter().all(|x| x.1 == TypeState::Same));
        let names: Vec<String> = std::fs::read_dir(&d)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [FILE_NAME]);
        let _ = std::fs::remove_dir_all(&d);
    }
}

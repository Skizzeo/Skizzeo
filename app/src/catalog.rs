//! Firmenkatalog als Datei (K2): `%APPDATA%\Skizzeo\firmenkatalog.szk` oder
//! ein anderer Ort aus den Einstellungen (`[firmenkatalog] datei=…`, auch
//! ein Netzlaufwerk). Fehlt die Datei am Vorgabeort, entsteht sie mit dem
//! Startbestand. Was nicht lesbar ist, wird zum Hinweis; Skizzeo arbeitet
//! dann mit dem eingebauten Startbestand weiter und blockiert nie.

use sk_model::{export_type, Guid, Model};
use sk_model::{read_szk, write_szk, Library};
use std::path::{Path, PathBuf};
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
                        self.lib = lib;
                        self.add_stock()
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
        let text = write_szk(&self.lib);
        if std::fs::read_to_string(&self.path).is_ok_and(|t| t != text) {
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

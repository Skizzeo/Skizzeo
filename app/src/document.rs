//! Das offene Projekt als Datei: Pfad, gespeicherter Stand, Laden und Speichern.

use sk_model::{szo, GuidGen, Model};
use std::path::{Path, PathBuf};

/// Dateitypen für die Dialoge.
pub const FILTERS: [(&str, &str); 2] = [
    ("Skizzeo-Projekt (*.szo)", "*.szo"),
    ("Alle Dateien (*.*)", "*.*"),
];

pub struct Document {
    /// Datei, aus der das Projekt stammt bzw. zuletzt gespeichert wurde.
    pub path: Option<PathBuf>,
    /// Modellrevision beim letzten Speichern oder Öffnen.
    saved_rev: u64,
}

impl Document {
    /// Neues, ungespeichertes Projekt; `rev` ist die Revision des leeren Modells.
    pub fn new(rev: u64) -> Document {
        Document {
            path: None,
            saved_rev: rev,
        }
    }

    pub fn opened(path: PathBuf, rev: u64) -> Document {
        Document {
            path: Some(path),
            saved_rev: rev,
        }
    }

    pub fn is_dirty(&self, model: &Model) -> bool {
        model.revision() != self.saved_rev
    }

    pub fn mark_saved(&mut self, path: PathBuf, rev: u64) {
        self.path = Some(path);
        self.saved_rev = rev;
    }

    /// Dateiname oder „Unbenannt“.
    pub fn name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map_or("Unbenannt".into(), |n| n.to_string_lossy().into_owned())
    }

    /// Text für die Titelleiste, mit `•` bei ungespeicherten Änderungen.
    #[cfg(test)]
    pub fn caption(&self, model: &Model) -> String {
        self.caption_at(model.revision())
    }

    /// Wie [`Document::caption`] für einen Modellstand.
    pub fn caption_at(&self, rev: u64) -> String {
        if rev != self.saved_rev {
            format!("{} •", self.name())
        } else {
            self.name()
        }
    }
}

/// Speichert atomar: erst `name.szo.tmp` schreiben und auf die Platte bringen,
/// dann umbenennen. Eine alte Datei bleibt heil, wenn das Schreiben scheitert
/// oder der Rechner dabei ausgeht.
pub fn save(model: &Model, path: &Path) -> Result<(), String> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let text = szo::write(model);
    write_synced(&tmp, text.as_bytes())
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("„{}“ konnte nicht gespeichert werden: {e}", path.display())
        })
}

/// Schreibt und wartet, bis die Daten auf der Platte sind. Ohne das kann nach
/// einem Absturz die umbenannte Datei leer sein, obwohl das Umbenennen schon
/// gespeichert war.
pub fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// Lädt eine Datei. Fehler als lesbarer Text; Hinweise gehen an den Aufrufer.
pub fn load(path: &Path) -> Result<szo::Loaded, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("„{}“ konnte nicht geöffnet werden: {e}", path.display()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("„{}“ ist keine Skizzeo-Datei (kein UTF-8).", path.display()))?;
    szo::read(&text, GuidGen::from_time())
        .map_err(|e| format!("„{}“ konnte nicht geöffnet werden.\n\n{e}", path.display()))
}

/// Hinweise nach dem Öffnen als kurze Meldung (höchstens zwölf Zeilen).
pub fn hints_message(hints: &[String]) -> String {
    let mut s = String::from("Das Projekt wurde geöffnet. Hinweise:\n");
    for h in hints.iter().take(12) {
        s.push_str("\n• ");
        s.push_str(h);
    }
    if hints.len() > 12 {
        s.push_str(&format!("\n… und {} weitere", hints.len() - 12));
    }
    s
}

/// Projektdatei aus der Befehlszeile (`skizzeo.exe haus.szo`).
pub fn path_from_args(args: impl Iterator<Item = String>) -> Option<PathBuf> {
    args.skip(1)
        .find(|a| !a.starts_with("--") && a.to_lowercase().ends_with(".szo"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;
    use sk_math::vec3;
    use sk_model::RefSide;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("skizzeo-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn house(s: &mut Scene) {
        let set = s.model().defaults().exterior_wall;
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
        ];
        s.add_wall(&sk_model::WallChain {
            base: 0.0,
            points: pts.to_vec(),
            closed: true,
            ref_side: RefSide::Left,
            height: 2750.0,
            layers: s.model().wall_layers(set),
            joints: Default::default(),
        });
    }

    #[test]
    fn speichern_und_oeffnen() {
        let d = dir("speichern");
        let mut s = Scene::new();
        let mut doc = Document::new(s.model().revision());
        assert_eq!(doc.caption(s.model()), "Unbenannt");
        house(&mut s);
        assert!(doc.is_dirty(s.model()));
        assert_eq!(doc.caption(s.model()), "Unbenannt •");
        let path = d.join("Haus.szo");
        save(s.model(), &path).unwrap();
        doc.mark_saved(path.clone(), s.model().revision());
        assert_eq!(doc.caption(s.model()), "Haus.szo");
        assert!(!d.join("Haus.szo.tmp").exists());
        // Überschreiben einer vorhandenen Datei
        save(s.model(), &path).unwrap();
        let l = load(&path).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(szo::write(&l.model), szo::write(s.model()));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Zeichentabelle nach dem Öffnen gleich, Baustoffe über ihre Guid verglichen.
    #[test]
    fn darstellung_nach_dem_oeffnen_gleich() {
        let mut s = Scene::new();
        house(&mut s);
        let l = szo::read(&szo::write(s.model()), GuidGen::with_seed(7)).unwrap();
        let t = Scene::with_model(l.model);
        let looks = |s: &Scene| {
            let mut v: Vec<_> = s
                .model()
                .materials()
                .iter()
                .map(|(id, m)| {
                    let k = sk_model::material_key(id) as usize;
                    (m.guid, s.table().mats[k])
                })
                .collect();
            v.sort_by_key(|x| x.0);
            v
        };
        assert_eq!(looks(&t), looks(&s));
        let rest = |s: &Scene| {
            let d = s.table();
            (
                d.drawing_edges,
                d.model_edges,
                d.paper,
                d.ground,
                d.section_line,
                d.section_ends,
                d.background,
            )
        };
        assert_eq!(rest(&t), rest(&s));
        assert_eq!(t.bounds(), s.bounds());
    }

    #[test]
    fn fehler_sind_lesbar() {
        let d = dir("fehler");
        let e = load(&d.join("gibtsnicht.szo")).err().unwrap();
        assert!(e.contains("gibtsnicht.szo"), "{e}");
        std::fs::write(d.join("alt.szo"), "SZO 4\n").unwrap();
        let e = load(&d.join("alt.szo")).err().unwrap();
        assert!(e.contains("neuerer Skizzeo-Version"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn datei_aus_der_befehlszeile() {
        let a = |v: &[&str]| path_from_args(v.iter().map(|s| s.to_string()));
        assert_eq!(a(&["skizzeo.exe"]), None);
        assert_eq!(
            a(&["skizzeo.exe", "--zeiten", "z.csv", "C:\\Haus.SZO"]),
            Some(PathBuf::from("C:\\Haus.SZO"))
        );
        assert_eq!(a(&["skizzeo.exe", "--screenshot", "a.png"]), None);
        assert_eq!(
            a(&["skizzeo.exe", "--ansicht", "schnitt", "C:\\b.szo"]),
            Some(PathBuf::from("C:\\b.szo"))
        );
        for v in crate::ui::ViewKind::ALL {
            assert_eq!(crate::ui::ViewKind::from_arg(v.arg()), Some(v));
        }
        assert_eq!(
            crate::ui::ViewKind::from_arg("Grundriss"),
            Some(crate::ui::ViewKind::Plan)
        );
        assert_eq!(crate::ui::ViewKind::from_arg("oben"), None);
    }
}

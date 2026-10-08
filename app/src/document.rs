//! Das offene Projekt als Datei: Pfad, gespeicherter Stand, Laden und Speichern.

use crate::meldung::Meldung;
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
    /// Ortszeit (Stunde, Minute) des letzten Speicherns oder Öffnens, für
    /// die Nachfrage „Änderungen speichern?“ (E17).
    pub saved_at: Option<(u8, u8)>,
    /// Werte, die seit dem letzten Speichern schon für neue Häuser gelten
    /// („Lohn 65,00 €/h“); die Nachfrage nennt sie (paket-ka2 §5, 4.5).
    pub fuer_neue: Vec<(Vec<sk_cost::SatzId>, String)>,
}

impl Document {
    /// Neues, ungespeichertes Projekt; `rev` ist die Revision des leeren Modells.
    pub fn new(rev: u64) -> Document {
        Document {
            path: None,
            saved_rev: rev,
            saved_at: None,
            fuer_neue: Vec::new(),
        }
    }

    pub fn opened(path: PathBuf, rev: u64) -> Document {
        Document {
            path: Some(path),
            saved_rev: rev,
            saved_at: None,
            fuer_neue: Vec::new(),
        }
    }

    /// Aus einer Sicherung wiederhergestellt (F-13): Pfad der gespeicherten
    /// Datei (oder keiner), aber ungespeichert, bis Strg+S sie schreibt.
    pub fn restored(path: Option<PathBuf>) -> Document {
        Document {
            path,
            saved_rev: u64::MAX,
            saved_at: None,
            fuer_neue: Vec::new(),
        }
    }

    pub fn is_dirty(&self, model: &Model) -> bool {
        model.revision() != self.saved_rev
    }

    pub fn mark_saved(&mut self, path: PathBuf, rev: u64) {
        self.path = Some(path);
        self.saved_rev = rev;
        self.fuer_neue.clear();
    }

    /// Ein Wert gilt jetzt auch für neue Häuser. Geführt wird nach den
    /// geänderten Sätzen (Bedienbarkeit 9.2): ein späterer Wert für einen
    /// derselben Sätze ersetzt den früheren; `wert` ist nur die Anzeige.
    pub fn fuer_neue_merken(&mut self, saetze: &[sk_cost::SatzId], wert: &str) {
        if saetze.is_empty() {
            return;
        }
        // Ein früherer Eintrag behält die übrigen Sätze (Bedienbarkeit
        // 10.2); sein Text nennt dann alte Werte und entfällt (leer)
        for (s, text) in &mut self.fuer_neue {
            let vorher = s.len();
            s.retain(|x| !saetze.contains(x));
            if s.len() != vorher {
                text.clear();
            }
        }
        self.fuer_neue.retain(|(s, _)| !s.is_empty());
        self.fuer_neue.push((saetze.to_vec(), wert.to_string()));
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
pub fn save(model: &Model, path: &Path) -> Result<(), Meldung> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let text = szo::write(model);
    write_synced(&tmp, text.as_bytes())
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            Meldung::aus_io("Projekt nicht gespeichert", "Projekt speichern", path, &e)
        })
}

/// Schreibt eine Tabelle (CSV) wie [`save`]: erst `name.tmp`, dann
/// umbenennen, damit eine vorhandene Datei bei einem Fehler unterwegs
/// (Platte voll, Netz weg) heil bleibt. Ist sie in einem anderen Programm
/// offen (Excel), scheitert schon das Öffnen mit dem passenden Fehler
/// („… in einem anderen Programm geöffnet“), bevor etwas geschrieben ist;
/// das Umbenennen meldete sonst nur „keine Schreibrechte“.
pub fn tabelle_schreiben(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if path.exists() {
        std::fs::OpenOptions::new().write(true).open(path)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    write_synced(&tmp, bytes)
        .and_then(|_| std::fs::rename(&tmp, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
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

/// Lädt eine Datei. Fehler als ganzer Satz mit dem Dateinamen; Pfad und
/// Lesefehler gehen ins Fehlerprotokoll. Hinweise gehen an den Aufrufer.
pub fn load(path: &Path) -> Result<szo::Loaded, Meldung> {
    let bytes = std::fs::read(path)
        .map_err(|e| Meldung::aus_io("Projekt nicht geöffnet", "Projekt öffnen", path, &e))?;
    let name = path.file_name().map_or_else(
        || "Diese Datei".into(),
        |n| n.to_string_lossy().into_owned(),
    );
    let kaputt = |grund: &str| {
        crate::meldung::protokoll(&format!("Projekt öffnen: {} – {grund}", path.display()));
    };
    let Ok(text) = String::from_utf8(bytes) else {
        kaputt("kein UTF-8");
        return Err(Meldung::mit(
            "Projekt nicht geöffnet: {} ist keine Skizzeo-Datei.",
            &[&name],
        ));
    };
    szo::read_with(&text, GuidGen::from_time(), &sk_cost::lesen::ABSCHNITTE_SZO).map_err(|e| {
        kaputt(&e.to_string());
        if e.line == 1 && e.message.contains("neuerer Skizzeo-Version") {
            Meldung::mit(
                "Projekt nicht geöffnet: {} stammt aus einer neueren Skizzeo-Fassung. Bitte Skizzeo aktualisieren.",
                &[&name],
            )
        } else if e.line == 1 {
            Meldung::mit(
                "Projekt nicht geöffnet: {} ist keine Skizzeo-Datei.",
                &[&name],
            )
        } else if e.line > 1 {
            Meldung::mit(
                "Projekt nicht geöffnet: {} ist ab Zeile {} nicht lesbar.",
                &[&name, &e.line.to_string()],
            )
        } else {
            Meldung::mit(
                "Projekt nicht geöffnet: {} ist unvollständig.",
                &[&name],
            )
        }
    })
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

    /// KA-0c: Öffnen übergibt die Kostenabschnitte an den
    /// Erweiterungsspeicher; Speichern schreibt sie bytegleich zurück.
    #[test]
    fn kostenzeilen_bleiben_beim_oeffnen_und_speichern() {
        let d = dir("kosten");
        let mut s = Scene::new();
        house(&mut s);
        let mut text = szo::write(s.model());
        text += "[rate] key=wage num=65 zukunft=1\n[costproject] key=project stand=5\n";
        let path = d.join("Haus.szo");
        std::fs::write(&path, &text).unwrap();
        let l = load(&path).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(l.model.ext("rate").count(), 1);
        assert_eq!(l.model.ext("costproject").count(), 1);
        save(&l.model, &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
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
        std::fs::write(d.join("alt.szo"), "SZO 5\n").unwrap();
        let e = load(&d.join("alt.szo")).err().unwrap();
        assert!(e.contains("neueren Skizzeo-Fassung"), "{e}");
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

/// Tabelle: ersetzt die alte Datei ganz, keine Zwischendatei bleibt;
/// scheitert es, bleibt die alte unverändert (Review 3ap).
#[cfg(test)]
mod tabelle {
    #[test]
    fn tabelle_ersetzt_ganz_oder_gar_nicht() {
        let d = std::env::temp_dir().join(format!("skizzeo-tabelle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("haus LV Rohbau.csv");
        std::fs::write(&p, "alt, viel länger als die neue Fassung").unwrap();
        super::tabelle_schreiben(&p, b"neu").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"neu");
        assert!(!d.join("haus LV Rohbau.csv.tmp").exists());
        // Ziel ist ein Ordner: scheitert, ohne etwas zu hinterlassen
        let o = d.join("ordner.csv");
        std::fs::create_dir_all(&o).unwrap();
        std::fs::write(o.join("drin"), "x").unwrap();
        assert!(super::tabelle_schreiben(&o, b"neu").is_err());
        assert_eq!(std::fs::read(o.join("drin")).unwrap(), b"x");
        assert!(!d.join("ordner.csv.tmp").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// Abnahme KA-2d1, Frage Koordinator 13:43: Sätze, die zur Laufzeit mit
/// `format!` gebaut werden und nicht über `Meldung` gehen, prüft
/// `nutzersaetze_sauber` nicht. Speichern an einen unmöglichen Ort zeigt im
/// Dialog den ganzen Pfad und den Systemtext.
#[cfg(test)]
mod abnahme_ka2d1_dialog {
    #[test]
    fn speichern_scheitert_ohne_pfad_und_systemtext() {
        let d = std::env::temp_dir().join("skizzeo-ka2d1-dialog");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        // Elternordner ist eine Datei: Speichern muss scheitern
        let datei = d.join("kein-ordner");
        std::fs::write(&datei, "x").unwrap();
        let ziel = datei.join("haus.szo");
        let e = super::save(&sk_model::Model::new(), &ziel).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        assert!(
            !e.contains(&*d.to_string_lossy()) && !e.contains("os error"),
            "Dialog zeigt Pfad oder Systemtext: {e}"
        );
    }
}

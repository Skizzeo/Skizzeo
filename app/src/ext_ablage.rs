//! Ablage der Erweiterungen (Schrittplan E5): der Ordner „Erweiterungen“
//! neben den Einstellungen, abgeschaltete Bauteile im Unterordner „Aus“.
//! Hier stehen Lesen, Prüfen beim Einlesen (neu, gleich, höher, anders,
//! kleiner), Schreiben über eine Temp-Datei, Ein/Aus und Entfernen sowie die
//! Vorabrechnung, was sich an gesetzten Exemplaren ändert. Fenster und
//! Rückfragen in [`crate::ext_verwaltung`].

use crate::ext_werkzeug::Bibliothek;
use sk_model::erweiterung::{anzeige, normal, ExtDef};
use sk_model::{ElementKind, Model};
use sk_szb::rechnen::{rechnen_mit, Rechner, MAX_SCHRITTE_PRUEFUNG};
use sk_szb::{zahl, Koerper};
use std::path::{Path, PathBuf};

/// Größte .szb (Vertrag §11), wie in der Prüfung.
pub const MAX_DATEI: u64 = sk_szb::lesen::MAX_DATEI as u64;
/// Unterordner der abgeschalteten Erweiterungen.
pub const AUS: &str = "Aus";
/// Höchstens so viele Zeilen nennt die Rückfrage, dann „und N weitere“.
pub const MAX_ZEILEN: usize = 8;

/// Eine Datei der Ablage.
#[derive(Clone, Debug)]
pub struct Eintrag {
    pub def: ExtDef,
    pub pfad: PathBuf,
    /// Liegt im Ordner selbst, nicht in „Aus“.
    pub an: bool,
    /// Hinweise der Prüfung, „Zeile n: …“ („Für Entwickler“).
    pub hinweise: Vec<String>,
}

/// Inhalt des Ordners: Einträge nach Gruppe und Name, Hinweise zu
/// abgewiesenen Dateien.
#[derive(Clone, Debug, Default)]
pub struct Ablage {
    pub dir: PathBuf,
    pub eintraege: Vec<Eintrag>,
    pub hinweise: Vec<String>,
}

/// Text einer .szb: höchstens [`MAX_DATEI`], vor dem Lesen geprüft (Review
/// 3ci), gültiges UTF-8 (Robustheit Nr. 2), Zeilenenden `\n`.
pub fn datei_text(p: &Path) -> Result<String, String> {
    let gross = || format!("Datei größer als {} KB", MAX_DATEI / 1024);
    let m = std::fs::metadata(p).map_err(|e| format!("nicht lesbar ({e})"))?;
    if m.len() > MAX_DATEI {
        return Err(gross());
    }
    let b = std::fs::read(p).map_err(|e| format!("nicht lesbar ({e})"))?;
    if b.len() as u64 > MAX_DATEI {
        return Err(gross());
    }
    match String::from_utf8(b) {
        Ok(t) => Ok(normal(&t)),
        Err(_) => Err("keine UTF-8-Textdatei".into()),
    }
}

fn szb_dateien(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("szb")))
        .collect();
    v.sort();
    v
}

impl Ablage {
    /// Liest `dir` und `dir/Aus`; jede Datei geprüft wie beim Einlesen. Ein
    /// zweiter `key` oder ein schon belegtes Präfix wird abgewiesen, mit
    /// Hinweis. Ein fehlender Ordner ist leer.
    pub fn lesen(dir: &Path) -> Ablage {
        let mut a = Ablage {
            dir: dir.to_path_buf(),
            ..Ablage::default()
        };
        let mut dateien: Vec<(PathBuf, bool)> =
            szb_dateien(dir).into_iter().map(|p| (p, true)).collect();
        dateien.extend(szb_dateien(&dir.join(AUS)).into_iter().map(|p| (p, false)));
        for (p, an) in dateien {
            let name = p
                .file_name()
                .map_or(String::new(), |n| anzeige(&n.to_string_lossy(), 80));
            let geprueft = datei_text(&p)
                .map_err(|e| vec![e])
                .and_then(|t| text_pruefen(&t));
            let (def, hinweise) = match geprueft {
                Ok(x) => x,
                Err(e) => {
                    let e = e.first().cloned().unwrap_or_default();
                    a.hinweise.push(format!("{name}: {}", anzeige(&e, 160)));
                    continue;
                }
            };
            if let Some(o) = a.eintraege.iter().find(|o| o.def.key == def.key) {
                a.hinweise.push(format!(
                    "{name}: {} steht schon in {}",
                    def.key,
                    o.pfad
                        .file_name()
                        .map_or(String::new(), |n| anzeige(&n.to_string_lossy(), 80))
                ));
                continue;
            }
            if let Some(o) = a.belegt(&def) {
                a.hinweise.push(format!(
                    "{name}: Präfix {} nutzt schon {}",
                    def.prefix(),
                    o.key
                ));
                continue;
            }
            a.eintraege.push(Eintrag {
                def,
                pfad: p,
                an,
                hinweise,
            });
        }
        a.ordnen();
        a
    }

    fn ordnen(&mut self) {
        self.eintraege.sort_by(|a, b| {
            (a.def.gruppe_rang(), a.def.name(), &a.def.key).cmp(&(
                b.def.gruppe_rang(),
                b.def.name(),
                &b.def.key,
            ))
        });
    }

    /// Andere Erweiterung der Ablage mit dem Präfix von `d`.
    fn belegt(&self, d: &ExtDef) -> Option<&ExtDef> {
        self.eintraege
            .iter()
            .map(|e| &e.def)
            .find(|o| o.key != d.key && o.prefix() == d.prefix())
    }

    pub fn eintrag(&self, key: &str) -> Option<&Eintrag> {
        self.eintraege.iter().find(|e| e.def.key == key)
    }

    /// Die eingeschalteten Definitionen fürs Werkzeug und den Katalog.
    pub fn bibliothek(&self) -> Bibliothek {
        let mut b = Bibliothek::default();
        for e in self.eintraege.iter().filter(|e| e.an) {
            b.dazu(e.def.clone());
        }
        b
    }

    /// Schreibt `d` an die Stelle des Eintrags mit gleichem `key` (an oder
    /// aus bleibt), sonst als `<key>.szb` in den Ordner, und ist der Name
    /// schon belegt, als `<key>-2.szb` usw.: erst eine
    /// Temp-Datei, dann Umbenennen, damit nie eine halbe Datei liegt.
    pub fn schreiben(&mut self, d: &ExtDef, hinweise: Vec<String>) -> Result<(), String> {
        if self.dir.as_os_str().is_empty() {
            return Err("kein Ordner für Erweiterungen".into());
        }
        let (pfad, an) = match self.eintrag(&d.key) {
            Some(e) => (e.pfad.clone(), e.an),
            // Eine fremde oder abgewiesene Datei gleichen Namens bleibt
            // (Robustheit Nr. 3, Test E5-a): dann `<key>-2.szb` usw.
            None => {
                let mut p = self.dir.join(format!("{}.szb", d.key));
                let mut n = 2;
                while p.exists() {
                    p = self.dir.join(format!("{}-{n}.szb", d.key));
                    n += 1;
                }
                (p, true)
            }
        };
        let dir = pfad.parent().unwrap_or(&self.dir).to_path_buf();
        std::fs::create_dir_all(&dir).map_err(|e| format!("Ordner nicht anlegbar ({e})"))?;
        let tmp = dir.join(format!("{}.szb.tmp", d.key));
        std::fs::write(&tmp, d.text.as_bytes())
            .and_then(|_| ersetzen(&tmp, &pfad))
            .map_err(|e| {
                let _ = std::fs::remove_file(&tmp);
                format!("nicht gespeichert ({e})")
            })?;
        self.eintraege.retain(|e| e.def.key != d.key);
        self.eintraege.push(Eintrag {
            def: d.clone(),
            pfad,
            an,
            hinweise,
        });
        self.ordnen();
        Ok(())
    }

    /// Schaltet `key` ein (zurück in den Ordner) oder aus (nach „Aus“).
    /// Präfixe sind über beide Ordner eindeutig ([`Ablage::lesen`]); eine
    /// Datei gleichen Namens am Ziel wird nie überschrieben.
    pub fn schalten(&mut self, key: &str, an: bool) -> Result<(), String> {
        let Some(i) = self.eintraege.iter().position(|e| e.def.key == key) else {
            return Err(format!("{key} fehlt"));
        };
        if self.eintraege[i].an == an {
            return Ok(());
        }
        let ziel_dir = if an {
            self.dir.clone()
        } else {
            self.dir.join(AUS)
        };
        let von = self.eintraege[i].pfad.clone();
        let name = von.file_name().map(PathBuf::from).unwrap_or_default();
        let ziel = ziel_dir.join(name);
        if ziel.exists() {
            return Err(format!(
                "{} liegt dort schon",
                anzeige(&ziel.file_name().unwrap_or_default().to_string_lossy(), 80)
            ));
        }
        std::fs::create_dir_all(&ziel_dir).map_err(|e| format!("Ordner nicht anlegbar ({e})"))?;
        std::fs::rename(&von, &ziel).map_err(|e| format!("nicht verschoben ({e})"))?;
        self.eintraege[i].pfad = ziel;
        self.eintraege[i].an = an;
        Ok(())
    }

    /// Löscht die Datei von `key`. Das Projekt behält seine Definition.
    pub fn entfernen(&mut self, key: &str) -> Result<(), String> {
        let Some(i) = self.eintraege.iter().position(|e| e.def.key == key) else {
            return Err(format!("{key} fehlt"));
        };
        std::fs::remove_file(&self.eintraege[i].pfad)
            .map_err(|e| format!("nicht gelöscht ({e})"))?;
        self.eintraege.remove(i);
        Ok(())
    }
}

/// `tmp` ersetzt `ziel`. Unter Windows schlägt `rename` auf eine vorhandene
/// Datei fehl; dann wird das Ziel erst beiseitegelegt und nach Erfolg
/// gelöscht, sonst zurückgelegt.
fn ersetzen(tmp: &Path, ziel: &Path) -> std::io::Result<()> {
    match std::fs::rename(tmp, ziel) {
        Ok(()) => Ok(()),
        Err(e) if ziel.exists() => {
            let alt = ziel.with_extension("szb.alt");
            let _ = std::fs::remove_file(&alt);
            std::fs::rename(ziel, &alt).map_err(|_| e)?;
            match std::fs::rename(tmp, ziel) {
                Ok(()) => {
                    let _ = std::fs::remove_file(&alt);
                    Ok(())
                }
                Err(e) => {
                    let _ = std::fs::rename(&alt, ziel);
                    Err(e)
                }
            }
        }
        Err(e) => Err(e),
    }
}

/// Wie sich eine eingelesene Datei zur Ablage verhält (Vertrag §5, Ende §11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fall {
    /// `key` noch nicht in der Ablage.
    Neu,
    /// Gleicher Text: nichts zu tun.
    Gleich,
    /// Höhere Version als die abgelegte.
    Hoeher(u32),
    /// Gleiche Version, anderer Inhalt.
    Anders,
    /// Kleinere Version: nur als ausdrückliches Zurücksetzen.
    Kleiner(u32),
}

/// Ergebnis der Prüfung beim Einlesen.
#[derive(Clone, Debug)]
pub struct Vorschlag {
    pub def: ExtDef,
    pub fall: Fall,
    /// Hinweise der Prüfung, „Zeile n: …“.
    pub hinweise: Vec<String>,
    /// Version der Definition im Projekt, wenn sie sich von `def` unterscheidet.
    pub projekt: Option<u32>,
    /// Was sich an gesetzten Exemplaren ändert, je Zeile.
    pub aenderungen: Vec<String>,
}

/// Prüft den Text einer .szb wie beim Einlesen (mit Grenzprüfung):
/// Definition und Hinweise, sonst die Fehler, je „Zeile n: …“.
pub fn text_pruefen(text: &str) -> Result<(ExtDef, Vec<String>), Vec<String>> {
    let text = normal(text);
    let p = sk_szb::pruefen(&text, &sk_szb::Bestand::werk(), &sk_szb::Geschoss::PROBE);
    let zeile = |b: &sk_szb::Befund| {
        let t = anzeige(&b.text, 160);
        if b.zeile > 0 {
            format!("Zeile {}: {t}", b.zeile)
        } else {
            t
        }
    };
    let fehler: Vec<String> = p
        .befunde
        .iter()
        .filter(|b| b.ist_fehler())
        .map(zeile)
        .collect();
    if !fehler.is_empty() {
        return Err(fehler);
    }
    // Ohne erneute Grenzprüfung: die lief eben
    let def = ExtDef::lesen(&text).map_err(|e| vec![e])?;
    Ok((def, p.befunde.iter().map(zeile).collect()))
}

/// Prüft den Text `text` zum Einlesen gegen Ablage und Projekt. `Err`: die
/// Fehler (Zeile und Text); dann wird nicht eingelesen.
pub fn pruefen(text: &str, ablage: &Ablage, model: &Model) -> Result<Vorschlag, Vec<String>> {
    let (def, hinweise) = text_pruefen(text)?;
    let andere = ablage.belegt(&def).or_else(|| {
        model
            .ext_defs()
            .iter()
            .find(|o| o.key != def.key && o.prefix() == def.prefix())
    });
    if let Some(o) = andere {
        return Err(vec![format!(
            "Präfix {} nutzt schon „{}“ ({})",
            def.prefix(),
            anzeige(o.name(), 60),
            o.key
        )]);
    }
    let fall = match ablage.eintrag(&def.key) {
        None => Fall::Neu,
        Some(e) if e.def.text == def.text => Fall::Gleich,
        Some(e) if def.version > e.def.version => Fall::Hoeher(e.def.version),
        Some(e) if def.version < e.def.version => Fall::Kleiner(e.def.version),
        Some(_) => Fall::Anders,
    };
    let projekt = model
        .ext_def(&def.key)
        .filter(|o| o.text != def.text)
        .map(|o| o.version);
    let aenderungen = match projekt {
        Some(_) => aenderungen(model, &def).map_err(|e| vec![e])?,
        None => Vec::new(),
    };
    Ok(Vorschlag {
        hinweise,
        def,
        fall,
        projekt,
        aenderungen,
    })
}

fn einheit(e: &str) -> String {
    match e {
        "m3" => "m³".into(),
        "m2" => "m²".into(),
        "stk" => "Stk".into(),
        x => anzeige(x, 12),
    }
}

/// Gleiche Körper bis auf Rundung (der Satzindex zählt nicht).
fn gleiche_form(a: &[Koerper], b: &[Koerper]) -> bool {
    let nah = |x: f64, y: f64| (x - y).abs() <= 1e-6 * (1.0 + x.abs().max(y.abs()));
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.baustoff == b.baustoff
                && a.ebene == b.ebene
                && a.umriss.len() == b.umriss.len()
                && a.umriss
                    .iter()
                    .zip(&b.umriss)
                    .all(|(p, q)| nah(p[0], q[0]) && nah(p[1], q[1]))
                && nah(a.von, b.von)
                && nah(a.bis, b.bis)
                && (0..3).all(|k| nah(a.ursprung[k], b.ursprung[k]))
                && nah(a.drehung, b.drehung)
        })
}

/// Was sich an den gesetzten Exemplaren ändert, wenn `neu` die Definition
/// gleichen `key`s im Projekt ersetzt: je Exemplar die geänderten Mengen
/// (alt → neu) oder „Form ändert sich“; höchstens [`MAX_ZEILEN`] Zeilen.
/// Alle Rechnungen zusammen höchstens [`MAX_SCHRITTE_PRUEFUNG`] Schritte
/// (Robustheit Nr. 17), sonst `Err`.
pub fn aenderungen(model: &Model, neu: &ExtDef) -> Result<Vec<String>, String> {
    let Some(alt) = model.ext_def(&neu.key) else {
        return Ok(Vec::new());
    };
    let mut rc = Rechner::neu(MAX_SCHRITTE_PRUEFUNG);
    let mut zeilen = Vec::new();
    let mut ids = model.ext_uses(&neu.key);
    ids.sort_by(|a, b| {
        let n = |id| model.element(id).map_or("", |e| e.number.as_str());
        n(*a).cmp(n(*b))
    });
    for id in ids {
        let Some(e) = model.element(id) else {
            continue;
        };
        let ElementKind::Ext(p) = &e.kind else {
            continue;
        };
        let g = model.ext_geschoss(e.storey);
        let a = rechnen_mit(&mut rc, &alt.def, &alt.werte(p, &g), &g);
        let b = rechnen_mit(&mut rc, &neu.def, &neu.werte(p, &g), &g);
        if rc.erschoepft {
            return Err(format!(
                "{}: zu aufwendig, um die Änderung vorab zu rechnen",
                anzeige(neu.name(), 60)
            ));
        }
        let mut was = Vec::new();
        for (i, m) in neu.def.menge.iter().enumerate() {
            let alt_i = alt.def.menge.iter().position(|o| o.key() == m.key());
            let va = alt_i.and_then(|j| a.mengen.iter().find(|(k, _)| *k == j)?.1);
            let vb = b.mengen.iter().find(|(k, _)| *k == i).and_then(|x| x.1);
            let gleich = match (va, vb) {
                (Some(x), Some(y)) => (x - y).abs() <= 5e-4 * (1.0 + x.abs()),
                (None, None) => true,
                _ => false,
            };
            if !gleich {
                let t = |v: Option<f64>| v.map_or("–".to_string(), |v| zahl(v, 3));
                was.push(format!(
                    "{} {} → {} {}",
                    anzeige(m.get("name").unwrap_or(m.key()), 40),
                    t(va),
                    t(vb),
                    einheit(m.get("einheit").unwrap_or(""))
                ));
            }
        }
        if !gleiche_form(&a.koerper, &b.koerper) || (a.z0 - b.z0).abs() > 1e-6 {
            was.insert(0, "Form ändert sich".into());
        }
        if !was.is_empty() {
            zeilen.push(format!("{}: {}", e.number, was.join(", ")));
        }
    }
    if zeilen.len() > MAX_ZEILEN {
        let n = zeilen.len() - (MAX_ZEILEN - 1);
        zeilen.truncate(MAX_ZEILEN - 1);
        zeilen.push(format!("und {n} weitere"));
    }
    Ok(zeilen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::erweiterung::ExtPart;

    const STUETZE: &str = include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb");
    const TREPPE: &str = include_str!("../../crates/sk-szb/beispiele/werk.treppe.szb");

    /// Leerer Ordner je Test im Temp-Ordner.
    fn ordner(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("skizzeo-ext-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn mit_version(t: &str, v: u32) -> String {
        t.replace("version=1 ", &format!("version={v} "))
    }

    fn projekt(text: &str, n: usize) -> Model {
        let mut m = Model::new();
        m.add_building(1);
        let eg = m
            .storeys()
            .iter()
            .find(|(_, s)| s.short == "EG" && s.building.is_some())
            .map(|(id, _)| id)
            .unwrap();
        let d = ExtDef::lesen(text).unwrap();
        m.put_ext_def(d.clone()).unwrap();
        for i in 0..n {
            m.add_ext(eg, ExtPart::new(&d, [1000.0 * i as f64, 0.0]))
                .unwrap();
        }
        m
    }

    /// tests/LIESMICH.md: Präfix-Doppel mit `test.stuetze`, höhere, gleiche
    /// (anderer Inhalt) und kleinere Version.
    #[test]
    fn faelle_beim_einlesen() {
        let dir = ordner("faelle");
        let mut a = Ablage::lesen(&dir);
        let leer = Model::new();
        let v = pruefen(STUETZE, &a, &leer).unwrap();
        assert_eq!(v.fall, Fall::Neu);
        a.schreiben(&v.def, Vec::new()).unwrap();
        assert!(dir.join("werk.stuetze.szb").is_file());
        assert!(!dir.join("werk.stuetze.szb.tmp").exists());
        assert_eq!(pruefen(STUETZE, &a, &leer).unwrap().fall, Fall::Gleich);
        // Präfix-Doppel
        let doppel = STUETZE.replace("key=werk.stuetze", "key=test.stuetze");
        let e = pruefen(&doppel, &a, &leer).unwrap_err();
        assert_eq!(
            e,
            ["Präfix ST nutzt schon „Stahlbetonstütze“ (werk.stuetze)"]
        );
        // höher, anders, kleiner
        let v2 = pruefen(&mit_version(STUETZE, 2), &a, &leer).unwrap();
        assert_eq!(v2.fall, Fall::Hoeher(1));
        let anders = STUETZE.replace("wert=240 min=200", "wert=250 min=200");
        assert_eq!(pruefen(&anders, &a, &leer).unwrap().fall, Fall::Anders);
        a.schreiben(&v2.def, Vec::new()).unwrap();
        let v1 = pruefen(STUETZE, &a, &leer).unwrap();
        assert_eq!(v1.fall, Fall::Kleiner(2));
        // Fehler: fehler.szb hat elf, nichts wird geschrieben
        let f = include_str!("../../crates/sk-szb/pruefdateien/fehler.szb");
        assert_eq!(pruefen(f, &a, &leer).unwrap_err().len(), 11);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Ein/Aus über den Unterordner, Entfernen, Lesen mit Doppeln und
    /// kaputten Dateien.
    #[test]
    fn ablage_ein_aus_entfernen() {
        let dir = ordner("ablage");
        let mut a = Ablage::lesen(&dir);
        for t in [STUETZE, TREPPE] {
            a.schreiben(&ExtDef::einlesen(t).unwrap(), Vec::new())
                .unwrap();
        }
        a.schalten("werk.treppe", false).unwrap();
        assert!(dir.join(AUS).join("werk.treppe.szb").is_file());
        let b = Ablage::lesen(&dir);
        assert_eq!(b.eintraege.len(), 2);
        assert!(b.hinweise.is_empty(), "{:?}", b.hinweise);
        let bib = b.bibliothek();
        let keys: Vec<&str> = bib.defs.iter().map(|d| d.key.as_str()).collect();
        assert_eq!(keys, ["werk.stuetze"]);
        // eine neue Version schreibt in „Aus“, bleibt aus
        a.schreiben(
            &ExtDef::einlesen(&mit_version(TREPPE, 3)).unwrap(),
            Vec::new(),
        )
        .unwrap();
        let b = Ablage::lesen(&dir);
        let t = b.eintrag("werk.treppe").unwrap();
        assert_eq!((t.def.version, t.an), (3, false));
        a.schalten("werk.treppe", true).unwrap();
        assert_eq!(Ablage::lesen(&dir).bibliothek().defs.len(), 2);
        // Doppel, Präfix-Doppel, ungültiges UTF-8, kaputte Datei
        std::fs::write(dir.join("kopie.szb"), STUETZE).unwrap();
        std::fs::write(
            dir.join("test.szb"),
            STUETZE.replace("key=werk.stuetze", "key=test.stuetze"),
        )
        .unwrap();
        std::fs::write(dir.join("roh.szb"), b"SZB 0\n\xff\xfe").unwrap();
        std::fs::write(dir.join("leer.szb"), "").unwrap();
        // Zu groß: abgewiesen, ohne sie zu lesen (Review 3ci)
        std::fs::write(dir.join("riesig.szb"), vec![b'#'; MAX_DATEI as usize + 1]).unwrap();
        let b = Ablage::lesen(&dir);
        assert_eq!(b.eintraege.len(), 2);
        assert_eq!(b.hinweise.len(), 5, "{:?}", b.hinweise);
        assert!(b
            .hinweise
            .contains(&"riesig.szb: Datei größer als 1024 KB".to_string()));
        assert!(b
            .hinweise
            .iter()
            .any(|h| h == "roh.szb: keine UTF-8-Textdatei"));
        assert!(b
            .hinweise
            .iter()
            .any(|h| h.starts_with("test.szb: Präfix ST nutzt schon")));
        a.entfernen("werk.stuetze").unwrap();
        assert!(!dir.join("werk.stuetze.szb").exists());
        // Entfernen ohne Datei: Fehler statt Panik
        assert!(a.entfernen("werk.stuetze").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Die Rückfrage nennt geänderte Mengen und Formen je Exemplar; eine
    /// Version, die nichts an den Exemplaren ändert, fragt nicht.
    #[test]
    fn aenderungen_an_exemplaren() {
        let m = projekt(STUETZE, 2);
        let a = Ablage::default();
        // nur die Version: Exemplare gleich, Projekt bekommt sie trotzdem
        let v = pruefen(&mit_version(STUETZE, 2), &a, &m).unwrap();
        assert_eq!(v.projekt, Some(1));
        assert!(v.aenderungen.is_empty(), "{:?}", v.aenderungen);
        // Standardtyp 30/30 statt 24/24: Form und Mengen
        let breiter = mit_version(STUETZE, 2).replace(
            "st24 name=\"Stütze 24/24\" werte=\"b=240; d=240\" standard=ja",
            "st24 name=\"Stütze 24/24\" werte=\"b=300; d=300\" standard=ja",
        );
        let v = pruefen(&breiter, &a, &m).unwrap();
        assert_eq!(v.aenderungen.len(), 2, "{:?}", v.aenderungen);
        let z = &v.aenderungen[0];
        assert!(
            z.starts_with("ST-001: Form ändert sich, Beton C25/30 0,"),
            "{z}"
        );
        assert!(z.contains(" m³, Schalung Stütze "), "{z}");
        assert!(v.aenderungen[1].starts_with("ST-002: "));
        // viele Exemplare: höchstens acht Zeilen
        let m = projekt(STUETZE, 12);
        let v = pruefen(&breiter, &a, &m).unwrap();
        assert_eq!(v.aenderungen.len(), MAX_ZEILEN);
        assert_eq!(v.aenderungen[7], "und 5 weitere");
    }

    /// Robustheit Nr. 3: Liegt unter `<key>.szb` schon eine andere
    /// Erweiterung (Dateiname passt nicht zum key), überschreibt das
    /// Einlesen sie nie still.
    #[test]
    fn einlesen_ueberschreibt_keine_fremde_datei() {
        let dir = ordner("fremd");
        std::fs::write(dir.join("werk.stuetze.szb"), TREPPE).unwrap();
        let mut a = Ablage::lesen(&dir);
        assert!(a.eintrag("werk.treppe").is_some());
        let v = pruefen(STUETZE, &a, &Model::new()).unwrap();
        assert_eq!(v.fall, Fall::Neu);
        let r = a.schreiben(&v.def, Vec::new());
        let neu = Ablage::lesen(&dir);
        assert!(neu.eintrag("werk.treppe").is_some(), "Treppe überschrieben");
        assert_eq!(r.is_ok(), neu.eintrag("werk.stuetze").is_some(), "{r:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! Grenzen fürs Wachstum (architektur.md §8 und §9, Bausteingrenze §8).
//!
//! Prüft in jedem `cargo test` den Quelltext des ganzen Arbeitsbereichs:
//! Dateigrößen, die Kanten zwischen den Crates und die Prüfpunkte der
//! Bausteingrenze, die sich als Textsuche prüfen lassen. Was ein Test nicht
//! sieht, bleibt beim Review (R4).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Höchstens so viele Zeilen je Datei.
const GRENZE: usize = 5_000;
/// In `sk-cost` und in den drei Kostenblättern der App, samt ihren Untermodulen.
const GRENZE_KLEIN: usize = 3_000;
const KLEIN: [&str; 3] = [
    "app/src/kosten_view",
    "app/src/ava_view",
    "app/src/schedule_view",
];

/// Dateien, die heute über ihrer Grenze liegen, mit ihrer Zeilenzahl. Sie
/// dürfen nur schrumpfen; wer eine davon anfasst, legt neuen Code in ein
/// eigenes Modul daneben (Pfadfinderregel). Ist eine Datei unter der Grenze,
/// kommt sie von der Liste, ist sie kleiner geworden, wird die Zahl gesenkt.
const UEBER: [(&str, usize); 3] = [
    ("app/src/main.rs", 9_182),
    ("app/src/schedule_view.rs", 3_101),
    ("app/src/kosten_view.rs", 3_027),
];

/// Die Kanten aus architektur.md §2: jede Crate mit den eigenen Crates, die
/// sie nennt. Eine neue Kante kommt nur zusammen mit architektur.md hierher.
const KANTEN: [(&str, &[&str]); 10] = [
    ("sk-math", &[]),
    ("sk-szb", &[]),
    ("sk-paint", &[]),
    ("sk-platform", &[]),
    ("sk-render", &["sk-math"]),
    ("sk-ui", &["sk-paint"]),
    ("sk-model", &["sk-math", "sk-szb"]),
    ("sk-cost", &["sk-math", "sk-model", "sk-szb"]),
    ("sk-ifc", &["sk-math", "sk-model"]),
    (
        "skizzeo",
        &[
            "sk-cost",
            "sk-math",
            "sk-model",
            "sk-paint",
            "sk-platform",
            "sk-render",
            "sk-szb",
            "sk-ui",
        ],
    ),
];

fn wurzel() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Alle `.rs`-Dateien unter `crates/` und `app/`, relativ zur Wurzel und
/// mit `/` getrennt.
fn quellen() -> Vec<(String, String)> {
    fn sammeln(w: &Path, d: &Path, aus: &mut Vec<(String, String)>) {
        let Ok(es) = fs::read_dir(d) else { return };
        for e in es.flatten() {
            let p = e.path();
            let name = e.file_name();
            if name == "target" || name.to_string_lossy().starts_with('.') {
                continue;
            }
            if p.is_dir() {
                sammeln(w, &p, aus);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let rel = p
                    .strip_prefix(w)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                aus.push((rel, fs::read_to_string(&p).unwrap()));
            }
        }
    }
    let w = wurzel().canonicalize().unwrap();
    let mut aus = Vec::new();
    sammeln(&w, &w.join("crates"), &mut aus);
    sammeln(&w, &w.join("app"), &mut aus);
    aus.sort();
    assert!(aus.len() > 100, "Quellen nicht gefunden unter {w:?}");
    aus
}

/// Abnahmetests gehören der Abteilung Test; sie teilt ihre Dateien selbst.
fn abnahme(rel: &str) -> bool {
    rel.rsplit('/').next().unwrap().starts_with("abnahme")
}

fn grenze_von(rel: &str) -> usize {
    let blatt = |k: &&str| {
        rel.strip_prefix(*k)
            .is_some_and(|r| r == ".rs" || r.starts_with('/'))
    };
    if rel.starts_with("crates/sk-cost/") || KLEIN.iter().any(blatt) {
        GRENZE_KLEIN
    } else {
        GRENZE
    }
}

#[test]
fn dateien_bleiben_unter_der_grenze() {
    let liste: BTreeMap<&str, usize> = UEBER.into_iter().collect();
    let mut fehler = Vec::new();
    let mut gefunden = 0;
    for (rel, text) in quellen() {
        if abnahme(&rel) {
            continue;
        }
        let n = text.lines().count();
        let g = grenze_von(&rel);
        match liste.get(rel.as_str()) {
            Some(&alt) => {
                gefunden += 1;
                if n > alt {
                    fehler.push(format!(
                        "{rel}: {n} Zeilen, darf nur schrumpfen (Liste {alt}); neuen Code in ein eigenes Modul"
                    ));
                } else if n <= g {
                    fehler.push(format!(
                        "{rel}: {n} Zeilen, jetzt unter {g}; bitte von der Liste UEBER nehmen"
                    ));
                } else if n < alt {
                    fehler.push(format!(
                        "{rel}: {n} Zeilen; bitte in UEBER von {alt} auf {n} senken"
                    ));
                }
            }
            None if n > g => fehler.push(format!(
                "{rel}: {n} Zeilen, Grenze {g} (architektur.md §8); Datei teilen"
            )),
            None => {}
        }
    }
    assert_eq!(
        gefunden,
        liste.len(),
        "eine Datei aus UEBER gibt es nicht mehr"
    );
    assert!(fehler.is_empty(), "\n{}", fehler.join("\n"));
}

/// Die eigenen Crates, die eine `Cargo.toml` in einem ihrer Abschnitte für
/// Abhängigkeiten nennt (`[dependencies]`, `[dev-dependencies]`,
/// `[target.….dependencies]` …).
fn abhaengigkeiten(toml: &str) -> (String, Vec<String>) {
    let mut name = String::new();
    let mut abschnitt = "";
    let mut deps = Vec::new();
    for z in toml.lines().map(str::trim) {
        if z.starts_with('[') {
            abschnitt = z;
            continue;
        }
        if abschnitt == "[package]" && name.is_empty() {
            if let Some(w) = z.strip_prefix("name") {
                name = w
                    .trim_start_matches([' ', '='])
                    .trim_matches('"')
                    .to_string();
            }
        }
        if abschnitt.ends_with("dependencies]") {
            let k = z.split(['=', ' ', '.']).next().unwrap_or("");
            if k.starts_with("sk-") {
                deps.push(k.to_string());
            }
        }
    }
    deps.sort();
    deps.dedup();
    (name, deps)
}

#[test]
fn crates_haengen_nur_wie_im_bild() {
    let w = wurzel();
    let mut ist = BTreeMap::new();
    let mut dirs: Vec<PathBuf> = fs::read_dir(w.join("crates"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("Cargo.toml").exists())
        .collect();
    dirs.push(w.join("app"));
    for d in dirs {
        let (name, deps) = abhaengigkeiten(&fs::read_to_string(d.join("Cargo.toml")).unwrap());
        ist.insert(name, deps);
    }
    let soll: BTreeMap<String, Vec<String>> = KANTEN
        .iter()
        .map(|(c, d)| (c.to_string(), d.iter().map(|s| s.to_string()).collect()))
        .collect();
    assert_eq!(
        ist, soll,
        "Die Kanten weichen von architektur.md §2 ab; neue Kante nur zusammen mit architektur.md und KANTEN"
    );
}

/// Zeilen ohne Kommentare und ohne die Einträge unter `#[cfg(test)]` (ein
/// `mod x;`, ein Block bis zu seiner schließenden Klammer oder eine Zeile bis
/// `;`); Dateien der Tests (`tests/`, `tests.rs`, `*_tests.rs`) fallen ganz
/// heraus. Die Klammern werden einfach gezählt, das reicht für den eigenen
/// Quelltext.
fn code(rel: &str, text: &str) -> Vec<(usize, String)> {
    let datei = rel.rsplit('/').next().unwrap();
    if rel.contains("/tests/") || datei == "tests.rs" || datei.ends_with("_tests.rs") {
        return Vec::new();
    }
    let mut aus = Vec::new();
    let mut tiefe = 0i64;
    // Tiefe beim `#[cfg(test)]` und ob der Eintrag schon eine Klammer öffnete
    let mut test: Option<(i64, bool)> = None;
    for (i, z) in text.lines().enumerate() {
        let t = z.trim_start();
        let auf = z.matches('{').count() as i64;
        let zu = z.matches('}').count() as i64;
        if test.is_none() && t.starts_with("#[cfg(test)]") {
            test = Some((tiefe, false));
            tiefe += auf - zu;
            continue;
        }
        tiefe += auf - zu;
        if let Some((d, offen)) = test {
            if t.starts_with("#[") && auf == zu {
                continue;
            }
            let offen = offen || auf > 0;
            test = if (offen && tiefe <= d) || (!offen && t.ends_with(';')) {
                None
            } else {
                Some((d, offen))
            };
            continue;
        }
        if t.starts_with("//") {
            continue;
        }
        aus.push((i + 1, z.to_string()));
    }
    aus
}

fn wort(z: &str, w: &str) -> bool {
    let teil = |c: char| c.is_alphanumeric() || c == '_';
    z.match_indices(w).any(|(i, _)| {
        !z[..i].chars().next_back().is_some_and(teil)
            && !z[i + w.len()..].chars().next().is_some_and(teil)
    })
}

#[test]
fn bausteingrenze_kosten() {
    let mut fehler = Vec::new();
    for (rel, text) in quellen() {
        let in_kosten = rel.starts_with("crates/sk-cost/");
        let im_modell = rel.starts_with("crates/sk-model/");
        for (nr, z) in code(&rel, &text) {
            let stelle = format!("{rel}:{nr}: {}", z.trim());
            if !in_kosten && !im_modell && (wort(&z, "ext_put") || wort(&z, "ext_remove")) {
                fehler.push(format!("{stelle}  (ext_put/ext_remove nur in sk-cost)"));
            }
            if rel.starts_with("crates/") && !in_kosten && z.contains("sk_cost::") {
                fehler.push(format!("{stelle}  (kein sk_cost in crates/)"));
            }
            if in_kosten {
                let t = z.trim_start();
                let statisch = ["static ", "pub static ", "pub(crate) static "]
                    .iter()
                    .any(|s| t.starts_with(s));
                let zustand = ["thread_local", "Cell", "RefCell", "Mutex", "OnceLock"]
                    .iter()
                    .any(|w| wort(&z, w));
                if statisch || zustand {
                    fehler.push(format!("{stelle}  (sk-cost hält keinen Zustand)"));
                }
                if z.contains("qto::schedule") {
                    fehler.push(format!("{stelle}  (sk-cost ruft qto::schedule nicht)"));
                }
            }
        }
    }
    assert!(
        fehler.is_empty(),
        "Bausteingrenze §8:\n{}",
        fehler.join("\n")
    );
}

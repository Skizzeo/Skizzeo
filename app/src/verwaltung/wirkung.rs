//! Wirkzeile der Verwaltung (paket-ka3a §1, Einstellungen §3 KA-3 Punkt 2,
//! Bedienbarkeit 2.17): je Referenzhaus die Summe netto vorher und mit den
//! gesammelten Änderungen. Jedes Haus rechnet mit dem Firmenkatalog, nie mit
//! seiner Kopie (S5). Beim Öffnen einmal geladen, die Mengenliste gehalten;
//! jede Eingabe rechnet nur `lesen::kosten` je Haus (paket-ka3a §3).
//!
//! KA-3a4: Referenzhäuser sind das eingebettete Standardhaus und jede
//! `*.szo` direkt im Ordner `referenzhaeuser/` neben dem Firmenkatalog,
//! nach Dateiname (Regel 106). Eine unlesbare Datei steht grau mit Befund
//! im Baum und wird nie geändert; nichts davon sperrt OK.

use sk_cost::{lesen, Cent, Umfang};
use sk_model::qto::Schedule;
use sk_model::{GuidGen, Library, Model};
use std::path::{Path, PathBuf};

/// Ordner der eigenen Referenzhäuser neben dem Firmenkatalog.
pub const ORDNER: &str = "referenzhaeuser";

/// Höchstens so viele Referenzhäuser zeigt die Wirkzeile (Regel 106).
pub const IN_DER_ZEILE: usize = 5;

/// Tooltip an „€/m² Grundfläche“ (verwaltung.md §9, Kosten A3).
pub const FLAECHE_TIPP: &str = "Grundfläche\nSumme der Grundflächen aller Geschosse, gemessen außen an der tragenden Wand, ohne Dämmung und Verblender.\nJe Geschoss zählen seine eigenen Außenwände, eine Dachterrasse also nicht.\nNicht die BGF nach DIN 277, also nicht mit BKI-Kennwerten vergleichen.\nWohnfläche folgt, sobald es Räume gibt.";

/// Ein Referenzhaus mit seiner Mengenliste.
pub struct Haus {
    pub name: String,
    pub m: Model,
    sched: Schedule,
    /// Summe netto mit dem Firmenkatalog beim Öffnen.
    pub vorher: Cent,
    /// Summe netto mit den gesammelten Änderungen.
    pub nachher: Cent,
    /// Bauleistungen, mit denen das Haus rechnet.
    pub genutzt: std::collections::HashSet<sk_model::Guid>,
    /// Das offene Haus: rechnet mit seiner Kopie, die beim OK die
    /// geänderten Sätze übernimmt (Regel 89), sonst mit der Firma.
    eigen: bool,
    /// Grundfläche in mm² (außen an der tragenden Wand), 0 ohne
    /// geschlossenen Außenwandzug.
    pub flaeche: f64,
    /// Dateiname eines eigenen Referenzhauses; `None` beim Standardhaus.
    pub datei: Option<String>,
    /// Befund 106, wenn die Datei sich nicht lesen lässt (dann rechnet
    /// das Haus nicht).
    pub fehler: Option<String>,
}

impl Haus {
    fn laden(name: &str, text: &str) -> Option<Haus> {
        let m = sk_model::szo::read_with(text, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
            .ok()?
            .model;
        Some(Haus::aus(name, m, false))
    }

    fn aus(name: &str, m: Model, eigen: bool) -> Haus {
        let sched = sk_model::qto::schedule(&m);
        let flaeche = sched.floor_area(&m);
        Haus {
            name: name.into(),
            m,
            sched,
            vorher: Cent::NULL,
            nachher: Cent::NULL,
            genutzt: Default::default(),
            eigen,
            flaeche,
            datei: None,
            fehler: None,
        }
    }

    /// Eine Datei aus dem Ordner; unlesbar: grau mit Befund 106.
    fn datei(pfad: &Path) -> Haus {
        let datei = pfad
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let name = pfad
            .file_stem()
            .map_or(datei.clone(), |n| n.to_string_lossy().into_owned());
        let gelesen = std::fs::read_to_string(pfad)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::InvalidData => "kein Text".to_string(),
                std::io::ErrorKind::PermissionDenied => "keine Leserechte".to_string(),
                _ => "nicht lesbar".to_string(),
            })
            .and_then(|t| {
                // Der Lesefehler ist für Entwickler: ins Fehlerprotokoll, der
                // Satz bleibt allgemein (Bedienbarkeit 14)
                sk_model::szo::read_with(&t, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO).map_err(
                    |e| {
                        crate::meldung::protokoll(&format!(
                            "Referenzhaus {}: {}",
                            pfad.display(),
                            e.message
                        ));
                        "keine Skizzeo-Datei oder beschädigt".to_string()
                    },
                )
            });
        let mut h = match gelesen {
            Ok(l) => Haus::aus(&name, l.model, false),
            Err(grund) => {
                let mut h = Haus::aus(&name, Model::new(), false);
                h.fehler = Some(format!(
                    "Referenzhaus {datei} lässt sich nicht lesen ({grund}); es wird nicht gerechnet."
                ));
                h
            }
        };
        h.datei = Some(datei);
        h
    }

    /// Hinweis 106 „hat keine Kostenzeile“ oder der Befund der Datei.
    pub fn hinweis(&self) -> Option<String> {
        if let Some(f) = &self.fehler {
            return Some(f.clone());
        }
        let datei = self.datei.as_ref()?;
        (self.vorher == Cent::NULL && self.nachher == Cent::NULL)
            .then(|| format!("Referenzhaus {datei} hat keine Kostenzeile."))
    }

    /// €/m² Grundfläche auf ganze €; `None` ohne Fläche.
    pub fn je_m2(&self, c: Cent) -> Option<i64> {
        (self.flaeche > 0.0).then(|| (c.0 as f64 / 100.0 / (self.flaeche / 1e6)).round() as i64)
    }

    /// Summe netto mit dem Firmenkatalog `firma` (`saetze`: die geänderten
    /// Stammsätze); merkt sich die Bauleistungen, mit denen das Haus
    /// rechnet (für „Verwendet in“).
    fn rechnen(&mut self, firma: &Library, saetze: &[sk_cost::SatzId]) -> Cent {
        use sk_cost::rechnung::Quelle;
        if self.fehler.is_some() {
            return Cent::NULL;
        }
        let k = if !self.eigen {
            lesen::firma_oder_werk(&self.m, Some(firma))
        } else if sk_cost::op::hat_kopie(&self.m) && !saetze.is_empty() {
            // Wie beim OK: die Kopie übernimmt, wo sie nicht selbst abweicht
            let op = sk_cost::Op::StandUebernehmen {
                saetze: sk_cost::op::ohne_abweichung(&self.m, saetze.to_vec()),
            };
            sk_cost::vorschau(
                &self.m,
                Some(firma),
                sk_cost::Rolle::Admin,
                std::slice::from_ref(&op),
            )
            .map_or_else(|_| lesen::katalog(&self.m, Some(firma)), |p| p.katalog)
        } else {
            lesen::katalog(&self.m, Some(firma))
        };
        let b = lesen::kosten(&self.m, &self.sched, &k, &Umfang::projekt());
        self.genutzt = b
            .positionen
            .iter()
            .filter_map(|p| match p.quelle {
                Quelle::Leistung(x) | Quelle::Geschaetzt(x) => Some(x),
                Quelle::Richtpreis(_) => None,
            })
            .collect();
        b.netto
    }
}

/// Die Referenzhäuser (KA-3a2: das Standardhaus des Werks; eigene Häuser
/// kommen mit KA-3a4).
#[derive(Default)]
pub struct Wirkung {
    /// Das offene Haus (Bedienbarkeit 13.1), wenn es etwas kostet.
    pub dieses: Option<Haus>,
    pub haeuser: Vec<Haus>,
}

impl Wirkung {
    /// `offen`: das Modell des offenen Hauses; `ordner`: der Ordner der
    /// eigenen Referenzhäuser (fehlt er, gibt es nur das Standardhaus).
    pub fn laden(firma: &Library, offen: &Model, ordner: Option<&Path>) -> Wirkung {
        let mut haeuser: Vec<Haus> = Haus::laden("Standardhaus", sk_cost::verwaltung::STANDARDHAUS)
            .into_iter()
            .collect();
        haeuser.extend(dateien(ordner).iter().map(|p| Haus::datei(p)));
        for h in &mut haeuser {
            h.vorher = h.rechnen(firma, &[]);
            h.nachher = h.vorher;
        }
        let mut dieses = Haus::aus("Dieses Haus", offen.clone(), true);
        dieses.vorher = dieses.rechnen(firma, &[]);
        dieses.nachher = dieses.vorher;
        let dieses = (dieses.vorher != Cent::NULL).then_some(dieses);
        Wirkung { dieses, haeuser }
    }

    /// Regel 97 als Sperre (paket-ka3a §3) gegen das Standardhaus.
    /// Neue Grundlage ohne die Häuser neu zu lesen (nach dem Schreiben in
    /// den Entwurf, KA-3b2): „vorher“ ist jetzt `firma`.
    pub fn grundlage(&mut self, firma: &Library) {
        for h in self.dieses.iter_mut().chain(self.haeuser.iter_mut()) {
            h.vorher = h.rechnen(firma, &[]);
            h.nachher = h.vorher;
        }
    }

    /// Summe netto jedes lesbaren Referenzhauses mit `a` und mit `b`, dazu
    /// seine Grundfläche (Vorschau, KA-3b3). Danach rechnet `rechnen` die
    /// Wirkzeile neu.
    pub fn vergleich(&mut self, a: &Library, b: &Library) -> Vec<(String, Cent, Cent, f64)> {
        self.haeuser
            .iter_mut()
            .filter(|h| h.fehler.is_none())
            .map(|h| {
                let x = h.rechnen(a, &[]);
                let y = h.rechnen(b, &[]);
                (h.name.clone(), x, y, h.flaeche)
            })
            .collect()
    }

    /// Summe netto des ersten Referenzhauses (Standardhaus) mit `firma`.
    pub fn erstes(&mut self, firma: &Library) -> Option<Cent> {
        self.haeuser
            .first_mut()
            .filter(|h| h.fehler.is_none())
            .map(|h| h.rechnen(firma, &[]))
    }

    pub fn luecken(&self, k: &sk_cost::katalog::Katalog) -> Vec<sk_cost::Befund> {
        self.haeuser.first().map_or(Vec::new(), |h| {
            sk_cost::verwaltung::luecken(k, &h.m, &h.sched)
        })
    }

    /// Nach jeder Änderung: Summen mit dem Katalog samt Änderungen
    /// (`saetze`: die dabei geänderten Stammsätze).
    pub fn rechnen(&mut self, firma: &Library, saetze: &[sk_cost::SatzId]) {
        for h in self.dieses.iter_mut().chain(self.haeuser.iter_mut()) {
            h.nachher = h.rechnen(firma, saetze);
        }
    }

    /// Die Wirkzeile: erst das offene Haus, dann die ersten fünf lesbaren
    /// Referenzhäuser.
    pub fn alle(&self) -> impl Iterator<Item = &Haus> {
        self.dieses.iter().chain(
            self.haeuser
                .iter()
                .filter(|h| h.fehler.is_none())
                .take(IN_DER_ZEILE),
        )
    }
}

/// Die `*.szo` direkt in `ordner`, nach Dateiname; Unterordner und andere
/// Endungen zählen nicht. Fehlt der Ordner: keine.
pub fn dateien(ordner: Option<&Path>) -> Vec<PathBuf> {
    let Some(Ok(rd)) = ordner.map(std::fs::read_dir) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "szo"))
        .collect();
    v.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    v
}

/// Freier Dateiname für „Aktuelles Haus als Referenzhaus“: `{name}.szo`,
/// sonst `{name} (2).szo` usw.
pub fn frei(ordner: &Path, name: &str) -> PathBuf {
    let p = ordner.join(format!("{name}.szo"));
    if !p.exists() {
        return p;
    }
    (2..)
        .map(|i| ordner.join(format!("{name} ({i}).szo")))
        .find(|p| !p.exists())
        .expect("ein freier Name")
}

/// Betrag auf ganze Euro: „60.090 €“.
pub fn euro_ganz(c: Cent) -> String {
    let e = Cent(sk_cost::geld::runden(i128::from(c.0), 100) as i64 * 100);
    let t = e.deutsch();
    format!("{} €", t.strip_suffix(",00").unwrap_or(&t))
}

/// Änderung in € und % („+412 € (+0,8 %)“, „−2.171 € (−3,5 %)“).
pub fn aenderung(vorher: Cent, nachher: Cent) -> String {
    let d = nachher.0 - vorher.0;
    let zeichen = if d < 0 { "−" } else { "+" };
    let euro = euro_ganz(Cent(d.abs()));
    match prozent(vorher, nachher) {
        Some(p) => format!("{zeichen}{euro} ({p})"),
        None => format!("{zeichen}{euro}"),
    }
}

/// Änderung in Prozent mit einer Stelle („−3,5 %“); ohne Summe vorher keine.
pub fn prozent(vorher: Cent, nachher: Cent) -> Option<String> {
    if vorher.0 == 0 {
        return None;
    }
    let d = nachher.0 - vorher.0;
    let zeichen = if d < 0 { "−" } else { "+" };
    let pm = ((d.abs() as f64) * 1000.0 / vorher.0 as f64).round() as i64;
    Some(format!("{zeichen}{},{} %", pm / 10, pm % 10))
}

/// Teile der Wirkzeile: je Haus Name, vorher (durchgestrichen) und nachher
/// oder „{Haus} unverändert {Summe}“.
pub fn zeile<'a>(haeuser: impl Iterator<Item = &'a Haus>) -> Vec<(String, Option<String>, String)> {
    haeuser
        .map(|h| {
            if h.vorher == h.nachher {
                (
                    format!("{} unverändert", h.name),
                    None,
                    euro_ganz(h.nachher),
                )
            } else {
                (
                    h.name.clone(),
                    Some(euro_ganz(h.vorher)),
                    euro_ganz(h.nachher),
                )
            }
        })
        .collect()
}

//! Wirkzeile der Verwaltung (paket-ka3a §1, Einstellungen §3 KA-3 Punkt 2,
//! Bedienbarkeit 2.17): je Referenzhaus die Summe netto vorher und mit den
//! gesammelten Änderungen. Jedes Haus rechnet mit dem Firmenkatalog, nie mit
//! seiner Kopie (S5). Beim Öffnen einmal geladen, die Mengenliste gehalten;
//! jede Eingabe rechnet nur `lesen::kosten` je Haus (paket-ka3a §3).

use sk_cost::{lesen, Cent, Umfang};
use sk_model::qto::Schedule;
use sk_model::{GuidGen, Library, Model};

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
        Haus {
            name: name.into(),
            m,
            sched,
            vorher: Cent::NULL,
            nachher: Cent::NULL,
            genutzt: Default::default(),
            eigen,
        }
    }

    /// Summe netto mit dem Firmenkatalog `firma` (`saetze`: die geänderten
    /// Stammsätze); merkt sich die Bauleistungen, mit denen das Haus
    /// rechnet (für „Verwendet in“).
    fn rechnen(&mut self, firma: &Library, saetze: &[sk_cost::SatzId]) -> Cent {
        use sk_cost::rechnung::Quelle;
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
    /// `offen`: das Modell des offenen Hauses.
    pub fn laden(firma: &Library, offen: &Model) -> Wirkung {
        let mut haeuser: Vec<Haus> = Haus::laden("Standardhaus", sk_cost::verwaltung::STANDARDHAUS)
            .into_iter()
            .collect();
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

    /// Erst das offene Haus, dann die Referenzhäuser.
    pub fn alle(&self) -> impl Iterator<Item = &Haus> {
        self.dieses.iter().chain(self.haeuser.iter())
    }
}

/// Betrag auf ganze Euro: „60.090 €“.
pub fn euro_ganz(c: Cent) -> String {
    let e = Cent(sk_cost::geld::runden(i128::from(c.0), 100) as i64 * 100);
    let t = e.deutsch();
    format!("{} €", t.strip_suffix(",00").unwrap_or(&t))
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

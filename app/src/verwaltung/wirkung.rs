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
}

impl Haus {
    fn laden(name: &str, text: &str) -> Option<Haus> {
        let m = sk_model::szo::read_with(text, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
            .ok()?
            .model;
        let sched = sk_model::qto::schedule(&m);
        Some(Haus {
            name: name.into(),
            m,
            sched,
            vorher: Cent::NULL,
            nachher: Cent::NULL,
            genutzt: Default::default(),
        })
    }

    /// Summe netto mit dem Firmenkatalog `firma`; merkt sich die
    /// Bauleistungen, mit denen das Haus rechnet (für „Verwendet in“).
    fn rechnen(&mut self, firma: &Library) -> Cent {
        use sk_cost::rechnung::Quelle;
        let k = lesen::firma_oder_werk(&self.m, Some(firma));
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
    pub haeuser: Vec<Haus>,
}

impl Wirkung {
    pub fn laden(firma: &Library) -> Wirkung {
        let mut haeuser: Vec<Haus> = Haus::laden("Standardhaus", sk_cost::verwaltung::STANDARDHAUS)
            .into_iter()
            .collect();
        for h in &mut haeuser {
            h.vorher = h.rechnen(firma);
            h.nachher = h.vorher;
        }
        Wirkung { haeuser }
    }

    /// Regel 97 als Sperre (paket-ka3a §3) gegen das Standardhaus.
    pub fn luecken(&self, k: &sk_cost::katalog::Katalog) -> Vec<sk_cost::Befund> {
        self.haeuser.first().map_or(Vec::new(), |h| {
            sk_cost::verwaltung::luecken(k, &h.m, &h.sched)
        })
    }

    /// Nach jeder Änderung: Summen mit dem Katalog samt Änderungen.
    pub fn rechnen(&mut self, firma: &Library) {
        for h in &mut self.haeuser {
            h.nachher = h.rechnen(firma);
        }
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
pub fn zeile(haeuser: &[Haus]) -> Vec<(String, Option<String>, String)> {
    haeuser
        .iter()
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

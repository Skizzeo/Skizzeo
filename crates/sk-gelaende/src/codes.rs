//! Aufnahmecodes des Vermessers. Einen bundesweiten Schlüssel gibt es nicht;
//! jedes Büro hat eigene Kürzel (Recherche Vermesserformate §4). Gängige
//! Kürzel sind vorbelegt, unbekannte zählen als Gelände. Punkte gleichen
//! Linien-Codes bilden in Nummernfolge eine Bruchkante.

use crate::lesen::Punkt;

/// Was ein Code bedeutet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bedeutung {
    Gelaende,
    Bruchkante,
    BoeschungOben,
    BoeschungUnten,
    Grenzpunkt,
    Gebaeude,
    Baum,
    Schacht,
    NichtImGelaende,
}

impl Bedeutung {
    pub const ALLE: [Bedeutung; 9] = [
        Bedeutung::Gelaende,
        Bedeutung::Bruchkante,
        Bedeutung::BoeschungOben,
        Bedeutung::BoeschungUnten,
        Bedeutung::Grenzpunkt,
        Bedeutung::Gebaeude,
        Bedeutung::Baum,
        Bedeutung::Schacht,
        Bedeutung::NichtImGelaende,
    ];

    /// Punkte dieses Codes bilden eine Linie (Bruchkante).
    pub fn linie(self) -> bool {
        matches!(
            self,
            Bedeutung::Bruchkante | Bedeutung::BoeschungOben | Bedeutung::BoeschungUnten
        )
    }

    /// Punkte dieses Codes gehen ins Gelände.
    pub fn im_gelaende(self) -> bool {
        self != Bedeutung::NichtImGelaende
    }

    /// Name in der Codetabelle des Dialogs.
    pub fn name(self) -> &'static str {
        match self {
            Bedeutung::Gelaende => "Gelände",
            Bedeutung::Bruchkante => "Bruchkante",
            Bedeutung::BoeschungOben => "Böschung oben",
            Bedeutung::BoeschungUnten => "Böschung unten",
            Bedeutung::Grenzpunkt => "Grenzpunkt",
            Bedeutung::Gebaeude => "Gebäude",
            Bedeutung::Baum => "Baum",
            Bedeutung::Schacht => "Schacht",
            Bedeutung::NichtImGelaende => "nicht ins Gelände",
        }
    }

    /// Schlüssel in der Datei.
    pub fn schluessel(self) -> &'static str {
        match self {
            Bedeutung::Gelaende => "terrain",
            Bedeutung::Bruchkante => "break",
            Bedeutung::BoeschungOben => "bank_top",
            Bedeutung::BoeschungUnten => "bank_toe",
            Bedeutung::Grenzpunkt => "boundary",
            Bedeutung::Gebaeude => "building",
            Bedeutung::Baum => "tree",
            Bedeutung::Schacht => "manhole",
            Bedeutung::NichtImGelaende => "off",
        }
    }

    pub fn aus_schluessel(s: &str) -> Option<Bedeutung> {
        Bedeutung::ALLE.into_iter().find(|b| b.schluessel() == s)
    }
}

/// Code → Bedeutung; Codes ohne Eintrag zählen als Gelände.
#[derive(Clone, Debug, PartialEq)]
pub struct Codetabelle(pub Vec<(String, Bedeutung)>);

impl Default for Codetabelle {
    /// Gängige Kürzel (plan-gelaendemodul.md §5 G1).
    fn default() -> Self {
        use Bedeutung::*;
        let v = [
            ("GP", Gelaende),
            ("GR", Grenzpunkt),
            ("GW", Gelaende),
            ("BO", Bruchkante),
            ("RI", Bruchkante),
            ("FB", Gelaende),
            ("SD", Schacht),
            ("BOK", BoeschungOben),
            ("BUK", BoeschungUnten),
            ("BK", Bruchkante),
            ("BA", Baum),
            ("GE", Gebaeude),
        ];
        Codetabelle(v.into_iter().map(|(c, b)| (c.to_string(), b)).collect())
    }
}

impl Codetabelle {
    fn finde(&self, code: &str) -> Option<Bedeutung> {
        let c = code.trim();
        self.0
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(c))
            .map(|x| x.1)
    }

    /// Bedeutung eines Codes; unbekannt oder leer = Gelände.
    pub fn bedeutung(&self, code: &str) -> Bedeutung {
        self.finde(code).unwrap_or(Bedeutung::Gelaende)
    }

    pub fn bekannt(&self, code: &str) -> bool {
        self.finde(code).is_some()
    }

    /// Eintrag setzen oder ändern.
    pub fn setze(&mut self, code: &str, b: Bedeutung) {
        let c = code.trim();
        match self.0.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(c)) {
            Some(e) => e.1 = b,
            None => self.0.push((c.to_string(), b)),
        }
    }

    /// Gefundene Codes mit Anzahl, in der Reihenfolge des ersten Auftretens
    /// (für die Tabelle im Dialog); leere Codes fehlen.
    pub fn zaehlen(&self, punkte: &[Punkt]) -> Vec<(String, usize, Bedeutung)> {
        let mut v: Vec<(String, usize, Bedeutung)> = Vec::new();
        for p in punkte.iter().filter(|p| !p.code.trim().is_empty()) {
            let c = p.code.trim();
            match v.iter_mut().find(|x| x.0.eq_ignore_ascii_case(c)) {
                Some(x) => x.1 += 1,
                None => v.push((c.to_string(), 1, self.bedeutung(c))),
            }
        }
        v
    }

    /// Bruchkanten aus Linien-Codes: je Code die Punkte in Nummernfolge,
    /// nur Punkte mit `drin`. Indizes in `punkte`.
    pub fn bruchkanten(&self, punkte: &[Punkt], drin: &[bool]) -> Vec<[usize; 2]> {
        let mut codes: Vec<&str> = Vec::new();
        for p in punkte {
            let c = p.code.trim();
            if self.bedeutung(c).linie() && !codes.iter().any(|x| x.eq_ignore_ascii_case(c)) {
                codes.push(c);
            }
        }
        let mut kanten = Vec::new();
        for c in codes {
            let mut idx: Vec<usize> = (0..punkte.len())
                .filter(|&i| {
                    drin.get(i).copied().unwrap_or(true)
                        && punkte[i].code.trim().eq_ignore_ascii_case(c)
                })
                .collect();
            idx.sort_by(|&a, &b| {
                folge(&punkte[a].nr)
                    .cmp(&folge(&punkte[b].nr))
                    .then(a.cmp(&b))
            });
            kanten.extend(idx.windows(2).map(|w| [w[0], w[1]]));
        }
        kanten
    }

    /// Codes, die nicht in der Tabelle stehen, mit Anzahl.
    pub fn unbekannt(&self, punkte: &[Punkt]) -> Vec<(String, usize)> {
        self.zaehlen(punkte)
            .into_iter()
            .filter(|(c, _, _)| !self.bekannt(c))
            .map(|(c, n, _)| (c, n))
            .collect()
    }
}

/// Sortierschlüssel einer Punktnummer: Zahlen nach Wert vor Text.
fn folge(nr: &str) -> (u8, u64, &str) {
    match nr.trim().parse::<u64>() {
        Ok(n) => (0, n, ""),
        Err(_) => (1, 0, nr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(nr: &str, code: &str) -> Punkt {
        Punkt {
            nr: nr.into(),
            e: 0.0,
            n: 0.0,
            h: 0.0,
            code: code.into(),
            text: String::new(),
            zeile: 0,
        }
    }

    #[test]
    fn linien_in_nummernfolge() {
        let t = Codetabelle::default();
        let pk = [
            p("12", "BO"),
            p("3", "bo"),
            p("100", "BO"),
            p("7", "GP"),
            p("5", "RI"),
            p("6", "RI"),
            p("4", "XY"),
        ];
        assert_eq!(t.bruchkanten(&pk, &[]), [[1, 0], [0, 2], [4, 5]]);
        assert_eq!(
            t.bruchkanten(&pk, &[true, false, true, true, true, true, true]),
            [[0, 2], [4, 5]]
        );
        assert_eq!(t.bedeutung("xy"), Bedeutung::Gelaende);
        assert_eq!(t.unbekannt(&pk), [("XY".to_string(), 1)]);
        assert_eq!(
            t.zaehlen(&pk)[0],
            ("BO".to_string(), 3, Bedeutung::Bruchkante)
        );
        let mut u = t.clone();
        u.setze("xy", Bedeutung::NichtImGelaende);
        assert!(!u.bedeutung("XY").im_gelaende());
        for b in Bedeutung::ALLE {
            assert_eq!(Bedeutung::aus_schluessel(b.schluessel()), Some(b));
        }
    }
}

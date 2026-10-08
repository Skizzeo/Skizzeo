//! Baum der Verwaltung (KA-3a2, Einstellungen §3 KA-3 Punkt 1): Äste mit
//! Anzahl, Ebene 0 fett; das Suchfeld zeigt die passenden Einträge mit
//! ihren Ästen.

use super::Verwaltung;
use sk_model::Guid;

/// Ein Eintrag im Baum.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Knoten {
    Firmenwerte,
    /// Ast „Baustoffe und Preise“ und ein Artikel darin.
    Artikel,
    ArtikelSatz(Guid),
    /// Ast „Bauleistungen“, darin Lose, Titel und Bauleistungen.
    Leistungen,
    Los(Guid),
    Titel(Guid),
    Leistung(Guid),
    /// Ast „Lose und Titel“ und ein Los oder Titel darin.
    Lose,
    LosSatz(Guid),
    Typen,
    Typ(Guid),
    Haeuser,
    Haus(usize),
    Protokoll,
    Stand(u32),
    Papierkorb,
}

/// Eine sichtbare Zeile des Baums.
#[derive(Clone, Debug, PartialEq)]
pub struct Zeile {
    pub knoten: Knoten,
    pub tiefe: u8,
    pub text: String,
    pub anzahl: Option<usize>,
    /// Hat Kinder (Pfeil davor).
    pub ast: bool,
    pub offen: bool,
}

/// Ein Eintrag mit seinen Kindern, bevor Suche und Aufklappen greifen.
struct Eintrag {
    knoten: Knoten,
    text: String,
    anzahl: Option<usize>,
    kinder: Vec<Eintrag>,
}

fn blatt(knoten: Knoten, text: impl Into<String>) -> Eintrag {
    Eintrag {
        knoten,
        text: text.into(),
        anzahl: None,
        kinder: Vec::new(),
    }
}

fn ast(knoten: Knoten, text: &str, kinder: Vec<Eintrag>, anzahl: usize) -> Eintrag {
    Eintrag {
        knoten,
        text: text.into(),
        anzahl: Some(anzahl),
        kinder,
    }
}

/// „08.10.2026 15:30“ aus `2026-10-08T15:30`.
pub fn zeit_text(z: &str) -> String {
    let (tag, uhr) = z.split_once('T').unwrap_or((z, ""));
    let mut t = tag.split('-');
    let datum = match (t.next(), t.next(), t.next()) {
        (Some(j), Some(m), Some(d)) => format!("{d}.{m}.{j}"),
        _ => tag.to_string(),
    };
    if uhr.is_empty() {
        datum
    } else {
        format!("{datum} {uhr}")
    }
}

impl Verwaltung {
    /// Der ganze Baum aus dem Katalog mit den gesammelten Änderungen.
    fn eintraege(&self) -> Vec<Eintrag> {
        let k = &self.jetzt;
        let mut artikel: Vec<_> = k.artikel.iter().filter(|a| !a.retired).collect();
        artikel.sort_by(|a, b| a.name.cmp(&b.name));
        let artikel: Vec<Eintrag> = artikel
            .iter()
            .map(|a| blatt(Knoten::ArtikelSatz(a.guid), a.name.clone()))
            .collect();
        let mut lose: Vec<_> = k
            .lose
            .iter()
            .filter(|l| l.parent.is_none() && !l.retired)
            .collect();
        lose.sort_by(|a, b| a.nr.cmp(&b.nr));
        let titel_von = |los: Guid| {
            let mut t: Vec<_> = k
                .lose
                .iter()
                .filter(|t| t.parent == Some(los) && !t.retired)
                .collect();
            t.sort_by(|a, b| a.nr.cmp(&b.nr));
            t
        };
        let mut leistungen = Vec::new();
        let mut lose_titel = Vec::new();
        let mut n_leistungen = 0;
        let mut n_lose = 0;
        for los in &lose {
            let mut im_los = Vec::new();
            let mut titel_saetze = Vec::new();
            let mut n_los = 0;
            for t in titel_von(los.guid) {
                let mut ls: Vec<_> = k
                    .leistungen
                    .iter()
                    .filter(|l| l.titel == t.guid && !l.retired)
                    .collect();
                ls.sort_by_key(|l| l.pos);
                let n = ls.len();
                n_los += n;
                let kinder = ls
                    .iter()
                    .map(|l| blatt(Knoten::Leistung(l.guid), l.kurz.clone()))
                    .collect();
                im_los.push(ast(Knoten::Titel(t.guid), &t.name, kinder, n));
                titel_saetze.push(blatt(
                    Knoten::LosSatz(t.guid),
                    format!("{} {}", t.nr, t.name),
                ));
            }
            n_leistungen += n_los;
            n_lose += 1 + titel_saetze.len();
            let n_titel = titel_saetze.len();
            leistungen.push(ast(Knoten::Los(los.guid), &los.name, im_los, n_los));
            lose_titel.push(ast(
                Knoten::LosSatz(los.guid),
                &format!("{} {}", los.nr, los.name),
                titel_saetze,
                n_titel,
            ));
        }
        let typen: Vec<Eintrag> = self
            .typen()
            .into_iter()
            .map(|(g, name)| blatt(Knoten::Typ(g), name))
            .collect();
        let haeuser: Vec<Eintrag> = self
            .wirkung
            .haeuser
            .iter()
            .enumerate()
            .map(|(i, h)| blatt(Knoten::Haus(i), h.name.clone()))
            .collect();
        let staende: Vec<Eintrag> = sk_cost::verwaltung::protokoll(&self.vorher)
            .into_iter()
            .map(|s| {
                blatt(
                    Knoten::Stand(s.stand),
                    format!("Stand {} · {}", s.stand, zeit_text(&s.zeit)),
                )
            })
            .collect();
        let mut korb = Vec::new();
        for a in k.artikel.iter().filter(|a| a.retired) {
            korb.push(blatt(Knoten::ArtikelSatz(a.guid), a.name.clone()));
        }
        for l in k.leistungen.iter().filter(|l| l.retired) {
            korb.push(blatt(Knoten::Leistung(l.guid), l.kurz.clone()));
        }
        for l in k.lose.iter().filter(|l| l.retired) {
            korb.push(blatt(
                Knoten::LosSatz(l.guid),
                format!("{} {}", l.nr, l.name),
            ));
        }
        let (n_artikel, n_typen, n_haeuser, n_staende, n_korb) = (
            artikel.len(),
            typen.len(),
            haeuser.len(),
            staende.len(),
            korb.len(),
        );
        vec![
            blatt(Knoten::Firmenwerte, "Firmenwerte"),
            ast(Knoten::Artikel, "Baustoffe und Preise", artikel, n_artikel),
            ast(
                Knoten::Leistungen,
                "Bauleistungen",
                leistungen,
                n_leistungen,
            ),
            ast(Knoten::Lose, "Lose und Titel", lose_titel, n_lose),
            ast(Knoten::Typen, "Bauteiltypen", typen, n_typen),
            ast(Knoten::Haeuser, "Referenzhäuser", haeuser, n_haeuser),
            ast(Knoten::Protokoll, "Protokoll", staende, n_staende),
            ast(Knoten::Papierkorb, "Papierkorb", korb, n_korb),
        ]
    }

    /// Die sichtbaren Zeilen: aufgeklappte Äste, bei einer Suche nur die
    /// passenden Einträge mit ihren Ästen (alle offen).
    pub(super) fn zeilen(&self) -> Vec<Zeile> {
        let q = self.suche.trim().to_lowercase();
        let mut out = Vec::new();
        for e in self.eintraege() {
            self.zeilen_von(&e, 0, &q, &mut out);
        }
        out
    }

    /// Hängt `e` und seine sichtbaren Kinder an; `false`, wenn bei einer
    /// Suche nichts davon passt.
    fn zeilen_von(&self, e: &Eintrag, tiefe: u8, q: &str, out: &mut Vec<Zeile>) -> bool {
        let passt = |t: &str| q.is_empty() || t.to_lowercase().contains(q);
        let ast = !e.kinder.is_empty();
        let offen = !q.is_empty() || self.offen.contains(&e.knoten);
        let stelle = out.len();
        out.push(Zeile {
            knoten: e.knoten.clone(),
            tiefe,
            text: e.text.clone(),
            anzahl: e.anzahl,
            ast,
            offen: ast && offen,
        });
        if !ast {
            if passt(&e.text) {
                return true;
            }
            out.pop();
            return false;
        }
        let mut treffer = passt(&e.text) && tiefe > 0;
        if offen {
            for k in &e.kinder {
                treffer |= self.zeilen_von(k, tiefe + 1, q, out);
            }
        }
        if !q.is_empty() && !treffer {
            out.truncate(stelle);
            return false;
        }
        true
    }

    /// Äste über `k` aufklappen, damit er zu sehen ist.
    pub(super) fn aufklappen_bis(&mut self, k: &Knoten) {
        let j = &self.jetzt;
        let titel_los = |t: Guid| j.los(t).and_then(|t| t.parent);
        let mut auf = |k: Knoten| {
            self.offen.insert(k);
        };
        match k {
            Knoten::ArtikelSatz(g) => auf(if j.artikel(*g).is_some_and(|a| a.retired) {
                Knoten::Papierkorb
            } else {
                Knoten::Artikel
            }),
            Knoten::Leistung(g) => match j.leistung(*g) {
                Some(l) if l.retired => auf(Knoten::Papierkorb),
                Some(l) => {
                    auf(Knoten::Leistungen);
                    if let Some(los) = titel_los(l.titel) {
                        auf(Knoten::Los(los));
                    }
                    auf(Knoten::Titel(l.titel));
                }
                None => {}
            },
            Knoten::Los(_) => auf(Knoten::Leistungen),
            Knoten::Titel(t) => {
                auf(Knoten::Leistungen);
                if let Some(los) = titel_los(*t) {
                    auf(Knoten::Los(los));
                }
            }
            Knoten::LosSatz(g) => match j.los(*g) {
                Some(l) if l.retired => auf(Knoten::Papierkorb),
                Some(l) => {
                    auf(Knoten::Lose);
                    if let Some(p) = l.parent {
                        auf(Knoten::LosSatz(p));
                    }
                }
                None => {}
            },
            Knoten::Typ(_) => auf(Knoten::Typen),
            Knoten::Haus(_) => auf(Knoten::Haeuser),
            Knoten::Stand(_) => auf(Knoten::Protokoll),
            _ => {}
        }
    }

    /// Bauteiltypen des Firmenkatalogs (Guid, Name), nach Namen.
    pub(super) fn typen(&self) -> Vec<(Guid, String)> {
        let mut t: Vec<_> = self
            .lib
            .types
            .iter()
            .map(|(_, t)| (t.guid, t.name.clone()))
            .collect();
        t.sort_by(|a, b| a.1.cmp(&b.1));
        t
    }
}

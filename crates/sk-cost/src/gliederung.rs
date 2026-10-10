//! Gliederung des Kostenblatts für Liste, CSV und später das LV (KA-2c,
//! Review 3ai): Gruppen je Gewerk, Geschoss oder Kostengruppe mit ihren
//! (Teil-)Zeilen. Jede Summe ist die Summe der Zeilen darunter
//! (Bedienbarkeit 2.1), am Ende steht der Rundungsausgleich gegen das
//! Gesamt (Regel 96). Die Ansicht formatiert nur noch; Geld rechnet allein
//! sk-cost.

use crate::geld::{Cent, Dez};
use crate::rechnung::{Ansatz, Kostenblatt, OhneZeile};
use sk_model::trade::TradeId;
use sk_model::{Guid, Model, StoreyId};

/// Wonach gegliedert wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Teilung {
    /// Gewerk → Position.
    Gewerk,
    /// Geschoss → Gewerk → Teil der Position im Geschoss.
    Geschoss,
    /// Kostengruppe 2. Ebene → 3. Ebene → Teil der Position.
    Kostengruppe,
}

/// Wofür eine Gruppe steht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Schluessel {
    Gewerk(Option<Guid>),
    Geschoss(StoreyId),
    /// Kostengruppe der 2. Ebene (300, 320, …).
    Kg2(Option<u16>),
    /// Kostengruppe der 3. Ebene (322, 331, …).
    Kg(Option<u16>),
}

/// Eine (Teil-)Zeile: Position, angezeigte Menge, die Ansatzzeilen, aus
/// denen sie besteht, und ihr Betrag. `betrag` fehlt beim NU-Preis im
/// Modus „nur Material“.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zeile {
    pub pos: usize,
    pub menge: Dez,
    pub ansatz: Vec<usize>,
    pub betrag: Option<Cent>,
}

/// Gruppe mit Untergruppen oder Zeilen und den Zeilen ohne Bauleistung
/// (Indizes in `Kostenblatt::ohne`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gruppe {
    pub schluessel: Schluessel,
    /// Summe der Beträge darunter; `None`, wenn nichts darunter einen
    /// Betrag hat.
    pub summe: Option<Cent>,
    pub gruppen: Vec<Gruppe>,
    pub zeilen: Vec<Zeile>,
    pub ohne: Vec<usize>,
}

impl Gruppe {
    fn neu(schluessel: Schluessel) -> Gruppe {
        Gruppe {
            schluessel,
            summe: None,
            gruppen: Vec::new(),
            zeilen: Vec::new(),
            ohne: Vec::new(),
        }
    }

    /// Die Gruppe ohne leere Untergruppen; `None`, wenn nichts bleibt.
    fn ohne_leere(mut self) -> Option<Gruppe> {
        self.gruppen = std::mem::take(&mut self.gruppen)
            .into_iter()
            .filter_map(Gruppe::ohne_leere)
            .collect();
        (!(self.gruppen.is_empty() && self.zeilen.is_empty() && self.ohne.is_empty()))
            .then_some(self)
    }

    /// Summe aus Zeilen und Untergruppen setzen.
    fn summieren(mut self) -> Gruppe {
        self.summe = self
            .zeilen
            .iter()
            .map(|z| z.betrag)
            .chain(self.gruppen.iter().map(|g| g.summe))
            .flatten()
            .fold(None, |acc, c| Some(acc.unwrap_or(Cent(0)) + c));
        self
    }
}

/// Welche Ansatzzeilen und Zeilen ohne Bauleistung eine Aufteilung zeigt;
/// `alle`: keine Auswahl, Positionen ohne Teilung stehen mit ihrer ganzen
/// Menge.
struct Filter<'a> {
    ansatz: &'a dyn Fn(&Ansatz) -> bool,
    ohne: &'a dyn Fn(&OhneZeile) -> bool,
    alle: bool,
}

impl Filter<'_> {
    fn alle() -> Filter<'static> {
        Filter {
            ansatz: &|_| true,
            ohne: &|_| true,
            alle: true,
        }
    }
}

/// Das gegliederte Blatt: Gruppen der obersten Ebene, Gesamtbetrag im
/// Modus und der Ausgleich = Gesamt − Σ Gruppen (0: keine Zeile).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aufteilung {
    pub gruppen: Vec<Gruppe>,
    pub gesamt: Cent,
    pub ausgleich: Cent,
}

/// Reihenfolge der Gewerke: DIN-Ordnung des Modells, ohne Gewerk zuletzt.
fn gewerk_folge(m: &Model, g: Option<Guid>) -> u16 {
    g.and_then(|g| m.trade(TradeId(g)))
        .map_or(u16::MAX, |t| t.order)
}

fn einmal<T: PartialEq>(v: &mut Vec<T>, x: T) {
    if !v.contains(&x) {
        v.push(x);
    }
}

impl Kostenblatt {
    /// Betrag einer Position für die Menge `menge` im Modus.
    fn betrag(&self, i: usize, menge: Dez, nur_material: bool) -> Option<Cent> {
        let p = &self.positionen[i];
        (!(nur_material && p.nu.is_some())).then(|| p.gp_von(menge, nur_material))
    }

    /// Gesamtbetrag im Modus: Netto bzw. „nur Material“.
    pub fn gesamt(&self, nur_material: bool) -> Cent {
        if nur_material {
            self.nur_material
        } else {
            self.netto
        }
    }

    /// Zeile der Position `i` mit den Ansatzzeilen, für die `f` gilt.
    fn teilzeile(
        &self,
        i: usize,
        f: impl Fn(&crate::rechnung::Ansatz) -> bool,
        ganz: bool,
        nur_material: bool,
    ) -> Option<Zeile> {
        let p = &self.positionen[i];
        let ansatz: Vec<usize> = (0..p.ansatz.len()).filter(|&j| f(&p.ansatz[j])).collect();
        if ansatz.is_empty() {
            return None;
        }
        let menge = if ganz {
            p.menge
        } else {
            p.teile(|a| f(a))
                .into_iter()
                .find(|t| t.0)
                .map_or(Dez::NULL, |t| t.1)
        };
        Some(Zeile {
            pos: i,
            menge,
            ansatz,
            betrag: self.betrag(i, menge, nur_material),
        })
    }

    /// Das Blatt gegliedert nach `t` im Modus (`nur_material`). Namen und
    /// Reihenfolge der Gewerke kommen aus dem Modell.
    pub fn aufteilung(&self, m: &Model, t: Teilung, nur_material: bool) -> Aufteilung {
        let gruppen = self.gruppen(m, t, nur_material, &Filter::alle());
        let gesamt = self.gesamt(nur_material);
        let ausgleich = gesamt - gruppen.iter().filter_map(|g| g.summe).sum::<Cent>();
        Aufteilung {
            gruppen,
            gesamt,
            ausgleich,
        }
    }

    /// Nur der Teil des Blatts, der an bestimmten Bauteilen hängt (Kosten
    /// zu einer Auswahl im Modell, Jörn 10.10.): je Position die
    /// Ansatzzeilen, für die `ansatz` gilt, mit ihrer Teilmenge und deren
    /// Betrag; Zeilen ohne Bauleistung, für die `ohne` gilt. Gruppen ohne
    /// Zeile entfallen. Das Gesamt ist die Summe der Gruppen, einen
    /// Rundungsausgleich gibt es hier nicht.
    pub fn aufteilung_fuer(
        &self,
        m: &Model,
        t: Teilung,
        nur_material: bool,
        ansatz: &dyn Fn(&Ansatz) -> bool,
        ohne: &dyn Fn(&OhneZeile) -> bool,
    ) -> Aufteilung {
        let f = Filter {
            ansatz,
            ohne,
            alle: false,
        };
        let gruppen: Vec<Gruppe> = self
            .gruppen(m, t, nur_material, &f)
            .into_iter()
            .filter_map(Gruppe::ohne_leere)
            .collect();
        let gesamt = gruppen.iter().filter_map(|g| g.summe).sum::<Cent>();
        Aufteilung {
            gruppen,
            gesamt,
            ausgleich: Cent(0),
        }
    }

    fn gruppen(&self, m: &Model, t: Teilung, nur_material: bool, f: &Filter) -> Vec<Gruppe> {
        match t {
            Teilung::Gewerk => self.nach_gewerk(m, nur_material, f),
            Teilung::Geschoss => self.nach_geschoss_gruppen(m, nur_material, f),
            Teilung::Kostengruppe => self.nach_kg_gruppen(nur_material, f),
        }
    }

    fn nach_gewerk(&self, m: &Model, nur_material: bool, f: &Filter) -> Vec<Gruppe> {
        let mut gewerke: Vec<Option<Guid>> = Vec::new();
        for g in self
            .positionen
            .iter()
            .map(|p| p.gewerk)
            .chain(self.ohne.iter().map(|o| o.gewerk))
        {
            einmal(&mut gewerke, g);
        }
        gewerke.sort_by_key(|g| gewerk_folge(m, *g));
        gewerke
            .into_iter()
            .map(|g| {
                let mut gr = Gruppe::neu(Schluessel::Gewerk(g));
                gr.zeilen = (0..self.positionen.len())
                    .filter(|&i| self.positionen[i].gewerk == g)
                    .filter_map(|i| self.teilzeile(i, |a| (f.ansatz)(a), f.alle, nur_material))
                    .collect();
                gr.ohne = (0..self.ohne.len())
                    .filter(|&j| self.ohne[j].gewerk == g && (f.ohne)(&self.ohne[j]))
                    .collect();
                gr.summieren()
            })
            .collect()
    }

    fn nach_geschoss_gruppen(&self, m: &Model, nur_material: bool, f: &Filter) -> Vec<Gruppe> {
        let mut geschosse: Vec<StoreyId> = self.nach_geschoss.iter().map(|x| x.0).collect();
        for o in &self.ohne {
            einmal(&mut geschosse, o.geschoss);
        }
        geschosse
            .into_iter()
            .map(|st| {
                let mut gewerke: Vec<Option<Guid>> = Vec::new();
                for p in &self.positionen {
                    if p.ansatz.iter().any(|a| a.geschoss == st) {
                        einmal(&mut gewerke, p.gewerk);
                    }
                }
                for o in self.ohne.iter().filter(|o| o.geschoss == st) {
                    einmal(&mut gewerke, o.gewerk);
                }
                gewerke.sort_by_key(|g| gewerk_folge(m, *g));
                let mut gr = Gruppe::neu(Schluessel::Geschoss(st));
                gr.gruppen = gewerke
                    .into_iter()
                    .map(|g| {
                        let mut sub = Gruppe::neu(Schluessel::Gewerk(g));
                        sub.zeilen = (0..self.positionen.len())
                            .filter(|&i| self.positionen[i].gewerk == g)
                            .filter_map(|i| {
                                self.teilzeile(
                                    i,
                                    |a| a.geschoss == st && (f.ansatz)(a),
                                    false,
                                    nur_material,
                                )
                            })
                            .collect();
                        sub.ohne = (0..self.ohne.len())
                            .filter(|&j| {
                                let o = &self.ohne[j];
                                o.gewerk == g && o.geschoss == st && (f.ohne)(o)
                            })
                            .collect();
                        sub.summieren()
                    })
                    .collect();
                gr.summieren()
            })
            .collect()
    }

    fn nach_kg_gruppen(&self, nur_material: bool, f: &Filter) -> Vec<Gruppe> {
        let mut kgs: Vec<Option<u16>> = Vec::new();
        for kg in self
            .positionen
            .iter()
            .flat_map(|p| p.ansatz.iter().map(|a| a.kg))
            .chain(self.ohne.iter().map(|o| o.kg))
        {
            einmal(&mut kgs, kg);
        }
        kgs.sort_by_key(|k| k.unwrap_or(u16::MAX));
        let mut ebenen2: Vec<Option<u16>> = Vec::new();
        for k in &kgs {
            einmal(&mut ebenen2, k.map(crate::din276::ebene2));
        }
        ebenen2
            .into_iter()
            .map(|e2| {
                let mut gr = Gruppe::neu(Schluessel::Kg2(e2));
                gr.gruppen = kgs
                    .iter()
                    .filter(|k| k.map(crate::din276::ebene2) == e2)
                    .map(|&kg| {
                        let mut sub = Gruppe::neu(Schluessel::Kg(kg));
                        sub.zeilen = (0..self.positionen.len())
                            .filter_map(|i| {
                                self.teilzeile(
                                    i,
                                    |a| a.kg == kg && (f.ansatz)(a),
                                    false,
                                    nur_material,
                                )
                            })
                            .collect();
                        sub.ohne = (0..self.ohne.len())
                            .filter(|&j| self.ohne[j].kg == kg && (f.ohne)(&self.ohne[j]))
                            .collect();
                        sub.summieren()
                    })
                    .collect();
                gr.summieren()
            })
            .collect()
    }

    /// Summe der Geschossteile aller Positionen in den Geschossen
    /// `geschosse` im Modus (Chip-Summen im Reiter Kosten, auch abgewählt).
    pub fn summe_geschosse(&self, geschosse: &[StoreyId], nur_material: bool) -> Cent {
        (0..self.positionen.len())
            .flat_map(|i| {
                self.positionen[i]
                    .teile(|a| a.geschoss)
                    .into_iter()
                    .filter(|(s, _)| geschosse.contains(s))
                    .filter_map(move |(_, menge)| self.betrag(i, menge, nur_material))
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lesen;
    use sk_model::{qto, szo, GuidGen};

    fn haus(text: &str) -> (Model, Kostenblatt) {
        let m = szo::read_with(text, GuidGen::with_seed(1), &lesen::ABSCHNITTE_SZO)
            .expect("lädt")
            .model;
        let k = lesen::katalog(&m, None);
        let b = lesen::kosten(&m, &qto::schedule(&m), &k, &crate::Umfang::projekt());
        (m, b)
    }

    /// Kosten zu einer Auswahl (Jörn 10.10.): Ohne Einschränkung dieselben
    /// Gruppen und Beträge wie die ganze Aufteilung; an einem Bauteil nur
    /// dessen Ansatzzeilen mit ihrer Teilmenge, jede Summe die Summe der
    /// Zeilen, keine leere Gruppe, kein Ausgleich. Die Gründung eines
    /// Fundaments sind Platte, Schürze und Dämmung desselben Zuges.
    #[test]
    fn aufteilung_fuer_eine_auswahl() {
        let (m, b) = haus(include_str!("../referenz/rh1-standardhaus.szo"));
        let alle = |_: &Ansatz| true;
        let ohne = |_: &OhneZeile| true;
        for t in [Teilung::Gewerk, Teilung::Geschoss, Teilung::Kostengruppe] {
            let a = b.aufteilung(&m, t, false);
            let f = b.aufteilung_fuer(&m, t, false, &alle, &ohne);
            let summen = |x: &Aufteilung| x.gruppen.iter().map(|g| g.summe).collect::<Vec<_>>();
            assert_eq!(summen(&a), summen(&f), "{t:?}");
            assert_eq!(f.ausgleich, Cent(0));
        }
        let wand = m
            .elements()
            .iter()
            .find(|(_, e)| e.category == sk_model::element::Category::ExteriorWall)
            .map(|(id, _)| id)
            .unwrap();
        for t in [Teilung::Gewerk, Teilung::Geschoss, Teilung::Kostengruppe] {
            let nur = |a: &Ansatz| a.element == wand;
            let keine = |o: &OhneZeile| o.element == wand;
            let f = b.aufteilung_fuer(&m, t, false, &nur, &keine);
            assert!(!f.gruppen.is_empty());
            fn pruefe(b: &Kostenblatt, g: &Gruppe, wand: sk_model::ElementId) {
                assert!(!g.gruppen.is_empty() || !g.zeilen.is_empty() || !g.ohne.is_empty());
                for u in &g.gruppen {
                    pruefe(b, u, wand);
                }
                for z in &g.zeilen {
                    let p = &b.positionen[z.pos];
                    assert!(z.ansatz.iter().all(|&j| p.ansatz[j].element == wand));
                    assert!(z.menge <= p.menge);
                }
            }
            for g in &f.gruppen {
                pruefe(&b, g, wand);
            }
            assert_eq!(
                f.gesamt,
                f.gruppen.iter().filter_map(|g| g.summe).sum::<Cent>()
            );
            assert!(f.gesamt < b.netto && f.gesamt > Cent(0));
        }
        let finde = |c: sk_model::element::Category| {
            m.elements()
                .iter()
                .find(|(_, e)| e.category == c)
                .map(|(id, _)| id)
                .unwrap()
        };
        let fs = finde(sk_model::element::Category::StripFooting);
        let platte = finde(sk_model::element::Category::GroundSlab);
        let g = qto::gruendung_von(&m, fs);
        assert!(g.contains(&fs) && g.contains(&platte), "{g:?}");
        assert_eq!(qto::gruendung_von(&m, platte), g);
        assert!(qto::gruendung_von(&m, wand).is_empty());
    }

    /// Jede Summe ist die Summe der Zeilen darunter; Gruppen und Ausgleich
    /// treffen das Gesamt in jeder Gliederung und beiden Modi und decken
    /// sich mit `nach_gewerk`, `nach_geschoss` und `nach_kg` samt Ausgleich
    /// (Regel 96). RH-1 bis RH-3.
    #[test]
    fn summen_sind_summen_der_zeilen() {
        for text in [
            include_str!("../referenz/rh1-standardhaus.szo"),
            include_str!("../referenz/rh2-mehrschalig.szo"),
            include_str!("../referenz/rh3-versatz-dachterrasse.szo"),
        ] {
            let (m, b) = haus(text);
            for nur in [false, true] {
                for t in [Teilung::Gewerk, Teilung::Geschoss, Teilung::Kostengruppe] {
                    let a = b.aufteilung(&m, t, nur);
                    fn pruefe(g: &Gruppe) {
                        for u in &g.gruppen {
                            pruefe(u);
                        }
                        let s: Option<Cent> = g
                            .zeilen
                            .iter()
                            .map(|z| z.betrag)
                            .chain(g.gruppen.iter().map(|u| u.summe))
                            .flatten()
                            .fold(None, |acc, c| Some(acc.unwrap_or(Cent(0)) + c));
                        assert_eq!(g.summe, s, "{:?}", g.schluessel);
                    }
                    a.gruppen.iter().for_each(pruefe);
                    let oben: Cent = a.gruppen.iter().filter_map(|g| g.summe).sum();
                    assert_eq!(oben + a.ausgleich, b.gesamt(nur), "{t:?} {nur}");
                    if nur {
                        continue;
                    }
                    match t {
                        Teilung::Gewerk => {
                            assert_eq!(a.ausgleich, Cent(0));
                            for (g, c) in &b.nach_gewerk {
                                let x = a
                                    .gruppen
                                    .iter()
                                    .find(|x| x.schluessel == Schluessel::Gewerk(*g))
                                    .unwrap();
                                assert_eq!(x.summe, Some(*c));
                            }
                        }
                        Teilung::Geschoss => {
                            assert_eq!(a.ausgleich, b.ausgleich_geschoss);
                            for (st, c) in &b.nach_geschoss {
                                let x = a
                                    .gruppen
                                    .iter()
                                    .find(|x| x.schluessel == Schluessel::Geschoss(*st))
                                    .unwrap();
                                assert_eq!(x.summe, Some(*c));
                                assert_eq!(b.summe_geschosse(&[*st], false), *c);
                            }
                        }
                        Teilung::Kostengruppe => {
                            assert_eq!(a.ausgleich, b.ausgleich_kg);
                            for (kg, c) in &b.nach_kg {
                                let x = a
                                    .gruppen
                                    .iter()
                                    .flat_map(|g| &g.gruppen)
                                    .find(|x| x.schluessel == Schluessel::Kg(*kg))
                                    .unwrap();
                                assert_eq!(x.summe, Some(*c));
                            }
                        }
                    }
                }
            }
        }
    }
}

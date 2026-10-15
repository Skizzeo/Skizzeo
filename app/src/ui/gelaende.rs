//! Leiste „Gelände“ im Paneel „Werkzeuge“ (plan-gelaendemodul.md §4.7,
//! Stufe G1): Ein Klick auf „Gelände“ zeigt sie statt der Wandvorgaben, wie
//! „Erweiterungen“ seine Zeilen zeigt. Sie hat nur Handlungen: Höhen und
//! Lageplan einlesen, das Gelände unter dem Haus verschieben, drehen und an
//! der Grenze ausrichten, die Höhenlage und der Zuschnitt. Was sie zeigt,
//! füllt die App aus dem Modell ([`GelaendeLeiste`]); hier werden daraus
//! Zeilen. Ohne Freischaltung zeigt der Knopf, was das Modul kann.

use super::*;
use sk_model::Show;

/// Knöpfe der Leiste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GelaendeKnopf {
    /// „Gelände“ im Paneel „Werkzeuge“: öffnet und schließt die Leiste.
    Leiste,
    /// Lageplan einlesen (G1c).
    Lageplan,
    /// Das Flurstück des eingelesenen Lageplans übernehmen.
    Uebernehmen,
    /// Höhenaufmaß einlesen.
    Hoehen,
    /// Gelände unter dem Haus verschieben bzw. drehen: zeigt die Felder
    /// dazu (Tab-Eingabe aus §4.8 als Feld).
    Verschieben,
    Drehen,
    /// „An Grenze ausrichten“ (§4.8).
    Ausrichten,
    /// Höhenlage über Gelände bzw. absolut (§4.3).
    Ueber,
    Absolut,
    /// Zuschnitt: alles, Umgebung blass, nur Grundstück (§4.5).
    Zuschnitt(Show),
    /// Werkzeug „Fläche zeichnen“ (G3d).
    Flaeche,
    /// Im Paneel der gewählten Geländefläche: befestigt ja/nein, löschen.
    Befestigt,
    Loeschen,
    /// Alle Hinweise des letzten Einlesens zeigen bzw. wieder nur die
    /// ersten (Bedienbarkeit 43.5).
    Hinweise,
}

impl GelaendeKnopf {
    /// „Gelände“ ist immer da; alles in der Leiste gehört zum Modul.
    pub fn modul(self) -> Option<Modul> {
        match self {
            GelaendeKnopf::Leiste => None,
            GelaendeKnopf::Lageplan
            | GelaendeKnopf::Uebernehmen
            | GelaendeKnopf::Hoehen
            | GelaendeKnopf::Verschieben
            | GelaendeKnopf::Drehen
            | GelaendeKnopf::Ausrichten
            | GelaendeKnopf::Ueber
            | GelaendeKnopf::Absolut
            | GelaendeKnopf::Zuschnitt(_)
            | GelaendeKnopf::Flaeche
            | GelaendeKnopf::Befestigt
            | GelaendeKnopf::Loeschen
            | GelaendeKnopf::Hinweise => Some(Modul::Gelaende),
        }
    }
}

/// Zahlenfelder der Leiste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GelaendeFeld {
    /// Höhenlage „über Gelände“: OK Sohle über dem höchsten Geländepunkt
    /// am Plattenrand (mm; das Feld zeigt ihn wie jede Länge).
    Versatz,
    /// Höhenlage „absolut“: ±0,00 in m ü. NHN.
    Nhn,
    /// Verschieben: das Gelände um so viel nach rechts und nach oben (m im
    /// Grundriss); danach steht wieder 0 da.
    Rechts,
    Oben,
    /// Drehen: um so viel Grad gegen den Uhrzeigersinn um die Hausmitte.
    Winkel,
    /// Paneel der gewählten Geländefläche (G3d): Höhe über ±0,00 (mm) und
    /// Böschung 1:n (0 senkrecht).
    Hoehe,
    Boeschung,
    /// Erdarbeiten am Grundstück: Oberboden andecken (mm) und Faktor fest
    /// zu eingebaut.
    Andecken,
    Faktor,
}

impl GelaendeFeld {
    /// Felder im Paneel „Eigenschaften“ der gewählten Fläche, nicht in der
    /// Leiste.
    pub fn im_paneel(self) -> bool {
        matches!(
            self,
            Self::Hoehe | Self::Boeschung | Self::Andecken | Self::Faktor
        )
    }
}

/// Was die Leiste zeigt; die App füllt es aus dem Modell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GelaendeLeiste {
    /// Ein Gelände ist da (Höhenaufmaß oder Grundstück aus dem Lageplan).
    pub aufmass: bool,
    /// Es hat Höhen: Höhenlage und ihre Folgen.
    pub hoehen: bool,
    /// Ein Lageplan ist gelesen und sein Flurstück kann übernommen werden.
    pub uebernehmen: bool,
    /// Es gibt eine Grenze: „An Grenze ausrichten“ und Zuschnitt.
    pub grenze: bool,
    /// Höhenlage absolut (sonst über Gelände).
    pub absolut: bool,
    pub zuschnitt: Option<Show>,
    /// Verschieben oder Drehen ist offen.
    pub tippen: Option<GelaendeKnopf>,
    /// Das Werkzeug „Fläche zeichnen“ ist an; `ecken` sind gesetzt.
    pub zeichnen: bool,
    pub ecken: usize,
    /// Felder in der Reihenfolge der Leiste.
    pub felder: Vec<FieldRow>,
    /// Höhe von ±0,00 und die Folgen der Höhenlage, je eine Zeile.
    pub hoehenlage: Vec<String>,
    /// Das Haus steckt im Gelände: rot unter den Folgen.
    pub steckt: Option<Meldung>,
    /// „Grundstück 822,87 m²“.
    pub grundstueck: Option<String>,
    /// Höhenlinien gröber als gewünscht oder keine, weil es zu viele wären.
    pub hoehenlinien: Option<String>,
    /// Ergebnis des letzten Einlesens (Satz, Hinweise) bzw. sein Fehler;
    /// umgebrochen auf die Breite des Paneels.
    pub meldung: Vec<String>,
    /// Weitere Hinweise, die nicht in `meldung` stehen; mit `alle` stehen
    /// sie dort und der Knopf nimmt sie wieder weg.
    pub weitere: usize,
    pub alle: bool,
    pub fehler: Option<Meldung>,
}

/// Knopfpaar aus zwei Knöpfen der Leiste.
fn paar(a: (GelaendeKnopf, &'static str), b: (GelaendeKnopf, &'static str)) -> Row {
    Row::Pair([(Id::Gelaende(a.0), a.1), (Id::Gelaende(b.0), b.1)])
}

impl Ui {
    /// Zeilen der Leiste unter den Wandknöpfen.
    pub(super) fn gelaende_rows(&self, l: &GelaendeLeiste) -> Vec<Row> {
        use GelaendeKnopf as K;
        let mut rows = vec![Row::Button(Id::Gelaende(K::Leiste), "Gelände")];
        if !self.frei.frei(Modul::Gelaende) {
            // Was das Modul kann (§4.7, auf Jörns Wunsch statt ausgeblendet)
            rows.extend([
                Row::Hint("Echtes Gelände aus Aufmaß"),
                Row::Hint("und Lageplan, Grundstück"),
                Row::Hint("und Lage des Hauses."),
                Row::Text("Geländemodul freischalten".into()),
            ]);
            return rows;
        }
        let feld = |rows: &mut Vec<Row>, f: GelaendeFeld| {
            if let Some(r) = l.felder.iter().find(|r| r.field == Field::Gelaende(f)) {
                rows.push(Row::Field(r.field, r.label));
                match self.edit.as_ref().filter(|e| e.field == r.field) {
                    Some(e) => rows.extend(e.error.clone().map(Row::Error)),
                    None => rows.extend(r.ausserhalb().map(Row::Error)),
                }
            }
        };
        rows.push(paar((K::Lageplan, "Lageplan"), (K::Hoehen, "Höhen")));
        if l.uebernehmen {
            rows.push(Row::Button(
                Id::Gelaende(K::Uebernehmen),
                "Flurstück übernehmen",
            ));
        }
        if l.aufmass {
            rows.push(paar((K::Verschieben, "Verschieben"), (K::Drehen, "Drehen")));
            match l.tippen {
                Some(K::Verschieben) => {
                    feld(&mut rows, GelaendeFeld::Rechts);
                    feld(&mut rows, GelaendeFeld::Oben);
                }
                Some(K::Drehen) => feld(&mut rows, GelaendeFeld::Winkel),
                _ => {}
            }
            if l.grenze {
                rows.push(Row::Button(
                    Id::Gelaende(K::Ausrichten),
                    "An Grenze ausrichten",
                ));
            }
            if l.hoehen {
                rows.extend([
                    Row::Separator,
                    Row::Label("Höhenlage"),
                    paar((K::Ueber, "über Gelände"), (K::Absolut, "absolut")),
                ]);
                feld(&mut rows, GelaendeFeld::Versatz);
                feld(&mut rows, GelaendeFeld::Nhn);
                rows.extend(l.hoehenlage.iter().map(|z| Row::Text(z.clone())));
                rows.extend(l.steckt.clone().map(Row::Error));
                rows.extend([
                    Row::Separator,
                    Row::Label("Geländeflächen"),
                    Row::Button(Id::Gelaende(K::Flaeche), "Fläche zeichnen"),
                ]);
                if l.zeichnen {
                    rows.extend(match l.ecken {
                        0 => [
                            Row::Hint("Ecken im Grundriss klicken,"),
                            Row::Hint("die erste Ecke schließt."),
                        ],
                        _ => [
                            Row::Hint("Enter schließt, Rücktaste"),
                            Row::Hint("nimmt die letzte Ecke."),
                        ],
                    });
                } else {
                    rows.push(Row::Hint("Klick wählt eine Fläche."));
                }
            }
            if l.grenze {
                rows.extend([
                    Row::Separator,
                    Row::Label("Zuschnitt"),
                    Row::Segments([
                        (Id::Gelaende(K::Zuschnitt(Show::All)), "alles"),
                        (Id::Gelaende(K::Zuschnitt(Show::Fade)), "blass"),
                        (Id::Gelaende(K::Zuschnitt(Show::Parcel)), "Grenze"),
                    ]),
                ]);
            }
            rows.extend(l.grundstueck.clone().map(Row::Text));
            rows.extend(l.hoehenlinien.clone().map(Row::Text));
        } else if l.meldung.is_empty() && l.fehler.is_none() {
            rows.extend([
                Row::Hint("Lageplan: PDF, Maßstab"),
                Row::Hint("und Flurstück erkannt."),
                Row::Hint("Höhen: Punktliste oder"),
                Row::Hint("DXF des Vermessers."),
            ]);
        }
        if !l.meldung.is_empty() || l.fehler.is_some() {
            rows.push(Row::Separator);
        }
        let breite = self.width(Panel::Tools) - 2.0 * self.size.panel_pad;
        let f = self.fonts.regular.as_ref();
        for m in &l.meldung {
            let z = widgets::wrap(f, m, self.size.font_small, breite);
            rows.extend(z.into_iter().map(Row::Text));
        }
        if l.weitere > 0 {
            let k = Id::Gelaende(GelaendeKnopf::Hinweise);
            if l.alle {
                rows.push(Row::Button(k, "Weniger Hinweise"));
            } else {
                rows.push(Row::Text(format!("und {} weitere Hinweise", l.weitere)));
                rows.push(Row::Button(k, "Alle Hinweise"));
            }
        }
        if let Some(e) = &l.fehler {
            let z = widgets::wrap(f, e.as_str(), self.size.font_detail, breite);
            rows.extend(z.iter().map(|t| Row::Error(Meldung::mit("{}", &[t]))));
        }
        rows
    }

    /// Knopf der Leiste hervorgehoben: die offene Leiste, die offene
    /// Eingabe, die Art der Höhenlage und der Zuschnitt.
    pub(super) fn gelaende_an(&self, k: GelaendeKnopf) -> bool {
        let Some(l) = self.gelaende.as_ref() else {
            return false;
        };
        match k {
            GelaendeKnopf::Leiste => true,
            GelaendeKnopf::Verschieben | GelaendeKnopf::Drehen => l.tippen == Some(k),
            GelaendeKnopf::Ueber => !l.absolut,
            GelaendeKnopf::Absolut => l.absolut,
            GelaendeKnopf::Zuschnitt(s) => l.zuschnitt == Some(s),
            GelaendeKnopf::Flaeche => l.zeichnen,
            GelaendeKnopf::Lageplan
            | GelaendeKnopf::Uebernehmen
            | GelaendeKnopf::Hoehen
            | GelaendeKnopf::Ausrichten
            | GelaendeKnopf::Befestigt
            | GelaendeKnopf::Loeschen
            | GelaendeKnopf::Hinweise => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leiste() -> GelaendeLeiste {
        GelaendeLeiste {
            aufmass: true,
            hoehen: true,
            grenze: true,
            zuschnitt: Some(Show::All),
            ..Default::default()
        }
    }

    /// Ohne Freischaltung zeigt „Gelände“ drei Zeilen und den Hinweis,
    /// keine Knöpfe der Leiste; mit ihr Höhenlage und Zuschnitt.
    #[test]
    fn leiste_mit_und_ohne_modul() {
        let t = Theme::dark();
        let mut ui = Ui::new(1.0, &t);
        ui.gelaende = Some(leiste());
        let knoepfe = |ui: &Ui| -> Vec<Id> {
            ui.rows(Panel::Tools)
                .iter()
                .flat_map(|r| match r {
                    Row::Button(id, _) => vec![*id],
                    Row::Pair(p) => p.iter().map(|x| x.0).collect(),
                    Row::Segments(s) => s.iter().map(|x| x.0).collect(),
                    _ => Vec::new(),
                })
                .filter(|id| matches!(id, Id::Gelaende(_)))
                .collect()
        };
        use GelaendeKnopf as K;
        assert_eq!(
            knoepfe(&ui),
            [
                K::Leiste,
                K::Lageplan,
                K::Hoehen,
                K::Verschieben,
                K::Drehen,
                K::Ausrichten,
                K::Ueber,
                K::Absolut,
                K::Flaeche,
                K::Zuschnitt(Show::All),
                K::Zuschnitt(Show::Fade),
                K::Zuschnitt(Show::Parcel),
            ]
            .map(Id::Gelaende)
        );
        assert!(ui.is_on(Id::Gelaende(K::Ueber)) && !ui.is_on(Id::Gelaende(K::Absolut)));
        assert!(ui.is_on(Id::Gelaende(K::Zuschnitt(Show::All))));
        assert!(!ui.is_disabled(Id::Gelaende(K::Lageplan)));
        // Fläche zeichnen: hervorgehoben, die Hinweise folgen den Ecken
        let hinweise = |ui: &Ui| -> Vec<String> {
            ui.rows(Panel::Tools)
                .iter()
                .filter_map(|r| match r {
                    Row::Hint(t) => Some(t.to_string()),
                    _ => None,
                })
                .collect()
        };
        assert!(!ui.is_on(Id::Gelaende(K::Flaeche)));
        assert!(hinweise(&ui).contains(&"Klick wählt eine Fläche.".into()));
        ui.gelaende = Some(GelaendeLeiste {
            zeichnen: true,
            ..leiste()
        });
        assert!(ui.is_on(Id::Gelaende(K::Flaeche)));
        assert!(hinweise(&ui)[1].starts_with("die erste Ecke"));
        ui.gelaende = Some(GelaendeLeiste {
            zeichnen: true,
            ecken: 2,
            ..leiste()
        });
        assert!(hinweise(&ui)[0].starts_with("Enter schließt"));
        ui.gelaende = Some(leiste());
        // Ein gelesener Lageplan bietet sein Flurstück an; ohne Höhen keine
        // Höhenlage
        ui.gelaende = Some(GelaendeLeiste {
            uebernehmen: true,
            hoehen: false,
            ..leiste()
        });
        let k = knoepfe(&ui);
        assert_eq!(
            k[..4],
            [K::Leiste, K::Lageplan, K::Hoehen, K::Uebernehmen].map(Id::Gelaende)
        );
        assert!(!k.contains(&Id::Gelaende(K::Ueber)));
        ui.gelaende = Some(leiste());
        ui.frei = Freischaltung::mit(&[Modul::Mengen]);
        assert_eq!(knoepfe(&ui), [Id::Gelaende(K::Leiste)]);
        assert!(ui
            .rows(Panel::Tools)
            .iter()
            .any(|r| matches!(r, Row::Text(t) if t == "Geländemodul freischalten")));
        // Geschlossen: nur der Knopf unter den Wandknöpfen
        ui.gelaende = None;
        assert_eq!(knoepfe(&ui), [Id::Gelaende(K::Leiste)]);
        assert!(!ui.is_on(Id::Gelaende(K::Leiste)));
    }

    /// Ohne Aufmaß: Einlesen und ein kurzer Hinweis; eine Meldung wird auf
    /// die Breite des Paneels umgebrochen.
    #[test]
    fn ohne_aufmass_und_meldung() {
        let t = Theme::dark();
        let mut ui = Ui::new(1.0, &t);
        ui.gelaende = Some(GelaendeLeiste::default());
        let rows = ui.rows(Panel::Tools);
        assert!(rows
            .iter()
            .any(|r| matches!(r, Row::Hint(t) if t.starts_with("Höhen:"))));
        assert!(!rows.iter().any(|r| matches!(r, Row::Label("Höhenlage"))));
        ui.gelaende = Some(GelaendeLeiste {
            fehler: Some(Meldung::satz(
                "Aus den Punkten entsteht kein Gelände: Es braucht drei Punkte.",
            )),
            ..Default::default()
        });
        let fehler = ui
            .rows(Panel::Tools)
            .into_iter()
            .filter(|r| matches!(r, Row::Error(_)))
            .count();
        assert!(fehler >= 1);
    }
}

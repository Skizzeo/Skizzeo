//! Leiste „Gelände“ im Paneel „Werkzeuge“ (plan-gelaendemodul.md §4.7,
//! Stufe G1): Ein Klick auf „Gelände“ zeigt sie statt der Wandvorgaben, wie
//! „Erweiterungen“ seine Zeilen zeigt. Sie hat nur Handlungen: Höhen und
//! Lageplan einlesen, das Gelände unter dem Haus verschieben, drehen und an
//! der Grenze ausrichten, die Höhenlage und der Zuschnitt. Was sie zeigt,
//! füllt die App aus dem Modell ([`GelaendeLeiste`]); hier werden daraus
//! Zeilen. Ohne Freischaltung zeigt der Knopf, was das Modul kann.

use super::*;
use sk_model::Show;

/// Beschriftungen der Zahlenfelder in der Leiste; links neben dem Feld ist
/// nur Platz für etwa 14 Zeichen (Bedienbarkeit, später zu Durchgang 42:
/// „Versatz“ sagte nicht, wovon; „nach oben“ las sich als Anheben, deshalb
/// steht [`IM_GRUNDRISS`] darüber).
pub const RECHTS: &str = "nach rechts";
pub const OBEN: &str = "nach oben";
pub const UEBER: &str = "über Gelände";
pub const IM_GRUNDRISS: &str = "Im Grundriss verschieben:";
pub const NHN: &str = "±0,00 ü. NHN";

/// Ohne Modul: wo man es freischaltet (Bedienbarkeit, später zu
/// Durchgang 42: „Geländemodul freischalten“ stand ohne Weg da).
const FREISCHALTEN: &str = "Dateimenü › Ausbaustufe …";

/// Rote Zeile, wenn das Aufmaß eine Baugrube nicht deckt (Bedienbarkeit
/// 42.5).
pub const DECKT_NICHT: &str = "Aufmaß deckt die Baugrube nicht, Erdmengen mit mittlerer Höhe. \
    Gelände zurückschieben oder ein größeres Aufmaß einlesen.";

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
    /// Umgebung zeigen: ganz, blass, nicht (§4.5).
    Zuschnitt(Show),
    /// „Gelände ans Haus“ (G3): den Streifen um den Sockel des Hauses der
    /// Höhenlage anlegen oder, wenn es ihn schon gibt, wählen.
    AmHaus,
    /// Werkzeug „Fläche zeichnen“ (G3d).
    Flaeche,
    /// Dasselbe Werkzeug als „Grenze zeichnen“: ein Grundstück ohne
    /// Lageplan (I2).
    Grenze,
    /// Im Paneel der gewählten Geländefläche: befestigt ja/nein, löschen.
    Befestigt,
    Loeschen,
    /// Alle Hinweise des letzten Einlesens zeigen bzw. wieder nur die
    /// ersten (Bedienbarkeit 43.5).
    Hinweise,
    /// Aufmaß, Lageplan und Grundstück entfernen, zurück zur Platte
    /// (Bedienbarkeit 42.6).
    Entfernen,
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
            | GelaendeKnopf::AmHaus
            | GelaendeKnopf::Flaeche
            | GelaendeKnopf::Grenze
            | GelaendeKnopf::Befestigt
            | GelaendeKnopf::Loeschen
            | GelaendeKnopf::Hinweise
            | GelaendeKnopf::Entfernen => Some(Modul::Gelaende),
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
    /// Böschung 1:n (0 senkrecht); am Streifen ums Haus dazu Breite (mm)
    /// und Gefälle vom Haus weg (%).
    Hoehe,
    Boeschung,
    Breite,
    Gefaelle,
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
            Self::Hoehe
                | Self::Boeschung
                | Self::Breite
                | Self::Gefaelle
                | Self::Andecken
                | Self::Faktor
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
    /// Alle Punkte sind von Hand gesetzt (I1): ohne Höhenlage in NHN.
    pub angenommen: bool,
    /// Ein Lageplan ist gelesen und sein Flurstück kann übernommen werden.
    pub uebernehmen: bool,
    /// Es gibt eine Grenze: „An Grenze ausrichten“ und Zuschnitt.
    pub grenze: bool,
    /// Höhenlage absolut (sonst über Gelände).
    pub absolut: bool,
    pub zuschnitt: Option<Show>,
    /// Verschieben oder Drehen ist offen.
    pub tippen: Option<GelaendeKnopf>,
    /// Das Werkzeug „Fläche zeichnen“ bzw. „Grenze zeichnen“ ist an;
    /// `ecken` sind gesetzt.
    pub zeichnen: bool,
    pub grenze_zeichnen: bool,
    pub ecken: usize,
    /// Felder in der Reihenfolge der Leiste.
    pub felder: Vec<FieldRow>,
    /// Höhe von ±0,00 und die Folgen der Höhenlage, je eine Zeile.
    pub hoehenlage: Vec<String>,
    /// Das Haus steckt im Gelände: rot unter den Folgen.
    pub steckt: Option<Meldung>,
    /// Das Aufmaß deckt eine Baugrube nicht: rot darunter (42.5).
    pub deckt_nicht: bool,
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
        let breite = self.width(Panel::Tools) - 2.0 * self.size.panel_pad;
        if !self.frei.frei(Modul::Gelaende) {
            // Was das Modul kann (§4.7, auf Jörns Wunsch statt ausgeblendet)
            rows.extend([
                Row::Hint("Echtes Gelände aus Aufmaß"),
                Row::Hint("und Lageplan, Grundstück"),
                Row::Hint("und Lage des Hauses."),
                Row::Text("Geländemodul freischalten".into()),
                // der Weg dorthin (Bedienbarkeit, später zu Durchgang 42)
                Row::Hint(FREISCHALTEN),
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
        // die Ecken beim Zeichnen; „die erste Ecke schließt“ wie in der
        // Statuszeile
        let ecken = |rows: &mut Vec<Row>| {
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
        };
        rows.push(paar((K::Lageplan, "Lageplan"), (K::Hoehen, "Höhen")));
        if l.uebernehmen {
            rows.push(Row::Button(
                Id::Gelaende(K::Uebernehmen),
                "Flurstück übernehmen",
            ));
        }
        // ohne Lageplan von Hand (I2); steht schon eine Grenze, wird sie
        // geändert, nicht eine zweite gezeichnet
        if !l.grenze || l.grenze_zeichnen {
            rows.push(Row::Button(Id::Gelaende(K::Grenze), "Grenze zeichnen"));
        }
        if l.grenze_zeichnen {
            ecken(&mut rows);
        }
        if l.aufmass {
            // „Verschieben“ passte nicht in den Knopf (Ist-Bild 42.7)
            rows.push(paar((K::Verschieben, "Schieben"), (K::Drehen, "Drehen")));
            match l.tippen {
                Some(K::Verschieben) => {
                    rows.push(Row::Hint(IM_GRUNDRISS));
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
            // ein angenommenes Gelände (I1) hat keine Höhenlage in NHN
            if l.hoehen && !l.angenommen {
                rows.extend([
                    Row::Separator,
                    Row::Label("Höhenlage"),
                    // „über Gelände“ passte nicht in den Knopf (Ist-Bild
                    // 42.7); der Tooltip sagt es
                    paar((K::Ueber, "relativ"), (K::Absolut, "absolut")),
                ]);
                feld(&mut rows, GelaendeFeld::Versatz);
                feld(&mut rows, GelaendeFeld::Nhn);
                // umgebrochen: „Einbindung …, frostfrei“ lief über den Rand
                // (Ist-Bild 42.7)
                for z in &l.hoehenlage {
                    let f = self.fonts.regular.as_ref();
                    let z = widgets::wrap(f, z, self.size.font_small, breite);
                    rows.extend(z.into_iter().map(Row::Text));
                }
                rows.extend(l.steckt.clone().map(Row::Error));
                if l.deckt_nicht {
                    rows.extend(self.rot(DECKT_NICHT));
                }
            }
        } else if l.meldung.is_empty() && l.fehler.is_none() {
            rows.extend([
                Row::Hint("Lageplan: PDF, Maßstab"),
                Row::Hint("und Flurstück erkannt."),
                Row::Hint("Höhen: Punktliste oder"),
                Row::Hint("DXF des Vermessers."),
            ]);
        }
        // Geländeflächen gehen auch ohne Aufmaß: Die erste legt das ebene
        // Gelände mit Punkten an (I1)
        rows.extend([
            Row::Separator,
            Row::Label("Geländeflächen"),
            Row::Button(Id::Gelaende(K::AmHaus), "Gelände ans Haus"),
            Row::Button(Id::Gelaende(K::Flaeche), "Fläche zeichnen"),
        ]);
        if l.zeichnen {
            ecken(&mut rows);
        } else {
            rows.push(Row::Hint("Klick wählt eine Fläche."));
        }
        if l.aufmass {
            if l.grenze {
                rows.extend([
                    Row::Separator,
                    // „Zuschnitt: alles | blass | Grenze“ las sich schwer
                    // (Bedienbarkeit, später zu Durchgang 42)
                    Row::Label("Umgebung zeigen"),
                    Row::Segments([
                        (Id::Gelaende(K::Zuschnitt(Show::All)), "ganz"),
                        (Id::Gelaende(K::Zuschnitt(Show::Fade)), "blass"),
                        (Id::Gelaende(K::Zuschnitt(Show::Parcel)), "nicht"),
                    ]),
                ]);
            }
            rows.extend(l.grundstueck.clone().map(Row::Text));
            rows.extend(l.hoehenlinien.clone().map(Row::Text));
            rows.push(Row::Button(Id::Gelaende(K::Entfernen), "Gelände entfernen"));
        }
        if !l.meldung.is_empty() || l.fehler.is_some() {
            rows.push(Row::Separator);
        }
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
            rows.extend(self.rot(e.as_str()));
        }
        rows
    }

    /// Rote Zeilen, umgebrochen auf die Breite des Paneels.
    fn rot(&self, satz: &str) -> Vec<Row> {
        let breite = self.width(Panel::Tools) - 2.0 * self.size.panel_pad;
        let f = self.fonts.regular.as_ref();
        let z = widgets::wrap(f, satz, self.size.font_detail, breite);
        z.iter()
            .map(|t| Row::Error(Meldung::mit("{}", &[t])))
            .collect()
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
            GelaendeKnopf::Grenze => l.grenze_zeichnen,
            GelaendeKnopf::Lageplan
            | GelaendeKnopf::Uebernehmen
            | GelaendeKnopf::Hoehen
            | GelaendeKnopf::Ausrichten
            | GelaendeKnopf::AmHaus
            | GelaendeKnopf::Befestigt
            | GelaendeKnopf::Loeschen
            | GelaendeKnopf::Hinweise
            | GelaendeKnopf::Entfernen => false,
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
                K::AmHaus,
                K::Flaeche,
                K::Zuschnitt(Show::All),
                K::Zuschnitt(Show::Fade),
                K::Zuschnitt(Show::Parcel),
                K::Entfernen,
            ]
            .map(Id::Gelaende)
        );
        // Deckt das Aufmaß die Baugrube nicht, steht es rot da (42.5)
        let rot = |ui: &Ui| -> String {
            ui.rows(Panel::Tools)
                .iter()
                .filter_map(|r| match r {
                    Row::Error(m) => Some(m.as_str().to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(rot(&ui), "");
        ui.gelaende = Some(GelaendeLeiste {
            deckt_nicht: true,
            ..leiste()
        });
        assert_eq!(rot(&ui), DECKT_NICHT);
        ui.gelaende = Some(leiste());
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
        // Grenze zeichnen (I2): hervorgehoben, die Hinweise unter dem Knopf
        ui.gelaende = Some(GelaendeLeiste {
            grenze_zeichnen: true,
            ..leiste()
        });
        assert!(ui.is_on(Id::Gelaende(K::Grenze)) && !ui.is_on(Id::Gelaende(K::Flaeche)));
        let rows = ui.rows(Panel::Tools);
        let i = rows
            .iter()
            .position(|r| matches!(r, Row::Button(id, _) if *id == Id::Gelaende(K::Grenze)))
            .unwrap();
        assert!(matches!(&rows[i + 1], Row::Hint(t) if t.starts_with("Ecken im Grundriss")));
        // mit Grenze kein zweites Flurstück von Hand
        ui.gelaende = Some(leiste());
        assert!(!knoepfe(&ui).contains(&Id::Gelaende(K::Grenze)));
        // Ein gelesener Lageplan bietet sein Flurstück an; ohne Höhen keine
        // Höhenlage
        ui.gelaende = Some(GelaendeLeiste {
            uebernehmen: true,
            hoehen: false,
            grenze: false,
            ..leiste()
        });
        let k = knoepfe(&ui);
        assert_eq!(
            k[..5],
            [K::Leiste, K::Lageplan, K::Hoehen, K::Uebernehmen, K::Grenze].map(Id::Gelaende)
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

    /// Mit Liberation Sans (gleich breit wie Arial): Jede Beschriftung
    /// passt in ihren Knopf, jede Zeile ins Paneel (Ist-Bild 42.7: „über
    /// Gelände“ und „Einbindung …, frostfrei“ liefen über den Rand).
    #[test]
    fn alles_passt_ins_paneel() {
        let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
        let lade = |n: &str| {
            std::fs::read(lib.join(n))
                .ok()
                .and_then(sk_paint::font::Font::parse)
        };
        let t = Theme::dark();
        let mut ui = Ui::new(1.0, &t);
        ui.fonts = widgets::Fonts {
            regular: lade("LiberationSans-Regular.ttf"),
            bold: lade("LiberationSans-Bold.ttf"),
            italic: None,
        };
        ui.gelaende = Some(GelaendeLeiste {
            uebernehmen: true,
            hoehenlage: vec![
                "±0,00 = 25,20 m ü. NHN".into(),
                "Sockel 0,0 bis 44,2 cm".into(),
                "Einbindung 0,80 bis 1,24 m, frostfrei".into(),
            ],
            grundstueck: Some("Grundstück 822,86 m²".into()),
            tippen: Some(GelaendeKnopf::Verschieben),
            ..leiste()
        });
        // ohne die Schriften nicht still bestanden (Review #202 H-202a)
        let (Some(regular), Some(bold)) = (&ui.fonts.regular, &ui.fonts.bold) else {
            eprintln!("alles_passt_ins_paneel übersprungen: keine Liberation-Schriften in {lib:?}");
            return;
        };
        for (_, r, label) in ui.buttons(Panel::Tools) {
            let w = bold.width(label, ui.size.font);
            assert!(w <= r.w - 2.0, "„{label}“ {w:.1} in {:.1}", r.w);
        }
        let breite = ui.width(Panel::Tools) - 2.0 * ui.size.panel_pad;
        for r in ui.rows(Panel::Tools) {
            let z = match r {
                Row::Text(z) => z,
                Row::Hint(z) => z.to_string(),
                _ => continue,
            };
            let w = regular.width(&z, ui.size.font_small);
            assert!(w <= breite, "„{z}“ {w:.1} in {breite:.1}");
        }
        assert!(regular.width(FREISCHALTEN, ui.size.font_small) <= breite);
        // Feldbeschriftungen links neben dem Feld, mit Luft davor
        let platz = breite - (EXT_FIELD_W + 4.0) * ui.scale;
        for z in [RECHTS, OBEN, UEBER, NHN, "Winkel"] {
            let w = regular.width(z, ui.size.font_small);
            assert!(w <= platz, "„{z}“ {w:.1} in {platz:.1}");
        }
    }

    /// Ohne Aufmaß: Einlesen, ein kurzer Hinweis und „Fläche zeichnen“
    /// (I1); eine Meldung wird auf die Breite des Paneels umgebrochen. Ein
    /// angenommenes Gelände hat keine Höhenlage in NHN.
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
        let flaeche = Id::Gelaende(GelaendeKnopf::Flaeche);
        assert!(rows
            .iter()
            .any(|r| matches!(r, Row::Button(id, _) if *id == flaeche)));
        assert!(!rows.iter().any(|r| matches!(r, Row::Button(id, _)
            if *id == Id::Gelaende(GelaendeKnopf::Entfernen))));
        ui.gelaende = Some(GelaendeLeiste {
            angenommen: true,
            ..leiste()
        });
        let rows = ui.rows(Panel::Tools);
        assert!(!rows.iter().any(|r| matches!(r, Row::Label("Höhenlage"))));
        assert!(rows
            .iter()
            .any(|r| matches!(r, Row::Label("Geländeflächen"))));
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

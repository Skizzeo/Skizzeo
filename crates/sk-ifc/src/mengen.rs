//! Mengen je Bauteil (ifc/plan.md §5, I4): die Zahlen der Mengenliste
//! ([`sk_model::qto::schedule`], dieselben wie im Mengenfenster) als
//! `IfcElementQuantity`. Wo IFC4 einen Satz `Qto_…BaseQuantities` für die
//! Klasse kennt, stehen sie dort unter den Namen der Norm; was die Norm
//! nicht kennt (Randdämmstreifen, Attikablech als Länge, Mengen der
//! Erweiterungen), steht in `Skizzeo_Mengen`.
//!
//! Einheiten wie in der Datei vereinbart: Länge mm, Fläche m², Rauminhalt
//! m³, Masse kg. Jedes Bauteil hat seinen Rauminhalt wie in der Liste
//! (`NetVolume` bzw. `Volumen`), so dass die Summen einer Gruppe im
//! Mengenfenster aus der Datei nachzurechnen sind.

use sk_model::qto::ElementQto;

/// Art einer Menge: welche `IfcQuantity…` sie wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// mm
    Laenge,
    /// m²
    Flaeche,
    /// m³
    Volumen,
    /// kg
    Masse,
    /// Stück
    Anzahl,
}

impl Art {
    /// Klasse der Menge in der Datei.
    pub fn klasse(self) -> &'static str {
        match self {
            Art::Laenge => "IFCQUANTITYLENGTH",
            Art::Flaeche => "IFCQUANTITYAREA",
            Art::Volumen => "IFCQUANTITYVOLUME",
            Art::Masse => "IFCQUANTITYWEIGHT",
            Art::Anzahl => "IFCQUANTITYCOUNT",
        }
    }
}

/// Eine Menge in der Einheit der Datei.
#[derive(Clone, Debug, PartialEq)]
pub struct Menge {
    pub name: String,
    pub art: Art,
    pub wert: f64,
}

/// Ein Mengensatz mit seinem Namen.
#[derive(Clone, Debug, PartialEq)]
pub struct Satz {
    pub name: &'static str,
    pub mengen: Vec<Menge>,
}

/// Name des eigenen Satzes für Mengen ohne Satz der Norm.
pub const EIGEN: &str = "Skizzeo_Mengen";

struct Bau(Vec<Menge>);

impl Bau {
    fn mm(mut self, n: &str, v: f64) -> Bau {
        self.add(n, Art::Laenge, v);
        self
    }
    /// Fläche aus mm².
    fn m2(mut self, n: &str, v: f64) -> Bau {
        self.add(n, Art::Flaeche, v / 1e6);
        self
    }
    /// Rauminhalt aus mm³.
    fn m3(mut self, n: &str, v: f64) -> Bau {
        self.add(n, Art::Volumen, v / 1e9);
        self
    }
    fn kg(mut self, n: &str, v: f64) -> Bau {
        self.add(n, Art::Masse, v);
        self
    }
    fn stk(mut self, n: &str, v: f64) -> Bau {
        self.add(n, Art::Anzahl, v);
        self
    }
    /// Nur endliche Werte über 0: eine Menge 0 sagt nichts, und eine
    /// fehlende Angabe ist ehrlicher als eine falsche.
    fn add(&mut self, n: &str, art: Art, v: f64) {
        if v.is_finite() && v > 0.0 {
            self.0.push(Menge {
                name: n.to_string(),
                art,
                wert: v,
            });
        }
    }
    fn satz(self, name: &'static str) -> Option<Satz> {
        (!self.0.is_empty()).then_some(Satz {
            name,
            mengen: self.0,
        })
    }
}

fn bau() -> Bau {
    Bau(Vec::new())
}

/// Mengensätze eines Bauteils der IFC-Klasse `klasse` aus seiner Zeile der
/// Mengenliste.
pub fn saetze(q: &ElementQto, klasse: &str) -> Vec<Satz> {
    let mut out = Vec::new();
    let mut push = |s: Option<Satz>| out.extend(s);
    match q {
        ElementQto::Wall(w) => {
            let masse: f64 = w.layers.iter().map(|l| l.mass).sum();
            push(
                bau()
                    .mm("Length", w.length)
                    .mm("Width", w.width)
                    .mm("Height", w.height)
                    .m2("GrossFootprintArea", w.footprint)
                    .m2("GrossSideArea", w.side_outer)
                    .m3("GrossVolume", w.volume_gross)
                    .m3("NetVolume", w.volume)
                    .kg("NetWeight", masse)
                    .satz("Qto_WallBaseQuantities"),
            );
            // Länge, wie sie in der Mengenliste steht (Außenwand an der
            // Außenseite, Innenwand licht), und der Abzug der Decke
            push(
                bau()
                    .mm("Listenlänge", w.list_length)
                    .m3("Deckenauflager", w.pocket)
                    .mm("Abfangung Verblender", w.facing_support)
                    .satz(EIGEN),
            );
        }
        ElementQto::Slab(p) => push(
            bau()
                .mm("Width", p.thickness)
                .mm("Perimeter", p.perimeter)
                .m2("GrossArea", p.area)
                .m3("NetVolume", p.volume)
                .satz("Qto_SlabBaseQuantities"),
        ),
        ElementQto::Floor(p) => {
            push(
                bau()
                    .mm("Width", p.thickness)
                    .mm("Perimeter", p.perimeter)
                    .m2("GrossArea", p.area)
                    .m3("NetVolume", p.volume)
                    .satz("Qto_SlabBaseQuantities"),
            );
            push(bau().m3("davon Auflager", p.bearing).satz(EIGEN));
        }
        ElementQto::Footing(f) => push(
            bau()
                .mm("Length", f.length)
                .mm("Width", f.width)
                .mm("Height", f.depth)
                .m3("NetVolume", f.volume)
                .satz("Qto_FootingBaseQuantities"),
        ),
        // IFC4 kennt für IfcBuildingElementPart keinen Mengensatz
        ElementQto::Strip(f) => push(
            bau()
                .mm("Länge", f.length)
                .mm("Breite", f.width)
                .mm("Höhe", f.height)
                .m3("Volumen", f.volume)
                .satz(EIGEN),
        ),
        ElementQto::Soffit(f) => {
            let dicke = f.thickness + f.cladding.as_ref().map_or(0.0, |c| c.thickness);
            push(
                bau()
                    .mm("Width", dicke)
                    .m2("NetArea", f.area)
                    .satz("Qto_CoveringBaseQuantities"),
            );
            push(
                bau()
                    .m3("Volumen", q.volume())
                    .m3("Volumen Dämmung", f.volume)
                    .m2(
                        "Fläche Bekleidung",
                        f.cladding.as_ref().map_or(0.0, |c| c.area),
                    )
                    .satz(EIGEN),
            );
        }
        ElementQto::Perimeter(f) => {
            push(
                bau()
                    .mm("Width", f.thickness)
                    .m2("NetArea", f.area)
                    .satz("Qto_CoveringBaseQuantities"),
            );
            push(bau().m3("Volumen", f.volume).satz(EIGEN));
        }
        ElementQto::Terrace(t) => {
            let dicke: f64 = t.layers.iter().map(|l| l.1).sum();
            if klasse == "IfcRoof" {
                push(
                    bau()
                        .m2("GrossArea", t.area)
                        .m2("ProjectedArea", t.area)
                        .satz("Qto_RoofBaseQuantities"),
                );
            } else {
                push(
                    bau()
                        .mm("Width", dicke)
                        .m2("GrossArea", t.area)
                        .satz("Qto_CoveringBaseQuantities"),
                );
            }
            push(
                bau()
                    .m3("Volumen", t.volume)
                    .m3("Volumen Dämmung", t.insulation_volume)
                    .m3("Volumen Belag", t.finish_volume)
                    .satz(EIGEN),
            );
        }
        // Blech: Fläche aus Länge und Abwicklung; abgerechnet wird nach Länge
        ElementQto::Coping(c) => {
            push(
                bau()
                    .m2("GrossArea", c.length * c.girth)
                    .satz("Qto_CoveringBaseQuantities"),
            );
            push(
                bau()
                    .mm("Länge", c.length)
                    .m3("Volumen", q.volume())
                    .mm("Abwicklung", c.girth)
                    .mm("Zuschnitt", c.cut)
                    .stk("Ecken", c.corners as f64)
                    .stk("Endabschlüsse", c.ends as f64)
                    .satz(EIGEN),
            );
        }
        ElementQto::Ext(x) => {
            let mut b = bau().m3("Volumen", x.volume);
            for m in &x.mengen {
                // Namen sind je Satz eindeutig: die erste Menge gilt
                let Some(v) = m.wert.filter(|_| b.0.iter().all(|y| y.name != m.name)) else {
                    continue;
                };
                b = match m.einheit.as_str() {
                    "stk" => b.stk(&m.name, v),
                    "m" => b.mm(&m.name, v * 1e3),
                    "m2" => b.m2(&m.name, v * 1e6),
                    "m3" => b.m3(&m.name, v * 1e9),
                    "kg" => b.kg(&m.name, v),
                    "t" => b.kg(&m.name, v * 1e3),
                    _ => b,
                };
            }
            push(b.satz(EIGEN));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::qto::{EdgeStripQto, ExtMenge, ExtQto};

    #[test]
    fn einheiten_der_datei() {
        let q = ElementQto::Strip(EdgeStripQto {
            length: 5000.0,
            volume: 5000.0 * 120.0 * 250.0,
            width: 120.0,
            height: 250.0,
        });
        let s = saetze(&q, "IfcBuildingElementPart");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name, EIGEN);
        let v = &s[0].mengen;
        assert_eq!(v[0].wert, 5000.0);
        assert_eq!(v[3].art, Art::Volumen);
        assert!((v[3].wert - 0.15).abs() < 1e-12);
    }

    #[test]
    fn erweiterung_nach_einheit_ohne_null() {
        let m = |name: &str, einheit: &str, wert: Option<f64>| ExtMenge {
            satz: 0,
            name: name.into(),
            einheit: einheit.into(),
            wert,
            gewerk: None,
            kg: None,
            dicke: None,
        };
        let q = ElementQto::Ext(ExtQto {
            key: "werk.stabgelaender".into(),
            volume: 0.0,
            mengen: vec![
                m("Geländer", "m", Some(3.0)),
                m("Pfosten", "stk", Some(4.0)),
                m("Stahl", "t", Some(0.05)),
                m("kaputt", "m", None),
                m("leer", "m2", Some(0.0)),
            ],
        });
        let s = saetze(&q, "IfcRailing");
        let v: Vec<(&str, Art, f64)> = s[0]
            .mengen
            .iter()
            .map(|m| (m.name.as_str(), m.art, m.wert))
            .collect();
        assert_eq!(
            v,
            vec![
                ("Geländer", Art::Laenge, 3000.0),
                ("Pfosten", Art::Anzahl, 4.0),
                ("Stahl", Art::Masse, 50.0),
            ]
        );
    }
}

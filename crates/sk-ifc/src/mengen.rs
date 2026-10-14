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

/// Mengen einer Öffnung in einer Wand (I5): sichtbare Breite und Höhe,
/// Wanddicke und Fläche.
pub fn oeffnung(breite: f64, hoehe: f64, tiefe: f64) -> Option<Satz> {
    bau()
        .mm("Width", breite)
        .mm("Height", hoehe)
        .mm("Depth", tiefe)
        .m2("Area", breite * hoehe)
        .satz("Qto_OpeningElementBaseQuantities")
}

/// Mengen einer Tür in ihrer Öffnung (I5): Breite, Höhe und Fläche der
/// Öffnung in der Wand des Blatts.
pub fn tuer(breite: f64, hoehe: f64) -> Option<Satz> {
    bau()
        .mm("Width", breite)
        .mm("Height", hoehe)
        .m2("Area", breite * hoehe)
        .satz("Qto_DoorBaseQuantities")
}

/// Maße eines Raums (I6), mm bzw. mm²: roh aus der Form, fertig aus dem
/// Fertigumriss, Wand und Decke aus dem Raumbuch.
#[derive(Clone, Copy, Debug, Default)]
pub struct Raum {
    /// OK Rohdecke bis UK Rohdecke darüber.
    pub hoehe: f64,
    /// OK Fertigfußboden bis UK Fertigdecke.
    pub lichte_hoehe: f64,
    /// Fußbodenaufbau.
    pub fussboden: f64,
    pub brutto_umfang: f64,
    pub netto_umfang: f64,
    pub brutto_flaeche: f64,
    pub netto_flaeche: f64,
    pub wand: f64,
    pub decke: f64,
}

/// `Qto_SpaceBaseQuantities` eines Raums (IFC4): `Height` im Rohbau,
/// `FinishCeilingHeight` licht, `FinishFloorHeight` der Fußbodenaufbau;
/// Brutto roh, Netto fertig; Rauminhalte als Fläche × Höhe.
pub fn raum(r: Raum) -> Option<Satz> {
    bau()
        .mm("Height", r.hoehe)
        .mm("FinishCeilingHeight", r.lichte_hoehe)
        .mm("FinishFloorHeight", r.fussboden)
        .mm("GrossPerimeter", r.brutto_umfang)
        .mm("NetPerimeter", r.netto_umfang)
        .m2("GrossFloorArea", r.brutto_flaeche)
        .m2("NetFloorArea", r.netto_flaeche)
        .m2("NetWallArea", r.wand)
        .m2("NetCeilingArea", r.decke)
        .m3("GrossVolume", r.brutto_flaeche * r.hoehe)
        .m3("NetVolume", r.netto_flaeche * r.lichte_hoehe)
        .satz("Qto_SpaceBaseQuantities")
}

/// Mengen einer Bekleidung des Raumbuchs (I6): Dicke und Fläche aus dem
/// Raumbuch in `Qto_CoveringBaseQuantities`, Rauminhalt des Körpers in
/// mm³; eine Sockelleiste (`leiste`) hat statt der Fläche ihre Länge in mm.
pub fn bekleidung(leiste: bool, dicke: f64, menge: f64, volumen: f64) -> Vec<Satz> {
    let norm = if leiste {
        bau().mm("Width", dicke)
    } else {
        bau().mm("Width", dicke).m2("NetArea", menge)
    };
    let mut eigen = bau().m3("Volumen", volumen);
    if leiste {
        eigen = eigen.mm("Länge", menge);
    }
    norm.satz("Qto_CoveringBaseQuantities")
        .into_iter()
        .chain(eigen.satz(EIGEN))
        .collect()
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
        // Räume: Der Export nimmt die Liste ohne Räume (`qto::schedule`);
        // Raum und Ausbau kommen mit I6 aus sk-raum (Qto_SpaceBaseQuantities)
        ElementQto::Room(_) => {}
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
    use sk_model::qto::{EdgeStripQto, ExtMenge, ExtQto, TerraceQto};

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

    /// Review #90 H5: die Terrasse als Dach oder als Bekleidung; in keinem
    /// Referenzhaus ist sie Bekleidung.
    #[test]
    fn terrasse_als_dach_oder_bekleidung() {
        let stoff = sk_model::Model::new().materials().ids().next().unwrap();
        let q = ElementQto::Terrace(TerraceQto {
            area: 12e6,
            volume: 12e6 * 260.0,
            insulation_volume: 12e6 * 200.0,
            finish_volume: 12e6 * 60.0,
            layers: vec![(stoff, 60.0, 12e6 * 60.0), (stoff, 200.0, 12e6 * 200.0)],
            depth: 3000.0,
        });
        let namen = |k: &str| -> Vec<(&'static str, Vec<String>)> {
            saetze(&q, k)
                .into_iter()
                .map(|s| (s.name, s.mengen.into_iter().map(|m| m.name).collect()))
                .collect()
        };
        let dach = namen("IfcRoof");
        assert_eq!(dach[0].0, "Qto_RoofBaseQuantities");
        assert_eq!(dach[0].1, ["GrossArea", "ProjectedArea"]);
        let bek = namen("IfcCovering");
        assert_eq!(bek[0].0, "Qto_CoveringBaseQuantities");
        assert_eq!(bek[0].1, ["Width", "GrossArea"]);
        let w = &saetze(&q, "IfcCovering")[0].mengen[0];
        assert_eq!((w.art, w.wert), (Art::Laenge, 260.0));
        assert_eq!(bek[1].0, EIGEN);
        assert_eq!(bek[1].1, ["Volumen", "Volumen Dämmung", "Volumen Belag"]);
    }
}

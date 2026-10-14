//! Rechenweg einer Mengenzeile (Jörn 11.10., vob/plan-rechenweg-
//! bestellmengen.md): die Terme, aus denen die gezeigte Menge entsteht, in
//! Summe genau die Menge. Zwei Sichten: VOB-Menge (maßgebend für
//! Ausschreibung und Kosten) und Bestellmenge (Öffnungen nach der Regel des
//! Baustoffs, dazu der Verschnittzuschlag).
//!
//! Gerechnet wird nur auf Anfrage, je Zeile; die Mengenliste selbst bleibt,
//! wie sie ist ([`crate::qto::schedule`]).

use crate::element::{Category, ElementKind};
use crate::matprop;
use crate::model::Model;
use crate::qto::{foundation_qto, wall_face, Face, LayerParts, LayerRow};
use crate::vob;

/// Welche Menge der Zeile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bezug {
    /// mm
    Laenge,
    /// mm² (Abrechnungsfläche `LayerRow.face`, sonst `area`)
    Flaeche,
    /// mm³
    Volumen,
}

/// Rolle eines Terms; das Vorzeichen steckt im Wert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// Grundmenge („Länge × Höhe“).
    Basis,
    Zuschlag,
    Abzug,
    /// Ohne Wert, z. B. „übermessen“.
    Hinweis,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Term {
    pub text: String,
    /// In der kleinsten Einheit des Bezugs, mit Vorzeichen; 0 beim Hinweis.
    pub wert: f64,
    pub art: Art,
}

/// Sicht auf die Mengen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Modus {
    /// Abrechnungsmenge nach VOB/C.
    #[default]
    Vob,
    /// Was der Unternehmer bestellt: mit Verschnittzuschlag.
    Bestellung,
}

impl Modus {
    pub const ALL: [Modus; 2] = [Modus::Vob, Modus::Bestellung];

    pub fn label(self) -> &'static str {
        match self {
            Modus::Vob => "VOB",
            Modus::Bestellung => "Bestellung",
        }
    }

    /// Wert in `einstellungen.txt`.
    pub fn key(self) -> &'static str {
        match self {
            Modus::Vob => "vob",
            Modus::Bestellung => "bestellung",
        }
    }

    pub fn from_key(k: &str) -> Option<Modus> {
        Modus::ALL.into_iter().find(|m| m.key() == k)
    }
}

/// Rechenweg einer Menge.
#[derive(Clone, Debug, PartialEq)]
pub struct Rechenweg {
    pub bezug: Bezug,
    pub terme: Vec<Term>,
}

impl Rechenweg {
    /// Ergebnis: Summe der Terme.
    pub fn menge(&self) -> f64 {
        self.terme.iter().map(|t| t.wert).sum()
    }
}

/// Unterschied, ab dem ein Ausgleichsterm erscheint (mm, mm², mm³): unter
/// einem Tausendstel der Anzeige.
fn eps(b: Bezug) -> f64 {
    match b {
        Bezug::Laenge => 0.5,
        Bezug::Flaeche => 500.0,
        Bezug::Volumen => 500_000.0,
    }
}

/// „12,240“ aus mm.
fn m(x: f64) -> String {
    format!("{:.3}", x / 1e3).replace('.', ",")
}

/// „12,240 m²“ bzw. „1,250 m³“.
fn menge_text(x: f64, b: Bezug) -> String {
    let (t, u) = match b {
        Bezug::Laenge => (1e3, "m"),
        Bezug::Flaeche => (1e6, "m²"),
        Bezug::Volumen => (1e9, "m³"),
    };
    format!("{} {u}", format!("{:.3}", x / t).replace('.', ","))
}

/// Menge der Zeile im Bezug.
pub fn wert(r: &LayerRow, b: Bezug) -> f64 {
    match b {
        // die Länge der Liste; die Abrechnungslänge nur, wo sie fehlt
        Bezug::Laenge => {
            if r.length > 0.0 {
                r.length
            } else {
                r.bill_length
            }
        }
        // Die Fläche der Liste bleibt, wo es sie gab (Dämmung, Platten;
        // Review #77/2); die Abrechnungsfläche nur, wo die Liste keine
        // hatte (Mauerwerk, Putz)
        Bezug::Flaeche => {
            if r.area > 0.0 {
                r.area
            } else {
                r.face
            }
        }
        Bezug::Volumen => r.volume,
    }
}

/// Verschnittzuschlag der Zeile in % (am Baustoff, sonst nach Art).
pub fn zuschlag(model: &Model, r: &LayerRow) -> f64 {
    model.material(r.material).map_or(0.0, matprop::waste)
}

/// Gezeigte Menge der Zeile in der Sicht `modus`, ohne Rechenweg (für die
/// ganze Liste): VOB wie die Mengenliste, Bestellung mit den Öffnungen
/// nach der Regel des Baustoffs und dem Zuschlag.
pub fn anzeige(model: &Model, r: &LayerRow, b: Bezug, modus: Modus) -> f64 {
    match modus {
        Modus::Vob => wert(r, b),
        Modus::Bestellung => vor_zuschlag(model, r, b) * (1.0 + zuschlag(model, r) / 100.0),
    }
}

/// Öffnungen in der Bestellmenge abziehen ab (mm²); `None`: übermessen.
fn ab(model: &Model, r: &LayerRow) -> Option<f64> {
    model.material(r.material).and_then(matprop::opening_from)
}

/// Bestellmenge vor dem Verschnittzuschlag: die VOB-Fläche mit den
/// Öffnungen nach der Regel des Baustoffs statt nach VOB; das Volumen
/// mit den übermessenen Öffnungen.
fn vor_zuschlag(model: &Model, r: &LayerRow, b: Bezug) -> f64 {
    let (da, dv) = vob::bestell_mehr(&r.holes, ab(model, r));
    match b {
        Bezug::Flaeche => wert(r, b) + da,
        Bezug::Volumen => wert(r, b) + dv,
        Bezug::Laenge => wert(r, b),
    }
}

/// Rechenwege einer Mengenzeile für jede Menge, die sie hat (Fläche,
/// Volumen, Länge), in der Sicht `modus`.
pub fn fuer(model: &Model, r: &LayerRow, modus: Modus) -> Vec<Rechenweg> {
    [Bezug::Flaeche, Bezug::Volumen, Bezug::Laenge]
        .into_iter()
        .filter_map(|b| weg(model, r, b, modus))
        .collect()
}

/// Rechenweg einer Menge der Zeile; `None`, wenn sie die Menge nicht hat.
/// Rechnet keine Geometrie (die Teile stehen an der Zeile).
pub fn weg(model: &Model, r: &LayerRow, b: Bezug, modus: Modus) -> Option<Rechenweg> {
    if wert(r, b) <= 0.0 {
        return None;
    }
    let ziel = match modus {
        Modus::Vob => wert(r, b),
        Modus::Bestellung => vor_zuschlag(model, r, b),
    };
    let mut terme = match (&r.parts, b) {
        (Some(p), Bezug::Flaeche) => {
            let mut t = wand_flaeche(model, r, p);
            t.extend(loch_terme(model, r, b, modus));
            t
        }
        (Some(p), Bezug::Volumen) => {
            let mut t = wand_volumen(p);
            t.extend(loch_terme(model, r, b, modus));
            t
        }
        (Some(_), Bezug::Laenge) => vec![basis(
            if r.category == Category::InteriorWall {
                "lichte Länge der tragenden Schicht".into()
            } else {
                "Länge der Außenseite".into()
            },
            ziel,
        )],
        // Raumzeile (RB5): Fläche bzw. Länge mit den Termen des Raumbuchs
        _ if !r.terms.is_empty() && b != Bezug::Volumen => r.terms.clone(),
        _ => match frost(model, r, b).or_else(|| decke(model, r, b)) {
            Some(t) => t,
            None => sonst(r, b),
        },
    };
    ausgleich(&mut terme, ziel, b);
    if modus == Modus::Bestellung {
        let z = zuschlag(model, r);
        if z > 0.0 {
            let summe: f64 = terme.iter().map(|t| t.wert).sum();
            terme.push(Term {
                text: format!("Verschnittzuschlag {} %", de(z)),
                wert: summe * z / 100.0,
                art: Art::Zuschlag,
            });
        }
    }
    Some(Rechenweg { bezug: b, terme })
}

/// Zahl ohne überflüssige Nullen: „5“, „7,5“.
fn de(x: f64) -> String {
    let s = format!("{x:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.replace('.', ",")
}

/// Ausgleich, damit die Terme genau die Menge ergeben (Ecken und Anschlüsse
/// an andere Wände, Rundung der Geometrie).
fn ausgleich(terme: &mut Vec<Term>, ziel: f64, b: Bezug) {
    let rest = ziel - terme.iter().map(|t| t.wert).sum::<f64>();
    if rest.abs() > eps(b) {
        terme.push(Term {
            text: "Ecken und Anschlüsse".into(),
            wert: rest,
            art: if rest > 0.0 {
                Art::Zuschlag
            } else {
                Art::Abzug
            },
        });
    } else if let Some(t) = terme.iter_mut().find(|t| t.art == Art::Basis) {
        // Rundungsrest in die Grundmenge, die Summe stimmt dann genau
        t.wert += rest;
    }
}

fn basis(text: String, wert: f64) -> Term {
    Term {
        text,
        wert,
        art: Art::Basis,
    }
}

/// Zuschlag oder Abzug mit Text, nur wenn er zählt.
fn dazu(terme: &mut Vec<Term>, text: impl Into<String>, wert: f64, b: Bezug) {
    if wert.abs() > eps(b) {
        terme.push(Term {
            text: text.into(),
            wert,
            art: if wert > 0.0 {
                Art::Zuschlag
            } else {
                Art::Abzug
            },
        });
    }
}

/// Abrechnungsfläche einer Wandschicht (Regel 84) nach ihrer Lage.
fn wand_flaeche(model: &Model, r: &LayerRow, p: &LayerParts) -> Vec<Term> {
    let b = Bezug::Flaeche;
    let layers = model.element_layers(r.element);
    let interior = r.category == Category::InteriorWall;
    let band = (p.h - p.h_cut - p.h_here).max(0.0);
    let mut t = Vec::new();
    match wall_face(model, &layers, r.layer, interior) {
        Face::Mean => {
            // Mittellinie der Schicht, an Ecken verschnitten: Grundfläche ÷ Dicke
            let len = if p.dicke > 0.0 {
                p.grund / p.dicke
            } else {
                0.0
            };
            t.push(basis(
                format!("Länge {} m × Höhe {} m", m(len), m(p.h)),
                len * p.h,
            ));
            dazu(
                &mut t,
                format!("Deckenauflager {} m × {} m", m(len), m(band)),
                -len * band,
                b,
            );
            dazu(
                &mut t,
                format!("Abschnitt der Wand darüber {} m × {} m", m(len), m(p.h_cut)),
                -len * p.h_cut,
                b,
            );
            if p.dicke > 0.0 {
                dazu(
                    &mut t,
                    "Abschnitte der Wand darüber",
                    p.up_volume / p.dicke,
                    b,
                );
                dazu(
                    &mut t,
                    "Attika über dem Terrassenrand",
                    p.attika / p.dicke,
                    b,
                );
            }
            dazu(
                &mut t,
                format!("Fußpunkt Schaumglas {} m × {} m", m(len), m(p.h_foot)),
                -len * p.h_foot,
                b,
            );
        }
        Face::Outer => {
            t.push(basis(
                format!("Länge außen {} m × Höhe {} m", m(p.outer_len), m(p.h)),
                p.outer_len * p.h,
            ));
            dazu(
                &mut t,
                format!(
                    "Abschnitt der Wand darüber {} m × {} m",
                    m(p.outer_len),
                    m(p.h_cut)
                ),
                -p.outer_len * p.h_cut,
                b,
            );
            dazu(
                &mut t,
                format!(
                    "Fußpunkt Schaumglas {} m × {} m",
                    m(p.outer_len),
                    m(p.h_foot)
                ),
                -p.outer_len * p.h_foot,
                b,
            );
            dazu(&mut t, "Abschnitte der Wand darüber", p.up_side, b);
            dazu(&mut t, "Attika über dem Terrassenrand", p.attika_side, b);
        }
        Face::Inner => {
            t.push(basis(
                format!("Länge innen {} m × Höhe {} m", m(p.inner_len), m(p.h)),
                p.inner_len * p.h,
            ));
            dazu(
                &mut t,
                format!("Deckenband {} m × {} m", m(p.inner_len), m(band)),
                -p.inner_len * band,
                b,
            );
            dazu(
                &mut t,
                format!(
                    "Abschnitt der Wand darüber {} m × {} m",
                    m(p.inner_len),
                    m(p.h_cut)
                ),
                -p.inner_len * p.h_cut,
                b,
            );
        }
    }
    t
}

/// Öffnungen einer Wandschicht: abgezogen mit ihren Maßen, übermessen als
/// Hinweis. Nach VOB/C die Fläche über 2,5 m² und das Volumen über 0,5 m³
/// (DIN 18330, 18331), in der Bestellung nach der Regel des Baustoffs.
fn loch_terme(model: &Model, r: &LayerRow, b: Bezug, modus: Modus) -> Vec<Term> {
    let ab = ab(model, r);
    let mut out: Vec<Term> = r
        .holes
        .iter()
        .map(|h| {
            let name = model.element(h.opening).map_or("", |e| e.number.as_str());
            let art = model
                .opening(h.opening)
                .map_or("Öffnung", |o| o.kind.name());
            let mass = format!("{name} {art} {} × {} m", m(h.width), m(h.height));
            let teil = match b {
                Bezug::Volumen if h.total_vol > h.volume + 1.0 => {
                    format!(", Teilstück von {}", menge_text(h.total_vol, b))
                }
                Bezug::Volumen => String::new(),
                _ if h.total > h.area + 1.0 => {
                    format!(", Teilstück von {}", menge_text(h.total, Bezug::Flaeche))
                }
                _ => String::new(),
            };
            let abzug = match (b, modus) {
                (Bezug::Volumen, Modus::Vob) => h.vob_volumen(),
                (_, Modus::Vob) => h.vob(),
                (_, Modus::Bestellung) => h.bestellung(ab),
            };
            let wert = if b == Bezug::Volumen {
                h.volume
            } else {
                h.area
            };
            if abzug {
                Term {
                    text: format!("{mass}{teil}"),
                    wert: -wert,
                    art: Art::Abzug,
                }
            } else {
                Term {
                    text: match (modus, b) {
                        (Modus::Vob, Bezug::Volumen) => {
                            format!("{mass} übermessen (bis 0,5 m³, VOB/C{teil})")
                        }
                        (Modus::Vob, _) => format!("{mass} übermessen (bis 2,5 m², VOB/C{teil})"),
                        (Modus::Bestellung, _) => format!("{mass} übermessen (Bestellung{teil})"),
                    },
                    wert: 0.0,
                    art: Art::Hinweis,
                }
            }
        })
        .collect();
    // Innentür: Die Wand ist bis an die Öffnung verputzt, die Laibung
    // bleibt roh, die Umfassungszarge greift um den Putz (Jörn 12.10.
    // 06:41, vob/pruefung-innentuer.md); daher keine Laibungsmenge
    let tuer = r.holes.iter().any(|h| {
        model
            .opening(h.opening)
            .is_some_and(|o| o.kind == crate::oeffnung::OpeningKind::Door)
    });
    if b == Bezug::Flaeche && r.category == Category::InteriorWall && tuer {
        out.push(Term {
            text: "Innentür: keine Laibung, die Umfassungszarge greift um den Putz".into(),
            wert: 0.0,
            art: Art::Hinweis,
        });
    }
    out
}

/// Volumen einer Wandschicht: Grundfläche × Höhe, Abzüge und Zuschläge.
fn wand_volumen(p: &LayerParts) -> Vec<Term> {
    let b = Bezug::Volumen;
    let band = (p.h - p.h_cut - p.h_here).max(0.0);
    let a = p.grund;
    let mut t = vec![basis(
        format!(
            "Grundfläche {} × Höhe {} m",
            menge_text(a, Bezug::Flaeche),
            m(p.h)
        ),
        a * p.h,
    )];
    dazu(
        &mut t,
        format!("Deckenauflager {} m", m(band)),
        -a * band,
        b,
    );
    dazu(
        &mut t,
        format!("Abschnitt der Wand darüber {} m", m(p.h_cut)),
        -a * p.h_cut,
        b,
    );
    dazu(&mut t, "Abschnitte der Wand darüber", p.up_volume, b);
    dazu(&mut t, "Attika über dem Terrassenrand", p.attika, b);
    dazu(
        &mut t,
        format!("Fußpunkt Schaumglas {} m", m(p.h_foot)),
        -a * p.h_foot,
        b,
    );
    t
}

/// Frostschürze: Länge auf der Mittellinie × Breite × Höhe ab UK Platte.
fn frost(model: &Model, r: &LayerRow, b: Bezug) -> Option<Vec<Term>> {
    if b != Bezug::Volumen {
        return None;
    }
    let ElementKind::StripFooting(_) = model.element(r.element)?.kind else {
        return None;
    };
    let (_, f) = foundation_qto(model, model.run_of(r.element)?)?;
    Some(vec![basis(
        format!(
            "Länge {} m × Breite {} m × Höhe {} m",
            m(f.length),
            m(f.width),
            m(f.depth)
        ),
        f.length * f.width * f.depth,
    )])
}

/// Decke mit Aussparungen für Fenster über zwei Geschosse (VOB-Prüfung
/// vob/pruefung-deckenaussparung.md): der volle Umriss als Basis, jede
/// Kerbe ein eigener Term, übermessen bis 2,5 m² bzw. 0,5 m³ oder
/// abgezogen. `None` ohne Aussparung (dann [`sonst`]).
fn decke(model: &Model, r: &LayerRow, b: Bezug) -> Option<Vec<Term>> {
    let ElementKind::Floor(f) = &model.element(r.element)?.kind else {
        return None;
    };
    let d = model.floor(f.run)?.ok()?;
    let kerben = d.kerben();
    if kerben.is_empty() || b == Bezug::Laenge {
        return None;
    }
    let layers = model.element_layers(r.element);
    let kern = layers.len() == 1 || layers.get(r.layer).is_some_and(|l| l.core);
    if b == Bezug::Volumen && !kern {
        return None;
    }
    let dicke = d.params.thickness;
    let voll = d.area() + kerben.iter().map(|k| k.1).sum::<f64>();
    let mut t = vec![match b {
        Bezug::Volumen => basis(
            format!(
                "Fläche ohne Aussparungen {} × Dicke {} m",
                menge_text(voll, Bezug::Flaeche),
                m(dicke)
            ),
            voll * dicke,
        ),
        _ => basis("Fläche aus dem Umriss ohne Aussparungen".into(), voll),
    }];
    for (id, fl) in kerben {
        let name = id
            .and_then(|i| model.element(i))
            .map_or("", |e| e.number.as_str());
        let schenkel: Vec<_> = d.aussparungen.iter().filter(|a| a.opening == id).collect();
        let mass = match schenkel[..] {
            [a] => format!("{} × {} m", m(a.stirn), m(a.tiefe)),
            _ => menge_text(fl, Bezug::Flaeche),
        };
        let (wert, ab, grenze) = match b {
            Bezug::Volumen => (fl * dicke, vob::ueber_grenze_vol(fl * dicke), "0,5 m³"),
            _ => (fl, vob::ueber_grenze(fl), "2,5 m²"),
        };
        t.push(if ab {
            Term {
                text: format!("Aussparung {name} {mass}"),
                wert: -wert,
                art: Art::Abzug,
            }
        } else {
            Term {
                text: format!("Aussparung {name} {mass} übermessen (bis {grenze}, VOB/C)"),
                wert: 0.0,
                art: Art::Hinweis,
            }
        });
    }
    if b == Bezug::Volumen && r.pocket > eps(b) {
        t.push(Term {
            text: format!("davon Auflager in den Wänden {}", menge_text(r.pocket, b)),
            wert: 0.0,
            art: Art::Hinweis,
        });
    }
    Some(t)
}

/// Bauteile ohne Wandgeometrie: Platten, Decken, Dämmungen, Streifen.
fn sonst(r: &LayerRow, b: Bezug) -> Vec<Term> {
    let ziel = wert(r, b);
    match b {
        Bezug::Volumen if r.thickness > 0.0 && wert(r, Bezug::Flaeche) > 0.0 => {
            let a = wert(r, Bezug::Flaeche);
            let mut t = vec![basis(
                format!(
                    "Fläche {} × Dicke {} m",
                    menge_text(a, Bezug::Flaeche),
                    m(r.thickness)
                ),
                a * r.thickness,
            )];
            // Die Fläche der Decke reicht schon bis in die Wände
            if r.pocket > eps(b) {
                t.push(Term {
                    text: format!("davon Auflager in den Wänden {}", menge_text(r.pocket, b)),
                    wert: 0.0,
                    art: Art::Hinweis,
                });
            }
            t
        }
        Bezug::Flaeche => vec![basis("Fläche aus dem Umriss".into(), ziel)],
        Bezug::Laenge => vec![basis("Länge aus dem Modell".into(), ziel)],
        Bezug::Volumen => vec![basis("Volumen aus dem Modell".into(), ziel)],
    }
}

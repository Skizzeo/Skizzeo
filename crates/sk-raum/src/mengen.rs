//! Raumbuch in der Mengenliste (RB5, `raumbuch/plan.md` §6.2, mit Kosten
//! abgestimmt): je gespeichertem Raum die Zeilen von
//! [`crate::raumbuch::rechnen`] als Zeilen der Mengenliste
//! ([`sk_model::qto_raum`]) mit Namen, Seite, Gewerk und Kostengruppe nach
//! dem Träger (DIN 276:2018):
//! - Boden, Estrich, Sockelleiste, Randdämmstreifen: auf der Sohlplatte 324
//!   (ihre Abdichtung 325), auf einer Decke 353;
//! - Wandseite, ihre Laibungen und Fliesen: an der Außenwand 336, sonst 345;
//! - Decke 354;
//! - Eckschienen, Silikon und Dichtband gehören zu keiner einzelnen
//!   Wandseite und zählen in 345 (Ecken liegen meist an Innenwänden;
//!   Nachtregel, änderbar).
//!
//! Die App gibt [`raeume`] an [`sk_model::qto::schedule_mit`].

use crate::ausbau::RaumAusbau;
use crate::erkennung::{raeume as formen, Begrenzung, Kante, RaumForm};
use crate::oeffnungen::Rohbau;
use crate::raumbuch::{rechnen, Flaeche, Regeln, Zeile};
use crate::raumliste::{fertigmass, traeger};
use sk_model::qto::{RoomLine, RoomQto, Side};
use sk_model::rechenweg::Rechenweg;
use sk_model::{Category, ElementId, LayerFunction, Model, StoreyId};

/// Kostengruppen nach DIN 276:2018.
const KG_SOHLE: u16 = 324;
const KG_SOHLE_ABDICHTUNG: u16 = 325;
const KG_DECKENBELAG: u16 = 353;
const KG_AUSSENWAND_INNEN: u16 = 336;
const KG_INNENWAND: u16 = 345;
const KG_DECKENBEKLEIDUNG: u16 = 354;

/// Die gespeicherten Räume des Geschosses `geschoss` mit den Zeilen ihres
/// Raumbuchs. Ein Raum ohne Ausbau (die Ausbautypen seiner Vorlage fehlen
/// im Projekt) hat keine Mengen und fehlt, so bleiben Listen ohne
/// Raumbuch, wie sie waren.
pub fn raeume(model: &Model, geschoss: StoreyId) -> Vec<(ElementId, RoomQto)> {
    formen(model, geschoss)
        .into_iter()
        .filter_map(|f| raum(model, f))
        .collect()
}

fn raum(model: &Model, f: RaumForm) -> Option<(ElementId, RoomQto)> {
    let id = f.raum?;
    let roh = Rohbau::neu(model, f);
    let aus = RaumAusbau::fuer(model, &roh)?;
    let (_, m, _) = fertigmass(model, &roh, &aus);
    let kanten: Vec<&Kante> = roh.kanten().collect();
    let mut lines: Vec<RoomLine> = rechnen(model, &roh, &aus, &Regeln::default())
        .into_iter()
        .map(|z| zeile(model, &roh, &kanten, z))
        .collect();
    // Ohne Ausbautypen im Projekt keine Zeilen, dann fehlt der Raum
    if lines.is_empty() {
        return None;
    }
    // Boden, Wände, Decke; in jeder Seite die Reihenfolge des Raumbuchs
    lines.sort_by_key(|l| match l.side {
        Side::Floor => 0,
        Side::Wall | Side::Inner | Side::Outer => 1,
        Side::Ceiling => 2,
    });
    Some((
        id,
        RoomQto {
            area: m.flaeche_fertig,
            height: m.hoehe_fertig,
            lines,
        },
    ))
}

fn zeile(model: &Model, roh: &Rohbau, kanten: &[&Kante], z: Zeile) -> RoomLine {
    let schicht = z
        .typ
        .and_then(|(t, j)| model.layer_set(t)?.layers.get(j).copied());
    let material = schicht.map(|l| l.material);
    let trade = schicht.and_then(|l| l.trade.or_else(|| model.material(l.material)?.trade));
    let side = match z.flaeche {
        Flaeche::Boden | Flaeche::Estrich | Flaeche::Sockel | Flaeche::Randstreifen => Side::Floor,
        Flaeche::Decke => Side::Ceiling,
        _ => Side::Wall,
    };
    let kg = match z.flaeche {
        Flaeche::Decke => KG_DECKENBEKLEIDUNG,
        Flaeche::Wand(k)
        | Flaeche::Laibung(k)
        | Flaeche::Fliesen(k)
        | Flaeche::FliesenLaibung(k) => wand_kg(model, kanten, k),
        Flaeche::Eckschienen | Flaeche::Silikon | Flaeche::Dichtband => KG_INNENWAND,
        Flaeche::Boden | Flaeche::Estrich | Flaeche::Sockel | Flaeche::Randstreifen => {
            let unten = traeger(model, roh, kanten, Flaeche::Boden)
                .and_then(|id| model.element(id))
                .map(|e| e.category);
            match unten {
                Some(Category::GroundSlab)
                    if schicht.is_some_and(|l| l.function == LayerFunction::Membrane) =>
                {
                    KG_SOHLE_ABDICHTUNG
                }
                Some(Category::GroundSlab) => KG_SOHLE,
                _ => KG_DECKENBELAG,
            }
        }
    };
    RoomLine {
        name: name(model, kanten, z.flaeche),
        side,
        finish: z.typ,
        material,
        thickness: z.dicke,
        trade,
        kg: Some(kg),
        key: z.schluessel,
        info: matches!(
            z.flaeche,
            Flaeche::Laibung(_) | Flaeche::FliesenLaibung(_) | Flaeche::Dichtband
        ),
        weg: Rechenweg {
            bezug: z.bezug,
            terme: z.terme,
        },
    }
}

/// Kostengruppe einer Wandseite: an der Außenwand 336, sonst 345.
fn wand_kg(model: &Model, kanten: &[&Kante], k: usize) -> u16 {
    let aussen = wand(kanten, k)
        .and_then(|w| model.element(w))
        .is_some_and(|e| matches!(e.category, Category::ExteriorWall | Category::Parapet));
    if aussen {
        KG_AUSSENWAND_INNEN
    } else {
        KG_INNENWAND
    }
}

fn wand(kanten: &[&Kante], k: usize) -> Option<ElementId> {
    let Begrenzung::Wand { wand, .. } = kanten.get(k)?.quelle;
    Some(wand)
}

/// „Wand AW-004 Nord“: Nummer der Wand und Himmelsrichtung, in die die
/// Wandseite vom Raum aus liegt.
fn seite(model: &Model, kanten: &[&Kante], k: usize) -> String {
    let nummer = wand(kanten, k)
        .and_then(|w| model.element(w))
        .map_or_else(|| "Wand".into(), |e| e.number.clone());
    match kanten.get(k) {
        Some(kante) => format!("{nummer} {}", richtung(model, kante)),
        None => nummer,
    }
}

/// Himmelsrichtung der Wandseite vom Raum aus: rechts der Kante liegt die
/// Wand (Umriss gegen, Löcher im Uhrzeigersinn), Nord nach dem Projekt.
fn richtung(model: &Model, k: &Kante) -> &'static str {
    const NAMEN: [&str; 8] = [
        "Nord", "Nordost", "Ost", "Südost", "Süd", "Südwest", "West", "Nordwest",
    ];
    let d = k.b - k.a;
    let az = d.y.atan2(-d.x).to_degrees() - model.location().north_deg();
    NAMEN[((az + 22.5).rem_euclid(360.0) / 45.0) as usize % 8]
}

fn name(model: &Model, kanten: &[&Kante], f: Flaeche) -> String {
    match f {
        Flaeche::Boden => "Boden".into(),
        Flaeche::Estrich => "Estrich".into(),
        Flaeche::Decke => "Decke".into(),
        Flaeche::Wand(k) => format!("Wand {}", seite(model, kanten, k)),
        Flaeche::Laibung(k) => format!("Laibungen {}", seite(model, kanten, k)),
        Flaeche::Fliesen(k) => format!("Wandfliesen {}", seite(model, kanten, k)),
        Flaeche::FliesenLaibung(k) => format!("Fliesenlaibungen {}", seite(model, kanten, k)),
        Flaeche::Sockel => "Sockelleiste".into(),
        Flaeche::Eckschienen => "Eckschienen".into(),
        Flaeche::Randstreifen => "Randdämmstreifen".into(),
        Flaeche::Silikon => "Silikonfugen".into(),
        Flaeche::Dichtband => "Dichtband".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raumliste::tests::haus;
    use sk_model::qto::{schedule, schedule_mit, ElementQto, LayerRow};
    use sk_model::rechenweg::{self, Bezug, Modus};

    /// Räume ohne Ausbautypen im Projekt (alte Dateien, Haus ohne Raumbuch)
    /// haben keine Mengen: Die Liste bleibt, wie sie war.
    #[test]
    fn ohne_ausbau_keine_raeume() {
        let m = crate::raumliste::tests::haus_roh();
        let n = m
            .elements()
            .iter()
            .filter(|(_, e)| e.category == Category::Space)
            .count();
        assert_eq!(n, 2, "zwei Räume");
        assert_eq!(schedule_mit(&m, &raeume), schedule(&m));
    }

    /// Haus 10 × 8 m mit zwei Geschossen und je einem Raum (Wohnzimmer):
    /// Ohne Räume bleibt die Liste, wie sie war; mit ihnen hat jedes
    /// Geschoss die Gruppe Räume, die Schichten zählen nach Gewerk und
    /// Kostengruppe nach Träger, Sockelleiste und Randdämmstreifen als
    /// Automatikmengen am Raum.
    #[test]
    fn raumbuch_in_der_mengenliste() {
        let m = haus();
        let ohne = schedule(&m);
        let mit = schedule_mit(&m, &raeume);
        let raumgruppe = |s: &sk_model::qto::StoreyQto| {
            s.groups
                .iter()
                .find(|g| g.category == Category::Space)
                .cloned()
        };
        let b = &mit.buildings[0];
        assert!(ohne.buildings[0]
            .storeys
            .iter()
            .all(|s| raumgruppe(s).is_none()));
        assert!(ohne.auto.iter().all(|a| !a.key.starts_with("room.")));
        let gruppen: Vec<_> = b.storeys.iter().filter_map(raumgruppe).collect();
        assert_eq!(gruppen.len(), 2);
        for g in &gruppen {
            assert_eq!(g.rows.len(), 1);
            let Some(ElementQto::Room(q)) = &g.rows[0].q else {
                panic!("Raum ohne Mengen");
            };
            // Fertigmaß: 15 mm Putz ringsum
            assert!(q.area > 0.0 && (g.total.area - q.area).abs() < 1e-9);
            assert_eq!(g.total.volume, 0.0);
            // Wandseiten nach Himmelsrichtung, alle an der Außenwand
            let mut seiten: Vec<&str> = q
                .lines
                .iter()
                .filter(|l| l.name.starts_with("Wand ") && l.finish.is_some_and(|f| f.1 == 0))
                .map(|l| l.name.rsplit(' ').next().unwrap())
                .collect();
            seiten.sort();
            assert_eq!(seiten, ["Nord", "Ost", "Süd", "West"]);
            // Reihenfolge: Boden, Wände, Decke
            let rang: Vec<u8> = q
                .lines
                .iter()
                .map(|l| match l.side {
                    Side::Floor => 0,
                    Side::Ceiling => 2,
                    _ => 1,
                })
                .collect();
            assert!(rang.windows(2).all(|w| w[0] <= w[1]), "{rang:?}");
        }

        // Die Zeilen der Bauteile bleiben, die der Räume kommen dazu
        let zeilen = |s: &sk_model::qto::Schedule| -> Vec<LayerRow> {
            s.layer_rows(&m).into_iter().map(|(_, r)| r).collect()
        };
        let (z0, z1) = (zeilen(&ohne), zeilen(&mit));
        let bauteile: Vec<&LayerRow> = z1
            .iter()
            .filter(|r| r.category != Category::Space)
            .collect();
        assert_eq!(bauteile.len(), z0.len());
        assert!(z0.iter().zip(&bauteile).all(|(a, b)| a == *b));
        let raum: Vec<&LayerRow> = z1
            .iter()
            .filter(|r| r.category == Category::Space)
            .collect();
        assert!(raum.len() > 10, "{}", raum.len());
        let eg = b
            .storeys
            .iter()
            .find(|s| raumgruppe(s).is_some())
            .unwrap()
            .id;
        for r in &raum {
            let side = r.side.expect("Seite");
            assert!(r.finish.is_some() && r.trade.is_some(), "{r:?}");
            assert_eq!(r.face, r.area);
            assert!((r.volume - r.area * r.thickness).abs() < 1e-6);
            let kg = r.kg.expect("kg");
            let soll: &[u16] = match (side, r.storey == eg) {
                (Side::Floor, true) => &[324, 325],
                (Side::Floor, false) => &[353],
                (Side::Ceiling, _) => &[354],
                _ => &[336],
            };
            assert!(soll.contains(&kg), "{side:?} {kg}");
            // Rechenweg aus dem Raumbuch: die Terme ergeben die Fläche
            let w = rechenweg::weg(&m, r, Bezug::Flaeche, Modus::Vob).unwrap();
            assert_eq!(w.terme, r.terms);
            assert!((w.menge() - r.area).abs() < 1e-6);
            // Volumen Fläche × Dicke, ohne Dicke keins
            let v = rechenweg::weg(&m, r, Bezug::Volumen, Modus::Vob);
            assert_eq!(v.is_some(), r.thickness > 0.0);
        }
        // Wandschichten der Bauteile: vor dem Kern außen, dahinter innen
        assert!(bauteile.iter().any(|r| r.side == Some(Side::Outer)));
        assert!(bauteile
            .iter()
            .filter(|r| r.side.is_some())
            .all(|r| matches!(r.side, Some(Side::Outer | Side::Inner))));

        // Längen als Automatikmengen am Raum
        let auto: Vec<_> = mit
            .auto
            .iter()
            .filter(|a| a.key.starts_with("room."))
            .collect();
        let keys: Vec<&str> = auto.iter().map(|a| a.key).collect();
        assert_eq!(
            keys,
            [
                "room.skirting",
                "room.edgestrip",
                "room.skirting",
                "room.edgestrip"
            ]
        );
        for a in &auto {
            assert_eq!(a.unit, "m");
            assert!(a.value > 30_000.0, "{}", a.value);
            assert_eq!(a.kg, Some(if a.storey == eg { 324 } else { 353 }));
            assert!(a.formula.ends_with(" m"), "{}", a.formula);
        }
        // Nach Gewerk: Parkett, Putz, Estrich mit Fläche
        assert!(b
            .by_trade
            .iter()
            .any(|t| t.rows.iter().any(|r| r.category == Category::Space)));
    }

    /// Richtung der Wandseite nach Nord des Projekts.
    #[test]
    fn himmelsrichtung() {
        let mut m = haus();
        let w = m.elements().iter().next().unwrap().0;
        let k = |a: (f64, f64), b: (f64, f64)| Kante {
            a: sk_math::vec3(a.0, a.1, 0.0),
            b: sk_math::vec3(b.0, b.1, 0.0),
            quelle: Begrenzung::Wand {
                wand: w,
                schicht: 0,
                seite: crate::Seite::Links,
                von: 0.0,
                bis: 1.0,
            },
            ausbau_typ: Vec::new(),
        };
        // Umriss gegen den Uhrzeigersinn: unten Süd, rechts Ost
        assert_eq!(richtung(&m, &k((0.0, 0.0), (1.0, 0.0))), "Süd");
        assert_eq!(richtung(&m, &k((1.0, 0.0), (1.0, 1.0))), "Ost");
        assert_eq!(richtung(&m, &k((1.0, 1.0), (0.0, 1.0))), "Nord");
        assert_eq!(richtung(&m, &k((0.0, 1.0), (0.0, 0.0))), "West");
        assert_eq!(richtung(&m, &k((0.0, 0.0), (1.0, 1.0))), "Südost");
        // Nord um 90° im Uhrzeigersinn gedreht: +x ist Nord
        let mut l = *m.location();
        l.north = Some(90.0);
        m.set_location(l);
        assert_eq!(richtung(&m, &k((1.0, 0.0), (1.0, 1.0))), "Nord");
        assert_eq!(richtung(&m, &k((0.0, 0.0), (1.0, 0.0))), "Ost");
    }
}

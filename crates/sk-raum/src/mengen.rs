//! Raumbuch in der Mengenliste (RB5, `raumbuch/plan.md` §6.2, mit Kosten
//! abgestimmt): je gespeichertem Raum die Zeilen von
//! [`crate::raumbuch::rechnen`] als Zeilen der Mengenliste
//! ([`sk_model::qto_raum`]) mit Namen, Seite, Gewerk und Kostengruppe nach
//! dem Träger (DIN 276:2018):
//! - Boden, Estrich, Sockelleiste, Randdämmstreifen: auf der Sohlplatte 324
//!   (die Abdichtung der Sohlplatte unter dem Estrich 325), auf einer Decke
//!   353; die Verbundabdichtung unter Fliesen (DIN 18534) gehört zum Belag,
//!   auf der Sohle 324, auf der Decke 353 (Review H-197j);
//! - Wandseite, ihre Laibungen und Fliesen: an der Außenwand 336, sonst 345;
//! - Decke 354;
//! - Eckschienen, Silikon und Dichtband gehören zu keiner einzelnen
//!   Wandseite und zählen in 345 (Ecken liegen meist an Innenwänden;
//!   Nachtregel, änderbar).
//!
//! Die App gibt [`raeume_mit`] an [`sk_model::qto::schedule_mit`], mit den
//! Räumen aus ihrer Erkennung und den Wandzügen und Platten der Szene.

use crate::ausbau::RaumAusbau;
use crate::erkennung::{raeume as formen, Begrenzung, Kante, RaumForm};
use crate::oeffnungen::Rohbau;
use crate::raumbuch::{rechnen, Flaeche, Regeln, Zeile};
use crate::raumliste::{fertigmass, traeger, Platten};
use crate::Ketten;
use sk_model::qto::{RoomLine, RoomQto, Side};
use sk_model::rechenweg::Rechenweg;
use sk_model::trade::TradeId;
use sk_model::{Category, ElementId, LayerFunction, LayerSetId, MaterialLayer, Model, StoreyId};

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
/// Raumbuch, wie sie waren. Baut Platten und Ketten je Aufruf; für eine
/// ganze Mengenliste [`raeume_mit`] (Review #170 H-170b).
pub fn raeume(model: &Model, geschoss: StoreyId) -> Vec<(ElementId, RoomQto)> {
    let platten = Platten::neu(model, model);
    raeume_mit(model, model, &platten, formen(model, geschoss))
}

/// Wie [`raeume`] für die Räume `formen` (in der App aus der Erkennung der
/// Szene), die Wandzüge aus `k` und Decken und Sohlplatten aus `platten`
/// (einmal je Mengenliste), die Höhen der Geschosse ebenfalls aus
/// `platten` (Review #137 B2). So wächst die Mengenliste mit den Räumen, nicht mit ihrem
/// Quadrat (Review #112 H8).
pub fn raeume_mit(
    model: &Model,
    k: &(impl Ketten + ?Sized),
    platten: &Platten,
    formen: impl IntoIterator<Item = RaumForm>,
) -> Vec<(ElementId, RoomQto)> {
    formen
        .into_iter()
        .filter(|f| f.raum.is_some())
        .filter_map(|f| {
            let roh = Rohbau::mit_geschossen(model, k, f, &platten.geschosse);
            raum(model, platten, roh)
        })
        .collect()
}

fn raum(model: &Model, platten: &Platten, roh: Rohbau) -> Option<(ElementId, RoomQto)> {
    let id = roh.form.raum?;
    let aus = RaumAusbau::fuer_in(model, &roh, &platten.geschosse)?;
    // Ohne Ausbautypen im Projekt fehlt der Raum, auch wenn der Putz der
    // Wandtypen Eckschienen ergäbe (wie die Grundfläche, Review #182 B1)
    if !aus.mit_typen() {
        return None;
    }
    let (_, m, _) = fertigmass(model, &roh, &aus);
    let kanten: Vec<&Kante> = roh.kanten().collect();
    // Unter dem Boden: Sohlplatte oder Decke, einmal je Raum
    let unten = traeger(model, platten, &roh, &kanten, Flaeche::Boden)
        .and_then(|id| model.element(id))
        .map(|e| e.category);
    let zeilen = rechnen(model, &roh, &aus, &Regeln::default());
    let gewerke: Vec<Option<TradeId>> = zeilen
        .iter()
        .map(|z| gewerk_der_laenge(model, &zeilen, z.flaeche))
        .collect();
    let mut lines: Vec<RoomLine> = zeilen
        .into_iter()
        .zip(gewerke)
        .map(|(z, t)| zeile(model, unten, &kanten, z, t))
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

/// Gewerk einer Schicht: das der Schicht, sonst das ihres Baustoffs.
fn gewerk(model: &Model, typ: Option<(LayerSetId, usize)>) -> Option<TradeId> {
    let l = typ.and_then(|(t, j)| model.layer_set(t)?.layers.get(j).copied())?;
    l.trade.or_else(|| model.material(l.material)?.trade)
}

/// Gewerk einer Länge ohne eigenen Typ, für ihre Zeile im Mengenblatt nach
/// Gewerk (Bedienbarkeit 43.10, mit Kosten abgestimmt): das der Fläche, an
/// der sie liegt. Randdämmstreifen zum Estrich, Eckschienen zum Putz der
/// Wandseiten (Schicht mit Dicke), Silikonfugen zu den Wandfliesen. Die
/// Sockelleiste hat ihren Typ und dessen Gewerk (ab Werk das des Belags).
fn gewerk_der_laenge(model: &Model, zeilen: &[Zeile], f: Flaeche) -> Option<TradeId> {
    // nur für die drei Längen über die Zeilen, so bleibt der Raum linear
    // in seinen Zeilen (Review #182 H-182f)
    if !matches!(
        f,
        Flaeche::Randstreifen | Flaeche::Eckschienen | Flaeche::Silikon
    ) {
        return None;
    }
    let an = |z: &Zeile| match f {
        Flaeche::Randstreifen => z.flaeche == Flaeche::Estrich,
        Flaeche::Eckschienen => matches!(z.flaeche, Flaeche::Wand(_)) && z.dicke > 0.0,
        Flaeche::Silikon => matches!(z.flaeche, Flaeche::Fliesen(_)),
        _ => false,
    };
    zeilen
        .iter()
        .filter(|z| an(z))
        .find_map(|z| gewerk(model, z.typ))
}

/// Zeile der Mengenliste aus der Zeile `z` des Raumbuchs; `laenge` ist das
/// Gewerk einer Länge ohne eigenen Typ ([`gewerk_der_laenge`]).
fn zeile(
    model: &Model,
    unten: Option<Category>,
    kanten: &[&Kante],
    z: Zeile,
    laenge: Option<TradeId>,
) -> RoomLine {
    let schicht = z
        .typ
        .and_then(|(t, j)| model.layer_set(t)?.layers.get(j).copied());
    let material = schicht.map(|l| l.material);
    let trade = gewerk(model, z.typ).or(laenge);
    let side = match z.flaeche {
        Flaeche::Boden | Flaeche::Estrich | Flaeche::Sockel | Flaeche::Randstreifen => Side::Floor,
        Flaeche::Decke => Side::Ceiling,
        _ => Side::Wall,
    };
    let kg = kostengruppe(model, unten, kanten, z.flaeche, schicht);
    RoomLine {
        name: name(model, kanten, z.flaeche),
        flaeche: z.flaeche.kennung(),
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

/// Kostengruppe einer Zeile nach dem Träger (Modulkopf, plan.md §10);
/// `unten` ist das Bauteil unter dem Boden, `schicht` die Schicht der Zeile.
/// Die Raumliste für den Austausch nimmt dieselbe Regel.
pub(crate) fn kostengruppe(
    model: &Model,
    unten: Option<Category>,
    kanten: &[&Kante],
    flaeche: Flaeche,
    schicht: Option<MaterialLayer>,
) -> u16 {
    match flaeche {
        Flaeche::Decke => KG_DECKENBEKLEIDUNG,
        Flaeche::Wand(k)
        | Flaeche::Laibung(k)
        | Flaeche::Fliesen(k)
        | Flaeche::FliesenLaibung(k) => wand_kg(model, kanten, k),
        Flaeche::Eckschienen | Flaeche::Silikon | Flaeche::Dichtband => KG_INNENWAND,
        Flaeche::Boden | Flaeche::Estrich | Flaeche::Sockel | Flaeche::Randstreifen => {
            match unten {
                // Nur die Abdichtung im Estrichaufbau dichtet die Sohle; die
                // Verbundabdichtung im Belag liegt auf dem Estrich (H-197j)
                Some(Category::GroundSlab)
                    if flaeche == Flaeche::Estrich
                        && schicht.is_some_and(|l| l.function == LayerFunction::Membrane) =>
                {
                    KG_SOHLE_ABDICHTUNG
                }
                Some(Category::GroundSlab) => KG_SOHLE,
                _ => KG_DECKENBELAG,
            }
        }
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

/// „AW-004 Nord“: Nummer der Wand und Himmelsrichtung, in die die
/// Wandseite der Kante `k` vom Raum aus liegt (auch für das Paneel des
/// Raums, RB6).
pub fn seite(model: &Model, kanten: &[&Kante], k: usize) -> String {
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
        Flaeche::Boden => "Boden (VOB)".into(),
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
        // Gewerk der Längen: Sockelleiste nach ihrem Typ (Eiche wie das
        // Parkett), Randdämmstreifen wie der Estrich (43.10)
        for g in &gruppen {
            let Some(ElementQto::Room(q)) = &g.rows[0].q else {
                continue;
            };
            for l in q.lines.iter().filter(|l| l.is_auto()) {
                let code = l.trade.and_then(|t| m.trade(t)).map(|t| t.code.as_str());
                let soll = match l.key {
                    Some("room.skirting") => "18356",
                    _ => "18353",
                };
                assert_eq!(code, Some(soll), "{}", l.name);
            }
        }
        // Nach Gewerk: Parkett, Putz, Estrich mit Fläche
        assert!(b
            .by_trade
            .iter()
            .any(|t| t.rows.iter().any(|r| r.category == Category::Space)));
    }

    /// Die Raumliste für den Austausch (IFC I6) trägt je Schicht dieselbe
    /// Kostengruppe wie die Zeile des Mengenblatts: auf der Sohlplatte 324
    /// und 325, auf der Decke 353, an der Außenwand 336, Decke 354.
    #[test]
    fn raumliste_mit_kostengruppen_des_mengenblatts() {
        let m = haus();
        let liste = crate::raumliste::raumliste(&m);
        assert_eq!(liste.len(), 2);
        let mut kgs = std::collections::BTreeSet::new();
        for r in &liste {
            let zeilen: Vec<RoomLine> = raeume(&m, r.geschoss)
                .into_iter()
                .find(|(id, _)| *id == r.raum)
                .expect("Raum im Mengenblatt")
                .1
                .lines;
            for a in &r.ausbau {
                for s in a.schichten.iter().filter(|s| s.menge > 0.0) {
                    let f = Some((a.typ, s.schicht));
                    let z: Vec<_> = zeilen.iter().filter(|z| z.finish == f).collect();
                    assert!(!z.is_empty(), "{:?} {}", a.flaeche, s.schicht);
                    assert!(
                        z.iter().any(|z| z.kg == Some(s.kg)),
                        "{:?} {} {}",
                        a.flaeche,
                        s.schicht,
                        s.kg
                    );
                }
                for s in &a.schichten {
                    kgs.insert(s.kg);
                }
            }
        }
        assert_eq!(
            kgs.into_iter().collect::<Vec<_>>(),
            [324, 325, 336, 353, 354]
        );
    }

    /// Review H-197j: Die Verbundabdichtung unter den Fliesen des Bads
    /// gehört zum Belag, auf der Sohle 324, auf der Decke 353; nur die
    /// Abdichtung der Sohlplatte im Estrichaufbau zählt in 325.
    #[test]
    fn verbundabdichtung_zum_belag() {
        use sk_model::model::{BATH_SPACE_GUID, SLAB_SEAL_GUID, TILE_SEAL_GUID};
        let mut m = haus();
        m.begin("Bäder");
        assert!(m.ensure_space_type(BATH_SPACE_GUID).is_some());
        let geschosse: Vec<StoreyId> = m.storeys().iter().map(|(g, _)| g).collect();
        for &g in &geschosse {
            for id in m.spaces(g) {
                assert!(m.set_space(id, |s| s.nutzung = Some(BATH_SPACE_GUID)));
                assert!(crate::ausbau::typen_anlegen(&mut m, id));
            }
        }
        m.commit();
        let mat = |g| {
            m.materials()
                .iter()
                .find(|(_, x)| x.guid == g)
                .map(|(id, _)| id)
                .unwrap()
        };
        let (verbund, sohle) = (mat(TILE_SEAL_GUID), mat(SLAB_SEAL_GUID));
        let mut gesehen = Vec::new();
        for &g in &geschosse {
            for (_, q) in raeume(&m, g) {
                for l in q.lines.iter().filter(|l| l.side == Side::Floor) {
                    let auf_sohle = l.kg.is_some_and(|k| k < 350);
                    if l.material == Some(verbund) {
                        assert_eq!(l.kg, Some(if auf_sohle { 324 } else { 353 }), "{}", l.name);
                        gesehen.push(("verbund", l.kg));
                    } else if l.material == Some(sohle) {
                        assert_eq!(l.kg, Some(325), "{}", l.name);
                        gesehen.push(("sohle", l.kg));
                    }
                }
            }
        }
        gesehen.sort();
        gesehen.dedup();
        assert_eq!(
            gesehen,
            [
                ("sohle", Some(325)),
                ("verbund", Some(324)),
                ("verbund", Some(353))
            ]
        );
    }

    /// Bedienbarkeit 43.8: Die Grundfläche des Grundrisses
    /// (`flaechen::grundflaeche_fertig`) ist die Fläche des Raums im
    /// Mengenblatt, auch im Bad mit Wandfliesen ab Boden, wo der Putz der
    /// Vorgabe allein eine größere Fläche ergäbe; ohne Ausbau bleibt sie die
    /// Grundfläche mit dem Putz der Vorgabe.
    #[test]
    fn grundflaeche_wie_im_mengenblatt() {
        use crate::flaechen::{grundflaeche, grundflaeche_fertig};
        use sk_model::model::BATH_SPACE_GUID;
        let gleich = |m: &Model, bad: bool, putz: f64| {
            for (g, _) in m.storeys().iter() {
                let mengen = raeume(m, g);
                for f in formen(m, g) {
                    let id = f.raum.unwrap();
                    let q = &mengen.iter().find(|x| x.0 == id).unwrap().1;
                    let ist = grundflaeche_fertig(m, &f);
                    assert!((ist - q.area).abs() < 1e-6, "{ist} {}", q.area);
                    let ist_bad = m.space(id).unwrap().nutzung == Some(BATH_SPACE_GUID);
                    if bad && ist_bad {
                        // Putz 15 mm und Fliese 10 mm statt Putz 15 mm
                        assert!(grundflaeche(&f) - ist > 0.0, "Fliese abgezogen");
                    } else {
                        let soll = f.flaeche - f.umfang * putz + 4.0 * putz * putz;
                        assert!((ist - soll).abs() < 1.0, "Putz {putz} mm: {ist} {soll}");
                    }
                }
            }
        };
        let mut m = haus();
        gleich(&m, false, 15.0);
        let g = m.storeys().iter().next().unwrap().0;
        let id = m.spaces(g)[0];
        m.begin("Bad");
        assert!(m.ensure_space_type(BATH_SPACE_GUID).is_some());
        assert!(m.set_space(id, |s| s.nutzung = Some(BATH_SPACE_GUID)));
        assert!(crate::ausbau::typen_anlegen(&mut m, id));
        m.commit();
        gleich(&m, true, 15.0);
        // Vorlage ohne Wandbekleidung, Boden im Projekt: Das Mengenblatt
        // zeigt den Raum mit der Rohbaufläche, die Grundfläche ebenso
        // (Review #182 B1)
        let mut m = haus();
        m.begin("Ohne Wand");
        let mut w = m
            .space_type(sk_model::model::DEFAULT_SPACE_GUID)
            .unwrap()
            .clone();
        (w.wall, w.tiles) = (None, None);
        assert!(m.put_space_type(w));
        m.commit();
        gleich(&m, false, 0.0);
        // Ohne Ausbau im Projekt: wie bisher mit dem Putz der Vorgabe, und
        // die Raumliste des IFC-Exports ebenso (Review #182 H1)
        let m = crate::raumliste::tests::haus_roh();
        let liste = crate::raumliste::raumliste(&m);
        assert_eq!(liste.len(), 2);
        for (g, _) in m.storeys().iter() {
            for f in formen(&m, g) {
                let ist = grundflaeche_fertig(&m, &f);
                assert!((ist - grundflaeche(&f)).abs() < 1e-6, "{ist}");
                let r = liste.iter().find(|r| Some(r.raum) == f.raum).unwrap();
                assert!((r.mengen.flaeche_fertig - ist).abs() < 1e-6);
                assert!(r.mengen.flaeche_fertig < r.mengen.flaeche_roh - 1e5);
            }
        }
    }

    /// Review #182 B1: Eine Wandzunge teilt die Seite der Außenwand in zwei
    /// Wandseiten gleichen Namens. In die Gewerksumme geht jede Fläche je
    /// Gewerk einmal, beide Wandseiten also ganz; nur die Schichten unter
    /// einer Fläche (Estrich samt Dämmung) zählen einmal.
    #[test]
    fn gewerksumme_mit_wandzunge() {
        use sk_math::vec3;
        let mut m = haus();
        let eg = m
            .levels_in(m.buildings().iter().next().map(|b| b.0))
            .into_iter()
            .find(|g| m.storey(*g).unwrap().kind == sk_model::LevelKind::Storey)
            .unwrap();
        let iw = m.defaults().interior_wall;
        let aw = m.defaults().exterior_wall;
        let t: f64 = m.wall_layers(aw).iter().map(|l| l.thickness).sum();
        let p = [vec3(5000.0, t, 0.0), vec3(5000.0, 2000.0, 0.0)];
        m.begin("Zunge");
        m.add_wall_run(
            &p,
            false,
            sk_model::RefSide::Center,
            eg,
            iw,
            Category::InteriorWall,
        )
        .unwrap();
        crate::abgleichen(&mut m, &[eg]);
        m.commit();
        let s = schedule_mit(&m, &raeume);
        let st = s.buildings[0].storeys.iter().find(|x| x.id == eg).unwrap();
        let r = st
            .groups
            .iter()
            .find(|g| g.category == Category::Space)
            .map(|g| &g.rows[0])
            .unwrap();
        let Some(ElementQto::Room(q)) = &r.q else {
            panic!("Raum ohne Mengen");
        };
        // zwei Wandseiten gleichen Namens, verschiedene Flächen
        let sued: Vec<&RoomLine> = q
            .lines
            .iter()
            .filter(|l| l.name == "Wand AW-004 Süd" && l.finish.is_some_and(|f| f.1 == 1))
            .collect();
        assert_eq!(
            sued.len(),
            2,
            "{:?}",
            q.lines.iter().map(|l| &l.name).collect::<Vec<_>>()
        );
        assert_eq!(sued[0].name, sued[1].name);
        assert_ne!(sued[0].flaeche, sued[1].flaeche);
        // je Gewerk: Summe nach VOB = je Fläche einmal ihre Menge
        let zeilen = sk_model::qto_raum::layer_rows(&m, r, eg, q);
        let mut gewerke: Vec<_> = zeilen.iter().map(|z| z.trade).collect();
        gewerke.dedup();
        for t in gewerke {
            let summe: f64 = zeilen
                .iter()
                .filter(|z| z.trade == t)
                .map(|z| z.bill_area)
                .sum();
            let mut je: Vec<(u32, f64)> = Vec::new();
            for z in zeilen.iter().filter(|z| z.trade == t) {
                let f = q.lines[z.layer].flaeche;
                if !je.iter().any(|x| x.0 == f) {
                    je.push((f, z.area));
                }
            }
            let soll: f64 = je.iter().map(|x| x.1).sum();
            assert!((summe - soll).abs() < 1e-6, "{t:?}: {summe} {soll}");
        }
        // Putz: jede Wandseite zählt, auch die zweite mit gleichem Namen
        let putz = |z: &&LayerRow| z.side == Some(Side::Wall) && z.thickness > 0.0;
        let a: f64 = zeilen.iter().filter(putz).map(|z| z.area).sum();
        let b: f64 = zeilen.iter().filter(putz).map(|z| z.bill_area).sum();
        assert!(a > 0.0 && (a - b).abs() < 1e-6, "{a} {b}");
    }

    /// Gewerk einer Länge ohne Typ nach der Fläche, an der sie liegt
    /// (43.10): Randdämmstreifen wie der Estrich, Eckschienen wie der Putz
    /// (nicht der Anstrich), Silikonfugen wie die Wandfliesen; ohne diese
    /// Fläche keins.
    #[test]
    fn gewerk_der_laengen() {
        use sk_model::model::{LIME_WALL_TYPE_GUID, SCREED_FLOOR_TYPE_GUID, WALL_TILES_TYPE_GUID};
        let mut m = haus();
        m.begin("Typen");
        let mut typ = |g| m.ensure_finish_type(g).unwrap();
        let (estrich, wand, fliesen) = (
            typ(SCREED_FLOOR_TYPE_GUID),
            typ(LIME_WALL_TYPE_GUID),
            typ(WALL_TILES_TYPE_GUID),
        );
        m.commit();
        let z = |flaeche, typ, dicke| Zeile {
            flaeche,
            typ: Some(typ),
            dicke,
            schluessel: None,
            bezug: Bezug::Flaeche,
            terme: Vec::new(),
        };
        let zeilen = [
            z(Flaeche::Wand(0), (wand, 0), 0.0),
            z(Flaeche::Wand(0), (wand, 1), 15.0),
            z(Flaeche::Estrich, (estrich, 0), 65.0),
            z(Flaeche::Fliesen(1), (fliesen, 0), 10.0),
        ];
        let code = |z: &[Zeile], f| {
            gewerk_der_laenge(&m, z, f)
                .and_then(|t| m.trade(t))
                .map(|t| t.code.clone())
        };
        assert_eq!(
            code(&zeilen, Flaeche::Randstreifen).as_deref(),
            Some("18353")
        );
        assert_eq!(
            code(&zeilen, Flaeche::Eckschienen).as_deref(),
            Some("18350")
        );
        assert_eq!(code(&zeilen, Flaeche::Silikon).as_deref(), Some("18352"));
        assert_eq!(
            code(&zeilen[..1], Flaeche::Eckschienen),
            None,
            "nur Anstrich"
        );
        assert_eq!(code(&zeilen[..2], Flaeche::Randstreifen), None);
        assert_eq!(
            code(&zeilen, Flaeche::Sockel),
            None,
            "die Sockelleiste hat ihren Typ"
        );
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

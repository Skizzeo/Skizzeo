//! Öffnungen in der Abrechnung nach VOB/C und in der Bestellmenge
//! (vob/oeffnung-vorgabe.md §1 und §2, Jörn 11.10. 16:59).

use crate::element::RunId;
use crate::model::Model;
use crate::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
use crate::qto::{schedule, LayerRow};
use crate::rechenweg::{anzeige, fuer, Art, Bezug, Modus};
use crate::wall::right_of;
use sk_math::{vec3, Vec3};

/// Prüfhaus 10 × 8 m, zwei Geschosse, WDVS.
fn haus() -> (Model, RunId) {
    let mut m = Model::with_seed(311);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.build_from_polygon(b, &pts).unwrap();
    (m, eg)
}

fn aussen(m: &Model, run: RunId, seg: usize, u: f64) -> Vec3 {
    let c = m.chain(run).unwrap();
    let (pts, _, dirs) = c.layout().unwrap();
    pts[seg] + dirs[seg] * u + right_of(dirs[seg]) * c.outer_offset()
}

/// Fenster in Segment 0 von `u.0` bis `u.1` (mm), Brüstung und Sturz nach
/// den Höhenlinien.
fn fenster(m: &Model, run: RunId, u: (f64, f64)) -> Opening {
    let st = m.run(run).unwrap().storey;
    Opening {
        kind: OpeningKind::Window,
        path: vec![aussen(m, run, 0, u.0), aussen(m, run, 0, u.1)],
        bottom: HeightRef::Line(st, HeightLine::Sill),
        top: HeightRef::Line(st, HeightLine::Lintel),
        reveal: None,
        install: Default::default(),
    }
}

/// Wandschichtzeilen mit Löchern.
fn mit_loch(m: &Model) -> Vec<LayerRow> {
    schedule(m)
        .layer_rows(m)
        .into_iter()
        .map(|(_, r)| r)
        .filter(|r| !r.holes.is_empty())
        .collect()
}

fn summe_stimmt(m: &Model, r: &LayerRow) {
    for modus in Modus::ALL {
        for w in fuer(m, r, modus) {
            let soll = anzeige(m, r, w.bezug, modus);
            assert!(
                (w.menge() - soll).abs() < 1.0,
                "{} {modus:?} {:?}: {} ≠ {soll}",
                r.number,
                w.bezug,
                w.menge()
            );
        }
    }
}

/// Ein Fenster 1,25 × 1,51 m bleibt übermessen: Abrechnungsfläche und
/// Volumen (je Schicht unter 0,5 m³, Kosten H79-1) ändern sich nicht, der
/// Rechenweg nennt es.
#[test]
fn kleines_fenster_uebermessen() {
    let (mut m, eg) = haus();
    let mengen = |m: &Model| -> Vec<(f64, f64)> {
        schedule(m)
            .layer_rows(m)
            .into_iter()
            .map(|(_, r)| (r.face, r.volume))
            .collect()
    };
    let vorher = mengen(&m);
    m.add_opening(fenster(&m, eg, (3000.0, 4250.0))).unwrap();
    let nachher = mengen(&m);
    assert_eq!(vorher.len(), nachher.len());
    for (a, b) in vorher.iter().zip(&nachher) {
        assert!((a.0 - b.0).abs() < 1e-3, "{a:?} → {b:?}");
        assert!((a.1 - b.1).abs() < 1.0, "{a:?} → {b:?}");
    }
    let rows = mit_loch(&m);
    assert!(!rows.is_empty());
    for r in &rows {
        assert!(r.holes.iter().all(|h| !h.vob()));
        summe_stimmt(&m, r);
        let f = fuer(&m, r, Modus::Vob);
        let fl = f.iter().find(|w| w.bezug == Bezug::Flaeche).unwrap();
        assert!(
            fl.terme
                .iter()
                .any(|t| t.art == Art::Hinweis && t.text.contains("übermessen")),
            "{:?}",
            fl.terme
        );
        let vol = f.iter().find(|w| w.bezug == Bezug::Volumen).unwrap();
        assert!(vol
            .terme
            .iter()
            .any(|t| t.art == Art::Hinweis && t.text.contains("bis 0,5 m³")));
    }
}

/// Ein Fenster 3,00 × 1,51 m (> 2,5 m²) geht in jeder Schicht von der
/// Abrechnungsfläche ab, mit seinen Maßen im Rechenweg. Die Bestellmenge
/// übermisst es (ohne „Öffnungen abziehen ab“) und schlägt den Verschnitt
/// auf; mit „ab 2 m²“ zieht sie es ab.
#[test]
fn grosses_fenster_abgezogen() {
    let (mut m, eg) = haus();
    let vorher = schedule(&m).layer_rows(&m);
    m.add_opening(fenster(&m, eg, (3000.0, 6000.0))).unwrap();
    let rows = mit_loch(&m);
    assert!(!rows.is_empty());
    let mut arten = (0, 0);
    for r in &rows {
        let (_, alt) = vorher
            .iter()
            .find(|(_, x)| x.element == r.element && x.layer == r.layer)
            .unwrap();
        let loch: f64 = r.holes.iter().map(|h| h.area).sum();
        assert!(r.holes.iter().all(|h| h.vob()));
        assert!(
            (alt.face - loch - r.face).abs() < 1.0,
            "{} Schicht {}: {} − {loch} ≠ {}",
            r.number,
            r.layer,
            alt.face,
            r.face
        );
        summe_stimmt(&m, r);
        let fl = fuer(&m, r, Modus::Vob)
            .into_iter()
            .find(|w| w.bezug == Bezug::Flaeche)
            .unwrap();
        assert!(
            fl.terme
                .iter()
                .any(|t| t.art == Art::Abzug && t.text.contains("OE-") && t.text.contains(" × ")),
            "{:?}",
            fl.terme
        );
        // Volumen: Abzug je Schicht nur über 0,5 m³ (Kosten H79-1)
        let vol = fuer(&m, r, Modus::Vob)
            .into_iter()
            .find(|w| w.bezug == Bezug::Volumen)
            .unwrap();
        let loch_vol: f64 = r.holes.iter().map(|h| h.volume).sum();
        let gross = r.holes.iter().all(|h| h.vob_volumen());
        let art = if gross { Art::Abzug } else { Art::Hinweis };
        assert!(
            vol.terme
                .iter()
                .any(|t| t.art == art && t.text.contains("OE-")),
            "{} Schicht {}: {:?}",
            r.number,
            r.layer,
            vol.terme
        );
        if gross {
            arten.0 += 1;
        } else {
            arten.1 += 1;
        }
        let soll = if gross {
            alt.volume - loch_vol
        } else {
            alt.volume
        };
        assert!((r.volume - soll).abs() < 1.0, "{} ≠ {soll}", r.volume);
        // Bestellung: übermessen, dann der Zuschlag
        let z = 1.0 + crate::rechenweg::zuschlag(&m, r) / 100.0;
        let best = anzeige(&m, r, Bezug::Flaeche, Modus::Bestellung);
        assert!((best - alt.face * z).abs() < 1.0, "{best} {}", alt.face * z);
    }
    // beide Schichten (Kern, Dämmung) über 0,5 m³; das kleine Fenster
    // deckt das Übermessen ab
    assert_eq!(arten, (rows.len(), 0));
    // „Öffnungen abziehen ab 2 m²“ am Baustoff der ersten Zeile
    let r = &rows[0];
    let mut mat = m.material(r.material).unwrap().clone();
    mat.props.insert(
        crate::matprop::OPENING_FROM.into(),
        crate::element::PropValue::Number(2.0),
    );
    m.begin("ab");
    assert!(m.set_material(r.material, mat));
    m.commit();
    let r = mit_loch(&m)
        .into_iter()
        .find(|x| x.element == r.element && x.layer == r.layer)
        .unwrap();
    let z = 1.0 + crate::rechenweg::zuschlag(&m, &r) / 100.0;
    let best = anzeige(&m, &r, Bezug::Flaeche, Modus::Bestellung);
    assert!((best - r.face * z).abs() < 1.0, "{best} {}", r.face * z);
    summe_stimmt(&m, &r);
}

/// Laibungen als Automatikmengen: WDVS und innen links, rechts und
/// oben für jede Öffnung, auch die übermessene; vier Ecken Diagonal-
/// bewehrung am Fenster.
#[test]
fn laibungen_je_oeffnung() {
    use crate::qto_laibung::{INNER, MESH, WDVS};
    let (mut m, eg) = haus();
    let id = m.add_opening(fenster(&m, eg, (3000.0, 4250.0))).unwrap();
    let s = schedule(&m);
    let auto: Vec<_> = s.auto.iter().filter(|a| a.element == id).collect();
    let wert = |k: &str| auto.iter().find(|a| a.key == k).map(|a| a.value);
    let q = crate::qto::run_qto(&m, eg);
    let h = &q[0].layers[0].holes[0];
    assert_eq!(wert(WDVS), Some(2.0 * h.height + h.width), "{auto:?}");
    assert_eq!(wert(MESH), Some(4.0));
    // Innenlaibung an der innersten Schicht (hier Porenbeton ohne Putz)
    let innen = q[0].layers.last().unwrap().holes.first().unwrap();
    assert_eq!(
        wert(INNER),
        Some(2.0 * innen.height + innen.width),
        "{auto:?}"
    );
    let f = auto.iter().find(|a| a.key == INNER).unwrap();
    assert!(f.formula.contains("Tiefe"), "{}", f.formula);
}

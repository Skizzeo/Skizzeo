use super::*;

use sk_model::{LayerSetId, Model, RunId};

fn typ(m: &Model, code: &str) -> LayerSetId {
    m.layer_sets()
        .iter()
        .find(|(_, t)| t.code == code)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("Typ {code} fehlt"))
}

fn auf_typ(s: &mut Scene, run: RunId, code: &str) {
    let id = typ(s.model(), code);
    assert!(
        s.edit_model("Wandtyp geändert", |m| m.set_run_type(run, id)),
        "Wechsel auf {code}"
    );
}

/// Haus B11 (10 × 8 m) mit AW-36,5 (Streifen 12,5 cm vor 24 cm Auflager).
fn haus() -> Scene {
    let mut s = Scene::with_model(Model::with_seed(127));
    let (aw, iw) = haus_b11(&mut s);
    auf_typ(&mut s, iw, "IW-17,5");
    auf_typ(&mut s, aw, "AW-36,5");
    s
}

/// Deckenhöhen, an denen Streifen liegen: UK/OK EG-Decke, UK OG-Decke.
const DECKE: [f32; 3] = [2635.0, 2855.0, 5490.0];

fn bei(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.0
}

/// Kanten, die ganz in einer Fassadenebene liegen und waagerecht auf einer
/// Deckenhöhe verlaufen.
fn fassadenkanten_in_deckenhoehe(m: &Shown) -> Vec<([[f32; 3]; 2], f32)> {
    let ebene = |p: &[[f32; 3]; 2]| {
        (bei(p[0][1], 0.0) && bei(p[1][1], 0.0))
            || (bei(p[0][1], 8000.0) && bei(p[1][1], 8000.0))
            || (bei(p[0][0], 0.0) && bei(p[1][0], 0.0))
            || (bei(p[0][0], 10000.0) && bei(p[1][0], 10000.0))
    };
    m.edges
        .iter()
        .filter(|(p, _)| ebene(p) && DECKE.iter().any(|&z| bei(p[0][2], z) && bei(p[1][2], z)))
        .cloned()
        .collect()
}

/// Senkrechte Kanten im Schnitt bei (x, y) decken z0…z1 lückenlos ab, alle
/// mit derselben Strichbreite.
fn durchgehend(c: &Shown, y: f32, x: f32, z0: f32, z1: f32) -> bool {
    let mut st: Vec<(f32, f32, f32)> = c
        .edges
        .iter()
        .filter(|(p, _)| bei(p[0][1], y) && bei(p[1][1], y) && bei(p[0][0], x) && bei(p[1][0], x))
        .map(|(p, w)| (p[0][2].min(p[1][2]), p[0][2].max(p[1][2]), *w))
        .filter(|&(a, b, _)| b > z0 && a < z1)
        .collect();
    st.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let Some(&(a, _, w)) = st.first() else {
        return false;
    };
    if a > z0 {
        return false;
    }
    let mut bis = z0;
    for &(a, b, v) in &st {
        if a > bis + 1.0 || v != w {
            return false;
        }
        bis = bis.max(b);
    }
    bis >= z1
}

/// A127: Bei AW-36,5 zeigt keine Ansicht (vorne, hinten, links, rechts, 3D)
/// eine Kante in Deckenhöhe auf der Fassade. Im Schnitt A–A liegen die
/// Streifen (x 0…125 und 9875…10 000, je EG- und OG-Decke) in Dämmschraffur
/// (Zickzack), ohne waagerechte Linie zur Wand darüber und darunter; die
/// Außenkontur der Wand läuft durch, die Kontur der Decke bleibt.
#[test]
fn a127_randstreifen_fugenlos() {
    let mut s = haus();
    assert_eq!(s.model().edge_strip_pairs().len(), 8, "8 Streifen");
    for v in [
        ViewKind::Front,
        ViewKind::Back,
        ViewKind::Left,
        ViewKind::Right,
        ViewKind::Persp,
    ] {
        let m = view_mesh(&mut s, v, None);
        let k = fassadenkanten_in_deckenhoehe(&m);
        assert!(k.is_empty(), "{v:?}: Kante in Deckenhöhe {k:?}");
    }

    let mut sect = SectionLine::default();
    sect.ensure(&s);
    let c = view_mesh(&mut s, ViewKind::Section, sect.plane());
    let y = 4000.0;
    let im_schnitt = |p: &[[f32; 3]; 2]| bei(p[0][1], y) && bei(p[1][1], y);
    for (x0, x1) in [(0.0f32, 125.0f32), (9875.0, 10000.0)] {
        // keine Trennlinie oben/unten im Streifenbereich
        let trenn: Vec<_> = c
            .edges
            .iter()
            .filter(|(p, _)| {
                im_schnitt(p)
                    && DECKE.iter().any(|&z| bei(p[0][2], z) && bei(p[1][2], z))
                    && p[0][0].min(p[1][0]) > x0 - 1.0
                    && p[0][0].max(p[1][0]) < x1 + 1.0
            })
            .collect();
        assert!(
            trenn.is_empty(),
            "Trennlinie am Streifen {x0}…{x1}: {trenn:?}"
        );
        // Streifenflächen in Dämmschraffur, EG und OG
        for (z0, z1) in [(2635.0f32, 2855.0f32), (5490.0, 5710.0)] {
            let tri: Vec<f32> = c
                .faces
                .chunks(3)
                .filter(|t| {
                    let cx = (t[0][0] + t[1][0] + t[2][0]) / 3.0;
                    let cy = (t[0][1] + t[1][1] + t[2][1]) / 3.0;
                    let cz = (t[0][2] + t[1][2] + t[2][2]) / 3.0;
                    bei(cy, y) && cx > x0 && cx < x1 && cz > z0 && cz < z1
                })
                .map(|t| t[0][9])
                .collect();
            assert!(!tri.is_empty(), "Streifen {x0}…{x1}, {z0}…{z1} im Schnitt");
            assert!(
                tri.iter().all(|&p| p == pattern::ZIGZAG),
                "Dämmschraffur: {tri:?}"
            );
        }
        // Außenkontur läuft über die Deckenhöhe hinweg durch: Teilstücke
        // stoßen lückenlos aneinander und haben dieselbe Strichbreite
        let aussen = if x0 == 0.0 { 0.0 } else { 10000.0 };
        assert!(
            durchgehend(&c, y, aussen, 2400.0, 3100.0),
            "Außenkontur bei x = {aussen} ohne Lücke und Breitenwechsel"
        );
        // Kontur der Decke an der Innenseite des Streifens bleibt
        let innen = if x0 == 0.0 { 125.0 } else { 9875.0 };
        assert!(
            c.edges.iter().any(|(p, _)| {
                im_schnitt(p)
                    && bei(p[0][0], innen)
                    && bei(p[1][0], innen)
                    && p[0][2].min(p[1][2]) < 2635.0 + 1.0
                    && p[0][2].max(p[1][2]) > 2855.0 - 1.0
            }),
            "Deckenkontur bei x = {innen}"
        );
    }
}

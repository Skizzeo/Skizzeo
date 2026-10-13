//! Paneel „Eigenschaften“ einer Öffnung (Grundriss G7): Art, Breite und
//! Höhe im Rohbau, bei einer Tür der Anschlag („DIN links“) und darunter
//! der Raum, in den sie aufschlägt („öffnet in Küche“), und die Wand.
//! Geändert wird am Grundriss (Griffe, Höhenlinien), das Paneel zeigt nur.
//! Dazu Löcher und Aufschlag für den Umriss der Auswahl.

use super::de_m;
use crate::scene::Scene;
use crate::ui::Props;
use sk_math::Vec3;
use sk_model::ElementId;

/// So weit liegt ein Loch höchstens neben dem Pfad seiner Öffnung (mm,
/// eine Wanddicke): Vortest über die Quader der Wandzüge.
const DICKE: f64 = 1000.0;

/// Wandzüge mit einem Loch der Öffnung `id`.
fn zuege<'a>(
    scene: &'a Scene,
    id: ElementId,
) -> impl Iterator<Item = (sk_model::RunId, &'a sk_model::WallChain)> + 'a {
    let path = scene.model().opening(id).map_or(&[][..], |o| &o.path[..]);
    let (lo, hi) = path.iter().fold(
        ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN)),
        |(lo, hi), p| {
            (
                (lo.0.min(p.x), lo.1.min(p.y)),
                (hi.0.max(p.x), hi.1.max(p.y)),
            )
        },
    );
    let nah = ((lo.0 - DICKE, lo.1 - DICKE), (hi.0 + DICKE, hi.1 + DICKE));
    scene
        .zuege_im(nah.0, nah.1)
        .filter_map(move |(run, c, quader)| {
            let weit = quader.is_some_and(|(a, b)| {
                a.x > hi.0 + DICKE || b.x < lo.0 - DICKE || a.y > hi.1 + DICKE || b.y < lo.1 - DICKE
            });
            (!weit && !path.is_empty()).then_some((run, c))
        })
}

/// Löcher der Öffnung `id` im Grundriss (z = 0) mit UK und OK (absolut).
pub fn loecher(scene: &Scene, id: ElementId) -> Vec<([Vec3; 4], (f64, f64))> {
    zuege(scene, id)
        .flat_map(|(run, _)| scene.loecher(run))
        .filter(|(h, _)| h.opening == Some(id))
        .map(|(h, q)| (*q, h.z))
        .collect()
}

/// Aufschlag der Tür `id` im Grundriss (z = 0) als Strecken: offenes Blatt
/// und Viertelkreis, wie gezeichnet; leer ohne Aufschlag.
pub fn aufschlag(scene: &Scene, id: ElementId) -> impl Iterator<Item = (Vec3, Vec3)> {
    const BOGEN: usize = 12;
    let mut out = Vec::new();
    for (_, c) in zuege(scene, id) {
        for (h, s) in c.door_swings() {
            if h.opening != Some(id) {
                continue;
            }
            let mut p = vec![s.band, s.band + s.offen];
            for k in 1..=BOGEN {
                let t = std::f64::consts::FRAC_PI_2 * k as f64 / BOGEN as f64;
                p.push(s.band + s.offen * t.cos() + s.zu * t.sin());
            }
            out.extend(p.windows(2).map(|w| (w[0], w[1])));
        }
    }
    out.into_iter()
}

/// Raum, in den die Tür `id` aufschlägt: der an einer Stelle mitten im
/// Viertelkreis.
fn raum_im_aufschlag(scene: &Scene, id: ElementId) -> Option<String> {
    let m = scene.model();
    let l = m.door_leaf_in(id, &|r| scene.koerper(r))?;
    let gegen = if (l.line.0 - l.hinge).length() < (l.line.1 - l.hinge).length() {
        l.line.1
    } else {
        l.line.0
    };
    let zu = gegen - l.hinge;
    let p = l.hinge + l.swing_into * (zu.length() * 0.4) + zu * 0.4;
    let formen = scene.raumgruppen(m.element(id)?.storey);
    let f = formen.flaechen().find(|f| {
        let k = f.kasten;
        k[0] <= p.x && p.x <= k[2] && k[1] <= p.y && p.y <= k[3] && f.form.enthaelt(p)
    })?;
    Some(m.space(f.form.raum?)?.name.clone())
}

pub fn props(
    scene: &Scene,
    id: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let o = m.opening(id)?;
    values.push(("Art", o.kind.name().into()));
    let schenkel: Vec<String> = o
        .path
        .windows(2)
        .map(|w| format!("{} m", de_m((w[1] - w[0]).length())))
        .collect();
    values.push(("Breite", schenkel.join(" + ")));
    if let Some((z0, z1)) = m.opening_band(o) {
        values.push(("Höhe (Rohbau)", format!("{} m", de_m(z1 - z0))));
        let boden = m.storey(e.storey).map_or(z0, |s| s.elevation);
        if z0 - boden > 0.5 {
            values.push(("Brüstung (Rohbau)", format!("{} m", de_m(z0 - boden))));
        }
    }
    if let Some(s) = o.swing {
        values.push(("Anschlag", s.din.name().into()));
        // Der Raum als Detailzeile: sie kürzt sich mit „…“, eine Wertzeile
        // liefe bei langen Namen über ihr Etikett (Einstellungen #132 B1)
        if let Some(r) = raum_im_aufschlag(scene, id) {
            values.push(("", format!("öffnet in {r}")));
        }
    }
    let mut waende: Vec<&str> = Vec::new();
    for (run, _) in zuege(scene, id) {
        for (h, _) in scene.loecher(run) {
            let w = m.wall_at(run, h.seg).and_then(|w| m.element(w));
            if let Some(w) = w.filter(|_| h.opening == Some(id)) {
                if !waende.contains(&w.number.as_str()) {
                    waende.push(w.number.as_str());
                }
            }
        }
    }
    Some(Props {
        values,
        set_label: "Wand",
        layer_set: if waende.is_empty() {
            "–".into()
        } else {
            waende.join(", ")
        },
        notes: m.warnings(id),
        ..Default::default()
    })
}

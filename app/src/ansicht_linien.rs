//! Hilfslinien der Ansichten aus `main.rs` (Pfadfinderregel,
//! architektur.md §8): die Linie der gezogenen Ebene. Die Geländelinie
//! ersetzt die Geländeplatte (Gelände G0, [`crate::gelaende_platte`]).

use crate::ui::ViewKind;
use sk_math::{vec3, Vec3};
use sk_ui::theme::Theme;

/// Hilfslinie der gezogenen Ebene in Höhe `z`: in 3D ein Rechteck um den
/// Grundriss des Modells (0,5 m Überstand), in Schnitt und Ansichten eine
/// waagerechte Linie wie die Geländelinie, im Grundriss nichts.
pub(crate) fn level_guide(
    v: ViewKind,
    bounds: Option<(Vec3, Vec3)>,
    z: f64,
    scale: f32,
    theme: &Theme,
) -> Vec<sk_render::Helper> {
    let (lo, hi) = bounds.unwrap_or((vec3(-2000.0, -2000.0, 0.0), vec3(12000.0, 10000.0, 0.0)));
    let line = |a: Vec3, b: Vec3| sk_render::Helper {
        a: a.to_f32(),
        b: b.to_f32(),
        color: theme.interact.drag,
        width: theme.size.level_guide * scale,
        dash: 0.0,
        pattern: sk_render::SOLID,
        occlude: false,
        round: true,
    };
    match v {
        ViewKind::Plan => Vec::new(),
        ViewKind::Persp => {
            let m = 500.0;
            let (x0, y0, x1, y1) = (lo.x - m, lo.y - m, hi.x + m, hi.y + m);
            let c = [
                vec3(x0, y0, z),
                vec3(x1, y0, z),
                vec3(x1, y1, z),
                vec3(x0, y1, z),
            ];
            (0..4).map(|i| line(c[i], c[(i + 1) % 4])).collect()
        }
        ViewKind::Left | ViewKind::Right => {
            let m = 3000.0;
            vec![line(vec3(lo.x, lo.y - m, z), vec3(lo.x, hi.y + m, z))]
        }
        _ => {
            let m = 3000.0;
            vec![line(vec3(lo.x - m, lo.y, z), vec3(hi.x + m, lo.y, z))]
        }
    }
}

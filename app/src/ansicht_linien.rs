//! Hilfslinien der Ansichten aus `main.rs` (Pfadfinderregel,
//! architektur.md §8): Geländelinie in Ansicht und Schnitt mit ihren
//! Lücken und die Linie der gezogenen Ebene.

use crate::draw_table::DrawTable;
use crate::ui::ViewKind;
use sk_math::{vec3, Vec3};
use sk_ui::theme::Theme;

/// Geländelinie in Ansicht und Schnitt: auf Höhe des Geländes an jedem
/// Gebäude (`patches` längs der Linie, sonst `z`), im Schnitt mit Lücken an
/// geschnittenen Bauteilen (`gaps`, längs der Linie).
pub(crate) fn ground_line(
    v: ViewKind,
    bounds: Option<(Vec3, Vec3)>,
    z: f64,
    patches: &[([f64; 4], f64)],
    scale: f32,
    table: &DrawTable,
    gaps: &[(f64, f64)],
) -> Vec<sk_render::Helper> {
    let (lo, hi) = bounds.unwrap_or((vec3(-2000.0, -2000.0, 0.0), vec3(12000.0, 10000.0, 0.0)));
    let m = 3000.0;
    let side = matches!(v, ViewKind::Left | ViewKind::Right);
    let (a, b) = if side {
        (lo.y - m, hi.y + m)
    } else {
        (lo.x - m, hi.x + m)
    };
    let at = |u: f64, z: f64| {
        if side {
            vec3(lo.x, u, z)
        } else {
            vec3(u, lo.y, z)
        }
    };
    // Höhe längs der Linie: das erste Gebäude, das die Stelle überdeckt
    let span = |p: &[f64; 4]| if side { (p[1], p[3]) } else { (p[0], p[2]) };
    let z_at = |u: f64| {
        patches
            .iter()
            .find(|(p, _)| {
                let (p0, p1) = span(p);
                u >= p0 && u <= p1
            })
            .map_or(z, |x| x.1)
    };
    let mut cuts: Vec<f64> = patches
        .iter()
        .flat_map(|(p, _)| {
            let (p0, p1) = span(p);
            [p0, p1]
        })
        .chain(gaps.iter().flat_map(|g| [g.0, g.1]))
        .filter(|u| *u > a && *u < b)
        .collect();
    cuts.extend([a, b]);
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    // Stücke zwischen den Teilungen: in einer Lücke keins, sonst auf der
    // Höhe in ihrer Mitte; gleiche Höhen hintereinander zu einem Stück
    let mut pieces: Vec<(f64, f64, f64)> = Vec::new();
    for w in cuts.windows(2) {
        let (p, q) = (w[0], w[1]);
        let mid = (p + q) * 0.5;
        if q <= p || gaps.iter().any(|g| mid > g.0 && mid < g.1) {
            continue;
        }
        let h = z_at(mid);
        match pieces.last_mut() {
            Some(l) if l.1 == p && l.2 == h => l.1 = q,
            _ => pieces.push((p, q, h)),
        }
    }
    let line = |p: Vec3, q: Vec3| sk_render::Helper {
        a: p.to_f32(),
        b: q.to_f32(),
        color: table.ground.1,
        width: table.ground.0 * scale,
        dash: 0.0,
        pattern: sk_render::SOLID,
        occlude: false,
        round: false,
    };
    let mut out: Vec<sk_render::Helper> = Vec::new();
    for (i, &(p, q, h)) in pieces.iter().enumerate() {
        // Stufe zur Höhe des vorigen Stücks, wenn es anschließt
        if let Some(&(_, q0, h0)) = i.checked_sub(1).and_then(|j| pieces.get(j)) {
            if q0 == p && h0 != h {
                out.push(line(at(p, h0), at(p, h)));
            }
        }
        out.push(line(at(p, h), at(q, h)));
    }
    out
}

/// OK Gelände an `(x, y)`: das erste Gebäude-Rechteck, das die Stelle
/// enthält, sonst `z`.
pub(crate) fn terrain_at(patches: &[([f64; 4], f64)], z: f64, x: f64, y: f64) -> f64 {
    patches
        .iter()
        .find(|(b, _)| x >= b[0] && y >= b[1] && x <= b[2] && y <= b[3])
        .map_or(z, |p| p.1)
}

/// Gelände-Rechtecke für den Renderer.
pub(crate) fn terrain_boxes(patches: &[([f64; 4], f64)]) -> Vec<([f32; 4], f32)> {
    patches
        .iter()
        .map(|(b, z)| (b.map(|v| v as f32), *z as f32))
        .collect()
}

/// Wo die Geländelinie (Höhe `ter(x, y)`) im Schnitt aussetzt, längs der Achse
/// `axis` (0 = x, 1 = y), sortiert und vereinigt: über jeder geschnittenen
/// Fläche, die die Linie berührt, und dazwischen, wo eine geschnittene
/// Fläche darüber liegt (unter der Platte, in den Räumen).
pub(crate) fn terrain_gaps(
    faces: &[[f32; 9]],
    ter: impl Fn(f64, f64) -> f64,
    axis: usize,
) -> Vec<(f64, f64)> {
    let tol = 0.5;
    let mut hit = Vec::new();
    let mut roof = Vec::new();
    for t in faces.chunks_exact(3) {
        let key = t[0][6] as u32;
        if key & sk_model::material::CUT as u32 == 0 {
            continue;
        }
        let p: Vec<(f64, f64)> = t.iter().map(|v| (v[axis] as f64, v[2] as f64)).collect();
        // OK Gelände am Gebäude des Dreiecks (Mitte im Grundriss)
        let c = |k: usize| t.iter().map(|v| v[k] as f64).sum::<f64>() / 3.0;
        let z = ter(c(0), c(1));
        let zmin = p.iter().map(|q| q.1).fold(f64::INFINITY, f64::min);
        let zmax = p.iter().map(|q| q.1).fold(f64::NEG_INFINITY, f64::max);
        let umin = p.iter().map(|q| q.0).fold(f64::INFINITY, f64::min);
        let umax = p.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max);
        if zmin > z + tol {
            roof.push((umin, umax));
            continue;
        }
        if zmax < z - tol {
            continue;
        }
        // Schnitt des Dreiecks mit der Höhe z: Ecken darauf und Kanten,
        // die sie kreuzen
        let mut us = Vec::new();
        for k in 0..3 {
            let (a, b) = (p[k], p[(k + 1) % 3]);
            if (a.1 - z).abs() <= tol {
                us.push(a.0);
            }
            if (a.1 - z) * (b.1 - z) < 0.0 {
                us.push(a.0 + (b.0 - a.0) * (z - a.1) / (b.1 - a.1));
            }
        }
        if let (Some(u0), Some(u1)) = (
            us.iter().copied().reduce(f64::min),
            us.iter().copied().reduce(f64::max),
        ) {
            hit.push((u0, u1));
        }
    }
    let merge = |mut v: Vec<(f64, f64)>| {
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out: Vec<(f64, f64)> = Vec::new();
        for (a, b) in v {
            match out.last_mut() {
                Some(l) if a <= l.1 + tol => l.1 = l.1.max(b),
                _ => out.push((a, b)),
            }
        }
        out
    };
    let roof = merge(roof);
    let covered = |a: f64, b: f64| roof.iter().any(|r| r.0 <= a + tol && r.1 >= b - tol);
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (a, b) in merge(hit) {
        match out.last_mut() {
            Some(l) if covered(l.1, a) => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

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

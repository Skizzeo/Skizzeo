//! Spurlinien und Winkelsprung beim Zeichnen eines Zugs: dieselben Hilfen
//! für die Außenwand ([`super::WallTool`]) und für „Grenze zeichnen“ und
//! „Fläche zeichnen“ im Gelände (Jörn 18.10.: ein immer wiederkehrender
//! Arbeitsablauf). Alles in der Draufsicht; `z` der Punkte bleibt, wie es
//! ist.

use sk_math::{vec3, Vec3};

/// Spurlinie oder Richtung: durch `origin` in Richtung `dir` (Länge 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub origin: Vec3,
    pub dir: Vec3,
    /// Durch den Startpunkt (sonst durch den letzten Punkt).
    pub from_start: bool,
}

pub fn perp(d: Vec3) -> Vec3 {
    vec3(-d.y, d.x, 0.0)
}

pub fn cross2(a: Vec3, b: Vec3) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Richtungen ohne Dubletten (auch gegenläufige gelten als gleich).
pub fn push_dir(dirs: &mut Vec<Vec3>, d: Vec3) {
    let d = vec3(d.x, d.y, 0.0).normalized();
    if d.length() < 0.5 {
        return;
    }
    if dirs.iter().all(|e| cross2(*e, d).abs() > 1e-3) {
        dirs.push(d);
    }
}

/// Spurlinien vom Startpunkt aus: parallel und rechtwinklig zum ersten
/// Segment und entlang der Achsen; erst ab zwei Punkten.
pub fn start_lines(points: &[Vec3]) -> Vec<Line> {
    if points.len() < 2 {
        return Vec::new();
    }
    let s = points[0];
    let mut dirs = Vec::new();
    push_dir(&mut dirs, points[1] - s);
    push_dir(&mut dirs, perp(points[1] - s));
    push_dir(&mut dirs, Vec3::X);
    push_dir(&mut dirs, Vec3::Y);
    dirs.into_iter()
        .map(|dir| Line {
            origin: s,
            dir,
            from_start: true,
        })
        .collect()
}

/// Richtungen vom letzten Punkt aus: entlang und rechtwinklig zum letzten
/// Segment, beim ersten entlang der Achsen und `start_dir`; mit `schraeg`
/// auch die Diagonalen dazu (G2).
pub fn last_lines(points: &[Vec3], start_dir: Option<Vec3>, schraeg: bool) -> Vec<Line> {
    let Some(&last) = points.last() else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    let mut kreuz = |d: Vec3| {
        push_dir(&mut dirs, d);
        push_dir(&mut dirs, perp(d));
        let d = vec3(d.x, d.y, 0.0);
        if schraeg && d.length() > 1e-9 {
            let d = d.normalized();
            push_dir(&mut dirs, d + perp(d));
            push_dir(&mut dirs, d - perp(d));
        }
    };
    if points.len() >= 2 {
        kreuz(last - points[points.len() - 2]);
    } else {
        if let Some(d) = start_dir {
            kreuz(d);
        }
        kreuz(Vec3::X);
    }
    dirs.into_iter()
        .map(|dir| Line {
            origin: last,
            dir,
            from_start: false,
        })
        .collect()
}

/// Winkelsprung: die Richtung in `lasts`, die am besten zu `raw` passt.
pub fn passendste(lasts: &[Line], raw: Vec3) -> Option<Line> {
    let v = raw - lasts.first()?.origin;
    lasts
        .iter()
        .max_by(|a, b| a.dir.dot(v).abs().total_cmp(&b.dir.dot(v).abs()))
        .copied()
}

/// `p` auf die Linie `l` gelotet.
pub fn lot(l: &Line, p: Vec3) -> Vec3 {
    l.origin + l.dir * (p - l.origin).dot(l.dir)
}

/// Nächster Schnitt einer Richtung in `lasts` mit einer Spurlinie in
/// `starts`, `px` Bildpunkte vom Cursor und näher als `thr`: (Abstand,
/// Punkt, beide Linien).
pub fn kreuzung(
    lasts: &[Line],
    starts: &[Line],
    px: &dyn Fn(Vec3) -> f64,
    thr: f64,
) -> Option<(f64, Vec3, [Line; 2])> {
    let mut best: Option<(f64, Vec3, [Line; 2])> = None;
    for l in lasts {
        for s in starts {
            let c = cross2(l.dir, s.dir);
            if c.abs() < 1e-9 {
                continue;
            }
            let t = cross2(s.origin - l.origin, s.dir) / c;
            let x = l.origin + l.dir * t;
            let d = px(x);
            if d < thr && best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, x, [*l, *s]));
            }
        }
    }
    best
}

/// Die nächste Linie in `lines`, auf die sich `raw` loten lässt, näher als
/// `thr` Bildpunkte: Punkt darauf und Linie.
pub fn naechste(
    lines: &[Line],
    raw: Vec3,
    px: &dyn Fn(Vec3) -> f64,
    thr: f64,
) -> Option<(Vec3, Line)> {
    let mut best: Option<(f64, Vec3, Line)> = None;
    for l in lines {
        let p = lot(l, raw);
        let d = px(p);
        if d < thr && best.as_ref().is_none_or(|b| d < b.0) {
            best = Some((d, p, *l));
        }
    }
    best.map(|(_, p, l)| (p, l))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spurlinien_ohne_dubletten() {
        let p = [Vec3::ZERO, vec3(4000.0, 0.0, 0.0)];
        // erstes Segment entlang x: x und y je einmal
        assert_eq!(start_lines(&p).len(), 2);
        assert!(start_lines(&p[..1]).is_empty());
        let l = last_lines(&p, None, true);
        assert_eq!(l.len(), 4, "längs, quer und zwei Diagonalen");
        assert!(l.iter().all(|l| l.origin == p[1] && !l.from_start));
    }

    #[test]
    fn kreuzung_und_lot() {
        let p = [
            Vec3::ZERO,
            vec3(4000.0, 0.0, 0.0),
            vec3(4000.0, 3000.0, 0.0),
        ];
        let lasts = last_lines(&p, None, false);
        let starts = start_lines(&p);
        // im Bild 1:1, Cursor bei (10, 2990)
        let c = vec3(10.0, 2990.0, 0.0);
        let px = |q: Vec3| (q - c).length();
        let (_, x, [l, s]) = kreuzung(&lasts, &starts, &px, 50.0).unwrap();
        assert!((x - vec3(0.0, 3000.0, 0.0)).length() < 1e-6);
        assert!(!l.from_start && s.from_start);
        let (q, _) = naechste(&lasts, c, &px, 50.0).unwrap();
        assert!((q - vec3(10.0, 3000.0, 0.0)).length() < 1e-6);
        assert!(naechste(&lasts, vec3(2000.0, 1500.0, 0.0), &px, 50.0).is_none());
        let b = passendste(&lasts, vec3(3000.0, 2900.0, 0.0)).unwrap();
        assert!(b.dir.x.abs() > 0.99, "fast waagerecht: längs x");
    }
}

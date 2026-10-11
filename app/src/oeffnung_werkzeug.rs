//! Werkzeug „Wandöffnung“ und „Türöffnung“ (Paket O3, Jörn 11.10.
//! 14:02–14:22): Über einer Außenwandfläche erscheinen Brüstung und Sturz
//! ihres Geschosses als dünne gestrichelte Linien, immer obenauf. Die erste
//! Ecke fängt auf einer Linie (oder am Wandfuß, OK Rohdecke), die zweite
//! diagonal dazu, auch auf der Nachbarwand um die Außenecke oder auf der
//! Wand des Geschosses darüber. Eine auf einer Linie gefangene Kante bleibt
//! an sie gebunden; frei gesetzt steht sie als Maß über OK Rohdecke.
//!
//! Hier steht die reine Rechnung (Fangen, Öffnung, Umriss); das Glue mit
//! Maus, Szene und Zeichnung steht in `oeffnung_app.rs`.

use sk_math::{vec3, Vec3};
use sk_model::oeffnung::{FaceLine, HeightRef, Opening, OpeningKind, OuterFace};
use sk_model::{LevelEdge, LevelRef};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Eine gefangene Ecke: Fläche, Punkt darauf und Höhenbezug.
#[derive(Clone, Debug, PartialEq)]
pub struct Fang {
    pub face: OuterFace,
    pub p: Vec3,
    pub bind: HeightRef,
    /// Auf einer Höhenlinie oder am Wandfuß gefangen.
    pub line: bool,
}

/// Raster längs und in der Höhe, wenn nichts fängt (mm).
const RASTER: f64 = 10.0;

/// Fängt `p` auf der Fläche `face`: in der Höhe an einer Linie aus `lines`
/// oder am Wandfuß (OK Rohdecke, `elevation`), wenn näher als `tol`; längs
/// an den Enden der Fläche. Sonst auf [`RASTER`] gerundet.
pub fn fangen(face: &OuterFace, lines: &[FaceLine], elevation: f64, p: Vec3, tol: f64) -> Fang {
    let foot = FaceLine {
        z: elevation,
        bind: HeightRef::Level(LevelRef::bottom(face.storey)),
    };
    let best = lines
        .iter()
        .chain([&foot])
        .filter(|l| (l.z - p.z).abs() < tol)
        .min_by(|a, b| (a.z - p.z).abs().total_cmp(&(b.z - p.z).abs()));
    let (z, bind, line) = match best {
        Some(l) => (l.z, l.bind, true),
        None => {
            let off = ((p.z - elevation) / RASTER).round() * RASTER;
            let bind = HeightRef::Level(LevelRef {
                storey: face.storey,
                edge: LevelEdge::Bottom,
                offset: off,
            });
            (elevation + off, bind, false)
        }
    };
    let u = face.u_of(vec3(p.x, p.y, 0.0));
    let u = if (u - face.u.0).abs() < tol {
        face.u.0
    } else if (u - face.u.1).abs() < tol {
        face.u.1
    } else {
        (u / RASTER).round() * RASTER
    };
    Fang {
        face: face.clone(),
        p: face.at(u, z),
        bind,
        line,
    }
}

/// Liegen beide Flächen in einer Flucht (dieselbe Außenfläche, auch über
/// Geschosse)?
fn flucht(a: &OuterFace, b: &OuterFace) -> bool {
    a.d.cross(b.d).length() < 1e-6 && a.on_line(b.a)
}

/// Außenecke zwischen den Flächen `a` und `b`: Schnitt ihrer Linien.
fn ecke(a: &OuterFace, b: &OuterFace) -> Option<Vec3> {
    let c = a.d.x * b.d.y - a.d.y * b.d.x;
    if c.abs() < 1e-6 {
        return None;
    }
    let w = b.a - a.a;
    let s = (w.x * b.d.y - w.y * b.d.x) / c;
    let p = a.a + a.d * s;
    Some(vec3(p.x, p.y, 0.0))
}

/// Grundrisspfad zwischen zwei Ecken: gerade in einer Flucht, sonst über
/// die Außenecke der beiden Flächen.
pub fn pfad(a: &Fang, b: &Fang) -> Option<Vec<Vec3>> {
    let flat = |p: Vec3| vec3(p.x, p.y, 0.0);
    if flucht(&a.face, &b.face) {
        return Some(vec![flat(a.p), flat(b.p)]);
    }
    let c = ecke(&a.face, &b.face)?;
    Some(vec![flat(a.p), c, flat(b.p)])
}

/// Öffnung der Art `kind` aus zwei diagonalen Ecken; `None`, wenn sich kein
/// Pfad ergibt (Flächen parallel, aber nicht in einer Flucht).
pub fn oeffnung(kind: OpeningKind, a: &Fang, b: &Fang) -> Option<Opening> {
    let (lo, hi) = if a.p.z <= b.p.z { (a, b) } else { (b, a) };
    Some(Opening {
        kind,
        path: pfad(a, b)?,
        bottom: lo.bind,
        top: hi.bind,
        reveal: None,
        install: Default::default(),
    })
}

/// Umriss der Öffnung von außen als Strecken (über die Ecke in zwei
/// Rechtecken), zum Zeichnen der Vorschau.
pub fn umriss(a: &Fang, b: &Fang) -> Vec<(Vec3, Vec3)> {
    let Some(path) = pfad(a, b) else {
        return Vec::new();
    };
    let (z0, z1) = (a.p.z.min(b.p.z), a.p.z.max(b.p.z));
    let up = |p: Vec3, z: f64| vec3(p.x, p.y, z);
    let mut out = Vec::new();
    for w in path.windows(2) {
        out.push((up(w[0], z0), up(w[1], z0)));
        out.push((up(w[0], z1), up(w[1], z1)));
    }
    for p in [path[0], path[path.len() - 1]] {
        out.push((up(p, z0), up(p, z1)));
    }
    out
}

/// Höhenlinien über die ganze Breite der Fläche, als Strecken.
pub fn linien(face: &OuterFace, lines: &[FaceLine]) -> Vec<(Vec3, Vec3)> {
    lines
        .iter()
        .filter(|l| l.z > face.z.0 && l.z < face.z.1)
        .map(|l| (face.at(face.u.0, l.z), face.at(face.u.1, l.z)))
        .collect()
}

/// Zustand des Werkzeugs: Art, Ecke unter der Maus, gesetzte erste Ecke.
#[derive(Clone, Debug, PartialEq)]
pub struct Werkzeug {
    pub kind: OpeningKind,
    pub hover: Option<Fang>,
    pub erste: Option<Fang>,
    /// Zuletzt gerechnete Vorschau, die der Wandwerkzeug zeichnet.
    pub vorschau: Bild,
}

/// Was die Vorschau zeichnet: Höhenlinien (gestrichelt, obenauf) und den
/// Umriss der Öffnung.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bild {
    pub linien: Vec<(Vec3, Vec3)>,
    pub umriss: Vec<(Vec3, Vec3)>,
    /// Die Ecke unter der Maus und ob sie auf einer Linie gefangen ist.
    pub punkt: Option<(Vec3, bool)>,
}

impl Werkzeug {
    pub fn new(kind: OpeningKind) -> Werkzeug {
        Werkzeug {
            kind,
            hover: None,
            erste: None,
            vorschau: Bild::default(),
        }
    }

    /// Klick: setzt die erste Ecke, beim zweiten die Öffnung (dann beginnt
    /// das Werkzeug von vorn). `None`, solange nichts entsteht.
    pub fn klick(&mut self) -> Option<Opening> {
        let h = self.hover.clone()?;
        match self.erste.take() {
            None => {
                self.erste = Some(h);
                None
            }
            Some(a) => {
                let o = oeffnung(self.kind, &a, &h);
                if o.is_none() {
                    self.erste = Some(a);
                }
                o
            }
        }
    }

    /// Esc: nimmt die erste Ecke zurück (`true`); ohne sie schließt das
    /// Werkzeug (`false`).
    pub fn esc(&mut self) -> bool {
        self.erste.take().is_some()
    }

    /// Vorschau; `lines` liefert die Höhenlinien einer Fläche.
    pub fn bild(&self, lines: impl Fn(&OuterFace) -> Vec<FaceLine>) -> Bild {
        let mut b = Bild::default();
        let mut faces: Vec<&OuterFace> = Vec::new();
        for f in [&self.erste, &self.hover].into_iter().flatten() {
            if !faces.iter().any(|x| **x == f.face) {
                faces.push(&f.face);
            }
        }
        for f in faces {
            b.linien.extend(linien(f, &lines(f)));
        }
        if let (Some(a), Some(h)) = (&self.erste, &self.hover) {
            b.umriss = umriss(a, h);
        }
        b.punkt = self.hover.as_ref().map(|h| (h.p, h.line));
        b
    }
}

/// Hilfslinien der Vorschau: Höhenlinien dünn gestrichelt, Umriss und Ecke
/// durchgezogen, alles obenauf (Jörn 14:02: ohne z-Fighting).
pub fn helpers(b: &Bild, scale: f32, theme: &Theme) -> Vec<Helper> {
    let col = &theme.interact;
    let p = |v: Vec3| [v.x as f32, v.y as f32, v.z as f32];
    let line = |a: Vec3, c: Vec3, color, width: f32, dash: f32| Helper {
        a: p(a),
        b: p(c),
        color,
        width: width * scale,
        dash: dash * scale,
        pattern: sk_render::SOLID,
        occlude: false,
        round: false,
    };
    let mut out: Vec<Helper> = b
        .linien
        .iter()
        .map(|(a, c)| line(*a, *c, col.guide, 1.0, 5.0))
        .collect();
    out.extend(
        b.umriss
            .iter()
            .map(|(a, c)| line(*a, *c, col.draw, 2.0, 0.0)),
    );
    if let Some((q, fang)) = b.punkt {
        let (color, size) = if fang {
            (col.track, 9.0)
        } else {
            (col.draw, 6.0)
        };
        out.push(line(q, q, col.shadow_tool, size + 3.0, 0.0));
        out.push(line(q, q, color, size, 0.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::oeffnung::HeightLine;
    use sk_model::{Model, RunId, StoreyId};

    fn haus() -> (Model, RunId, RunId) {
        let mut m = Model::with_seed(311);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let og = m.runs_above(eg)[0];
        (m, eg, og)
    }

    fn fang(m: &Model, run: RunId, seg: usize, u: f64, z: f64) -> Fang {
        let f = m.outer_face(run, seg).unwrap();
        let st: StoreyId = f.storey;
        let el = m.storey(st).unwrap().elevation;
        fangen(&f, &m.face_lines(st), el, f.at(u, z), 40.0)
    }

    /// Ecke nahe der Brüstung fängt und bindet; frei gesetzt steht sie
    /// gerundet über OK Rohdecke; nahe dem Wandfuß fängt sie dort.
    #[test]
    fn fangen_an_linien() {
        let (m, eg, _) = haus();
        let el = m
            .storey(m.outer_face(eg, 0).unwrap().storey)
            .unwrap()
            .elevation;
        let a = fang(&m, eg, 0, 3003.0, el + 890.0);
        assert!(a.line);
        assert!((a.p.z - el - 875.0).abs() < 1e-9);
        assert!(matches!(a.bind, HeightRef::Line(_, HeightLine::Sill)));
        let b = fang(&m, eg, 0, 3003.0, el + 1504.0);
        assert!(!b.line && (b.p.z - el - 1500.0).abs() < 1e-9);
        let c = fang(&m, eg, 0, 3003.0, el + 12.0);
        assert!(c.line && (c.p.z - el).abs() < 1e-9);
        let f = m.outer_face(eg, 0).unwrap();
        assert!((f.u_of(a.p) - 3000.0).abs() < 1e-9);
    }

    /// Zwei Ecken auf derselben Fläche: Fenster an Brüstung und Sturz, das
    /// Modell nimmt es an.
    #[test]
    fn fenster_aufziehen() {
        let (mut m, eg, _) = haus();
        let el = m
            .storey(m.outer_face(eg, 0).unwrap().storey)
            .unwrap()
            .elevation;
        let a = fang(&m, eg, 0, 3000.0, el + 880.0);
        let b = fang(&m, eg, 0, 4250.0, el + 2380.0);
        let o = oeffnung(OpeningKind::Window, &b, &a).unwrap();
        assert_eq!(o.path.len(), 2);
        assert!(matches!(o.bottom, HeightRef::Line(_, HeightLine::Sill)));
        assert!(matches!(o.top, HeightRef::Line(_, HeightLine::Lintel)));
        assert!(m.add_opening(o).is_some());
        assert_eq!(m.chain(eg).unwrap().joints.holes.len(), 1);
        assert_eq!(umriss(&a, &b).len(), 4);
    }

    /// Um die Außenecke (Jörn 14:22) und über zwei Geschosse (14:04).
    #[test]
    fn ecke_und_geschosse() {
        let (mut m, eg, og) = haus();
        let el = m
            .storey(m.outer_face(eg, 0).unwrap().storey)
            .unwrap()
            .elevation;
        let a = fang(&m, eg, 0, 7000.0, el + 875.0);
        let b = fang(&m, eg, 1, 1500.0, el + 2385.0);
        let o = oeffnung(OpeningKind::Window, &a, &b).unwrap();
        assert_eq!(o.path.len(), 3);
        assert!(m.add_opening(o).is_some());
        assert_eq!(m.chain(eg).unwrap().joints.holes.len(), 2);
        assert_eq!(umriss(&a, &b).len(), 6);
        let el_og = m
            .storey(m.outer_face(og, 0).unwrap().storey)
            .unwrap()
            .elevation;
        let c = fang(&m, eg, 0, 2000.0, el + 875.0);
        let d = fang(&m, og, 0, 3000.0, el_og + 2385.0);
        let o = oeffnung(OpeningKind::Window, &c, &d).unwrap();
        assert_eq!(o.path.len(), 2);
        assert!(m.add_opening(o).is_some());
        assert_eq!(m.chain(og).unwrap().joints.holes.len(), 1);
    }

    /// Gegenüberliegende Wände ergeben keine Öffnung; die Linien liegen auf
    /// der Fläche über ihre ganze Breite.
    #[test]
    fn parallel_keine_oeffnung_und_linien() {
        let (m, eg, _) = haus();
        let el = m
            .storey(m.outer_face(eg, 0).unwrap().storey)
            .unwrap()
            .elevation;
        let a = fang(&m, eg, 0, 3000.0, el + 875.0);
        let b = fang(&m, eg, 2, 3000.0, el + 2385.0);
        assert!(oeffnung(OpeningKind::Door, &a, &b).is_none());
        let f = m.outer_face(eg, 0).unwrap();
        let l = linien(&f, &m.face_lines(f.storey));
        assert_eq!(l.len(), 2);
        assert!(l.iter().all(|(p, q)| f.on_line(*p) && f.on_line(*q)));
    }

    /// Ablauf: Klick, Klick ergibt die Öffnung; Esc nimmt die erste Ecke
    /// zurück, dann schließt es.
    #[test]
    fn werkzeug_ablauf() {
        let (m, eg, _) = haus();
        let el = m
            .storey(m.outer_face(eg, 0).unwrap().storey)
            .unwrap()
            .elevation;
        let mut w = Werkzeug::new(OpeningKind::Door);
        assert!(w.klick().is_none(), "ohne Fläche nichts");
        w.hover = Some(fang(&m, eg, 0, 3000.0, el));
        assert!(w.klick().is_none());
        assert!(w.erste.is_some());
        w.hover = Some(fang(&m, eg, 0, 4010.0, el + 2140.0));
        let b = w.bild(|f| m.face_lines(f.storey));
        assert_eq!((b.linien.len(), b.umriss.len()), (2, 4));
        let o = w.klick().unwrap();
        assert_eq!(o.kind, OpeningKind::Door);
        assert!(matches!(o.bottom, HeightRef::Level(_)));
        assert!(w.erste.is_none());
        w.hover = Some(fang(&m, eg, 0, 3000.0, el));
        w.klick();
        assert!(w.esc() && !w.esc());
    }
}

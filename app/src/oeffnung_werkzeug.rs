//! Werkzeug „Wandöffnung“ und „Türöffnung“ (Paket O3, Jörn 11.10.
//! 14:02–14:22): Über einer Außenwandfläche erscheinen Brüstung und Sturz
//! ihres Geschosses als dünne gestrichelte Linien, immer obenauf. Die erste
//! Ecke fängt auf einer Linie (oder am Wandfuß, OK Rohdecke), die zweite
//! diagonal dazu, auch auf der Nachbarwand um die Außenecke oder auf der
//! Wand des Geschosses darüber. Eine auf einer Linie gefangene Kante bleibt
//! an sie gebunden; frei gesetzt steht sie als Maß über OK Rohdecke.
//!
//! Im Grundriss (Jörn 12.10. 09:26) zieht man Fenster und Außentür in der
//! Außenwand auf: Klick auf die erste Kante, die Breite folgt der Maus in
//! der Vorschau, der zweite Klick setzt. Die Höhen kommen aus den
//! Höhenbezugslinien des Geschosses: das Fenster von der Brüstung zum
//! Sturz, die Außentür von OK Rohdecke zum Sturz.
//!
//! Hier steht die reine Rechnung (Fangen, Öffnung, Umriss); das Glue mit
//! Maus, Szene und Zeichnung steht in `oeffnung_app.rs`.

use sk_math::{vec3, Vec3};
use sk_model::oeffnung::{FaceLine, HeightLine, HeightRef, Opening, OpeningKind, OuterFace};
use sk_model::{LevelEdge, LevelRef, Model, StoreyId};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Eine gefangene Ecke: Fläche, Punkt darauf und Höhenbezug.
#[derive(Clone, Debug, PartialEq)]
pub struct Fang {
    pub face: OuterFace,
    pub p: Vec3,
    pub bind: HeightRef,
    /// Auf einer Höhenlinie oder am Wandfuß gefangen; im Grundriss am Ende
    /// des freien Bereichs.
    pub line: bool,
    /// Wanddicke für den Umriss im Grundriss; 0 in 3D und den Ansichten.
    pub dicke: f64,
}

/// Raster längs und in der Höhe, wenn nichts fängt (mm).
const RASTER: f64 = 10.0;

/// Fängt `p` auf der Fläche `face`: in der Höhe an einer Linie aus `lines`
/// oder am Wandfuß (OK Rohdecke, `elevation`), wenn näher als `tol`; längs
/// an den Enden des freien Bereichs ([`OuterFace::free`]), nie darüber
/// hinaus. Sonst auf [`RASTER`] gerundet.
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
    Fang {
        face: face.clone(),
        p: face.at(laengs(face, p, tol).0, z),
        bind,
        line,
        dicke: 0.0,
    }
}

/// Lage längs der Fläche: an den Enden des freien Bereichs (`true`), nicht
/// an der Ecke selbst (dort kann eine Öffnung nicht enden, Review #79),
/// sonst auf [`RASTER`] gerundet.
fn laengs(face: &OuterFace, p: Vec3, tol: f64) -> (f64, bool) {
    let u = face.u_of(vec3(p.x, p.y, 0.0));
    let (f0, f1) = face.free;
    if u < f0 + tol {
        (f0, true)
    } else if u > f1 - tol {
        (f1, true)
    } else {
        (((u / RASTER).round() * RASTER).clamp(f0, f1), false)
    }
}

/// Kante im Grundriss: Lage längs wie [`fangen`], gezeichnet in Höhe `z`
/// (Schnitthöhe); `dicke` ist die Wanddicke für den Umriss.
pub fn plan_fang(face: &OuterFace, p: Vec3, tol: f64, z: f64, dicke: f64) -> Fang {
    let (u, line) = laengs(face, p, tol);
    Fang {
        face: face.clone(),
        p: face.at(u, z),
        bind: HeightRef::Level(LevelRef::bottom(face.storey)),
        line,
        dicke,
    }
}

/// Höhen einer im Grundriss gesetzten Öffnung an den Höhenbezugslinien des
/// Geschosses `st`: das Fenster von der Brüstung zum Sturz, die Außentür
/// von OK Rohdecke zum Sturz (Jörn 12.10. 09:26).
pub fn plan_hoehen(kind: OpeningKind, st: StoreyId) -> (HeightRef, HeightRef) {
    let unten = match kind {
        OpeningKind::Window => HeightRef::Line(st, HeightLine::Sill),
        OpeningKind::Door => HeightRef::Level(LevelRef::bottom(st)),
    };
    (unten, HeightRef::Line(st, HeightLine::Lintel))
}

/// Öffnung aus zwei Kanten im Grundriss, Höhen nach [`plan_hoehen`].
pub fn plan_oeffnung(kind: OpeningKind, a: &Fang, b: &Fang) -> Option<Opening> {
    let (bottom, top) = plan_hoehen(kind, a.face.storey);
    Some(Opening {
        kind,
        path: pfad(a, b)?,
        bottom,
        top,
        reveal: None,
        install: Default::default(),
        swing: None,
        lock: Default::default(),
    })
}

/// Pfad `path` um `s` Wanddicken hinter die Außenflächen von `fa`
/// (Pfadanfang) und `fb` (Pfadende) versetzt; an der Außenecke der Schnitt
/// beider Versatzlinien.
fn versatz(path: &[Vec3], fa: &Fang, fb: &Fang, s: f64) -> Vec<Vec3> {
    let (na, nb) = (fa.face.n, fb.face.n);
    let (ta, tb) = (fa.dicke * s, fb.dicke * s);
    let mut out = vec![path[0] - na * ta];
    if path.len() == 3 {
        // (x − c)·na = −ta und (x − c)·nb = −tb
        let det = na.x * nb.y - na.y * nb.x;
        let dx = (-ta * nb.y + tb * na.y) / det;
        let dy = (-na.x * tb + nb.x * ta) / det;
        out.push(path[1] + vec3(dx, dy, 0.0));
    }
    out.push(path[path.len() - 1] - nb * tb);
    out
}

/// Umriss im Grundriss in Zeichenhöhe: Außenkante, Innenkante und die
/// beiden Laibungen quer durch die Wand; beim Fenster dazu die Glaslinie
/// in Wandmitte.
pub fn plan_umriss(kind: OpeningKind, a: &Fang, b: &Fang) -> Vec<(Vec3, Vec3)> {
    let Some(path) = pfad(a, b) else {
        return Vec::new();
    };
    let hoch = |p: Vec3| vec3(p.x, p.y, a.p.z);
    let mut linien = vec![path.clone(), versatz(&path, a, b, 1.0)];
    if kind == OpeningKind::Window {
        linien.push(versatz(&path, a, b, 0.5));
    }
    let mut out: Vec<(Vec3, Vec3)> = linien
        .iter()
        .flat_map(|l| l.windows(2).map(|w| (hoch(w[0]), hoch(w[1]))))
        .collect();
    let innen = &linien[1];
    out.push((hoch(path[0]), hoch(innen[0])));
    out.push((hoch(path[path.len() - 1]), hoch(innen[innen.len() - 1])));
    out
}

/// Breite längs des Pfads (über die Ecke beide Schenkel).
pub fn plan_breite(a: &Fang, b: &Fang) -> Option<f64> {
    let path = pfad(a, b)?;
    Some(path.windows(2).map(|w| (w[1] - w[0]).length()).sum())
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
        swing: None,
        lock: Default::default(),
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
    /// Innentür im Grundriss (Knopf „Tür“ dort).
    pub innen: crate::oeffnung_app::innentuer::Innentuer,
    /// Kanten im Grundriss statt Ecken auf der Fläche (Jörn 12.10. 09:26).
    pub grundriss: bool,
    /// Nummer der Tinte der Vorschau beim Renderer, neu mit jeder anderen.
    pub tinte_nr: u64,
}

/// Was ein Klick mit [`Werkzeug::setzen`] ergibt.
#[derive(Debug, PartialEq)]
pub enum Klick {
    /// Keine Fläche unter der Maus.
    Nichts,
    /// Die erste Ecke steht.
    Erste,
    /// Die Öffnung, die das Modell annimmt.
    Oeffnung(Opening),
    /// Abgelehnt, mit Grund; die erste Ecke bleibt.
    Abgelehnt(String),
}

/// Was die Vorschau zeichnet: Höhenlinien (gestrichelt, obenauf) und den
/// Umriss der Öffnung.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bild {
    pub linien: Vec<(Vec3, Vec3)>,
    pub umriss: Vec<(Vec3, Vec3)>,
    /// Maßkette der Innentür zu den Wandflächen, gezeichnet wie die
    /// Außenmaßketten, in der Ziehfarbe: Striche mit ihrer Stärke (mm auf
    /// dem Papier) und ob rund (Kreise), dazu die Tinte (Zahlen, Punkte,
    /// Pfeile).
    pub kette: Vec<(Vec3, Vec3, f64, bool)>,
    pub tinte: Vec<[Vec3; 3]>,
    /// Pille der Innentür am Cursor: Abstand zur näheren Wandfläche.
    pub pille: Option<([Vec3; 2], String)>,
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
            innen: Default::default(),
            grundriss: false,
            tinte_nr: 0,
        }
    }

    /// Öffnung aus erster Ecke bzw. Kante `a` und der unter der Maus `b`.
    fn aus(&self, a: &Fang, b: &Fang) -> Option<Opening> {
        if self.grundriss {
            plan_oeffnung(self.kind, a, b)
        } else {
            oeffnung(self.kind, a, b)
        }
    }

    /// Wechsel zwischen Grundriss und den anderen Ansichten: Eine Ecke aus
    /// der einen gilt in der anderen nicht.
    pub fn ansicht(&mut self, grundriss: bool) {
        if self.grundriss != grundriss {
            self.grundriss = grundriss;
            self.erste = None;
        }
    }

    /// Pille am Gummiband im Grundriss: die Breite live zwischen erster
    /// Kante und Maus.
    pub fn label(&self) -> Option<([Vec3; 2], String)> {
        if !self.grundriss {
            return None;
        }
        // Innentür am Cursor: Abstand zur näheren Wandfläche (G7)
        if self.erste.is_none() && self.innen.hover.is_some() {
            return self.vorschau.pille.clone();
        }
        let (a, b) = (self.erste.as_ref()?, self.hover.as_ref()?);
        let w = plan_breite(a, b).filter(|w| *w >= 1.0)?;
        Some(([a.p, b.p], crate::ui::live_length_text(w)))
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
                let o = self.aus(&a, &h);
                if o.is_none() {
                    self.erste = Some(a);
                }
                o
            }
        }
    }

    /// Klick mit Prüfung am Modell (Review #86, H4): Lehnt das Modell die
    /// Öffnung ab oder passt die Gegenecke nicht zur ersten Ecke, bleibt die
    /// erste Ecke und der Grund kommt zurück.
    pub fn setzen(&mut self, m: &Model) -> Klick {
        let erste = self.erste.clone();
        match self.klick() {
            Some(o) => match m.opening_fault(&o, None) {
                None => Klick::Oeffnung(o),
                Some(f) => {
                    self.erste = erste;
                    Klick::Abgelehnt(f.text(m))
                }
            },
            None if erste.is_some() => {
                Klick::Abgelehnt("Gegenecke passt nicht zur ersten Ecke".into())
            }
            None if self.erste.is_some() => Klick::Erste,
            None => Klick::Nichts,
        }
    }

    /// Nach Rückgängig, Wiederherstellen oder Fokusverlust (Review #86,
    /// H5): Ecke, Fang und Vorschau zeigten auf Flächen, die es so nicht
    /// mehr gibt.
    pub fn neu(&mut self) {
        self.erste = None;
        self.hover = None;
        self.vorschau = Bild::default();
        self.innen.weg();
    }

    /// Esc: nimmt die erste Ecke zurück (`true`); ohne sie schließt das
    /// Werkzeug (`false`).
    pub fn esc(&mut self) -> bool {
        self.erste.take().is_some()
    }

    /// Vorschau; `lines` liefert die Höhenlinien einer Fläche.
    pub fn bild(&self, lines: impl Fn(&OuterFace) -> Vec<FaceLine>) -> Bild {
        let mut b = Bild::default();
        if self.grundriss {
            b.umriss = match (&self.erste, &self.hover) {
                (Some(a), Some(h)) => plan_umriss(self.kind, a, h),
                // Ohne erste Kante ein Strich quer durch die Wand
                (None, Some(h)) => vec![(h.p, h.p - h.face.n * h.dicke)],
                _ => Vec::new(),
            };
            b.punkt = self.hover.as_ref().map(|h| (h.p, h.line));
            return b;
        }
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
    // Maßkette der Innentür: Stärke wie die Außenmaßketten
    let px = theme.px_per_mm * scale;
    out.extend(b.kette.iter().map(|&(a, c, mm, round)| Helper {
        width: (mm as f32 * px).max(1.0),
        round,
        ..line(a, c, col.drag, 1.0, 0.0)
    }));
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

    /// Zur Ecke gezogen endet die Öffnung am Ende des freien Bereichs, nie an
    /// oder hinter der Ecke; das Modell nimmt sie an (Review #79).
    #[test]
    fn fang_vor_der_ecke() {
        let (mut m, eg, _) = haus();
        let f = m.outer_face(eg, 0).unwrap();
        let el = m.storey(f.storey).unwrap().elevation;
        assert!(f.free.0 > f.u.0 && f.free.1 < f.u.1, "{f:?}");
        let a = fang(&m, eg, 0, 6000.0, el + 880.0);
        for u in [f.u.1, f.u.1 + 300.0, f.free.1 - 20.0] {
            let b = fang(&m, eg, 0, u, el + 2380.0);
            assert!((f.u_of(b.p) - f.free.1).abs() < 1e-9, "{u}");
        }
        let b = fang(&m, eg, 0, f.u.1, el + 2380.0);
        let o = oeffnung(OpeningKind::Window, &a, &b).unwrap();
        assert_eq!(m.opening_fault(&o, None), None);
        assert!(m.add_opening(o).is_some());
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

    /// Review #86, H4 und H5: Eine abgelehnte Gegenecke behält die erste
    /// Ecke und nennt den Grund; `neu` setzt Ecke, Fang und Vorschau zurück.
    #[test]
    fn ablehnung_und_neu() {
        let (mut m, eg, _) = haus();
        let el = m
            .storey(m.outer_face(eg, 0).unwrap().storey)
            .unwrap()
            .elevation;
        let mut w = Werkzeug::new(OpeningKind::Window);
        assert_eq!(w.setzen(&m), Klick::Nichts);
        w.hover = Some(fang(&m, eg, 0, 3000.0, el + 875.0));
        assert_eq!(w.setzen(&m), Klick::Erste);
        w.hover = Some(fang(&m, eg, 0, 4000.0, el + 2385.0));
        let Klick::Oeffnung(o) = w.setzen(&m) else {
            panic!("Öffnung erwartet");
        };
        let nr = m
            .add_opening(o)
            .map(|id| m.element(id).unwrap().number.clone());
        // Dieselbe Stelle noch einmal: überschneidet die erste
        w.hover = Some(fang(&m, eg, 0, 3500.0, el + 875.0));
        assert_eq!(w.setzen(&m), Klick::Erste);
        w.hover = Some(fang(&m, eg, 0, 4500.0, el + 2385.0));
        assert_eq!(
            w.setzen(&m),
            Klick::Abgelehnt(format!("Öffnung überschneidet {}", nr.unwrap()))
        );
        assert!(w.erste.is_some(), "erste Ecke bleibt");
        // Gegenüberliegende Wand: keine Öffnung, die erste Ecke bleibt
        w.hover = Some(fang(&m, eg, 2, 3000.0, el + 2385.0));
        assert_eq!(
            w.setzen(&m),
            Klick::Abgelehnt("Gegenecke passt nicht zur ersten Ecke".into())
        );
        assert!(w.erste.is_some());
        w.vorschau = w.bild(|f| m.face_lines(f.storey));
        assert_ne!(w.vorschau, Bild::default());
        w.neu();
        assert!(w.erste.is_none() && w.hover.is_none());
        assert_eq!(w.vorschau, Bild::default());
    }

    /// Grundriss (Jörn 12.10. 09:26): Kante auf der Außenwand, die Breite
    /// folgt der Maus mit Maß; das Fenster bindet an Brüstung und Sturz,
    /// die Außentür unten an OK Rohdecke; das Modell nimmt beide an.
    #[test]
    fn im_grundriss_aufziehen() {
        use crate::oeffnung_app::innentuer::treffer_aussen;
        let (mut m, eg, _) = haus();
        let f = m.outer_face(eg, 0).unwrap();
        let st = f.storey;
        let z = 1000.0;
        let im_raum = f.at(3000.0, 0.0) - f.n * 2000.0;
        assert!(treffer_aussen(&m, &|r| m.chain(r), st, im_raum, 40.0).is_none());
        let p = f.at(3003.0, 0.0) - f.n * 50.0;
        let (face, _, _, dicke) = treffer_aussen(&m, &|r| m.chain(r), st, p, 40.0).unwrap();
        assert_eq!((face.run, face.seg), (eg, 0));
        assert!(dicke > 50.0);
        let a = plan_fang(&face, p, 40.0, z, dicke);
        assert!((face.u_of(a.p) - 3000.0).abs() < 1e-9 && a.p.z == z && !a.line);
        let b = plan_fang(&face, face.at(4255.0, 0.0), 40.0, z, dicke);
        let mut w = Werkzeug::new(OpeningKind::Window);
        w.ansicht(true);
        w.hover = Some(a.clone());
        assert_eq!(w.bild(|_| Vec::new()).umriss.len(), 1, "Strich quer");
        assert_eq!(w.setzen(&m), Klick::Erste);
        w.hover = Some(b);
        let (ends, text) = w.label().unwrap();
        assert_eq!(text, crate::ui::live_length_text(1260.0));
        assert_eq!(ends[0], a.p);
        let bild = w.bild(|_| Vec::new());
        assert!(bild.linien.is_empty());
        assert_eq!(bild.umriss.len(), 5, "außen, innen, Glas, zwei Laibungen");
        let Klick::Oeffnung(o) = w.setzen(&m) else {
            panic!("Fenster erwartet");
        };
        assert_eq!(o.bottom, HeightRef::Line(st, HeightLine::Sill));
        assert_eq!(o.top, HeightRef::Line(st, HeightLine::Lintel));
        assert!(m.add_opening(o).is_some());
        let mut d = Werkzeug::new(OpeningKind::Door);
        d.ansicht(true);
        d.hover = Some(plan_fang(&face, face.at(6000.0, 0.0), 40.0, z, dicke));
        assert_eq!(d.setzen(&m), Klick::Erste);
        d.hover = Some(plan_fang(&face, face.at(7010.0, 0.0), 40.0, z, dicke));
        assert_eq!(d.bild(|_| Vec::new()).umriss.len(), 4);
        let Klick::Oeffnung(o) = d.setzen(&m) else {
            panic!("Außentür erwartet");
        };
        assert_eq!(o.bottom, HeightRef::Level(LevelRef::bottom(st)));
        assert_eq!(o.top, HeightRef::Line(st, HeightLine::Lintel));
        assert!(m.add_opening(o).is_some());
        assert_eq!(m.chain(eg).unwrap().joints.holes.len(), 2);
        // Wechsel in eine andere Ansicht nimmt die erste Kante zurück
        d.hover = Some(plan_fang(&face, face.at(8000.0, 0.0), 40.0, z, dicke));
        d.klick();
        assert!(d.erste.is_some());
        d.ansicht(false);
        assert!(d.erste.is_none() && d.label().is_none());
    }

    /// Im Grundriss um die Außenecke: die Innenkante schneidet sich in
    /// Wanddicke hinter beiden Flächen, die Breite zählt beide Schenkel.
    #[test]
    fn im_grundriss_um_die_ecke() {
        let (mut m, eg, _) = haus();
        let (f0, f1) = (m.outer_face(eg, 0).unwrap(), m.outer_face(eg, 1).unwrap());
        let t = 300.0;
        let a = plan_fang(&f0, f0.at(f0.free.1 - 1000.0, 0.0), 40.0, 1000.0, t);
        let b = plan_fang(&f1, f1.at(f1.free.0 + 800.0, 0.0), 40.0, 1000.0, t);
        let path = pfad(&a, &b).unwrap();
        assert_eq!(path.len(), 3);
        let innen = versatz(&path, &a, &b, 1.0);
        let c = path[1];
        assert!(((innen[1] - c).dot(f0.n) + t).abs() < 1e-9);
        assert!(((innen[1] - c).dot(f1.n) + t).abs() < 1e-9);
        let breite = (path[1] - path[0]).length() + (path[2] - path[1]).length();
        assert!((plan_breite(&a, &b).unwrap() - breite).abs() < 1e-9);
        assert_eq!(plan_umriss(OpeningKind::Window, &a, &b).len(), 8);
        let o = plan_oeffnung(OpeningKind::Window, &a, &b).unwrap();
        assert_eq!(m.opening_fault(&o, None), None);
        assert!(m.add_opening(o).is_some());
    }
}

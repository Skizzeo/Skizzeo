//! Öffnungen im Modell (Paket O2): anlegen, ändern, Höhen auflösen und die
//! Löcher je Wandzug ableiten ([`crate::oeffnung`], [`crate::wand_loch`]).

use super::*;
use crate::hoehen::HeightLines;
use crate::oeffnung::{
    HeightLine, HeightRef, InstallDepth, Opening, ELEMENT_DEPTH, INSTALL_BEHIND_SHELL,
    INSTALL_MONO, MAX_INSTALL, MAX_REVEAL, MIN_OPENING, REVEAL_ETICS, REVEAL_FACING,
};
use crate::wall::{cross2, flat, right_of};
use crate::wand_loch::Hole;

/// So weit darf ein Pfadpunkt neben der Außenfläche liegen (mm).
const ON_FACE: f64 = 1.0;

impl Model {
    /// Absolute Höhe einer Öffnungskante (mm).
    pub fn height_z(&self, r: HeightRef) -> Option<f64> {
        match r {
            HeightRef::Line(st, line) => {
                let s = self.storey(st)?;
                Some(
                    s.elevation
                        + match line {
                            HeightLine::Sill => s.lines.sill,
                            HeightLine::Lintel => s.lines.lintel,
                        },
                )
            }
            HeightRef::Level(l) => self.level_z(l),
        }
    }

    /// Unter- und Oberkante der Öffnung (z absolut, mm); `None`, wenn eine
    /// Höhe fehlt oder die Öffnung niedriger als [`MIN_OPENING`] ist.
    pub fn opening_band(&self, o: &Opening) -> Option<(f64, f64)> {
        let (z0, z1) = (self.height_z(o.bottom)?, self.height_z(o.top)?);
        (z1 >= z0 + MIN_OPENING - 1e-6).then_some((z0, z1))
    }

    /// Die Öffnung `id`, falls es eine ist.
    pub fn opening(&self, id: ElementId) -> Option<&Opening> {
        match &self.element(id)?.kind {
            ElementKind::Opening(o) => Some(o),
            _ => None,
        }
    }

    /// Kann die Öffnung so stehen? Pfad aus 2 oder 3 Punkten, mindestens
    /// [`MIN_OPENING`] breit und hoch, an der Ecke wirklich abgeknickt,
    /// Anschläge 0 bis [`MAX_REVEAL`].
    pub fn opening_ok(&self, o: &Opening) -> bool {
        let n = o.path.len();
        let finite = o.path.iter().all(|p| p.x.is_finite() && p.y.is_finite());
        let legs = o
            .path
            .windows(2)
            .all(|w| (flat(w[1]) - flat(w[0])).length() >= 1.0);
        let bent = n != 3 || {
            let (a, b) = (o.path[1] - o.path[0], o.path[2] - o.path[1]);
            cross2(a, b).abs() > 0.1 * a.length() * b.length()
        };
        let reveal = o
            .reveal
            .is_none_or(|r| r.iter().all(|x| (0.0..=MAX_REVEAL).contains(x)));
        let install = [o.install.outer, o.install.element]
            .iter()
            .flatten()
            .all(|x| (0.0..=MAX_INSTALL).contains(x));
        (2..=3).contains(&n)
            && finite
            && legs
            && bent
            && o.width() >= MIN_OPENING - 1e-6
            && reveal
            && install
            && self.opening_band(o).is_some()
    }

    /// Legt eine Öffnung an; ihr Geschoss ist das der Unterkante. Sie muss
    /// an den Wänden stehen können ([`Model::opening_fault`]).
    pub fn add_opening(&mut self, o: Opening) -> Option<ElementId> {
        if self.opening_fault(&o, None).is_some() {
            return None;
        }
        let storey = o.bottom.storey();
        let id = self.new_element(Category::Opening, storey, WALL_SEQ, ElementKind::Opening(o));
        self.touch();
        Some(id)
    }

    /// Ändert die Öffnung `id` (Pfad, Höhen, Art, Anschlag).
    pub fn set_opening(&mut self, id: ElementId, o: Opening) -> bool {
        if self.opening(id).is_none() {
            return false;
        }
        if self.opening(id) == Some(&o) {
            return true;
        }
        if self.opening_fault(&o, Some(id)).is_some() {
            return false;
        }
        note!(self, Element, self.elements, id);
        if let Some(e) = self.elements.get_mut(id) {
            e.storey = o.bottom.storey();
            e.kind = ElementKind::Opening(o);
        }
        self.touch();
        true
    }

    /// Höhenlinien des Geschosses `st` setzen (Brüstung, Sturz).
    pub fn set_height_lines(&mut self, st: StoreyId, lines: HeightLines) -> bool {
        if !lines.valid() || self.storey(st).is_none() {
            return false;
        }
        if self.storey(st).map(|s| s.lines) == Some(lines) {
            return true;
        }
        note!(self, Storey, self.storeys, st);
        if let Some(s) = self.storeys.get_mut(st) {
            s.lines = lines;
        }
        self.touch();
        true
    }

    /// Außenschale und Anschlag nach Wandtyp: vor einem Verblender bis vor
    /// die Luftschicht bzw. Dämmung hinter ihm, mit WDVS alle Schichten vor
    /// dem Kern; monolithisch keine Außenschale (Jörn 14:15).
    pub(crate) fn opening_shell(&self, run: RunId) -> (usize, [f64; 4]) {
        let layers = self
            .run(run)
            .and_then(|r| r.segments.first())
            .and_then(|e| self.element(*e))
            .and_then(|e| e.layer_set)
            .and_then(|s| self.layer_set(s))
            .map_or(&[][..], |s| &s.layers[..]);
        let n = layers.len();
        let core = layers.iter().position(|l| l.core).unwrap_or(n);
        let outside = &layers[..core];
        if let Some(f) = outside.iter().position(|l| crate::qto::is_facing(self, l)) {
            return (f + 1, REVEAL_FACING);
        }
        if outside
            .iter()
            .any(|l| l.function == LayerFunction::Insulation)
        {
            return (core, REVEAL_ETICS);
        }
        (n, [0.0; 4])
    }

    /// Einbautiefen der Öffnung `o` im Außenwandzug `run` (Jörn 17:08):
    /// außen nach Wandtyp monolithisch [`INSTALL_MONO`], sonst
    /// [`INSTALL_BEHIND_SHELL`] hinter der Innenseite der Außenschale
    /// ([`Model::opening_shell`]); das Element [`ELEMENT_DEPTH`]; innen der
    /// Rest bis zur Rohbau-Innenseite (ohne Putz und Bekleidung innen vom
    /// Kern). Alles in der Rohbaudicke geklemmt.
    pub fn install_depth(&self, o: &Opening, run: RunId) -> InstallDepth {
        let layers = self
            .run(run)
            .and_then(|r| r.segments.first())
            .and_then(|e| self.element(*e))
            .and_then(|e| e.layer_set)
            .and_then(|s| self.layer_set(s))
            .map_or(&[][..], |s| &s.layers[..]);
        let core = layers.iter().position(|l| l.core).unwrap_or(0);
        let t: f64 = layers
            .iter()
            .enumerate()
            .filter(|(i, l)| *i <= core || l.function != LayerFunction::Finish)
            .map(|(_, l)| l.thickness)
            .sum();
        let (shell, _) = self.opening_shell(run);
        let by_wall = if shell >= layers.len() {
            INSTALL_MONO
        } else {
            layers[..shell].iter().map(|l| l.thickness).sum::<f64>() + INSTALL_BEHIND_SHELL
        };
        let element = o.install.element.unwrap_or(ELEMENT_DEPTH).clamp(0.0, t);
        let outer = o.install.outer.unwrap_or(by_wall).clamp(0.0, t - element);
        InstallDepth {
            outer,
            element,
            inner: (t - outer - element).max(0.0),
        }
    }

    /// Löcher der Öffnungen im Außenwandzug `run` mit Körper `c`: je
    /// Pfadstück, das auf der Außenfläche eines Segments liegt, ein Loch;
    /// zur Außenecke hin offen.
    pub(crate) fn holes_of(&self, run: RunId, c: &WallChain) -> Vec<Hole> {
        if !self.category_of(run).is_some_and(Category::is_outer_wall) {
            return Vec::new();
        }
        let building = self
            .run(run)
            .and_then(|r| self.storey(r.storey))
            .and_then(|s| s.building);
        let mut out = Vec::new();
        for (id, e) in self.elements.iter() {
            let ElementKind::Opening(op) = &e.kind else {
                continue;
            };
            if self.storey(e.storey).and_then(|s| s.building) != building {
                continue;
            }
            out.extend(
                self.legs_on(run, c, Some(id), op)
                    .into_iter()
                    .map(|(_, h)| h),
            );
        }
        out
    }

    /// Löcher der Öffnung `op` (Kennung `id`) im Außenwandzug `run` mit
    /// Körper `c`, je mit dem Pfadstück, aus dem sie stammen.
    fn legs_on(
        &self,
        run: RunId,
        c: &WallChain,
        id: Option<ElementId>,
        op: &Opening,
    ) -> Vec<(usize, Hole)> {
        let mut out = Vec::new();
        let Some((pts, _, dirs)) = c.layout() else {
            return out;
        };
        let (zb, zt) = (c.base, c.base + c.height);
        let Some(z) = self.opening_band(op) else {
            return out;
        };
        if z.1 <= zb + 1e-6 || z.0 >= zt - 1e-6 {
            return out;
        }
        let (shell, plan) = self.opening_shell(run);
        let mono = shell >= c.layers.len();
        let o = c.outer_offset();
        let outward = (o - c.inner_offset()).signum();
        let reach = c.thickness();
        let rev = if mono {
            [0.0; 4]
        } else {
            op.reveal.unwrap_or(plan)
        };
        let n = op.path.len();
        for k in 0..n.saturating_sub(1) {
            let (a, b) = (flat(op.path[k]), flat(op.path[k + 1]));
            // Enden an der Außenecke sind offen
            let (open_a, open_b) = (k > 0, k + 2 < n);
            let leg = b - a;
            let len = leg.length();
            for (i, &d) in dirs.iter().enumerate() {
                let nr = right_of(d);
                if cross2(leg, d).abs() > 1e-3 * len
                    || (a - pts[i] - nr * o).dot(nr).abs() > ON_FACE
                {
                    continue;
                }
                let seg = (pts[(i + 1) % pts.len()] - pts[i]).length();
                let (ua, ub) = ((a - pts[i]).dot(d), (b - pts[i]).dot(d));
                let (lo, hi) = (ua.min(ub), ua.max(ub));
                if hi < -reach || lo > seg + reach {
                    continue;
                }
                let (open_lo, open_hi) = if ua <= ub {
                    (open_a, open_b)
                } else {
                    (open_b, open_a)
                };
                // Von außen gesehen: läuft u nach rechts?
                let rechts = d.dot(right_of(nr * -outward)) > 0.0;
                out.push((
                    k,
                    Hole {
                        seg: i,
                        u: (
                            if open_lo { f64::NEG_INFINITY } else { lo },
                            if open_hi { f64::INFINITY } else { hi },
                        ),
                        z,
                        reveal_u: if rechts {
                            (rev[0], rev[1])
                        } else {
                            (rev[1], rev[0])
                        },
                        reveal_z: (rev[3], rev[2]),
                        shell: if mono { c.layers.len() } else { shell },
                        u_right: rechts,
                        opening: id,
                        install: self.install_depth(op, run),
                    },
                ));
            }
        }
        out
    }

    /// Kann die Öffnung `o` so an den Wänden stehen? `except`: die Öffnung
    /// selbst, wenn sie schon im Modell steht. Neben [`Model::opening_ok`]
    /// muss jedes Pfadstück auf der Außenfläche einer Außenwand liegen, jedes
    /// freie Ende samt Anschlag innerhalb der Wandfläche (nicht an oder hinter
    /// der Ecke), und keine andere Öffnung darf sie überschneiden (Review
    /// #79).
    pub fn opening_fault(&self, o: &Opening, except: Option<ElementId>) -> Option<OpeningFault> {
        if !self.opening_ok(o) {
            return Some(OpeningFault::Shape);
        }
        let building = self.storey(o.bottom.storey()).and_then(|s| s.building);
        let n = o.path.len();
        let mut seated = vec![false; n - 1];
        // Die freien Enden: erster und letzter Punkt
        let mut ends_ok = [false; 2];
        for run in self.runs.ids() {
            if !self.category_of(run).is_some_and(Category::is_outer_wall)
                || self
                    .run(run)
                    .and_then(|r| self.storey(r.storey))
                    .and_then(|s| s.building)
                    != building
            {
                continue;
            }
            let Some(c) = self.chain(run) else {
                continue;
            };
            let legs = self.legs_on(run, &c, None, o);
            if legs.is_empty() {
                continue;
            }
            let Some((pts, _, dirs)) = c.layout() else {
                continue;
            };
            for (k, h) in &legs {
                seated[*k] = true;
                let ((u0, u1), zs) = h.kasten(false);
                for other in &c.joints.holes {
                    let Some(oid) = other.opening else {
                        continue;
                    };
                    if other.seg != h.seg || Some(oid) == except {
                        continue;
                    }
                    let ((v0, v1), zo) = other.kasten(false);
                    if u1.min(v1) - u0.max(v0) > 1e-3 && zs.1.min(zo.1) - zs.0.max(zo.0) > 1e-3 {
                        return Some(OpeningFault::Overlap(oid));
                    }
                }
                let (a, b) = face_span(&c, &pts, &dirs, h.seg);
                let (p0, d) = (pts[h.seg], dirs[h.seg]);
                for (e, p, leg) in [(0, o.path[0], 0), (1, o.path[n - 1], n - 2)] {
                    if leg != *k {
                        continue;
                    }
                    let u = (flat(p) - p0).dot(d);
                    let ok = if (u - h.u.0).abs() < 1e-6 {
                        u - h.reveal_u.0 >= a + ON_FACE && u <= b - ON_FACE
                    } else {
                        u + h.reveal_u.1 <= b - ON_FACE && u >= a + ON_FACE
                    };
                    ends_ok[e] |= ok;
                }
            }
        }
        if seated.contains(&false) {
            return Some(OpeningFault::NoWall);
        }
        if ends_ok.contains(&false) {
            return Some(OpeningFault::AtEnd);
        }
        None
    }

    /// Außenwandzüge, die die Öffnung `o` treffen kann: ihr Grundriss
    /// kommt dem Pfad bis auf [`RUN_REACH`] nahe. Damit rechnet eine
    /// Öffnungsänderung nur diese Züge neu (Review #79).
    pub(crate) fn opening_runs(&self, o: &Opening) -> Vec<RunId> {
        let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
        for p in &o.path {
            (lo.0, lo.1, hi.0, hi.1) = (lo.0.min(p.x), lo.1.min(p.y), hi.0.max(p.x), hi.1.max(p.y));
        }
        self.runs
            .ids()
            .filter(|&r| {
                self.category_of(r).is_some_and(Category::is_outer_wall)
                    && self.run(r).is_some_and(|run| {
                        let (mut a, mut b) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
                        for p in &run.points {
                            (a.0, a.1, b.0, b.1) =
                                (a.0.min(p.x), a.1.min(p.y), b.0.max(p.x), b.1.max(p.y));
                        }
                        a.0 <= hi.0 + RUN_REACH
                            && b.0 >= lo.0 - RUN_REACH
                            && a.1 <= hi.1 + RUN_REACH
                            && b.1 >= lo.1 - RUN_REACH
                    })
            })
            .collect()
    }
}

/// So weit darf eine Außenfläche von der Bezugslinie ihres Zuges liegen
/// (mm), großzügig: [`Model::opening_runs`].
const RUN_REACH: f64 = 2000.0;

/// Warum eine Öffnung so nicht stehen kann ([`Model::opening_fault`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpeningFault {
    /// Pfad, Maße, Höhen oder Anschläge ungültig ([`Model::opening_ok`]).
    Shape,
    /// Ein Pfadstück liegt auf keiner Außenwand (Wand verschoben oder
    /// gelöscht).
    NoWall,
    /// Ein freies Ende samt Anschlag liegt an oder hinter dem Ende der
    /// Wandfläche (Ecke, Wandende).
    AtEnd,
    /// Überschneidet die Öffnung mit dieser Kennung.
    Overlap(ElementId),
}

impl OpeningFault {
    /// Befund für `check`.
    pub fn text(self, m: &Model) -> String {
        match self {
            OpeningFault::Shape => "Öffnung ungültig".into(),
            OpeningFault::NoWall => "Öffnung liegt nicht auf einer Außenwand".into(),
            OpeningFault::AtEnd => "Öffnung reicht an oder über das Ende der Wandfläche".into(),
            OpeningFault::Overlap(o) => format!(
                "Öffnung überschneidet {}",
                m.element(o)
                    .map_or("eine andere Öffnung", |e| e.number.as_str())
            ),
        }
    }
}

/// Bereich längs (u ab Segmentanfang), in dem alle Schichtflächen des
/// Segments `i` liegen: vom spätesten Anfang bis zum frühesten Ende.
fn face_span(c: &WallChain, pts: &[Vec3], dirs: &[Vec3], i: usize) -> (f64, f64) {
    let (p0, d) = (pts[i], dirs[i]);
    let (mut a, mut b) = (f64::MIN, f64::MAX);
    for (li, (lo, hi, _)) in c.layer_offsets().into_iter().enumerate() {
        if c.layers[li].air {
            continue;
        }
        for off in [lo, hi] {
            let cs = c.face_corners_in(off, Some(li));
            let j = (i + 1) % cs.len().max(1);
            if let (Some(&s), Some(&e)) = (cs.get(i), cs.get(j)) {
                a = a.max((flat(s) - p0).dot(d));
                b = b.min((flat(e) - p0).dot(d));
            }
        }
    }
    (a, b)
}

impl Model {
    /// Außenfläche von Segment `seg` des Außenwandzugs `run`.
    pub fn outer_face(&self, run: RunId, seg: usize) -> Option<crate::oeffnung::OuterFace> {
        if !self.category_of(run).is_some_and(Category::is_outer_wall) {
            return None;
        }
        let c = self.chain(run)?;
        let (pts, _, dirs) = c.layout()?;
        let d = *dirs.get(seg)?;
        let o = c.outer_offset();
        let nr = right_of(d);
        let corners = c.face_corners(o);
        let j = (seg + 1) % corners.len().max(1);
        let a = pts[seg] + nr * o;
        let u_of = |p: Vec3| (flat(p) - a).dot(d);
        // Freie Enden: wie opening_fault, mit dem größeren Anschlag seitlich
        // und etwas Luft gegen Rundung
        let (shell, plan) = self.opening_shell(run);
        let rev = if shell >= c.layers.len() {
            0.0
        } else {
            plan[0].max(plan[1])
        };
        let (s0, s1) = face_span(&c, &pts, &dirs, seg);
        let rand = rev + 2.0 * ON_FACE;
        Some(crate::oeffnung::OuterFace {
            run,
            seg,
            storey: self.run(run)?.storey,
            a,
            d,
            n: nr * (o - c.inner_offset()).signum(),
            u: (u_of(*corners.get(seg)?), u_of(*corners.get(j)?)),
            free: (s0 + rand, s1 - rand),
            z: (c.base, c.base + c.height),
        })
    }

    /// Höhenlinien Brüstung und Sturz des Geschosses `st`.
    pub fn face_lines(&self, st: StoreyId) -> Vec<crate::oeffnung::FaceLine> {
        [HeightLine::Sill, HeightLine::Lintel]
            .into_iter()
            .filter_map(|l| {
                let bind = HeightRef::Line(st, l);
                Some(crate::oeffnung::FaceLine {
                    z: self.height_z(bind)?,
                    bind,
                })
            })
            .collect()
    }
}

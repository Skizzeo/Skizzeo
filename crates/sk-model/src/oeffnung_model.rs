//! Öffnungen im Modell (Paket O2): anlegen, ändern, Höhen auflösen und die
//! Löcher je Wandzug ableiten ([`crate::oeffnung`], [`crate::wand_loch`]).

use super::*;
use crate::hoehen::HeightLines;
use crate::oeffnung::{
    DoorLeaf, FaceSide, HeightLine, HeightRef, InstallDepth, Opening, OpeningKind, Swing,
    ELEMENT_DEPTH, INSTALL_BEHIND_SHELL, INSTALL_MONO, MAX_INSTALL, MAX_REVEAL, MIN_OPENING,
    REVEAL_ETICS, REVEAL_FACING,
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
                            HeightLine::Door => s.lines.door,
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
        // Aufschlag nur an einer Tür mit geradem Pfad
        let swing = o.swing.is_none() || (o.kind == OpeningKind::Door && n == 2);
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
            && swing
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

    /// Ändert nur den Aufschlag der Tür `id` (Griff im Grundriss). Lage und
    /// Höhen bleiben, deshalb ohne die Prüfung an den Wänden (Review #99
    /// B2); ein Aufschlag gilt nur an einer Tür mit geradem Pfad.
    pub fn set_swing(&mut self, id: ElementId, s: Option<Swing>) -> bool {
        let Some(o) = self.opening(id) else {
            return false;
        };
        if o.swing == s {
            return true;
        }
        if s.is_some() && !(o.kind == OpeningKind::Door && o.path.len() == 2) {
            return false;
        }
        note!(self, Element, self.elements, id);
        if let Some(ElementKind::Opening(o)) = self.elements.get_mut(id).map(|e| &mut e.kind) {
            o.swing = s;
        }
        self.touch();
        true
    }

    /// Ändert der offene Schritt nur Aufschläge von Türen? Dann bleibt alles
    /// Abgeleitete (Randstreifen, Untersicht, Terrassen, Flachdächer), und
    /// der Abgleich beim Schließen entfällt (Review #99 B2).
    pub(crate) fn only_swings(&self) -> bool {
        let Some(open) = &self.txn else {
            return false;
        };
        !open.changes.is_empty()
            && open.changes.iter().all(|c| {
                let crate::txn::Change::Element {
                    id, old: Some(a), ..
                } = c
                else {
                    return false;
                };
                let Some(b) = self.elements.get(*id) else {
                    return false;
                };
                match (&a.kind, &b.kind) {
                    (ElementKind::Opening(x), ElementKind::Opening(y)) => {
                        let mut a = a.clone();
                        a.kind = ElementKind::Opening(Opening {
                            swing: y.swing,
                            ..x.clone()
                        });
                        a == *b
                    }
                    _ => false,
                }
            })
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
        if !self.category_of(run).is_some_and(Category::takes_openings) {
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
    pub(crate) fn legs_on(
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
                        outer: self.category_of(run).is_some_and(Category::is_outer_wall),
                        leaf: self.leaf_in(run, c, op, d, ua, ub),
                    },
                ));
            }
        }
        out
    }

    /// Kann die Öffnung `o` so an den Wänden stehen? `except`: die Öffnung
    /// selbst, wenn sie schon im Modell steht. Neben [`Model::opening_ok`]
    /// muss jedes Pfadstück auf der Außenfläche einer Wand liegen, jedes
    /// freie Ende samt Anschlag innerhalb der Wandfläche (nicht an oder hinter
    /// der Ecke), und keine andere Öffnung darf sie überschneiden (Review
    /// #79).
    pub fn opening_fault(&self, o: &Opening, except: Option<ElementId>) -> Option<OpeningFault> {
        self.opening_fault_in(o, except, &mut Koerper::default())
    }

    /// Wie [`Model::opening_fault`], mit den Körpern der Wandzüge aus
    /// `koerper` (einmal je Prüfung gebaut, Review #79 N4).
    pub(crate) fn opening_fault_in(
        &self,
        o: &Opening,
        except: Option<ElementId>,
        koerper: &mut Koerper,
    ) -> Option<OpeningFault> {
        if !self.opening_ok(o) {
            return Some(OpeningFault::Shape);
        }
        let Some(l) = self.opening_lage(o, except, koerper) else {
            return Some(OpeningFault::Shape);
        };
        if !l.seated {
            return Some(OpeningFault::NoWall);
        }
        if let Some(x) = l.overlap {
            return Some(OpeningFault::Overlap(x));
        }
        (!l.ends).then_some(OpeningFault::AtEnd)
    }

    /// Steht die Öffnung `o` auf Wänden: jedes Pfadstück in jedem Geschoss,
    /// das ihr Höhenband schneidet, auf der Außenfläche einer Wand? Nur
    /// die Wandlage, ohne Form, Enden und Überschneidung (Löschen).
    pub(crate) fn opening_on_walls(&self, o: &Opening) -> bool {
        self.opening_lage(o, None, &mut Koerper::default())
            .is_some_and(|l| l.seated)
    }

    /// Lage der Öffnung `o` an den Außenwänden ihres Gebäudes; `None` ohne
    /// Höhenband.
    /// Nur die Züge am Pfad ([`Model::opening_runs`]) zählen, damit die
    /// Prüfung nicht mit der Modellgröße wächst.
    fn opening_lage(
        &self,
        o: &Opening,
        except: Option<ElementId>,
        koerper: &mut Koerper,
    ) -> Option<Lage> {
        let z = self.opening_band(o)?;
        let building = self.storey(o.bottom.storey()).and_then(|s| s.building);
        let n = o.path.len();
        if n < 2 {
            return None;
        }
        // Jedes Geschoss, dessen Band die Öffnung schneidet, braucht je
        // Pfadstück eine Wand (Review #79, N1)
        let floors: Vec<StoreyId> = self
            .storeys
            .iter()
            .filter(|(_, s)| {
                s.building == building
                    && z.1.min(s.elevation + s.height) - z.0.max(s.elevation) > ON_FACE
            })
            .map(|(id, _)| id)
            .collect();
        let mut seat = vec![vec![false; floors.len()]; n - 1];
        // Die freien Enden (erster und letzter Punkt): in jedem Zug, in dem
        // ihr Pfadstück sitzt, innerhalb der Wandfläche
        let mut ends: [Option<bool>; 2] = [None; 2];
        let mut overlap = None;
        for run in self.opening_runs(o) {
            let Some(st) = self.run(run).map(|r| r.storey) else {
                continue;
            };
            if self.storey(st).and_then(|s| s.building) != building {
                continue;
            }
            let Some(c) = koerper.entry(run).or_insert_with(|| self.chain(run)) else {
                continue;
            };
            let legs = self.legs_on(run, c, None, o);
            if legs.is_empty() {
                continue;
            }
            let Some((pts, _, dirs)) = c.layout() else {
                continue;
            };
            let fi = floors.iter().position(|f| *f == st);
            let mut hier: [Option<bool>; 2] = [None; 2];
            for (k, h) in &legs {
                if let Some(fi) = fi {
                    seat[*k][fi] = true;
                }
                let ((u0, u1), zs) = h.kasten(false);
                for other in &c.joints.holes {
                    let Some(oid) = other.opening else {
                        continue;
                    };
                    if other.seg != h.seg || Some(oid) == except || overlap.is_some() {
                        continue;
                    }
                    let ((v0, v1), zo) = other.kasten(false);
                    if u1.min(v1) - u0.max(v0) > 1e-3 && zs.1.min(zo.1) - zs.0.max(zo.0) > 1e-3 {
                        overlap = Some(oid);
                    }
                }
                let (a, b) = face_span(c, &pts, &dirs, h.seg);
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
                    // Im selben Zug genügt ein Segment (Fuge, Typwechsel)
                    hier[e] = Some(hier[e].unwrap_or(false) || ok);
                }
            }
            for e in 0..2 {
                if let Some(x) = hier[e] {
                    ends[e] = Some(ends[e].unwrap_or(true) && x);
                }
            }
        }
        Some(Lage {
            seated: !floors.is_empty() && seat.iter().flatten().all(|x| *x),
            ends: ends.iter().all(|e| *e == Some(true)),
            overlap,
        })
    }

    /// Wandzüge mit Öffnungen ([`Category::takes_openings`]), die die
    /// Öffnung `o` treffen kann: ihr Grundriss
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
                self.category_of(r).is_some_and(Category::takes_openings)
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

/// Körper der Wandzüge, je Prüfung einmal gebaut.
pub(crate) type Koerper = std::collections::HashMap<RunId, Option<WallChain>>;

/// Lage einer Öffnung an den Wänden ([`Model::opening_fault`]).
struct Lage {
    /// Jedes Pfadstück steht in jedem geschnittenen Geschoss auf einer Wand.
    seated: bool,
    /// Beide freien Enden samt Anschlag liegen in der Wandfläche.
    ends: bool,
    /// Die erste Öffnung, die sie überschneidet.
    overlap: Option<ElementId>,
}

/// Warum eine Öffnung so nicht stehen kann ([`Model::opening_fault`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpeningFault {
    /// Pfad, Maße, Höhen oder Anschläge ungültig ([`Model::opening_ok`]).
    Shape,
    /// Ein Pfadstück liegt auf keiner Wand (Wand verschoben oder
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
            OpeningFault::NoWall => "Öffnung liegt nicht auf einer Wand".into(),
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
    /// Außenfläche von Segment `seg` des Zugs `run`; bei einer Innenwand
    /// die Fläche auf der Seite von [`WallChain::outer_offset`].
    pub fn outer_face(&self, run: RunId, seg: usize) -> Option<crate::oeffnung::OuterFace> {
        if !self.category_of(run).is_some_and(Category::takes_openings) {
            return None;
        }
        self.face_in(run, seg, &self.chain(run)?)
    }

    /// [`Model::outer_face`] im schon gebauten Körper `c` des Zugs `run`:
    /// mehrere Segmente, ein Körper (Review #99 B1).
    pub fn outer_face_in(
        &self,
        run: RunId,
        seg: usize,
        c: &WallChain,
    ) -> Option<crate::oeffnung::OuterFace> {
        if !self.category_of(run).is_some_and(Category::takes_openings) {
            return None;
        }
        self.face_in(run, seg, c)
    }

    /// Außenfläche von Segment `seg` im Körper `c` des Zugs `run`.
    fn face_in(&self, run: RunId, seg: usize, c: &WallChain) -> Option<crate::oeffnung::OuterFace> {
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
        let (s0, s1) = face_span(c, &pts, &dirs, seg);
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

    /// Lage des Türblatts der Tür `id` (Raumbuch und Grundriss, Integration
    /// 05:54): im Wandzug ihres Geschosses, den sie trifft, auf der
    /// Rohbauflucht der Seite, in die sie aufschlägt. `None` ohne Aufschlag.
    pub fn door_leaf(&self, id: ElementId) -> Option<DoorLeaf> {
        self.door_leaf_in(id, &|r| self.chain(r))
    }

    /// [`Model::door_leaf`] mit den Wandkörpern aus `chain_of`, z. B. aus dem
    /// Zwischenspeicher der Szene (Review #99 B1: [`Model::chain`] wächst
    /// mit der Zahl der Anschlüsse).
    pub fn door_leaf_in(
        &self,
        id: ElementId,
        chain_of: &dyn Fn(RunId) -> Option<WallChain>,
    ) -> Option<DoorLeaf> {
        let o = self.opening(id)?;
        let sw = o.swing?;
        let (a, b) = (flat(*o.path.first()?), flat(*o.path.last()?));
        let len = (b - a).length();
        if o.path.len() != 2 || len < 1e-9 {
            return None;
        }
        let along = (b - a) / len;
        let into = sw.side.normal(along);
        let st = self.element(id)?.storey;
        let (run, c, h) = self
            .opening_runs(o)
            .into_iter()
            .filter(|r| self.run(*r).is_some_and(|x| x.storey == st))
            .find_map(|r| {
                let c = chain_of(r)?;
                let (_, h) = self.legs_on(r, &c, Some(id), o).into_iter().next()?;
                Some((r, c, h))
            })?;
        let leaf = h.leaf?;
        let (pts, _, dirs) = c.layout()?;
        let d = dirs[h.seg];
        let at = |u: f64| pts[h.seg] + d * u + right_of(d) * leaf.off;
        Some(DoorLeaf {
            run,
            seg: h.seg,
            u: h.u,
            face: leaf.side,
            line: (at(h.u.0), at(h.u.1)),
            swing_into: into,
            hinge: at(if leaf.hinge_lo { h.u.0 } else { h.u.1 }),
        })
    }

    /// Türblatt der Öffnung `op` im Zug `run`, deren gerader Pfad auf dem
    /// Segment mit Richtung `d` von `ua` nach `ub` läuft.
    fn leaf_in(
        &self,
        run: RunId,
        c: &WallChain,
        op: &Opening,
        d: Vec3,
        ua: f64,
        ub: f64,
    ) -> Option<crate::wand_loch::Leaf> {
        let sw = op.swing.filter(|_| op.path.len() == 2)?;
        let along = flat(op.path[1]) - flat(op.path[0]);
        let into = sw.side.normal(along);
        let side = if into.dot(right_of(d)) > 0.0 {
            FaceSide::Right
        } else {
            FaceSide::Left
        };
        let hinge = if sw.hinge_at_start() { ua } else { ub };
        Some(crate::wand_loch::Leaf {
            off: self.rough_face(run, c, side),
            side,
            hinge_lo: hinge <= ua.min(ub),
        })
    }

    /// Lage der Rohbauflucht auf der Seite `side` des Zugs `run` quer zur
    /// Bezugslinie (positiv rechts): ohne Putz und Bekleidung außerhalb
    /// des Kerns.
    fn rough_face(&self, run: RunId, c: &WallChain, side: FaceSide) -> f64 {
        let layers = self
            .run(run)
            .and_then(|r| r.segments.first())
            .and_then(|e| self.element(*e))
            .and_then(|e| e.layer_set)
            .and_then(|s| self.layer_set(s))
            .map_or(&[][..], |s| &s.layers[..]);
        let offs = c.layer_offsets();
        let roh = offs.iter().enumerate().filter(|(i, _)| {
            layers
                .get(*i)
                .is_none_or(|l| l.core || l.function != LayerFunction::Finish)
        });
        let (lo, hi) = roh.fold((f64::MAX, f64::MIN), |(lo, hi), (_, x)| {
            (lo.min(x.0), hi.max(x.1))
        });
        let (lo, hi) = if lo <= hi {
            (lo, hi)
        } else {
            c.ref_side.span(c.thickness())
        };
        match side {
            FaceSide::Right => hi,
            FaceSide::Left => lo,
        }
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

/// Lage eines Pfadpunkts einer Öffnung auf der Außenfläche ihres Zugs.
#[derive(Clone, Copy, Debug)]
enum Anchor {
    /// Auf Segment `seg`, `du` längs ab dem Bezugspunkt `p` (Grundriss, Mitte
    /// des Pfads): der Bezugspunkt bleibt, wo er ist, nur quer auf die neue
    /// Außenfläche gelegt (die Nachbarwand verschiebt ihn nicht); die Breite
    /// längs der Fläche bleibt, auch wenn die Wand sich dreht (Geometrie #92).
    On(usize, Vec3, f64),
    /// Auf Segment `seg`, `du` längs ab der Außenecke `corner` (Ende einer
    /// Ecköffnung: der Schenkel behält seine Länge).
    FromCorner(usize, usize, f64),
    /// Außenecke `corner` (Index in den Ecken der Außenfläche).
    Corner(usize),
}

/// Lage einer Öffnung beim Greifen: ihr Pfad und je Pfadpunkt der Anker.
#[derive(Clone, Debug)]
struct OpeningAnchor {
    id: ElementId,
    path: Vec<Vec3>,
    at: Vec<Anchor>,
    /// Tür mit Blatt: Aufschlag beim Greifen und seine Seite zur Richtung
    /// des Wandsegments (Geometrie #99): Daraus kommt der Aufschlag nach
    /// jeder Bewegung, nicht aus der vorigen Lage.
    swing: Option<(Swing, FaceSide)>,
}

/// Richtung eines Pfads (Grundriss, nicht normiert).
fn path_dir(p: &[Vec3]) -> Vec3 {
    flat(p[p.len() - 1]) - flat(p[0])
}

/// Seite `side` zur Pfadrichtung als Seite zur Flächenrichtung `d` und
/// zurück (dieselbe Umrechnung in beide Richtungen).
fn side_to(side: FaceSide, path: &[Vec3], d: Vec3) -> FaceSide {
    if path_dir(path).dot(d) >= 0.0 {
        side
    } else {
        side.other()
    }
}

/// Ist der Zug `now` der Zug `grab` mit umgekehrter Punktfolge?
fn reversed(grab: &[Vec3], now: &[Vec3]) -> bool {
    grab.len() == now.len()
        && grab.len() > 1
        && grab
            .iter()
            .zip(now.iter().rev())
            .all(|(a, b)| (*a - *b).length() < 1e-6)
}

/// Ein Außenwandzug beim Greifen: sein Körper, der zuletzt mitgeführte und
/// die Anker der Öffnungen darauf.
#[derive(Clone, Debug)]
struct GrabWall {
    run: RunId,
    chain: WallChain,
    last: WallChain,
    anchors: Vec<OpeningAnchor>,
}

/// Lage der Öffnungen zu Beginn des offenen Schritts (Review #92 B1, B2):
/// Jede Bewegung beim Ziehen trägt die Öffnungen von hier aus neu ab, nicht
/// vom Stand der vorigen Bewegung, so hängt die Endlage nicht vom Mausweg ab.
/// Gemerkt werden nur Züge mit Öffnungen im Gebäude und in der Stapelspalte
/// des gezogenen Zugs.
#[derive(Clone, Debug, Default)]
pub(crate) struct OpeningGrab {
    roots: Vec<RunId>,
    walls: Vec<GrabWall>,
}

/// Verschiebung, wenn alle Punkte eines Zugs um denselben Vektor wandern;
/// sonst null.
fn parallel_shift(old: &[Vec3], new: &[Vec3]) -> Vec3 {
    let none = vec3(0.0, 0.0, 0.0);
    let (Some(a), Some(b)) = (old.first(), new.first()) else {
        return none;
    };
    let v = flat(*b) - flat(*a);
    let same = old.len() == new.len()
        && old
            .iter()
            .zip(new)
            .all(|(p, q)| (flat(*q) - flat(*p) - v).length() < 1e-6);
    if same {
        v
    } else {
        none
    }
}

impl Model {
    /// Die Züge, die das Ziehen von `run` mitnehmen kann: über Anschlüsse
    /// und Stapel verbunden (Gebäude und Stapelspalte).
    fn opening_scope(&self, run: RunId) -> Vec<RunId> {
        let mut out = vec![run];
        let mut k = 0;
        while k < out.len() {
            let r = out[k];
            k += 1;
            let next = self.joined_runs(r).into_iter().chain(self.runs_above(r));
            for n in next.chain(self.run_below(r)) {
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        out
    }

    /// Lage der Öffnungen um den Zug `run` vor seiner Änderung: aus dem
    /// offenen Schritt, beim ersten Ziehen dieses Zugs im Schritt ergänzt.
    /// Zurück in den Schritt mit [`Model::keep_opening_grab`]. Ändert
    /// `set_opening` eine gemerkte Öffnung im selben Schritt, springt sie
    /// beim nächsten Ziehen auf ihren Anker zurück (Review #92).
    pub(crate) fn opening_grab(&mut self, run: RunId) -> OpeningGrab {
        let mut g = self
            .txn
            .as_mut()
            .map(|t| std::mem::take(&mut t.grab))
            .unwrap_or_default();
        if g.roots.contains(&run) {
            return g;
        }
        g.roots.push(run);
        let any = self
            .elements
            .iter()
            .any(|(_, e)| matches!(e.kind, ElementKind::Opening(_)));
        if !any {
            return g;
        }
        for r in self.opening_scope(run) {
            if g.walls.iter().any(|w| w.run == r)
                || !self.category_of(r).is_some_and(Category::takes_openings)
            {
                continue;
            }
            let Some(chain) = self.base_chain(r) else {
                continue;
            };
            let skip: Vec<ElementId> = g
                .walls
                .iter()
                .flat_map(|w| w.anchors.iter().map(|a| a.id))
                .collect();
            let anchors = self.opening_anchors(r, &chain, &skip);
            if !anchors.is_empty() {
                g.walls.push(GrabWall {
                    run: r,
                    last: chain.clone(),
                    chain,
                    anchors,
                });
            }
        }
        g
    }

    /// Legt die Lage der Öffnungen zurück in den offenen Schritt.
    pub(crate) fn keep_opening_grab(&mut self, g: OpeningGrab) {
        if let Some(t) = self.txn.as_mut() {
            t.grab = g;
        }
    }

    /// Lage der Öffnungen des Geschosses von `run` auf dessen Außenflächen im
    /// Körper `c`, ohne die Öffnungen `skip`.
    fn opening_anchors(&self, run: RunId, c: &WallChain, skip: &[ElementId]) -> Vec<OpeningAnchor> {
        let mut out = Vec::new();
        let Some(st) = self.run(run).map(|r| r.storey) else {
            return out;
        };
        let segs = c.layout().map_or(0, |l| l.2.len());
        let f: Vec<_> = (0..segs).filter_map(|s| self.face_in(run, s, c)).collect();
        let corners = c.face_corners(c.outer_offset());
        let seat = |p: Vec3| {
            f.iter().position(|x| {
                let u = x.u_of(p);
                let (lo, hi) = (x.u.0.min(x.u.1), x.u.0.max(x.u.1));
                x.on_line(p) && u > lo - ON_FACE && u < hi + ON_FACE
            })
        };
        'o: for (id, e) in self.elements.iter() {
            let ElementKind::Opening(o) = &e.kind else {
                continue;
            };
            if e.storey != st || skip.contains(&id) {
                continue;
            }
            let n = o.path.len();
            // Ecköffnung: die Außenecke, die dem Eckpunkt am nächsten liegt
            let corner = (n == 3).then(|| {
                let q = flat(o.path[1]);
                (0..corners.len()).min_by(|&i, &j| {
                    (flat(corners[i]) - q)
                        .length()
                        .total_cmp(&(flat(corners[j]) - q).length())
                })
            });
            // Aufschlag zur Segmentrichtung beim Greifen
            let swing = o.swing.filter(|_| n == 2).and_then(|sw| {
                let s = seat(o.path[0])?;
                Some((sw, side_to(sw.side, &o.path, f[s].d)))
            });
            let sum = o.path.iter().fold(vec3(0.0, 0.0, 0.0), |a, p| a + flat(*p));
            let m = sum / n.max(1) as f64;
            let mut at = Vec::with_capacity(n);
            for (k, p) in o.path.iter().enumerate() {
                match corner {
                    Some(Some(j)) if k == 1 => at.push(Anchor::Corner(j)),
                    Some(Some(j)) => {
                        let Some(s) = seat(*p) else {
                            continue 'o;
                        };
                        let du = f[s].u_of(*p) - f[s].u_of(corners[j]);
                        at.push(Anchor::FromCorner(s, j, du));
                    }
                    Some(None) => continue 'o,
                    None => {
                        let Some(s) = seat(*p) else {
                            continue 'o;
                        };
                        at.push(Anchor::On(s, m, f[s].u_of(*p) - f[s].u_of(m)));
                    }
                }
            }
            out.push(OpeningAnchor {
                id,
                path: o.path.clone(),
                at,
                swing,
            });
        }
        out
    }

    /// Führt die Öffnungen nach einer Änderung der Wandzüge mit, aus ihrer
    /// Lage beim Greifen `g`: ein gerader Pfad kommt mit seiner Mitte quer
    /// auf die neue Außenfläche seines Segments und behält seine Breite, ein
    /// Eckpunkt kommt auf die neue Ecke, die Enden einer Ecköffnung behalten
    /// ihren Abstand von der Ecke. Wird der ganze Zug parallel verschoben,
    /// gehen die Öffnungen mit; steht er wieder wie beim Greifen, stehen sie
    /// es auch. Ändert sich die Segmentzahl des Zugs, bleibt die Öffnung
    /// stehen. Passt sie danach nicht mehr (Wand zu kurz), meldet es die
    /// Prüfung.
    pub(crate) fn carry_openings(&mut self, g: &mut OpeningGrab) {
        let segs = |c: &WallChain| c.layout().map_or(0, |l| l.2.len());
        for w in &mut g.walls {
            let Some(c) = self.base_chain(w.run) else {
                continue;
            };
            if c == w.last {
                continue;
            }
            let back = c == w.chain;
            if back || segs(&c) == segs(&w.chain) {
                let shift = parallel_shift(&w.chain.points, &c.points);
                let umgekehrt = reversed(&w.chain.points, &c.points);
                let corners = c.face_corners(c.outer_offset());
                for a in &w.anchors {
                    let path: Option<Vec<Vec3>> = if back {
                        Some(a.path.clone())
                    } else {
                        a.at.iter()
                            .map(|x| match *x {
                                Anchor::On(s, p, du) => self
                                    .face_in(w.run, s, &c)
                                    .map(|f| f.at(f.u_of(p + shift) + du, 0.0)),
                                Anchor::FromCorner(s, j, du) => {
                                    let f = self.face_in(w.run, s, &c)?;
                                    let q = *corners.get(j)?;
                                    Some(f.at(f.u_of(q) + du, 0.0))
                                }
                                Anchor::Corner(j) => corners.get(j).map(|p| flat(*p)),
                            })
                            .collect()
                    };
                    // Aufschlag aus dem Greifzustand: Seite zum Segment bleibt,
                    // nur bei umgekehrter Punktfolge des Zugs wechselt sie
                    // (der Raum bleibt derselbe, Integration 05:54)
                    let swing = match (&path, a.swing, a.at.first()) {
                        (_, Some((sw, _)), _) if back => Some(sw),
                        (Some(p), Some((sw, seite)), Some(&Anchor::On(s, ..))) if p.len() == 2 => {
                            self.face_in(w.run, s, &c).map(|f| {
                                let seite = if umgekehrt { seite.other() } else { seite };
                                Swing {
                                    side: side_to(seite, p, f.d),
                                    ..sw
                                }
                            })
                        }
                        _ => None,
                    };
                    self.put_carried(a.id, path, back, swing);
                }
            }
            w.last = c;
        }
    }

    /// Setzt den mitgeführten Pfad einer Öffnung, ohne die Prüfung an den
    /// Wänden: die Öffnung bleibt an ihrer Wand, auch wenn sie dort nicht
    /// mehr passt. Zurück auf die Lage beim Greifen (`back`) geht es immer.
    /// `swing` ersetzt den Aufschlag, wenn gesetzt.
    fn put_carried(
        &mut self,
        id: ElementId,
        path: Option<Vec<Vec3>>,
        back: bool,
        swing: Option<Swing>,
    ) {
        let (Some(path), Some(o)) = (path, self.opening(id)) else {
            return;
        };
        let near = path.len() == o.path.len()
            && path
                .iter()
                .zip(&o.path)
                .all(|(p, q)| (*p - *q).length() < 1e-6);
        let swing = swing.or(o.swing);
        let same = (path == o.path || (near && !back)) && swing == o.swing;
        let mut o = o.clone();
        o.path = path;
        o.swing = swing;
        if !same && (back || self.opening_ok(&o)) {
            note!(self, Element, self.elements, id);
            if let Some(e) = self.elements.get_mut(id) {
                e.kind = ElementKind::Opening(o);
            }
            self.touch();
        }
    }

    /// Außenwandzüge, deren Öffnungen der offene Schritt gemerkt hat.
    #[cfg(test)]
    pub(crate) fn grabbed_walls(&self) -> Vec<RunId> {
        self.txn
            .as_ref()
            .map_or_else(Vec::new, |t| t.grab.walls.iter().map(|w| w.run).collect())
    }
}

/// `[opening]`-Zeile der Datei, die nicht zählt (Regel 72): roh, mit den
/// Geschossen, auf die sie zeigt, und ihrer Nummer, die belegt bleibt
/// (Review #86 B3).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RawOpening {
    pub line: String,
    pub storeys: Vec<Guid>,
    pub number: Option<String>,
}

impl Model {
    /// Die rohen `[opening]`-Zeilen, die beim Speichern bleiben: ohne die,
    /// deren Geschoss inzwischen gelöscht ist. Sie zeigten sonst ins Leere,
    /// und die nächste Fassung lehnte die Datei ab.
    fn raw_openings(&self) -> impl Iterator<Item = &RawOpening> {
        self.opening_raw.iter().filter(|r| {
            r.storeys
                .iter()
                .all(|g| self.storeys().iter().any(|(_, s)| s.guid == *g))
        })
    }

    /// Rohe `[opening]`-Zeilen, die beim Speichern bleiben.
    pub(crate) fn opening_raw(&self) -> impl Iterator<Item = &str> {
        self.raw_openings().map(|r| r.line.as_str())
    }

    /// Trägt eine rohe `[opening]`-Zeile die Nummer `n`?
    pub(crate) fn raw_number_taken(&self, n: &str) -> bool {
        self.raw_openings().any(|r| r.number.as_deref() == Some(n))
    }

    pub(crate) fn load_opening_raw(&mut self, raw: Vec<RawOpening>) {
        self.opening_raw = raw;
    }
}

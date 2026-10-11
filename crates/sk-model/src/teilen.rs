//! Wände teilen und vereinen (W-11, W-12, Jörn 11.10.): Methoden von
//! `Model`, aus `model.rs` ausgelagert.
//!
//! Eine Wand wird an einem oder zwei Punkten geteilt; dort entsteht je eine
//! Fuge ([`crate::fuge`]). Zwei Wände an einer Fuge lassen sich wieder zu
//! einer vereinen. Beides gilt für den ganzen Stapel: Gestapelte Wände (OG,
//! Aufkantung) stehen Segment für Segment auf denen darunter (Regel 28),
//! also wird der Zug ganz unten geteilt bzw. vereint und die Züge darüber
//! folgen. Die Wand mit der längeren Strecke behält Nummer und Kennung
//! (Regel 11).

use super::*;
use crate::fuge::same_dir;

/// Warum Teilen oder Vereinen nicht geht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FugeError {
    /// Kein Segment eines Wandzugs.
    NoWall,
    /// Ein Teilstück würde kürzer als [`crate::fuge::MIN_PIECE`].
    TooShort,
    /// Die beiden Wände stoßen nicht in einer Flucht aneinander.
    NotJoined,
    /// Darüber oder darunter springt eine Wand an der Fuge vor oder zurück.
    Steps,
    /// Die beiden Wände haben verschiedene Typen.
    Types,
    /// Der Stapel würde ungültig (Wandrest zu kurz, Umriss kreuzt sich).
    Invalid,
}

impl FugeError {
    /// Satz für die Hinweiskarte.
    pub fn message(self) -> &'static str {
        match self {
            FugeError::NoWall => "Teilen und Vereinen geht nur an Wänden.",
            FugeError::TooShort => "Jedes Teilstück braucht mindestens 10 cm.",
            FugeError::NotJoined => "Die Wände stoßen nicht in einer Flucht aneinander.",
            FugeError::Steps => {
                "Ein Geschoss springt an dieser Fuge vor oder zurück; erst bündig setzen."
            }
            FugeError::Types => "Die Wände haben verschiedene Typen; sie bleiben getrennt.",
            FugeError::Invalid => {
                "Das geht hier nicht: Ein Wandstück würde zu kurz oder der Grundriss ungültig."
            }
        }
    }
}

impl Model {
    /// Unterste Wand des Stapels unter `wall` (über alle Kopplungen,
    /// gekoppelt oder gelöst); `wall` selbst, wenn sie auf nichts steht.
    pub fn stack_bottom(&self, wall: ElementId) -> ElementId {
        let mut at = wall;
        while let Some(b) = self.wall_below(at) {
            if b == wall {
                break;
            }
            at = b;
        }
        at
    }

    /// Fugen des Zuges `run`: je Fuge die Wand davor und die danach.
    pub fn straight_joints(&self, run: RunId) -> Vec<(ElementId, ElementId)> {
        let (Some(c), Some(r)) = (self.base_chain(run), self.run(run)) else {
            return Vec::new();
        };
        let n = r.segments.len();
        c.straight_joints()
            .into_iter()
            .filter_map(|k| Some((*r.segments.get((k + n - 1) % n)?, *r.segments.get(k)?)))
            .collect()
    }

    /// Teilt die Wand `wall` an den Abständen `at` (mm ab ihrem Anfangspunkt
    /// auf der Bezugslinie, ein oder zwei Werte) samt dem Stapel, auf dem sie
    /// steht, und dem, was darüber steht. Liefert die geänderten Züge;
    /// ändert nichts, wenn es abgelehnt wird.
    pub fn split_wall(&mut self, wall: ElementId, at: &[f64]) -> Result<Vec<RunId>, FugeError> {
        let (run, pts) = self.split_points(wall, at)?;
        self.set_run_points(run, &pts).ok_or(FugeError::Invalid)
    }

    /// Ginge [`Model::split_wall`]? Ändert nichts.
    pub fn can_split(&self, wall: ElementId, at: &[f64]) -> Result<(), FugeError> {
        let (run, pts) = self.split_points(wall, at)?;
        if self.stack_fits(run, &pts) {
            Ok(())
        } else {
            Err(FugeError::Invalid)
        }
    }

    /// Zug ganz unten im Stapel von `wall` und seine Punkte mit den
    /// Teilungspunkten.
    fn split_points(&self, wall: ElementId, at: &[f64]) -> Result<(RunId, Vec<Vec3>), FugeError> {
        let (run, seg) = self.segment_of(wall).ok_or(FugeError::NoWall)?;
        let (p0, dir) = self
            .base_chain(run)
            .and_then(|c| c.segment_frame(seg))
            .ok_or(FugeError::NoWall)?;
        let bottom = self.stack_bottom(wall);
        let (brun, bseg) = self.segment_of(bottom).ok_or(FugeError::NoWall)?;
        let chain = self.base_chain(brun).ok_or(FugeError::NoWall)?;
        let (b0, bdir) = chain.segment_frame(bseg).ok_or(FugeError::NoWall)?;
        if !same_dir(dir, bdir) {
            return Err(FugeError::Invalid);
        }
        // Gleiche Stelle längs der Flucht im Zug ganz unten
        let ts: Vec<f64> = at.iter().map(|t| (p0 + dir * *t - b0).dot(bdir)).collect();
        let c = chain.with_splits(bseg, &ts).ok_or(FugeError::TooShort)?;
        Ok((brun, c.points))
    }

    /// Vereint die Wände `a` und `b`, die an einer Fuge aneinanderstoßen, zu
    /// einer, samt dem Stapel darunter und darüber. Liefert die geänderten
    /// Züge; ändert nichts, wenn es abgelehnt wird.
    pub fn merge_walls(&mut self, a: ElementId, b: ElementId) -> Result<Vec<RunId>, FugeError> {
        let (run, pts) = self.merge_points(a, b)?;
        self.set_run_points(run, &pts).ok_or(FugeError::Invalid)
    }

    /// Ginge [`Model::merge_walls`]? Ändert nichts.
    pub fn can_merge(&self, a: ElementId, b: ElementId) -> Result<(), FugeError> {
        let (run, pts) = self.merge_points(a, b)?;
        if self.stack_fits(run, &pts) {
            Ok(())
        } else {
            Err(FugeError::Invalid)
        }
    }

    /// Zug ganz unten im Stapel und seine Punkte ohne die Fuge zwischen `a`
    /// und `b`. Jede Ebene des Stapels muss dort in einer Flucht liegen.
    fn merge_points(&self, a: ElementId, b: ElementId) -> Result<(RunId, Vec<Vec3>), FugeError> {
        // In der eigenen Ebene: `a` und `b` an einer Fuge, `a` davor
        let (first, second) = self.joint_order(a, b).ok_or(FugeError::NotJoined)?;
        let (ba, bb) = (self.stack_bottom(first), self.stack_bottom(second));
        let (run, _) = self.segment_of(ba).ok_or(FugeError::NoWall)?;
        let (_, seg) = self.segment_of(bb).ok_or(FugeError::NoWall)?;
        // Ganz unten ebenso, sonst springt ein Geschoss dort
        if self.joint_order(ba, bb) != Some((ba, bb)) {
            return Err(FugeError::Steps);
        }
        let merged = self
            .base_chain(run)
            .and_then(|c| c.without_joint(seg))
            .ok_or(FugeError::Steps)?;
        // Jede Ebene darüber: die beiden Wände dort ebenfalls an einer Fuge,
        // gleicher Typ
        let mut pair = (ba, bb);
        loop {
            let set = |w: ElementId| self.element(w).and_then(|e| e.layer_set);
            if set(pair.0) != set(pair.1) {
                return Err(FugeError::Types);
            }
            let Some(up) = self.wall_above(pair.0).zip(self.wall_above(pair.1)) else {
                break;
            };
            if self.joint_order(up.0, up.1) != Some(up) {
                return Err(FugeError::Steps);
            }
            pair = up;
        }
        Ok((run, merged.points))
    }

    /// Vereint `wall` mit den Wänden, die an ihren Enden in einer Flucht an
    /// sie stoßen (nach dem Zusammenschieben eine oder zwei Fugen). Liefert,
    /// wie viele Fugen wegfielen; scheitert erst die zweite, bleibt die erste
    /// vereint.
    pub fn merge_around(&mut self, wall: ElementId) -> Result<usize, FugeError> {
        let (run, seg) = self.segment_of(wall).ok_or(FugeError::NoWall)?;
        let c = self.base_chain(run).ok_or(FugeError::NoWall)?;
        let pts = c.clean_points();
        let mid = (pts[seg] + pts[(seg + 1) % pts.len()]) * 0.5;
        let (mut done, mut w) = (0, wall);
        loop {
            // Erst mit dem längeren Nachbarn: so behält am Ende das längste
            // Stück Nummer und Kennung (Regel 11)
            let len = |x: ElementId| {
                let (_, k) = self.segment_of(x)?;
                let c = self.base_chain(run)?;
                let p = c.clean_points();
                Some((p[(k + 1) % p.len()] - p[k]).length())
            };
            let joints = self.straight_joints(run);
            let Some((a, b)) =
                joints
                    .into_iter()
                    .filter(|j| j.0 == w || j.1 == w)
                    .max_by(|x, y| {
                        let other = |j: &(ElementId, ElementId)| if j.0 == w { j.1 } else { j.0 };
                        len(other(x))
                            .partial_cmp(&len(other(y)))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            else {
                break;
            };
            match self.merge_walls(a, b) {
                Ok(_) => done += 1,
                Err(e) if done == 0 => return Err(e),
                Err(_) => break,
            }
            match self.wall_near(run, mid) {
                Some(n) => w = n,
                None => break,
            }
        }
        if done == 0 {
            Err(FugeError::NotJoined)
        } else {
            Ok(done)
        }
    }

    /// Wand des Zuges `run`, deren Bezugslinie `p` am nächsten liegt.
    pub fn wall_near(&self, run: RunId, p: Vec3) -> Option<ElementId> {
        let c = self.base_chain(run)?;
        let pts = c.clean_points();
        let n = pts.len();
        let d = |k: usize| {
            let (a, b) = (pts[k], pts[(k + 1) % n]);
            sk_math::dist_to_segment((p.x, p.y), (a.x, a.y), (b.x, b.y))
        };
        let k = (0..c.segment_count()).min_by(|&x, &y| d(x).total_cmp(&d(y)))?;
        self.wall_at(run, k)
    }

    /// `a` und `b` stoßen im selben Zug an einer Fuge aneinander: in
    /// Laufrichtung (davor, danach).
    fn joint_order(&self, a: ElementId, b: ElementId) -> Option<(ElementId, ElementId)> {
        let (ra, _) = self.segment_of(a)?;
        let (rb, _) = self.segment_of(b)?;
        if ra != rb {
            return None;
        }
        self.straight_joints(ra)
            .into_iter()
            .find(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a))
    }

    /// Alle Wände des Stapels, in dem `wall` steht, von unten nach oben:
    /// Teilen und Vereinen ändern sie alle, eine Sperre an einer hält es auf
    /// (Review 61-3).
    pub fn stack_column(&self, wall: ElementId) -> Vec<ElementId> {
        let mut out = vec![self.stack_bottom(wall)];
        while let Some(up) = out.last().and_then(|w| self.wall_above(*w)) {
            if out.contains(&up) {
                break;
            }
            out.push(up);
        }
        out
    }

    /// Wand, die gestapelt auf `wall` steht (OG über EG).
    pub fn wall_above(&self, wall: ElementId) -> Option<ElementId> {
        let (run, _) = self.segment_of(wall)?;
        self.runs_above(run)
            .into_iter()
            .flat_map(|up| self.run(up).map(|r| r.segments.clone()).unwrap_or_default())
            .find(|w| self.wall_below(*w) == Some(wall))
    }
}

/// Welche alte Wand welches neue Segment trägt, wenn ein Zug neue Punkte
/// bekommt (Regel 11). Gleiche Zahl und je Index gleiche Richtung: Index
/// auf Index (Ziehen ohne Stufenwechsel, Punkt längs verschoben). Sonst
/// nach Lage ([`match_walls`]); bei gleicher Zahl bekommen gekippte
/// Segmente (Punkt quer verschoben) danach ihren Index, wenn die
/// Reihenfolge bleibt. Fällt links eine Stufe weg und entsteht rechts eine
/// (Review 61-1), wandert so keine Kennung auf die falsche Wand.
pub(super) fn assign_walls(
    old: &[(Vec3, Vec3)],
    new: &[(Vec3, Vec3)],
    closed: bool,
) -> Vec<Option<usize>> {
    let dir = |&(a, b): &(Vec3, Vec3)| ((b - a).length() > 1e-9).then(|| (b - a).normalized());
    let same = old.len() == new.len()
        && old
            .iter()
            .zip(new)
            .all(|(o, n)| dir(o).zip(dir(n)).is_some_and(|(x, y)| same_dir(x, y)));
    if same {
        return (0..new.len()).map(Some).collect();
    }
    let mut out = match_walls(old, new, closed);
    if old.len() == new.len() {
        for k in 0..new.len() {
            if out[k].is_none() && !out.contains(&Some(k)) {
                out[k] = Some(k);
                if !in_order(&out, closed) {
                    out[k] = None;
                }
            }
        }
    }
    out
}

/// Bleibt die Reihenfolge der zugeordneten alten Segmente längs des neuen
/// Zuges erhalten (geschlossen: zyklisch)?
fn in_order(out: &[Option<usize>], closed: bool) -> bool {
    let ks: Vec<usize> = out.iter().flatten().copied().collect();
    let mut down = ks.windows(2).filter(|w| w[1] < w[0]).count();
    if closed && ks.len() > 1 && ks[0] < ks[ks.len() - 1] {
        // Der Sprung vom letzten zum ersten zählt mit
        down += 1;
    }
    down <= usize::from(closed)
}

/// Ordnet jedem neuen Segment höchstens ein altes zu (Regel 11): zuerst die
/// auf derselben Linie in gleicher Richtung, längste Überdeckung zuerst
/// ([`match_segments`]); dann die parallel verschobenen (gleiche Richtung,
/// überlappend längs der Flucht, nächste Linie zuerst), etwa das gezogene
/// Teilstück, wenn dabei eine Stufe entsteht oder wegfällt. Eine
/// Zuordnung, die die Reihenfolge längs des Zuges bräche, gilt nicht: eine
/// weggefallene Stufe wandert nicht an die Stelle einer neuen.
pub(super) fn match_walls(
    old: &[(Vec3, Vec3)],
    new: &[(Vec3, Vec3)],
    closed: bool,
) -> Vec<Option<usize>> {
    let mut out = match_segments(old, new);
    let mut used = vec![false; old.len()];
    for k in out.iter().flatten() {
        used[*k] = true;
    }
    let mut pairs: Vec<(f64, f64, usize, usize)> = Vec::new();
    for (i, &(a, b)) in new.iter().enumerate() {
        if out[i].is_some() || (b - a).length() < 1e-9 {
            continue;
        }
        let dn = (b - a).normalized();
        for (k, &(c, d)) in old.iter().enumerate() {
            let len = (d - c).length();
            if used[k] || len < 1e-9 {
                continue;
            }
            let dir = (d - c) * (1.0 / len);
            if !same_dir(dn, dir) {
                continue;
            }
            let (s0, s1) = ((a - c).dot(dir), (b - c).dot(dir));
            let overlap = s1.min(len) - s0.max(0.0);
            if overlap > 1.0 {
                let side = vec3(-dir.y, dir.x, 0.0);
                pairs.push(((a - c).dot(side).abs(), -overlap, i, k));
            }
        }
    }
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
    for (_, _, i, k) in pairs {
        if out[i].is_none() && !used[k] {
            out[i] = Some(k);
            if in_order(&out, closed) {
                used[k] = true;
            } else {
                out[i] = None;
            }
        }
    }
    out
}

/// Vorlage für ein neues Segment `i` ohne Zuordnung: das alte Segment, mit
/// dem es auf einer Linie am meisten überdeckt (ein Teilstück), sonst der
/// Nachbar davor bzw. danach im neuen Zug (eine Stufe).
pub(super) fn template_of(
    old: &[(Vec3, Vec3)],
    new: &[(Vec3, Vec3)],
    matched: &[Option<usize>],
    i: usize,
) -> Option<usize> {
    let (a, b) = new[i];
    let mut best: Option<(f64, usize)> = None;
    if (b - a).length() > 1e-9 {
        let dn = (b - a).normalized();
        for (k, &(c, d)) in old.iter().enumerate() {
            let len = (d - c).length();
            if len < 1e-9 {
                continue;
            }
            let dir = (d - c) * (1.0 / len);
            let side = vec3(-dir.y, dir.x, 0.0);
            if !same_dir(dn, dir) || (a - c).dot(side).abs() > 1.0 {
                continue;
            }
            let overlap = (b - c).dot(dir).min(len) - (a - c).dot(dir).max(0.0);
            if overlap > 1.0 && best.is_none_or(|(o, _)| overlap > o) {
                best = Some((overlap, k));
            }
        }
    }
    let n = new.len();
    best.map(|(_, k)| k)
        .or_else(|| (1..n).find_map(|s| matched[(i + n - s) % n].or(matched[(i + s) % n])))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn haus(points: &[Vec3]) -> (Model, RunId, RunId) {
        let mut m = Model::with_seed(111);
        let b = m.add_building(2);
        let eg = m.build_from_polygon(b, points).unwrap();
        let og = m.runs_above(eg)[0];
        (m, eg, og)
    }

    fn rechteck() -> (Model, RunId, RunId) {
        haus(&[
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ])
    }

    /// Zieht Segment `seg` wie am Gummiband um `out` nach außen.
    fn ziehen(m: &mut Model, run: RunId, seg: usize, out: f64) -> bool {
        let c = m.chain(run).unwrap();
        let Some(c) = c.with_segment_moved_snap(seg, c.outward_sign() * out, MIN_OFFSET) else {
            return false;
        };
        m.set_run_points(run, &c.points).is_some()
    }

    fn pruefen(m: &mut Model) {
        m.sync_soffits();
        m.sync_terraces();
        let c = m.check();
        assert!(c.is_empty(), "{c:?}");
    }

    fn nummer(m: &Model, w: ElementId) -> String {
        m.element(w).unwrap().number.clone()
    }

    fn zahl(m: &Model, run: RunId) -> usize {
        m.run(run).unwrap().segments.len()
    }

    /// Nordwand im OG teilen: EG und OG bekommen beide Fugen, Wand über
    /// Wand; das längste Stück behält die Nummer.
    #[test]
    fn teilen_gilt_fuer_den_ganzen_stapel() {
        let (mut m, eg, og) = rechteck();
        let nord_og = m.wall_at(og, 1).unwrap();
        let nord_eg = m.wall_at(eg, 1).unwrap();
        let (n_og, n_eg) = (nummer(&m, nord_og), nummer(&m, nord_eg));
        assert_eq!(m.can_split(nord_og, &[2000.0, 7000.0]), Ok(()));
        assert!(m.split_wall(nord_og, &[2000.0, 7000.0]).is_ok());
        assert_eq!((zahl(&m, eg), zahl(&m, og)), (6, 6));
        assert_eq!(m.straight_joints(eg).len(), 2);
        assert_eq!(m.straight_joints(og).len(), 2);
        // Mittelstück (5 m) behält die Nummer, in beiden Geschossen
        assert_eq!(m.wall_at(og, 2), Some(nord_og));
        assert_eq!(m.wall_at(eg, 2), Some(nord_eg));
        assert_eq!((nummer(&m, nord_og), nummer(&m, nord_eg)), (n_og, n_eg));
        for k in 0..6 {
            let up = m.wall_at(og, k).unwrap();
            assert_eq!(m.wall_below(up), m.wall_at(eg, k), "Segment {k}");
            assert_eq!(m.stack_offset(up), Some((0.0, true)));
        }
        // Gleicher Typ wie die geteilte Wand
        let set = |w| m.element(w).unwrap().layer_set;
        assert_eq!(set(m.wall_at(eg, 1).unwrap()), set(nord_eg));
        pruefen(&mut m);
        // Zu kurze Stücke: nichts ändert sich
        let w = m.wall_at(eg, 1).unwrap();
        assert_eq!(m.split_wall(w, &[50.0]), Err(FugeError::TooShort));
        assert_eq!(zahl(&m, eg), 6);
    }

    /// Mittelstück rausziehen: zwei Stufen, das OG folgt, das Stück behält
    /// seine Nummer; zurückschieben lässt zwei Fugen, Vereinen macht das
    /// Rechteck wieder ganz.
    #[test]
    fn mittelstueck_raus_und_zurueck() {
        let (mut m, eg, og) = rechteck();
        let nord = m.wall_at(eg, 1).unwrap();
        let n = nummer(&m, nord);
        m.split_wall(nord, &[3000.0, 6000.0]).unwrap();
        let mitte = m.wall_at(eg, 2).unwrap();
        let n_mitte = nummer(&m, mitte);
        assert!(ziehen(&mut m, eg, 2, 1500.0));
        assert_eq!((zahl(&m, eg), zahl(&m, og)), (8, 8));
        assert_eq!(m.wall_at(eg, 3), Some(mitte), "das gezogene Stück bleibt");
        assert_eq!(nummer(&m, mitte), n_mitte);
        let p = &m.run(og).unwrap().points;
        assert!(p.iter().any(|q| (q.y - 9500.0).abs() < 1e-6), "{p:?}");
        pruefen(&mut m);
        // Zurück: wieder zwei Fugen, die Stufen sind weg
        assert!(ziehen(&mut m, eg, 3, -1500.0));
        assert_eq!((zahl(&m, eg), zahl(&m, og)), (6, 6));
        assert_eq!(m.wall_at(eg, 2), Some(mitte));
        assert_eq!(m.straight_joints(eg).len(), 2);
        pruefen(&mut m);
        // Vereinen: erst rechts, dann links
        while let Some(&(a, b)) = m.straight_joints(eg).last() {
            assert_eq!(m.can_merge(a, b), Ok(()));
            m.merge_walls(a, b).unwrap();
            pruefen(&mut m);
        }
        assert_eq!((zahl(&m, eg), zahl(&m, og)), (4, 4));
        assert!(m.straight_joints(eg).is_empty());
        // Die längste Wand (rechts 4 m, dann 7 m) hat ihre Nummer behalten
        assert_eq!(m.wall_at(eg, 1), Some(nord));
        assert_eq!(nummer(&m, nord), n);
    }

    /// Teilen ändert keine Menge: Länge, Flächen und Volumen der Wände
    /// bleiben in Summe gleich (Mengen und Kosten zählen je Wand).
    #[test]
    fn teilen_laesst_die_mengen_gleich() {
        let summe = |m: &Model, runs: [RunId; 2]| {
            let mut s = [0.0; 6];
            for run in runs {
                for q in crate::qto::run_qto(m, run) {
                    for (x, v) in s.iter_mut().zip([
                        q.length,
                        q.side_outer,
                        q.side_inner,
                        q.volume,
                        q.footprint,
                        q.layers.iter().map(|l| l.area).sum(),
                    ]) {
                        *x += v;
                    }
                }
            }
            s
        };
        // Auch ein Eckstück kürzer als die Wand dick ist (Review 61-2)
        for (seg, at) in [
            (1, vec![3000.0, 6000.0]),
            (0, vec![7900.0]),
            (0, vec![100.0]),
        ] {
            let (mut m, eg, og) = rechteck();
            let vorher = summe(&m, [eg, og]);
            let w = m.wall_at(eg, seg).unwrap();
            m.split_wall(w, &at).unwrap();
            pruefen(&mut m);
            let nachher = summe(&m, [eg, og]);
            for (a, b) in vorher.iter().zip(&nachher) {
                assert!(
                    (a - b).abs() <= a.abs() * 1e-9 + 1e-3,
                    "{at:?}: {vorher:?} {nachher:?}"
                );
            }
        }
    }

    /// Teilen unter bzw. an einer versetzten OG-Wand (Dachterrasse, Erker):
    /// Alle Teilstücke oben behalten den Versatz, auch das zwei Fugen weit
    /// vom Stück mit der alten Nummer (Review Geometrie #61).
    #[test]
    fn teilen_mit_versatz_im_og() {
        for off in [-1500.0, 600.0] {
            let (mut m, eg, og) = rechteck();
            let w = m.wall_at(og, 1).unwrap();
            assert!(m.set_offset(w, off).is_some());
            pruefen(&mut m);
            for wand in [m.wall_at(eg, 1).unwrap(), w] {
                let mut p = m.clone();
                assert_eq!(p.can_split(wand, &[3000.0, 6000.0]), Ok(()), "{off}");
                p.split_wall(wand, &[3000.0, 6000.0]).unwrap();
                assert_eq!((zahl(&p, eg), zahl(&p, og)), (6, 6));
                for k in 1..4 {
                    let up = p.wall_at(og, k).unwrap();
                    assert_eq!(p.stack_offset(up), Some((off, true)), "{off} Stück {k}");
                }
                pruefen(&mut p);
            }
        }
    }

    /// Kosten #61: Ein Eckstück von 100 mm an der Wand unter einer
    /// Dachterrasse lässt Terrasse, Attika und Blech unverändert.
    #[test]
    fn eckstueck_an_der_terrasse() {
        let plan = |m: &Model, eg: RunId| {
            let Some(Ok(f)) = m.floor(eg) else {
                panic!("Decke")
            };
            let t = &f.terraces;
            let attika: f64 = t
                .attika
                .iter()
                .map(|p| sk_math::polygon::signed_area(&p.quad))
                .sum();
            (t.area(), attika.abs(), t.coping_length())
        };
        for at in [vec![100.0], vec![9900.0], vec![100.0, 9900.0]] {
            let (mut m, eg, og) = rechteck();
            let nord = m.wall_at(og, 1).unwrap();
            assert!(m.set_offset(nord, -1500.0).is_some());
            pruefen(&mut m);
            let vorher = plan(&m, eg);
            assert!(vorher.0 > 0.0);
            m.split_wall(m.wall_at(eg, 1).unwrap(), &at).unwrap();
            pruefen(&mut m);
            let nachher = plan(&m, eg);
            for (a, b) in [
                (vorher.0, nachher.0),
                (vorher.1, nachher.1),
                (vorher.2, nachher.2),
            ] {
                assert!((a - b).abs() < 1e-3, "{at:?}: {vorher:?} {nachher:?}");
            }
        }
    }

    /// Nach dem Zurückschieben vereint ein Schritt am Mittelstück beide
    /// Fugen; ohne Fuge daneben abgelehnt.
    #[test]
    fn vereinen_um_das_mittelstueck() {
        let (mut m, eg, og) = rechteck();
        let nord = m.wall_at(eg, 1).unwrap();
        m.split_wall(nord, &[3000.0, 6000.0]).unwrap();
        let mitte = m.wall_at(eg, 2).unwrap();
        assert!(ziehen(&mut m, eg, 2, -1200.0));
        assert_eq!(m.merge_around(mitte), Err(FugeError::NotJoined));
        assert!(ziehen(&mut m, eg, 3, 1200.0));
        assert_eq!(m.merge_around(mitte), Ok(2));
        assert_eq!((zahl(&m, eg), zahl(&m, og)), (4, 4));
        assert_eq!(m.wall_at(eg, 1), Some(nord));
        pruefen(&mut m);
    }

    /// Review 61-1: Rechter Teil des L geteilt, das linke Stück rückt in die
    /// Flucht. Links fällt die Stufe weg, rechts entsteht eine; die Zahl
    /// bleibt 7. Das Stück behält seine Kennung auf Segment 2, die alte
    /// Stufe ist weg, die neue ist eine neue Wand; die Fuge ist da.
    #[test]
    fn stufe_wechselt_die_seite_bei_gleicher_zahl() {
        let (mut m, eg, og) = haus(&[
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(4000.0, 8000.0, 0.0),
            vec3(4000.0, 6000.0, 0.0),
            vec3(10000.0, 6000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ]);
        let stufe = m.wall_at(eg, 2).unwrap();
        let rechts = m.wall_at(eg, 3).unwrap();
        m.split_wall(rechts, &[3000.0]).unwrap();
        assert_eq!((zahl(&m, eg), zahl(&m, og)), (7, 7));
        let r1 = m.wall_at(eg, 3).unwrap();
        let r2 = m.wall_at(eg, 4).unwrap();
        let typ = m.element(r1).unwrap().layer_set;
        let n1 = nummer(&m, r1);
        assert!(ziehen(&mut m, eg, 3, 2000.0));
        assert_eq!((zahl(&m, eg), zahl(&m, og)), (7, 7));
        assert_eq!(m.wall_at(eg, 2), Some(r1), "Stück auf Segment 2");
        assert_eq!(nummer(&m, r1), n1);
        assert_eq!(m.element(r1).unwrap().layer_set, typ);
        assert!(m.element(stufe).is_none(), "alte Stufe weg");
        let neu = m.wall_at(eg, 3).unwrap();
        assert!(neu != stufe && neu != r1 && neu != r2);
        assert_eq!(m.wall_at(eg, 4), Some(r2));
        assert!(m
            .straight_joints(eg)
            .contains(&(m.wall_at(eg, 1).unwrap(), r1)));
        for k in 0..7 {
            let up = m.wall_at(og, k).unwrap();
            assert_eq!(m.wall_below(up), m.wall_at(eg, k), "Segment {k}");
        }
        pruefen(&mut m);
    }

    /// L zusammenschieben (Jörn 11.10., Tagesthema 1): Der rechte Teil der
    /// Nordwand rückt in die Flucht des linken. Knapp davor rastet er ein,
    /// die Stufe fällt in beiden Geschossen weg, eine Fuge bleibt; Vereinen
    /// macht eine Wand daraus.
    #[test]
    fn l_zusammenschieben_und_vereinen() {
        let (mut m, eg, og) = haus(&[
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(4000.0, 8000.0, 0.0),
            vec3(4000.0, 6000.0, 0.0),
            vec3(10000.0, 6000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ]);
        let stufe = m.wall_at(eg, 2).unwrap();
        let rechts = m.wall_at(eg, 3).unwrap();
        assert!(ziehen(&mut m, eg, 3, 1990.0), "10 mm davor rastet ein");
        assert_eq!((zahl(&m, eg), zahl(&m, og)), (5, 5));
        assert!(m.element(stufe).is_none(), "Stufe weg");
        assert_eq!(m.wall_at(eg, 2), Some(rechts));
        let fugen = m.straight_joints(eg);
        assert_eq!(fugen, vec![(m.wall_at(eg, 1).unwrap(), rechts)]);
        assert_eq!(m.straight_joints(og).len(), 1);
        pruefen(&mut m);
        let (a, b) = fugen[0];
        m.merge_walls(a, b).unwrap();
        assert_eq!((zahl(&m, eg), zahl(&m, og)), (4, 4));
        assert!(m.element(a).is_none(), "4 m gehen in 6 m auf");
        assert_eq!(m.wall_at(eg, 1), Some(rechts));
        pruefen(&mut m);
    }

    /// Ein OG-Teilstück allein zurück geht in Phase 2 noch nicht (Regel 28):
    /// abgelehnt, nichts ändert sich. Vereinen über verschiedene Typen und
    /// ohne Fuge ebenso.
    #[test]
    fn ablehnungen() {
        let (mut m, eg, og) = rechteck();
        let nord = m.wall_at(eg, 1).unwrap();
        m.split_wall(nord, &[5000.0]).unwrap();
        let (a, b) = m.straight_joints(eg)[0];
        assert_eq!(
            m.can_merge(a, m.wall_at(eg, 0).unwrap()),
            Err(FugeError::NotJoined)
        );
        let t = m
            .duplicate_type(m.element(a).unwrap().layer_set.unwrap())
            .unwrap();
        let mut probe = m.clone();
        if let Some(e) = probe.elements.get_mut(b) {
            e.layer_set = Some(t);
        }
        assert_eq!(probe.can_merge(a, b), Err(FugeError::Types));
        let _ = og;
        pruefen(&mut m);
    }
    /// Teilrücksprung im OG (Jörn 11.10., Tagesthema 2): Die Nordwand ist
    /// in beiden Geschossen geteilt, nur das OG-Mittelstück springt 1,5 m
    /// zurück. Das OG bekommt zwei Stufen, das EG bleibt; auf der Decke
    /// liegt eine Dachterrasse genau vor dem Mittelstück.
    #[test]
    fn og_mittelstueck_zurueck_gibt_terrasse() {
        let (mut m, eg, og) = rechteck();
        let nord = m.wall_at(eg, 1).unwrap();
        m.split_wall(nord, &[3000.0, 6000.0]).unwrap();
        let mitte = m.wall_at(og, 2).unwrap();
        let st = m.run(og).unwrap().storey;
        assert!(m.set_flat_roof(st, true));
        let ak = m.parapet_above(og).unwrap();
        m.begin("Rücksprung");
        assert!(m.set_offset(mitte, -1500.0).is_some());
        m.commit();
        assert_eq!((zahl(&m, eg), zahl(&m, og)), (6, 8));
        assert_eq!(m.stack_offset(mitte), Some((-1500.0, true)));
        // Aufkantung und Flachdach folgen dem OG samt Stufen
        assert_eq!(zahl(&m, ak), 8);
        let dach = m.flat_roof(ak).unwrap();
        assert_eq!(dach.outline.len(), 8);
        let s = |k| Some(k);
        assert_eq!(
            m.stack_map(og),
            vec![s(0), s(1), None, s(2), None, s(3), s(4), s(5)]
        );
        pruefen(&mut m);
        let t = m.terrace_outlines(eg);
        assert_eq!(t.len(), 1, "{t:?}");
        assert_eq!(t[0].segments, vec![2]);
        // Zwischen den Stufen, bis zur Deckenkante: 3 m × knapp 1,5 m
        let a = t[0].area() / 1e6;
        assert!(a > 3.0 && a < 5.0, "{a}");
        // Attika nur über dem Mittelstück, an beiden Stufen Stirnfläche;
        // das Blech endet dort
        let Some(Ok(f)) = m.floor(eg) else {
            panic!("Decke")
        };
        let plan = &f.terraces;
        assert!(!plan.attika.is_empty());
        assert!(plan
            .attika
            .iter()
            .all(|p| p.seg == 2 && p.free == [true, true]));
        assert_eq!(plan.coping.len(), 1);
        let c = &plan.coping[0];
        assert_eq!(c.points.len(), 2, "{c:?}");
        // Die Stufen stehen innen, ihre Außenfläche liegt auf der Fuge
        assert!((c.length() - 3000.0).abs() < 1e-6, "{c:?}");
        // Die Stufe ziehen: die Fuge darunter wandert mit, die Terrasse
        // wird breiter
        let stufe = m.wall_at(og, 2).unwrap();
        assert_eq!(m.step_joint(stufe), Some((eg, 2)));
        assert_eq!(m.step_joint(mitte), None);
        let c = m.chain(eg).unwrap().with_joint_moved(2, -1000.0).unwrap();
        m.begin("Fuge");
        assert!(m.set_run_points(eg, &c.points).is_some());
        m.commit();
        assert_eq!((zahl(&m, eg), zahl(&m, og)), (6, 8));
        assert_eq!(m.stack_offset(mitte), Some((-1500.0, true)));
        let b = m.terrace_outlines(eg)[0].area() / 1e6;
        assert!((b - a * 4.0 / 3.0).abs() < 1e-6, "{a} {b}");
        pruefen(&mut m);
        // Zurück auf 0: Stufen weg, keine Terrasse
        m.begin("Bündig");
        assert!(m.set_offset(mitte, 0.0).is_some());
        let t = m.commit().unwrap();
        assert_eq!(zahl(&m, og), 6);
        assert!(m.terrace_outlines(eg).is_empty());
        pruefen(&mut m);
        // Rückgängig: wieder der Rücksprung
        m.apply(&t, crate::txn::Direction::Undo);
        assert_eq!(zahl(&m, og), 8);
        assert_eq!(m.terrace_outlines(eg).len(), 1);
        pruefen(&mut m);
    }

    /// Geometrie (Prüfung #69): Ein Eckstück schmaler als die Attika
    /// (100–139 mm bei 140 mm) springt zurück. Dort ist kein Platz für eine
    /// Terrasse: Keine Terrassenfläche, und jedes Attikastück ist ein
    /// einfaches Viereck (kein Überschlag an der Stufe).
    #[test]
    fn schmales_eckstueck_zurueck() {
        use sk_math::polygon;
        for x in [100.0, 120.0, 139.0] {
            let (mut m, eg, og) = rechteck();
            let nord = m.wall_at(eg, 1).unwrap();
            m.split_wall(nord, &[x]).unwrap();
            let st = m.run(og).unwrap().storey;
            assert!(m.set_flat_roof(st, true));
            let ecke = m.wall_at(og, 1).unwrap();
            m.begin("Rücksprung");
            let ok = m.set_offset(ecke, -1500.0).is_some();
            m.commit();
            if !ok {
                continue;
            }
            pruefen(&mut m);
            let Some(Ok(f)) = m.floor(eg) else {
                panic!("Decke")
            };
            let t = &f.terraces;
            assert!(t.area() < 1.0, "{x}: Terrasse {} mm²", t.area());
            for p in &t.attika {
                assert!(polygon::is_simple(&p.quad), "{x}: Attika {p:?}");
            }
        }
    }
}

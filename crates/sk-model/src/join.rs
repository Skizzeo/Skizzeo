//! Anschlüsse zwischen Wandzügen (Paket B5a): L-Ecken zwischen zwei freien
//! Enden und T-Anschlüsse eines freien Endes an eine Wandfläche.
//!
//! Anschlüsse sind abgeleitet. Sie entstehen allein aus den Punkten der
//! Wandzüge ([`crate::Model::joins`]) und stehen weder im Rückgängig-Protokoll
//! noch in der Datei. Hier liegt nur die Geometrie; Erkennen und Mitführen
//! macht das Modell.

use crate::element::{ElementId, RunId};
use crate::wall::{cross2, right_of, Gap, Line2, WallChain};
use sk_math::Vec3;

/// Fangabstand für Anschlüsse (mm).
pub const SNAP: f64 = 50.0;
/// Kleinster Winkel für einen T-Anschluss, als |sin| zwischen den Richtungen.
pub(crate) const MIN_SIN: f64 = 0.2;
/// Größter Abstand einer Gehrungsecke vom Stoßpunkt, in Wanddicken.
const MITER_LIMIT: f64 = 8.0;

/// Ende eines offenen Wandzugs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JoinEnd {
    Start,
    End,
}

impl JoinEnd {
    pub const BOTH: [JoinEnd; 2] = [JoinEnd::Start, JoinEnd::End];

    /// 0 für den Anfang, 1 für das Ende.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Richtung vom Wandkörper weg über das Ende hinaus.
    fn out(self, d: Vec3) -> Vec3 {
        match self {
            JoinEnd::Start => -d,
            JoinEnd::End => d,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum JoinKind {
    /// Zwei freie Enden treffen sich (Ecke mit Gehrung).
    L,
    /// Ein freies Ende stößt an die Fläche einer anderen Wand.
    T,
}

/// Ein Anschluss des freien Endes `a_end` von Wand `a`.
#[derive(Clone, Debug, PartialEq)]
pub struct Join {
    /// Wand am anschließenden Ende (erstes bzw. letztes Segment ihres Zuges).
    pub a: ElementId,
    pub a_end: JoinEnd,
    /// Bei L die Partnerwand, bei T die Wirtswand (das getroffene Segment).
    pub b: ElementId,
    /// Bei L das Ende des Partners, bei T `None`.
    pub b_end: Option<JoinEnd>,
    pub kind: JoinKind,
    pub a_run: RunId,
    pub b_run: RunId,
    /// Lage des Partners beim Erkennen: bei L sein Endpunkt (`p`), bei T die
    /// zugewandte Wirtsfläche. Weicht sie später ab, wird `a` mitgeführt.
    pub anchor: Line2,
}

impl Join {
    /// Gleicher Anschluss (ohne die Lage beim Erkennen).
    pub fn same(&self, o: &Join) -> bool {
        (self.a, self.a_end, self.b, self.b_end, self.kind) == (o.a, o.a_end, o.b, o.b_end, o.kind)
    }
}

/// T-Anschluss: Abschluss je Schicht (Linie, fugenlos) und Lücken im Wirt.
pub(crate) type TCut = (Vec<(Line2, bool)>, Vec<Gap>);

/// Zugewandte Fläche von Segment `seg` des Wirts für das Ende `e` von `a`,
/// und ob `a` von der Seite größerer Versätze kommt.
fn approach(a: &WallChain, e: JoinEnd, host: &WallChain, seg: usize) -> Option<(Line2, bool)> {
    let (_, da) = a.end_frame(e.index())?;
    let (pb, db) = host.segment_frame(seg)?;
    let nb = right_of(db);
    let from_hi = e.out(da).dot(nb) < 0.0;
    let (lo, hi) = host.span();
    let off = if from_hi { hi } else { lo };
    Some((
        Line2 {
            p: pb + nb * off,
            d: db,
        },
        from_hi,
    ))
}

/// Die Fläche des Wirts, auf die ein T-Anschluss gelegt wird.
pub(crate) fn facing_face(
    a: &WallChain,
    e: JoinEnd,
    host: &WallChain,
    seg: usize,
) -> Option<Line2> {
    approach(a, e, host, seg).map(|x| x.0)
}

/// T-Anschluss des Endes `e` von `a` an Segment `seg` des Wirts: je Schicht
/// von `a` die Linie, an der sie endet, und ob sie dort fugenlos weiterläuft;
/// dazu die Lücken in der Kontur des Wirts. `prio` liefert die Priorität zu
/// einem Baustoffschlüssel.
///
/// Jede Schicht reicht bis zur ersten Wirtsschicht (von der Anschlussseite
/// her) mit gleicher oder höherer Priorität. Gibt es keine, läuft sie bis
/// zur abgewandten Fläche durch.
pub(crate) fn t_cut(
    a: &WallChain,
    e: JoinEnd,
    host: &WallChain,
    seg: usize,
    prio: &dyn Fn(u16) -> u16,
) -> Option<TCut> {
    let (p, da) = a.end_frame(e.index())?;
    let (pb, db) = host.segment_frame(seg)?;
    let (_, from_hi) = approach(a, e, host, seg)?;
    let nb = right_of(db);
    let mut order = host.layer_offsets();
    if from_hi {
        order.sort_by(|x, y| y.1.total_cmp(&x.1));
    } else {
        order.sort_by(|x, y| x.0.total_cmp(&y.0));
    }
    let near = |l: &(f64, f64, u16)| if from_hi { l.1 } else { l.0 };
    let far = |l: &(f64, f64, u16)| if from_hi { l.0 } else { l.1 };
    let line = |off: f64| Line2 {
        p: pb + nb * off,
        d: db,
    };
    let na = right_of(da);
    let mut cuts = Vec::new();
    let mut gaps = Vec::new();
    for (lo, hi, key) in a.layer_offsets() {
        let pa = prio(key);
        let (off, seamless) = match order.iter().find(|l| prio(l.2) >= pa) {
            Some(l) => (near(l), l.2 == key),
            None => (far(order.last()?), false),
        };
        let ln = line(off);
        if seamless {
            let t = |o: f64| ln.meet(p + na * o, da).map(|x| (x - pb).dot(db));
            if let (Some(t0), Some(t1)) = (t(lo), t(hi)) {
                gaps.push(Gap {
                    seg,
                    off,
                    from: t0.min(t1),
                    to: t0.max(t1),
                });
            }
        }
        cuts.push((ln, seamless));
    }
    Some((cuts, gaps))
}

/// Gehrungslinie einer L-Ecke zwischen Ende `ea` von `a` und Ende `eb` von `b`:
/// durch die Schnittpunkte der beiden Konturflächen, wie bei einer Ecke im
/// Zug. Laufen die Wände (fast) parallel, ein rechtwinkliger Stoß in der Mitte.
pub(crate) fn l_miter(a: &WallChain, ea: JoinEnd, b: &WallChain, eb: JoinEnd) -> Option<Line2> {
    let (pa, da) = a.end_frame(ea.index())?;
    let (pb, db) = b.end_frame(eb.index())?;
    let ((alo, ahi), (blo, bhi)) = (a.span(), b.span());
    // In Laufrichtung über die Ecke: a kommt an, b geht weg; rechts trifft rechts
    let (ar, al) = match ea {
        JoinEnd::End => (ahi, alo),
        JoinEnd::Start => (alo, ahi),
    };
    let (br, bl) = match eb {
        JoinEnd::Start => (bhi, blo),
        JoinEnd::End => (blo, bhi),
    };
    let face = |p: Vec3, d: Vec3, o: f64| Line2 {
        p: p + right_of(d) * o,
        d,
    };
    let mid = (pa + pb) * 0.5;
    let butt = Line2 {
        p: mid,
        d: right_of(da),
    };
    let (fbr, fbl) = (face(pb, db, br), face(pb, db, bl));
    let (Some(c1), Some(c2)) = (
        face(pa, da, ar).meet(fbr.p, fbr.d),
        face(pa, da, al).meet(fbl.p, fbl.d),
    ) else {
        return Some(butt);
    };
    let limit = MITER_LIMIT * a.thickness().max(b.thickness());
    if (c1 - mid).length() > limit || (c2 - mid).length() > limit || (c2 - c1).length() < 1e-6 {
        return Some(butt);
    }
    Some(Line2 {
        p: c1,
        d: (c2 - c1).normalized(),
    })
}

/// Grundriss jedes Segments über die ganze Dicke (ohne Anschlüsse).
pub(crate) fn footprints(c: &WallChain) -> Vec<[Vec3; 4]> {
    let (o, i) = (
        c.face_corners(c.outer_offset()),
        c.face_corners(c.inner_offset()),
    );
    let n = o.len();
    (0..c.segment_count())
        .map(|k| {
            let j = (k + 1) % n;
            [o[k], o[j], i[j], i[k]]
        })
        .collect()
}

/// Abstand eines Punktes von einem Viereck in der Ebene (0, wenn innen).
pub(crate) fn quad_distance(q: &[Vec3; 4], p: Vec3) -> f64 {
    let mut inside = false;
    let mut best = f64::INFINITY;
    for k in 0..4 {
        let (a, b) = (q[k], q[(k + 1) % 4]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                inside = !inside;
            }
        }
        let ab = b - a;
        let len2 = ab.x * ab.x + ab.y * ab.y;
        let t = if len2 > 0.0 {
            (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (dx, dy) = (p.x - a.x - ab.x * t, p.y - a.y - ab.y * t);
        best = best.min((dx * dx + dy * dy).sqrt());
    }
    if inside {
        0.0
    } else {
        best
    }
}

/// |sin| des Winkels zwischen zwei Richtungen in der Ebene.
pub(crate) fn sin_between(a: Vec3, b: Vec3) -> f64 {
    cross2(a, b).abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Category;
    use crate::guid::GuidGen;
    use crate::model::Model;
    use crate::qto::run_qto;
    use crate::solid::edge_kind;
    use crate::txn::Direction;
    use crate::wall::RefSide;
    use crate::{szo, MaterialId};
    use sk_math::vec3;

    const H: f64 = 2750.0;

    /// m³ auf vier Stellen. Das Paket nennt 3,5469 m³ für die Innenwand;
    /// genau sind es 7,37 × 0,175 × 2,75 = 3,5468125 m³, also 3,5468. Ebenso
    /// Gasbeton gesamt 16,449125 + 3,5468125 = 19,9959375 m³ (Paket: 19,9960).
    fn m3(v: f64) -> f64 {
        (v / 1e9 * 1e4).round() / 1e4
    }

    /// Rechteck 10 × 8 m AW 31,5, Außenkante auf der Linie, Uhrzeigersinn.
    fn haus(m: &mut Model) -> RunId {
        let set = m.defaults().exterior_wall;
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.add_wall_run(&pts, true, RefSide::Left, H, set, Category::ExteriorWall)
            .unwrap()
    }

    fn innenwand(m: &mut Model, y0: f64, y1: f64) -> RunId {
        let set = m.defaults().interior_wall;
        let pts = [vec3(5000.0, y0, 0.0), vec3(5000.0, y1, 0.0)];
        m.add_wall_run(&pts, false, RefSide::Center, H, set, Category::InteriorWall)
            .unwrap()
    }

    fn volume(m: &Model, r: RunId) -> f64 {
        m3(run_qto(m, r).iter().map(|q| q.volume).sum())
    }

    fn material_volume(m: &Model, mat: MaterialId) -> f64 {
        let v: f64 = m
            .runs()
            .ids()
            .flat_map(|r| run_qto(m, r))
            .flat_map(|q| q.layers)
            .filter(|l| l.material == mat)
            .map(|l| l.volume)
            .sum();
        m3(v)
    }

    fn gasbeton(m: &Model) -> MaterialId {
        m.materials()
            .iter()
            .find(|(_, x)| x.name == "Gasbeton")
            .unwrap()
            .0
    }

    fn t_joins(m: &Model) -> usize {
        m.joins().iter().filter(|j| j.kind == JoinKind::T).count()
    }

    #[test]
    fn innenwand_bis_zur_innenflaeche_oder_in_die_aussenwand() {
        for (y0, y1) in [(315.0, 7685.0), (0.0, 8000.0)] {
            let mut m = Model::with_seed(1);
            let aw = haus(&mut m);
            let iw = innenwand(&mut m, y0, y1);
            assert_eq!(t_joins(&m), 2, "{y0}..{y1}");
            assert_eq!(m.joins().len(), 2);
            assert_eq!(volume(&m, iw), 3.5468, "{y0}..{y1}");
            assert_eq!(volume(&m, aw), 30.0935);
            assert_eq!(material_volume(&m, gasbeton(&m)), 19.9959);
            // Brutto bleibt die gezeichnete Wand
            let gross: f64 = run_qto(&m, iw).iter().map(|q| q.volume_gross).sum();
            assert_eq!(m3(gross), m3((y1 - y0) * 175.0 * H));
            assert!(m.check().is_empty(), "{:?}", m.check());
            assert_eq!(
                m.element(m.wall_at(iw, 0).unwrap()).unwrap().number,
                "IW-001"
            );
        }
    }

    #[test]
    fn luecke_ergibt_keinen_anschluss() {
        let mut m = Model::with_seed(2);
        haus(&mut m);
        let iw = innenwand(&mut m, 1000.0, 7000.0);
        assert!(m.joins().is_empty());
        assert_eq!(volume(&m, iw), 2.8875);
        let t = szo::write(&m);
        let l = szo::read(&t, GuidGen::with_seed(5)).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert!(l.model.joins().is_empty());
        assert_eq!(szo::write(&l.model), t);
    }

    #[test]
    fn innenwand_folgt_der_aussenwand() {
        let mut m = Model::with_seed(3);
        m.require_steps();
        m.begin("Haus");
        let aw = haus(&mut m);
        let iw = innenwand(&mut m, 315.0, 7685.0);
        m.commit();
        let before = (m.run(aw).cloned(), m.run(iw).cloned());
        // Außenwand y = 8 um 1 m nach außen, in vielen kleinen Schritten wie beim Ziehen
        m.begin("Wand verschieben");
        let base = m.chain(aw).unwrap();
        for k in 1..=10 {
            let moved = base.with_segment_moved(1, -100.0 * k as f64).unwrap();
            let runs = m.set_run_points(aw, &moved.points).unwrap();
            assert!(runs.contains(&iw));
        }
        let t = m.commit().unwrap();
        // Ein Eintrag je Zug
        assert_eq!(t.changes.len(), 2, "{:?}", t.changes);
        let p = &m.run(iw).unwrap().points;
        assert!((p[1].y - 8685.0).abs() < 1e-6, "{p:?}");
        assert_eq!(volume(&m, iw), 4.0281);
        assert_eq!(t_joins(&m), 2);
        assert!(m.check().is_empty(), "{:?}", m.check());

        // Speichern und Öffnen: dieselben Anschlüsse und Mengen
        let text = szo::write(&m);
        let l = szo::read(&text, GuidGen::with_seed(9)).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(t_joins(&l.model), 2);
        let iw2 = l.model.runs().iter().find(|(_, r)| !r.closed).unwrap().0;
        assert_eq!(volume(&l.model, iw2), 4.0281);
        assert_eq!(szo::write(&l.model), text);

        // Rückgängig stellt beide Züge zurück
        m.apply(&t, Direction::Undo);
        assert_eq!((m.run(aw).cloned(), m.run(iw).cloned()), before);
        assert_eq!(volume(&m, iw), 3.5468);
        assert_eq!(t_joins(&m), 2);
        assert!(m.check().is_empty(), "{:?}", m.check());
        m.apply(&t, Direction::Redo);
        assert_eq!(volume(&m, iw), 4.0281);
    }

    #[test]
    fn hineingezeichnete_innenwand_landet_beim_mitfuehren_auf_der_flaeche() {
        let mut m = Model::with_seed(4);
        let aw = haus(&mut m);
        let iw = innenwand(&mut m, 0.0, 8000.0);
        let moved = m.chain(aw).unwrap().with_segment_moved(1, -1000.0).unwrap();
        m.set_run_points(aw, &moved.points).unwrap();
        let p = &m.run(iw).unwrap().points;
        assert!((p[1].y - 8685.0).abs() < 1e-6, "{p:?}");
        // Das andere Ende hängt an einer unveränderten Wand und bleibt, wo es war
        assert!(p[0].y.abs() < 1e-6, "{p:?}");
        assert_eq!(volume(&m, iw), 4.0281);
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    #[test]
    fn l_zwischen_zuegen_wie_ecke_im_zug() {
        let set = |m: &Model| m.defaults().exterior_wall;
        let (a, b, c) = (
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
        );
        let mut one = Model::with_seed(5);
        let s = set(&one);
        let r = one
            .add_wall_run(
                &[a, b, c],
                false,
                RefSide::Left,
                H,
                s,
                Category::ExteriorWall,
            )
            .unwrap();
        let want = run_qto(&one, r);
        let mut two = Model::with_seed(5);
        let s = set(&two);
        let r1 = two
            .add_wall_run(&[a, b], false, RefSide::Left, H, s, Category::ExteriorWall)
            .unwrap();
        let r2 = two
            .add_wall_run(&[b, c], false, RefSide::Left, H, s, Category::ExteriorWall)
            .unwrap();
        assert_eq!(two.joins().len(), 2);
        assert!(two.joins().iter().all(|j| j.kind == JoinKind::L));
        let got = [run_qto(&two, r1), run_qto(&two, r2)].concat();
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(&want) {
            assert!(
                (g.volume - w.volume).abs() < 1e-3,
                "{} {}",
                g.volume,
                w.volume
            );
            for (gl, wl) in g.layers.iter().zip(&w.layers) {
                assert!((gl.volume - wl.volume).abs() < 1e-3);
            }
        }
        assert!(two.check().is_empty(), "{:?}", two.check());
        // Der Partner folgt dem verschobenen Ende
        let mut p = two.run(r1).unwrap().points.clone();
        p[1] = vec3(10000.0, 500.0, 0.0);
        let runs = two.set_run_points(r1, &p).unwrap();
        assert!(runs.contains(&r2));
        assert_eq!(two.run(r2).unwrap().points[0], vec3(10000.0, 500.0, 0.0));
        assert!(two.check().is_empty(), "{:?}", two.check());
    }

    /// Kanten auf der Linie y = `y` zwischen x0 und x1 (ohne Endpunkte).
    fn edges_across(s: &crate::Solid, y: f64, x0: f64, x1: f64) -> usize {
        s.edges
            .iter()
            .filter(|e| (e.a.y - y).abs() < 1e-6 && (e.b.y - y).abs() < 1e-6)
            .filter(|e| e.a.x.min(e.b.x) < x1 - 1e-6 && e.a.x.max(e.b.x) > x0 + 1e-6)
            .count()
    }

    #[test]
    fn keine_fuge_zwischen_gleichen_baustoffen() {
        let mut m = Model::with_seed(6);
        let aw = haus(&mut m);
        let iw = innenwand(&mut m, 315.0, 7685.0);
        let (host, wall) = (m.chain(aw).unwrap(), m.chain(iw).unwrap());
        for s in [host.solid_cut_at(1000.0), host.solid()] {
            // Innenfläche unten (y = 315) und oben (y = 7685) über die Breite der Innenwand offen
            assert_eq!(edges_across(&s, 315.0, 4912.5, 5087.5), 0);
            assert_eq!(edges_across(&s, 7685.0, 4912.5, 5087.5), 0);
            // daneben bleibt die Kontur
            assert!(edges_across(&s, 315.0, 1000.0, 4000.0) > 0);
        }
        // Die Innenwand hat an ihren Enden keine Stirnkanten
        let cut = wall.solid_cut_at(1000.0);
        assert_eq!(edges_across(&cut, 315.0, 4912.5, 5087.5), 0);
        assert!(cut.edges.iter().all(|e| e.kind != edge_kind::FINE));
        // Schnitt quer durch den Anschluss (Ebene x = 5000, Blick nach −x)
        let p0 = vec3(5000.0, 0.0, 0.0);
        let n = vec3(1.0, 0.0, 0.0);
        let caps = host.section_caps(p0, n);
        let fugen = caps
            .edges
            .iter()
            .filter(|e| (e.a.y - 315.0).abs() < 1e-6 && (e.b.y - 315.0).abs() < 1e-6)
            .filter(|e| (e.a.z - e.b.z).abs() > 1.0)
            .count();
        assert_eq!(fugen, 0);
    }

    #[test]
    fn ohne_anschluss_unveraendert() {
        let mut m = Model::with_seed(7);
        let aw = haus(&mut m);
        let before = m.chain(aw).unwrap().solid();
        innenwand(&mut m, 1000.0, 7000.0);
        let after = m.chain(aw).unwrap().solid();
        assert_eq!(before.edges.len(), after.edges.len());
        assert_eq!(before.triangles.len(), after.triangles.len());
        assert!(m.chain(aw).unwrap().joints.is_empty());
    }

    #[test]
    fn loeschen_und_rueckgaengig() {
        let mut m = Model::with_seed(8);
        m.require_steps();
        m.begin("Haus");
        let aw = haus(&mut m);
        m.commit();
        m.begin("Innenwand");
        let iw = innenwand(&mut m, 315.0, 7685.0);
        let t = m.commit().unwrap();
        assert_eq!(m.joined_runs(aw), vec![iw]);
        let touched = m.apply(&t, Direction::Undo);
        assert!(touched.runs.contains(&aw), "{touched:?}");
        assert!(m.joins().is_empty());
        assert!(m.chain(aw).unwrap().joints.is_empty());
        m.apply(&t, Direction::Redo);
        assert_eq!(t_joins(&m), 2);
        m.begin("Löschen");
        assert!(m.remove_run(aw));
        m.commit();
        assert!(m.joins().is_empty());
        assert!(m.check().is_empty(), "{:?}", m.check());
    }
}

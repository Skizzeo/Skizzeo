//! Ecke bilden und Verlängern (Grundriss G5, Jörn 11.10. 16:47, J4 und J5):
//! Methoden von `Model`, aus `model.rs` ausgelagert.
//!
//! Beide ändern nur freie Enden offener Züge: Ein Ende läuft auf seiner
//! Bezugslinie bis zum Ziel, die Richtung der Wand bleibt. Die Anschlüsse
//! (L an der Ecke, T an der Fläche) entstehen danach wie beim Zeichnen aus
//! den Punkten (B5a). Schlägt der zweite Zug bei „Ecke bilden“ fehl, ist der
//! erste schon gesetzt; die App rollt den Schritt dann zurück
//! (`Scene::edit_model`).

use super::*;
use crate::fuge::MIN_PIECE;
use crate::join;
use crate::wall::{cross2, right_of};

/// Warum Ecke bilden oder Verlängern nicht geht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EckeFehler {
    /// Kein Segment eines Wandzugs.
    KeineWand,
    /// Das Ende, das wandern müsste, ist kein freies Ende: geschlossener Zug
    /// wie die Außenwand oder eine Ecke innerhalb eines Zuges.
    KeinEnde,
    /// Die Wände laufen (fast) parallel und nicht in einer Flucht.
    Parallel,
    /// Beide Wände gehören zu einem Zug.
    EinZug,
    /// Eine Wand würde kürzer als [`MIN_PIECE`].
    ZuKurz,
    /// Das Modell lehnt die neuen Punkte ab (Stapel, Umriss).
    Ungueltig,
}

impl EckeFehler {
    /// Satz für die Statuszeile.
    pub fn message(self) -> &'static str {
        match self {
            EckeFehler::KeineWand => "Ecke bilden und Verlängern geht nur an Wänden.",
            EckeFehler::KeinEnde => {
                "Dort hat die Wand kein freies Ende; Außenwände und Ecken im Zug bleiben."
            }
            EckeFehler::Parallel => "Die Wände laufen parallel und treffen sich nicht.",
            EckeFehler::EinZug => "Beide Wände gehören zu einem Zug.",
            EckeFehler::ZuKurz => "Eine Wand würde kürzer als 10 cm.",
            EckeFehler::Ungueltig => "Das geht hier nicht: Der Grundriss würde ungültig.",
        }
    }
}

/// Neue Punkte eines Zuges.
pub type ZugPunkte = (RunId, Vec<Vec3>);

/// Bezugslinie einer Wand: Zug, Segment, Anfang, Einheitsrichtung, Länge und
/// ob Anfang bzw. Ende des Segments freie Enden des Zuges sind.
struct Strecke {
    run: RunId,
    seg: usize,
    p0: Vec3,
    d: Vec3,
    len: f64,
    frei: [bool; 2],
}

impl Strecke {
    /// Lage von `p` längs der Strecke (0 am Anfang).
    fn bei(&self, p: Vec3) -> f64 {
        let v = p - self.p0;
        v.x * self.d.x + v.y * self.d.y
    }

    fn punkt(&self, s: f64) -> Vec3 {
        self.p0 + self.d * s
    }

    /// Schnitt mit der Geraden durch `q` in Richtung `e`, als Lage längs der
    /// Strecke; `None`, wenn beide fast parallel laufen.
    fn schnitt(&self, q: Vec3, e: Vec3) -> Option<f64> {
        let k = cross2(self.d, e);
        (k.abs() >= join::MIN_SIN).then(|| cross2(q - self.p0, e) / k)
    }
}

impl Model {
    fn strecke(&self, wall: ElementId) -> Result<Strecke, EckeFehler> {
        let (run, seg) = self.segment_of(wall).ok_or(EckeFehler::KeineWand)?;
        let r = self.run(run).ok_or(EckeFehler::KeineWand)?;
        let n = r.points.len();
        let (a, b) = match (r.points.get(seg), r.points.get((seg + 1) % n.max(1))) {
            (Some(a), Some(b)) if r.closed || seg + 1 < n => (flach(*a), flach(*b)),
            _ => return Err(EckeFehler::KeineWand),
        };
        let len = (b - a).length();
        if len < 1e-6 {
            return Err(EckeFehler::KeineWand);
        }
        Ok(Strecke {
            run,
            seg,
            p0: a,
            d: (b - a) * (1.0 / len),
            len,
            frei: [!r.closed && seg == 0, !r.closed && seg + 2 == n],
        })
    }

    /// Punkte des Zuges von `s`, dessen Ende `ende` (0 Anfang, 1 Ende des
    /// Segments) an der Lage `bei` liegt; prüft freies Ende und Mindestlänge.
    fn mit_ende(&self, s: &Strecke, ende: usize, bei: f64) -> Result<ZugPunkte, EckeFehler> {
        if !s.frei[ende] {
            return Err(EckeFehler::KeinEnde);
        }
        let rest = if ende == 0 { s.len - bei } else { bei };
        if rest < MIN_PIECE {
            return Err(EckeFehler::ZuKurz);
        }
        let mut pts: Vec<Vec3> = self
            .run(s.run)
            .ok_or(EckeFehler::KeineWand)?
            .points
            .iter()
            .map(|p| flach(*p))
            .collect();
        pts[s.seg + ende] = s.punkt(bei);
        Ok((s.run, pts))
    }

    /// Hat die Wand ein freies Ende (Anfang oder Ende eines offenen Zuges)?
    /// Nur dann gehen „Ecke bilden“ und „Verlängern“.
    pub fn freies_ende(&self, wall: ElementId) -> bool {
        self.strecke(wall).is_ok_and(|s| s.frei.iter().any(|f| *f))
    }

    /// Neue Punkte für [`Model::ecke_bilden`], ohne etwas zu ändern (Vorschau).
    pub fn ecke_punkte(
        &self,
        a: ElementId,
        a_bleibt: Vec3,
        b: ElementId,
        b_bleibt: Vec3,
    ) -> Result<[ZugPunkte; 2], EckeFehler> {
        let (sa, sb) = (self.strecke(a)?, self.strecke(b)?);
        if sa.run == sb.run {
            return Err(EckeFehler::EinZug);
        }
        let ta = sa.schnitt(sb.p0, sb.d).ok_or(EckeFehler::Parallel)?;
        let tb = sb.bei(sa.punkt(ta));
        // Es bleibt die Seite des Schnittpunkts, auf der der Klick liegt; das
        // Ende der anderen Seite läuft zum Schnittpunkt
        let ende = |s: &Strecke, t: f64, bleibt: Vec3| usize::from(s.bei(bleibt) < t);
        Ok([
            self.mit_ende(&sa, ende(&sa, ta, a_bleibt), ta)?,
            self.mit_ende(&sb, ende(&sb, tb, b_bleibt), tb)?,
        ])
    }

    /// Ecke bilden (J4): Die Wände `a` und `b` laufen bis zum Schnittpunkt
    /// ihrer Bezugslinien, verlängert oder gekürzt. Es bleibt jeweils die
    /// Seite, auf der `a_bleibt` bzw. `b_bleibt` liegt (Klickpunkte); das
    /// Ende auf der anderen Seite wandert. Die Ecke entsteht danach als L.
    pub fn ecke_bilden(
        &mut self,
        a: ElementId,
        a_bleibt: Vec3,
        b: ElementId,
        b_bleibt: Vec3,
    ) -> Result<(), EckeFehler> {
        for (run, pts) in self.ecke_punkte(a, a_bleibt, b, b_bleibt)? {
            self.set_run_points(run, &pts)
                .ok_or(EckeFehler::Ungueltig)?;
        }
        Ok(())
    }

    /// Neue Punkte für [`Model::verlaengern`], ohne etwas zu ändern (Vorschau).
    pub fn verlaengern_punkte(
        &self,
        a: ElementId,
        ziel: ElementId,
    ) -> Result<ZugPunkte, EckeFehler> {
        let (sa, sb) = (self.strecke(a)?, self.strecke(ziel)?);
        if sa.run == sb.run {
            return Err(EckeFehler::EinZug);
        }
        if let Some(tx) = sa.schnitt(sb.p0, sb.d) {
            // Quer oder schräg: Das Ende näher am Schnittpunkt läuft bis an
            // die Fläche der Zielwand, die dem anderen Ende zugewandt ist
            let ende = usize::from(tx.abs() > (tx - sa.len).abs());
            let fest = sa.punkt(if ende == 0 { sa.len } else { 0.0 });
            let (lo, hi) = self.chain(sb.run).ok_or(EckeFehler::KeineWand)?.span();
            let n = right_of(sb.d);
            let seite = (fest - sb.p0).dot(n);
            let off = if seite > (lo + hi) * 0.5 { hi } else { lo };
            let t = sa
                .schnitt(sb.p0 + n * off, sb.d)
                .ok_or(EckeFehler::Parallel)?;
            return self.mit_ende(&sa, ende, t);
        }
        // In einer Flucht: bis an das nähere Ende der Zielwand, auf der
        // eigenen Bezugslinie
        if cross2(sb.p0 - sa.p0, sa.d).abs() > join::SNAP {
            return Err(EckeFehler::Parallel);
        }
        let (u0, u1) = (sa.bei(sb.p0), sa.bei(sb.punkt(sb.len)));
        let (lo, hi) = (u0.min(u1), u0.max(u1));
        if lo >= sa.len {
            self.mit_ende(&sa, 1, lo)
        } else if hi <= 0.0 {
            self.mit_ende(&sa, 0, hi)
        } else {
            Err(EckeFehler::Parallel)
        }
    }

    /// Verlängern (J5): Das Ende der Wand `a`, das der Zielwand näher
    /// liegt, läuft auf der eigenen Bezugslinie bis an die zugewandte Fläche
    /// der Zielwand (T-Anschluss) oder, in einer Flucht, bis an ihr näheres
    /// Ende. Liegt die Zielwand quer über `a`, wird `a` dorthin gekürzt.
    pub fn verlaengern(&mut self, a: ElementId, ziel: ElementId) -> Result<(), EckeFehler> {
        let (run, pts) = self.verlaengern_punkte(a, ziel)?;
        self.set_run_points(run, &pts)
            .map(|_| ())
            .ok_or(EckeFehler::Ungueltig)
    }
}

fn flach(p: Vec3) -> Vec3 {
    vec3(p.x, p.y, 0.0)
}

impl Model {
    /// Enden der Wand, die Enden eines offenen Zuges sind (G5b, Griffe zum
    /// Ziehen): 0 Anfang, 1 Ende des Segments, mit dem Punkt der Bezugslinie.
    pub fn wand_enden(&self, wall: ElementId) -> Vec<(usize, Vec3)> {
        let Ok(s) = self.strecke(wall) else {
            return Vec::new();
        };
        (0..2)
            .filter(|&e| s.frei[e])
            .map(|e| (e, s.punkt(if e == 0 { 0.0 } else { s.len })))
            .collect()
    }

    /// Neue Punkte des Zuges, wenn Ende `ende` der Wand nach `p` wandert
    /// (Vorschau beim Ziehen). Das andere Ende des Segments bleibt; das
    /// Segment wird nicht kürzer als 10 cm.
    pub fn ende_punkte(
        &self,
        wall: ElementId,
        ende: usize,
        p: Vec3,
    ) -> Result<ZugPunkte, EckeFehler> {
        let s = self.strecke(wall)?;
        if !s.frei.get(ende).copied().unwrap_or(false) {
            return Err(EckeFehler::KeinEnde);
        }
        let fest = s.punkt(if ende == 0 { s.len } else { 0.0 });
        if (flach(p) - fest).length() < MIN_PIECE {
            return Err(EckeFehler::ZuKurz);
        }
        let mut pts: Vec<Vec3> = self
            .run(s.run)
            .ok_or(EckeFehler::KeineWand)?
            .points
            .iter()
            .map(|q| flach(*q))
            .collect();
        pts[s.seg + ende] = flach(p);
        Ok((s.run, pts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f64 = 2750.0;

    fn innenwand(m: &mut Model, st: StoreyId, a: Vec3, b: Vec3) -> ElementId {
        let iw = m.defaults().interior_wall;
        let r = m
            .add_wall_run(
                &[a, b],
                false,
                RefSide::Center,
                st,
                iw,
                Category::InteriorWall,
            )
            .unwrap();
        m.wall_at(r, 0).unwrap()
    }

    /// Haus 10 × 8 m (Außenwand, Bezug außen); dazu die Dicke der Außenwand.
    fn haus() -> (Model, StoreyId, f64) {
        let mut m = Model::new();
        let st = m.eg_at(H);
        let aw = m.defaults().exterior_wall;
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
            .unwrap();
        let t = m
            .layer_set(aw)
            .map(|s| s.layers.iter().map(|l| l.thickness).sum::<f64>())
            .unwrap();
        (m, st, t)
    }

    fn enden(m: &Model, w: ElementId) -> (Vec3, Vec3) {
        let (run, _) = m.segment_of(w).unwrap();
        let p = &m.run(run).unwrap().points;
        (p[0], p[p.len() - 1])
    }

    fn nah(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-6
    }

    /// Zwei Innenwände, die sich nicht erreichen, laufen bis zum Schnittpunkt
    /// und bilden dort ein L; eine, die übersteht, wird gekürzt.
    #[test]
    fn ecke_verlaengert_und_kuerzt() {
        let (mut m, st, _) = haus();
        let a = innenwand(
            &mut m,
            st,
            vec3(2000.0, 1000.0, 0.0),
            vec3(2000.0, 4000.0, 0.0),
        );
        let b = innenwand(
            &mut m,
            st,
            vec3(3000.0, 5000.0, 0.0),
            vec3(6000.0, 5000.0, 0.0),
        );
        m.ecke_bilden(a, vec3(2000.0, 2000.0, 0.0), b, vec3(5000.0, 5000.0, 0.0))
            .unwrap();
        let x = vec3(2000.0, 5000.0, 0.0);
        assert!(nah(enden(&m, a).1, x), "{:?}", enden(&m, a));
        assert!(nah(enden(&m, b).0, x), "{:?}", enden(&m, b));
        // Eine Ecke, von beiden Enden aus gesehen
        let l = m.joins().iter().filter(|j| j.kind == JoinKind::L).count();
        assert_eq!(l, 2, "{:?}", m.joins());
        assert!(m.check().is_empty(), "{:?}", m.check());

        // Kreuzende Wand: Es bleibt die geklickte Seite, das andere Ende kommt zurück
        let c = innenwand(
            &mut m,
            st,
            vec3(7000.0, 1000.0, 0.0),
            vec3(7000.0, 4000.0, 0.0),
        );
        let d = innenwand(
            &mut m,
            st,
            vec3(6500.0, 3000.0, 0.0),
            vec3(9000.0, 3000.0, 0.0),
        );
        m.ecke_bilden(c, vec3(7000.0, 2000.0, 0.0), d, vec3(8500.0, 3000.0, 0.0))
            .unwrap();
        let x = vec3(7000.0, 3000.0, 0.0);
        assert!(nah(enden(&m, c).1, x), "c gekürzt: {:?}", enden(&m, c));
        assert!(nah(enden(&m, d).0, x), "d gekürzt: {:?}", enden(&m, d));
    }

    /// Parallele Wände, ein Zug, die Außenwand und zu kurze Reste lehnen ab.
    #[test]
    fn ecke_lehnt_ab() {
        let (mut m, st, _) = haus();
        let a = innenwand(
            &mut m,
            st,
            vec3(2000.0, 1000.0, 0.0),
            vec3(2000.0, 4000.0, 0.0),
        );
        let p = innenwand(
            &mut m,
            st,
            vec3(3000.0, 1000.0, 0.0),
            vec3(3000.0, 4000.0, 0.0),
        );
        let mitte = vec3(2000.0, 2000.0, 0.0);
        assert_eq!(
            m.ecke_punkte(a, mitte, p, vec3(3000.0, 2000.0, 0.0))
                .unwrap_err(),
            EckeFehler::Parallel
        );
        // Außenwand: geschlossener Zug, kein freies Ende
        let aw = m.wall_at(m.runs().ids().next().unwrap(), 1).unwrap();
        assert_eq!(
            m.ecke_punkte(a, mitte, aw, vec3(5000.0, 8000.0, 0.0))
                .unwrap_err(),
            EckeFehler::KeinEnde
        );
        // Kurzer Rest: Schnittpunkt 50 mm hinter dem Anfang, die lange Seite bleibt nicht
        let q = innenwand(
            &mut m,
            st,
            vec3(1500.0, 1050.0, 0.0),
            vec3(1900.0, 1050.0, 0.0),
        );
        assert_eq!(
            m.ecke_punkte(a, vec3(2000.0, 1020.0, 0.0), q, vec3(1600.0, 1050.0, 0.0))
                .unwrap_err(),
            EckeFehler::ZuKurz
        );
    }

    /// Verlängern quer: bis an die zugewandte Fläche, auch schräg und als
    /// Kürzen; das andere Ende bleibt. Danach steht ein T.
    #[test]
    fn verlaengern_bis_an_die_flaeche() {
        let (mut m, st, t) = haus();
        let aw = m.wall_at(m.runs().ids().next().unwrap(), 1).unwrap();
        // senkrecht nach oben an die Innenfläche der oberen Außenwand
        let a = innenwand(
            &mut m,
            st,
            vec3(3000.0, 2000.0, 0.0),
            vec3(3000.0, 5000.0, 0.0),
        );
        m.verlaengern(a, aw).unwrap();
        let (p0, p1) = enden(&m, a);
        assert!(nah(p0, vec3(3000.0, 2000.0, 0.0)), "Anfang bleibt");
        assert!(nah(p1, vec3(3000.0, 8000.0 - t, 0.0)), "{p1:?}");
        let tj: Vec<_> = m
            .joins()
            .iter()
            .filter(|j| j.kind == JoinKind::T && j.b == aw)
            .collect();
        assert_eq!(tj.len(), 1, "T an der Außenwand");

        // Innenwand als Ziel: die Fläche, die dem festen Ende zugewandt ist
        let ziel = innenwand(
            &mut m,
            st,
            vec3(5000.0, 1000.0, 0.0),
            vec3(5000.0, 6000.0, 0.0),
        );
        let ti: f64 = m
            .wall_layers(m.defaults().interior_wall)
            .iter()
            .map(|l| l.thickness)
            .sum();
        let b = innenwand(
            &mut m,
            st,
            vec3(7000.0, 3000.0, 0.0),
            vec3(8000.0, 3000.0, 0.0),
        );
        m.verlaengern(b, ziel).unwrap();
        let (q0, q1) = enden(&m, b);
        assert!(nah(q0, vec3(5000.0 + ti / 2.0, 3000.0, 0.0)), "{q0:?}");
        assert!(nah(q1, vec3(8000.0, 3000.0, 0.0)), "Ende bleibt");

        // Schräg darüber hinaus: gekürzt bis an die Fläche auf der Seite des
        // festen Endes
        let c = innenwand(
            &mut m,
            st,
            vec3(6500.0, 1000.0, 0.0),
            vec3(4500.0, 3000.0, 0.0),
        );
        m.verlaengern(c, ziel).unwrap();
        let (c0, c1) = enden(&m, c);
        let x = 5000.0 + ti / 2.0;
        assert!(nah(c0, vec3(6500.0, 1000.0, 0.0)), "Anfang bleibt");
        assert!(nah(c1, vec3(x, 7500.0 - x, 0.0)), "{c1:?}");
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Verlängern in einer Flucht: bis an das nähere Ende der Zielwand;
    /// parallel daneben geht es nicht.
    #[test]
    fn verlaengern_in_einer_flucht() {
        let (mut m, st, _) = haus();
        let a = innenwand(
            &mut m,
            st,
            vec3(1000.0, 3000.0, 0.0),
            vec3(3000.0, 3000.0, 0.0),
        );
        let b = innenwand(
            &mut m,
            st,
            vec3(4500.0, 3020.0, 0.0),
            vec3(7000.0, 3020.0, 0.0),
        );
        m.verlaengern(a, b).unwrap();
        assert!(
            nah(enden(&m, a).1, vec3(4500.0, 3000.0, 0.0)),
            "{:?}",
            enden(&m, a)
        );
        assert!(m.check().is_empty(), "{:?}", m.check());
        let c = innenwand(
            &mut m,
            st,
            vec3(1000.0, 5000.0, 0.0),
            vec3(3000.0, 5000.0, 0.0),
        );
        let d = innenwand(
            &mut m,
            st,
            vec3(4000.0, 5600.0, 0.0),
            vec3(6000.0, 5600.0, 0.0),
        );
        assert_eq!(m.verlaengern(c, d).unwrap_err(), EckeFehler::Parallel);
        assert_eq!(m.verlaengern(c, c).unwrap_err(), EckeFehler::EinZug);
    }

    #[test]
    fn wandende_wandert() {
        let (mut m, st, _) = haus();
        let w = innenwand(
            &mut m,
            st,
            vec3(2000.0, 2000.0, 0.0),
            vec3(2000.0, 5000.0, 0.0),
        );
        let e = m.wand_enden(w);
        assert_eq!(e.len(), 2);
        assert!(nah(e[1].1, vec3(2000.0, 5000.0, 0.0)));
        let (run, pts) = m.ende_punkte(w, 1, vec3(2000.0, 6000.0, 0.0)).unwrap();
        assert!(nah(pts[1], vec3(2000.0, 6000.0, 0.0)) && nah(pts[0], vec3(2000.0, 2000.0, 0.0)));
        assert!(m.set_run_points(run, &pts).is_some());
        assert_eq!(
            m.ende_punkte(w, 0, vec3(2000.0, 5950.0, 0.0)),
            Err(EckeFehler::ZuKurz)
        );
        // Außenwand (geschlossen): keine Enden
        let aw = m
            .elements()
            .iter()
            .find(|(_, e)| e.category == Category::ExteriorWall)
            .map(|(id, _)| id)
            .unwrap();
        assert!(m.wand_enden(aw).is_empty());
        assert_eq!(
            m.ende_punkte(aw, 0, vec3(0.0, 0.0, 0.0)),
            Err(EckeFehler::KeinEnde)
        );
    }
}

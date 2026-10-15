//! Ecke bilden und Verlängern (Grundriss G5, Jörn 11.10. 16:47, J4 und J5):
//! Methoden von `Model`, aus `model.rs` ausgelagert.
//!
//! Beide ändern nur freie Enden offener Züge: Ein Ende läuft auf seiner
//! Bezugslinie bis zum Ziel, die Richtung der Wand bleibt. Bei einer
//! geteilten Wand gilt die ganze Flucht ihrer Stücke, es wandert das freie
//! Ende des Zuges. Die Anschlüsse (L an der Ecke, T an der Fläche) entstehen
//! danach wie beim Zeichnen aus den Punkten (B5a). Die App setzt die Punkte
//! aus [`Model::ecke_punkte`] und [`Model::verlaengern_punkte`] in einem
//! offenen Schritt und rollt ihn zurück, wenn ein Zug sie ablehnt.

use super::*;
use crate::fuge::MIN_PIECE;
use crate::join;
use crate::wall::{cross2, right_of};

/// Kosinus von 10°: Wandende ziehen lehnt ab, wenn das Segment so eng auf
/// das Segment davor zurückläuft.
const FALTEN_COS: f64 = 0.984_807_753_012_208;

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
    /// Verlängern: Die Bezugslinie läuft an der Zielwand vorbei.
    Daneben,
    /// Wandende ziehen: Das Segment liefe auf das Segment davor zurück.
    Faltet,
    /// Wandende ziehen: So spitz und kurz überlappten sich die Wandkörper
    /// an der Ecke.
    Spitz,
    /// Die Wände liegen in verschiedenen Geschossen.
    Geschoss,
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
            EckeFehler::Daneben => "Die Wand liefe an der Zielwand vorbei.",
            EckeFehler::Faltet => "Die Wand läge auf der Wand davor.",
            EckeFehler::Spitz => "Zu spitz für diese Länge: Die Wände würden sich überlappen.",
            EckeFehler::Geschoss => "Die Wände liegen in verschiedenen Geschossen.",
        }
    }
}

/// Neue Punkte eines Zuges.
pub type ZugPunkte = (RunId, Vec<Vec3>);

/// Bezugslinie einer Wand oder der Flucht ihrer Stücke: Zug, Punkte am
/// Anfang und Ende, Anfang, Einheitsrichtung, Länge, die Lage der Punkte,
/// über die ein wanderndes Ende nicht hinaus darf, und ob Anfang bzw. Ende
/// freie Enden des Zuges sind.
struct Strecke {
    run: RunId,
    i: [usize; 2],
    p0: Vec3,
    d: Vec3,
    len: f64,
    /// Für das Anfangsende der nächste Punkt danach, für das Endende der
    /// davor (bei einem Segment: Länge bzw. 0).
    innen: [f64; 2],
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
    /// Bezugslinie des Segments der Wand.
    fn strecke(&self, wall: ElementId) -> Result<Strecke, EckeFehler> {
        let (run, seg) = self.segment_of(wall).ok_or(EckeFehler::KeineWand)?;
        let r = self.run(run).ok_or(EckeFehler::KeineWand)?;
        let n = r.points.len();
        let j = (seg + 1) % n.max(1);
        let (a, b) = match (r.points.get(seg), r.points.get(j)) {
            (Some(a), Some(b)) if r.closed || seg + 1 < n => (flach(*a), flach(*b)),
            _ => return Err(EckeFehler::KeineWand),
        };
        let len = (b - a).length();
        if len < 1e-6 {
            return Err(EckeFehler::KeineWand);
        }
        Ok(Strecke {
            run,
            i: [seg, j],
            p0: a,
            d: (b - a) * (1.0 / len),
            len,
            innen: [len, 0.0],
            frei: [!r.closed && seg == 0, !r.closed && seg + 2 == n],
        })
    }

    /// Bezugslinie der Wand über die Fugen ihres offenen Zuges hinweg, so
    /// weit die Stücke in einer Flucht liegen (geteilte Wand, Review #82 4).
    fn flucht(&self, wall: ElementId) -> Result<Strecke, EckeFehler> {
        let s = self.strecke(wall)?;
        let r = self.run(s.run).ok_or(EckeFehler::KeineWand)?;
        if r.closed {
            return Ok(s);
        }
        let p: Vec<Vec3> = r.points.iter().map(|q| flach(*q)).collect();
        let gerade = |a: usize, b: usize| {
            let e = p[b] - p[a];
            let l = e.length();
            l > 1e-6 && cross2(e, s.d).abs() < 1e-6 * l && e.dot(s.d) > 0.0
        };
        let (mut i0, mut i1) = (s.i[0], s.i[1]);
        while i0 > 0 && gerade(i0 - 1, i0) {
            i0 -= 1;
        }
        while i1 + 1 < p.len() && gerade(i1, i1 + 1) {
            i1 += 1;
        }
        if (i0, i1) == (s.i[0], s.i[1]) {
            return Ok(s);
        }
        let len = (p[i1] - p[i0]).length();
        let mut f = Strecke {
            run: s.run,
            i: [i0, i1],
            p0: p[i0],
            d: s.d,
            len,
            innen: [0.0; 2],
            frei: [i0 == 0, i1 + 1 == p.len()],
        };
        f.innen = [f.bei(p[i0 + 1]), f.bei(p[i1 - 1])];
        Ok(f)
    }

    /// Punkte des Zuges von `s`, dessen Ende `ende` (0 Anfang, 1 Ende der
    /// Strecke) an der Lage `bei` liegt; prüft freies Ende und Mindestlänge.
    fn mit_ende(&self, s: &Strecke, ende: usize, bei: f64) -> Result<ZugPunkte, EckeFehler> {
        if !s.frei[ende] {
            return Err(EckeFehler::KeinEnde);
        }
        let rest = if ende == 0 {
            s.innen[0] - bei
        } else {
            bei - s.innen[1]
        };
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
        pts[s.i[ende]] = s.punkt(bei);
        Ok((s.run, pts))
    }

    /// Beide Wände in einem Geschoss, aber nicht in einem Zug.
    fn zwei_zuege(&self, a: &Strecke, b: &Strecke) -> Result<(), EckeFehler> {
        if a.run == b.run {
            return Err(EckeFehler::EinZug);
        }
        let g = |s: &Strecke| self.run(s.run).map(|r| r.storey);
        if g(a) != g(b) {
            return Err(EckeFehler::Geschoss);
        }
        Ok(())
    }

    /// Hat die Wand oder die Flucht ihrer Stücke ein freies Ende (Anfang oder
    /// Ende eines offenen Zuges)? Nur dann gehen „Ecke bilden“ und
    /// „Verlängern“.
    pub fn freies_ende(&self, wall: ElementId) -> bool {
        self.flucht(wall).is_ok_and(|s| s.frei.iter().any(|f| *f))
    }

    /// Neue Punkte für [`Model::ecke_bilden`], ohne etwas zu ändern (Vorschau).
    pub fn ecke_punkte(
        &self,
        a: ElementId,
        a_bleibt: Vec3,
        b: ElementId,
        b_bleibt: Vec3,
    ) -> Result<[ZugPunkte; 2], EckeFehler> {
        let (sa, sb) = (self.flucht(a)?, self.flucht(b)?);
        self.zwei_zuege(&sa, &sb)?;
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
    /// Lehnt der zweite Zug ab, ist der erste schon gesetzt: im offenen
    /// Schritt zurückrollen.
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

    /// Neue Punkte für [`Model::verlaengern`], ohne etwas zu ändern
    /// (Vorschau); werden zwei Wände eine, die Punkte des Zuges, der bleibt.
    pub fn verlaengern_punkte(
        &self,
        a: ElementId,
        ziel: ElementId,
    ) -> Result<ZugPunkte, EckeFehler> {
        if let Some(v) = self.vereinen_punkte(a, ziel) {
            return Ok(v.zug);
        }
        let (sa, sb) = (self.flucht(a)?, self.flucht(ziel)?);
        self.zwei_zuege(&sa, &sb)?;
        if let Some(tx) = sa.schnitt(sb.p0, sb.d) {
            // Quer oder schräg: Das Ende näher am Schnittpunkt läuft bis an
            // die Fläche der Zielwand, die dem anderen Ende zugewandt ist
            let (lo, hi) = self.base_chain(sb.run).ok_or(EckeFehler::KeineWand)?.span();
            // Nur, wenn die Bezugslinie die Zielwand trifft (über ihre Enden
            // hinaus höchstens um ihre Dicke, für Gehrung und Anschluss)
            let tb = sb.bei(sa.punkt(tx));
            let e = hi - lo;
            if tb < -e || tb > sb.len + e {
                return Err(EckeFehler::Daneben);
            }
            let ende = usize::from(tx.abs() > (tx - sa.len).abs());
            let fest = sa.punkt(if ende == 0 { sa.len } else { 0.0 });
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
    /// In einer Flucht mit gleichem Typ werden beide eine Wand
    /// ([`Model::vereinen_punkte`]). Lehnt es ab, bleibt alles, wie es war.
    pub fn verlaengern(&mut self, a: ElementId, ziel: ElementId) -> Result<(), EckeFehler> {
        if let Some(v) = self.vereinen_punkte(a, ziel) {
            return self.vereinen(&v);
        }
        let (run, pts) = self.verlaengern_punkte(a, ziel)?;
        self.set_run_points(run, &pts)
            .map(|_| ())
            .ok_or(EckeFehler::Ungueltig)
    }

    /// Verlängern in einer Flucht mit gleichem Typ (J5, Plan G5c): Statt
    /// stumpf anzustoßen, werden beide Wände eine. `None`, wenn sie stumpf
    /// anstoßen: anderer Typ, andere Kategorie oder Höhe, die Körper liegen
    /// nicht deckungsgleich in der Flucht, ein Zug steht in einem Stapel,
    /// die Enden an der Stoßstelle sind keine freien Enden, oder beide Züge
    /// haben mehr als eine Wand.
    ///
    /// Einer der beiden Züge ist eine einzelne Wand; sie fällt weg, und das
    /// Ende des anderen Zuges läuft bis an ihr fernes Ende. Sind beide
    /// einzelne Wände, fällt die kürzere weg. Ist die wegfallende Wand
    /// länger als die Wand, in der sie aufgeht, gibt sie ihr Nummer und
    /// Kennung (Regel 11).
    pub fn vereinen_punkte(&self, a: ElementId, ziel: ElementId) -> Option<Vereint> {
        let (sa, sb) = (self.flucht(a).ok()?, self.flucht(ziel).ok()?);
        self.zwei_zuege(&sa, &sb).ok()?;
        if sa.schnitt(sb.p0, sb.d).is_some() {
            return None;
        }
        // Lage von `b` auf der Linie von `a`: welches Ende von `a` wandert,
        // welches Ende von `b` liegt ihm gegenüber
        let (u0, u1) = (sa.bei(sb.p0), sa.bei(sb.punkt(sb.len)));
        let (ea, eb) = if u0.min(u1) >= sa.len {
            (1, usize::from(u1 < u0))
        } else if u0.max(u1) <= 0.0 {
            (0, usize::from(u1 > u0))
        } else {
            return None;
        };
        if !sa.frei[ea] || !sb.frei[eb] {
            return None;
        }
        let (ra, rb) = (self.run(sa.run)?, self.run(sb.run)?);
        if (ra.base, ra.top) != (rb.base, rb.top) {
            return None;
        }
        for r in [sa.run, sb.run] {
            if !self.runs_above(r).is_empty() || self.run_below(r).is_some() {
                return None;
            }
        }
        // Die Wände an der Stoßstelle: gleicher Typ, gleiche Kategorie
        let stoss = |r: &WallRun, e: usize| {
            let k = if e == 0 {
                0
            } else {
                r.segments.len().checked_sub(1)?
            };
            let id = *r.segments.get(k)?;
            let l = (flach(r.points[k + 1]) - flach(r.points[k])).length();
            Some((id, self.element(id)?, l))
        };
        let ((wa, xa, la), (wb, xb, lb)) = (stoss(ra, ea)?, stoss(rb, eb)?);
        if (xa.category, xa.layer_set) != (xb.category, xb.layer_set) {
            return None;
        }
        // Die Bezugslinie von `b` liegt an beiden Enden gleich weit neben
        // der von `a`: in einer Flucht, nicht nur fast parallel (Review
        // #210 B2)
        let n = right_of(sa.d);
        let quer = (sb.p0 - sa.p0).dot(n);
        if ((sb.punkt(sb.len) - sa.p0).dot(n) - quer).abs() > DECKUNG {
            return None;
        }
        // Die Körper liegen deckungsgleich in der Flucht (die Bezugsseiten
        // dürfen verschieden sein); quer gemessen rechts von `a`
        let gegen = sa.d.dot(sb.d) < 0.0;
        let (ca, cb) = (self.base_chain(sa.run)?, self.base_chain(sb.run)?);
        let quer_b = |o: f64| (if gegen { -o } else { o }) + quer;
        let ((lo_a, hi_a), (lo_b, hi_b)) = (ca.span(), cb.span());
        let (lo_b, hi_b) = (
            quer_b(lo_b).min(quer_b(hi_b)),
            quer_b(lo_b).max(quer_b(hi_b)),
        );
        if (lo_b - lo_a).abs() > DECKUNG || (hi_b - hi_a).abs() > DECKUNG {
            return None;
        }
        // Die Schichten liegen von der Außenfläche aus (Öffnungen sitzen
        // dort): Liegen die Außenflächen auf verschiedenen Seiten, müssen
        // die Schichten von beiden Seiten gleich liegen (Review #210 B3)
        let (aussen_a, aussen_b) = (ca.outer_offset(), quer_b(cb.outer_offset()));
        let gespiegelt = (aussen_a - aussen_b).abs() > DECKUNG;
        if gespiegelt {
            let s = self.layer_set(xa.layer_set?)?;
            if !s.layers.iter().eq(s.layers.iter().rev()) {
                return None;
            }
        }
        // Es fällt die einzelne Wand weg, von zwei einzelnen die kürzere
        let einzeln = |r: &WallRun| r.segments.len() == 1;
        let a_weg = match (einzeln(ra), einzeln(rb)) {
            (true, true) => la < lb,
            (true, false) => true,
            (false, true) => false,
            (false, false) => return None,
        };
        let (s, e, weg, fern, l_bleibt, l_weg, w_weg) = if a_weg {
            let fern = sa.punkt(if ea == 0 { sa.len } else { 0.0 });
            (&sb, eb, sa.run, fern, lb, la, wa)
        } else {
            let fern = sb.punkt(if eb == 0 { sb.len } else { 0.0 });
            (&sa, ea, sb.run, fern, la, lb, wb)
        };
        let zug = self.mit_ende(s, e, s.bei(fern)).ok()?;
        let seg = if e == 0 { 0 } else { zug.1.len() - 2 };
        // Öffnungen der wegfallenden Wand kommen auf die Außenfläche des
        // Zuges, der bleibt
        let versatz = match (gespiegelt, a_weg) {
            (false, _) => vec3(0.0, 0.0, 0.0),
            (true, true) => n * (aussen_b - aussen_a),
            (true, false) => n * (aussen_a - aussen_b),
        };
        Some(Vereint {
            zug,
            weg,
            seg,
            kennung: (l_weg > l_bleibt).then_some(w_weg),
            gegen,
            versatz,
        })
    }

    /// Setzt [`Vereint`]: Der eine Zug fällt weg, der andere bekommt die
    /// neuen Punkte. Erst geprüft, dann geändert: Lehnt es ab, bleibt alles.
    /// Öffnungen der wegfallenden Wand bleiben, wo sie sind, bei
    /// gespiegelter Außenfläche auf der neuen; die Abweichungen des
    /// Raumbuchs an ihren Wandseiten gehen auf die vereinte Wand über
    /// (Review #210 B3, B4).
    fn vereinen(&mut self, v: &Vereint) -> Result<(), EckeFehler> {
        let (run, pts) = &v.zug;
        if !self.stack_fits(*run, pts) {
            return Err(EckeFehler::Ungueltig);
        }
        let guid = |m: &Model, w: Option<ElementId>| m.element(w?).map(|e| e.guid);
        let bleibt = self.wall_at(*run, v.seg);
        let weg = self.wall_at(v.weg, 0);
        let (g_bleibt, g_weg) = (guid(self, bleibt), guid(self, weg));
        let kennung = v.kennung.and_then(|w| self.element(w)).cloned();
        let tueren = self.oeffnungen_versetzt(v);
        if !self.remove_run(v.weg) || self.set_run_points(*run, pts).is_none() {
            return Err(EckeFehler::Ungueltig);
        }
        for (id, path) in tueren {
            if let Some(ElementKind::Opening(o)) = self.elements.get(id).map(|e| &e.kind) {
                let o = crate::oeffnung::Opening { path, ..o.clone() };
                note!(self, Element, self.elements, id);
                if let Some(e) = self.elements.get_mut(id) {
                    e.kind = ElementKind::Opening(o);
                }
            }
        }
        let w = self.wall_at(*run, v.seg).ok_or(EckeFehler::Ungueltig)?;
        if let Some(k) = kennung {
            // Die längere, wegfallende Wand gibt Nummer und Kennung weiter;
            // ihre Merkmale gehen vor, die der bleibenden bleiben daneben
            note!(self, Element, self.elements, w);
            if let Some(e) = self.elements.get_mut(w) {
                e.guid = k.guid;
                e.number = k.number;
                let mut props = std::mem::take(&mut e.props);
                props.extend(k.props);
                e.props = props;
                e.seq = k.seq;
            }
        }
        // Raumbuch: Wandseiten links bzw. rechts der Zeichenrichtung; die
        // wegfallende Wand lief gegenläufig, ihre Seiten tauschen
        let neu = self.element(w).map(|e| e.guid);
        if let (Some(neu), Some(g_bleibt), Some(g_weg)) = (neu, g_bleibt, g_weg) {
            if neu == g_weg {
                if v.gegen {
                    self.ausbau_wand_vereint(g_weg, neu, true);
                }
                self.ausbau_wand_vereint(g_bleibt, neu, false);
            } else {
                self.ausbau_wand_vereint(g_weg, neu, v.gegen);
            }
        }
        self.touch();
        Ok(())
    }

    /// Öffnungen auf der Außenfläche der wegfallenden Wand, um
    /// [`Vereint::versatz`] verschoben; leer, wenn sie bleiben, wo sie sind.
    fn oeffnungen_versetzt(&self, v: &Vereint) -> Vec<(ElementId, Vec<Vec3>)> {
        if v.versatz.length() < 1e-9 {
            return Vec::new();
        }
        let (Some(r), Some(c)) = (self.run(v.weg), self.base_chain(v.weg)) else {
            return Vec::new();
        };
        let (a, b) = (flach(r.points[0]), flach(r.points[r.points.len() - 1]));
        let len = (b - a).length();
        if len < 1e-6 {
            return Vec::new();
        }
        let d = (b - a) * (1.0 / len);
        let (n, aussen) = (right_of(d), c.outer_offset());
        let auf = |p: Vec3| {
            let u = (flach(p) - a).dot(d);
            ((flach(p) - a).dot(n) - aussen).abs() < 1.0 && u > -1.0 && u < len + 1.0
        };
        self.elements
            .iter()
            .filter_map(|(id, e)| match &e.kind {
                ElementKind::Opening(o)
                    if e.storey == r.storey && o.path.iter().all(|p| auf(*p)) =>
                {
                    Some((id, o.path.iter().map(|p| *p + v.versatz).collect()))
                }
                _ => None,
            })
            .collect()
    }
}

/// Wie weit die Körper zweier Wände in einer Flucht quer auseinander liegen
/// dürfen, um eine Wand zu werden (mm).
const DECKUNG: f64 = 0.5;

/// Zwei Wände in einer Flucht werden eine ([`Model::vereinen_punkte`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Vereint {
    /// Der Zug, der bleibt, mit seinen neuen Punkten.
    pub zug: ZugPunkte,
    /// Der Zug, der wegfällt (eine Wand).
    pub weg: RunId,
    /// Segment der vereinten Wand im Zug, der bleibt.
    pub seg: usize,
    /// Die wegfallende Wand, wenn sie länger ist: Sie gibt der vereinten
    /// Wand Nummer und Kennung (Regel 11).
    pub kennung: Option<ElementId>,
    /// Die wegfallende Wand läuft gegen den Zug, der bleibt.
    pub gegen: bool,
    /// Verschiebung ihrer Öffnungen auf die Außenfläche des Zuges, der
    /// bleibt (null, wenn die Außenflächen zusammenfallen).
    pub versatz: Vec3,
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
    /// Segment wird nicht kürzer als 10 cm und faltet nicht auf das Segment
    /// davor zurück (Review #82 3); an einer spitzen Ecke müssen beide
    /// Segmente so lang sein, dass sich die Körper nicht überlappen (H1).
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
        let u = flach(p) - fest;
        if u.length() < MIN_PIECE {
            return Err(EckeFehler::ZuKurz);
        }
        let mut pts: Vec<Vec3> = self
            .run(s.run)
            .ok_or(EckeFehler::KeineWand)?
            .points
            .iter()
            .map(|q| flach(*q))
            .collect();
        // Segment davor: am festen Punkt
        let davor = if ende == 1 {
            s.i[0].checked_sub(1)
        } else {
            Some(s.i[1] + 1).filter(|k| *k < pts.len())
        };
        if let Some(v) = davor.map(|k| pts[k] - fest) {
            let (lu, lv) = (u.length(), v.length());
            let cos = u.dot(v) / (lu * lv).max(1e-9);
            if cos > FALTEN_COS {
                return Err(EckeFehler::Faltet);
            }
            // Die innere Fläche läuft an der Ecke um w / tan(φ/2) an beiden
            // Segmenten entlang (w: Abstand der inneren Fläche von der
            // Bezugslinie); ist eins kürzer, kippt sein Körper über die Ecke
            let (lo, hi) = self.base_chain(s.run).ok_or(EckeFehler::KeineWand)?.span();
            // Drehsinn in Zeichenrichtung des Zuges: links innen negativ
            let (da, db) = if ende == 1 { (-v, u) } else { (-u, v) };
            let w = if cross2(da, db) > 0.0 { -lo } else { hi }.max(0.0);
            let halb = cos.clamp(-1.0, 1.0).acos() * 0.5;
            if w / halb.tan() > lu.min(lv) {
                return Err(EckeFehler::Spitz);
            }
        }
        pts[s.i[ende]] = flach(p);
        // Jede Fläche jeder Schicht muss an jedem Segment vorwärts laufen:
        // Über der Gehrungsgrenze springt die Ecke der äußeren Fläche auf
        // die Normale des Nachbarsegments, und der Körper kippt auch dann,
        // wenn die Bezugslinie innen liegt (Review #87 B5)
        let mut c = self.base_chain(s.run).ok_or(EckeFehler::KeineWand)?;
        c.points = pts.clone();
        if !c.gekippte_segmente().is_empty() {
            return Err(EckeFehler::Spitz);
        }
        Ok((s.run, pts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
    use crate::{PropValue, Space};

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

    /// Innenwände im Geschoss.
    fn innenwaende(m: &Model) -> usize {
        m.elements()
            .iter()
            .filter(|(_, e)| e.category == Category::InteriorWall)
            .count()
    }

    /// Verlängern in einer Flucht mit gleichem Typ (J5, G5c): Beide Wände
    /// werden eine. Die kürzere fällt weg, die längere läuft bis an ihr
    /// fernes Ende und behält Nummer und Kennung, auch gegenläufig
    /// gezeichnet und wenn die längere die ist, die wegfällt (Regel 11).
    #[test]
    fn verlaengern_vereint_gleichen_typ() {
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
            vec3(4500.0, 3000.0, 0.0),
            vec3(7000.0, 3000.0, 0.0),
        );
        let nr_b = m.element(b).unwrap().number.clone();
        let n = innenwaende(&m);
        let v = m.vereinen_punkte(a, b).unwrap();
        assert_eq!(m.verlaengern_punkte(a, b).unwrap(), v.zug, "Vorschau");
        m.verlaengern(a, b).unwrap();
        assert!(m.segment_of(a).is_none(), "a geht in b auf");
        assert_eq!(innenwaende(&m), n - 1);
        assert!(
            nah(enden(&m, b).0, vec3(1000.0, 3000.0, 0.0)),
            "{:?}",
            enden(&m, b)
        );
        assert!(
            nah(enden(&m, b).1, vec3(7000.0, 3000.0, 0.0)),
            "{:?}",
            enden(&m, b)
        );
        assert_eq!(m.element(b).unwrap().number, nr_b);
        assert!(m.check().is_empty(), "{:?}", m.check());

        // Gegenläufig gezeichnet: d fällt weg, c läuft bis an sein fernes Ende
        let c = innenwand(
            &mut m,
            st,
            vec3(1000.0, 5000.0, 0.0),
            vec3(4000.0, 5000.0, 0.0),
        );
        let d = innenwand(
            &mut m,
            st,
            vec3(7000.0, 5000.0, 0.0),
            vec3(5000.0, 5000.0, 0.0),
        );
        m.verlaengern(d, c).unwrap();
        assert!(m.segment_of(d).is_none());
        let (run, _) = m.segment_of(c).unwrap();
        assert_eq!(m.run(run).unwrap().points.len(), 2);
        assert!(
            nah(enden(&m, c).1, vec3(7000.0, 5000.0, 0.0)),
            "{:?}",
            enden(&m, c)
        );
        assert!(m.check().is_empty(), "{:?}", m.check());

        // Ein Zug mit Ecke und kurzem Schenkel in der Flucht, dazu eine lange
        // Wand: die lange fällt weg und gibt der vereinten Nummer und Kennung
        let iw = m.defaults().interior_wall;
        let r = m
            .add_wall_run(
                &[
                    vec3(1000.0, 7000.0, 0.0),
                    vec3(1000.0, 6000.0, 0.0),
                    vec3(2000.0, 6000.0, 0.0),
                ],
                false,
                RefSide::Center,
                st,
                iw,
                Category::InteriorWall,
            )
            .unwrap();
        let e = m.wall_at(r, 1).unwrap();
        let f = innenwand(
            &mut m,
            st,
            vec3(3000.0, 6000.0, 0.0),
            vec3(8000.0, 6000.0, 0.0),
        );
        let (nr_f, guid_f) = {
            let x = m.element(f).unwrap();
            (x.number.clone(), x.guid)
        };
        let v = m.vereinen_punkte(e, f).unwrap();
        assert_eq!((v.zug.0, v.seg, v.kennung), (r, 1, Some(f)));
        m.verlaengern(e, f).unwrap();
        assert!(m.segment_of(f).is_none());
        let x = m.element(e).unwrap();
        assert_eq!((x.number.clone(), x.guid), (nr_f, guid_f));
        assert!(nah(m.run(r).unwrap().points[2], vec3(8000.0, 6000.0, 0.0)));
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Anderer Typ, versetzte Körper oder zwei Züge mit Ecken: stumpfer
    /// Stoß wie bisher, beide Wände bleiben.
    #[test]
    fn verlaengern_stoesst_stumpf_an() {
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
            vec3(4500.0, 3000.0, 0.0),
            vec3(7000.0, 3000.0, 0.0),
        );
        let t = m
            .duplicate_type(m.element(b).unwrap().layer_set.unwrap())
            .unwrap();
        if let Some(e) = m.elements.get_mut(b) {
            e.layer_set = Some(t);
        }
        assert_eq!(m.vereinen_punkte(a, b), None);
        m.verlaengern(a, b).unwrap();
        assert!(m.segment_of(b).is_some());
        assert!(nah(enden(&m, a).1, vec3(4500.0, 3000.0, 0.0)));

        // Gleicher Typ, aber Bezug links: der Körper liegt daneben
        let iw = m.defaults().interior_wall;
        let links = |m: &mut Model, p: [Vec3; 2]| {
            let r = m
                .add_wall_run(&p, false, RefSide::Left, st, iw, Category::InteriorWall)
                .unwrap();
            m.wall_at(r, 0).unwrap()
        };
        let c = innenwand(
            &mut m,
            st,
            vec3(1000.0, 5000.0, 0.0),
            vec3(3000.0, 5000.0, 0.0),
        );
        let d = links(
            &mut m,
            [vec3(4500.0, 5000.0, 0.0), vec3(7000.0, 5000.0, 0.0)],
        );
        assert_eq!(m.vereinen_punkte(c, d), None);

        // Zwei Züge mit Ecken: stumpf
        let zug = |m: &mut Model, p: [Vec3; 3]| {
            m.add_wall_run(&p, false, RefSide::Center, st, iw, Category::InteriorWall)
                .unwrap()
        };
        let g = zug(
            &mut m,
            [
                vec3(1000.0, 7500.0, 0.0),
                vec3(1000.0, 6500.0, 0.0),
                vec3(2000.0, 6500.0, 0.0),
            ],
        );
        let h = zug(
            &mut m,
            [
                vec3(3000.0, 6500.0, 0.0),
                vec3(5000.0, 6500.0, 0.0),
                vec3(5000.0, 7500.0, 0.0),
            ],
        );
        let (g, h) = (m.wall_at(g, 1).unwrap(), m.wall_at(h, 0).unwrap());
        assert_eq!(m.vereinen_punkte(g, h), None);
        m.verlaengern(g, h).unwrap();
        assert!(m.segment_of(h).is_some());
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Verschiedene Bezugsseiten, gegenläufig gezeichnet: Liegen die Körper
    /// deckungsgleich, werden sie eine Wand; die Punkte kommen auf die
    /// Bezugslinie des Zuges, der bleibt.
    #[test]
    fn vereint_ueber_bezugsseiten() {
        let (mut m, st, _) = haus();
        let iw = m.defaults().interior_wall;
        let ti = m
            .layer_set(iw)
            .map(|s| s.layers.iter().map(|l| l.thickness).sum::<f64>())
            .unwrap();
        let c = innenwand(
            &mut m,
            st,
            vec3(1000.0, 4000.0, 0.0),
            vec3(3000.0, 4000.0, 0.0),
        );
        // Gegen x gezeichnet liegt der Körper bei Bezug links über der Linie
        let y = 4000.0 - ti / 2.0;
        let r = m
            .add_wall_run(
                &[vec3(7000.0, y, 0.0), vec3(4500.0, y, 0.0)],
                false,
                RefSide::Left,
                st,
                iw,
                Category::InteriorWall,
            )
            .unwrap();
        let e = m.wall_at(r, 0).unwrap();
        let v = m.vereinen_punkte(c, e).unwrap();
        assert_eq!((v.zug.0, v.kennung), (r, None));
        m.verlaengern(c, e).unwrap();
        assert!(m.segment_of(c).is_none());
        let p = &m.run(r).unwrap().points;
        assert!(
            nah(p[0], vec3(7000.0, y, 0.0)) && nah(p[1], vec3(1000.0, y, 0.0)),
            "{p:?}"
        );
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Eine Tür in der Wand, die wegfällt, bleibt, wo sie ist, jetzt in der
    /// vereinten Wand; rückgängig sind beide Wände wieder da.
    #[test]
    fn vereint_mit_tuer_und_rueckgaengig() {
        let (mut m, st, _) = haus();
        let a = innenwand(
            &mut m,
            st,
            vec3(1000.0, 3000.0, 0.0),
            vec3(4000.0, 3000.0, 0.0),
        );
        let b = innenwand(
            &mut m,
            st,
            vec3(5000.0, 3000.0, 0.0),
            vec3(7000.0, 3000.0, 0.0),
        );
        let (rb, _) = m.segment_of(b).unwrap();
        let c = m.chain(rb).unwrap();
        let (p, _, d) = c.layout().unwrap();
        let q = |u: f64| p[0] + d[0] * u + right_of(d[0]) * c.outer_offset();
        let tuer = Opening {
            kind: OpeningKind::Door,
            path: vec![q(500.0), q(1385.0)],
            bottom: HeightRef::Level(LevelRef::bottom(st)),
            top: HeightRef::Line(st, HeightLine::Door),
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Default::default(),
        };
        let id = m.add_opening(tuer.clone()).unwrap();
        let vorher = m.clone();
        m.begin("Wand verlängert");
        m.verlaengern(a, b).unwrap();
        let t = m.commit().unwrap();
        assert!(m.segment_of(b).is_none(), "b fällt weg");
        assert_eq!(m.opening(id).unwrap().path, tuer.path);
        assert!(m.check().is_empty(), "{:?}", m.check());
        let (ra, _) = m.segment_of(a).unwrap();
        assert_eq!(m.chain(ra).unwrap().joints.holes.len(), 1);
        m.apply(&t, Direction::Undo);
        assert_eq!(m.runs().len(), vorher.runs().len());
        assert_eq!(
            m.element(b).map(|e| e.guid),
            vorher.element(b).map(|e| e.guid)
        );
        assert!(nah(enden(&m, a).1, vec3(4000.0, 3000.0, 0.0)));
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Tür auf der Außenfläche der Wand `w` bei `u` mm ab ihrem Anfang.
    fn tuer(m: &Model, st: StoreyId, w: ElementId, u: f64) -> Opening {
        let (r, _) = m.segment_of(w).unwrap();
        let c = m.chain(r).unwrap();
        let (p, _, d) = c.layout().unwrap();
        let q = |u: f64| p[0] + d[0] * u + right_of(d[0]) * c.outer_offset();
        Opening {
            kind: OpeningKind::Door,
            path: vec![q(u), q(u + 885.0)],
            bottom: HeightRef::Level(LevelRef::bottom(st)),
            top: HeightRef::Line(st, HeightLine::Door),
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Default::default(),
        }
    }

    /// Fast in einer Flucht, aber schräg (5°): stumpf, keine vereinte Wand
    /// (Review #210 B2).
    #[test]
    fn schraeg_wird_nicht_vereint() {
        let (mut m, st, _) = haus();
        let a = innenwand(
            &mut m,
            st,
            vec3(1000.0, 3000.0, 0.0),
            vec3(3000.0, 3000.0, 0.0),
        );
        let dy = 2500.0 * 5f64.to_radians().tan();
        let b = innenwand(
            &mut m,
            st,
            vec3(4500.0, 3000.0, 0.0),
            vec3(7000.0, 3000.0 + dy, 0.0),
        );
        assert_eq!(m.vereinen_punkte(a, b), None);
        assert_eq!(m.vereinen_punkte(b, a), None);
    }

    /// Gegenläufig mit Bezug Mitte liegen die Außenflächen auf
    /// verschiedenen Seiten: Die Tür der wegfallenden Wand kommt auf die
    /// Außenfläche der vereinten (Review #210 B3). Mit Schichten, die nicht
    /// von beiden Seiten gleich liegen, bleibt es stumpf; liegen die
    /// Außenflächen dann auf einer Seite, wird es eine Wand.
    #[test]
    fn vereint_auf_der_aussenflaeche() {
        let (mut m, st, _) = haus();
        let iw = m.defaults().interior_wall;
        let ti = m
            .layer_set(iw)
            .map(|s| s.layers.iter().map(|l| l.thickness).sum::<f64>())
            .unwrap();
        let a = innenwand(
            &mut m,
            st,
            vec3(1000.0, 3000.0, 0.0),
            vec3(4000.0, 3000.0, 0.0),
        );
        let b = innenwand(
            &mut m,
            st,
            vec3(7000.0, 3000.0, 0.0),
            vec3(5000.0, 3000.0, 0.0),
        );
        let t = tuer(&m, st, b, 500.0);
        let id = m.add_opening(t.clone()).unwrap();
        let v = m.vereinen_punkte(a, b).unwrap();
        assert!(v.gegen && (v.versatz.length() - ti).abs() < 1e-6, "{v:?}");
        m.verlaengern(a, b).unwrap();
        let p = &m.opening(id).unwrap().path;
        for (q, alt) in p.iter().zip(&t.path) {
            assert!(
                (q.x - alt.x).abs() < 1e-6 && (q.y - alt.y).abs() > ti - 1e-6,
                "{p:?}"
            );
        }
        let (ra, _) = m.segment_of(a).unwrap();
        assert_eq!(m.chain(ra).unwrap().joints.holes.len(), 1);
        assert!(m.check().is_empty(), "{:?}", m.check());

        // Schichten, die nicht von beiden Seiten gleich liegen
        let mut set = m.layer_set(iw).unwrap().clone();
        let mut dazu = set.layers[0];
        dazu.thickness = 10.0;
        set.layers.insert(0, dazu);
        set.code = "IW-X".into();
        set.guid = crate::Guid(0x6a10);
        let schief = m.add_layer_set(set).unwrap();
        let wand = |m: &mut Model, p: [Vec3; 2]| {
            let r = m
                .add_wall_run(
                    &p,
                    false,
                    RefSide::Center,
                    st,
                    schief,
                    Category::InteriorWall,
                )
                .unwrap();
            m.wall_at(r, 0).unwrap()
        };
        let c = wand(
            &mut m,
            [vec3(1000.0, 5000.0, 0.0), vec3(3000.0, 5000.0, 0.0)],
        );
        let d = wand(
            &mut m,
            [vec3(7000.0, 5000.0, 0.0), vec3(4500.0, 5000.0, 0.0)],
        );
        assert_eq!(m.vereinen_punkte(c, d), None, "gespiegelt");
        let e = wand(
            &mut m,
            [vec3(1000.0, 6500.0, 0.0), vec3(3000.0, 6500.0, 0.0)],
        );
        let f = wand(
            &mut m,
            [vec3(4500.0, 6500.0, 0.0), vec3(7000.0, 6500.0, 0.0)],
        );
        assert!(m.vereinen_punkte(e, f).is_some(), "gleiche Außenseite");
    }

    /// Raumbuch: Die Abweichung an der wegfallenden Wand gilt danach an
    /// der vereinten, gegenläufig auf der anderen Seite der Zeichenrichtung;
    /// eine an derselben Stelle der bleibenden geht vor (Review #210 B4).
    #[test]
    fn vereint_nimmt_raumbuch_mit() {
        use crate::model::{AusbauAbweichung, AusbauStelle, AusbauWahl, WandSeite};
        let (mut m, st, _) = haus();
        let a = innenwand(
            &mut m,
            st,
            vec3(1000.0, 3000.0, 0.0),
            vec3(4000.0, 3000.0, 0.0),
        );
        let b = innenwand(
            &mut m,
            st,
            vec3(7000.0, 3000.0, 0.0),
            vec3(5000.0, 3000.0, 0.0),
        );
        let (ga, gb) = (m.element(a).unwrap().guid, m.element(b).unwrap().guid);
        let r = m.add_space(st, Space::neu(vec3(2000.0, 2000.0, 0.0), "Raum".into()));
        let seite = |wand, links| Some(WandSeite { wand, links });
        let ab = |stelle, wahl, hoehe| AusbauAbweichung {
            stelle,
            wahl,
            hoehe,
        };
        let keiner = Some(AusbauWahl::Keiner);
        m.set_space_finish(r, ab(AusbauStelle::Wand(seite(gb, true)), keiner, None))
            .unwrap();
        m.set_space_finish(
            r,
            ab(AusbauStelle::Fliesen(seite(gb, true)), None, Some(2000.0)),
        )
        .unwrap();
        m.set_space_finish(
            r,
            ab(AusbauStelle::Fliesen(seite(ga, false)), None, Some(1500.0)),
        )
        .unwrap();
        m.verlaengern(a, b).unwrap();
        assert_eq!(m.element(a).unwrap().guid, ga);
        let s = m.space(r).unwrap();
        assert!(s
            .ausbau
            .iter()
            .all(|x| x.stelle.seite().is_some_and(|w| w.wand == ga)));
        let wand = m
            .space_finish(r, AusbauStelle::Wand(seite(ga, false)))
            .unwrap();
        assert_eq!(wand.wahl, keiner);
        let fliesen = m
            .space_finish(r, AusbauStelle::Fliesen(seite(ga, false)))
            .unwrap();
        assert_eq!(fliesen.hoehe, Some(1500.0), "die bleibende geht vor");
        assert_eq!(s.ausbau.len(), 2);
    }

    /// Die längere Wand fällt weg und gibt der vereinten Kennung, Nummer
    /// und Merkmale (die der bleibenden bleiben daneben) und ihre
    /// Raumbuch-Abweichungen; rückgängig ist alles wie vorher.
    #[test]
    fn kennung_uebertragen_und_rueckgaengig() {
        use crate::model::{AusbauAbweichung, AusbauStelle, AusbauWahl, WandSeite};
        let (mut m, st, _) = haus();
        let iw = m.defaults().interior_wall;
        let r = m
            .add_wall_run(
                &[
                    vec3(1000.0, 7000.0, 0.0),
                    vec3(1000.0, 6000.0, 0.0),
                    vec3(2000.0, 6000.0, 0.0),
                ],
                false,
                RefSide::Center,
                st,
                iw,
                Category::InteriorWall,
            )
            .unwrap();
        let e = m.wall_at(r, 1).unwrap();
        let f = innenwand(
            &mut m,
            st,
            vec3(3000.0, 6000.0, 0.0),
            vec3(8000.0, 6000.0, 0.0),
        );
        let text = |s: &str| PropValue::Text(s.into());
        m.elements
            .get_mut(e)
            .unwrap()
            .props
            .insert("Kurz".into(), text("e"));
        m.elements
            .get_mut(e)
            .unwrap()
            .props
            .insert("Beide".into(), text("e"));
        m.elements
            .get_mut(f)
            .unwrap()
            .props
            .insert("Beide".into(), text("f"));
        let (ge, gf) = (m.element(e).unwrap().guid, m.element(f).unwrap().guid);
        let raum = m.add_space(st, Space::neu(vec3(5000.0, 5000.0, 0.0), "Raum".into()));
        let seite = |wand| Some(WandSeite { wand, links: false });
        let ab = |stelle| AusbauAbweichung {
            stelle,
            wahl: Some(AusbauWahl::Keiner),
            hoehe: None,
        };
        m.set_space_finish(raum, ab(AusbauStelle::Wand(seite(ge))))
            .unwrap();
        m.set_space_finish(raum, ab(AusbauStelle::Sockel(seite(gf))))
            .unwrap();
        let vorher = m.clone();
        m.begin("Wand verlängert");
        m.verlaengern(e, f).unwrap();
        let t = m.commit().unwrap();
        let x = m.element(e).unwrap();
        assert_eq!(x.guid, gf);
        assert_eq!(x.props.get("Kurz"), Some(&text("e")));
        assert_eq!(x.props.get("Beide"), Some(&text("f")));
        let s = m.space(raum).unwrap();
        assert_eq!(s.ausbau.len(), 2);
        assert!(
            s.ausbau.iter().all(|a| a.stelle.seite() == seite(gf)),
            "{:?}",
            s.ausbau
        );
        assert!(m.check().is_empty(), "{:?}", m.check());
        m.apply(&t, Direction::Undo);
        assert_eq!(m.element(e), vorher.element(e));
        assert_eq!(m.element(f), vorher.element(f));
        assert_eq!(m.space(raum), vorher.space(raum));
        assert_eq!(m.run(r), vorher.run(r));
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

    /// Verlängern trifft die Zielwand: Läuft die Bezugslinie an der Wand
    /// vorbei, gibt es keine Fläche und kein T; die Wand darf dann nicht bis
    /// zur gedachten Verlängerung der Zielwand ins Leere laufen.
    #[test]
    fn verlaengern_nur_bis_an_eine_getroffene_wand() {
        let (mut m, st, _) = haus();
        let a = innenwand(
            &mut m,
            st,
            vec3(3000.0, 2000.0, 0.0),
            vec3(3000.0, 5000.0, 0.0),
        );
        let ziel = innenwand(
            &mut m,
            st,
            vec3(5000.0, 7000.0, 0.0),
            vec3(6000.0, 7000.0, 0.0),
        );
        assert_eq!(
            m.verlaengern_punkte(a, ziel).unwrap_err(),
            EckeFehler::Daneben
        );
        // Knapp an ihr Ende reicht: der Wandkörper trifft noch
        let b = innenwand(
            &mut m,
            st,
            vec3(6050.0, 2000.0, 0.0),
            vec3(6050.0, 5000.0, 0.0),
        );
        assert!(m.verlaengern_punkte(b, ziel).is_ok());
    }

    /// Wandende ziehen faltet den Zug nicht zurück: Ein Ende, das auf das
    /// eigene Vorsegment zurückläuft, legt zwei Wandkörper übereinander.
    #[test]
    fn wandende_faltet_nicht_zurueck() {
        let (mut m, st, _) = haus();
        let iw = m.defaults().interior_wall;
        let r = m
            .add_wall_run(
                &[
                    vec3(1000.0, 1000.0, 0.0),
                    vec3(4000.0, 1000.0, 0.0),
                    vec3(4000.0, 1500.0, 0.0),
                ],
                false,
                RefSide::Center,
                st,
                iw,
                Category::InteriorWall,
            )
            .unwrap();
        let w = m.wall_at(r, 1).unwrap();
        assert_eq!(
            m.ende_punkte(w, 1, vec3(2000.0, 1000.0, 0.0)),
            Err(EckeFehler::Faltet),
            "Ende liegt auf dem Vorsegment"
        );
        // Auch der Anfang des ersten Segments nicht auf das zweite
        let w0 = m.wall_at(r, 0).unwrap();
        assert_eq!(
            m.ende_punkte(w0, 0, vec3(4000.0, 2500.0, 0.0)),
            Err(EckeFehler::Faltet)
        );
        // Spitz, aber nicht deckungsgleich, geht
        assert!(m.ende_punkte(w, 1, vec3(2000.0, 1500.0, 0.0)).is_ok());
    }

    /// Geteilte Wand: Stück 0 hat an der Fuge kein freies Ende; Verlängern
    /// und Ecke bilden nehmen das freie Ende des Zuges in der Flucht.
    #[test]
    fn geteilte_wand_wandert_am_zugende() {
        let (mut m, st, t) = haus();
        let aw = m.wall_at(m.runs().ids().next().unwrap(), 1).unwrap();
        let a = innenwand(
            &mut m,
            st,
            vec3(3000.0, 2000.0, 0.0),
            vec3(3000.0, 5000.0, 0.0),
        );
        m.split_wall(a, &[1000.0]).unwrap();
        let (run, _) = m.segment_of(a).unwrap();
        assert_eq!(m.run(run).unwrap().points.len(), 3);
        assert!(m.freies_ende(a));
        m.verlaengern(a, aw).unwrap();
        let p = &m.run(run).unwrap().points;
        assert!(nah(p[0], vec3(3000.0, 2000.0, 0.0)), "{p:?}");
        assert!(nah(p[1], vec3(3000.0, 3000.0, 0.0)), "Fuge bleibt: {p:?}");
        assert!(nah(p[2], vec3(3000.0, 8000.0 - t, 0.0)), "{p:?}");
        // Kürzen über die Fuge hinaus geht nicht
        let quer = innenwand(
            &mut m,
            st,
            vec3(2000.0, 2500.0, 0.0),
            vec3(2800.0, 2500.0, 0.0),
        );
        assert_eq!(
            m.ecke_punkte(
                a,
                vec3(3000.0, 2200.0, 0.0),
                quer,
                vec3(2500.0, 2500.0, 0.0)
            )
            .unwrap_err(),
            EckeFehler::ZuKurz
        );
        // Ecke am freien Anfang des Zuges
        let b = m.wall_at(run, 1).unwrap();
        let unten = innenwand(
            &mut m,
            st,
            vec3(1000.0, 1000.0, 0.0),
            vec3(2500.0, 1000.0, 0.0),
        );
        m.ecke_bilden(
            b,
            vec3(3000.0, 6000.0, 0.0),
            unten,
            vec3(1500.0, 1000.0, 0.0),
        )
        .unwrap();
        let p = &m.run(run).unwrap().points;
        assert!(nah(p[0], vec3(3000.0, 1000.0, 0.0)), "{p:?}");
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Wände in verschiedenen Geschossen bilden keine Ecke.
    #[test]
    fn verschiedene_geschosse() {
        let (mut m, st, _) = haus();
        let a = innenwand(
            &mut m,
            st,
            vec3(2000.0, 1000.0, 0.0),
            vec3(2000.0, 4000.0, 0.0),
        );
        let b2 = m.add_building(0);
        let st2 = m.ground_of(Some(b2)).unwrap();
        assert_ne!(st, st2);
        let b = innenwand(
            &mut m,
            st2,
            vec3(3000.0, 5000.0, 0.0),
            vec3(6000.0, 5000.0, 0.0),
        );
        let mitte = vec3(2000.0, 2000.0, 0.0);
        assert_eq!(
            m.ecke_punkte(a, mitte, b, vec3(5000.0, 5000.0, 0.0))
                .unwrap_err(),
            EckeFehler::Geschoss
        );
        assert_eq!(
            m.verlaengern_punkte(a, b).unwrap_err(),
            EckeFehler::Geschoss
        );
    }

    /// Spitze Ecke beim Wandende (Review #82 H1): Der Körper des kurzen
    /// Segments kippte über die Ecke; lang genug geht es.
    #[test]
    fn spitze_ecke_nur_lang_genug() {
        let (mut m, st, _) = haus();
        let iw = m.defaults().interior_wall;
        let r = m
            .add_wall_run(
                &[
                    vec3(1000.0, 1000.0, 0.0),
                    vec3(4000.0, 1000.0, 0.0),
                    vec3(4000.0, 1500.0, 0.0),
                ],
                false,
                RefSide::Center,
                st,
                iw,
                Category::InteriorWall,
            )
            .unwrap();
        let w = m.wall_at(r, 1).unwrap();
        let t: f64 = m.wall_layers(iw).iter().map(|l| l.thickness).sum();
        let p = |deg: f64, len: f64| {
            let a = deg.to_radians();
            vec3(4000.0 - len * a.cos(), 1000.0 + len * a.sin(), 0.0)
        };
        // 15°: die innere Fläche läuft t/2 / tan(7,5°) am Segment entlang
        let noetig = t / 2.0 / 7.5f64.to_radians().tan();
        assert_eq!(
            m.ende_punkte(w, 1, p(15.0, noetig - 20.0)),
            Err(EckeFehler::Spitz)
        );
        assert!(m.ende_punkte(w, 1, p(15.0, noetig + 20.0)).is_ok());
        // 45° mit 15 cm: auch dort kippte der Körper
        assert_eq!(m.ende_punkte(w, 1, p(45.0, 150.0)), Err(EckeFehler::Spitz));
        // Rechter Winkel mit 10 cm bleibt erlaubt (Achse, halbe Dicke < 10 cm)
        assert!(m.ende_punkte(w, 1, vec3(4000.0, 1100.0, 0.0)).is_ok());
        // Was durchgeht, ergibt Grundrisse, die nicht über die Ecke kippen
        // (konvexe Vierecke); das zu kurze Segment kippte
        let konvex = |c: &WallChain| {
            let q = c.segment_footprint(1).unwrap();
            let k: Vec<f64> = (0..4)
                .map(|i| cross2(q[(i + 1) % 4] - q[i], q[(i + 2) % 4] - q[(i + 1) % 4]))
                .collect();
            k.iter().all(|x| *x > 0.0) || k.iter().all(|x| *x < 0.0)
        };
        let mut v = m.run(r).unwrap().points.clone();
        v[2] = p(15.0, noetig - 20.0);
        let mut kurz = m.chain(r).unwrap();
        kurz.points = v.clone();
        assert!(!konvex(&kurz), "ohne Prüfung kippt das kurze Segment");
        // Seit H-160d lehnt auch das Setzen der Punkte das ab
        assert!(m.set_run_points(r, &v).is_none());
        let (run, pts) = m.ende_punkte(w, 1, p(15.0, noetig + 20.0)).unwrap();
        assert!(m.set_run_points(run, &pts).is_some());
        assert!(konvex(&m.chain(r).unwrap()));
    }

    /// Gekippte Dreiecke im Körper des Zuges: Eckfolge gegen die Normale.
    fn gekippt(m: &Model, r: RunId) -> usize {
        m.chain(r)
            .unwrap()
            .solid()
            .triangles
            .iter()
            .filter(|t| {
                let [a, b, c] = t.p;
                let x = (b - a).cross(c - a);
                x.length() > 1e-6 && x.dot(t.n) < 0.0
            })
            .count()
    }

    /// Spitze Ecke beim Wandende ziehen (H1 aus #82): Liegt die Bezugslinie
    /// innen am spitzen Winkel (Außenwand, Bezug links), läuft die Gehrung
    /// der äußeren Fläche über das kurze Stück hinaus und der Körper kippt.
    /// Entweder lehnt ende_punkte ab, oder der Körper bleibt ungekippt.
    #[test]
    fn spitze_ecke_kippt_keinen_koerper() {
        for grad in [10.5_f64, 11.0, 12.0, 13.0] {
            let mut m = Model::new();
            let st = m.eg_at(H);
            let aw = m.defaults().exterior_wall;
            let r = m
                .add_wall_run(
                    &[
                        vec3(1000.0, 1000.0, 0.0),
                        vec3(4000.0, 1000.0, 0.0),
                        vec3(3000.0, 1500.0, 0.0),
                    ],
                    false,
                    RefSide::Left,
                    st,
                    aw,
                    Category::ExteriorWall,
                )
                .unwrap();
            let w = m.wall_at(r, 1).unwrap();
            let a = grad.to_radians();
            let p = vec3(4000.0 - 4000.0 * a.cos(), 1000.0 + 4000.0 * a.sin(), 0.0);
            if let Ok((run, pts)) = m.ende_punkte(w, 1, p) {
                m.set_run_points(run, &pts).unwrap();
                assert_eq!(gekippt(&m, r), 0, "{grad}°: Körper gekippt");
            }
        }
    }

    /// Durchlauf zu B5: Außen- und Innenwand, Bezug links, rechts und
    /// mittig, das Ende nach links und rechts auf 10 bis 20° zurück, kurze
    /// und lange Stücke. Was `ende_punkte` erlaubt, kippt keinen Körper.
    #[test]
    fn gekippte_segmente_bezug_links_und_rechts() {
        // Bezugslinie innen am spitzen Winkel: links bei Linksdrehung,
        // rechts bei Rechtsdrehung (Review #87 B5, Integration: Fall rechts)
        for (seite, s) in [(RefSide::Left, 1.0), (RefSide::Right, -1.0)] {
            for i in 0..=30 {
                let grad: f64 = 10.0 + 0.5 * i as f64;
                let (sin, cos) = grad.to_radians().sin_cos();
                let mut m = Model::new();
                let st = m.eg_at(H);
                let aw = m.defaults().exterior_wall;
                let pts = [
                    vec3(1000.0, 1000.0, 0.0),
                    vec3(4000.0, 1000.0, 0.0),
                    vec3(4000.0 - 4000.0 * cos, 1000.0 + s * 4000.0 * sin, 0.0),
                ];
                let r = m
                    .add_wall_run(&pts, false, seite, st, aw, Category::ExteriorWall)
                    .unwrap();
                let k = m.chain(r).unwrap().gekippte_segmente();
                let fall = format!("{seite:?} {grad}°: {k:?}");
                if k.is_empty() {
                    assert_eq!(gekippt(&m, r), 0, "{fall}");
                }
                if grad == 12.0 {
                    assert_eq!(k, [0], "{fall}");
                }
            }
        }
    }

    #[test]
    fn spitze_ecke_durchlauf() {
        let mut erlaubt = 0;
        for (cat, seite) in [
            (Category::ExteriorWall, RefSide::Left),
            (Category::ExteriorWall, RefSide::Right),
            (Category::InteriorWall, RefSide::Center),
        ] {
            for links in [false, true] {
                for laenge in [600.0_f64, 1500.0, 4000.0] {
                    for i in 0..=40 {
                        let grad = 10.0 + 0.25 * i as f64;
                        let mut m = Model::new();
                        let st = m.eg_at(H);
                        let typ = if cat == Category::ExteriorWall {
                            m.defaults().exterior_wall
                        } else {
                            m.defaults().interior_wall
                        };
                        let y = if links { 1500.0 } else { 500.0 };
                        let r = m
                            .add_wall_run(
                                &[
                                    vec3(1000.0, 1000.0, 0.0),
                                    vec3(4000.0, 1000.0, 0.0),
                                    vec3(3000.0, y, 0.0),
                                ],
                                false,
                                seite,
                                st,
                                typ,
                                cat,
                            )
                            .unwrap();
                        let w = m.wall_at(r, 1).unwrap();
                        let (s, c) = grad.to_radians().sin_cos();
                        let dy = if links { laenge * s } else { -laenge * s };
                        let p = vec3(4000.0 - laenge * c, 1000.0 + dy, 0.0);
                        if let Ok((run, pts)) = m.ende_punkte(w, 1, p) {
                            erlaubt += 1;
                            m.set_run_points(run, &pts).unwrap();
                            assert_eq!(
                                gekippt(&m, r),
                                0,
                                "{cat:?} {seite:?} links {links} {laenge} mm {grad}°"
                            );
                        }
                    }
                }
            }
        }
        assert!(erlaubt > 300, "{erlaubt} erlaubte Fälle");
    }

    /// Gezeichnete spitze Ecke (Vorbefund vor I4): Außenwand, Bezug rechts,
    /// (1000,1000) → (4000,1000) und dann unter 26,6° zurück. Mit 1,1 m
    /// läuft die Gehrung über das kurze Stück hinaus und kippt den Körper;
    /// die Prüfung meldet es mit demselben Prädikat wie das Ziehen. Mit 4 m
    /// kippt nichts, und die Prüfung schweigt.
    #[test]
    fn pruefung_meldet_gekippte_gehrung() {
        let (sin, cos) = 0.5f64.atan().sin_cos();
        for (laenge, kippt) in [(1100.0, true), (4000.0, false)] {
            let mut m = Model::new();
            let st = m.eg_at(H);
            let aw = m.defaults().exterior_wall;
            let r = m
                .add_wall_run(
                    &[
                        vec3(1000.0, 1000.0, 0.0),
                        vec3(4000.0, 1000.0, 0.0),
                        vec3(4000.0 - laenge * cos, 1000.0 + laenge * sin, 0.0),
                    ],
                    false,
                    RefSide::Right,
                    st,
                    aw,
                    Category::ExteriorWall,
                )
                .unwrap();
            let gemeldet: Vec<String> = m
                .check()
                .into_iter()
                .filter(|f| f.contains("Gehrung"))
                .collect();
            assert_eq!(gekippt(&m, r) > 0, kippt, "{laenge} mm");
            assert_eq!(!gemeldet.is_empty(), kippt, "{laenge} mm: {gemeldet:?}");
        }
    }

    /// Kleiner Vorsprung, schmaler als die Wand dick ist (A31, #98): Ohne
    /// Zuschnitt liefe die Fläche an seinem Kopf rückwärts und der Körper
    /// kippte. Mit Zuschnitt treffen sich die Nachbarflächen direkt, der
    /// Körper ist geschlossen und ungekippt, und die Prüfung schweigt; ein
    /// breiter Vorsprung bleibt, wie er war.
    #[test]
    fn kleiner_vorsprung_wird_zugeschnitten() {
        for (b, v, roh) in [
            (50.0, 50.0, true),
            (200.0, 200.0, true),
            (2000.0, 200.0, false),
        ] {
            for seite in [RefSide::Left, RefSide::Right, RefSide::Center] {
                let mut m = Model::new();
                let st = m.eg_at(H);
                let aw = m.defaults().exterior_wall;
                let pts = [
                    vec3(0.0, 0.0, 0.0),
                    vec3(0.0, 8000.0, 0.0),
                    vec3(10000.0, 8000.0, 0.0),
                    vec3(10000.0, 0.0, 0.0),
                    vec3(4000.0 + b, 0.0, 0.0),
                    vec3(4000.0 + b, -v, 0.0),
                    vec3(4000.0, -v, 0.0),
                    vec3(4000.0, 0.0, 0.0),
                ];
                let r = m
                    .add_wall_run(&pts, true, seite, st, aw, Category::ExteriorWall)
                    .unwrap();
                let c = m.chain(r).unwrap();
                let fall = format!("{b} × {v} {seite:?}");
                if roh && seite != RefSide::Right {
                    assert!(
                        !c.gekippte_segmente().is_empty(),
                        "{fall}: ohne Zuschnitt gekippt"
                    );
                }
                assert_eq!(gekippt(&m, r), 0, "{fall}");
                assert!(c.koerper_kippt().is_empty(), "{fall}");
                let s = c.solid();
                let sum = s.triangles.iter().fold(vec3(0.0, 0.0, 0.0), |a, t| {
                    let [p, q, w] = t.p;
                    a + (q - p).cross(w - p)
                });
                assert!(sum.length() < 1e-3, "{fall}: offen {sum:?}");
                assert!(m.check().is_empty(), "{fall}: {:?}", m.check());
            }
        }
    }
}

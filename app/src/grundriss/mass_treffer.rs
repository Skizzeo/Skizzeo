//! Treffer an den Außenmaßketten (Konzept `bemassung/konzept.md` §4, §5):
//! Maßzahl oder Maßlinie unter der Maus, Ketten im Kreuzungsrechteck und das
//! Maß, das sich beim Ziehen eines Bauteils ändert. Gerechnet wird auf dem
//! Bild der Ketten ([`crate::bemassung::Stand`]) in Pixeln der Ansicht.
//! Teil von [`crate::wall_edit`].

use crate::bemassung::{Blick, Stand};
use crate::camera::Camera;
use sk_math::{dist_to_segment, Vec3};
use sk_model::bemassung::Kettenart;
use sk_model::{RunId, StoreyId};
use sk_plan::aussen::{Aussenmasse, Bezug, Kette, Quelle, Seite};
use sk_plan::bild::{Bild, Zahlart};

/// Greifabstand zur Maßlinie (px bei 96 dpi).
pub const LINIE_PX: f64 = 6.0;
/// Rand um eine Maßzahl, der noch trifft (px bei 96 dpi).
const ZAHL_PX: f64 = 3.0;
/// Gleiche Richtung bis 1°: Kosinus.
const PARALLEL: f64 = 0.999_847_7;
/// Welche Kette beim Ziehen ein Bauteil misst, in dieser Reihenfolge: das
/// Maß zum Nachbarn steht in der Wandkette.
const VORRANG: [Kettenart; 4] = [
    Kettenart::Wand,
    Kettenart::Sprung,
    Kettenart::Gesamt,
    Kettenart::Oeffnung,
];

/// Eine Maßkette, gleich über neue Rechnungen hinweg: Geschoss, Seite (Grad
/// der Außennormalen) und Art; so heißt auch ihr Versatz im Modell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KettenId {
    pub st: StoreyId,
    pub grad: i32,
    pub art: Kettenart,
}

/// Ein Maß einer Kette: zwischen Bezug `i` und `i + 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MassId {
    pub kette: KettenId,
    pub i: usize,
}

/// Was unter der Maus liegt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Treffer {
    /// Eine Maßzahl, die sich überschreiben lässt.
    Zahl(MassId),
    /// Eine Maßlinie, oder eine Zahl, die sich nicht überschreiben lässt
    /// (Wanddicke, Höhe und BRH der Öffnungen): Sie greift die Kette.
    Linie(KettenId),
}

impl Treffer {
    pub fn kette(self) -> KettenId {
        match self {
            Treffer::Zahl(m) => m.kette,
            Treffer::Linie(k) => k,
        }
    }
}

/// Eine Kette im Stand mit ihrer Seite und ihrem Geschoss.
pub struct Fund<'a> {
    pub a: &'a Aussenmasse,
    pub seite: &'a Seite,
    pub kette: &'a Kette,
}

/// Name der Kette `kette` der Seite `seite`.
pub fn id_von(a: &Aussenmasse, seite: usize, kette: usize) -> Option<KettenId> {
    let s = a.seiten.get(seite)?;
    let k = s.ketten.get(kette)?;
    Some(KettenId {
        st: a.storey,
        grad: s.grad,
        art: k.art,
    })
}

/// Die Kette `k` im Stand.
pub fn finde(stand: &Stand, k: KettenId) -> Option<Fund<'_>> {
    for (a, _) in stand.paare() {
        if a.storey != k.st {
            continue;
        }
        for s in &a.seiten {
            if s.grad != k.grad {
                continue;
            }
            if let Some(kette) = s.ketten.iter().find(|x| x.art == k.art) {
                return Some(Fund { a, seite: s, kette });
            }
        }
    }
    None
}

/// Bezüge des Maßes `m`: (Anfang, Ende), nach `t` geordnet.
pub fn enden(stand: &Stand, m: MassId) -> Option<(Bezug, Bezug)> {
    let f = finde(stand, m.kette)?;
    Some((*f.kette.bezuege.get(m.i)?, *f.kette.bezuege.get(m.i + 1)?))
}

/// Gehören zwei Bezüge zum selben Bauteil, das sich als Ganzes bewegt
/// (Wanddicke)? Zwei Kanten einer Öffnung zählen nicht: Dort ändert das
/// Maß die Breite.
fn ein_bauteil(a: &Bezug, b: &Bezug) -> bool {
    match (a.quelle, b.quelle) {
        (Some(Quelle::Wand { run: r, seg: s }), Some(Quelle::Wand { run, seg })) => {
            r == run && s == seg
        }
        (Some(Quelle::Innenwand { run: r }), Some(Quelle::Innenwand { run })) => r == run,
        _ => false,
    }
}

/// Lässt sich das Ende `ende` (0: Anfang, 1: Ende) des Maßes bewegen?
pub fn beweglich(a: &Bezug, b: &Bezug, ende: usize) -> bool {
    let q = if ende == 0 { a } else { b };
    q.quelle.is_some() && !ein_bauteil(a, b)
}

/// Ende, das sich beim Überschreiben zuerst bewegt: eine Öffnung gegen
/// eine Ecke (Bedienbarkeit 40.4: wie auf dem Papierplan wandert das
/// Fenster, nicht die Hauswand), sonst das rechte bzw. obere (größeres
/// `t`), sonst das andere; `None`, wenn keins.
pub fn erstes_ende(a: &Bezug, b: &Bezug) -> Option<usize> {
    let oeffnung = |q: &Bezug| matches!(q.quelle, Some(Quelle::Oeffnung { .. }));
    let zuerst = match (oeffnung(a), oeffnung(b)) {
        (true, false) => [0, 1],
        _ => [1, 0],
    };
    zuerst.into_iter().find(|&e| beweglich(a, b, e))
}

fn bp(cam: &Camera, p: Vec3, w: f64, h: f64) -> Option<(f64, f64)> {
    cam.project(p, w, h)
}

/// Liegt `p` im Viereck `q` (konvex, beliebiger Umlauf) oder näher als `r`
/// an seinem Rand?
fn im_viereck(q: &[(f64, f64); 4], p: (f64, f64), r: f64) -> bool {
    let kreuz =
        |a: (f64, f64), b: (f64, f64)| (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
    let s: [f64; 4] = std::array::from_fn(|i| kreuz(q[i], q[(i + 1) % 4]));
    let innen = s.iter().all(|&v| v >= 0.0) || s.iter().all(|&v| v <= 0.0);
    innen || (0..4).any(|i| dist_to_segment(p, q[i], q[(i + 1) % 4]) <= r)
}

/// Rahmen einer Zahl im Bild, falls alle Ecken vor der Kamera liegen.
fn rahmen(cam: &Camera, r: &[Vec3; 4], w: f64, h: f64) -> Option<[(f64, f64); 4]> {
    Some([
        bp(cam, r[0], w, h)?,
        bp(cam, r[1], w, h)?,
        bp(cam, r[2], w, h)?,
        bp(cam, r[3], w, h)?,
    ])
}

/// Kette, zu der eine Zahl gehört (Höhe und BRH: die Öffnungskette).
fn kette_der_zahl(a: &Aussenmasse, art: Zahlart) -> Option<KettenId> {
    match art {
        Zahlart::Mass { seite, kette, .. } => id_von(a, seite, kette),
        Zahlart::Hoehe { seite, .. } | Zahlart::Brh { seite, .. } => {
            let s = a.seiten.get(seite)?;
            let ki = s.ketten.iter().position(|k| k.art == Kettenart::Oeffnung)?;
            id_von(a, seite, ki)
        }
    }
}

/// Ketten und Bild der Geschosse, von denen etwas im Blick und im
/// Rechteck `r` liegt, und ob ihre Zahlen lesbar sind.
fn im_bereich<'a>(
    stand: &'a Stand,
    blick: &Blick,
    r: [f64; 4],
) -> Vec<(&'a Aussenmasse, &'a Bild, bool)> {
    stand
        .geschosse
        .iter()
        .filter_map(|g| Some((&*g.a, &g.b, g.im_bereich(blick, r)?)))
        .collect()
}

/// Maßzahl oder Maßlinie bei `(x, y)` (Pixel der Ansicht). Geprüft werden
/// nur Geschosse, deren Rahmen der Blickstrahl der Maus trifft, Zahlen nur,
/// wenn sie gezeichnet werden (lesbar, Review W36 und W39).
#[allow(clippy::too_many_arguments)]
pub fn treffer(
    stand: &Stand,
    cam: &Camera,
    w: f64,
    h: f64,
    scale: f64,
    x: f64,
    y: f64,
) -> Option<Treffer> {
    let p = (x, y);
    let blick = Blick { cam, w, h };
    let r = ZAHL_PX.max(LINIE_PX) * scale;
    let strahl = cam.ray(x, y, w, h);
    // Nur Geschosse aus dem Raster nahe dem Strahl (Review L29)
    let nah: Vec<(&Aussenmasse, &Bild, bool)> = stand
        .am_strahl(&blick, strahl, r)
        .into_iter()
        .filter_map(|g| Some((&*g.a, &g.b, g.unter_strahl(&blick, strahl, r)?)))
        .collect();
    // Zahlen zuerst, die nächste zur Mitte
    let mut best: Option<(f64, Treffer)> = None;
    for &(a, b, _) in nah.iter().filter(|g| g.2) {
        for z in &b.zahlen {
            let Some(q) = rahmen(cam, &z.rahmen, w, h) else {
                continue;
            };
            if !im_viereck(&q, p, ZAHL_PX * scale) {
                continue;
            }
            let Some(k) = kette_der_zahl(a, z.art) else {
                continue;
            };
            let t = match z.art {
                Zahlart::Mass { seite, kette, i } => {
                    let kt = &a.seiten[seite].ketten[kette];
                    match (kt.bezuege.get(i), kt.bezuege.get(i + 1)) {
                        (Some(r0), Some(r1)) if erstes_ende(r0, r1).is_some() => {
                            Treffer::Zahl(MassId { kette: k, i })
                        }
                        _ => Treffer::Linie(k),
                    }
                }
                _ => Treffer::Linie(k),
            };
            let mx = q.iter().map(|c| c.0).sum::<f64>() / 4.0;
            let my = q.iter().map(|c| c.1).sum::<f64>() / 4.0;
            let d = (mx - x).hypot(my - y);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, t));
            }
        }
    }
    if let Some((_, t)) = best {
        return Some(t);
    }
    let mut best: Option<(f64, KettenId)> = None;
    for &(a, b, _) in &nah {
        for l in &b.linien {
            let (Some(pa), Some(pb)) = (bp(cam, l.a, w, h), bp(cam, l.b, w, h)) else {
                continue;
            };
            let d = dist_to_segment(p, pa, pb);
            if d > LINIE_PX * scale || best.is_some_and(|(bd, _)| d >= bd) {
                continue;
            }
            if let Some(k) = id_von(a, l.seite, l.kette) {
                best = Some((d, k));
            }
        }
    }
    best.map(|(_, k)| Treffer::Linie(k))
}

/// Schneidet die Strecke `a`–`b` das Rechteck `r` (x0, y0, x1, y1)?
fn strecke_im_rechteck(a: (f64, f64), b: (f64, f64), r: [f64; 4]) -> bool {
    let innen = |p: (f64, f64)| p.0 >= r[0] && p.0 <= r[2] && p.1 >= r[1] && p.1 <= r[3];
    if innen(a) || innen(b) {
        return true;
    }
    let ecken = [(r[0], r[1]), (r[2], r[1]), (r[2], r[3]), (r[0], r[3])];
    let kreuz = |p: (f64, f64), q: (f64, f64), s: (f64, f64)| {
        (q.0 - p.0) * (s.1 - p.1) - (q.1 - p.1) * (s.0 - p.0)
    };
    (0..4).any(|i| {
        let (c, d) = (ecken[i], ecken[(i + 1) % 4]);
        let (k1, k2) = (kreuz(a, b, c), kreuz(a, b, d));
        let (k3, k4) = (kreuz(c, d, a), kreuz(c, d, b));
        k1 * k2 <= 0.0 && k3 * k4 <= 0.0
    })
}

/// Ketten, die das Rechteck mit den Ecken `p` und `q` (Pixel) kreuzen:
/// mit der Maßlinie oder einer ihrer Zahlen, wenn diese gezeichnet werden.
pub fn im_rechteck(
    stand: &Stand,
    cam: &Camera,
    w: f64,
    h: f64,
    p: (f64, f64),
    q: (f64, f64),
) -> Vec<KettenId> {
    let r = [p.0.min(q.0), p.1.min(q.1), p.0.max(q.0), p.1.max(q.1)];
    let mut out: Vec<KettenId> = Vec::new();
    let mut dazu = |k: KettenId| {
        if !out.contains(&k) {
            out.push(k);
        }
    };
    for (a, b, lesbar) in im_bereich(stand, &Blick { cam, w, h }, r) {
        for l in &b.linien {
            if let (Some(pa), Some(pb)) = (bp(cam, l.a, w, h), bp(cam, l.b, w, h)) {
                if strecke_im_rechteck(pa, pb, r) {
                    if let Some(k) = id_von(a, l.seite, l.kette) {
                        dazu(k);
                    }
                }
            }
        }
        for z in b.zahlen.iter().filter(|_| lesbar) {
            let Some(c) = rahmen(cam, &z.rahmen, w, h) else {
                continue;
            };
            if (0..4).any(|i| strecke_im_rechteck(c[i], c[(i + 1) % 4], r)) {
                if let Some(k) = kette_der_zahl(a, z.art) {
                    dazu(k);
                }
            }
        }
    }
    out
}

/// Alle Ketten des Blocks (derselben Seite und desselben Geschosses) von `k`.
pub fn block(stand: &Stand, k: KettenId) -> Vec<KettenId> {
    finde(stand, k).map_or_else(Vec::new, |f| {
        f.seite
            .ketten
            .iter()
            .map(|x| KettenId { art: x.art, ..k })
            .collect()
    })
}

/// Wie ein Maß beim Ziehen aus dem getippten Wert wird: Weg des Bauteils
/// `s = b · (v − d0)` in Zugrichtung.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Abbildung {
    pub mass: MassId,
    /// Maß beim Greifen (mm).
    pub d0: f64,
    /// +1: das Maß hinter dem Bauteil (wächst mit dem Weg), −1: das Maß
    /// davor (wenn dahinter keins ist).
    pub b: f64,
}

impl Abbildung {
    /// Weg in Zugrichtung (mm) für das Maß `v`.
    pub fn weg(&self, v: f64) -> f64 {
        self.b * (v - self.d0)
    }

    /// Maß nach dem Weg `s` in Zugrichtung.
    pub fn mass(&self, s: f64) -> f64 {
        self.d0 + self.b * s
    }
}

/// Das Maß, das sich beim Ziehen eines Bauteils ändert, je Zugrichtung:
/// `plus` längs der Normalen des gezogenen Segments, `minus` dagegen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bezugsmass {
    pub plus: Option<Abbildung>,
    pub minus: Option<Abbildung>,
}

impl Bezugsmass {
    /// Abbildung für die Zugrichtung `seite` (±1).
    pub fn fuer(&self, seite: f64) -> Option<&Abbildung> {
        if seite >= 0.0 {
            self.plus.as_ref()
        } else {
            self.minus.as_ref()
        }
    }
}

/// Abbildung für Bezüge `idx` (die des Bauteils) der Kette, beim Ziehen in
/// Richtung `rt` längs der Kette (±1).
fn abbildung(k: &Kette, id: KettenId, idx: &[usize], rt: f64) -> Option<Abbildung> {
    let (lo, hi) = (*idx.iter().min()?, *idx.iter().max()?);
    let t = |i: usize| k.bezuege[i].t;
    let n = k.bezuege.len();
    let hinten_lo = (lo > 0).then(|| (lo - 1, t(lo) - t(lo - 1)));
    let vorn_hi = (hi + 1 < n).then(|| (hi, t(hi + 1) - t(hi)));
    let (hinter, vor) = if rt > 0.0 {
        (hinten_lo, vorn_hi)
    } else {
        (vorn_hi, hinten_lo)
    };
    let (i, d0, b) = match (hinter, vor) {
        (Some((i, d)), _) => (i, d, 1.0),
        (None, Some((i, d))) => (i, d, -1.0),
        (None, None) => return None,
    };
    Some(Abbildung {
        mass: MassId { kette: id, i },
        d0,
        b,
    })
}

/// Misst der Bezug `r` das Segment `seg` des Zugs `run`?
fn misst(r: &Bezug, run: RunId, seg: usize) -> bool {
    match r.quelle {
        Some(Quelle::Wand { run: x, seg: s }) => x == run && s == seg,
        Some(Quelle::Innenwand { run: x }) => x == run,
        _ => false,
    }
}

/// Bezugsmaß beim Ziehen von Segment `seg` des Zugs `run` mit der Normalen
/// `n`, gegriffen bei `p` (Konzept §4): das Maß zwischen dem Bauteil und
/// seinem Nachbarn hinter ihm, in der Wandkette der Seite, die `p` am
/// nächsten liegt (ohne Wandkette in der nächsten Kette, die es misst).
pub fn bezugsmass(stand: &Stand, run: RunId, seg: usize, n: Vec3, p: Vec3) -> Option<Bezugsmass> {
    let mut best: Option<(f64, Bezugsmass)> = None;
    for (a, _) in stand.paare() {
        for s in &a.seiten {
            let k_nt = n.dot(s.t);
            if k_nt.abs() < PARALLEL {
                continue;
            }
            let gefunden = VORRANG.iter().find_map(|&art| {
                let k = s.ketten.iter().find(|k| k.art == art)?;
                let idx: Vec<usize> = (0..k.bezuege.len())
                    .filter(|&i| misst(&k.bezuege[i], run, seg))
                    .collect();
                if idx.is_empty() {
                    return None;
                }
                let id = KettenId {
                    st: a.storey,
                    grad: s.grad,
                    art,
                };
                let r = k_nt.signum();
                let m = Bezugsmass {
                    plus: abbildung(k, id, &idx, r),
                    minus: abbildung(k, id, &idx, -r),
                };
                (m.plus.is_some() || m.minus.is_some()).then_some(m)
            });
            let Some(m) = gefunden else { continue };
            let d = (s.aussen - p.dot(s.n)).abs();
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, m));
            }
        }
    }
    best.map(|(_, m)| m)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::bemassung::Massketten;
    use crate::scene::Scene;
    use crate::ui::ViewKind;
    use sk_math::vec3;
    use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
    use sk_model::{Category, ElementId, Model, RefSide};
    use std::rc::Rc;

    pub(crate) fn schrift() -> Option<sk_paint::font::Font> {
        std::fs::read("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf")
            .ok()
            .and_then(sk_paint::font::Font::parse)
    }

    /// Haus 10 × 8 m mit einer Innenwand bei x = 4 m (an die Südwand
    /// angeschlossen) und einem Fenster 1,50–2,51 m in der Südwand:
    /// (Szene, Außenwandzug, Innenwand, Fenster).
    pub(crate) fn haus() -> (Scene, RunId, RunId, ElementId) {
        let mut m = Model::new();
        m.begin("Haus");
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10_000.0, 8000.0, 0.0),
            vec3(10_000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let st = m.run(eg).unwrap().storey;
        let set = m.defaults().interior_wall;
        let iw = m
            .add_wall_run(
                &[vec3(4000.0, 0.0, 0.0), vec3(4000.0, 5000.0, 0.0)],
                false,
                RefSide::Center,
                st,
                set,
                Category::InteriorWall,
            )
            .unwrap();
        let fenster = m
            .add_opening(Opening {
                kind: OpeningKind::Window,
                path: vec![vec3(2510.0, 0.0, 0.0), vec3(1500.0, 0.0, 0.0)],
                bottom: HeightRef::Line(st, HeightLine::Sill),
                top: HeightRef::Line(st, HeightLine::Lintel),
                reveal: None,
                install: Default::default(),
                swing: None,
                lock: Vec::new(),
            })
            .unwrap();
        m.commit();
        (Scene::with_model(m), eg, iw, fenster)
    }

    pub(crate) fn stand(s: &Scene) -> Option<Rc<Stand>> {
        let f = schrift()?;
        let mut mk = Massketten::default();
        let st = crate::bemassung::geschosse(s, ViewKind::Plan, &[]);
        mk.stand(s, st, Some(&f))
    }

    /// Grundriss von oben über dem Haus.
    pub(crate) fn kamera() -> Camera {
        Camera::parallel(
            vec3(5000.0, 4000.0, 0.0),
            90f64.to_radians(),
            -89.9f64.to_radians(),
            9000.0,
        )
    }

    #[test]
    fn zahl_und_linie_unter_der_maus() {
        let (s, ..) = haus();
        let Some(st) = stand(&s) else {
            eprintln!("Liberation Sans fehlt, Test übersprungen");
            return;
        };
        let cam = kamera();
        let (w, h) = (1600.0, 1000.0);
        // Jede überschreibbare Zahl trifft sich an ihrer Mitte
        let mut zahlen = 0;
        for (a, b) in st.paare() {
            for z in &b.zahlen {
                let Zahlart::Mass { seite, kette, i } = z.art else {
                    continue;
                };
                let m = z.rahmen.iter().fold(Vec3::ZERO, |s, p| s + *p) * 0.25;
                let (x, y) = cam.project(m, w, h).unwrap();
                let t = treffer(&st, &cam, w, h, 1.0, x, y).unwrap();
                assert_eq!(t.kette(), id_von(a, seite, kette).unwrap());
                if let Treffer::Zahl(mi) = t {
                    assert_eq!(mi.i, i);
                    zahlen += 1;
                }
            }
        }
        assert!(zahlen >= 3, "{zahlen}");
        // Die Maßlinie trifft neben den Zahlen, weit weg nichts
        let (a, b) = (&st.geschosse[0].a, &st.geschosse[0].b);
        let l = b.linien[0];
        let p = l.a + (l.b - l.a) * 0.02;
        let (x, y) = cam.project(p, w, h).unwrap();
        assert_eq!(
            treffer(&st, &cam, w, h, 1.0, x, y),
            Some(Treffer::Linie(id_von(a, l.seite, l.kette).unwrap()))
        );
        let (x, y) = cam.project(vec3(5000.0, 4000.0, 0.0), w, h).unwrap();
        assert_eq!(treffer(&st, &cam, w, h, 1.0, x, y), None);
    }

    /// Weit weg, wo die Zahlen nicht gezeichnet werden, trifft auch keine
    /// (Review W36): an ihrer Stelle trifft die Maßlinie.
    #[test]
    fn unlesbare_zahl_trifft_nicht() {
        let (s, ..) = haus();
        let Some(st) = stand(&s) else {
            return;
        };
        let (w, h) = (1600.0, 1000.0);
        let cam = Camera::parallel(
            vec3(5000.0, 4000.0, 0.0),
            90f64.to_radians(),
            -89.9f64.to_radians(),
            400_000.0,
        );
        // Eine Zahl, die nah die Maus trifft
        let nah = kamera();
        let m = st.geschosse[0]
            .b
            .zahlen
            .iter()
            .map(|z| z.rahmen.iter().fold(Vec3::ZERO, |s, p| s + *p) * 0.25)
            .find(|&m| {
                let (x, y) = nah.project(m, w, h).unwrap();
                matches!(treffer(&st, &nah, w, h, 1.0, x, y), Some(Treffer::Zahl(_)))
            })
            .unwrap();
        let (x, y) = cam.project(m, w, h).unwrap();
        let t = treffer(&st, &cam, w, h, 1.0, x, y);
        assert!(matches!(t, Some(Treffer::Linie(_))), "{t:?}");
    }

    #[test]
    fn rechteck_kreuzt_ketten() {
        let (s, ..) = haus();
        let Some(st) = stand(&s) else {
            return;
        };
        let cam = kamera();
        let (w, h) = (1600.0, 1000.0);
        // Ein schmales Rechteck quer über den Block unter dem Haus (Süden)
        let p = cam.project(vec3(2000.0, -6000.0, 0.0), w, h).unwrap();
        let q = cam.project(vec3(2200.0, -1000.0, 0.0), w, h).unwrap();
        let k = im_rechteck(&st, &cam, w, h, p, q);
        let sued: Vec<KettenId> = st.geschosse[0]
            .a
            .seiten
            .iter()
            .find(|s| s.n.y < -0.5)
            .unwrap()
            .ketten
            .iter()
            .map(|x| KettenId {
                st: st.geschosse[0].a.storey,
                grad: 270,
                art: x.art,
            })
            .collect();
        assert_eq!(k.len(), sued.len(), "{k:?}");
        assert!(sued.iter().all(|x| k.contains(x)));
        assert_eq!(block(&st, sued[0]), sued);
        // Mitten im Haus kreuzt nichts
        let p = cam.project(vec3(3000.0, 3000.0, 0.0), w, h).unwrap();
        let q = cam.project(vec3(6000.0, 5000.0, 0.0), w, h).unwrap();
        assert!(im_rechteck(&st, &cam, w, h, p, q).is_empty());
    }

    /// Ostwand nach außen ziehen: das Maß dahinter ist das Feld zwischen
    /// Innenwand und Innenseite der Ostwand; nach innen (nichts dahinter)
    /// das Maß davor, das kleiner wird.
    #[test]
    fn bezugsmass_der_ostwand() {
        let (s, eg, ..) = haus();
        let Some(st) = stand(&s) else {
            return;
        };
        let c = s.chain(eg).unwrap();
        // Segment der Ostwand: Normale längs ±x
        let seg = (0..c.segment_count())
            .find(|&i| {
                let p = c.clean_points();
                let (a, b) = (p[i], p[(i + 1) % p.len()]);
                (a.x - 10000.0).abs() < 1.0 && (b.x - 10000.0).abs() < 1.0
            })
            .unwrap();
        let n = c.segment_normal(seg).unwrap();
        let aus = n.x.signum();
        // Gegriffen nahe der Südecke: die Südseite
        let m = bezugsmass(&st, eg, seg, n, vec3(10000.0, 500.0, 0.0)).unwrap();
        let raus = m.fuer(aus).unwrap();
        assert_eq!(raus.mass.kette.art, Kettenart::Wand);
        assert_eq!(raus.mass.kette.grad, 270);
        assert_eq!(raus.b, 1.0);
        let (r0, r1) = enden(&st, raus.mass).unwrap();
        assert!((r1.t - r0.t - raus.d0).abs() < 1e-6);
        assert!(r0.t > 4000.0 && r1.t < 10000.0, "{} {}", r0.t, r1.t);
        // Getippt 6,00 m: die Wand wandert um 6000 − d0
        assert!((raus.weg(6000.0) - (6000.0 - raus.d0)).abs() < 1e-9);
        assert!((raus.mass(raus.weg(6000.0)) - 6000.0).abs() < 1e-9);
        // Nach innen: dahinter liegt kein Bezug mehr, also das Maß davor
        let rein = m.fuer(-aus).unwrap();
        assert_eq!(rein.mass, raus.mass);
        assert_eq!(rein.b, -1.0);
        // Nahe der Nordecke die Nordseite (ohne Innenwand: das Feld von der
        // Westwand)
        let m = bezugsmass(&st, eg, seg, n, vec3(10000.0, 7500.0, 0.0)).unwrap();
        assert_eq!(m.fuer(aus).unwrap().mass.kette.grad, 90);
        assert!(m.fuer(aus).unwrap().d0 > 8000.0);
    }

    #[test]
    fn enden_und_bewegliche_seiten() {
        let (s, _, iw, _) = haus();
        let Some(st) = stand(&s) else {
            return;
        };
        let a = &st.geschosse[0].a;
        let sued = a.seiten.iter().find(|s| s.grad == 270).unwrap();
        let wand = sued
            .ketten
            .iter()
            .find(|k| k.art == Kettenart::Wand)
            .unwrap();
        // Erstes Feld: Wanddicke der Westwand, nicht überschreibbar
        let (r0, r1) = (&wand.bezuege[0], &wand.bezuege[1]);
        assert_eq!(erstes_ende(r0, r1), None);
        // Feld zur Innenwand: das rechte Ende ist die Innenwand
        let i = wand
            .bezuege
            .iter()
            .position(|r| r.quelle == Some(Quelle::Innenwand { run: iw }))
            .unwrap();
        let (r0, r1) = (&wand.bezuege[i - 1], &wand.bezuege[i]);
        assert_eq!(erstes_ende(r0, r1), Some(1));
        assert!(beweglich(r0, r1, 0));
        // Öffnungskette: Ecke, Fenster, Ecke. Zwischen Fenster und Ecke
        // wandert das Fenster, auch wenn es links steht (Bedienbarkeit
        // 40.4); die Breite ändert sich nach rechts
        let o = sued
            .ketten
            .iter()
            .find(|k| k.art == Kettenart::Oeffnung)
            .unwrap();
        let b = &o.bezuege;
        assert_eq!(b.len(), 4);
        assert_eq!(erstes_ende(&b[0], &b[1]), Some(1));
        assert_eq!(erstes_ende(&b[1], &b[2]), Some(1));
        assert_eq!(erstes_ende(&b[2], &b[3]), Some(0));
        assert!(beweglich(&b[2], &b[3], 1), "Tab nimmt die Hauswand");
    }
}

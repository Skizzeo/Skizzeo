//! Aussparung der Decke für eine Öffnung, die durch sie hindurch läuft
//! (Fenster über zwei Geschosse, Jörn 13.10.: „Aussparen“).
//!
//! Die Decke liegt mit ihrem Rand in der Tasche der Außenwand. Reicht eine
//! Öffnung in das Deckenband, bekommt der Umriss dort eine Kerbe über die
//! ganze Deckendicke: so breit wie das Loch in der Schicht, in der der
//! Deckenrand sitzt, so tief bis zur Innenfläche der Wand. Hinter der
//! Öffnung bleibt die Deckenstirn bündig mit der Innenfläche stehen, die
//! beiden Seiten der Kerbe setzen die Laibungen fort. Über eine Ecke
//! treffen sich die Kerben in der inneren Wandecke, über einen Knick ohne
//! Richtungswechsel laufen sie durch. An einer zu spitzen Ecke (unter etwa
//! 29°, `MAX_ECKE`) treffen sie sich nicht; kreuzen sie sich dabei, werden
//! sie verworfen und der Umriss bleibt ganz, die Öffnung endet dann an der
//! Decke (`Model::spart_aus`).
//!
//! Hier nur die Geometrie des Umrisses; welche Öffnungen eine Decke
//! aussparen, weiß das Modell (`decke.rs`).

use crate::wall::cross2;
use crate::ElementId;
use sk_math::{polygon, vec3, Vec3};

/// Unter diesem Maß (mm) gilt eine Kerbe als nicht da.
const EPS: f64 = 0.5;
/// So viele Kerbtiefen darf die innere Ecke höchstens vom Eckpunkt des
/// Umrisses weg liegen (spitze Ecken: etwa 29°).
pub(crate) const MAX_ECKE: f64 = 4.0;

/// Eine Kerbe im Umriss der Decke.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aussparung {
    /// Die Öffnung, die durch die Decke läuft.
    pub opening: Option<ElementId>,
    /// Wandsegment der Öffnung und Anfang und Ende ihres Lochs auf der
    /// Bezugslinie (Randdämmstreifen, `floor.rs`).
    pub seg: usize,
    pub stelle: (Vec3, Vec3),
    /// Kante des Umrisses vor dem Aussparen (von Punkt `kante` zum
    /// nächsten) und Bereich längs ihr ab ihrem Anfang (mm).
    pub kante: usize,
    pub u: (f64, f64),
    /// Tiefe nach innen bis zur Innenfläche der Wand und Höhe (die
    /// Deckendicke), mm.
    pub tiefe: f64,
    pub dicke: f64,
    /// Fläche der ganzen Kerbe dieser Öffnung in der Decke, alle Schenkel
    /// zusammen (mm², an jedem Schenkel gleich; VOB/C: eine Aussparung).
    pub flaeche: f64,
    /// Länge der Deckenstirn in der Kerbe (mm) und Zahl ihrer Seiten
    /// (Laibungen), aus [`aussparen`].
    pub stirn: f64,
    pub seiten: u32,
}

/// Schnittpunkt der Geraden `p` + s·`d` und `q` + t·`e` (waagerecht).
fn schnitt(p: Vec3, d: Vec3, q: Vec3, e: Vec3) -> Option<Vec3> {
    let n = cross2(d, e);
    (n.abs() > 1e-9).then(|| p + d * (cross2(q - p, e) / n))
}

/// Kante `k` des geschlossenen Umrisses: Anfang, Richtung, Länge und
/// Normale nach innen (gegen den Uhrzeigersinn: links).
fn kante(outline: &[Vec3], k: usize) -> (Vec3, Vec3, f64, Vec3) {
    let (a, b) = (outline[k], outline[(k + 1) % outline.len()]);
    let l = (b - a).length();
    let d = if l > 0.0 {
        (b - a) * (1.0 / l)
    } else {
        Vec3::default()
    };
    (a, d, l, vec3(-d.y, d.x, 0.0))
}

/// Der Umriss `outline` (gegen den Uhrzeigersinn) mit den Kerben `aus`;
/// trägt in jede Kerbe Stirnlänge und Seiten ein. Überlappen sich Kerben
/// auf einer Kante, werden sie eine (die erste zählt). Ohne gültige Kerbe
/// oder wenn der Umriss sich kreuzen würde, bleibt er, wie er ist, und die
/// Kerben sind leer.
pub fn aussparen(outline: &[Vec3], aus: &mut [Aussparung]) -> Vec<Vec3> {
    let n = outline.len();
    for a in aus.iter_mut() {
        (a.stirn, a.seiten) = (0.0, 0);
    }
    if n < 3 || aus.is_empty() {
        return outline.to_vec();
    }
    // Je Kante: Bereich, Tiefe, Kerbe
    let mut je: Vec<Vec<(f64, f64, f64, usize)>> = vec![Vec::new(); n];
    for (i, a) in aus.iter().enumerate() {
        if a.kante >= n {
            continue;
        }
        let l = kante(outline, a.kante).2;
        let (u0, u1) = (a.u.0.max(0.0), a.u.1.min(l));
        if u1 - u0 > EPS && a.tiefe > EPS {
            je[a.kante].push((u0, u1, a.tiefe, i));
        }
    }
    for v in &mut je {
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut m: Vec<(f64, f64, f64, usize)> = Vec::new();
        for x in v.drain(..) {
            match m.last_mut() {
                Some(y) if x.0 <= y.1 + EPS => (y.1, y.2) = (y.1.max(x.1), y.2.max(x.2)),
                _ => m.push(x),
            }
        }
        *v = m;
    }
    let anfang = |k: usize| je[k].first().is_some_and(|x| x.0 <= EPS);
    let ende = |k: usize| {
        let l = kante(outline, k).2;
        je[k].last().is_some_and(|x| x.1 >= l - EPS)
    };
    // Innere Ecke am Punkt `k`, wenn die Kerben beider Kanten dort enden:
    // Ende der Stirn auf der vorigen Kante und Anfang auf dieser (an einem
    // geraden Knick mit verschiedener Tiefe eine Stufe). `None` an einer
    // zu spitzen Ecke oder einer Kehre.
    let ecke = |k: usize| -> Option<(Vec3, Vec3)> {
        if !(anfang(k) && ende((k + n - 1) % n)) {
            return None;
        }
        let prev = (k + n - 1) % n;
        let (p, d, _, nk) = kante(outline, k);
        let (q, e, _, nq) = kante(outline, prev);
        let (tk, tq) = (je[k][0].2, je[prev].last().map_or(0.0, |x| x.2));
        if cross2(e, d).abs() < 1e-6 {
            return (e.dot(d) > 0.0).then_some((p + nq * tq, p + nk * tk));
        }
        let s = schnitt(q + nq * tq, e, p + nk * tk, d)?;
        ((s - p).length() <= MAX_ECKE * tk.max(tq)).then_some((s, s))
    };
    let mut out = Vec::new();
    for k in 0..n {
        let (p, d, l, nk) = kante(outline, k);
        let next = (k + 1) % n;
        if let Some((a, b)) = ecke(k) {
            out.push(a);
            if (b - a).length() > EPS {
                out.push(b);
                aus[je[k][0].3].stirn += (b - a).length();
            }
        } else if anfang(k) {
            out.push(p);
            out.push(p + nk * je[k][0].2);
            aus[je[k][0].3].seiten += 1;
        } else {
            out.push(p);
        }
        for &(u0, u1, t, i) in &je[k] {
            let innen_a = if u0 > EPS {
                out.push(p + d * u0);
                out.push(p + d * u0 + nk * t);
                aus[i].seiten += 1;
                p + d * u0 + nk * t
            } else {
                *out.last().unwrap_or(&p)
            };
            let innen_b = if u1 < l - EPS {
                let b = p + d * u1 + nk * t;
                out.push(b);
                out.push(p + d * u1);
                aus[i].seiten += 1;
                b
            } else if let Some((b, _)) = ecke(next) {
                // Die nächste Kante setzt an der inneren Ecke an
                b
            } else {
                let b = p + d * l + nk * t;
                out.push(b);
                aus[i].seiten += 1;
                b
            };
            aus[i].stirn += (innen_b - innen_a).length();
        }
    }
    let neu = polygon::simplified(&out);
    if neu.len() < 3 || !polygon::is_simple(&neu) {
        for a in aus.iter_mut() {
            (a.stirn, a.seiten) = (0.0, 0);
        }
        return outline.to_vec();
    }
    neu
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rechteck() -> Vec<Vec3> {
        vec![
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ]
    }

    fn kerbe(kante: usize, u: (f64, f64), tiefe: f64) -> Aussparung {
        Aussparung {
            opening: None,
            seg: 0,
            stelle: Default::default(),
            kante,
            u,
            tiefe,
            dicke: 200.0,
            flaeche: 0.0,
            stirn: 0.0,
            seiten: 0,
        }
    }

    /// Eine Kerbe mitten in einer Kante: Fläche um Breite × Tiefe kleiner,
    /// Umfang um zwei Tiefen größer, Stirn so lang wie die Kerbe breit.
    #[test]
    fn kerbe_in_der_kante() {
        let o = rechteck();
        let mut aus = [kerbe(0, (6000.0, 7200.0), 240.0)];
        let neu = aussparen(&o, &mut aus);
        assert_eq!(neu.len(), 8);
        let a = polygon::area(&o) - polygon::area(&neu);
        assert!((a - 1200.0 * 240.0).abs() < 1e-3);
        let u = polygon::perimeter(&neu) - polygon::perimeter(&o);
        assert!((u - 2.0 * 240.0).abs() < 1e-6);
        assert!((aus[0].stirn - 1200.0).abs() < 1e-6);
        assert_eq!(aus[0].seiten, 2);
        assert!(neu.contains(&vec3(6000.0, 240.0, 0.0)));
    }

    /// Über eine Ecke: zwei Kerben treffen sich in der inneren Ecke, je
    /// eine Seite, die Stirn läuft innen um die Ecke.
    #[test]
    fn kerbe_ueber_die_ecke() {
        let o = rechteck();
        let mut aus = [
            kerbe(0, (8800.0, 10000.0), 240.0),
            kerbe(1, (0.0, 1500.0), 300.0),
        ];
        let neu = aussparen(&o, &mut aus);
        assert!(neu.contains(&vec3(9700.0, 240.0, 0.0)), "{neu:?}");
        assert!(!neu.contains(&vec3(10000.0, 0.0, 0.0)));
        assert_eq!((aus[0].seiten, aus[1].seiten), (1, 1));
        assert!((aus[0].stirn - 900.0).abs() < 1e-6);
        assert!((aus[1].stirn - 1260.0).abs() < 1e-6);
        let a = polygon::area(&o) - polygon::area(&neu);
        assert!((a - (1200.0 * 240.0 + 300.0 * 1500.0 - 300.0 * 240.0)).abs() < 1e-3);
    }

    /// Über einen geraden Knick im Umriss (zwei Kanten in einer Flucht):
    /// eine durchgehende Kerbe; mit verschiedener Tiefe eine Stufe in der
    /// Stirn.
    #[test]
    fn kerbe_ueber_den_knick() {
        let mut o = rechteck();
        o.insert(1, vec3(5000.0, 0.0, 0.0));
        let mut aus = [
            kerbe(0, (4400.0, 5000.0), 240.0),
            kerbe(1, (0.0, 600.0), 240.0),
        ];
        let neu = aussparen(&o, &mut aus);
        let a = polygon::area(&o) - polygon::area(&neu);
        assert!((a - 1200.0 * 240.0).abs() < 1e-3, "{neu:?}");
        assert_eq!((aus[0].seiten, aus[1].seiten), (1, 1));
        assert!((aus[0].stirn + aus[1].stirn - 1200.0).abs() < 1e-6);
        let u = polygon::perimeter(&neu) - polygon::perimeter(&o);
        assert!((u - 2.0 * 240.0).abs() < 1e-6);
        // Tiefer auf der zweiten Kante: Stufe von 60 mm in der Stirn
        aus[1].tiefe = 300.0;
        let neu = aussparen(&o, &mut aus);
        assert!(neu.contains(&vec3(5000.0, 240.0, 0.0)), "{neu:?}");
        assert!(neu.contains(&vec3(5000.0, 300.0, 0.0)), "{neu:?}");
        assert!((aus[0].stirn + aus[1].stirn - 1260.0).abs() < 1e-6);
    }

    /// An einer zu spitzen Ecke läge die innere Ecke weit draußen: Die
    /// Kerben treffen sich nicht, und kreuzen sie sich, bleibt der Umriss.
    #[test]
    fn spitze_ecke_ohne_weite_spitze() {
        let o = vec![
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(0.0, 1500.0, 0.0),
        ];
        let mut aus = [
            kerbe(0, (9000.0, 10000.0), 240.0),
            kerbe(1, (0.0, 1000.0), 240.0),
        ];
        let neu = aussparen(&o, &mut aus);
        // Die Wandecke innen läge gut 3 m von der Spitze weg
        let weit = neu
            .iter()
            .filter(|p| !o.contains(p))
            .map(|p| (*p - vec3(10000.0, 0.0, 0.0)).length())
            .fold(0.0, f64::max);
        assert!(weit <= 1500.0, "{neu:?}");
        assert!(polygon::is_simple(&neu));
        assert!(polygon::area(&neu) <= polygon::area(&o) + 1e-6);
    }

    /// Geometrie zu #143: Ein Eckfenster über eine spitze Ecke (45°) spart
    /// die Decke aus. Der Umriss der Decke reicht dort um Dicke / tan(22,5°)
    /// über den Eckpunkt der Bezugslinie hinaus, mehr als eine Wanddicke.
    #[test]
    fn eckfenster_ueber_spitze_ecke() {
        use crate::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
        use crate::solid::right_of;
        use crate::Model;
        let a = 45f64.to_radians();
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(15000.0, 0.0, 0.0),
            vec3(15000.0, 12000.0 * a.sin() + 4000.0, 0.0),
            vec3(12000.0 * a.cos(), 12000.0 * a.sin(), 0.0),
        ];
        let mut m = Model::with_seed(415);
        let b = m.add_building(2);
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let og = m.runs_above(eg)[0];
        let (st, st_og) = (m.run(eg).unwrap().storey, m.run(og).unwrap().storey);
        let c = m.chain(eg).unwrap();
        let (lp, _, dirs) = c.layout().unwrap();
        let n = lp.len();
        let k = (0..n)
            .min_by(|&i, &j| lp[i].length().total_cmp(&lp[j].length()))
            .unwrap();
        let v = (k + n - 1) % n;
        let lv = (lp[k] - lp[v]).length();
        let auf = |s: usize, u: f64| lp[s] + dirs[s] * u + right_of(dirs[s]) * c.outer_offset();
        let ecke = *c
            .face_corners(c.outer_offset())
            .iter()
            .min_by(|p, q| p.length().total_cmp(&q.length()))
            .unwrap();
        let id = m
            .add_opening(Opening {
                kind: OpeningKind::Window,
                path: vec![auf(v, lv - 900.0), ecke, auf(k, 900.0)],
                bottom: HeightRef::Line(st, HeightLine::Sill),
                top: HeightRef::Line(st_og, HeightLine::Lintel),
                reveal: None,
                install: Default::default(),
                swing: None,
                lock: Default::default(),
            })
            .unwrap();
        let d = m.floor(eg).unwrap().unwrap();
        assert_eq!(d.aussparungen.len(), 2, "{:?}", d.aussparungen);
        assert!(d.aussparungen.iter().all(|x| x.opening == Some(id)));
        assert!(polygon::is_simple(&d.outline));
        assert!(m.spart_aus(eg, id));
    }

    /// Zu tief: der Umriss würde sich kreuzen, er bleibt.
    #[test]
    fn zu_tief_bleibt() {
        let o = rechteck();
        let mut aus = [kerbe(0, (1000.0, 2000.0), 9000.0)];
        assert_eq!(aussparen(&o, &mut aus), o);
        assert_eq!(aus[0].seiten, 0);
    }

    /// Im Modell: Ein Fenster vom Brüstung EG bis Sturz OG spart die Decke
    /// über dem EG aus, ein Fenster nur im EG nicht.
    #[test]
    fn fenster_ueber_zwei_geschosse() {
        use crate::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
        use crate::Model;
        let mut m = Model::with_seed(415);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let og = m.runs_above(eg)[0];
        let (st, st_og) = (m.run(eg).unwrap().storey, m.run(og).unwrap().storey);
        let seg = (0..4)
            .find(|&k| m.outer_face(eg, k).is_some_and(|f| f.n.y < -0.5))
            .unwrap();
        let f = m.outer_face(eg, seg).unwrap();
        let fenster = |x0: f64, oben| Opening {
            kind: OpeningKind::Window,
            path: vec![
                f.at(f.u_of(vec3(x0, 0.0, 0.0)), 0.0),
                f.at(f.u_of(vec3(x0 + 1000.0, 0.0, 0.0)), 0.0),
            ],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: oben,
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Default::default(),
        };
        let vorher = m.floor(eg).unwrap().unwrap();
        assert!(vorher.aussparungen.is_empty());
        m.add_opening(fenster(2000.0, HeightRef::Line(st, HeightLine::Lintel)))
            .unwrap();
        assert!(m.floor(eg).unwrap().unwrap().aussparungen.is_empty());
        let id = m
            .add_opening(fenster(6500.0, HeightRef::Line(st_og, HeightLine::Lintel)))
            .unwrap();
        let d = m.floor(eg).unwrap().unwrap();
        assert_eq!(d.aussparungen.len(), 1, "{:?}", d.aussparungen);
        let a = d.aussparungen[0];
        assert_eq!((a.opening, a.seiten), (Some(id), 2));
        // Mindestens die Öffnung breit, durch die ganze Tasche tief
        assert!(a.stirn >= 1000.0 - 1e-6, "{a:?}");
        assert!(a.tiefe > 100.0, "{a:?}");
        let weg = vorher.area() - d.area();
        assert!((weg - a.stirn * a.tiefe).abs() < 1.0, "{weg} {a:?}");
        use crate::ElementId;
        // Randschalung: nur die beiden Seiten kommen dazu, die Stirn
        // ersetzt das Stück Rand vor der Öffnung
        assert!((d.perimeter() - vorher.perimeter() - 2.0 * a.tiefe).abs() < 1e-6);
        assert_eq!(a.dicke, d.band().1 - d.band().0);
        let schalung = |m: &Model| crate::qto::formwork_qto(m, m.floor_of(eg).unwrap()).unwrap();
        let rand = schalung(&m).edge;
        // Laibungen an der Aussparung: zwei Seiten je Deckendicke, Stirn
        use crate::qto_laibung::{SLAB_EDGE, SLAB_REVEAL};
        let s = crate::qto::schedule(&m);
        let wert = |k: &str| {
            s.auto
                .iter()
                .find(|x| x.element == id && x.key == k)
                .map(|x| x.value)
        };
        let (zb, zt) = d.band();
        assert_eq!(wert(SLAB_REVEAL), Some(2.0 * (zt - zb)));
        assert_eq!(wert(SLAB_EDGE), Some(a.stirn));
        assert!(s
            .auto
            .iter()
            .filter(|x| x.key == SLAB_REVEAL || x.key == SLAB_EDGE)
            .all(|x| x.kg == Some(336)));
        let fenster_eg = s.auto.iter().filter(|x| x.key == SLAB_EDGE).count();
        assert_eq!(fenster_eg, 1, "nur das Fenster über zwei Geschosse");
        // Innenlaibung ohne das Stück im Deckenband (VOB, Doppelzählung):
        // Gegenüber dem gleich breiten Fenster nur im EG kommen genau die
        // beiden Seiten um den Höhenunterschied dazu
        use crate::qto_laibung::INNER;
        let innen = |e: ElementId| {
            s.auto
                .iter()
                .find(|x| x.element == e && x.key == INNER)
                .map_or(0.0, |x| x.value)
        };
        let nur_eg = m
            .elements()
            .iter()
            .find(|(e, x)| *e != id && matches!(x.kind, crate::ElementKind::Opening(_)))
            .map(|(e, _)| e)
            .unwrap();
        let hoch = |e: ElementId| {
            let (z0, z1) = m.opening_band(m.opening(e).unwrap()).unwrap();
            z1 - z0
        };
        let mehr = innen(id) + wert(SLAB_REVEAL).unwrap() - innen(nur_eg);
        assert!(
            (mehr - 2.0 * (hoch(id) - hoch(nur_eg))).abs() < 1e-6,
            "{mehr} {} {}",
            hoch(id),
            hoch(nur_eg)
        );
        // Fläche und Volumen nach VOB/C: Die kleine Kerbe wird übermessen,
        // im Rechenweg steht sie als eigener Term
        assert!(a.flaeche > 0.0 && (a.flaeche - weg).abs() < 1.0, "{a:?}");
        let q = crate::qto::floor_qto(&m, eg).unwrap();
        assert!((q.area - vorher.area()).abs() < 1e-3);
        assert!((q.volume - vorher.volume()).abs() < 1.0);
        let reihe = s
            .layer_rows(&m)
            .into_iter()
            .map(|x| x.1)
            .find(|r| r.element == m.floor_of(eg).unwrap() && r.volume > 0.0)
            .unwrap();
        use crate::rechenweg::{weg as rechenweg, Bezug, Modus};
        let w = rechenweg(&m, &reihe, Bezug::Flaeche, Modus::Vob).unwrap();
        assert!(
            w.terme
                .iter()
                .any(|t| t.text.contains("übermessen (bis 2,5 m², VOB/C)")),
            "{w:?}"
        );
        assert!((w.menge() - q.area).abs() < 1.0, "{w:?}");
        // Über 2,5 m²: abgezogen
        let mut gross = d.clone();
        gross.aussparungen[0].flaeche = 2.6e6;
        assert!((gross.vob_mengen().0 - gross.area()).abs() < 1e-6);
        let v = 2.6e6 * gross.params.thickness;
        let soll = gross.volume()
            + if crate::vob::ueber_grenze_vol(v) {
                0.0
            } else {
                v
            };
        assert!((gross.vob_mengen().1 - soll).abs() < 1e-6);
        // Reicht die Öffnung nur ein Stück in die Decke: Kerbe über die
        // ganze Dicke
        use crate::element::LevelRef;
        let mut o = m.opening(id).unwrap().clone();
        o.top = HeightRef::Level(LevelRef {
            offset: -50.0,
            ..LevelRef::bottom(st_og)
        });
        assert!(m.set_opening(id, o.clone()));
        let teil = m.floor(eg).unwrap().unwrap();
        assert_eq!(teil.aussparungen.len(), 1);
        assert!((teil.area() - d.area()).abs() < 1e-3);
        // Bis knapp unter die Decke und wieder hoch: die Decke folgt
        o.top = HeightRef::Level(LevelRef {
            offset: -zt + zb - 1.0,
            ..LevelRef::bottom(st_og)
        });
        assert!(m.set_opening(id, o.clone()));
        let zu = m.floor(eg).unwrap().unwrap();
        assert!(zu.aussparungen.is_empty());
        assert_eq!(zu.outline, vorher.outline);
        assert!((schalung(&m).edge - rand + 2.0 * a.tiefe).abs() < 1e-6);
        o.top = HeightRef::Line(st_og, HeightLine::Lintel);
        assert!(m.set_opening(id, o));
        assert_eq!(m.floor(eg).unwrap().unwrap(), d);
    }

    /// Springt die Wand darüber zurück (Dachterrasse) oder vor, bleibt die
    /// Decke ganz: Die Öffnung hat dort keine Wand, in der sie weiterläuft.
    #[test]
    fn ruecksprung_ohne_kerbe() {
        use crate::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
        use crate::Model;
        for versatz in [-600.0, 600.0, 0.0] {
            let mut m = Model::with_seed(415);
            let b = m.add_building(2);
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ];
            let eg = m.build_from_polygon(b, &pts).unwrap();
            let og = m.runs_above(eg)[0];
            let st = m.run(eg).unwrap().storey;
            let seg = (0..4)
                .find(|&k| m.outer_face(eg, k).is_some_and(|f| f.n.y < -0.5))
                .unwrap();
            let f = m.outer_face(eg, seg).unwrap();
            let id = m
                .add_opening(Opening {
                    kind: OpeningKind::Window,
                    path: vec![
                        f.at(f.u_of(vec3(6500.0, 0.0, 0.0)), 0.0),
                        f.at(f.u_of(vec3(7500.0, 0.0, 0.0)), 0.0),
                    ],
                    bottom: HeightRef::Line(st, HeightLine::Sill),
                    top: HeightRef::Line(st, HeightLine::Lintel),
                    reveal: None,
                    install: Default::default(),
                    swing: None,
                    lock: Default::default(),
                })
                .unwrap();
            if versatz != 0.0 {
                let wand = m.wall_at(og, seg).unwrap();
                m.begin("Versatz");
                assert!(m.set_offset(wand, versatz).is_some());
                m.commit().unwrap();
            }
            assert_eq!(m.spart_aus(eg, id), versatz == 0.0, "{versatz}");
        }
    }

    /// Flachdach mit Attika: Ein Fenster im OG, dessen Oberkante durch die
    /// Flachdachdecke in die Attika reicht, kerbt die Decke nicht, denn
    /// darüber liegt der Dachaufbau (Review W48 B2). Ein Fenster aus dem EG
    /// in das OG kerbt die Decke dazwischen weiter.
    #[test]
    fn attika_ohne_kerbe() {
        use crate::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
        use crate::{LevelEdge, LevelRef, Model};
        let mut m = Model::with_seed(415);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let og = m.runs_above(eg)[0];
        let (st, st_og) = (m.run(eg).unwrap().storey, m.run(og).unwrap().storey);
        assert!(m.set_flat_roof(st_og, true));
        let fd = m.roof_level_of(m.storey(st_og).unwrap().building).unwrap();
        // Die Attika steht bündig auf dem OG
        assert_eq!(m.runs_above(og).len(), 1);
        let seg = (0..4)
            .find(|&k| m.outer_face(eg, k).is_some_and(|f| f.n.y < -0.5))
            .unwrap();
        let f = m.outer_face(eg, seg).unwrap();
        let fenster = |x0: f64, unten, oben| Opening {
            kind: OpeningKind::Window,
            path: vec![
                f.at(f.u_of(vec3(x0, 0.0, 0.0)), 0.0),
                f.at(f.u_of(vec3(x0 + 1000.0, 0.0, 0.0)), 0.0),
            ],
            bottom: unten,
            top: oben,
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Default::default(),
        };
        let attika = HeightRef::Level(LevelRef {
            storey: fd,
            edge: LevelEdge::Bottom,
            offset: 300.0,
        });
        let oben = m
            .add_opening(fenster(
                2000.0,
                HeightRef::Line(st_og, HeightLine::Sill),
                attika,
            ))
            .unwrap();
        let d = m.floor(og).unwrap().unwrap();
        assert!(d.aussparungen.is_empty(), "{:?}", d.aussparungen);
        assert!(!m.spart_aus(og, oben));
        let unten = m
            .add_opening(fenster(
                6500.0,
                HeightRef::Line(st, HeightLine::Sill),
                HeightRef::Line(st_og, HeightLine::Lintel),
            ))
            .unwrap();
        let d = m.floor(eg).unwrap().unwrap();
        assert_eq!(d.aussparungen.len(), 1, "{:?}", d.aussparungen);
        assert!(m.spart_aus(eg, unten));
        assert!(m.floor(og).unwrap().unwrap().aussparungen.is_empty());
    }

    /// Einschalige Wand mit Randdämmstreifen: Der Streifen bekommt an der
    /// Aussparung eine Lücke, so breit wie die Kerbe; Länge, Volumen und
    /// die Randschalung, die er ersetzt, folgen.
    #[test]
    fn streifen_mit_luecke() {
        use crate::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
        use crate::Model;
        let mut m = Model::with_seed(39);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let mono = m
            .layer_sets()
            .iter()
            .find(|(_, t)| t.guid == crate::MONO_TYPE_GUID)
            .map(|(id, _)| id)
            .unwrap();
        m.begin("Wandtyp geändert");
        assert!(m.set_run_type(eg, mono));
        m.commit().unwrap();
        let og = m.runs_above(eg)[0];
        let (st, st_og) = (m.run(eg).unwrap().storey, m.run(og).unwrap().storey);
        let seg = (0..4)
            .find(|&k| m.outer_face(eg, k).is_some_and(|f| f.n.y < -0.5))
            .unwrap();
        let f = m.outer_face(eg, seg).unwrap();
        let vorher = m.floor(eg).unwrap().unwrap();
        assert!(vorher.strip.is_some());
        let k = (0..vorher.strips.len())
            .find(|&k| vorher.strips[k][0].y.abs() < 1.0 && vorher.strips[k][1].y.abs() < 1.0)
            .unwrap();
        assert_eq!(vorher.strip_teile(k).len(), 1);
        m.add_opening(Opening {
            kind: OpeningKind::Window,
            path: vec![
                f.at(f.u_of(vec3(6500.0, 0.0, 0.0)), 0.0),
                f.at(f.u_of(vec3(7500.0, 0.0, 0.0)), 0.0),
            ],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st_og, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Default::default(),
        })
        .unwrap();
        let d = m.floor(eg).unwrap().unwrap();
        assert_eq!(d.aussparungen.len(), 1, "{:?}", d.aussparungen);
        let a = d.aussparungen[0];
        let teile = d.strip_teile(k);
        assert_eq!(teile.len(), 2, "{teile:?}");
        assert_eq!(
            teile.iter().map(|t| t.luecke).collect::<Vec<_>>(),
            [(false, true), (true, false)]
        );
        let breite = (a.stelle.1 - a.stelle.0).length();
        assert!(breite >= 1000.0 - 1e-6, "{a:?}");
        assert!((vorher.strip_length(k) - d.strip_length(k) - breite).abs() < 1e-6);
        assert!((vorher.strip_inner_length(k) - d.strip_inner_length(k) - breite).abs() < 1e-6);
        let w = d.strip.unwrap().width;
        let dv = vorher.strip_volume(k) - d.strip_volume(k);
        assert!((dv - breite * w * d.params.thickness).abs() < 1.0, "{dv}");
        // Körper: zwei Stücke mit je einer Stirn an der Lücke
        let s = d.strip_solid(k);
        assert_eq!(
            s.triangles.len(),
            2 * (vorher.strip_solid(k).triangles.len() + 2)
        );
    }
}

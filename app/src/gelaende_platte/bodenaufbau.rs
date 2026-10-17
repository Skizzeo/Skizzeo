//! Bodenaufbau unter und um die Gründung (Jörn 17.10.): statt einer
//! einzigen Erdreichfläche die Schichten, wie sie vor Ort eingebaut werden.
//! Nur Darstellung; die Mengen rechnet `sk_model::qto_earth` aus denselben
//! Bodenkennwerten.
//!
//! So wie gebaut wird (Jörn 17.10.): Oberboden im Baufeld abgeschoben, der
//! Boden [`AUSKOFFERUNG`] unter OK Gelände ausgekoffert (tiefer, wenn die
//! Gründung tiefer liegt), [`UEBERSTAND`] über die Außenkante der Schürze
//! hinaus, bei Platten unter dem gewachsenen Boden mindestens im
//! Arbeitsraum; die Wände über 1,25 m Tiefe geböscht. Darin Füllsand,
//! nivelliert auf UK Dämmung: Die Platte bekommt keine Aussparung, nur die
//! Fundamentgräben sind bis UK Schürze gezogen. Über dem gewachsenen Boden
//! läuft die Sandplatte in der Böschung der Bodenkennwerte aus. Darüber
//! Verfüllung und Oberboden: im Baufeld angedeckt (Dicke „Oberboden
//! andecken“), sonst gewachsen.
//!
//! Im Schnitt eine Fläche je Schicht mit eigenem Zeichen (gewachsener Boden
//! nach DIN 1356-1, Füllsand Punkte, Kies Kreise, Oberboden und Verfüllung
//! Striche), in 3D die Wände und Sohlen der Aussparung in den Farben der
//! Schichten: Bei ausgeblendetem Gebäude steht der Stand vor dem
//! Betonieren da.

use crate::draw_table::{fill_kind, LineLook, MatLook};
use crate::gelaende_netz::gitter::Gitter;
use sk_math::{polygon, vec3, Vec3};
use sk_model::qto_earth::SENKRECHT_BIS;
use sk_model::solid::{right_of, SectionFrame};
use sk_model::{edge_kind, material, BuildingId, FootingShape, GroundBasis, Model, Solid};
use sk_ui::theme::Theme;
use std::collections::HashMap;

type P2 = [f64; 2];

/// Überstand der Sandplatte über die Außenkante der Schürze (mm). Fest,
/// bis die Mengen die Sandplatte rechnen und ein Feld unter „Baugrund“
/// kommt.
pub(crate) const UEBERSTAND: f64 = 500.0;

/// Tiefe der Auskofferung unter OK Gelände, Mutterboden eingeschlossen
/// (mm, Jörn 17.10.: ca. 80 cm).
pub(crate) const AUSKOFFERUNG: f64 = 800.0;

/// So weit reicht der gewachsene Boden unter die tiefste Schürze und
/// Auskofferung (mm), damit er im Schnitt darunter zu sehen ist.
pub(crate) const UNTER_GRUENDUNG: f64 = 500.0;

/// Schichten von unten nach oben (Platz in [`Stapel`] und in den
/// Schlüsseln): gewachsener Boden, Füllsand, Kies, Verfüllung, Oberboden.
pub(crate) const BODEN: usize = 0;
#[cfg(test)]
pub(crate) const SAND: usize = 1;
#[cfg(test)]
pub(crate) const KIES: usize = 2;
#[cfg(test)]
pub(crate) const VERFUELLUNG: usize = 3;
#[cfg(test)]
pub(crate) const OBERBODEN: usize = 4;
const SCHICHTEN: usize = 5;

/// So viele Looks hängt die Zeichentabelle hinter Rasen blass an:
/// Füllsand, Kies, Verfüllung, Oberboden.
pub(crate) const LOOKS: usize = 4;

/// Abstand der Stützstellen längs einer Schnittlinie in Reichweite eines
/// Hauses (mm): Die Böschung folgt dem Abstand zur Platte.
const SCHRITT: f64 = 100.0;

/// Unterkante und Oberkante je Schicht an einer Stelle (gleich: fehlt).
pub(crate) type Stapel = [(f64, f64); SCHICHTEN];

/// Darstellungsschlüssel der Schichten aus denen der Geländeplatte (Rasen,
/// Erdreich, Rasen blass): gewachsener Boden ist das Erdreich, die übrigen
/// folgen hinter Rasen blass ([`looks`]).
pub(crate) fn schluessel(terrain: [u16; 3]) -> [u16; SCHICHTEN] {
    let n = terrain[2];
    [terrain[1], n + 1, n + 2, n + 3, n + 4]
}

/// Look des gewachsenen Bodens: Zeichen nach DIN 1356-1, Zelle 3 mm.
pub(crate) fn erde(face: [f32; 3], base: MatLook, mm: f32) -> MatLook {
    zeichen(face, base, fill_kind::ERDE, 3.0 * mm, 0.13 * mm)
}

fn zeichen(face: [f32; 3], base: MatLook, kind: f32, zelle: f32, w: f32) -> MatLook {
    MatLook {
        face,
        cut: face,
        kind,
        width_px: w,
        lines: [
            LineLook {
                spacing_px: zelle,
                ..LineLook::default()
            },
            LineLook::default(),
        ],
        line_count: 1,
        ..base
    }
}

/// Looks von Füllsand, Kies, Verfüllung und Oberboden: im Schnitt auf
/// Papier in der Farbe des Stifts „Gelände“, in 3D in Erdfarben.
pub(crate) fn looks(theme: &Theme, paper: [f32; 3], ink: [f32; 3]) -> [MatLook; LOOKS] {
    let mm = theme.px_per_mm;
    let base = MatLook {
        cut_bg: paper,
        cut_fg: ink,
        ..crate::draw_table::fallback_look(theme)
    };
    let striche = |face: [f32; 3], winkel: f32, abstand: f32, strich: f32, luecke: f32| {
        let l = LineLook {
            angle_deg: winkel,
            spacing_px: abstand * mm,
            offset_px: 0.0,
            dash_px: strich * mm,
            gap_px: luecke * mm,
        };
        MatLook {
            face,
            cut: face,
            kind: fill_kind::LINES,
            width_px: 0.13 * mm,
            lines: [l, LineLook::default()],
            line_count: 1,
            ..base
        }
    };
    [
        zeichen(
            [0.86, 0.77, 0.55],
            base,
            fill_kind::SAND,
            1.0 * mm,
            0.18 * mm,
        ),
        zeichen(
            [0.63, 0.62, 0.59],
            base,
            fill_kind::KIES,
            2.4 * mm,
            0.13 * mm,
        ),
        striche([0.56, 0.44, 0.32], 45.0, 1.5, 1.5, 0.75),
        striche([0.34, 0.25, 0.18], 0.0, 1.0, 1.0, 0.6),
    ]
}

/// Bodenaufbau einer Gründung (mm, z zu ±0,00).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Aufbau {
    /// Außenumriss über der Gründung (hier endet das Gelände), gegen den
    /// Uhrzeigersinn.
    haus: Vec<P2>,
    /// Plattenumriss = Außenkante der Schürze.
    platte: Vec<P2>,
    /// Innenseite der Schürze und je Ecke der Platte ihre Ecke dort
    /// ([`polygon::Inset`]); `None`, wenn die Schürze alles füllt.
    innen: Option<(Vec<P2>, Vec<usize>)>,
    /// OK Gelände der Erdmengen (ohne Netz für die Wände in 3D), OK
    /// gewachsener Boden, UK Platte, UK Dämmung, UK Kies (ohne Kies UK
    /// Dämmung), UK Schürze, Sohle der Auskofferung.
    t: f64,
    n: f64,
    s: f64,
    b: f64,
    p: f64,
    f: f64,
    ka: f64,
    oberboden: f64,
    andecken: f64,
    /// Bis hier ist der Oberboden abgetragen (Abstand zur Platte).
    baufeld: f64,
    /// Böschung der Sandplatte: Fall je mm waagerecht.
    tan: f64,
    /// Sohle der Auskofferung ab der Platte und Rücklauf ihrer Wand je mm
    /// Höhe (0 = senkrecht).
    w: f64,
    k: f64,
    /// Reichweite im Grundriss (x0, y0, x1, y1).
    pub(crate) kasten: [f64; 4],
}

/// Die Gründungen des Projekts mit ihrem Aufbau und die Dicke des
/// gewachsenen Oberbodens.
#[derive(Clone, Debug, Default)]
pub(crate) struct Bodenaufbau {
    pub(crate) haeuser: Vec<Aufbau>,
    oberboden: f64,
    /// Raster über der Reichweite der Häuser (`Aufbau::kasten`): Eine
    /// Stelle prüft nur die Häuser in ihrer Nähe (Review B-261a, sonst je
    /// Stelle alle Häuser, das Netz der Platten bei 4000 Häusern 0,8 s).
    raster: Gitter,
}

/// Gleich, wenn die Häuser gleich sind; das Raster folgt aus ihnen.
impl PartialEq for Bodenaufbau {
    fn eq(&self, o: &Bodenaufbau) -> bool {
        self.haeuser == o.haeuser && self.oberboden == o.oberboden
    }
}

fn flach(r: &[Vec3]) -> Vec<P2> {
    r.iter().map(|p| [p.x, p.y]).collect()
}

fn raum(r: &[P2], z: f64) -> Vec<Vec3> {
    r.iter().map(|p| vec3(p[0], p[1], z)).collect()
}

/// Liegt `q` im Ring `r`?
fn drin(r: &[P2], q: P2) -> bool {
    let mut c = false;
    let n = r.len();
    for i in 0..n {
        let (a, b) = (r[i], r[(i + 1) % n]);
        if (a[1] > q[1]) != (b[1] > q[1]) {
            let x = a[0] + (q[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if q[0] < x {
                c = !c;
            }
        }
    }
    c
}

/// Abstand von `q` zum Rand des Rings `r`.
fn abstand(r: &[P2], q: P2) -> f64 {
    let n = r.len();
    (0..n)
        .map(|i| {
            let (a, b) = (r[i], r[(i + 1) % n]);
            sk_math::dist_to_segment((q[0], q[1]), (a[0], a[1]), (b[0], b[1]))
        })
        .fold(f64::INFINITY, f64::min)
}

impl Bodenaufbau {
    /// Aus dem Modell: Gründungen ([`Model::ground_bases`]), Bodenkennwerte
    /// des Projekts und „Oberboden andecken“ des Grundstücks.
    pub(crate) fn aus(m: &Model) -> Bodenaufbau {
        m.lesend(|| {
            let boden = m.project().soil;
            let andecken = m.site_soil().spread.max(0.0);
            // die Umrisse je Gebäude, einmal gesammelt (Review B-261b:
            // sonst je Gründung alle Umrisse)
            let umrisse = m.plate_outlines();
            let mut je_haus: HashMap<BuildingId, Vec<&[Vec3]>> = HashMap::new();
            for (b, r) in &umrisse {
                je_haus.entry(*b).or_default().push(r);
            }
            let haeuser: Vec<Aufbau> = m
                .ground_bases()
                .iter()
                .filter_map(|g| {
                    let u = g.building.and_then(|b| je_haus.get(&b));
                    Aufbau::neu(m, g, &boden, andecken, u.map_or(&[][..], |u| &u[..]))
                })
                .collect();
            let raster = Gitter::neu(haeuser.iter().map(|h| h.kasten).collect());
            Bodenaufbau {
                haeuser,
                oberboden: boden.oberboden.max(0.0),
                raster,
            }
        })
    }

    /// Unterkante des Geländes: `unten`, mindestens [`UNTER_GRUENDUNG`] unter
    /// der tiefsten Schürze.
    pub(crate) fn unten(&self, unten: f64) -> f64 {
        self.haeuser
            .iter()
            .map(|h| h.f.min(h.ka) - UNTER_GRUENDUNG)
            .fold(unten, f64::min)
    }

    /// Die Häuser, in deren Reichweite `q` liegt (aus dem Raster).
    fn bei(&self, q: P2) -> impl Iterator<Item = &Aufbau> {
        let mut nah = Vec::new();
        self.raster.bei(&[q[0], q[1], q[0], q[1]], &mut nah);
        nah.into_iter()
            .filter_map(|i| self.haeuser.get(i as usize))
            .filter(move |h| {
                let k = h.kasten;
                q[0] >= k[0] && q[1] >= k[1] && q[0] <= k[2] && q[1] <= k[3]
            })
    }

    /// Liegt `q` auf dem Außenumriss eines Hauses (1 mm)?
    pub(crate) fn am_haus(&self, q: P2) -> bool {
        self.bei(q).any(|h| abstand(&h.haus, q) < 1.0)
    }

    /// Schichten an der Stelle `q` außerhalb der Häuser mit OK Gelände
    /// `top`, von `unten` an.
    fn aussen(&self, q: P2, top: f64, unten: f64) -> Stapel {
        match self.naechstes(q) {
            Some((h, d)) => h.stapel(Lage::Aussen(d), top, unten),
            None => schichten(
                unten,
                [top - self.oberboden, f64::MIN, f64::MIN, f64::MIN, top],
            ),
        }
    }

    /// Das Haus, in dessen Reichweite `q` liegt, mit dem kleinsten Abstand
    /// zur Platte.
    fn naechstes(&self, q: P2) -> Option<(&Aufbau, f64)> {
        self.bei(q)
            .map(|h| (h, abstand(&h.platte, q)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

/// Wo eine Stelle zur Gründung liegt.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Lage {
    /// Außerhalb des Hauses, im Abstand zur Platte.
    Aussen(f64),
    /// Im Haus, aber außerhalb der Platte (Sockelrücksprung).
    Rand,
    /// Unter der Schürze.
    Ring,
    /// Innerhalb der Schürze.
    Kern,
}

/// Stapel aus den Oberkanten je Schicht von unten nach oben; eine Schicht
/// unter der Oberkante der vorigen fehlt.
fn schichten(unten: f64, ziele: [f64; SCHICHTEN]) -> Stapel {
    let mut z = unten;
    let mut out = [(unten, unten); SCHICHTEN];
    for (o, ziel) in out.iter_mut().zip(ziele) {
        let hi = ziel.max(z);
        *o = (z, hi);
        z = hi;
    }
    out
}

impl Aufbau {
    fn neu(
        m: &Model,
        g: &GroundBasis,
        boden: &sk_model::qto_earth::Boden,
        andecken: f64,
        umrisse: &[&[Vec3]],
    ) -> Option<Aufbau> {
        let fu = m.foundation(g.run)?.ok()?;
        let platte = flach(&polygon::to_ccw(&g.outline));
        if platte.len() < 3 {
            return None;
        }
        // ein Punkt knapp innerhalb der Platte: in ihm liegt das Haus
        let (a, b) = (platte[0], platte[1]);
        let l = (b[0] - a[0]).hypot(b[1] - a[1]).max(1e-9);
        let innen_pt = [
            (a[0] + b[0]) * 0.5 - (b[1] - a[1]) / l,
            (a[1] + b[1]) * 0.5 + (b[0] - a[0]) / l,
        ];
        let haus = umrisse
            .iter()
            .map(|r| flach(&polygon::to_ccw(r)))
            .find(|r| drin(r, innen_pt))
            .unwrap_or_else(|| platte.clone());
        let innen = match &fu.footing {
            FootingShape::Ring(i) if i.map.len() == platte.len() => {
                Some((flach(&i.pts), i.map.clone()))
            }
            _ => None,
        };
        let o = boden.oberboden.max(0.0);
        let (t, s, f) = (g.terrain_z, g.slab_bottom_z, g.footing_bottom_z);
        let n = t - o;
        let b = g.insulation_bottom_z.min(s);
        // Kies unter der Dämmung nur, wenn er eingebaut wird (Bodenkennwerte)
        let p = b - boden.schicht();
        let ka = (t - AUSKOFFERUNG).min(p);
        let tan = boden.boeschung.clamp(1.0, 89.0).to_radians().tan();
        let w = if b < n {
            UEBERSTAND.max(boden.arbeitsraum)
        } else {
            UEBERSTAND
        };
        let k = if t - ka > SENKRECHT_BIS {
            1.0 / tan
        } else {
            0.0
        };
        let reichweite = (w + (n - ka).max(0.0) * k).max(UEBERSTAND + (b - n).max(0.0) / tan);
        let baufeld = boden.rand.max(0.0).max(reichweite);
        let mut kasten = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for q in platte.iter().chain(&haus) {
            kasten = [
                kasten[0].min(q[0]),
                kasten[1].min(q[1]),
                kasten[2].max(q[0]),
                kasten[3].max(q[1]),
            ];
        }
        let r = baufeld + 1.0;
        Some(Aufbau {
            haus,
            platte,
            innen,
            t,
            n,
            s,
            b,
            p,
            f,
            ka,
            oberboden: o,
            andecken: andecken.min(o),
            baufeld,
            tan,
            w,
            k,
            kasten: [kasten[0] - r, kasten[1] - r, kasten[2] + r, kasten[3] + r],
        })
    }

    /// Wo `q` zur Gründung liegt.
    fn lage(&self, q: P2) -> Lage {
        if !drin(&self.haus, q) {
            Lage::Aussen(abstand(&self.platte, q))
        } else if !drin(&self.platte, q) {
            Lage::Rand
        } else if self.innen.as_ref().is_some_and(|(r, _)| drin(r, q)) {
            Lage::Kern
        } else {
            Lage::Ring
        }
    }

    /// Schichten einer Lage, außen unter OK Gelände `top`.
    fn stapel(&self, lage: Lage, top: f64, unten: f64) -> Stapel {
        let nie = f64::MIN;
        let ziele = match lage {
            Lage::Aussen(d) => {
                let on = top - self.oberboden;
                if d > self.baufeld {
                    [on, nie, nie, nie, top]
                } else {
                    let oa = (top - self.andecken).max(on);
                    let sohle = if d <= self.w {
                        self.ka
                    } else if self.k > 0.0 {
                        self.ka + (d - self.w) / self.k
                    } else {
                        f64::INFINITY
                    };
                    // ausgekoffert: Füllsand bis UK Dämmung, höchstens bis
                    // zum gewachsenen Boden; darüber die Böschung
                    let boeschung = self.b - (d - UEBERSTAND).max(0.0) * self.tan;
                    let sand = if sohle < on {
                        boeschung.max(on.min(self.b))
                    } else {
                        boeschung
                    };
                    [sohle.min(on), sand.min(oa), nie, oa, top]
                }
            }
            Lage::Rand => [self.ka.min(self.b), self.b, nie, nie, nie],
            Lage::Ring => [self.ka.min(self.f), self.f, nie, nie, nie],
            Lage::Kern => [self.ka.min(self.p), self.p, self.b, nie, nie],
        };
        schichten(unten, ziele)
    }

    /// Abstände zur Platte, an denen sich die Schichten außen knicken
    /// (senkrechte Wand der Auskofferung: zwei dicht beieinander).
    fn knicke(&self) -> [f64; 5] {
        let wand = if self.k > 0.0 {
            self.w + (self.n - self.ka).max(0.0) * self.k
        } else {
            self.w + 1.0
        };
        [
            self.w,
            wand,
            UEBERSTAND,
            UEBERSTAND + (self.b - self.n).max(0.0) / self.tan,
            self.baufeld,
        ]
    }
}

/// Oberste Schicht eines Stapels, die da ist.
fn oberste(st: &Stapel) -> Option<usize> {
    (0..SCHICHTEN).rev().find(|&i| st[i].1 - st[i].0 > 0.5)
}

/// Schnittflächen der Schichten zwischen `u0` und `u1` (Stapel an beiden
/// Enden), dazu feine Linien zwischen den Schichten; `deckel`: auch über
/// der obersten (im Haus der Rand der Aussparung).
fn streifen(
    s: &mut Solid,
    f: &SectionFrame,
    (u0, a): (f64, &Stapel),
    (u1, b): (f64, &Stapel),
    keys: [u16; SCHICHTEN],
    deckel: bool,
) {
    let oben = oberste(a).max(oberste(b));
    for i in 0..SCHICHTEN {
        let (da, db) = (a[i].1 - a[i].0, b[i].1 - b[i].0);
        if da.max(db) <= 0.5 {
            continue;
        }
        s.mat = keys[i] | material::CUT;
        s.quad(
            f.pt(u0, a[i].0),
            f.pt(u1, b[i].0),
            f.pt(u1, b[i].1),
            f.pt(u0, a[i].1),
            f.n,
        );
        if deckel || Some(i) != oben {
            s.edge(f.pt(u0, a[i].1), f.pt(u1, b[i].1));
        }
    }
}

/// Schnitt durch den Bodenaufbau: `oben` sind die Strecken der
/// Geländelinie außerhalb der Häuser (Lauflänge, Höhe), darunter die
/// Schichten bis `unten`; in den Häusern die Schichten unter der
/// Aussparung, soweit die Geländelinie reicht. Flächen in Richtung `f.n`.
pub(crate) fn schnitt(
    ba: &Bodenaufbau,
    f: &SectionFrame,
    oben: &[(P2, P2)],
    unten: f64,
    keys: [u16; SCHICHTEN],
) -> Solid {
    let mut s = Solid {
        edge_kind: edge_kind::FINE,
        ..Solid::default()
    };
    let grund = |u: f64| {
        let p = f.pt(u, 0.0);
        [p.x, p.y]
    };
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &(a, b) in oben {
        let (a, b) = if a[0] <= b[0] { (a, b) } else { (b, a) };
        (lo, hi) = (lo.min(a[0]), hi.max(b[0]));
        if b[0] - a[0] < 1e-6 {
            continue;
        }
        let z = |u: f64| a[1] + (b[1] - a[1]) * (u - a[0]) / (b[0] - a[0]);
        for (u0, u1) in stuetzen(ba, a[0], b[0], &grund) {
            let st0 = ba.aussen(grund(u0), z(u0), unten);
            let st1 = ba.aussen(grund(u1), z(u1), unten);
            // zwischen zwei Stützstellen gilt das Haus der Mitte
            let (st0, st1) = match ba.naechstes(grund((u0 + u1) * 0.5)) {
                Some((h, _)) => {
                    let d = |u: f64| Lage::Aussen(abstand(&h.platte, grund(u)));
                    (h.stapel(d(u0), z(u0), unten), h.stapel(d(u1), z(u1), unten))
                }
                None => (st0, st1),
            };
            streifen(&mut s, f, (u0, &st0), (u1, &st1), keys, false);
        }
    }
    // in den Häusern
    let p0 = f.pt(0.0, 0.0);
    for h in &ba.haeuser {
        let schnitte = |r: &[P2]| polygon::plane_intervals(&raum(r, 0.0), p0, f.n, f.along);
        let grenzen: Vec<f64> = schnitte(&h.platte)
            .into_iter()
            .chain(h.innen.as_ref().map_or(Vec::new(), |(r, _)| schnitte(r)))
            .flat_map(|(a, b)| [a, b])
            .collect();
        for (a, b) in schnitte(&h.haus) {
            let (a, b) = (a.max(lo), b.min(hi));
            if a >= b {
                continue;
            }
            let mut us: Vec<f64> = grenzen
                .iter()
                .copied()
                .filter(|&u| u > a && u < b)
                .chain([a, b])
                .collect();
            us.sort_by(f64::total_cmp);
            us.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
            let mut vorher: Option<f64> = None;
            for w in us.windows(2) {
                let lage = h.lage(grund((w[0] + w[1]) * 0.5));
                let st = h.stapel(lage, h.t, unten);
                streifen(&mut s, f, (w[0], &st), (w[1], &st), keys, true);
                // senkrecht zur Nachbarschicht, wo die Aussparung springt
                let top = oberste(&st).map_or(unten, |i| st[i].1);
                if let Some(z) = vorher {
                    if (z - top).abs() > 0.5 {
                        s.edge(f.pt(w[0], z), f.pt(w[0], top));
                    }
                }
                vorher = Some(top);
            }
        }
    }
    s
}

/// Stützstellen zwischen `a` und `b` (Lauflänge): in Reichweite eines
/// Hauses alle [`SCHRITT`] und an den Knicken der Schichten (Abstand zur
/// Platte gleich Überstand, Fuß der Böschung, Rand des Baufelds), sonst
/// nur die Enden.
fn stuetzen(ba: &Bodenaufbau, a: f64, b: f64, grund: &dyn Fn(f64) -> P2) -> Vec<(f64, f64)> {
    let nah = |u: f64| ba.naechstes(grund(u)).is_some();
    let n = ((b - a) / SCHRITT).ceil().max(1.0) as usize;
    let mut us: Vec<f64> = (0..=n).map(|i| a + (b - a) * i as f64 / n as f64).collect();
    if !us.iter().any(|&u| nah(u)) && !nah((a + b) * 0.5) {
        return vec![(a, b)];
    }
    let mut dazu = Vec::new();
    for w in us.windows(2) {
        let Some((h, _)) = ba.naechstes(grund((w[0] + w[1]) * 0.5)) else {
            continue;
        };
        let d = |u: f64| abstand(&h.platte, grund(u));
        for k in h.knicke() {
            let (mut x, mut y) = (w[0], w[1]);
            if (d(x) - k) * (d(y) - k) >= 0.0 {
                continue;
            }
            for _ in 0..40 {
                let m = (x + y) * 0.5;
                if (d(x) - k) * (d(m) - k) <= 0.0 {
                    y = m;
                } else {
                    x = m;
                }
            }
            dazu.push((x + y) * 0.5);
        }
    }
    us.extend(dazu);
    us.sort_by(f64::total_cmp);
    us.dedup_by(|x, y| (*x - *y).abs() < 1e-3);
    us.windows(2).map(|w| (w[0], w[1])).collect()
}

/// Senkrechte Wand von `a` nach `b` (Grundriss), Fläche rechts der
/// Laufrichtung, je Schicht zwischen `ab` und den Stapeln an beiden Enden;
/// feine Kanten zwischen den Schichten.
fn wand(
    s: &mut Solid,
    (a, sa): (P2, &Stapel),
    (b, sb): (P2, &Stapel),
    ab: f64,
    keys: [u16; SCHICHTEN],
) {
    let d = vec3(b[0] - a[0], b[1] - a[1], 0.0).normalized();
    let pa = |z: f64| vec3(a[0], a[1], z);
    let pb = |z: f64| vec3(b[0], b[1], z);
    for i in 0..SCHICHTEN {
        let (a0, a1) = (sa[i].0.max(ab), sa[i].1.max(ab));
        let (b0, b1) = (sb[i].0.max(ab), sb[i].1.max(ab));
        if (a1 - a0).max(b1 - b0) <= 0.5 {
            continue;
        }
        s.mat = keys[i];
        s.quad(pa(a0), pb(b0), pb(b1), pa(a1), right_of(d));
        if a0 > ab + 0.5 || b0 > ab + 0.5 {
            s.edge_kind = edge_kind::FINE;
            s.edge(pa(a0), pb(b0));
        }
    }
    s.edge_kind = edge_kind::VIEW;
    s.edge(pa(ab), pb(ab));
}

/// Die Punkte von `a` nach `b` (in ihrer Höhe), dazwischen die aus `teile`,
/// die auf der Strecke liegen, der Reihe nach.
fn auf_kante(a: Vec3, b: Vec3, teile: &[P2]) -> Vec<Vec3> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let mut ts: Vec<f64> = teile
        .iter()
        .filter(|q| sk_math::dist_to_segment((q[0], q[1]), (a.x, a.y), (b.x, b.y)) < 1e-6)
        .map(|q| ((q[0] - a.x) * dx + (q[1] - a.y) * dy) / l2.max(1e-18))
        .filter(|&t| t > 1e-9 && t < 1.0 - 1e-9)
        .collect();
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    let mut out = vec![a];
    out.extend(ts.into_iter().map(|t| a + (b - a) * t));
    out.push(b);
    out
}

/// Waagerechte Fläche nach oben aus Dreiecken.
fn deckel(s: &mut Solid, p: [Vec3; 3]) {
    s.triangles.push(sk_model::solid::Tri {
        p,
        n: vec3(0.0, 0.0, 1.0),
        mat: s.mat,
        uv: [[0.0; 2]; 3],
        elem: s.elem,
        layer: sk_model::solid::NO_LAYER,
    });
}

/// Die Aussparung in 3D: Wände am Haus von OK Gelände (`hoehe`, ohne Netz
/// die Ebene der Erdmengen) bis UK Schürze, ihre Sohle unter der Schürze,
/// die Wände innen an der Schürze und oben die kapillarbrechende Schicht;
/// je Schicht in ihrer Farbe. `teilung`: Teilstellen einer Kante (0 bis
/// 1), an denen sich das Netz knickt.
pub(crate) fn grube(
    ba: &Bodenaufbau,
    hoehe: &dyn Fn(P2) -> Option<f64>,
    teilung: &dyn Fn(P2, P2) -> Vec<f64>,
    unten: f64,
    keys: [u16; SCHICHTEN],
) -> Solid {
    let mut s = Solid {
        edge_kind: edge_kind::VIEW,
        ..Solid::default()
    };
    let lerp = |a: P2, b: P2, t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    for h in &ba.haeuser {
        // am Haus nach innen gerichtet: die Kanten rückwärts; die Teilstellen
        // bekommt auch die Sohle, damit der Körper dicht ist (Export)
        let mut teile: Vec<P2> = Vec::new();
        let n = h.haus.len();
        for i in 0..n {
            let (a, b) = (h.haus[(i + 1) % n], h.haus[i]);
            let mut ts = teilung(a, b);
            ts.extend([0.0, 1.0]);
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            teile.extend(ts[1..ts.len() - 1].iter().map(|&t| lerp(a, b, t)));
            for w in ts.windows(2) {
                let (p, q) = (lerp(a, b, w[0]), lerp(a, b, w[1]));
                let st = |x: P2| {
                    let top = hoehe(x).unwrap_or(h.t);
                    h.stapel(Lage::Aussen(abstand(&h.platte, x)), top, unten)
                };
                wand(&mut s, (p, &st(p)), (q, &st(q)), h.f, keys);
            }
        }
        let ring = h.stapel(Lage::Ring, h.t, unten);
        s.mat = keys[oberste(&ring).unwrap_or(BODEN)];
        let sohle = |r: &[P2]| raum(r, h.f);
        match &h.innen {
            Some((r, map)) => {
                // Sohle unter der Schürze: je Kante der Platte eine Zelle
                let (c0, c1) = (sohle(&h.platte), sohle(r));
                let m = c0.len();
                for i in 0..m {
                    let j = (i + 1) % m;
                    for w in auf_kante(c0[i], c0[j], &teile).windows(2) {
                        deckel(&mut s, [w[0], w[1], c1[map[j]]]);
                    }
                    if map[i] != map[j] {
                        deckel(&mut s, [c0[i], c1[map[j]], c1[map[i]]]);
                    }
                    s.edge(c0[i], c0[j]);
                }
                // innen an der Schürze nach außen, oben die Schicht
                let kern = h.stapel(Lage::Kern, h.t, unten);
                let k = r.len();
                for i in 0..k {
                    let (a, b) = (r[i], r[(i + 1) % k]);
                    wand(&mut s, (a, &kern), (b, &kern), h.f, keys);
                }
                let top = oberste(&kern).map_or(h.f, |i| kern[i].1);
                s.mat = keys[oberste(&kern).unwrap_or(BODEN)];
                let c = raum(r, top);
                for t in polygon::triangulate(&c) {
                    deckel(&mut s, t.map(|x| c[x]));
                }
                for i in 0..k {
                    s.edge(c[i], c[(i + 1) % k]);
                }
            }
            None => {
                let c = sohle(&h.platte);
                for t in polygon::triangulate(&c) {
                    deckel(&mut s, t.map(|x| c[x]));
                }
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::Model;

    /// Haus 10 × 8 m, Gelände 30 cm unter ±0,00: Platte 20 cm, Schürze
    /// 35 × 60 cm, Oberboden 30 cm, Kies 15 cm.
    pub(super) fn haus(gelaende: f64) -> Model {
        let mut m = Model::with_seed(3);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(10_000.0, 0.0, 0.0),
            vec3(10_000.0, 8_000.0, 0.0),
            vec3(0.0, 8_000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        m.begin("Gelände");
        m.set_terrain_offset_of(b, gelaende);
        m.commit();
        m
    }

    fn aufbau(m: &Model) -> Aufbau {
        let ba = Bodenaufbau::aus(m);
        assert_eq!(ba.haeuser.len(), 1);
        ba.haeuser[0].clone()
    }

    /// Über das Raster dieselben Häuser wie über alle (Review B-261a): 3 × 3
    /// Häuser zu 10 × 8 m, 12 m auseinander, die Reichweiten überlappen.
    #[test]
    fn raster_wie_alle_haeuser() {
        let mut m = Model::with_seed(3);
        for i in 0..9 {
            let b = m.add_building(1);
            let (ox, oy) = ((i % 3) as f64 * 22_000.0, (i / 3) as f64 * 20_000.0);
            let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
            let pts = [
                p(0.0, 0.0),
                p(10_000.0, 0.0),
                p(10_000.0, 8_000.0),
                p(0.0, 8_000.0),
            ];
            m.build_from_polygon(b, &pts).unwrap();
        }
        let ba = Bodenaufbau::aus(&m);
        assert_eq!(ba.haeuser.len(), 9);
        let alle = |q: P2| {
            ba.haeuser
                .iter()
                .enumerate()
                .filter(|(_, h)| {
                    let k = h.kasten;
                    q[0] >= k[0] && q[1] >= k[1] && q[0] <= k[2] && q[1] <= k[3]
                })
                .map(|(i, h)| (i, abstand(&h.platte, q)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|x| x.0)
        };
        let (mut nah, mut am) = (0, 0);
        for i in -20..90 {
            for j in -20..80 {
                let q = [i as f64 * 777.0, j as f64 * 733.0];
                let r = ba.naechstes(q).map(|(h, _)| h as *const Aufbau);
                let a = alle(q).map(|i| &ba.haeuser[i] as *const Aufbau);
                assert_eq!(r, a, "{q:?}");
                nah += usize::from(r.is_some());
                let brute = ba.haeuser.iter().any(|h| abstand(&h.haus, q) < 1.0);
                assert_eq!(ba.am_haus(q), brute, "{q:?}");
            }
        }
        // auf den Kanten der Häuser
        for h in &ba.haeuser {
            for w in h.haus.windows(2) {
                let q = [(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0];
                assert!(ba.am_haus(q));
                am += 1;
            }
        }
        assert!(nah > 100 && am > 0);
    }

    /// Platte über dem gewachsenen Boden: 80 cm ausgekoffert, Füllsand bis
    /// UK Dämmung, unter der Schürze nur bis UK Schürze.
    #[test]
    fn sandplatte_unter_dem_haus() {
        let m = haus(300.0);
        let h = aufbau(&m);
        assert_eq!(h.n, h.t - 300.0);
        assert!(h.b <= h.s && h.b > h.n);
        assert_eq!(h.ka, h.t - AUSKOFFERUNG);
        assert_eq!(h.k, 0.0);
        let tief = -10_000.0;
        let kern = h.stapel(Lage::Kern, h.t, tief);
        assert_eq!(kern[BODEN], (tief, h.ka));
        assert_eq!(kern[SAND], (h.ka, h.p));
        assert_eq!(kern[KIES].1 - kern[KIES].0, h.b - h.p);
        assert_eq!(kern[OBERBODEN].1 - kern[OBERBODEN].0, 0.0);
        let ring = h.stapel(Lage::Ring, h.t, tief);
        assert_eq!(ring[SAND], (h.ka, h.f));
        // außen am Haus: Sand von der Sohle bis unter den angedeckten
        // Oberboden
        let a = h.stapel(Lage::Aussen(0.0), h.t, tief);
        assert_eq!(a[OBERBODEN], (h.t - 200.0, h.t));
        assert_eq!(a[SAND], (h.ka, h.t - 200.0));
        // knapp hinter der senkrechten Wand: gewachsener Boden, darauf die
        // Böschung der Sandplatte
        let b = h.stapel(Lage::Aussen(UEBERSTAND + 50.0), h.t, tief);
        assert_eq!(b[BODEN], (tief, h.n));
        assert_eq!(b[SAND], (h.n, h.t - 200.0));
        // 1 m weiter kein Sand mehr
        let w = h.stapel(Lage::Aussen(UEBERSTAND + 1_000.0), h.t, tief);
        assert_eq!(w[SAND].1 - w[SAND].0, 0.0);
        assert!(w[VERFUELLUNG].1 > w[VERFUELLUNG].0);
        // hinter dem Baufeld gewachsener Oberboden in voller Dicke
        let g = h.stapel(Lage::Aussen(h.baufeld + 1.0), h.t, tief);
        assert_eq!(g[OBERBODEN], (h.t - 300.0, h.t));
        assert_eq!(g[BODEN], (tief, h.t - 300.0));
        assert_eq!(
            Bodenaufbau::aus(&m).unten(0.0),
            h.ka.min(h.f) - UNTER_GRUENDUNG
        );
    }

    /// Platte unter dem gewachsenen Boden: ausgekoffert im Arbeitsraum,
    /// Füllsand bis UK Dämmung, darüber verfüllt.
    #[test]
    fn baugrube_mit_arbeitsraum() {
        let m = haus(-500.0);
        let h = aufbau(&m);
        assert!(h.b < h.n);
        let tief = -10_000.0;
        let im = h.stapel(Lage::Aussen(h.w - 100.0), h.t, tief);
        assert_eq!(im[BODEN].1, h.ka);
        assert_eq!(im[SAND], (h.ka, h.b));
        assert_eq!(im[VERFUELLUNG], (h.b, h.t - 200.0));
        let aus = h.stapel(Lage::Aussen(h.w + 100.0), h.t, tief);
        assert_eq!(aus[BODEN].1, h.t - 300.0);
        assert_eq!(aus[SAND].1 - aus[SAND].0, 0.0);
    }

    /// Schnitt quer durch das Haus: Flächen aller Schichten, keine über UK
    /// Dämmung innerhalb der Schürze.
    #[test]
    fn schnitt_quer() {
        let m = haus(300.0);
        let ba = Bodenaufbau::aus(&m);
        let h = &ba.haeuser[0];
        let f = SectionFrame::new(vec3(5_000.0, 4_000.0, 0.0), vec3(0.0, 1.0, 0.0));
        // Lauflänge längs x: along = (−1, 0) für n = +y
        let gx = |x: f64| -x;
        let oben = [
            ([gx(-5_000.0), h.t], [gx(0.0), h.t]),
            ([gx(10_000.0), h.t], [gx(15_000.0), h.t]),
        ];
        let unten = ba.unten(h.t - 500.0);
        let keys = [1, 2, 3, 4, 5];
        let s = schnitt(&ba, &f, &oben, unten, keys);
        let mats: std::collections::BTreeSet<u16> =
            s.triangles.iter().map(|t| t.mat & !material::CUT).collect();
        let mut soll: std::collections::BTreeSet<u16> = [1, 2, 4, 5].into();
        if h.b > h.p {
            soll.insert(3);
        }
        assert_eq!(mats, soll);
        // innerhalb der Schürze nichts über UK Dämmung
        let (r, _) = h.innen.as_ref().expect("Schürze mit Innenseite");
        let x0 = r.iter().map(|q| q[0]).fold(f64::MAX, f64::min) - 1.0;
        let x1 = r.iter().map(|q| q[0]).fold(f64::MIN, f64::max) + 1.0;
        let kern = s
            .triangles
            .iter()
            .filter(|t| t.p.iter().all(|p| p.x >= x0 && p.x <= x1))
            .flat_map(|t| t.p)
            .map(|p| p.z)
            .fold(f64::MIN, f64::max);
        assert_eq!(kern, h.b);
        assert!(s.triangles.iter().all(|t| t.n == f.n));
    }

    /// In 3D: Sohle unter der Schürze auf UK Schürze, oben im Kern die
    /// oberste Schicht auf UK Dämmung.
    #[test]
    fn grube_in_3d() {
        let m = haus(300.0);
        let ba = Bodenaufbau::aus(&m);
        let h = &ba.haeuser[0];
        let keys = [1, 2, 3, 4, 5];
        let s = grube(&ba, &|_| None, &|_, _| Vec::new(), -10_000.0, keys);
        let oben: Vec<_> = s.triangles.iter().filter(|t| t.n.z > 0.5).collect();
        let kern = if h.b > h.p { 3 } else { 2 };
        assert!(oben
            .iter()
            .any(|t| t.mat == kern && t.p.iter().all(|p| p.z == h.b)));
        assert!(oben.iter().any(|t| t.p.iter().all(|p| p.z == h.f)));
        // Wände am Haus zeigen nach innen
        let y0 = h.haus.iter().map(|q| q[1]).fold(f64::MAX, f64::min);
        let w = s
            .triangles
            .iter()
            .find(|t| t.n.z.abs() < 0.5 && t.p.iter().all(|p| (p.y - y0).abs() < 1e-6))
            .expect("Wand vorn");
        assert!(w.n.y > 0.5);
    }
}

/// Ist-Bilder (Mesa unter Xvfb, wie [`crate::gpu_probe`]): Schnitt an der
/// Ecke der Gründung mit und ohne Gebäude, 3D ohne Gebäude, für ein Haus
/// 30 cm über dem Gelände (Sandplatte) und eins 50 cm darunter (Baugrube):
///
/// `xvfb-run -a env SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo gpu_istbilder_bodenaufbau -- --ignored`
#[cfg(all(test, target_os = "linux"))]
mod gpu {
    use super::super::{netz_key, Platte, Sicht, MESH_PLATE};
    use super::*;
    use crate::camera::Camera;
    use crate::scene::Scene;
    use crate::ui::ViewKind;
    use sk_render::Renderer;
    use std::time::Duration;

    const W: u32 = 1600;
    const H: u32 = 1000;

    /// Bild der Ansicht `v` wie in der App, das Gebäude nur mit `haus`.
    fn bild(
        r: &mut Renderer,
        s: &mut Scene,
        v: ViewKind,
        ebene: Option<(Vec3, Vec3)>,
        cam: &Camera,
        haus: bool,
    ) -> Vec<u8> {
        let theme = Theme::dark();
        s.set_theme(&theme);
        r.set_style(crate::style(&theme.env));
        r.set_looks(&s.table().looks_with(1.0, |_| 1.0));
        let modell = if haus {
            s.mesh(v, ebene, &[])
        } else {
            sk_render::MeshData::default()
        };
        r.set_mesh(crate::MESH_MODEL, &modell);
        r.set_sun(None);
        let mut pl = Platte::default();
        let m = s.model();
        pl.im_bild(m);
        let sicht: Sicht = (v, ebene, s.table().terrain, false, false);
        let key = netz_key(pl.fassung, &sicht);
        let netz = loop {
            if let Some(n) = pl.netz_zur_sicht(key, sicht) {
                break n;
            }
            std::thread::sleep(Duration::from_millis(1));
            pl.nachziehen(m, sicht);
        };
        r.set_mesh(MESH_PLATE, &netz);
        r.set_grass(sicht.2[0]);
        let h = pl
            .linien_im_bild(m, (v, ebene), s.bounds(), s.table(), (1.0, 10.0))
            .to_vec();
        r.set_helpers(&h);
        let unten = pl.im_bild(m).iter().map(|p| p.bottom).reduce(f64::min);
        r.set_ground_z(unten.map(|b| pl.aufbau.unten(b) as f32));
        let mut view = cam.view(W, H);
        if v != ViewKind::Persp {
            view.paper = Some(s.table().paper);
        }
        view.patterns = crate::draw_table::pattern_mode(v, theme.env.patterns_3d);
        r.draw(W, H, 0, &view).unwrap();
        r.read_pixels(W, H)
    }

    #[test]
    #[ignore = "braucht einen X-Server (xvfb-run) und SKIZZEO_ISTBILDER"]
    fn gpu_istbilder_bodenaufbau() {
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        std::fs::create_dir_all(&ziel).unwrap();
        let ab = |px: &[u8], n: &str| {
            std::fs::write(ziel.join(n), sk_paint::encode_png_gepackt(W, H, px)).unwrap();
        };
        let k = sk_render::glx::kontext(W as i32, H as i32).expect("GLX-Kontext (DISPLAY?)");
        let theme = Theme::dark();
        let mut r = Renderer::new(k.gl, crate::style(&theme.env)).unwrap();
        for (gelaende, name) in [(300.0, "sandplatte"), (-500.0, "baugrube")] {
            let mut s = Scene::with_model(tests::haus(gelaende));
            let schnitt = ViewKind::Section;
            let ebene = Some((vec3(5_000.0, 4_000.0, 0.0), vec3(0.0, 1.0, 0.0)));
            // linke Ecke der Gründung
            let ecke = Some((
                vec3(-3_000.0, 4_000.0, -2_200.0),
                vec3(3_000.0, 4_000.0, 800.0),
            ));
            let cam = crate::fit_parallel(schnitt, ecke, f64::from(W), f64::from(H));
            for (haus, mit) in [(true, "mit-haus"), (false, "ohne-haus")] {
                ab(
                    &bild(&mut r, &mut s, schnitt, ebene, &cam, haus),
                    &format!("ist-bodenaufbau-{name}-schnitt-{mit}.png"),
                );
            }
            let ganz = Some((
                vec3(-6_000.0, 4_000.0, -2_500.0),
                vec3(16_000.0, 4_000.0, 4_000.0),
            ));
            let cam = crate::fit_parallel(schnitt, ganz, f64::from(W), f64::from(H));
            ab(
                &bild(&mut r, &mut s, schnitt, ebene, &cam, true),
                &format!("ist-bodenaufbau-{name}-schnitt-ganz.png"),
            );
            let mitte = vec3(5_000.0, 4_000.0, -500.0);
            let cam3 = Camera::looking_at(
                mitte + vec3(0.55, -1.0, 0.9).normalized() * 18_000.0,
                mitte,
                45.0,
            );
            ab(
                &bild(&mut r, &mut s, ViewKind::Persp, None, &cam3, false),
                &format!("ist-bodenaufbau-{name}-3d-ohne-haus.png"),
            );
        }
    }
}

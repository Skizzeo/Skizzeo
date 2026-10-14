//! Planungsgelände (G3, plan-gelaendemodul.md „G3 Feinplan“): ein Baustein
//! aus ebenen Teilflächen, an seinem Rand je Kante eine Böschung 1:n bis ans
//! Urgelände. Die Böschung einer Kante ist die Ebene durch die Kante, die nach
//! außen mit 1:n fällt (Auftrag) oder steigt (Abtrag); benachbarte
//! Böschungen treffen sich in Grat- und Kehllinien (Gehrung, wie die Ecken
//! der Baugrube in G2). Alles ist eben, also bleiben Abtrag und Auftrag über
//! dem Netz des Urgeländes exakt je Dreieck ([`Abfrage::erdmengen`]).
//!
//! Das Planungsgelände P ist innen die Teilfläche, im Auftragsbereich einer
//! Kante max(U, Böschung), im Abtragsbereich min(U, Böschung), sonst das
//! Urgelände U. Höhen in m wie im Netz.

use crate::abfrage::Abfrage;
use crate::volumen::{halbieren, Bezug};

/// Ebene Teilfläche eines Bausteins.
#[derive(Clone, Debug, PartialEq)]
pub struct Teil {
    pub ring: Vec<[f64; 2]>,
    pub ebene: Bezug,
}

/// Kante am Rand eines Bausteins von `a` nach `b` (x, y, z); `neigung` n der
/// Böschung 1:n, `None` senkrecht (später Stützmauer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kante {
    pub a: [f64; 3],
    pub b: [f64; 3],
    pub neigung: Option<f64>,
}

/// Ein Baustein: Teilflächen und ihr äußerer Rand als geschlossener Zug
/// (`rand[i].b == rand[i + 1].a`, Umlaufsinn beliebig).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Baustein {
    pub teile: Vec<Teil>,
    pub rand: Vec<Kante>,
}

/// Bereich der Böschung einer Kante: das Polygon zwischen Kante, Grat- oder
/// Kehllinien und der Reichweite bis zum Urgelände, mit der Ebene.
#[derive(Clone, Debug, PartialEq)]
pub struct Bereich {
    pub ring: Vec<[f64; 2]>,
    pub ebene: Bezug,
    /// Nummer der Kante im Rand, wie er übergeben wurde.
    pub kante: usize,
}

/// Was beim Planen auffällt; die Leiste sagt es.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hinweis {
    /// Die Kante ist steiler als ihre Böschung; dort keine Böschung.
    ZuSteil(usize),
    /// Kehllinien kreuzen sich vor dem Urgelände (enge Einbuchtung); der
    /// Bereich endet am Kreuzungspunkt.
    Einbuchtung(usize),
    /// Die Böschung erreicht das Urgelände innerhalb des Netzes nicht.
    OffenesEnde(usize),
    /// Innenwinkel unter 60° am Anfang der Kante: Gehrung an spitzer Ecke
    /// (VOB-Prüfung G3, Frage 4).
    SpitzeEcke(usize),
    /// Die Kante hat keine endlichen Werte oder eine Neigung, die keine
    /// ist (0, negativ, nicht endlich); dort keine Böschung (Review #169
    /// B3, H-169b).
    Ungueltig(usize),
}

/// Unter diesem Innenwinkel ist eine Ecke spitz (VOB-Prüfung G3).
const SPITZ: f64 = std::f64::consts::FRAC_PI_3;

/// Ein Baustein, fertig über dem Urgelände.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Planung {
    pub teile: Vec<Teil>,
    pub auftrag: Vec<Bereich>,
    pub abtrag: Vec<Bereich>,
    pub hinweise: Vec<Hinweis>,
}

/// Abtrag und Auftrag eines Bausteins gegen das Urgelände.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Planmenge {
    /// Fläche der Teilflächen (m², waagerecht).
    pub flaeche: f64,
    /// ∫ (P − U)⁺ und ∫ (U − P)⁺ (m³), getrennt, nie als Saldo.
    pub auftrag: f64,
    pub abtrag: f64,
    /// Fläche, auf der aufgetragen bzw. abgetragen wird (m², waagerecht).
    pub flaeche_auftrag: f64,
    pub flaeche_abtrag: f64,
    /// Schräge Fläche der entstehenden Böschungen (m²).
    pub boeschung: f64,
}

/// Abstand der Proben entlang einer Kante für die Reichweite (m).
const PROBE: f64 = 1.0;

/// Höchstens so viele Proben je Kante (Review #169 B3): eine sehr lange
/// Kante wird gröber geprobt.
const MAX_PROBEN: usize = 2000;

/// Längste Grat- oder Kehllinie als Vielfaches der Reichweite: an sehr
/// spitzen Ecken liefe sie sonst fast entlang der Kante ins Unendliche.
const STRAHL_MAX: f64 = 10.0;

impl Planung {
    /// Den Baustein `b` über das Urgelände `u` legen.
    pub fn neu(b: &Baustein, u: &Abfrage) -> Planung {
        let mut p = Planung {
            teile: b.teile.clone(),
            ..Planung::default()
        };
        let (rand, nummer, umgekehrt) = gegen_uhrzeiger(&b.rand);
        // Kanten ohne Länge unterbrechen sonst die Gehrung (Review #169 B4)
        let (rand, nummer): (Vec<Kante>, Vec<usize>) = rand
            .into_iter()
            .zip(nummer)
            .filter(|(k, _)| !endlich(k) || (k.b[0] - k.a[0]).hypot(k.b[1] - k.a[1]) > 1e-9)
            .unzip();
        let n = rand.len();
        if n < 3 {
            return p;
        }
        let Some((h_lo, h_hi)) = u.hoehenbereich() else {
            return p;
        };
        let geo: Vec<Option<Geo>> = rand.iter().map(Geo::neu).collect();
        for (i, k) in rand.iter().enumerate() {
            let neigung_falsch = k.neigung.is_some_and(|n| !(n.is_finite() && n > 0.0));
            if !endlich(k) || neigung_falsch {
                p.hinweise.push(Hinweis::Ungueltig(nummer[i]));
            } else if let (Some(_), Some(g)) = (k.neigung, geo[i]) {
                if g.steigung(k).is_none() {
                    p.hinweise.push(Hinweis::ZuSteil(nummer[i]));
                }
            }
        }
        let (z_lo, z_hi) = rand
            .iter()
            .filter(|k| endlich(k))
            .fold((h_lo, h_hi), |(lo, hi), k| {
                (lo.min(k.a[2]).min(k.b[2]), hi.max(k.a[2]).max(k.b[2]))
            });
        let spanne = z_hi - z_lo + 0.1;
        for i in 0..n {
            let j = (i + n - 1) % n;
            let (Some(v), Some(w)) = (geo[j], geo[i]) else {
                continue;
            };
            // nur zwischen zwei Böschungen (H-169c)
            let boeschung = v.steigung(&rand[j]).is_some() && w.steigung(&rand[i]).is_some();
            let kreuz = v.t[0] * w.t[1] - v.t[1] * w.t[0];
            let skalar = v.t[0] * w.t[0] + v.t[1] * w.t[1];
            let innen = std::f64::consts::PI - kreuz.atan2(skalar);
            if boeschung && innen < SPITZ - 1e-9 {
                // die Ecke ist der Anfang von Kante i im Umlauf gegen den
                // Uhrzeigersinn, übergeben im Uhrzeigersinn der Anfang der
                // Kante davor (Review #169 B5)
                let kante = if umgekehrt { nummer[j] } else { nummer[i] };
                p.hinweise.push(Hinweis::SpitzeEcke(kante));
            }
        }
        for vz in [-1.0, 1.0] {
            // Ebene je Kante: Gradient s·t + vz·α·o (vz −1 fällt nach außen)
            let ebenen: Vec<Option<[f64; 2]>> = (0..n)
                .map(|i| {
                    let g = geo[i]?;
                    let (s, alpha) = g.steigung(&rand[i])?;
                    Some([
                        s * g.t[0] + vz * alpha * g.o[0],
                        s * g.t[1] + vz * alpha * g.o[1],
                    ])
                })
                .collect();
            // Grat- oder Kehllinie am Anfang jeder Kante
            let strahl: Vec<[f64; 2]> = (0..n)
                .map(|i| {
                    let j = (i + n - 1) % n;
                    strahl_an(geo[j], geo[i], ebenen[j], ebenen[i])
                })
                .collect();
            for i in 0..n {
                let (Some(g), Some(grad)) = (geo[i], ebenen[i]) else {
                    continue;
                };
                let k = &rand[i];
                let ebene = [
                    k.a[2] - grad[0] * k.a[0] - grad[1] * k.a[1],
                    grad[0],
                    grad[1],
                ];
                let alpha = (grad[0] * g.o[0] + grad[1] * g.o[1]) * vz;
                let r_max = (spanne / alpha.max(1e-9) + 1.0).min(u.diagonale() + 1.0);
                let (r, offen) = reichweite(u, k, &g, &ebene, vz, r_max);
                if offen {
                    p.hinweise.push(Hinweis::OffenesEnde(nummer[i]));
                }
                if r <= 0.0 {
                    continue;
                }
                let (ra, rb) = (strahl[i], strahl[(i + 1) % n]);
                let (ring, eng) = bereich(k, &g, ra, rb, r);
                if eng && !p.hinweise.contains(&Hinweis::Einbuchtung(nummer[i])) {
                    p.hinweise.push(Hinweis::Einbuchtung(nummer[i]));
                }
                let b = Bereich {
                    ring,
                    ebene,
                    kante: nummer[i],
                };
                if vz < 0.0 {
                    p.auftrag.push(b);
                } else {
                    p.abtrag.push(b);
                }
            }
        }
        p
    }

    /// Höhe des Planungsgeländes an (x, y); `None`, wo das Urgelände keine
    /// Höhe hat (H-169d).
    pub fn hoehe(&self, u: &Abfrage, x: f64, y: f64) -> Option<f64> {
        let z = u.hoehe(x, y)?;
        if let Some(t) = self.teile.iter().rev().find(|t| innen(&t.ring, x, y)) {
            return Some(an(&t.ebene, x, y));
        }
        let mut lo = f64::NEG_INFINITY;
        let mut hi = f64::INFINITY;
        for b in self.auftrag.iter().filter(|b| innen(&b.ring, x, y)) {
            lo = lo.max(an(&b.ebene, x, y));
        }
        for b in self.abtrag.iter().filter(|b| innen(&b.ring, x, y)) {
            hi = hi.min(an(&b.ebene, x, y));
        }
        Some(z.max(lo).min(hi))
    }

    /// Abtrag und Auftrag gegen das Urgelände, exakt je Dreieck. Die
    /// Bereiche zählen nur außerhalb der Teilflächen; wo sich Bereiche
    /// derselben Art decken (Einbuchtung), zählt nur die sichtbare Böschung
    /// (beim Auftrag die höhere, beim Abtrag die tiefere Ebene), gleich
    /// viele Bereiche übereinander wie zwei ([`Planung::sichtbar`]). Die
    /// Teilflächen eines Bausteins decken sich nicht; das ist dem Aufrufer
    /// überlassen (H-169f).
    pub fn mengen(&self, u: &Abfrage) -> Planmenge {
        let mut m = Planmenge::default();
        for t in &self.teile {
            let e = u.erdmenge(&t.ring, t.ebene);
            m.flaeche += flaeche(&t.ring);
            m.abtrag += e.ueber;
            m.auftrag += e.unter;
            m.flaeche_abtrag += e.flaeche_ueber;
            m.flaeche_auftrag += e.flaeche_unter;
        }
        for (bereiche, auftrag) in [(&self.auftrag, true), (&self.abtrag, false)] {
            for (ring, b) in sichtbar(bereiche, auftrag) {
                self.zaehlen(u, &mut m, &ring, b, auftrag, 1.0);
            }
        }
        m
    }

    /// Die Böschung `b` über dem konvexen Polygon `ring` außerhalb der
    /// Teilflächen mit dem Vorzeichen `vz` zählen.
    fn zaehlen(
        &self,
        u: &Abfrage,
        m: &mut Planmenge,
        ring: &[[f64; 2]],
        b: &Bereich,
        auftrag: bool,
        vz: f64,
    ) {
        if ring.len() < 3 {
            return;
        }
        let schraeg = (1.0 + b.ebene[1].powi(2) + b.ebene[2].powi(2)).sqrt();
        let mut zu = |ring: &[[f64; 2]], vz: f64| {
            let e = u.erdmenge(ring, b.ebene);
            let (v, a) = if auftrag {
                (e.unter, e.flaeche_unter)
            } else {
                (e.ueber, e.flaeche_ueber)
            };
            if auftrag {
                m.auftrag += vz * v;
                m.flaeche_auftrag += vz * a;
            } else {
                m.abtrag += vz * v;
                m.flaeche_abtrag += vz * a;
            }
            m.boeschung += vz * a * schraeg;
        };
        zu(ring, vz);
        for t in &self.teile {
            let s = konvex_schnitt(&t.ring, ring);
            if s.len() >= 3 {
                zu(&s, -vz);
            }
        }
    }
}

/// Richtung `t` einer Kante, Normale `o` nach außen (rechts, der Rand läuft
/// gegen den Uhrzeigersinn) und Länge, waagerecht.
#[derive(Clone, Copy, Debug)]
struct Geo {
    t: [f64; 2],
    o: [f64; 2],
    l: f64,
}

impl Geo {
    fn neu(k: &Kante) -> Option<Geo> {
        let (dx, dy) = (k.b[0] - k.a[0], k.b[1] - k.a[1]);
        let l = dx.hypot(dy);
        (l > 1e-9 && endlich(k)).then(|| Geo {
            t: [dx / l, dy / l],
            o: [dy / l, -dx / l],
            l,
        })
    }

    /// Steigung entlang der Kante und Gefälle der Böschung quer zu ihr:
    /// zusammen 1/n. `None` ohne Böschung oder wenn die Kante steiler ist.
    fn steigung(&self, k: &Kante) -> Option<(f64, f64)> {
        let n = k.neigung?;
        if !(n.is_finite() && n > 0.0) {
            return None;
        }
        let s = (k.b[2] - k.a[2]) / self.l;
        let q = 1.0 / (n * n) - s * s;
        (q > 1e-12).then(|| (s, q.sqrt()))
    }
}

/// Sind alle Werte der Kante endlich (Review #169 B3)?
fn endlich(k: &Kante) -> bool {
    k.a.iter().chain(&k.b).all(|v| v.is_finite())
}

/// Der Rand gegen den Uhrzeigersinn und je Kante ihre Nummer im übergebenen
/// Rand; `true`, wenn er dafür umgedreht wurde.
fn gegen_uhrzeiger(rand: &[Kante]) -> (Vec<Kante>, Vec<usize>, bool) {
    let a2: f64 = rand
        .iter()
        .filter(|k| endlich(k))
        .map(|k| k.a[0] * k.b[1] - k.b[0] * k.a[1])
        .sum();
    if a2 >= 0.0 {
        return (rand.to_vec(), (0..rand.len()).collect(), false);
    }
    let n = rand.len();
    let umgekehrt = (0..n)
        .rev()
        .map(|i| Kante {
            a: rand[i].b,
            b: rand[i].a,
            neigung: rand[i].neigung,
        })
        .collect();
    (umgekehrt, (0..n).rev().collect(), true)
}

/// Grat- oder Kehllinie an der Ecke zwischen Kante `v` (davor) und `w`
/// (danach): die Schnittlinie ihrer Böschungsebenen, nach außen gerichtet,
/// Länge 1. Hat nur eine Kante eine Böschung, deren Normale.
fn strahl_an(
    v: Option<Geo>,
    w: Option<Geo>,
    gv: Option<[f64; 2]>,
    gw: Option<[f64; 2]>,
) -> [f64; 2] {
    let (Some(v), Some(w)) = (v, w) else {
        return w.or(v).map_or([0.0, 0.0], |g| g.o);
    };
    let aussen = [v.o[0] + w.o[0], v.o[1] + w.o[1]];
    let (Some(gv), Some(gw)) = (gv, gw) else {
        return if gw.is_some() { w.o } else { v.o };
    };
    let d = [gv[0] - gw[0], gv[1] - gw[1]];
    let l = d[0].hypot(d[1]);
    if l < 1e-12 || aussen[0].hypot(aussen[1]) < 1e-9 {
        return w.o;
    }
    let r = [-d[1] / l, d[0] / l];
    if r[0] * aussen[0] + r[1] * aussen[1] < 0.0 {
        [-r[0], -r[1]]
    } else {
        r
    }
}

/// Wie weit die Böschung der Kante `k` quer nach außen reicht, bis sie das
/// Urgelände trifft (m), höchstens `r_max`; geprobt an den Enden und
/// höchstens alle [`PROBE`] m (höchstens [`MAX_PROBEN`]), je Probe exakt
/// über das Profil. Gesucht wird stückweise nach außen bis zum ersten
/// Treffer, so hängt die Zeit an der Reichweite, nicht am Netz (Review #169
/// B2). Getroffen wird nur im Netz: Verlässt eine Probe es vorher, endet
/// sie am Rand und die Kante hat ein offenes Ende (B6). `true`, wenn eine
/// Probe das Urgelände nicht trifft.
fn reichweite(u: &Abfrage, k: &Kante, g: &Geo, ebene: &Bezug, vz: f64, r_max: f64) -> (f64, bool) {
    let schritte = ((g.l / PROBE).ceil() as usize).clamp(1, MAX_PROBEN);
    let stueck = (8.0 * u.zelle()).max(PROBE);
    let (mut r, mut offen) = (0.0f64, false);
    for j in 0..=schritte {
        let f = j as f64 / schritte as f64;
        let p = [
            k.a[0] + f * (k.b[0] - k.a[0]),
            k.a[1] + f * (k.b[1] - k.a[1]),
        ];
        let x = |w: f64| [p[0] + g.o[0] * w, p[1] + g.o[1] * w];
        // Abstand der Böschung über (Auftrag) bzw. unter (Abtrag) dem Gelände
        let d = |w: f64, z: f64| {
            let q = x(w);
            -vz * (an(ebene, q[0], q[1]) - z)
        };
        let mut vorher: Option<(f64, f64)> = None;
        let mut w0 = 0.0;
        let ende = loop {
            let w1 = (w0 + stueck).min(r_max);
            let (profil, raus) = u.profil_innen(x(w0), x(w1));
            // erster Punkt mit h ≤ 0: zwischen ihm und dem davor schneidet
            // die Böschung das Gelände
            let mut treffer = None;
            for q in &profil {
                let w = w0 + q[0];
                let h = d(w, q[1]);
                if h <= 0.0 {
                    treffer = Some(match vorher {
                        Some((v0, h0)) if h0 > 0.0 && v0 < w => v0 + (w - v0) * h0 / (h0 - h),
                        _ => w,
                    });
                    break;
                }
                vorher = Some((w, h));
            }
            if treffer.is_some() {
                break treffer;
            }
            if raus {
                offen = true;
                r = r.max(vorher.map_or(0.0, |v| v.0));
                break None;
            }
            if w1 >= r_max {
                offen |= vorher.is_some();
                r = r.max(if vorher.is_some() { r_max } else { 0.0 });
                break None;
            }
            w0 = w1;
        };
        if let Some(w) = ende {
            r = r.max(w);
        }
    }
    (r, offen)
}

/// Bereich der Kante `k` zwischen den Strahlen `ra` (am Anfang) und `rb`
/// (am Ende) bis zum Abstand `r` von der Kante. `true`, wenn sich die
/// Strahlen vorher kreuzen; dann endet der Bereich dort (Dreieck).
fn bereich(k: &Kante, g: &Geo, ra: [f64; 2], rb: [f64; 2], r: f64) -> (Vec<[f64; 2]>, bool) {
    let a = [k.a[0], k.a[1]];
    let b = [k.b[0], k.b[1]];
    let laenge = |s: [f64; 2]| {
        let c = s[0] * g.o[0] + s[1] * g.o[1];
        if c > 1.0 / STRAHL_MAX {
            r / c
        } else {
            r * STRAHL_MAX
        }
    };
    let (la, lb) = (laenge(ra), laenge(rb));
    // a + s·ra = b + v·rb
    let det = ra[0] * (-rb[1]) - ra[1] * (-rb[0]);
    if det.abs() > 1e-12 {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let s = (dx * (-rb[1]) - dy * (-rb[0])) / det;
        let v = (ra[0] * dy - ra[1] * dx) / det;
        if s > 0.0 && v > 0.0 && s < la && v < lb {
            let x = [a[0] + s * ra[0], a[1] + s * ra[1]];
            return (vec![a, b, x], true);
        }
    }
    let pa = [a[0] + la * ra[0], a[1] + la * ra[1]];
    let pb = [b[0] + lb * rb[0], b[1] + lb * rb[1]];
    (vec![a, b, pb, pa], false)
}

/// Die sichtbaren Teile der Bereiche einer Art als konvexe Stücke, je mit
/// ihrem Bereich (Review #169 B1): Wo sich Bereiche decken, zeigt sich beim
/// Auftrag die höhere, beim Abtrag die tiefere Ebene, bei gleicher Ebene
/// der erste Bereich. Je Bereich wird abgezogen, was die anderen verdecken,
/// gleich wie viele sich an einer Stelle decken.
pub(crate) fn sichtbar(bereiche: &[Bereich], auftrag: bool) -> Vec<(Vec<[f64; 2]>, &Bereich)> {
    let rahmen: Vec<[f64; 4]> = bereiche.iter().map(|b| rahmen(&b.ring)).collect();
    let mut out = Vec::new();
    for (i, bi) in bereiche.iter().enumerate() {
        let mut stuecke = vec![bi.ring.clone()];
        for (j, bj) in bereiche.iter().enumerate() {
            // Rechteck-Vortest (H-169e)
            let (ri, rj) = (rahmen[i], rahmen[j]);
            if i == j || ri[0] > rj[2] || rj[0] > ri[2] || ri[1] > rj[3] || rj[1] > ri[3] {
                continue;
            }
            let gleich = (0..3).all(|k| (bi.ebene[k] - bj.ebene[k]).abs() < 1e-12);
            let mut neu = Vec::new();
            for st in stuecke {
                let s = konvex_schnitt(&st, &bj.ring);
                if flaeche(&s) < 1e-12 {
                    neu.push(st);
                    continue;
                }
                neu.extend(ohne_konvex(&st, &bj.ring));
                // im Schnitt bleibt, wo i oben (Auftrag) bzw. unten liegt
                let teil = if gleich {
                    if i < j {
                        s
                    } else {
                        Vec::new()
                    }
                } else {
                    halbieren(&s, |q| {
                        let d = an(&bi.ebene, q[0], q[1]) - an(&bj.ebene, q[0], q[1]);
                        if auftrag {
                            d
                        } else {
                            -d
                        }
                    })
                };
                if flaeche(&teil) > 1e-12 {
                    neu.push(teil);
                }
            }
            stuecke = neu;
        }
        out.extend(stuecke.into_iter().map(|r| (r, bi)));
    }
    out
}

/// Rechteck um ein Polygon: x0, y0, x1, y1.
fn rahmen(ring: &[[f64; 2]]) -> [f64; 4] {
    ring.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |r, p| {
            [
                r[0].min(p[0]),
                r[1].min(p[1]),
                r[2].max(p[0]),
                r[3].max(p[1]),
            ]
        },
    )
}

/// Das konvexe `subjekt` ohne das konvexe `fenster`: konvexe Stücke, je
/// außerhalb einer Kante des Fensters und innerhalb der Kanten davor.
fn ohne_konvex(subjekt: &[[f64; 2]], fenster: &[[f64; 2]]) -> Vec<Vec<[f64; 2]>> {
    let n = fenster.len();
    if n < 3 {
        return vec![subjekt.to_vec()];
    }
    let a2: f64 = (0..n)
        .map(|i| fenster[i][0] * fenster[(i + 1) % n][1] - fenster[(i + 1) % n][0] * fenster[i][1])
        .sum();
    let mut rest = subjekt.to_vec();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = if a2 >= 0.0 {
            (fenster[i], fenster[(i + 1) % n])
        } else {
            (fenster[(i + 1) % n], fenster[i])
        };
        let links = |p: [f64; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        let aussen = halbieren(&rest, |p| -links(p));
        if flaeche(&aussen) > 1e-12 {
            out.push(aussen);
        }
        rest = halbieren(&rest, links);
        if rest.len() < 3 {
            break;
        }
    }
    out
}

/// Das Polygon `subjekt` (beliebig) im konvexen `fenster` (Sutherland-
/// Hodgman; ein nicht konvexes Subjekt bekommt Stege ohne Fläche).
fn konvex_schnitt(subjekt: &[[f64; 2]], fenster: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let n = fenster.len();
    if n < 3 || subjekt.len() < 3 {
        return Vec::new();
    }
    let a2: f64 = (0..n)
        .map(|i| fenster[i][0] * fenster[(i + 1) % n][1] - fenster[(i + 1) % n][0] * fenster[i][1])
        .sum();
    let mut v = subjekt.to_vec();
    for i in 0..n {
        let (a, b) = if a2 >= 0.0 {
            (fenster[i], fenster[(i + 1) % n])
        } else {
            (fenster[(i + 1) % n], fenster[i])
        };
        v = halbieren(&v, |p| {
            (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
        });
        if v.len() < 3 {
            return Vec::new();
        }
    }
    v
}

/// Höhe der Ebene an (x, y).
fn an(e: &Bezug, x: f64, y: f64) -> f64 {
    e[0] + e[1] * x + e[2] * y
}

/// Liegt (x, y) im Polygon (gerade-ungerade)?
fn innen(ring: &[[f64; 2]], x: f64, y: f64) -> bool {
    let mut drin = false;
    for i in 0..ring.len() {
        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
        if (p[1] > y) != (q[1] > y) && x < p[0] + (y - p[1]) / (q[1] - p[1]) * (q[0] - p[0]) {
            drin = !drin;
        }
    }
    drin
}

/// Betrag der Fläche eines Polygons.
fn flaeche(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| ring[i][0] * ring[(i + 1) % n][1] - ring[(i + 1) % n][0] * ring[i][1])
        .sum::<f64>()
        .abs()
        / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netz::{netz, Rand};

    /// Urgelände über [−20, 40]² aus einem Gitter zu 2 m mit Höhe `z(x, y)`.
    fn gelaende(z: impl Fn(f64, f64) -> f64) -> Abfrage {
        let mut p = Vec::new();
        for i in 0..=30 {
            for j in 0..=30 {
                let (x, y) = (-20.0 + 2.0 * i as f64, -20.0 + 2.0 * j as f64);
                p.push([x, y, z(x, y)]);
            }
        }
        Abfrage::neu(&netz(&p, &[], Rand::Huelle))
    }

    /// Ebener Baustein über dem Polygon `ring` mit der Ebene `e`, Böschung
    /// 1:`n` an jeder Kante.
    fn baustein(ring: &[[f64; 2]], e: Bezug, n: f64) -> Baustein {
        let k = ring.len();
        Baustein {
            teile: vec![Teil {
                ring: ring.to_vec(),
                ebene: e,
            }],
            rand: (0..k)
                .map(|i| {
                    let (a, b) = (ring[i], ring[(i + 1) % k]);
                    Kante {
                        a: [a[0], a[1], an(&e, a[0], a[1])],
                        b: [b[0], b[1], an(&e, b[0], b[1])],
                        neigung: Some(n),
                    }
                })
                .collect(),
        }
    }

    const QUADRAT: [[f64; 2]; 4] = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];

    /// Probe: ∫ (P − U)⁺ und ∫ (U − P)⁺ auf einem Raster zu `h` m über
    /// [−15, 25]², Mittelpunktregel.
    fn probe(p: &Planung, u: &Abfrage, h: f64) -> (f64, f64) {
        let n = (40.0 / h) as usize;
        let (mut auf, mut ab) = (0.0, 0.0);
        for i in 0..n {
            for j in 0..n {
                let (x, y) = (-15.0 + (i as f64 + 0.5) * h, -15.0 + (j as f64 + 0.5) * h);
                let d = p.hoehe(u, x, y).unwrap() - u.hoehe(x, y).unwrap();
                auf += d.max(0.0) * h * h;
                ab += (-d).max(0.0) * h * h;
            }
        }
        (auf, ab)
    }

    #[test]
    fn quadrat_auftrag_mit_gehrung() {
        // 1 m über ebenem Gelände, 1:1,5: Fläche 100 m³, je Kante ein
        // Prisma L·h²·n/2 = 7,5 m³ und an jeder Ecke die Gehrung
        // n²·h³/3 = 0,75 m³ (wie die Baugrubenecken in G2)
        let u = gelaende(|_, _| 10.0);
        let p = Planung::neu(&baustein(&QUADRAT, [11.0, 0.0, 0.0], 1.5), &u);
        assert!(p.hinweise.is_empty(), "{:?}", p.hinweise);
        assert_eq!((p.auftrag.len(), p.abtrag.len()), (4, 0));
        let m = p.mengen(&u);
        assert!((m.auftrag - 133.0).abs() < 1e-6, "{m:?}");
        assert_eq!(m.abtrag, 0.0);
        assert!((m.flaeche - 100.0).abs() < 1e-9);
        // waagerecht je Kante ein Trapez 10 · 1,5 + 1,5²
        assert!((m.flaeche_auftrag - 169.0).abs() < 1e-6, "{m:?}");
        let schraeg = 69.0 * (1.0f64 + 1.0 / 2.25).sqrt();
        assert!((m.boeschung - schraeg).abs() < 1e-6, "{m:?}");
        // am Rand die Höhe der Fläche, am Fuß das Gelände
        assert!((p.hoehe(&u, 10.0, 5.0).unwrap() - 11.0).abs() < 1e-9);
        assert!((p.hoehe(&u, 10.75, 5.0).unwrap() - 10.5).abs() < 1e-9);
        assert!((p.hoehe(&u, 11.6, 5.0).unwrap() - 10.0).abs() < 1e-9);
        let (auf, ab) = probe(&p, &u, 0.05);
        assert!((auf - 133.0).abs() < 0.5 && ab == 0.0, "{auf} {ab}");
    }

    #[test]
    fn quadrat_abtrag_ist_spiegelbild() {
        let u = gelaende(|_, _| 10.0);
        let p = Planung::neu(&baustein(&QUADRAT, [9.0, 0.0, 0.0], 1.5), &u);
        assert_eq!((p.auftrag.len(), p.abtrag.len()), (0, 4));
        let m = p.mengen(&u);
        assert!((m.abtrag - 133.0).abs() < 1e-6, "{m:?}");
        assert_eq!(m.auftrag, 0.0);
        // Umlaufsinn egal
        let mut cw = QUADRAT;
        cw.reverse();
        let q = Planung::neu(&baustein(&cw, [9.0, 0.0, 0.0], 1.5), &u).mengen(&u);
        assert!((q.abtrag - m.abtrag).abs() < 1e-9, "{q:?}");
    }

    #[test]
    fn am_hang_gegen_probe() {
        // geneigtes, gewelltes Gelände, eine geneigte L-Fläche mit 2 %:
        // teils Abtrag, teils Auftrag; exakt gegen ein feines Raster
        let u = gelaende(|x, y| 10.0 + 0.08 * x - 0.05 * y + 0.3 * (x / 4.0).sin());
        let l = [
            [0.0, 0.0],
            [12.0, 0.0],
            [12.0, 5.0],
            [5.0, 5.0],
            [5.0, 11.0],
            [0.0, 11.0],
        ];
        let p = Planung::neu(&baustein(&l, [10.4, 0.02, 0.0], 1.5), &u);
        let m = p.mengen(&u);
        assert!(m.auftrag > 5.0 && m.abtrag > 5.0, "{m:?}");
        let (auf, ab) = probe(&p, &u, 0.04);
        assert!(
            (auf - m.auftrag).abs() < 0.01 * m.auftrag + 0.3,
            "{auf} {m:?}"
        );
        assert!((ab - m.abtrag).abs() < 0.01 * m.abtrag + 0.3, "{ab} {m:?}");
        // stetig über die Grat- und Kehllinien: dicht daneben gleich hoch
        for b in p.auftrag.iter().chain(&p.abtrag) {
            let (a, x) = (b.ring[0], b.ring[b.ring.len() - 1]);
            for f in [0.25, 0.5, 0.75] {
                let q = [a[0] + f * (x[0] - a[0]), a[1] + f * (x[1] - a[1])];
                let (h1, h2) = (
                    p.hoehe(&u, q[0] + 1e-7, q[1] + 1e-7).unwrap(),
                    p.hoehe(&u, q[0] - 1e-7, q[1] - 1e-7).unwrap(),
                );
                assert!((h1 - h2).abs() < 1e-5, "{q:?}: {h1} {h2}");
            }
        }
    }

    #[test]
    fn enge_einbuchtung() {
        // U-Form mit 1 m breitem Schlitz, 2 m über dem Gelände: die
        // Kehllinien im Schlitz kreuzen sich vor dem Fuß
        let u = gelaende(|_, _| 10.0);
        let form = [
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [5.5, 10.0],
            [5.5, 4.0],
            [4.5, 4.0],
            [4.5, 10.0],
            [0.0, 10.0],
        ];
        let p = Planung::neu(&baustein(&form, [12.0, 0.0, 0.0], 1.5), &u);
        assert!(
            p.hinweise
                .iter()
                .any(|h| matches!(h, Hinweis::Einbuchtung(_))),
            "{:?}",
            p.hinweise
        );
        let m = p.mengen(&u);
        let (auf, _) = probe(&p, &u, 0.04);
        assert!(
            (auf - m.auftrag).abs() < 0.01 * m.auftrag + 0.3,
            "{auf} {m:?}"
        );
    }

    #[test]
    fn spitze_ecke() {
        let u = gelaende(|_, _| 10.0);
        let keil = [[0.0, 0.0], [10.0, 0.0], [0.0, 4.0]];
        let p = Planung::neu(&baustein(&keil, [11.0, 0.0, 0.0], 1.5), &u);
        // an (10, 0) rund 21,8°, die anderen Ecken 90° und 68,2°
        assert_eq!(p.hinweise, vec![Hinweis::SpitzeEcke(1)]);
        let (auf, _) = probe(&p, &u, 0.04);
        let m = p.mengen(&u);
        assert!(
            (auf - m.auftrag).abs() < 0.01 * m.auftrag + 0.3,
            "{auf} {m:?}"
        );
    }

    #[test]
    fn auf_hoehe_des_gelaendes_nichts() {
        // die Fläche liegt genau auf dem Gelände: weder Menge noch Fläche
        let u = gelaende(|_, _| 10.0);
        let p = Planung::neu(&baustein(&QUADRAT, [10.0, 0.0, 0.0], 1.5), &u);
        let m = p.mengen(&u);
        assert!(p.auftrag.is_empty() && p.abtrag.is_empty(), "{p:?}");
        assert_eq!((m.auftrag, m.abtrag), (0.0, 0.0));
        assert_eq!((m.flaeche_auftrag, m.flaeche_abtrag), (0.0, 0.0));
        assert!((m.flaeche - 100.0).abs() < 1e-9);
    }

    #[test]
    fn zu_steil_und_senkrecht() {
        let u = gelaende(|_, _| 10.0);
        // eine Kante steigt 1:1, steiler als 1:1,5
        let mut b = baustein(&QUADRAT, [11.0, 0.0, 0.0], 1.5);
        b.rand[0].b[2] = 21.0;
        let p = Planung::neu(&b, &u);
        assert!(
            p.hinweise.contains(&Hinweis::ZuSteil(0)),
            "{:?}",
            p.hinweise
        );
        assert!(p.auftrag.iter().all(|b| b.kante != 0));
        // senkrechter Rand: nur die Fläche
        let mut b = baustein(&QUADRAT, [11.0, 0.0, 0.0], 1.5);
        for k in &mut b.rand {
            k.neigung = None;
        }
        let p = Planung::neu(&b, &u);
        assert!(p.auftrag.is_empty() && p.hinweise.is_empty());
        assert!((p.mengen(&u).auftrag - 100.0).abs() < 1e-6);
    }

    /// Kamm mit fünf Zähnen, 2 m Auftrag: in den Lücken decken sich drei
    /// Böschungen (Review #169 B1).
    #[test]
    fn kamm_dreifach_ueberdeckt() {
        let u = gelaende(|_, _| 10.0);
        // die Zähne laufen nach oben, der Rand gegen den Uhrzeigersinn
        let mut kamm = vec![[0.0, 0.0], [18.0, 0.0]];
        for z in (0..5).rev() {
            let x = 4.0 * z as f64;
            kamm.extend([[x + 2.0, 10.0], [x, 10.0]]);
            if z > 0 {
                kamm.extend([[x, 3.0], [x - 2.0, 3.0]]);
            }
        }
        let p = Planung::neu(&baustein(&kamm, [12.0, 0.0, 0.0], 1.5), &u);
        let m = p.mengen(&u);
        let (auf, ab) = probe(&p, &u, 0.04);
        assert!(
            ab == 0.0 && (auf - m.auftrag).abs() < 0.005 * auf,
            "{auf} {m:?}"
        );
        // höchstens ein Quader 18 × 10 × 2 plus Rand
        assert!(m.auftrag < 360.0 + 60.0 * 2.0 * 3.0, "{m:?}");
    }

    /// Ein doppelter Eckpunkt ändert nichts (B4); im Uhrzeigersinn nennt
    /// die spitze Ecke dieselbe Kante (B5).
    #[test]
    fn doppelpunkt_und_uhrzeigersinn() {
        let u = gelaende(|_, _| 10.0);
        let doppelt = [
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
        ];
        let p = Planung::neu(&baustein(&doppelt, [11.0, 0.0, 0.0], 1.5), &u);
        assert!(
            (p.mengen(&u).auftrag - 133.0).abs() < 1e-6,
            "{:?}",
            p.mengen(&u)
        );
        let keil = [[0.0, 4.0], [10.0, 0.0], [0.0, 0.0]];
        let p = Planung::neu(&baustein(&keil, [11.0, 0.0, 0.0], 1.5), &u);
        // die Ecke (10, 0) ist der Anfang von Kante 1
        assert_eq!(p.hinweise, vec![Hinweis::SpitzeEcke(1)]);
    }

    /// Am Rand des Netzes endet die Böschung dort, mit Hinweis (B6).
    #[test]
    fn offenes_ende_am_netzrand() {
        let u = gelaende(|_, _| 10.0);
        let q = [[34.0, 0.0], [39.0, 0.0], [39.0, 10.0], [34.0, 10.0]];
        let p = Planung::neu(&baustein(&q, [13.0, 0.0, 0.0], 1.5), &u);
        assert!(
            p.hinweise.contains(&Hinweis::OffenesEnde(1)),
            "{:?}",
            p.hinweise
        );
        let osten = p.auftrag.iter().find(|b| b.kante == 1).unwrap();
        assert!(osten.ring.iter().all(|q| q[0] <= 40.0 + 1e-6), "{osten:?}");
        // auf den anderen Seiten trifft sie das Gelände
        assert!(!p.hinweise.contains(&Hinweis::OffenesEnde(3)));
    }

    /// Nicht endliche Werte und Neigungen, die keine sind: Hinweis statt
    /// Hängen (B3, H-169b); am leeren Netz keine Höhe (H-169d).
    #[test]
    fn ungueltige_kanten() {
        let u = gelaende(|_, _| 10.0);
        for weit in [1e7, 1e9, f64::INFINITY, f64::NAN] {
            let mut b = baustein(&QUADRAT, [11.0, 0.0, 0.0], 1.5);
            b.rand[1].b[0] = weit;
            b.rand[2].a[0] = weit;
            let t = std::time::Instant::now();
            let p = Planung::neu(&b, &u);
            let _ = p.mengen(&u);
            assert!(t.elapsed().as_secs_f64() < 2.0, "{weit}: {:?}", t.elapsed());
            if !weit.is_finite() {
                assert!(
                    p.hinweise.contains(&Hinweis::Ungueltig(1)),
                    "{:?}",
                    p.hinweise
                );
            }
        }
        for n in [0.0, -1.5, f64::NAN, f64::INFINITY] {
            let mut b = baustein(&QUADRAT, [11.0, 0.0, 0.0], 1.5);
            b.rand[2].neigung = Some(n);
            let p = Planung::neu(&b, &u);
            assert!(
                p.hinweise.contains(&Hinweis::Ungueltig(2)),
                "{n}: {:?}",
                p.hinweise
            );
            assert!(p.auftrag.iter().all(|b| b.kante != 2));
        }
        let leer = Abfrage::neu(&netz(&[], &[], Rand::Huelle));
        let p = Planung::neu(&baustein(&QUADRAT, [11.0, 0.0, 0.0], 1.5), &leer);
        assert_eq!(p.hoehe(&leer, 5.0, 5.0), None);
    }

    /// Eine ferne Spitze im Netz macht die Reichweite nicht langsamer, und
    /// die Zeit hängt nicht an der Größe des Netzes (B2). Messung, kein
    /// Prüfstein: `cargo test --release -- --ignored reichweite_tempo`.
    #[test]
    #[ignore]
    fn reichweite_tempo() {
        for (n, spitze) in [(200usize, false), (200, true), (800, false), (800, true)] {
            let mut p = Vec::new();
            for i in 0..=n {
                for j in 0..=n {
                    let (x, y) = (i as f64 * 0.25 - 50.0, j as f64 * 0.25 - 50.0);
                    let z = if spitze && i == n && j == n {
                        500.0
                    } else {
                        10.0
                    };
                    p.push([x, y, z]);
                }
            }
            let u = Abfrage::neu(&netz(&p, &[], Rand::Huelle));
            let mut ring = Vec::new();
            for k in 0..400 {
                let w = k as f64 / 400.0 * std::f64::consts::TAU;
                ring.push([10.0 * w.cos(), 10.0 * w.sin()]);
            }
            let t = std::time::Instant::now();
            let pl = Planung::neu(&baustein(&ring, [11.0, 0.0, 0.0], 1.5), &u);
            println!(
                "{} Dreiecke, Spitze {spitze}: {:.1} ms, {} Bereiche",
                2 * n * n,
                t.elapsed().as_secs_f64() * 1e3,
                pl.auftrag.len()
            );
        }
    }
}

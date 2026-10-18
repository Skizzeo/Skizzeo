//! Erdbau eines Bausteins nach der VOB-Prüfung G3 (vob/pruefung-gelaende-g3.md):
//! Abtrag und Auftrag zwischen N (Urgelände ohne Oberboden) und Q (Planum
//! unter der Andeckung), der Oberboden auf dem Baustein, das Andecken und
//! was in der Baugrube anders zählt (B1 bis B5).
//!
//! Gerechnet wird je Stück des Bausteins (Teilfläche oder Böschungsbereich,
//! mit Vorzeichen für Überdeckungen wie in [`Planung::mengen`]) und je
//! Dreieck des Urgeländes, exakt: Im Dreieck sind U, P und Q Ebenen. Ein
//! Böschungsbereich zählt nur, wo die Böschung wirklich entsteht (Maske:
//! Auftrag über, Abtrag unter dem Urgelände). Zonen (Haus, Baufeld,
//! Baugrube) werden in Dreiecke zerlegt und je Dreieck geschnitten.

use super::{an, konvex_schnitt, Bereich, Planung};
use crate::abfrage::Abfrage;
use crate::volumen::{ebene, flaeche_schwerpunkt, halbieren, schneiden, ueberlappt, Bezug};

/// Dreieck im Grundriss.
type Dreieck = [[f64; 2]; 3];

/// Wie unter einem Baustein gezählt wird (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aufbau {
    /// Oberboden o: N = U − o.
    pub oberboden: f64,
    /// Andeckdicke d auf Grün, senkrecht zur Fläche: Q = P − d/cos α (B5).
    pub andecken: f64,
    /// Befestigte Fläche: Q = P, kein Andecken (B2, Aufbau ab G4).
    pub befestigt: bool,
}

/// Eine Baugrube: Sohle (Platte und Arbeitsraum) und Platte, beide als
/// Polygon, und die Höhe B ihrer Aushubfläche (UK Platte).
#[derive(Clone, Debug, PartialEq)]
pub struct Grube {
    pub sohle: Vec<[f64; 2]>,
    pub platte: Vec<[f64; 2]>,
    pub b: f64,
}

/// Flächen, in denen anders gezählt wird (Polygone im Netz, m).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Zonen {
    /// Häuser (Platten): darunter zählt kein Baustein.
    pub haus: Vec<Vec<[f64; 2]>>,
    /// Baufelder um die Häuser (samt Haus): Dort ist der Oberboden schon
    /// abgetragen (311).
    pub baufeld: Vec<Vec<[f64; 2]>>,
    /// Höfe in den Baufeldern, die ihr Rand umschließt, aber nicht deckt:
    /// Dort liegt der Oberboden noch.
    pub baufeld_hoefe: Vec<Vec<[f64; 2]>>,
    /// Baugruben, in denen bis N schon ausgehoben ist (B3).
    pub gruben: Vec<Grube>,
}

/// Gefälle, ab dem eine Grünfläche als Böschung angedeckt wird (1:4, B5).
pub const STEIL_ANDECKEN: f64 = 0.25;

/// Erdbau eines Bausteins (m³, m²), außerhalb der Häuser.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Erdbau {
    /// ∫ (N − Q)⁺, in der Baugrube ab dem Aushub ∫ (min(N, B) − Q)⁺ (B3).
    pub abtrag: f64,
    /// ∫ (Q − N)⁺, auch über der Baugrube (B3).
    pub auftrag: f64,
    /// Waagerechte Fläche, auf der das Gelände geändert wird.
    pub flaeche: f64,
    /// Davon im Baufeld (Oberboden dort schon abgetragen).
    pub im_baufeld: f64,
    /// Andecken auf Grün, schräg gemessen: bis 1:4 und steiler.
    pub andecken: f64,
    pub andecken_steil: f64,
    /// In der Baugrube: um so viel mindert sich die Verfüllung 311,
    /// ∫ (N − max(Q, B))⁺, gesamt und je Baugrube der Zonen.
    pub weniger_verfuellen: f64,
    pub je_grube: Vec<f64>,
    /// In der Baugrube unter UK Platte, wo ausgehoben ist: ∫ (B − Q)⁺ (in
    /// `abtrag`), Hinweis.
    pub unter_sohle: f64,
}

impl Erdbau {
    /// Fläche des Oberbodenabtrags auf dem Baustein außerhalb der
    /// Baufelder (B1).
    pub fn oberboden_flaeche(&self) -> f64 {
        (self.flaeche - self.im_baufeld).max(0.0)
    }
}

/// Ein Stück des Bausteins mit Vorzeichen: Polygon, Ebene P, Maske (Ebene
/// und Seite: nur wo `seite · (P − U) > 0`).
struct Stueck {
    ring: Vec<[f64; 2]>,
    ebene: Bezug,
    maske: Option<f64>,
    vz: f64,
}

/// Summen eines Stücks über einem Fenster.
#[derive(Clone, Copy, Debug, Default)]
struct Summe {
    flaeche: f64,
    schraeg: f64,
    /// ∫ (Q − N)⁺ und ∫ (N − Q)⁺.
    plus: f64,
    minus: f64,
    /// Gegen die Höhe B des Fensters: ∫ (B − Q)⁺, wo N über B liegt (unter
    /// der Sohle), ∫ (min(N, B) − Q)⁺ (Abtrag nach dem Aushub) und
    /// ∫ (N − max(Q, B))⁺ (weniger Verfüllung).
    unter_b: f64,
    ab_grube: f64,
    ueber_b: f64,
}

impl Summe {
    fn dazu(&mut self, o: Summe, vz: f64) {
        self.flaeche += vz * o.flaeche;
        self.schraeg += vz * o.schraeg;
        self.plus += vz * o.plus;
        self.minus += vz * o.minus;
        self.unter_b += vz * o.unter_b;
        self.ab_grube += vz * o.ab_grube;
        self.ueber_b += vz * o.ueber_b;
    }
}

/// ∫ f⁺ und ∫ (−f)⁺ der linearen Funktion `f` über dem Polygon `v`.
fn plus_minus(v: &[[f64; 2]], f: impl Fn([f64; 2]) -> f64) -> (f64, f64) {
    let teil = |vz: f64| {
        let h = halbieren(v, |q| vz * f(q));
        flaeche_schwerpunkt(&h).map_or(0.0, |(a, c)| a * (vz * f(c)).max(0.0))
    };
    (teil(1.0), teil(-1.0))
}

/// Das einfache Polygon `ring` in Dreiecke gegen den Uhrzeigersinn
/// (Ohren abschneiden; O(n²), für Haus- und Baufeldumrisse).
pub fn ohren(ring: &[[f64; 2]]) -> Vec<[[f64; 2]; 3]> {
    let mut v: Vec<[f64; 2]> = ring.to_vec();
    let a2: f64 = (0..v.len())
        .map(|i| {
            let (p, q) = (v[i], v[(i + 1) % v.len()]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum();
    if a2 < 0.0 {
        v.reverse();
    }
    let kreuz = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let mut out = Vec::new();
    while v.len() > 3 {
        let n = v.len();
        let ohr = (0..n).find(|&i| {
            let (a, b, c) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
            kreuz(a, b, c) > 1e-12
                && (0..n).all(|j| {
                    let p = v[j];
                    j == i
                        || j == (i + n - 1) % n
                        || j == (i + 1) % n
                        || !(kreuz(a, b, p) >= 0.0
                            && kreuz(b, c, p) >= 0.0
                            && kreuz(c, a, p) >= 0.0)
                })
        });
        // ohne Ohr (entartet): der Rest als Fächer
        let Some(i) = ohr else {
            for k in 1..v.len() - 1 {
                out.push([v[0], v[k], v[k + 1]]);
            }
            return out;
        };
        out.push([v[(i + n - 1) % n], v[i], v[(i + 1) % n]]);
        v.remove(i);
    }
    if v.len() == 3 {
        out.push([v[0], v[1], v[2]]);
    }
    out
}

/// Rechteck um die Punkte `p`: [x0, y0, x1, y1]; ohne Punkte leer.
fn huelle(p: &[[f64; 2]]) -> [f64; 4] {
    p.iter()
        .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |r, q| {
            [
                r[0].min(q[0]),
                r[1].min(q[1]),
                r[2].max(q[0]),
                r[3].max(q[1]),
            ]
        })
}

/// Treffen sich die Rechtecke `a` und `b` (Rand eingeschlossen)?
fn treffen(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// Ein einfaches Polygon, einmal in Dreiecke zerlegt, mit dem Rechteck um
/// das Ganze und um jedes Dreieck: Paare, deren Rechtecke sich nicht
/// treffen, schneidet [`Zerlegt::ueberlappung`] nicht (Review #173 B1).
#[derive(Clone, Debug)]
pub struct Zerlegt {
    huelle: [f64; 4],
    dreiecke: Vec<(Dreieck, [f64; 4])>,
}

impl Zerlegt {
    pub fn neu(ring: &[[f64; 2]]) -> Zerlegt {
        Zerlegt {
            huelle: huelle(ring),
            dreiecke: ohren(ring).into_iter().map(|d| (d, huelle(&d))).collect(),
        }
    }

    /// Gemeinsame Fläche mit `b`.
    pub fn ueberlappung(&self, b: &Zerlegt) -> f64 {
        if !treffen(&self.huelle, &b.huelle) {
            return 0.0;
        }
        let mut s = 0.0;
        for (x, hx) in &self.dreiecke {
            if !treffen(hx, &b.huelle) {
                continue;
            }
            for (y, hy) in &b.dreiecke {
                if treffen(hx, hy) {
                    s += super::flaeche(&konvex_schnitt(x, y));
                }
            }
        }
        s
    }
}

/// Gemeinsame Fläche zweier einfacher Polygone (je in Dreiecke zerlegt).
pub fn ueberlappung(a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
    Zerlegt::neu(a).ueberlappung(&Zerlegt::neu(b))
}

impl Planung {
    /// Die Stücke mit Vorzeichen, wie [`Planung::mengen`] sie zählt.
    /// Rechteck um den Baustein samt Böschungen im Netz (x0, y0, x1, y1):
    /// Zonen außerhalb ändern [`Planung::erdbau`] nicht.
    pub fn huelle(&self) -> [f64; 4] {
        let ringe = self.teile.iter().map(|t| &t.ring);
        let ringe = ringe.chain(self.auftrag.iter().chain(&self.abtrag).map(|b| &b.ring));
        ringe.fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |r, q| {
            let h = huelle(q);
            [
                r[0].min(h[0]),
                r[1].min(h[1]),
                r[2].max(h[2]),
                r[3].max(h[3]),
            ]
        })
    }

    fn stuecke(&self) -> Vec<Stueck> {
        let mut out: Vec<Stueck> = self
            .teile
            .iter()
            .map(|t| Stueck {
                ring: t.ring.clone(),
                ebene: t.ebene,
                maske: None,
                vz: 1.0,
            })
            .collect();
        let mut zaehlen = |ring: Vec<[f64; 2]>, b: &Bereich, seite: f64| {
            if ring.len() < 3 {
                return;
            }
            for t in &self.teile {
                let s = konvex_schnitt(&t.ring, &ring);
                if s.len() >= 3 {
                    out.push(Stueck {
                        ring: s,
                        ebene: b.ebene,
                        maske: Some(seite),
                        vz: -1.0,
                    });
                }
            }
            out.push(Stueck {
                ring,
                ebene: b.ebene,
                maske: Some(seite),
                vz: 1.0,
            });
        };
        // je Stelle nur die sichtbare Böschung, gleich wie viele sich decken
        for (bereiche, auftrag) in [(&self.auftrag, true), (&self.abtrag, false)] {
            let seite = if auftrag { 1.0 } else { -1.0 };
            for (ring, b) in super::sichtbar(bereiche, auftrag) {
                zaehlen(ring, b, seite);
            }
        }
        out
    }

    /// Erdbau des Bausteins mit dem Aufbau `a` gegen das Urgelände `u`, in
    /// den Zonen `z` (VOB-Prüfung G3, B1 bis B5).
    pub fn erdbau(&self, u: &Abfrage, a: &Aufbau, z: &Zonen) -> Erdbau {
        let haus: Vec<[[f64; 2]; 3]> = z.haus.iter().flat_map(|r| ohren(r)).collect();
        let baufeld: Vec<[[f64; 2]; 3]> = z.baufeld.iter().flat_map(|r| ohren(r)).collect();
        let hoefe: Vec<[[f64; 2]; 3]> = z.baufeld_hoefe.iter().flat_map(|r| ohren(r)).collect();
        let gruben: Vec<(Vec<Dreieck>, Vec<Dreieck>, f64)> = z
            .gruben
            .iter()
            .map(|g| (ohren(&g.sohle), ohren(&g.platte), g.b))
            .collect();
        let mut alle = Summe::default();
        let mut im_haus = Summe::default();
        let mut im_feld = Summe::default();
        let mut im_ring = Summe::default();
        let mut je_grube = vec![0.0; gruben.len()];
        let stuecke = self.stuecke();
        for s in &stuecke {
            let h = huelle(&s.ring);
            // Fenster, die das Stück nicht treffen, zählen nichts
            let sum = |fenster: Option<[[f64; 2]; 3]>, b: f64| match fenster {
                Some(t) if !treffen(&h, &huelle(&t)) => Summe::default(),
                _ => summe(u, a, s, fenster, b),
            };
            alle.dazu(sum(None, f64::NAN), s.vz);
            for t in &haus {
                im_haus.dazu(sum(Some(*t), f64::NAN), s.vz);
            }
            for t in &baufeld {
                im_feld.dazu(sum(Some(*t), f64::NAN), s.vz);
            }
            for t in &hoefe {
                im_feld.dazu(sum(Some(*t), f64::NAN), -s.vz);
            }
            for ((sohle, platte, b), je) in gruben.iter().zip(&mut je_grube) {
                let mut g = Summe::default();
                for t in sohle {
                    g.dazu(sum(Some(*t), *b), 1.0);
                }
                for t in platte {
                    g.dazu(sum(Some(*t), *b), -1.0);
                }
                *je += s.vz * g.ueber_b;
                im_ring.dazu(g, s.vz);
            }
        }
        let mut aussen = alle;
        aussen.dazu(im_haus, -1.0);
        let mut feld = im_feld;
        feld.dazu(im_haus, -1.0);
        let (andecken, andecken_steil) = if a.befestigt {
            (0.0, 0.0)
        } else {
            andecken(u, &stuecke, &haus)
        };
        Erdbau {
            abtrag: (aussen.minus - im_ring.minus + im_ring.ab_grube).max(0.0),
            auftrag: aussen.plus.max(0.0),
            flaeche: aussen.flaeche.max(0.0),
            im_baufeld: feld.flaeche.max(0.0),
            andecken,
            andecken_steil,
            weniger_verfuellen: im_ring.ueber_b.max(0.0),
            je_grube: je_grube.into_iter().map(|v| v.max(0.0)).collect(),
            unter_sohle: im_ring.unter_b.max(0.0),
        }
    }
}

/// Schräge Fläche der Grünflächen `stuecke` außerhalb der Häuser, bis 1:4
/// und steiler (je Stück nach dem Gefälle seiner Ebene).
fn andecken(u: &Abfrage, stuecke: &[Stueck], haus: &[[[f64; 2]; 3]]) -> (f64, f64) {
    let a = Aufbau {
        oberboden: 0.0,
        andecken: 0.0,
        befestigt: true,
    };
    let (mut eben, mut steil) = (0.0, 0.0);
    for s in stuecke {
        let h = huelle(&s.ring);
        let mut x = summe(u, &a, s, None, f64::NAN).schraeg;
        for t in haus.iter().filter(|t| treffen(&h, &huelle(*t))) {
            x -= summe(u, &a, s, Some(*t), f64::NAN).schraeg;
        }
        if s.ebene[1].hypot(s.ebene[2]) > STEIL_ANDECKEN {
            steil += s.vz * x;
        } else {
            eben += s.vz * x;
        }
    }
    (eben.max(0.0), steil.max(0.0))
}

/// Das Stück `s` im Fenster (`None`: überall) je Dreieck des Urgeländes;
/// `b` die Höhe der Aushubfläche im Fenster (NaN: keine).
fn summe(u: &Abfrage, a: &Aufbau, s: &Stueck, fenster: Option<[[f64; 2]; 3]>, b: f64) -> Summe {
    let mut out = Summe::default();
    let ring = match fenster {
        Some(t) => konvex_schnitt(&s.ring, &t),
        None => s.ring.clone(),
    };
    if ring.len() < 3 {
        return out;
    }
    let g2 = s.ebene[1] * s.ebene[1] + s.ebene[2] * s.ebene[2];
    let sec = (1.0 + g2).sqrt();
    // Q = P − d/cos α auf Grün, P auf befestigten Flächen; N = U − o
    let d_q = if a.befestigt { 0.0 } else { a.andecken * sec };
    for p in u.dreiecke_unter(&ring) {
        if !ueberlappt(&p, &ring) {
            continue;
        }
        let mut teil = schneiden(
            &ring,
            [[p[0][0], p[0][1]], [p[1][0], p[1][1]], [p[2][0], p[2][1]]],
        );
        if let Some(seite) = s.maske {
            teil = halbieren(&teil, |q| seite * (an(&s.ebene, q[0], q[1]) - ebene(&p, q)));
        }
        let Some((fl, _)) = flaeche_schwerpunkt(&teil) else {
            continue;
        };
        out.flaeche += fl;
        out.schraeg += fl * sec;
        let q = |x: [f64; 2]| an(&s.ebene, x[0], x[1]) - d_q;
        let n = |x: [f64; 2]| ebene(&p, x) - a.oberboden;
        let (plus, minus) = plus_minus(&teil, |x| q(x) - n(x));
        out.plus += plus;
        out.minus += minus;
        if b.is_finite() {
            // Wo N über B liegt, ist bis B ausgehoben, sonst nichts: der
            // Abtrag zählt ab min(N, B), je Teil linear
            let ueber_b = halbieren(&teil, |x| n(x) - b);
            let unter_b = halbieren(&teil, |x| b - n(x));
            let unter = plus_minus(&ueber_b, |x| b - q(x)).0;
            out.unter_b += unter;
            out.ab_grube += unter + plus_minus(&unter_b, |x| n(x) - q(x)).0;
            // N − max(Q, B), wo N über B liegt: geteilt, wo Q über bzw.
            // unter B liegt, je linear
            let q_oben = halbieren(&ueber_b, |x| q(x) - b);
            let q_unten = halbieren(&ueber_b, |x| b - q(x));
            out.ueber_b += plus_minus(&q_oben, |x| n(x) - q(x)).0;
            out.ueber_b += plus_minus(&q_unten, |x| n(x) - b).0;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{Baustein, Kante, Teil};
    use super::*;
    use crate::netz::{netz, Rand};

    /// Ebenes Urgelände in 10 m über [−20, 40]².
    fn eben() -> Abfrage {
        let mut p = Vec::new();
        for i in 0..=30 {
            for j in 0..=30 {
                p.push([-20.0 + 2.0 * i as f64, -20.0 + 2.0 * j as f64, 10.0]);
            }
        }
        Abfrage::neu(&netz(&p, &[], Rand::Huelle))
    }

    fn rechteck(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    /// Quadrat 10 × 10 m in Höhe `z`, Böschung 1:1,5 ringsum.
    fn quadrat(u: &Abfrage, z: f64) -> Planung {
        let ring = rechteck(0.0, 0.0, 10.0, 10.0);
        let b = Baustein {
            teile: vec![Teil {
                ring: ring.clone(),
                ebene: [z, 0.0, 0.0],
            }],
            rand: (0..4)
                .map(|i| {
                    let (a, b) = (ring[i], ring[(i + 1) % 4]);
                    Kante {
                        a: [a[0], a[1], z],
                        b: [b[0], b[1], z],
                        neigung: Some(1.5),
                    }
                })
                .collect(),
        };
        Planung::neu(&b, u)
    }

    const GRUEN: Aufbau = Aufbau {
        oberboden: 0.3,
        andecken: 0.2,
        befestigt: false,
    };

    #[test]
    fn auftrag_zwischen_n_und_q() {
        // 1 m Auftrag auf ebenem Gelände: in der Fläche Q − N = 10,8 − 9,7;
        // auf der Böschung P − U plus 0,30 − 0,20/cos α
        let u = eben();
        let e = quadrat(&u, 11.0).erdbau(&u, &GRUEN, &Zonen::default());
        let sec = (1.0f64 + 1.0 / 2.25).sqrt();
        let c = 0.3 - 0.2 * sec;
        let soll = 100.0 * 1.1 + 33.0 + 69.0 * c;
        assert!((e.auftrag - soll).abs() < 1e-6, "{e:?} {soll}");
        assert_eq!(e.abtrag, 0.0);
        assert!((e.flaeche - 169.0).abs() < 1e-6, "{e:?}");
        assert!((e.andecken - 100.0).abs() < 1e-6, "{e:?}");
        assert!((e.andecken_steil - 69.0 * sec).abs() < 1e-6, "{e:?}");
        assert_eq!((e.im_baufeld, e.weniger_verfuellen), (0.0, 0.0));
        // befestigt: Q = P, kein Andecken (B2)
        let b = Aufbau {
            befestigt: true,
            ..GRUEN
        };
        let e = quadrat(&u, 11.0).erdbau(&u, &b, &Zonen::default());
        assert!(
            (e.auftrag - (130.0 + 33.0 + 69.0 * 0.3)).abs() < 1e-6,
            "{e:?}"
        );
        assert_eq!((e.andecken, e.andecken_steil), (0.0, 0.0));
    }

    #[test]
    fn haus_und_baufeld() {
        let u = eben();
        let z = Zonen {
            haus: vec![rechteck(2.0, 2.0, 4.0, 4.0)],
            // Baufeld 1 m um das Haus, dazu bis über den Rand des Bausteins
            baufeld: vec![vec![[1.0, 1.0], [12.0, 1.0], [12.0, 5.0], [1.0, 5.0]]],
            gruben: Vec::new(),
            ..Zonen::default()
        };
        let ohne = quadrat(&u, 11.0).erdbau(&u, &GRUEN, &Zonen::default());
        let e = quadrat(&u, 11.0).erdbau(&u, &GRUEN, &z);
        // unter dem Haus 4 m² × 1,10 m weniger
        assert!((ohne.auftrag - e.auftrag - 4.4).abs() < 1e-6, "{e:?}");
        assert!((ohne.flaeche - e.flaeche - 4.0).abs() < 1e-6);
        assert!((ohne.andecken - e.andecken - 4.0).abs() < 1e-6);
        // im Baufeld: 9 × 4 in der Fläche, 1,5 × 4 auf der Böschung, ohne Haus
        assert!((e.im_baufeld - (36.0 + 6.0 - 4.0)).abs() < 1e-6, "{e:?}");
        assert!((e.oberboden_flaeche() - (169.0 - 4.0 - 38.0)).abs() < 1e-6);
        // ein Hof 2 × 2 m im Baufeld, den sein Rand nicht deckt: dort liegt
        // der Oberboden noch (B-288-geo-a)
        let mit_hof = Zonen {
            baufeld_hoefe: vec![rechteck(7.0, 2.0, 9.0, 4.0)],
            ..z
        };
        let h = quadrat(&u, 11.0).erdbau(&u, &GRUEN, &mit_hof);
        assert!((e.im_baufeld - h.im_baufeld - 4.0).abs() < 1e-6, "{h:?}");
        assert_eq!(
            (h.abtrag, h.auftrag, h.flaeche),
            (e.abtrag, e.auftrag, e.flaeche)
        );
    }

    #[test]
    fn in_der_baugrube() {
        // Baugrube mit Sohle UK Platte 8,00 unter N 9,70; der Baustein liegt
        // im Arbeitsraum (Sohle 0 … 10, Platte 2 … 8)
        let u = eben();
        let z = |b: f64| Zonen {
            haus: vec![rechteck(2.0, 2.0, 8.0, 8.0)],
            baufeld: Vec::new(),
            gruben: vec![Grube {
                sohle: rechteck(0.0, 0.0, 10.0, 10.0),
                platte: rechteck(2.0, 2.0, 8.0, 8.0),
                b,
            }],
            ..Zonen::default()
        };
        let ring = 100.0 - 36.0;
        // Abtrag 1 m: Q = 8,80 über B, bis N schon ausgehoben
        let e = quadrat(&u, 9.0).erdbau(&u, &GRUEN, &z(8.0));
        let frei = quadrat(&u, 9.0).erdbau(
            &u,
            &GRUEN,
            &Zonen {
                gruben: Vec::new(),
                ..z(8.0)
            },
        );
        assert!((e.weniger_verfuellen - ring * 0.9).abs() < 1e-6, "{e:?}");
        assert_eq!(e.je_grube, [e.weniger_verfuellen]);
        assert!(
            (frei.abtrag - e.abtrag - ring * 0.9).abs() < 1e-6,
            "{e:?} {frei:?}"
        );
        assert_eq!(e.unter_sohle, 0.0);
        // Q = 7,30 unter B = 8,00: Verfüllung bis N fällt ganz weg, 0,70 m
        // Abtrag unter der Sohle
        let e = quadrat(&u, 7.5).erdbau(&u, &GRUEN, &z(8.0));
        assert!((e.weniger_verfuellen - ring * 1.7).abs() < 1e-6, "{e:?}");
        assert!((e.unter_sohle - ring * 0.7).abs() < 1e-6, "{e:?}");
        let frei = quadrat(&u, 7.5).erdbau(
            &u,
            &GRUEN,
            &Zonen {
                gruben: Vec::new(),
                ..z(8.0)
            },
        );
        assert!(
            (frei.abtrag - e.abtrag - ring * 1.7).abs() < 1e-6,
            "{e:?} {frei:?}"
        );
        // B = 9,80 über N: dort ist nichts ausgehoben, alles zählt wie ohne
        // Baugrube
        let e = quadrat(&u, 7.5).erdbau(&u, &GRUEN, &z(9.8));
        let frei = quadrat(&u, 7.5).erdbau(
            &u,
            &GRUEN,
            &Zonen {
                gruben: Vec::new(),
                ..z(9.8)
            },
        );
        assert!((e.abtrag - frei.abtrag).abs() < 1e-6, "{e:?} {frei:?}");
        assert_eq!((e.weniger_verfuellen, e.unter_sohle), (0.0, 0.0));
    }

    #[test]
    fn ueberlappung_l_und_quadrat() {
        let l = [
            [0.0, 0.0],
            [6.0, 0.0],
            [6.0, 2.0],
            [2.0, 2.0],
            [2.0, 5.0],
            [0.0, 5.0],
        ];
        // 1 … 3 × 1 … 3: in der L-Form liegen 1 × 1 + 1 × 2 + 1 × 1
        let q = rechteck(1.0, 1.0, 3.0, 3.0);
        assert!((ueberlappung(&l, &q) - 3.0).abs() < 1e-12);
        assert_eq!(ueberlappung(&l, &rechteck(3.0, 3.0, 5.0, 5.0)), 0.0);
    }

    #[test]
    fn ohren_l_form() {
        let l = [
            [0.0, 0.0],
            [6.0, 0.0],
            [6.0, 2.0],
            [2.0, 2.0],
            [2.0, 5.0],
            [0.0, 5.0],
        ];
        let t = ohren(&l);
        assert_eq!(t.len(), 4);
        let a: f64 = t.iter().map(|t| super::super::flaeche(t)).sum();
        assert!((a - 18.0).abs() < 1e-12);
        // im Uhrzeigersinn dieselbe Fläche
        let mut r = l.to_vec();
        r.reverse();
        let a: f64 = ohren(&r).iter().map(|t| super::super::flaeche(t)).sum();
        assert!((a - 18.0).abs() < 1e-12);
    }
}

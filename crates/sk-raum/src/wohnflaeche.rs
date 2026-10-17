//! Wohnfläche nach der Wohnflächenverordnung (Grundriss G8,
//! `grundriss/plan.md` §4.5, §8 und §9; Jörn 11.10. 16:55, 16:57, 17:43 und
//! 13.10. 02:14): je Raum die Grundfläche an der fertigen Oberfläche, also
//! am modellierten Ausbau des Raumbuchs (Putz und Wandfliesen in
//! Fußbodenhöhe, [`ausbau::dicken`], dieselbe Linie wie die Putzkörper der
//! Ansicht), zerlegt in Teilflächen, die ein Prüfer nachrechnen kann:
//! Rechtecke als Länge × Breite, schräge Ränder als Dreieck ½ × a × b.
//!
//! - Türnischen zählen nicht (§ 3 Abs. 3 Nr. 3), auch nicht die einer Tür in
//!   der Außenwand (Haustür, Fenstertür).
//! - Fensternischen zählen, wenn sie bis zum Fußboden reichen und mehr als
//!   0,13 m tief sind (§ 3 Abs. 3 Nr. 4): von der fertigen Wandoberfläche bis
//!   zum Fenster, zwischen den geputzten Laibungen.
//! - Pfeiler, Schornsteine und Vormauerungen im Raum über 0,1 m² werden
//!   abgezogen (§ 3 Abs. 3 Nr. 1), kleinere nicht; Stützen über 0,1 m² und
//!   höher als 1,50 m und Treppen mit über drei Steigungen ebenso
//!   ([`einbau`], § 3 Abs. 3 Nr. 1 und 2). Eine Treppe aus dem Geschoss
//!   darunter steht als Treppenloch ohne Fußboden und wird abgezogen; was
//!   abgezogene Pfeiler, Stützen und Treppen gemeinsam decken, geht einmal
//!   ab, auch wo mehrere übereinander liegen (Review #195 H-195j, H-241b).
//! - Faktor nach § 4: lichte Höhe ab 2 m voll, ab 1 m zur Hälfte, darunter
//!   nicht, mal die Anrechnung der Raumvorlage (100, 50, 25 oder 0 %); ein
//!   Faktor am Raum geht beiden vor.
//! - Gerundet je Raum auf 0,01 m², kaufmännisch; Summen aus den gerundeten
//!   Raumwerten.
//!
//! Die Maße der Teilflächen stehen auf den Millimeter, ihre Flächen rechnen
//! mit genau diesen Maßen: Was das Blatt zeigt, rechnet sich nach. Die
//! Grundfläche selbst kommt aus der genauen Fläche des Fertigumrisses, wie
//! am Stempel; weicht die Summe der Teilflächen davon ab, gleicht eine
//! Zeile „Rundungsausgleich“ das aus (Review #195 B3).

use crate::ausbau::{self, RaumAusbau};
use crate::erkennung::{Begrenzung, Kante, RaumForm};
use crate::flaechen::PUTZ;
use crate::oeffnungen::{Geschosse, OpeningKind, Rohbau};
use crate::Ketten;
use sk_math::{vec3, Vec3};
use sk_model::rechenweg::Art;
use sk_model::{BuildingId, ElementId, LevelKind, Model, StoreyId};
use std::collections::HashMap;

#[path = "wohnflaeche_einbau.rs"]
pub mod einbau;
pub use einbau::{Einbau, Einbauten};

/// Bis zu dieser Tiefe (mm) zählt eine bodentiefe Fensternische nicht.
pub const NISCHE: f64 = 130.0;
/// Bis zu dieser Grundfläche (mm²) bleibt ein Pfeiler in der Fläche.
pub const PFEILER: f64 = 100_000.0;
/// Lichte Höhen (mm) für den vollen und den halben Faktor (§ 4 Nr. 1, 2).
pub const VOLL: f64 = 2000.0;
pub const HALB: f64 = 1000.0;
/// Unter diesem Maß (mm) gelten zwei Höhen als gleich und ein Rand als
/// gerade.
const EPS: f64 = 0.5;

/// Eine Zeile des Rechenwegs: Teilfläche, Zuschlag, Abzug oder Hinweis.
#[derive(Clone, Debug, PartialEq)]
pub struct Teil {
    /// Laufende Nummer im Raum, wie in der Skizze; keine beim Hinweis.
    pub nr: Option<usize>,
    /// „4,970 × 3,970“, „½ × 1,200 × 0,800“, „Fensternische W-003 …“.
    pub text: String,
    /// mm², mit Vorzeichen; 0 beim Hinweis.
    pub wert: f64,
    pub art: Art,
    /// Umriss für die Skizze (z = 0); leer beim Hinweis.
    pub umriss: Vec<Vec3>,
}

/// Anrechnung der Grundfläche.
#[derive(Clone, Debug, PartialEq)]
pub struct Faktor {
    /// Prozent (0 bis 100).
    pub prozent: f64,
    /// Woher: „lichte Höhe 2,465 m“, „Raumvorlage „Technik“: 0 %“.
    pub grund: String,
    /// Lichte Höhe (mm), Fertigfußboden bis Fertigdecke.
    pub lichte_hoehe: f64,
}

/// Wohnfläche eines Raums.
#[derive(Clone, Debug, PartialEq)]
pub struct WohnRaum {
    pub raum: Option<ElementId>,
    /// „R-001“.
    pub nummer: String,
    pub name: String,
    pub geschoss: StoreyId,
    pub teile: Vec<Teil>,
    /// Grundfläche in 0,01 m²: Summe der Teile, gerundet.
    pub grund: i64,
    pub faktor: Faktor,
    /// Wohnfläche in 0,01 m²: gerundete Grundfläche mal Faktor, gerundet.
    pub wohn: i64,
    /// Fertigumriss: Rand und Löcher, für die Skizze.
    pub umriss: Vec<Vec<Vec3>>,
}

/// Räume eines Geschosses.
#[derive(Clone, Debug, PartialEq)]
pub struct Geschoss {
    pub id: StoreyId,
    pub name: String,
    pub raeume: Vec<WohnRaum>,
}

impl Geschoss {
    /// Wohnfläche in 0,01 m², aus den gerundeten Raumwerten.
    pub fn wohn(&self) -> i64 {
        self.raeume.iter().map(|r| r.wohn).sum()
    }

    pub fn grund(&self) -> i64 {
        self.raeume.iter().map(|r| r.grund).sum()
    }
}

/// Geschosse eines Gebäudes, von unten nach oben.
#[derive(Clone, Debug, PartialEq)]
pub struct Gebaeude {
    pub id: Option<BuildingId>,
    pub name: String,
    pub geschosse: Vec<Geschoss>,
}

impl Gebaeude {
    pub fn wohn(&self) -> i64 {
        self.geschosse.iter().map(Geschoss::wohn).sum()
    }

    pub fn grund(&self) -> i64 {
        self.geschosse.iter().map(Geschoss::grund).sum()
    }
}

/// Wohnflächenberechnung des Projekts: je Gebäude, Geschoss und Raum.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wohnflaeche {
    pub gebaeude: Vec<Gebaeude>,
}

impl Wohnflaeche {
    pub fn wohn(&self) -> i64 {
        self.gebaeude.iter().map(Gebaeude::wohn).sum()
    }

    pub fn grund(&self) -> i64 {
        self.gebaeude.iter().map(Gebaeude::grund).sum()
    }

    pub fn raeume(&self) -> impl Iterator<Item = &WohnRaum> {
        self.gebaeude
            .iter()
            .flat_map(|g| &g.geschosse)
            .flat_map(|g| &g.raeume)
    }
}

/// „4,970“ aus mm.
pub fn m(x: f64) -> String {
    drei_stellen(x)
}

/// „19,731 m²“ aus mm².
pub fn qm3(x: f64) -> String {
    format!("{} m²", drei_stellen(x / 1e3))
}

/// Tausendstel `t` mit drei Nachkommastellen, kaufmännisch gerundet (ein
/// Gleichstand von der Null weg wie im Aufmaß, nicht zur geraden Ziffer).
fn drei_stellen(t: f64) -> String {
    let n = t.round();
    let s = if n < 0.0 { "-" } else { "" };
    let n = n.abs() as u64;
    format!("{s}{},{:03}", n / 1000, n % 1000)
}

/// „19,73 m²“ aus 0,01 m².
pub fn qm(h: i64) -> String {
    let s = if h < 0 { "-" } else { "" };
    let h = h.unsigned_abs();
    format!("{s}{},{:02} m²", tausender(h / 100), h % 100)
}

/// „1.234“.
fn tausender(n: u64) -> String {
    let d = n.to_string();
    let mut out = String::new();
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// „50 %“, „33,3 %“.
pub fn prozent(p: f64) -> String {
    if (p - p.round()).abs() < 1e-9 {
        format!("{} %", p.round())
    } else {
        format!("{:.1} %", p).replace('.', ",")
    }
}

/// mm² auf 0,01 m², die Hälfte vom Nullpunkt weg.
pub fn hundertstel(x: f64) -> i64 {
    // ein Hauch über der Rechnung, damit 0,005 m² sicher aufrundet
    let h = x / 1e4;
    (h.abs() + 1e-9).round().copysign(h) as i64
}

/// Wohnfläche (0,01 m²) zur Grundfläche `grund` (0,01 m²) und `prozent`.
pub fn anrechnen(grund: i64, prozent: f64) -> i64 {
    let w = grund as f64 * prozent / 100.0;
    (w.abs() + 1e-9).round().copysign(w) as i64
}

// --- Zerlegung ----------------------------------------------------------------

/// Rechtwinkliger Rahmen: Ursprung, Achse längs und quer.
#[derive(Clone, Copy, Debug)]
struct Rahmen {
    o: Vec3,
    u: Vec3,
    v: Vec3,
}

impl Rahmen {
    fn lokal(&self, p: Vec3) -> (f64, f64) {
        let d = vec3(p.x - self.o.x, p.y - self.o.y, 0.0);
        // Rechenrauschen weg, damit gleiche Maße gleich bleiben
        let r = |x: f64| (x * 1e3).round() / 1e3;
        (r(d.dot(self.u)), r(d.dot(self.v)))
    }

    fn welt(&self, s: f64, t: f64) -> Vec3 {
        self.o + self.u * s + self.v * t
    }
}

/// Stück eines Streifens: zwischen `t0` und `t1` vom linken Rand bis zum
/// rechten, je bei `t0` und `t1`. Beide Ränder sind gerade.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Stueck {
    t0: f64,
    t1: f64,
    l: (f64, f64),
    r: (f64, f64),
}

/// Läuft der Rand `(a0, a1)` von `t0` bis `t1` gerade in `(b0, b1)` bis
/// `t2` weiter?
fn weiter(a: (f64, f64), b: (f64, f64), t0: f64, t1: f64, t2: f64) -> bool {
    let fort = a.0 + (a.1 - a.0) * (t2 - t0) / (t1 - t0);
    (a.1 - b.0).abs() < EPS && (fort - b.1).abs() < EPS
}

/// Zerlegt den einfachen Ring `q` (lokal) in Streifen quer zur Achse `t`:
/// Schnitte an jeder Ecke, Stücke, deren Ränder gerade weiterlaufen, über
/// mehrere Streifen zusammengefasst.
fn streifen(q: &[(f64, f64)]) -> Vec<Stueck> {
    let n = q.len();
    let mut ts: Vec<f64> = q.iter().map(|p| p.1).collect();
    ts.sort_by(f64::total_cmp);
    let mut stufen: Vec<f64> = Vec::new();
    for t in ts {
        if stufen.last().is_none_or(|l| t - l > EPS) {
            stufen.push(t);
        }
    }
    let einrasten = |t: f64| {
        stufen
            .iter()
            .copied()
            .min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
            .unwrap_or(t)
    };
    let q: Vec<(f64, f64)> = q.iter().map(|&(s, t)| (s, einrasten(t))).collect();
    let mut offen: Vec<Stueck> = Vec::new();
    let mut fertig: Vec<Stueck> = Vec::new();
    for w in stufen.windows(2) {
        let (t0, t1) = (w[0], w[1]);
        let tm = 0.5 * (t0 + t1);
        let mut x: Vec<(f64, f64, f64)> = (0..n)
            .filter_map(|i| {
                let (a, b) = (q[i], q[(i + 1) % n]);
                let (lo, hi) = if a.1 < b.1 { (a, b) } else { (b, a) };
                (lo.1 <= t0 && hi.1 >= t1 && hi.1 > lo.1).then(|| {
                    let bei = |t: f64| lo.0 + (hi.0 - lo.0) * (t - lo.1) / (hi.1 - lo.1);
                    (bei(tm), bei(t0), bei(t1))
                })
            })
            .collect();
        x.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut neu = Vec::new();
        for p in x.chunks_exact(2) {
            let (l, r) = ((p[0].1, p[0].2), (p[1].1, p[1].2));
            let alt = offen
                .iter()
                .position(|s| weiter(s.l, l, s.t0, s.t1, t1) && weiter(s.r, r, s.t0, s.t1, t1));
            match alt {
                Some(k) => {
                    let mut s = offen.remove(k);
                    (s.t1, s.l.1, s.r.1) = (t1, l.1, r.1);
                    neu.push(s);
                }
                None => neu.push(Stueck { t0, t1, l, r }),
            }
        }
        fertig.append(&mut offen);
        offen = neu;
    }
    fertig.append(&mut offen);
    fertig.sort_by(|a, b| a.t0.total_cmp(&b.t0).then(a.l.0.total_cmp(&b.l.0)));
    fertig
}

/// Eine Teilfläche vor der Nummerierung: Text, Fläche (mm²), Umriss.
type Roh = (String, f64, Vec<Vec3>);

/// Teilflächen eines Stücks: Rechteck, dazu ein Dreieck je schrägem Rand;
/// kreuzen sich die schrägen Ränder, ein Trapez.
fn teile_stueck(s: &Stueck, r: &Rahmen) -> Vec<Roh> {
    let mm = |x: f64| x.round();
    let h = mm(s.t1 - s.t0);
    let p = |pts: &[(f64, f64)]| pts.iter().map(|&(x, t)| r.welt(x, t)).collect::<Vec<_>>();
    let (l_innen, r_innen) = (s.l.0.max(s.l.1), s.r.0.min(s.r.1));
    let mut out = Vec::new();
    if r_innen - l_innen < -EPS {
        let (a, b) = (mm(s.r.0 - s.l.0), mm(s.r.1 - s.l.1));
        out.push((
            format!("({} + {}) / 2 × {}", m(a), m(b), m(h)),
            (a + b) / 2.0 * h,
            p(&[(s.l.0, s.t0), (s.r.0, s.t0), (s.r.1, s.t1), (s.l.1, s.t1)]),
        ));
        return out;
    }
    let b = mm(r_innen - l_innen);
    if b > 0.0 {
        out.push((
            format!("{} × {}", m(b), m(h)),
            b * h,
            p(&[
                (l_innen, s.t0),
                (r_innen, s.t0),
                (r_innen, s.t1),
                (l_innen, s.t1),
            ]),
        ));
    }
    let dreieck = |rand: (f64, f64), innen: f64| -> Option<Roh> {
        let a = mm((rand.1 - rand.0).abs());
        (a > 0.0).then(|| {
            (
                format!("½ × {} × {}", m(a), m(h)),
                a * h / 2.0,
                p(&[(rand.0, s.t0), (innen, s.t0), (innen, s.t1), (rand.1, s.t1)]),
            )
        })
    };
    out.extend(dreieck(s.l, l_innen));
    out.extend(dreieck(s.r, r_innen));
    out
}

/// Zerlegt den Ring `punkte` in Teilflächen: in Streifen längs oder quer zu
/// einer seiner Kanten, so dass es die wenigsten Teile gibt; bei gleich
/// vielen, so dass die Maße auf den Millimeter die Fläche am genauesten
/// treffen, dann längs der längsten Kante.
fn zerlegen(punkte: &[Vec3]) -> Vec<Roh> {
    let n = punkte.len();
    if n < 3 {
        return Vec::new();
    }
    let flaeche = sk_math::polygon::area(punkte);
    let mut kanten: Vec<usize> = (0..n).collect();
    let laenge = |i: usize| (punkte[(i + 1) % n] - punkte[i]).length();
    kanten.sort_by(|&i, &j| laenge(j).total_cmp(&laenge(i)));
    let mut richtungen: Vec<Vec3> = Vec::new();
    let mut beste: Option<(usize, f64, Vec<Roh>)> = None;
    for k in kanten {
        let (a, b) = (punkte[k], punkte[(k + 1) % n]);
        let (u, quer) = richtung(a, b);
        if u.length() < 0.5 {
            continue;
        }
        let o = vec3(a.x, a.y, 0.0);
        for (u, v) in [(u, quer), (quer, u * -1.0)] {
            // Jede Richtung einmal, auch die Gegenrichtung zählt als dieselbe
            if richtungen.iter().any(|r| r.dot(u).abs() > 1.0 - 1e-9) {
                continue;
            }
            richtungen.push(u);
            let r = Rahmen { o, u, v };
            let q: Vec<(f64, f64)> = punkte.iter().map(|p| r.lokal(*p)).collect();
            let t: Vec<Roh> = streifen(&q)
                .iter()
                .flat_map(|s| teile_stueck(s, &r))
                .collect();
            let fehler = (t.iter().map(|x| x.1).sum::<f64>() - flaeche).abs();
            let besser = beste
                .as_ref()
                .is_none_or(|(z, f, _)| t.len() < *z || (t.len() == *z && fehler < f - 1.0));
            if besser {
                beste = Some((t.len(), fehler, t));
            }
        }
    }
    beste.map_or_else(Vec::new, |b| b.2)
}

// --- Raum -------------------------------------------------------------------

/// Nummer des Bauteils `id`, sonst `ersatz`.
fn nummer(model: &Model, id: ElementId, ersatz: &str) -> String {
    model
        .element(id)
        .map_or_else(|| ersatz.into(), |e| e.number.clone())
}

/// Wand der Kante.
fn wand(k: &Kante) -> ElementId {
    let Begrenzung::Wand { wand, .. } = &k.quelle;
    *wand
}

/// Faktor des Raums nach § 4 und seiner Raumvorlage.
fn faktor(model: &Model, form: &RaumForm, lichte_hoehe: f64) -> Faktor {
    let am_raum = form
        .raum
        .and_then(|id| model.space(id))
        .and_then(|s| s.faktor);
    if let Some(f) = am_raum {
        return Faktor {
            prozent: (f * 100.0).clamp(0.0, 100.0),
            grund: "am Raum festgelegt".into(),
            lichte_hoehe,
        };
    }
    let hoehe = |h: f64| {
        if h >= VOLL - EPS {
            100.0
        } else if h >= HALB - EPS {
            50.0
        } else {
            0.0
        }
    };
    let h = hoehe(lichte_hoehe);
    let vorlage = ausbau::vorlage(model, ausbau::nutzung(model, form));
    let v = vorlage.as_ref().map_or(100.0, |s| f64::from(s.woflv));
    let text_h = format!("lichte Höhe {} m", m(lichte_hoehe));
    let text_v = |s: &str| {
        let regel = match v as u8 {
            50 => " (§ 4 Nr. 3)",
            25 => " (§ 4 Nr. 4)",
            0 => " (Zubehörraum, § 2 Abs. 3)",
            _ => "",
        };
        format!("Raumvorlage „{s}“: {}{regel}", prozent(v))
    };
    let text_h = match h as u8 {
        100 => text_h,
        50 => format!("{text_h}: 50 % (§ 4 Nr. 2)"),
        _ => format!("{text_h}: 0 % (unter 1 m, § 4)"),
    };
    let grund = match (&vorlage, v < 100.0, h < 100.0) {
        (Some(s), true, true) => format!("{} · {text_h}", text_v(&s.name)),
        (Some(s), true, false) => text_v(&s.name),
        _ => text_h,
    };
    Faktor {
        prozent: v * h / 100.0,
        grund,
        lichte_hoehe,
    }
}

/// Wohnfläche eines Raums ohne Stützen und Treppen; siehe [`raum_mit`].
pub fn raum(model: &Model, roh: &Rohbau, aus: &RaumAusbau) -> WohnRaum {
    raum_mit(model, roh, aus, &Einbauten::default())
}

/// Wohnfläche eines Raums. `aus`: sein Ausbau ([`RaumAusbau::fuer`]); hat er
/// keinen Typ im Projekt ([`RaumAusbau::mit_typen`], wie die Grundfläche am
/// Stempel), gilt an jeder Wandseite ohne Bekleidung im Wandtyp der
/// Innenputz der Vorgabe ([`PUTZ`]). `einbauten`: Stützen und Treppen seines
/// Geschosses ([`einbau::sammeln`]).
pub fn raum_mit(model: &Model, roh: &Rohbau, aus: &RaumAusbau, einbauten: &Einbauten) -> WohnRaum {
    let form = &roh.form;
    let mut hinweise: Vec<String> = Vec::new();
    // Wie grundflaeche_fertig (Review #195 H-195d), ohne Gang über alle
    // Geschosse (B2)
    let vorgabe = !aus.mit_typen();
    let kanten: Vec<&Kante> = roh.kanten().collect();
    let (dicken, laibungen, (ffb, fd)) = if vorgabe {
        hinweise.push(format!(
            "ohne Ausbautypen des Raumbuchs: Innenputz {} mm angenommen",
            PUTZ
        ));
        let d: Vec<f64> = kanten.iter().map(|k| crate::flaechen::ausbau(k)).collect();
        let l = roh
            .oeffnungen
            .iter()
            .map(|o| {
                if ausbau::laibung_behandelt(o.art) {
                    d.get(o.kante).copied().unwrap_or(PUTZ)
                } else {
                    0.0
                }
            })
            .collect();
        let ffb = roh.z_boden + ausbau::AUFBAU;
        (d, l, (ffb, roh.z_decke))
    } else {
        (
            ausbau::dicken(model, roh, aus),
            ausbau::laibungen(model, roh, aus),
            ausbau::hoehen(model, roh, aus),
        )
    };

    // Grundfläche an der fertigen Oberfläche; `genau` ohne die Rundung der
    // Maße (B3)
    let mut roh_teile: Vec<(Roh, Art)> = Vec::new();
    let mut umriss: Vec<Vec<Vec3>> = Vec::new();
    let mut genau = 0.0;
    // Abgezogene Pfeiler und Einbauten in Stücken ohne Überdeckung: Was ein
    // weiterer Einbau mit ihnen deckt, ist schon ab, auch wo mehrere
    // übereinander liegen (Review #195 H-195j, H-241b)
    let mut abgezogen: Vec<Vec<Vec3>> = Vec::new();
    let mut erste = 0;
    for (i, ring) in roh.ringe().enumerate() {
        let p: Vec<Vec3> = ring.iter().map(|k| k.a).collect();
        let d = &dicken[erste..erste + ring.len()];
        erste += ring.len();
        let Some(fertig) = sk_math::flaeche::versatz(&p, d) else {
            hinweise.push(if i == 0 {
                "Raum schmaler als sein Ausbau: keine Grundfläche".into()
            } else {
                "Loch im Raum zerfällt im Ausbau".into()
            });
            continue;
        };
        if i == 0 {
            genau += sk_math::polygon::area(&fertig);
            roh_teile.extend(zerlegen(&fertig).into_iter().map(|t| (t, Art::Basis)));
        } else {
            let name = nummer(model, wand(&ring[0]), "Pfeiler");
            let a = sk_math::polygon::area(&fertig);
            if (a / 1e3).round() <= PFEILER / 1e3 {
                hinweise.push(format!(
                    "{name} {} bleibt in der Fläche (bis 0,1 m², § 3 Abs. 3 Nr. 1)",
                    qm3(a)
                ));
            } else {
                genau -= a;
                abgezogen.extend(einbau::dreiecke(&fertig).iter().map(|t| t.to_vec()));
                for (text, wert, u) in zerlegen(&fertig) {
                    roh_teile.push(((format!("{name} {text}"), -wert, u), Art::Abzug));
                }
            }
        }
        umriss.push(fertig);
    }

    // Nischen
    let mut gesehen: Vec<ElementId> = Vec::new();
    for (i, o) in roh.oeffnungen.iter().enumerate() {
        let name = nummer(model, o.oeffnung, "Öffnung");
        let am_boden = o.uk <= ffb + EPS && o.uk >= roh.z_boden - EPS;
        match o.art {
            OpeningKind::InnerDoor | OpeningKind::Door => {
                if !gesehen.contains(&o.oeffnung) {
                    gesehen.push(o.oeffnung);
                    hinweise.push(format!("Türnische {name} zählt nicht (§ 3 Abs. 3 Nr. 3)"));
                }
            }
            OpeningKind::Window if am_boden => {
                let Some(k) = kanten.get(o.kante) else {
                    continue;
                };
                let dk = dicken.get(o.kante).copied().unwrap_or(0.0);
                let tiefe = (o.tiefe + dk).round();
                if tiefe <= NISCHE {
                    hinweise.push(format!(
                        "Fensternische {name} {} m tief zählt nicht (bis 0,13 m, § 3 Abs. 3 Nr. 4)",
                        m(tiefe)
                    ));
                    continue;
                }
                let lp = laibungen.get(i).copied().unwrap_or(0.0);
                let (von, bis) = (o.von + lp, o.bis - lp);
                let b = (bis - von).round();
                if b <= 0.0 {
                    continue;
                }
                genau += (bis - von) * (o.tiefe + dk);
                let (u, n) = richtung(k.a, k.b);
                let a = vec3(k.a.x, k.a.y, 0.0);
                let at = |s: f64, q: f64| a + u * s + n * q;
                roh_teile.push((
                    (
                        format!("Fensternische {name} {} × {}", m(b), m(tiefe)),
                        b * tiefe,
                        vec![
                            at(von, dk),
                            at(von, -o.tiefe),
                            at(bis, -o.tiefe),
                            at(bis, dk),
                        ],
                    ),
                    Art::Zuschlag,
                ));
            }
            OpeningKind::Window => {}
        }
    }

    // Stützen und Treppen im Raum (B1); eine Stütze mit dem Putz des Raums
    // ringsum, wie ein Pfeiler aus einem Wandring an der Vorderkante seiner
    // Bekleidung (§ 3 Abs. 1, Review #195 H-195i)
    let putz = if vorgabe {
        PUTZ
    } else {
        // Putz der Wandseiten (Raumbuch oder Wandtyp), der dickste
        kanten
            .iter()
            .zip(&aus.seiten)
            .map(|(k, s)| ausbau::putz(model, k, s))
            .fold(0.0, f64::max)
    };
    if let Some(rand) = umriss.first().filter(|_| !einbauten.is_empty()) {
        let dreiecke = einbau::dreiecke(rand);
        for e in einbauten.bei(einbau::rahmen(rand)) {
            let geputzt = (!e.treppe && putz > 0.0)
                .then(|| einbau::geputzt(e, putz))
                .flatten();
            let mit_putz = match geputzt {
                Some(_) => format!(" einschl. Putz {} mm", putz.round()),
                None => String::new(),
            };
            let e = geputzt.as_ref().unwrap_or(e);
            // Die Schwelle 0,1 m² gilt für die ganze bekleidete Stütze im
            // Raum, abgezogen wird der Rest neben schon Abgezogenem (VOB
            // A-241a)
            let ganz = einbau::im_raum(e, &dreiecke);
            let neu = einbau::im_raum_ohne(e, &dreiecke, &abgezogen);
            let f: f64 = neu.iter().map(|p| sk_math::polygon::area(p)).sum();
            if f < 1.0 {
                continue;
            }
            let name = nummer(model, e.id, if e.treppe { "Treppe" } else { "Stütze" });
            let hoch = e.z.1.min(fd) - e.z.0.max(ffb);
            let (abzug, grund) = if e.loch {
                (true, String::new())
            } else if e.treppe {
                match e.steigungen {
                    Some(n) if n <= einbau::STEIGUNGEN => (
                        false,
                        format!("{name} mit {n} Steigungen bleibt in der Fläche (bis drei, § 3 Abs. 3 Nr. 2)"),
                    ),
                    _ => (true, String::new()),
                }
            } else if (ganz / 1e3).round() <= PFEILER / 1e3 {
                (
                    false,
                    format!(
                        "{name} {} bleibt in der Fläche (bis 0,1 m², § 3 Abs. 3 Nr. 1)",
                        qm3(ganz)
                    ),
                )
            } else if hoch <= einbau::STUETZE_HOCH + EPS {
                (
                    false,
                    format!(
                        "{name} {} m hoch bleibt in der Fläche (bis 1,50 m, § 3 Abs. 3 Nr. 1)",
                        m(hoch.max(0.0))
                    ),
                )
            } else {
                (true, String::new())
            };
            if !abzug {
                hinweise.push(grund);
                continue;
            }
            genau -= f;
            abgezogen.extend(neu);
            // Ein Treppenloch ist im Modell nicht abgebildet: angenommen wie
            // die Treppe darunter (VOB V-241b/c)
            let (name, regel) = if e.loch {
                (format!("Treppenloch {name}"), "Deckenöffnung, § 3 Abs. 1")
            } else if e.treppe {
                (name, "§ 3 Abs. 3 Nr. 2")
            } else {
                (name, "§ 3 Abs. 3 Nr. 1")
            };
            let wie = if e.loch {
                " angenommen wie Treppe darunter"
            } else {
                ""
            };
            let (text, wert) = match einbau::rechteck(e, f) {
                Some((a, b)) => {
                    let (a, b) = (a.round(), b.round());
                    (
                        format!("{name} {} × {}{mit_putz}{wie} ({regel})", m(a), m(b)),
                        a * b,
                    )
                }
                None => (
                    format!("{name} im Raum{mit_putz}{wie} ({regel})"),
                    (f / 1e3).round() * 1e3,
                ),
            };
            let u = e.stuecke.first().cloned().unwrap_or_default();
            roh_teile.push(((text, -wert, u), Art::Abzug));
        }
    }

    let mut teile: Vec<Teil> = roh_teile
        .into_iter()
        .enumerate()
        .map(|(i, ((text, wert, umriss), art))| Teil {
            nr: Some(i + 1),
            text,
            wert,
            art,
            umriss,
        })
        .collect();
    teile.extend(hinweise.into_iter().map(|text| Teil {
        nr: None,
        text,
        wert: 0.0,
        art: Art::Hinweis,
        umriss: Vec::new(),
    }));
    // Rundungsausgleich: Die Teilflächen rechnen mit Maßen auf den
    // Millimeter, die Grundfläche mit der genauen Fläche (B3)
    let summe: f64 = teile.iter().map(|t| t.wert).sum();
    let ausgleich = genau - summe;
    if ausgleich.abs() >= 0.5 && !teile.is_empty() {
        let pos = teile
            .iter()
            .position(|t| t.art == Art::Hinweis)
            .unwrap_or(teile.len());
        teile.insert(
            pos,
            Teil {
                nr: None,
                text: "Rundungsausgleich der Maße auf den Millimeter".into(),
                wert: ausgleich,
                art: if ausgleich > 0.0 {
                    Art::Zuschlag
                } else {
                    Art::Abzug
                },
                umriss: Vec::new(),
            },
        );
    }
    let grund = hundertstel(genau);
    let faktor = faktor(model, form, (fd - ffb).max(0.0));
    let wohn = anrechnen(grund, faktor.prozent);
    let (nr, name) = form
        .raum
        .and_then(|id| {
            Some((
                model.element(id)?.number.clone(),
                model.space(id)?.name.clone(),
            ))
        })
        .unwrap_or_default();
    WohnRaum {
        raum: form.raum,
        nummer: nr,
        name,
        geschoss: form.geschoss,
        teile,
        grund,
        faktor,
        wohn,
        umriss,
    }
}

/// Richtung und linke Normale der Kante (waagerecht).
fn richtung(a: Vec3, b: Vec3) -> (Vec3, Vec3) {
    let d = vec3(b.x - a.x, b.y - a.y, 0.0);
    let u = d * (1.0 / d.length().max(1e-9));
    (u, vec3(-u.y, u.x, 0.0))
}

// --- Projekt ----------------------------------------------------------------

/// Wohnflächenberechnung des Projekts, Räume aus dem Modell: eine
/// Erkennung für alle Geschosse, so wächst die Arbeit mit dem Modell, nicht
/// mit Geschossen mal Zügen (Review #195 B2).
pub fn projekt(model: &Model) -> Wohnflaeche {
    let geschosse: Vec<StoreyId> = model.storeys().iter().map(|(id, _)| id).collect();
    let mut je: HashMap<StoreyId, Vec<RaumForm>> = geschosse
        .iter()
        .copied()
        .zip(crate::Erkennung::raeume_alle(model, &geschosse))
        .collect();
    projekt_mit(model, model, |st| je.remove(&st).unwrap_or_default())
}

/// Was die Wohnfläche eines Raums außer seiner Form liest: Höhen der
/// Geschosse, Stützen und Treppen je Geschoss. Einmal je Modellstand für
/// alle Räume (Review #137 B2, #195 B1, B2); Blatt, Stempel und Paneel
/// rechnen damit dieselbe Grundfläche (Review #195 B, Bedienbarkeit).
#[derive(Clone, Debug, Default)]
pub struct Umfeld {
    lagen: Geschosse,
    einbauten: HashMap<StoreyId, Einbauten>,
}

impl Umfeld {
    pub fn aus(model: &Model) -> Umfeld {
        Umfeld {
            lagen: Geschosse::aus(model),
            einbauten: einbau::sammeln(model),
        }
    }

    /// Wohnfläche des Raums der Form `f` wie im Blatt; `None`, wenn die
    /// Form keinen Raum trägt oder sein Ausbau fehlt.
    pub fn raum(&self, model: &Model, k: &(impl Ketten + ?Sized), f: RaumForm) -> Option<WohnRaum> {
        f.raum?;
        let roh = Rohbau::mit_geschossen(model, k, f, &self.lagen);
        let aus = RaumAusbau::fuer_in(model, &roh, &self.lagen)?;
        let leer = Einbauten::default();
        let ein = self.einbauten.get(&roh.form.geschoss).unwrap_or(&leer);
        Some(raum_mit(model, &roh, &aus, ein))
    }
}

/// Wie [`projekt`], die Wandzüge aus `k` und die Räume je Geschoss aus
/// `raeume` (in der App aus der Erkennung der Szene). Nur gespeicherte
/// Räume zählen; Gebäude und Geschosse ohne Raum fehlen.
pub fn projekt_mit(
    model: &Model,
    k: &(impl Ketten + ?Sized),
    raeume: impl FnMut(StoreyId) -> Vec<RaumForm>,
) -> Wohnflaeche {
    projekt_im_umfeld(model, k, &Umfeld::aus(model), raeume)
}

/// Wie [`projekt_mit`] mit dem Umfeld `umfeld` dieses Modellstands (in der
/// App dasselbe wie für Stempel und Paneel, Review #220 B1).
pub fn projekt_im_umfeld(
    model: &Model,
    k: &(impl Ketten + ?Sized),
    umfeld: &Umfeld,
    mut raeume: impl FnMut(StoreyId) -> Vec<RaumForm>,
) -> Wohnflaeche {
    let mut gebaeude: Vec<Gebaeude> = Vec::new();
    let mut gebaeude_nr: HashMap<Option<BuildingId>, usize> = HashMap::new();
    let mut geschosse: Vec<(StoreyId, &sk_model::Storey)> = model
        .storeys()
        .iter()
        .filter(|(_, s)| s.kind != LevelKind::Foundation)
        .collect();
    geschosse.sort_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation));
    for (id, s) in geschosse {
        let formen: Vec<RaumForm> = raeume(id)
            .into_iter()
            .filter(|f| f.raum.is_some())
            .collect();
        if formen.is_empty() {
            continue;
        }
        let mut rs: Vec<WohnRaum> = formen
            .into_iter()
            .filter_map(|f| umfeld.raum(model, k, f))
            .collect();
        rs.sort_by(|a, b| a.nummer.cmp(&b.nummer));
        let g = Geschoss {
            id,
            name: s.name.clone(),
            raeume: rs,
        };
        match gebaeude_nr.get(&s.building) {
            Some(&i) => gebaeude[i].geschosse.push(g),
            None => {
                gebaeude_nr.insert(s.building, gebaeude.len());
                gebaeude.push(Gebaeude {
                    id: s.building,
                    name: s
                        .building
                        .and_then(|b| model.building(b))
                        .map_or_else(|| "Gebäude".into(), |b| b.name.clone()),
                    geschosse: vec![g],
                });
            }
        }
    }
    Wohnflaeche { gebaeude }
}

#[cfg(test)]
#[path = "wohnflaeche_tests.rs"]
mod tests;

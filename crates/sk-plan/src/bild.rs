//! Bild der Außenmaßketten (Konzept §3, §6): Maßlinien, Maßhilfslinien,
//! Begrenzungen und Maßzahlen in Modellkoordinaten, flach auf der
//! Geschossebene. Linien behalten ihre Strichstärke am Bildschirm wie alle
//! Stifte; Kreise, Pfeile und Schrift sind so groß wie auf dem Blatt im
//! Bezugsmaßstab und wachsen mit dem Zoom.

use crate::aussen::{Aussenmasse, Bezug, Kette, Seite};
use crate::schrift::{Dreiecke, Schrift};
use crate::zahl::{masszahl, Masszahl};
use sk_math::{vec3, Vec3};
use sk_model::bemassung::{Kettenart, Stil, Symbol};

/// Teilstriche eines Kreises.
const KREIS: usize = 24;
/// Abstand der Maßzahl zu ihren Hilfslinien und Nachbarn, in Ziffernhöhen.
const LUFT: f64 = 0.25;
/// Abstand der Hochzahl zur Hauptzahl, in Ziffernhöhen.
const HOCH_LUFT: f64 = 0.04;

/// Wozu ein Strich gehört (Strichstärke und Farbe wählt die App).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strichart {
    Masslinie,
    Hilfslinie,
    /// Kreis und Schrägstrich der Begrenzung.
    Symbol,
}

/// Ein gerader Strich in Modellkoordinaten.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strich {
    pub a: Vec3,
    pub b: Vec3,
    pub art: Strichart,
    /// Kette, zu der er gehört: (Seite, Kette) in [`Aussenmasse::seiten`].
    pub kette: (usize, usize),
}

/// Was eine Zahl misst.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zahlart {
    /// Maß `i` der Kette (zwischen Bezug `i` und `i + 1`).
    Mass {
        seite: usize,
        kette: usize,
        i: usize,
    },
    /// Höhe der Öffnung `o` der Seite.
    Hoehe { seite: usize, o: usize },
    /// Brüstungshöhe der Öffnung `o` der Seite.
    Brh { seite: usize, o: usize },
}

/// Eine gesetzte Zahl: ihre Dreiecke in [`Bild::tinte`] und ihr Rahmen
/// (zum Anklicken).
#[derive(Clone, Debug, PartialEq)]
pub struct Zahl {
    pub art: Zahlart,
    /// Gemessener Wert (mm).
    pub wert: f64,
    pub text: Masszahl,
    pub tinte: std::ops::Range<usize>,
    /// Ecken des Rahmens: links unten, rechts unten, rechts oben, links oben
    /// (in Leserichtung).
    pub rahmen: [Vec3; 4],
}

/// Eine Maßlinie zum Greifen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Linie {
    pub seite: usize,
    pub kette: usize,
    pub a: Vec3,
    pub b: Vec3,
}

/// Ein Maßbezugspunkt am Bauteil und das Ende seiner Hilfslinie.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bezugslinie {
    pub am_bauteil: Vec3,
    pub an_kette: Vec3,
    pub kette: (usize, usize),
}

/// Alles, was die Ketten eines Geschosses zeigen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bild {
    pub striche: Vec<Strich>,
    /// Gefüllte Dreiecke: Schrift, Punkte und Pfeile.
    pub tinte: Vec<[Vec3; 3]>,
    pub zahlen: Vec<Zahl>,
    pub linien: Vec<Linie>,
    pub bezuege: Vec<Bezugslinie>,
}

/// Lage in der Ebene einer Seite: längs `t`, quer `n`, auf Höhe `z`.
struct Ebene {
    t: Vec3,
    n: Vec3,
    /// Nach oben in Leserichtung (senkrecht zu `t`), ±`n`.
    oben: Vec3,
    z: f64,
}

impl Ebene {
    fn new(s: &Seite, z: f64) -> Ebene {
        Ebene {
            t: s.t,
            n: s.n,
            oben: vec3(-s.t.y, s.t.x, 0.0),
            z,
        }
    }

    /// Punkt bei `t` längs und `d` quer.
    fn p(&self, t: f64, d: f64) -> Vec3 {
        let q = self.t * t + self.n * d;
        vec3(q.x, q.y, self.z)
    }

    /// `+1`, wenn „oben“ nach außen zeigt.
    fn auf(&self) -> f64 {
        self.oben.dot(self.n).signum()
    }
}

/// Zeichnet die Ketten `a` mit dem Stil `stil` und der Schrift `schrift`.
pub fn bild(a: &Aussenmasse, stil: &Stil, schrift: &Schrift) -> Bild {
    let mut b = Bild::default();
    for (si, s) in a.seiten.iter().enumerate() {
        let e = Ebene::new(s, a.z);
        for (ki, k) in s.ketten.iter().enumerate() {
            kette(&mut b, &e, stil, schrift, (si, ki), k);
            if k.art == Kettenart::Oeffnung {
                oeffnungen(&mut b, &e, stil, schrift, si, s, k);
            }
        }
    }
    b
}

fn kette(b: &mut Bild, e: &Ebene, stil: &Stil, schrift: &Schrift, id: (usize, usize), k: &Kette) {
    let (t0, t1) = (k.bezuege[0].t, k.bezuege[k.bezuege.len() - 1].t);
    let ueber = stil.am_bau(stil.ueberstand);
    let hu = stil.am_bau(stil.hilfs_ueber);
    let l = k.lage;
    let (a, z) = (e.p(t0 - ueber, l), e.p(t1 + ueber, l));
    b.striche.push(Strich {
        a,
        b: z,
        art: Strichart::Masslinie,
        kette: id,
    });
    b.linien.push(Linie {
        seite: id.0,
        kette: id.1,
        a,
        b: z,
    });
    for r in &k.bezuege {
        b.striche.push(Strich {
            a: e.p(r.t, l - hu),
            b: e.p(r.t, l + hu),
            art: Strichart::Hilfslinie,
            kette: id,
        });
        b.bezuege.push(Bezugslinie {
            am_bauteil: vec3(r.p.x, r.p.y, e.z),
            an_kette: e.p(r.t, l - hu),
            kette: id,
        });
    }
    begrenzungen(b, e, stil, id, l, &k.bezuege);
    zahlen(b, e, stil, schrift, id, l, &k.bezuege);
}

/// Begrenzung an jedem Bezugspunkt.
fn begrenzungen(b: &mut Bild, e: &Ebene, stil: &Stil, id: (usize, usize), l: f64, rs: &[Bezug]) {
    for (i, r) in rs.iter().enumerate() {
        let (striche, tinte) = begrenzung(e.p(r.t, l), e.t, e.n, stil, i > 0, i + 1 < rs.len());
        b.striche.extend(striche.into_iter().map(|(a, z)| Strich {
            a,
            b: z,
            art: Strichart::Symbol,
            kette: id,
        }));
        b.tinte.extend(tinte);
    }
}

/// Begrenzung des Stils an `m` auf einer Maßlinie längs `t`, quer dazu `n`
/// (Einheitsvektoren in der Grundrissebene), am Bau bemessen: Striche
/// (Kreis, Schrägstrich) und gefüllte Dreiecke (Punkt, Pfeil). `vor` und
/// `nach`: Es gibt ein Maß davor bzw. danach, dorthin zeigt je ein Pfeil.
/// Auch für Live-Ketten anderer Werkzeuge, damit sie aussehen wie die
/// Außenmaßketten.
pub fn begrenzung(
    m: Vec3,
    t: Vec3,
    n: Vec3,
    stil: &Stil,
    vor: bool,
    nach: bool,
) -> (Vec<(Vec3, Vec3)>, Vec<[Vec3; 3]>) {
    let g = stil.am_bau(stil.symbol_mm);
    let (mut striche, mut tinte) = (Vec::new(), Vec::new());
    let kreis = |rad: f64, k: usize| {
        let w = std::f64::consts::TAU * k as f64 / KREIS as f64;
        m + t * (rad * w.cos()) + n * (rad * w.sin())
    };
    match stil.symbol {
        Symbol::Kreis => {
            for k in 0..KREIS {
                striche.push((kreis(0.5 * g, k), kreis(0.5 * g, k + 1)));
            }
        }
        Symbol::Schraeg => {
            // 45°, von links unten nach rechts oben in Leserichtung
            let oben = vec3(-t.y, t.x, 0.0);
            let d = (t + oben) * (0.5 * g);
            striche.push((m - d, m + d));
        }
        Symbol::Punkt => {
            for k in 0..KREIS {
                tinte.push([m, kreis(0.5 * g, k), kreis(0.5 * g, k + 1)]);
            }
        }
        Symbol::Pfeil => {
            // Spitze an der Hilfslinie, je ein Pfeil zu jedem Nachbarmaß
            let breit = g / 3.0;
            let mut pfeil = |s: f64| {
                let fuss = m + t * (s * g);
                tinte.push([m, fuss + n * (0.5 * breit), fuss - n * (0.5 * breit)]);
            };
            if vor {
                pfeil(-1.0);
            }
            if nach {
                pfeil(1.0);
            }
        }
    }
    (striche, tinte)
}

/// Setzt `z` (Hauptzahl und Hochzahl, davor `vor`) in der Schrift des
/// Stils, am Bau bemessen, in der Leseebene (x längs, y nach oben, linke
/// Grundlinie im Ursprung); gibt die Dreiecke und die Breite zurück. Auch
/// für Live-Ketten anderer Werkzeuge.
pub fn satz(schrift: &Schrift, stil: &Stil, z: &Masszahl, vor: &str) -> (Dreiecke, f64) {
    let h = stil.am_bau(stil.schrift_mm);
    let hs = h * stil.hochzahl / 100.0;
    let mut v = Vec::new();
    let mut x = 0.0;
    if !vor.is_empty() {
        x += schrift.zeile(vor, h, x, &mut v);
    }
    x += schrift.zeile(&z.haupt, h, x, &mut v);
    if !z.hoch.is_empty() {
        x += HOCH_LUFT * h;
        let start = v.len();
        x += schrift.zeile(&z.hoch, hs, x, &mut v);
        // Oberkanten bündig
        for d in &mut v[start..] {
            for p in d.iter_mut() {
                p[1] += h - hs;
            }
        }
    }
    (v, x)
}

/// Legt Dreiecke aus der Leseebene (x längs, y nach oben) bei `ursprung`
/// in die Seite und merkt die Zahl.
#[allow(clippy::too_many_arguments)]
fn setzen(
    b: &mut Bild,
    e: &Ebene,
    stil: &Stil,
    ursprung: Vec3,
    v: Dreiecke,
    breite: f64,
    art: Zahlart,
    wert: f64,
    text: Masszahl,
) {
    let h = stil.am_bau(stil.schrift_mm);
    let p = |x: f64, y: f64| ursprung + e.t * x + e.oben * y;
    let start = b.tinte.len();
    b.tinte.extend(v.iter().map(|d| d.map(|q| p(q[0], q[1]))));
    b.zahlen.push(Zahl {
        art,
        wert,
        text,
        tinte: start..b.tinte.len(),
        rahmen: [p(0.0, 0.0), p(breite, 0.0), p(breite, h), p(0.0, h)],
    });
}

/// Maßzahlen einer Kette: über der Linie mittig; passt eine Zahl nicht
/// zwischen ihre Hilfslinien, rückt sie am Rand der Kette nach außen
/// daneben, sonst eine Zeile höher.
fn zahlen(
    b: &mut Bild,
    e: &Ebene,
    stil: &Stil,
    schrift: &Schrift,
    id: (usize, usize),
    l: f64,
    rs: &[Bezug],
) {
    let h = stil.am_bau(stil.schrift_mm);
    let abstand = stil.am_bau(stil.text_abstand);
    let luft = LUFT * h;
    let ueber = stil.am_bau(stil.ueberstand);
    // Belegte Bereiche längs je Zeile
    let mut zeilen: Vec<Vec<(f64, f64)>> = vec![Vec::new()];
    let n = rs.len() - 1;
    for i in 0..n {
        let (ta, tb) = (rs[i].t, rs[i + 1].t);
        let wert = tb - ta;
        let z = masszahl(wert, stil.rundung);
        let (v, w) = satz(schrift, stil, &z, "");
        let frei = |zeile: &Vec<(f64, f64)>, x0: f64, x1: f64| {
            zeile.iter().all(|&(a, c)| x1 + luft <= a || x0 >= c + luft)
        };
        let mitte = 0.5 * (ta + tb) - 0.5 * w;
        let (x0, zeile) = if w + 2.0 * luft <= wert {
            (mitte, 0)
        } else if i == 0 && frei(&zeilen[0], ta - ueber - luft - w, ta - ueber - luft) {
            (ta - ueber - luft - w, 0)
        } else if i + 1 == n {
            (tb + ueber + luft, 0)
        } else {
            let k = (1..)
                .find(|&k| zeilen.get(k).is_none_or(|zl| frei(zl, mitte, mitte + w)))
                .unwrap_or(1);
            (mitte, k)
        };
        while zeilen.len() <= zeile {
            zeilen.push(Vec::new());
        }
        zeilen[zeile].push((x0, x0 + w));
        // Grundlinie über der Maßlinie in Leserichtung
        let y = abstand + zeile as f64 * (h + abstand);
        let ursprung = e.p(x0, l) + e.oben * y;
        setzen(
            b,
            e,
            stil,
            ursprung,
            v,
            w,
            Zahlart::Mass {
                seite: id.0,
                kette: id.1,
                i,
            },
            wert,
            z,
        );
    }
}

/// Höhe unter der Breite und Brüstungshöhe vor der Wand.
fn oeffnungen(
    b: &mut Bild,
    e: &Ebene,
    stil: &Stil,
    schrift: &Schrift,
    si: usize,
    s: &Seite,
    k: &Kette,
) {
    let h = stil.am_bau(stil.schrift_mm);
    let abstand = stil.am_bau(stil.text_abstand);
    for (oi, o) in s.oeffnungen.iter().enumerate() {
        let mitte = 0.5 * (o.t0 + o.t1);
        if stil.hoehe {
            let z = masszahl(o.hoehe, stil.rundung);
            let (v, w) = satz(schrift, stil, &z, "");
            // Oberkante der Zahl `abstand` unter der Maßlinie
            let ursprung = e.p(mitte - 0.5 * w, k.lage) - e.oben * (abstand + h);
            setzen(
                b,
                e,
                stil,
                ursprung,
                v,
                w,
                Zahlart::Hoehe { seite: si, o: oi },
                o.hoehe,
                z,
            );
        }
        if let (true, Some(brh)) = (stil.brh, o.brh) {
            let z = masszahl(brh, stil.rundung);
            let (v, w) = satz(schrift, stil, &z, "BRH ");
            // Die wandnahe Kante der Zahl `brh_abstand` vor der Wand
            let vor = o.wand + stil.am_bau(stil.brh_abstand);
            let d = if e.auf() > 0.0 { vor } else { vor + h };
            let ursprung = e.p(mitte - 0.5 * w, d);
            setzen(
                b,
                e,
                stil,
                ursprung,
                v,
                w,
                Zahlart::Brh { seite: si, o: oi },
                brh,
                z,
            );
        }
    }
}

#[cfg(test)]
mod tests;

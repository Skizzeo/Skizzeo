//! Platz zwischen Gebäuden derselben Ebene (Einstellungen zu #116, B2):
//! Keine Kette ragt in den Kettenbereich eines anderen Gebäudes, also in
//! den Streifen von dessen Außenwand bis über seine äußerste Kette. Ist der
//! Platz zu knapp, weicht auf der Seite zuerst das Gesamtmaß, dann die
//! Kette der Vor- und Rücksprünge, dann die Wandkette; die Öffnungskette
//! bleibt. Das Gesamtmaß steht weiter auf der Gegenseite. Eine von Hand
//! verschobene Kette weicht nicht: sie steht, wo man sie hingezogen hat,
//! und ihr Bereich gilt für die Nachbarn.

use crate::aussen::{Aussenmasse, Kette, Seite};
use sk_model::bemassung::{Kettenart, Stil};

/// Ketten eines Geschosses, die weichen: Seite (Grad) und Art, sortiert.
pub type Weg = Vec<(i32, Kettenart)>;

/// Reihenfolge, in der die Ketten einer Seite weichen.
const WEICHT: [Kettenart; 3] = [Kettenart::Gesamt, Kettenart::Sprung, Kettenart::Wand];

/// So weit müssen sich zwei Felder überdecken, damit sie sich stören (mm).
const LUFT: f64 = 1.0;

/// Rechteck in der Ebene: Mitte, Achse längs (`u`, Einheitsvektor), Achse
/// quer (`v`) und halbe Ausdehnungen.
#[derive(Clone, Copy, Debug)]
struct Feld {
    m: [f64; 2],
    u: [f64; 2],
    v: [f64; 2],
    hu: f64,
    hv: f64,
}

impl Feld {
    /// Feld der Seite `s` von `t0` bis `t1` längs und `n0` bis `n1` quer.
    fn neu(s: &Seite, (t0, t1): (f64, f64), (n0, n1): (f64, f64)) -> Feld {
        let (tm, nm) = ((t0 + t1) * 0.5, (n0 + n1) * 0.5);
        Feld {
            m: [s.t.x * tm + s.n.x * nm, s.t.y * tm + s.n.y * nm],
            u: [s.t.x, s.t.y],
            v: [s.n.x, s.n.y],
            hu: (t1 - t0).abs() * 0.5,
            hv: (n1 - n0).abs() * 0.5,
        }
    }

    /// Halbe Breite des Feldes, auf die Achse `a` projiziert.
    fn halb(&self, a: [f64; 2]) -> f64 {
        let d = |w: [f64; 2]| (w[0] * a[0] + w[1] * a[1]).abs();
        self.hu * d(self.u) + self.hv * d(self.v)
    }

    /// Überdecken sich die Felder um mehr als [`LUFT`]? (Trennende Achsen.)
    fn stoert(&self, o: &Feld) -> bool {
        let d = [o.m[0] - self.m[0], o.m[1] - self.m[1]];
        [self.u, self.v, o.u, o.v].into_iter().all(|a| {
            let abstand = (d[0] * a[0] + d[1] * a[1]).abs();
            self.halb(a) + o.halb(a) - abstand > LUFT
        })
    }

    /// Achsparalleler Kasten: x0, y0, x1, y1.
    fn kasten(&self) -> [f64; 4] {
        let (hx, hy) = (self.halb([1.0, 0.0]), self.halb([0.0, 1.0]));
        [
            self.m[0] - hx,
            self.m[1] - hy,
            self.m[0] + hx,
            self.m[1] + hy,
        ]
    }
}

/// Längs-Ausdehnung der Kette (erster bis letzter Bezugspunkt).
fn laenge(k: &Kette) -> (f64, f64) {
    let t = |i: usize| k.bezuege.get(i).map_or(0.0, |b| b.t);
    (t(0), t(k.bezuege.len().saturating_sub(1)))
}

/// Steht die Kette noch (nicht in `weg`)?
fn steht(s: &Seite, k: &Kette, weg: &Weg) -> bool {
    !weg.contains(&(s.grad, k.art))
}

/// Kann die Kette weichen (steht noch und ist nicht von Hand verschoben)?
fn kann(s: &Seite, k: &Kette, weg: &Weg) -> bool {
    steht(s, k, weg) && k.versatz == 0.0
}

/// Streifen der Kette `k`: ihre Länge und um `rand` breiter, quer um die
/// Maßlinie (Zahl, Hilfslinien, Begrenzungen).
fn streifen(s: &Seite, k: &Kette, rand: f64) -> Feld {
    let (t0, t1) = laenge(k);
    Feld::neu(s, (t0 - rand, t1 + rand), (k.lage - rand, k.lage + rand))
}

/// Kettenbereich der Seite: von der Außenwand bis über die äußerste
/// stehende Kette, so lang wie ihre Ketten.
fn bereich(s: &Seite, weg: &Weg, rand: f64) -> Option<Feld> {
    let mut stehend = s.ketten.iter().filter(|k| steht(s, k, weg)).peekable();
    stehend.peek()?;
    let (mut t0, mut t1, mut n1) = (f64::MAX, f64::MIN, s.aussen);
    for k in stehend {
        let (a, b) = laenge(k);
        (t0, t1, n1) = (t0.min(a), t1.max(b), n1.max(k.lage));
    }
    Some(Feld::neu(s, (t0 - rand, t1 + rand), (s.aussen, n1 + rand)))
}

/// Halbe Breite des Streifens einer Kette: der halbe Kettenabstand,
/// mindestens Zahl samt Abstand von der Maßlinie.
fn rand(stil: &Stil) -> f64 {
    (stil.abstand * 0.5).max(stil.am_bau(stil.text_abstand + stil.schrift_mm))
}

/// Ketten, die je Geschoss weichen, damit keine in den Kettenbereich eines
/// anderen Geschosses aus `alle` ragt (alle auf einer Ebene, je Gebäude
/// eines): je Paar von Nachbarn [`zu_zweit`], je Geschoss alles, was bei
/// einem seiner Nachbarn weicht. Weicht mehr, wird kein Bereich größer und
/// keine Kette kommt dazu, es bleibt also frei.
pub fn platz(alle: &[&Aussenmasse], stil: &Stil) -> Vec<Weg> {
    let kaesten: Vec<_> = alle.iter().map(|a| kasten(a, stil)).collect();
    let mut weg = vec![Weg::new(); alle.len()];
    for (i, j) in nachbarn(&kaesten) {
        let [wi, wj] = zu_zweit(alle[i], alle[j], stil);
        dazu(&mut weg[i], &wi);
        dazu(&mut weg[j], &wj);
    }
    weg
}

/// Nimmt `neu` in `w` auf (sortiert, ohne Doppelte).
pub fn dazu(w: &mut Weg, neu: &Weg) {
    w.extend_from_slice(neu);
    w.sort();
    w.dedup();
}

/// Kasten um alle Kettenbereiche des Geschosses, ehe etwas weicht (der
/// größte): x0, y0, x1, y1. Ohne Ketten keiner.
pub fn kasten(a: &Aussenmasse, stil: &Stil) -> Option<[f64; 4]> {
    let r = rand(stil);
    a.seiten
        .iter()
        .filter_map(|s| bereich(s, &Weg::new(), r))
        .map(|f| f.kasten())
        .reduce(|a, q| {
            [
                a[0].min(q[0]),
                a[1].min(q[1]),
                a[2].max(q[2]),
                a[3].max(q[3]),
            ]
        })
}

/// Paare von Geschossen, deren [`kasten`] sich überdecken (nach x sortiert
/// durchlaufen), kleinere Nummer zuerst.
pub fn nachbarn(kaesten: &[Option<[f64; 4]>]) -> Vec<(usize, usize)> {
    let mut k: Vec<(usize, [f64; 4])> = kaesten
        .iter()
        .enumerate()
        .filter_map(|(i, k)| Some((i, (*k)?)))
        .collect();
    k.sort_by(|a, b| a.1[0].total_cmp(&b.1[0]));
    let mut paare = Vec::new();
    for (n, (i, a)) in k.iter().enumerate() {
        for (j, b) in &k[n + 1..] {
            if b[0] > a[2] {
                break;
            }
            if ueberdecken(a, b) {
                paare.push((*i.min(j), *i.max(j)));
            }
        }
    }
    paare
}

/// Wie [`nachbarn`], wenn sich gegenüber den Paaren `paare` nur die Kästen
/// `neu` (Nummern, aufsteigend) geändert haben: Die übrigen Paare bleiben,
/// nur die geänderten Kästen werden gegen alle geprüft (Review L24: beim
/// Ziehen nicht je Mausbewegung alle sortieren). Reihenfolge der Paare wie
/// sie anfallen.
pub fn nachbarn_neu(
    kaesten: &[Option<[f64; 4]>],
    paare: &[(usize, usize)],
    neu: &[usize],
) -> Vec<(usize, usize)> {
    let geaendert = |i: usize| neu.binary_search(&i).is_ok();
    let mut out: Vec<(usize, usize)> = paare
        .iter()
        .copied()
        .filter(|&(i, j)| !geaendert(i) && !geaendert(j))
        .collect();
    for &i in neu {
        let Some(a) = kaesten.get(i).copied().flatten() else {
            continue;
        };
        for (j, b) in kaesten.iter().enumerate() {
            // Zwei geänderte nur einmal, beim kleineren
            if j == i || (j < i && geaendert(j)) {
                continue;
            }
            if b.is_some_and(|b| ueberdecken(&a, &b)) {
                out.push((i.min(j), i.max(j)));
            }
        }
    }
    out
}

/// Überdecken sich die Kästen `a` und `b`: in x mit Rand, in y echt.
fn ueberdecken(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && b[1] < a[3] && a[1] < b[3]
}

/// Was bei zwei Nachbarn `a` und `b` weicht. Je Runde weicht auf jeder
/// Seite, die stört, ihre äußerste weichende Kette, bis nichts mehr stört
/// oder nichts mehr weichen kann. Danach kommt zurück, was ohne neue
/// Überdeckung wieder Platz hat (innen zuerst): Weichen beide Seiten
/// zugleich, braucht es oft nur eine.
pub fn zu_zweit(a: &Aussenmasse, b: &Aussenmasse, stil: &Stil) -> [Weg; 2] {
    let r = rand(stil);
    let alle = [a, b];
    let von = |x: usize, w: &Weg| -> Vec<Feld> {
        alle[x]
            .seiten
            .iter()
            .filter_map(|s| bereich(s, w, r))
            .collect()
    };
    let mut weg = [Weg::new(), Weg::new()];
    let mut bereiche = [von(0, &weg[0]), von(1, &weg[1])];
    let mut folge = Vec::new();
    loop {
        let mut neu: Vec<(usize, i32, Kettenart)> = Vec::new();
        for (x, si, _) in ragen(&alle, &weg, &bereiche, r) {
            let s = &alle[x].seiten[si];
            let weicht = WEICHT.iter().find(|art| {
                s.ketten
                    .iter()
                    .any(|k| k.art == **art && kann(s, k, &weg[x]))
            });
            if let Some(&art) = weicht {
                neu.push((x, s.grad, art));
            }
        }
        neu.dedup();
        if neu.is_empty() {
            break;
        }
        for &(x, grad, art) in &neu {
            weg[x].push((grad, art));
            bereiche[x] = von(x, &weg[x]);
        }
        folge.extend(neu);
    }
    for &(x, grad, art) in folge.iter().rev() {
        let vorher = ragen(&alle, &weg, &bereiche, r);
        weg[x].retain(|e| *e != (grad, art));
        let alt = std::mem::replace(&mut bereiche[x], von(x, &weg[x]));
        let nachher = ragen(&alle, &weg, &bereiche, r);
        if nachher.iter().any(|e| vorher.binary_search(e).is_err()) {
            weg[x].push((grad, art));
            bereiche[x] = alt;
        }
    }
    for w in &mut weg {
        w.sort();
    }
    weg
}

/// Stehende, nicht verschobene Ketten des einen, die in einen
/// Kettenbereich des anderen ragen (Geschoss, Seite, Art), sortiert.
fn ragen(
    alle: &[&Aussenmasse; 2],
    weg: &[Weg; 2],
    bereiche: &[Vec<Feld>; 2],
    r: f64,
) -> Vec<(usize, usize, Kettenart)> {
    let mut v = Vec::new();
    for (x, y) in [(0, 1), (1, 0)] {
        for (si, s) in alle[x].seiten.iter().enumerate() {
            for k in s.ketten.iter().filter(|k| kann(s, k, &weg[x])) {
                let f = streifen(s, k, r);
                if bereiche[y].iter().any(|b| f.stoert(b)) {
                    v.push((x, si, k.art));
                }
            }
        }
    }
    v.sort();
    v.dedup();
    v
}

/// Die Ketten von `a` ohne die weichenden.
pub fn ohne(a: &Aussenmasse, weg: &Weg) -> Aussenmasse {
    let mut a = a.clone();
    for s in &mut a.seiten {
        s.ketten.retain(|k| !weg.contains(&(s.grad, k.art)));
    }
    a
}

#[cfg(test)]
mod tests;

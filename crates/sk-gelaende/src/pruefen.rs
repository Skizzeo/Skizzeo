//! Aufmaß prüfen, bevor das Netz entsteht: doppelte Nummern, zwei Punkte an
//! derselben Stelle, Ausreißer, ferne Punkte (plan-gelaendemodul.md §5 G1,
//! Schritt 6). Das Gelände entsteht trotzdem; jeder Fund wird ein Satz, und
//! jeder Punkt bleibt einzeln an- und abwählbar.

use crate::codes::Codetabelle;
use crate::lesen::{dez, liste, Punkt};

/// Unter diesem Abstand (m) liegen zwei Punkte an derselben Stelle.
pub const GLEICHE_STELLE: f64 = 0.01;
/// Weiter als so weit (m) vom Grundstück entfernt bleibt ein Punkt außen vor.
pub const FERN: f64 = 2000.0;
/// Ausreißer: so weit (m) von den Nachbarn …
pub const AUSREISSER: f64 = 1.0;
/// … und mehr als so oft die örtliche Streuung.
pub const STREUUNG: f64 = 3.0;
/// Höher oder tiefer als so viel (m) ist keine Geländehöhe.
pub const MAX_HOEHE: f64 = 9000.0;
/// Nachbarn für den Ausreißertest.
const NACHBARN: usize = 6;

/// Ergebnis der Prüfung.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pruefung {
    /// Punkt geht ins Gelände.
    pub drin: Vec<bool>,
    /// Ausreißer (bleiben drin, die Liste bietet sie zum Abwählen an).
    pub ausreisser: Vec<usize>,
    pub hinweise: Vec<String>,
}

/// Prüft die Punkte; doppelte Nummern werden umbenannt (der zweite
/// bekommt „-2“).
pub fn pruefen(punkte: &mut [Punkt], codes: &Codetabelle) -> Pruefung {
    let mut hinweise = Vec::new();
    // doppelte Nummern
    let mut umbenannt: Vec<(String, String)> = Vec::new();
    let mut gesehen = std::collections::HashSet::new();
    // alle Nummern der Datei, damit „-2“ keine spätere trifft; je
    // Grundnummer die nächste freie Endung
    let alle: std::collections::HashSet<String> = punkte.iter().map(|p| p.nr.clone()).collect();
    let mut weiter: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for p in punkte.iter_mut() {
        if gesehen.insert(p.nr.clone()) {
            continue;
        }
        let alt = p.nr.clone();
        let k0 = weiter.get(&alt).copied().unwrap_or(2);
        let (k, neu) = (k0..)
            .map(|k| (k, format!("{alt}-{k}")))
            .find(|(_, n)| !gesehen.contains(n) && !alle.contains(n))
            .unwrap_or_default();
        weiter.insert(alt.clone(), k + 1);
        gesehen.insert(neu.clone());
        p.nr = neu.clone();
        umbenannt.push((alt, neu));
    }
    match umbenannt.as_slice() {
        [] => {}
        [(alt, neu)] => hinweise.push(format!(
            "Punktnummer {alt} doppelt, der zweite heißt jetzt {neu}."
        )),
        v => {
            let mut s: Vec<String> = v
                .iter()
                .take(8)
                .map(|(a, n)| format!("{a} → {n}"))
                .collect();
            if v.len() > 8 {
                s.push(format!("{} weitere", v.len() - 8));
            }
            hinweise.push(format!(
                "Punktnummern doppelt, umbenannt: {}.",
                s.join(", ")
            ));
        }
    }
    let mut drin: Vec<bool> = punkte
        .iter()
        .map(|p| codes.bedeutung(&p.code).im_gelaende())
        .collect();
    // unplausible Höhen (Kommafehler, Platzhalter) bleiben außen vor
    let hoch: Vec<String> = punkte
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            !(p.h.is_finite() && p.e.is_finite() && p.n.is_finite()) || p.h.abs() > MAX_HOEHE
        })
        .map(|(i, p)| {
            drin[i] = false;
            p.nr.clone()
        })
        .collect();
    if !hoch.is_empty() {
        hinweise.push(liste(
            "mit unplausibler Höhe (über 9000 m), außen vor",
            "Punkt",
            "Punkte",
            &hoch,
        ));
    }
    // ferne Punkte: weiter als 2 km vom Median
    if !punkte.is_empty() {
        let median = |f: fn(&Punkt) -> f64| {
            let mut v: Vec<f64> = punkte.iter().map(f).collect();
            v.sort_by(f64::total_cmp);
            v[v.len() / 2]
        };
        let (me, mn) = (median(|p| p.e), median(|p| p.n));
        let fern: Vec<String> = punkte
            .iter()
            .enumerate()
            .filter(|(_, p)| (p.e - me).hypot(p.n - mn) > FERN)
            .map(|(i, p)| {
                drin[i] = false;
                p.nr.clone()
            })
            .collect();
        if !fern.is_empty() {
            hinweise.push(liste(
                "liegt über 2 km vom Grundstück entfernt und bleibt außen vor",
                "Punkt",
                "Punkte",
                &fern,
            ));
        }
    }
    // gleiche Stelle: bei gleicher Höhe der spätere, sonst der höhere außen
    // vor, wenn sein Code keine Linie ist
    let ix: Vec<usize> = (0..punkte.len()).filter(|&i| drin[i]).collect();
    let g = Gitter::neu(punkte, &ix, GLEICHE_STELLE);
    for &i in &ix {
        for j in g.nah(punkte, i, GLEICHE_STELLE) {
            if j <= i || !drin[i] || !drin[j] {
                continue;
            }
            let (a, b) = (&punkte[i], &punkte[j]);
            let dh = b.h - a.h;
            if dh.abs() < 0.0005 {
                drin[j] = false;
                hinweise.push(format!("Punkt {} liegt mit gleicher Höhe an derselben Stelle wie {} und bleibt außen vor.", b.nr, a.nr));
                continue;
            }
            let (hoch, tief) = if dh > 0.0 { (j, i) } else { (i, j) };
            if !codes.bedeutung(&punkte[hoch].code).linie() {
                drin[hoch] = false;
                hinweise.push(format!(
                    "Punkt {} liegt an derselben Stelle wie {}, aber {} m höher (Mauer oder Bord?), und bleibt außen vor.",
                    punkte[hoch].nr,
                    punkte[tief].nr,
                    dez(dh.abs(), 2)
                ));
            }
        }
    }
    // Ausreißer gegen die sechs nächsten Nachbarn
    let ix: Vec<usize> = (0..punkte.len()).filter(|&i| drin[i]).collect();
    let mut ausreisser = Vec::new();
    if ix.len() > NACHBARN {
        let g = Gitter::neu(punkte, &ix, 0.0);
        for &i in &ix {
            let nb = g.naechste(punkte, i, NACHBARN);
            let n = nb.len() as f64;
            let mittel = nb.iter().map(|&j| punkte[j].h).sum::<f64>() / n;
            let sigma = (nb
                .iter()
                .map(|&j| (punkte[j].h - mittel).powi(2))
                .sum::<f64>()
                / n)
                .sqrt();
            let d = punkte[i].h - mittel;
            if d.abs() > AUSREISSER && d.abs() > STREUUNG * sigma {
                ausreisser.push(i);
                hinweise.push(if d > 0.0 {
                    format!(
                        "Punkt {} liegt {} m über seinen Nachbarn, Mauerkrone oder Tippfehler?",
                        punkte[i].nr,
                        dez(d, 2)
                    )
                } else {
                    format!(
                        "Punkt {} liegt {} m unter seinen Nachbarn, Schacht oder Tippfehler?",
                        punkte[i].nr,
                        dez(-d, 2)
                    )
                });
            }
        }
    }
    let unbekannt: Vec<String> = codes
        .unbekannt(punkte)
        .into_iter()
        .map(|(c, n)| format!("{c} ({n})"))
        .collect();
    if !unbekannt.is_empty() {
        hinweise.push(format!(
            "Unbekannte Codes zählen als Gelände: {}.",
            unbekannt.join(", ")
        ));
    }
    Pruefung {
        drin,
        ausreisser,
        hinweise,
    }
}

/// Gleichmäßiges Gitter über Punkten für Nachbarsuchen.
struct Gitter {
    x0: f64,
    y0: f64,
    zelle: f64,
    nx: usize,
    ny: usize,
    /// Punkte je Zelle, Zeile für Zeile.
    start: Vec<usize>,
    inhalt: Vec<usize>,
}

impl Gitter {
    /// Gitter über den Punkten `ix`; `zelle` = 0: aus der Punktdichte.
    fn neu(punkte: &[Punkt], ix: &[usize], zelle: f64) -> Gitter {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &i in ix {
            x0 = x0.min(punkte[i].e);
            y0 = y0.min(punkte[i].n);
            x1 = x1.max(punkte[i].e);
            y1 = y1.max(punkte[i].n);
        }
        if ix.is_empty() {
            (x0, y0, x1, y1) = (0.0, 0.0, 0.0, 0.0);
        }
        let flaeche = ((x1 - x0) * (y1 - y0)).max(1e-6);
        let dichte = (flaeche / ix.len().max(1) as f64).sqrt() * 1.5;
        // höchstens etwa 4 Zellen je Punkt, damit das Gitter klein bleibt
        let minimal =
            ((x1 - x0).max(y1 - y0) / (2.0 * (ix.len().max(1) as f64).sqrt() + 1.0)).max(1e-6);
        let zelle = if zelle > 0.0 {
            zelle.max(minimal)
        } else {
            dichte.max(minimal)
        };
        let nx = (((x1 - x0) / zelle) as usize + 1).max(1);
        let ny = (((y1 - y0) / zelle) as usize + 1).max(1);
        let zelle_von = |i: usize| {
            let cx = (((punkte[i].e - x0) / zelle) as usize).min(nx - 1);
            let cy = (((punkte[i].n - y0) / zelle) as usize).min(ny - 1);
            cy * nx + cx
        };
        let mut start = vec![0usize; nx * ny + 1];
        for &i in ix {
            start[zelle_von(i) + 1] += 1;
        }
        for k in 0..nx * ny {
            start[k + 1] += start[k];
        }
        let mut pos = start.clone();
        let mut inhalt = vec![0usize; ix.len()];
        for &i in ix {
            let c = zelle_von(i);
            inhalt[pos[c]] = i;
            pos[c] += 1;
        }
        Gitter {
            x0,
            y0,
            zelle,
            nx,
            ny,
            start,
            inhalt,
        }
    }

    fn zelle(&self, x: f64, y: f64) -> (isize, isize) {
        (
            ((x - self.x0) / self.zelle).floor() as isize,
            ((y - self.y0) / self.zelle).floor() as isize,
        )
    }

    /// Punkte in Zellen mit Chebyshev-Abstand `r` um (cx, cy).
    fn ring(&self, cx: isize, cy: isize, r: isize, mut f: impl FnMut(usize)) {
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                if (x - cx).abs() != r && (y - cy).abs() != r {
                    continue;
                }
                if x < 0 || y < 0 || x as usize >= self.nx || y as usize >= self.ny {
                    continue;
                }
                let c = y as usize * self.nx + x as usize;
                self.inhalt[self.start[c]..self.start[c + 1]]
                    .iter()
                    .for_each(|&i| f(i));
            }
        }
    }

    /// Punkte näher als `d` an Punkt `i` (ohne ihn selbst).
    fn nah(&self, punkte: &[Punkt], i: usize, d: f64) -> Vec<usize> {
        let p = &punkte[i];
        let (cx, cy) = self.zelle(p.e, p.n);
        let r = (d / self.zelle).ceil() as isize;
        let mut v = Vec::new();
        for k in 0..=r {
            self.ring(cx, cy, k, |j| {
                if j != i && (punkte[j].e - p.e).hypot(punkte[j].n - p.n) < d {
                    v.push(j);
                }
            });
        }
        v
    }

    /// Die `k` nächsten Punkte zu Punkt `i` (ohne ihn selbst).
    fn naechste(&self, punkte: &[Punkt], i: usize, k: usize) -> Vec<usize> {
        let p = &punkte[i];
        let (cx, cy) = self.zelle(p.e, p.n);
        let mut best: Vec<(f64, usize)> = Vec::new();
        let rmax = self.nx.max(self.ny) as isize;
        for r in 0..=rmax {
            self.ring(cx, cy, r, |j| {
                if j != i {
                    best.push(((punkte[j].e - p.e).hypot(punkte[j].n - p.n), j));
                }
            });
            best.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            best.truncate(k);
            // alles jenseits des Rings r liegt weiter als r · zelle
            if best.len() == k && best[k - 1].0 <= r as f64 * self.zelle {
                break;
            }
        }
        best.into_iter().map(|x| x.1).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(nr: &str, e: f64, n: f64, h: f64, code: &str) -> Punkt {
        Punkt {
            nr: nr.into(),
            e,
            n,
            h,
            code: code.into(),
            text: String::new(),
            zeile: 0,
        }
    }

    fn feld() -> Vec<Punkt> {
        let mut v = Vec::new();
        for i in 0..10 {
            for j in 0..10 {
                let (x, y) = (i as f64 * 4.0, j as f64 * 4.0);
                v.push(p(
                    &(i * 10 + j + 1).to_string(),
                    x,
                    y,
                    24.0 + 0.02 * x + 0.01 * y,
                    "GP",
                ));
            }
        }
        v
    }

    #[test]
    fn sauberes_feld_ohne_hinweis() {
        let mut v = feld();
        let r = pruefen(&mut v, &Codetabelle::default());
        assert!(r.hinweise.is_empty(), "{:?}", r.hinweise);
        assert!(r.drin.iter().all(|&d| d));
    }

    #[test]
    fn viele_doppelte_nummern() {
        // 20.000 Punkte, je zehn mit derselben Nummer; „5-2“ steht schon in
        // der Datei und wird übersprungen
        let mut v: Vec<Punkt> = (0..20_000)
            .map(|i| {
                p(
                    &(i / 10).to_string(),
                    (i % 200) as f64,
                    (i / 200) as f64,
                    24.0,
                    "GP",
                )
            })
            .collect();
        v[19_999].nr = "5-2".into();
        let t = std::time::Instant::now();
        let r = pruefen(&mut v, &Codetabelle::default());
        assert!(t.elapsed().as_secs_f64() < 5.0);
        assert_eq!(
            [&v[50].nr, &v[51].nr, &v[52].nr, &v[59].nr],
            ["5", "5-3", "5-4", "5-11"]
        );
        let alle: std::collections::HashSet<&str> = v.iter().map(|p| p.nr.as_str()).collect();
        assert_eq!(alle.len(), v.len());
        assert!(r.hinweise[0].starts_with("Punktnummern doppelt, umbenannt: 0 → 0-2,"));
    }

    #[test]
    fn unplausible_hoehen_bleiben_aussen_vor() {
        let mut v = feld();
        v[3].h = 2_434_500.0; // Kommafehler
        v[7].h = f64::NAN;
        v[9].h = -9000.5;
        let r = pruefen(&mut v, &Codetabelle::default());
        assert!(!r.drin[3] && !r.drin[7] && !r.drin[9] && r.drin[4]);
        assert_eq!(
            r.hinweise[0],
            "Punkte 4, 8 und 10 mit unplausibler Höhe (über 9000 m), außen vor."
        );
    }

    #[test]
    fn funde() {
        let mut v = feld();
        v[44].h += 2.1; // Ausreißer
        v.push(p("12", 50.0, 50.0, 25.0, "GP")); // doppelte Nummer
        v.push(p("200", 8.004, 8.0, 24.65, "GP")); // Mauer auf Punkt 23 (8, 8)
        v.push(p("201", 12.0, 12.003, 24.36, "GP")); // gleiche Höhe wie 34
        v.push(p("202", 28.0, 28.002, 25.3, "BO")); // höher als 78, aber Linie: bleibt
        v.push(p("203", 5000.0, 0.0, 24.0, "GP")); // fern
        v.push(p("204", 1.0, 30.0, 24.5, "ZZ"));
        let r = pruefen(&mut v, &Codetabelle::default());
        let n = v.len();
        assert_eq!(v[n - 6].nr, "12-2");
        assert!(!r.drin[n - 5] && !r.drin[n - 4] && r.drin[n - 3] && !r.drin[n - 2]);
        assert_eq!(r.ausreisser, [44]);
        assert_eq!(
            r.hinweise,
            [
                "Punktnummer 12 doppelt, der zweite heißt jetzt 12-2.",
                "Punkt 203 liegt über 2 km vom Grundstück entfernt und bleibt außen vor.",
                "Punkt 200 liegt an derselben Stelle wie 23, aber 0,41 m höher (Mauer oder Bord?), und bleibt außen vor.",
                "Punkt 201 liegt mit gleicher Höhe an derselben Stelle wie 34 und bleibt außen vor.",
                "Punkt 45 liegt 2,13 m über seinen Nachbarn, Mauerkrone oder Tippfehler?",
                "Unbekannte Codes zählen als Gelände: ZZ (1).",
            ]
        );
    }
}

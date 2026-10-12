//! Lage des Lageplans (plan-gelaendemodul.md §5 G1c, „Lage ohne Klicken“):
//! Gitterkreuze mit Rahmenkoordinaten ergeben Zone, Maßstab und Lage;
//! sonst gilt der Maßstab aus dem Kopf („1:500“, Norden oben); sonst passt
//! man zwei Punkte ein.

use crate::seite::{punkt, Matrix, Seite, Text};

/// Meter je pt bei Maßstab 1:1.
pub const PT: f64 = 0.0254 / 72.0;

/// Einpassung pt → ETRS89/UTM (E, N) in m.
#[derive(Clone, Debug, PartialEq)]
pub struct Einpassung {
    /// Affine Matrix wie im PDF: E = a x + c y + e, N = b x + d y + f.
    pub matrix: Matrix,
    /// UTM-Zone, wenn die Rahmenbeschriftung sie nennt.
    pub zone: Option<u8>,
    /// Gemessener Maßstab (500,0 heißt 1:500).
    pub massstab: f64,
    /// Drehung der Blatt-x-Achse gegen Ost in Grad, gegen den Uhrzeigersinn.
    pub drehung: f64,
    /// Mittlerer Restfehler an den Passpunkten in m.
    pub restfehler: f64,
    /// Größte Abweichung an einem Passpunkt in m.
    pub groesster: f64,
    /// Zahl der Passpunkte.
    pub punkte: usize,
}

/// Wie die Lage gefunden wurde.
#[derive(Clone, Debug, PartialEq)]
pub enum Lage {
    /// Gitterkreuze und Rahmenkoordinaten: Lage, Maßstab und Zone stehen.
    Gitter(Einpassung),
    /// Nur der Maßstab aus dem Kopf, Norden oben; die Lage fehlt (ein Klick
    /// auf einen Grenzpunkt oder zwei Punkte einpassen).
    Massstab(f64),
    /// Nichts erkannt: zwei Punkte einpassen.
    Offen,
}

/// Ein Wert der Rahmenbeschriftung („E = 32512120“).
#[derive(Clone, Debug, PartialEq)]
pub struct Rahmenwert {
    /// 'E' (Ost) oder 'N' (Nord).
    pub achse: char,
    /// Wert in m, ohne Zonenkennzahl.
    pub wert: f64,
    pub zone: Option<u8>,
    /// Mitte des Textes in pt.
    pub lage: [f64; 2],
}

impl Einpassung {
    /// Punkt in pt nach UTM.
    pub fn utm(&self, p: [f64; 2]) -> [f64; 2] {
        punkt(&self.matrix, p[0], p[1])
    }

    /// Abstand in m zwischen dem eingepassten `p` und dem Sollwert `u`
    /// (die Abweichung am dritten Punkt im Dialog).
    pub fn abweichung(&self, p: [f64; 2], u: [f64; 2]) -> f64 {
        let q = self.utm(p);
        (q[0] - u[0]).hypot(q[1] - u[1])
    }

    fn aus(matrix: Matrix, paare: &[([f64; 2], [f64; 2])], zone: Option<u8>) -> Einpassung {
        let det = matrix[0] * matrix[3] - matrix[1] * matrix[2];
        let mut e = Einpassung {
            matrix,
            zone,
            massstab: det.abs().sqrt() / PT,
            drehung: matrix[1].atan2(matrix[0]).to_degrees(),
            restfehler: 0.0,
            groesster: 0.0,
            punkte: paare.len(),
        };
        let r: Vec<f64> = paare.iter().map(|(p, u)| e.abweichung(*p, *u)).collect();
        e.groesster = r.iter().fold(0.0, |a, &b| a.max(b));
        e.restfehler = (r.iter().map(|x| x * x).sum::<f64>() / r.len().max(1) as f64).sqrt();
        e
    }

    /// Ähnlichkeit (Maßstab, Drehung, Verschiebung) aus zwei oder mehr
    /// Punktpaaren pt ↔ UTM, ausgeglichen nach Helmert.
    pub fn aehnlich(paare: &[([f64; 2], [f64; 2])]) -> Option<Einpassung> {
        if paare.len() < 2 {
            return None;
        }
        let n = paare.len() as f64;
        let mp = paare
            .iter()
            .fold([0.0; 2], |s, (p, _)| [s[0] + p[0] / n, s[1] + p[1] / n]);
        let mu = paare
            .iter()
            .fold([0.0; 2], |s, (_, u)| [s[0] + u[0] / n, s[1] + u[1] / n]);
        let (mut q, mut a, mut b) = (0.0, 0.0, 0.0);
        for (p, u) in paare {
            let (px, py) = (p[0] - mp[0], p[1] - mp[1]);
            let (ux, uy) = (u[0] - mu[0], u[1] - mu[1]);
            q += px * px + py * py;
            a += px * ux + py * uy;
            b += px * uy - py * ux;
        }
        if q < 1e-12 {
            return None;
        }
        let (a, b) = (a / q, b / q);
        let m = [
            a,
            b,
            -b,
            a,
            mu[0] - a * mp[0] + b * mp[1],
            mu[1] - b * mp[0] - a * mp[1],
        ];
        Some(Einpassung::aus(m, paare, None))
    }

    /// Zwei Punkte einpassen (zwei Grenzpunkte aus dem Aufmaß oder zwei
    /// getippte Koordinaten).
    pub fn zwei_punkte(
        p1: [f64; 2],
        u1: [f64; 2],
        p2: [f64; 2],
        u2: [f64; 2],
    ) -> Option<Einpassung> {
        Einpassung::aehnlich(&[(p1, u1), (p2, u2)])
    }

    /// Maßstab bekannt, Norden oben, ein Punkt legt die Lage fest.
    pub fn ein_punkt(massstab: f64, p: [f64; 2], u: [f64; 2]) -> Einpassung {
        let s = massstab * PT;
        let m = [s, 0.0, 0.0, s, u[0] - s * p[0], u[1] - s * p[1]];
        Einpassung::aus(m, &[(p, u)], None)
    }

    /// Affine Abbildung aus drei oder mehr Punktpaaren, kleinste Quadrate.
    pub fn affin(paare: &[([f64; 2], [f64; 2])], zone: Option<u8>) -> Option<Einpassung> {
        if paare.len() < 3 {
            return None;
        }
        let n = paare.len() as f64;
        let mp = paare
            .iter()
            .fold([0.0; 2], |s, (p, _)| [s[0] + p[0] / n, s[1] + p[1] / n]);
        let mu = paare
            .iter()
            .fold([0.0; 2], |s, (_, u)| [s[0] + u[0] / n, s[1] + u[1] / n]);
        let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
        let (mut sxe, mut sye, mut sxn, mut syn) = (0.0, 0.0, 0.0, 0.0);
        for (p, u) in paare {
            let (x, y) = (p[0] - mp[0], p[1] - mp[1]);
            let (e, nn) = (u[0] - mu[0], u[1] - mu[1]);
            sxx += x * x;
            sxy += x * y;
            syy += y * y;
            sxe += x * e;
            sye += y * e;
            sxn += x * nn;
            syn += y * nn;
        }
        let det = sxx * syy - sxy * sxy;
        if det.abs() < 1e-9 * (sxx * syy).max(1e-12) {
            return None;
        }
        let a = (sxe * syy - sye * sxy) / det;
        let c = (sye * sxx - sxe * sxy) / det;
        let b = (sxn * syy - syn * sxy) / det;
        let d = (syn * sxx - sxn * sxy) / det;
        let m = [
            a,
            b,
            c,
            d,
            mu[0] - a * mp[0] - c * mp[1],
            mu[1] - b * mp[0] - d * mp[1],
        ];
        Some(Einpassung::aus(m, paare, zone))
    }
}

/// Maßstabszahl aus einem Text wie „Maßstab 1:500“, „M 1 : 1.000“.
pub fn massstab_in(t: &str) -> Option<f64> {
    let b: Vec<char> = t.chars().collect();
    for i in 0..b.len() {
        if b[i] != '1' || (i > 0 && b[i - 1].is_ascii_digit()) {
            continue;
        }
        let mut j = i + 1;
        while b.get(j) == Some(&' ') {
            j += 1;
        }
        if b.get(j) != Some(&':') {
            continue;
        }
        j += 1;
        while b.get(j) == Some(&' ') {
            j += 1;
        }
        let mut z = String::new();
        while let Some(&c) = b.get(j) {
            if c.is_ascii_digit() {
                z.push(c);
            } else if !(c == '.' && b.get(j + 1).is_some_and(char::is_ascii_digit)) {
                break;
            }
            j += 1;
        }
        if let Ok(m) = z.parse::<f64>() {
            if (50.0..=100_000.0).contains(&m) {
                return Some(m);
            }
        }
    }
    None
}

/// Maßstab aus den Texten der Seite.
pub fn massstab(texte: &[Text]) -> Option<f64> {
    texte.iter().find_map(|t| massstab_in(&t.text))
}

/// Mitte eines Textes (halbe Höhe über der Grundlinie).
pub fn mitte(t: &Text) -> [f64; 2] {
    let (s, c) = t.winkel.to_radians().sin_cos();
    let h = 0.35 * t.groesse;
    [
        (t.lage[0] + t.ende[0]) / 2.0 - s * h,
        (t.lage[1] + t.ende[1]) / 2.0 + c * h,
    ]
}

/// Rahmenwerte „E = 32512120“ und „N = 5771155“ (auch ohne Leerzeichen,
/// mit Doppelpunkt oder Tausenderpunkten).
pub fn rahmenwerte(texte: &[Text]) -> Vec<Rahmenwert> {
    let mut aus = Vec::new();
    for t in texte {
        let s: String = t.text.chars().filter(|c| !c.is_whitespace()).collect();
        let mut c = s.chars();
        let achse = match c.next() {
            Some('E') | Some('O') | Some('R') => 'E',
            Some('N') | Some('H') => 'N',
            _ => continue,
        };
        let rest: String = c.collect();
        let Some(zahl) = rest.strip_prefix('=').or_else(|| rest.strip_prefix(':')) else {
            continue;
        };
        let ziffern: String = zahl.chars().filter(|&c| c != '.').collect();
        if ziffern.is_empty() || !ziffern.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(v) = ziffern.parse::<f64>() else {
            continue;
        };
        // Ostwert mit Zonenkennzahl: 8 Stellen, die ersten zwei sind die Zone
        let (wert, zone) = if achse == 'E' && ziffern.len() == 8 {
            (v % 1_000_000.0, Some((v / 1_000_000.0) as u8))
        } else {
            (v, None)
        };
        aus.push(Rahmenwert {
            achse,
            wert,
            zone,
            lage: mitte(t),
        });
    }
    aus
}

/// Gerade Strecken dünner Striche (höchstens 0,3 pt), ohne Kurven.
fn duenne_strecken(seite: &Seite) -> Vec<[[f64; 2]; 2]> {
    let mut aus = Vec::new();
    for p in &seite.pfade {
        if p.kurven || !p.strich.is_some_and(|(_, b)| b <= 0.3) {
            continue;
        }
        for z in &p.zuege {
            for w in z.punkte.windows(2) {
                aus.push([w[0], w[1]]);
            }
        }
    }
    aus.truncate(20_000);
    aus
}

fn laenge(s: &[[f64; 2]; 2]) -> f64 {
    (s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1])
}

/// Kollineare Strecken mit gemeinsamem Ende zusammenfassen (ein Kreuzarm
/// aus zwei Hälften wird ein Strich).
fn verbinden(mut s: Vec<[[f64; 2]; 2]>) -> Vec<[[f64; 2]; 2]> {
    let nah = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 0.02;
    'aussen: loop {
        for i in 0..s.len() {
            for j in i + 1..s.len() {
                let (a, b) = (s[i], s[j]);
                let (la, lb) = (laenge(&a), laenge(&b));
                if la < 1e-6 || lb < 1e-6 || la > 60.0 || lb > 60.0 {
                    continue;
                }
                let da = [a[1][0] - a[0][0], a[1][1] - a[0][1]];
                let db = [b[1][0] - b[0][0], b[1][1] - b[0][1]];
                if (da[0] * db[1] - da[1] * db[0]).abs() > 0.01 * la * lb {
                    continue;
                }
                for (k, l) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                    if nah(a[k], b[l]) {
                        let neu = [a[1 - k], b[1 - l]];
                        if laenge(&neu) > la.max(lb) {
                            s[i] = neu;
                            s.swap_remove(j);
                            continue 'aussen;
                        }
                    }
                }
            }
        }
        return s;
    }
}

/// Gitterkreuze: zwei dünne, gleich lange Striche (4 bis 60 pt), die sich
/// rechtwinklig in ihrer Mitte kreuzen. Gibt die Mitten.
pub fn gitterkreuze(seite: &Seite) -> Vec<[f64; 2]> {
    let s: Vec<[[f64; 2]; 2]> = verbinden(duenne_strecken(seite))
        .into_iter()
        .filter(|s| (4.0..=60.0).contains(&laenge(s)))
        .collect();
    let mut aus: Vec<[f64; 2]> = Vec::new();
    for i in 0..s.len() {
        for j in i + 1..s.len() {
            let (a, b) = (s[i], s[j]);
            let (la, lb) = (laenge(&a), laenge(&b));
            if (la - lb).abs() > 0.05 * la.max(lb) {
                continue;
            }
            let da = [(a[1][0] - a[0][0]) / la, (a[1][1] - a[0][1]) / la];
            let db = [(b[1][0] - b[0][0]) / lb, (b[1][1] - b[0][1]) / lb];
            if (da[0] * db[0] + da[1] * db[1]).abs() > 0.02 {
                continue;
            }
            let ma = [(a[0][0] + a[1][0]) / 2.0, (a[0][1] + a[1][1]) / 2.0];
            let mb = [(b[0][0] + b[1][0]) / 2.0, (b[0][1] + b[1][1]) / 2.0];
            if (ma[0] - mb[0]).hypot(ma[1] - mb[1]) > 0.05 * la {
                continue;
            }
            let m = [(ma[0] + mb[0]) / 2.0, (ma[1] + mb[1]) / 2.0];
            if !aus.iter().any(|q| (q[0] - m[0]).hypot(q[1] - m[1]) < 1.0) {
                aus.push(m);
            }
        }
    }
    aus
}

/// Rundes Rastermaß in m, das zu `d` passt (auf 10 %).
fn raster(d: f64) -> Option<f64> {
    const RUND: [f64; 15] = [
        1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0, 2500.0,
        5000.0,
    ];
    RUND.into_iter().find(|&r| (d / r - 1.0).abs() < 0.1)
}

/// Lage aus Gitterkreuzen und Rahmenwerten. Die Rahmenwerte (auf volle
/// Meter gerundet) sagen nur, welches Kreuz welcher Rasterwert ist; die
/// Lage kommt aus den Kreuzen, ausgeglichen nach Helmert (eine Karte ist
/// maßstabstreu, der Restfehler sagt, wie gut die Kreuze dazu passen; frei
/// affin gingen sechs Kreuze immer fast ohne Rest auf). `rahmen` (x0, y0,
/// x1, y1) macht die Zuordnung der Rahmenwerte genauer; `massstab` aus dem
/// Kopf gibt den groben Maßstab, sonst kommt er aus zwei Rahmenwerten.
pub fn gitter(
    seite: &Seite,
    rahmen: Option<[f64; 4]>,
    massstab: Option<f64>,
) -> Option<Einpassung> {
    let kreuze = gitterkreuze(seite);
    let werte = rahmenwerte(&seite.texte);
    if kreuze.len() < 3 {
        return None;
    }
    // Rahmenwert an die nächste Rahmenkante (E an eine senkrechte, N an eine waagerechte)
    let stelle = |w: &Rahmenwert| -> f64 {
        let (v, kanten) = match (w.achse, rahmen) {
            ('E', Some(r)) => (w.lage[0], [r[0], r[2]]),
            ('N', Some(r)) => (w.lage[1], [r[1], r[3]]),
            ('E', None) => return w.lage[0],
            _ => return w.lage[1],
        };
        let k = if (v - kanten[0]).abs() < (v - kanten[1]).abs() {
            kanten[0]
        } else {
            kanten[1]
        };
        if (v - k).abs() < 40.0 {
            k
        } else {
            v
        }
    };
    let e: Vec<(f64, f64)> = werte
        .iter()
        .filter(|w| w.achse == 'E')
        .map(|w| (stelle(w), w.wert))
        .collect();
    let n: Vec<(f64, f64)> = werte
        .iter()
        .filter(|w| w.achse == 'N')
        .map(|w| (stelle(w), w.wert))
        .collect();
    let (&(xe, ve), &(yn, vn)) = (e.first()?, n.first()?);
    // grober Maßstab in m je pt
    let aus_zwei = |v: &[(f64, f64)]| {
        v.iter()
            .flat_map(|a| v.iter().map(move |b| (a, b)))
            .find(|(a, b)| (a.0 - b.0).abs() > 50.0)
            .map(|(a, b)| (b.1 - a.1) / (b.0 - a.0))
    };
    let s = massstab
        .map(|m| m * PT)
        .or_else(|| aus_zwei(&e))
        .or_else(|| aus_zwei(&n))?;
    if s <= 0.0 {
        return None;
    }
    let grob = |p: [f64; 2]| [ve + (p[0] - xe) * s, vn + (p[1] - yn) * s];
    // Rastermaß aus dem kleinsten Abstand der Kreuze in einer Richtung
    let mut d = f64::MAX;
    for a in &kreuze {
        for b in &kreuze {
            for k in 0..2 {
                let x = (a[k] - b[k]).abs() * s;
                if x > 1.0 {
                    d = d.min(x);
                }
            }
        }
    }
    let r = raster(d)?;
    let mut paare = Vec::new();
    for k in &kreuze {
        let g = grob(*k);
        let soll = [(g[0] / r).round() * r, (g[1] / r).round() * r];
        if (g[0] - soll[0]).abs() > r / 3.0 || (g[1] - soll[1]).abs() > r / 3.0 {
            return None;
        }
        paare.push((*k, soll));
    }
    let zone = werte.iter().find_map(|w| w.zone);
    let mut e = Einpassung::aehnlich(&paare)?;
    e.zone = zone;
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seite::{Pfad, Zug};

    fn text(t: &str, lage: [f64; 2], winkel: f64) -> Text {
        let (s, c) = winkel.to_radians().sin_cos();
        let l = t.len() as f64 * 3.0;
        Text {
            text: t.into(),
            lage,
            ende: [lage[0] + c * l, lage[1] + s * l],
            groesse: 6.0,
            winkel,
            schrift: String::new(),
            farbe: [0; 3],
        }
    }

    fn strich(a: [f64; 2], b: [f64; 2]) -> Pfad {
        Pfad {
            zuege: vec![Zug {
                punkte: vec![a, b],
                zu: false,
            }],
            strich: Some(([0; 3], 0.0)),
            fuellung: None,
            gerade_ungerade: false,
            kurven: false,
        }
    }

    #[test]
    fn massstab_und_rahmenwerte() {
        assert_eq!(massstab_in("Lageplan 1:500"), Some(500.0));
        assert_eq!(massstab_in("M 1 : 1.000"), Some(1000.0));
        assert_eq!(
            massstab_in("Maßstab 1:500 · ETRS89/UTM Zone 32"),
            Some(500.0)
        );
        assert_eq!(massstab_in("11:30 Uhr"), None);
        assert_eq!(massstab_in("1:5"), None);
        let w = rahmenwerte(&[
            text("E = 32512120", [0.0, 0.0], -90.0),
            text("N=5771155", [0.0, 0.0], 0.0),
            text("Eingang", [0.0, 0.0], 0.0),
            text("E = 12a", [0.0, 0.0], 0.0),
        ]);
        assert_eq!(w.len(), 2);
        assert_eq!(
            (w[0].achse, w[0].wert, w[0].zone),
            ('E', 512120.0, Some(32))
        );
        assert_eq!((w[1].achse, w[1].wert, w[1].zone), ('N', 5771155.0, None));
    }

    #[test]
    fn kreuze_im_raster_ergeben_die_lage() {
        // Raster 50 m bei 1:500, leicht gedreht; ein Arm aus zwei Hälften
        let s = 500.0 * PT;
        let (sd, cd) = 0.3f64.to_radians().sin_cos();
        let blatt = |e: f64, n: f64| {
            let (x, y) = ((e - 512_000.0) / s, (n - 5_771_000.0) / s);
            [100.0 + cd * x + sd * y, 100.0 - sd * x + cd * y]
        };
        let mut seite = Seite::default();
        for e in [512_050.0, 512_100.0] {
            for n in [5_771_050.0, 5_771_100.0, 5_771_150.0] {
                let m = blatt(e, n);
                let a = 7.0;
                seite.pfade.push(strich([m[0], m[1] - a], [m[0], m[1] + a]));
                seite.pfade.push(strich([m[0] - a, m[1]], m));
                seite.pfade.push(strich(m, [m[0] + a, m[1]]));
            }
        }
        // kleines Messpunktkreuz und ein schräger Strich stören nicht
        seite.pfade.push(strich([10.0, 10.0], [11.8, 10.0]));
        seite.pfade.push(strich([10.9, 9.1], [10.9, 10.9]));
        let r = blatt(512_030.0, 5_771_045.0);
        seite
            .texte
            .push(text("E = 32512030", [r[0] - 10.0, r[1] + 40.0], -90.0));
        seite
            .texte
            .push(text("N = 5771045", [r[0], r[1] - 9.0], 0.0));
        assert_eq!(gitterkreuze(&seite).len(), 6);
        let e = gitter(&seite, None, Some(500.0)).unwrap();
        assert_eq!((e.zone, e.punkte), (Some(32), 6));
        assert!((e.massstab - 500.0).abs() < 1e-6, "{}", e.massstab);
        assert!((e.drehung - 0.3).abs() < 1e-6, "{}", e.drehung);
        assert!(e.groesster < 1e-6);
        let p = blatt(512_058.40, 5_771_071.20);
        assert!(e.abweichung(p, [512_058.40, 5_771_071.20]) < 1e-6);
        // ohne Kopf-Maßstab und mit nur einem E-Wert geht es nicht
        assert_eq!(gitter(&seite, None, None), None);
    }

    #[test]
    fn zwei_punkte_einpassen() {
        let u1 = [467_108.214, 5_878_741.637];
        let u3 = [467_128.548, 5_878_777.885];
        let e = Einpassung::zwei_punkte([237.73, 374.18], u1, [353.01, 579.68], u3).unwrap();
        assert!((e.massstab - 500.0).abs() < 0.5, "{}", e.massstab);
        assert!(e.drehung.abs() < 0.05);
        assert!(e.abweichung([372.78, 390.76], [467_132.035, 5_878_744.562]) < 0.02);
        assert_eq!(Einpassung::zwei_punkte(u1, u1, u1, u1), None);
        let k = Einpassung::ein_punkt(500.0, [0.0, 0.0], u1);
        assert!(k.abweichung([100.0, 0.0], [u1[0] + 100.0 * 500.0 * PT, u1[1]]) < 1e-9);
    }
}

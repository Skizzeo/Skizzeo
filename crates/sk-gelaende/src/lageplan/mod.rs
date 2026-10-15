//! Lageplan oder Liegenschaftskarte auswerten (plan-gelaendemodul.md §5
//! G1c und §4.5): Lage finden, Linien nach Farbe und Breite ordnen,
//! Grenzpunkte und das Flurstück zur Nummer finden, und was als Unterlage
//! gezeigt wird (ohne Rahmen, Kopf, Fußzeile, Gitterkreuze und Wappen).
//!
//! Gerechnet wird auf dem Blatt in pt; erst [`lage::Einpassung`] bringt
//! Punkte nach ETRS89/UTM in m.

pub mod flaeche;
pub mod lage;

use crate::pdf::Pdf;
use crate::seite::{seite, Farbe, Pfad, Seite, Text};
use lage::Lage;

/// Was eine Linie im Lageplan ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bedeutung {
    /// Flurstücksgrenze (und die Bruchstriche der Nummern).
    Grenze,
    /// Kreis eines Grenzpunkts.
    Grenzpunkt,
    Gebaeude,
    /// Nutzungsgrenze oder Nutzungsfläche.
    Nutzung,
    Gewaesser,
    /// Rahmen und Maßstabsleiste.
    Rahmen,
    /// Gitterkreuz.
    Gitter,
    Sonstiges,
}

/// Erzeuger, für den Skizzeo eine Tabelle hat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Erzeuger {
    /// AED-SICAD (ALKIS-Auskunft).
    AedSicad,
    Unbekannt,
}

/// Linienart: Strichfarbe und Breite in pt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Linienart {
    pub farbe: Farbe,
    pub breite: f64,
}

/// Zuordnung einer Linienart, wie der Dialog sie zeigt.
#[derive(Clone, Debug, PartialEq)]
pub struct Zuordnung {
    pub art: Linienart,
    pub bedeutung: Bedeutung,
    /// Zahl der Pfade dieser Art.
    pub anzahl: usize,
}

/// Ein Grenzpunkt auf dem Blatt.
#[derive(Clone, Debug, PartialEq)]
pub struct Grenzpunkt {
    /// Mitte in pt.
    pub lage: [f64; 2],
    /// Durchmesser des Kreises in pt.
    pub durchmesser: f64,
    /// Nummer, wenn eine danebensteht.
    pub nr: Option<String>,
}

/// Ein gefundenes Flurstück.
#[derive(Clone, Debug, PartialEq)]
pub struct Flurstueck {
    pub nummer: String,
    /// Stelle der Nummer in der Karte (pt).
    pub lage: [f64; 2],
    /// Ecken in pt, gegen den Uhrzeigersinn.
    pub ecken_pt: Vec<[f64; 2]>,
    /// Ecken in ETRS89/UTM (E, N) in m, wenn die Lage bekannt ist.
    pub ecken: Option<Vec<[f64; 2]>>,
    /// Fläche aus dem Lageplan in m², wenn der Maßstab bekannt ist.
    pub flaeche: Option<f64>,
}

/// Was als Unterlage gezeigt wird.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Unterlage {
    pub pfade: Vec<(Pfad, Bedeutung)>,
    pub texte: Vec<Text>,
}

/// Der ausgewertete Lageplan.
#[derive(Clone, Debug, PartialEq)]
pub struct Lageplan {
    pub seite: Seite,
    pub erzeuger: Erzeuger,
    /// Bedeutung je Pfad der Seite (gleiche Reihenfolge).
    pub bedeutungen: Vec<Bedeutung>,
    /// Zuordnung je Linienart für den Dialog.
    pub zuordnung: Vec<Zuordnung>,
    /// Kartenrahmen x0, y0, x1, y1 in pt.
    pub rahmen: Option<[f64; 4]>,
    pub lage: Lage,
    pub grenzpunkte: Vec<Grenzpunkt>,
    /// Flurstücksnummer aus dem Kopf.
    pub nummer: Option<String>,
    /// Das Flurstück zur Nummer aus dem Kopf.
    pub flurstueck: Option<Flurstueck>,
    pub hinweise: Vec<String>,
}

fn gleich(a: Farbe, b: Farbe) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 8)
}

/// Weiß oder fast weiß (Abdeckungen am Blattrand).
fn hell(f: Farbe) -> bool {
    f.iter().all(|&x| x >= 250)
}

fn schwarz(f: Farbe) -> bool {
    f.iter().all(|&x| x <= 64)
}

fn kasten(p: &Pfad) -> [f64; 4] {
    let mut k = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for q in p.zuege.iter().flat_map(|z| &z.punkte) {
        k = [
            k[0].min(q[0]),
            k[1].min(q[1]),
            k[2].max(q[0]),
            k[3].max(q[1]),
        ];
    }
    k
}

/// Kleiner Kreis (1 bis 4 mm): geschlossener Kurvenzug, alle Punkte gleich
/// weit von der Mitte. Gibt Mitte und Durchmesser.
fn kreis(p: &Pfad) -> Option<([f64; 2], f64)> {
    if !p.kurven || p.zuege.len() != 1 || p.zuege[0].punkte.len() < 8 {
        return None;
    }
    let k = kasten(p);
    let m = [(k[0] + k[2]) / 2.0, (k[1] + k[3]) / 2.0];
    let d = (k[2] - k[0]).max(k[3] - k[1]);
    if !(2.8..=11.4).contains(&d) || ((k[2] - k[0]) - (k[3] - k[1])).abs() > 0.1 * d {
        return None;
    }
    let rund = p.zuege[0]
        .punkte
        .iter()
        .all(|q| ((q[0] - m[0]).hypot(q[1] - m[1]) - d / 2.0).abs() < 0.08 * d);
    rund.then_some((m, d))
}

/// Bedeutung nach der Linientabelle für AED-SICAD (ALKIS-Auskunft).
fn aed_sicad(p: &Pfad) -> Bedeutung {
    // Breiten wie in der Datei auf 0,006 pt (Nordpfeil 1,99 und 1,04 pt ist keine Grenze und kein Gebäude)
    let nah = |b: f64, soll: f64| (b - soll).abs() < 0.006;
    match (p.strich, p.fuellung) {
        (Some(_), _) if kreis(p).is_some() => Bedeutung::Grenzpunkt,
        (Some((f, b)), _) if schwarz(f) && nah(b, 1.98) => Bedeutung::Grenze,
        (Some((f, b)), _) if schwarz(f) && b < 0.05 => Bedeutung::Gitter,
        (Some((f, b)), _) if schwarz(f) && nah(b, 0.288) => Bedeutung::Rahmen,
        (Some((f, _)), _) if gleich(f, [0, 104, 160]) => Bedeutung::Gewaesser,
        (Some((f, b)), _) if gleich(f, [178, 178, 178]) && nah(b, 1.008) => Bedeutung::Nutzung,
        (Some((f, b)), None) if schwarz(f) && nah(b, 1.008) => Bedeutung::Gebaeude,
        (None, Some(f)) if gleich(f, [178, 178, 178]) || gleich(f, [229, 229, 229]) => {
            Bedeutung::Gebaeude
        }
        (None, Some(f)) if hell(f) => Bedeutung::Sonstiges,
        (None, Some(f)) if gleich(f, [191, 232, 249]) => Bedeutung::Gewaesser,
        (None, Some(_)) => Bedeutung::Nutzung,
        _ => Bedeutung::Sonstiges,
    }
}

/// Ist der Pfad ein Arm eines Gitterkreuzes (dünner, kurzer Strich mit
/// einem Ende nahe der Kreuzmitte)?
fn am_kreuz(p: &Pfad, kreuze: &[[f64; 2]]) -> bool {
    p.strich.is_some_and(|(_, b)| b <= 0.3)
        && p.zuege.iter().all(|z| {
            let [a, b] = z.punkte[..] else { return false };
            let l = (a[0] - b[0]).hypot(a[1] - b[1]);
            l <= 60.0
                && [a, b].iter().any(|q| {
                    kreuze
                        .iter()
                        .any(|k| (q[0] - k[0]).hypot(q[1] - k[1]) < 0.6 * l)
                })
        })
}

/// Rahmen: Rechteck aus langen, achsparallelen Strichen einer Linienart
/// (mindestens 40 % der Seite in beiden Richtungen).
fn rahmen_suchen(s: &Seite) -> Option<[f64; 4]> {
    let w = s.mediabox[2] - s.mediabox[0];
    let h = s.mediabox[3] - s.mediabox[1];
    let mut arten: Vec<(Linienart, Vec<[[f64; 2]; 2]>)> = Vec::new();
    for p in &s.pfade {
        let Some((f, b)) = p.strich else { continue };
        if hell(f) {
            continue;
        }
        for z in &p.zuege {
            let mut pk = z.punkte.clone();
            if z.zu {
                pk.push(z.punkte[0]);
            }
            for q in pk.windows(2) {
                let (dx, dy) = ((q[1][0] - q[0][0]).abs(), (q[1][1] - q[0][1]).abs());
                if (dy < 0.01 && dx > 0.4 * w) || (dx < 0.01 && dy > 0.4 * h) {
                    match arten
                        .iter_mut()
                        .find(|(a, _)| gleich(a.farbe, f) && (a.breite - b).abs() < 0.01)
                    {
                        Some(a) => a.1.push([q[0], q[1]]),
                        None => arten.push((
                            Linienart {
                                farbe: f,
                                breite: b,
                            },
                            vec![[q[0], q[1]]],
                        )),
                    }
                }
            }
        }
    }
    let mut best: Option<[f64; 4]> = None;
    for (_, st) in arten {
        let senk: Vec<f64> = st
            .iter()
            .filter(|q| (q[0][0] - q[1][0]).abs() < 0.01)
            .map(|q| q[0][0])
            .collect();
        let waag: Vec<f64> = st
            .iter()
            .filter(|q| (q[0][1] - q[1][1]).abs() < 0.01)
            .map(|q| q[0][1])
            .collect();
        if senk.len() < 2 || waag.len() < 2 {
            continue;
        }
        let r = [
            senk.iter().copied().fold(f64::MAX, f64::min),
            waag.iter().copied().fold(f64::MAX, f64::min),
            senk.iter().copied().fold(f64::MIN, f64::max),
            waag.iter().copied().fold(f64::MIN, f64::max),
        ];
        let a = (r[2] - r[0]) * (r[3] - r[1]);
        if r[2] - r[0] > 0.4 * w
            && r[3] - r[1] > 0.4 * h
            && best.is_none_or(|b| a > (b[2] - b[0]) * (b[3] - b[1]))
        {
            best = Some(r);
        }
    }
    best
}

/// Linien ordnen: Bedeutung je Pfad und die Zuordnung je Linienart. Bei
/// unbekanntem Erzeuger gilt die Linienart als Grenze, deren Ecken auf den
/// Grenzpunkt-Kreisen liegen, sonst die dickste schwarze.
pub fn ordnen(
    s: &Seite,
    erzeuger: Erzeuger,
    rahmen: Option<[f64; 4]>,
) -> (Vec<Bedeutung>, Vec<Zuordnung>) {
    let kreuze = lage::gitterkreuze(s);
    let ist_rahmen = |p: &Pfad| {
        let Some(r) = rahmen else { return false };
        let k = kasten(p);
        let an = |v: f64, a: f64| (v - a).abs() < 0.5;
        (an(k[0], r[0]) || an(k[0], r[2]) || an(k[1], r[1]) || an(k[1], r[3]))
            && (an(k[2], r[0]) || an(k[2], r[2]) || an(k[3], r[1]) || an(k[3], r[3]))
            && ((k[2] - k[0]) < 0.5 || (k[3] - k[1]) < 0.5)
    };
    let mut b: Vec<Bedeutung> = s
        .pfade
        .iter()
        .map(|p| {
            if am_kreuz(p, &kreuze) {
                Bedeutung::Gitter
            } else if erzeuger == Erzeuger::AedSicad {
                aed_sicad(p)
            } else if ist_rahmen(p) {
                Bedeutung::Rahmen
            } else if kreis(p).is_some() && p.strich.is_some() {
                Bedeutung::Grenzpunkt
            } else {
                Bedeutung::Sonstiges
            }
        })
        .collect();
    // passt die Tabelle nicht (anders gedruckt), gilt die Regel für unbekannte Erzeuger
    if erzeuger == Erzeuger::AedSicad && !b.contains(&Bedeutung::Grenze) {
        return ordnen(s, Erzeuger::Unbekannt, rahmen);
    }
    if erzeuger == Erzeuger::Unbekannt {
        let mitten: Vec<[f64; 2]> = s
            .pfade
            .iter()
            .zip(&b)
            .filter(|(_, &b)| b == Bedeutung::Grenzpunkt)
            .filter_map(|(p, _)| kreis(p).map(|k| k.0))
            .collect();
        // Linienarten mit Treffern auf Grenzpunkten und größter Breite
        let mut arten: Vec<(Linienart, usize)> = Vec::new();
        for (p, &bb) in s.pfade.iter().zip(&b) {
            let Some((f, w)) = p.strich else { continue };
            if bb != Bedeutung::Sonstiges || p.kurven {
                continue;
            }
            let treffer = p
                .zuege
                .iter()
                .flat_map(|z| &z.punkte)
                .filter(|q| {
                    mitten
                        .iter()
                        .any(|m| (q[0] - m[0]).hypot(q[1] - m[1]) < 0.3)
                })
                .count();
            match arten
                .iter_mut()
                .find(|(a, _)| gleich(a.farbe, f) && (a.breite - w).abs() < 0.01)
            {
                Some(a) => a.1 += treffer,
                None => arten.push((
                    Linienart {
                        farbe: f,
                        breite: w,
                    },
                    treffer,
                )),
            }
        }
        let grenze = arten
            .iter()
            .filter(|a| a.1 >= 2)
            .max_by(|a, b| a.1.cmp(&b.1).then(a.0.breite.total_cmp(&b.0.breite)))
            .or_else(|| {
                arten
                    .iter()
                    .filter(|a| schwarz(a.0.farbe))
                    .max_by(|a, b| a.0.breite.total_cmp(&b.0.breite))
            })
            .map(|a| a.0);
        if let Some(g) = grenze {
            for (p, bb) in s.pfade.iter().zip(&mut b) {
                if *bb == Bedeutung::Sonstiges
                    && !p.kurven
                    && p.strich
                        .is_some_and(|(f, w)| gleich(f, g.farbe) && (w - g.breite).abs() < 0.01)
                {
                    *bb = Bedeutung::Grenze;
                }
            }
        }
    }
    let mut z: Vec<Zuordnung> = Vec::new();
    for (p, &bb) in s.pfade.iter().zip(&b) {
        let Some((f, w)) = p.strich else { continue };
        match z.iter_mut().find(|x| {
            gleich(x.art.farbe, f) && (x.art.breite - w).abs() < 0.01 && x.bedeutung == bb
        }) {
            Some(x) => x.anzahl += 1,
            None => z.push(Zuordnung {
                art: Linienart {
                    farbe: f,
                    breite: w,
                },
                bedeutung: bb,
                anzahl: 1,
            }),
        }
    }
    (b, z)
}

/// Grenzpunkte: Kreise mit Bedeutung Grenzpunkt, mit der nächsten Nummer
/// daneben (eine ganze Zahl, Schrift nicht größer als das 1,5-Fache des
/// Kreises, Abstand unter dem Dreifachen).
pub fn grenzpunkte(s: &Seite, b: &[Bedeutung]) -> Vec<Grenzpunkt> {
    let mut gp: Vec<Grenzpunkt> = s
        .pfade
        .iter()
        .zip(b)
        .filter(|(_, &b)| b == Bedeutung::Grenzpunkt)
        .filter_map(|(p, _)| kreis(p))
        .map(|(lage, durchmesser)| Grenzpunkt {
            lage,
            durchmesser,
            nr: None,
        })
        .collect();
    // jede Nummer nur einmal, die nächsten Paare zuerst
    let mut paare = Vec::new();
    for (i, g) in gp.iter().enumerate() {
        for (j, t) in s.texte.iter().enumerate() {
            let zahl = !t.text.is_empty() && t.text.chars().all(|c| c.is_ascii_digit());
            let m = lage::mitte(t);
            let d = (m[0] - g.lage[0]).hypot(m[1] - g.lage[1]);
            if zahl && t.groesse <= 1.5 * g.durchmesser && d < 3.0 * g.durchmesser {
                paare.push((d, i, j));
            }
        }
    }
    paare.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut text_frei = vec![true; s.texte.len()];
    for (_, i, j) in paare {
        if gp[i].nr.is_none() && text_frei[j] {
            gp[i].nr = Some(s.texte[j].text.clone());
            text_frei[j] = false;
        }
    }
    gp
}

/// Flurstücksnummer aus dem Kopf („Flurstück: 45/6“, „Flurstück 123/4“),
/// auch wenn die Nummer ein eigener Text rechts daneben ist.
pub fn nummer_im_kopf(texte: &[Text]) -> Option<String> {
    let nummer = |s: &str| -> Option<String> {
        let s = s.trim_start_matches([':', ' ', '.']);
        let n: String = s
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '/')
            .collect();
        let n = n.trim_end_matches('/');
        (n.chars().next().is_some_and(|c| c.is_ascii_digit())).then(|| n.to_string())
    };
    for t in texte {
        let kl = t.text.to_lowercase();
        let Some(i) = ["flurstück", "flurstueck", "flst."]
            .iter()
            .find_map(|w| kl.find(w).map(|i| i + w.len()))
        else {
            continue;
        };
        // Rest bis zum nächsten Leerzeichen des Wortes überspringen
        let rest: String = kl[i..].chars().skip_while(|c| c.is_alphabetic()).collect();
        if let Some(n) = nummer(&rest) {
            return Some(n);
        }
        // Nummer als eigener Text rechts daneben auf derselben Grundlinie
        let nachbar = texte
            .iter()
            .filter(|u| (u.lage[1] - t.lage[1]).abs() < 0.3 * t.groesse && u.lage[0] > t.lage[0])
            .filter(|u| u.lage[0] - t.ende[0] < 3.0 * t.groesse)
            .min_by(|a, b| a.lage[0].total_cmp(&b.lage[0]));
        if let Some(n) = nachbar.and_then(|u| nummer(&u.text)) {
            return Some(n);
        }
    }
    None
}

/// Stelle der Nummer in der Karte: als ein Text („123/4“) oder als Bruch
/// („45“ über „6“, Mitte zwischen beiden). Texte in `aus` zählen nicht.
pub fn nummer_in_karte(texte: &[Text], nummer: &str, aus: &[usize]) -> Option<[f64; 2]> {
    // frei als Maske statt `aus.contains` in der Doppelschleife (Review
    // W37 B7)
    let mut frei = vec![true; texte.len()];
    for &i in aus {
        if let Some(f) = frei.get_mut(i) {
            *f = false;
        }
    }
    if let Some((_, t)) = texte
        .iter()
        .enumerate()
        .filter(|(i, _)| frei[*i])
        .find(|(_, t)| t.text.trim() == nummer)
    {
        return Some(lage::mitte(t));
    }
    let (z, n) = nummer.split_once('/')?;
    // Zähler und Nenner vorab, je höchstens 1000 Kandidaten
    let wahl = |w: &str| -> Vec<&Text> {
        texte
            .iter()
            .enumerate()
            .filter(|(i, t)| frei[*i] && t.text.trim() == w)
            .map(|(_, t)| t)
            .take(1000)
            .collect()
    };
    let (zaehler, nenner) = (wahl(z), wahl(n));
    for a in &zaehler {
        let ma = lage::mitte(a);
        for b in &nenner {
            if std::ptr::eq(*a, *b) {
                continue;
            }
            let mb = lage::mitte(b);
            // Nenner unter dem Zähler, quer zur Schrift
            let (s, c) = a.winkel.to_radians().sin_cos();
            let (dx, dy) = (mb[0] - ma[0], mb[1] - ma[1]);
            let laengs = dx * c + dy * s;
            let quer = -dx * s + dy * c;
            if quer < -0.5 * a.groesse && quer > -2.0 * a.groesse && laengs.abs() < a.groesse {
                return Some([(ma[0] + mb[0]) / 2.0, (ma[1] + mb[1]) / 2.0]);
            }
        }
    }
    None
}

/// Strecken aller Pfade mit Bedeutung Grenze.
fn grenzstrecken(s: &Seite, b: &[Bedeutung]) -> Vec<[[f64; 2]; 2]> {
    let mut aus = Vec::new();
    for (p, _) in s
        .pfade
        .iter()
        .zip(b)
        .filter(|(_, &b)| b == Bedeutung::Grenze)
    {
        for z in &p.zuege {
            let mut q = z.punkte.clone();
            if z.zu && q.len() > 2 {
                q.push(q[0]);
            }
            for w in q.windows(2) {
                aus.push([w[0], w[1]]);
            }
        }
    }
    aus
}

/// Das Flurstück zur Nummer: die kleinste Fläche aus Grenzlinien um die
/// Nummer in der Karte.
pub fn flurstueck(plan: &Lageplan, nummer: &str) -> Option<Flurstueck> {
    let s = &plan.seite;
    // Texte außerhalb des Rahmens (Kopf, Fußzeile) zählen nicht
    let aus: Vec<usize> = match plan.rahmen {
        Some(r) => (0..s.texte.len())
            .filter(|&i| {
                let m = lage::mitte(&s.texte[i]);
                m[0] < r[0] || m[0] > r[2] || m[1] < r[1] || m[1] > r[3]
            })
            .collect(),
        None => (0..s.texte.len())
            .filter(|&i| s.texte[i].text.to_lowercase().contains("flurst"))
            .collect(),
    };
    let q = nummer_in_karte(&s.texte, nummer, &aus)?;
    let ecken_pt = flaeche::kleinste_um(&grenzstrecken(s, &plan.bedeutungen), q)?;
    let (ecken, flaeche) = match &plan.lage {
        Lage::Gitter(e) => {
            let u: Vec<[f64; 2]> = ecken_pt.iter().map(|p| e.utm(*p)).collect();
            let a = flaeche::flaeche(&u).abs();
            (Some(u), Some(a))
        }
        Lage::Massstab(m) => (
            None,
            Some(flaeche::flaeche(&ecken_pt).abs() * (m * lage::PT).powi(2)),
        ),
        Lage::Offen => (None, None),
    };
    Some(Flurstueck {
        nummer: nummer.to_string(),
        lage: q,
        ecken_pt,
        ecken,
        flaeche,
    })
}

/// Was gezeigt wird: alles im Kartenbereich außer Rahmen, Gitterkreuzen und
/// weißen Abdeckungen; Texte im Kartenbereich außer den Rahmenwerten. Der
/// Kartenbereich ist der Rahmen, sonst die Umgebung von Grenzen,
/// Grenzpunkten und Gebäuden. Bilder (Wappen) zeigt Skizzeo nicht.
pub fn unterlage(plan: &Lageplan) -> Unterlage {
    let s = &plan.seite;
    let bereich = plan.rahmen.or_else(|| {
        let mut k = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for (p, b) in s.pfade.iter().zip(&plan.bedeutungen) {
            if matches!(
                b,
                Bedeutung::Grenze | Bedeutung::Grenzpunkt | Bedeutung::Gebaeude
            ) {
                let q = kasten(p);
                k = [
                    k[0].min(q[0]),
                    k[1].min(q[1]),
                    k[2].max(q[2]),
                    k[3].max(q[3]),
                ];
            }
        }
        let (w, h) = (k[2] - k[0], k[3] - k[1]);
        (w >= 0.0).then(|| {
            [
                k[0] - 0.25 * w,
                k[1] - 0.25 * h,
                k[2] + 0.25 * w,
                k[3] + 0.25 * h,
            ]
        })
    });
    let Some(r) = bereich else {
        return Unterlage::default();
    };
    let innen = |m: [f64; 2]| m[0] >= r[0] && m[0] <= r[2] && m[1] >= r[1] && m[1] <= r[3];
    let werte = lage::rahmenwerte(&s.texte);
    let mut u = Unterlage::default();
    for (p, &b) in s.pfade.iter().zip(&plan.bedeutungen) {
        let k = kasten(p);
        let weiss = p.strich.is_none_or(|(f, _)| hell(f)) && p.fuellung.is_some_and(hell);
        // Linien über den Rahmen hinaus schneidet die Darstellung ab; hier zählt die Mitte
        let m = [(k[0] + k[2]) / 2.0, (k[1] + k[3]) / 2.0];
        let ueberdeckt = k[0] <= r[0] && k[2] >= r[2] && k[1] <= r[1] && k[3] >= r[3];
        if matches!(b, Bedeutung::Rahmen | Bedeutung::Gitter) || weiss || !(innen(m) || ueberdeckt)
        {
            continue;
        }
        u.pfade.push((p.clone(), b));
    }
    for t in &s.texte {
        let m = lage::mitte(t);
        if innen(m) && !werte.iter().any(|w| w.lage == m) {
            u.texte.push(t.clone());
        }
    }
    u
}

/// Seite auswerten: ordnen, Lage, Grenzpunkte, Flurstück aus dem Kopf.
pub fn auswerten(seite: Seite, erzeuger_text: &str) -> Lageplan {
    let erzeuger = if erzeuger_text.contains("AED") || erzeuger_text.contains("SICAD") {
        Erzeuger::AedSicad
    } else {
        Erzeuger::Unbekannt
    };
    let rahmen = rahmen_suchen(&seite);
    let (bedeutungen, zuordnung) = ordnen(&seite, erzeuger, rahmen);
    let massstab = lage::massstab(&seite.texte);
    let lage = match lage::gitter(&seite, rahmen, massstab) {
        Some(e) => Lage::Gitter(e),
        None => massstab.map_or(Lage::Offen, Lage::Massstab),
    };
    let grenzpunkte = grenzpunkte(&seite, &bedeutungen);
    let nummer = nummer_im_kopf(&seite.texte);
    let mut hinweise = Vec::new();
    if seite.pfade.is_empty() && seite.texte.is_empty() && !seite.bilder.is_empty() {
        hinweise.push("Die Seite ist nur ein Bild (eingescannter Plan); Grenzen und Maßstab lassen sich daraus nicht lesen. Beim Vermessungsamt einen Auszug als Vektor-PDF anfordern, oder die Grenzen mit der DXF des Vermessers über „Höhen“ einlesen.".to_string());
    }
    let mut plan = Lageplan {
        seite,
        erzeuger,
        bedeutungen,
        zuordnung,
        rahmen,
        lage,
        grenzpunkte,
        nummer,
        flurstueck: None,
        hinweise,
    };
    if let Some(n) = plan.nummer.clone() {
        plan.flurstueck = flurstueck(&plan, &n);
        if plan.flurstueck.is_none() {
            plan.hinweise.push(format!(
                "Das Flurstück {n} aus dem Kopf ist in der Karte nicht als geschlossene Fläche zu finden. Die Grenze kommt dann aus der DXF des Vermessers über „Höhen“, oder aus einem neuen Auszug als Vektor-PDF."
            ));
        }
    }
    plan
}

/// Hinweis, wenn die Datei mehr als das Budget des PDF-Lesers wollte.
pub const ZU_GROSS: &str = "Die Datei verlangt mehr Daten, als Skizzeo liest (64 MB je Strom, 256 MB insgesamt); dort fehlt etwas, der Lageplan kann unvollständig sein.";

/// PDF-Lageplan lesen und auswerten (erste Seite).
pub fn lesen(roh: &[u8]) -> Result<Lageplan, String> {
    let pdf = Pdf::lesen(roh)?;
    let seiten = pdf.seiten();
    let Some(b) = seiten.first() else {
        return Err("Die PDF-Datei hat keine lesbare Seite.".into());
    };
    let s = seite(&pdf, b);
    let mut plan = auswerten(s, &pdf.erzeuger());
    if pdf.zu_gross() {
        plan.hinweise.insert(0, ZU_GROSS.into());
    }
    if seiten.len() > 1 {
        plan.hinweise.insert(
            0,
            format!(
                "Die Datei hat {} Seiten; gelesen wird die erste.",
                seiten.len()
            ),
        );
    }
    Ok(plan)
}

#[cfg(test)]
mod tests;

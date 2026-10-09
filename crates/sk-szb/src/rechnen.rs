//! Körper und Mengen eines Bauteils (Vertrag §3, §7, §8), wie `build` der
//! Werkbank: abgeleitete Werte, Einfügehöhe, Körper aus Grundformen mit
//! `anzahl`, `wenn` und `drehung`, Volumen je Baustoff und die Mengen.
//!
//! Koordinaten in mm, Ursprung ist der Einfügepunkt auf UK Geschoss; die
//! Höhe des Einfügepunkts über UK ist [`Ergebnis::z0`] (`[hoehe] versatz`).

use crate::formel::{self, Formel, Umfeld, Volumen};
use crate::lesen::{Def, Satz};
use crate::{zahl, Befund};
use std::collections::HashMap;

/// Das Bezugsgeschoss: Geschosshöhe und Decke darüber (mm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geschoss {
    pub gh: f64,
    pub decke: f64,
}

impl Geschoss {
    /// Probegeschoss der Werkbank (EG wie RH-1).
    pub const PROBE: Geschoss = Geschoss {
        gh: 2855.0,
        decke: 220.0,
    };

    /// `GH`, `DECKE`, `LICHT`
    pub fn umfeld(&self) -> Umfeld {
        let mut u = Umfeld::new();
        u.insert("GH".into(), self.gh);
        u.insert("DECKE".into(), self.decke);
        u.insert("LICHT".into(), self.gh - self.decke);
        u
    }
}

/// Ebene eines Umrisses: `xy` Grundriss, `xz` Vorderansicht, `yz` Seite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ebene {
    Xy,
    Xz,
    Yz,
}

impl Ebene {
    fn von(s: &str) -> Option<Ebene> {
        Some(match s {
            "xy" => Ebene::Xy,
            "xz" => Ebene::Xz,
            "yz" => Ebene::Yz,
            _ => return None,
        })
    }
}

/// Ein Körper: ebener Umriss (u, v) in `ebene`, ausgezogen von `von` bis
/// `bis` entlang der dritten Achse, verschoben um `ursprung` und um
/// `drehung` Grad um die Senkrechte durch den Ursprung gedreht.
#[derive(Clone, Debug, PartialEq)]
pub struct Koerper {
    /// Index des `[koerper]`-Satzes.
    pub satz: usize,
    pub baustoff: String,
    pub teil: Option<String>,
    pub funktion: Option<String>,
    /// Gegen den Uhrzeigersinn in (u, v).
    pub umriss: Vec<[f64; 2]>,
    pub von: f64,
    pub bis: f64,
    pub ebene: Ebene,
    /// x, y und z (ohne die Einfügehöhe).
    pub ursprung: [f64; 3],
    pub drehung: f64,
    /// Zylindermantel: Seitenkanten nicht zeichnen.
    pub glatt: bool,
    /// mm³
    pub volumen: f64,
}

impl Koerper {
    /// Punkt (u, v, w) in Bauteilkoordinaten; `z0` ist die Einfügehöhe.
    pub fn punkt(&self, u: f64, v: f64, w: f64, z0: f64) -> [f64; 3] {
        let p = match self.ebene {
            Ebene::Xy => [u, v, w],
            Ebene::Xz => [u, w, v],
            Ebene::Yz => [w, u, v],
        };
        let (s, c) = self.drehung.to_radians().sin_cos();
        let [ox, oy, oz] = self.ursprung;
        [
            ox + p[0] * c - p[1] * s,
            oy + p[0] * s + p[1] * c,
            oz + z0 + p[2],
        ]
    }
}

/// Höchstzahl der Punkte eines Umrisses (die Prüfung auf
/// Selbstüberschneidung wächst quadratisch).
pub const MAX_PUNKTE: usize = 256;
/// Höchstzahl der Körper eines Exemplars, alle `anzahl` zusammen.
pub const MAX_KOERPER: usize = 2000;

/// Ergebnis einer Rechnung.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ergebnis {
    pub koerper: Vec<Koerper>,
    /// m³ je Baustoff.
    pub vol: Volumen,
    /// Je `[wert]`: Index und Wert.
    pub werte: Vec<(usize, f64)>,
    /// Je `[menge]`: Index und Wert, `None` bei Fehler.
    pub mengen: Vec<(usize, Option<f64>)>,
    /// Je `[menge]` mit `dicke`: Index und Dicke (mm), wenn sie rechnet.
    pub dicken: Vec<(usize, f64)>,
    pub befunde: Vec<Befund>,
    /// Einfügehöhe über UK Geschoss (mm).
    pub z0: f64,
    /// Zahl der erzeugten Körper.
    pub anzahl: usize,
}

/// Die Vorgaben der Parameter (`wert`) der Reihe nach; eine fehlerhafte
/// Vorgabe gilt als 0.
pub fn vorgaben(def: &Def, g: &Geschoss) -> Umfeld {
    let mut pv = Umfeld::new();
    for r in &def.param {
        let mut u = g.umfeld();
        u.extend(pv.iter().map(|(k, v)| (k.clone(), *v)));
        let v = r
            .get("wert")
            .and_then(|w| formel::rechnen(w, &u, None).ok())
            .unwrap_or(0.0);
        pv.insert(r.key().to_string(), v);
    }
    pv
}

/// Höchstzahl der Rechenschritte (Formelknoten) einer Rechnung (Review
/// 3cg): darüber „Bauteil zu aufwendig“ statt Rechnen.
pub const MAX_SCHRITTE: u64 = 2_000_000;
/// Höchstzahl der Rechenschritte aller Rechnungen einer Prüfung.
pub const MAX_SCHRITTE_PRUEFUNG: u64 = 20_000_000;

/// Rechnet Formeln: jede einmal übersetzt, alle zusammen mit höchstens
/// `rest` Schritten.
pub struct Rechner {
    formeln: HashMap<String, Result<Formel, String>>,
    /// Übrige Rechenschritte.
    pub rest: u64,
    /// Die Schritte reichten nicht.
    pub erschoepft: bool,
}

impl Rechner {
    pub fn neu(schritte: u64) -> Rechner {
        Rechner {
            formeln: HashMap::new(),
            rest: schritte,
            erschoepft: false,
        }
    }

    /// Rechnet `src` im Umfeld `u`; `vol` nur in `[menge]`.
    pub fn wert(&mut self, src: &str, u: &Umfeld, vol: Option<&Volumen>) -> Result<f64, String> {
        if !self.formeln.contains_key(src) {
            self.formeln.insert(src.to_string(), Formel::neu(src));
        }
        let r = match &self.formeln[src] {
            Ok(f) => f.wert_im(u, vol, &mut self.rest),
            Err(x) => Err(x.clone()),
        };
        if self.rest == 0 && r.is_err() {
            self.erschoepft = true;
        }
        r
    }

    /// Zieht `n` Schritte für Geometrie ab (Review 3ch-2); reicht es nicht,
    /// ist der Fehler [`formel::ZU_AUFWENDIG`] und `rest` 0.
    fn verbrauchen(&mut self, n: u64) -> Result<(), String> {
        if n > self.rest {
            self.rest = 0;
            self.erschoepft = true;
            return Err(formel::ZU_AUFWENDIG.into());
        }
        self.rest -= n;
        Ok(())
    }

    /// Befund eines Satzes; nach dem Erschöpfen nur noch der eine am Ende.
    fn befund(&self, e: &mut Ergebnis, b: Befund) {
        if !self.erschoepft {
            e.befunde.push(b);
        }
    }
}

/// Rechnet Werte, Körper und Mengen mit den Parametern `pv` im Geschoss
/// `g`. Fehler einzelner Sätze werden Befunde, die Rechnung geht weiter.
pub fn rechnen(def: &Def, pv: &Umfeld, g: &Geschoss) -> Ergebnis {
    rechnen_mit(&mut Rechner::neu(MAX_SCHRITTE), def, pv, g)
}

/// Wie [`rechnen`] mit dem Rechner `rc`: höchstens [`MAX_SCHRITTE`] und
/// höchstens `rc.rest` Schritte; `rc.rest` nimmt um die verbrauchten ab.
pub fn rechnen_mit(rc: &mut Rechner, def: &Def, pv: &Umfeld, g: &Geschoss) -> Ergebnis {
    let gesamt = rc.rest;
    let erlaubt = gesamt.min(MAX_SCHRITTE);
    rc.rest = erlaubt;
    rc.erschoepft = false;
    let mut e = rechnen_innen(rc, def, pv, g);
    if rc.erschoepft {
        e.befunde.push(Befund::fehler(
            0,
            format!(
                "{}: mehr als {} Rechenschritte",
                formel::ZU_AUFWENDIG,
                zahl(erlaubt as f64, 0)
            ),
        ));
    }
    rc.rest = gesamt - (erlaubt - rc.rest);
    e
}

fn rechnen_innen(rc: &mut Rechner, def: &Def, pv: &Umfeld, g: &Geschoss) -> Ergebnis {
    let mut e = Ergebnis::default();
    let mut sc = g.umfeld();
    for r in &def.param {
        let k = r.key();
        sc.insert(k.to_string(), pv.get(k).copied().unwrap_or(0.0));
    }
    for (i, r) in def.wert.iter().enumerate() {
        let k = r.key().to_string();
        match rc.wert(r.get("formel").unwrap_or(""), &sc, None) {
            Ok(v) => {
                sc.insert(k, v);
                e.werte.push((i, v));
            }
            Err(x) => {
                rc.befund(&mut e, Befund::fehler(r.zeile, format!("[wert] {k}: {x}")));
                sc.insert(k, 0.0);
            }
        }
    }
    if let Some(h) = def.hoehe.first() {
        if let Some(v) = h.get("versatz") {
            match rc.wert(v, &sc, None) {
                Ok(z) => e.z0 = z,
                Err(x) => rc.befund(
                    &mut e,
                    Befund::fehler(h.zeile, format!("[hoehe] versatz: {x}")),
                ),
            }
        }
    }
    for (ix, r) in def.koerper.iter().enumerate() {
        let form = r.get("form").unwrap_or("");
        if !matches!(form, "quader" | "prisma" | "zylinder") {
            continue;
        }
        if let Err(x) = koerper(rc, &mut e, ix, r, &sc) {
            let wer = r.get("teil").unwrap_or(form);
            rc.befund(
                &mut e,
                Befund::fehler(r.zeile, format!("[koerper] {wer}: {x}")),
            );
        }
    }
    for (i, r) in def.menge.iter().enumerate() {
        let k = r.key();
        let v = match rc.wert(r.get("formel").unwrap_or(""), &sc, Some(&e.vol)) {
            Ok(v) => {
                if r.get("einheit") == Some("stk") && (v - v.round()).abs() > 1e-9 {
                    e.befunde.push(Befund::hinweis(
                        r.zeile,
                        format!("[menge] {k}: Stückzahl nicht ganz"),
                    ));
                }
                Some(v)
            }
            Err(x) => {
                rc.befund(&mut e, Befund::fehler(r.zeile, format!("[menge] {k}: {x}")));
                None
            }
        };
        e.mengen.push((i, v));
        if let Some(src) = r.get("dicke") {
            match rc.wert(src, &sc, Some(&e.vol)) {
                Ok(d) => e.dicken.push((i, d)),
                Err(x) => {
                    let b = Befund::fehler(r.zeile, format!("[menge] {k} dicke: {x}"));
                    rc.befund(&mut e, b);
                }
            }
        }
    }
    e
}

/// Alle Exemplare eines `[koerper]`-Satzes.
fn koerper(
    rc: &mut Rechner,
    e: &mut Ergebnis,
    ix: usize,
    r: &Satz,
    sc: &Umfeld,
) -> Result<(), String> {
    let mut n = 1;
    if let Some(a) = r.get("anzahl") {
        let v = rc.wert(a, sc, None)?;
        if (v - v.round()).abs() > 1e-9 {
            return Err("anzahl muss ganz sein".into());
        }
        let v = v.round();
        if !(0.0..=500.0).contains(&v) {
            return Err("anzahl 0 bis 500".into());
        }
        n = v as usize;
    }
    let form = r.get("form").unwrap_or("");
    let baustoff = r.get("baustoff").unwrap_or("").to_string();
    // einmal je Satz: Umfeld mit i und n, die Punkte des Prismas
    let mut s2 = sc.clone();
    s2.insert("n".into(), n as f64);
    let mut pk: Option<Vec<(String, String)>> = None;
    for i in 0..n {
        if e.anzahl >= MAX_KOERPER {
            return Err(format!("mehr als {MAX_KOERPER} Körper im Bauteil"));
        }
        s2.insert("i".into(), i as f64);
        let s2 = &s2;
        let g = |rc: &mut Rechner, k: &str| -> Result<f64, String> {
            match r.get(k) {
                Some(f) => rc.wert(f, s2, None),
                None => Ok(0.0),
            }
        };
        if r.get("wenn").is_some() && g(rc, "wenn")? == 0.0 {
            continue;
        }
        let drehung = g(rc, "drehung")?;
        let (umriss, von, bis, ebene, glatt, volumen) = match form {
            "quader" => {
                let (b, t, h) = (g(rc, "b")?, g(rc, "t")?, g(rc, "h")?);
                if !(b > 0.0 && t > 0.0 && h > 0.0) {
                    return Err(format!(
                        "b, t und h müssen > 0 sein (b={} t={} h={})",
                        zahl(b, 0),
                        zahl(t, 0),
                        zahl(h, 0)
                    ));
                }
                let p = vec![[0.0, 0.0], [b, 0.0], [b, t], [0.0, t]];
                (p, 0.0, h, Ebene::Xy, false, b * t * h)
            }
            "prisma" => {
                if pk.is_none() {
                    pk = Some(punkte(r.get("punkte").unwrap_or(""))?);
                }
                let mut p = Vec::new();
                for (a, b) in pk.iter().flatten() {
                    p.push([rc.wert(a, s2, None)?, rc.wert(b, s2, None)?]);
                }
                if p.len() < 3 {
                    return Err("punkte: mindestens 3".into());
                }
                // die Umrissprüfung vergleicht jede Kante mit jeder
                rc.verbrauchen((p.len() * p.len()) as u64)?;
                umriss_pruefen(&p)?;
                if flaeche2(&p) < 0.0 {
                    p.reverse();
                }
                let (w0, w1) = (g(rc, "von")?, g(rc, "bis")?);
                if (w1 - w0).abs() < 1e-6 {
                    return Err("von und bis gleich: Prisma ohne Dicke".into());
                }
                let ebene = Ebene::von(r.get("ebene").unwrap_or("")).unwrap_or(Ebene::Xy);
                let v = flaeche2(&p).abs() * (w1 - w0).abs();
                (p, w0.min(w1), w0.max(w1), ebene, false, v)
            }
            _ => {
                let (r0, h) = (g(rc, "r")?, g(rc, "h")?);
                if !(r0 > 0.0 && h > 0.0) {
                    return Err("r und h müssen > 0 sein".into());
                }
                let sd = match r.get("seiten") {
                    None => 24.0,
                    Some(_) => (g(rc, "seiten")? + 0.5).floor(),
                };
                if !(6.0..=96.0).contains(&sd) {
                    return Err("seiten 6 bis 96".into());
                }
                let sd = sd as usize;
                let tau = std::f64::consts::TAU;
                let p = (0..sd)
                    .map(|k| {
                        let a = tau * k as f64 / sd as f64;
                        [r0 * a.cos(), r0 * a.sin()]
                    })
                    .collect();
                let ebene = match r.get("achse").unwrap_or("z") {
                    "x" => Ebene::Yz,
                    "y" => Ebene::Xz,
                    _ => Ebene::Xy,
                };
                (p, 0.0, h, ebene, true, std::f64::consts::PI * r0 * r0 * h)
            }
        };
        if !volumen.is_finite() {
            return Err("Maße ergeben keine Zahl".into());
        }
        *e.vol.entry(baustoff.clone()).or_insert(0.0) += volumen / 1e9;
        e.anzahl += 1;
        e.koerper.push(Koerper {
            satz: ix,
            baustoff: baustoff.clone(),
            teil: r.get("teil").map(str::to_string),
            funktion: r.get("funktion").map(str::to_string),
            umriss,
            von,
            bis,
            ebene,
            ursprung: [g(rc, "x")?, g(rc, "y")?, g(rc, "z")?],
            drehung,
            glatt,
            volumen,
        });
    }
    Ok(())
}

/// `"u,v; u,v; …"` in Formelpaare; Kommas und Semikolons in Klammern
/// gehören zur Formel.
pub fn punkte(s: &str) -> Result<Vec<(String, String)>, String> {
    let mut teile = Vec::new();
    let (mut tiefe, mut cur) = (0i32, String::new());
    for ch in s.chars() {
        if ch == '(' {
            tiefe += 1;
        }
        if ch == ')' {
            tiefe -= 1;
        }
        if ch == ';' && tiefe == 0 {
            teile.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    teile.push(cur);
    let mut out = Vec::new();
    for p in teile {
        if p.trim().is_empty() {
            continue;
        }
        let (mut d, mut a, mut b) = (0i32, String::new(), None::<String>);
        for ch in p.chars() {
            if ch == '(' {
                d += 1;
            }
            if ch == ')' {
                d -= 1;
            }
            if ch == ',' && d == 0 && b.is_none() {
                b = Some(String::new());
                continue;
            }
            match &mut b {
                None => a.push(ch),
                Some(b) => b.push(ch),
            }
        }
        let Some(b) = b else {
            return Err(format!("Punkt „{}“ braucht u,v", p.trim()));
        };
        out.push((a.trim().to_string(), b.trim().to_string()));
        if out.len() > MAX_PUNKTE {
            return Err(format!("mehr als {MAX_PUNKTE} Punkte"));
        }
    }
    Ok(out)
}

/// Doppelte Fläche mit Vorzeichen (positiv gegen den Uhrzeigersinn) / 2.
pub fn flaeche2(p: &[[f64; 2]]) -> f64 {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

fn kreuz(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn schneiden(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let (d1, d2, d3, d4) = (
        kreuz(c, d, a),
        kreuz(c, d, b),
        kreuz(a, b, c),
        kreuz(a, b, d),
    );
    ((d1 > 1e-9 && d2 < -1e-9) || (d1 < -1e-9 && d2 > 1e-9))
        && ((d3 > 1e-9 && d4 < -1e-9) || (d3 < -1e-9 && d4 > 1e-9))
}

/// Umriss ohne doppelte Punkte, mit Fläche, ohne Selbstüberschneidung.
pub fn umriss_pruefen(p: &[[f64; 2]]) -> Result<(), String> {
    let n = p.len();
    for i in 0..n {
        let (a, b) = (p[i], p[(i + 1) % n]);
        if (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6 {
            return Err("doppelter Punkt im Umriss".into());
        }
    }
    if flaeche2(p).abs() < 1e-6 {
        return Err("Umriss hat keine Fläche".into());
    }
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if schneiden(p[i], p[(i + 1) % n], p[j], p[(j + 1) % n]) {
                return Err("Umriss überschneidet sich selbst".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lesen;

    #[test]
    fn punkte_mit_klammern() {
        let p = punkte("0,0; max(1,2),3; (a;b),c;").unwrap();
        assert_eq!(
            p,
            [
                ("0".into(), "0".into()),
                ("max(1,2)".into(), "3".into()),
                ("(a;b)".into(), "c".into())
            ]
        );
        assert_eq!(punkte("1;2").unwrap_err(), "Punkt „1“ braucht u,v");
    }

    #[test]
    fn umriss() {
        let q = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        assert_eq!(umriss_pruefen(&q), Ok(()));
        assert_eq!(flaeche2(&q), 1.0);
        let schleife = [[0.0, 0.0], [100.0, 100.0], [100.0, 0.0], [0.0, 100.0]];
        assert_eq!(
            umriss_pruefen(&schleife).unwrap_err(),
            "Umriss hat keine Fläche"
        );
        let x = [[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 1.0]];
        assert_eq!(
            umriss_pruefen(&x).unwrap_err(),
            "Umriss überschneidet sich selbst"
        );
        let d = [[0.0, 0.0], [0.0, 0.0], [1.0, 1.0]];
        assert_eq!(umriss_pruefen(&d).unwrap_err(), "doppelter Punkt im Umriss");
    }

    #[test]
    fn anzahl_wenn_drehung() {
        let (d, b) = lesen::lesen(
            "SZB 0\n[param] key=l wert=3000\n[koerper] form=zylinder baustoff=s anzahl=3 x=\"i*l/(n-1)\" r=10 h=100 seiten=6\n[koerper] form=quader baustoff=s wenn=0 b=1 t=1 h=1\n[koerper] form=quader baustoff=s drehung=90 x=100 b=200 t=10 h=10\n",
        );
        assert!(b.is_empty());
        let g = Geschoss::PROBE;
        let e = rechnen(&d, &vorgaben(&d, &g), &g);
        assert!(e.befunde.is_empty(), "{:?}", e.befunde);
        assert_eq!(e.anzahl, 4);
        let xs: Vec<f64> = e.koerper[..3].iter().map(|k| k.ursprung[0]).collect();
        assert_eq!(xs, [0.0, 1500.0, 3000.0]);
        assert_eq!(e.koerper[0].umriss.len(), 6);
        // Gedreht um 90°: die Breite liegt in +Y
        let q = &e.koerper[3];
        let p = q.punkt(200.0, 0.0, 0.0, 0.0);
        assert!(
            (p[0] - 100.0).abs() < 1e-9 && (p[1] - 200.0).abs() < 1e-9,
            "{p:?}"
        );
    }
    /// Review 3cg: jede Formel einmal übersetzt, Schritte gezählt; reichen
    /// sie nicht, ein Befund statt vieler, und die übrigen Schritte des
    /// Rechners nehmen nur um die verbrauchten ab.
    #[test]
    fn rechenschritte() {
        let (d, _) = lesen::lesen(
            "SZB 0\n[param] key=l wert=3000\n[koerper] form=quader baustoff=s anzahl=500 x=\"i*l+1+1+1+1\" b=1 t=1 h=1\n[menge] key=m formel=1\n",
        );
        let g = Geschoss::PROBE;
        let pv = vorgaben(&d, &g);
        let mut rc = Rechner::neu(10_000_000);
        let e = rechnen_mit(&mut rc, &d, &pv, &g);
        assert!(e.befunde.is_empty(), "{:?}", e.befunde);
        let verbraucht = 10_000_000 - rc.rest;
        // anzahl 1, je Exemplar x 11, b/t/h je 1 (y, z, drehung fehlen), Menge 1
        assert_eq!(verbraucht, 1 + 500 * 14 + 1);
        assert_eq!(rc.formeln.len(), 3, "500, x und 1");
        let mut knapp = Rechner::neu(1000);
        let e = rechnen_mit(&mut knapp, &d, &pv, &g);
        assert_eq!(knapp.rest, 0);
        assert!(knapp.erschoepft);
        let t: Vec<&str> = e.befunde.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(t, ["Bauteil zu aufwendig: mehr als 1.000 Rechenschritte"]);
        assert!(e.anzahl < 100);
    }

    /// Review 3ch-2: Die Umrissprüfung eines Prismas kostet Punktzahl²
    /// Schritte; 500 Prismen mit je 256 Punkten sprengen das Budget.
    #[test]
    fn prisma_zaehlt_die_umrisspruefung() {
        let (d, _) = lesen::lesen(
            "SZB 0\n[koerper] form=prisma baustoff=s punkte=\"0,0; 10,0; 0,10\" von=0 bis=1\n",
        );
        let g = Geschoss::PROBE;
        let pv = vorgaben(&d, &g);
        let mut rc = Rechner::neu(1000);
        let e = rechnen_mit(&mut rc, &d, &pv, &g);
        assert!(e.befunde.is_empty(), "{:?}", e.befunde);
        // 6 Punktformeln, Umriss 3², von, bis, x, y, z, drehung fehlen
        assert_eq!(1000 - rc.rest, 6 + 9 + 2);

        let mut pk = String::new();
        for k in 0..256 {
            let a = std::f64::consts::TAU * k as f64 / 256.0;
            pk.push_str(&format!(
                "{:.3},{:.3}; ",
                1000.0 * a.cos(),
                1000.0 * a.sin()
            ));
        }
        let (d, _) = lesen::lesen(&format!(
            "SZB 0\n[koerper] form=prisma baustoff=s anzahl=500 punkte=\"{pk}\" von=0 bis=1\n"
        ));
        let pv = vorgaben(&d, &g);
        let t = std::time::Instant::now();
        let mut rc = Rechner::neu(MAX_SCHRITTE_PRUEFUNG);
        let e = rechnen_mit(&mut rc, &d, &pv, &g);
        assert!(rc.erschoepft);
        let t2: Vec<&str> = e.befunde.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(
            t2,
            ["Bauteil zu aufwendig: mehr als 2.000.000 Rechenschritte"]
        );
        // 2 000 000 / (512 + 65 536 + 2) Schritte je Exemplar
        assert_eq!(e.anzahl, 30);
        assert!(t.elapsed() < std::time::Duration::from_secs(2));
    }
}

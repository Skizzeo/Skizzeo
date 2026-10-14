//! Raster über Quader in der Ebene (x, y): Die Maus fragt nur die Zellen
//! unter ihr ab, nicht jeden Wandzug und jedes Geschoss (Review L26, L29).
//! Ein Eintrag steht in jeder Zelle, die sein Quader überdeckt; ohne Quader
//! oder über mehr als [`GROSS`] Zellen gehört er zu jeder Abfrage. Die
//! Abfragen liefern eine Obermenge: Wer ruft, prüft wie bisher am Quader.

use super::{Aabb, Scene};
use crate::camera::Camera;
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, Model, RunId, WallChain};
use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Kantenlänge einer Zelle (mm), etwa ein Haus.
pub const ZELLE: f64 = 10_000.0;
/// So viele Zellen darf ein Eintrag überdecken; ein größerer gehört zu
/// jeder Abfrage.
const GROSS: i64 = 1024;

/// Schneller Schlüssel für Zellen (zwei ganze Zahlen; kein Schutz gegen
/// gezielte Kollisionen nötig): das erste Zeigen nach einer Änderung baut
/// das Raster der Geschosse neu.
#[derive(Default)]
struct Zellhash(u64);

impl Hasher for Zellhash {
    fn write(&mut self, b: &[u8]) {
        for &x in b {
            self.write_u32(x as u32);
        }
    }

    fn write_i32(&mut self, i: i32) {
        self.write_u32(i as u32);
    }

    fn write_u32(&mut self, i: u32) {
        self.0 = (self.0.rotate_left(5) ^ i as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type Zellen = HashMap<(i32, i32), Vec<u32>, BuildHasherDefault<Zellhash>>;

#[derive(Clone, Copy, Default, PartialEq)]
enum Lage {
    #[default]
    Frei,
    /// Zellen x0, y0, x1, y1 (einschließlich).
    Zellen([i32; 4]),
    Immer,
}

/// Zellen eines in einem Zug gebauten Rasters, dicht ab (x0, y0) über
/// nx × ny, je Zelle ein Stück aus `plaetze`: ohne eine Liste je Zelle,
/// denn das erste Zeigen nach einer Änderung baut das Raster der Geschosse
/// neu.
struct Dicht {
    x0: i32,
    y0: i32,
    nx: i32,
    ny: i32,
    start: Vec<u32>,
    plaetze: Vec<u32>,
}

impl Dicht {
    fn index(&self, (x, y): (i32, i32)) -> Option<usize> {
        let (i, j) = (x as i64 - self.x0 as i64, y as i64 - self.y0 as i64);
        let innen = i >= 0 && j >= 0 && i < self.nx as i64 && j < self.ny as i64;
        innen.then(|| (j * self.nx as i64 + i) as usize)
    }

    fn zelle(&self, c: (i32, i32)) -> &[u32] {
        self.index(c).map_or(&[], |k| {
            &self.plaetze[self.start[k] as usize..self.start[k + 1] as usize]
        })
    }
}

/// Raster je Platz (Arena-Platz eines Wandzugs, Nummer eines Geschosses,
/// Schenkel einer Öffnung).
#[derive(Default)]
pub struct Raster {
    zellen: Zellen,
    /// Statt `zellen`, solange das Raster so ist, wie [`Raster::aus`] es baute.
    dicht: Option<Dicht>,
    lage: Vec<Lage>,
    immer: Vec<u32>,
    /// Quader je Platz.
    quader: Vec<Option<Aabb>>,
    /// Quader um alles Eingetragene und die längste Diagonale (Review H2
    /// zu #147); `None`, seit ein Quader an ihrem Rand ging, bis sie jemand
    /// liest: dann einmal über alle Plätze, nicht je Zug (Review B2 zu
    /// #151).
    rahmen: Cell<Option<(Option<Aabb>, f64)>>,
}

/// Zelle, in der die Koordinate `v` liegt.
pub(super) fn zelle(v: f64) -> i32 {
    (v / ZELLE).floor().clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

impl Raster {
    /// Raster über `quader`, je Platz in seiner Reihenfolge; dicht, wenn
    /// die Zellen nicht weit verstreut liegen.
    pub fn aus(quader: impl IntoIterator<Item = Option<Aabb>>) -> Raster {
        let mut r = Raster::default();
        for (i, q) in quader.into_iter().enumerate() {
            r.quader.push(q);
            let lage = r.lage_von(q);
            if lage == Lage::Immer {
                r.immer.push(i as u32);
            }
            r.lage.push(lage);
        }
        let bereich = r
            .lage
            .iter()
            .fold(None, |b: Option<[i32; 4]>, l| match (b, l) {
                (None, Lage::Zellen(z)) => Some(*z),
                (Some(b), Lage::Zellen(z)) => Some([
                    b[0].min(z[0]),
                    b[1].min(z[1]),
                    b[2].max(z[2]),
                    b[3].max(z[3]),
                ]),
                (b, _) => b,
            });
        let Some([x0, y0, x1, y1]) = bereich else {
            return r;
        };
        let (nx, ny) = (x1 as i64 - x0 as i64 + 1, y1 as i64 - y0 as i64 + 1);
        if nx * ny > 64 * r.lage.len() as i64 + 4096 {
            for (i, l) in r.lage.clone().into_iter().enumerate() {
                if let Lage::Zellen(z) = l {
                    r.eintragen(i as u32, z);
                }
            }
            return r;
        }
        // Zählen, Anfänge, Füllen
        let mut d = Dicht {
            x0,
            y0,
            nx: nx as i32,
            ny: ny as i32,
            start: vec![0; (nx * ny) as usize + 1],
            plaetze: Vec::new(),
        };
        let zellen =
            |z: [i32; 4]| (z[1]..=z[3]).flat_map(move |y| (z[0]..=z[2]).map(move |x| (x, y)));
        for l in &r.lage {
            if let Lage::Zellen(z) = *l {
                for c in zellen(z) {
                    let k = d.index(c).unwrap();
                    d.start[k + 1] += 1;
                }
            }
        }
        for k in 1..d.start.len() {
            d.start[k] += d.start[k - 1];
        }
        d.plaetze = vec![0; *d.start.last().unwrap() as usize];
        let mut frei = d.start.clone();
        for (i, l) in r.lage.iter().enumerate() {
            if let Lage::Zellen(z) = *l {
                for c in zellen(z) {
                    let k = d.index(c).unwrap();
                    d.plaetze[frei[k] as usize] = i as u32;
                    frei[k] += 1;
                }
            }
        }
        r.dicht = Some(d);
        r
    }

    /// Lage für Quader `q`; merkt ihn im Quader um alles.
    fn lage_von(&mut self, q: Option<Aabb>) -> Lage {
        let Some((lo, hi)) = q else {
            return Lage::Immer;
        };
        if let Some((alles, diagonale)) = self.rahmen.get() {
            let diagonale = diagonale.max((hi - lo).length());
            self.rahmen.set(Some((super::union(alles, q), diagonale)));
        }
        let z = [zelle(lo.x), zelle(lo.y), zelle(hi.x), zelle(hi.y)];
        let n = (z[2] as i64 - z[0] as i64 + 1) * (z[3] as i64 - z[1] as i64 + 1);
        if n > GROSS {
            Lage::Immer
        } else {
            Lage::Zellen(z)
        }
    }

    fn eintragen(&mut self, p: u32, z: [i32; 4]) {
        for x in z[0]..=z[2] {
            for y in z[1]..=z[3] {
                self.zellen.entry((x, y)).or_default().push(p);
            }
        }
    }

    /// Ein dichtes Raster wird beim Ändern zu einem lockeren.
    fn locker(&mut self) {
        let Some(d) = self.dicht.take() else {
            return;
        };
        for y in d.y0..d.y0 + d.ny {
            for x in d.x0..d.x0 + d.nx {
                let v = d.zelle((x, y));
                if !v.is_empty() {
                    self.zellen.insert((x, y), v.to_vec());
                }
            }
        }
    }

    /// Plätze in Zelle `c`, ohne die, die zu jeder Abfrage gehören.
    pub(super) fn in_zelle(&self, c: (i32, i32)) -> &[u32] {
        match &self.dicht {
            Some(d) => d.zelle(c),
            None => self.zellen.get(&c).map_or(&[], Vec::as_slice),
        }
    }

    pub fn leeren(&mut self) {
        *self = Raster::default();
    }

    /// Trägt Platz `platz` mit Quader `q` ein (`None`: ohne Quader, gehört
    /// zu jeder Abfrage); ersetzt, was dort stand.
    pub fn setze(&mut self, platz: usize, q: Option<Aabb>) {
        let alt = self.quader.get(platz).copied().flatten();
        let am_rand = self.raus(platz);
        if self.lage.len() <= platz {
            self.lage.resize(platz + 1, Lage::Frei);
            self.quader.resize(platz + 1, None);
        }
        let p = platz as u32;
        let lage = self.lage_von(q);
        match lage {
            Lage::Zellen(z) => self.eintragen(p, z),
            _ => self.immer.push(p),
        }
        self.lage[platz] = lage;
        self.quader[platz] = q;
        // Gleich groß oder größer: der Rahmen mit dem neuen deckt den alten
        let deckt = match (alt, q) {
            (Some((a, b)), Some((lo, hi))) => {
                lo.x <= a.x
                    && lo.y <= a.y
                    && lo.z <= a.z
                    && hi.x >= b.x
                    && hi.y >= b.y
                    && hi.z >= b.z
            }
            _ => false,
        };
        if am_rand && !deckt {
            self.rahmen.set(None);
        }
    }

    /// Nimmt Platz `platz` heraus.
    pub fn entferne(&mut self, platz: usize) {
        if self.raus(platz) {
            self.rahmen.set(None);
        }
    }

    /// Quader um alles und längste Diagonale; nach einer Änderung am Rand
    /// aus den Quadern der Plätze.
    fn rahmen(&self) -> (Option<Aabb>, f64) {
        if let Some(r) = self.rahmen.get() {
            return r;
        }
        let mut alles = None;
        let mut diagonale = 0.0f64;
        for &(lo, hi) in self.quader.iter().flatten() {
            alles = super::union(alles, Some((lo, hi)));
            diagonale = diagonale.max((hi - lo).length());
        }
        self.rahmen.set(Some((alles, diagonale)));
        (alles, diagonale)
    }

    /// Nimmt Platz `platz` heraus; `true`, wenn sein Quader am Rand des
    /// Quaders um alles lag oder die längste Diagonale hatte.
    fn raus(&mut self, platz: usize) -> bool {
        let am_rand = match (self.quader.get(platz).copied().flatten(), self.rahmen.get()) {
            (Some((lo, hi)), Some((Some((a, b)), diagonale))) => {
                lo.x <= a.x
                    || lo.y <= a.y
                    || lo.z <= a.z
                    || hi.x >= b.x
                    || hi.y >= b.y
                    || hi.z >= b.z
                    || (hi - lo).length() >= diagonale
            }
            _ => false,
        };
        if let Some(q) = self.quader.get_mut(platz) {
            *q = None;
        }
        self.locker();
        let p = platz as u32;
        match self.lage.get(platz).copied().unwrap_or_default() {
            Lage::Frei => {}
            Lage::Immer => self.immer.retain(|&q| q != p),
            Lage::Zellen(z) => {
                for x in z[0]..=z[2] {
                    for y in z[1]..=z[3] {
                        if let Some(v) = self.zellen.get_mut(&(x, y)) {
                            v.retain(|&q| q != p);
                            if v.is_empty() {
                                self.zellen.remove(&(x, y));
                            }
                        }
                    }
                }
            }
        }
        if let Some(l) = self.lage.get_mut(platz) {
            *l = Lage::Frei;
        }
        am_rand
    }

    /// Längste Diagonale eines eingetragenen Quaders.
    pub fn diagonale(&self) -> f64 {
        self.rahmen().1
    }

    /// Quader um alles Eingetragene.
    pub fn quader(&self) -> Option<Aabb> {
        self.rahmen().0
    }

    /// Plätze, die zu jeder Abfrage gehören.
    pub(super) fn immer(&self) -> &[u32] {
        &self.immer
    }

    /// Zahl der Plätze, auch der freien.
    pub(super) fn plaetze(&self) -> usize {
        self.lage.len()
    }

    /// Spielraum in mm um den Blickstrahl `(o, d)`, der `px` Bildpunkten
    /// entspricht, so groß, wie ihn ein Quader annimmt, den der Strahl damit
    /// berührt: in 3D am fernsten Punkt seines Stücks im Quader um alles,
    /// dazu `tiefer` mm, die das Gefragte hinter dem berührten Punkt liegen
    /// darf. Unendlich, wenn nichts vor der Kamera liegt.
    pub fn rand(&self, cam: &Camera, h: f64, px: f64, (o, d): (Vec3, Vec3), tiefer: f64) -> f64 {
        let Some((lo, hi)) = self.quader() else {
            return 0.0;
        };
        if let Some(half) = cam.ortho {
            return px * 2.0 * half / h;
        }
        // mm je mm Tiefe
        let c = px * 2.0 * (cam.fov_y * 0.5).tan() / h;
        let f = cam.forward();
        let tiefe = |p: Vec3| (p - cam.eye).dot(f);
        let am_fernsten = (0..8)
            .map(|i| {
                tiefe(sk_math::vec3(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                ))
            })
            .fold(f64::MIN, f64::max);
        if am_fernsten <= 0.0 {
            return f64::INFINITY;
        }
        let grob = c * (am_fernsten + tiefer);
        // Ein getroffener Quader liegt bei einem Punkt des Strahlstücks, den
        // Spielraum (höchstens √3 · Rand) daneben: so tief höchstens
        let k = 1.0 - 1.75 * c;
        match stueck(o, d, (lo, hi), grob) {
            Some((t0, t1)) if k > 0.1 => {
                let weit = tiefe(o + d * t0).max(tiefe(o + d * t1)).max(0.0);
                grob.min(c * (weit + tiefer) / k)
            }
            _ => grob,
        }
    }

    /// Plätze, deren Quader das Rechteck (x, y) `lo`–`hi` berühren kann,
    /// aufsteigend.
    pub fn im(&self, lo: (f64, f64), hi: (f64, f64)) -> Vec<usize> {
        let z = [zelle(lo.0), zelle(lo.1), zelle(hi.0), zelle(hi.1)];
        let n = (z[2] as i64 - z[0] as i64 + 1).max(0) * (z[3] as i64 - z[1] as i64 + 1).max(0);
        let mut out: Vec<u32> = self.immer.clone();
        if let Some(d) = &self.dicht {
            // Nur der Teil im Raster
            let (x0, y0) = (z[0].max(d.x0), z[1].max(d.y0));
            let (x1, y1) = (z[2].min(d.x0 + d.nx - 1), z[3].min(d.y0 + d.ny - 1));
            for x in x0..=x1 {
                for y in y0..=y1 {
                    out.extend(d.zelle((x, y)));
                }
            }
        } else if n as usize > self.zellen.len() {
            for (&(x, y), v) in &self.zellen {
                if x >= z[0] && x <= z[2] && y >= z[1] && y <= z[3] {
                    out.extend(v);
                }
            }
        } else {
            for x in z[0]..=z[2] {
                for y in z[1]..=z[3] {
                    out.extend(self.in_zelle((x, y)));
                }
            }
        }
        fertig(out)
    }

    /// Plätze, deren Quader der Strahl `o` + t·`d` (t ≥ 0) mit Spielraum
    /// `rand` (mm) berühren kann, aufsteigend: die Zellen längs seines
    /// Stücks im Quader um alles, mit so vielen Nachbarzellen, wie der
    /// Spielraum reicht, jede Zelle einmal. Wären das mehr Zellen als
    /// Einträge (ferne Kamera in 3D, Review W47 B1), alle Plätze wie ohne
    /// Raster.
    pub fn am_strahl(&self, o: Vec3, d: Vec3, rand: f64) -> Vec<usize> {
        let nur_immer = || fertig(self.immer.clone());
        let Some((lo, hi)) = self.quader() else {
            return nur_immer();
        };
        let Some((t0, t1)) = stueck(o, d, (lo, hi), rand) else {
            return nur_immer();
        };
        let (a, b) = (o + d * t0, o + d * t1);
        // Außerhalb der Zellen des Quaders um alles steht nichts
        let g = [zelle(lo.x), zelle(lo.y), zelle(hi.x), zelle(hi.y)];
        // Nicht auf die Breite des Rasters kappen: Das Stück liegt bis zum
        // Spielraum neben dem Quader, die Spannen unten kappen auf ihn
        let k = (rand / ZELLE).ceil().clamp(0.0, i32::MAX as f64) as i64;
        let (ai, aj, bi, bj) = (zelle(a.x), zelle(a.y), zelle(b.x), zelle(b.y));
        let schritte = (bi as i64 - ai as i64).abs() + (bj as i64 - aj as i64).abs();
        if schritte as usize >= self.lage.len() {
            return self.alle();
        }
        // Je Zeile der Strecke das kleinste und größte x; die Strecke läuft
        // in x und y nur in eine Richtung, die Zeilen kommen der Reihe nach
        let mut zeilen: Vec<(i64, i64, i64)> = Vec::new();
        zellen_der_strecke((a.x, a.y), (b.x, b.y), &mut |(i, j)| {
            let (i, j) = (i as i64, j as i64);
            match zeilen.last_mut() {
                Some(z) if z.0 == j => (z.1, z.2) = (z.1.min(i), z.2.max(i)),
                _ => zeilen.push((j, i, i)),
            }
        });
        if zeilen
            .first()
            .zip(zeilen.last())
            .is_some_and(|(f, l)| f.0 > l.0)
        {
            zeilen.reverse();
        }
        // Je Zeile y des Ergebnisses die Spanne über die Zeilen der Strecke
        // bis k darüber und darunter; weil beide Enden der Spannen mit y
        // nur in eine Richtung laufen, liegt das Äußerste am Rand des
        // Fensters
        let (y0, y1) = (
            (zeilen[0].0 - k).max(g[1] as i64),
            (zeilen[zeilen.len() - 1].0 + k).min(g[3] as i64),
        );
        let mut spannen = Vec::new();
        let (mut von, mut bis, mut zahl) = (0, 0, 0usize);
        for y in y0..=y1 {
            while zeilen[von].0 < y - k {
                von += 1;
            }
            while bis + 1 < zeilen.len() && zeilen[bis + 1].0 <= y + k {
                bis += 1;
            }
            let (p, q) = (zeilen[von], zeilen[bis]);
            let x0 = (p.1.min(q.1) - k).max(g[0] as i64);
            let x1 = (p.2.max(q.2) + k).min(g[2] as i64);
            if x0 <= x1 {
                zahl += (x1 - x0 + 1) as usize;
                spannen.push((y as i32, x0 as i32, x1 as i32));
            }
            if zahl >= self.lage.len() {
                return self.alle();
            }
        }
        let mut out = self.immer.clone();
        for (y, x0, x1) in spannen {
            for x in x0..=x1 {
                out.extend(self.in_zelle((x, y)));
            }
        }
        fertig(out)
    }

    /// Alle eingetragenen Plätze, aufsteigend.
    pub(super) fn alle(&self) -> Vec<usize> {
        (0..self.lage.len())
            .filter(|&i| self.lage[i] != Lage::Frei)
            .collect()
    }
}

fn fertig(mut v: Vec<u32>) -> Vec<usize> {
    v.sort_unstable();
    v.dedup();
    v.into_iter().map(|p| p as usize).collect()
}

/// Stück [t0, t1] (t ≥ 0) des Strahls im Quader, um `rand` vergrößert.
fn stueck(o: Vec3, d: Vec3, (lo, hi): Aabb, rand: f64) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
    for (o, d, lo, hi) in [
        (o.x, d.x, lo.x, hi.x),
        (o.y, d.y, lo.y, hi.y),
        (o.z, d.z, lo.z, hi.z),
    ] {
        let (lo, hi) = (lo - rand, hi + rand);
        if d.abs() < 1e-12 {
            if o < lo || o > hi {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo - o) / d, (hi - o) / d);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t0 > t1 {
            return None;
        }
    }
    t1.is_finite().then_some((t0, t1))
}

/// Ruft `f` für jede Zelle, die die Strecke `a`–`b` durchläuft (Schritt
/// für Schritt von Zellgrenze zu Zellgrenze).
fn zellen_der_strecke(a: (f64, f64), b: (f64, f64), f: &mut impl FnMut((i32, i32))) {
    let (mut i, mut j) = (zelle(a.0), zelle(a.1));
    let (bi, bj) = (zelle(b.0), zelle(b.1));
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let schritt = |d: f64| if d > 0.0 { 1 } else { -1 };
    let (si, sj) = (schritt(dx), schritt(dy));
    // Weg (Anteil der Strecke) bis zur nächsten Zellgrenze und je Zelle
    let bis = |v: f64, d: f64, c: i32| {
        if d.abs() < 1e-12 {
            f64::INFINITY
        } else {
            let g = if d > 0.0 { c + 1 } else { c } as f64 * ZELLE;
            (g - v) / d
        }
    };
    let je = |d: f64| {
        if d.abs() < 1e-12 {
            f64::INFINITY
        } else {
            ZELLE / d.abs()
        }
    };
    let (mut tx, mut ty) = (bis(a.0, dx, i), bis(a.1, dy, j));
    let (jx, jy) = (je(dx), je(dy));
    f((i, j));
    let n = (bi as i64 - i as i64).abs() + (bj as i64 - j as i64).abs();
    for _ in 0..n {
        if j == bj || (i != bi && tx < ty) {
            i += si;
            tx += jx;
        } else {
            j += sj;
            ty += jy;
        }
        f((i, j));
    }
}

/// Bereich (flach) um den Pfad der Öffnung `id`, in dem die Maus ihre
/// Griffe zeigen kann ([`crate::wall_edit::tuer_edit::griff_reichweite`]).
fn griffbereich(m: &Model, id: ElementId) -> Option<Aabb> {
    let o = m.opening(id)?;
    let r = crate::wall_edit::tuer_edit::griff_reichweite(o.width());
    let (mut lo, mut hi) = (vec3(f64::MAX, f64::MAX, 0.0), vec3(f64::MIN, f64::MIN, 0.0));
    for p in &o.path {
        (lo.x, lo.y) = (lo.x.min(p.x), lo.y.min(p.y));
        (hi.x, hi.y) = (hi.x.max(p.x), hi.y.max(p.y));
    }
    (lo.x <= hi.x).then(|| (lo - vec3(r, r, 0.0), hi + vec3(r, r, 0.0)))
}

impl Scene {
    /// Berechnete Wandzüge mit dem Quader ihres Körpers, die das Rechteck
    /// (x, y) `lo`–`hi` berühren können (Vortest beim Zeigen, Review #82 1
    /// und L26).
    pub fn zuege_im(
        &self,
        lo: (f64, f64),
        hi: (f64, f64),
    ) -> impl Iterator<Item = (RunId, &WallChain, Option<Aabb>)> + '_ {
        self.raster
            .im(lo, hi)
            .into_iter()
            .filter_map(|i| self.cache.get(i)?.as_ref())
            .map(|c| (c.id, &c.chain, c.bounds))
    }

    /// Trägt den Zug `rc` auf Platz `platz` mit Quader `q` ein und seine
    /// Öffnungen mit dem Bereich ihrer Griffe ([`griffbereich`]).
    pub(super) fn raster_setzen(&mut self, platz: usize, q: Option<Aabb>, rc: &super::RunCache) {
        self.raster.setze(platz, q);
        let bereich = rc
            .loecher
            .iter()
            .filter_map(|(h, _)| griffbereich(&self.model, h.opening?))
            .fold(None, |acc, b| super::union(acc, Some(b)));
        match bereich {
            Some(b) => self.griffraster.setze(platz, Some(b)),
            None => self.griffraster.entferne(platz),
        }
    }

    /// Nimmt den Zug auf Platz `platz` aus beiden Rastern.
    pub(super) fn raster_entfernen(&mut self, platz: usize) {
        self.raster.entferne(platz);
        self.griffraster.entferne(platz);
    }

    pub(super) fn raster_leeren(&mut self) {
        self.raster.leeren();
        self.griffraster.leeren();
    }

    /// Öffnungen mit einem Loch in einem berechneten Wandzug, deren Griffe
    /// die Maus im Punkt `p` des Grundrisses zeigen kann, mit Spielraum `r`
    /// (mm, das Quadrat um `p`), je mit dem Zug: nach Bauteil, dann Zug
    /// aufsteigend. Gefragt wird nur das Raster der Griffe, so kostet jede
    /// Mausbewegung gleich viel, wie groß das Modell und wie lang sein
    /// längster Zug auch ist (Review #149 B1).
    pub fn oeffnungen_bei(&self, p: Vec3, r: f64) -> Vec<(ElementId, RunId)> {
        let (lo, hi) = ((p.x - r, p.y - r), (p.x + r, p.y + r));
        let trifft = |(a, b): Aabb| a.x <= hi.0 && b.x >= lo.0 && a.y <= hi.1 && b.y >= lo.1;
        let mut out = Vec::new();
        for platz in self.griffraster.im(lo, hi) {
            let Some(c) = self.cache.get(platz).and_then(Option::as_ref) else {
                continue;
            };
            for (h, _) in &c.loecher {
                let Some(id) = h.opening else {
                    continue;
                };
                if griffbereich(&self.model, id).is_some_and(trifft) {
                    out.push((id, c.id));
                }
            }
        }
        out.sort_by_key(|(id, run)| (id.index(), run.index()));
        out.dedup();
        out
    }

    /// Plätze der Wandzüge, die der Strahl mit Spielraum `rand` (mm)
    /// treffen kann.
    pub(super) fn plaetze_am_strahl(&self, o: Vec3, d: Vec3, rand: f64) -> Vec<usize> {
        self.raster.am_strahl(o, d, rand)
    }

    /// Wie [`Scene::feet`] und [`Scene::stacked_feet`] zusammen, nur die
    /// Züge, deren Fuß bis `px` Bildpunkte am Blickstrahl `(o, d)` liegen
    /// kann (Gummiband greifen, Review L26).
    pub fn feet_am_strahl(
        &self,
        cam: &Camera,
        h: f64,
        (o, d): (Vec3, Vec3),
        px: f64,
    ) -> impl Iterator<Item = (RunId, Option<Aabb>, &[(Vec3, Vec3)])> + '_ {
        let rand = self.raster.rand(cam, h, px, (o, d), 0.0);
        // Erst die Füße, dann die gestapelten, je nach Platz wie bisher
        let mut zuege: Vec<&super::RunCache> = self
            .raster
            .am_strahl(o, d, rand)
            .into_iter()
            .filter_map(|i| self.cache.get(i)?.as_ref())
            .collect();
        zuege.sort_by_key(|c| c.below.is_some());
        zuege
            .into_iter()
            .map(|c| (c.id, c.foot_bounds, c.foot.as_slice()))
    }
}

#[cfg(test)]
#[path = "scene_raster_probe.rs"]
mod probe;

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use sk_math::{dist_to_segment, vec3};
    use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
    use sk_model::Model;

    fn kasten(x: f64, y: f64, b: f64) -> Option<Aabb> {
        Some((vec3(x, y, 0.0), vec3(x + b, y + b, 3000.0)))
    }

    #[test]
    fn rechteck_findet_nur_die_nahen() {
        // 100 × 100 Kästen im Abstand 20 m
        let r = Raster::aus((0..10_000).map(|i| {
            kasten(
                (i % 100) as f64 * 20_000.0,
                (i / 100) as f64 * 20_000.0,
                10_000.0,
            )
        }));
        let p = r.im((45_000.0, 45_000.0), (46_000.0, 46_000.0));
        assert!(p.len() <= 4, "{p:?}");
        assert!(p.contains(&(2 * 100 + 2)));
        // Was das Rechteck trifft, ist immer dabei
        let p = r.im((15_000.0, 0.0), (21_000.0, 1.0));
        assert!(p.contains(&1), "{p:?}");
        // Großes Rechteck: alle
        assert_eq!(r.im((-1e9, -1e9), (1e9, 1e9)).len(), 10_000);
    }

    /// Geht ein Quader am Rand, schrumpfen der Quader um alles und die
    /// längste Diagonale mit; einer innen ändert nichts (Review H2 zu #147).
    #[test]
    fn quader_um_alles_folgt() {
        let mut r = Raster::default();
        r.setze(0, kasten(0.0, 0.0, 5000.0));
        r.setze(1, kasten(20_000.0, 0.0, 5000.0));
        r.setze(2, kasten(500_000.0, 300_000.0, 40_000.0));
        r.setze(3, kasten(10_000.0, 1000.0, 1000.0));
        let um = |a: [usize; 2], b: f64| {
            Some((vec3(a[0] as f64, a[1] as f64, 0.0), vec3(b, 5000.0, 3000.0)))
        };
        assert_eq!(r.quader().unwrap().1.x, 540_000.0);
        // Der Ferne geht: zurück auf die drei nahen
        r.entferne(2);
        assert_eq!(r.quader(), um([0, 0], 25_000.0));
        assert_eq!(r.diagonale(), vec3(5000.0, 5000.0, 3000.0).length());
        // Einer innen geht: gleich
        r.entferne(3);
        assert_eq!(r.quader(), um([0, 0], 25_000.0));
        // Der rechte rückt nach innen
        r.setze(1, kasten(8000.0, 0.0, 5000.0));
        assert_eq!(r.quader(), um([0, 0], 13_000.0));
        // Wie neu gebaut
        let neu = Raster::aus([kasten(0.0, 0.0, 5000.0), kasten(8000.0, 0.0, 5000.0)]);
        assert_eq!(r.quader(), neu.quader());
        assert_eq!(r.diagonale(), neu.diagonale());
        r.entferne(0);
        r.entferne(1);
        assert_eq!(r.quader(), None);
        assert_eq!(r.diagonale(), 0.0);
    }

    /// Viele gleich große Quader, jeder mit der längsten Diagonale: Wird
    /// einer gleich groß oder größer neu eingetragen, bleibt der Rahmen ohne
    /// Rechnen über alle; rücken die am Rand nach innen, rechnet ihn erst das
    /// nächste Lesen, einmal für alle (Review B2 zu #151: sonst je Zug über
    /// alle Plätze, 247 ms für viele Züge bei 4000 Häusern).
    #[test]
    fn rahmen_traege() {
        let ort = |i: usize| ((i % 50) as f64 * 20_000.0, (i / 50) as f64 * 20_000.0);
        let mut r = Raster::aus((0..2000).map(|i| kasten(ort(i).0, ort(i).1, 5000.0)));
        let voll = r.quader();
        assert_eq!(voll.unwrap().1.x, 985_000.0);
        for i in 0..2000 {
            r.setze(i, kasten(ort(i).0, ort(i).1, 5000.0));
            assert!(r.rahmen.get().is_some(), "Platz {i}");
        }
        assert_eq!(r.quader(), voll);
        // Einer wächst: nur dazu
        r.setze(7, kasten(ort(7).0 - 1000.0, ort(7).1 - 1000.0, 7000.0));
        assert!(r.rahmen.get().is_some());
        assert_eq!(r.diagonale(), vec3(7000.0, 7000.0, 3000.0).length());
        // Die am rechten Rand rücken nach innen: erst das Lesen rechnet
        for i in (49..2000).step_by(50) {
            r.setze(i, kasten(ort(i).0 - 10_000.0, ort(i).1, 5000.0));
        }
        assert!(r.rahmen.get().is_none());
        let neu = Raster::aus((0..2000).map(|i| r.quader[i]));
        assert_eq!(r.quader(), neu.quader());
        assert_eq!(r.quader().unwrap().1.x, 975_000.0);
        assert_eq!(r.diagonale(), neu.diagonale());
        assert!(r.rahmen.get().is_some());
    }

    #[test]
    fn entfernen_und_ersetzen() {
        let mut r = Raster::aus([kasten(0.0, 0.0, 5000.0), kasten(50_000.0, 0.0, 5000.0)]);
        assert_eq!(r.im((1000.0, 1000.0), (1000.0, 1000.0)), vec![0]);
        r.setze(0, kasten(50_000.0, 0.0, 5000.0));
        assert!(r.im((1000.0, 1000.0), (1000.0, 1000.0)).is_empty());
        assert_eq!(r.im((51_000.0, 1000.0), (51_000.0, 1000.0)), vec![0, 1]);
        r.entferne(1);
        assert_eq!(r.im((51_000.0, 1000.0), (51_000.0, 1000.0)), vec![0]);
        // Ohne Quader und übergroß: in jeder Abfrage
        r.setze(3, None);
        r.setze(4, Some((vec3(-1e9, -1e9, 0.0), vec3(1e9, 1e9, 1.0))));
        assert_eq!(r.im((1e6, 1e6), (1e6, 1e6)), vec![3, 4]);
        assert_eq!(
            r.am_strahl(vec3(1e6, 1e6, 1e5), vec3(0.0, 0.0, -1.0), 0.0),
            vec![3, 4]
        );
    }

    /// Ein kleines Raster (eine Zelle) und ein Strahl, der weiter als das
    /// Raster breit ist, aber im Spielraum daneben vorbeigeht (Geometrie zu
    /// #142): der Platz ist dabei.
    #[test]
    fn spielraum_breiter_als_raster() {
        let q = (vec3(1000.0, 1000.0, 0.0), vec3(9000.0, 9000.0, 3000.0));
        let r = Raster::aus([Some(q)]);
        let d = vec3(0.0, 0.0, -1.0);
        assert_eq!(
            r.am_strahl(vec3(24_000.0, 5000.0, 5000.0), d, 16_000.0),
            vec![0]
        );
        assert_eq!(
            r.am_strahl(vec3(5000.0, -20_000.0, 5000.0), d, 25_000.0),
            vec![0]
        );
        assert!(r
            .am_strahl(vec3(5000.0, -20_000.0, 5000.0), d, 20_000.0)
            .is_empty());
    }

    /// Jeder Quader, den der Strahl mit Spielraum trifft, ist dabei;
    /// senkrecht von oben nur die Zellen unter dem Punkt.
    #[test]
    fn strahl_verpasst_nichts() {
        let q: Vec<Option<Aabb>> = (0..2500)
            .map(|i| {
                kasten(
                    (i % 50) as f64 * 17_000.0,
                    (i / 50) as f64 * 13_000.0,
                    9000.0,
                )
            })
            .collect();
        let r = Raster::aus(q.iter().copied());
        let von_oben = r.am_strahl(vec3(100_000.0, 100_000.0, 1e6), vec3(0.0, 0.0, -1.0), 0.0);
        assert!(von_oben.len() <= 4, "{von_oben:?}");
        let naeher = |o: Vec3, d: Vec3, (lo, hi): Aabb, e: f64| stueck(o, d, (lo, hi), e).is_some();
        for (k, (o, d)) in [
            (vec3(-50_000.0, -40_000.0, 30_000.0), vec3(1.0, 0.8, -0.05)),
            (vec3(900_000.0, 10_000.0, 5_000.0), vec3(-1.0, 0.31, -0.001)),
            (vec3(400_000.0, 300_000.0, 200_000.0), vec3(0.1, -0.2, -1.0)),
            (vec3(10_000.0, 600_000.0, 1500.0), vec3(0.0, -1.0, 0.0)),
        ]
        .into_iter()
        .enumerate()
        {
            let d = d.normalized();
            for e in [0.0, 3000.0, 25_000.0] {
                let p = r.am_strahl(o, d, e);
                for (i, q) in q.iter().enumerate() {
                    if naeher(o, d, q.unwrap(), e) {
                        assert!(p.contains(&i), "Strahl {k}, Rand {e}: Quader {i} fehlt");
                    }
                }
                assert!(p.len() < q.len() / 2, "Strahl {k}, Rand {e}: {}", p.len());
            }
        }
    }

    /// Ferne Kamera (Review W47 B1): Der Spielraum reicht über viele Zellen,
    /// auch über 64 (H1); jede Zelle zählt einmal, und wären es mehr Zellen
    /// als Einträge, kommen alle Plätze wie ohne Raster. Nichts fehlt.
    #[test]
    fn weiter_spielraum_verpasst_nichts_und_hoechstens_alle() {
        let q: Vec<Option<Aabb>> = (0..2500)
            .map(|i| {
                kasten(
                    (i % 50) as f64 * 17_000.0,
                    (i / 50) as f64 * 13_000.0,
                    9000.0,
                )
            })
            .collect();
        let r = Raster::aus(q.iter().copied());
        let mut fast = 0;
        for (k, (o, d)) in [
            (vec3(-3e6, -4e6, 4e6), vec3(0.5, 0.6, -0.5)),
            (vec3(425_000.0, -2e6, 3e6), vec3(0.0, 0.6, -0.8)),
            (vec3(-1e6, 300_000.0, 10_000.0), vec3(1.0, 0.0, -0.005)),
        ]
        .into_iter()
        .enumerate()
        {
            let d = d.normalized();
            for e in [40_000.0, 120_000.0, 800_000.0, 1e9] {
                let p = r.am_strahl(o, d, e);
                for (i, b) in q.iter().enumerate() {
                    if stueck(o, d, b.unwrap(), e).is_some() {
                        assert!(p.contains(&i), "Strahl {k}, Rand {e}: Quader {i} fehlt");
                    }
                }
                assert!(p.len() <= q.len() && p.windows(2).all(|w| w[0] < w[1]));
                if e >= 800_000.0 {
                    assert_eq!(p.len(), q.len(), "Strahl {k}, Rand {e}: alle");
                } else if p.len() < q.len() {
                    fast += 1;
                }
            }
        }
        assert!(
            fast >= 3,
            "mit 40 und 120 m bleibt es meist ein Teil: {fast}"
        );
    }

    pub(in crate::scene) const W: f64 = 1600.0;
    pub(in crate::scene) const H: f64 = 900.0;

    /// 7 × 7 Häuser 10 × 8 m im Abstand von 18 m, je mit einem Fenster.
    pub(in crate::scene) fn siedlung() -> Scene {
        let mut m = Model::new();
        m.begin("Siedlung");
        for i in 0..49 {
            let (ox, oy) = ((i % 7) as f64 * 18_000.0, (i / 7) as f64 * 18_000.0);
            let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
            let b = m.add_building(1);
            let pts = [
                p(0.0, 0.0),
                p(0.0, 8000.0),
                p(10_000.0, 8000.0),
                p(10_000.0, 0.0),
            ];
            let eg = m.build_from_polygon(b, &pts).expect("Haus");
            let st = m.run(eg).unwrap().storey;
            m.add_opening(Opening {
                kind: OpeningKind::Window,
                path: vec![p(2510.0, 0.0), p(1500.0, 0.0)],
                bottom: HeightRef::Line(st, HeightLine::Sill),
                top: HeightRef::Line(st, HeightLine::Lintel),
                reveal: None,
                install: Default::default(),
                swing: None,
                lock: Vec::new(),
            })
            .expect("Fenster");
        }
        m.commit();
        Scene::with_model(m)
    }

    /// Jeder berechnete Zug steht im Raster, und nur sie.
    fn raster_stimmt(s: &Scene) {
        for (i, c) in s.cache.iter().enumerate() {
            let Some((lo, hi)) = c.as_ref().and_then(|c| c.bounds) else {
                continue;
            };
            let mitte = ((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
            assert!(s.raster.im(mitte, mitte).contains(&i), "Platz {i}");
        }
        for i in s.raster.im((-1e9, -1e9), (1e9, 1e9)) {
            assert!(s.cache.get(i).is_some_and(Option::is_some), "Platz {i}");
        }
        // Der Quader um alles ist der um die Züge, die da sind (H2 zu #147)
        let um_alle = s.cache.iter().flatten().fold(None, |a, c| {
            let q = c
                .bounds
                .map(|b| super::super::union(Some(b), c.foot_bounds).unwrap_or(b));
            super::super::union(a, q)
        });
        assert_eq!(s.raster.quader(), um_alle);
    }

    /// Nächster Treffer über alle Züge, wie vor dem Raster.
    fn ueber_alle(s: &Scene, o: Vec3, d: Vec3) -> Option<f64> {
        s.cache
            .iter()
            .flatten()
            .filter(|c| {
                c.bounds
                    .is_some_and(|b| super::super::ray_hits_box(o, d, b))
            })
            .filter_map(|c| Some(c.solid.raycast_where(o, d, |_| true)?.0))
            .min_by(f64::total_cmp)
    }

    pub(in crate::scene) fn kameras() -> [Camera; 3] {
        let mitte = vec3(60_000.0, 60_000.0, 0.0);
        [
            Camera::parallel(mitte, 0.3, -89.9f64.to_radians(), 70_000.0),
            Camera::looking_at(vec3(-30_000.0, -40_000.0, 25_000.0), mitte, 45.0),
            Camera::looking_at(
                vec3(150_000.0, 20_000.0, 1700.0),
                vec3(0.0, 90_000.0, 1500.0),
                60.0,
            ),
        ]
    }

    /// Strahl, Wandwahl und Gummiband treffen über das Raster dasselbe wie
    /// über alle Züge, auch nach Ziehen, Löschen und Rückgängig (Review L26).
    #[test]
    fn szene_trifft_wie_ueber_alle() {
        let mut s = siedlung();
        raster_stimmt(&s);
        assert_eq!(
            s.cache
                .iter()
                .flatten()
                .filter(|c| c.bounds.is_some())
                .count(),
            49
        );
        let pruefen = |s: &Scene| {
            let (mut treffer, mut fuesse) = (0, 0);
            for cam in kameras() {
                for k in 0..(24 * 14) {
                    let (x, y) = ((k % 24) as f64 * W / 23.0, (k / 24) as f64 * H / 13.0);
                    let (o, d) = cam.ray(x, y, W, H);
                    let t = s.raycast(o, d).map(|t| t.0);
                    assert_eq!(t, ueber_alle(s, o, d), "{x} {y}");
                    treffer += usize::from(t.is_some());
                    // Gummiband: jeder Fuß bis 12 Bildpunkte neben der Maus
                    let nah: Vec<RunId> = s
                        .feet_am_strahl(&cam, H, (o, d), 12.0)
                        .map(|f| f.0)
                        .collect();
                    for c in s.cache.iter().flatten() {
                        let bei = c.foot.iter().any(|&(a, b)| {
                            match (cam.project(a, W, H), cam.project(b, W, H)) {
                                (Some(pa), Some(pb)) => dist_to_segment((x, y), pa, pb) < 12.0,
                                _ => false,
                            }
                        });
                        assert!(!bei || nah.contains(&c.id), "{x} {y}: Fuß von {:?}", c.id);
                        fuesse += usize::from(bei);
                    }
                    assert!(nah.len() < 49, "{x} {y}: {}", nah.len());
                }
            }
            assert!(
                treffer > 50 && fuesse > 50,
                "{treffer} Treffer, {fuesse} Füße"
            );
            // Löcher aus dem Zwischenspeicher wie aus dem Zug
            for c in s.cache.iter().flatten() {
                let q: Vec<_> = c
                    .chain
                    .hole_quads()
                    .into_iter()
                    .map(|(h, q)| (h.clone(), q))
                    .collect();
                assert_eq!(s.loecher(c.id), q.as_slice());
            }
        };
        pruefen(&s);
        // Eine Wand ziehen und loslassen
        let run = s.model().runs().ids().nth(10).unwrap();
        let moved = s.chain(run).unwrap().with_segment_moved(1, 3000.0).unwrap();
        s.begin("Wand verschieben");
        assert!(s.set_run_points(run, &moved.points));
        s.commit();
        s.rebuild_dirty(false);
        raster_stimmt(&s);
        pruefen(&s);
        // Ein Haus löschen, dann zurück
        let wand = s.model().wall_at(run, 0).unwrap();
        s.delete_elements(&[wand]);
        s.rebuild_dirty(false);
        raster_stimmt(&s);
        pruefen(&s);
        assert!(s.undo());
        s.rebuild_dirty(false);
        raster_stimmt(&s);
        pruefen(&s);
    }

    /// Geht die ferne Wand, ist der Quader um alles wieder der kleine Rahmen
    /// um die beiden Häuser, ebenso, wenn sie heranrückt; beides über die
    /// Szene Zug für Zug, nicht neu gebaut (Review H2 zu #147).
    #[test]
    fn ferne_wand_geht_rahmen_schrumpft() {
        use sk_model::{Category, RefSide};
        let mut m = Model::new();
        m.begin("Zwei Häuser, eine Wand fern");
        let mut st = None;
        for ox in [0.0, 18_000.0] {
            let p = |x: f64, y: f64| vec3(ox + x, y, 0.0);
            let b = m.add_building(1);
            let pts = [
                p(0.0, 0.0),
                p(0.0, 8000.0),
                p(10_000.0, 8000.0),
                p(10_000.0, 0.0),
            ];
            let eg = m.build_from_polygon(b, &pts).expect("Haus");
            st = Some(m.run(eg).unwrap().storey);
        }
        let fern = [vec3(2_000_000.0, 0.0, 0.0), vec3(2_004_000.0, 0.0, 0.0)];
        let set = m.defaults().interior_wall;
        let (st, mitte) = (st.unwrap(), RefSide::Center);
        let wand = m
            .add_wall_run(&fern, false, mitte, st, set, Category::InteriorWall)
            .unwrap();
        m.commit();
        let mut s = Scene::with_model(m);
        let gross = s.raster.quader().unwrap();
        assert!(gross.1.x > 2_000_000.0);
        let klein = |s: &Scene| {
            let (lo, hi) = s.raster.quader().unwrap();
            lo.x > -1000.0 && (28_000.0..29_000.0).contains(&hi.x)
        };
        // Löschen
        let seg = s.model().run(wand).unwrap().segments[0];
        assert!(!s.delete_elements(&[seg]).removed.is_empty());
        s.rebuild_dirty(false);
        raster_stimmt(&s);
        assert!(klein(&s), "{:?}", s.raster.quader());
        assert!(s.raster.diagonale() < 20_000.0);
        assert!(s.undo());
        s.rebuild_dirty(false);
        raster_stimmt(&s);
        assert_eq!(s.raster.quader(), Some(gross));
        // Heranrücken
        let nah = [vec3(5000.0, 2000.0, 0.0), vec3(5000.0, 6000.0, 0.0)];
        s.begin("Wand verschieben");
        assert!(s.set_run_points(wand, &nah));
        s.commit();
        s.rebuild_dirty(false);
        raster_stimmt(&s);
        assert!(klein(&s), "{:?}", s.raster.quader());
    }
}

//! Dreiecksnetz durch die Messpunkte (plan-gelaendemodul.md §7.1):
//! Delaunay, inkrementell nach Bowyer und Watson mit Geisterdreiecken außen
//! am Rand, Bruchkanten nach Sloan erzwungen (kreuzende Kanten tauschen,
//! danach Delaunay wiederherstellen, ohne Bruchkanten zu tauschen), Rand auf
//! Wunsch beschnitten.
//!
//! Gerechnet wird auf dem Raster aus [`crate::praedikate`] mit exakten
//! Tests; die Reihenfolge des Einfügens ist fest (Hilbert-Kurve, bei
//! Gleichstand Lage und Nummer). Dasselbe Aufmaß ergibt so auf jedem Rechner
//! dasselbe Netz.

use crate::praedikate::{auf_strecke, kreuzen, orient, raster, umkreis, Ip};
use std::collections::{BTreeSet, VecDeque};

/// Ecke „unendlich“ der Geisterdreiecke außen am Rand.
const INF: u32 = u32::MAX;
/// Kein Dreieck.
const NIX: u32 = u32::MAX;

/// Rand des Netzes (E9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rand {
    /// Konvexe Hülle aller Punkte („wie gemessen“).
    Huelle,
    /// Dreiecke am Rand mit einer Randkante länger als das Vierfache der
    /// mittleren Kantenlänge fallen weg, solange kein Punkt allein steht.
    #[default]
    Beschnitten,
}

/// Das fertige Netz: Dreiecke gegen den Uhrzeigersinn über den Punkten in
/// Eingabereihenfolge.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Netz {
    /// x, y, z in m wie übergeben.
    pub punkte: Vec<[f64; 3]>,
    pub dreiecke: Vec<[u32; 3]>,
    /// Erzwungene Bruchkanten, je kleinere Nummer zuerst.
    pub bruch: Vec<[u32; 2]>,
    /// Punkte, die nicht im Netz liegen (gleiche Lage wie ein früherer,
    /// zu weit weg, nicht endlich).
    pub aussen: Vec<u32>,
    /// Was beim Bauen auffiel, als Sätze.
    pub hinweise: Vec<String>,
}

/// Triangulation im Bau: Ecken, Dreiecke und Nachbarn. Nachbar `i` liegt
/// gegenüber Ecke `i`, also an der Kante `(t[i+1], t[i+2])`. Ein
/// Geisterdreieck `(u, v, INF)` liegt außen an der Randkante, deren Inneres
/// rechts von u → v liegt.
struct Dt {
    p: Vec<Ip>,
    tri: Vec<[u32; 3]>,
    nb: Vec<[u32; 3]>,
    tot: Vec<bool>,
    frei: Vec<u32>,
    /// Ein Dreieck je Ecke, von dem aus man die Ecke umläuft.
    ecke: Vec<u32>,
    /// Zuletzt gebaut: Anfang der Suche.
    letzt: u32,
    /// Merker für die Suche nach der Höhle.
    marke: Vec<u32>,
    runde: u32,
    /// Bruchkanten, je kleinere Nummer zuerst.
    fest: BTreeSet<(u32, u32)>,
}

fn kante(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

impl Dt {
    fn geist(&self, t: u32) -> bool {
        self.tri[t as usize][2] == INF
    }

    /// d liegt im Umkreis von t (beim Geisterdreieck: außen jenseits der
    /// Randkante oder auf ihr).
    fn drin(&self, t: u32, d: Ip) -> bool {
        let [a, b, c] = self.tri[t as usize];
        let (pa, pb) = (self.p[a as usize], self.p[b as usize]);
        if c == INF {
            let o = orient(pa, pb, d);
            o > 0 || (o == 0 && auf_strecke(pa, pb, d))
        } else {
            umkreis(pa, pb, self.p[c as usize], d) > 0
        }
    }

    fn neu(&mut self, t: [u32; 3]) -> u32 {
        let k = match self.frei.pop() {
            Some(k) => {
                self.tri[k as usize] = t;
                self.nb[k as usize] = [NIX; 3];
                self.tot[k as usize] = false;
                k
            }
            None => {
                self.tri.push(t);
                self.nb.push([NIX; 3]);
                self.tot.push(false);
                self.marke.push(0);
                (self.tri.len() - 1) as u32
            }
        };
        for v in t {
            if v != INF {
                self.ecke[v as usize] = k;
            }
        }
        self.letzt = k;
        k
    }

    /// Nachbar von t an der gerichteten Kante a → b setzen.
    fn setze(&mut self, t: u32, a: u32, b: u32, n: u32) {
        let v = self.tri[t as usize];
        let i = (0..3)
            .find(|&i| v[(i + 1) % 3] == a && v[(i + 2) % 3] == b)
            .expect("Kante im Dreieck");
        self.nb[t as usize][i] = n;
    }

    /// Ecken mit „unendlich“ an letzter Stelle, Umlauf erhalten.
    fn gedreht(t: [u32; 3]) -> [u32; 3] {
        match t.iter().position(|&v| v == INF) {
            Some(0) => [t[1], t[2], t[0]],
            Some(1) => [t[2], t[0], t[1]],
            _ => t,
        }
    }

    /// Anfangsdreieck aus drei Punkten, die nicht auf einer Linie liegen,
    /// samt drei Geisterdreiecken.
    fn start(p: Vec<Ip>, a: u32, b: u32, c: u32) -> Dt {
        let n = p.len();
        let mut d = Dt {
            p,
            tri: Vec::new(),
            nb: Vec::new(),
            tot: Vec::new(),
            frei: Vec::new(),
            ecke: vec![NIX; n],
            letzt: 0,
            marke: Vec::new(),
            runde: 0,
            fest: BTreeSet::new(),
        };
        let (a, b, c) = if orient(d.p[a as usize], d.p[b as usize], d.p[c as usize]) > 0 {
            (a, b, c)
        } else {
            (a, c, b)
        };
        let t = d.neu([a, b, c]);
        let g = [d.neu([c, b, INF]), d.neu([a, c, INF]), d.neu([b, a, INF])];
        // Dreieck: gegenüber a die Kante b → c usw.
        d.nb[t as usize] = g;
        // Geist (u, v, INF): gegenüber u der nächste Geist ab v, gegenüber v
        // der vorige bis u, gegenüber INF das Dreieck
        let gv: Vec<[u32; 3]> = g.iter().map(|&h| d.tri[h as usize]).collect();
        let ab = |x: u32| g[gv.iter().position(|v| v[0] == x).unwrap()];
        let bis = |x: u32| g[gv.iter().position(|v| v[1] == x).unwrap()];
        for (k, &gk) in g.iter().enumerate() {
            let [u, v, _] = gv[k];
            d.nb[gk as usize] = [ab(v), bis(u), t];
        }
        d
    }

    /// Ein Dreieck, in dessen Umkreis `q` liegt: Gang durch das Netz von
    /// zuletzt Gebautem aus, über die Randkante hinaus in den Geist.
    fn suchen(&mut self, q: Ip) -> u32 {
        let mut t = self.letzt;
        if self.tot[t as usize] {
            t = (0..self.tri.len() as u32)
                .find(|&k| !self.tot[k as usize])
                .unwrap();
        }
        if self.geist(t) {
            t = self.nb[t as usize][2];
        }
        let mut dreh = 0usize;
        let mut schritte = 0usize;
        'gang: loop {
            schritte += 1;
            if schritte > 4 * self.tri.len() + 16 {
                // Sicherheitsnetz: alle Dreiecke durchsehen
                return (0..self.tri.len() as u32)
                    .find(|&k| !self.tot[k as usize] && self.drin(k, q))
                    .unwrap();
            }
            let v = self.tri[t as usize];
            dreh = (dreh + 1) % 3;
            for j in 0..3 {
                let i = (j + dreh) % 3;
                let (a, b) = (v[(i + 1) % 3], v[(i + 2) % 3]);
                if orient(self.p[a as usize], self.p[b as usize], q) < 0 {
                    let n = self.nb[t as usize][i];
                    if self.geist(n) {
                        return n;
                    }
                    t = n;
                    continue 'gang;
                }
            }
            return t;
        }
    }

    /// Punkt `k` einfügen; `Some(j)`, wenn er auf Ecke j fällt.
    fn einfuegen(&mut self, k: u32) -> Option<u32> {
        let q = self.p[k as usize];
        let t0 = self.suchen(q);
        if let Some(&v) = self.tri[t0 as usize]
            .iter()
            .find(|&&v| v != INF && self.p[v as usize] == q)
        {
            return Some(v);
        }
        // Höhle: alle verbundenen Dreiecke, in deren Umkreis q liegt
        self.runde += 1;
        let r = self.runde;
        let mut hoehle = vec![t0];
        self.marke[t0 as usize] = r;
        let mut rand: Vec<(u32, u32, u32)> = Vec::new();
        let mut i = 0;
        while i < hoehle.len() {
            let t = hoehle[i];
            i += 1;
            let v = self.tri[t as usize];
            for j in 0..3 {
                let n = self.nb[t as usize][j];
                let (a, b) = (v[(j + 1) % 3], v[(j + 2) % 3]);
                if self.marke[n as usize] == r {
                    continue;
                }
                if self.drin(n, q) {
                    if let Some(&w) = self.tri[n as usize]
                        .iter()
                        .find(|&&w| w != INF && self.p[w as usize] == q)
                    {
                        return Some(w);
                    }
                    self.marke[n as usize] = r;
                    hoehle.push(n);
                } else {
                    rand.push((a, b, n));
                }
            }
        }
        for &t in &hoehle {
            self.tot[t as usize] = true;
            self.frei.push(t);
        }
        // Fächer um q: je Randkante ein Dreieck (a, b, q)
        let mut neu: Vec<(u32, u32, u32)> = Vec::with_capacity(rand.len());
        for &(a, b, n) in &rand {
            let t = self.neu(Self::gedreht([a, b, k]));
            // außen
            let vn = self.tri[n as usize];
            let j = (0..3)
                .find(|&j| vn[(j + 1) % 3] == b && vn[(j + 2) % 3] == a)
                .expect("Gegenkante");
            self.nb[n as usize][j] = t;
            self.setze(t, a, b, n);
            neu.push((a, b, t));
        }
        for &(a, b, t) in &neu {
            // an b → q liegt das Dreieck, dessen Randkante bei b beginnt;
            // an q → a das, dessen Randkante bei a endet
            let nach = neu.iter().find(|x| x.0 == b).unwrap().2;
            let vor = neu.iter().find(|x| x.1 == a).unwrap().2;
            self.setze(t, b, k, nach);
            self.setze(t, k, a, vor);
        }
        None
    }

    /// Dreiecke um Ecke a, beginnend bei `ecke[a]`.
    fn um(&self, a: u32) -> Vec<u32> {
        let s = self.ecke[a as usize];
        let mut out = vec![s];
        let mut t = s;
        loop {
            let v = self.tri[t as usize];
            let i = v.iter().position(|&x| x == a).unwrap();
            // weiter über die Kante (a, v[i+1]): gegenüber v[i+2]
            t = self.nb[t as usize][(i + 2) % 3];
            if t == s || out.len() > self.tri.len() {
                return out;
            }
            out.push(t);
        }
    }

    /// Dreieck mit der gerichteten Kante a → b.
    fn mit_kante(&self, a: u32, b: u32) -> Option<u32> {
        self.um(a).into_iter().find(|&t| {
            let v = self.tri[t as usize];
            let i = v.iter().position(|&x| x == a).unwrap();
            v[(i + 1) % 3] == b
        })
    }

    /// Kante u–v tauschen, wenn beide Dreiecke echt sind und das Viereck
    /// streng konvex ist; gibt die neue Kante zurück.
    fn tauschen(&mut self, u: u32, v: u32) -> Option<(u32, u32)> {
        let t1 = self.mit_kante(u, v)?;
        let t2 = self.mit_kante(v, u)?;
        if self.geist(t1) || self.geist(t2) {
            return None;
        }
        let a = self.tri[t1 as usize];
        let b = self.tri[t2 as usize];
        let i1 = a.iter().position(|&x| x == u).unwrap();
        let i2 = b.iter().position(|&x| x == v).unwrap();
        // t1 = (u, v, w1), t2 = (v, u, w2)
        let w1 = a[(i1 + 2) % 3];
        let w2 = b[(i2 + 2) % 3];
        let pt = |x: u32| self.p[x as usize];
        if !(orient(pt(w1), pt(w2), pt(u)) < 0 && orient(pt(w1), pt(w2), pt(v)) > 0) {
            return None;
        }
        // Außennachbarn merken
        let n_vw1 = self.nb[t1 as usize][i1]; // gegenüber u: Kante v → w1
        let n_w1u = self.nb[t1 as usize][(i1 + 1) % 3]; // gegenüber v: w1 → u
        let n_uw2 = self.nb[t2 as usize][i2]; // gegenüber v: u → w2
        let n_w2v = self.nb[t2 as usize][(i2 + 1) % 3]; // gegenüber u: w2 → v
                                                        // t1 = (w1, u, w2), t2 = (w2, v, w1)
        self.tri[t1 as usize] = [w1, u, w2];
        self.tri[t2 as usize] = [w2, v, w1];
        self.nb[t1 as usize] = [n_uw2, t2, n_w1u];
        self.nb[t2 as usize] = [n_vw1, t1, n_w2v];
        let zeig = |d: &mut Dt, n: u32, alt: u32, neu: u32| {
            if let Some(j) = d.nb[n as usize].iter().position(|&x| x == alt) {
                d.nb[n as usize][j] = neu;
            }
        };
        zeig(self, n_uw2, t2, t1);
        zeig(self, n_vw1, t1, t2);
        for (x, t) in [(u, t1), (w1, t1), (w2, t2), (v, t2)] {
            self.ecke[x as usize] = t;
        }
        Some((w1, w2))
    }

    /// Kante u–v ist Delaunay oder fest oder liegt am Rand.
    fn delaunay_ok(&self, u: u32, v: u32) -> bool {
        if self.fest.contains(&kante(u, v)) {
            return true;
        }
        let (Some(t1), Some(t2)) = (self.mit_kante(u, v), self.mit_kante(v, u)) else {
            return true;
        };
        if self.geist(t1) || self.geist(t2) {
            return true;
        }
        let a = self.tri[t1 as usize];
        let b = self.tri[t2 as usize];
        let w2 = b[(b.iter().position(|&x| x == v).unwrap() + 2) % 3];
        let pt = |x: u32| self.p[x as usize];
        umkreis(pt(a[0]), pt(a[1]), pt(a[2]), pt(w2)) <= 0
    }

    /// Lawson: Kanten der Schlange tauschen, solange sie nicht Delaunay sind.
    fn lawson(&mut self, mut schlange: VecDeque<(u32, u32)>) {
        let mut runden = 0usize;
        while let Some((u, v)) = schlange.pop_front() {
            runden += 1;
            if runden > 50 * self.tri.len() + 1000 {
                break;
            }
            if self.delaunay_ok(u, v) {
                continue;
            }
            if let Some((w1, w2)) = self.tauschen(u, v) {
                schlange.extend([(u, w1), (w1, v), (v, w2), (w2, u)]);
            }
        }
    }

    /// Bruchkante a–b erzwingen. Liegt eine Ecke genau darauf, wird sie
    /// dort geteilt. `Err` mit Satz, wenn sie eine andere Bruchkante kreuzt.
    fn erzwingen(&mut self, a: u32, b: u32, aus: &mut Vec<(u32, u32)>) -> Result<(), String> {
        if a == b {
            return Ok(());
        }
        if self.mit_kante(a, b).is_some() {
            self.fest.insert(kante(a, b));
            aus.push((a, b));
            return Ok(());
        }
        let pt = |d: &Dt, x: u32| d.p[x as usize];
        let (pa, pb) = (pt(self, a), pt(self, b));
        // Ecke auf der Strecke: teilen
        let auf = self
            .um(a)
            .into_iter()
            .flat_map(|t| self.tri[t as usize])
            .find(|&w| w != INF && w != a && w != b && auf_strecke(pa, pb, pt(self, w)));
        if let Some(w) = auf {
            self.erzwingen(a, w, aus)?;
            return self.erzwingen(w, b, aus);
        }
        // Kreuzende Kanten: von a aus durch die Dreiecke bis b
        let mut kreuz: VecDeque<(u32, u32)> = VecDeque::new();
        let start = self.um(a).into_iter().find_map(|t| {
            let v = self.tri[t as usize];
            let i = v.iter().position(|&x| x == a).unwrap();
            let (c, d) = (v[(i + 1) % 3], v[(i + 2) % 3]);
            (c != INF && d != INF && kreuzen(pa, pb, pt(self, c), pt(self, d))).then_some((t, c, d))
        });
        let Some((mut t, mut c, mut d)) = start else {
            return Err("Bruchkante liegt außerhalb des Netzes".into());
        };
        loop {
            if self.fest.contains(&kante(c, d)) {
                return Err("Bruchkante kreuzt eine andere Bruchkante".into());
            }
            kreuz.push_back((c, d));
            // über c → d ins Nachbardreieck
            let v = self.tri[t as usize];
            let i = (0..3)
                .find(|&i| v[(i + 1) % 3] == c && v[(i + 2) % 3] == d)
                .unwrap();
            t = self.nb[t as usize][i];
            let w = self.tri[t as usize]
                .iter()
                .copied()
                .find(|&x| x != c && x != d)
                .unwrap();
            if w == b {
                break;
            }
            if w == INF {
                return Err("Bruchkante liegt außerhalb des Netzes".into());
            }
            let o = orient(pa, pb, pt(self, w));
            if o == 0 {
                // Ecke genau auf der Strecke
                self.erzwingen(a, w, aus)?;
                return self.erzwingen(w, b, aus);
            }
            // weiter über die Kante, die die Strecke noch kreuzt; im neuen
            // Dreieck (d, c, w) laufen sie c → w und w → d
            if kreuzen(pa, pb, pt(self, c), pt(self, w)) {
                d = w;
            } else {
                c = w;
            }
        }
        // Tauschen, bis keine Kante mehr kreuzt (Sloan)
        let mut neu: Vec<(u32, u32)> = Vec::new();
        let mut runden = 0usize;
        while let Some((u, v)) = kreuz.pop_front() {
            runden += 1;
            if runden > 100_000 {
                return Err("Bruchkante ließ sich nicht einfügen".into());
            }
            match self.tauschen(u, v) {
                Some((w1, w2)) => {
                    if (w1 != a && w1 != b && w2 != a && w2 != b)
                        && kreuzen(pa, pb, pt(self, w1), pt(self, w2))
                    {
                        kreuz.push_back((w1, w2));
                    } else {
                        neu.push((w1, w2));
                    }
                }
                None => kreuz.push_back((u, v)),
            }
        }
        self.fest.insert(kante(a, b));
        aus.push((a, b));
        // Delaunay wiederherstellen, die Bruchkante bleibt
        self.lawson(
            neu.into_iter()
                .filter(|e| kante(e.0, e.1) != kante(a, b))
                .collect(),
        );
        Ok(())
    }
}

/// Hilbert-Index von (x, y) im Gitter 2¹⁶ × 2¹⁶.
fn hilbert(mut x: u32, mut y: u32) -> u64 {
    let mut d: u64 = 0;
    let mut s: u32 = 1 << 15;
    while s > 0 {
        let rx = u32::from((x & s) > 0);
        let ry = u32::from((y & s) > 0);
        d += s as u64 * s as u64 * ((3 * rx) ^ ry) as u64;
        if ry == 0 {
            if rx == 1 {
                x = 0xFFFF - x;
                y = 0xFFFF - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        s >>= 1;
    }
    d
}

/// Netz durch `punkte` (x, y, z in m, relativ zu einem Bezugspunkt in der
/// Nähe) mit den Bruchkanten `bruch` (Nummern in `punkte`).
pub fn netz(punkte: &[[f64; 3]], bruch: &[[usize; 2]], rand: Rand) -> Netz {
    let mut out = Netz {
        punkte: punkte.to_vec(),
        ..Netz::default()
    };
    // Raster und doppelte Lage
    let mut ip: Vec<Ip> = Vec::with_capacity(punkte.len());
    let mut gilt: Vec<Option<u32>> = vec![None; punkte.len()];
    let mut ecken: Vec<u32> = Vec::new();
    for (i, p) in punkte.iter().enumerate() {
        match raster(p[0], p[1]).filter(|_| p[2].is_finite()) {
            Some(r) => {
                gilt[i] = Some(ip.len() as u32);
                ip.push(r);
                ecken.push(i as u32);
            }
            None => out.aussen.push(i as u32),
        }
    }
    if ip.len() < 3 {
        out.hinweise
            .push("Zu wenige Punkte für ein Gelände (mindestens drei).".into());
        return out;
    }
    // Reihenfolge: Hilbert-Kurve über das umgebende Rechteck
    let (mut lo, mut hi) = ([i64::MAX; 2], [i64::MIN; 2]);
    for p in &ip {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1) as f64;
    let mut ord: Vec<u32> = (0..ip.len() as u32).collect();
    let key = |i: u32| {
        let p = ip[i as usize];
        let g = |k: usize| (((p[k] - lo[k]) as f64 / span) * 65535.0).round() as u32;
        (hilbert(g(0), g(1)), p[0], p[1], i)
    };
    ord.sort_by_key(|&i| key(i));
    // Anfangsdreieck: die ersten zwei verschiedenen Punkte und der erste,
    // der nicht auf ihrer Linie liegt
    let a = ord[0];
    let Some(b) = ord
        .iter()
        .copied()
        .find(|&j| ip[j as usize] != ip[a as usize])
    else {
        out.hinweise
            .push("Alle Punkte liegen an derselben Stelle.".into());
        return out;
    };
    let Some(c) = ord
        .iter()
        .copied()
        .find(|&j| orient(ip[a as usize], ip[b as usize], ip[j as usize]) != 0)
    else {
        out.hinweise
            .push("Alle Punkte liegen auf einer Linie.".into());
        return out;
    };
    let mut dt = Dt::start(ip, a, b, c);
    // `auf[j]`: die Ecke, für die Punkt j steht (sich selbst oder einen
    // früheren an derselben Stelle)
    let mut auf: Vec<u32> = (0..dt.p.len() as u32).collect();
    for &k in &ord {
        if k == a || k == b || k == c {
            continue;
        }
        if let Some(j) = dt.einfuegen(k) {
            auf[k as usize] = j;
        }
    }
    let nr = |j: u32| ecken[j as usize];
    let mut doppelt: Vec<u32> = (0..auf.len() as u32)
        .filter(|&j| auf[j as usize] != j)
        .map(nr)
        .collect();
    doppelt.sort_unstable();
    if !doppelt.is_empty() {
        out.hinweise.push(format!(
            "{} Punkte an derselben Stelle wie ein anderer, nicht im Netz.",
            doppelt.len()
        ));
    }
    out.aussen.extend(doppelt);
    out.aussen.sort_unstable();
    // Bruchkanten
    let mut gesetzt = Vec::new();
    let mut fehler = 0usize;
    for &[i, j] in bruch {
        let (Some(Some(x)), Some(Some(y))) = (gilt.get(i), gilt.get(j)) else {
            fehler += 1;
            continue;
        };
        let (x, y) = (auf[*x as usize], auf[*y as usize]);
        if dt.erzwingen(x, y, &mut gesetzt).is_err() {
            fehler += 1;
        }
    }
    if fehler > 0 {
        out.hinweise.push(format!(
            "{fehler} Bruchkanten ließen sich nicht einfügen (kreuzen eine andere oder liegen außerhalb)."
        ));
    }
    let mut br: Vec<[u32; 2]> = gesetzt
        .iter()
        .map(|&(x, y)| {
            let (x, y) = (nr(x), nr(y));
            [x.min(y), x.max(y)]
        })
        .collect();
    br.sort_unstable();
    br.dedup();
    out.bruch = br;
    // Echte Dreiecke
    let echt: Vec<u32> = (0..dt.tri.len() as u32)
        .filter(|&t| !dt.tot[t as usize] && !dt.geist(t))
        .collect();
    let mut weg = vec![false; dt.tri.len()];
    if rand == Rand::Beschnitten {
        beschneiden(&dt, &echt, &mut weg);
    }
    out.dreiecke = echt
        .iter()
        .filter(|&&t| !weg[t as usize])
        .map(|&t| dt.tri[t as usize].map(nr))
        .collect();
    out.dreiecke.sort_unstable();
    out
}

/// Lange Randkanten abtragen (E9): Ein Randdreieck fällt weg, wenn seine
/// Randkante länger als das Vierfache der mittleren Kantenlänge ist und
/// danach jede seiner Ecken noch an einem Dreieck hängt.
fn beschneiden(dt: &Dt, echt: &[u32], weg: &mut [bool]) {
    let len = |a: u32, b: u32| {
        let (p, q) = (dt.p[a as usize], dt.p[b as usize]);
        ((p[0] - q[0]) as f64).hypot((p[1] - q[1]) as f64)
    };
    let mut summe = 0.0;
    let mut n = 0usize;
    for &t in echt {
        let v = dt.tri[t as usize];
        for i in 0..3 {
            summe += len(v[i], v[(i + 1) % 3]);
            n += 1;
        }
    }
    let grenze = 4.0 * summe / n.max(1) as f64;
    let mut an: Vec<u32> = vec![0; dt.p.len()];
    for &t in echt {
        for v in dt.tri[t as usize] {
            an[v as usize] += 1;
        }
    }
    let raus = |t: u32, weg: &[bool]| dt.geist(t) || weg[t as usize];
    let mut schlange: VecDeque<u32> = echt.iter().copied().collect();
    while let Some(t) = schlange.pop_front() {
        if weg[t as usize] {
            continue;
        }
        let v = dt.tri[t as usize];
        let lang = (0..3).any(|i| {
            let n = dt.nb[t as usize][i];
            raus(n, weg) && len(v[(i + 1) % 3], v[(i + 2) % 3]) > grenze
        });
        if lang && v.iter().all(|&x| an[x as usize] > 1) {
            weg[t as usize] = true;
            for x in v {
                an[x as usize] -= 1;
            }
            for n in dt.nb[t as usize] {
                if !raus(n, weg) {
                    schlange.push_back(n);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gitter(n: usize) -> Vec<[f64; 3]> {
        (0..n * n)
            .map(|i| [(i % n) as f64, (i / n) as f64, 0.0])
            .collect()
    }

    /// Jedes Dreieck hat einen leeren Umkreis (außer über Bruchkanten).
    fn ist_delaunay(n: &Netz) -> bool {
        let ip: Vec<Ip> = n
            .punkte
            .iter()
            .map(|p| raster(p[0], p[1]).unwrap())
            .collect();
        n.dreiecke.iter().all(|t| {
            let [a, b, c] = t.map(|v| ip[v as usize]);
            orient(a, b, c) > 0
                && ip.iter().enumerate().all(|(j, &d)| {
                    t.contains(&(j as u32))
                        || n.aussen.contains(&(j as u32))
                        || umkreis(a, b, c, d) <= 0
                })
        })
    }

    #[test]
    fn gitter_mit_gleichstand() {
        // Gitter: lauter Punkte auf gemeinsamen Kreisen
        let n = netz(&gitter(7), &[], Rand::Huelle);
        assert_eq!(n.dreiecke.len(), 2 * 36);
        assert!(ist_delaunay(&n));
        // dieselbe Eingabe, dasselbe Netz
        assert_eq!(netz(&gitter(7), &[], Rand::Huelle), n);
    }

    #[test]
    fn doppelte_und_kollineare() {
        let mut p = gitter(3);
        p.push([1.0, 1.0, 5.0]);
        p.push([0.5, 0.0, 0.0]);
        let n = netz(&p, &[], Rand::Huelle);
        assert_eq!(n.aussen, [9]);
        // Punkt auf der Randkante teilt sie: 8 + 1 Dreiecke
        assert_eq!(n.dreiecke.len(), 9);
        assert!(ist_delaunay(&n));
        let linie: Vec<[f64; 3]> = (0..5).map(|i| [i as f64, 0.0, 0.0]).collect();
        assert!(netz(&linie, &[], Rand::Huelle).dreiecke.is_empty());
    }

    #[test]
    fn zufallswolke() {
        // feste Folge, viele Punkte, Delaunay und Eulersche Formel
        let mut s: u64 = 12345;
        let mut r = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((s >> 33) as f64) / (1u64 << 31) as f64
        };
        let p: Vec<[f64; 3]> = (0..2000).map(|_| [r() * 50.0, r() * 30.0, r()]).collect();
        let n = netz(&p, &[], Rand::Huelle);
        assert!(ist_delaunay(&n));
        // Kanten aus den Dreiecken: Euler V − E + F = 1 (ohne Außenfläche)
        let mut k = BTreeSet::new();
        for t in &n.dreiecke {
            for i in 0..3 {
                k.insert(kante(t[i], t[(i + 1) % 3]));
            }
        }
        let v = p.len() - n.aussen.len();
        assert_eq!(v as i64 - k.len() as i64 + n.dreiecke.len() as i64, 1);
    }

    #[test]
    fn bruchkante_wird_erzwungen() {
        // Raute: Delaunay wählt die kurze Diagonale 1–3, erzwungen wird 0–2
        let p = [
            [0.0, 0.0, 0.0],
            [5.0, -1.0, 0.0],
            [10.0, 0.0, 0.0],
            [5.0, 1.0, 1.0],
        ];
        let frei = netz(&p, &[], Rand::Huelle);
        let hat = |n: &Netz, a: u32, b: u32| {
            n.dreiecke
                .iter()
                .any(|t| (0..3).any(|i| kante(t[i], t[(i + 1) % 3]) == kante(a, b)))
        };
        assert!(hat(&frei, 1, 3) && !hat(&frei, 0, 2));
        let fest = netz(&p, &[[0, 2]], Rand::Huelle);
        assert!(hat(&fest, 0, 2) && !hat(&fest, 1, 3));
        assert_eq!(fest.bruch, [[0, 2]]);
        // Bruchkante schräg über viele Dreiecke; die Zahl bleibt
        let mut g = gitter(9);
        g.push([0.5, 4.0, 0.0]);
        let quer = netz(&g, &[[81, 53]], Rand::Huelle);
        assert!(hat(&quer, 81, 53));
        assert_eq!(quer.dreiecke.len(), 2 * 82 - 32 - 2);
        // durch Punkte hindurch: in Stücken von Punkt zu Punkt
        let gerade = netz(&g, &[[81, 44]], Rand::Huelle);
        assert!(hat(&gerade, 81, 37) && hat(&gerade, 43, 44));
        assert_eq!(gerade.bruch.len(), 8);
        // Delaunay außerhalb der Bruchkanten bleibt
        let n = netz(&gitter(9), &[[0, 80]], Rand::Huelle);
        assert!(hat(&n, 0, 10) && hat(&n, 70, 80));
    }

    #[test]
    fn rand_beschnitten() {
        // Gitter mit einem fernen Punkt: lange Splitter am Rand fallen weg,
        // der Punkt bleibt an einem Dreieck
        let mut p = gitter(5);
        p.push([40.0, 2.0, 0.0]);
        let h = netz(&p, &[], Rand::Huelle);
        let b = netz(&p, &[], Rand::Beschnitten);
        assert!(b.dreiecke.len() < h.dreiecke.len());
        assert!(b.dreiecke.iter().any(|t| t.contains(&25)));
    }
}

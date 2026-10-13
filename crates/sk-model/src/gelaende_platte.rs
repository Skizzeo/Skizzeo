//! Geländeplatte (Gelände G0, Jörn 12.10.): ohne Geländemodul eine ebene
//! Platte um jedes Gebäude in Höhe OK Gelände, damit man sieht, wie das Haus
//! zum Gelände liegt. Sie ersetzt die Geländelinie in Ansicht und Schnitt.
//!
//! Form je Gebäude: das Rechteck um seine Wandzüge, um den Abstand
//! vergrößert (wie [`Model::terrain_patches`]), mit einem Loch, wo Gebäude
//! stehen (Umriss ihrer Gründung). Gebäude mit gleicher Geländehöhe teilen
//! sich eine Platte; bei verschiedenen Höhen gilt, wo sich Rechtecke
//! überdecken, das erste Gebäude (nach Guid). Nur Darstellung: Die Platte ist
//! kein Bauteil, nicht wählbar außer im Objektbaum und zählt in keiner Menge.
//!
//! Die Draufsicht wird in waagerechte Streifen zerlegt (an jeder Ecke und
//! jedem Kantenschnitt eine Grenze); in jedem Streifen sind Rechtecke, Löcher
//! und Vorrang einfache Intervalle. Das ergibt lückenlose Trapeze und die
//! Randkanten, auch bei schrägen Häusern und mehreren Gebäuden.

use super::*;
use sk_math::{vec3, Vec3};
use std::collections::{BTreeSet, HashMap};

/// Dicke und Abstand der Geländeplatte, für alle Gebäude des Projekts (mm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlateSettings {
    /// Dicke (OK bis UK).
    pub thick: f64,
    /// Abstand ums Gebäude (Rand der Wandzüge bis Plattenrand).
    pub margin: f64,
}

impl Default for PlateSettings {
    fn default() -> PlateSettings {
        PlateSettings {
            thick: PLATE_THICK.0,
            margin: PLATE_MARGIN.0,
        }
    }
}

/// Dicke der Platte: Vorgabe, kleinster und größter Wert (mm).
pub const PLATE_THICK: (f64, f64, f64) = (500.0, 100.0, 2000.0);
/// Abstand ums Gebäude: Vorgabe, kleinster und größter Wert (mm, Jörn:
/// 1,50 m bis 3,00 m).
pub const PLATE_MARGIN: (f64, f64, f64) = (3000.0, 1500.0, 3000.0);

impl PlateSettings {
    /// Liegen beide Werte im erlaubten Bereich?
    pub fn valid(&self) -> bool {
        let ok = |v: f64, r: (f64, f64, f64)| v.is_finite() && v >= r.1 - 1e-9 && v <= r.2 + 1e-9;
        ok(self.thick, PLATE_THICK) && ok(self.margin, PLATE_MARGIN)
    }
}

/// Eine Geländeplatte: Gebäude gleicher Geländehöhe.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPlate {
    /// Die Gebäude dieser Platte, nach Guid.
    pub buildings: Vec<BuildingId>,
    /// OK Platte = OK Gelände (mm, relativ zu ±0,00).
    pub top: f64,
    /// UK Platte (mm).
    pub bottom: f64,
    /// Draufsicht lückenlos als Trapeze, je vier Ecken gegen den
    /// Uhrzeigersinn (z = 0); zwei Ecken können zusammenfallen.
    pub pieces: Vec<[Vec3; 4]>,
    /// Randkanten (z = 0), so gerichtet, dass die Platte links liegt: außen
    /// gegen, an Löchern im Uhrzeigersinn. Gerade Ränder können in mehrere
    /// Stücke geteilt sein.
    pub edges: Vec<(Vec3, Vec3)>,
}

impl TerrainPlate {
    /// Fläche der Draufsicht (mm²).
    pub fn area(&self) -> f64 {
        self.pieces
            .iter()
            .map(|q| sk_math::polygon::signed_area(q))
            .sum()
    }
}

impl Model {
    /// Dicke und Abstand der Geländeplatte.
    pub fn plate_settings(&self) -> PlateSettings {
        self.project.terrain_plate
    }

    /// Setzt Dicke und Abstand der Geländeplatte (im offenen Schritt).
    /// Werte außerhalb von [`PLATE_THICK`] und [`PLATE_MARGIN`] und gleiche
    /// Werte ändern nichts.
    pub fn set_plate_settings(&mut self, s: PlateSettings) -> bool {
        if s == self.project.terrain_plate || !s.valid() {
            return false;
        }
        self.note_project();
        self.project.terrain_plate = s;
        self.project.terrain_plate_raw = None;
        self.touch();
        true
    }

    /// Die Geländeplatten des Projekts, nach dem ersten Gebäude geordnet;
    /// leer ohne Wandzüge. Gründungen über das Verzeichnis ([`Model::lesend`]),
    /// nicht je Zug über alle Bauteile (Review W34 B2).
    pub fn terrain_plates(&self) -> Vec<TerrainPlate> {
        self.terrain_plate_input().build()
    }

    /// Was die Platten aus dem Modell brauchen ([`PlateInput`]); das
    /// Zerlegen selbst braucht das Modell nicht mehr.
    pub fn terrain_plate_input(&self) -> PlateInput {
        self.lesend(|| self.platten_eingaben())
    }

    fn platten_eingaben(&self) -> PlateInput {
        let s = self.plate_settings();
        // Rechtecke je Gebäude, nach Guid
        let mut boxes: Vec<(Guid, BuildingId, [f64; 4])> = Vec::new();
        let mut stelle: HashMap<BuildingId, usize> = HashMap::new();
        for (_, r) in self.runs.iter() {
            let Some(b) = self.building_of(r.storey) else {
                continue;
            };
            let i = match stelle.get(&b) {
                Some(&i) => i,
                None => {
                    let Some(guid) = self.building(b).map(|x| x.guid) else {
                        continue;
                    };
                    let e = f64::INFINITY;
                    boxes.push((guid, b, [e, e, -e, -e]));
                    stelle.insert(b, boxes.len() - 1);
                    boxes.len() - 1
                }
            };
            let bx = &mut boxes[i].2;
            for p in &r.points {
                bx[0] = bx[0].min(p.x);
                bx[1] = bx[1].min(p.y);
                bx[2] = bx[2].max(p.x);
                bx[3] = bx[3].max(p.y);
            }
        }
        boxes.retain(|x| x.2[0] <= x.2[2] && x.2[1] <= x.2[3]);
        boxes.sort_by_key(|x| x.0);
        // Gruppen gleicher Geländehöhe (auf 1 nm) in der Reihenfolge ihres
        // ersten Gebäudes
        let mut groups: Vec<(f64, Vec<BuildingId>, Vec<[f64; 4]>)> = Vec::new();
        let mut gruppe: HashMap<i64, usize> = HashMap::new();
        for (_, b, x) in &boxes {
            // Höhenkette: Höhe über den Ort abfragen (Rechteckmitte), damit
            // ein Geländemodell die Platte mitnimmt
            let z = self.terrain_z_at(Some(*b), (x[0] + x[2]) / 2.0, (x[1] + x[3]) / 2.0);
            let m = s.margin;
            let r = [x[0] - m, x[1] - m, x[2] + m, x[3] + m];
            let g = *gruppe.entry((z * 1e6).round() as i64).or_insert_with(|| {
                groups.push((z, Vec::new(), Vec::new()));
                groups.len() - 1
            });
            groups[g].1.push(*b);
            groups[g].2.push(r);
        }
        // Löcher: Außenfläche der Wände über jeder Gründung (mit allen
        // Schichten, auch bei Sockelrücksprung), sonst der Plattenumriss
        let mut holes: Vec<Vec<Vec3>> = Vec::new();
        for (id, r) in self.runs.iter() {
            if self.building_of(r.storey).is_none() || self.foundation_of(id).is_none() {
                continue;
            }
            let face = self
                .base_chain(id)
                .map(|c| c.face_corners(c.outer_offset()))
                .filter(|f| f.len() >= 3 && sk_math::polygon::area(f) > 1.0);
            let outline = face.or_else(|| match self.foundation(id) {
                Some(Ok(f)) if f.outline.len() >= 3 => Some(f.outline),
                _ => None,
            });
            if let Some(o) = outline {
                holes.push(sk_math::polygon::to_ccw(&o));
            }
        }
        PlateInput {
            thick: s.thick,
            groups,
            holes,
        }
    }
}

/// Was die Geländeplatten aus dem Modell brauchen: die Gruppen gleicher
/// Geländehöhe mit ihren Gebäuden und Rechtecken, die Löcher und die Dicke.
/// Gelesen wird schnell im Modell, das Zerlegen ([`PlateInput::build`])
/// braucht kein Modell und kann in einem eigenen Faden laufen (Review #119
/// W34 B6).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlateInput {
    thick: f64,
    /// Höhe, Gebäude und Rechtecke je Gruppe, in der Reihenfolge ihres
    /// ersten Gebäudes (Vorrang).
    groups: Vec<(f64, Vec<BuildingId>, Vec<[f64; 4]>)>,
    holes: Vec<Vec<Vec3>>,
}

impl PlateInput {
    /// Die Platten: Gruppen abzüglich der Löcher in Trapeze und Randkanten
    /// zerlegt.
    pub fn build(&self) -> Vec<TerrainPlate> {
        let rects: Vec<Vec<[f64; 4]>> = self.groups.iter().map(|g| g.2.clone()).collect();
        let (pieces, edges) = streifen(&rects, &self.holes);
        self.groups
            .iter()
            .zip(pieces.into_iter().zip(edges))
            .filter(|(_, (p, _))| !p.is_empty())
            .map(|((z, bs, _), (pieces, edges))| TerrainPlate {
                buildings: bs.clone(),
                top: *z,
                bottom: z - self.thick,
                pieces,
                edges,
            })
            .collect()
    }
}

/// Abstand, unter dem zwei Werte als gleich gelten (mm).
const EPS: f64 = 1e-6;

/// Ab so vielen Kanten je Haufen wird in senkrechte Scheiben geteilt, je
/// Scheibe etwa so viele.
const SCHEIBE: usize = 64;

/// Höchstens so viele Scheiben je Haufen.
const MAX_SCHEIBEN: usize = 4096;

/// Ein Rechteck über mehr Rasterfelder steht in keinem Feld, sondern wird
/// mit jedem verglichen (wenige, sehr große Gebäude).
const MAX_FELDER: i64 = 64;

/// Gerade einer Intervallgrenze: die Kante, die sie bildet.
#[derive(Clone, Copy, Debug)]
struct Grenze {
    a: Vec3,
    b: Vec3,
}

impl Grenze {
    fn x_bei(&self, y: f64) -> f64 {
        let dy = self.b.y - self.a.y;
        if dy.abs() < EPS {
            return self.a.x.min(self.b.x);
        }
        self.a.x + (y - self.a.y) * (self.b.x - self.a.x) / dy
    }
}

/// Was eine Grenze im Streifen öffnet oder schließt.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Art {
    /// Rechteck der Gruppe; `true` an der linken Kante (öffnet).
    Rechteck(usize, bool),
    /// Loch (Gebäude, gegen den Uhrzeigersinn): `+1` an einer Kante nach
    /// unten (links, hinein), `-1` nach oben; überlappende Löcher zählen
    /// zusammen, nicht gegeneinander.
    Loch(i32),
}

/// Kante der Zerlegung; `schnitt`: nur der Schnitt an einer Scheibengrenze,
/// keine Randkante der Platte.
#[derive(Clone, Copy, Debug)]
struct Kante {
    g: Grenze,
    art: Art,
    schnitt: bool,
}

/// Ein Intervall eines Streifens: Gruppe, linke und rechte Grenze, je mit
/// `schnitt` wie an [`Kante`].
#[derive(Clone, Copy, Debug)]
struct Stueck {
    g: usize,
    l: Grenze,
    ls: bool,
    r: Grenze,
    rs: bool,
}

/// Trapeze und Randkanten je Gruppe.
type Zerlegt = (Vec<Vec<[Vec3; 4]>>, Vec<Vec<(Vec3, Vec3)>>);

/// Zerlegt die Gruppen (je eine Liste von Rechtecken) abzüglich der Löcher in
/// Trapeze und Randkanten; eine frühere Gruppe hat Vorrang vor einer späteren.
///
/// Damit die Arbeit mit der Zahl der Häuser wächst und nicht mit ihrem
/// Quadrat (Review W34 B2): Rechtecke, die sich berühren, bilden einen
/// Haufen, jeder Haufen rechnet für sich. Ein Haufen mit vielen Kanten wird
/// in senkrechte Scheiben geteilt (Grenzen zwischen den Ecken, je Scheibe
/// gleich viele), so läuft kein Streifen über eine ganze Reihe Häuser.
/// Scheibengrenzen sind keine Randkanten.
fn streifen(rects: &[Vec<[f64; 4]>], holes: &[Vec<Vec3>]) -> Zerlegt {
    let n = rects.len();
    let mut pieces = vec![Vec::new(); n];
    let mut edges = vec![Vec::new(); n];
    let alle: Vec<(usize, [f64; 4])> = rects
        .iter()
        .enumerate()
        .flat_map(|(g, rs)| rs.iter().map(move |r| (g, *r)))
        .filter(|(_, r)| r[2] - r[0] > EPS && r[3] - r[1] > EPS)
        .collect();
    for (ri, hi) in haufen(&alle, holes) {
        // Gruppen des Haufens in ihrer Reihenfolge (Vorrang)
        let mut gs: Vec<usize> = ri.iter().map(|&i| alle[i].0).collect();
        gs.sort_unstable();
        gs.dedup();
        let lokal: HashMap<usize, usize> = gs.iter().enumerate().map(|(l, &g)| (g, l)).collect();
        let rs: Vec<(usize, [f64; 4])> =
            ri.iter().map(|&i| (lokal[&alle[i].0], alle[i].1)).collect();
        let hs: Vec<&[Vec3]> = hi.iter().map(|&i| holes[i].as_slice()).collect();
        let (p, e) = zerlegen(&rs, &hs, gs.len());
        for (l, (p, e)) in p.into_iter().zip(e).enumerate() {
            pieces[gs[l]].extend(p);
            edges[gs[l]].extend(e);
        }
    }
    (pieces, edges)
}

/// Rahmen `[x0, y0, x1, y1]` zweier Rechtecke berühren oder überdecken sich.
fn beruehren(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] + EPS && b[0] <= a[2] + EPS && a[1] <= b[3] + EPS && b[1] <= a[3] + EPS
}

/// Rahmen eines Lochs.
fn rahmen_von(h: &[Vec3]) -> [f64; 4] {
    h.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |r, p| [r[0].min(p.x), r[1].min(p.y), r[2].max(p.x), r[3].max(p.y)],
    )
}

/// Haufen: Rechtecke (Stellen in `alle`), die sich über Berührung verbinden,
/// und die Löcher, die eines davon berühren. Gesucht über ein Raster in der
/// mittleren Rechteckgröße, nicht über alle Paare. Gleiche Rechtecke
/// (Dubletten, z. B. Häuser übereinander kopiert) vertritt das erste, so
/// wächst ein Rasterfeld nicht mit ihrer Zahl (Review #119 B5).
fn haufen(alle: &[(usize, [f64; 4])], holes: &[Vec<Vec3>]) -> Vec<(Vec<usize>, Vec<usize>)> {
    let n = alle.len();
    if n == 0 {
        return Vec::new();
    }
    let mut eltern: Vec<usize> = (0..n).collect();
    let mut erst: HashMap<[u64; 4], usize> = HashMap::new();
    for (i, (_, r)) in alle.iter().enumerate() {
        eltern[i] = *erst.entry(r.map(f64::to_bits)).or_insert(i);
    }
    // Vertreter: Stellen, die für sich im Raster stehen
    let vertreter: Vec<usize> = (0..n).filter(|&i| eltern[i] == i).collect();
    let mut seiten: Vec<f64> = alle
        .iter()
        .map(|(_, r)| (r[2] - r[0]).max(r[3] - r[1]))
        .collect();
    seiten.sort_by(f64::total_cmp);
    let feld = seiten[n / 2].max(1.0);
    let zellen = |r: &[f64; 4]| -> Option<(i64, i64, i64, i64)> {
        let z = |v: f64| (v / feld).floor() as i64;
        let c = (z(r[0]), z(r[1]), z(r[2]), z(r[3]));
        ((c.2 - c.0 + 1) * (c.3 - c.1 + 1) <= MAX_FELDER).then_some(c)
    };
    let mut raster: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut gross: Vec<usize> = Vec::new();
    for &i in &vertreter {
        match zellen(&alle[i].1) {
            Some((x0, y0, x1, y1)) => {
                for cx in x0..=x1 {
                    for cy in y0..=y1 {
                        raster.entry((cx, cy)).or_default().push(i);
                    }
                }
            }
            None => gross.push(i),
        }
    }
    fn wurzel(e: &mut [usize], mut i: usize) -> usize {
        while e[i] != i {
            e[i] = e[e[i]];
            i = e[i];
        }
        i
    }
    let verbinde = |e: &mut Vec<usize>, i: usize, j: usize| {
        let (a, b) = (wurzel(e, i), wurzel(e, j));
        if a != b && beruehren(&alle[i].1, &alle[j].1) {
            e[a.max(b)] = a.min(b);
        }
    };
    let mut felder: Vec<&Vec<usize>> = raster.values().collect();
    felder.sort_by_key(|v| v[0]);
    for v in felder {
        for (k, &i) in v.iter().enumerate() {
            for &j in &v[k + 1..] {
                verbinde(&mut eltern, i, j);
            }
        }
    }
    for &i in &gross {
        for &j in &vertreter {
            verbinde(&mut eltern, i, j);
        }
    }
    // Haufen in der Reihenfolge ihres ersten Rechtecks
    let mut nummer: HashMap<usize, usize> = HashMap::new();
    let mut out: Vec<(Vec<usize>, Vec<usize>)> = Vec::new();
    let haufen_von: Vec<usize> = (0..n)
        .map(|i| {
            let w = wurzel(&mut eltern, i);
            let h = *nummer.entry(w).or_insert_with(|| {
                out.push((Vec::new(), Vec::new()));
                out.len() - 1
            });
            out[h].0.push(i);
            h
        })
        .collect();
    // Löcher zu jedem Haufen, dessen Rechteck sie berühren
    for (k, h) in holes.iter().enumerate() {
        let r = rahmen_von(h);
        let mut treffer: Vec<usize> = match zellen(&r) {
            Some((x0, y0, x1, y1)) => {
                let mut t: Vec<usize> = (x0..=x1)
                    .flat_map(|cx| (y0..=y1).map(move |cy| (cx, cy)))
                    .filter_map(|c| raster.get(&c))
                    .flatten()
                    .copied()
                    .chain(gross.iter().copied())
                    .collect();
                t.sort_unstable();
                t.dedup();
                t
            }
            None => vertreter.clone(),
        };
        treffer.retain(|&i| beruehren(&alle[i].1, &r));
        let mut hs: Vec<usize> = treffer.into_iter().map(|i| haufen_von[i]).collect();
        hs.sort_unstable();
        hs.dedup();
        for h in hs {
            out[h].1.push(k);
        }
    }
    out
}

/// Ein Haufen: Rechtecke mit Gruppe (lokal, `n` Gruppen) und Löcher; bei
/// vielen Kanten in senkrechten Scheiben. Je Scheibe kommen nur die Kanten,
/// die sie berühren, auf die Scheibe beschnitten; was links der Scheibe
/// liegt, ersetzt an ihrer linken Grenze je eine Kante mit dem Umlauf der
/// Löcher dort (Schnitt, keine Randkante).
fn zerlegen(rs: &[(usize, [f64; 4])], hs: &[&[Vec3]], n: usize) -> Zerlegt {
    let mut pieces = vec![Vec::new(); n];
    let mut edges = vec![Vec::new(); n];
    // Lochkanten mit ihrem Umlauf (waagerechte zählen nur beim Kreuzen
    // einer Scheibengrenze)
    let mut loecher: Vec<(Vec3, Vec3, i32)> = Vec::new();
    for h in hs {
        for i in 0..h.len() {
            let (a, b) = (h[i], h[(i + 1) % h.len()]);
            loecher.push((a, b, if b.y < a.y { 1 } else { -1 }));
        }
    }
    let zahl = 2 * rs.len() + loecher.len();
    let mut xs: Vec<f64> = rs
        .iter()
        .flat_map(|(_, r)| [r[0], r[2]])
        .chain(hs.iter().flat_map(|h| h.iter().map(|p| p.x)))
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| *a - *b < 1e-3);
    // Scheibengrenzen mitten zwischen zwei Ecken (nie auf einer), je
    // Scheibe gleich viele Ecken
    let scheiben = (zahl / SCHEIBE).clamp(1, MAX_SCHEIBEN).min(xs.len().max(1));
    let mut grenzen = vec![f64::NEG_INFINITY];
    for k in 1..scheiben {
        let i = k * (xs.len() - 1) / scheiben;
        let g = 0.5 * (xs[i] + xs[i + 1]);
        if g > grenzen[grenzen.len() - 1] {
            grenzen.push(g);
        }
    }
    grenzen.push(f64::INFINITY);
    // von links nach rechts: aufnehmen, sobald der linke Rand in die
    // Scheibe reicht, entlassen, wenn der rechte links von ihr liegt
    let mut r_neu: Vec<usize> = (0..rs.len()).collect();
    r_neu.sort_by(|&i, &j| rs[i].1[0].total_cmp(&rs[j].1[0]));
    let links = |k: usize| loecher[k].0.x.min(loecher[k].1.x);
    let rechts = |k: usize| loecher[k].0.x.max(loecher[k].1.x);
    let mut l_neu: Vec<usize> = (0..loecher.len()).collect();
    l_neu.sort_by(|&i, &j| links(i).total_cmp(&links(j)));
    let (mut r_neu, mut l_neu) = (r_neu.into_iter().peekable(), l_neu.into_iter().peekable());
    let (mut r_aktiv, mut l_aktiv): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    let mut kanten: Vec<Kante> = Vec::new();
    let mut kreuz: Vec<(f64, i32)> = Vec::new();
    // je Haufen einmal, nicht je Scheibe (viele Gruppen, viele Scheiben)
    let mut zaehler = vec![0i32; n];
    let mut positiv: BTreeSet<usize> = BTreeSet::new();
    for w in grenzen.windows(2) {
        let (xa, xb) = (w[0], w[1]);
        while let Some(k) = r_neu.next_if(|&k| rs[k].1[0] < xb) {
            r_aktiv.push(k);
        }
        r_aktiv.retain(|&k| rs[k].1[2] > xa);
        r_aktiv.sort_unstable();
        while let Some(k) = l_neu.next_if(|&k| links(k) < xb) {
            l_aktiv.push(k);
        }
        l_aktiv.retain(|&k| rechts(k) > xa);
        l_aktiv.sort_unstable();
        kanten.clear();
        for &k in &r_aktiv {
            let (g, r) = rs[k];
            let (x0, x1) = (r[0].max(xa), r[2].min(xb));
            if x1 - x0 <= EPS {
                continue;
            }
            let senk = |x: f64| Grenze {
                a: vec3(x, r[1], 0.0),
                b: vec3(x, r[3], 0.0),
            };
            kanten.push(Kante {
                g: senk(x0),
                art: Art::Rechteck(g, true),
                schnitt: r[0] < xa,
            });
            kanten.push(Kante {
                g: senk(x1),
                art: Art::Rechteck(g, false),
                schnitt: r[2] > xb,
            });
        }
        kreuz.clear();
        for &k in &l_aktiv {
            let (a, b, w) = loecher[k];
            let bei = |x: f64| {
                let t = (x - a.x) / (b.x - a.x);
                vec3(x, a.y + t * (b.y - a.y), 0.0)
            };
            // über die linke Grenze: der Umlauf rechts von ihr springt
            if links(k) < xa && xa < rechts(k) {
                kreuz.push((bei(xa).y, if a.x < b.x { 1 } else { -1 }));
            }
            let auf = |p: Vec3| {
                if p.x < xa {
                    bei(xa)
                } else if p.x > xb {
                    bei(xb)
                } else {
                    p
                }
            };
            let (p, q) = (auf(a), auf(b));
            if (p.y - q.y).abs() > EPS {
                kanten.push(Kante {
                    g: Grenze { a: p, b: q },
                    art: Art::Loch(w),
                    schnitt: false,
                });
            }
        }
        // Umlauf der Löcher an der linken Grenze, von unten nach oben
        kreuz.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut umlauf = 0;
        for k in kreuz.windows(2) {
            umlauf += k[0].1;
            if umlauf != 0 && k[1].0 - k[0].0 > EPS {
                kanten.push(Kante {
                    g: Grenze {
                        a: vec3(xa, k[0].0, 0.0),
                        b: vec3(xa, k[1].0, 0.0),
                    },
                    art: Art::Loch(umlauf),
                    schnitt: true,
                });
            }
        }
        scheibe(&kanten, &mut pieces, &mut edges, &mut zaehler, &mut positiv);
    }
    (pieces, edges)
}

/// Eine Scheibe: waagerechte Streifen (an jeder Ecke und jedem
/// Kantenschnitt eine Grenze); in jedem Streifen sind Rechtecke, Löcher und
/// Vorrang einfache Intervalle. Stücke und Kanten kommen zu denen des
/// Haufens; `zaehler` und `positiv` sind vorher und nachher leer.
fn scheibe(
    kanten: &[Kante],
    pieces: &mut [Vec<[Vec3; 4]>],
    edges: &mut [Vec<(Vec3, Vec3)>],
    zaehler: &mut [i32],
    positiv: &mut BTreeSet<usize>,
) {
    let rahmen: Vec<[f64; 4]> = kanten
        .iter()
        .map(|k| {
            let (a, b) = (k.g.a, k.g.b);
            [a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y)]
        })
        .collect();
    // Streifengrenzen: Ecken und Schnittpunkte der Kanten
    let mut ys: Vec<f64> = kanten.iter().flat_map(|k| [k.g.a.y, k.g.b.y]).collect();
    // Schnitte: gleiche Kanten nur einmal, nach unterem Rand sortiert, je
    // Kante die folgenden bis zu ihrem oberen Rand (Paare nur bei sich
    // überdeckenden Rahmen)
    let mut einmal: HashMap<[u64; 4], usize> = HashMap::new();
    for (i, k) in kanten.iter().enumerate() {
        let (a, b) = (k.g.a, k.g.b);
        let (p, q) = if (a.x, a.y) <= (b.x, b.y) {
            (a, b)
        } else {
            (b, a)
        };
        einmal
            .entry([p.x, p.y, q.x, q.y].map(f64::to_bits))
            .or_insert(i);
    }
    let mut nach_y: Vec<usize> = einmal.into_values().collect();
    nach_y.sort_by(|&i, &j| rahmen[i][2].total_cmp(&rahmen[j][2]).then(i.cmp(&j)));
    for (o, &i) in nach_y.iter().enumerate() {
        for &j in &nach_y[o + 1..] {
            if rahmen[j][2] > rahmen[i][3] + EPS {
                break;
            }
            if rahmen[j][0] > rahmen[i][1] + EPS || rahmen[i][0] > rahmen[j][1] + EPS {
                continue;
            }
            if let Some(y) = schnitt_y(&kanten[i].g, &kanten[j].g) {
                ys.push(y);
            }
        }
    }
    ys.retain(|y| y.is_finite());
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < EPS);
    // aktive Kanten: von unten nach oben aufgenommen, oben entlassen
    let mut neu: Vec<usize> = (0..kanten.len()).collect();
    neu.sort_by(|&i, &j| rahmen[i][2].total_cmp(&rahmen[j][2]));
    let mut neu = neu.into_iter().peekable();
    let mut aktiv: Vec<usize> = Vec::new();
    let mut hits: Vec<(f64, usize)> = Vec::new();
    // `vorher`: die Intervalle des Streifens darunter (für die waagerechten
    // Randkanten), an seiner Oberkante festgehalten
    let mut vorher: Vec<Stueck> = Vec::new();
    for w in ys.windows(2) {
        let (y0, y1) = (w[0], w[1]);
        let ym = 0.5 * (y0 + y1);
        while let Some(&k) = neu.peek().filter(|&&k| rahmen[k][2] < ym) {
            aktiv.push(k);
            neu.next();
        }
        aktiv.retain(|&k| rahmen[k][3] > ym);
        // in der ursprünglichen Reihenfolge (gleiche Stellen wie zuvor)
        aktiv.sort_unstable();
        hits.clear();
        hits.extend(
            aktiv
                .iter()
                .filter(|&&k| rahmen[k][2] < ym && ym < rahmen[k][3])
                .map(|&k| (kanten[k].g.x_bei(ym), k)),
        );
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        let jetzt = intervalle(&hits, kanten, zaehler, positiv, ym);
        waagerecht(&vorher, &jetzt, y0, edges);
        for s in &jetzt {
            let (l0, l1, r0, r1) = (s.l.x_bei(y0), s.l.x_bei(y1), s.r.x_bei(y0), s.r.x_bei(y1));
            pieces[s.g].push([
                vec3(l0, y0, 0.0),
                vec3(r0, y0, 0.0),
                vec3(r1, y1, 0.0),
                vec3(l1, y1, 0.0),
            ]);
            if !s.ls {
                edges[s.g].push((vec3(l1, y1, 0.0), vec3(l0, y0, 0.0)));
            }
            if !s.rs {
                edges[s.g].push((vec3(r0, y0, 0.0), vec3(r1, y1, 0.0)));
            }
        }
        // die Oberkante dieses Streifens ist die Unterkante des nächsten
        vorher = jetzt
            .into_iter()
            .map(|s| Stueck {
                l: fest(s.l, y1),
                r: fest(s.r, y1),
                ..s
            })
            .collect();
    }
    if let Some(&y) = ys.last() {
        waagerecht(&vorher, &[], y, edges);
    }
}

/// Senkrechte Grenze bei `x(y)` (damit die waagerechte Rechnung am Ende
/// eines Streifens dieselbe Zahl sieht wie der Streifen selbst).
fn fest(g: Grenze, y: f64) -> Grenze {
    let x = g.x_bei(y);
    Grenze {
        a: vec3(x, y - 1.0, 0.0),
        b: vec3(x, y + 1.0, 0.0),
    }
}

/// Höhe des Schnittpunkts zweier Kanten im Inneren beider (sonst `None`).
fn schnitt_y(p: &Grenze, q: &Grenze) -> Option<f64> {
    let (d1, d2) = (p.b - p.a, q.b - q.a);
    let den = d1.x * d2.y - d1.y * d2.x;
    if den.abs() < 1e-12 {
        return None;
    }
    let w = q.a - p.a;
    let t = (w.x * d2.y - w.y * d2.x) / den;
    let u = (w.x * d1.y - w.y * d1.x) / den;
    let ok = |s: f64| s > 1e-9 && s < 1.0 - 1e-9;
    (ok(t) && ok(u)).then_some(p.a.y + t * d1.y)
}

/// Intervalle auf der Höhe `y` (Mitte eines Streifens) aus den Treffern
/// (`x`, Kante; nach `x` sortiert): in einem Rechteck einer Gruppe, in keinem
/// Loch; wo Rechtecke mehrerer Gruppen liegen, gilt die erste. `zaehler` und
/// `positiv` (Gruppen mit offenem Rechteck, geordnet, je Gruppe O(log n)
/// statt Verschieben eines Vec, Review #119 B5) sind danach wieder leer.
fn intervalle(
    hits: &[(f64, usize)],
    kanten: &[Kante],
    zaehler: &mut [i32],
    positiv: &mut BTreeSet<usize>,
    y: f64,
) -> Vec<Stueck> {
    let mut out = Vec::new();
    let mut loch = 0i32;
    let mut offen: Option<(usize, Grenze, bool)> = None;
    let mut beruehrt: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < hits.len() {
        // alle Treffer an derselben Stelle zusammen
        let x = hits[i].0;
        let mut j = i;
        while j < hits.len() && (hits[j].0 - x).abs() < EPS {
            match kanten[hits[j].1].art {
                // Rechtecke: linke Kante öffnet, rechte schließt
                Art::Rechteck(g, links) => zaehler[g] += if links { 1 } else { -1 },
                // Löcher: Umlaufzahl
                Art::Loch(w) => loch += w,
            }
            j += 1;
        }
        for &(_, k) in &hits[i..j] {
            if let Art::Rechteck(g, _) = kanten[k].art {
                beruehrt.push(g);
                if zaehler[g] > 0 {
                    positiv.insert(g);
                } else {
                    positiv.remove(&g);
                }
            }
        }
        // die Gruppe nach der Stelle: die erste mit offenem Rechteck
        let jetzt = if loch <= 0 {
            positiv.first().copied()
        } else {
            None
        };
        if jetzt != offen.map(|o| o.0) {
            let (b, bs) = grenze_an(&hits[i..j], kanten, x);
            if let Some((g, l, ls)) = offen {
                if b.x_bei(y) - l.x_bei(y) > EPS {
                    out.push(Stueck {
                        g,
                        l,
                        ls,
                        r: b,
                        rs: bs,
                    });
                }
            }
            offen = jetzt.map(|g| (g, b, bs));
        }
        i = j;
    }
    for g in beruehrt {
        zaehler[g] = 0;
    }
    positiv.clear();
    out
}

/// Die Grenze an einer Stelle: bei mehreren Treffern die erste echte Kante
/// (vor einem Schnitt an der Scheibengrenze).
fn grenze_an(hits: &[(f64, usize)], kanten: &[Kante], x: f64) -> (Grenze, bool) {
    hits.iter()
        .map(|h| &kanten[h.1])
        .find(|k| !k.schnitt)
        .or_else(|| hits.first().map(|h| &kanten[h.1]))
        .map_or(
            (
                Grenze {
                    a: vec3(x, 0.0, 0.0),
                    b: vec3(x, 1.0, 0.0),
                },
                true,
            ),
            |k| (k.g, k.schnitt),
        )
}

/// Waagerechte Randkanten auf Höhe `y` zwischen dem Streifen darunter
/// (`unten`, Oberkanten) und darüber (`oben`, Unterkanten), je Gruppe: wo
/// nur unten Platte ist, liegt sie links einer Kante nach Westen, wo nur
/// oben, links einer Kante nach Osten.
fn waagerecht(unten: &[Stueck], oben: &[Stueck], y: f64, edges: &mut [Vec<(Vec3, Vec3)>]) {
    // je Seite (Gruppe, links, rechts), nach Gruppe und x geordnet, statt je
    // Gruppe alle Stücke zu filtern
    let iv = |v: &[Stueck]| -> Vec<(usize, f64, f64)> {
        let mut out: Vec<(usize, f64, f64)> = v
            .iter()
            .map(|s| (s.g, s.l.x_bei(y), s.r.x_bei(y)))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        out
    };
    let (u, o) = (iv(unten), iv(oben));
    let (mut i, mut j) = (0, 0);
    while let Some(g) = [u.get(i), o.get(j)]
        .into_iter()
        .flatten()
        .map(|s| s.0)
        .min()
    {
        let bis =
            |v: &[(usize, f64, f64)], k: usize| k + v[k..].iter().take_while(|s| s.0 == g).count();
        let (i1, j1) = (bis(&u, i), bis(&o, j));
        let paare =
            |v: &[(usize, f64, f64)]| -> Vec<(f64, f64)> { v.iter().map(|s| (s.1, s.2)).collect() };
        let (a, b) = (paare(&u[i..i1]), paare(&o[j..j1]));
        for (x0, x1) in differenz(&a, &b) {
            edges[g].push((vec3(x1, y, 0.0), vec3(x0, y, 0.0)));
        }
        for (x0, x1) in differenz(&b, &a) {
            edges[g].push((vec3(x0, y, 0.0), vec3(x1, y, 0.0)));
        }
        (i, j) = (i1, j1);
    }
}

/// `a` ohne `b` (beide sortiert, ohne Überlappung in sich), in einem
/// Durchgang über beide.
fn differenz(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut k = 0;
    for &(mut x0, x1) in a {
        while k < b.len() && b[k].1 <= x0 + EPS {
            k += 1;
        }
        for &(c0, c1) in &b[k..] {
            if c0 >= x1 - EPS {
                break;
            }
            if c1 <= x0 + EPS {
                continue;
            }
            if c0 > x0 + EPS {
                out.push((x0, c0));
            }
            x0 = x0.max(c1);
            if x0 >= x1 - EPS {
                break;
            }
        }
        if x1 - x0 > EPS {
            out.push((x0, x1));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quadrat(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Vec3> {
        vec![
            vec3(x0, y0, 0.0),
            vec3(x1, y0, 0.0),
            vec3(x1, y1, 0.0),
            vec3(x0, y1, 0.0),
        ]
    }

    fn flaeche(p: &[[Vec3; 4]]) -> f64 {
        p.iter().map(|q| sk_math::polygon::signed_area(q)).sum()
    }

    /// Länge der Randkanten mit Vorzeichen der Umlaufrichtung: Summe der
    /// Kreuzprodukte ergibt die doppelte Fläche (geschlossener Rand).
    fn rand_flaeche(e: &[(Vec3, Vec3)]) -> f64 {
        0.5 * e.iter().map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f64>()
    }

    /// Höhenkette (gelaende/schnittstelle.md): neue Höhen nur über
    /// `terrain_z_at(building, x, y)`, damit ein späteres Geländemodell die
    /// Platte mitnimmt; `terrain_z_of` liefert nur die Ebene des Gebäudes.
    #[test]
    fn hoehe_ueber_terrain_z_at() {
        let code = include_str!("gelaende_platte.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(
            !code.contains(concat!("terrain_z", "_of(")),
            "Geländeplatte fragt die Höhe ohne Ort ab"
        );
        assert!(code.contains(concat!("terrain_z", "_at(")));
    }

    /// Ein Rechteck ohne Loch: ein Trapez, Rand = Umfang, Fläche stimmt.
    #[test]
    fn rechteck_ohne_loch() {
        let (p, e) = streifen(&[vec![[0.0, 0.0, 10.0, 6.0]]], &[]);
        assert!((flaeche(&p[0]) - 60.0).abs() < 1e-9);
        assert!((rand_flaeche(&e[0]) - 60.0).abs() < 1e-9);
        let laenge: f64 = e[0].iter().map(|(a, b)| (*b - *a).length()).sum();
        assert!((laenge - 32.0).abs() < 1e-9);
    }

    /// Schräges Haus im Rechteck: Fläche = Rechteck − Haus, der Rand ist
    /// geschlossen (Kreuzsumme = Fläche) und läuft am Loch mit.
    #[test]
    fn schraeges_loch() {
        let haus = vec![
            vec3(4.0, 1.0, 0.0),
            vec3(9.0, 3.0, 0.0),
            vec3(7.0, 8.0, 0.0),
            vec3(2.0, 6.0, 0.0),
        ];
        let a_haus = sk_math::polygon::area(&haus);
        let (p, e) = streifen(&[vec![[0.0, 0.0, 11.0, 9.0]]], &[haus]);
        assert!((flaeche(&p[0]) - (99.0 - a_haus)).abs() < 1e-6);
        assert!((rand_flaeche(&e[0]) - (99.0 - a_haus)).abs() < 1e-6);
        assert!(p[0]
            .iter()
            .all(|q| sk_math::polygon::signed_area(q) >= -1e-9));
    }

    /// L-Haus mit Innenecke und zwei Gruppen: die frühere Gruppe hat
    /// Vorrang, die spätere bekommt nur den Rest, nichts doppelt.
    #[test]
    fn zwei_gruppen_und_vorrang() {
        let l_haus = vec![
            vec3(2.0, 2.0, 0.0),
            vec3(8.0, 2.0, 0.0),
            vec3(8.0, 4.0, 0.0),
            vec3(4.0, 4.0, 0.0),
            vec3(4.0, 8.0, 0.0),
            vec3(2.0, 8.0, 0.0),
        ];
        let a_l = sk_math::polygon::area(&l_haus);
        let r1 = [0.0, 0.0, 10.0, 10.0];
        let r2 = [6.0, 6.0, 16.0, 12.0];
        let garage = quadrat(12.0, 8.0, 14.0, 10.0);
        let (p, e) = streifen(&[vec![r1], vec![r2]], &[l_haus, garage]);
        let a0 = flaeche(&p[0]);
        let a1 = flaeche(&p[1]);
        assert!((a0 - (100.0 - a_l)).abs() < 1e-6, "{a0}");
        // r2 ohne Überdeckung mit r1 (4 × 4) und ohne Garage (2 × 2)
        assert!((a1 - (60.0 - 16.0 - 4.0)).abs() < 1e-6, "{a1}");
        assert!((rand_flaeche(&e[0]) - a0).abs() < 1e-6);
        assert!((rand_flaeche(&e[1]) - a1).abs() < 1e-6);
    }

    /// Haus 10 × 8 m mit Gründung: eine Platte in Höhe OK Gelände, 3 m
    /// über die Wandzüge hinaus, mit dem Plattenumriss als Loch; sie folgt
    /// dem Versatz im Geschossmanager und den eingestellten Werten.
    #[test]
    fn platte_um_das_haus() {
        let mut m = Model::with_seed(12);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let p = m.terrain_plates();
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].top, p[0].bottom), (0.0, -500.0));
        assert_eq!(p[0].buildings, vec![b]);
        let a = p[0].area();
        assert!((a - (16.0 * 14.0 - 80.0) * 1e6).abs() < 1.0, "{a}");
        assert!((rand_flaeche(&p[0].edges) - a).abs() < 1.0);
        m.begin("Gelände");
        assert!(m.set_terrain_offset_of(b, 300.0));
        assert!(!m.set_plate_settings(PlateSettings {
            thick: 500.0,
            margin: 1000.0
        }));
        assert!(m.set_plate_settings(PlateSettings {
            thick: 800.0,
            margin: 1500.0
        }));
        m.commit();
        let p = m.terrain_plates();
        assert_eq!((p[0].top, p[0].bottom), (-300.0, -1100.0));
        let a = p[0].area();
        assert!((a - (13.0 * 11.0 - 80.0) * 1e6).abs() < 1.0, "{a}");
    }

    /// Budget im Release (Review W34 B2), im Debug großzügig.
    fn budget(ms: f64) -> f64 {
        if cfg!(debug_assertions) {
            ms * 40.0
        } else {
            ms
        }
    }

    /// `n` schräg gestellte Häuser 10 × 8 m (80 m²) im Raster zu 15 m, in
    /// einem Gebäude oder je Haus ein Gebäude, mit Innenwand und je Haus
    /// eigener Geländehöhe (997 verschiedene).
    fn haeuser(n: usize, je_haus: bool, innen: bool, hoehen: bool) -> Model {
        let mut m = Model::with_seed(5);
        let b0 = m.add_building(1);
        let iw = m.defaults().interior_wall;
        let w = 0.3_f64;
        let mut gebaeude = Vec::new();
        for i in 0..n {
            let b = if je_haus && i > 0 {
                m.add_building(1)
            } else {
                b0
            };
            gebaeude.push(b);
            let (ox, oy) = ((i % 64) as f64 * 15000.0, (i / 64) as f64 * 15000.0);
            let p = |x: f64, y: f64| {
                vec3(
                    ox + x * w.cos() - y * w.sin(),
                    oy + x * w.sin() + y * w.cos(),
                    0.0,
                )
            };
            m.build_from_polygon(
                b,
                &[
                    p(0.0, 0.0),
                    p(0.0, 8000.0),
                    p(10000.0, 8000.0),
                    p(10000.0, 0.0),
                ],
            )
            .unwrap();
            if innen {
                let eg = m.ground_of(Some(b)).unwrap();
                m.add_wall_run(
                    &[p(5000.0, 0.0), p(5000.0, 8000.0)],
                    false,
                    RefSide::Center,
                    eg,
                    iw,
                    Category::InteriorWall,
                )
                .unwrap();
            }
        }
        if hoehen {
            m.begin("Gelände");
            for (i, b) in gebaeude.iter().enumerate() {
                m.set_terrain_offset_of(*b, (i % 997) as f64 * 3.0);
            }
            m.commit();
        }
        m
    }

    /// Platten von `m` mit Zeit (ms); jedes Haus ist ausgespart, der Rand
    /// geschlossen, die Stücke je Haus begrenzt.
    fn platten_geprueft(m: &Model, n: usize) -> (Vec<TerrainPlate>, f64) {
        let t = std::time::Instant::now();
        let p = m.terrain_plates();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let a: f64 = p.iter().map(TerrainPlate::area).sum();
        let rand: f64 = p.iter().map(|q| rand_flaeche(&q.edges)).sum();
        assert!((rand - a).abs() < 1e-6 * a, "{rand} {a}");
        let stuecke: usize = p.iter().map(|q| q.pieces.len()).sum();
        assert!(stuecke < 40 * n, "{stuecke} Stücke");
        (p, ms)
    }

    /// Budget (Review W34 B2, Nachtrag): 4000 Häuser in einem Gebäude und je
    /// Haus ein Gebäude. In einem Gebäude ist es ein Rechteck um alle, aus
    /// dem jedes Haus ausgespart ist.
    #[test]
    fn viertausend_haeuser_im_budget() {
        const N: usize = 4000;
        for je_haus in [false, true] {
            let m = haeuser(N, je_haus, false, false);
            let (p, ms) = platten_geprueft(&m, N);
            // vorher 973 ms je Haus ein Gebäude (Review, Release)
            assert!(ms < budget(250.0), "je Haus {je_haus}: {ms:.1} ms");
            assert_eq!(p.len(), 1, "eine Höhe, eine Platte");
            if !je_haus {
                let (mut lo, mut hi) = (vec3(1e18, 1e18, 0.0), vec3(-1e18, -1e18, 0.0));
                for (_, r) in m.runs().iter() {
                    for q in &r.points {
                        lo = vec3(lo.x.min(q.x), lo.y.min(q.y), 0.0);
                        hi = vec3(hi.x.max(q.x), hi.y.max(q.y), 0.0);
                    }
                }
                let rahmen = (hi.x - lo.x + 6000.0) * (hi.y - lo.y + 6000.0);
                let soll = rahmen - N as f64 * 80e6;
                let a = p[0].area();
                assert!((a - soll).abs() < 1e-6 * soll, "{a} {soll}");
            }
        }
    }

    /// Budget mit Innenwänden (Review W34 B2, Nachtrag: mehr Züge, die
    /// Gründung über das Verzeichnis) und je Haus eigener Geländehöhe.
    #[test]
    fn innenwaende_und_hoehen_im_budget() {
        const N: usize = 1000;
        for (je_haus, hoehen) in [(false, false), (true, true)] {
            let m = haeuser(N, je_haus, true, hoehen);
            let (p, ms) = platten_geprueft(&m, N);
            // vorher 98 ms in einem Gebäude mit Innenwänden (Review, Release)
            assert!(ms < budget(60.0), "je Haus {je_haus}: {ms:.1} ms");
            assert_eq!(p.len(), if hoehen { 997 } else { 1 });
        }
    }

    /// Entartete Eingaben (Review W34 B2, Nachtrag): kollineare Punkte,
    /// lauter gleiche Häuser, viele Höhen und eine fast waagerechte Reihe
    /// rechnen in der Zeit der Eingabe, die Fläche stimmt.
    #[test]
    fn entartete_eingaben_im_budget() {
        let zeit = |f: &dyn Fn() -> Zerlegt| {
            let t = std::time::Instant::now();
            let z = f();
            (z, t.elapsed().as_secs_f64() * 1000.0)
        };
        // ein Loch mit 100.000 kollinearen Punkten an einer schrägen Kante
        let mut loch: Vec<Vec3> = (0..=100_000)
            .map(|i| {
                let t = i as f64 / 100_000.0;
                vec3(1000.0 + t * 998_000.0, 1000.0 + t * 8000.0, 0.0)
            })
            .collect();
        loch.extend([vec3(999_000.0, 15_000.0, 0.0), vec3(1000.0, 15_000.0, 0.0)]);
        let a_loch = sk_math::polygon::area(&loch);
        let ((p, e), ms) = zeit(&|| streifen(&[vec![[0.0, 0.0, 1e6, 2e4]]], &[loch.clone()]));
        let a = flaeche(&p[0]);
        assert!((a - (2e10 - a_loch)).abs() < 1e-6 * a, "{a}");
        assert!((rand_flaeche(&e[0]) - a).abs() < 1e-6 * a);
        assert!(ms < budget(300.0), "kollinear: {ms:.1} ms");
        // gleiche Häuser, jedes ein eigenes Gebäude mit eigener Höhe
        // (Gruppe) an derselben Stelle: 25.000 vorher 8,6 s (Review #119 B5)
        for (zahl, grenze) in [(1000, 100.0), (25_000, 300.0)] {
            let rects = vec![vec![[0.0, 0.0, 20_000.0, 20_000.0]]; zahl];
            let holes = vec![quadrat(5000.0, 5000.0, 15_000.0, 15_000.0); zahl];
            let ((p, e), ms) = zeit(&|| streifen(&rects, &holes));
            assert!((flaeche(&p[0]) - 3e8).abs() < 1e-3);
            assert!(p[1..].iter().all(|q| q.is_empty()));
            assert!((rand_flaeche(&e[0]) - 3e8).abs() < 1e-3);
            assert!(ms < budget(grenze), "{zahl} gleiche: {ms:.1} ms");
        }
        // 4096 Gruppen (eigene Höhen), die sich je mit den Nachbarn überdecken
        let ort = |i: usize| ((i % 64) as f64 * 15_000.0, (i / 64) as f64 * 12_000.0);
        let rects: Vec<Vec<[f64; 4]>> = (0..4096)
            .map(|i| {
                let (x, y) = ort(i);
                vec![[x - 3000.0, y - 3000.0, x + 13_000.0, y + 11_000.0]]
            })
            .collect();
        let holes: Vec<Vec<Vec3>> = (0..4096)
            .map(|i| {
                let (x, y) = ort(i);
                quadrat(x, y, x + 10_000.0, y + 8000.0)
            })
            .collect();
        let ((p, e), ms) = zeit(&|| streifen(&rects, &holes));
        let a: f64 = p.iter().map(|q| flaeche(q)).sum();
        let rand: f64 = e.iter().map(|q| rand_flaeche(q)).sum();
        // Vereinigung der Rechtecke ohne die Häuser
        let soll = (63.0 * 15_000.0 + 16_000.0) * (63.0 * 12_000.0 + 14_000.0) - 4096.0 * 80e6;
        assert!((a - soll).abs() < 1e-6 * soll, "{a} {soll}");
        assert!((rand - a).abs() < 1e-6 * a);
        assert!(ms < budget(200.0), "viele Höhen: {ms:.1} ms");
        // 10.000 Häuser in einer fast waagerechten Reihe, eine Gruppe
        let holes: Vec<Vec<Vec3>> = (0..10_000)
            .map(|i| {
                let (x, y) = (i as f64 * 15_000.0, i as f64 * 1.5);
                quadrat(x, y, x + 10_000.0, y + 8000.0)
            })
            .collect();
        let rect = [
            -3000.0,
            -3000.0,
            10_000.0 * 15_000.0,
            8000.0 + 15_000.0 + 3000.0,
        ];
        let ((p, e), ms) = zeit(&|| streifen(&[vec![rect]], &holes));
        let a = flaeche(&p[0]);
        let soll = (rect[2] - rect[0]) * (rect[3] - rect[1]) - 10_000.0 * 80e6;
        assert!((a - soll).abs() < 1e-6 * soll, "{a} {soll}");
        assert!((rand_flaeche(&e[0]) - a).abs() < 1e-6 * a);
        assert!(p[0].len() < 40 * 10_000, "{} Stücke", p[0].len());
        assert!(ms < budget(300.0), "Reihe: {ms:.1} ms");
    }

    /// Scheiben ändern nichts: schräge Häuser in einem Rechteck, mit und
    /// ohne Teilung dieselbe Fläche, derselbe geschlossene Rand, keine
    /// Randkante auf einer Scheibengrenze.
    #[test]
    fn scheiben_ohne_naht() {
        let haus = |ox: f64, oy: f64, w: f64| -> Vec<Vec3> {
            [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)]
                .iter()
                .map(|&(x, y)| {
                    vec3(
                        ox + x * w.cos() - y * w.sin(),
                        oy + x * w.sin() + y * w.cos(),
                        0.0,
                    )
                })
                .collect()
        };
        let holes: Vec<Vec<Vec3>> = (0..300)
            .map(|i| {
                haus(
                    (i % 20) as f64 * 17.0,
                    (i / 20) as f64 * 15.0,
                    0.1 * (i % 7) as f64,
                )
            })
            .collect();
        let a_loecher: f64 = holes.iter().map(|h| sk_math::polygon::area(h)).sum();
        let rect = [-5.0, -5.0, 20.0 * 17.0 + 5.0, 15.0 * 15.0 + 5.0];
        let (p, e) = streifen(&[vec![rect]], &holes);
        let a = flaeche(&p[0]);
        let soll = (rect[2] - rect[0]) * (rect[3] - rect[1]) - a_loecher;
        assert!((a - soll).abs() < 1e-6 * soll, "{a} {soll}");
        assert!((rand_flaeche(&e[0]) - a).abs() < 1e-6 * a);
        // Randlänge: Rechteckumfang plus Umfang der Häuser (36 m je Haus)
        let laenge: f64 = e[0].iter().map(|(a, b)| (*b - *a).length()).sum();
        let soll = 2.0 * (rect[2] - rect[0] + rect[3] - rect[1]) + 300.0 * 36.0;
        assert!((laenge - soll).abs() < 1e-6 * soll, "{laenge} {soll}");
    }

    /// Zwei Löcher, die sich überdecken (Anbau an einem Haus): die Platte
    /// spart ihre Vereinigung aus, die Überdeckung bleibt Loch.
    #[test]
    fn ueberlappende_loecher() {
        let (p, e) = streifen(
            &[vec![[0.0, 0.0, 12.0, 10.0]]],
            &[quadrat(2.0, 2.0, 7.0, 7.0), quadrat(5.0, 4.0, 10.0, 8.0)],
        );
        let a = flaeche(&p[0]);
        assert!((a - (120.0 - (25.0 + 20.0 - 6.0))).abs() < 1e-9, "{a}");
        assert!((rand_flaeche(&e[0]) - a).abs() < 1e-9);
    }

    /// Zwei Rechtecke derselben Gruppe, die sich überlappen und berühren:
    /// eine Fläche ohne Naht innen (Rand = Außenumfang).
    #[test]
    fn vereinigung_einer_gruppe() {
        let (p, e) = streifen(&[vec![[0.0, 0.0, 6.0, 4.0], [4.0, 2.0, 10.0, 6.0]]], &[]);
        let a = flaeche(&p[0]);
        assert!((a - (24.0 + 24.0 - 4.0)).abs() < 1e-9, "{a}");
        let laenge: f64 = e[0].iter().map(|(a, b)| (*b - *a).length()).sum();
        assert!((laenge - 32.0).abs() < 1e-9, "{laenge}");
    }
}

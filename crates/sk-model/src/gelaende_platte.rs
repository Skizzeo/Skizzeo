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

    /// Außenumriss jedes Hauses über seiner Gründung, gegen den
    /// Uhrzeigersinn (z = 0): die Außenfläche der Wände mit allen Schichten,
    /// auch bei Sockelrücksprung, sonst der Plattenumriss. Hier endet das
    /// Gelände am Haus (Loch der Platte, Plattenrand der Höhenlage).
    pub fn plate_outlines(&self) -> Vec<(BuildingId, Vec<Vec3>)> {
        self.runs
            .iter()
            .filter_map(|(id, r)| Some((self.building_of(r.storey)?, self.plate_outline(id)?)))
            .collect()
    }

    /// Außenumriss über der Gründung des Wandzugs `run` (siehe
    /// [`Model::plate_outlines`]); `None` ohne Gründung.
    pub(super) fn plate_outline(&self, run: RunId) -> Option<Vec<Vec3>> {
        self.foundation_of(run)?;
        let face = self
            .base_chain(run)
            .map(|c| c.face_corners(c.outer_offset()))
            .filter(|f| f.len() >= 3 && sk_math::polygon::area(f) > 1.0);
        let outline = face.or_else(|| match self.foundation(run) {
            Some(Ok(f)) if f.outline.len() >= 3 => Some(f.outline),
            _ => None,
        })?;
        Some(sk_math::polygon::to_ccw(&outline))
    }

    /// Die Geländeplatten des Projekts, nach dem ersten Gebäude geordnet;
    /// leer ohne Wandzüge.
    pub fn terrain_plates(&self) -> Vec<TerrainPlate> {
        let s = self.plate_settings();
        // Rechtecke je Gebäude, nach Guid
        let mut boxes: Vec<(Guid, BuildingId, [f64; 4])> = Vec::new();
        for (_, r) in self.runs.iter() {
            let Some(b) = self.building_of(r.storey) else {
                continue;
            };
            let Some(guid) = self.building(b).map(|x| x.guid) else {
                continue;
            };
            let i = match boxes.iter().position(|x| x.1 == b) {
                Some(i) => i,
                None => {
                    let e = f64::INFINITY;
                    boxes.push((guid, b, [e, e, -e, -e]));
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
        // Gruppen gleicher Geländehöhe in der Reihenfolge ihres ersten Gebäudes
        let mut groups: Vec<(f64, Vec<BuildingId>, Vec<[f64; 4]>)> = Vec::new();
        for (_, b, x) in &boxes {
            // Höhenkette: Höhe über den Ort abfragen (Rechteckmitte), damit
            // ein Geländemodell die Platte mitnimmt
            let z = self.terrain_z_at(Some(*b), (x[0] + x[2]) / 2.0, (x[1] + x[3]) / 2.0);
            let m = s.margin;
            let r = [x[0] - m, x[1] - m, x[2] + m, x[3] + m];
            match groups.iter_mut().find(|g| (g.0 - z).abs() < 1e-6) {
                Some(g) => {
                    g.1.push(*b);
                    g.2.push(r);
                }
                None => groups.push((z, vec![*b], vec![r])),
            }
        }
        let holes: Vec<Vec<Vec3>> = self.plate_outlines().into_iter().map(|o| o.1).collect();
        let rects: Vec<Vec<[f64; 4]>> = groups.iter().map(|g| g.2.clone()).collect();
        let (pieces, edges) = streifen(&rects, &holes);
        groups
            .into_iter()
            .zip(pieces.into_iter().zip(edges))
            .filter(|(_, (p, _))| !p.is_empty())
            .map(|((z, bs, _), (pieces, edges))| TerrainPlate {
                buildings: bs,
                top: z,
                bottom: z - s.thick,
                pieces,
                edges,
            })
            .collect()
    }
}

/// Abstand, unter dem zwei Werte als gleich gelten (mm).
const EPS: f64 = 1e-6;

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

/// Zerlegt die Gruppen (je eine Liste von Rechtecken) abzüglich der Löcher in
/// Trapeze und Randkanten; eine frühere Gruppe hat Vorrang vor einer späteren.
#[allow(clippy::type_complexity)]
fn streifen(
    rects: &[Vec<[f64; 4]>],
    holes: &[Vec<Vec3>],
) -> (Vec<Vec<[Vec3; 4]>>, Vec<Vec<(Vec3, Vec3)>>) {
    let n = rects.len();
    let mut pieces = vec![Vec::new(); n];
    let mut edges = vec![Vec::new(); n];
    // alle Kanten mit ihrer Art
    let mut kanten: Vec<(Grenze, Art)> = Vec::new();
    for (g, rs) in rects.iter().enumerate() {
        for r in rs {
            let (a, b) = (vec3(r[0], r[1], 0.0), vec3(r[0], r[3], 0.0));
            kanten.push((Grenze { a, b }, Art::Rechteck(g, true)));
            let (a, b) = (vec3(r[2], r[1], 0.0), vec3(r[2], r[3], 0.0));
            kanten.push((Grenze { a, b }, Art::Rechteck(g, false)));
        }
    }
    for h in holes {
        for i in 0..h.len() {
            let (a, b) = (h[i], h[(i + 1) % h.len()]);
            if (a.y - b.y).abs() > EPS {
                let w = if b.y < a.y { 1 } else { -1 };
                kanten.push((Grenze { a, b }, Art::Loch(w)));
            }
        }
    }
    // Streifengrenzen: Ecken und Schnittpunkte der Kanten
    let mut ys: Vec<f64> = Vec::new();
    for rs in rects {
        for r in rs {
            ys.extend([r[1], r[3]]);
        }
    }
    for h in holes {
        ys.extend(h.iter().map(|p| p.y));
    }
    // Schnitte nur zwischen Kanten, deren Rahmen sich überdecken: nach
    // linkem Rand sortiert, je Kante die folgenden bis zu ihrem rechten Rand
    let rahmen: Vec<[f64; 4]> = kanten
        .iter()
        .map(|(k, _)| {
            [
                k.a.x.min(k.b.x),
                k.a.x.max(k.b.x),
                k.a.y.min(k.b.y),
                k.a.y.max(k.b.y),
            ]
        })
        .collect();
    let mut nach_x: Vec<usize> = (0..kanten.len()).collect();
    nach_x.sort_by(|&i, &j| rahmen[i][0].total_cmp(&rahmen[j][0]));
    for (o, &i) in nach_x.iter().enumerate() {
        for &j in &nach_x[o + 1..] {
            if rahmen[j][0] > rahmen[i][1] + EPS {
                break;
            }
            if rahmen[j][2] > rahmen[i][3] + EPS || rahmen[i][2] > rahmen[j][3] + EPS {
                continue;
            }
            if let Some(y) = schnitt_y(&kanten[i].0, &kanten[j].0) {
                ys.push(y);
            }
        }
    }
    ys.retain(|y| y.is_finite());
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < EPS);
    // aktive Kanten: von unten nach oben aufgenommen, oben entlassen
    let mut nach_y: Vec<usize> = (0..kanten.len()).collect();
    nach_y.sort_by(|&i, &j| rahmen[i][2].total_cmp(&rahmen[j][2]));
    let mut neu = nach_y.into_iter().peekable();
    let mut aktiv: Vec<usize> = Vec::new();
    // je Streifen die Intervalle jeder Gruppe; `vorher` merkt die Intervalle
    // des Streifens darunter (für die waagerechten Randkanten)
    let mut vorher: Vec<Vec<(Grenze, Grenze)>> = vec![Vec::new(); n];
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
        let jetzt = intervalle(aktiv.iter().map(|&k| &kanten[k]), n, ym);
        for g in 0..n {
            waagerecht(&vorher[g], &jetzt[g], y0, &mut edges[g]);
            for (l, r) in &jetzt[g] {
                let (l0, l1, r0, r1) = (l.x_bei(y0), l.x_bei(y1), r.x_bei(y0), r.x_bei(y1));
                pieces[g].push([
                    vec3(l0, y0, 0.0),
                    vec3(r0, y0, 0.0),
                    vec3(r1, y1, 0.0),
                    vec3(l1, y1, 0.0),
                ]);
                edges[g].push((vec3(l1, y1, 0.0), vec3(l0, y0, 0.0)));
                edges[g].push((vec3(r0, y0, 0.0), vec3(r1, y1, 0.0)));
            }
        }
        // die Oberkante dieses Streifens ist die Unterkante des nächsten
        vorher = jetzt
            .into_iter()
            .map(|v| {
                v.into_iter()
                    .map(|(l, r)| (fest(l, y1), fest(r, y1)))
                    .collect()
            })
            .collect();
    }
    if let Some(&y) = ys.last() {
        for g in 0..n {
            waagerecht(&vorher[g], &[], y, &mut edges[g]);
        }
    }
    (pieces, edges)
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

/// Intervalle jeder Gruppe auf der Höhe `y` (Mitte eines Streifens): in einem
/// Rechteck der Gruppe, in keinem Loch, in keinem Rechteck einer früheren
/// Gruppe. Je Intervall die Grenzen links und rechts.
fn intervalle<'a>(
    kanten: impl Iterator<Item = &'a (Grenze, Art)>,
    n: usize,
    y: f64,
) -> Vec<Vec<(Grenze, Grenze)>> {
    // Treffer: x, Grenze, Art
    let mut hits: Vec<(f64, Grenze, Art)> = kanten
        .filter(|(k, _)| {
            let (lo, hi) = (k.a.y.min(k.b.y), k.a.y.max(k.b.y));
            lo < y && y < hi
        })
        .map(|(k, a)| (k.x_bei(y), *k, *a))
        .collect();
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut zaehler = vec![0i32; n];
    let mut loch = 0i32;
    let mut out: Vec<Vec<(Grenze, Grenze)>> = vec![Vec::new(); n];
    let mut offen: Vec<Option<Grenze>> = vec![None; n];
    let mut i = 0;
    while i < hits.len() {
        // alle Treffer an derselben Stelle zusammen
        let x = hits[i].0;
        let mut j = i;
        while j < hits.len() && (hits[j].0 - x).abs() < EPS {
            match hits[j].2 {
                // Rechtecke: linke Kante öffnet, rechte schließt
                Art::Rechteck(g, links) => zaehler[g] += if links { 1 } else { -1 },
                // Löcher: Umlaufzahl
                Art::Loch(w) => loch += w,
            }
            j += 1;
        }
        // Zustand nach der Stelle
        let mut frueher = false;
        for g in 0..n {
            let drin = zaehler[g] > 0 && loch <= 0 && !frueher;
            frueher |= zaehler[g] > 0;
            match (offen[g], drin) {
                (None, true) => offen[g] = Some(grenze_an(&hits[i..j], x)),
                (Some(l), false) => {
                    let r = grenze_an(&hits[i..j], x);
                    if r.x_bei(y) - l.x_bei(y) > EPS {
                        out[g].push((l, r));
                    }
                    offen[g] = None;
                }
                _ => {}
            }
        }
        i = j;
    }
    out
}

/// Die Grenze an einer Stelle: bei mehreren Treffern die erste.
fn grenze_an(hits: &[(f64, Grenze, Art)], x: f64) -> Grenze {
    hits.first().map(|h| h.1).unwrap_or(Grenze {
        a: vec3(x, 0.0, 0.0),
        b: vec3(x, 1.0, 0.0),
    })
}

/// Waagerechte Randkanten auf Höhe `y` zwischen dem Streifen darunter
/// (`unten`, Oberkanten) und darüber (`oben`, Unterkanten): wo nur unten
/// Platte ist, liegt sie links einer Kante nach Westen, wo nur oben, links
/// einer Kante nach Osten.
fn waagerecht(
    unten: &[(Grenze, Grenze)],
    oben: &[(Grenze, Grenze)],
    y: f64,
    edges: &mut Vec<(Vec3, Vec3)>,
) {
    let iv = |v: &[(Grenze, Grenze)]| -> Vec<(f64, f64)> {
        v.iter().map(|(l, r)| (l.x_bei(y), r.x_bei(y))).collect()
    };
    let (u, o) = (iv(unten), iv(oben));
    for (a, b) in differenz(&u, &o) {
        edges.push((vec3(b, y, 0.0), vec3(a, y, 0.0)));
    }
    for (a, b) in differenz(&o, &u) {
        edges.push((vec3(a, y, 0.0), vec3(b, y, 0.0)));
    }
}

/// `a` ohne `b` (beide sortiert, ohne Überlappung in sich).
fn differenz(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &(mut x0, x1) in a {
        for &(c0, c1) in b {
            if c1 <= x0 + EPS || c0 >= x1 - EPS {
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

    /// Budget (Review W34 B2): 1000 schräg gestellte Häuser in einem
    /// Gebäude und je Haus ein Gebäude; die Platten rechnen fast linear
    /// (Kantenpaare nur bei sich überdeckenden Rahmen, je Streifen nur die
    /// aktiven Kanten), jedes Haus ist ausgespart.
    #[test]
    fn tausend_haeuser_im_budget() {
        for je_haus in [false, true] {
            let mut m = Model::with_seed(5);
            let b0 = m.add_building(1);
            let w = 0.3_f64;
            for i in 0..1000 {
                let b = if je_haus && i > 0 {
                    m.add_building(1)
                } else {
                    b0
                };
                let (ox, oy) = ((i % 32) as f64 * 15000.0, (i / 32) as f64 * 15000.0);
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
            }
            let t = std::time::Instant::now();
            let p = m.terrain_plates();
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            // vorher 77 ms (Review, Release); jetzt um 20 ms
            let budget = if cfg!(debug_assertions) {
                3000.0
            } else {
                200.0
            };
            assert!(ms < budget, "je Haus ein Gebäude {je_haus}: {ms:.1} ms");
            let a: f64 = p.iter().map(TerrainPlate::area).sum();
            let rand: f64 = p.iter().map(|q| rand_flaeche(&q.edges)).sum();
            assert!((rand - a).abs() < 1e-6 * a, "{rand} {a}");
        }
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

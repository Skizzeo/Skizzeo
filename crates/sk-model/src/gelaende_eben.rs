//! Ein Gelände von Anfang an (I1, gelaende/analyse-interaktives-gelaende.md
//! §4.1): Ohne Aufmaß liegt um jedes Haus die ebene Platte in Höhe OK
//! Gelände. Braucht ein Werkzeug des Geländemoduls ein Netz (die erste
//! Geländefläche), legt Skizzeo daraus ein Gelände mit Punkten von Hand an:
//! die Ecken der Platten in ihrer Höhe, so dass nichts springt (§5 J4 B).
//! Bis dahin steht in der Datei nichts (J3 B: alte Dateien bleiben
//! bytegleich).
//!
//! Die Grenze geht auch ohne Lageplan: von Hand gezeichnet (I2,
//! [`Model::add_parcel`]) liegt sie wie ein Flurstück im Gelände. Ein
//! angenommenes Gelände reicht dann bis über sie hinaus (I6, Jörn 17.10.):
//! Die ebene Platte wächst bis zur Grenze und um den Plattenabstand
//! darüber, die Umgebung außerhalb zeigt sie ganz, blass oder gar nicht.

use super::gelaende_punkte::{doppelt_bei, punkte_dazu};
use super::*;
use std::collections::{HashMap, HashSet};

/// Erste Nummer der Punkte von Hand; die übrigen zählen weiter („H2“ …).
const NR: &str = "H";

/// Halbe Breite (mm) des Streifens zwischen zwei getrennten Platten: Dort
/// geht das Gelände von der einen Höhe in die andere über. So breit wie
/// der Abstand, unter dem die Prüfung zwei Punkte als dieselbe Stelle
/// nimmt: Die Ecken beider Seiten liegen dann auch nach dem Runden auf mm
/// fast doppelt so weit auseinander. Mit 5 mm fiel eine Ecke als „Mauer
/// oder Bord?“ heraus, und beide Häuser lagen 0,76 m daneben (Geometrie zu
/// #245).
const FUGE: f64 = sk_gelaende::pruefen::GLEICHE_STELLE * 1000.0;

/// Code der Punkte, die Skizzeo an eine gezeichnete Grenze setzt
/// ([`anheften`]), und der Ecken, mit denen ein angenommenes Gelände über
/// die Grenzen hinaus reicht ([`abdecken`]).
/// Ändert sich die Grenze, nimmt Skizzeo sie heraus und setzt sie passend
/// neu: Dieselbe Grenze gibt dasselbe Gelände und dieselben Mengen, ob
/// gezeichnet oder gezogen (VOB B-264a). Wer einen davon zieht oder hebt,
/// macht ihn zu seinem eigenen ([`Model::move_terrain_points`]); der
/// bleibt. Unbekannte Codes gelten als Gelände.
pub(super) const AN_DER_GRENZE: &str = "SK-GRENZE";
/// Siehe [`AN_DER_GRENZE`].
pub(super) const UM_DIE_GRENZE: &str = "SK-RAND";

#[path = "gelaende_trennen.rs"]
mod trennen;

impl Terrain {
    /// Das Gelände hat Punkte, und alle sind von Hand gesetzt: Es ist
    /// angenommen, nicht vermessen. Mengen nennen das im Ansatz.
    pub fn assumed(&self) -> bool {
        !self.points.is_empty() && self.points.iter().all(|p| p.hand)
    }
}

impl Model {
    /// Ein Gelände ohne Punkte (keins oder nur ein Grundstück aus dem
    /// Lageplan) bekommt Punkte von Hand an den Ecken der Platten um die
    /// Häuser, je Haus in seiner Höhe OK Gelände, dazu um `extra` (mm, etwa
    /// die Ecken einer neuen Geländefläche) und die vorhandenen Flächen ein
    /// Rechteck im Abstand der Platte in der Höhe des Bezugsgebäudes (im
    /// offenen Schritt). Ohne
    /// Gelände entsteht eins im örtlichen System, Nullpunkt im Modell.
    /// `false`, wenn schon Punkte da sind oder es keine Ecken gibt.
    pub fn seed_flat_terrain(&mut self, extra: &[[f64; 2]]) -> bool {
        if self.terrain().is_some_and(|t| !t.points.is_empty()) {
            return false;
        }
        let s = self.plate_settings();
        let bereiche = trennen::trennen(&self.platten_zum_trennen(s.margin), FUGE);
        // je Ecke ihre Höhe und ihr Umlauf: die Ecken der Bereiche, die von
        // den Platten nach dem Trennen bleiben
        let mut ecken: Vec<([f64; 2], f64, Option<usize>)> = Vec::new();
        let mut umlaeufe = 0;
        for b in &bereiche {
            for u in b.umlaeufe() {
                if b.rand {
                    ecken.extend(u.into_iter().map(|p| (p, b.z, Some(umlaeufe))));
                    umlaeufe += 1;
                } else {
                    // Ecken in einer früheren Platte fallen weg (wie main)
                    let frei = |p: &[f64; 2]| {
                        !b.verdeckt
                            .iter()
                            .any(|r| (r[0]..=r[2]).contains(&p[0]) && (r[1]..=r[3]).contains(&p[1]))
                    };
                    ecken.extend(u.into_iter().filter(frei).map(|p| (p, b.z, None)));
                }
            }
        }
        // die Geländeflächen, die schon da sind, und das Grundstück liegen
        // mit darauf (I2)
        let flaechen = self.site_patches().iter().flat_map(|f| f.outline.iter());
        let grenzen: Vec<Vec<[f64; 2]>> = match self.terrain() {
            Some(t) => match self.survey_frame_of(t) {
                Some(f) => t
                    .boundary_rings()
                    .iter()
                    .map(|r| r.iter().map(|&(e, n)| f.to_model(e, n)).collect())
                    .collect(),
                None => Vec::new(),
            },
            None => Vec::new(),
        };
        let mut extra: Vec<[f64; 2]> = extra.iter().chain(flaechen).copied().collect();
        // ohne Haus und Fläche liegt das Rechteck um die Grenzen
        if extra.is_empty() && ecken.is_empty() {
            extra.extend(grenzen.iter().flatten());
        }
        if !extra.is_empty() {
            let e = f64::INFINITY;
            let mut k = [e, e, -e, -e];
            for p in &extra {
                k = [
                    k[0].min(p[0]),
                    k[1].min(p[1]),
                    k[2].max(p[0]),
                    k[3].max(p[1]),
                ];
            }
            let m = s.margin;
            let z = self.terrain_z_of(self.level_building());
            let r = [k[0] - m, k[1] - m, k[2] + m, k[3] + m];
            for p in [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]] {
                // in einer Platte gilt deren Höhe
                if !bereiche.iter().any(|b| b.enthaelt(p)) {
                    ecken.push((p, z, None));
                }
            }
        }
        let mut gesehen = std::collections::HashSet::new();
        ecken.retain(|(p, ..)| gesehen.insert(p.map(f64::to_bits)));
        if ecken.len() < 3 {
            return false;
        }
        let mut t = self.terrain().cloned().unwrap_or_else(|| self.oertlich());
        let Some(f) = self.survey_frame_of(&t) else {
            return false;
        };
        // ±0,00 bleibt, wo es ist: fest in NHN, sonst bei 0
        let h0 = self.location().nhn.unwrap_or(0.0);
        // Die Ränder der Platten werden Bruchkanten: Sonst verbindet das
        // Netz quer durch eine Platte mit der Ecke eines anderen Hauses in
        // anderer Höhe, und es springt doch (J4 B; Geometrie, Routine
        // 16.10. 19:03).
        let mut ringe: Vec<Vec<String>> = vec![Vec::new(); umlaeufe];
        for (i, (p, z, umlauf)) in ecken.iter().enumerate() {
            let nr = format!("{NR}{}", i + 1);
            if let Some(k) = *umlauf {
                ringe[k].push(nr.clone());
            }
            let (e, n) = f.to_survey(p[0], p[1]);
            t.points.push(TerrainPoint {
                nr: nr.into(),
                e: rund(e),
                n: rund(n),
                h: rund(h0 + z / 1000.0),
                code: Default::default(),
                off: false,
                hand: true,
                gemessen: None,
            });
        }
        t.breaks.extend(
            ringe
                .into_iter()
                .filter(|r| r.len() >= 3)
                .map(|pts| TerrainLine { pts, closed: true }),
        );
        if !self.set_terrain(Some(t)) {
            return false;
        }
        // Die Grenzen halten das Gelände (Jörn 17.10.): Ecken um sie und
        // Punkte an ihren Ecken und Viertelpunkten, wie wenn sie erst nach
        // der Fläche gezeichnet wären; Fläche und Grenze geben in jeder
        // Reihenfolge dasselbe Gelände (VOB B-264d).
        for ring in &grenzen {
            if let Some(t) = self.terrain().cloned() {
                self.grenze_bauen(t, ring, None);
            }
        }
        true
    }

    /// Die Platten um die Häuser zum Trennen, nach Guid wie
    /// [`Model::terrain_patches`], mit dem Rechteck um ihre Sohle samt
    /// Überstand ([`Model::plate_outlines`]).
    fn platten_zum_trennen(&self, margin: f64) -> Vec<trennen::Platte> {
        let sohlen = self.plate_outlines();
        self.terrain_patch_boxes()
            .into_iter()
            .map(|(b, w)| {
                let sohle = sohlen
                    .iter()
                    .filter(|(x, _)| *x == b)
                    .flat_map(|(_, r)| r.iter())
                    .fold(None, |k: Option<[f64; 4]>, p| {
                        Some(k.map_or([p.x, p.y, p.x, p.y], |k| {
                            [k[0].min(p.x), k[1].min(p.y), k[2].max(p.x), k[3].max(p.y)]
                        }))
                    })
                    .unwrap_or(w);
                let m = margin;
                trennen::Platte {
                    rect: [w[0] - m, w[1] - m, w[2] + m, w[3] + m],
                    wand: w,
                    sohle,
                    z: self.terrain_z_of(Some(b)),
                }
            })
            .collect()
    }

    /// Eine von Hand gezeichnete Grenze (I2): `ring` sind ihre Ecken im
    /// Grundriss (mm). Sie kommt als Flurstück zu den Grenzen des Geländes,
    /// in dessen System auf mm gerundet; ohne Gelände entsteht eins im
    /// örtlichen System wie bei [`Model::seed_flat_terrain`]. Das Gelände
    /// reicht danach bis über die Grenze ([`Model::grenze_bauen`], im
    /// offenen Schritt). `false` unter drei Ecken, ohne Fläche oder ohne
    /// Lage des Systems.
    pub fn add_parcel(&mut self, ring: &[[f64; 2]]) -> bool {
        if !super::gelaende_grenze::parcel_ring_ok(ring) {
            return false;
        }
        let mut t = self.terrain().cloned().unwrap_or_else(|| self.oertlich());
        let Some(f) = self.survey_frame_of(&t) else {
            return false;
        };
        let r: Vec<(f64, f64)> = ring
            .iter()
            .map(|p| {
                let (e, n) = f.to_survey(p[0], p[1]);
                (rund(e), rund(n))
            })
            .collect();
        t.parcels.push(r);
        self.grenze_bauen(t, ring, None)
    }

    /// Das Gelände `t` mit einer neuen oder geänderten Grenze `ring` (mm;
    /// mit `alt`, wo sie vorher lag) schreiben, im offenen Schritt; das
    /// Flurstück steht in `t` schon. Ein angenommenes Gelände reicht bis um
    /// den Plattenabstand über alle Grenzen (I6), und die Grenze hält es
    /// (Jörn 17.10.): Punkte an ihren Ecken und Viertelpunkten in der Höhe,
    /// die das Netz dort zeigt, ihr Ring als Bruchkante. Mit `alt` gehen
    /// vorher die Punkte der alten Linie samt Bruchkante und die Ecken um
    /// die Grenzen (U-264a); dann kommen sie, als wäre die Grenze gleich so
    /// gezeichnet (VOB B-264a). Alles an der Kopie, mit den Höhen aus dem
    /// Netz von vorher, und am Ende eine Änderung: Jede führt alle
    /// Gründungen nach (Review B-264e). Ein Aufmaß des Vermessers bleibt,
    /// wie es ist. Ohne Lageplan zeigt das Gelände die Umgebung außerhalb
    /// der Grenze ganz, bis man „blass“ oder „nicht“ wählt. `false`, wenn
    /// sich nichts ändert.
    pub(super) fn grenze_bauen(
        &mut self,
        mut t: Terrain,
        ring: &[[f64; 2]],
        alt: Option<&[[f64; 2]]>,
    ) -> bool {
        let gezeichnet = t.underlay.is_none() && t.show.is_none();
        if t.assumed() {
            let (Some(f), Some(h0)) = (self.survey_frame_of(&t), self.terrain_nhn()) else {
                return self.set_terrain(Some(t));
            };
            let z0 = self.hoehe_an_der_platte();
            let pts = grenz_punkte(ring);
            let mut hoehen: Vec<Option<f64>> = pts
                .iter()
                .map(|p| self.terrain_z_survey_with(h0, p[0], p[1]))
                .collect();
            let ringe: Vec<Vec<[f64; 2]>> = t
                .parcels
                .iter()
                .map(|r| r.iter().map(|&(e, n)| f.to_model(e, n)).collect())
                .collect();
            if let Some(alt) = alt {
                let gleich = |r: &Vec<[f64; 2]>| {
                    r.len() == ring.len() && r.iter().zip(ring).all(|(a, b)| nah(*a, *b))
                };
                let fremde: Vec<Vec<Kante>> = ringe
                    .iter()
                    .filter(|r| !gleich(r))
                    .map(|r| kanten(r, true))
                    .collect();
                abraeumen(&mut t, &f, alt, &fremde);
            }
            let margin = self.plate_settings().margin;
            abdecken(&mut t, &f, h0, &ringe, margin, z0);
            // eine neue Grenze außerhalb des Netzes von vorher: in der Höhe,
            // die das Netz nach dem Abdecken dort zeigt, so dass nichts
            // springt
            if alt.is_none() && hoehen.iter().any(Option::is_none) {
                let mut netz = t.clone();
                netz.netz_vergessen();
                for (z, p) in hoehen.iter_mut().zip(&pts).filter(|(z, _)| z.is_none()) {
                    let (e, n) = f.to_survey(p[0], p[1]);
                    *z = netz.height(e, n).map(|h| (h - h0) * 1000.0);
                }
            }
            let hoehen: Vec<f64> = hoehen.into_iter().map(|z| z.unwrap_or(z0)).collect();
            if pts.len() >= 3 {
                // fällt ein Schnitt doch auf eine belegte Stelle, halten
                // die Punkte der Grenze allein, ohne Bruchkante
                for schnitte in [true, false] {
                    let mut probe = t.clone();
                    let neu = anheften(&mut probe, &f, h0, &pts, &hoehen, schnitte);
                    if !doppelt_bei(&probe, &neu) {
                        t = probe;
                        break;
                    }
                }
            }
        } else if !t.points.is_empty() {
            return self.set_terrain(Some(t));
        }
        if gezeichnet && t.show() != Show::All {
            t.show = Some(Show::All);
        }
        // ±0,00 bleibt wie beim Setzen von Punkten
        self.punkte_schreiben(t, None)
    }

    /// OK Gelände (mm), wie das Netz es in der Mitte des Bezugsgebäudes
    /// zeigt, also auf seiner Platte; ohne Netz dort sein Versatz.
    fn hoehe_an_der_platte(&self) -> f64 {
        let b = self.level_building();
        let e = f64::INFINITY;
        let mut k = [e, e, -e, -e];
        for (_, r) in self.runs.iter() {
            if b.is_none() || self.building_of(r.storey) != b {
                continue;
            }
            for p in &r.points {
                k = [k[0].min(p.x), k[1].min(p.y), k[2].max(p.x), k[3].max(p.y)];
            }
        }
        (k[0] <= k[2])
            .then(|| self.terrain_z_survey((k[0] + k[2]) / 2.0, (k[1] + k[3]) / 2.0))
            .flatten()
            .unwrap_or_else(|| self.terrain_z_of(b))
    }

    /// Die Grenzen eines Geländes ohne Lageplan, also von Hand gezeichnet
    /// (I2), im Grundriss (mm).
    pub(super) fn gezeichnete_grenzen(&self) -> Vec<Vec<[f64; 2]>> {
        let Some(t) = self.terrain().filter(|t| t.underlay.is_none()) else {
            return Vec::new();
        };
        let Some(f) = self.survey_frame_of(t) else {
            return Vec::new();
        };
        t.parcels
            .iter()
            .map(|r| r.iter().map(|&(e, n)| f.to_model(e, n)).collect())
            .collect()
    }

    /// Ein neues Aufmaß ohne eigene Grenze übernimmt die gezeichneten
    /// `grenzen` (mm) dort, wo sie im Grundriss lagen, in seinem System;
    /// `t` hat seinen Platz schon. Sonst bliebe die Grenze, die man vor dem
    /// Vermessen gezeichnet hat, nicht stehen.
    pub(super) fn grenzen_mitnehmen(&self, t: &mut Terrain, grenzen: &[Vec<[f64; 2]>]) {
        if grenzen.is_empty() || !t.boundary_rings().is_empty() {
            return;
        }
        let Some(f) = self.survey_frame_of(t) else {
            return;
        };
        t.parcels = grenzen
            .iter()
            .map(|r| {
                r.iter()
                    .map(|p| {
                        let (e, n) = f.to_survey(p[0], p[1]);
                        (rund(e), rund(n))
                    })
                    .collect()
            })
            .collect();
    }

    /// Ein leeres Gelände im örtlichen System, Nullpunkt im Modell, Nord
    /// nach oben; mit fester Höhe in der Lage in NHN.
    fn oertlich(&self) -> Terrain {
        let mut t = Terrain::new(Crs::Local);
        t.local = Some(LocalFrame {
            e0: 0.0,
            n0: 0.0,
            rot: 0.0,
        });
        if self.location().nhn.is_some() {
            t.href = HeightRef::Abs;
        }
        t
    }
}

/// Liegt `p` in der konvexen Hülle `h` (gegen den Uhrzeigersinn), auf dem
/// Rand eingeschlossen (1 mm)?
fn in_huelle(h: &[Vec3], p: [f64; 2]) -> bool {
    let n = h.len();
    n >= 3
        && (0..n).all(|i| {
            let (a, b) = (h[i], h[(i + 1) % n]);
            let l = (b.x - a.x).hypot(b.y - a.y);
            (b.x - a.x) * (p[1] - a.y) - (b.y - a.y) * (p[0] - a.x) >= -l
        })
}

/// Der Rand der Hülle `h` als Ring von Punkten (Indizes in
/// [`Terrain::points`]): ihre Ecken und die Punkte, die auf ihren Kanten
/// liegen (bis 0,5 mm), damit die Bruchkante keinen Punkt überspringt.
fn huelle_ring(h: &[Vec3], spots: &[TerrainSpot]) -> Vec<usize> {
    let n = h.len();
    if n < 3 {
        return Vec::new();
    }
    let mut ring: Vec<usize> = Vec::new();
    let mut lage: Vec<[f64; 2]> = Vec::new();
    for k in 0..n {
        let (a, b) = (h[k], h[(k + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l2 = dx * dx + dy * dy;
        if l2 < 1e-12 {
            continue;
        }
        let mut auf: Vec<(f64, usize, [f64; 2])> = spots
            .iter()
            .filter_map(|s| {
                let (px, py) = (s.p[0] - a.x, s.p[1] - a.y);
                let t = (px * dx + py * dy) / l2;
                let quer = (px * dy - py * dx).abs() / l2.sqrt();
                ((0.0..1.0).contains(&t) && quer <= 0.5).then_some((t, s.i, [s.p[0], s.p[1]]))
            })
            .collect();
        auf.sort_by(|x, y| x.0.total_cmp(&y.0));
        for (_, i, p) in auf {
            // je Stelle ein Punkt
            if !lage.iter().any(|q| (q[0] - p[0]).hypot(q[1] - p[1]) <= 0.5) {
                ring.push(i);
                lage.push(p);
            }
        }
    }
    ring
}

/// Die Punkte der alten Grenze `alt` (mm) aus `t` lösen (U-264a): die mit
/// dem Code [`AN_DER_GRENZE`] an der alten Linie, außer an einer der
/// `fremde`n Grenzen, samt der geschlossenen Bruchkante, deren Punkte alle
/// an der alten Linie liegen; dazu die Ecken mit dem Code
/// [`UM_DIE_GRENZE`] samt den Bruchkanten durch sie. Aus anderen
/// Bruchkanten gehen nur die Schnittpunkte mit der alten Grenze. Auch wo
/// die neue Linie einen Punkt an derselben Stelle hat, geht er und kommt
/// mit [`anheften`] in derselben Höhe wieder: Sonst läge er beim Abdecken
/// im Rand des Netzes, und der Rand würde eine andere Bruchkante als bei
/// gleich so gezeichneter Grenze.
fn abraeumen(t: &mut Terrain, f: &SurveyFrame, alt: &[[f64; 2]], fremde: &[Vec<Kante>]) {
    let auf = |q: [f64; 2], k: &[Kante]| k.iter().any(|&(a, b)| abstand(a, b, q) <= 2.0 * FUGE);
    let alte = kanten(alt, true);
    let mut lage: HashMap<String, ([f64; 2], String)> = HashMap::new();
    for p in t.points.iter().filter(|p| p.hand && !p.off) {
        lage.insert(p.nr.to_string(), (f.to_model(p.e, p.n), p.code.to_string()));
    }
    let weg: HashSet<String> = lage
        .iter()
        .filter(|(_, (q, code))| {
            code == UM_DIE_GRENZE
                || code == AN_DER_GRENZE && auf(*q, &alte) && !fremde.iter().any(|k| auf(*q, k))
        })
        .map(|(n, _)| n.clone())
        .collect();
    if weg.is_empty() {
        return;
    }
    let code = |n: &String| lage.get(n).map(|l| l.1.as_str());
    let an_der_linie = |n: &String| lage.get(n).is_some_and(|l| auf(l.0, &alte));
    t.breaks.retain(|b| {
        let alte_grenze = b.closed
            && b.pts.iter().any(|n| code(n) == Some(AN_DER_GRENZE))
            && b.pts.iter().all(an_der_linie);
        let um = b
            .pts
            .iter()
            .any(|n| code(n) == Some(UM_DIE_GRENZE) && weg.contains(n));
        !alte_grenze && !um
    });
    for b in &mut t.breaks {
        b.pts.retain(|n| !weg.contains(n));
    }
    t.breaks
        .retain(|b| b.pts.len() >= if b.closed { 3 } else { 2 });
    let in_kante: HashSet<String> = t
        .breaks
        .iter()
        .flat_map(|b| b.pts.iter().cloned())
        .collect();
    t.points.retain(|p| {
        let n = p.nr.to_string();
        !weg.contains(&n) || in_kante.contains(&n)
    });
}

/// Die Ecken des Rechtecks um die Grenzen `ringe` (mm) im Abstand
/// `margin`, die das Netz von `t` noch nicht deckt, in Höhe `z` (mm) mit
/// dem Code [`UM_DIE_GRENZE`] (I6; Kosten H-251a: so wie bei Grenze vor
/// Fläche). Der bisherige Rand des Netzes wird Bruchkante: Innen bleibt
/// jedes Dreieck, wie es war, auch zwischen Häusern in verschiedener Höhe
/// (VOB B-251a).
fn abdecken(
    t: &mut Terrain,
    f: &SurveyFrame,
    h0: f64,
    ringe: &[Vec<[f64; 2]>],
    margin: f64,
    z: f64,
) {
    let e = f64::INFINITY;
    let mut k = [e, e, -e, -e];
    for p in ringe.iter().flatten() {
        k = [
            k[0].min(p[0]),
            k[1].min(p[1]),
            k[2].max(p[0]),
            k[3].max(p[1]),
        ];
    }
    if k[0] > k[2] {
        return;
    }
    let r = [k[0] - margin, k[1] - margin, k[2] + margin, k[3] + margin];
    // das Netz deckt die konvexe Hülle seiner Punkte
    let spots: Vec<TerrainSpot> = t
        .points
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.off)
        .map(|(i, p)| {
            let q = f.to_model(p.e, p.n);
            TerrainSpot {
                i,
                p: [q[0], q[1], (p.h - h0) * 1000.0],
                hand: p.hand,
                geaendert: p.gemessen.is_some(),
            }
        })
        .collect();
    let punkte: Vec<Vec3> = spots.iter().map(|s| vec3(s.p[0], s.p[1], 0.0)).collect();
    let huelle = crate::qto_site::convex_hull(&punkte);
    let neu: Vec<[f64; 3]> = [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]
        .into_iter()
        .filter(|p| !in_huelle(&huelle, *p))
        .map(|p| [p[0], p[1], z])
        .collect();
    if neu.is_empty() {
        return;
    }
    let rand: Vec<String> = huelle_ring(&huelle, &spots)
        .into_iter()
        .filter_map(|i| t.points.get(i).map(|p| p.nr.to_string()))
        .collect();
    punkte_dazu(t, f, h0, &neu, UM_DIE_GRENZE);
    if rand.len() < 3 {
        return;
    }
    // die Platte eines einzelnen Hauses ist schon eine
    let sortiert = |v: &[String]| {
        let mut v = v.to_vec();
        v.sort();
        v
    };
    let s = sortiert(&rand);
    if !t.breaks.iter().any(|b| b.closed && sortiert(&b.pts) == s) {
        t.breaks.push(TerrainLine {
            pts: rand,
            closed: true,
        });
    }
}

/// Die Grenze mit den Punkten `pts` (mm, [`grenz_punkte`]) in den Höhen
/// `hoehen` (mm) an `t` heften: neue Punkte mit dem Code
/// [`AN_DER_GRENZE`], wo noch keiner liegt; der Ring als Bruchkante. Kreuzt
/// sie eine andere Bruchkante, kommt mit `schnitte` an jeden Schnitt ein
/// Punkt, der in beiden liegt, in der Höhe jener Kante dort; Schnitte an
/// derselben Stelle (zwei Bruchkanten teilen dort eine Kante) bekommen
/// einen (VOB B-264d). Berührt sie eine nur, oder ohne `schnitte`, bleibt
/// sie ohne Bruchkante und hält mit ihren Punkten allein. Gibt die
/// Indizes der neuen Punkte.
fn anheften(
    t: &mut Terrain,
    f: &SurveyFrame,
    h0: f64,
    pts: &[[f64; 2]],
    hoehen: &[f64],
    schnitte_setzen: bool,
) -> Vec<usize> {
    let lage: Vec<(String, [f64; 2], f64)> = t
        .points
        .iter()
        .filter(|p| !p.off)
        .map(|p| (p.nr.to_string(), f.to_model(p.e, p.n), (p.h - h0) * 1000.0))
        .collect();
    let bei: HashMap<&str, ([f64; 2], f64)> =
        lage.iter().map(|q| (q.0.as_str(), (q.1, q.2))).collect();
    // die Kanten der übrigen Bruchkanten, je mit Bruchkante, Stelle und
    // den Höhen ihrer Enden
    let mut andere: Vec<(Kante, usize, usize, [f64; 2])> = Vec::new();
    for (j, b) in t.breaks.iter().enumerate() {
        let q: Vec<Option<([f64; 2], f64)>> =
            b.pts.iter().map(|n| bei.get(n.as_str()).copied()).collect();
        let n = q.len();
        let m = if b.closed && n >= 3 {
            n
        } else {
            n.saturating_sub(1)
        };
        for k in 0..m {
            if let (Some(a), Some(c)) = (q[k], q[(k + 1) % n]) {
                andere.push(((a.0, c.0), j, k, [a.1, c.1]));
            }
        }
    }
    let vorhanden: Vec<Option<String>> = pts
        .iter()
        .map(|p| lage.iter().find(|q| nah(q.1, *p)).map(|q| q.0.clone()))
        .collect();
    let mut schnitte: Vec<(usize, f64, usize, usize, f64, [f64; 3])> = Vec::new();
    let mut heikel = false;
    for (i, e) in kanten(pts, true).into_iter().enumerate() {
        for &(g, j, k, z) in &andere {
            match schnitt(e, g) {
                Schnitt::Keiner => {}
                Schnitt::Heikel => heikel = true,
                Schnitt::Echt(s, u, p) => {
                    heikel |= !schnitte_setzen
                        || lage.iter().any(|q| nah(q.1, p))
                        || pts.iter().any(|q| nah(*q, p));
                    schnitte.push((i, s, j, k, u, [p[0], p[1], z[0] + (z[1] - z[0]) * u]));
                }
            }
        }
    }
    drop(bei);
    let mut neu: Vec<[f64; 3]> = pts
        .iter()
        .zip(hoehen)
        .zip(&vorhanden)
        .filter(|(_, v)| v.is_none())
        .map(|((p, z), _)| [p[0], p[1], *z])
        .collect();
    let anker = neu.len();
    let mut einzeln: Vec<[f64; 3]> = Vec::new();
    let mut wo: Vec<usize> = Vec::with_capacity(schnitte.len());
    for s in &schnitte {
        let p = [s.5[0], s.5[1]];
        match einzeln.iter().position(|q| nah([q[0], q[1]], p)) {
            Some(i) => wo.push(i),
            None => {
                wo.push(einzeln.len());
                einzeln.push(s.5);
            }
        }
    }
    if !heikel {
        neu.extend(einzeln);
    }
    let idx = punkte_dazu(t, f, h0, &neu, AN_DER_GRENZE);
    if heikel {
        return idx;
    }
    let name = |i: usize| t.points[i].nr.to_string();
    let mut neue = idx[..anker].iter().map(|&i| name(i));
    let ecken: Vec<String> = vorhanden
        .into_iter()
        .map(|v| v.or_else(|| neue.next()).unwrap_or_default())
        .collect();
    let an_stellen: Vec<String> = idx[anker..].iter().map(|&i| name(i)).collect();
    let an_schnitten: Vec<&String> = wo.iter().map(|&i| &an_stellen[i]).collect();
    // die Grenze der Reihe nach, mit den Schnitten auf ihren Kanten
    let mut namen: Vec<String> = Vec::new();
    for (i, e) in ecken.into_iter().enumerate() {
        namen.push(e);
        let mut auf: Vec<(f64, &String)> = schnitte
            .iter()
            .zip(&an_schnitten)
            .filter(|(s, _)| s.0 == i)
            .map(|(s, n)| (s.1, *n))
            .collect();
        auf.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, n) in auf {
            if namen.last() != Some(n) {
                namen.push(n.clone());
            }
        }
    }
    // in den gekreuzten Bruchkanten von hinten nach vorn einfügen
    let mut rein: Vec<(usize, usize, f64, &String)> = schnitte
        .iter()
        .zip(&an_schnitten)
        .map(|(s, n)| (s.2, s.3, s.4, *n))
        .collect();
    rein.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)).then(b.2.total_cmp(&a.2)));
    for (j, k, _, n) in rein {
        if let Some(b) = t.breaks.get_mut(j).filter(|b| !b.pts.contains(n)) {
            b.pts.insert(k + 1, n.clone());
        }
    }
    t.breaks.push(TerrainLine {
        pts: namen,
        closed: true,
    });
    idx
}

/// Teilung jeder Grenzkante für die Punkte an der Grenze (Jörn 17.10.:
/// Drittel- oder Viertelpunkte).
const GRENZ_TEILE: usize = 4;
/// Kürzer (mm) als dieses Stück je Teil bleibt eine Grenzkante ungeteilt:
/// Die Punkte lägen sonst dicht an dicht.
const GRENZ_TEIL_MIN: f64 = 500.0;

/// Die Ecken eines Grenzrings (mm) und die Viertelpunkte seiner Kanten, der
/// Reihe nach.
fn grenz_punkte(ring: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let n = ring.len();
    let mut out = Vec::with_capacity(n * GRENZ_TEILE);
    for k in 0..n {
        let (a, b) = (ring[k], ring[(k + 1) % n]);
        out.push(a);
        if (b[0] - a[0]).hypot(b[1] - a[1]) / GRENZ_TEILE as f64 >= GRENZ_TEIL_MIN {
            for i in 1..GRENZ_TEILE {
                let s = i as f64 / GRENZ_TEILE as f64;
                out.push([a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s]);
            }
        }
    }
    out
}

/// Liegen zwei Punkte (mm) an derselben Stelle, so nah, dass die Prüfung
/// des Netzes sie zusammennähme?
fn nah(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) <= 2.0 * FUGE
}

/// Abstand (mm) des Punkts `p` von der Strecke `a`–`b`.
fn abstand(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let l2 = d[0] * d[0] + d[1] * d[1];
    let s = if l2 > 0.0 {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - d[0] * s).hypot(p[1] - a[1] - d[1] * s)
}

/// Die Kanten eines Linienzugs, mit `zu` geschlossen.
fn kanten(r: &[[f64; 2]], zu: bool) -> Vec<([f64; 2], [f64; 2])> {
    let n = r.len();
    let m = if zu && n >= 3 { n } else { n.saturating_sub(1) };
    (0..m).map(|k| (r[k], r[(k + 1) % n])).collect()
}

/// Eine Strecke im Grundriss (mm), von–bis.
type Kante = ([f64; 2], [f64; 2]);

/// Wie sich zwei Strecken treffen.
enum Schnitt {
    Keiner,
    /// Sie kreuzen sich innen: Anteil auf der ersten, auf der zweiten und
    /// der Punkt.
    Echt(f64, f64, [f64; 2]),
    /// Sie berühren sich an einem Ende oder liegen aufeinander.
    Heikel,
}

/// Schnitt der Strecken `e` und `g` (mm); näher als [`nah`] an einem Ende
/// gilt als Berühren.
fn schnitt(e: ([f64; 2], [f64; 2]), g: ([f64; 2], [f64; 2])) -> Schnitt {
    let r = [e.1[0] - e.0[0], e.1[1] - e.0[1]];
    let q = [g.1[0] - g.0[0], g.1[1] - g.0[1]];
    let x = |a: [f64; 2], b: [f64; 2]| a[0] * b[1] - a[1] * b[0];
    let d = x(r, q);
    let w = [g.0[0] - e.0[0], g.0[1] - e.0[1]];
    let (lr, lq) = (r[0].hypot(r[1]), q[0].hypot(q[1]));
    if lr < 1e-9 || lq < 1e-9 {
        return Schnitt::Keiner;
    }
    if d.abs() < 1e-12 * lr * lq {
        // gleich gerichtet: auf einer Geraden und überlappend ist heikel
        if x(w, r).abs() > 1e-9 * lr * lr {
            return Schnitt::Keiner;
        }
        let a = (w[0] * r[0] + w[1] * r[1]) / (lr * lr);
        let b = a + (q[0] * r[0] + q[1] * r[1]) / (lr * lr);
        let (lo, hi) = (a.min(b), a.max(b));
        return if hi < 0.0 || lo > 1.0 {
            Schnitt::Keiner
        } else {
            Schnitt::Heikel
        };
    }
    let s = x(w, q) / d;
    let u = x(w, r) / d;
    let (rs, ru) = (2.0 * FUGE / lr, 2.0 * FUGE / lq);
    if s < -rs || s > 1.0 + rs || u < -ru || u > 1.0 + ru {
        return Schnitt::Keiner;
    }
    if s <= rs || s >= 1.0 - rs || u <= ru || u >= 1.0 - ru {
        return Schnitt::Heikel;
    }
    Schnitt::Echt(s, u, [e.0[0] + r[0] * s, e.0[1] + r[1] * s])
}

/// Auf mm.
pub(super) fn rund(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Inhalt eines Umrisses (mm²).
pub(super) fn flaeche(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    let s: f64 = (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    s.abs() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    /// Haus 10 × 8 m, OK Gelände 50 cm unter ±0,00.
    fn haus() -> Model {
        let mut m = Model::with_seed(3);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        m.begin("Gelände");
        assert!(m.set_terrain_offset_of(b, 500.0));
        m.commit();
        m
    }

    /// Die Ecken der Platte in ihrer Höhe, von Hand; das Gelände liegt dort,
    /// wo die Platte lag, und ±0,00 bleibt. Ein zweites Mal ändert nichts.
    #[test]
    fn ecken_der_platte() {
        let mut m = haus();
        let z = m.terrain_z();
        let rand = m.plate_settings().margin;
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        assert!(!m.seed_flat_terrain(&[]), "schon Punkte");
        m.commit();
        let t = m.terrain().unwrap();
        assert!(t.assumed());
        assert_eq!(t.crs, Crs::Local);
        assert_eq!(t.points.len(), 4);
        assert_eq!(t.points[0].nr, "H1");
        assert!(t.points.iter().all(|p| p.h == -0.5), "{:?}", t.points);
        let e: Vec<f64> = t.points.iter().map(|p| p.e).collect();
        assert_eq!(
            e,
            [
                -rand / 1000.0,
                10.0 + rand / 1000.0,
                10.0 + rand / 1000.0,
                -rand / 1000.0
            ]
        );
        assert_eq!(m.terrain_nhn(), Some(0.0));
        for (x, y) in [(5000.0, 4000.0), (-1000.0, -1000.0), (11000.0, 9000.0)] {
            assert!((m.terrain_z_at(None, x, y) - z).abs() < 1e-6, "{x} {y}");
        }
        let mut m = haus();
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.rollback();
        assert!(m.terrain().is_none(), "im Schritt");
    }

    /// Die Erdmengen der Gründung bleiben, wenn das Gelände angelegt wird:
    /// gleiche Werte über das Netz wie über die Ebene; der Ansatz sagt, dass
    /// das Gelände angenommen ist.
    #[test]
    fn mengen_bleiben() {
        let mut m = haus();
        let vorher = crate::qto::schedule(&m).auto;
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let nachher = crate::qto::schedule(&m).auto;
        let erde = |v: &[crate::qto::AutoMenge]| -> Vec<(String, &str, f64)> {
            let mut e: Vec<(String, &str, f64)> = v
                .iter()
                .filter(|a| a.key.starts_with("earth."))
                .map(|a| (a.key.to_string(), a.unit, a.value))
                .collect();
            e.sort_by(|a, b| a.0.cmp(&b.0));
            e
        };
        let (a, b) = (erde(&vorher), erde(&nachher));
        assert!(!a.is_empty());
        assert_eq!(a.len(), b.len(), "{a:?}\n{b:?}");
        for (x, y) in a.iter().zip(&b) {
            assert_eq!((&x.0, x.1), (&y.0, y.1));
            // je Einheit (VOB-Hinweis c zu #229): unter 1 dm³ bzw. 1 dm²
            let tol = match x.1 {
                "m3" => 1e6,
                "m2" => 1e4,
                "m" => 1e2,
                _ => 1e-3,
            };
            assert!((x.2 - y.2).abs() < tol, "{} {} {} {}", x.0, x.1, x.2, y.2);
        }
        let netz: Vec<&crate::qto::AutoMenge> = nachher
            .iter()
            .filter(|a| a.formula.contains(crate::qto_earth::DREIECKSPRISMEN))
            .collect();
        assert!(!netz.is_empty());
        assert!(netz
            .iter()
            .all(|a| a.formula.contains(crate::qto_earth::ANGENOMMEN)));
        assert_eq!(crate::qto_earth::netz_herkunft(&m), None);
        assert_eq!(crate::qto_earth::netz_angenommen(&m).as_deref(), Some("2"));
    }

    /// Langes Haus und Schuppen 1 m tiefer, die Platten 6 m auseinander:
    /// Jede Platte bleibt in ihrer Höhe, auch wo die Ecke des anderen
    /// Hauses im Umkreis ihrer Diagonale liegt (ohne Bruchkanten bis
    /// 0,35 m im Haus), und die Erdmengen bleiben (Geometrie, Routine
    /// 16.10. 19:03).
    #[test]
    fn zwei_haeuser_springen_nicht() {
        let mut m = Model::with_seed(3);
        let neu = |m: &mut Model, x0: f64, y0: f64, w: f64, h: f64, versatz: f64| {
            let b = m.add_building(1);
            let pts = [
                vec3(x0, y0, 0.0),
                vec3(x0 + w, y0, 0.0),
                vec3(x0 + w, y0 + h, 0.0),
                vec3(x0, y0 + h, 0.0),
            ];
            m.build_from_polygon(b, &pts).unwrap();
            m.begin("Gelände");
            assert!(m.set_terrain_offset_of(b, versatz));
            m.commit();
            b
        };
        let haus = neu(&mut m, 0.0, 0.0, 30000.0, 6000.0, 500.0);
        let schuppen = neu(&mut m, 13000.0, 18000.0, 4000.0, 3000.0, 1500.0);
        let platten = m.terrain_patches(m.plate_settings().margin);
        let vorher = crate::qto::schedule(&m).auto;
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        assert_eq!(m.terrain().unwrap().breaks.len(), 2);
        for (r, z) in &platten {
            let b = if *z == m.terrain_z_of(Some(haus)) {
                haus
            } else {
                schuppen
            };
            for i in 0..=20 {
                for j in 0..=20 {
                    let x = r[0] + (r[2] - r[0]) * i as f64 / 20.0;
                    let y = r[1] + (r[3] - r[1]) * j as f64 / 20.0;
                    let d = m.terrain_z_at(Some(b), x, y) - z;
                    assert!(d.abs() < 1e-6, "{x} {y}: {d}");
                }
            }
        }
        let nachher = crate::qto::schedule(&m).auto;
        let erde = |v: &[crate::qto::AutoMenge]| -> Vec<(String, f64)> {
            let mut e: Vec<(String, f64)> = v
                .iter()
                .filter(|a| a.key.starts_with("earth.") && a.unit == "m3")
                .map(|a| (a.key.to_string(), a.value))
                .collect();
            e.sort_by(|a, b| a.0.cmp(&b.0));
            e
        };
        let (a, b) = (erde(&vorher), erde(&nachher));
        assert!(!a.is_empty());
        assert_eq!(a.len(), b.len(), "{a:?}\n{b:?}");
        for (x, y) in a.iter().zip(&b) {
            assert!((x.1 - y.1).abs() < 1e6, "{} {} {}", x.0, x.1, y.1);
        }
    }

    /// Doppelhaus, Garage am Haus: Ein Haus 8 × 10 m in 0,50 m und ein
    /// zweites (oder eine Garage 3 × 3 m) in 1,50 m unter ±0,00 näher als
    /// zwei Plattenabstände. Die Platten werden in der Mitte zwischen den
    /// Sohlen getrennt (H-243a, VOB; Geometrie zu #245). Im Umriss jedes
    /// Hauses gilt seine Höhe, auch beim späteren, und liegen die Sohlen
    /// samt Überstand nicht übereinander, auch auf der ganzen Sohle; dann
    /// bleiben auch die Erdmengen beim Anlegen der ersten Fläche, wie sie
    /// waren (Review zu #245). Hinter einer kürzeren Garage gilt das Haus
    /// (VOB-Spruch c zu H-245a). Wand an Wand bleibt unter der Wand ein
    /// Streifen von 2 × FUGE.
    #[test]
    fn doppelhaus_und_garage() {
        // (Abstand der Wände, quer, Garage)
        let faelle = [
            (0.0, false, false),
            (630.0, false, false),
            (650.0, false, false),
            (800.0, false, false),
            (1000.0, false, false),
            (1500.0, false, false),
            (3000.0, false, false),
            (6000.0, false, false),
            (12000.0, false, false),
            (0.0, true, false),
            (630.0, true, false),
            (650.0, true, false),
            (2000.0, true, false),
            (630.0, false, true),
            (650.0, false, true),
            (1000.0, false, true),
            (2000.0, false, true),
        ];
        let erde = |m: &Model| -> Vec<(String, f64)> {
            let mut e: Vec<(String, f64)> = crate::qto::schedule(m)
                .auto
                .iter()
                .filter(|a| a.key.starts_with("earth.") && a.unit == "m3")
                .map(|a| (format!("{:?} {}", a.building, a.key), a.value))
                .collect();
            e.sort_by(|a, b| a.0.cmp(&b.0));
            e
        };
        for (spalt, quer, garage) in faelle {
            let fall = format!("{spalt} quer {quer} Garage {garage}");
            let mut m = Model::with_seed(5);
            let mut neu = |x0: f64, y0: f64, w: f64, h: f64, versatz: f64| {
                let b = m.add_building(1);
                let pts = [
                    vec3(x0, y0, 0.0),
                    vec3(x0 + w, y0, 0.0),
                    vec3(x0 + w, y0 + h, 0.0),
                    vec3(x0, y0 + h, 0.0),
                ];
                m.build_from_polygon(b, &pts).unwrap();
                m.begin("Gelände");
                assert!(m.set_terrain_offset_of(b, versatz));
                m.commit();
                (b, [x0, y0, x0 + w, y0 + h])
            };
            let a = neu(0.0, 0.0, 8000.0, 10000.0, 500.0);
            let (w, h) = if garage {
                (3000.0, 3000.0)
            } else {
                (8000.0, 10000.0)
            };
            let b = if quer {
                neu(3000.0, 10000.0 + spalt, w, h, 1500.0)
            } else {
                neu(8000.0 + spalt, 0.0, w, h, 1500.0)
            };
            let vorher = erde(&m);
            m.begin("Fläche");
            assert!(m.seed_flat_terrain(&[]));
            m.commit();
            let t = m.terrain().unwrap();
            // Wände ineinander: wie main gilt die frühere Platte, die
            // spätere bleibt ohne Rand (Hinweis „ineinander“)
            let ineinander = spalt < 2.0 * 315.0;
            let soll = if ineinander { 1 } else { 2 };
            assert_eq!(t.breaks.len(), soll, "{fall}");
            assert!(
                t.mesh().hinweise.is_empty(),
                "{fall}: {:?}",
                t.mesh().hinweise
            );
            // Mitte zwischen den Häusern, in x oder y
            let (d, mitte) = if quer {
                (1, 10000.0 + spalt / 2.0)
            } else {
                (0, 8000.0 + spalt / 2.0)
            };
            // Sohle samt Überstand, wenn sie nicht über der anderen liegt
            let u = if spalt >= 2.0 * 315.0 + 2.0 * FUGE {
                315.0
            } else {
                0.0
            };
            let haeuser = if ineinander { vec![] } else { vec![a, b] };
            for (haus, r) in haeuser {
                let z = m.terrain_z_of(Some(haus));
                for i in 0..=20 {
                    for j in 0..=20 {
                        let x = r[0] - u + (r[2] - r[0] + 2.0 * u) * i as f64 / 20.0;
                        let y = r[1] - u + (r[3] - r[1] + 2.0 * u) * j as f64 / 20.0;
                        if ([x, y][d] - mitte).abs() <= FUGE {
                            continue;
                        }
                        let dz = m.terrain_z_at(Some(haus), x, y) - z;
                        assert!(dz.abs() < 1e-6, "{fall}: {x} {y}: {dz}");
                    }
                }
            }
            // ab Wand an Wand; dort rechnet der Graben unter der
            // Haustrennwand mit der eigenen Höhe (VOB-Spruch zu H-245c)
            if spalt >= 2.0 * 315.0 {
                let nachher = erde(&m);
                assert_eq!(vorher.len(), nachher.len(), "{fall}");
                for (x, y) in vorher.iter().zip(&nachher) {
                    assert_eq!(x.0, y.0, "{fall}");
                    assert!((x.1 - y.1).abs() < 1e6, "{fall}: {} {} → {}", x.0, x.1, y.1);
                }
            }
        }
    }

    /// Häuser ineinander (H-245-geo-a): Erst wenn sich die Wände
    /// überdecken (Bezugslinien näher als zwei Wanddicken), meldet das
    /// Modell das Paar, und die Mengenliste nennt beide Häuser in einem
    /// Projekthinweis (VOB). Wand an Wand (0,63 m) und mit Spalt nicht.
    #[test]
    fn haeuser_ineinander_mit_hinweis() {
        // (Abstand der Bezugslinien, Garage, ineinander)
        let faelle = [
            (0.0, false, true),
            (400.0, false, true),
            (0.0, true, true),
            (630.0, false, false),
            (650.0, false, false),
            (2000.0, true, false),
        ];
        for (spalt, garage, soll) in faelle {
            let mut m = Model::with_seed(5);
            let mut neu = |x0: f64, w: f64, h: f64| {
                let b = m.add_building(1);
                let pts = [
                    vec3(x0, 0.0, 0.0),
                    vec3(x0 + w, 0.0, 0.0),
                    vec3(x0 + w, h, 0.0),
                    vec3(x0, h, 0.0),
                ];
                (b, m.build_from_polygon(b, &pts).unwrap())
            };
            let (a, _) = neu(0.0, 8000.0, 10000.0);
            let (w, h) = if garage {
                (3000.0, 3000.0)
            } else {
                (8000.0, 10000.0)
            };
            let (b, zug) = neu(8000.0 + spalt, w, h);
            let fall = format!("{spalt} Garage {garage}");
            let paare = m.haeuser_ineinander();
            if soll {
                assert_eq!(paare.len(), 1, "{fall}");
                let p = paare[0];
                assert!(p == (a, b) || p == (b, a), "{fall}");
            } else {
                assert!(paare.is_empty(), "{fall}");
            }
            let s = crate::qto::schedule(&m);
            let h: Vec<&String> = s
                .hinweise
                .iter()
                .filter(|h| h.contains(crate::qto_earth::INEINANDER))
                .collect();
            if soll {
                assert_eq!(h.len(), 1, "{fall}: {:?}", s.hinweise);
                let stelle = |haus| {
                    let x = m.building(haus).unwrap();
                    (
                        h[0].find(&format!("{} ({})", x.name, x.number)),
                        x.number.clone(),
                    )
                };
                let ((sa, na), (sb, nb)) = (stelle(a), stelle(b));
                assert!(sa.is_some() && sb.is_some(), "{h:?}");
                // nach Nummer gelesen, nicht nach Guid (Kosten K-248a)
                assert_eq!(sa < sb, na < nb, "{h:?}");
            } else {
                assert!(h.is_empty(), "{fall}: {h:?}");
            }
            // gemerkt mit der Lage: weggerückt ist das Paar fort
            if soll {
                let weg: Vec<_> = m
                    .run(zug)
                    .unwrap()
                    .points
                    .iter()
                    .map(|p| vec3(p.x + 5000.0, p.y, p.z))
                    .collect();
                m.begin("Weg");
                assert!(m.set_run_points(zug, &weg).is_some());
                m.commit();
                assert!(m.haeuser_ineinander().is_empty(), "{fall}");
            }
        }
    }

    /// Im Umfang eines Hauses nennt die Mengenliste nur dessen Paar
    /// ineinander (VOB V-254a): GB-03 weit weg ohne den Hinweis zu GB-01
    /// und GB-02, das Projekt und beide Häuser des Paars mit.
    #[test]
    fn ineinander_nur_beim_paar() {
        let mut m = Model::with_seed(5);
        let mut haeuser = Vec::new();
        for x0 in [0.0, 7800.0, 60000.0] {
            let b = m.add_building(1);
            let pts = [
                vec3(x0, 0.0, 0.0),
                vec3(x0 + 8000.0, 0.0, 0.0),
                vec3(x0 + 8000.0, 10000.0, 0.0),
                vec3(x0, 10000.0, 0.0),
            ];
            m.build_from_polygon(b, &pts).unwrap();
            haeuser.push(b);
        }
        let s = crate::qto::schedule(&m);
        assert_eq!(s.ineinander.len(), 1, "{:?}", s.hinweise);
        assert_eq!(s.ineinander[0].haeuser, (haeuser[0], haeuser[1]));
        let satz = s.ineinander[0].satz.clone();
        let hat = |s: &crate::qto::Schedule| (s.hinweise.contains(&satz), s.ineinander.len());
        assert_eq!(hat(&s), (true, 1));
        for (b, soll) in haeuser.iter().zip([true, true, false]) {
            let r = s.restrict(&m, &crate::qto::Umfang::gebaeude(*b));
            assert_eq!(hat(&r), (soll, usize::from(soll)), "{:?}", r.hinweise);
        }
        // in einer Kette nach Nummer, nicht nach Guid (VOB H-256a)
        for seed in 1..=8 {
            let mut m = Model::with_seed(seed);
            let mut kette = Vec::new();
            for i in 0..4 {
                let b = m.add_building(1);
                let x0 = f64::from(i) * 7800.0;
                let pts = [
                    vec3(x0, 0.0, 0.0),
                    vec3(x0 + 8000.0, 0.0, 0.0),
                    vec3(x0 + 8000.0, 10000.0, 0.0),
                    vec3(x0, 10000.0, 0.0),
                ];
                m.build_from_polygon(b, &pts).unwrap();
                kette.push(b);
            }
            let s = crate::qto::schedule(&m);
            let paare: Vec<_> = s.ineinander.iter().map(|x| x.haeuser).collect();
            let soll: Vec<_> = kette.windows(2).map(|w| (w[0], w[1])).collect();
            assert_eq!(paare, soll, "Seed {seed}");
        }
    }

    /// Der Hinweis gilt genau dort, wo die Trennung der Platten endet
    /// (Geometrie zu #248): Rechnen die Gründungen beim Anlegen der ersten
    /// Fläche gemeinsam, weil sich die Wände überdecken, meldet das Modell
    /// das Paar; trennt es die Platten, nicht. Auch 2 bis 40 mm Überdeckung.
    #[test]
    fn ineinander_wo_die_trennung_endet() {
        for spalt in [
            500.0, 590.0, 600.0, 620.0, 628.0, 628.6, 629.4, 630.0, 650.0,
        ] {
            let mut m = Model::with_seed(5);
            let mut gr = Vec::new();
            for (x0, v) in [(0.0, 500.0), (8000.0 + spalt, 1500.0)] {
                let b = m.add_building(1);
                let pts = [
                    vec3(x0, 0.0, 0.0),
                    vec3(x0 + 8000.0, 0.0, 0.0),
                    vec3(x0 + 8000.0, 10000.0, 0.0),
                    vec3(x0, 10000.0, 0.0),
                ];
                let zug = m.build_from_polygon(b, &pts).unwrap();
                m.begin("Versatz");
                m.set_terrain_offset_of(b, v);
                m.commit();
                let st = m.run(zug).unwrap().storey;
                gr.push(m.foundation_level_of(st).unwrap());
            }
            let vor: Vec<f64> = gr.iter().map(|g| m.terrain_z_of_level(*g)).collect();
            let hinweis = !m.haeuser_ineinander().is_empty();
            m.begin("Fläche");
            assert!(m.seed_flat_terrain(&[]));
            m.commit();
            let gemeinsam = gr
                .iter()
                .zip(&vor)
                .any(|(g, z)| (m.terrain_z_of_level(*g) - z).abs() > 1e-6);
            assert_eq!(hinweis, gemeinsam, "Spalt {spalt}");
            assert_eq!(hinweis, spalt < 629.0, "Spalt {spalt}");
        }
    }

    /// Wand an Wand, 1 m Höhenunterschied: Die Gründungen bleiben beim
    /// Anlegen der ersten Fläche, wo sie waren. Die Einbindetiefe zählt nur
    /// am freien Plattenrand, nicht an der Haustrennwand, wo das Gelände
    /// des Nachbarn liegt (H-245a, VOB; vorher rutschte die Gründung des
    /// höheren Hauses 1 m tiefer). Ebenso mit einer Fuge von 1 bis 19 mm
    /// zwischen den Außenflächen (VOB B-245b: bis 0,45 m tiefer), längs und
    /// quer, mit Haus und mit Garage.
    #[test]
    fn trennwand_zieht_nicht_hinab() {
        for spalt in [630.0, 631.0, 635.0, 640.0, 645.0, 649.0, 650.0] {
            for (quer, garage) in [(false, false), (true, false), (false, true), (true, true)] {
                let fall = format!("{spalt} quer {quer} Garage {garage}");
                let mut m = Model::with_seed(5);
                let (w, h) = if garage {
                    (3000.0, 3000.0)
                } else {
                    (8000.0, 10000.0)
                };
                let zweites = if quer {
                    (0.0, 10000.0 + spalt, w, h)
                } else {
                    (8000.0 + spalt, 0.0, w, h)
                };
                for ((x0, y0, w, h), versatz) in
                    [((0.0, 0.0, 8000.0, 10000.0), 500.0), (zweites, 1500.0)]
                {
                    let b = m.add_building(1);
                    let pts = [
                        vec3(x0, y0, 0.0),
                        vec3(x0 + w, y0, 0.0),
                        vec3(x0 + w, y0 + h, 0.0),
                        vec3(x0, y0 + h, 0.0),
                    ];
                    m.build_from_polygon(b, &pts).unwrap();
                    m.begin("Gelände");
                    assert!(m.set_terrain_offset_of(b, versatz));
                    m.commit();
                }
                let gruendungen = |m: &Model| -> Vec<(StoreyId, f64, f64)> {
                    m.storeys
                        .iter()
                        .filter(|(_, s)| s.kind == LevelKind::Foundation)
                        .map(|(id, s)| (id, s.elevation, m.terrain_z_of_level(id)))
                        .collect()
                };
                let vorher = gruendungen(&m);
                assert_eq!(vorher.len(), 2);
                m.begin("Fläche");
                assert!(m.seed_flat_terrain(&[]));
                m.commit();
                let nachher = gruendungen(&m);
                for ((g, uk0, z0), (h, uk1, z1)) in vorher.iter().zip(&nachher) {
                    assert_eq!(g, h);
                    assert!((uk0 - uk1).abs() < 1e-6, "{fall}: UK {uk0} → {uk1}");
                    assert!((z0 - z1).abs() < 1e-6, "{fall}: Gelände {z0} → {z1}");
                }
                let f = m.terrain_folgen().unwrap();
                assert!(f.frostfrei, "{fall}");
            }
        }
    }

    /// Gedrehte Häuser, deren Rechtecke sich überdecken (Geometrie
    /// H-245-geo-c): wie auf main behält die frühere Platte ihren Rand; von
    /// der späteren bleiben ohne Rand die Ecken außerhalb der früheren. Ein
    /// Haus weit weg behält seinen Rand.
    #[test]
    fn gedreht_wie_vorher() {
        for (grad, spalt) in [
            (30.0_f64, 3000.0),
            (45.0, 3000.0),
            (10.0, 630.0),
            (45.0, 630.0),
        ] {
            let mut m = Model::with_seed(5);
            let (s, c) = grad.to_radians().sin_cos();
            let dreh = |x: f64, y: f64| vec3(x * c - y * s, x * s + y * c, 0.0);
            for (x0, versatz) in [(0.0, 500.0), (8000.0 + spalt, 1500.0)] {
                let b = m.add_building(1);
                let pts = [
                    dreh(x0, 0.0),
                    dreh(x0 + 8000.0, 0.0),
                    dreh(x0 + 8000.0, 10000.0),
                    dreh(x0, 10000.0),
                ];
                m.build_from_polygon(b, &pts).unwrap();
                m.begin("Gelände");
                assert!(m.set_terrain_offset_of(b, versatz));
                m.commit();
            }
            let b = m.add_building(1);
            let weit = [
                vec3(90000.0, 0.0, 0.0),
                vec3(98000.0, 0.0, 0.0),
                vec3(98000.0, 10000.0, 0.0),
                vec3(90000.0, 10000.0, 0.0),
            ];
            m.build_from_polygon(b, &weit).unwrap();
            m.begin("Fläche");
            assert!(m.seed_flat_terrain(&[]));
            m.commit();
            let t = m.terrain().unwrap();
            assert!(t.points.len() < 12, "{grad}° {spalt}");
            assert_eq!(t.breaks.len(), 2, "{grad}° {spalt}");
            assert!(t.breaks.iter().all(|b| b.pts.len() == 4), "{grad}° {spalt}");
        }
    }

    /// Gedrehter Nachbar, seine Ecke 10 bis 20 mm vor der Hauswand (VOB
    /// H-245e): Der Rand des Hauses neben der schrägen Wand liegt im
    /// Fugenstreifen zwischen den Rechtecken und zählt nicht als frei. OK
    /// Gelände und UK Gründung beider Häuser bleiben, und die Erdmengen
    /// des Hauses beim Anlegen der ersten Fläche auch (vorher bis 0,50 m
    /// tiefer, Graben 6,50 → 12,76 m³).
    #[test]
    fn gedrehter_nachbar_knapp() {
        for (grad, spalt) in [(2.0_f64, 640.0), (2.0, 650.0)] {
            let fall = format!("{grad}° {spalt}");
            let mut m = Model::with_seed(5);
            let (s, c) = grad.to_radians().sin_cos();
            let x0 = 8000.0 + spalt;
            let nachbar: Vec<Vec3> = [[0.0, 0.0], [6000.0, 0.0], [6000.0, 8000.0], [0.0, 8000.0]]
                .iter()
                .map(|p| {
                    vec3(
                        x0 + p[0] * c - p[1] * s + 8000.0 * s,
                        p[0] * s + p[1] * c,
                        0.0,
                    )
                })
                .collect();
            let haus = [
                vec3(0.0, 0.0, 0.0),
                vec3(8000.0, 0.0, 0.0),
                vec3(8000.0, 10000.0, 0.0),
                vec3(0.0, 10000.0, 0.0),
            ];
            let mut gr = Vec::new();
            for (pts, versatz) in [(&haus[..], 500.0), (&nachbar[..], 1500.0)] {
                let b = m.add_building(1);
                let run = m.build_from_polygon(b, pts).unwrap();
                m.begin("Gelände");
                assert!(m.set_terrain_offset_of(b, versatz));
                m.commit();
                gr.push(m.foundation_level_of(m.run(run).unwrap().storey).unwrap());
            }
            let stand = |m: &Model| -> Vec<(f64, f64)> {
                gr.iter()
                    .map(|g| (m.terrain_z_of_level(*g), m.storey(*g).unwrap().elevation))
                    .collect()
            };
            let erde = |m: &Model| -> Vec<(String, f64)> {
                crate::qto::schedule(m)
                    .auto
                    .iter()
                    .filter(|a| a.key.starts_with("earth.") && a.unit == "m3")
                    .map(|a| (format!("{:?} {}", a.building, a.key), a.value))
                    .collect()
            };
            let (h0, e0) = (stand(&m), erde(&m));
            m.begin("Fläche");
            assert!(m.seed_flat_terrain(&[]));
            m.commit();
            for (a, b) in h0.iter().zip(stand(&m)) {
                assert!((a.0 - b.0).abs() < 1e-6, "{fall}: OK {} → {}", a.0, b.0);
                assert!((a.1 - b.1).abs() < 1e-6, "{fall}: UK {} → {}", a.1, b.1);
            }
            let e1 = erde(&m);
            assert_eq!(e0.len(), e1.len(), "{fall}");
            for (x, y) in e0.iter().zip(&e1) {
                assert!((x.1 - y.1).abs() < 1e6, "{fall}: {} {} → {}", x.0, x.1, y.1);
            }
        }
    }

    /// Haus und Garage in jedem Abstand der Wandzüge von 0,65 bis 3 m (in
    /// cm, jeder siebte um 0,4 mm daneben), längs und quer: keine Ecke
    /// fällt bei der Prüfung heraus, und OK Gelände an beiden Gründungen
    /// bleibt (Geometrie zu #245; mit FUGE 5 mm fast überall 0,76 m tiefer).
    /// Darunter berühren sich die Platten selbst (Wand 0,315 m außen).
    #[test]
    fn fuge_in_jedem_abstand() {
        for quer in [false, true] {
            for k in 65..=300 {
                let spalt = k as f64 * 10.0 + if k % 7 == 3 { 0.4 } else { 0.0 };
                let mut m = Model::with_seed(5);
                let mut gr = Vec::new();
                let zweites = if quer {
                    (3000.0, 10000.0 + spalt, 1500.0)
                } else {
                    (8000.0 + spalt, 0.0, 1500.0)
                };
                for (x0, y0, versatz) in [(0.0, 0.0, 500.0), zweites] {
                    let b = m.add_building(1);
                    let pts = [
                        vec3(x0, y0, 0.0),
                        vec3(x0 + 8000.0, y0, 0.0),
                        vec3(x0 + 8000.0, y0 + 10000.0, 0.0),
                        vec3(x0, y0 + 10000.0, 0.0),
                    ];
                    let run = m.build_from_polygon(b, &pts).unwrap();
                    m.begin("Gelände");
                    assert!(m.set_terrain_offset_of(b, versatz));
                    m.commit();
                    let st = m.run(run).unwrap().storey;
                    gr.push(m.foundation_level_of(st).unwrap());
                }
                m.begin("Fläche");
                assert!(m.seed_flat_terrain(&[]));
                m.commit();
                let t = m.terrain().unwrap();
                assert!(
                    t.mesh().hinweise.is_empty(),
                    "{spalt} {quer}: {:?}",
                    t.mesh().hinweise
                );
                assert_eq!(t.breaks.len(), 2, "{spalt} {quer}");
                for (g, z) in gr.iter().zip([-500.0, -1500.0]) {
                    let d = m.terrain_z_of_level(*g) - z;
                    assert!(d.abs() < 1e-6, "{spalt} {quer}: {d}");
                }
            }
        }
    }

    /// Eine Fläche neben dem Haus bekommt ihr Rechteck dazu; Ecken in der
    /// Platte fallen weg. Mit NHN in der Lage stehen die Höhen in NHN.
    #[test]
    fn flaeche_neben_dem_haus() {
        let mut m = haus();
        let mut l = *m.location();
        l.nhn = Some(52.3);
        m.begin("Lage");
        m.put_location(l);
        m.commit();
        let rand = m.plate_settings().margin;
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[[2000.0, 2000.0], [30000.0, 4000.0]]));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.href, HeightRef::Abs);
        // zwei Ecken des Zusatzrechtecks liegen in der Platte
        assert_eq!(t.points.len(), 6);
        assert!(t.points.iter().all(|p| (p.h - 51.8).abs() < 1e-9));
        assert!(t.points.iter().any(|p| p.e == 30.0 + rand / 1000.0));
        assert_eq!(m.terrain_nhn(), Some(52.3));
    }

    /// Nur ein Grundstück aus dem Lageplan (UTM): Die Punkte kommen in
    /// dessen System dazu, Grenze und Lage bleiben.
    #[test]
    fn zum_grundstueck() {
        let mut m = haus();
        let mut t = Terrain::new(Crs::Local);
        t.local = Some(LocalFrame {
            e0: 1000.0,
            n0: 2000.0,
            rot: 90.0,
        });
        t.parcels = vec![vec![(990.0, 1990.0), (1010.0, 1990.0), (1010.0, 2010.0)]];
        m.begin("Lageplan");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.parcels.len(), 1);
        assert_eq!(t.local.unwrap().e0, 1000.0);
        // Modell-x zeigt nach Nord: Ecke (−rand, −rand) liegt östlich
        let rand = m.plate_settings().margin / 1000.0;
        assert!(
            (t.points[0].e - (1000.0 + rand)).abs() < 1e-9,
            "{:?}",
            t.points[0]
        );
        assert!(
            (t.points[0].n - (2000.0 - rand)).abs() < 1e-9,
            "{:?}",
            t.points[0]
        );
        assert!(t.assumed());
    }

    /// Eine gezeichnete Grenze ohne Gelände: ein örtliches Gelände nur mit
    /// ihr, ohne Punkte, die Platte bleibt eben. Eine zweite kommt dazu;
    /// zu wenig Ecken oder keine Fläche ändern nichts.
    #[test]
    fn grenze_gezeichnet() {
        let mut m = haus();
        let z = m.terrain_z();
        let ring = [
            [-5000.0, -4000.0],
            [20000.0, -4000.0],
            [20000.0, 16000.0],
            [-5000.0, 16000.0],
        ];
        m.begin("Grenze");
        assert!(!m.add_parcel(&ring[..2]));
        assert!(!m.add_parcel(&[[0.0, 0.0], [5000.0, 0.0], [10000.0, 0.0]]));
        assert!(m.terrain().is_none());
        assert!(m.add_parcel(&ring));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.crs, Crs::Local);
        assert!(t.points.is_empty() && !t.assumed());
        // die Umgebung um die gezeichnete Grenze ganz (I6)
        assert_eq!(t.show(), Show::All);
        assert_eq!(
            t.parcels,
            vec![vec![(-5.0, -4.0), (20.0, -4.0), (20.0, 16.0), (-5.0, 16.0)]]
        );
        assert!((t.parcel_area() - 500.0).abs() < 1e-9);
        let b = m.level_building();
        assert_eq!(m.terrain_z(), z);
        // ohne Punkte die ebene Platte am Haus
        assert_eq!(m.terrain_z_at(b, 5000.0, 4000.0), z);
        m.begin("Grenze");
        assert!(m.add_parcel(&[[30000.0, 0.0], [40000.0, 0.0], [40000.0, 10000.0]]));
        m.commit();
        assert_eq!(m.terrain().unwrap().parcels.len(), 2);
        m.begin("Grenze");
        assert!(m.add_parcel(&[[0.0, 0.0], [2000.0, 0.0], [0.0, 2000.0]]));
        m.rollback();
        assert_eq!(m.terrain().unwrap().parcels.len(), 2, "im Schritt");
    }

    /// Steht schon ein angenommenes Gelände (Punkte von Hand um das Haus),
    /// reicht es nach einer größeren Grenze bis um den Plattenabstand über
    /// sie hinaus, eben in der Höhe des Hauses (I6, Jörn 17.10.). Eine
    /// gezogene Grenze nimmt es mit; „nicht“ bleibt gewählt.
    #[test]
    fn platte_waechst_bis_zur_grenze() {
        let mut m = haus();
        let z = m.terrain_z();
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let ring = [
            [-20000.0, -15000.0],
            [35000.0, -15000.0],
            [35000.0, 30000.0],
            [-20000.0, 30000.0],
        ];
        let rand = m.plate_settings().margin;
        // Die Höhenabfrage hält sich außen an den Netzrand, darum zählt die
        // Ausdehnung der Punkte.
        let bis = |m: &Model| {
            let s = m.terrain_spots();
            let x = s.iter().map(|p| p.p[0]);
            let y = s.iter().map(|p| p.p[1]);
            [
                x.clone().fold(f64::MAX, f64::min),
                y.clone().fold(f64::MAX, f64::min),
                x.fold(f64::MIN, f64::max),
                y.fold(f64::MIN, f64::max),
            ]
        };
        let n0 = m.terrain().unwrap().points.len();
        assert!(bis(&m)[0] > -20000.0, "vorher {:?}", bis(&m));
        m.begin("Grenze");
        assert!(m.add_parcel(&ring));
        m.commit();
        let t = m.terrain().unwrap();
        assert!(t.assumed());
        assert_eq!(t.show(), Show::All);
        for p in ring {
            for q in [[p[0] - rand * 0.99, p[1]], [p[0] + rand * 0.99, p[1]], p] {
                let h = m.terrain_z_survey(q[0], q[1]);
                assert!(h.is_some_and(|h| (h - z).abs() < 1.0), "{q:?} {h:?}");
            }
        }
        assert!(t.points.len() > n0);
        let b = bis(&m);
        assert!(
            b[0] <= -20000.0 - rand * 0.99 && b[1] <= -15000.0 - rand * 0.99,
            "{b:?}"
        );
        assert!(
            b[2] >= 35000.0 + rand * 0.99 && b[3] >= 30000.0 + rand * 0.99,
            "{b:?}"
        );
        // am Haus bleibt es, wo es war
        assert!((m.terrain_z_at(m.level_building(), 5000.0, -1000.0) - z).abs() < 1.0);
        let n = m.terrain().unwrap().points.len();
        // „nicht“ gewählt, dann eine Ecke weit hinaus gezogen
        m.begin("Zuschnitt");
        assert!(m.set_terrain_show(Show::Parcel));
        m.commit();
        let mut weiter = ring;
        weiter[2] = [60000.0, 50000.0];
        m.begin("Ecke");
        assert!(m.set_parcel_ring(0, &weiter));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.show(), Show::Parcel, "die Wahl bleibt");
        // die Ecken um die Grenze rücken mit hinaus
        assert!(t.points.len() >= n);
        let b = bis(&m);
        assert!(
            b[2] >= 60000.0 + rand * 0.99 && b[3] >= 50000.0 + rand * 0.99,
            "{b:?}"
        );
        let h = m.terrain_z_survey(60000.0 + rand * 0.99, 50000.0 + rand * 0.99);
        assert!(h.is_some_and(|h| (h - z).abs() < 1.0), "{h:?}");
        // innen genügt: nichts kommt dazu
        // innen genügt: außer den Punkten an der Grenze kommt nichts dazu
        let ohne_grenze = |m: &Model, r: &[[f64; 2]]| {
            let a = grenz_punkte(r);
            let s = m.terrain_spots();
            s.iter()
                .filter(|p| !a.iter().any(|q| nah(*q, [p.p[0], p.p[1]])))
                .count()
        };
        let n = ohne_grenze(&m, &weiter);
        let mut innen = weiter;
        innen[0] = [-19000.0, -14000.0];
        m.begin("Ecke");
        assert!(m.set_parcel_ring(0, &innen));
        m.commit();
        assert_eq!(ohne_grenze(&m, &innen), n);
    }

    /// Zwei Häuser in verschiedener Höhe, dann eine Grenze dicht um sie
    /// (VOB B-251a): Innerhalb des bisherigen Netzrands bleibt jede Höhe,
    /// die neuen Ecken liegen so hoch, wie das Netz an der Platte des
    /// Bezugsgebäudes steht (Kosten H-251a), und ein zweites Mal kommt
    /// keine Bruchkante doppelt dazu.
    #[test]
    fn grenze_aendert_innen_nichts() {
        let mut m = Model::with_seed(3);
        let neu = |m: &mut Model, x0: f64, w: f64, versatz: f64| {
            let b = m.add_building(1);
            let pts = [
                vec3(x0, 0.0, 0.0),
                vec3(x0 + w, 0.0, 0.0),
                vec3(x0 + w, 10000.0, 0.0),
                vec3(x0, 10000.0, 0.0),
            ];
            m.build_from_polygon(b, &pts).unwrap();
            m.begin("Gelände");
            assert!(m.set_terrain_offset_of(b, versatz));
            m.commit();
        };
        neu(&mut m, 0.0, 8000.0, 500.0);
        neu(&mut m, 11000.0, 8000.0, 1500.0);
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let spots = m.terrain_spots();
        let x = |f: fn(f64, f64) -> f64, i: usize, a: f64| spots.iter().map(|s| s.p[i]).fold(a, f);
        let k = [
            x(f64::min, 0, f64::MAX),
            x(f64::min, 1, f64::MAX),
            x(f64::max, 0, f64::MIN),
            x(f64::max, 1, f64::MIN),
        ];
        // ein Raster im bisherigen Netz, 1 mm vom Rand
        let mut raster = Vec::new();
        for i in 0..=40 {
            for j in 0..=40 {
                let q = [
                    k[0] + 1.0 + (k[2] - k[0] - 2.0) * f64::from(i) / 40.0,
                    k[1] + 1.0 + (k[3] - k[1] - 2.0) * f64::from(j) / 40.0,
                ];
                raster.push((q, m.terrain_z_survey(q[0], q[1]).unwrap()));
            }
        }
        let platte = m.hoehe_an_der_platte();
        let n = m.terrain().unwrap().points.len();
        let brueche = m.terrain().unwrap().breaks.len();
        let ring = [
            [k[0] - 500.0, k[1] - 500.0],
            [k[2] + 500.0, k[1] - 500.0],
            [k[2] + 500.0, k[3] + 500.0],
            [k[0] - 500.0, k[3] + 500.0],
        ];
        m.begin("Grenze");
        assert!(m.add_parcel(&ring));
        m.commit();
        let t = m.terrain().unwrap();
        // vier Ecken des Rechtecks, dazu die Grenze mit ihren Ecken und
        // Viertelpunkten (Jörn 17.10.)
        assert_eq!(t.points.len(), n + 4 + 16);
        assert_eq!(t.breaks.len(), brueche + 2, "der alte Rand und die Grenze");
        for (q, z) in &raster {
            let h = m.terrain_z_survey(q[0], q[1]).unwrap();
            assert!((h - z).abs() < 1e-6, "{q:?}: {z} → {h}");
        }
        for s in m.terrain_spots().iter().skip(n).take(4) {
            assert!((s.p[2] - platte).abs() < 1.0, "{:?} statt {platte}", s.p);
        }
        // weiter gezogen: der Rand von eben kommt dazu, keiner doppelt
        let mut weiter = ring;
        weiter[2] = [k[2] + 20000.0, k[3] + 20000.0];
        m.begin("Ecke");
        assert!(m.set_parcel_ring(0, &weiter));
        m.commit();
        let t = m.terrain().unwrap();
        // die Ecken von eben gehen samt ihrem Rand, die neuen kommen um den
        // alten Netzrand; die alte Grenze geht als Bruchkante, die neue
        // kommt (B-264a: wie gleich so gezeichnet)
        assert_eq!(t.breaks.len(), brueche + 2);
        let mut ringe: Vec<Vec<String>> = t
            .breaks
            .iter()
            .map(|b| {
                let mut v = b.pts.clone();
                v.sort();
                v
            })
            .collect();
        ringe.sort();
        ringe.dedup();
        assert_eq!(ringe.len(), t.breaks.len());
        for (q, z) in &raster {
            let h = m.terrain_z_survey(q[0], q[1]).unwrap();
            assert!((h - z).abs() < 1e-6, "weiter {q:?}: {z} → {h}");
        }
    }

    /// Eine gezeichnete Grenze hält das Gelände (Jörn 17.10.): Mit den
    /// ersten Punkten kommen Punkte an ihre Ecken und Viertelpunkte in der
    /// Höhe der Platte, ihr Ring wird Bruchkante. Ein tiefer Punkt mitten
    /// im Grundstück wirkt nur bis zur Grenze, draußen bleibt alles eben.
    #[test]
    fn grenze_haelt_das_gelaende() {
        let mut m = haus();
        let z = m.terrain_z();
        let ring = [
            [-10000.0, -8000.0],
            [20000.0, -8000.0],
            [20000.0, 16000.0],
            [-10000.0, 16000.0],
        ];
        m.begin("Grenze");
        assert!(m.add_parcel(&ring));
        m.commit();
        m.begin("Punkt");
        assert_eq!(
            m.add_terrain_points(&[[15000.0, 12000.0, z - 2000.0]])
                .len(),
            1
        );
        m.commit();
        let t = m.terrain().unwrap();
        let f = m.survey_frame_of(t).unwrap();
        let lage: Vec<[f64; 2]> = t.points.iter().map(|p| f.to_model(p.e, p.n)).collect();
        let an_der_grenze = grenz_punkte(&ring);
        assert_eq!(an_der_grenze.len(), 16);
        for q in &an_der_grenze {
            assert!(lage.iter().any(|p| nah(*p, *q)), "{q:?}");
        }
        assert!(t.breaks.iter().any(|b| b.closed && b.pts.len() == 16));
        let innen = m.terrain_z_survey(15000.0, 12000.0).unwrap();
        assert!((innen - (z - 2000.0)).abs() < 1.0, "{innen}");
        // auf der Grenze und in einem Band draußen eben
        let rand = m.plate_settings().margin;
        let mut draussen = vec![[20000.0, 13000.0], [17000.0, 16000.0]];
        for i in 0..=20 {
            let s = f64::from(i) / 20.0;
            for d in [0.3, 0.6, 0.9] {
                let w = rand * d;
                draussen.push([20000.0 + w, -8000.0 + 24000.0 * s]);
                draussen.push([-10000.0 + 30000.0 * s, 16000.0 + w]);
            }
        }
        for q in draussen {
            let h = m.terrain_z_survey(q[0], q[1]).unwrap();
            assert!((h - z).abs() < 1.0, "{q:?}: {h} statt {z}");
        }
    }

    /// Mit Griffen geändert zieht die Grenze ihre Punkte mit (Bedienbarkeit
    /// U-264a): Die obere Kante 5 m nach außen, dann steht kein Punkt mehr
    /// an der alten Linie, wo die neue nicht liegt, die neue hat ihre Punkte
    /// und ihre Bruchkante, der tiefe Punkt innen bleibt, und draußen ist
    /// es eben.
    #[test]
    fn grenze_zieht_ihre_punkte_mit() {
        let mut m = haus();
        let z = m.terrain_z();
        let ring = [
            [-10000.0, -8000.0],
            [20000.0, -8000.0],
            [20000.0, 16000.0],
            [-10000.0, 16000.0],
        ];
        m.begin("Grenze");
        assert!(m.add_parcel(&ring));
        m.commit();
        m.begin("Punkt");
        assert_eq!(
            m.add_terrain_points(&[[15000.0, 12000.0, z - 2000.0]])
                .len(),
            1
        );
        m.commit();
        let brueche = m.terrain().unwrap().breaks.len();
        let mut weiter = ring;
        weiter[2][1] = 21000.0;
        weiter[3][1] = 21000.0;
        m.begin("Kante");
        assert!(m.set_parcel_ring(0, &weiter));
        m.commit();
        let t = m.terrain().unwrap();
        let f = m.survey_frame_of(t).unwrap();
        let lage: Vec<[f64; 2]> = t
            .points
            .iter()
            .filter(|p| !p.off)
            .map(|p| f.to_model(p.e, p.n))
            .collect();
        let neu = grenz_punkte(&weiter);
        for q in grenz_punkte(&ring) {
            let da = lage.iter().any(|p| nah(*p, q));
            assert_eq!(da, neu.iter().any(|p| nah(*p, q)), "alte Stelle {q:?}");
        }
        for q in &neu {
            assert!(lage.iter().any(|p| nah(*p, *q)), "neue Stelle {q:?}");
        }
        // die alte Bruchkante ging, die neue kam; die Ecken um die Grenze
        // rückten mit, ohne einen weiteren Rand (B-264a)
        assert!(t.breaks.iter().any(|b| b.closed && b.pts.len() == 16));
        assert_eq!(t.breaks.len(), brueche);
        let innen = m.terrain_z_survey(15000.0, 12000.0).unwrap();
        assert!((innen - (z - 2000.0)).abs() < 1.0, "{innen}");
        let rand = m.plate_settings().margin;
        let mut draussen = vec![[20000.0, 18000.0], [5000.0, 21000.0]];
        for i in 0..=20 {
            let s = f64::from(i) / 20.0;
            for d in [0.3, 0.6, 0.9] {
                let w = rand * d;
                draussen.push([20000.0 + w, -8000.0 + 29000.0 * s]);
                draussen.push([-10000.0 + 30000.0 * s, 21000.0 + w]);
            }
        }
        for q in draussen {
            let h = m.terrain_z_survey(q[0], q[1]).unwrap();
            assert!((h - z).abs() < 1.0, "{q:?}: {h} statt {z}");
        }
    }

    /// Kurze Kanten bleiben ungeteilt; zwei Kanten kreuzen sich, treffen
    /// sich nicht oder berühren sich nur (heikel).
    #[test]
    fn grenzpunkte_und_schnitte() {
        let klein = [[0.0, 0.0], [1500.0, 0.0], [1500.0, 1000.0]];
        assert_eq!(grenz_punkte(&klein).len(), 3);
        let p = grenz_punkte(&[[0.0, 0.0], [4000.0, 0.0], [0.0, 1000.0]]);
        assert_eq!(
            p[..4],
            [[0.0, 0.0], [1000.0, 0.0], [2000.0, 0.0], [3000.0, 0.0]]
        );
        assert_eq!(p.len(), 9, "die kurze Kante ungeteilt");
        let a = kanten(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], true);
        assert_eq!(a.len(), 3);
        assert_eq!(kanten(&[[0.0, 0.0], [10.0, 0.0]], false).len(), 1);
        let k = |r: &[[f64; 2]]| {
            kanten(
                &r.iter()
                    .map(|p| [p[0] * 1000.0, p[1] * 1000.0])
                    .collect::<Vec<_>>(),
                false,
            )[0]
        };
        let a = [
            k(&[[0.0, 0.0], [10.0, 0.0]]),
            k(&[[10.0, 0.0], [10.0, 10.0]]),
        ];
        let quer = [k(&[[5.0, -5.0], [5.0, 5.0]])];
        let weit = [k(&[[50.0, -5.0], [50.0, 5.0]])];
        let ecke = [k(&[[10.0, 10.0], [20.0, 20.0]])];
        assert!(matches!(schnitt(a[0], quer[0]), Schnitt::Echt(..)));
        assert!(matches!(schnitt(a[0], weit[0]), Schnitt::Keiner));
        assert!(matches!(schnitt(a[1], ecke[0]), Schnitt::Heikel), "berührt");
    }

    /// Im System eines Lageplans (gedreht) kommt die Grenze in dessen
    /// Werten dazu; die erste Fläche legt die Punkte dann bis über das
    /// Grundstück an.
    #[test]
    fn grenze_im_system_und_gelaende_darueber() {
        let mut m = haus();
        let mut t = Terrain::new(Crs::Local);
        t.local = Some(LocalFrame {
            e0: 1000.0,
            n0: 2000.0,
            rot: 90.0,
        });
        m.begin("Lageplan");
        assert!(m.set_terrain(Some(t)));
        m.commit();
        let ring = [
            [-10000.0, -10000.0],
            [30000.0, -10000.0],
            [30000.0, 20000.0],
        ];
        m.begin("Grenze");
        assert!(m.add_parcel(&ring));
        m.commit();
        let p = &m.terrain().unwrap().parcels[0];
        // Modell-x zeigt nach Nord
        assert_eq!(p[0], (1010.0, 1990.0));
        assert_eq!(p[1], (1010.0, 2030.0));
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&[]));
        m.commit();
        let t = m.terrain().unwrap();
        let f = m.survey_frame_of(t).unwrap();
        let rand = m.plate_settings().margin;
        let xs: Vec<[f64; 2]> = t.points.iter().map(|q| f.to_model(q.e, q.n)).collect();
        let max_x = xs.iter().map(|q| q[0]).fold(f64::MIN, f64::max);
        assert!((max_x - (30000.0 + rand)).abs() < 1e-3, "{xs:?}");
        assert_eq!(t.parcels.len(), 1, "die Grenze bleibt");
    }

    /// Das Aufmaß kommt nach der gezeichneten Grenze: Sie bleibt im
    /// Grundriss, wo sie war, jetzt im System des Aufmaßes. Bringt das
    /// Aufmaß eine eigene Grenze mit, gilt seine.
    #[test]
    fn grenze_bleibt_beim_aufmass() {
        let ring = [
            [-5000.0, -4000.0],
            [20000.0, -4000.0],
            [20000.0, 16000.0],
            [-5000.0, 16000.0],
        ];
        let aufmass = |grenze: bool| {
            let mut t = Terrain::new(Crs::Local);
            for (i, (e, n)) in [
                (100.0, 200.0),
                (160.0, 200.0),
                (160.0, 250.0),
                (100.0, 250.0),
            ]
            .into_iter()
            .enumerate()
            {
                t.points.push(TerrainPoint {
                    nr: format!("{}", i + 1).into(),
                    e,
                    n,
                    h: 30.0,
                    code: Default::default(),
                    off: false,
                    hand: false,
                    gemessen: None,
                });
            }
            if grenze {
                t.parcels = vec![vec![(110.0, 210.0), (150.0, 210.0), (150.0, 240.0)]];
            }
            t
        };
        for grenze in [false, true] {
            let mut m = haus();
            m.begin("Grenze");
            assert!(m.add_parcel(&ring));
            m.commit();
            m.begin("Aufmaß");
            assert!(m.import_terrain(aufmass(grenze)));
            m.commit();
            let t = m.terrain().unwrap();
            assert_eq!(t.parcels.len(), 1);
            let f = m.survey_frame_of(t).unwrap();
            let im_plan: Vec<[f64; 2]> = t.parcels[0]
                .iter()
                .map(|&(e, n)| f.to_model(e, n))
                .collect();
            if grenze {
                assert_eq!(t.parcels[0].len(), 3, "die Grenze des Aufmaßes");
            } else {
                for (a, b) in im_plan.iter().zip(&ring) {
                    assert!(
                        (a[0] - b[0]).abs() < 1.0 && (a[1] - b[1]).abs() < 1.0,
                        "{a:?} {b:?}"
                    );
                }
            }
        }
    }

    /// Ohne Haus und ohne Fläche gibt es nichts anzulegen; mit vermessenen
    /// Punkten ist das Gelände nicht angenommen.
    #[test]
    fn nichts_anzulegen() {
        let mut m = Model::with_seed(1);
        m.begin("Fläche");
        assert!(!m.seed_flat_terrain(&[]));
        m.commit();
        assert!(m.terrain().is_none());
        let mut t = Terrain::new(Crs::Local);
        assert!(!t.assumed(), "ohne Punkte");
        t.points.push(TerrainPoint {
            nr: "1".into(),
            e: 0.0,
            n: 0.0,
            h: 1.0,
            code: Default::default(),
            off: false,
            hand: false,
            gemessen: None,
        });
        assert!(!t.assumed());
    }

    /// Erd- und Baustellenmengen je Gebäude und Schlüssel, summiert.
    fn erdmengen(m: &Model) -> Vec<(String, f64)> {
        let mut e: Vec<(String, f64)> = crate::qto::schedule(m)
            .auto
            .iter()
            .filter(|a| a.key.starts_with("earth.") || a.key.starts_with("site."))
            .map(|a| (format!("{:?} {} {}", a.building, a.key, a.unit), a.value))
            .collect();
        e.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out: Vec<(String, f64)> = Vec::new();
        for (k, v) in e {
            match out.last_mut() {
                Some(l) if l.0 == k => l.1 += v,
                _ => out.push((k, v)),
            }
        }
        out
    }

    /// Dieselbe Grenze gibt dieselben Mengen, ob gleich so gezeichnet oder
    /// aus einer anderen gezogen (VOB B-264a, wie vob/probe-264-ecke.rs):
    /// Fläche, Grenze, dann ein Punkt vor der Südwand 1 m tiefer oder
    /// höher. Ein Punkt der alten Grenze, den man von Hand gehoben hat,
    /// bleibt beim Ziehen.
    #[test]
    fn gezogen_wie_gezeichnet() {
        let alt = [
            [-3000.0, -3000.0],
            [13000.0, -3000.0],
            [13000.0, 11000.0],
            [-3000.0, 11000.0],
        ];
        let mut sued = alt;
        sued[0][1] = -8000.0;
        sued[1][1] = -8000.0;
        let mut ecke = alt;
        ecke[1] = [18000.0, -8000.0];
        let mit = |neu: &[[f64; 2]], dz: f64, gezogen: bool| {
            let mut m = haus();
            m.begin("Fläche");
            assert!(m.seed_flat_terrain(&[]));
            m.commit();
            m.begin("Grenze");
            if gezogen {
                assert!(m.add_parcel(&alt));
                m.commit();
                m.begin("Ecke");
                assert!(m.set_parcel_ring(0, neu));
            } else {
                assert!(m.add_parcel(neu));
            }
            m.commit();
            let z = m.terrain_z();
            m.begin("Punkt");
            assert_eq!(m.add_terrain_points(&[[5000.0, -1200.0, z + dz]]).len(), 1);
            m.commit();
            m
        };
        for neu in [&sued, &ecke] {
            for dz in [-1000.0, 1000.0] {
                let a = mit(neu, dz, true);
                let b = mit(neu, dz, false);
                let n = |m: &Model| m.terrain().unwrap().points.len();
                assert_eq!(n(&a), n(&b), "{neu:?} {dz}");
                let (a, b) = (erdmengen(&a), erdmengen(&b));
                assert_eq!(a.len(), b.len());
                assert!(a.iter().any(|x| x.0.contains("earth.fill")));
                for (x, y) in a.iter().zip(&b) {
                    assert_eq!(x.0, y.0);
                    let d = (x.1 - y.1).abs();
                    assert!(d <= 1e-9 * x.1.abs().max(1.0), "{neu:?} {dz} {x:?} {y:?}");
                }
            }
        }
        // von Hand gehoben bleibt der Punkt der alten Grenze
        let mut m = haus();
        m.begin("Grenze");
        assert!(m.add_parcel(&alt));
        m.commit();
        let z = m.terrain_z();
        m.begin("Punkt");
        assert_eq!(m.add_terrain_points(&[[5000.0, 4000.0, z]]).len(), 1);
        m.commit();
        let t = m.terrain().unwrap();
        let f = m.survey_frame_of(t).unwrap();
        let mitte = [5000.0, -3000.0];
        let i = t
            .points
            .iter()
            .position(|p| nah(f.to_model(p.e, p.n), mitte))
            .unwrap();
        assert_eq!(t.points[i].code, AN_DER_GRENZE);
        m.begin("Heben");
        assert!(m.move_terrain_points(&[(i, [mitte[0], mitte[1], z + 300.0])]));
        m.commit();
        m.begin("Ecke");
        assert!(m.set_parcel_ring(0, &sued));
        m.commit();
        let t = m.terrain().unwrap();
        let da = |q: [f64; 2]| {
            t.points
                .iter()
                .any(|p| !p.off && nah(f.to_model(p.e, p.n), q))
        };
        assert!(da(mitte), "gehoben bleibt");
        assert!(!da([9000.0, -3000.0]), "die übrigen gehen");
        assert!(grenz_punkte(&sued).into_iter().all(da));
    }

    /// Haus und Garage in verschiedener Höhe, eine Grenze weit um beide
    /// (VOB B-264d, wie vob/probe-264-reihenfolge.rs): Erst Fläche, dann
    /// Grenze setzt die Punkte an der Grenze wie erst Grenze, dann Fläche,
    /// auch wo die Grenze zwei Bruchkanten an derselben Stelle kreuzt; mit
    /// einem Punkt danach sind die Mengen gleich
    /// (vob/probe-264-grenze-punkt.rs).
    #[test]
    fn grenzpunkte_in_jeder_reihenfolge() {
        let mut mengen = Vec::new();
        let ring = [
            [-2000.0, -2000.0],
            [60000.0, -2000.0],
            [60000.0, 12000.0],
            [-2000.0, 12000.0],
        ];
        for flaeche_zuerst in [true, false] {
            let mut m = Model::with_seed(3);
            for h in [
                (0.0, 0.0, 10000.0, 8000.0, 500.0),
                (12000.0, 2000.0, 3000.0, 6000.0, 0.0),
            ] {
                let b = m.add_building(1);
                let p = [
                    vec3(h.0, h.1, 0.0),
                    vec3(h.0 + h.2, h.1, 0.0),
                    vec3(h.0 + h.2, h.1 + h.3, 0.0),
                    vec3(h.0, h.1 + h.3, 0.0),
                ];
                m.build_from_polygon(b, &p).unwrap();
                m.begin("Gelände");
                m.set_terrain_offset_of(b, h.4);
                m.commit();
            }
            if flaeche_zuerst {
                m.begin("Fläche");
                assert!(m.seed_flat_terrain(&[]));
                m.commit();
            }
            m.begin("Grenze");
            assert!(m.add_parcel(&ring));
            m.commit();
            if !flaeche_zuerst {
                m.begin("Fläche");
                assert!(m.seed_flat_terrain(&[]));
                m.commit();
            }
            let t = m.terrain().unwrap();
            let f = m.survey_frame_of(t).unwrap();
            for q in grenz_punkte(&ring) {
                assert!(
                    t.points
                        .iter()
                        .any(|p| !p.off && nah(f.to_model(p.e, p.n), q)),
                    "Fläche zuerst {flaeche_zuerst}: {q:?}"
                );
            }
            let z = m.terrain_z();
            m.begin("Punkt");
            assert_eq!(
                m.add_terrain_points(&[[5000.0, -1200.0, z - 1000.0]]).len(),
                1
            );
            m.commit();
            mengen.push(erdmengen(&m));
        }
        let (a, b) = (&mengen[0], &mengen[1]);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            assert_eq!(x.0, y.0);
            assert!(
                (x.1 - y.1).abs() <= 1e-9 * x.1.abs().max(1.0),
                "{x:?} {y:?}"
            );
        }
    }
}

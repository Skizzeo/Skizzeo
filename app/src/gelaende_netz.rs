//! Echtes Gelände in der App (Gelände G1, plan-gelaendemodul.md §5 G1
//! „Darstellung“, §4.5 und §4.6): Sobald ein Höhenaufmaß da ist, steht statt
//! der Geländeplatte sein Dreiecksnetz da, gleich bedient und gleich
//! gefärbt. In 3D ein Körper wie ein Geländemodell: Rasen oben, Erdreich
//! am Rand bis eine Geländedicke unter den tiefsten Punkt, die vollen
//! Meter als Höhenlinien. Im Grundriss Höhenlinien (alle 0,25 m, volle
//! Meter kräftig), Messpunkte und die Grenze; im Schnitt die Schnittfläche
//! mit der Schraffur „Erdreich“ und die Geländelinie; in den Ansichten die
//! Geländelinie in der Ebene der Fassade, die zum Betrachter zeigt.
//!
//! Der Zuschnitt gilt überall gleich. Abgeschnitten wird nur die
//! Darstellung: Dreiecke und Linien werden an der Grenze geteilt, gerechnet
//! wird weiter mit allen Punkten. Bei „Umgebung blass“ liegt das ganze
//! Netz blass da und das Grundstück knapp darüber in Rasen.
//!
//! Ohne Höhen (nur das Grundstück aus dem Lageplan, G1c) liegt das
//! Gelände eben in Höhe OK Gelände, in Form des Grundstücks und mit dem
//! Abstand der Platte als Umgebung. Der Lageplan liegt im Grundriss als
//! blasse Unterlage darunter, die Grenzen 0,25 mm.
//!
//! Am Haus endet das Gelände wie die Platte (Außenumriss über der
//! Gründung): kein Rasen, keine Höhenlinie und kein Messpunkt im Haus,
//! im Schnitt endet die Geländelinie an der Außenwand. Darunter liegt
//! Erdreich ab der Unterkante der Gründung.
//!
//! Mit Geländeflächen (G3c) steht überall das Planungsgelände
//! ([`Model::planned_terrain`]): in 3D und im Schnitt die Flächen mit ihren
//! Böschungen, die Höhenlinien auch dort aus dem Planungsgelände. Im
//! Grundriss dazu der Umriss jeder Fläche und die Böschungsschraffen vom
//! Kopf zum Fuß (lang, kurz), in Schnitt und Ansicht das Urgelände, wo es
//! sich ändert, gestrichelt unter oder über der Geländelinie.

use crate::draw_table::{DrawTable, Stroke};
use crate::gelaende_platte::bauphase::{self, Bauphase};
use crate::gelaende_platte::bodenaufbau::{self, schluessel, Bodenaufbau};
use crate::ui::ViewKind;
use gitter::{ueberlappt, Gitter, Kasten};
use sk_math::{polygon, vec3, Vec3};
use sk_model::solid::{right_of, SectionFrame, Tri, NO_LAYER};
use sk_model::{edge_kind, BuildingId, LevelKind, Model, Show, Solid, UnderlayKind};
use sk_render::{DashPattern, Helper};
use std::collections::{HashMap, HashSet};
use std::f64::consts::{FRAC_PI_2, PI};

pub(crate) mod gitter;

/// Abstand der Höhenlinien im Grundriss (m); volle Meter sind
/// Hauptlinien, nur sie stehen in 3D. Gäbe er zu viele Stücke, gilt der
/// gröbere aus [`Model::terrain_contour_step`] (Review #121, Auflage H1).
pub(crate) const STUFE: f64 = 0.25;

/// Höchstens so viele Dreiecke hat das geglättete Gelände; mehr hieße
/// beim Ziehen und Drehen spürbar längere Rechenzeit. Darüber wird
/// gröber geglättet, notfalls gar nicht (die Leiste sagt es).
pub(crate) const GLATT_HOECHSTENS: usize = 1_500_000;

/// Netz und Höhenlinien vorab rechnen, beim Öffnen eines Projekts, wo
/// ohnehin gewartet wird; sonst entstünden sie träge im ersten Bild
/// (Review #123 W45 H5: 0,3 s bei 100.000 Punkten). Beide bleiben am
/// Aufmaß gemerkt.
pub(crate) fn vorab(m: &Model) {
    if let (Some(s), Some(t)) = (m.terrain_contour_step(STUFE), m.terrain()) {
        t.mesh().hoehenlinien(s);
    }
}

/// Das Grundstück liegt bei „Umgebung blass“ so weit über dem blassen Netz
/// (mm), damit beide nicht flimmern.
const HEBEN: f64 = 5.0;

/// Kantenlänge einer Punktmarke im Grundriss (Bildpunkte bei Skalierung 1).
const MARKE: f64 = 3.0;

/// Ein Punkt im Grundriss.
type P2 = [f64; 2];

/// Ein Stück Höhenlinie, an Grenze und Haus geteilt: Höhe, Anfang und
/// Ende, im Grundstück, Hauptlinie (volle Meter).
type Stueck = (f64, [P2; 2], bool, bool);

/// Eine Höhenzahl im Grundriss: Mitte, Richtung der Schrift (rad, lesbar
/// von unten oder rechts), Höhe (mm zu ±0,00), Text in m ü. NHN, voll (im
/// Grundstück) oder blass.
pub(crate) type Zahl = (P2, f64, f64, String, bool);

/// Abstand der Höhenzahlen längs einer Höhenlinie (mm); die erste steht
/// einen halben Abstand vom Anfang, kurze Linien tragen eine in der Mitte.
const ZAHL_ABSTAND: f64 = 40_000.0;

/// Darf an der vorgesehenen Stelle keine Zahl stehen (Haus, außerhalb des
/// Zuschnitts), rückt sie in diesen Schritten (mm) weiter.
const ZAHL_SCHRITT: f64 = 2_000.0;

/// Bis zu so vielen Messpunkten steht an jedem seine Kote; ein größeres
/// Aufmaß (Laserscan, dichtes Raster) zeigt nur die Kreuze, sonst deckten
/// die Zahlen das Bild zu und kosteten Speicher.
const PUNKT_KOTEN: usize = 2_000;
/// Abstand einer Kote von der Grundstücksgrenze (mm am Bau), damit der
/// Strichpunkt keine Ziffer streift.
const RAND: f64 = 50.0;

/// Gelände aus dem Aufmaß im Modell (mm, z zu ±0,00). Was nur vom Modell
/// abhängt, steht hier einmal gerechnet: Höhenlinien und Messpunkte sind
/// schon an Grenze und Haus geteilt, Häuser und Dreiecke stehen in je
/// einem Raster (Review #129 B3, B4). Zoom und Sicht kosten danach nur
/// noch das Zeichnen.
pub(crate) struct Gelaende {
    dreiecke: Vec<[Vec3; 3]>,
    /// Raster über den Dreiecken.
    netz: Gitter,
    /// Grenzen; leer ohne Grenze.
    ringe: Vec<Vec<P2>>,
    /// Zuschnitt; ohne Grenze immer „alles“.
    show: Show,
    /// Stücke der Höhenlinien außerhalb der Häuser.
    stuecke: Vec<Stueck>,
    /// Messpunkte außerhalb der Häuser, die gezeigt werden: voll im
    /// Grundstück, sonst blass.
    marken: Vec<(Vec3, bool)>,
    /// Außenumriss jedes Hauses über der Gründung und die Unterkante
    /// seiner Gründung (mm).
    haus: Vec<(Vec<P2>, f64)>,
    /// Raster über den Häusern.
    haeuser: Gitter,
    /// Schichten unter und um jede Gründung ([`Bodenaufbau`]).
    pub(crate) aufbau: Bodenaufbau,
    /// Unterkante des Rands: eine Geländedicke unter dem tiefsten Punkt,
    /// mindestens unter der tiefsten Gründung ([`Bodenaufbau::unten`]).
    pub(crate) unten: f64,
    /// Der Lageplan im Grundriss in Kacheln ([`Kachel`]).
    pub(crate) unterlage: Vec<Kachel>,
    /// Dreiecke des Urgeländes, an deren Stelle das Planungsgelände steht
    /// (G3c); leer ohne Geländeflächen. Mit Raster für den Schnitt.
    ur: Vec<[Vec3; 3]>,
    ur_netz: Gitter,
    /// Böschungsschraffen (lang: `true`) und Umrisse der Geländeflächen.
    schraffen: Vec<([Vec3; 2], bool)>,
    /// Höhenzahlen an den Höhenlinien und in ebenen Geländeflächen
    /// (Bedienbarkeit, Ist-Bilder zu #202), einmal je Gelände.
    pub(crate) zahlen: Vec<Zahl>,
    /// Koten der Messpunkte (bis [`PUNKT_KOTEN`]), rechts neben dem Kreuz.
    pub(crate) koten: Vec<Zahl>,
    umrisse: Vec<Vec<Vec3>>,
    /// Senkrechte Wände, wo die Höhe springt (Review #183 b, B2).
    waende: Vec<[Vec3; 3]>,
    /// Außenrand des Netzes aus dem Modell, das ihn je Netz und Plan einmal
    /// bestimmt (Review #183 B1): Die Stücke des Planungsgeländes teilen
    /// die Kanten, an denen sie an ungeteilte Dreiecke stoßen, das sind
    /// keine Ränder. `None` (ebenes Gelände): der Rand der Dreiecke.
    rand: Option<Vec<(Vec3, Vec3)>>,
}

/// Höchstens so viele Strecken des Lageplans liegen in einer Kachel
/// (Review #139 N1): Ein Schritt an einem Haus ändert nur die Kacheln mit
/// Strecken, die es schneidet; der Renderer baut und lädt nur sie neu, wie
/// dicht der Plan auch ist. Kleinere Kacheln hießen mehr Zeichenaufrufe
/// je Bild.
const KACHEL: usize = 1024;

/// Teile der Strecken `ix` (Mitten `mitte`) im Rechteck `r` (x0, y0, x1,
/// y1) in Kacheln mit höchstens [`KACHEL`] Strecken: Das Rechteck wird
/// geviertelt, bis es passt (höchstens 12 Stufen).
fn kacheln(mitte: &[P2], ix: Vec<usize>, r: [f64; 4], stufe: u32, out: &mut Vec<Vec<usize>>) {
    if ix.len() <= KACHEL || stufe >= 12 {
        if !ix.is_empty() {
            out.push(ix);
        }
        return;
    }
    let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    let mut teil: [Vec<usize>; 4] = Default::default();
    for i in ix {
        let p = mitte[i];
        teil[usize::from(p[0] >= cx) + 2 * usize::from(p[1] >= cy)].push(i);
    }
    let [a, b, c, d] = teil;
    kacheln(mitte, a, [r[0], r[1], cx, cy], stufe + 1, out);
    kacheln(mitte, b, [cx, r[1], r[2], cy], stufe + 1, out);
    kacheln(mitte, c, [r[0], cy, cx, r[3]], stufe + 1, out);
    kacheln(mitte, d, [cx, cy, r[2], r[3]], stufe + 1, out);
}

/// Ein Stück des Lageplans: Strecken (mm, auf dem Gelände, nicht im Haus),
/// ob sie Grenzen sind, und ihre Prüfsumme als Schlüssel ihres Puffers im
/// Renderer. Welche Strecke in welche Kachel fällt, hängt nur am Plan und
/// seiner Lage (über ihre Mitte), nicht an den Häusern; schneidet ein
/// Haus eine lange Strecke, ändert sich die Prüfsumme ihrer Kachel, wo
/// diese auch liegt.
#[derive(Clone, Debug)]
pub(crate) struct Kachel {
    pub(crate) key: u64,
    striche: Vec<([Vec3; 2], bool)>,
}

impl Gelaende {
    /// Das Gelände zum Modell; `None` ohne Aufmaß, Lage oder Netz. Ohne
    /// Höhen, aber mit Grundstück eben ([`Gelaende::eben`]). Gründungen und
    /// Umrisse über das Verzeichnis ([`Model::lesend`]), nicht je Zug über
    /// alle Bauteile (Review #129 B4).
    pub(crate) fn aus(m: &Model) -> Option<Gelaende> {
        m.lesend(|| Gelaende::rechnen(m))
    }

    /// Trifft der Sehstrahl von `o` in Richtung `d` die Oberfläche? Ein
    /// Klick in 3D wählt so das Gelände wie die Platte (I1). Über das
    /// Raster wie [`Gelaende::treffpunkt`], nicht über alle Dreiecke
    /// (Review H-264c).
    pub(crate) fn getroffen(&self, o: Vec3, d: Vec3) -> bool {
        self.treffpunkt(o, d).is_some()
    }

    /// Der nächste Punkt, in dem der Sehstrahl von `o` in Richtung `d` die
    /// Oberfläche trifft; dort setzt „Grenze zeichnen“ in 3D die Ecke (Jörn
    /// 17.10.). Geprüft werden nur die Dreiecke im Raster an der Linie des
    /// Strahls im Grundriss (Review B-264b: sonst je Mausbewegung alle).
    pub(crate) fn treffpunkt(&self, o: Vec3, d: Vec3) -> Option<Vec3> {
        let mut nah = Vec::new();
        if d.x == 0.0 && d.y == 0.0 {
            self.netz.bei(&[o.x, o.y, o.x, o.y], &mut nah);
        } else {
            self.netz.entlang([o.x, o.y], [-d.y, d.x], &mut nah);
        }
        nah.iter()
            .map(|&i| &self.dreiecke[i as usize])
            .filter_map(|t| sk_math::ray_triangle(o, d, t[0], t[1], t[2]))
            .min_by(f64::total_cmp)
            .map(|t| o + d * t)
    }

    fn rechnen(m: &Model) -> Option<Gelaende> {
        let mut dreiecke = m.terrain_triangles();
        if dreiecke.is_empty() {
            return Gelaende::eben(m);
        }
        let (t, f, h0) = (m.terrain()?, m.survey_frame()?, m.terrain_nhn()?);
        // mit Geländeflächen das Planungsgelände statt des Urgeländes
        let plan = m.planned_terrain().unwrap_or_default();
        let mut weg = vec![false; dreiecke.len()];
        for &i in &plan.replaced {
            if let Some(w) = weg.get_mut(i as usize) {
                *w = true;
            }
        }
        // die Dreiecke daneben mit den Ecken der Stücke auf der Kante: Sie
        // bleiben Urgelände (Höhenlinien und -zahlen wie bisher), aber ohne
        // T-Stoß (G-275a)
        let mut naht = vec![false; dreiecke.len()];
        for (i, _) in &plan.seam {
            if let (Some(w), Some(n)) = (weg.get_mut(*i as usize), naht.get_mut(*i as usize)) {
                *w = true;
                *n = true;
            }
        }
        // geglättet (Jörn 17.10.): Ersetzte Dreiecke bleiben eben, ihre
        // Kanten gerade, so schließt das Planungsgelände ohne Spalt an
        let (glatt, je) = m.terrain_triangles_smooth(&weg, GLATT_HOECHSTENS);
        // Die ebenen Dreiecke teilen sich die Zwischenpunkte auf ihren
        // Kanten mit den geglätteten daneben, am Rand des Netzes liegen sie
        // auf seinen geraden Kanten. Naht, Flächen, Wände und der Rand
        // bekommen sie auch, sonst stünde dort ein T-Stoß (Review H-276a:
        // mit „fein“ im Export 198 offene Kanten, ohne Flächen 96 am Rand).
        let mut zwischen = Vec::new();
        if je > 1 {
            let roh = std::mem::replace(&mut dreiecke, glatt);
            zwischen = kantenpunkte(&roh, &dreiecke, &weg, &m.terrain_border(), je);
            let mal = |v: &[bool]| -> Vec<bool> {
                v.iter().flat_map(|&w| std::iter::repeat_n(w, je)).collect()
            };
            (weg, naht) = (mal(&weg), mal(&naht));
        }
        let ur: Vec<[Vec3; 3]> = dreiecke
            .iter()
            .zip(weg.iter().zip(&naht))
            .filter_map(|(d, (w, n))| (*w && !*n).then_some(*d))
            .collect();
        let rand: Vec<(Vec3, Vec3)> = if ur.is_empty() {
            m.terrain_border()
        } else {
            plan.border.clone()
        }
        .into_iter()
        .map(|[a, b]| (a, b))
        .collect();
        let rand = strecken_mit_punkten(rand, &zwischen);
        if !ur.is_empty() {
            let mut k = 0;
            dreiecke.retain(|_| {
                k += 1;
                !weg[k - 1]
            });
            let naht: Vec<[Vec3; 3]> = plan.seam.iter().flat_map(|x| x.1.iter().copied()).collect();
            dreiecke.extend(mit_kantenpunkten(naht, &zwischen));
        }
        // Höhenlinien auf dem geglätteten Gelände je Dreieck, nicht die des
        // Aufmaßes: Die lägen neben oder unter der runden Fläche
        let glatt_linien = if je > 1 {
            let s = m.terrain_contour_step(STUFE);
            s.map(|s| hoehenstuecke(&dreiecke, h0, s))
        } else {
            None
        };
        dreiecke.extend(mit_kantenpunkten(plan.triangles.clone(), &zwischen));
        let tief = dreiecke
            .iter()
            .flatten()
            .map(|p| p.z)
            .fold(f64::INFINITY, f64::min);
        let ringe = m.terrain_boundaries();
        let show = if ringe.is_empty() {
            Show::All
        } else {
            t.show()
        };
        let stufe = m.terrain_contour_step(STUFE);
        let linien: Vec<(f64, Vec<P2>, bool)> = stufe
            .map_or(Vec::new(), |s| m.terrain_contours(s))
            .into_iter()
            .map(|(z, mut p, zu)| {
                // eine geschlossene Linie nennt ihren ersten Punkt nicht
                // noch einmal: ohne ihn fehlte das letzte Stück
                if zu && p.len() > 2 {
                    p.push(p[0]);
                }
                ((z - h0) * 1000.0, p, (z - z.round()).abs() < 1e-6)
            })
            .collect();
        let mut g = Gelaende::roh(dreiecke, ringe, show, tief - m.plate_settings().thick);
        g.am_haus(m);
        // Höhenlinien des Urgeländes nur, wo es bleibt; im Planungsgelände
        // je Dreieck
        let unter_plan = Gitter::neu(ur.iter().map(|t| kasten(&t.map(|p| [p.x, p.y]))).collect());
        let im_plan = |p: P2| {
            let mut nah = Vec::new();
            unter_plan.bei(&[p[0], p[1], p[0], p[1]], &mut nah);
            nah.iter()
                .any(|&i| baryzentrisch(&ur[i as usize], p).is_some())
        };
        // Höhenzahlen an den ganzen Linien, bevor sie in Stücke zerfallen
        let frei = |q: P2| (!g.im_haus(q)).then(|| g.zeigen(g.innen(q))).flatten();
        let urgelaende = |q: P2| frei(q).filter(|_| ur.is_empty() || !im_plan(q));
        let mut zahlen = hoehenzahlen(&linien, h0, urgelaende);
        // Streifen am Haus: in seiner größten Teilfläche, nicht in der
        // Mitte des Umrisses (dort steht das Haus)
        let (streifen, flaechen): (Vec<_>, Vec<_>) = plan
            .outlines
            .iter()
            .zip(plan.strip_marks.iter().chain(std::iter::repeat(&None)))
            .partition(|(_, s)| s.is_some());
        let flaechen: Vec<Vec<Vec3>> = flaechen.into_iter().map(|(r, _)| r.clone()).collect();
        zahlen.extend(flaechenzahlen(&flaechen, h0, frei));
        zahlen.extend(streifen.into_iter().filter_map(|(_, q)| {
            let q = (*q)?;
            let voll = frei([q.x, q.y])?;
            Some(([q.x, q.y], 0.0, q.z, nhn_text(q.z / 1000.0 + h0), voll))
        }));
        let mut linien: Vec<(f64, [P2; 2], bool)> = match glatt_linien {
            Some(l) => l,
            None => linien
                .iter()
                .flat_map(|(z, p, haupt)| p.windows(2).map(move |w| (*z, [w[0], w[1]], *haupt)))
                .filter(|(_, w, _)| ur.is_empty() || !im_plan(lerp2(w[0], w[1], 0.5)))
                .collect(),
        };
        if let Some(s) = stufe {
            linien.extend(hoehenstuecke(&plan.triangles, h0, s));
        }
        // Höhenlinien und Messpunkte einmal je Gelände an Grenze und Haus
        // teilen, nicht je Zoomstufe (B3)
        let mut stuecke = Vec::new();
        for (z, w, haupt) in &linien {
            for (t0, t1, innen) in g.teile(w[0], w[1]) {
                let ab = [lerp2(w[0], w[1], t0), lerp2(w[0], w[1], t1)];
                stuecke.push((*z, ab, innen, *haupt));
            }
        }
        g.stuecke = stuecke;
        g.zahlen = zahlen;
        g.ur_netz = unter_plan;
        g.ur = ur;
        g.schraffen = plan.hatch.clone();
        g.umrisse = plan.outlines.clone();
        g.waende = mit_kantenpunkten(plan.walls.clone(), &zwischen);
        g.rand = Some(rand);
        g.marken = t
            .points
            .iter()
            .filter(|p| !p.off)
            .filter_map(|p| {
                let q = f.to_model(p.e, p.n);
                if g.im_haus(q) {
                    return None;
                }
                let voll = g.zeigen(g.innen(q))?;
                Some((vec3(q[0], q[1], (p.h - h0) * 1000.0), voll))
            })
            .collect();
        // Koten der Messpunkte in m ü. NHN, zwei Nachkommastellen (Plan §8,
        // Bedienbarkeit Gelände)
        if g.marken.len() <= PUNKT_KOTEN {
            g.koten = g
                .marken
                .iter()
                .map(|&(p, voll)| {
                    let text = crate::selection::de(p.z / 1000.0 + h0, 2);
                    ([p.x, p.y], 0.0, p.z, text, voll)
                })
                .collect();
        }
        Some(g)
    }

    /// Gelände aus Dreiecken, Grenzen und Zuschnitt, noch ohne Häuser,
    /// Höhenlinien und Messpunkte; die Dreiecke gleich im Raster.
    fn roh(dreiecke: Vec<[Vec3; 3]>, ringe: Vec<Vec<P2>>, show: Show, unten: f64) -> Gelaende {
        Gelaende {
            netz: Gitter::neu(
                dreiecke
                    .iter()
                    .map(|t| kasten(&t.map(|p| [p.x, p.y])))
                    .collect(),
            ),
            dreiecke,
            ringe,
            show,
            stuecke: Vec::new(),
            marken: Vec::new(),
            haus: Vec::new(),
            haeuser: Gitter::neu(Vec::new()),
            aufbau: Bodenaufbau::default(),
            unten,
            unterlage: Vec::new(),
            ur: Vec::new(),
            ur_netz: Gitter::neu(Vec::new()),
            schraffen: Vec::new(),
            zahlen: Vec::new(),
            koten: Vec::new(),
            umrisse: Vec::new(),
            waende: Vec::new(),
            rand: None,
        }
    }

    /// Ebenes Gelände in Höhe OK Gelände: das Rechteck um die Grenzen mit
    /// dem Abstand der Platte, zugeschnitten wie das Netz. `None` ohne
    /// Grenze oder Lage.
    fn eben(m: &Model) -> Option<Gelaende> {
        let t = m.terrain()?;
        let ringe = m.terrain_boundaries();
        if ringe.is_empty() {
            return None;
        }
        let (z, s) = (m.terrain_z(), m.plate_settings());
        let mut k = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for p in ringe.iter().flatten() {
            k = [
                k[0].min(p[0]),
                k[1].min(p[1]),
                k[2].max(p[0]),
                k[3].max(p[1]),
            ];
        }
        let [x0, y0, x1, y1] = [
            k[0] - s.margin,
            k[1] - s.margin,
            k[2] + s.margin,
            k[3] + s.margin,
        ];
        let e = [
            vec3(x0, y0, z),
            vec3(x1, y0, z),
            vec3(x1, y1, z),
            vec3(x0, y1, z),
        ];
        let d = vec![[e[0], e[1], e[2]], [e[0], e[2], e[3]]];
        let mut g = Gelaende::roh(d, ringe, t.show(), z - s.thick);
        g.am_haus(m);
        Some(g)
    }

    /// Umrisse der Häuser, ihr Raster und der Bodenaufbau unter ihnen.
    fn am_haus(&mut self, m: &Model) {
        // Unterkante der Gründung je Gebäude, einmal gesammelt
        let mut uk: HashMap<BuildingId, f64> = HashMap::new();
        for (_, s) in m.storeys().iter() {
            if let (LevelKind::Foundation, Some(b)) = (s.kind, s.building) {
                let x = uk.entry(b).or_insert(0.0);
                *x = x.min(s.elevation);
            }
        }
        let uk = |b: BuildingId| uk.get(&b).copied().unwrap_or(0.0);
        let flach = |r: &[Vec3]| r.iter().map(|p| [p.x, p.y]).collect::<Vec<P2>>();
        self.haus = m
            .plate_outlines()
            .into_iter()
            .map(|(b, r)| (flach(&r), uk(b)))
            .collect();
        self.haeuser = Gitter::neu(self.haus.iter().map(|(r, _)| kasten(r)).collect());
        self.aufbau = Bodenaufbau::aus(m);
        self.unten = self.aufbau.unten(self.unten);
    }

    /// Linien des Lageplans (Modell, mm) auf das Gelände legen, in
    /// Kacheln über dem Bereich des Plans ([`KACHEL`]).
    pub(crate) fn mit_unterlage(mut self, l: &[(Vec<P2>, bool, UnderlayKind)]) -> Gelaende {
        // außerhalb des Netzes auf seiner mittleren Höhe
        let n = (self.dreiecke.len() * 3).max(1) as f64;
        let mittel = self.dreiecke.iter().flatten().map(|p| p.z).sum::<f64>() / n;
        let mut strecken = Vec::new();
        for (p, zu, art) in l {
            let k = p.len();
            let m = if *zu && k > 2 { k } else { k.saturating_sub(1) };
            for i in 0..m {
                strecken.push((p[i], p[(i + 1) % k], *art == UnderlayKind::Boundary));
            }
        }
        let mitte: Vec<P2> = strecken.iter().map(|s| lerp2(s.0, s.1, 0.5)).collect();
        let mut r = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for q in &mitte {
            r = [
                r[0].min(q[0]),
                r[1].min(q[1]),
                r[2].max(q[0]),
                r[3].max(q[1]),
            ];
        }
        let mut ix = Vec::new();
        kacheln(&mitte, (0..strecken.len()).collect(), r, 0, &mut ix);
        // im Haus liegt nichts: jede Strecke einzeln, am Haus geteilt; die
        // Stücke bleiben in der Kachel ihrer Strecke
        let u: Vec<Vec<([Vec3; 2], bool)>> = ix
            .iter()
            .map(|k| {
                let mut v = Vec::with_capacity(k.len());
                for &i in k {
                    let (a, b, grenze) = strecken[i];
                    for (t0, t1, _) in self.teile(a, b) {
                        let q = [lerp2(a, b, t0), lerp2(a, b, t1)]
                            .map(|p| vec3(p[0], p[1], self.hoehe(p).unwrap_or(mittel)));
                        v.push((q, grenze));
                    }
                }
                v
            })
            .collect();
        self.unterlage = u
            .into_iter()
            .enumerate()
            .filter(|(_, s)| !s.is_empty())
            .map(|(i, striche)| {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                std::hash::Hash::hash(&i, &mut h);
                for ([a, b], grenze) in &striche {
                    std::hash::Hash::hash(
                        &([a.x, a.y, a.z, b.x, b.y, b.z].map(f64::to_bits), grenze),
                        &mut h,
                    );
                }
                Kachel {
                    key: std::hash::Hasher::finish(&h),
                    striche,
                }
            })
            .collect();
        self
    }

    /// Tiefste und höchste Höhe der Dreiecke (mm), `None` ohne Dreiecke.
    pub(crate) fn hoehen(&self) -> Option<(f64, f64)> {
        let mut z = self.dreiecke.iter().flatten().map(|p| p.z);
        let a = z.next()?;
        Some(z.fold((a, a), |(lo, hi), z| (lo.min(z), hi.max(z))))
    }

    /// Quader um das Netz bis zur Unterkante des Rands.
    #[cfg_attr(not(all(test, target_os = "linux")), allow(dead_code))] // Prüfbilder
    pub(crate) fn rahmen(&self) -> (Vec3, Vec3) {
        let mut lo = vec3(f64::MAX, f64::MAX, self.unten);
        let mut hi = vec3(f64::MIN, f64::MIN, self.unten);
        for p in self.dreiecke.iter().flatten() {
            lo = vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z);
            hi = vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        (lo, hi)
    }

    /// Liegt `p` im Grundstück (ohne Grenze: überall)?
    fn innen(&self, p: P2) -> bool {
        self.ringe.is_empty() || self.ringe.iter().any(|r| im_ring(r, p))
    }

    /// Liegt `p` in einem Haus?
    fn im_haus(&self, p: P2) -> bool {
        let mut nah = Vec::new();
        self.haeuser.bei(&[p[0], p[1], p[0], p[1]], &mut nah);
        self.in_einem(&nah, p)
    }

    /// Kreuzt die Grundstücksgrenze das Rechteck `k`: liegen seine Ecken
    /// auf beiden Seiten, oder eine Ecke der Grenze darin? (Eine Gerade
    /// durch ein Rechteck lässt auf jeder Seite eine Ecke.)
    fn grenze_trifft(&self, k: &Kasten) -> bool {
        if self.ringe.is_empty() {
            return false;
        }
        let [x0, y0, x1, y1] = *k;
        let innen = [[x0, y0], [x1, y0], [x0, y1], [x1, y1]].map(|p| self.innen(p));
        let im = |p: &P2| p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1;
        innen.iter().any(|&i| i != innen[0]) || self.ringe.iter().flatten().any(im)
    }

    /// Berührt das Rechteck `k` ein Haus: eine Ecke des Hauses darin oder
    /// ein Punkt seines Rands oder seiner Mitte im Haus?
    fn haus_trifft(&self, k: &Kasten) -> bool {
        let mut nah = Vec::new();
        self.haeuser.bei(k, &mut nah);
        if nah.is_empty() {
            return false;
        }
        let [x0, y0, x1, y1] = *k;
        let (xm, ym) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let proben = [
            [x0, y0],
            [xm, y0],
            [x1, y0],
            [x0, ym],
            [xm, ym],
            [x1, ym],
            [x0, y1],
            [xm, y1],
            [x1, y1],
        ];
        let innen = |p: &P2| p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1;
        proben.iter().any(|&p| self.in_einem(&nah, p))
            || nah
                .iter()
                .any(|&i| self.haus[i as usize].0.iter().any(innen))
    }

    /// Liegt `p` in einem der Häuser `nah`?
    fn in_einem(&self, nah: &[u32], p: P2) -> bool {
        nah.iter().any(|&h| im_ring(&self.haus[h as usize].0, p))
    }

    /// Teile der Strecke `a`→`b` an Grenze und Haus: Parameter von, bis
    /// und ob der Teil im Grundstück liegt; Teile im Haus fallen weg. Nur
    /// die Häuser am Rechteck der Strecke zählen.
    fn teile(&self, a: P2, b: P2) -> Vec<(f64, f64, bool)> {
        let mut nah = Vec::new();
        self.haeuser.bei(&kasten(&[a, b]), &mut nah);
        let mut ts = vec![0.0, 1.0];
        let haeuser = nah.iter().map(|&h| &self.haus[h as usize].0);
        for r in self.ringe.iter().chain(haeuser) {
            ts.extend(schnitte(r, a, b));
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        ts.windows(2)
            .filter_map(|w| {
                let m = lerp2(a, b, (w[0] + w[1]) / 2.0);
                (!self.in_einem(&nah, m)).then(|| (w[0], w[1], self.innen(m)))
            })
            .collect()
    }

    /// Die Dreiecke ohne die Häuser: Ein Dreieck, das ein Haus berührt,
    /// wird an den Geraden der Hauskanten in konvexe Stücke geteilt, die
    /// Stücke im Haus fallen weg. Je Dreieck zählen nur die Häuser an
    /// seinem Rechteck. Wo eine Gerade eine Kante zu einem Dreieck kreuzt,
    /// das sie nicht oder anders teilt, bekommt dieses den Punkt auch,
    /// sonst stünde dort eine offene Kante (Gelände G-261e, im Export des
    /// Lindenhofs 24).
    fn ohne_haus(&self, d: Vec<[Vec3; 3]>) -> Vec<[Vec3; 3]> {
        if self.haus.is_empty() {
            return d;
        }
        let mut nah = Vec::new();
        let mut out = Vec::with_capacity(d.len());
        // je geteiltes Dreieck: Nummer, seine Dreiecke in `out`, Stücke
        let mut geteilt: Vec<(usize, std::ops::Range<usize>, Vec<Vec<P2>>)> = Vec::new();
        // je Kante die Ecken der Stücke innen auf ihr, je Dreieck
        let mut auf: HashMap<[u64; 4], Vec<(usize, Vec<P2>)>> = HashMap::new();
        for (j, t) in d.iter().enumerate() {
            let Some(stuecke) = self.stuecke_ohne_haus(t, &mut nah) else {
                out.push(*t);
                continue;
            };
            let ecken = t.map(|p| [p.x, p.y]);
            let seiten = seiten(&ecken);
            let mut je: [Vec<P2>; 3] = Default::default();
            for (v, g) in je.iter_mut().zip(&seiten) {
                v.extend(stuecke.iter().flatten().filter(|q| g.innen(**q)));
            }
            for (i, mut v) in je.into_iter().enumerate() {
                if v.is_empty() {
                    continue;
                }
                v.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
                v.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9);
                let k = kante_von(ecken[i], ecken[(i + 1) % 3]);
                auf.entry(k).or_default().push((j, v));
            }
            let von = out.len();
            for s in &stuecke {
                faechern(t, s, false, &mut out);
            }
            geteilt.push((j, von..out.len(), stuecke));
        }
        // wo beide Dreiecke einer Kante sie gleich teilen, ist nichts zu tun
        let gleich = |a: &[P2], b: &[P2]| {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(p, q)| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-6)
        };
        let auf: HashMap<[u64; 4], Vec<P2>> = auf
            .into_iter()
            .filter(|(_, v)| !(v.len() == 2 && gleich(&v[0].1, &v[1].1)))
            .map(|(k, v)| (k, v.into_iter().flat_map(|x| x.1).collect()))
            .collect();
        if auf.is_empty() {
            return out;
        }
        // die Dreiecke daneben neu, mit den Punkten auf ihren Kanten: die
        // geteilten aus ihren Stücken, die übrigen über das Raster der Kanten
        let kanten: Vec<[u64; 4]> = auf.keys().copied().collect();
        let raster = Gitter::neu(
            kanten
                .iter()
                .map(|k| {
                    let [x0, y0, x1, y1] = k.map(f64::from_bits);
                    kasten(&[[x0, y0], [x1, y1]])
                })
                .collect(),
        );
        let mut neu = Vec::with_capacity(out.len());
        let mut geteilt = geteilt.into_iter().peekable();
        let mut i = 0;
        let mut treffer = Vec::new();
        for (j, t) in d.iter().enumerate() {
            let ecken = t.map(|p| [p.x, p.y]);
            let eigene = [0, 1, 2].map(|k| kante_von(ecken[k], ecken[(k + 1) % 3]));
            let (alt, stuecke) = match geteilt.next_if(|x| x.0 == j) {
                Some((_, r, s)) => (r, Some(s)),
                None => (i..i + 1, None),
            };
            i = alt.end;
            let fehlt = if stuecke.is_none() {
                raster.bei(&kasten(&ecken), &mut treffer);
                treffer
                    .iter()
                    .any(|&k| eigene.contains(&kanten[k as usize]))
            } else {
                eigene.iter().any(|k| auf.contains_key(k))
            };
            if !fehlt {
                neu.extend_from_slice(&out[alt]);
                continue;
            }
            let seiten = seiten(&ecken);
            let Some(stuecke) = stuecke else {
                let (s, mehr) = mit_punkten(ecken.to_vec(), &seiten, &eigene, &auf);
                if mehr {
                    faechern(t, &s, true, &mut neu);
                } else {
                    neu.push(*t);
                }
                continue;
            };
            for s in stuecke {
                let (s, mehr) = mit_punkten(s, &seiten, &eigene, &auf);
                faechern(t, &s, mehr, &mut neu);
            }
        }
        neu
    }

    /// Die Stücke des Dreiecks `t` an den Geraden der Kanten der Häuser an
    /// seinem Rechteck, ohne die im Haus; `None` ohne Haus daran.
    fn stuecke_ohne_haus(&self, t: &[Vec3; 3], nah: &mut Vec<u32>) -> Option<Vec<Vec<P2>>> {
        let ecken = t.map(|p| [p.x, p.y]);
        self.haeuser.bei(&kasten(&ecken), nah);
        if nah.is_empty() {
            return None;
        }
        let mut stuecke = vec![ecken.to_vec()];
        for &h in nah.iter() {
            let r = &self.haus[h as usize].0;
            for i in 0..r.len() {
                let (c, e) = (r[i], r[(i + 1) % r.len()]);
                let k = kasten(&[c, e]);
                stuecke = stuecke
                    .into_iter()
                    .flat_map(|s| {
                        if ueberlappt(&k, &kasten(&s)) {
                            halbieren(s, c, e)
                        } else {
                            vec![s]
                        }
                    })
                    .collect();
            }
        }
        stuecke.retain(|s| {
            let m = s.iter().fold([0.0, 0.0], |m, p| [m[0] + p[0], m[1] + p[1]]);
            let k = s.len() as f64;
            !self.in_einem(nah, [m[0] / k, m[1] / k])
        });
        Some(stuecke)
    }

    /// Höhe des Netzes an `p`; `None` außerhalb. Gefragt werden nur die
    /// Dreiecke an `p` (mit einem Hauch Rand für die Toleranz).
    fn hoehe(&self, p: P2) -> Option<f64> {
        let mut nah = Vec::new();
        let e = 1e-3;
        self.netz
            .bei(&[p[0] - e, p[1] - e, p[0] + e, p[1] + e], &mut nah);
        nah.iter().find_map(|&i| {
            let t = &self.dreiecke[i as usize];
            let l = baryzentrisch(t, p)?;
            Some(l[0] * t[0].z + l[1] * t[1].z + l[2] * t[2].z)
        })
    }

    /// Die Höhe an `p` auf der Seite zu `q` hin, wo das Netz an `p`
    /// springt (Wand einer Geländefläche über die Hauskante), sonst
    /// `None`. `p`→`q` liegt in einem Dreieck (zwischen zwei Teilstellen).
    fn sprung(&self, p: P2, q: P2) -> Option<f64> {
        if self.waende.is_empty() {
            return None;
        }
        let z = self.hoehe(p)?;
        let m = lerp2(p, q, 0.5);
        let mut nah = Vec::new();
        let e = 1e-3;
        self.netz
            .bei(&[m[0] - e, m[1] - e, m[0] + e, m[1] + e], &mut nah);
        let t = nah
            .iter()
            .map(|&i| &self.dreiecke[i as usize])
            .find(|t| baryzentrisch(t, m).is_some())?;
        let s = ebene(t, p);
        ((s - z).abs() > 1e-6).then_some(s)
    }

    /// Die Wände der Geländeflächen ohne die Häuser (dort ist auch kein
    /// Gelände): Eine Wand über eine Hauskante endet senkrecht auf ihr,
    /// die Teile im Haus fallen weg (Geometrie H-276-geo-a: vorher 4 bis 20
    /// offene Kanten im Export, wo die Böschung einer Fläche ins Haus
    /// reicht). Dazu die Stellen auf den Hauskanten, an denen eine Wand
    /// endet: Dort springt das Gelände, die Grube hat je Seite eine Höhe.
    fn waende_ohne_haus(&self) -> (Vec<[Vec3; 3]>, Vec<P2>) {
        let (mut out, mut stellen) = (Vec::with_capacity(self.waende.len()), Vec::new());
        let mut nah = Vec::new();
        for t in &self.waende {
            // senkrecht: im Grundriss die Strecke a→b
            let [a, b] = fusslinie(t);
            self.haeuser.bei(&kasten(&[a, b]), &mut nah);
            if nah.is_empty() {
                out.push(*t);
                continue;
            }
            let mut ts = vec![0.0, 1.0];
            for &h in &nah {
                ts.extend(schnitte(&self.haus[h as usize].0, a, b));
            }
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            let g = Gerade::durch(a, b);
            let von = out.len();
            for w in ts.windows(2) {
                if self.in_einem(&nah, lerp2(a, b, (w[0] + w[1]) / 2.0)) {
                    continue;
                }
                let mut v = t.to_vec();
                if w[0] > 0.0 {
                    v = abschneiden(&v, |p| g.anteil([p.x, p.y]) - w[0]);
                }
                if w[1] < 1.0 {
                    v = abschneiden(&v, |p| w[1] - g.anteil([p.x, p.y]));
                }
                for i in 1..v.len().saturating_sub(1) {
                    out.push([v[0], v[i], v[i + 1]]);
                }
            }
            let auf_haus = |p: P2| {
                nah.iter().any(|&h| {
                    let r = &self.haus[h as usize].0;
                    (0..r.len()).any(|i| Gerade::durch(r[i], r[(i + 1) % r.len()]).auf(p))
                })
            };
            for p in out[von..].iter().flatten() {
                let q = [p.x, p.y];
                if auf_haus(q) {
                    stellen.push(q);
                }
            }
        }
        (out, stellen)
    }

    /// Teilstellen der Strecke `a`→`b` (0 bis 1, mit den Enden) an jeder
    /// Netzkante, damit ein Rand genau auf dem Netz liegt.
    pub(crate) fn teilung(&self, a: P2, b: P2) -> Vec<f64> {
        self.teilung_mit(a, b, &mut Vec::new())
    }

    fn teilung_mit(&self, a: P2, b: P2, nah: &mut Vec<u32>) -> Vec<f64> {
        let mut ts = vec![0.0, 1.0];
        // nur die Dreiecke am Rechteck der Kante (Review #129 B4)
        self.netz.bei(&kasten(&[a, b]), nah);
        for t in nah.iter().map(|&j| &self.dreiecke[j as usize]) {
            for k in 0..3 {
                let (c, d) = (t[k], t[(k + 1) % 3]);
                if let Some(x) = kreuzung(a, b, [c.x, c.y], [d.x, d.y]) {
                    ts.push(x);
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        ts
    }

    /// Wie die Strecke `a`→`b` gezeigt wird: im Grundstück voll, außerhalb
    /// blass (`Some(false)`) oder gar nicht (`None`).
    fn zeigen(&self, innen: bool) -> Option<bool> {
        match (self.show, innen) {
            (_, true) | (Show::All, false) => Some(true),
            (Show::Fade, false) => Some(false),
            (Show::Parcel, false) => None,
        }
    }

    /// Profil des Netzes in der senkrechten Ebene durch `p0` mit Normale
    /// `n`: Strecken (Lauflänge, Höhe) je Dreieck, an der Grenze geteilt,
    /// mit Sichtbarkeit wie [`Gelaende::zeigen`].
    fn profil(&self, f: &SectionFrame, p0: Vec3) -> Vec<([f64; 2], [f64; 2], bool)> {
        let mut p = self.profil_von(&self.dreiecke, Some(&self.netz), f, p0);
        p.extend(self.profil_von(&self.waende, None, f, p0));
        p
    }

    /// [`Gelaende::profil`] der Dreiecke `d`; mit ihrem Raster `gitter`
    /// nur die an der Linie (Review #183 B1).
    fn profil_von(
        &self,
        d: &[[Vec3; 3]],
        gitter: Option<&Gitter>,
        f: &SectionFrame,
        p0: Vec3,
    ) -> Vec<([f64; 2], [f64; 2], bool)> {
        let n = f.n;
        let mut out = Vec::new();
        let mut nah = Vec::new();
        let auswahl: Box<dyn Iterator<Item = &[Vec3; 3]>> = match gitter {
            Some(g) => {
                g.entlang([p0.x, p0.y], [n.x, n.y], &mut nah);
                Box::new(nah.iter().map(|&i| &d[i as usize]))
            }
            None => Box::new(d.iter()),
        };
        for t in auswahl {
            let d = t.map(|p| (p - p0).dot(n));
            let mut q: Vec<Vec3> = Vec::new();
            for i in 0..3 {
                let (a, b) = (i, (i + 1) % 3);
                if (d[a] < 0.0) != (d[b] < 0.0) {
                    let s = d[a] / (d[a] - d[b]);
                    q.push(t[a] + (t[b] - t[a]) * s);
                }
            }
            let [a, b] = match q[..] {
                [a, b] => [a, b],
                _ => continue,
            };
            for (t0, t1, innen) in self.teile([a.x, a.y], [b.x, b.y]) {
                let Some(voll) = self.zeigen(innen) else {
                    continue;
                };
                let (pa, pb) = (a + (b - a) * t0, a + (b - a) * t1);
                let u = |p: Vec3| (p - f.pt(0.0, 0.0)).dot(f.along);
                out.push(([u(pa), pa.z], [u(pb), pb.z], voll));
            }
        }
        out
    }
}

/// Abstand zweier Punkte im Grundriss.
fn abstand(a: P2, b: P2) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn lerp2(a: P2, b: P2, t: f64) -> P2 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Höhenlinien im Abstand `stufe` (m) über den Dreiecken `d` (mm, z zu
/// ±0,00 auf `h0` m ü. NHN), je Dreieck ein Stück: Höhe (mm), Anfang und
/// Ende, Hauptlinie (volle Meter). Eine Ecke genau auf der Höhe zählt
/// oben, so hat jedes Dreieck je Höhe kein oder ein Stück.
fn hoehenstuecke(d: &[[Vec3; 3]], h0: f64, stufe: f64) -> Vec<(f64, [P2; 2], bool)> {
    let mut out = Vec::new();
    if !(stufe.is_finite() && stufe > 0.0) {
        return out;
    }
    for t in d {
        let h = t.map(|p| h0 + p.z / 1000.0);
        let (lo, hi) = (h[0].min(h[1]).min(h[2]), h[0].max(h[1]).max(h[2]));
        let (k0, k1) = ((lo / stufe).ceil() as i64, (hi / stufe).floor() as i64);
        for k in k0..=k1 {
            let l = k as f64 * stufe;
            let mut q = Vec::with_capacity(2);
            for i in 0..3 {
                let (a, b) = (i, (i + 1) % 3);
                if (h[a] >= l) != (h[b] >= l) {
                    let s = (l - h[a]) / (h[b] - h[a]);
                    q.push(lerp2([t[a].x, t[a].y], [t[b].x, t[b].y], s));
                }
            }
            if let [a, b] = q[..] {
                out.push(((l - h0) * 1000.0, [a, b], (l - l.round()).abs() < 1e-6));
            }
        }
    }
    out
}

/// Parameter auf `a`→`b`, wo sie den Ring `r` kreuzt.
fn schnitte(r: &[P2], a: P2, b: P2) -> impl Iterator<Item = f64> + '_ {
    (0..r.len()).filter_map(move |i| kreuzung(a, b, r[i], r[(i + 1) % r.len()]))
}

/// Rechteck um die Punkte: x, y von, x, y bis.
fn kasten(p: &[P2]) -> Kasten {
    p.iter()
        .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |k, q| {
            [
                k[0].min(q[0]),
                k[1].min(q[1]),
                k[2].max(q[0]),
                k[3].max(q[1]),
            ]
        })
}

/// Konvexes Vieleck `s` an der Geraden durch `c`→`e` geteilt (beide Teile
/// im gleichen Umlaufsinn); liegt es ganz auf einer Seite, bleibt es.
fn halbieren(s: Vec<P2>, c: P2, e: P2) -> Vec<Vec<P2>> {
    let l = (e[0] - c[0]).hypot(e[1] - c[1]);
    if l < 1e-9 {
        return vec![s];
    }
    let seite = |p: P2| {
        let d = ((e[0] - c[0]) * (p[1] - c[1]) - (e[1] - c[1]) * (p[0] - c[0])) / l;
        if d.abs() < 1e-6 {
            0.0
        } else {
            d
        }
    };
    let d: Vec<f64> = s.iter().map(|p| seite(*p)).collect();
    if d.iter().all(|x| *x >= 0.0) || d.iter().all(|x| *x <= 0.0) {
        return vec![s];
    }
    let (mut links, mut rechts) = (Vec::new(), Vec::new());
    for i in 0..s.len() {
        let j = (i + 1) % s.len();
        if d[i] >= 0.0 {
            links.push(s[i]);
        }
        if d[i] <= 0.0 {
            rechts.push(s[i]);
        }
        if d[i] * d[j] < 0.0 {
            let x = lerp2(s[i], s[j], d[i] / (d[i] - d[j]));
            links.push(x);
            rechts.push(x);
        }
    }
    [links, rechts]
        .into_iter()
        .filter(|v| v.len() >= 3)
        .collect()
}

/// Punkt im Ring (gerade-ungerade, Umlaufsinn beliebig).
fn im_ring(r: &[P2], p: P2) -> bool {
    let mut innen = false;
    let n = r.len();
    for i in 0..n {
        let (a, b) = (r[i], r[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1])
        {
            innen = !innen;
        }
    }
    innen
}

/// Parameter auf `a`→`b`, wo die Strecke `c`→`d` sie kreuzt (echt innen).
fn kreuzung(a: P2, b: P2, c: P2, d: P2) -> Option<f64> {
    let (r, s) = ([b[0] - a[0], b[1] - a[1]], [d[0] - c[0], d[1] - c[1]]);
    let den = r[0] * s[1] - r[1] * s[0];
    if den.abs() < 1e-12 {
        return None;
    }
    let q = [c[0] - a[0], c[1] - a[1]];
    let t = (q[0] * s[1] - q[1] * s[0]) / den;
    let u = (q[0] * r[1] - q[1] * r[0]) / den;
    (t > 1e-9 && t < 1.0 - 1e-9 && (0.0..=1.0).contains(&u)).then_some(t)
}

/// Baryzentrische Koordinaten von `p` im Dreieck (Grundriss); `None`
/// außerhalb.
fn baryzentrisch(t: &[Vec3; 3], p: P2) -> Option<[f64; 3]> {
    let d = (t[1].x - t[0].x) * (t[2].y - t[0].y) - (t[1].y - t[0].y) * (t[2].x - t[0].x);
    if d.abs() < 1e-12 {
        return None;
    }
    let l1 = ((t[1].x - p[0]) * (t[2].y - p[1]) - (t[1].y - p[1]) * (t[2].x - p[0])) / d;
    let l2 = ((t[2].x - p[0]) * (t[0].y - p[1]) - (t[2].y - p[1]) * (t[0].x - p[0])) / d;
    let l3 = 1.0 - l1 - l2;
    let e = -1e-9;
    (l1 >= e && l2 >= e && l3 >= e).then_some([l1, l2, l3])
}

/// Höhe der Ebene durch das Dreieck an `p`.
fn ebene(t: &[Vec3; 3], p: P2) -> f64 {
    let d = (t[1].x - t[0].x) * (t[2].y - t[0].y) - (t[1].y - t[0].y) * (t[2].x - t[0].x);
    let l1 = ((t[1].x - p[0]) * (t[2].y - p[1]) - (t[1].y - p[1]) * (t[2].x - p[0])) / d;
    let l2 = ((t[2].x - p[0]) * (t[0].y - p[1]) - (t[2].y - p[1]) * (t[0].x - p[0])) / d;
    l1 * t[0].z + l2 * t[1].z + (1.0 - l1 - l2) * t[2].z
}

/// Abstand (mm), bis zu dem ein Punkt auf einer Kante liegt.
const AUF_DER_KANTE: f64 = 1e-4;

/// Anteil von `q` auf der Strecke `a`→`b` im Raum, wenn er echt innen
/// darauf liegt.
fn auf_strecke(a: Vec3, b: Vec3, q: Vec3) -> Option<f64> {
    let d = b - a;
    let l2 = d.dot(d);
    if l2 <= 0.0 {
        return None;
    }
    let s = (q - a).dot(d) / l2;
    let innen = s * l2.sqrt() > AUF_DER_KANTE && (1.0 - s) * l2.sqrt() > AUF_DER_KANTE;
    (innen && (a + d * s - q).length() < AUF_DER_KANTE).then_some(s)
}

/// Die Punkte der Glättung `glatt` (je Dreieck `je`) auf den Kanten der
/// Dreiecke `roh` mit `fest` und auf den Kanten des Rands `rand`, ohne
/// deren Ecken.
fn kantenpunkte(
    roh: &[[Vec3; 3]],
    glatt: &[[Vec3; 3]],
    fest: &[bool],
    rand: &[[Vec3; 2]],
    je: usize,
) -> Vec<Vec3> {
    let schluessel = |a: Vec3, b: Vec3| {
        let (a, b) = ([a.x, a.y].map(f64::to_bits), [b.x, b.y].map(f64::to_bits));
        if a <= b {
            [a, b]
        } else {
            [b, a]
        }
    };
    let am_rand: HashSet<_> = rand.iter().map(|k| schluessel(k[0], k[1])).collect();
    let mut out = Vec::new();
    for (i, t) in roh.iter().enumerate() {
        let fest = fest.get(i).copied().unwrap_or(false);
        let kanten: Vec<[Vec3; 2]> = (0..3)
            .map(|k| [t[k], t[(k + 1) % 3]])
            .filter(|k| fest || am_rand.contains(&schluessel(k[0], k[1])))
            .collect();
        let Some(teile) = glatt
            .get(i * je..(i + 1) * je)
            .filter(|_| !kanten.is_empty())
        else {
            continue;
        };
        out.extend(teile.iter().flatten().filter(|q: &&Vec3| {
            kanten
                .iter()
                .any(|k| auf_strecke(k[0], k[1], **q).is_some())
        }));
    }
    out.sort_by(|a: &Vec3, b: &Vec3| {
        a.x.total_cmp(&b.x)
            .then(a.y.total_cmp(&b.y))
            .then(a.z.total_cmp(&b.z))
    });
    out.dedup();
    out
}

/// Raster über die Punkte `p`.
fn punktraster(p: &[Vec3]) -> Gitter {
    let e = AUF_DER_KANTE;
    Gitter::neu(
        p.iter()
            .map(|q| [q.x - e, q.y - e, q.x + e, q.y + e])
            .collect(),
    )
}

/// Die Punkte `nah` aus `p`, die echt innen auf `a`→`b` liegen, der Reihe
/// nach.
fn auf_der_strecke(a: Vec3, b: Vec3, p: &[Vec3], nah: &[u32]) -> Vec<Vec3> {
    let mut auf: Vec<(f64, Vec3)> = nah
        .iter()
        .map(|&j| p[j as usize])
        .filter_map(|q| Some((auf_strecke(a, b, q)?, q)))
        .collect();
    auf.sort_by(|x, y| x.0.total_cmp(&y.0));
    auf.into_iter().map(|x| x.1).collect()
}

/// Die Strecken `s`, an den Punkten `p` darauf geteilt.
fn strecken_mit_punkten(s: Vec<(Vec3, Vec3)>, p: &[Vec3]) -> Vec<(Vec3, Vec3)> {
    if p.is_empty() {
        return s;
    }
    let raster = punktraster(p);
    let mut nah = Vec::new();
    let mut out = Vec::with_capacity(s.len());
    for (a, b) in s {
        raster.bei(&kasten(&[[a.x, a.y], [b.x, b.y]]), &mut nah);
        let mut von = a;
        for q in auf_der_strecke(a, b, p, &nah) {
            out.push((von, q));
            von = q;
        }
        out.push((von, b));
    }
    out
}

/// Die Dreiecke `d` mit den Punkten `p`, die auf ihren Kanten liegen,
/// aus der Mitte gefächert; ohne solche bleibt ein Dreieck, wie es ist.
fn mit_kantenpunkten(d: Vec<[Vec3; 3]>, p: &[Vec3]) -> Vec<[Vec3; 3]> {
    if p.is_empty() {
        return d;
    }
    let raster = punktraster(p);
    let mut out = Vec::with_capacity(d.len());
    let mut nah = Vec::new();
    for t in d {
        raster.bei(&kasten(&t.map(|q| [q.x, q.y])), &mut nah);
        let mut ring = Vec::with_capacity(3 + nah.len());
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            ring.push(a);
            ring.extend(auf_der_strecke(a, b, p, &nah));
        }
        if ring.len() == 3 {
            out.push(t);
            continue;
        }
        let m = (t[0] + t[1] + t[2]) * (1.0 / 3.0);
        for k in 0..ring.len() {
            out.push([m, ring[k], ring[(k + 1) % ring.len()]]);
        }
    }
    out
}

/// Die beiden Enden einer senkrechten Wand im Grundriss (die Ecken am
/// weitesten auseinander).
fn fusslinie(t: &[Vec3; 3]) -> [P2; 2] {
    let p = t.map(|p| [p.x, p.y]);
    [(0, 1), (1, 2), (2, 0)]
        .into_iter()
        .map(|(i, j)| [p[i], p[j]])
        .max_by(|x, y| abstand(x[0], x[1]).total_cmp(&abstand(y[0], y[1])))
        .unwrap_or([p[0], p[1]])
}

/// Das konvexe Vieleck `v` ohne den Teil mit `f < 0` (Sutherland-Hodgman);
/// Ecken mit `f` nahe 0 bleiben, wie sie sind.
fn abschneiden(v: &[Vec3], f: impl Fn(Vec3) -> f64) -> Vec<Vec3> {
    let d: Vec<f64> = v
        .iter()
        .map(|p| {
            let x = f(*p);
            if x.abs() < 1e-9 {
                0.0
            } else {
                x
            }
        })
        .collect();
    let mut out = Vec::with_capacity(v.len() + 1);
    for i in 0..v.len() {
        let j = (i + 1) % v.len();
        if d[i] >= 0.0 {
            out.push(v[i]);
        }
        if d[i] * d[j] < 0.0 {
            out.push(v[i] + (v[j] - v[i]) * (d[i] / (d[i] - d[j])));
        }
    }
    if out.len() < 3 {
        out.clear();
    }
    out
}

/// Senkrechte Kanten der Dreiecke in `bereiche` an den `stellen` an jeder
/// Höhe teilen, die dort eine Ecke dieser Dreiecke hat: Wo eine Wand einer
/// Geländefläche an einer Hauskante endet, stoßen die Wände der Grube mit
/// den Schichten beider Seiten und die Wand selbst an einer Senkrechten
/// zusammen (H-276-geo-a).
fn senkrecht_teilen(t: &mut Vec<Tri>, bereiche: &[std::ops::Range<usize>], stellen: &[P2]) {
    if stellen.is_empty() {
        return;
    }
    let k = |x: f64, y: f64| [(x * 1e4).round() as i64, (y * 1e4).round() as i64];
    let mut hoehen: HashMap<[i64; 2], Vec<f64>> = stellen
        .iter()
        .map(|p| (k(p[0], p[1]), Vec::new()))
        .collect();
    let drin = |i: usize| bereiche.iter().any(|r| r.contains(&i));
    for (_, d) in t.iter().enumerate().filter(|(i, _)| drin(*i)) {
        for p in &d.p {
            if let Some(z) = hoehen.get_mut(&k(p.x, p.y)) {
                z.push(p.z);
            }
        }
    }
    for z in hoehen.values_mut() {
        z.sort_by(f64::total_cmp);
        z.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    }
    let mut neu = Vec::with_capacity(t.len());
    for (i, d) in t.drain(..).enumerate() {
        let kante = (0..3).find(|&j| {
            let (a, b) = (d.p[j], d.p[(j + 1) % 3]);
            k(a.x, a.y) == k(b.x, b.y)
        });
        let zs = kante
            .filter(|_| drin(i))
            .and_then(|j| Some((j, hoehen.get(&{ k(d.p[j].x, d.p[j].y) })?)));
        let Some((j, zs)) = zs else {
            neu.push(d);
            continue;
        };
        let (a, b, c) = (d.p[j], d.p[(j + 1) % 3], d.p[(j + 2) % 3]);
        let (lo, hi) = (a.z.min(b.z), a.z.max(b.z));
        let mut q: Vec<Vec3> = zs
            .iter()
            .filter(|&&z| z > lo + 1e-6 && z < hi - 1e-6)
            .map(|&z| vec3(a.x, a.y, z))
            .collect();
        if q.is_empty() {
            neu.push(d);
            continue;
        }
        if a.z > b.z {
            q.reverse();
        }
        q.insert(0, a);
        q.push(b);
        for w in q.windows(2) {
            neu.push(Tri {
                p: [w[0], w[1], c],
                ..d
            });
        }
    }
    *t = neu;
}

/// Strecke `a`→`b` zum schnellen Prüfen vieler Punkte: Abstand über die
/// Normale, Anteil über die Richtung.
struct Gerade {
    a: P2,
    f: P2,
    l2: f64,
    n: P2,
}

impl Gerade {
    fn durch(a: P2, b: P2) -> Gerade {
        let f = [b[0] - a[0], b[1] - a[1]];
        let l2 = f[0] * f[0] + f[1] * f[1];
        let l = l2.sqrt().max(f64::MIN_POSITIVE);
        Gerade {
            a,
            f,
            l2,
            n: [-f[1] / l, f[0] / l],
        }
    }

    fn anteil(&self, q: P2) -> f64 {
        ((q[0] - self.a[0]) * self.f[0] + (q[1] - self.a[1]) * self.f[1]) / self.l2
    }

    fn innen(&self, q: P2) -> bool {
        let s = self.anteil(q);
        self.auf_linie(q) && s > 1e-9 && s < 1.0 - 1e-9
    }

    /// Auf der Strecke, die Enden eingeschlossen.
    fn auf(&self, q: P2) -> bool {
        let s = self.anteil(q);
        self.auf_linie(q) && s > -1e-9 && s < 1.0 + 1e-9
    }

    fn auf_linie(&self, q: P2) -> bool {
        let quer = (q[0] - self.a[0]) * self.n[0] + (q[1] - self.a[1]) * self.n[1];
        self.l2 > 0.0 && quer.abs() <= 1e-6
    }
}

/// Das konvexe Stück `s` des Dreiecks `t` auf dessen Ebene in Dreiecke
/// nach `out`; `mitte`: aus der Mitte gefächert, weil Punkte auf seinen
/// Seiten liegen (kein Dreieck liegt dann flach auf einer Seite), sonst
/// von der ersten Ecke.
fn faechern(t: &[Vec3; 3], s: &[P2], mitte: bool, out: &mut Vec<[Vec3; 3]>) {
    let p: Vec<Vec3> = s.iter().map(|q| vec3(q[0], q[1], ebene(t, *q))).collect();
    let mut dazu = |d: [Vec3; 3]| {
        if (d[1] - d[0]).cross(d[2] - d[0]).z > 1e-3 {
            out.push(d);
        }
    };
    if mitte {
        let k = p.len() as f64;
        let m = p
            .iter()
            .fold(vec3(0.0, 0.0, 0.0), |m, q| m + *q * (1.0 / k));
        for i in 0..p.len() {
            dazu([m, p[i], p[(i + 1) % p.len()]]);
        }
    } else {
        for i in 1..p.len().saturating_sub(1) {
            dazu([p[0], p[i], p[i + 1]]);
        }
    }
}

/// Schlüssel einer Kante, in beiden Richtungen gleich.
fn kante_von(a: P2, b: P2) -> [u64; 4] {
    let (a, b) = if a.map(f64::to_bits) <= b.map(f64::to_bits) {
        (a, b)
    } else {
        (b, a)
    };
    [a[0], a[1], b[0], b[1]].map(f64::to_bits)
}

/// Die drei Seiten des Dreiecks mit den Ecken `e`, Seite `i` von Ecke `i`.
fn seiten(e: &[P2; 3]) -> [Gerade; 3] {
    [0, 1, 2].map(|i| Gerade::durch(e[i], e[(i + 1) % 3]))
}

/// Das Stück `s` eines Dreiecks mit den Seiten `seiten` (Schlüssel
/// `kanten`), auf jeder Seite des Stücks, die auf einer Seite des Dreiecks
/// liegt, mit den Punkten `auf` dieser Kante dazwischen; `true`, wenn
/// welche dazukamen.
fn mit_punkten(
    s: Vec<P2>,
    seiten: &[Gerade; 3],
    kanten: &[[u64; 4]; 3],
    auf: &HashMap<[u64; 4], Vec<P2>>,
) -> (Vec<P2>, bool) {
    let mut neu = Vec::with_capacity(s.len());
    let mut mehr = false;
    for k in 0..s.len() {
        let (a, b) = (s[k], s[(k + 1) % s.len()]);
        neu.push(a);
        let Some(punkte) = (0..3)
            .find(|&i| seiten[i].auf(a) && seiten[i].auf(b))
            .and_then(|i| auf.get(&kanten[i]))
        else {
            continue;
        };
        let g = Gerade::durch(a, b);
        let mut ts: Vec<(f64, P2)> = punkte
            .iter()
            .filter(|q| g.innen(**q))
            .map(|q| (g.anteil(*q), *q))
            .collect();
        ts.sort_by(|x, y| x.0.total_cmp(&y.0));
        ts.dedup_by(|x, y| (x.0 - y.0).abs() < 1e-12);
        mehr |= !ts.is_empty();
        neu.extend(ts.into_iter().map(|x| x.1));
    }
    (neu, mehr)
}

/// Ring `r` mit dem Dreieck `t` (gegen den Uhrzeigersinn) geschnitten
/// (Sutherland-Hodgman, das Dreieck ist das konvexe Fenster).
fn im_dreieck(r: &[P2], t: &[Vec3; 3]) -> Vec<P2> {
    let mut v: Vec<P2> = r.to_vec();
    for k in 0..3 {
        let (a, b) = (t[k], t[(k + 1) % 3]);
        let seite = |p: P2| (b.x - a.x) * (p[1] - a.y) - (b.y - a.y) * (p[0] - a.x);
        let mut neu = Vec::with_capacity(v.len() + 2);
        for i in 0..v.len() {
            let (p, q) = (v[i], v[(i + 1) % v.len()]);
            let (sp, sq) = (seite(p), seite(q));
            if sp >= 0.0 {
                neu.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                neu.push(lerp2(p, q, sp / (sp - sq)));
            }
        }
        v = neu;
        if v.len() < 3 {
            return Vec::new();
        }
    }
    v
}

fn normale(t: &[Vec3; 3]) -> Vec3 {
    (t[1] - t[0]).cross(t[2] - t[0]).normalized()
}

fn dreieck(s: &mut Solid, p: [Vec3; 3], n: Vec3) {
    s.triangles.push(Tri {
        p,
        n,
        mat: s.mat,
        uv: [[0.0; 2]; 3],
        elem: s.elem,
        layer: NO_LAYER,
    });
}

fn heben(p: Vec3, dz: f64) -> Vec3 {
    vec3(p.x, p.y, p.z + dz)
}

/// Dreiecke im Grundstück, an der Grenze geteilt.
fn im_grundstueck(g: &Gelaende) -> Vec<[Vec3; 3]> {
    let mut out = Vec::new();
    for t in &g.dreiecke {
        // Ganz innen: alle Ecken innen, keine Kante kreuzt die Grenze und
        // kein Grenzpunkt liegt im Dreieck
        let ecken = t.iter().all(|p| g.innen([p.x, p.y]));
        let kreuzt = (0..3).any(|i| {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            g.ringe
                .iter()
                .any(|r| schnitte(r, [a.x, a.y], [b.x, b.y]).next().is_some())
        });
        let grenzpunkt = g
            .ringe
            .iter()
            .flatten()
            .any(|q| baryzentrisch(t, *q).is_some());
        if ecken && !kreuzt && !grenzpunkt {
            out.push(*t);
            continue;
        }
        for r in &g.ringe {
            let v = im_dreieck(r, t);
            if v.len() < 3 {
                continue;
            }
            let p: Vec<Vec3> = v.iter().map(|q| vec3(q[0], q[1], ebene(t, *q))).collect();
            for [a, b, c] in polygon::triangulate(&p) {
                out.push([p[a], p[b], p[c]]);
            }
        }
    }
    out
}

/// Kanten, die nur ein Dreieck hat (Rand des Netzes), in seinem Umlauf.
fn randkanten(d: &[[Vec3; 3]]) -> Vec<(Vec3, Vec3)> {
    use std::collections::HashMap;
    type Ecke = (u64, u64);
    let key = |p: Vec3| -> Ecke { (p.x.to_bits(), p.y.to_bits()) };
    let mut n: HashMap<(Ecke, Ecke), u32> = HashMap::new();
    for t in d {
        for i in 0..3 {
            let (a, b) = (key(t[i]), key(t[(i + 1) % 3]));
            *n.entry(if a < b { (a, b) } else { (b, a) }).or_default() += 1;
        }
    }
    let mut out = Vec::new();
    for t in d {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            let (ka, kb) = (key(a), key(b));
            if n[&if ka < kb { (ka, kb) } else { (kb, ka) }] == 1 {
                out.push((a, b));
            }
        }
    }
    out
}

/// Seitenfläche von der Kante `a`→`b` bis `unten`, nach außen rechts.
fn seite(s: &mut Solid, a: Vec3, b: Vec3, unten: f64) {
    let d = (b - a).normalized();
    let (au, bu) = (vec3(a.x, a.y, unten), vec3(b.x, b.y, unten));
    s.quad(au, bu, b, a, right_of(d));
    s.edge(a, b);
    s.edge(au, bu);
}

/// Körper in 3D; `keys`: Rasen, Erdreich, Rasen blass. Die App nimmt
/// [`koerper_mit_netz`].
#[cfg(test)]
pub(crate) fn koerper(g: &Gelaende, keys: [u16; 3]) -> Solid {
    koerper_mit_netz(g, keys, false)
}

/// Körper in 3D wie `koerper`, mit `netz` dazu das Hilfsnetz (Jörns §4.3a): die Kanten
/// des Dreiecksnetzes fein auf dem Rasen, die vollen Meter kräftig. Als
/// Kanten des Körpers bleiben sie hinter Hügeln und Häusern verdeckt.
#[cfg_attr(not(test), allow(dead_code))] // 3D-Export (#258)
pub(crate) fn koerper_mit_netz(g: &Gelaende, keys: [u16; 3], netz: bool) -> Solid {
    koerper_mit(g, keys, netz, true)
}

/// Körper in 3D wie [`koerper_mit_netz`]; ohne `linien` keine Höhenlinien
/// darauf (Jörn 17.10.: mit den Höhenfarben stören sie).
pub(crate) fn koerper_mit(g: &Gelaende, keys: [u16; 3], netz: bool, linien: bool) -> Solid {
    // Die Grube des Bodenaufbaus rechnet bei vielen Häusern ein zweiter
    // Faden neben dem Netz (Test T-261a: bei 1000 Häusern 10 bis 17 ms)
    if g.aufbau.haeuser.len() < 64 {
        return koerper_und_grube(g, keys, netz, linien, || grube(g, keys));
    }
    std::thread::scope(|sc| {
        let h = sc.spawn(|| grube(g, keys));
        koerper_und_grube(g, keys, netz, linien, || h.join().expect("Grube"))
    })
}

/// Körper in 3D wie [`koerper_mit`] in der Bauphase `ph`: in einer offenen
/// Phase um die Häuser statt Gelände, Linien darauf und Aussparung die
/// Baugrube ([`bodenaufbau::decke`]).
pub(crate) fn koerper_in(
    g: &Gelaende,
    keys: [u16; 3],
    netz: bool,
    linien: bool,
    ph: Bauphase,
) -> Solid {
    if !ph.offen() {
        return koerper_mit(g, keys, netz, linien);
    }
    let felder = g.aufbau.felder();
    let mut s = koerper_und_grube(g, keys, netz, linien, Solid::default);
    let im_feld = |p: Vec3| felder.iter().any(|r| bauphase::im_feld(r, [p.x, p.y]));
    for t in std::mem::take(&mut s.triangles) {
        if t.n.z <= 0.1 {
            s.triangles.push(t);
            continue;
        }
        for teil in bauphase::ausserhalb(&t.p, &felder) {
            for i in 1..teil.len() - 1 {
                s.triangles.push(Tri {
                    p: [teil[0], teil[i], teil[i + 1]],
                    ..t
                });
            }
        }
    }
    s.edges.retain(|e| !im_feld((e.a + e.b) * 0.5));
    let dz = if g.show == Show::Fade { HEBEN } else { 0.0 };
    let hoehe = |p: P2| g.hoehe(p).map(|z| z + dz);
    let k = schluessel(keys);
    s.append(&bodenaufbau::decke(
        &g.aufbau, ph, &hoehe, g.unten, k, keys[0],
    ));
    s
}

/// Am Haus die Aussparung im Bodenaufbau, im Zuschnitt (Fade: um das
/// Grundstück gehoben).
fn grube(g: &Gelaende, keys: [u16; 3]) -> Solid {
    let dz = if g.show == Show::Fade { HEBEN } else { 0.0 };
    let hoehe = |p: P2| g.hoehe(p).map(|z| z + dz);
    let teilung = |a: P2, b: P2| g.teilung(a, b);
    let sprung = |a: P2, b: P2| g.sprung(a, b).map(|z| z + dz);
    bodenaufbau::grube(
        &g.aufbau,
        &hoehe,
        &teilung,
        &sprung,
        g.unten,
        schluessel(keys),
    )
}

/// [`koerper_mit`] mit der Grube aus `grube`.
fn koerper_und_grube(
    g: &Gelaende,
    keys: [u16; 3],
    netz: bool,
    linien: bool,
    grube: impl FnOnce() -> Solid,
) -> Solid {
    let mut s = Solid {
        mat: keys[0],
        edge_kind: edge_kind::VIEW,
        ..Solid::default()
    };
    let ganz = match g.show {
        Show::Parcel => im_grundstueck(g),
        Show::All | Show::Fade => g.dreiecke.clone(),
    };
    let oben = g.ohne_haus(ganz.clone());
    s.mat = if g.show == Show::Fade {
        keys[2]
    } else {
        keys[0]
    };
    for t in &oben {
        dreieck(&mut s, *t, normale(t));
    }
    if g.show == Show::Fade {
        s.mat = keys[0];
        for t in g.ohne_haus(im_grundstueck(g)) {
            dreieck(&mut s, t.map(|p| heben(p, HEBEN)), normale(&t));
        }
    }
    // Boden, Rand und Wände in Erdreich; die Wände ohne die Häuser
    s.mat = keys[1];
    let (waende, stellen) = g.waende_ohne_haus();
    let w0 = s.triangles.len();
    for t in &waende {
        let m = (t[0] + t[1] + t[2]) * (1.0 / 3.0);
        if g.zeigen(g.innen([m.x, m.y])).is_some() {
            dreieck(&mut s, *t, normale(t));
        }
    }
    let w1 = s.triangles.len();
    let unten = g.unten;
    let down = vec3(0.0, 0.0, -1.0);
    for t in &ganz {
        let u = t.map(|p| vec3(p.x, p.y, unten));
        dreieck(&mut s, [u[0], u[2], u[1]], down);
    }
    match g.show {
        Show::Parcel => {
            for r in &g.ringe {
                rand_an_grenze(&mut s, g, r, unten, true);
            }
            // Endet das Netz vor der Grenze, schließt sein eigener Rand den
            // Körper im Grundstück; sonst sah man dort hindurch (Jörn 17.10.)
            let rand = g.rand.clone().unwrap_or_else(|| randkanten(&g.dreiecke));
            for (a, b) in rand {
                for (t0, t1, innen) in g.teile([a.x, a.y], [b.x, b.y]) {
                    if innen {
                        seite(&mut s, a + (b - a) * t0, a + (b - a) * t1, unten);
                    }
                }
            }
        }
        Show::All | Show::Fade => {
            let rand = g.rand.clone().unwrap_or_else(|| randkanten(&g.dreiecke));
            for (a, b) in rand {
                seite(&mut s, a, b, unten);
            }
        }
    }
    let g0 = s.triangles.len();
    s.append(&grube());
    let g1 = s.triangles.len();
    senkrecht_teilen(&mut s.triangles, &[w0..w1, g0..g1], &stellen);
    // Umriss jeder Geländefläche als Kante
    for r in &g.umrisse {
        for i in 0..r.len() {
            s.edge(r[i], r[(i + 1) % r.len()]);
        }
    }
    // Die Grenze als Kante auf dem Gelände wie im Grundriss (Jörn 17.10.);
    // bei „nicht“ ist sie schon der Rand des Körpers
    if g.show != Show::Parcel {
        let dz = if g.show == Show::Fade { HEBEN } else { 0.0 };
        for r in &g.ringe {
            grenze_als_kante(&mut s, g, r, dz);
        }
    }
    // Volle Meter als feine Kanten auf dem Rasen, mit dem Netz kräftig
    s.edge_kind = if netz {
        edge_kind::VIEW
    } else {
        edge_kind::FINE
    };
    for &(z, [a, b], innen, haupt) in g.stuecke.iter().filter(|_| linien) {
        if !haupt || (g.show == Show::Parcel && !innen) {
            continue;
        }
        let dz = if innen && g.show == Show::Fade {
            HEBEN
        } else {
            0.0
        };
        s.edge(vec3(a[0], a[1], z + dz), vec3(b[0], b[1], z + dz));
    }
    if netz {
        s.edge_kind = edge_kind::FINE;
        for (a, b, innen) in netzkanten(g) {
            if g.show == Show::Parcel && !innen {
                continue;
            }
            let dz = if innen && g.show == Show::Fade {
                HEBEN
            } else {
                0.0
            };
            s.edge(heben(a, dz), heben(b, dz));
        }
    }
    s
}

/// Kanten des Dreiecksnetzes, jede einmal, an Grenze und Haus geteilt wie
/// die Höhenlinien (im Haus keine): von, bis und ob im Grundstück. Eine
/// Kante zwischen zwei Dreiecken in derselben Ebene fällt weg, dort knickt
/// nichts: Sonst teilte eine Diagonale die ebene Platte (Jörn 17.10.).
fn netzkanten(g: &Gelaende) -> Vec<(Vec3, Vec3, bool)> {
    let key = |p: Vec3| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
    // je Kante von, bis, die Normale des ersten Dreiecks und ob ein zweites
    // in derselben Ebene liegt
    let mut stelle = HashMap::with_capacity(g.dreiecke.len() * 2);
    let mut kanten: Vec<(Vec3, Vec3, Vec3, bool)> = Vec::with_capacity(g.dreiecke.len() * 2);
    for t in &g.dreiecke {
        let n = normale(t);
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            let (ka, kb) = (key(a), key(b));
            match stelle.entry(if ka < kb { (ka, kb) } else { (kb, ka) }) {
                std::collections::hash_map::Entry::Occupied(e) => {
                    let k: &mut (Vec3, Vec3, Vec3, bool) = &mut kanten[*e.get()];
                    k.3 |= k.2.dot(n) > 1.0 - EBEN;
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(kanten.len());
                    kanten.push((a, b, n, false));
                }
            }
        }
    }
    let mut out = Vec::with_capacity(kanten.len());
    for (a, b, _, eben) in kanten {
        if eben {
            continue;
        }
        for (t0, t1, innen) in g.teile([a.x, a.y], [b.x, b.y]) {
            out.push((a + (b - a) * t0, a + (b - a) * t1, innen));
        }
    }
    out
}

/// Unter dieser Abweichung (1 − cos) liegen zwei Dreiecke in derselben
/// Ebene: etwa 0,0008°.
const EBEN: f64 = 1e-10;

/// Die Grenze `r` als Kante auf dem Netz, um `dz` gehoben: an jeder
/// Netzkante geteilt wie [`rand_an_grenze`], im Haus nicht.
fn grenze_als_kante(s: &mut Solid, g: &Gelaende, r: &[P2], dz: f64) {
    let n = r.len();
    let mut nah = Vec::new();
    for i in 0..n {
        let (a, b) = (r[i], r[(i + 1) % n]);
        let mut ts = vec![0.0, 1.0];
        g.netz.bei(&kasten(&[a, b]), &mut nah);
        for t in nah.iter().map(|&j| &g.dreiecke[j as usize]) {
            for k in 0..3 {
                let (c, d) = (t[k], t[(k + 1) % 3]);
                if let Some(x) = kreuzung(a, b, [c.x, c.y], [d.x, d.y]) {
                    ts.push(x);
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        for w in ts.windows(2) {
            let (p, q) = (lerp2(a, b, w[0]), lerp2(a, b, w[1]));
            if g.im_haus(lerp2(p, q, 0.5)) {
                continue;
            }
            if let (Some(zp), Some(zq)) = (g.hoehe(p), g.hoehe(q)) {
                s.edge(vec3(p[0], p[1], zp + dz), vec3(q[0], q[1], zq + dz));
            }
        }
    }
}

/// Rand längs des Rings `r` bis `unten`: geteilt an jeder Netzkante,
/// damit die Oberkante genau auf dem Netz liegt. An der Grenze (`aussen`)
/// nach außen gerichtet, am Haus nach innen und nur, wo das Gelände
/// gezeigt wird; liegt das Netz dort tiefer als `unten`, entfällt er.
fn rand_an_grenze(s: &mut Solid, g: &Gelaende, r: &[P2], unten: f64, aussen: bool) {
    let ccw =
        polygon::signed_area(&r.iter().map(|p| vec3(p[0], p[1], 0.0)).collect::<Vec<_>>()) > 0.0;
    let dz = if !aussen && g.show == Show::Fade {
        HEBEN
    } else {
        0.0
    };
    let n = r.len();
    let mut nah = Vec::new();
    for i in 0..n {
        let (a, b) = if ccw == aussen {
            (r[i], r[(i + 1) % n])
        } else {
            (r[(i + 1) % n], r[i])
        };
        let ts = g.teilung_mit(a, b, &mut nah);
        for w in ts.windows(2) {
            let (p, q) = (lerp2(a, b, w[0]), lerp2(a, b, w[1]));
            if !aussen && g.zeigen(g.innen(lerp2(p, q, 0.5))).is_none() {
                continue;
            }
            if let (Some(zp), Some(zq)) = (g.hoehe(p), g.hoehe(q)) {
                let (zp, zq) = (zp + dz, zq + dz);
                if zp.max(zq) > unten {
                    let (zp, zq) = (zp.max(unten), zq.max(unten));
                    seite(s, vec3(p[0], p[1], zp), vec3(q[0], q[1], zq), unten);
                }
            }
        }
    }
}

/// Schnittflächen mit der Ebene (zeigen in Richtung `f.n`): die Schichten
/// des Bodenaufbaus unter der Geländelinie, unter dem Haus bis zur
/// Aussparung ([`bodenaufbau::schnitt`]); `keys` wie im Körper.
pub(crate) fn kappen(g: &Gelaende, f: &SectionFrame, p0: Vec3, keys: [u16; 3]) -> Solid {
    kappen_in(g, f, p0, keys, Bauphase::Fertig)
}

/// [`kappen`] in der Bauphase `ph`.
pub(crate) fn kappen_in(
    g: &Gelaende,
    f: &SectionFrame,
    p0: Vec3,
    keys: [u16; 3],
    ph: Bauphase,
) -> Solid {
    let oben: Vec<(P2, P2)> = g
        .profil(f, p0)
        .into_iter()
        .filter(|s| s.2)
        .map(|(a, b, _)| (a, b))
        .collect();
    bodenaufbau::schnitt(&g.aufbau, f, &oben, g.unten, schluessel(keys), ph)
}

/// Linien in Grundriss, Schnitt und Ansicht samt Lageplan und Punktmarken
/// im Grundriss ([`unterlage_linien`], [`grundlinien`] und [`marken`]).
/// `mm_px`: Millimeter je Bildpunkt (Grundriss), `fassade`: Ebene der
/// Geländelinie einer Ansicht.
#[cfg_attr(not(all(test, target_os = "linux")), allow(dead_code))] // Prüfbilder
pub(crate) fn linien(
    g: &Gelaende,
    view: ViewKind,
    plane: Option<(Vec3, Vec3)>,
    fassade: Option<(Vec3, Vec3)>,
    t: &DrawTable,
    scale: f32,
    mm_px: f64,
) -> Vec<Helper> {
    let mut out = unterlage_linien(g, view, t, scale);
    out.extend(grundlinien(g, view, plane, fassade, t, scale));
    out.extend(marken(g, view, t, scale, mm_px));
    out
}

/// Der Lageplan im Grundriss, blass unter dem Gelände, Grenzen 0,25 mm.
/// Er liegt in eigenen Puffern des Renderers, je [`Kachel`] einer, die nur
/// bei neuen Strecken der Kachel, neuer Zeichentabelle oder neuem Maßstab
/// neu hochgeladen werden (Review #139 B1 und N1).
pub(crate) fn unterlage_linien(
    g: &Gelaende,
    view: ViewKind,
    t: &DrawTable,
    scale: f32,
) -> Vec<Helper> {
    if view != ViewKind::Plan {
        return Vec::new();
    }
    let stil = unterlage_stil(t, scale);
    (0..g.unterlage.len())
        .flat_map(|i| unterlage_kachel(g, i, stil))
        .collect()
}

/// Stifte der Unterlage: blass, Grenzen 0,25 mm, und der Maßstab; klein
/// genug, um mit dem Gelände in einen Faden zu gehen (Review #139 N2).
#[derive(Clone, Copy, Debug)]
pub(crate) struct UnterlageStil {
    blass: Stroke,
    grenze: Stroke,
    scale: f32,
}

/// [`UnterlageStil`] aus Zeichentabelle und Maßstab.
pub(crate) fn unterlage_stil(t: &DrawTable, scale: f32) -> UnterlageStil {
    UnterlageStil {
        blass: t.background,
        grenze: (t.ground.0 * 0.5, t.background.1),
        scale,
    }
}

/// Die Linien der Kachel `i` des Lageplans ([`unterlage_linien`]).
pub(crate) fn unterlage_kachel(g: &Gelaende, i: usize, stil: UnterlageStil) -> Vec<Helper> {
    g.unterlage.get(i).map_or(Vec::new(), |k| {
        k.striche
            .iter()
            .map(|([a, b], ist_grenze)| {
                let st = if *ist_grenze { stil.grenze } else { stil.blass };
                strich(*a, *b, st, sk_render::SOLID, stil.scale)
            })
            .collect()
    })
}

/// Höhe in m ü. NHN als Text: volle Meter ohne Nachkommastellen, sonst
/// auf cm.
fn nhn_text(m: f64) -> String {
    if (m - m.round()).abs() < 1e-6 {
        crate::selection::de(m, 0)
    } else {
        crate::selection::de(m, 2)
    }
}

/// Richtung (rad) von `a` nach `b`, so gedreht, dass Schrift längs der
/// Strecke von unten oder von rechts lesbar steht.
fn lesbar(a: P2, b: P2) -> f64 {
    let w = (b[1] - a[1]).atan2(b[0] - a[0]);
    if w > FRAC_PI_2 + 1e-9 {
        w - PI
    } else if w <= -FRAC_PI_2 + 1e-9 {
        w + PI
    } else {
        w
    }
}

/// Höhenzahlen längs der Höhenlinien `linien` (Höhe mm zu ±0,00, Punkte,
/// Hauptlinie; `h0` ist ±0,00 in m ü. NHN): an den Hauptlinien (volle
/// Meter), gibt es keine, an allen. Je Linie alle [`ZAHL_ABSTAND`], die
/// erste einen halben Abstand vom Anfang, eine kürzere Linie in der Mitte.
/// `frei(p)` sagt, ob an `p` eine Zahl stehen darf (nicht im Haus, nicht
/// im Planungsgelände, gezeigt) und ob voll; wo nicht, rückt die Zahl um
/// [`ZAHL_SCHRITT`] weiter, die nächste steht einen Abstand nach ihr.
fn hoehenzahlen(
    linien: &[(f64, Vec<P2>, bool)],
    h0: f64,
    frei: impl Fn(P2) -> Option<bool>,
) -> Vec<Zahl> {
    let haupt = linien.iter().any(|l| l.2);
    let mut out = Vec::new();
    for (z, p, h) in linien {
        if haupt && !h {
            continue;
        }
        let laenge: f64 = p.windows(2).map(|w| abstand(w[0], w[1])).sum();
        if laenge <= 0.0 {
            continue;
        }
        let text = nhn_text(z / 1000.0 + h0);
        let mut ziel = (ZAHL_ABSTAND / 2.0).min(laenge / 2.0);
        let mut bis = 0.0;
        for w in p.windows(2) {
            let d = abstand(w[0], w[1]);
            while d > 0.0 && ziel <= bis + d {
                let q = lerp2(w[0], w[1], (ziel - bis) / d);
                if let Some(voll) = frei(q) {
                    out.push((q, lesbar(w[0], w[1]), *z, text.clone(), voll));
                    ziel += ZAHL_ABSTAND;
                } else {
                    ziel += ZAHL_SCHRITT;
                }
            }
            bis += d;
        }
    }
    out
}

/// Höhenzahlen in der Mitte jeder ebenen Geländefläche (Umriss in mm, z
/// zu ±0,00), waagerecht; eine Fläche mit Gefälle trägt keine.
fn flaechenzahlen(umrisse: &[Vec<Vec3>], h0: f64, frei: impl Fn(P2) -> Option<bool>) -> Vec<Zahl> {
    umrisse
        .iter()
        .filter_map(|r| {
            let z = r.first()?.z;
            if r.len() < 3 || r.iter().any(|p| (p.z - z).abs() > 1.0) {
                return None;
            }
            let ring: Vec<P2> = r.iter().map(|p| [p.x, p.y]).collect();
            let q = schwerpunkt(&ring)?;
            let voll = frei(q)?;
            Some((q, 0.0, z, nhn_text(z / 1000.0 + h0), voll))
        })
        .collect()
}

/// Flächenschwerpunkt des Rings; `None` ohne Inhalt.
fn schwerpunkt(r: &[P2]) -> Option<P2> {
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..r.len() {
        let (p, q) = (r[i], r[(i + 1) % r.len()]);
        let k = p[0] * q[1] - q[0] * p[1];
        a += k;
        cx += (p[0] + q[0]) * k;
        cy += (p[1] + q[1]) * k;
    }
    (a.abs() > 1e-9).then(|| [cx / (3.0 * a), cy / (3.0 * a)])
}

/// Inhalt der Höhenzahlen und Koten (Ort, Richtung, Text, voll), die
/// Häuser und die Grenze als Prüfsumme: gleich, solange die Tinte gleich
/// bliebe.
pub(crate) fn zahlen_inhalt(g: &Gelaende) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut s = std::collections::hash_map::DefaultHasher::new();
    for (q, w, z, text, voll) in g.zahlen.iter().chain(&g.koten) {
        for v in [q[0], q[1], *w, *z] {
            v.to_bits().hash(&mut s);
        }
        (text, voll).hash(&mut s);
    }
    g.zahlen.len().hash(&mut s);
    // die Koten weichen den Häusern und der Grenze aus
    let haeuser = g.haus.iter().map(|(ring, _)| ring);
    for ring in haeuser.chain(&g.ringe) {
        for p in ring {
            p.map(f64::to_bits).hash(&mut s);
        }
        ring.len().hash(&mut s);
    }
    g.haus.len().hash(&mut s);
    s.finish()
}

/// Die Höhenzahlen als Tinte für den Renderer: Ziffernhöhe `h` (mm am
/// Bau, wie die Maßzahlen), mittig über ihrem Punkt wie eine Maßzahl über
/// der Maßlinie, die Koten der Messpunkte rechts neben ihrem Kreuz, auf
/// halber Ziffernhöhe; leicht über dem Gelände; voll im Stift „Gelände“,
/// sonst blass.
pub(crate) fn zahlen_tinte(
    g: &Gelaende,
    schrift: &sk_plan::schrift::Schrift,
    h: f64,
    t: &DrawTable,
) -> Vec<sk_render::Ink> {
    let mut out = Vec::new();
    let mut d = Vec::new();
    let mut belegt = Belegt::neu(4.0 * h);
    // keine Zahl über dem Kreuz eines Messpunkts (Test H-223e); je
    // sichtbarem Kreuz eine Kote: ein Punkt fast auf einem schon
    // beschrifteten bekommt keine zweite (Bedienbarkeit #223)
    for (q, ..) in &g.koten {
        belegt.setze(um(*q, 0.25 * h));
    }
    let mut beschriftet = Belegt::neu(4.0 * h);
    let mitte = g.zahlen.iter().map(|z| (z, true));
    for ((q, w, z, text, voll), mittig) in mitte.chain(g.koten.iter().map(|z| (z, false))) {
        d.clear();
        let b = schrift.zeile(text, h, 0.0, &mut d);
        let (c, s) = (w.cos(), w.sin());
        let color = if *voll { t.ground.1 } else { t.background.1 };
        let (dx, dy) = if mittig {
            // Rechteck der gedrehten Zahl, damit Koten ihr ausweichen
            let ecken = [
                (-b / 2.0, 0.0),
                (b / 2.0, 0.0),
                (-b / 2.0, 1.3 * h),
                (b / 2.0, 1.3 * h),
            ]
            .map(|(x, y)| [q[0] + c * x - s * y, q[1] + s * x + c * y]);
            belegt.setze(kasten(&ecken));
            (-b / 2.0, 0.3 * h)
        } else {
            // rechts vom Kreuz, sonst links; ans Haus, über die Grenze
            // oder über eine schon gesetzte Zahl keine (Test und
            // Bedienbarkeit #223)
            let k = |x0: f64| [q[0] + x0, q[1] - 0.5 * h, q[0] + x0 + b, q[1] + 0.5 * h];
            let rand = |k: Kasten| [k[0] - RAND, k[1] - RAND, k[2] + RAND, k[3] + RAND];
            let punkt = um(*q, 0.5 * h);
            let Some(x0) = [0.5 * h, -0.5 * h - b].into_iter().find(|&x0| {
                belegt.frei(&k(x0)) && !g.haus_trifft(&k(x0)) && !g.grenze_trifft(&rand(k(x0)))
            }) else {
                continue;
            };
            if !beschriftet.frei(&punkt) {
                continue;
            }
            belegt.setze(k(x0));
            beschriftet.setze(punkt);
            (x0, -0.5 * h)
        };
        out.extend(d.iter().map(|tri| sk_render::Ink {
            p: tri.map(|[x, y]| {
                let (x, y) = (x + dx, y + dy);
                [
                    (q[0] + c * x - s * y) as f32,
                    (q[1] + s * x + c * y) as f32,
                    (z + 1.0) as f32,
                ]
            }),
            color,
            occlude: false,
        }));
    }
    out
}

/// Quadrat mit halber Seite `r` um `q`.
fn um(q: P2, r: f64) -> Kasten {
    [q[0] - r, q[1] - r, q[0] + r, q[1] + r]
}

/// Schon gesetzte Zahlen im Grundriss, je Feld des Rasters `zelle`.
struct Belegt {
    zelle: f64,
    kaesten: Vec<Kasten>,
    felder: HashMap<(i64, i64), Vec<u32>>,
}

impl Belegt {
    fn neu(zelle: f64) -> Belegt {
        Belegt {
            zelle: zelle.max(1.0),
            kaesten: Vec::new(),
            felder: HashMap::new(),
        }
    }

    fn felder(&self, k: &Kasten) -> impl Iterator<Item = (i64, i64)> {
        let f = |v: f64| (v / self.zelle).floor() as i64;
        let (x0, y0, x1, y1) = (f(k[0]), f(k[1]), f(k[2]), f(k[3]));
        (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
    }

    fn frei(&self, k: &Kasten) -> bool {
        self.felder(k).all(|f| {
            self.felder
                .get(&f)
                .is_none_or(|v| v.iter().all(|&i| !ueberlappt(&self.kaesten[i as usize], k)))
        })
    }

    fn setze(&mut self, k: Kasten) {
        let i = self.kaesten.len() as u32;
        let felder: Vec<_> = self.felder(&k).collect();
        for f in felder {
            self.felder.entry(f).or_default().push(i);
        }
        self.kaesten.push(k);
    }
}

/// Strich von `a` nach `b` mit Stift und Muster.
fn strich(a: Vec3, b: Vec3, (w, c): Stroke, pattern: DashPattern, scale: f32) -> Helper {
    Helper {
        a: a.to_f32(),
        b: b.to_f32(),
        color: c,
        width: w * scale,
        dash: 0.0,
        pattern,
        occlude: false,
        round: false,
    }
}

/// Kreuze der Messpunkte im Grundriss, je zwei Striche; nur sie hängen
/// vom Zoom ab (Review #129 B3).
pub(crate) fn marken(
    g: &Gelaende,
    view: ViewKind,
    t: &DrawTable,
    scale: f32,
    mm_px: f64,
) -> Vec<Helper> {
    if view != ViewKind::Plan {
        return Vec::new();
    }
    let fine = t.drawing_edges[edge_kind::FINE as usize];
    let k = MARKE * f64::from(scale) * mm_px;
    let mut out = Vec::with_capacity(2 * g.marken.len());
    for &(p, voll) in &g.marken {
        let st = if voll { fine } else { t.background };
        for (dx, dy) in [(k, k), (k, -k)] {
            let (a, b) = (vec3(p.x - dx, p.y - dy, p.z), vec3(p.x + dx, p.y + dy, p.z));
            out.push(strich(a, b, st, sk_render::SOLID, scale));
        }
    }
    out
}

/// Linien in Grundriss (ohne die Punktmarken), Schnitt und Ansicht;
/// unabhängig vom Zoom.
pub(crate) fn grundlinien(
    g: &Gelaende,
    view: ViewKind,
    plane: Option<(Vec3, Vec3)>,
    fassade: Option<(Vec3, Vec3)>,
    t: &DrawTable,
    scale: f32,
) -> Vec<Helper> {
    let fine = t.drawing_edges[edge_kind::FINE as usize];
    let blass = t.background;
    let line = |a: Vec3, b: Vec3, st: Stroke, pattern| strich(a, b, st, pattern, scale);
    let solid = sk_render::SOLID;
    let mut out = Vec::new();
    match (view, plane.or(fassade)) {
        (ViewKind::Plan, _) => {
            // der Lageplan darunter steht in [`unterlage_linien`]
            for &(z, [a, b], innen, haupt) in &g.stuecke {
                let Some(voll) = g.zeigen(innen) else {
                    continue;
                };
                let stift = if haupt { t.ground } else { fine };
                let st = if voll { stift } else { blass };
                out.push(line(vec3(a[0], a[1], z), vec3(b[0], b[1], z), st, solid));
            }
            // Geländeflächen: Umriss, Böschungsschraffen vom Kopf zum Fuß
            let kante = t.drawing_edges[edge_kind::VIEW as usize];
            for r in &g.umrisse {
                for i in 0..r.len() {
                    out.push(line(r[i], r[(i + 1) % r.len()], kante, solid));
                }
            }
            for &([a, b], _) in &g.schraffen {
                out.push(line(a, b, fine, solid));
            }
            // Grenze strichpunktiert, kräftig
            for r in &g.ringe {
                for i in 0..r.len() {
                    let (a, b) = (r[i], r[(i + 1) % r.len()]);
                    let za = g.hoehe(a).unwrap_or(0.0);
                    let zb = g.hoehe(b).unwrap_or(0.0);
                    let (a, b) = (vec3(a[0], a[1], za), vec3(b[0], b[1], zb));
                    out.push(line(a, b, t.ground, t.section_dash));
                }
            }
        }
        (ViewKind::Persp, _) | (_, None) => {}
        (_, Some((p0, n))) => {
            let f = SectionFrame::new(p0, n);
            let profil = g.profil(&f, p0);
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for (a, b, voll) in &profil {
                let st = if *voll { t.ground } else { blass };
                out.push(line(f.pt(a[0], a[1]), f.pt(b[0], b[1]), st, solid));
                if *voll {
                    lo = lo.min(a[0]).min(b[0]);
                    hi = hi.max(a[0]).max(b[0]);
                }
            }
            // das Urgelände, wo das Planungsgelände es ändert, gestrichelt
            for (a, b, voll) in g.profil_von(&g.ur, Some(&g.ur_netz), &f, p0) {
                let st = if voll { fine } else { blass };
                out.push(line(f.pt(a[0], a[1]), f.pt(b[0], b[1]), st, t.element_dash));
            }
            // Unterkante und Enden fein, wie bei der Platte
            if lo < hi {
                let z = |u: f64| {
                    profil
                        .iter()
                        .find_map(|(a, b, _)| {
                            [a, b]
                                .into_iter()
                                .find(|p| (p[0] - u).abs() < 1e-6)
                                .map(|p| p[1])
                        })
                        .unwrap_or(g.unten)
                };
                out.push(line(f.pt(lo, g.unten), f.pt(hi, g.unten), fine, solid));
                out.push(line(f.pt(lo, g.unten), f.pt(lo, z(lo)), fine, solid));
                out.push(line(f.pt(hi, g.unten), f.pt(hi, z(hi)), fine, solid));
            }
        }
    }
    out
}

/// Ebene der Geländelinie einer Ansicht: die Fassade, die zum Betrachter
/// zeigt („Vorne“ blickt nach +y), am Rechteck um das Haus; ohne Haus
/// durch die Mitte des Netzes.
pub(crate) fn fassade(
    g: &Gelaende,
    view: ViewKind,
    haus: Option<(Vec3, Vec3)>,
) -> Option<(Vec3, Vec3)> {
    let (lo, hi) = haus.unwrap_or_else(|| {
        let mut lo = vec3(f64::MAX, f64::MAX, 0.0);
        let mut hi = vec3(f64::MIN, f64::MIN, 0.0);
        for p in g.dreiecke.iter().flatten() {
            lo = vec3(lo.x.min(p.x), lo.y.min(p.y), 0.0);
            hi = vec3(hi.x.max(p.x), hi.y.max(p.y), 0.0);
        }
        let m = (lo + hi) * 0.5;
        (m, m)
    });
    let n = match view {
        ViewKind::Front => (vec3(lo.x, lo.y, 0.0), vec3(0.0, 1.0, 0.0)),
        ViewKind::Back => (vec3(hi.x, hi.y, 0.0), vec3(0.0, -1.0, 0.0)),
        ViewKind::Left => (vec3(lo.x, lo.y, 0.0), vec3(1.0, 0.0, 0.0)),
        ViewKind::Right => (vec3(hi.x, hi.y, 0.0), vec3(-1.0, 0.0, 0.0)),
        ViewKind::Persp | ViewKind::Plan | ViewKind::Section => return None,
    };
    Some(n)
}

/// Körper für den 3D-Export ([`crate::gelaende_platte::export_koerper`]):
/// wie in 3D ohne Hilfsnetz, `keys` Belag und Erdreich. Bei „Umgebung
/// blass“ das ganze Netz im Belag; die angehobene Kopie des Grundstücks
/// gehört nur zur Darstellung.
pub(crate) fn export_koerper(mut g: Gelaende, keys: [u16; 2]) -> Solid {
    if g.show == Show::Fade {
        g.show = Show::All;
    }
    koerper_mit_netz(&g, [keys[0], keys[1], keys[0]], false)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sk_model::material;

    const CSV: &[u8] =
        include_bytes!("../../crates/sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.csv");

    /// Der Treffpunkt über das Raster ist derselbe wie über alle Dreiecke
    /// (Review B-264b), schräg, flach und senkrecht von oben.
    #[test]
    fn treffpunkt_wie_ueber_alle() {
        let g = Gelaende::aus(&lindenhof()).unwrap();
        let alle = |o: Vec3, d: Vec3| {
            g.dreiecke
                .iter()
                .filter_map(|t| sk_math::ray_triangle(o, d, t[0], t[1], t[2]))
                .min_by(f64::total_cmp)
                .map(|t| o + d * t)
        };
        let (lo, hi) = g.rahmen();
        let (mut treffer, mut n) = (0, 0);
        for i in 0..=12 {
            for j in 0..=12 {
                let x = lo.x + (hi.x - lo.x) * i as f64 / 12.0;
                let y = lo.y + (hi.y - lo.y) * j as f64 / 12.0;
                let o = vec3(x, y, hi.z + 20_000.0);
                for d in [
                    vec3(0.0, 0.0, -1.0),
                    vec3(1.0, 0.4, -0.8),
                    vec3(-0.3, 1.0, -0.2),
                    vec3(-1.0, -1.0, -0.05),
                ] {
                    let d = d.normalized();
                    let (a, b) = (g.treffpunkt(o, d), alle(o, d));
                    assert_eq!(a, b, "{o:?} {d:?}");
                    assert_eq!(g.getroffen(o, d), b.is_some(), "{o:?} {d:?}");
                    treffer += usize::from(a.is_some());
                    n += 1;
                }
            }
        }
        assert!(treffer > n / 5, "{treffer} von {n}");
    }

    fn lindenhof() -> Model {
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            vec3(7000.0, 6000.0, 0.0),
            vec3(7000.0, 14000.0, 0.0),
            vec3(17000.0, 14000.0, 0.0),
            vec3(17000.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let (t, ..) = sk_model::Terrain::lesen(CSV, "a.csv", "2026-10-12", 32).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        m
    }

    /// Kanten, die nur ein Dreieck hat, mit Punkten auf 0,001 mm
    /// verschweißt (offen; ein dichter Körper hat keine).
    fn offene_kanten(s: &Solid) -> Vec<[Vec3; 2]> {
        let k = |p: Vec3| [p.x, p.y, p.z].map(|v| (v * 1000.0).round() as i64);
        let mut n: HashMap<_, (i32, [Vec3; 2])> = HashMap::new();
        for t in &s.triangles {
            for i in 0..3 {
                let (p, q) = (t.p[i], t.p[(i + 1) % 3]);
                let (a, b) = (k(p), k(q));
                if a == b {
                    continue;
                }
                n.entry(if a < b { (a, b) } else { (b, a) })
                    .or_insert((0, [p, q]))
                    .0 += 1;
            }
        }
        n.into_values().filter(|e| e.0 == 1).map(|e| e.1).collect()
    }

    /// Der Export ist mit der Grube des Bodenaufbaus dicht (Gelände
    /// G-261c), jede Kante hat zwei Dreiecke, auch dort, wo die Geraden der
    /// Hauskanten ein Dreieck am Haus teilen und das daneben nicht (G-261e:
    /// vorher 24 offene Kanten etwa 3,5 m um das Haus).
    #[test]
    fn export_mit_grube_dicht() {
        let m = lindenhof();
        for show in [Show::All, Show::Parcel] {
            let mut g = Gelaende::aus(&m).unwrap();
            g.show = show;
            assert_eq!(g.aufbau.haeuser.len(), 1);
            let offen = offene_kanten(&export_koerper(g, [10, 11]));
            assert!(offen.is_empty(), "{show:?}: {offen:?}");
        }
    }

    /// Geglättet ist der Export dicht (Review H-276a): Der Rand des Netzes
    /// bekommt die Zwischenpunkte seiner geraden Kanten (vorher mit „fein“
    /// 96 offene Kanten bei „alles“), und eine Schicht der Grube, die nur
    /// an einem Ende dünner als 0,5 mm ist, fehlt der Wand nicht mehr (mit
    /// „sehr fein“ am Haus 3).
    #[test]
    fn export_geglaettet_dicht() {
        let mut m = lindenhof();
        for glatt in [sk_model::Glaettung::Fein, sk_model::Glaettung::SehrFein] {
            m.begin("Glätten");
            assert!(m.set_terrain_smoothing(glatt));
            m.commit();
            for show in [Show::All, Show::Parcel] {
                let mut g = Gelaende::aus(&m).unwrap();
                g.show = show;
                let offen = offene_kanten(&export_koerper(g, [10, 11]));
                assert!(offen.is_empty(), "{glatt:?} {show:?}: {offen:?}");
            }
        }
    }

    /// Mit Geländeflächen ist der Export dicht (G-275a: mit den ersten
    /// beiden vorher 191 offene Kanten, T-Stöße der Stücke gegen die
    /// Dreiecke daneben und untereinander): eine über, eine unter dem
    /// Gelände, mit Böschung, und eine dritte mit senkrechtem Rand über der
    /// ersten (Wände, an ihren Ecken drei Höhen an einer Stelle).
    #[test]
    fn export_mit_flaechen_dicht() {
        let mut m = lindenhof();
        for (dz, [x0, y0], [w, h], senkrecht) in [
            (600.0, [9_000.0, 1_000.0], [6_000.0, 3_000.0], false),
            (-400.0, [1_000.0, 1_000.0], [6_000.0, 3_000.0], false),
            (300.0, [12_000.0, 2_000.0], [4_000.0, 3_500.0], true),
        ] {
            let z = m.terrain_z_at(None, x0 + w / 2.0, y0 + h / 2.0) + dz;
            let r = vec![[x0, y0], [x0 + w, y0], [x0 + w, y0 + h], [x0, y0 + h]];
            let mut f = sk_model::TerrainPatch::flat(r, z);
            if senkrecht {
                f.slopes = vec![0.0];
            }
            m.begin("Fläche");
            assert!(m.add_site_patch(f).is_some());
            m.commit();
        }
        let plan = m.planned_terrain().unwrap();
        assert!(!plan.walls.is_empty() && !plan.seam.is_empty());
        // auch geglättet (Review H-276a: mit „fein“ vorher 198 bzw. 90
        // offene Kanten, die Nachbarn haben Zwischenpunkte auf der Kante)
        use sk_model::Glaettung;
        for glatt in [Glaettung::Aus, Glaettung::Fein, Glaettung::SehrFein] {
            m.begin("Glätten");
            m.set_terrain_smoothing(glatt);
            m.commit();
            for show in [Show::All, Show::Parcel] {
                let mut g = Gelaende::aus(&m).unwrap();
                g.show = show;
                let offen = offene_kanten(&export_koerper(g, [10, 11]));
                assert!(
                    offen.is_empty(),
                    "{glatt:?} {show:?}: {} offen, {offen:?}",
                    offen.len()
                );
            }
        }
    }

    /// Rechteck oder L (`l`) mit Mitte `c`, Breite `w`, Höhe `h`, um
    /// `grad` gedreht.
    fn ring_gedreht(c: P2, [w, h]: [f64; 2], grad: f64, l: bool) -> Vec<P2> {
        let (s, k) = grad.to_radians().sin_cos();
        let r: Vec<P2> = if l {
            vec![
                [0.0, 0.0],
                [w, 0.0],
                [w, h / 2.0],
                [w / 2.0, h / 2.0],
                [w / 2.0, h],
                [0.0, h],
            ]
        } else {
            vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
        };
        r.iter()
            .map(|p| {
                let (x, y) = (p[0] - w / 2.0, p[1] - h / 2.0);
                [c[0] + x * k - y * s, c[1] + x * s + y * k]
            })
            .collect()
    }

    /// Auch wo die Böschung einer Fläche ins Haus reicht oder die Fläche
    /// selbst, ist der Export dicht (Geometrie H-276-geo-a: in diesen
    /// zehn Fällen der Sonde vorher 4 bis 20 offene Kanten, Wände der
    /// Flächen im Haus und die Grube ohne den Sprung an der Hauskante),
    /// auch mit senkrechtem Rand am Haus.
    #[test]
    fn export_mit_flaechen_am_haus_dicht() {
        // je Fläche: Mitte, Breite und Höhe, Drehung, dz, senkrecht, L
        type F = (P2, [f64; 2], f64, f64, bool, bool);
        let eins = |grad, dz| {
            (
                [12_000.0, 2_500.0],
                [6_000.0, 3_000.0],
                grad,
                dz,
                false,
                false,
            )
        };
        let am_haus = |grad, senkrecht| {
            (
                [12_000.0, 4_800.0],
                [6_000.0, 2_000.0],
                grad,
                200.0,
                senkrecht,
                false,
            )
        };
        let zwei = |grad| {
            vec![
                (
                    [5_000.0, 2_500.0],
                    [6_000.0, 3_000.0],
                    grad,
                    -400.0,
                    false,
                    false,
                ),
                (
                    [11_000.0, 2_500.0],
                    [6_000.0, 3_000.0],
                    grad,
                    600.0,
                    false,
                    false,
                ),
            ]
        };
        let mut faelle: Vec<Vec<F>> = vec![
            vec![eins(20.0, 600.0)],
            vec![eins(45.0, 600.0)],
            vec![eins(45.0, -400.0)],
            zwei(20.0),
            zwei(45.0),
            vec![(
                [12_000.0, 3_000.0],
                [6_000.0, 4_000.0],
                45.0,
                400.0,
                false,
                true,
            )],
        ];
        for grad in [0.0, 10.0, 20.0, 45.0] {
            faelle.push(vec![am_haus(grad, false)]);
            faelle.push(vec![am_haus(grad, true)]);
        }
        let mut waende = 0;
        for fl in &faelle {
            let mut m = lindenhof();
            for &(c, wh, grad, dz, senkrecht, l) in fl {
                let z = m.terrain_z_at(None, c[0], c[1]) + dz;
                let mut f = sk_model::TerrainPatch::flat(ring_gedreht(c, wh, grad, l), z);
                if senkrecht {
                    f.slopes = vec![0.0];
                }
                m.begin("Fläche");
                assert!(m.add_site_patch(f).is_some());
                m.commit();
            }
            for show in [Show::All, Show::Parcel] {
                let mut g = Gelaende::aus(&m).unwrap();
                g.show = show;
                let (w, _) = g.waende_ohne_haus();
                // vorher Wände im Haus, jetzt keine
                let im_haus = |t: &[Vec3; 3]| {
                    let m = (t[0] + t[1] + t[2]) * (1.0 / 3.0);
                    g.im_haus([m.x, m.y])
                };
                waende += usize::from(g.waende.iter().any(im_haus));
                assert!(!w.iter().any(im_haus), "{fl:?}");
                let offen = offene_kanten(&export_koerper(g, [10, 11]));
                assert!(
                    offen.is_empty(),
                    "{fl:?} {show:?}: {} offen, {offen:?}",
                    offen.len()
                );
            }
        }
        // bis auf den senkrechten Rand ohne Drehung reichen Wände ins Haus
        assert_eq!(waende, 2 * 13);
    }

    fn flaeche(d: &[[Vec3; 3]]) -> f64 {
        d.iter()
            .map(|t| (t[1] - t[0]).cross(t[2] - t[0]).z.abs() / 2.0)
            .sum::<f64>()
            / 1e6
    }

    /// Zugeschnitten (Vorgabe mit Grenze) ist die Rasenfläche das
    /// Grundstück (822,87 m²), der Rand geschlossen; mit „alles“ die Fläche
    /// des ganzen Netzes.
    #[test]
    fn zuschnitt_am_grundstueck() {
        let mut m = lindenhof();
        assert_eq!(Gelaende::aus(&m).unwrap().show, Show::Parcel);
        m.begin("Zuschnitt");
        assert!(m.set_terrain_show(Show::All));
        m.commit();
        let g = Gelaende::aus(&m).unwrap();
        let ganz = flaeche(&g.dreiecke);
        assert!(ganz > 900.0, "{ganz}");
        let innen = flaeche(&im_grundstueck(&g));
        assert!((innen - 822.87).abs() < 0.05, "{innen}");
        // Rasen oben bis auf das Haus, Erdreich unten und am Rand, jede
        // Fläche zeigt nach außen
        let haus = {
            let q: Vec<Vec3> = g.haus[0].0.iter().map(|p| vec3(p[0], p[1], 0.0)).collect();
            polygon::area(&q) / 1e6
        };
        let keys = [10, 11, 12];
        let s = koerper(&g, keys);
        let rasen: Vec<[Vec3; 3]> = s
            .triangles
            .iter()
            .filter(|t| t.mat == 10)
            .map(|t| t.p)
            .collect();
        assert!((flaeche(&rasen) + haus - ganz).abs() < 0.01);
        assert!(s.triangles.iter().all(|t| t.n.length() > 0.99));
        m.begin("Zuschnitt");
        assert!(m.set_terrain_show(Show::Parcel));
        m.commit();
        let g = Gelaende::aus(&m).unwrap();
        let s = koerper(&g, keys);
        let oben: Vec<[Vec3; 3]> = s
            .triangles
            .iter()
            .filter(|t| t.mat == 10)
            .map(|t| t.p)
            .collect();
        assert!((flaeche(&oben) + haus - 822.87).abs() < 0.05);
        // Der Rand reicht überall vom Netz bis zur Unterkante
        let seiten = s
            .triangles
            .iter()
            .filter(|t| t.mat == 11 && t.n.z.abs() < 1e-9)
            .count();
        assert!(seiten > 8, "{seiten}");
        assert!(s
            .triangles
            .iter()
            .all(|t| t.p.iter().all(|p| p.z >= g.unten - 1e-6)));
    }

    /// Mit Hilfsnetz (§4.3a) liegt jede Kante des Netzes einmal fein auf
    /// dem Körper, nicht im Haus, und die vollen Meter werden kräftig;
    /// Geglättet (Jörn 17.10.): je Dreieck 4 bzw. 16, die Messpunkte
    /// bleiben Ecken, die Höhenlinien folgen der runden Fläche; der Körper
    /// bleibt geschlossen wie ohne Glätten.
    #[test]
    fn geglaettet() {
        use sk_model::Glaettung;
        let mut m = lindenhof();
        let eckig = Gelaende::aus(&m).unwrap();
        let ecken: std::collections::HashSet<[u64; 3]> = eckig
            .dreiecke
            .iter()
            .flatten()
            .map(|p| [p.x, p.y, p.z].map(f64::to_bits))
            .collect();
        for (g, je) in [(Glaettung::Fein, 4), (Glaettung::SehrFein, 16)] {
            m.begin("Glätten");
            assert!(m.set_terrain_smoothing(g));
            m.commit();
            let rund = Gelaende::aus(&m).unwrap();
            assert_eq!(rund.dreiecke.len(), eckig.dreiecke.len() * je);
            let alle: std::collections::HashSet<[u64; 3]> = rund
                .dreiecke
                .iter()
                .flatten()
                .map(|p| [p.x, p.y, p.z].map(f64::to_bits))
                .collect();
            assert!(ecken.is_subset(&alle), "Messpunkte bleiben Ecken");
            assert!(!rund.stuecke.is_empty());
            let k = koerper(&rund, [10, 11, 12]);
            assert!(k.triangles.len() > rund.dreiecke.len());
        }
    }

    /// Ohne Höhenlinien (Jörn 17.10.) fehlen nur ihre Kanten; die Höhen
    /// reichen vom tiefsten bis zum höchsten Punkt des Netzes.
    #[test]
    fn koerper_ohne_hoehenlinien() {
        let m = lindenhof();
        let g = Gelaende::aus(&m).unwrap();
        let keys = [10, 11, 12];
        let mit = koerper_mit(&g, keys, false, true);
        let ohne = koerper_mit(&g, keys, false, false);
        assert_eq!(mit.triangles.len(), ohne.triangles.len());
        let fein = |s: &Solid| s.edges.iter().filter(|e| e.kind == edge_kind::FINE).count();
        // fein sind ohne Höhenlinien nur die Fugen zwischen den Schichten
        // am Haus (#261)
        let grube = grube(&g, keys);
        let fugen = grube
            .edges
            .iter()
            .filter(|e| e.kind == edge_kind::FINE)
            .count();
        assert!(fein(&mit) > fugen);
        assert_eq!(fein(&ohne), fugen);
        assert_eq!(mit.edges.len() - fein(&mit), ohne.edges.len() - fugen);
        let (lo, hi) = g.hoehen().unwrap();
        assert!(lo < hi);
        for p in g.dreiecke.iter().flatten() {
            assert!(p.z >= lo && p.z <= hi);
        }
    }

    /// Rasen und Erdreich bleiben gleich.
    #[test]
    fn hilfsnetz_im_koerper() {
        let m = lindenhof();
        let g = Gelaende::aus(&m).unwrap();
        let keys = [10, 11, 12];
        let ohne = koerper(&g, keys);
        let mit = koerper_mit_netz(&g, keys, true);
        assert_eq!(ohne.triangles.len(), mit.triangles.len());
        let zahl = |s: &Solid, k: u8| s.edges.iter().filter(|e| e.kind == k).count();
        // fein sind auch die Fugen zwischen den Schichten am Haus
        let grube = grube(&g, keys);
        let fugen = zahl(&grube, edge_kind::FINE);
        let meter = zahl(&ohne, edge_kind::FINE) - fugen;
        assert!(meter > 0);
        // die vollen Meter sind mit dem Netz Sichtkanten
        assert_eq!(
            zahl(&mit, edge_kind::VIEW),
            zahl(&ohne, edge_kind::VIEW) + meter
        );
        let netz = zahl(&mit, edge_kind::FINE) - fugen;
        let kanten = netzkanten(&g);
        let im_grundstueck = kanten.iter().filter(|k| k.2).count();
        assert_eq!(g.show, Show::Parcel);
        assert_eq!(netz, im_grundstueck);
        // jede Kante des Netzes einmal: mehr als Dreiecke, weniger als
        // dreimal so viele (Rand und Teilung an Grenze und Haus)
        let n = g.dreiecke.len();
        assert!(
            kanten.len() > n && kanten.len() < 3 * n,
            "{} {n}",
            kanten.len()
        );
        // keine im Haus
        let r = &g.haus[0].0;
        let fuge =
            |e: &&sk_model::solid::Edge| grube.edges.iter().any(|x| x.a == e.a && x.b == e.b);
        for e in mit
            .edges
            .iter()
            .filter(|e| e.kind == edge_kind::FINE && !fuge(e))
        {
            let mitte = [(e.a.x + e.b.x) / 2.0, (e.a.y + e.b.y) / 2.0];
            assert!(!im_ring(r, mitte), "{mitte:?}");
        }
    }

    /// Aufmaß 20 × 20 m, die Grenze ragt rechts 4 m darüber hinaus;
    /// gibt das Modell und das Rechteck des Netzes (mm).
    pub(crate) fn grenze_ueber_netz() -> (Model, [f64; 4]) {
        let mut m = Model::with_seed(7);
        let mut text = String::new();
        for (k, (e, n)) in [(0, 0), (20, 0), (20, 20), (0, 20), (10, 10)]
            .iter()
            .enumerate()
        {
            text.push_str(&format!("{} {e} {n} 10.{k}\n", k + 1));
        }
        let (t, ..) = sk_model::Terrain::lesen(text.as_bytes(), "x.txt", "2026-10-17", 32).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        let k = m
            .terrain_border()
            .iter()
            .flatten()
            .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |k, p| {
                [k[0].min(p.x), k[1].min(p.y), k[2].max(p.x), k[3].max(p.y)]
            });
        let ring = [
            [k[0] + 2000.0, k[1] + 2000.0],
            [k[2] + 4000.0, k[1] + 2000.0],
            [k[2] + 4000.0, k[3] - 2000.0],
            [k[0] + 2000.0, k[3] - 2000.0],
        ];
        m.begin("Grenze");
        assert!(m.add_parcel(&ring));
        m.set_terrain_show(Show::Parcel);
        m.commit();
        (m, k)
    }

    /// Endet das Netz vor der Grenze, schließt sein Rand den Körper im
    /// Grundstück (Jörn 17.10.: eine Ecke war durchsichtig).
    #[test]
    fn netz_endet_vor_der_grenze() {
        let (m, k) = grenze_ueber_netz();
        let g = Gelaende::aus(&m).unwrap();
        assert_eq!(g.show, Show::Parcel);
        let s = koerper(&g, [10, 11, 12]);
        let mitte = (k[1] + k[3]) / 2.0;
        let wand = s.triangles.iter().any(|t| {
            t.mat == 11
                && t.p.iter().all(|p| (p.x - k[2]).abs() < 1.0)
                && t.p.iter().any(|p| p.y <= mitte)
                && t.p.iter().any(|p| p.y >= mitte)
        });
        assert!(wand, "Seitenwand am Rand des Netzes im Grundstück");
        // außerhalb des Grundstücks keine
        let draussen = s.triangles.iter().any(|t| {
            t.mat == 11
                && t.p.iter().all(|p| (p.x - k[2]).abs() < 1.0)
                && t.p.iter().all(|p| p.y < k[1] + 2000.0 - 1.0)
        });
        assert!(!draussen);
    }

    /// Ebene Platte mit von Hand gezeichneter Grenze in 3D (Jörn 17.10.):
    /// Das Hilfsnetz zeigt keine Diagonale zwischen den beiden Dreiecken
    /// der Platte, und die Grenze liegt ganz als Kante auf dem Gelände, bei
    /// „blass“ auf dem gehobenen Grundstück. Bei „nicht“ ist sie der Rand.
    #[test]
    fn ebene_platte_grenze_in_3d() {
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(6000.0, 0.0, 0.0),
            vec3(6000.0, 6000.0, 0.0),
            vec3(0.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let ring = [
            [-9000.0, -11000.0],
            [17000.0, -12000.0],
            [15000.0, 4000.0],
            [16000.0, 9000.0],
            [-8000.0, 15000.0],
        ];
        m.begin("Grenze");
        assert!(m.add_parcel(&ring));
        m.commit();
        let umfang: f64 = (0..ring.len())
            .map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                (b[0] - a[0]).hypot(b[1] - a[1])
            })
            .sum();
        let z = m.terrain_z();
        let keys = [10, 11, 12];
        for (show, dz) in [(Show::All, 0.0), (Show::Fade, HEBEN)] {
            m.begin("Zuschnitt");
            m.set_terrain_show(show);
            m.commit();
            let g = Gelaende::aus(&m).unwrap();
            assert_eq!(g.dreiecke.len(), 2, "eben");
            assert!(netzkanten(&g).iter().all(|(a, b, _)| {
                // nur der Rand der Platte, keine Diagonale
                (a.x - b.x).abs() < 1e-6 || (a.y - b.y).abs() < 1e-6
            }));
            let s = koerper_mit_netz(&g, keys, true);
            let auf_der_grenze: f64 = s
                .edges
                .iter()
                .filter(|e| (e.a.z - (z + dz)).abs() < 1e-6 && (e.b.z - (z + dz)).abs() < 1e-6)
                .filter(|e| {
                    let mitte = [(e.a.x + e.b.x) / 2.0, (e.a.y + e.b.y) / 2.0];
                    (0..ring.len()).any(|i| {
                        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                        let (d, l) = ([b[0] - a[0], b[1] - a[1]], (b[0] - a[0]).hypot(b[1] - a[1]));
                        ((mitte[0] - a[0]) * d[1] - (mitte[1] - a[1]) * d[0]).abs() / l < 1e-3
                    })
                })
                .map(|e| (e.b - e.a).length())
                .sum();
            assert!(
                (auf_der_grenze - umfang).abs() < 1.0,
                "{show:?}: {auf_der_grenze} {umfang}"
            );
        }
    }

    /// Nur der Lageplan: eben in Höhe OK Gelände, zugeschnitten auf das
    /// Grundstück ohne das Haus; die Unterlage blass im Grundriss.
    #[test]
    fn eben_aus_dem_lageplan() {
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            vec3(7000.0, 6000.0, 0.0),
            vec3(7000.0, 14000.0, 0.0),
            vec3(17000.0, 14000.0, 0.0),
            vec3(17000.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let roh = include_bytes!("../../crates/sk-gelaende/tests/daten/lageplan-lindenhof-7.pdf");
        let plan = sk_model::SitePlan::read(roh, 32).unwrap();
        m.begin("Lageplan");
        assert!(m.import_parcel(&plan, "lageplan.pdf", None));
        m.commit();
        let g = Gelaende::aus(&m)
            .unwrap()
            .mit_unterlage(&m.underlay_lines(&plan));
        assert_eq!(g.show, Show::Parcel);
        let z = m.terrain_z();
        assert!(g.dreiecke.iter().flatten().all(|p| p.z == z));
        assert_eq!(g.unten, g.aufbau.unten(z - m.plate_settings().thick));
        let s = koerper(&g, [10, 11, 12]);
        let rasen: Vec<[Vec3; 3]> = s
            .triangles
            .iter()
            .filter(|t| t.mat == 10)
            .map(|t| t.p)
            .collect();
        let q: Vec<Vec3> = g.haus[0].0.iter().map(|p| vec3(p[0], p[1], 0.0)).collect();
        let haus = polygon::area(&q) / 1e6;
        assert!(
            (flaeche(&rasen) + haus - 822.87).abs() < 0.1,
            "{}",
            flaeche(&rasen)
        );
        let t = DrawTable::resolve(&m, &sk_ui::theme::Theme::dark());
        let l = linien(&g, ViewKind::Plan, None, None, &t, 1.0, 10.0);
        let blass: Vec<&Helper> = l.iter().filter(|h| h.color == t.background.1).collect();
        assert!(blass.len() > 4, "{}", blass.len());
        for h in blass {
            let m = [
                f64::from(h.a[0] + h.b[0]) / 2.0,
                f64::from(h.a[1] + h.b[1]) / 2.0,
            ];
            assert!(!g.im_haus(m), "Unterlage im Haus: {h:?}");
        }
        assert!(l.iter().any(|h| h.pattern == t.section_dash), "Grenze");
    }

    /// Grundriss: Höhenlinien im Zuschnitt nur im Grundstück, außerhalb
    /// blass oder weg; die Grenze strichpunktiert.
    #[test]
    fn grundriss_und_schnitt() {
        let mut m = lindenhof();
        let t = DrawTable::resolve(&m, &sk_ui::theme::Theme::dark());
        m.begin("Zuschnitt");
        assert!(m.set_terrain_show(Show::All));
        m.commit();
        let g = Gelaende::aus(&m).unwrap();
        let alle = linien(&g, ViewKind::Plan, None, None, &t, 1.0, 10.0);
        assert!(alle.len() > 50, "{}", alle.len());
        m.begin("Zuschnitt");
        m.set_terrain_show(Show::Parcel);
        m.commit();
        let g = Gelaende::aus(&m).unwrap();
        let innen = linien(&g, ViewKind::Plan, None, None, &t, 1.0, 10.0);
        assert!(innen.len() < alle.len());
        for h in &innen {
            let mitte = [
                f64::from(h.a[0] + h.b[0]) / 2.0,
                f64::from(h.a[1] + h.b[1]) / 2.0,
            ];
            // Grenze und Marken der Grenzpunkte liegen auf der Grenze
            let auf_grenze = h.pattern == t.section_dash;
            let grenzpunkt = g
                .ringe
                .iter()
                .flatten()
                .any(|q| (q[0] - mitte[0]).hypot(q[1] - mitte[1]) < 1.0);
            assert!(auf_grenze || grenzpunkt || g.innen(mitte), "{h:?}");
            assert!(auf_grenze || !g.im_haus(mitte), "im Haus: {h:?}");
        }
        // Höhenzahlen: welche, keine im Haus, im Zuschnitt nur innen
        assert!(!g.zahlen.is_empty());
        for (q, _, _, text, voll) in &g.zahlen {
            assert!(!g.im_haus(*q) && g.innen(*q) && *voll, "{q:?} {text}");
        }
        // an jedem gezeigten Messpunkt seine Kote, zwei Nachkommastellen
        let h0 = m.terrain_nhn().unwrap();
        assert!(!g.marken.is_empty());
        for &(p, _) in &g.marken {
            let text = crate::selection::de(p.z / 1000.0 + h0, 2);
            assert!(
                g.koten.iter().any(|z| z.0 == [p.x, p.y] && z.3 == text),
                "{p:?} {text}"
            );
        }
        if let Some(f) = crate::wall_edit::mass_treffer::tests::schrift() {
            let s = sk_plan::schrift::Schrift::new(f);
            assert!(!zahlen_tinte(&g, &s, 250.0, &t).is_empty());
            // blasse Zahlen (außerhalb des Grundstücks bei „Umgebung
            // blass“) stehen im Stift der blassen Höhenlinien, nicht in
            // der Papierfarbe (Review #216 H-216a)
            let mut b = Gelaende::aus(&m).unwrap();
            // gleiche Zahlen, gleiche Prüfsumme; blass ist eine andere
            assert_eq!(zahlen_inhalt(&b), zahlen_inhalt(&g));
            b.zahlen.iter_mut().for_each(|z| z.4 = false);
            b.koten.iter_mut().for_each(|z| z.4 = false);
            assert_ne!(zahlen_inhalt(&b), zahlen_inhalt(&g));
            let tinte = zahlen_tinte(&b, &s, 250.0, &t);
            assert!(!tinte.is_empty());
            let abstand: f32 = (0..3).map(|i| (t.background.1[i] - t.paper[i]).abs()).sum();
            assert!(abstand > 0.3, "{:?} {:?}", t.background, t.paper);
            assert!(tinte.iter().all(|i| i.color == t.background.1));
            // Koten weichen aus (Test #223): an der linken Hauswand steht
            // sie links vom Kreuz, eine zweite fast am selben Punkt
            // entfällt, zwischen zwei Häuserwänden keine
            let ring = &b.haus[0].0;
            let x0 = ring.iter().map(|p| p[0]).fold(f64::MAX, f64::min);
            let (y0, y1) = ring
                .iter()
                .fold((f64::MAX, f64::MIN), |(a, z), p| (a.min(p[1]), z.max(p[1])));
            let q = [x0 - 300.0, (y0 + y1) / 2.0];
            assert!(!b.im_haus(q));
            let kote = |q: P2| (q, 0.0, 0.0, "25,06".to_string(), true);
            b.zahlen.clear();
            b.koten = vec![kote(q)];
            let eine = zahlen_tinte(&b, &s, 250.0, &t);
            assert!(!eine.is_empty());
            let rechts = eine.iter().flat_map(|i| i.p).map(|p| f64::from(p[0]));
            assert!(rechts.fold(f64::MIN, f64::max) < q[0], "links vom Kreuz");
            b.koten.push(kote([q[0] + 40.0, q[1] + 60.0]));
            assert_eq!(zahlen_tinte(&b, &s, 250.0, &t).len(), eine.len());
            // frei stehend rechts
            b.koten = vec![kote([x0 - 20_000.0, q[1]])];
            let frei = zahlen_tinte(&b, &s, 250.0, &t);
            let links = frei.iter().flat_map(|i| i.p).map(|p| f64::from(p[0]));
            assert!(links.fold(f64::MAX, f64::min) > x0 - 20_000.0);
            // frei stehend fast am selben Punkt: eine Kote, nicht eine
            // rechts und eine links desselben Kreuzes (24,30 × 24,41)
            let p = [x0 - 20_000.0, q[1]];
            b.koten.push(kote([p[0] + 40.0, p[1] + 60.0]));
            assert_eq!(zahlen_tinte(&b, &s, 250.0, &t).len(), frei.len());
            // ein Kreuz rechts daneben: die Kote weicht nach links aus
            // statt es zu überschreiben (H-223e)
            b.koten = vec![kote(p), kote([p[0] + 400.0, p[1]])];
            let zwei = zahlen_tinte(&b, &s, 250.0, &t);
            let erste = &zwei[..frei.len()];
            let rechts = erste.iter().flat_map(|i| i.p).map(|p| f64::from(p[0]));
            assert!(rechts.fold(f64::MIN, f64::max) < p[0], "links vom Kreuz");
            assert_eq!(zwei.len(), 2 * frei.len());
            // an der Grenze (Bedienbarkeit #223): 30 cm links einer steilen
            // Grenzkante steht die Kote links vom Kreuz, nicht über dem
            // Strichpunkt
            let grenze = &b.ringe[0];
            let n = grenze.len();
            let (a, e) = (0..n)
                .map(|i| (grenze[i], grenze[(i + 1) % n]))
                .filter(|(a, e)| (e[1] - a[1]).abs() > 4.0 * (e[0] - a[0]).abs())
                .max_by(|x, y| (x.1[1] - x.0[1]).abs().total_cmp(&(y.1[1] - y.0[1]).abs()))
                .expect("steile Grenzkante");
            let mitte = [(a[0] + e[0]) / 2.0, (a[1] + e[1]) / 2.0];
            let qg = [mitte[0] - 300.0, mitte[1]];
            assert_ne!(b.innen(qg), b.innen([mitte[0] + 300.0, mitte[1]]));
            assert!(!b.im_haus(qg));
            b.koten = vec![kote(qg)];
            let tinte = zahlen_tinte(&b, &s, 250.0, &t);
            let rechts = tinte.iter().flat_map(|i| i.p).map(|p| f64::from(p[0]));
            assert!(rechts.fold(f64::MIN, f64::max) < qg[0], "vor der Grenze");
            // geändertes Haus, andere Prüfsumme (die Lage der Koten hängt
            // daran)
            let vorher = zahlen_inhalt(&b);
            b.haus[0].0[0][0] += 1.0;
            assert_ne!(zahlen_inhalt(&b), vorher);
            // ebenso eine verschobene Grenzecke (Review B2 zu #223)
            let vorher = zahlen_inhalt(&b);
            b.ringe[0][0][1] += 1.0;
            assert_ne!(zahlen_inhalt(&b), vorher);
        }
        // Schnitt quer durch das Haus: Schnittfläche bis zur Unterkante
        let (p0, n) = (vec3(12000.0, 10000.0, 0.0), vec3(0.0, 1.0, 0.0));
        let f = SectionFrame::new(p0, n);
        let keys = [10, 11, 12];
        let k = kappen(&g, &f, p0, keys);
        assert!(!k.triangles.is_empty());
        let schichten = schluessel(keys);
        assert!(k
            .triangles
            .iter()
            .all(|t| t.mat & material::CUT != 0 && schichten.contains(&(t.mat & !material::CUT))));
        let l = linien(&g, ViewKind::Section, Some((p0, n)), None, &t, 1.0, 10.0);
        assert!(l.len() > 3);
        // Am Haus endet die Geländelinie, darunter der Bodenaufbau bis zur
        // Aussparung: in der Schürze bis unter die Platte, unter der
        // Gründung gewachsener Boden
        let (ring, uk) = &g.haus[0];
        assert!(*uk < 0.0 && *uk > g.unten, "{uk} {}", g.unten);
        assert_eq!(g.aufbau.haeuser.len(), 1);
        let dach = -200.0;
        let q: Vec<Vec3> = ring.iter().map(|p| vec3(p[0], p[1], 0.0)).collect();
        let [(ha, hb)] = polygon::plane_intervals(&q, p0, f.n, f.along)[..] else {
            panic!("Schnitt trifft das Haus nicht");
        };
        let u = |p: [f32; 3]| {
            let p = vec3(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
            (p - f.pt(0.0, 0.0)).dot(f.along)
        };
        let oben = |h: &&Helper| [h.a, h.b].iter().all(|p| f64::from(p[2]) > g.unten + 1.0);
        for h in l.iter().filter(oben) {
            let m = (u(h.a) + u(h.b)) / 2.0;
            assert!(m < ha || m > hb, "Geländelinie im Haus: {h:?}");
        }
        for tri in &k.triangles {
            let m = tri
                .p
                .iter()
                .map(|p| (*p - f.pt(0.0, 0.0)).dot(f.along))
                .sum::<f64>()
                / 3.0;
            if m > ha && m < hb {
                assert!(tri.p.iter().all(|p| p.z <= dach + 1e-6), "{tri:?}");
            }
        }
        // In 3D kein Rasen im Haus, die Fläche um die Grundfläche kleiner
        let s = koerper(&g, [10, 11, 12]);
        let rasen: Vec<[Vec3; 3]> = s
            .triangles
            .iter()
            .filter(|t| t.mat == 10)
            .map(|t| t.p)
            .collect();
        for d in &rasen {
            let m = [
                (d[0].x + d[1].x + d[2].x) / 3.0,
                (d[0].y + d[1].y + d[2].y) / 3.0,
            ];
            assert!(!g.im_haus(m), "{d:?}");
        }
        let haus = polygon::area(&q) / 1e6;
        assert!(
            (flaeche(&rasen) + haus - 822.87).abs() < 0.05,
            "{} + {haus}",
            flaeche(&rasen)
        );
    }

    /// Eine Geländefläche 0,60 m über dem Gelände vor dem Haus (G3c): Das
    /// Planungsgelände steht statt des Urgeländes, ohne Höhenlinie in der
    /// ebenen Fläche, mit Umriss und Schraffen im Grundriss und dem
    /// Urgelände gestrichelt im Schnitt; der Rasen deckt dieselbe Fläche.
    #[test]
    fn gelaendeflaeche_im_bild() {
        let mut m = lindenhof();
        let t = DrawTable::resolve(&m, &sk_ui::theme::Theme::dark());
        m.begin("Zuschnitt");
        assert!(m.set_terrain_show(Show::All));
        m.commit();
        let ohne = Gelaende::aus(&m).unwrap();
        let ohne_plan = linien(&ohne, ViewKind::Plan, None, None, &t, 1.0, 10.0);
        let z = m.terrain_z_at(None, 12_000.0, 2_500.0) + 600.0;
        let r = vec![
            [9_000.0, 1_000.0],
            [15_000.0, 1_000.0],
            [15_000.0, 4_000.0],
            [9_000.0, 4_000.0],
        ];
        m.begin("Fläche");
        assert!(m
            .add_site_patch(sk_model::TerrainPatch::flat(r, z))
            .is_some());
        m.commit();
        let g = Gelaende::aus(&m).unwrap();
        assert!(!g.ur.is_empty());
        assert_eq!(g.umrisse.len(), 1);
        assert!(!g.schraffen.is_empty());
        assert!((g.hoehe([12_000.0, 2_500.0]).unwrap() - z).abs() < 1e-6);
        // keine Höhenlinie des Urgeländes in der ebenen Fläche
        for &(_, [a, b], _, _) in &g.stuecke {
            let c = lerp2(a, b, 0.5);
            let innen = c[0] > 9_001.0 && c[0] < 14_999.0 && c[1] > 1_001.0 && c[1] < 3_999.0;
            assert!(!innen, "Höhenlinie in der Fläche: {a:?} {b:?}");
        }
        let plan = linien(&g, ViewKind::Plan, None, None, &t, 1.0, 10.0);
        let kante = t.drawing_edges[edge_kind::VIEW as usize];
        let umriss = plan
            .iter()
            .filter(|h| h.color == kante.1 && (h.width - kante.0).abs() < 1e-6)
            .count();
        assert!(umriss >= 4, "{umriss}");
        assert!(plan.len() > ohne_plan.len());
        // die ebene Fläche trägt ihre Höhe in m ü. NHN in der Mitte
        let h0 = m.terrain_nhn().unwrap();
        let soll = nhn_text(z / 1000.0 + h0);
        assert!(
            g.zahlen.iter().any(|(q, w, _, text, _)| *text == soll
                && *w == 0.0
                && (q[0] - 12_000.0).abs() < 1.0
                && (q[1] - 2_500.0).abs() < 1.0),
            "{soll}: {:?}",
            g.zahlen
        );
        // Schnitt längs durch die Fläche: Urgelände gestrichelt
        let (p0, n) = (vec3(12_000.0, 2_500.0, 0.0), vec3(0.0, 1.0, 0.0));
        let l = linien(&g, ViewKind::Section, Some((p0, n)), None, &t, 1.0, 10.0);
        assert!(l.iter().any(|h| h.pattern == t.element_dash));
        let l = linien(&ohne, ViewKind::Section, Some((p0, n)), None, &t, 1.0, 10.0);
        assert!(!l.iter().any(|h| h.pattern == t.element_dash));
        // 3D: Rasen auf derselben Fläche, auf der Fläche in ihrer Höhe
        let rasen = |g: &Gelaende| -> Vec<[Vec3; 3]> {
            koerper(g, [10, 11, 12])
                .triangles
                .iter()
                .filter(|t| t.mat == 10)
                .map(|t| t.p)
                .collect()
        };
        let mit = rasen(&g);
        assert!((flaeche(&mit) - flaeche(&rasen(&ohne))).abs() < 0.01);
        assert!(mit
            .iter()
            .flatten()
            .any(|p| (p.z - z).abs() < 1e-6 && (p.x - 12_000.0).abs() < 3_000.0));
        // der Rand bleibt der Rand des Netzes: die geteilten Kanten am
        // ersetzten Bereich sind keiner (sonst Seiten und Kanten mitten im
        // Gelände); seit G-275a haben sie keinen T-Stoß mehr, so findet
        // auch die Suche über die Dreiecke nur den Rand
        let lang = |r: &[(Vec3, Vec3)]| -> f64 {
            r.iter()
                .map(|(a, b)| ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt())
                .sum()
        };
        let soll = lang(&randkanten(&ohne.dreiecke));
        assert!((lang(g.rand.as_ref().unwrap()) - soll).abs() < 1.0);
        let ueber_dreiecke = lang(&randkanten(&g.dreiecke));
        assert!(
            (ueber_dreiecke - soll).abs() < 1.0,
            "{ueber_dreiecke} {soll}"
        );
        // mit Böschung keine Stufe: Wände nur an den Füßen, die die
        // Planung im Meter tastet (Review #183 H-183g, wenige cm)
        for t in &g.waende {
            let z = t.map(|p| p.z);
            let hoch = z.iter().copied().fold(f64::MIN, f64::max)
                - z.iter().copied().fold(f64::MAX, f64::min);
            assert!(hoch < 50.0, "{t:?}");
        }
        // senkrecht: Wand 0,60 m hoch, im Schnitt ein senkrechtes Stück
        m.begin("senkrecht");
        let mut p = m.site_patches()[0].clone();
        p.slopes = vec![0.0];
        assert!(m.set_site_patch(0, p));
        m.commit();
        let g = Gelaende::aus(&m).unwrap();
        assert!(!g.waende.is_empty());
        let erde = koerper(&g, [10, 11, 12]);
        assert!(erde
            .triangles
            .iter()
            .any(|t| t.mat == 11 && t.n.z.abs() < 1e-9 && (t.p[0].x - 12_000.0).abs() < 3_100.0));
        let senkrecht = g
            .profil(&SectionFrame::new(p0, n), p0)
            .iter()
            .filter(|(a, b, _)| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() > 1.0)
            .count();
        assert!(senkrecht >= 2, "{senkrecht}");
    }

    /// Bis [`PUNKT_KOTEN`] Messpunkte hat jeder seine Kote, darüber keiner.
    #[test]
    fn koten_bis_punktgrenze() {
        use std::fmt::Write;
        let aufmass = |k: usize| {
            let mut txt = String::new();
            for i in 0..k {
                for j in 0..k {
                    let z = 10.0 + 0.01 * (i + j) as f64;
                    let _ = writeln!(txt, "{} {} {} {z}", i * k + j + 1, 2 * i, 2 * j);
                }
            }
            let (t, ..) =
                sk_model::Terrain::lesen(txt.as_bytes(), "t.txt", "2026-10-16", 32).unwrap();
            let mut m = Model::with_seed(3);
            m.begin("Aufmaß");
            assert!(m.import_terrain(t));
            m.commit();
            let g = Gelaende::aus(&m).unwrap();
            let koten = g
                .marken
                .iter()
                .filter(|(p, _)| g.koten.iter().any(|z| z.0 == [p.x, p.y]))
                .count();
            (g.marken.len(), koten)
        };
        // 44 × 44 = 1.936 Punkte: jeder mit Kote; 45 × 45 = 2.025: keine
        let (n, k) = aufmass(44);
        assert!(n <= PUNKT_KOTEN && k == n, "{n} {k}");
        let (n, k) = aufmass(45);
        assert!(n > PUNKT_KOTEN && k == 0, "{n} {k}");
    }

    /// Eine geschlossene Höhenlinie bleibt geschlossen (Jörn 17.10.): Um
    /// eine Kuppe fehlte das Stück vom letzten zum ersten Punkt, in 3D wie
    /// im Grundriss.
    #[test]
    fn hoehenlinie_um_kuppe_geschlossen() {
        let mut m = Model::with_seed(7);
        let mut text = String::new();
        for i in 0..5 {
            for j in 0..5 {
                let h = if (i, j) == (2, 2) { 10.6 } else { 10.0 };
                text.push_str(&format!("{} {} {} {h}\n", i * 5 + j + 1, 10 * j, 10 * i));
            }
        }
        let (t, ..) = sk_model::Terrain::lesen(text.as_bytes(), "x.txt", "2026-10-17", 32).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        let h0 = m.terrain_nhn().unwrap();
        let g = Gelaende::aus(&m).unwrap();
        for z in [10.25, 10.5] {
            let z = (z - h0) * 1000.0;
            let mut enden: Vec<[i64; 2]> = g
                .stuecke
                .iter()
                .filter(|s| (s.0 - z).abs() < 1e-6)
                .flat_map(|s| s.1)
                .map(|q| [q[0].round() as i64, q[1].round() as i64])
                .collect();
            assert!(enden.len() >= 8, "{z}: {}", enden.len());
            enden.sort_unstable();
            for k in enden.chunk_by(|a, b| a == b) {
                assert_eq!(k.len(), 2, "{z}: offenes Ende {:?}", k[0]);
            }
        }
    }

    /// Höhenzahlen (Ist-Bilder zu #202): je 40 m eine, die erste nach 20 m,
    /// eine kurze Linie in der Mitte; nur Hauptlinien, wenn es welche
    /// gibt; lesbar von unten oder rechts; nicht, wo `frei` nein sagt.
    #[test]
    fn hoehenzahlen_an_linien() {
        let gerade = |z: f64, von: P2, bis: P2, haupt| (z, vec![von, bis], haupt);
        let frei = |_: P2| Some(true);
        let z = hoehenzahlen(
            &[gerade(1_000.0, [100_000.0, 0.0], [0.0, 0.0], true)],
            50.0,
            frei,
        );
        let x: Vec<f64> = z.iter().map(|(q, ..)| q[0]).collect();
        assert_eq!(x, [80_000.0, 40_000.0, 0.0]);
        assert!(z.iter().all(|(_, w, _, t, _)| *w == 0.0 && t == "51"));
        // kurz: eine in der Mitte; Nebenlinien nur ohne Hauptlinien
        let kurz = gerade(250.0, [0.0, 0.0], [0.0, 10_000.0], false);
        let z = hoehenzahlen(std::slice::from_ref(&kurz), 50.0, frei);
        assert_eq!(z.len(), 1);
        assert_eq!(z[0].0, [0.0, 5_000.0]);
        assert_eq!(z[0].3, "50,25");
        assert!((z[0].1 - FRAC_PI_2).abs() < 1e-9);
        let haupt = gerade(1_000.0, [0.0, 0.0], [10_000.0, 0.0], true);
        assert_eq!(hoehenzahlen(&[kurz.clone(), haupt], 50.0, frei).len(), 1);
        // abwärts gezeichnet steht die Zahl wie aufwärts
        let ab = gerade(250.0, [0.0, 10_000.0], [0.0, 0.0], false);
        assert!((hoehenzahlen(&[ab], 50.0, frei)[0].1 - FRAC_PI_2).abs() < 1e-9);
        assert!((lesbar([0.0, 0.0], [-1.0, -1.0]) - std::f64::consts::FRAC_PI_4).abs() < 1e-9);
        // im Haus oder außerhalb des Zuschnitts keine, blass bleibt blass
        assert!(hoehenzahlen(std::slice::from_ref(&kurz), 50.0, |_| None).is_empty());
        let lang = gerade(1_000.0, [0.0, 0.0], [100_000.0, 0.0], true);
        let ab_30 = |q: P2| (q[0] >= 30_000.0).then_some(true);
        let x: Vec<f64> = hoehenzahlen(&[lang], 50.0, ab_30)
            .iter()
            .map(|(q, ..)| q[0])
            .collect();
        assert_eq!(
            x,
            [30_000.0, 70_000.0],
            "rückt bis zur ersten freien Stelle"
        );
        assert!(!hoehenzahlen(&[kurz], 50.0, |_| Some(false))[0].4);
        // Flächen: eben eine Zahl im Schwerpunkt, mit Gefälle keine
        let eben = vec![
            vec3(0.0, 0.0, 600.0),
            vec3(6_000.0, 0.0, 600.0),
            vec3(6_000.0, 3_000.0, 600.0),
            vec3(0.0, 3_000.0, 600.0),
        ];
        let mut schief = eben.clone();
        schief[2].z = 900.0;
        let f = flaechenzahlen(&[eben, schief], 50.0, frei);
        assert_eq!(f.len(), 1);
        let [x, y] = f[0].0;
        assert!(
            (x - 3_000.0).abs() < 1e-6 && (y - 1_500.0).abs() < 1e-6,
            "{x} {y}"
        );
        assert_eq!(f[0].3, "50,60");
        assert_eq!(nhn_text(-0.5), "-0,50");
    }

    /// Tempo (Review H-223b): Tinte der Höhenzahlen bei 1.936 Koten, knapp
    /// unter [`PUNKT_KOTEN`] (`--ignored --nocapture`, Release). Sie
    /// entsteht nur nach einer Änderung neu, sonst zeichnet das Bild die
    /// gemerkten Dreiecke.
    #[test]
    #[ignore]
    fn tempo_koten() {
        use std::fmt::Write;
        use std::time::Instant;
        let Some(f) = crate::wall_edit::mass_treffer::tests::schrift() else {
            return;
        };
        let s = sk_plan::schrift::Schrift::new(f);
        let mut txt = String::new();
        let k = 44;
        for i in 0..k {
            for j in 0..k {
                let z = 10.0 + 0.01 * (i + j) as f64;
                let _ = writeln!(txt, "{} {} {} {z}", i * k + j + 1, 2 * i, 2 * j);
            }
        }
        let (t, ..) = sk_model::Terrain::lesen(txt.as_bytes(), "t.txt", "2026-10-16", 32).unwrap();
        let mut m = Model::with_seed(3);
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        let g = Gelaende::aus(&m).unwrap();
        assert_eq!(g.koten.len(), k * k);
        let t = DrawTable::resolve(&m, &sk_ui::theme::Theme::dark());
        let mut tinte = 0;
        let mut best = f64::MAX;
        for _ in 0..5 {
            let t0 = Instant::now();
            tinte = zahlen_tinte(&g, &s, 250.0, &t).len();
            best = best.min(t0.elapsed().as_secs_f64() * 1e3);
        }
        let t0 = Instant::now();
        let _ = zahlen_inhalt(&g);
        let pruef = t0.elapsed().as_secs_f64() * 1e3;
        eprintln!(
            "{} Koten, {} Zahlen: Tinte {best:.1} ms, {tinte} Dreiecke, \
             Prüfsumme {pruef:.2} ms",
            g.koten.len(),
            g.zahlen.len()
        );
    }

    /// Tempo (Review #183 B1): Bild und Schnitt bei 100 Flächen mit je 400
    /// Kanten auf 650.000 Dreiecken (`--ignored --nocapture`, Release).
    #[test]
    #[ignore]
    fn tempo_planungsgelaende() {
        use std::fmt::Write;
        use std::time::Instant;
        let mut txt = String::new();
        let k = 571;
        for i in 0..k {
            for j in 0..k {
                let (x, y) = (i as f64 * 0.25, j as f64 * 0.25);
                let z = 10.0 + 0.4 * (x / 9.0).sin() + 0.3 * (y / 7.0).cos();
                let _ = writeln!(txt, "{} {x} {y} {z}", i * k + j);
            }
        }
        let (t, ..) = sk_model::Terrain::lesen(txt.as_bytes(), "t.txt", "2026-10-15", 32).unwrap();
        let mut m = Model::with_seed(7);
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        assert!(m.set_terrain_nhn(9.5));
        m.commit();
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1e3;
        m.begin("Flächen");
        for f in 0..100 {
            let (cx, cy) = (
                -10_000.0 + (f % 10) as f64 * 11_000.0,
                30_000.0 + (f / 10) as f64 * 9_000.0,
            );
            let r: Vec<[f64; 2]> = (0..400)
                .map(|i| {
                    let w = i as f64 / 400.0 * std::f64::consts::TAU;
                    [cx + 4000.0 * w.cos(), cy + 4000.0 * w.sin()]
                })
                .collect();
            let z = if f % 2 == 0 { 600.0 } else { -200.0 };
            assert!(m
                .add_site_patch(sk_model::TerrainPatch::flat(r, z))
                .is_some());
        }
        m.commit();
        let t0 = Instant::now();
        let g = Gelaende::aus(&m).unwrap();
        let kalt = ms(t0);
        let t0 = Instant::now();
        let _ = Gelaende::aus(&m).unwrap();
        let warm = ms(t0);
        let (p0, n) = (vec3(0.0, 34_000.0, 0.0), vec3(0.0, 1.0, 0.0));
        let f = SectionFrame::new(p0, n);
        let mut schnitt = f64::MAX;
        let mut stuecke = 0;
        for _ in 0..5 {
            let t0 = Instant::now();
            stuecke = g.profil(&f, p0).len();
            schnitt = schnitt.min(ms(t0));
        }
        eprintln!(
            "{} Dreiecke, {} Wände: Bild kalt {kalt:.0} ms, warm {warm:.0} ms, \
             Schnitt {schnitt:.1} ms ({stuecke} Stücke)",
            g.dreiecke.len(),
            g.waende.len()
        );
    }
}

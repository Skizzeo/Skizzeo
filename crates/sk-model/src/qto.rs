//! Mengen je Bauteil (IFC: Qto_WallBaseQuantities), aus der Parametrik
//! berechnet, nie aus dem Anzeigenetz. Einheit mm, mm², mm³ und kg.
//!
//! Schichtmengen und Volumen sind netto: verschnitten an den Anschlüssen
//! zwischen Wandzügen (B5a). Grundfläche, Ansichtsflächen und
//! `volume_gross` bleiben brutto (ohne Verschnitt). Öffnungen gibt es noch nicht.

use crate::element::{BuildingId, Category, ElementId, ElementKind, RunId, StoreyId};
use crate::floor::{FloorError, FloorSlab};
use crate::foundation::{Foundation, FoundationError};
use crate::library::{LayerSetId, MatCategory, MaterialId};
use crate::model::Model;
use crate::wall::WallChain;
use sk_math::Vec3;
use std::collections::HashMap;

/// Mengen einer Schicht in einem Wandsegment.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerQto {
    pub material: MaterialId,
    /// Dicke in mm.
    pub thickness: f64,
    /// Länge der Schichtmittellinie im Segment (mm).
    pub length: f64,
    /// Grundfläche der Schicht im Segment, verschnitten an Ecken und Anschlüssen (mm²).
    pub area: f64,
    /// Grundfläche × Höhe (mm³).
    pub volume: f64,
    /// Volumen × Rohdichte (kg).
    pub mass: f64,
    /// Volumen, das eine Geschossdecke aus der Schicht nimmt (Auflagertasche
    /// bzw. Deckenstreifen), mm³: brutto minus netto.
    pub pocket: f64,
    /// Außenfläche der Schicht: Länge der äußeren Schichtkante × Höhe (mm²),
    /// für die Abrechnung des WDVS nach Fläche.
    pub side_area: f64,
}

/// Mengen eines Wandsegments.
#[derive(Clone, Debug, PartialEq)]
pub struct WallQto {
    /// Länge auf der Bezugslinie (IFC Length), mm.
    pub length: f64,
    /// Gesamtdicke (IFC Width), mm.
    pub width: f64,
    /// Höhe (IFC Height), mm.
    pub height: f64,
    /// Grundfläche ohne Verschnitt mit anderen Zügen (IFC GrossFootprintArea), mm².
    pub footprint: f64,
    /// Außenfläche: Länge der Außenkante × Höhe (IFC GrossSideArea), mm².
    pub side_outer: f64,
    /// Innenfläche: Länge der Innenkante × Höhe, mm².
    pub side_inner: f64,
    /// Summe der Schichtvolumen, netto (IFC NetVolume), mm³.
    pub volume: f64,
    /// Volumen ohne Verschnitt mit anderen Zügen (IFC GrossVolume), mm³.
    pub volume_gross: f64,
    /// Schichten von außen nach innen.
    pub layers: Vec<LayerQto>,
    /// Volumen, das die Decke aus der Wand nimmt (Summe der Schichten), mm³.
    pub pocket: f64,
    /// Länge in der Mengenliste (mm): Außenwand Länge der Außenseite,
    /// Innenwand lichte Länge der tragenden Schicht.
    pub list_length: f64,
}

/// Fläche eines ebenen Vielecks in der Grundrissebene.
fn area(p: &[Vec3]) -> f64 {
    let n = p.len();
    let twice: f64 = (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    twice.abs() * 0.5
}

/// Mengen aller Segmente eines Wandzugs, in Segmentreihenfolge.
pub fn run_qto(model: &Model, run: RunId) -> Vec<WallQto> {
    let (Some(chain), Some(r)) = (model.chain(run), model.run(run)) else {
        return Vec::new();
    };
    let Some(set) = r
        .segments
        .first()
        .and_then(|e| model.element(*e))
        .and_then(|e| e.layer_set)
        .and_then(|s| model.layer_set(s))
    else {
        return Vec::new();
    };
    let category = model.element(r.segments[0]).map(|e| e.category);
    let pts = chain.clean_points();
    let offsets = chain.layer_offsets();
    // Außenkante einer Schicht: `a` (kleinerer Versatz), wenn außen links liegt
    let outer_first = chain.outer_offset() <= chain.inner_offset();
    let h = chain.height;
    let gross = WallChain {
        joints: Default::default(),
        ..chain.clone()
    };
    let (c_out, c_in) = (
        gross.face_corners(gross.outer_offset()),
        gross.face_corners(gross.inner_offset()),
    );
    let gross_faces: Vec<(Vec<Vec3>, Vec<Vec3>)> = offsets
        .iter()
        .map(|&(a, b, _)| (gross.face_corners(a), gross.face_corners(b)))
        .collect();
    let faces: Vec<(Vec<Vec3>, Vec<Vec3>)> = offsets
        .iter()
        .enumerate()
        .map(|(i, &(a, b, _))| {
            (
                chain.face_corners_in(a, Some(i)),
                chain.face_corners_in(b, Some(i)),
            )
        })
        .collect();
    let n = pts.len();
    (0..chain.segment_count())
        .map(|k| {
            let j = (k + 1) % n;
            let layers: Vec<LayerQto> = set
                .layers
                .iter()
                .zip(&faces)
                .enumerate()
                .map(|(li, (l, (fa, fb)))| {
                    let quad = [fa[k], fa[j], fb[j], fb[k]];
                    let area = area(&quad);
                    // netto: ohne das Band einer Geschossdecke (Auflagertasche)
                    let hn: f64 = chain.layer_spans(li).iter().map(|(a, b)| b - a).sum();
                    let volume = area * hn;
                    let density = model.material(l.material).map_or(0.0, |m| m.density);
                    let (la, lb) = ((fa[j] - fa[k]).length(), (fb[j] - fb[k]).length());
                    LayerQto {
                        material: l.material,
                        thickness: l.thickness,
                        length: (la + lb) * 0.5,
                        area,
                        volume,
                        mass: volume * 1e-9 * density,
                        pocket: (area * h - volume).max(0.0),
                        side_area: if outer_first { la } else { lb } * h,
                    }
                })
                .collect();
            let footprint: f64 = gross_faces
                .iter()
                .map(|(fa, fb)| area(&[fa[k], fa[j], fb[j], fb[k]]))
                .sum();
            let side_outer = (c_out[j] - c_out[k]).length() * h;
            let list_length = if category == Some(Category::ExteriorWall) && h > 0.0 {
                side_outer / h
            } else {
                // lichte Länge: Mittellinie der (ersten) tragenden Schicht
                set.layers
                    .iter()
                    .position(|l| l.core)
                    .and_then(|i| layers.get(i))
                    .map_or((pts[j] - pts[k]).length(), |l| l.length)
            };
            WallQto {
                length: (pts[j] - pts[k]).length(),
                width: chain.thickness(),
                height: h,
                footprint,
                side_outer,
                side_inner: (c_in[j] - c_in[k]).length() * h,
                volume: layers.iter().map(|l| l.volume).sum(),
                volume_gross: footprint * h,
                pocket: layers.iter().map(|l| l.pocket).sum(),
                list_length,
                layers,
            }
        })
        .collect()
}

/// Mengen einer Wand.
pub fn wall_qto(model: &Model, wall: ElementId) -> Option<WallQto> {
    let (run, seg) = model.segment_of(wall)?;
    run_qto(model, run).into_iter().nth(seg)
}

/// Mengen einer Sohlplatte (IFC: Qto_SlabBaseQuantities), Hauptmenge Fläche.
#[derive(Clone, Debug, PartialEq)]
pub struct SlabQto {
    /// Fläche des Umrisses (mm²).
    pub area: f64,
    pub volume: f64,
    /// Umfang (Randschalung), mm.
    pub perimeter: f64,
    pub thickness: f64,
    pub recess: f64,
}

/// Mengen einer Frostschürze (IFC: Qto_FootingBaseQuantities), Hauptmenge Länge.
#[derive(Clone, Debug, PartialEq)]
pub struct FootingQto {
    /// Länge auf der Mittellinie (mm).
    pub length: f64,
    /// Volumen aus Ringfläche × Tiefe (mm³).
    pub volume: f64,
    pub width: f64,
    pub depth: f64,
}

/// Mengen der Gründung unter einem Wandzug; `None` ohne Sohlplatte oder wenn
/// kein Körper entstehen kann.
pub fn foundation_qto(model: &Model, run: RunId) -> Option<(SlabQto, FootingQto)> {
    Some(foundation_qto_of(&model.foundation(run)?.ok()?))
}

/// Mengen aus einer schon berechneten Gründung.
pub fn foundation_qto_of(f: &Foundation) -> (SlabQto, FootingQto) {
    let p = f.params;
    (
        SlabQto {
            area: f.slab_area(),
            volume: f.slab_volume(),
            perimeter: f.slab_perimeter(),
            thickness: p.slab_thickness,
            recess: p.recess,
        },
        FootingQto {
            length: f.footing_axis_length(),
            volume: f.footing_volume(),
            width: p.footing_width,
            depth: p.footing_depth,
        },
    )
}

/// Mengen einer Geschossdecke (IFC: Qto_SlabBaseQuantities), Hauptmenge Fläche.
#[derive(Clone, Debug, PartialEq)]
pub struct FloorQto {
    /// Fläche des Umrisses bis Außenseite Kern (mm²).
    pub area: f64,
    pub volume: f64,
    /// Umfang (Randschalung), mm.
    pub perimeter: f64,
    pub thickness: f64,
    /// Oberkante über Wandfuß (mm).
    pub top: f64,
    /// „davon Auflager in den Außenwänden“: Summe der Taschen in den Wänden
    /// des Zugs (mm³), im Volumen enthalten. 0, solange nicht bekannt
    /// ([`floor_qto_of`] kennt die Wände nicht).
    pub bearing: f64,
}

/// Mengen der Decke über einem Wandzug; `None` ohne Decke oder wenn kein
/// Körper entstehen kann.
pub fn floor_qto(model: &Model, run: RunId) -> Option<FloorQto> {
    let mut q = floor_qto_of(&model.floor(run)?.ok()?);
    q.bearing = run_qto(model, run).iter().map(|w| w.pocket).sum();
    Some(q)
}

/// Mengen aus einer schon berechneten Decke.
pub fn floor_qto_of(f: &FloorSlab) -> FloorQto {
    FloorQto {
        area: f.area(),
        volume: f.volume(),
        perimeter: f.perimeter(),
        thickness: f.params.thickness,
        top: f.params.top,
        bearing: 0.0,
    }
}

// --- Mengenliste (B7) ----------------------------------------------------

/// Mengen eines Bauteils in der Liste.
#[derive(Clone, Debug, PartialEq)]
pub enum ElementQto {
    Wall(WallQto),
    Slab(SlabQto),
    Footing(FootingQto),
    Floor(FloorQto),
}

impl ElementQto {
    /// Volumen (mm³), bei Wänden netto.
    pub fn volume(&self) -> f64 {
        match self {
            ElementQto::Wall(w) => w.volume,
            ElementQto::Slab(s) => s.volume,
            ElementQto::Footing(f) => f.volume,
            ElementQto::Floor(f) => f.volume,
        }
    }
}

/// Eine Zeile der Liste: ein Bauteil. `q` ist `None`, wenn kein Körper
/// entsteht; dann steht in `note` der Grund.
#[derive(Clone, Debug, PartialEq)]
pub struct RowQto {
    pub element: ElementId,
    pub number: String,
    pub q: Option<ElementQto>,
    pub note: Option<String>,
}

/// Summen einer Gruppe (mm, mm², mm³), aus ungerundeten Werten.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Totals {
    /// Bauteile mit Körper.
    pub count: usize,
    /// Länge in der Liste (Wände, Frostschürze).
    pub length: f64,
    /// Fläche (Platten, Decken).
    pub area: f64,
    /// Volumen, bei Wänden netto.
    pub volume: f64,
    /// Wände: Abzug Deckenauflager bzw. Deckenstreifen; Decken: davon
    /// Auflager in den Außenwänden.
    pub pocket: f64,
}

/// Gruppe: eine Bauteilart (Wände zusätzlich je Aufbau) in einem Geschoss.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupQto {
    pub category: Category,
    pub layer_set: Option<LayerSetId>,
    pub rows: Vec<RowQto>,
    pub total: Totals,
}

/// Geschoss (bzw. Fundament) mit seinen Gruppen.
#[derive(Clone, Debug, PartialEq)]
pub struct StoreyQto {
    pub id: StoreyId,
    pub groups: Vec<GroupQto>,
}

/// Summe eines Baustoffs in einem Gebäude; `area` (mm²) nur für Dämmung.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialSum {
    pub material: MaterialId,
    pub volume: f64,
    pub area: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildingQto {
    pub id: BuildingId,
    pub storeys: Vec<StoreyQto>,
    pub by_material: Vec<MaterialSum>,
}

/// Die Mengenliste: abgeleitet, nie gespeichert.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Schedule {
    pub buildings: Vec<BuildingQto>,
    /// Bauteile in Geschossen ohne Gebäude (Vorlage), damit keines fehlt.
    pub loose: Vec<StoreyQto>,
}

/// Reihenfolge der Gruppen nach Bauablauf.
fn group_rank(c: Category) -> u8 {
    match c {
        Category::StripFooting => 0,
        Category::GroundSlab => 1,
        Category::ExteriorWall => 2,
        Category::InteriorWall => 3,
        Category::Floor => 4,
        _ => 5,
    }
}

/// Reihenfolge der Baustoffsummen.
fn material_rank(c: MatCategory) -> u8 {
    match c {
        MatCategory::Concrete => 0,
        MatCategory::Masonry => 1,
        MatCategory::Timber => 2,
        MatCategory::Insulation => 3,
        MatCategory::Plaster => 4,
    }
}

/// Baut die Mengenliste: Gebäude → Fundament und Geschosse von unten →
/// Bauteilart (Wände je Aufbau) → Bauteile nach Nummer. Nur Lesen.
pub fn schedule(model: &Model) -> Schedule {
    // Je Wandzug einmal rechnen
    let mut walls: HashMap<RunId, Vec<WallQto>> = HashMap::new();
    let mut found: HashMap<RunId, Result<Foundation, FoundationError>> = HashMap::new();
    let mut floors: HashMap<RunId, Result<FloorSlab, FloorError>> = HashMap::new();
    // (Geschoss der Gruppe, Rang, Aufbau) → Zeilen mit Baustoffanteilen
    type Key = (StoreyId, u8, Option<LayerSetId>);
    let mut groups: HashMap<Key, (Category, Vec<RowQto>)> = HashMap::new();
    for (id, e) in model.elements().iter() {
        let Some(run) = model.run_of(id) else {
            continue;
        };
        let (q, note) = match e.kind {
            ElementKind::Wall(w) => {
                let qs = walls.entry(run).or_insert_with(|| run_qto(model, run));
                match qs.get(w.seg as usize) {
                    Some(q) => (Some(ElementQto::Wall(q.clone())), None),
                    None => (None, Some("Kein Körper: Wandzug ungültig".to_string())),
                }
            }
            ElementKind::GroundSlab(_) | ElementKind::StripFooting(_) => {
                let f = found
                    .entry(run)
                    .or_insert_with(|| match model.foundation(run) {
                        Some(r) => r,
                        None => Err(FoundationError::NotClosed),
                    });
                match f {
                    Ok(f) => {
                        let (s, fs) = foundation_qto_of(f);
                        let q = if matches!(e.kind, ElementKind::GroundSlab(_)) {
                            ElementQto::Slab(s)
                        } else {
                            ElementQto::Footing(fs)
                        };
                        (Some(q), None)
                    }
                    Err(err) => (None, Some(foundation_note(*err))),
                }
            }
            ElementKind::Floor(_) => {
                let f = floors.entry(run).or_insert_with(|| match model.floor(run) {
                    Some(r) => r,
                    None => Err(FloorError::NotClosed),
                });
                match f {
                    Ok(f) => {
                        let mut q = floor_qto_of(f);
                        let qs = walls.entry(run).or_insert_with(|| run_qto(model, run));
                        q.bearing = qs.iter().map(|w| w.pocket).sum();
                        (Some(ElementQto::Floor(q)), None)
                    }
                    Err(err) => (
                        None,
                        Some(format!(
                            "Kein Körper: {}",
                            crate::model::explain_floor(*err)
                        )),
                    ),
                }
            }
        };
        // Fundament: Kostengruppe 322 unter dem Gründungsband
        let storey = match e.category {
            Category::GroundSlab | Category::StripFooting => {
                model.foundation_level_of(e.storey).unwrap_or(e.storey)
            }
            _ => e.storey,
        };
        let set = match e.kind {
            ElementKind::Wall(_) => e.layer_set,
            _ => None,
        };
        groups
            .entry((storey, group_rank(e.category), set))
            .or_insert_with(|| (e.category, Vec::new()))
            .1
            .push(RowQto {
                element: id,
                number: e.number.clone(),
                q,
                note,
            });
    }

    let mut keys: Vec<Key> = groups.keys().copied().collect();
    // Aufbauten in fester Reihenfolge (Name, dann Kennung)
    let set_name = |s: Option<LayerSetId>| {
        s.and_then(|s| model.layer_set(s))
            .map_or(String::new(), |x| x.name.clone())
    };
    keys.sort_by(|a, b| {
        (a.1, set_name(a.2), a.2.map(|s| s.index())).cmp(&(
            b.1,
            set_name(b.2),
            b.2.map(|s| s.index()),
        ))
    });
    let storey_qto = |sid: StoreyId, groups: &mut HashMap<Key, (Category, Vec<RowQto>)>| {
        let mut out = Vec::new();
        for k in keys.iter().filter(|k| k.0 == sid) {
            let (category, mut rows) = groups.remove(k).unwrap_or((Category::Space, Vec::new()));
            rows.sort_by(|a, b| a.number.cmp(&b.number));
            let total = totals(&rows);
            out.push(GroupQto {
                category,
                layer_set: k.2,
                rows,
                total,
            });
        }
        StoreyQto {
            id: sid,
            groups: out,
        }
    };

    let mut sched = Schedule::default();
    // Gebäude nach Nummer (GB-01, GB-02 …), nicht nach Lage im Speicher
    let mut buildings: Vec<_> = model.buildings().iter().collect();
    buildings.sort_by(|a, b| a.1.number.cmp(&b.1.number));
    for (bid, _) in buildings {
        let storeys: Vec<StoreyQto> = model
            .levels_in(Some(bid))
            .into_iter()
            .map(|sid| storey_qto(sid, &mut groups))
            .filter(|s| !s.groups.is_empty())
            .collect();
        let by_material = material_sums(model, &storeys);
        sched.buildings.push(BuildingQto {
            id: bid,
            storeys,
            by_material,
        });
    }
    // Was übrig ist, liegt in Geschossen ohne Gebäude
    let mut rest: Vec<StoreyId> = groups.keys().map(|k| k.0).collect();
    rest.sort_by(|a, b| {
        let z = |s: &StoreyId| model.storey(*s).map_or(0.0, |x| x.elevation);
        z(a).total_cmp(&z(b))
    });
    rest.dedup();
    for sid in rest {
        let s = storey_qto(sid, &mut groups);
        if !s.groups.is_empty() {
            sched.loose.push(s);
        }
    }
    sched
}

/// Grund, warum Sohlplatte und Frostschürze keinen Körper haben.
fn foundation_note(e: FoundationError) -> String {
    match e {
        FoundationError::RecessTooLarge => "Kein Körper: Rücksprung zu groß".to_string(),
        e => format!("Kein Körper: {}", crate::model::explain(e)),
    }
}

/// Summen einer Gruppe.
fn totals(rows: &[RowQto]) -> Totals {
    let mut t = Totals::default();
    for q in rows.iter().filter_map(|r| r.q.as_ref()) {
        t.count += 1;
        t.volume += q.volume();
        match q {
            ElementQto::Wall(w) => {
                t.length += w.list_length;
                t.pocket += w.pocket;
            }
            ElementQto::Slab(s) => t.area += s.area,
            ElementQto::Footing(f) => t.length += f.length,
            ElementQto::Floor(f) => {
                t.area += f.area;
                t.pocket += f.bearing;
            }
        }
    }
    t
}

/// Summe nach Baustoff über alle Bauteile mit Körper; Dämmung auch als Fläche.
fn material_sums(model: &Model, storeys: &[StoreyQto]) -> Vec<MaterialSum> {
    let mut sums: Vec<MaterialSum> = Vec::new();
    let mut add = |m: MaterialId, v: f64, a: f64| {
        let ins = model
            .material(m)
            .is_some_and(|x| x.category == MatCategory::Insulation);
        let area = ins.then_some(a);
        match sums.iter_mut().find(|s| s.material == m) {
            Some(s) => {
                s.volume += v;
                if let (Some(x), Some(y)) = (s.area.as_mut(), area) {
                    *x += y;
                }
            }
            None => sums.push(MaterialSum {
                material: m,
                volume: v,
                area,
            }),
        }
    };
    for row in storeys.iter().flat_map(|s| &s.groups).flat_map(|g| &g.rows) {
        let Some(q) = &row.q else { continue };
        let mat = model.element(row.element).and_then(|e| match e.kind {
            ElementKind::GroundSlab(s) => Some(s.material),
            ElementKind::StripFooting(f) => Some(f.material),
            ElementKind::Floor(f) => Some(f.material),
            ElementKind::Wall(_) => None,
        });
        match (q, mat) {
            (ElementQto::Wall(w), _) => {
                for l in &w.layers {
                    add(l.material, l.volume, l.side_area);
                }
            }
            (q, Some(m)) => add(m, q.volume(), 0.0),
            _ => {}
        }
    }
    sums.sort_by_key(|s| {
        (
            model
                .material(s.material)
                .map_or(9, |m| material_rank(m.category)),
            s.material.index(),
        )
    });
    sums
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wall::RefSide;
    use sk_math::vec3;

    const M3: f64 = 1e9;

    #[test]
    fn rechteck_10_x_8() {
        let mut m = Model::with_seed(1);
        let set = m.defaults().exterior_wall;
        // Außenkante auf der Bezugslinie, im Uhrzeigersinn, Bezugsseite links
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.eg_at(2750.0);
        let r = m
            .add_wall_run(&pts, true, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap();
        let q = run_qto(&m, r);
        assert_eq!(q.len(), 4);
        let vol: f64 = q.iter().map(|w| w.volume).sum();
        // netto ohne die Tasche der Erdgeschossdecke (B10): 30,0935 − 1,31593
        assert!((vol / M3 - 28.7776).abs() < 1e-4, "{}", vol / M3);
        let layer = |i: usize| -> f64 { q.iter().map(|w| w.layers[i].volume).sum::<f64>() / M3 };
        assert!((layer(0) - 13.6444).abs() < 1e-4, "{}", layer(0));
        assert!((layer(1) - 15.1332).abs() < 1e-4, "{}", layer(1));
        let len: f64 = q.iter().map(|w| w.length).sum();
        assert!((len - 36000.0).abs() < 1e-6);
        assert!(q.iter().all(|w| (w.width - 315.0).abs() < 1e-9));
        // Obere Wand 10 m: außen 10,00 m, innen 9,37 m lang
        let top = &q[1];
        assert!((top.side_outer / 1e6 - 27.5).abs() < 1e-9);
        assert!((top.side_inner / 1e6 - 9.37 * 2.75).abs() < 1e-9);
        // Masse: Volumen × Rohdichte
        let l = &top.layers[1];
        assert!((l.mass - l.volume / M3 * 350.0).abs() < 1e-9);
        assert_eq!(m.material(l.material).unwrap().name, "Gasbeton");
        assert_eq!(wall_qto(&m, m.wall_at(r, 1).unwrap()).as_ref(), Some(top));
    }

    #[test]
    fn gerade_wand() {
        let mut m = Model::with_seed(2);
        let set = m.defaults().exterior_wall;
        let pts = [vec3(0.0, 0.0, 0.0), vec3(5000.0, 0.0, 0.0)];
        let eg = m.eg_at(2750.0);
        let r = m
            .add_wall_run(&pts, false, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap();
        let q = run_qto(&m, r);
        assert_eq!(q.len(), 1);
        assert!((q[0].volume / M3 - 5.0 * 0.315 * 2.75).abs() < 1e-9);
        assert!((q[0].length - 5000.0).abs() < 1e-9);
        assert!((q[0].layers[0].length - 5000.0).abs() < 1e-9);
    }

    fn haus(m: &mut Model) -> RunId {
        let set = m.defaults().exterior_wall;
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.eg_at(2750.0);
        m.add_wall_run(&pts, true, RefSide::Left, eg, set, Category::ExteriorWall)
            .unwrap()
    }

    fn near(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    /// Sollwerte aus Paket B9 (Rechteck 10 × 8 m, AW 31,5) mit Jörns
    /// Standard 10:13: Platte 22 cm, Schürze 35 × 58 cm bis −0,80.
    #[test]
    fn gruendung_rechteck() {
        let mut m = Model::with_seed(3);
        let r = haus(&mut m);
        let (slab, footing) = m.foundation_of(r).unwrap();
        let footing = footing.unwrap();
        assert_eq!(m.element(slab).unwrap().number, "SP-001");
        assert_eq!(m.element(footing).unwrap().number, "FS-001");
        assert_eq!(m.element(footing).unwrap().seq, 1);
        assert_eq!(m.element(slab).unwrap().seq, 2);
        let (s, f) = foundation_qto(&m, r).unwrap();
        assert!(near(s.area / 1e6, 80.0, 1e-9), "{}", s.area);
        assert!(near(s.volume / M3, 17.6, 1e-9));
        assert!(near(s.perimeter, 36000.0, 1e-6));
        assert!(near(f.length, 34600.0, 1e-6), "{}", f.length);
        assert!(near(f.volume / M3, 7.0238, 1e-9), "{}", f.volume / M3);
        assert!(m.check().is_empty(), "{:?}", m.check());
        assert!(m.warnings(slab).is_empty());

        // Rücksprung 2 cm
        assert!(!m.set_slab_recess(slab, 10.0), "1 cm wird abgelehnt");
        assert!(m.set_slab_recess(slab, 20.0));
        let (s, f) = foundation_qto(&m, r).unwrap();
        assert!(near(s.area / 1e6, 79.2816, 1e-9), "{}", s.area);
        assert!(near(s.volume / M3, 17.441952, 1e-9));
        assert!(near(f.length, 34440.0, 1e-6), "{}", f.length);
        assert!(near(f.volume / M3, 6.99132, 1e-9), "{}", f.volume / M3);
        assert!(m.set_slab_recess(slab, 0.0));
        assert!(near(
            foundation_qto(&m, r).unwrap().0.area / 1e6,
            80.0,
            1e-9
        ));

        // 160 mm: Körper mit Warnung; 315 mm: Fehler, kein Körper
        assert!(m.set_slab_recess(slab, 160.0));
        assert!(foundation_qto(&m, r).is_some());
        assert!(m.check().is_empty(), "{:?}", m.check());
        assert_eq!(m.warnings(slab).len(), 1, "{:?}", m.warnings(slab));
        assert!(m.set_slab_recess(slab, 315.0));
        assert!(foundation_qto(&m, r).is_none());
        assert_eq!(m.check().len(), 1, "{:?}", m.check());

        // Dicke 25 cm, Oberkante bleibt bei 0
        assert!(m.set_slab_recess(slab, 0.0));
        assert!(m.set_slab_thickness(slab, 250.0));
        let f = m.foundation(r).unwrap().unwrap();
        let s = f.slab_solid();
        let zs: Vec<f64> = s.triangles.iter().flat_map(|t| t.p.map(|p| p.z)).collect();
        let (lo, hi) = zs
            .iter()
            .fold((f64::MAX, f64::MIN), |a, z| (a.0.min(*z), a.1.max(*z)));
        assert!(near(lo, -250.0, 1e-9) && near(hi, 0.0, 1e-9));
        let fz: Vec<f64> = f
            .footing_solid()
            .triangles
            .iter()
            .flat_map(|t| t.p.map(|p| p.z))
            .collect();
        // UK Gründung bleibt (B11), die Schürze wird kürzer
        assert!(near(
            fz.iter().cloned().fold(f64::MAX, f64::min),
            -800.0,
            1e-9
        ));
    }

    #[test]
    fn gruendung_folgt_dem_zug() {
        let mut m = Model::with_seed(4);
        m.require_steps();
        m.begin("Haus");
        let r = haus(&mut m);
        let t = m.commit().unwrap();
        let (slab, footing) = m.foundation_of(r).unwrap();
        // Rückgängig: beide weg; Wiederholen: gleiche Kennungen
        m.apply(&t, crate::txn::Direction::Undo);
        assert!(m.element(slab).is_none() && m.element(footing.unwrap()).is_none());
        assert!(m.check().is_empty(), "{:?}", m.check());
        m.apply(&t, crate::txn::Direction::Redo);
        assert_eq!(m.foundation_of(r), Some((slab, footing)));
        assert_eq!(m.element(slab).unwrap().number, "SP-001");
        // Gummiband: Wand y = 8 um 1 m nach außen, ohne Neuanlage
        m.begin("Ziehen");
        let moved = m.chain(r).unwrap().with_segment_moved(1, -1000.0).unwrap();
        m.set_run_points(r, &moved.points).unwrap();
        m.commit();
        assert_eq!(m.foundation_of(r), Some((slab, footing)));
        let (s, f) = foundation_qto(&m, r).unwrap();
        assert!(near(s.area / 1e6, 90.0, 1e-9), "{}", s.area);
        assert!(near(f.length, 36600.0, 1e-6), "{}", f.length);
        // Offener Zug: keine Gründung; Löschen nimmt sie mit
        let set = m.defaults().exterior_wall;
        m.begin("offen");
        let eg = m.eg_at(2750.0);
        let o = m
            .add_wall_run(
                &[vec3(20000.0, 0.0, 0.0), vec3(30000.0, 0.0, 0.0)],
                false,
                RefSide::Left,
                eg,
                set,
                Category::ExteriorWall,
            )
            .unwrap();
        assert!(m.foundation_of(o).is_none());
        assert!(m.remove_run(r));
        m.commit();
        assert!(m.element(slab).is_none());
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    #[test]
    fn gruendung_speichern_und_oeffnen() {
        let mut m = Model::with_seed(5);
        let r = haus(&mut m);
        let (slab, _) = m.foundation_of(r).unwrap();
        assert!(m.set_slab_recess(slab, 20.0));
        let text = crate::szo::write(&m);
        assert!(text.contains("[slab]") && text.contains("[footing]"));
        let l = crate::szo::read(&text, crate::GuidGen::with_seed(1)).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(crate::szo::write(&l.model), text);
        let r2 = l
            .model
            .runs()
            .ids()
            .find(|r| l.model.run_below(*r).is_none())
            .unwrap();
        assert_eq!(foundation_qto(&l.model, r2), foundation_qto(&m, r));
        let (s2, f2) = l.model.foundation_of(r2).unwrap();
        assert_eq!(
            l.model.element(s2).unwrap().guid,
            m.element(slab).unwrap().guid
        );
        assert_eq!(l.model.element(f2.unwrap()).unwrap().number, "FS-001");
    }

    #[test]
    fn datei_vor_b9_bekommt_gruendung() {
        let mut m = Model::with_seed(5);
        haus(&mut m);
        let text = crate::szo::write(&m);
        // Datei vor B9: ohne Gründung und ohne Stahlbeton-Schraffur
        let old: String = text
            .lines()
            .filter(|l| {
                !l.starts_with("[slab]")
                    && !l.starts_with("[footing]")
                    && !(l.starts_with("[fill]") && l.contains("name=\"Stahlbeton\""))
            })
            .map(|l| format!("{l}\n"))
            .collect();
        let old = old.replace(
            &format!("fill={}", fill_guid(&m)),
            &format!("fill={}", diagonal_guid(&m)),
        );
        let l = crate::szo::read(&old, crate::GuidGen::with_seed(1)).unwrap();
        assert_eq!(l.hints.len(), 2, "{:?}", l.hints);
        assert!(l.model.check().is_empty(), "{:?}", l.model.check());
        let r = l
            .model
            .runs()
            .ids()
            .find(|r| l.model.run_below(*r).is_none())
            .unwrap();
        assert!(l.model.foundation(r).unwrap().is_ok());
        let (_, rc) = l
            .model
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Stahlbeton")
            .unwrap();
        let f = l.model.attr().fill(rc.cut_fill).unwrap();
        assert_eq!(f.name, "Stahlbeton");
        // Ein zweites Öffnen ergänzt nichts mehr
        let again =
            crate::szo::read(&crate::szo::write(&l.model), crate::GuidGen::with_seed(1)).unwrap();
        assert!(again.hints.is_empty(), "{:?}", again.hints);
    }

    fn fill_guid(m: &Model) -> String {
        let (_, rc) = m
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Stahlbeton")
            .unwrap();
        m.attr().fill(rc.cut_fill).unwrap().guid.to_ifc()
    }

    fn diagonal_guid(m: &Model) -> String {
        let (_, g) = m
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Gasbeton")
            .unwrap();
        m.attr().fill(g.cut_fill).unwrap().guid.to_ifc()
    }
}

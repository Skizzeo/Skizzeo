//! Geländeplatte in der App (Gelände G0, plan-gelaendemodul §4.6): statt
//! der Geländelinie eine ebene Platte um jedes Gebäude. In 3D ein Körper
//! mit Rasen oben und Erdreich an den Seiten, der auf dem Boden steht; im
//! Schnitt ihre Schnittfläche mit Erdreich-Schraffur; in den Ansichten ihr
//! Band (Oberkante mit dem Stift „Gelände“, Unterkante und Enden fein). Sie
//! ist kein Bauteil: kein Hover und keine Auswahl im Bild (sie läge sonst
//! unter jeder Maßkette); gewählt wird sie nur im Objektbaum, dort stehen
//! Dicke und Außenoffset. Mit Höhenaufmaß steht statt der Platte das echte
//! Gelände da ([`crate::gelaende_netz`], G1).

use crate::draw_table::{fallback_look, fill_kind, DrawTable, LineLook, MatLook, Stroke};
use crate::gelaende_netz::{self, Gelaende};
use crate::selection::de;
use crate::ui::{kote_text, Field, FieldRow, Props, Section, ViewKind};
use crate::App;
use sk_math::{polygon, vec3, Vec3};
use sk_model::solid::{right_of, SectionFrame};
use sk_model::{
    edge_kind, material, LevelKind, Model, Solid, TerrainPlate, PLATE_MARGIN, PLATE_THICK,
};
use sk_render::{Helper, MeshData};
use sk_ui::theme::Theme;

/// Platz der Platte im Renderer (neben Modell, Vorschau, Live und Geist).
pub(crate) const MESH_PLATE: usize = 4;

/// Rand der Gelände-Rechtecke um die Wandzüge eines Gebäudes (mm), für
/// „unter Gelände“ in den Ansichten (S11): deckt Dämmung, Schürze und
/// Sockel.
pub(crate) const TERRAIN_MARGIN: f64 = 1500.0;

/// Gelände-Rechtecke für den Renderer.
pub(crate) fn terrain_boxes(patches: &[([f64; 4], f64)]) -> Vec<([f32; 4], f32)> {
    patches
        .iter()
        .map(|(b, z)| (b.map(|v| v as f32), *z as f32))
        .collect()
}

/// Was im Renderer liegt: Modellstand, Ansicht, Schnittebene (Bits),
/// Schlüssel von Rasen und Erdreich, ausgeblendet.
type NetzKey = (u64, ViewKind, Option<[u64; 6]>, [u16; 3], bool);

/// Gelände-Rechtecke (x0, y0, x1, y1) mit Höhe, wie der Renderer sie nimmt.
type Kisten = Vec<([f32; 4], f32)>;

/// Platten des Modells und was davon gezeigt wird.
#[derive(Default)]
pub(crate) struct Platte {
    /// Modellstand, zu dem `platten` gehört.
    stand: Option<u64>,
    platten: Vec<TerrainPlate>,
    /// Gelände aus dem Höhenaufmaß; statt der Platten, wenn da.
    gelaende: Option<Gelaende>,
    netz: Option<NetzKey>,
    /// Das Paneel „Eigenschaften“ zeigt die Platte (zum Stand der Platten).
    props: Option<u64>,
    /// Gelände-Rechtecke für „unter Gelände“ zum Modellstand.
    kisten: Option<(u64, Kisten)>,
    /// Leiste „Gelände“ (G1).
    pub(crate) leiste: crate::gelaende_app::Zustand,
    /// Der Lageplan der Unterlage, gelesen (G1c).
    pub(crate) unterlage: crate::gelaende_app::Unterlage,
}

impl Platte {
    /// Die Platten zum Modell; gerechnet nur, wenn es sich geändert hat.
    /// Solange ein Schritt offen ist (`halten`: jeder Zug, auch Gelände,
    /// Höhen und Öffnungen ohne Live-Netz, Review W34 B1) bleiben sie stehen
    /// und folgen beim Loslassen, statt je Mausbewegung alle Häuser neu zu
    /// rechnen.
    pub(crate) fn platten(&mut self, m: &Model, halten: bool) -> &[TerrainPlate] {
        self.abgleich(m, halten);
        &self.platten
    }

    /// Das echte Gelände zum Modell, wenn ein Aufmaß da ist; im Zug wie
    /// [`Platte::platten`].
    pub(crate) fn gelaende(&mut self, m: &Model, halten: bool) -> Option<&Gelaende> {
        self.abgleich(m, halten);
        self.gelaende.as_ref()
    }

    fn abgleich(&mut self, m: &Model, halten: bool) {
        let rev = m.revision();
        if self.stand != Some(rev) && !(halten && self.stand.is_some()) {
            self.stand = Some(rev);
            self.platten = m.terrain_plates();
            let u = &mut self.unterlage;
            self.gelaende = Gelaende::aus(m).map(|g| g.mit_unterlage(u.linien(m)));
        }
    }

    /// Für das Bild: die Platten, wie sie stehen; gerechnet nur, wenn es
    /// noch keine gibt. Einer Änderung folgen sie im Leerlauf
    /// ([`Platte::nachziehen`]), nicht im Loslassen selbst: Bei 1000
    /// Häusern kostete das Loslassen sonst 13 bis 29 ms statt höchstens 8
    /// (Review #119 B4).
    pub(crate) fn im_bild(&mut self, m: &Model) -> &[TerrainPlate] {
        self.platten(m, true)
    }

    /// Das Gelände für das Bild, wie [`Platte::im_bild`].
    pub(crate) fn gelaende_im_bild(&mut self, m: &Model) -> Option<&Gelaende> {
        self.gelaende(m, true)
    }

    /// Im Leerlauf: die Platten einer Änderung nachziehen, nicht in einem
    /// offenen Schritt; `true`, wenn sie neu sind (dann folgt ein Bild).
    pub(crate) fn nachziehen(&mut self, m: &Model) -> bool {
        if m.in_step() || self.stand == Some(m.revision()) {
            return false;
        }
        self.abgleich(m, false);
        true
    }
}

/// Aussehen von Rasen, Erdreich und blassem Rasen (Schlüssel hinter den
/// Baustoffen der Zeichentabelle): Rasen ohne Schnitt; Erdreich im Schnitt
/// auf Papier mit der Schraffur „gewachsener Boden“ (45°, durchgezogen und
/// gestrichelt im Wechsel) in der Farbe des Stifts „Gelände“; blasser
/// Rasen außerhalb der Grenze bei „Umgebung blass“ (G1).
pub(crate) fn looks(theme: &Theme, paper: [f32; 3], ink: [f32; 3]) -> [MatLook; 3] {
    let rgb = |c: sk_paint::Rgba| {
        let [r, g, b, _] = c.to_f32();
        [r, g, b]
    };
    let (grass, soil) = (rgb(theme.env.terrain_grass), rgb(theme.env.terrain_soil));
    let fade = rgb(theme.env.terrain_fade);
    let mm = theme.px_per_mm;
    let base = MatLook {
        cut: soil,
        cut_bg: paper,
        cut_fg: ink,
        ..fallback_look(theme)
    };
    let line = |offset: f32, dash: f32| LineLook {
        angle_deg: 45.0,
        spacing_px: 2.0 * mm,
        offset_px: offset * mm,
        dash_px: dash * mm,
        gap_px: dash * mm,
    };
    [
        MatLook {
            face: grass,
            ..base
        },
        MatLook {
            face: soil,
            kind: fill_kind::LINES,
            width_px: 0.13 * mm,
            lines: [line(0.0, 0.0), line(1.0, 0.4)],
            line_count: 2,
            ..base
        },
        MatLook { face: fade, ..base },
    ]
}

/// Abschnitte der Platte längs der Schnittebene (sortiert, vereinigt).
fn schnitt(p: &TerrainPlate, f: &SectionFrame, p0: Vec3) -> Vec<(f64, f64)> {
    let mut iv: Vec<(f64, f64)> = p
        .pieces
        .iter()
        .flat_map(|q| polygon::plane_intervals(q, p0, f.n, f.along))
        .collect();
    vereinigt(&mut iv)
}

/// Die Abschnitte sortiert, Berührende und Überlappende zu einem.
fn vereinigt(iv: &mut [(f64, f64)]) -> Vec<(f64, f64)> {
    iv.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for &(a, b) in iv.iter() {
        match out.last_mut() {
            Some(l) if a <= l.1 + 1e-3 => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Band der Platte in einer Ansicht: Abschnitte längs `x` (vorn, hinten)
/// oder `y` (Seiten), vereinigt.
fn band(p: &TerrainPlate, side: bool) -> Vec<(f64, f64)> {
    let u = |v: &Vec3| if side { v.y } else { v.x };
    let mut iv: Vec<(f64, f64)> = p
        .pieces
        .iter()
        .map(|q| {
            let lo = q.iter().map(u).fold(f64::INFINITY, f64::min);
            let hi = q.iter().map(u).fold(f64::NEG_INFINITY, f64::max);
            (lo, hi)
        })
        .filter(|(a, b)| b > a)
        .collect();
    vereinigt(&mut iv)
}

fn at(q: Vec3, z: f64) -> Vec3 {
    vec3(q.x, q.y, z)
}

/// Körper einer Platte in 3D bis hinunter auf `bot` (den Boden): Rasen
/// oben, Erdreich unten und an den Seiten; Kanten am oberen und unteren
/// Rand und senkrecht dort, wo der Rand die Richtung wechselt.
fn koerper(p: &TerrainPlate, bot: f64, keys: [u16; 3]) -> Solid {
    let top = p.top;
    let up = vec3(0.0, 0.0, 1.0);
    let mut s = Solid {
        mat: keys[0],
        edge_kind: edge_kind::VIEW,
        ..Solid::default()
    };
    for q in &p.pieces {
        s.quad(
            at(q[0], top),
            at(q[1], top),
            at(q[2], top),
            at(q[3], top),
            up,
        );
    }
    s.mat = keys[1];
    for q in &p.pieces {
        s.quad(
            at(q[0], bot),
            at(q[3], bot),
            at(q[2], bot),
            at(q[1], bot),
            up * -1.0,
        );
    }
    let gleich = |a: Vec3, b: Vec3| (a - b).length() < 1e-3;
    // Kanten nach ihrem Endpunkt (Raster 1 mm, Nachbarzellen mit), statt
    // je Kante alle zu durchsuchen
    let zelle = |v: Vec3| (v.x.round() as i64, v.y.round() as i64);
    let mut enden: std::collections::HashMap<(i64, i64), Vec<usize>> =
        std::collections::HashMap::new();
    for (i, e) in p.edges.iter().enumerate() {
        enden.entry(zelle(e.1)).or_default().push(i);
    }
    let davor = |a: Vec3| {
        let (x, y) = zelle(a);
        let mut best: Option<usize> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &i in enden.get(&(x + dx, y + dy)).into_iter().flatten() {
                    if gleich(p.edges[i].1, a) && best.is_none_or(|b| i < b) {
                        best = Some(i);
                    }
                }
            }
        }
        best.map(|i| p.edges[i])
    };
    for &(a, b) in &p.edges {
        let d = (b - a).normalized();
        s.quad(at(a, bot), at(b, bot), at(b, top), at(a, top), right_of(d));
        s.edge(at(a, top), at(b, top));
        s.edge(at(a, bot), at(b, bot));
        // Ecke am Anfang: die Kante davor läuft in eine andere Richtung
        let davor = davor(a);
        let ecke = davor.is_none_or(|e| {
            let c = (e.1 - e.0).normalized();
            (c.x * d.y - c.y * d.x).abs() > 1e-6 || c.dot(d) < 0.0
        });
        if ecke {
            s.edge(at(a, bot), at(a, top));
        }
    }
    s
}

/// Schnittflächen einer Platte mit der Ebene (zeigen in Richtung `f.n`).
fn kappen(p: &TerrainPlate, f: &SectionFrame, p0: Vec3, soil: u16) -> Solid {
    let mut s = Solid {
        mat: soil | material::CUT,
        edge_kind: edge_kind::CUT,
        ..Solid::default()
    };
    for (a, b) in schnitt(p, f, p0) {
        s.quad(
            f.pt(a, p.bottom),
            f.pt(b, p.bottom),
            f.pt(b, p.top),
            f.pt(a, p.top),
            f.n,
        );
    }
    s
}

/// Netz der Platten: in 3D die Körper, alle bis auf die tiefste
/// Unterkante (dort liegt der Boden, keine Platte schwebt); im Schnitt die
/// Schnittflächen (ihr Umriss kommt aus [`linien`]); sonst leer.
pub(crate) fn netz(
    plates: &[TerrainPlate],
    view: ViewKind,
    plane: Option<(Vec3, Vec3)>,
    keys: [u16; 3],
) -> MeshData {
    let mut s = Solid::default();
    match (view, plane) {
        (ViewKind::Persp, _) => {
            let boden = plates
                .iter()
                .map(|p| p.bottom)
                .fold(f64::INFINITY, f64::min);
            for p in plates {
                s.append(&koerper(p, boden, keys));
            }
        }
        (ViewKind::Section, Some((p0, n))) => {
            let f = SectionFrame::new(p0, n);
            for p in plates {
                s.append(&kappen(p, &f, p0, keys[1]));
            }
        }
        _ => {}
    }
    crate::scene::mesh_of(&s)
}

/// Linien der Platten in Ansicht und Schnitt (ersetzen die Geländelinie):
/// Oberkante mit dem Stift „Gelände“, Unterkante und Enden mit der feinen
/// Linie. In den Ansichten in der Mitte des Modells (`bounds`), verdeckt
/// werden sie nicht: Was unter dem Gelände liegt, regelt „unter Gelände“
/// (S11).
pub(crate) fn linien(
    plates: &[TerrainPlate],
    view: ViewKind,
    plane: Option<(Vec3, Vec3)>,
    bounds: Option<(Vec3, Vec3)>,
    t: &DrawTable,
    scale: f32,
) -> Vec<Helper> {
    let fine = t.drawing_edges[edge_kind::FINE as usize];
    let line = |a: Vec3, b: Vec3, (w, c): Stroke| Helper {
        a: a.to_f32(),
        b: b.to_f32(),
        color: c,
        width: w * scale,
        dash: 0.0,
        pattern: sk_render::SOLID,
        occlude: false,
        round: false,
    };
    let mut out = Vec::new();
    let mut rahmen = |pt: &dyn Fn(f64, f64) -> Vec3, p: &TerrainPlate, iv: Vec<(f64, f64)>| {
        for (a, b) in iv {
            out.push(line(pt(a, p.top), pt(b, p.top), t.ground));
            out.push(line(pt(a, p.bottom), pt(b, p.bottom), fine));
            out.push(line(pt(a, p.bottom), pt(a, p.top), fine));
            out.push(line(pt(b, p.bottom), pt(b, p.top), fine));
        }
    };
    match (view, plane) {
        (ViewKind::Section, Some((p0, n))) => {
            let f = SectionFrame::new(p0, n);
            for p in plates {
                rahmen(&|u, z| f.pt(u, z), p, schnitt(p, &f, p0));
            }
        }
        (ViewKind::Front | ViewKind::Back | ViewKind::Left | ViewKind::Right, _) => {
            let side = matches!(view, ViewKind::Left | ViewKind::Right);
            let mitte = bounds.map_or(vec3(0.0, 0.0, 0.0), |(lo, hi)| (lo + hi) * 0.5);
            let pt = |u: f64, z: f64| {
                if side {
                    vec3(mitte.x, u, z)
                } else {
                    vec3(u, mitte.y, z)
                }
            };
            for p in plates {
                rahmen(&pt, p, band(p, side));
            }
        }
        _ => {}
    }
    out
}

/// Zahlenfeld ohne Sonderwerte.
fn feld(field: Field, label: &'static str, value: f64, r: (f64, f64, f64)) -> FieldRow {
    FieldRow {
        field,
        label,
        value,
        min: r.1,
        max: r.2,
        zero: false,
        einheit: None,
    }
}

/// Inhalt des Paneels „Eigenschaften“ der Geländeplatte: Höhenlage aus dem
/// Geschossmanager, Fläche, die zwei Felder.
pub(crate) fn props(m: &Model, plates: &[TerrainPlate]) -> Props {
    let s = m.plate_settings();
    let ok = plates.first().map_or(m.terrain_z(), |p| p.top);
    let flaeche: f64 = plates.iter().map(TerrainPlate::area).sum();
    let values = vec![
        ("Kategorie", "Geländeplatte".to_string()),
        ("OK Gelände", format!("{} m", kote_text(ok))),
        ("UK Platte", format!("{} m", kote_text(ok - s.thick))),
        ("Fläche", format!("{} m²", de(flaeche / 1e6, 2))),
    ];
    Props {
        values,
        sections: vec![Section {
            title: "Geländeplatte",
            fields: vec![
                feld(Field::PlateThick, "Dicke", s.thick, PLATE_THICK),
                feld(Field::PlateMargin, "Außenoffset", s.margin, PLATE_MARGIN),
            ],
            hint: "Höhe aus dem Geschossmanager. Echtes Gelände mit dem Geländemodul.",
            buttons: Vec::new(),
        }],
        ..Default::default()
    }
}

/// Bits einer Schnittebene (Schlüssel).
fn bits(p: (Vec3, Vec3)) -> [u64; 6] {
    let (a, n) = p;
    [a.x, a.y, a.z, n.x, n.y, n.z].map(f64::to_bits)
}

impl App {
    /// Netz der Platte hochladen, wenn sich Modell, Ansicht, Schnitt,
    /// Aussehen oder Sichtbarkeit geändert haben (aus `build_mesh`, je
    /// Bild).
    pub(crate) fn platte_hochladen(&mut self) {
        let keys = self.scene.table().terrain;
        let hidden = self.scene.terrain_hidden();
        let plane = self.plane();
        let view = self.ui.view;
        let m = self.scene.model();
        self.platte.im_bild(m);
        let key = (
            self.platte.stand.unwrap_or(0),
            view,
            plane.map(bits),
            keys,
            hidden,
        );
        if self.platte.netz == Some(key) {
            return;
        }
        self.platte.netz = Some(key);
        let netz = match (&self.platte.gelaende, view, plane) {
            _ if hidden => MeshData::default(),
            (Some(g), ViewKind::Persp, _) => {
                crate::scene::mesh_of(&gelaende_netz::koerper(g, keys))
            }
            (Some(g), ViewKind::Section, Some((p0, n))) => {
                let f = SectionFrame::new(p0, n);
                crate::scene::mesh_of(&gelaende_netz::kappen(g, &f, p0, keys[1]))
            }
            (Some(_), ..) => MeshData::default(),
            (None, ..) => netz(&self.platte.platten, view, plane, keys),
        };
        self.renderer.set_mesh(MESH_PLATE, &netz);
        self.renderer.set_grass(keys[0]);
    }

    /// Linien der Platte in Ansicht und Schnitt, mit Aufmaß die des
    /// Geländes, im Grundriss nur im Erdgeschoss; ausgeblendet keine. `vh`:
    /// Höhe der Ansicht in Bildpunkten.
    pub(crate) fn platte_linien(&mut self, vh: f64, scale: f32) -> Vec<Helper> {
        if self.scene.terrain_hidden() {
            return Vec::new();
        }
        let plane = self.plane();
        let view = self.ui.view;
        let b = self.scene.bounds();
        let t = self.scene.table();
        let m = self.scene.model();
        if let Some(g) = self.platte.gelaende_im_bild(m) {
            if view == ViewKind::Plan && !self.scene.ground_active() {
                return Vec::new();
            }
            let mm_px = self.cam.ortho.map_or(10.0, |half| 2.0 * half / vh.max(1.0));
            let f = gelaende_netz::fassade(g, view, b);
            return gelaende_netz::linien(g, view, plane, f, t, scale, mm_px);
        }
        // ohne Aufmaß zeigen nur Ansichten und Schnitt Linien der Platte
        if matches!(view, ViewKind::Plan | ViewKind::Persp) {
            return Vec::new();
        }
        linien(self.platte.im_bild(m), view, plane, b, t, scale)
    }

    /// Hüllquader einer Ansicht: ohne Teile unter dem Gelände, wenn sie
    /// dort ausblendet (S11). Aus `main.rs`.
    pub(crate) fn ansicht_huelle(&self, v: ViewKind) -> Option<(Vec3, Vec3)> {
        let (lo, hi) = self.scene.bounds()?;
        let m = self.scene.model();
        let ganz = crate::ansicht_schatten::platz(v).is_none_or(|i| m.view_below(i));
        // Das tiefste Gelände: über keinem Gebäude wird etwas abgeschnitten;
        // mit Aufmaß zählt je Gründung ihr tiefster Punkt am Plattenrand
        let gruendungen = m
            .storeys()
            .iter()
            .filter(|(_, s)| s.kind == LevelKind::Foundation);
        let g = m
            .terrain_patches(TERRAIN_MARGIN)
            .iter()
            .map(|p| p.1)
            .chain(gruendungen.map(|(id, _)| m.terrain_z_of_level(id)))
            .fold(m.terrain_z(), f64::min);
        Some(if ganz || hi.z <= g {
            (lo, hi)
        } else {
            (vec3(lo.x, lo.y, lo.z.max(g)), hi)
        })
    }

    /// Gelände im Renderer: OK Gelände für „unter Gelände“, mit Aufmaß
    /// aus seinem Raster, sonst je Gebäude, und der Boden in 3D auf der
    /// Unterkante der Platte (die Platte steht auf ihm); ausgeblendet auf
    /// OK Gelände.
    pub(crate) fn gelaende_renderer(&mut self) {
        let raster = self.scene.terrain_grid();
        self.renderer.set_terrain_grid(raster.as_deref());
        let m = self.scene.model();
        self.renderer.set_terrain(m.terrain_z() as f32);
        // die Rechtecke je Modellstand, nicht je Bild
        let rev = m.revision();
        if self.platte.kisten.as_ref().is_none_or(|k| k.0 != rev) {
            let k = terrain_boxes(&m.terrain_patches(TERRAIN_MARGIN));
            self.platte.kisten = Some((rev, k));
        }
        if let Some((_, k)) = &self.platte.kisten {
            self.renderer.set_terrain_boxes(k);
        }
        let unten = if self.scene.terrain_hidden() {
            None
        } else if let Some(g) = self.platte.gelaende_im_bild(m) {
            Some(g.unten as f32)
        } else {
            let p = self.platte.im_bild(m);
            p.iter().map(|p| p.bottom as f32).reduce(f32::min)
        };
        self.renderer.set_ground_z(unten);
    }

    /// Eigenschaften der Geländeplatte, solange sie im Objektbaum gewählt
    /// ist; `true`, wenn das Paneel sie zeigt. Wird sie abgewählt, leert
    /// sich das Paneel für die übliche Auswahl.
    pub(crate) fn platte_props(&mut self) -> bool {
        let m = self.scene.model();
        // am Stand der Platten, nicht an der Revision: im Zug rechnet das
        // Paneel nicht je Mausbewegung neu (Review #119)
        let soll = if self.picking.plate && self.sel.id.is_none() {
            self.platte.im_bild(m);
            self.platte.stand
        } else {
            None
        };
        if soll == self.platte.props {
            return soll.is_some();
        }
        let war = self.platte.props.is_some();
        self.platte.props = soll;
        if soll.is_some() {
            if !war {
                self.ui.reset_props_scroll();
            }
            let p = props(m, &self.platte.platten);
            self.ui.set_props(Some(p));
        } else {
            self.ui.set_props(None);
            self.props_key = None;
        }
        self.props_dirty = true;
        soll.is_some()
    }

    /// Leerlauf: die Platten einer Änderung nachziehen ([`Platte::nachziehen`]);
    /// `true`, wenn ein Bild folgen soll.
    pub(crate) fn platte_nachziehen(&mut self) -> bool {
        self.platte.nachziehen(self.scene.model())
    }

    /// Neues Projekt (Neu, Öffnen, Sicherung): Objektbaum zu, Platte
    /// abgewählt und neu zu rechnen (geladene Dateien beginnen alle bei
    /// Revision 0, sonst zeigte B die Platte von A).
    pub(crate) fn baum_neu(&mut self) {
        self.tree.reset();
        self.platte = Platte::default();
        self.picking.plate = false;
    }

    /// Zeile „Geländeplatte“ im Objektbaum: die Platte wählen, die
    /// Bauteile abwählen.
    pub(crate) fn platte_waehlen(&mut self) {
        self.picking.selected.clear();
        self.picking.plate = true;
        self.quantity.dirty = true;
        self.redraw = true;
    }

    /// Feld der Platte im Paneel übernommen (ein Schritt); `false`, wenn
    /// das Feld nicht zur Platte gehört.
    /// Felder der Leiste „Gelände“ gehen weiter an [`App::gelaende_feld`].
    pub(crate) fn platte_feld(&mut self, submit: Option<(Field, f64)>) -> bool {
        if let Some((Field::Gelaende(g), v)) = submit {
            self.gelaende_feld(g, v);
            return true;
        }
        let Some((f @ (Field::PlateThick | Field::PlateMargin), mm)) = submit else {
            return false;
        };
        let mut s = self.scene.model().plate_settings();
        match f {
            Field::PlateThick => s.thick = mm,
            _ => s.margin = mm,
        }
        if self
            .scene
            .edit_model("Geländeplatte", |m| m.set_plate_settings(s))
        {
            self.upload_model();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn haus() -> Model {
        let mut m = Model::with_seed(3);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        m
    }

    /// Nach dem Loslassen rechnet das Bild die Platten nicht, erst der
    /// Leerlauf; im offenen Schritt auch er nicht (Review #119 B4).
    #[test]
    fn leerlauf_zieht_nach() {
        let mut s = crate::scene::Scene::with_model(haus());
        let mut pl = Platte::default();
        let a0 = pl.im_bild(s.model())[0].area();
        assert!(!pl.nachziehen(s.model()), "aktuell");
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        s.begin("Wand verschieben");
        let moved = orig.with_segment_moved(0, 1000.0).unwrap();
        assert!(s.set_run_points(run, &moved.points));
        assert!(!pl.nachziehen(s.model()), "im Schritt nicht");
        s.commit();
        assert_eq!(
            pl.im_bild(s.model())[0].area(),
            a0,
            "das Bild rechnet nicht"
        );
        assert!(pl.nachziehen(s.model()));
        let a1 = pl.im_bild(s.model())[0].area();
        assert!((a1 - a0).abs() > 1e6, "{a0} {a1}");
        assert!(!pl.nachziehen(s.model()));
        // der Leerlauf ruft es; die App fragt im Bild nur `im_bild`
        let main = include_str!("main.rs");
        assert!(main.contains("a.redraw |= a.platte_nachziehen();"));
        let quelle = include_str!("gelaende_platte.rs");
        let app = &quelle[quelle.find("impl App {").unwrap()..quelle.find("#[cfg(test)]").unwrap()];
        assert!(!app.contains(concat!("platte.", "platten(")));
        assert!(!app.contains(concat!("platte.", "gelaende(")));
    }

    /// Während eines Zugs bleiben die Platten stehen und folgen beim
    /// Loslassen (Review W34 B1); ein neues Projekt rechnet neu, auch wenn
    /// beide Dateien bei Revision 0 beginnen (B3).
    #[test]
    fn zug_und_neues_projekt() {
        let mut s = crate::scene::Scene::with_model(haus());
        let mut pl = Platte::default();
        let a0 = pl.platten(s.model(), false)[0].area();
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        s.begin("Wand verschieben");
        let moved = orig.with_segment_moved(0, 1000.0).unwrap();
        assert!(s.set_run_points(run, &moved.points));
        assert!(s.zieht());
        let halten = |s: &crate::scene::Scene| s.model().in_step();
        assert_eq!(pl.platten(s.model(), halten(&s))[0].area(), a0, "steht");
        s.commit();
        assert!(!s.zieht());
        let a1 = pl.platten(s.model(), halten(&s))[0].area();
        assert!((a1 - a0).abs() > 1e6, "folgt beim Loslassen: {a0} {a1}");
        // ein Zug ohne Live-Netz (Geländehöhe ziehen): steht ebenso
        let top = pl.platten(s.model(), false)[0].top;
        let b = s.model().buildings().ids().next().unwrap();
        s.begin("Gelände");
        assert!(s.edit_attr(|m| m.set_terrain_offset_of(b, 400.0)));
        assert!(!s.zieht() && halten(&s));
        assert_eq!(
            pl.platten(s.model(), halten(&s))[0].top,
            top,
            "steht im Schritt"
        );
        s.commit();
        assert_eq!(pl.platten(s.model(), halten(&s))[0].top, top - 400.0);
        // zwei Dateien nacheinander: beide Revision 0
        let laden = |m: &Model| {
            let t = sk_model::szo::write(m);
            sk_model::szo::read(&t, sk_model::GuidGen::with_seed(1))
                .unwrap()
                .model
        };
        let (a, b) = (laden(&Model::with_seed(4)), laden(&haus()));
        assert_eq!(a.revision(), b.revision());
        let mut pl = Platte::default();
        assert!(pl.platten(&a, false).is_empty());
        assert!(pl.platten(&b, false).is_empty(), "ohne Zurücksetzen");
        let mut pl = Platte::default();
        assert_eq!(pl.platten(&b, false).len(), 1);
        let quelle = include_str!("gelaende_platte.rs");
        assert!(!quelle.contains(concat!("halten = self.scene.", "zieht()")));
        let main = include_str!("main.rs");
        let install = &main[main.find("fn install_scene").unwrap()..];
        assert!(install[..install.find("\n    }\n").unwrap()].contains("self.baum_neu();"));
    }

    /// In 3D ein geschlossener Körper: Rasen oben und Erdreich unten mit
    /// gleicher Fläche, vier Außen- und vier Lochecken als senkrechte
    /// Kanten; in Grundriss und Ansicht kein Netz.
    #[test]
    fn koerper_in_3d() {
        let m = haus();
        let p = m.terrain_plates();
        let keys = [40, 41, 42];
        let s = koerper(&p[0], p[0].bottom, keys);
        let flaeche = |k: u16| -> f64 {
            s.triangles
                .iter()
                .filter(|t| t.mat == k && t.n.z.abs() > 0.5)
                .map(|t| (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length() * 0.5)
                .sum()
        };
        assert!((flaeche(40) - p[0].area()).abs() < 1.0);
        assert!((flaeche(41) - p[0].area()).abs() < 1.0);
        let senkrecht = s
            .edges
            .iter()
            .filter(|e| (e.a.z - e.b.z).abs() > 1.0)
            .count();
        assert_eq!(senkrecht, 8);
        for v in [ViewKind::Plan, ViewKind::Front] {
            assert!(netz(&p, v, None, keys).faces.is_empty());
        }
    }

    /// Schnitt quer durch die Mitte: links und rechts vom Haus je 3 m
    /// Erdreich in voller Dicke, geschnitten; die Linien rahmen beide
    /// Stücke, die Oberkante im Stift „Gelände“.
    #[test]
    fn schnitt_durch_die_mitte() {
        let m = haus();
        let p = m.terrain_plates();
        let plane = (vec3(5000.0, 4000.0, 0.0), vec3(0.0, -1.0, 0.0));
        let f = SectionFrame::new(plane.0, plane.1);
        let iv = schnitt(&p[0], &f, plane.0);
        assert_eq!(iv.len(), 2, "{iv:?}");
        for (a, b) in &iv {
            assert!((b - a - 3000.0).abs() < 1e-6, "{a} {b}");
        }
        let n = netz(&p, ViewKind::Section, Some(plane), [40, 41, 42]);
        assert_eq!(n.faces.len(), 2 * 6);
        assert!(n
            .faces
            .iter()
            .all(|v| v[6] as u32 == 41 | material::CUT as u32));
        let theme = Theme::dark();
        let t = DrawTable::resolve(&m, &theme);
        let l = linien(&p, ViewKind::Section, Some(plane), None, &t, 1.0);
        assert_eq!(l.len(), 8);
        let oben: Vec<_> = l
            .iter()
            .filter(|h| h.a[2] == 0.0 && h.b[2] == 0.0)
            .collect();
        assert_eq!(oben.len(), 2);
        assert!(oben.iter().all(|h| h.color == t.ground.1));
    }

    /// Ansicht von vorn: ein Band über die ganze Platte (16 m), von OK
    /// Gelände bis zur Unterkante; Dicke und Außenoffset folgen den
    /// Einstellungen.
    #[test]
    fn band_in_der_ansicht() {
        let mut m = haus();
        let theme = Theme::dark();
        let t = DrawTable::resolve(&m, &theme);
        let l = linien(&m.terrain_plates(), ViewKind::Front, None, None, &t, 1.0);
        assert_eq!(l.len(), 4);
        let xs: Vec<f32> = l.iter().flat_map(|h| [h.a[0], h.b[0]]).collect();
        let (lo, hi) = xs
            .iter()
            .fold((f32::MAX, f32::MIN), |a, &x| (a.0.min(x), a.1.max(x)));
        assert_eq!((lo, hi), (-3000.0, 13000.0));
        m.begin("Platte");
        assert!(m.set_plate_settings(sk_model::PlateSettings {
            thick: 800.0,
            margin: 1500.0
        }));
        m.commit();
        let l = linien(&m.terrain_plates(), ViewKind::Left, None, None, &t, 1.0);
        let zs: Vec<f32> = l.iter().flat_map(|h| [h.a[2], h.b[2]]).collect();
        assert!(zs.contains(&-800.0) && zs.contains(&0.0));
        let ys: Vec<f32> = l.iter().flat_map(|h| [h.a[1], h.b[1]]).collect();
        assert!(ys.contains(&-1500.0) && ys.contains(&9500.0));
    }

    /// Dicke und Außenoffset sind ein Schritt: Rückgängig stellt die
    /// Vorgabe wieder her, Wiederholen den neuen Wert; das Paneel zeigt
    /// die Werte und die Höhe aus dem Geschossmanager.
    #[test]
    fn einstellungen_rueckgaengig() {
        let mut s = crate::scene::Scene::with_model(haus());
        let neu = sk_model::PlateSettings {
            thick: 700.0,
            margin: 2000.0,
        };
        assert!(s.edit_model("Geländeplatte", |m| m.set_plate_settings(neu)));
        assert_eq!(s.model().plate_settings(), neu);
        assert!(s.undo());
        assert_eq!(s.model().plate_settings(), Default::default());
        assert!(s.redo());
        assert_eq!(s.model().plate_settings(), neu);
        let p = props(s.model(), &s.model().terrain_plates());
        let f = &p.sections[0].fields;
        assert_eq!((f[0].field, f[0].value), (Field::PlateThick, 700.0));
        assert_eq!((f[1].field, f[1].value), (Field::PlateMargin, 2000.0));
        assert_eq!((f[1].min, f[1].max), (1500.0, 3000.0));
        assert!(
            p.values.contains(&("UK Platte", "−0,70 m".to_string())),
            "{:?}",
            p.values
        );
    }

    /// Rasen und Erdreich liegen hinter den Baustoffen der Tabelle; das
    /// Erdreich ist im Schnitt schraffiert.
    #[test]
    fn rasen_und_erdreich_in_der_tabelle() {
        let m = haus();
        let theme = Theme::dark();
        let t = DrawTable::resolve(&m, &theme);
        let [g, s, blass] = t.terrain;
        assert_eq!((s, blass), (g + 1, g + 2));
        assert_eq!(blass as usize + 1, t.mats.len());
        let [r, gr, b, _] = theme.env.terrain_fade.to_f32();
        assert_eq!(t.mats[blass as usize].face, [r, gr, b]);
        assert!(m
            .materials()
            .iter()
            .all(|(id, _)| sk_model::material_key(id) < g));
        assert_eq!(t.mats[s as usize].kind, fill_kind::LINES);
        let [r, gr, b, _] = theme.env.terrain_grass.to_f32();
        assert_eq!(t.mats[g as usize].face, [r, gr, b]);
    }
}

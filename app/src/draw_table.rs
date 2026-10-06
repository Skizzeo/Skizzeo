//! Zeichentabelle: die Attributtabellen des Modells, aufgelöst in die Werte,
//! die das Netz und der Renderer brauchen (Farben 0..1, Strichbreiten in
//! Bildpunkten bei 96 dpi). Aufgelöst wird nur, wenn sich die Attribute ändern,
//! nicht je Bild.

use sk_model::{edge_kind, EdgeStyle, FillKind, FillSpace, Model};
use sk_paint::Rgba;
use sk_render::{EdgeLooks, Looks, EDGE_KINDS, LOOK_ROWS};
use sk_ui::theme::Theme;

/// Art der Schnittflächen-Füllung (Zeile 2, Alpha der Tabelle).
pub mod fill_kind {
    pub const EMPTY: f32 = 0.0;
    pub const SOLID: f32 = 1.0;
    pub const LINES: f32 = 2.0;
    pub const ZIGZAG: f32 = 3.0;
}

/// Eine Schar Schraffurlinien in Bildpunkten bei 96 dpi.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineLook {
    pub angle_deg: f32,
    pub spacing_px: f32,
    pub offset_px: f32,
}

/// Aussehen eines Baustoffs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatLook {
    /// Ansichtsfläche in 3D.
    pub face: [f32; 3],
    /// Schnittfläche in 3D.
    pub cut: [f32; 3],
    /// Grund unter der Schraffur in der Zeichnung.
    pub cut_bg: [f32; 3],
    /// Farbe der Schraffur (oder der Vollfläche).
    pub cut_fg: [f32; 3],
    /// Art der Füllung ([`fill_kind`]).
    pub kind: f32,
    /// Strichbreite der Schraffur in Bildpunkten bei 96 dpi.
    pub width_px: f32,
    /// Bis zu zwei Linienscharen; zwei = Kreuzschraffur.
    pub lines: [LineLook; 2],
    pub line_count: u8,
    /// Zickzack: Periode längs in Schichtdicken.
    pub zigzag_period: f32,
}

/// Strichbreite (Bildpunkte bei 96 dpi) und Farbe.
pub type Stroke = (f32, [f32; 4]);

#[derive(Clone, Debug, PartialEq)]
pub struct DrawTable {
    /// Stand der Attribute, aus dem die Tabelle stammt.
    pub rev: u64,
    /// Stand des Farbschemas (Rückfallfarben, Bildpunkte je mm).
    pub theme_rev: u64,
    /// Index = Darstellungsschlüssel des Baustoffs ([`sk_model::material_key`]);
    /// 0 = ohne Baustoff.
    pub mats: Vec<MatLook>,
    pub drawing_edges: [Stroke; edge_kind::COUNT],
    pub model_edges: [Stroke; edge_kind::COUNT],
    pub paper: [f32; 3],
    pub ground: Stroke,
    pub section_line: Stroke,
    pub section_ends: Stroke,
    /// Was die Darstellung noch nicht kann (modellbezogene Schraffuren, mehr
    /// als zwei Scharen); gezeichnet wird ersatzweise.
    pub notes: Vec<String>,
}

fn rgb([r, g, b]: [u8; 3]) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn rgb_of(c: Rgba) -> [f32; 3] {
    let [r, g, b, _] = c.to_f32();
    [r, g, b]
}

/// Aussehen von Flächen ohne Baustoff.
const NO_MATERIAL: MatLook = MatLook {
    face: [0.0; 3],
    cut: [0.0; 3],
    cut_bg: [0.0; 3],
    cut_fg: [0.0; 3],
    kind: fill_kind::EMPTY,
    width_px: 1.0,
    lines: [LineLook {
        angle_deg: 0.0,
        spacing_px: 0.0,
        offset_px: 0.0,
    }; 2],
    line_count: 0,
    zigzag_period: 1.0,
};

impl DrawTable {
    pub fn resolve(model: &Model, theme: &Theme) -> DrawTable {
        let a = model.attr();
        let env = &theme.env;
        let px_per_mm = theme.px_per_mm;
        let stroke = |s: &EdgeStyle| -> Stroke {
            a.pen(s.pen).map_or((1.0, env.edge.to_f32()), |p| {
                let [r, g, b] = rgb(p.color);
                (p.width_mm * px_per_mm, [r, g, b, 1.0])
            })
        };
        let pen_rgb = |id, fallback| a.pen(id).map_or(fallback, |p| rgb(p.color));
        let fallback = MatLook {
            face: rgb_of(env.face),
            cut: rgb_of(env.face),
            cut_bg: rgb_of(env.fill_fallback),
            cut_fg: rgb_of(env.edge),
            ..NO_MATERIAL
        };
        let mut mats = vec![fallback];
        let mut notes = Vec::new();
        for (id, m) in model.materials().iter() {
            let key = id.index() as usize + 1;
            if mats.len() <= key {
                mats.resize(key + 1, fallback);
            }
            let mut look = MatLook {
                cut_bg: pen_rgb(m.cut_bg, rgb_of(env.fill_fallback)),
                cut_fg: pen_rgb(m.cut_fg, rgb_of(env.edge)),
                width_px: a.pen(m.cut_fg).map_or(1.0, |p| p.width_mm * px_per_mm),
                ..fallback
            };
            (look.face, look.cut) = a
                .surface(m.surface)
                .map_or((fallback.face, fallback.cut), |s| {
                    (rgb(s.color), rgb(s.cut_color))
                });
            if let Some(f) = a.fill(m.cut_fill) {
                if f.space == FillSpace::Model {
                    // Bis E3b wie papierbezogen
                    notes.push(format!(
                        "Schraffur „{}“ (modellbezogen) wird papierbezogen gezeichnet",
                        f.name
                    ));
                }
                match &f.kind {
                    FillKind::Empty => {}
                    FillKind::Solid => look.kind = fill_kind::SOLID,
                    FillKind::Lines(lines) => {
                        look.kind = fill_kind::LINES;
                        if lines.len() > 2 {
                            notes.push(format!(
                                "Schraffur „{}“: nur die ersten zwei von {} Scharen",
                                f.name,
                                lines.len()
                            ));
                        }
                        for (slot, l) in look.lines.iter_mut().zip(lines) {
                            *slot = LineLook {
                                angle_deg: l.angle_deg,
                                spacing_px: l.spacing_mm * px_per_mm,
                                offset_px: l.offset_mm * px_per_mm,
                            };
                        }
                        look.line_count = lines.len().min(2) as u8;
                        if look.line_count == 0 {
                            look.kind = fill_kind::EMPTY;
                        }
                    }
                    FillKind::Zigzag { period } => {
                        look.kind = fill_kind::ZIGZAG;
                        look.zigzag_period = if *period > 0.0 { *period } else { 1.0 };
                    }
                }
            }
            mats[key] = look;
        }
        let d = a.display();
        DrawTable {
            rev: a.rev(),
            theme_rev: theme.rev,
            mats,
            drawing_edges: d.drawing.map(|s| stroke(&s)),
            model_edges: d.model3d.map(|s| stroke(&s)),
            paper: rgb(d.paper),
            ground: stroke(&d.ground),
            section_line: stroke(&d.section_line),
            section_ends: stroke(&d.section_ends),
            notes,
        }
    }

    /// Aussehen zu einem Darstellungsschlüssel (Schnittbit egal).
    #[cfg(test)]
    pub fn look(&self, key: u16) -> &MatLook {
        let k = (key & !sk_model::material::CUT) as usize;
        self.mats.get(k).unwrap_or(&self.mats[0])
    }

    /// Strichbreite einer Kante in Bildpunkten bei 96 dpi.
    #[cfg(test)]
    pub fn edge_width(&self, drawing: bool, kind: u8) -> f32 {
        let t = if drawing {
            &self.drawing_edges
        } else {
            &self.model_edges
        };
        t.get(kind as usize).map_or(1.0, |s| s.0)
    }

    /// Tabelle für die Grafikkarte; `px_scale` = Skalierung der Oberfläche
    /// (Bildpunkte je Bildpunkt bei 96 dpi). Aufbau: [`sk_render::Looks`].
    pub fn pack(&self, px_scale: f32) -> Vec<[f32; 4]> {
        let keys = self.mats.len();
        let mut t = vec![[0.0; 4]; keys * LOOK_ROWS];
        for (k, m) in self.mats.iter().enumerate() {
            let mut put = |row: usize, v: [f32; 4]| t[row * keys + k] = v;
            put(0, [m.face[0], m.face[1], m.face[2], 0.0]);
            put(1, [m.cut[0], m.cut[1], m.cut[2], 0.0]);
            put(2, [m.cut_bg[0], m.cut_bg[1], m.cut_bg[2], m.kind]);
            let w = m.width_px * px_scale;
            put(3, [m.cut_fg[0], m.cut_fg[1], m.cut_fg[2], w]);
            let mut offsets = [0.0; 2];
            for (i, l) in m.lines.iter().enumerate().take(m.line_count as usize) {
                let (f, offset) = family(l, px_scale);
                put(4 + i, f);
                offsets[i] = offset;
            }
            let count = m.line_count as f32;
            put(6, [offsets[0], offsets[1], count, m.zigzag_period]);
        }
        t
    }

    /// Breite und Farbe je Kantenart, für Zeichnung oder 3D.
    pub fn edge_looks(&self, drawing: bool, px_scale: f32) -> EdgeLooks {
        let t = if drawing {
            &self.drawing_edges
        } else {
            &self.model_edges
        };
        let mut e = EdgeLooks::default();
        for (i, (w, c)) in t.iter().enumerate().take(EDGE_KINDS) {
            e.width[i] = w * px_scale;
            e.color[i] = [c[0], c[1], c[2]];
        }
        e
    }

    /// Alles, was der Renderer zum Aussehen braucht.
    pub fn looks(&self, px_scale: f32) -> Looks {
        Looks {
            keys: self.mats.len(),
            texels: self.pack(px_scale),
            drawing: self.edge_looks(true, px_scale),
            model: self.edge_looks(false, px_scale),
        }
    }
}

/// Texel einer Linienschar und ihr Versatz.
///
/// Die Linien sind `cx·x + cy·y = Versatz + n·Periode` mit `(cx, cy) =
/// (sin w, cos w) / k`, `k = max(|sin w|, |cos w|)`: so hat eine der beiden
/// Zahlen genau den Betrag 1, und 45° rechnet im Shader bitgleich wie die
/// frühere feste Schraffur (`mod(x + y, Abstand·√2)·√½`). Der Winkel zählt
/// wie bisher im Uhrzeigersinn ab der Waagerechten (45° fällt nach rechts).
fn family(l: &LineLook, px_scale: f32) -> ([f32; 4], f32) {
    let w = (l.angle_deg as f64).to_radians();
    let (sin, cos) = (w.sin(), w.cos());
    let k = sin.abs().max(cos.abs());
    let inv_k = (1.0 / k) as f32;
    let spacing = l.spacing_px * px_scale;
    let period = spacing * inv_k;
    let offset = l.offset_px * px_scale * inv_k;
    let f = [(sin / k) as f32, (cos / k) as f32, period, k as f32];
    (f, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::{material, Fill, HatchLine};

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() <= 0.05
    }

    fn key(m: &Model, name: &str) -> u16 {
        let (id, _) = m.materials().iter().find(|(_, x)| x.name == name).unwrap();
        sk_model::material_key(id)
    }

    /// Texel `row` des Schlüssels `key` aus [`DrawTable::pack`].
    fn texel(t: &DrawTable, packed: &[[f32; 4]], key: u16, row: usize) -> [f32; 4] {
        packed[row * t.mats.len() + (key & !material::CUT) as usize]
    }

    #[test]
    fn startwerte_wie_vorher() {
        let m = Model::with_seed(1);
        assert!(m.check().is_empty(), "{:?}", m.check());
        let t = DrawTable::resolve(&m, &Theme::dark());
        // Frühere Werte: Faktor × 1,25 px
        assert!(near(t.edge_width(true, edge_kind::CUT), 2.2 * 1.25));
        assert!(near(t.edge_width(true, edge_kind::VIEW), 1.35 * 1.25));
        assert!(near(t.edge_width(true, edge_kind::CUT_LAYER), 1.35 * 1.25));
        assert!(near(t.edge_width(true, edge_kind::FINE), 0.55 * 1.25));
        assert!(near(t.edge_width(false, edge_kind::VIEW), 1.25));
        assert!(near(t.edge_width(false, edge_kind::FINE), 0.55 * 1.25));
        assert!(near(t.ground.0, 2.2 * 1.25));
        assert!(near(t.section_line.0, 1.2) && near(t.section_ends.0, 3.2));
        assert_eq!(t.paper, rgb([245, 244, 239]));
        assert!(t.notes.is_empty(), "{:?}", t.notes);
        let gas = t.look(key(&m, "Gasbeton") | material::CUT);
        assert_eq!(gas.kind, fill_kind::LINES);
        assert_eq!(gas.line_count, 1);
        assert!(near(gas.lines[0].spacing_px, 7.0) && near(gas.width_px, 1.0));
        assert_eq!(gas.face, rgb([238, 237, 232]));
        assert_eq!(gas.cut, rgb([176, 177, 174]));
        assert_eq!(gas.cut_bg, rgb([255, 255, 255]));
        assert_eq!(gas.cut_fg, [0.0; 3]);
        let ins = t.look(key(&m, "Dämmung (WDVS)"));
        assert_eq!(ins.kind, fill_kind::ZIGZAG);
        assert_eq!(ins.cut, rgb([232, 196, 92]));
        assert_eq!(t.look(key(&m, "Putz")).kind, fill_kind::EMPTY);
        // E10: Stahlbeton mit Kreuzschraffur, gleicher Abstand und Strich
        let rc = t.look(key(&m, "Stahlbeton") | material::CUT);
        assert_eq!((rc.kind, rc.line_count), (fill_kind::LINES, 2));
        assert_eq!(rc.lines[0].spacing_px, gas.lines[0].spacing_px);
        assert_eq!(rc.cut_bg, rgb([255, 255, 255]));
        assert_eq!(t.look(material::PLAIN).face, rgb_of(Theme::dark().env.face));
    }

    /// E3, Test 2: die gepackte Tabelle bei den Startwerten.
    #[test]
    fn tabelle_startwerte() {
        let m = Model::with_seed(1);
        let t = DrawTable::resolve(&m, &Theme::dark());
        let p = t.pack(1.0);
        assert_eq!(p.len(), t.mats.len() * LOOK_ROWS);
        let gas = key(&m, "Gasbeton");
        let surf = |k| t.look(k);
        // Farben gleich den Oberflächen, Art 2 mit einer Schar unter 45°
        assert_eq!(texel(&t, &p, gas, 0)[..3], surf(gas).face);
        assert_eq!(texel(&t, &p, gas, 1)[..3], surf(gas).cut);
        assert_eq!(texel(&t, &p, gas, 2)[3], 2.0);
        let f = texel(&t, &p, gas, 4);
        // 45°: cx = cy = 1, Periode = Abstand·√2, k = √½ wie im früheren Shader
        assert_eq!((f[0], f[1]), (1.0, 1.0));
        assert_eq!(
            f[2],
            t.look(gas).lines[0].spacing_px * std::f32::consts::SQRT_2
        );
        assert_eq!(f[3], std::f32::consts::FRAC_1_SQRT_2);
        assert!(near(t.look(gas).lines[0].spacing_px, 7.0));
        assert_eq!(texel(&t, &p, gas, 6), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(texel(&t, &p, key(&m, "Dämmung (WDVS)"), 2)[3], 3.0);
        assert_eq!(texel(&t, &p, key(&m, "Putz"), 2)[3], 0.0);
        // Stahlbeton: zweite Schar unter 135° = x − y
        let rc = key(&m, "Stahlbeton");
        assert_eq!(texel(&t, &p, rc, 6)[2], 2.0);
        let g = texel(&t, &p, rc, 5);
        assert_eq!((g[0], g[1]), (1.0, -1.0));
        // Skalierung der Oberfläche geht in Abstand und Strichbreite ein
        let p2 = t.pack(2.0);
        assert_eq!(texel(&t, &p2, gas, 4)[2], f[2] * 2.0);
        assert_eq!(texel(&t, &p2, gas, 3)[3], texel(&t, &p, gas, 3)[3] * 2.0);
        // Kanten: Breite × Skalierung, Farbe des Stifts
        let e = t.edge_looks(true, 1.0);
        assert_eq!(
            e.width[edge_kind::CUT as usize],
            t.edge_width(true, edge_kind::CUT)
        );
        assert_eq!(e.color[edge_kind::CUT as usize], [0.0; 3]);
    }

    /// E3, Test 3: zwei Baustoffe mit verschiedenen Linienschraffuren.
    #[test]
    fn eigene_schraffur_je_baustoff() {
        let mut m = Model::with_seed(1);
        let mut lines = |name: &str, l: Vec<HatchLine>| {
            let guid = m.new_guid();
            m.add_fill(Fill {
                guid,
                name: name.into(),
                kind: FillKind::Lines(l),
                space: FillSpace::Paper,
            })
        };
        let line = |angle_deg, spacing_mm| HatchLine {
            angle_deg,
            spacing_mm,
            offset_mm: 0.0,
        };
        let a = lines("30°", vec![line(30.0, 2.0)]);
        let b = lines("Kreuz", vec![line(0.0, 1.0), line(90.0, 1.0)]);
        let base = m.materials().iter().next().unwrap().1.clone();
        let mut add = |name: &str, cut_fill| {
            let guid = m.new_guid();
            let id = m.add_material(sk_model::Material {
                guid,
                name: name.into(),
                cut_fill,
                ..base.clone()
            });
            sk_model::material_key(id)
        };
        let (gas, putz) = (add("A", a), add("B", b));
        let t = DrawTable::resolve(&m, &Theme::dark());
        let p = t.pack(1.0);
        let fa = texel(&t, &p, gas, 4);
        let px = Theme::dark().px_per_mm;
        // 30°: k = cos 30°, cx = tan 30°, cy = 1
        assert!((fa[0] - 0.57735).abs() < 1e-5 && fa[1] == 1.0);
        assert!((fa[2] - 2.0 * px / 0.866_025_4).abs() < 1e-3);
        assert_eq!(texel(&t, &p, gas, 6)[2], 1.0);
        // Kreuz 0° + 90°: waagerechte und senkrechte Linien, Periode = Abstand
        let (f0, f1) = (texel(&t, &p, putz, 4), texel(&t, &p, putz, 5));
        assert_eq!((f0[0], f0[1], f0[3]), (0.0, 1.0, 1.0));
        assert!(f1[0] == 1.0 && f1[1].abs() < 1e-7);
        assert!((f0[2] - px).abs() < 1e-5 && f1[2] == f0[2]);
        assert_eq!(texel(&t, &p, putz, 6)[2], 2.0);
        assert_ne!(texel(&t, &p, gas, 4), texel(&t, &p, putz, 4));
    }
}

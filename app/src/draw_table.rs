//! Zeichentabelle: die Attributtabellen des Modells, aufgelöst in die Werte,
//! die das Netz und der Renderer brauchen (Farben 0..1, Strichbreiten in
//! Bildpunkten bei 96 dpi). Aufgelöst wird nur, wenn sich die Attribute ändern,
//! nicht je Bild.

use sk_model::{edge_kind, EdgeStyle, FillKind, Model};
use sk_paint::Rgba;
use sk_render::pattern;
use sk_ui::theme::Theme;

/// Aussehen eines Baustoffs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatLook {
    /// Ansichtsfläche in 3D.
    pub face: [f32; 3],
    /// Schnittfläche in 3D.
    pub cut: [f32; 3],
    /// Grund unter der Schraffur in der Zeichnung.
    pub cut_bg: [f32; 3],
    /// Schraffur der Schnittfläche ([`pattern`]).
    pub pattern: f32,
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
    pub hatch_spacing_px: f32,
    pub hatch_width_px: f32,
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
    pattern: pattern::NONE,
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
        let pen_rgb = |id| {
            a.pen(id)
                .map_or(rgb_of(env.fill_fallback), |p| rgb(p.color))
        };
        let fallback = MatLook {
            face: rgb_of(env.face),
            cut: rgb_of(env.face),
            cut_bg: rgb_of(env.fill_fallback),
            ..NO_MATERIAL
        };
        let mut mats = vec![fallback];
        // Schraffurabstand und -strich: aus der ersten Linienschraffur bzw. ihrem Stift
        let (mut spacing, mut hatch_width) = (None, None);
        for (id, m) in model.materials().iter() {
            let key = id.index() as usize + 1;
            if mats.len() <= key {
                mats.resize(key + 1, fallback);
            }
            let fill = a.fill(m.cut_fill).map(|f| &f.kind);
            let pat = match fill {
                Some(FillKind::Lines(lines)) => {
                    if spacing.is_none() {
                        spacing = lines.first().map(|l| l.spacing_mm * px_per_mm);
                        hatch_width = a.pen(m.cut_fg).map(|p| p.width_mm * px_per_mm);
                    }
                    // Bis E3: zwei Scharen = Kreuzschraffur, sonst einfach 45°
                    if lines.len() >= 2 {
                        pattern::CROSS
                    } else {
                        pattern::DIAGONAL
                    }
                }
                Some(FillKind::Zigzag { .. }) => pattern::ZIGZAG,
                _ => pattern::NONE,
            };
            let (face, cut) = a
                .surface(m.surface)
                .map_or((fallback.face, fallback.cut), |s| {
                    (rgb(s.color), rgb(s.cut_color))
                });
            mats[key] = MatLook {
                face,
                cut,
                cut_bg: pen_rgb(m.cut_bg),
                pattern: pat,
            };
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
            hatch_spacing_px: spacing.unwrap_or(7.0),
            hatch_width_px: hatch_width.unwrap_or(1.0),
        }
    }

    /// Aussehen zu einem Darstellungsschlüssel (Schnittbit egal).
    pub fn look(&self, key: u16) -> &MatLook {
        let k = (key & !sk_model::material::CUT) as usize;
        self.mats.get(k).unwrap_or(&self.mats[0])
    }

    /// Strichbreite einer Kante in Bildpunkten bei 96 dpi.
    pub fn edge_width(&self, drawing: bool, kind: u8) -> f32 {
        let t = if drawing {
            &self.drawing_edges
        } else {
            &self.model_edges
        };
        t.get(kind as usize).map_or(1.0, |s| s.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::material;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() <= 0.05
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
        assert!(near(t.hatch_spacing_px, 7.0) && near(t.hatch_width_px, 1.0));
        assert_eq!(t.paper, rgb([245, 244, 239]));
        let key = |name: &str| {
            let (id, _) = m.materials().iter().find(|(_, x)| x.name == name).unwrap();
            sk_model::material_key(id)
        };
        let gas = t.look(key("Gasbeton") | material::CUT);
        assert_eq!(gas.pattern, pattern::DIAGONAL);
        assert_eq!(gas.face, rgb([238, 237, 232]));
        assert_eq!(gas.cut, rgb([176, 177, 174]));
        assert_eq!(gas.cut_bg, rgb([255, 255, 255]));
        let ins = t.look(key("Dämmung (WDVS)"));
        assert_eq!(ins.pattern, pattern::ZIGZAG);
        assert_eq!(ins.cut, rgb([232, 196, 92]));
        assert_eq!(t.look(key("Putz")).pattern, pattern::NONE);
        // E10: Stahlbeton mit Kreuzschraffur, gleicher Abstand und Strich
        let rc = t.look(key("Stahlbeton") | material::CUT);
        assert_eq!(rc.pattern, pattern::CROSS);
        assert_eq!(rc.cut_bg, rgb([255, 255, 255]));
        assert_eq!(t.look(material::PLAIN).face, rgb_of(Theme::dark().env.face));
    }
}

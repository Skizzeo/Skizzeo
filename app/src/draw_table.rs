//! Zeichentabelle: die Attributtabellen des Modells, aufgelöst in die Werte,
//! die das Netz und der Renderer brauchen (Farben 0..1, Strichbreiten in
//! Bildpunkten bei 96 dpi). Aufgelöst wird nur, wenn sich die Attribute ändern,
//! nicht je Bild.

use sk_model::{edge_kind, EdgeStyle, FillKind, FillSpace, LineTypeId, Model};
use sk_paint::Rgba;
use sk_render::{DashPattern, EdgeLooks, Looks, EDGE_KINDS, LOOK_ROWS, SOLID};
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
    /// Strich und Lücke; beide 0 = durchgezogen.
    pub dash_px: f32,
    pub gap_px: f32,
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
    /// Bis zu zwei Linienscharen (Stahlbeton: zweite gestrichelt).
    pub lines: [LineLook; 2],
    pub line_count: u8,
    /// Zickzack: Periode längs in Schichtdicken.
    pub zigzag_period: f32,
    /// Muster der Oberfläche (Paket 6, 7), Looks-Zeilen 8–14.
    pub pattern: [[f32; 4]; 7],
    /// Startwert des wilden Verbands (Deckkraft des Musters in Zeile 12
    /// hängt an seiner Tabelle, [`DrawTable::looks_with`]).
    pub wild_seed: Option<u32>,
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
    /// Grundriss des Geschosses darunter (E16, Kantenart `BACKGROUND`).
    pub background: Stroke,
    /// Fugen in Ansichten (Paket 6, Stift „Ansichtsmuster“).
    pub pattern: Stroke,
    /// Startwerte der Verbandstabellen (wilder Verband) in der Reihenfolge
    /// ihrer Nummer, siehe [`wild_seeds`].
    pub bonds: Vec<u32>,
    /// Strichmuster (E4) in Bildpunkten bei 96 dpi, je Kantenart bzw. Linie.
    pub drawing_dash: [DashPattern; edge_kind::COUNT],
    pub model_dash: [DashPattern; edge_kind::COUNT],
    pub background_dash: DashPattern,
    pub section_dash: DashPattern,
    /// Was die Darstellung noch nicht kann (modellbezogene Schraffuren, mehr
    /// als zwei Scharen); gezeichnet wird ersatzweise.
    pub notes: Vec<String>,
}

/// Strichmuster eines Linientyps in Bildpunkten bei 96 dpi (höchstens zwei
/// Einträge). Fehlt der Linientyp oder ist ein Wert unbrauchbar, Volllinie.
pub fn dash_px(m: &Model, id: LineTypeId, px_per_mm: f32) -> DashPattern {
    let Some(l) = m.attr().line_type(id) else {
        return SOLID;
    };
    let mut p = SOLID;
    for (slot, d) in p.iter_mut().zip(&l.pattern) {
        *slot = [
            d.len_mm * px_per_mm,
            d.gap_mm * px_per_mm,
            d.dot as u8 as f32,
            0.0,
        ];
    }
    let ok = p.iter().flatten().all(|v| v.is_finite() && *v >= 0.0)
        && p.iter().any(|e| e[1] > 0.0)
        && p.iter().all(|e| e[0] + e[1] > 0.0 || *e == [0.0; 4]);
    if ok {
        p
    } else {
        SOLID
    }
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
        dash_px: 0.0,
        gap_px: 0.0,
    }; 2],
    line_count: 0,
    zigzag_period: 1.0,
    pattern: [[0.0; 4]; 7],
    wild_seed: None,
};

/// Aussehen von Flächen ohne Baustoff (und Rückfall bei fehlenden Verweisen).
pub fn fallback_look(theme: &Theme) -> MatLook {
    let env = &theme.env;
    MatLook {
        face: rgb_of(env.face),
        cut: rgb_of(env.face),
        cut_bg: rgb_of(env.fill_fallback),
        cut_fg: rgb_of(env.edge),
        ..NO_MATERIAL
    }
}

/// Aussehen zu den Darstellungsverweisen eines Baustoffs (auch für die
/// Vorschau im Einstellungsfenster). Was die Darstellung nicht kann, landet
/// mit Hinweis in `notes`.
pub fn mat_look(
    model: &Model,
    theme: &Theme,
    d: &sk_model::MaterialDisplay,
    notes: &mut Vec<String>,
) -> MatLook {
    let a = model.attr();
    let env = &theme.env;
    let px_per_mm = theme.px_per_mm;
    let pen_rgb = |id, fallback| a.pen(id).map_or(fallback, |p| rgb(p.color));
    let fallback = fallback_look(theme);
    let mut look = MatLook {
        cut_bg: pen_rgb(d.cut_bg, rgb_of(env.fill_fallback)),
        cut_fg: pen_rgb(d.cut_fg, rgb_of(env.edge)),
        width_px: a.pen(d.cut_fg).map_or(1.0, |p| p.width_mm * px_per_mm),
        ..fallback
    };
    (look.face, look.cut) = a
        .surface(d.surface)
        .map_or((fallback.face, fallback.cut), |s| {
            (rgb(s.color), rgb(s.cut_color))
        });
    look.pattern = pattern_rows(model, d.surface);
    look.wild_seed = match a.surface(d.surface).and_then(|s| s.pattern.as_ref()) {
        Some(sk_model::proctex::Pattern::Masonry {
            bond: sk_model::proctex::Bond::Wild,
            seed,
            ..
        }) => Some(*seed),
        _ => None,
    };
    if let Some(f) = a.fill(d.cut_fill) {
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
            FillKind::Lines(all) => {
                look.kind = fill_kind::LINES;
                // Abstand ≤ 0 oder nicht endlich ergäbe im Shader mod(x, 0)
                // (Review H11): solche Scharen fallen mit Hinweis weg
                let ok = |l: &&sk_model::HatchLine| {
                    l.spacing_mm > 0.0
                        && [l.angle_deg, l.spacing_mm, l.offset_mm]
                            .iter()
                            .all(|v| v.is_finite())
                };
                let lines: Vec<_> = all.iter().filter(ok).copied().collect();
                if lines.len() < all.len() {
                    notes.push(format!(
                        "Schraffur „{}“: Schar ohne gültigen Abstand übersprungen",
                        f.name
                    ));
                }
                if lines.len() > 2 {
                    notes.push(format!(
                        "Schraffur „{}“: nur die ersten zwei von {} Scharen",
                        f.name,
                        lines.len()
                    ));
                }
                for (slot, l) in look.lines.iter_mut().zip(&lines) {
                    *slot = LineLook {
                        angle_deg: l.angle_deg,
                        spacing_px: l.spacing_mm * px_per_mm,
                        offset_px: l.offset_mm * px_per_mm,
                        dash_px: l.dash_mm * px_per_mm,
                        gap_px: l.gap_mm * px_per_mm,
                    };
                    // Strich nur mit endlichem, positivem Paar
                    let (d, g) = (slot.dash_px, slot.gap_px);
                    if !(d.is_finite() && g.is_finite() && d > 0.0 && g > 0.0) {
                        (slot.dash_px, slot.gap_px) = (0.0, 0.0);
                    }
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
    look
}

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
        let fallback = fallback_look(theme);
        let mut mats = vec![fallback];
        let mut notes = Vec::new();
        for (id, m) in model.materials().iter() {
            let key = id.index() as usize + 1;
            if mats.len() <= key {
                mats.resize(key + 1, fallback);
            }
            mats[key] = mat_look(model, theme, &m.display(), &mut notes);
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
            background: stroke(&d.background),
            pattern: stroke(&d.pattern),
            bonds: wild_seeds(model),
            drawing_dash: d.drawing.map(|s| dash_px(model, s.line_type, px_per_mm)),
            model_dash: d.model3d.map(|s| dash_px(model, s.line_type, px_per_mm)),
            background_dash: dash_px(model, d.background.line_type, px_per_mm),
            section_dash: dash_px(model, d.section_line.line_type, px_per_mm),
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
            for (row, v) in look_rows(m, px_scale).into_iter().enumerate() {
                t[row * keys + k] = v;
            }
        }
        t
    }

    /// Breite und Farbe je Kantenart, für Zeichnung oder 3D.
    pub fn edge_looks(&self, drawing: bool, px_scale: f32) -> EdgeLooks {
        let (t, dash) = if drawing {
            (&self.drawing_edges, &self.drawing_dash)
        } else {
            (&self.model_edges, &self.model_dash)
        };
        let scaled =
            |p: &DashPattern| p.map(|[l, g, dot, _]| [l * px_scale, g * px_scale, dot, 0.0]);
        let mut e = EdgeLooks::default();
        for (i, ((w, c), p)) in t.iter().zip(dash).enumerate().take(EDGE_KINDS) {
            e.width[i] = w * px_scale;
            e.color[i] = [c[0], c[1], c[2]];
            [e.dash[2 * i], e.dash[2 * i + 1]] = scaled(p);
        }
        let (w, c) = self.background;
        let k = edge_kind::BACKGROUND as usize;
        e.width[k] = w * px_scale;
        e.color[k] = [c[0], c[1], c[2]];
        [e.dash[2 * k], e.dash[2 * k + 1]] = scaled(&self.background_dash);
        e
    }

    /// Alles, was der Renderer zum Aussehen braucht.
    #[cfg(test)]
    pub fn looks(&self, px_scale: f32) -> Looks {
        self.looks_with(px_scale, |_| 1.0)
    }

    /// Wie [`DrawTable::looks`], Deckkraft der Muster mit wildem Verband je
    /// Startwert aus `fade` (Einblenden). Liegt eine Verbandstabelle noch
    /// nicht vor, rechnet sie im Hintergrund, und die Fläche zeigt bis
    /// dahin ihre Mischfarbe (Deckkraft 0, Koordinator 19:55).
    pub fn looks_with(&self, px_scale: f32, fade: impl Fn(u32) -> f32) -> Looks {
        let tables: Vec<_> = self
            .bonds
            .iter()
            .map(|&seed| sk_model::proctex::bond_table_ready(seed))
            .collect();
        let opacity = |seed: u32| {
            self.bonds
                .iter()
                .zip(&tables)
                .find(|(s, _)| **s == seed)
                .map_or(0.0, |(_, t)| if t.is_some() { fade(seed) } else { 0.0 })
        };
        let mut texels = self.pack(px_scale);
        let keys = self.mats.len();
        for (k, m) in self.mats.iter().enumerate() {
            if let Some(seed) = m.wild_seed {
                texels[12 * keys + k][3] = opacity(seed);
            }
        }
        Looks {
            keys,
            texels,
            drawing: self.edge_looks(true, px_scale),
            model: self.edge_looks(false, px_scale),
            pattern_ink: {
                let (w, c) = self.pattern;
                [c[0], c[1], c[2], w * px_scale]
            },
            bond: tables
                .iter()
                .flat_map(|t| {
                    t.as_ref()
                        .map_or_else(|| vec![0; sk_render::BOND_TABLE_BYTES], |t| t.cells.clone())
                })
                .collect(),
        }
    }
}

/// Die [`LOOK_ROWS`] Texel eines Aussehens (Aufbau: [`sk_render::Looks`]).
pub fn look_rows(m: &MatLook, px_scale: f32) -> [[f32; 4]; LOOK_ROWS] {
    let mut t = [[0.0; 4]; LOOK_ROWS];
    t[0] = [m.face[0], m.face[1], m.face[2], 0.0];
    t[1] = [m.cut[0], m.cut[1], m.cut[2], 0.0];
    t[2] = [m.cut_bg[0], m.cut_bg[1], m.cut_bg[2], m.kind];
    let w = m.width_px * px_scale;
    t[3] = [m.cut_fg[0], m.cut_fg[1], m.cut_fg[2], w];
    let mut offsets = [0.0; 2];
    let mut dashes = [0.0; 4];
    for (i, l) in m.lines.iter().enumerate().take(m.line_count as usize) {
        let (f, offset) = family(l, px_scale);
        t[4 + i] = f;
        offsets[i] = offset;
        if l.dash_px > 0.0 && l.gap_px > 0.0 {
            dashes[2 * i] = l.dash_px * px_scale;
            dashes[2 * i + 1] = l.gap_px * px_scale;
        }
    }
    t[6] = [offsets[0], offsets[1], m.line_count as f32, m.zigzag_period];
    t[7] = dashes;
    t[8..15].copy_from_slice(&m.pattern);
    // Deckkraft des Musters (Einblenden nach der Verbandstabelle)
    t[12][3] = 1.0;
    t
}

/// Farbe als eine Zahl `r·65536 + g·256 + b`, in `f32` exakt (bis 2²⁴).
pub fn pack_rgb(c: [u8; 3]) -> f32 {
    (c[0] as u32 * 65536 + c[1] as u32 * 256 + c[2] as u32) as f32
}

/// Umkehrung von [`pack_rgb`] (wie im Shader).
#[cfg(test)]
pub fn unpack_rgb(f: f32) -> [u8; 3] {
    let n = f as u32;
    [(n >> 16) as u8, (n >> 8) as u8, n as u8]
}

/// Startwerte aller Oberflächen mit wildem Verband, ohne Doppelte, in der
/// Reihenfolge der Oberflächen: Nummer = Platz der Verbandstabelle in
/// [`sk_render::Looks::bond`].
pub fn wild_seeds(m: &Model) -> Vec<u32> {
    use sk_model::proctex::{Bond, Pattern};
    let mut out = Vec::new();
    for (_, s) in m.attr().surfaces().iter() {
        if let Some(Pattern::Masonry {
            bond: Bond::Wild,
            seed,
            ..
        }) = &s.pattern
        {
            if !out.contains(seed) {
                out.push(*seed);
            }
        }
    }
    out
}

/// Looks-Zeilen 8–14 einer Oberfläche (Aufbau: [`sk_render::Looks`]):
/// ohne Muster und bei Fremdem Art 0. Die Deckkraft (Zeile 12, Feld w)
/// setzt [`look_rows`].
pub fn pattern_rows(m: &Model, s: sk_model::SurfaceId) -> [[f32; 4]; 7] {
    use sk_model::proctex::{Bond, Palette, Pattern};
    let mut t = [[0.0; 4]; 7];
    let surface = m.attr().surface(s);
    let pattern = surface.and_then(|x| x.pattern.as_ref());
    let base = surface.map_or([0; 3], |x| x.color);
    // Palette in zwei Zeilen: Farbe 1, Anteil 1, Farbe 2, Anteil 2 | Farbe 3,
    // (frei), Anteil 3, (frei)
    let pal = |t: &mut [[f32; 4]; 7], row: usize, p: &Palette| {
        t[row] = [pack_rgb(p[0].0), p[0].1, pack_rgb(p[1].0), p[1].1];
        t[row + 1][0] = pack_rgb(p[2].0);
        t[row + 1][2] = p[2].1;
    };
    match pattern {
        Some(Pattern::Masonry {
            len,
            h,
            joint,
            bond,
            joint_rgb,
            palette,
            hpal,
            flame,
            fend,
            relief,
            spread,
            seed,
        }) => {
            let offset = match bond {
                Bond::Half => 0.5,
                Bond::Third => 1.0 / 3.0,
                Bond::Wild => -1.0,
                Bond::Block => 2.0,
                Bond::Cross => 3.0,
            };
            t[0] = [1.0, *len, *h, *joint];
            // Nummer der Verbandstabelle (Looks-Zeile 9, Feld w)
            let table = wild_seeds(m).iter().position(|x| x == seed).unwrap_or(0);
            t[1] = [offset, *spread, *seed as f32, table as f32];
            pal(&mut t, 2, palette);
            t[3][1] = pack_rgb(*joint_rgb);
            t[4] = [*flame, *fend, *relief, 0.0];
            pal(&mut t, 5, hpal.as_ref().unwrap_or(palette));
        }
        Some(Pattern::Plaster {
            grain,
            spread,
            seed,
        }) => {
            t[0] = [2.0, 0.0, 0.0, 0.0];
            t[1] = [0.0, *spread, *seed as f32, *grain];
        }
        Some(Pattern::Concrete {
            w,
            h,
            joint,
            anchors,
            cloud,
            pores,
            seed,
        }) => {
            t[0] = [3.0, *w, *h, *joint];
            t[1] = [*anchors as u8 as f32, *cloud, *seed as f32, 0.0];
            t[4][2] = *pores;
        }
        Some(Pattern::Timber {
            vertical,
            board,
            joint,
            grain,
            c1,
            c2,
            seed,
        }) => {
            t[0] = [4.0, *board, *joint, *vertical as u8 as f32];
            t[1] = [*grain, 0.0, *seed as f32, 0.0];
            t[2] = [pack_rgb(*c1), 0.0, pack_rgb(*c2), 0.0];
        }
        Some(Pattern::Tiles {
            len,
            wid,
            joint,
            half,
            joint_rgb,
            palette,
            spread,
            seed,
        }) => {
            t[0] = [5.0, *len, *wid, *joint];
            t[1] = [*half as u8 as f32, *spread, *seed as f32, 0.0];
            pal(&mut t, 2, palette);
            t[3][1] = pack_rgb(*joint_rgb);
        }
        Some(Pattern::Stone {
            size,
            joint,
            irr,
            joint_rgb,
            palette,
            seed,
        }) => {
            t[0] = [6.0, *size, *joint, *irr];
            t[1] = [0.0, 0.0, *seed as f32, 0.0];
            pal(&mut t, 2, palette);
            t[3][1] = pack_rgb(*joint_rgb);
        }
        Some(Pattern::Foreign(_)) | None => {}
    }
    // Mischfarbe für 3D aus der Ferne und ohne Muster (Zeile 11, Feld w)
    if t[0][0] > 0.0 {
        if let Some(p) = pattern {
            t[3][3] = pack_rgb(sk_model::proctex::mix(p, base));
        }
    }
    t
}

/// Wie eine Fläche ihr Muster zeigt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternUse {
    None,
    /// Ansicht: nur Fugenlinien in Tinte.
    Lines,
    /// 3D: Steinfarben und Fugen.
    Colors,
}

/// Musterdarstellung einer Fläche (paket-6 §3.3): Ansichten zeigen Linien
/// (auch mit Schalter „Muster in 3D“ aus), 3D Farben nur mit Schalter;
/// geschnittene Flächen, Grundriss, Schnitt und Blasses nie.
pub fn pattern_use(view: crate::ui::ViewKind, cut: bool, alpha: f32, on: bool) -> PatternUse {
    use crate::ui::ViewKind as V;
    if cut || alpha < 1.0 {
        return PatternUse::None;
    }
    match view {
        V::Front | V::Back | V::Left | V::Right => PatternUse::Lines,
        V::Persp if on => PatternUse::Colors,
        _ => PatternUse::None,
    }
}

/// `u_patterns` einer Ansicht (Flächen deckend, nicht geschnitten).
pub fn pattern_mode(view: crate::ui::ViewKind, on: bool) -> i32 {
    match pattern_use(view, false, 1.0, on) {
        PatternUse::None => sk_render::pattern_mode::NONE,
        PatternUse::Lines => sk_render::pattern_mode::LINES,
        PatternUse::Colors => sk_render::pattern_mode::COLORS,
    }
}

/// Texel einer Linienschar und ihr Versatz.
///
/// Die Linien sind `cx·x + cy·y = Versatz + n·Periode` (Bildpunkte, y nach
/// oben) mit der Normalen `(cx, cy) = (sin w, −cos w) / k`, `k = max(|sin w|,
/// |cos w|)`: so hat eine der beiden Zahlen genau den Betrag 1. Der Winkel
/// zählt gegen den Uhrzeigersinn (E3b, 45° = „/“); 135° rechnet im Shader
/// bitgleich wie die frühere feste Schraffur (`mod(x + y, Abstand·√2)·√½`).
fn family(l: &LineLook, px_scale: f32) -> ([f32; 4], f32) {
    let w = (l.angle_deg as f64).to_radians();
    let (sin, cos) = (w.sin(), w.cos());
    let k = sin.abs().max(cos.abs());
    let inv_k = (1.0 / k) as f32;
    let spacing = l.spacing_px * px_scale;
    let period = spacing * inv_k;
    let offset = l.offset_px * px_scale * inv_k;
    let f = [(sin / k) as f32, (-cos / k) as f32 + 0.0, period, k as f32];
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
        // E15: Stahlbeton zwei Scharen mit doppeltem Abstand, gleicher Strich
        let rc = t.look(key(&m, "Stahlbeton") | material::CUT);
        assert_eq!((rc.kind, rc.line_count), (fill_kind::LINES, 2));
        assert!(near(rc.lines[0].spacing_px, 2.0 * gas.lines[0].spacing_px));
        assert_eq!(rc.width_px, gas.width_px);
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
        // Farben gleich den Oberflächen, Art 2 mit einer Schar unter 135°
        assert_eq!(texel(&t, &p, gas, 0)[..3], surf(gas).face);
        assert_eq!(texel(&t, &p, gas, 1)[..3], surf(gas).cut);
        assert_eq!(texel(&t, &p, gas, 2)[3], 2.0);
        let f = texel(&t, &p, gas, 4);
        // 135° („\\“): cx = cy = 1, Periode = Abstand·√2, k = √½ wie im früheren Shader
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
        // E15, Test 5: Stahlbeton zwei Scharen wie Mauerwerk mit doppeltem
        // Abstand, die zweite um einen Abstand versetzt und gestrichelt
        let rc = key(&m, "Stahlbeton");
        let (r4, r5, r6) = (
            texel(&t, &p, rc, 4),
            texel(&t, &p, rc, 5),
            texel(&t, &p, rc, 6),
        );
        assert_eq!((r4[0], r4[1], r5[0], r5[1]), (1.0, 1.0, 1.0, 1.0));
        assert!((r4[2] - 2.0 * f[2]).abs() < 1e-4 && r5[2] == r4[2]);
        assert_eq!((r6[0], r6[2]), (0.0, 2.0));
        assert!(
            (r6[1] - f[2]).abs() < 1e-4,
            "Versatz = ein Mauerwerksabstand"
        );
        let px = Theme::dark().px_per_mm;
        assert_eq!(texel(&t, &p, rc, 7), [0.0, 0.0, 1.5 * px, 0.75 * px]);
        assert_eq!(texel(&t, &p, gas, 7), [0.0; 4]);
        assert_eq!(texel(&t, &p.clone(), rc, 7)[2], 8.25);
        assert_eq!(t.pack(2.0)[7 * t.mats.len() + rc as usize][2], 16.5);
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

    /// Review H11: Scharen ohne gültigen Abstand fallen mit Hinweis weg,
    /// ungültige Striche zeichnen durchgezogen.
    #[test]
    fn ungueltige_schar_wird_uebersprungen() {
        let mut m = Model::with_seed(1);
        let guid = m.new_guid();
        let bad = m.add_fill(Fill {
            guid,
            name: "Kaputt".into(),
            kind: FillKind::Lines(vec![
                HatchLine::solid(45.0, 0.0, 0.0),
                HatchLine::solid(45.0, f32::NAN, 0.0),
                HatchLine {
                    dash_mm: f32::INFINITY,
                    gap_mm: 1.0,
                    ..HatchLine::solid(45.0, 2.0, 0.0)
                },
            ]),
            space: FillSpace::Paper,
        });
        let base = m.materials().iter().next().unwrap().1.clone();
        let guid = m.new_guid();
        let id = m.add_material(sk_model::Material {
            guid,
            name: "X".into(),
            cut_fill: bad,
            ..base
        });
        let t = DrawTable::resolve(&m, &Theme::dark());
        let k = sk_model::material_key(id);
        let look = t.look(k);
        assert_eq!((look.kind, look.line_count), (fill_kind::LINES, 1));
        assert_eq!((look.lines[0].dash_px, look.lines[0].gap_px), (0.0, 0.0));
        assert!(
            t.notes.iter().any(|n| n.contains("Kaputt")),
            "{:?}",
            t.notes
        );
        assert!(t.pack(1.0).iter().flatten().all(|v| v.is_finite()));
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
        let line = |angle_deg, spacing_mm| HatchLine::solid(angle_deg, spacing_mm, 0.0);
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
        // 30° (gegen den Uhrzeigersinn): k = cos 30°, cx = tan 30°, cy = −1
        assert!((fa[0] - 0.57735).abs() < 1e-5 && fa[1] == -1.0);
        assert!((fa[2] - 2.0 * px / 0.866_025_4).abs() < 1e-3);
        assert_eq!(texel(&t, &p, gas, 6)[2], 1.0);
        // Kreuz 0° + 90°: waagerechte und senkrechte Linien, Periode = Abstand
        let (f0, f1) = (texel(&t, &p, putz, 4), texel(&t, &p, putz, 5));
        assert_eq!((f0[0], f0[1], f0[3]), (0.0, -1.0, 1.0));
        assert!(f1[0] == 1.0 && f1[1].abs() < 1e-7);
        assert!((f0[2] - px).abs() < 1e-5 && f1[2] == f0[2]);
        assert_eq!(texel(&t, &p, putz, 6)[2], 2.0);
        assert_ne!(texel(&t, &p, gas, 4), texel(&t, &p, putz, 4));
    }
}

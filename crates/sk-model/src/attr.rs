//! Attributtabellen der Darstellung: Stifte, Linientypen, Schraffuren,
//! Oberflächen und die Bauteildarstellung (welche Kante mit welchem Stift).
//!
//! Reine Daten ohne Abhängigkeit zur Oberfläche. Baustoffe verweisen auf diese
//! Tabellen, statt Farben und Muster selbst zu tragen. Jede Änderung über die
//! Methoden erhöht [`Attributes::rev`]; daran erkennt die Anzeige, dass sie ihre
//! Zeichentabelle neu auflösen muss.

use crate::guid::{Guid, GuidGen};
use crate::id::{Arena, Id};
use crate::solid::edge_kind;

pub type PenId = Id<Pen>;
pub type LineTypeId = Id<LineType>;
pub type FillId = Id<Fill>;
pub type SurfaceId = Id<Surface>;

/// Stift: Farbe und Strichbreite auf dem Papier.
#[derive(Clone, Debug, PartialEq)]
pub struct Pen {
    pub guid: Guid,
    /// Stiftnummer für Menschen (wie in CAD-Stiftlisten).
    pub number: u16,
    pub name: String,
    pub color: [u8; 3],
    /// Strichbreite in mm auf dem Papier.
    pub width_mm: f32,
}

/// Ein Strich mit folgender Lücke (Längen in mm auf dem Papier).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dash {
    pub len_mm: f32,
    pub gap_mm: f32,
    /// Nach der Lücke ein Punkt (Strichpunktlinie).
    pub dot: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LineType {
    pub guid: Guid,
    pub name: String,
    /// Leer = Volllinie.
    pub pattern: Vec<Dash>,
}

/// Bezug einer Schraffur: fest auf dem Papier oder mit dem Modell skaliert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillSpace {
    Paper,
    Model,
}

/// Eine Schar paralleler Schraffurlinien.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HatchLine {
    /// Richtung der Linien, gegen den Uhrzeigersinn ab der Waagerechten
    /// (mathematisch, E3b): 45° steigt nach rechts („/“), 135° fällt („\“).
    pub angle_deg: f32,
    pub spacing_mm: f32,
    pub offset_mm: f32,
    /// Strich und Lücke (mm); beide 0 = durchgezogen (E15).
    pub dash_mm: f32,
    pub gap_mm: f32,
}

impl HatchLine {
    /// Durchgezogene Schar.
    pub const fn solid(angle_deg: f32, spacing_mm: f32, offset_mm: f32) -> HatchLine {
        HatchLine {
            angle_deg,
            spacing_mm,
            offset_mm,
            dash_mm: 0.0,
            gap_mm: 0.0,
        }
    }
}

/// Stahlbeton (E15, Jörn 06.10.): Diagonale wie Mauerwerk (135°, Linien alle
/// 1,27 mm), jede zweite gestrichelt 1,5/0,75 mm. Zwei Scharen mit doppeltem
/// Abstand, die zweite um einen Abstand versetzt.
pub fn concrete_lines() -> Vec<HatchLine> {
    vec![
        HatchLine::solid(135.0, 2.54, 0.0),
        HatchLine {
            dash_mm: 1.5,
            gap_mm: 0.75,
            ..HatchLine::solid(135.0, 2.54, 1.27)
        },
    ]
}

/// Harte Dämmbauteile (W1, Jörn 10.10. 05:53): Kreuzschraffur 45° und
/// 135° im Abstand der Mauerwerkschraffur (1,27 mm), z. B. Schaumglas am
/// Fußpunkt der Verblendschale.
pub fn cross_lines() -> Vec<HatchLine> {
    vec![
        HatchLine::solid(45.0, 1.27, 0.0),
        HatchLine::solid(135.0, 1.27, 0.0),
    ]
}

/// Name der Werksschraffur aus [`cross_lines`].
pub const CROSS_FILL_NAME: &str = "Dämmung hart (Kreuz)";

/// Mauerwerk: Diagonale 135° alle 1,27 mm.
pub fn masonry_lines() -> Vec<HatchLine> {
    vec![HatchLine::solid(135.0, 1.27, 0.0)]
}

#[derive(Clone, Debug, PartialEq)]
pub enum FillKind {
    Empty,
    Solid,
    Lines(Vec<HatchLine>),
    /// Zickzack quer durch die Schicht; Periode längs in Schichtdicken.
    Zigzag {
        period: f32,
    },
}

/// Schraffur einer Schnittfläche.
#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    pub guid: Guid,
    pub name: String,
    pub kind: FillKind,
    pub space: FillSpace,
}

/// Oberfläche in 3D: Farbe der Ansichtsfläche und der Schnittfläche.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub guid: Guid,
    pub name: String,
    pub color: [u8; 3],
    pub cut_color: [u8; 3],
    /// Prozedurales Muster (Paket 6): Fugen in Ansichten, Steine und Korn
    /// in 3D; ändert nie Mengen oder Geometrie (Regel 56).
    pub pattern: Option<crate::proctex::Pattern>,
}

/// Stift und Linientyp einer Kante.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeStyle {
    pub pen: PenId,
    pub line_type: LineTypeId,
}

/// Bauteildarstellung: Kantenart → Stift und Linientyp, je Ansichtsstil.
#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    /// Grundriss, Schnitt und Ansichten, Index = [`edge_kind`].
    pub drawing: [EdgeStyle; edge_kind::COUNT],
    /// 3D, Index = [`edge_kind`].
    pub model3d: [EdgeStyle; edge_kind::COUNT],
    /// Geländelinie in Schnitt und Ansichten.
    pub ground: EdgeStyle,
    /// Schnittlinie A–A, dünne Mitte.
    pub section_line: EdgeStyle,
    /// Schnittlinie, kräftige Enden.
    pub section_ends: EdgeStyle,
    /// Grundriss des Geschosses darunter, wenn ein Obergeschoss aktiv ist (E16).
    pub background: EdgeStyle,
    /// Fugenmuster in den Ansichten (Paket 6), Stift „Ansichtsmuster“.
    pub pattern: EdgeStyle,
    /// Papiergrund der Zeichnungsansichten.
    pub paper: [u8; 3],
}

/// Alle Attributtabellen eines Modells.
/// Verweis auf ein Attribut (für „verwendet von …“).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrRef {
    Pen(PenId),
    LineType(LineTypeId),
    Fill(FillId),
    Surface(SurfaceId),
}

/// Wer ein Attribut verwendet: eine Stelle der Darstellung oder ein Baustoff.
#[derive(Clone, Debug, PartialEq)]
pub enum AttrUser {
    /// Bezeichnung der Stelle, z. B. „Schnittkante im Grundriss“.
    Display(&'static str),
    /// Baustoff und Rolle („Schraffur“, „Grund“, „Schnittmuster“, „Oberfläche“).
    Material {
        id: crate::library::MaterialId,
        name: String,
        role: &'static str,
    },
}

impl AttrUser {
    /// Deutsche Bezeichnung für Hinweise.
    pub fn label(&self) -> String {
        match self {
            AttrUser::Display(s) => (*s).into(),
            AttrUser::Material { name, role, .. } => format!("{name} ({role})"),
        }
    }
}

/// Stellen der Darstellung mit ihrer Bezeichnung: Kantenarten in Zeichnungen
/// und in 3D, Gelände, Schnittlinie, Hintergrund.
pub fn display_slots(d: &Display) -> Vec<(&'static str, EdgeStyle)> {
    let kind = |k: u8| match k {
        edge_kind::VIEW => ("Sichtkante in Zeichnungen", "Sichtkante in 3D"),
        edge_kind::CUT => ("Schnittkante in Zeichnungen", "Schnittkante in 3D"),
        edge_kind::FINE => ("Fuge in Zeichnungen", "Fuge in 3D"),
        _ => (
            "Schnittkante Schicht in Zeichnungen",
            "Schnittkante Schicht in 3D",
        ),
    };
    let mut v = Vec::new();
    for (k, s) in d.drawing.iter().enumerate() {
        v.push((kind(k as u8).0, *s));
    }
    for (k, s) in d.model3d.iter().enumerate() {
        v.push((kind(k as u8).1, *s));
    }
    v.push(("Gelände", d.ground));
    v.push(("Schnittlinie", d.section_line));
    v.push(("Schnittlinie Enden", d.section_ends));
    v.push(("Hintergrund", d.background));
    v.push(("Fugen in Ansichten", d.pattern));
    v
}

#[derive(Clone, Debug)]
pub struct Attributes {
    pens: Arena<Pen>,
    line_types: Arena<LineType>,
    fills: Arena<Fill>,
    surfaces: Arena<Surface>,
    display: Display,
    rev: u64,
}

impl Attributes {
    /// Tabellen aus einer Datei ([`crate::szo`]); die Revision ist neu (1).
    pub(crate) fn from_parts(
        pens: Arena<Pen>,
        line_types: Arena<LineType>,
        fills: Arena<Fill>,
        surfaces: Arena<Surface>,
        display: Display,
    ) -> Attributes {
        Attributes {
            pens,
            line_types,
            fills,
            surfaces,
            display,
            rev: 1,
        }
    }

    /// Steigt bei jeder Änderung.
    pub fn rev(&self) -> u64 {
        self.rev
    }

    pub fn pens(&self) -> &Arena<Pen> {
        &self.pens
    }

    pub fn pen(&self, id: PenId) -> Option<&Pen> {
        self.pens.get(id)
    }

    pub fn line_types(&self) -> &Arena<LineType> {
        &self.line_types
    }

    pub fn line_type(&self, id: LineTypeId) -> Option<&LineType> {
        self.line_types.get(id)
    }

    pub fn fills(&self) -> &Arena<Fill> {
        &self.fills
    }

    pub fn fill(&self, id: FillId) -> Option<&Fill> {
        self.fills.get(id)
    }

    pub fn surfaces(&self) -> &Arena<Surface> {
        &self.surfaces
    }

    pub fn surface(&self, id: SurfaceId) -> Option<&Surface> {
        self.surfaces.get(id)
    }

    pub fn display(&self) -> &Display {
        &self.display
    }

    pub(crate) fn add_pen(&mut self, p: Pen) -> PenId {
        self.rev += 1;
        self.pens.insert(p)
    }

    pub(crate) fn add_line_type(&mut self, l: LineType) -> LineTypeId {
        self.rev += 1;
        self.line_types.insert(l)
    }

    pub(crate) fn add_fill(&mut self, f: Fill) -> FillId {
        self.rev += 1;
        self.fills.insert(f)
    }

    pub(crate) fn add_surface(&mut self, s: Surface) -> SurfaceId {
        self.rev += 1;
        self.surfaces.insert(s)
    }

    pub(crate) fn set_pen(&mut self, id: PenId, p: Pen) -> bool {
        let ok = self.pens.get_mut(id).map(|old| *old = p).is_some();
        self.rev += ok as u64;
        ok
    }

    pub(crate) fn remove_pen(&mut self, id: PenId) -> Option<Pen> {
        let p = self.pens.remove(id)?;
        self.rev += 1;
        Some(p)
    }

    pub(crate) fn set_line_type(&mut self, id: LineTypeId, l: LineType) -> bool {
        let ok = self.line_types.get_mut(id).map(|old| *old = l).is_some();
        self.rev += ok as u64;
        ok
    }

    pub(crate) fn remove_line_type(&mut self, id: LineTypeId) -> Option<LineType> {
        let l = self.line_types.remove(id)?;
        self.rev += 1;
        Some(l)
    }

    pub(crate) fn remove_fill(&mut self, id: FillId) -> Option<Fill> {
        let f = self.fills.remove(id)?;
        self.rev += 1;
        Some(f)
    }

    pub(crate) fn remove_surface(&mut self, id: SurfaceId) -> Option<Surface> {
        let s = self.surfaces.remove(id)?;
        self.rev += 1;
        Some(s)
    }

    pub(crate) fn set_fill(&mut self, id: FillId, f: Fill) -> bool {
        let ok = self.fills.get_mut(id).map(|old| *old = f).is_some();
        self.rev += ok as u64;
        ok
    }

    pub(crate) fn set_surface(&mut self, id: SurfaceId, s: Surface) -> bool {
        let ok = self.surfaces.get_mut(id).map(|old| *old = s).is_some();
        self.rev += ok as u64;
        ok
    }

    pub(crate) fn set_display(&mut self, d: Display) {
        self.display = d;
        self.rev += 1;
    }

    // Rückgängig/Wiederholen ([`crate::Model::apply`]): Einträge mit ihrer
    // alten Kennung setzen; die Revision steigt danach einmal über `bump`.

    pub(crate) fn put_pen(&mut self, id: PenId, v: Option<Pen>) {
        self.pens.set(id, v);
    }

    pub(crate) fn put_line_type(&mut self, id: LineTypeId, v: Option<LineType>) {
        self.line_types.set(id, v);
    }

    pub(crate) fn put_fill(&mut self, id: FillId, v: Option<Fill>) {
        self.fills.set(id, v);
    }

    pub(crate) fn put_surface(&mut self, id: SurfaceId, v: Option<Surface>) {
        self.surfaces.set(id, v);
    }

    pub(crate) fn put_display(&mut self, d: Display) {
        self.display = d;
    }

    pub(crate) fn bump(&mut self) {
        self.rev += 1;
    }

    /// Verstöße gegen die Verweisregeln (jede Kantenart verweist auf Lebendes).
    pub fn check(&self) -> Vec<String> {
        let d = &self.display;
        let styles = d.drawing.iter().chain(&d.model3d).chain([
            &d.ground,
            &d.section_line,
            &d.section_ends,
            &d.background,
            &d.pattern,
        ]);
        let mut out = Vec::new();
        let mut numbers: Vec<u16> = self.pens.iter().map(|(_, p)| p.number).collect();
        numbers.sort_unstable();
        if numbers.windows(2).any(|w| w[0] == w[1]) {
            out.push("Stiftnummer doppelt vergeben".into());
        }
        for s in styles {
            if !self.pens.contains(s.pen) {
                out.push(format!("Darstellung: Stift {:?} fehlt", s.pen));
            }
            if !self.line_types.contains(s.line_type) {
                out.push(format!("Darstellung: Linientyp {:?} fehlt", s.line_type));
            }
        }
        out
    }

    /// Guids aller Attribute (für die Eindeutigkeitsprüfung des Modells).
    pub fn guids(&self) -> impl Iterator<Item = Guid> + '_ {
        let p = self.pens.iter().map(|(_, x)| x.guid);
        let l = self.line_types.iter().map(|(_, x)| x.guid);
        let f = self.fills.iter().map(|(_, x)| x.guid);
        let s = self.surfaces.iter().map(|(_, x)| x.guid);
        p.chain(l).chain(f).chain(s)
    }
}

/// Startsatz der Linientypen nach DIN ISO 128-20 / DIN 1356-1 (E4), Längen
/// in mm auf dem Papier. Die Volllinie steht in [`defaults`], die übrigen
/// legt das Modell danach an (und ergänzt sie in älteren Dateien).
pub fn standard_line_types() -> [(&'static str, Vec<Dash>); 4] {
    let d = |len_mm, gap_mm, dot| Dash {
        len_mm,
        gap_mm,
        dot,
    };
    [
        ("Volllinie", Vec::new()),
        ("Strichlinie", vec![d(3.0, 1.0, false)]),
        ("Strichpunktlinie", vec![d(6.0, 1.0, true)]),
        ("Punktlinie", vec![d(0.0, 1.0, true)]),
    ]
}

/// Name der Linientyps der Schnittlinie A–A im Startsatz.
pub const SECTION_LINE_TYPE: &str = "Strichpunktlinie";

/// Startverweise der Standardbaustoffe auf die Tabellen.
#[derive(Clone, Copy, Debug)]
pub struct Standard {
    pub hatch_pen: PenId,
    pub background: PenId,
    pub empty: FillId,
    pub masonry: FillId,
    pub insulation: FillId,
}

/// Nummer des Stifts „Hintergrund“ (E16), in älteren Dateien ergänzt.
pub(crate) const BACKGROUND_PEN: u16 = 9;

/// Werks-Stift der Fugen in Ansichten (Paket 6): Nummer, Name, Farbe, Breite.
/// Ältere Dateien bekommen ihn beim Öffnen.
pub const PATTERN_PEN: (u16, &str, [u8; 3], f32) = (10, "Ansichtsmuster", [96, 96, 96], 0.13);

/// Feste Guid des Stifts „Ansichtsmuster“, in jedem Projekt gleich (wie die
/// Werkstypen), damit die übrigen Guids des Startsatzes gleich bleiben.
pub const PATTERN_PEN_GUID: Guid = Guid(0x25d4de18452e4d84aba3d0fd35e1ba32);

/// Der Stift „Ansichtsmuster“; ist Nummer 10 vergeben, die nächste freie
/// Nummer.
pub(crate) fn pattern_pen(guid: Guid, taken: impl Fn(u16) -> bool) -> Pen {
    let (mut number, name, color, width_mm) = PATTERN_PEN;
    while taken(number) {
        number += 1;
    }
    Pen {
        guid,
        number,
        name: name.into(),
        color,
        width_mm,
    }
}

/// Starttabellen. Die Werte ergeben dieselbe Zeichnung wie vor den Tabellen
/// (bei 5,5 px je mm).
pub fn defaults(guids: &mut GuidGen) -> (Attributes, Standard) {
    let mut pens = Arena::new();
    let mut pen = |number, name: &str, color, width_mm| {
        pens.insert(Pen {
            guid: guids.next_guid(),
            number,
            name: name.into(),
            color,
            width_mm,
        })
    };
    let black = [0, 0, 0];
    let fine = pen(1, "Fein", black, 0.13);
    let medium = pen(2, "Mittel", black, 0.30);
    let strong = pen(3, "Kräftig", black, 0.50);
    let hatch_pen = pen(4, "Schraffur", black, 0.18);
    let background = pen(5, "Hintergrund weiß", [255, 255, 255], 0.0);
    let edge3d = pen(6, "3D-Kante", black, 0.23);
    let sect_thin = pen(7, "Schnittlinie", black, 0.22);
    let sect_strong = pen(8, "Schnittlinie Enden", black, 0.58);
    let under = pen(BACKGROUND_PEN, "Hintergrund", [160, 160, 160], 0.13);

    let mut line_types = Arena::new();
    let solid = line_types.insert(LineType {
        guid: guids.next_guid(),
        name: "Volllinie".into(),
        pattern: Vec::new(),
    });

    let mut fills = Arena::new();
    let mut fill = |name: &str, kind| {
        fills.insert(Fill {
            guid: guids.next_guid(),
            name: name.into(),
            kind,
            space: FillSpace::Paper,
        })
    };
    let empty = fill("Leer", FillKind::Empty);
    let masonry = fill("Mauerwerk", FillKind::Lines(masonry_lines()));
    let insulation = fill("Dämmung hart", FillKind::Zigzag { period: 1.0 });

    let style = |pen| EdgeStyle {
        pen,
        line_type: solid,
    };
    let mut drawing = [style(medium); edge_kind::COUNT];
    drawing[edge_kind::VIEW as usize] = style(medium);
    drawing[edge_kind::CUT as usize] = style(strong);
    drawing[edge_kind::FINE as usize] = style(fine);
    drawing[edge_kind::CUT_LAYER as usize] = style(medium);
    let mut model3d = [style(edge3d); edge_kind::COUNT];
    model3d[edge_kind::FINE as usize] = style(fine);

    let attr = Attributes {
        pens,
        line_types,
        fills,
        surfaces: Arena::new(),
        display: Display {
            drawing,
            model3d,
            ground: style(strong),
            section_line: style(sect_thin),
            section_ends: style(sect_strong),
            background: style(under),
            // Startwert; das Modell setzt den Stift „Ansichtsmuster“
            pattern: style(fine),
            paper: [245, 244, 239],
        },
        rev: 0,
    };
    let std = Standard {
        hatch_pen,
        background,
        empty,
        masonry,
        insulation,
    };
    (attr, std)
}

#[cfg(test)]
mod tests {
    use crate::model::Model;

    #[test]
    fn startwerte_sind_vollstaendig() {
        let m = Model::with_seed(1);
        assert!(m.check().is_empty(), "{:?}", m.check());
        let a = m.attr();
        // Paket 6: Stift 10 „Ansichtsmuster“
        assert_eq!(a.pens().len(), 10);
        // E16: Stift 9 „Hintergrund“ für den Grundriss darunter
        let bg = a.pen(a.display().background.pen).unwrap();
        assert_eq!(
            (bg.number, bg.name.as_str(), bg.color, bg.width_mm),
            (9, "Hintergrund", [160, 160, 160], 0.13)
        );
        for (_, mat) in m.materials().iter() {
            assert!(a.fill(mat.cut_fill).is_some());
            assert!(a.pen(mat.cut_fg).is_some() && a.pen(mat.cut_bg).is_some());
            assert!(a.surface(mat.surface).is_some());
        }
        // E15: Stahlbeton als Diagonale 135°, jede zweite gestrichelt, Stift 4 auf Stift 5
        let (_, rc) = m
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Stahlbeton")
            .unwrap();
        let f = a.fill(rc.cut_fill).unwrap();
        assert_eq!(f.name, "Stahlbeton");
        let super::FillKind::Lines(l) = &f.kind else {
            panic!("Linienschraffur erwartet");
        };
        let v: Vec<_> = l
            .iter()
            .map(|h| (h.angle_deg, h.spacing_mm, h.offset_mm, h.dash_mm, h.gap_mm))
            .collect();
        assert_eq!(
            v,
            [(135.0, 2.54, 0.0, 0.0, 0.0), (135.0, 2.54, 1.27, 1.5, 0.75)]
        );
        let masonry = a
            .fills()
            .iter()
            .find(|(_, f)| f.name == "Mauerwerk")
            .unwrap()
            .1;
        assert_eq!(masonry.kind, super::FillKind::Lines(super::masonry_lines()));
        assert_eq!(super::masonry_lines()[0].angle_deg, 135.0);
        assert_eq!(a.pen(rc.cut_fg).unwrap().number, 4);
        assert_eq!(a.pen(rc.cut_bg).unwrap().number, 5);
    }

    #[test]
    fn aenderung_erhoeht_revision_auch_nach_rueckgaengig() {
        let mut m = Model::with_seed(2);
        let (id, pen) = m
            .attr()
            .pens()
            .iter()
            .next()
            .map(|(i, p)| (i, p.clone()))
            .unwrap();
        let (rev, model_rev) = (m.attr().rev(), m.revision());
        m.begin("Stift");
        assert!(m.set_pen(
            id,
            super::Pen {
                width_mm: 1.0,
                ..pen.clone()
            }
        ));
        let t = m.commit().unwrap();
        assert!(m.attr().rev() > rev && m.revision() > model_rev);
        // Zurück zum alten Stand: Werte wie vorher, Revision aber neu
        let changed = m.attr().rev();
        let touched = m.apply(&t, crate::Direction::Undo);
        assert!(touched.attr);
        assert!(m.attr().rev() > changed);
        assert!((m.attr().pen(id).unwrap().width_mm - 0.13).abs() < 1e-6);
        // Ein Verweis ins Leere fällt auf: Stift aus einem anderen Modell
        let mut other = Model::with_seed(3);
        let foreign = other.add_pen(pen);
        assert!(m.attr().pen(foreign).is_none());
        let mut d = m.attr().display().clone();
        d.ground.pen = foreign;
        m.set_display(d);
        assert!(!m.check().is_empty());
    }
}

//! Paneele über der 3D-Ansicht: links „Werkzeuge“ mit dem Knopf „Gebäude“ und
//! unten der Knopf „Geschossmanager“ (Jörn 11.10.; das Diagramm „Geschosse“,
//! E14, steht im eigenen Fenster), rechts „Ansichten“ (3D, Grundriss, Schnitt
//! und vier Ansichten) und darunter, solange ein Bauteil gewählt ist,
//! „Eigenschaften“.

use crate::ext_props::ExtZeile;
use crate::freischaltung::{Freischaltung, Modul};
use crate::meldung::Meldung;
use crate::type_look::TypeLook;
use crate::wall_tool::Sprung;
use sk_model::oeffnung::OpeningKind;
use sk_model::{RefSide, StoreyId};
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{Cursor, Event, Key, Modifiers, MouseButton};
use sk_ui::theme::{Sizes, Theme};
use sk_ui::widgets::{self, ButtonState, FieldState, Fonts, Rect};

mod fenster;
mod gebaeude_dialog;
mod gelaende;
mod geschosse;
mod hoehenlinien;
#[cfg(test)]
mod umbruch_tests;
mod werkzeuge;
use hoehenlinien::check_word;
use werkzeuge::tool_rows;

pub use gelaende::{GelaendeFeld, GelaendeKnopf, GelaendeLeiste};
pub(crate) use gelaende::{NHN, OBEN, RECHTS, UEBER};

pub use fenster::TITEL as LEVELS_TITEL;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewKind {
    Persp,
    Plan,
    Section,
    Front,
    Back,
    Left,
    Right,
}

impl ViewKind {
    pub const ALL: [ViewKind; 7] = [
        ViewKind::Persp,
        ViewKind::Plan,
        ViewKind::Section,
        ViewKind::Front,
        ViewKind::Back,
        ViewKind::Left,
        ViewKind::Right,
    ];

    /// Name für die Befehlszeile (`--ansicht grundriss`).
    pub fn arg(self) -> &'static str {
        match self {
            ViewKind::Persp => "3d",
            ViewKind::Plan => "grundriss",
            ViewKind::Section => "schnitt",
            ViewKind::Front => "vorne",
            ViewKind::Back => "hinten",
            ViewKind::Left => "links",
            ViewKind::Right => "rechts",
        }
    }

    pub fn from_arg(s: &str) -> Option<ViewKind> {
        let s = s.to_lowercase();
        ViewKind::ALL.into_iter().find(|v| v.arg() == s)
    }

    /// Name auf dem Knopf: „Vorne“ … ohne Nordrichtung, sonst die
    /// Himmelsrichtung der gezeigten Fassade (Sonnenstand S3).
    pub fn knopf(self, nord: Option<f64>) -> &'static str {
        match (self.himmel(nord), self) {
            (Some(i), _) => HIMMEL[i],
            (None, ViewKind::Front) => "Vorne",
            (None, ViewKind::Back) => "Hinten",
            (None, ViewKind::Left) => "Links",
            (None, ViewKind::Right) => "Rechts",
            (None, ViewKind::Persp) => "3D",
            (None, ViewKind::Plan) => "Grundriss",
            (None, ViewKind::Section) => "Schnitt",
        }
    }

    /// Voller Name für den Tooltip: „Südansicht“, ohne Nordrichtung wie der
    /// Knopf.
    pub fn titel(self, nord: Option<f64>) -> String {
        match self.himmel(nord) {
            Some(i) => format!("{}ansicht", HIMMEL[i]),
            None => self.knopf(None).to_string(),
        }
    }

    /// Himmelsrichtung der Fassade, die die Ansicht zeigt (Sonnenstand S3,
    /// Analyse §3.2): Index in [`HIMMEL`]; nur für die vier Ansichten und
    /// mit gesetzter Nordrichtung. Die Ansicht heißt nach der Fassade wie im
    /// Bauantrag: „Vorne“ blickt nach +y und zeigt die Fassade nach −y, bei
    /// Nord 0° die Südansicht. Zwischenrichtungen von 22,5° bis 67,5° usw.;
    /// genau auf der Grenze gilt die Hauptrichtung.
    pub fn himmel(self, nord: Option<f64>) -> Option<usize> {
        // Richtung der Fassade in Grad im Uhrzeigersinn von +y
        let fassade = match self {
            ViewKind::Front => 180.0,
            ViewKind::Back => 0.0,
            ViewKind::Left => 270.0,
            ViewKind::Right => 90.0,
            ViewKind::Persp | ViewKind::Plan | ViewKind::Section => return None,
        };
        let nord = nord.filter(|n| n.is_finite())?;
        let x = (fassade - nord).rem_euclid(360.0) / 45.0;
        let i = if ((x - x.floor()) - 0.5).abs() < 1e-9 {
            // Auf der Grenze: die Hauptrichtung (gerader Index)
            let f = x.floor() as usize;
            if f.is_multiple_of(2) {
                f
            } else {
                f + 1
            }
        } else {
            x.round() as usize
        };
        Some(i % 8)
    }
}

/// Himmelsrichtungen im Uhrzeigersinn ab Nord (Sonnenstand S3).
pub const HIMMEL: [&str; 8] = [
    "Nord", "Nordost", "Ost", "Südost", "Süd", "Südwest", "West", "Nordwest",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Building,
    Interior,
    Ref(RefSide),
    /// Winkelsprung 90°, 45° oder frei (G2).
    Sprung(Sprung),
    View(ViewKind),
    /// „Mengen · Kosten · AVA“ (B7, KA-2a, KA-4): öffnet das Mengenfenster oder holt es nach vorn.
    Quantity,
    /// „Projektdaten“ über „Werkzeuge“ (Paket PD-2): öffnet die Maske.
    Projektdaten,
    /// Symbolkachel unter den Projektdaten (Sonnenstand S2): Nordpfeil
    /// aufziehen.
    Nord,
    Field(Field),
    /// Griff einer Ebene im Paneel „Geschosse“.
    Grip(Grip),
    /// Name eines Geschosses im Paneel „Geschosse“: macht es aktiv.
    Storey(StoreyId),
    /// Schalter „Flachdach“ im Fuß des Paneels „Geschosse“ (Jörn 10.10.).
    FlatRoof,
    /// Schalter im Geschossmanager: Fensterlinien (0) bzw. Innentürlinie (1)
    /// zeigen (Jörn 12.10. 05:26).
    LineVis(u8),
    /// Dialog „Gebäude erstellen“ (E16): Schließkreuz, Zähler − und +
    /// (derzeit gesperrt), „Abbrechen“ und „Zeichnen beginnen“.
    DialogClose,
    DialogMinus,
    DialogPlus,
    DialogCancel,
    DialogStart,
    /// Typ-Chip (K3) unter dem Werkzeug bzw. im Paneel „Eigenschaften“:
    /// öffnet die Typ-Liste.
    ToolType,
    PropsType,
    /// Gestapelte Wand (OG Phase 2): Kettensymbol (löst bzw. koppelt) und
    /// „bündig setzen“.
    PropsLink,
    PropsFlush,
    /// Wand im Zug (W-11, W-12): „Wand teilen“ und „an der Fuge vereinen“.
    PropsSplit,
    PropsMerge,
    /// Wand mit freiem Ende (Grundriss G5): „Ecke bilden“ und „Verlängern“.
    PropsEcke,
    PropsVerlaengern,
    /// „Abläufe vorschlagen“ im Abschnitt „Gefälle“ des Flachdachs (G2).
    PropsDrains,
    /// „Ablauf hinzufügen“ und „Ablauf löschen“ (den gewählten) im
    /// Abschnitt „Gefälle“ (Jörn 11.10.).
    PropsDrainAdd,
    PropsDrainDelete,
    /// Knopf unter einem Abschnitt der Eigenschaften des Regenfallrohrs:
    /// Notentwässerung und Kontrollablauf.
    PropsAction(crate::selection::fallrohr::Action),
    /// „Mehr …“ unter dem Aufbau der Dachterrasse: klappt die Schichtliste
    /// auf und zu.
    PropsMore,
    /// Baustoffname einer Schicht (Paket 5): öffnet „Baustoffe …“ mit ihm.
    PropsMaterial(sk_model::Guid),
    /// „Erweiterungen“ unter den Wandknöpfen (E6): öffnet die Liste; beim
    /// Setzen trägt er den Namen des Bauteils.
    Ext,
    /// Typ-Chip des Werkzeugs „Erweiterungen“.
    ExtTyp,
    /// Eigenschaften einer Erweiterung (E7): Knopf `k` der Wahl des
    /// Parameters `i`, Schalter `janein`, Knopf zur Liste einer Wahl ab vier.
    ExtWahl(u8, u8),
    ExtJa(u8),
    ExtListe(u8),
    /// Knopf „Geschossmanager“ unten links (Jörn 11.10.): öffnet das
    /// Geschossfenster links neben dem Hauptfenster oder holt es nach vorn.
    LevelsOpen,
    /// „Wandöffnung“ und „Türöffnung“ unter den Wandknöpfen (Paket O3):
    /// schaltet das Werkzeug der Art ein und aus.
    Opening(OpeningKind),
    /// Knopf „Gelände“ und die Knöpfe seiner Leiste (plan-gelaendemodul.md
    /// §4.7).
    Gelaende(GelaendeKnopf),
    /// Knopf des Raumbuchs im Paneel eines Raums (RB6): öffnet die Liste.
    Raumbuch(u8),
}

/// Muster der Knöpfe der Eigenschaften, die `App::drain_button` ausführt:
/// Abläufe am Flachdach, Notentwässerung und Kontrollablauf am
/// Regenfallrohr.
macro_rules! props_button {
    () => {
        $crate::ui::Id::PropsDrains
            | $crate::ui::Id::PropsDrainAdd
            | $crate::ui::Id::PropsDrainDelete
            | $crate::ui::Id::PropsAction(_)
    };
}
pub(crate) use props_button;

/// Muster der Knöpfe an der Wand, die `App::kette_klick` ausführt: Kette,
/// „bündig setzen“, Teilen, Vereinen, „Ecke bilden“ und „Verlängern“.
macro_rules! kette_button {
    () => {
        $crate::ui::Id::PropsLink
            | $crate::ui::Id::PropsFlush
            | $crate::ui::Id::PropsSplit
            | $crate::ui::Id::PropsMerge
            | $crate::ui::Id::PropsEcke
            | $crate::ui::Id::PropsVerlaengern
    };
}
pub(crate) use kette_button;

/// Ziehbare Ebene im Paneel „Geschosse“. ±0,00 liegt fest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grip {
    /// Unterkante der Gründung.
    FoundationBottom,
    /// Oberkante eines Geschosses.
    Top(StoreyId),
    /// OK Gelände (Gelände Thema 1).
    Terrain,
    /// Höhenlinien eines Geschosses im Geschossmanager (Jörn 11.10.
    /// 13:57): Brüstung und Sturz, den Ebenen untergeordnet, und die
    /// Oberkante der Innentüren (12.10. 05:26).
    Sill(StoreyId),
    Lintel(StoreyId),
    Door(StoreyId),
    /// Schnitthöhe des Grundrisses im aktiven Geschoss (Jörn 14:12).
    PlanCut,
}

/// Zahlenfelder im Paneel „Eigenschaften“.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    SlabThickness,
    Recess,
    /// Perimeterdämmung unter der Sohlplatte (cm, 0 = keine; Gelände Thema 4).
    Insulation,
    FootingWidth,
    FootingDepth,
    FloorThickness,
    /// Versatz einer gestapelten Wand in m (+ außen), OG Phase 2.
    Offset,
    /// Dicke der Untersichtdämmung an der Decke (OG-17).
    Soffit,
    /// Bekleidung darunter (W3): Dicke (0 = keine), Überstand des
    /// Verblenders, Achsabstände der Grund- und Traglattung.
    Cladding(sk_model::CladdingValue),
    /// Dachterrasse (D1–D3): Dicken von Dämmung und Belag ihres Typs,
    /// Attika über OK Belag an der Decke.
    TerraceInsulation,
    TerraceFinish,
    Upstand,
    /// Dämmstreifen auf der Attikakrone unter dem Blech (0 = keiner).
    Crown,
    /// Attikadämmung innen an der Aufkantung des Flachdachs (0 = keine).
    AttikaInner,
    /// Dämmdicke im Typ des Flachdachs (D3).
    RoofInsulation,
    /// Gefälle des Flachdachs in % (0 = waagerecht; Gefälledämmung G2).
    RoofSlope,
    /// Schellenabstand des Regenfallrohrs (m, Jörn 11.10.).
    DownpipeSpacing,
    /// Paneel „Geschosse“ (in m): Kote der Gründungsunterkante, Kote der
    /// Oberkante eines Geschosses, Geschosshöhe (bei der Gründung die
    /// Gründungstiefe) und lichte Höhe.
    LevelBottom,
    LevelTop(StoreyId),
    StoreyHeight(StoreyId),
    ClearHeight(StoreyId),
    /// Kote von OK Gelände im Paneel „Geschosse“ (m, Gelände Thema 1).
    Terrain,
    /// Dialog „Gebäude erstellen“ (E16, Jörn 10:13).
    Draft(Draft),
    /// Feld `i` des Werkzeugs „Erweiterungen“ (E6), in der Einheit des
    /// Parameters.
    ExtTool(u8),
    /// Parameter `i` einer gewählten Erweiterung im Paneel „Eigenschaften“
    /// (E7).
    ExtProp(u8),
    /// Geländeplatte (Gelände G0): Dicke in cm, Abstand ums Haus in m.
    PlateThick,
    PlateMargin,
    /// Felder der Leiste „Gelände“ (G1).
    Gelaende(GelaendeFeld),
    /// Höhe der Wandfliesen eines Raums (m, RB6).
    RaumFliesen,
}

/// Vorgaben im Dialog „Gebäude erstellen“, von oben nach unten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Draft {
    /// Aufkantung des Flachdachs über dem OG (cm, 0 = kein Flachdach;
    /// Plan Flachdach P8: vorgewählt).
    RoofUpstand,
    FloorOg,
    ClearOg,
    FloorEg,
    ClearEg,
    Slab,
    /// Perimeterdämmung unter der Sohlplatte (cm, 0 = keine; Gelände Thema 4).
    Insulation,
    /// OK Sohlplatte über OK Gelände (cm, + = höher; Gelände Thema 1).
    Terrain,
}

impl Draft {
    pub const ALL: [Draft; 8] = [
        Draft::RoofUpstand,
        Draft::FloorOg,
        Draft::ClearOg,
        Draft::FloorEg,
        Draft::ClearEg,
        Draft::Slab,
        Draft::Insulation,
        Draft::Terrain,
    ];

    /// Name für `Scene::set_building_dialog_value`.
    pub fn key(self) -> &'static str {
        match self {
            Draft::RoofUpstand => "flachdach",
            Draft::FloorOg => "decke_og",
            Draft::ClearOg => "lichte_og",
            Draft::FloorEg => "decke_eg",
            Draft::ClearEg => "lichte_eg",
            Draft::Slab => "sohlplatte",
            Draft::Insulation => "perimeter",
            Draft::Terrain => "gelaende",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Draft::RoofUpstand => "Aufkantung Flachdach",
            Draft::FloorOg => "Dicke OG-Decke",
            Draft::ClearOg => "lichte Höhe OG",
            Draft::FloorEg => "Dicke EG-Decke",
            Draft::ClearEg => "lichte Höhe EG",
            Draft::Slab => "Dicke Sohlplatte",
            Draft::Insulation => "Perimeterdämmung",
            Draft::Terrain => "OK Sohle über Gelände",
        }
    }

    fn next(self) -> Option<Draft> {
        let i = Draft::ALL.iter().position(|d| *d == self)?;
        Draft::ALL.get(i + 1).copied()
    }
}

impl Field {
    /// Zahl im Paneel „Geschosse“, in Metern.
    pub fn is_level(self) -> bool {
        matches!(
            self,
            Field::LevelBottom
                | Field::LevelTop(_)
                | Field::StoreyHeight(_)
                | Field::ClearHeight(_)
                | Field::Terrain
        )
    }

    /// Zahl in Metern (sonst in Zentimetern).
    fn in_metres(self) -> bool {
        self.is_level()
            || matches!(
                self,
                Field::Offset | Field::DownpipeSpacing | Field::PlateMargin | Field::RaumFliesen
            )
            || matches!(self, Field::Draft(Draft::ClearEg | Draft::ClearOg))
            || self == Field::Gelaende(GelaendeFeld::Hoehe)
            || self == Field::Gelaende(GelaendeFeld::Breite)
    }

    /// Zahl in Prozent, ohne Umrechnung (Gefälle).
    fn in_percent(self) -> bool {
        self == Field::RoofSlope
    }

    fn unit(self) -> &'static str {
        if self.in_percent() {
            "%"
        } else if self.in_metres() {
            "m"
        } else {
            "cm"
        }
    }

    /// Kote (mit Vorzeichen) statt Länge.
    fn is_kote(self) -> bool {
        matches!(
            self,
            Field::LevelBottom
                | Field::LevelTop(_)
                | Field::Terrain
                | Field::Gelaende(GelaendeFeld::Hoehe)
        )
    }

    /// Wert zum Bearbeiten (ohne Einheit, Minus als „-“). Der Versatz trägt
    /// sein Vorzeichen auch nach außen: „+0,30“.
    fn text(self, mm: f64) -> String {
        if self.in_percent() {
            zahl_text(mm)
        } else if !self.in_metres() {
            cm_text(mm)
        } else if mm.round() < 0.0 {
            format!("-{}", m_text(mm))
        } else if self == Field::Offset && mm.round() > 0.0 {
            format!("+{}", m_text(mm))
        } else {
            m_text(mm)
        }
    }

    /// Wert, wie er im Feld steht, solange es nicht bearbeitet wird: Minus
    /// als echtes Minuszeichen („−0,30“), 0 ohne Zeichen.
    fn display(self, mm: f64) -> String {
        self.text(mm).replacen('-', "\u{2212}", 1)
    }

    /// Wert mit Einheit für Hinweise.
    fn show(self, mm: f64) -> String {
        if self.in_percent() {
            format!("{} %", zahl_text(mm))
        } else if !self.in_metres() {
            format!("{} cm", cm_text(mm))
        } else if self.is_kote() {
            format!("{} m", kote_text(mm))
        } else {
            format!("{} m", m_text(mm))
        }
    }
}

/// Ein Zahlenfeld: Wert und erlaubter Bereich in mm, angezeigt in cm.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldRow {
    pub field: Field,
    pub label: &'static str,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    /// 0 ist zusätzlich erlaubt (Sockelrücksprung: bündig).
    pub zero: bool,
    /// Feld eines Erweiterungsbauteils (E6): Wert in dieser Einheit, ohne
    /// Umrechnung; außerhalb min/max gilt er, rot mit „zulässig X bis Y“.
    pub einheit: Option<&'static str>,
}

/// Zahl wie im Feld eines Erweiterungsbauteils: höchstens drei Stellen
/// nach dem Komma, ohne Nullen am Ende.
pub fn zahl_text(v: f64) -> String {
    let t = format!("{:.3}", v);
    let t = t.trim_end_matches('0').trim_end_matches('.');
    let t = if t == "-0" { "0" } else { t };
    t.replace('.', ",")
}

impl FieldRow {
    /// Wert zum Bearbeiten.
    fn text(&self, v: f64) -> String {
        match self.einheit {
            Some(_) => zahl_text(v),
            None => self.field.text(v),
        }
    }

    /// Wert, wie er im Feld steht.
    fn display(&self, v: f64) -> String {
        match self.einheit {
            Some(_) => zahl_text(v).replacen('-', "\u{2212}", 1),
            None => self.field.display(v),
        }
    }

    fn unit(&self) -> &'static str {
        self.einheit.unwrap_or_else(|| self.field.unit())
    }

    /// Außerhalb von min/max (nur Felder von Erweiterungen): „zulässig X
    /// bis Y“.
    pub fn ausserhalb(&self) -> Option<Meldung> {
        let e = self.einheit?;
        let null = self.zero && self.value == 0.0;
        if null || self.value >= self.min - 1e-9 && self.value <= self.max + 1e-9 {
            return None;
        }
        let z = |v: f64| format!("{} {e}", zahl_text(v)).trim_end().to_string();
        Some(match (self.min.is_finite(), self.max.is_finite()) {
            // Wie die Werkbank: die Einheit einmal am Ende
            (true, true) => {
                Meldung::mit("zulässig {} bis {}", &[&zahl_text(self.min), &z(self.max)])
            }
            (true, false) => Meldung::mit("zulässig ab {}", &[&z(self.min)]),
            _ => Meldung::mit("zulässig bis {}", &[&z(self.max)]),
        })
    }

    /// Prüft eine Eingabe in cm (Paneel „Geschosse“: in m, Gefälle: in %);
    /// `Ok` mit dem Wert in mm (auf 1 mm gerundet, Gefälle in %). Felder von Erweiterungen: in ihrer
    /// Einheit, auch außerhalb von min/max.
    pub fn parse(&self, text: &str) -> Result<f64, Meldung> {
        let t = text.trim().replace(',', ".");
        if t.is_empty() {
            return Err(Meldung::satz("Zahl fehlt"));
        }
        let n: f64 = t.parse().map_err(|_| Meldung::satz("keine Zahl"))?;
        if !n.is_finite() {
            return Err(Meldung::satz("keine Zahl"));
        }
        if self.einheit.is_some() {
            // Felder der Geländefläche: nur im Bereich (Bedienbarkeit 43.1)
            if matches!(self.field, Field::Gelaende(g) if g.im_paneel()) {
                let r = FieldRow {
                    value: n,
                    ..self.clone()
                };
                if let Some(m) = r.ausserhalb() {
                    return Err(if self.zero {
                        Meldung::mit("0 oder {}", &[m.as_str()])
                    } else {
                        m
                    });
                }
            }
            return Ok(n);
        }
        let mm = if self.field.in_percent() {
            // Prozent auf eine Nachkommastelle
            (n * 10.0).round() / 10.0 + 0.0
        } else {
            let per = if self.field.in_metres() { 1000.0 } else { 10.0 };
            (n * per).round() + 0.0
        };
        if self.zero && mm == 0.0 {
            return Ok(0.0);
        }
        let f = self.field;
        if mm < self.min - 1e-6 {
            return Err(if self.zero {
                Meldung::mit("0 oder mindestens {}", &[&f.show(self.min)])
            } else {
                Meldung::mit("mindestens {}", &[&f.show(self.min)])
            });
        }
        if mm > self.max + 1e-6 {
            return Err(Meldung::mit("höchstens {}", &[&f.show(self.max)]));
        }
        Ok(mm)
    }
}

/// Art eines getippten Maßes (Paket 8): Länge in m (> 0 bis 200 m),
/// Winkel in Grad, Versatz in m mit Vorzeichen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasureKind {
    Length,
    Angle,
    Offset,
}

/// Getipptes Maß, das nicht gilt; [`MeasureError::message`] für die
/// Statuszeile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasureError(pub MeasureKind);

impl MeasureError {
    pub fn message(&self) -> &'static str {
        match self.0 {
            MeasureKind::Length => "Länge zwischen 0,01 und 200 m",
            MeasureKind::Angle => "Winkel zwischen −360 und 360°",
            MeasureKind::Offset => "Versatz zwischen −200 und 200 m",
        }
    }
}

/// Strenges Zahlbild für getippte Maße (Paket 8 §1.1, Nachtrag 3o): nur
/// Ziffern, höchstens ein Dezimalkomma oder -punkt, ein führendes Minus nur
/// bei Winkel und Versatz. Länge und Versatz kommen in mm (auf 1 mm
/// gerundet), der Winkel in Grad (auf 0,1° gerundet).
pub fn parse_measure(text: &str, kind: MeasureKind) -> Result<f64, MeasureError> {
    let err = Err(MeasureError(kind));
    let t = text.trim();
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) if kind != MeasureKind::Length => (true, b),
        Some(_) => return err,
        None => (false, t),
    };
    let mut sep = false;
    let mut digits = false;
    for ch in body.chars() {
        match ch {
            '0'..='9' => digits = true,
            ',' | '.' if !sep => sep = true,
            _ => return err,
        }
    }
    if !digits {
        return err;
    }
    let Ok(v) = body.replace(',', ".").parse::<f64>() else {
        return err;
    };
    let v = if neg { -v } else { v };
    match kind {
        MeasureKind::Length => {
            let mm = (v * 1000.0).round();
            if mm > 0.0 && mm <= 200_000.0 {
                Ok(mm)
            } else {
                err
            }
        }
        MeasureKind::Offset => {
            let mm = (v * 1000.0).round();
            if mm.abs() <= 200_000.0 {
                Ok(mm + 0.0)
            } else {
                err
            }
        }
        MeasureKind::Angle => {
            let g = (v * 10.0).round() / 10.0;
            if g.abs() <= 360.0 {
                Ok(g + 0.0)
            } else {
                err
            }
        }
    }
}

/// mm als Zentimeter mit höchstens einer Nachkommastelle, Dezimalkomma.
pub fn cm_text(mm: f64) -> String {
    let t = (mm.round() / 10.0).to_string();
    t.replace('.', ",")
}

/// mm als Meter ohne Vorzeichen: zwei Nachkommastellen, eine dritte nur,
/// wenn sie nötig ist (2,855).
pub fn m_text(mm: f64) -> String {
    let mm = mm.round().abs();
    let d = if mm % 10.0 == 0.0 { 2 } else { 3 };
    format!("{:.*}", d, mm / 1000.0).replace('.', ",")
}

/// Live-Länge am Gummiband (Paket 8, Darstellung §2.1): immer zwei
/// Nachkommastellen mit Komma und „ m“, z. B. „4,37 m“.
pub fn live_length_text(mm: f64) -> String {
    format!("{:.2} m", mm.abs() / 1000.0).replace('.', ",")
}

/// Höhenkote wie in der Bauzeichnung: ±0,00, +2,855, −0,80.
pub fn kote_text(mm: f64) -> String {
    let r = mm.round();
    if r == 0.0 {
        "±0,00".into()
    } else if r > 0.0 {
        format!("+{}", m_text(r))
    } else {
        format!("\u{2212}{}", m_text(r))
    }
}

/// Laufende Eingabe in einem Zahlenfeld. Text nur aus ASCII-Zeichen.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub field: Field,
    pub text: String,
    caret: usize,
    anchor: usize,
    /// Grund, warum die Eingabe nicht gilt (unter dem Feld).
    pub error: Option<Meldung>,
    /// Wert beim Beginn; Esc stellt ihn im Dialog wieder her (dort gilt jede
    /// gültige Taste sofort).
    orig: f64,
}

impl Edit {
    fn new(row: &FieldRow) -> Edit {
        let text = row.text(row.value);
        Edit {
            field: row.field,
            orig: row.value,
            caret: text.len(),
            anchor: 0,
            text,
            error: None,
        }
    }

    fn selection(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    fn replace_selection(&mut self, with: &str) {
        let (a, z) = self.selection();
        self.text.replace_range(a..z, with);
        self.caret = a + with.len();
        self.anchor = self.caret;
    }

    fn move_to(&mut self, i: usize, extend: bool) {
        self.caret = i.min(self.text.len());
        if !extend {
            self.anchor = self.caret;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Tools,
    Views,
    Props,
    Levels,
    /// Dialog „Gebäude erstellen“ (E16), rechts neben dem Paneel „Geschosse“.
    Dialog,
}

/// Ein Geschossband im Paneel „Geschosse“.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub id: StoreyId,
    /// Anzeigename: „Fundament“, „EG“, „OG“.
    pub name: String,
    /// Unter- und Oberkante (mm).
    pub bottom: f64,
    pub top: f64,
    /// Gründungsband: Unterkante ziehbar, Oberkante (±0,00) fest.
    pub foundation: bool,
    /// Ebene Flachdach: Oberkante ist OK Aufkantung.
    pub roof: bool,
    pub active: bool,
    /// Brüstung, Sturz und Innentür als Koten (mm); nur Geschosse, nicht
    /// Gründung und Flachdach.
    pub lines: Option<(f64, f64, f64)>,
}

/// Inhalt des Paneels „Geschosse“.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Levels {
    /// Bänder von unten nach oben.
    pub bands: Vec<Band>,
    /// Lichte Höhe (mm) je Geschoss (EG, OG, …), von unten nach oben.
    pub clear: Vec<(StoreyId, f64)>,
    /// Zahlen, die sich eingeben lassen, mit erlaubtem Bereich.
    pub fields: Vec<FieldRow>,
    /// OK Gelände (mm) und Einbindetiefe der Gründung (OK Gelände bis UK
    /// Fundament, mm; Gelände Thema 1). `None` ohne Gründung.
    pub terrain: Option<f64>,
    pub embedment: Option<f64>,
    /// Schalter „Flachdach“ (Jörn 10.10.): an, wenn die Ebene besteht;
    /// `None`: kein Schalter.
    pub flat_roof: Option<bool>,
    /// Schnitthöhe des Grundrisses im aktiven Geschoss als Kote (mm);
    /// `None` im Fundament.
    pub plan_cut: Option<f64>,
}

/// Eine Ebene (Linie) im Diagramm.
struct LevelLine {
    z: f64,
    name: String,
    /// Kote als Zahl änderbar bzw. Ebene ziehbar; ±0,00 weder noch.
    field: Option<Field>,
    grip: Option<Grip>,
    active: bool,
}

/// Linien von unten nach oben: Unterkante jedes Bandes und Oberkante des obersten.
fn level_lines(l: &Levels) -> Vec<LevelLine> {
    let mut out = Vec::new();
    for (i, b) in l.bands.iter().enumerate() {
        let below = i.checked_sub(1).map(|j| &l.bands[j]);
        let (field, grip) = match below {
            _ if b.foundation => (Some(Field::LevelBottom), Some(Grip::FoundationBottom)),
            Some(u) if !u.foundation => (Some(Field::LevelTop(u.id)), Some(Grip::Top(u.id))),
            _ => (None, None),
        };
        out.push(LevelLine {
            z: b.bottom,
            name: b.name.clone(),
            field,
            grip,
            active: b.active,
        });
    }
    if let Some(b) = l.bands.last() {
        out.push(LevelLine {
            z: b.top,
            name: if b.roof {
                "OK Aufkantung".to_string()
            } else {
                format!("OK Decke {}", b.name)
            },
            field: Some(Field::LevelTop(b.id)),
            grip: Some(Grip::Top(b.id)),
            active: false,
        });
    }
    out
}

/// Aufteilung des Paneels „Geschosse“, beim Ziehen eingefroren.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LevelsLayout {
    /// Maßstab (dip je m).
    ppm: f32,
    /// Lage von ±0,00 im Paneel und Paneelhöhe (Pixel).
    anchor: f32,
    height: f32,
    /// Höhe des Fußes (dip); im knappen Paneel 0, ohne Gelände auch.
    fuss: f32,
}

/// Ziehen einer Ebene im Paneel „Geschosse“.
#[derive(Clone, Copy, Debug)]
struct LevelDrag {
    grip: Grip,
    /// Maus und Höhe beim Greifen.
    y0: f64,
    z0: f64,
    layout: LevelsLayout,
}

/// Was das Paneel „Geschosse“ beim Ziehen meldet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LevelEvent {
    Begin(Grip),
    /// Neue Höhe (mm, gefangen) der gezogenen Ebene.
    Move(Grip, f64),
    End,
}

/// Abstände im Diagramm (dip): Titel, Luft über der obersten und unter der
/// untersten Linie.
const LEVEL_HEAD: f32 = 34.0;
const LEVEL_TOP: f32 = 30.0;
const LEVEL_BOTTOM: f32 = 8.0;
/// Zeilen im Fuß des Paneels: OK Gelände, Einbindetiefe.
const LEVEL_FOOTER_ROW: f32 = 20.0;
/// Rechter Rand: Kette der Geschosshöhen und davor die der lichten Höhe.
const CHAIN_OUTER: f32 = 2.0;
const CHAIN_GAP: f32 = 40.0;

/// Schichtzeile im Paneel „Eigenschaften“: Farbfeld, Text, Menge und der
/// Baustoff samt Anfang seines Namens im Text.
pub type LayerRow = (Rgba, String, String, Option<(sk_model::Guid, usize)>);

/// Inhalt des Paneels „Eigenschaften“.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Props {
    /// Zeilen aus Bezeichnung und Wert, z. B. („Länge“, „10,00 m“); ohne
    /// Bezeichnung blass, eingerückt und gekürzt unter der Zeile davor.
    pub values: Vec<(&'static str, String)>,
    /// Name des Aufbaus.
    pub layer_set: String,
    /// Je Schicht: Farbfeld, „14 cm Dämmung (WDVS)“, „3,796 m³ · 76 kg“
    /// und der Baustoff mit dem Anfang seines Namens im Text (Verweis).
    pub layers: Vec<LayerRow>,
    /// Überschrift über `layer_set`; leer heißt „Aufbau“.
    pub set_label: &'static str,
    /// Zahlenfelder (Parameter des Bauteils), zwischen Werten und Aufbau.
    pub fields: Vec<FieldRow>,
    /// Hinweise (Warnungen der Prüfung).
    pub notes: Vec<String>,
    /// Typ der Wand (K3): Chip statt der Zeile mit dem Namen des Aufbaus.
    pub chip: Option<Chip>,
    /// Gestapelte Wand (OG Phase 2): Zeilen „Kopplung“, „Versatz“ und
    /// „bündig setzen“; das Feld „Versatz“ steht in `fields`.
    pub stack: Option<Stack>,
    /// Abschnitte nach den Zahlenfeldern, z. B. „Untersicht“ an der Decke.
    pub sections: Vec<Section>,
    /// Die Schichten stehen erst nach „Mehr …“ unter dem letzten Abschnitt
    /// (Dachterrasse); dann fehlt der Abschnitt „Aufbau“ unten.
    pub more: bool,
    /// Gesperrt (Paket 4): Felder blass, darunter der Satz zum Entsperren.
    pub locked: bool,
    /// Erweiterung (E7): Typ-Chip und Parameter je Gruppe statt Aufbau.
    pub ext: Option<Vec<ExtZeile>>,
    /// Wand im Zug: Knopf „Wand teilen“; dazu, ob sie an einer Fuge in
    /// einer Flucht steht (Knopf „an der Fuge vereinen“).
    pub split: bool,
    pub joint: bool,
    /// Wand mit freiem Ende: Knöpfe „Ecke bilden“ und „Verlängern“ (G5).
    pub ende: bool,
    /// `layer_set` bricht an Leerzeichen auf die Innenbreite um, ungekürzt
    /// (Wände um einen Raum), statt rechts überzulaufen.
    pub set_umbruch: bool,
    /// Zeilen in `values`, die zu Mengen gehören (Raumbuchmengen, Review
    /// #120 B2); ohne Mengen fehlen sie.
    pub mengen: std::ops::Range<usize>,
}

/// Kopplung einer gestapelten Wand im Paneel „Eigenschaften“.
#[derive(Clone, Debug, PartialEq)]
pub struct Stack {
    pub linked: bool,
    /// „mit EG“ bzw. am EG-Segment „mit OG“.
    pub partner: &'static str,
    /// Versatz ≠ 0: „bündig setzen“ ist aktiv.
    pub offset: bool,
}

/// Abschnitt im Paneel „Eigenschaften“: Überschrift, Felder, blasser Hinweis.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub title: &'static str,
    pub fields: Vec<FieldRow>,
    pub hint: &'static str,
    /// Knöpfe unter den Feldern, z. B. „Abläufe vorschlagen“.
    pub buttons: Vec<(Id, &'static str)>,
}

/// Typ-Chip (K3): Kachel mit Schnittbild, Name, „Kürzel · Dicke“, Pfeil.
#[derive(Clone, Debug, PartialEq)]
pub struct Chip {
    pub name: String,
    pub detail: String,
    /// Schnittbild des Typs; Erweiterungen haben keins (E6).
    pub look: Option<TypeLook>,
    /// Die Typ-Liste dazu ist offen: Rand in Akzent, Pfeil nach oben.
    pub open: bool,
    /// Das Bauteil überschreibt Merkmale des Typs: Punkt in Akzent.
    pub marked: bool,
}

/// Paneel „Werkzeuge“ beim Setzen einer Erweiterung (E6, Vertrag §14).
#[derive(Clone, Debug, PartialEq)]
pub struct ExtPanel {
    /// Name des Bauteils auf seinem Knopf.
    pub name: &'static str,
    /// Typ-Auswahl, wenn die Definition Typen hat.
    pub chip: Option<Chip>,
    /// Felder mit `zeichnen=ja`, höchstens vier.
    pub felder: Vec<FieldRow>,
    /// „Länge aus der Eingabe“.
    /// Zeilen, langes zweizeilig ohne Abstand.
    pub aus_eingabe: Vec<&'static str>,
    pub hinweise: [&'static str; 3],
}

/// Breite eines Zahlenfelds (dip).
const FIELD_W: f32 = 60.0;
/// Feld eines Erweiterungsbauteils, Platz für „4200 mm“ (dip).
const EXT_FIELD_W: f32 = 82.0;
/// Zeile „Kopplung“: Breite von Kettensymbol und Partner, Abstand des Texts
/// vom linken Rand dieses Bereichs (dip).
const LINK_W: f32 = 82.0;
/// Klickfläche von „Mehr …“ (dip).
const MORE_W: f32 = 80.0;
/// Kachel des Nordpfeils (Sonnenstand S2, §8 07:30): quadratisch,
/// rechtsbündig neben dem Knopf „Projektdaten“ und der Beschriftung
/// darunter, oben bündig mit dem Knopf; fast dreimal so groß wie zuerst
/// (Jörn 10:06), ohne das Paneel höher zu machen. Ihr Abstand zu Knopf
/// und Beschriftung (dip).
const TILE: f32 = 64.0;
const TILE_GAP: f32 = 8.0;
/// Farbfeld einer Schichtzeile und Abstand zum Text (dip).
const LAYER_SWATCH: f32 = 12.0;
const LAYER_GAP: f32 = 8.0;
const LINK_CHIP_GAP: f32 = 30.0;

/// Fenstergröße (dip), ab der die Paneele in voller Größe erscheinen.
const FULL_W: f32 = 1440.0;
const FULL_H: f32 = 810.0;
/// Kleinster Verkleinerungsfaktor, damit die Schrift lesbar bleibt.
const MIN_FIT: f32 = 0.6;

pub struct Ui {
    /// Wirksame Skalierung der Paneele: Bildschirmskalierung × Fensterfaktor.
    pub scale: f32,
    /// Bildschirmskalierung (dpi / 96).
    dpi: f32,
    pub fonts: Fonts,
    pub hover: Option<Id>,
    pressed: Option<Id>,
    pub view: ViewKind,
    /// Mengenfenster offen: Knopf „Mengen · Kosten · AVA“ in `accent`.
    pub quantity_open: bool,
    /// Freie Module: gesperrte Knöpfe fehlen in den Paneelen.
    pub frei: Freischaltung,
    pub building: bool,
    /// Das Werkzeug zeichnet Innenwände (sonst Außenwände).
    pub interior: bool,
    pub ref_side: RefSide,
    pub sprung: Sprung,
    /// Schichten der Wand, die das Werkzeug zeichnet: Farbfeld und Text (aus der Bibliothek).
    pub wall_layers: Vec<(Rgba, String)>,
    /// Typ, den das Werkzeug zeichnet (K3); ersetzt die Schichtzeilen.
    pub tool_chip: Option<Chip>,
    /// Werkzeug „Erweiterungen“ läuft (E6): sein Paneel statt der Wand.
    pub ext_tool: Option<ExtPanel>,
    /// Knopf „Erweiterungen“ unter „Innenwand“: nur, wenn welche da sind,
    /// sonst wäre das Paneel ohne Nutzen eine Zeile höher.
    pub ext_knopf: bool,
    /// Werkzeug „Wandöffnung“ bzw. „Türöffnung“ läuft (Paket O3).
    pub opening: Option<OpeningKind>,
    /// Leiste „Gelände“ offen (G1): ihre Zeilen statt der Wandvorgaben.
    pub gelaende: Option<GelaendeLeiste>,
    /// Ein Obergeschoss ist aktiv: Außenwände entstehen aus dem EG, der Knopf
    /// „Gebäude“ ist gesperrt (E16).
    pub upper_active: bool,
    /// Das Fundament ist aktiv: dort gibt es noch nichts zu zeichnen, beide
    /// Wandknöpfe sind gesperrt (E18).
    pub foundation_active: bool,
    /// Unter dem Knopf „Projektdaten“: Bezeichnung (sonst Projektart) und
    /// „Projekt-Nr. …“, ohne Daten „noch leer“ (Paket PD-2).
    pub projekt_zeilen: Vec<String>,
    /// Nordpfeil wird aufgezogen: Kachel in `accent` (Sonnenstand S2).
    pub nord_aktiv: bool,
    /// Sonnenstand eingeschaltet (S4): Kachel hervorgehoben.
    pub sonne_an: bool,
    /// Gesetzte Nordrichtung (Grad im Uhrzeigersinn von +y): dreht das
    /// Symbol der Kachel; ohne nur umrissen.
    pub nord: Option<f64>,
    /// Dialog „Gebäude erstellen“ offen (modal, E16).
    pub dialog: bool,
    /// Zahlenfelder des Dialogs (Vorgaben des Gebäudes).
    dialog_fields: Vec<FieldRow>,
    /// Eigenschaften des gewählten Bauteils; ohne Auswahl kein Paneel.
    props: Option<Props>,
    /// „Mehr …“ ist aufgeklappt (gilt, solange das Paneel es anbietet).
    more_open: bool,
    /// Eingabe in einem Zahlenfeld.
    pub edit: Option<Edit>,
    /// Maße aus dem Farbschema (für Lage und Treffertest) und dessen Stand.
    size: Sizes,
    theme_rev: u64,
    /// Zuletzt gezeichnete Paneelbilder (Tools, Views, Props, Levels, Dialog)
    /// für das Neuzeichnen einzelner Knöpfe.
    images: [Option<PanelImage>; 5],
    /// Paneel „Geschosse“: Inhalt und laufendes Ziehen.
    levels: Levels,
    level_drag: Option<LevelDrag>,
    /// Oberfläche des Geschossfensters ([`fenster`]): nur das Diagramm,
    /// über die ganze Breite `fenster_w` (dip).
    fenster: bool,
    fenster_w: f32,
    /// Im Geschossmanager gezeigt: Fensterlinien, Innentürlinie.
    pub line_vis: [bool; 2],
    /// Geschossfenster offen: Knopf „Geschossmanager“ in `accent`.
    pub levels_open: bool,
    /// Geschossfenster gesperrt ([`fenster::SPERRE`]).
    pub gesperrt: bool,
    /// Fensterhöhe und Höhe der Titelleiste (Pixel), für die Höhe des
    /// Paneels „Geschosse“.
    win_h: f32,
    pub top: u32,
    /// Platz des Baumpanels zwischen „Ansichten“ und „Eigenschaften“
    /// (Pixel, samt Abstand darunter; Paket 4).
    pub tree_slot: f32,
    /// Höchste Höhe der „Eigenschaften“ (Pixel, 0 = ohne Grenze); ist der
    /// Inhalt höher, rollt er mit dem Mausrad.
    pub props_room: f32,
    props_scroll: f32,
}

/// Paneelbild ohne Knöpfe und mit Knöpfen, in Paneelkoordinaten.
struct PanelImage {
    scale: f32,
    base: Canvas,
    cur: Canvas,
}

/// Neu gezeichneter Ausschnitt eines Paneelbildes.
pub struct Patch {
    pub panel: Panel,
    /// Links oben im Paneelbild.
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    /// Vormultipliziertes RGBA8.
    pub px: Vec<u8>,
}

fn panel_index(p: Panel) -> usize {
    match p {
        Panel::Tools => 0,
        Panel::Views => 1,
        Panel::Props => 2,
        Panel::Levels => 3,
        Panel::Dialog => 4,
    }
}

/// Ergebnis eines Ereignisses.
#[derive(Default)]
pub struct UiOut {
    /// Knöpfe, deren Aussehen sich geändert hat (Hover, Drücken).
    pub changed: Vec<Id>,
    /// Angeklickter Knopf.
    pub clicked: Option<Id>,
    /// Die Maus steht über einem Paneel: das Ereignis gehört der Oberfläche.
    pub consumed: bool,
    /// Gültige Eingabe in einem Zahlenfeld (Wert in mm).
    pub submit: Option<(Field, f64)>,
    /// Die Höhe eines Paneels hat sich geändert (Hinweis unter einem Feld).
    pub relayout: bool,
    /// Ziehen einer Ebene im Paneel „Geschosse“.
    pub level: Option<LevelEvent>,
}

/// Unter dem Knopf „Projektdaten“, solange nichts gesetzt ist.
pub const PROJEKT_LEER: &str = "noch leer";

/// Die Zeilen unter dem Knopf „Projektdaten“: Bezeichnung, sonst Projektart,
/// und „Projekt-Nr. …“; fehlt alles, „noch leer“ (soll-projektdaten-knopf).
pub fn projekt_zeilen(p: &sk_model::Project) -> Vec<String> {
    let erste = |t: &str| t.lines().next().unwrap_or("").to_string();
    let mut v = Vec::new();
    if !p.site.is_empty() {
        v.push(erste(&p.site));
    } else if !p.kind.is_empty() {
        v.push(erste(&p.kind));
    }
    if !p.number.is_empty() {
        v.push(format!("Projekt-Nr. {}", erste(&p.number)));
    }
    if v.is_empty() {
        v.push(PROJEKT_LEER.into());
    }
    v
}

/// Zeilen des Werkzeug-Paneels (für Zeichnen und Treffertest gleich).
pub(crate) enum Row {
    Title(&'static str),
    Button(Id, &'static str),
    Label(&'static str),
    /// Farbfeld, Text, Baustoff samt Anfang seines Namens (Verweis).
    Layer(Rgba, String, Option<(sk_model::Guid, usize)>),
    /// Typ-Chip (K3).
    TypeChip(Id),
    /// „Kopplung“ links, rechts Kettensymbol (Knopf) und Partner.
    Link(Id, &'static str),
    /// Bezeichnung links, Wert rechtsbündig.
    Value(&'static str, String),
    /// Blasser Text, eingerückt wie der Text einer Schichtzeile.
    Detail(String),
    /// Blasser Text über die ganze Breite.
    Text(String),
    /// Blasser Text, gekürzt, ohne Abstand danach, nur die letzte Zeile
    /// mit Luft (Projektdaten unter dem Knopf). Rechts daneben steht die
    /// Kachel des Nordpfeils, siehe [`TILE`].
    Caption(String, bool),
    /// Verweis in Akzent („Mehr …“), klappt auf bzw. zu.
    More(Id),
    Segments([(Id, &'static str); 3]),
    Pair([(Id, &'static str); 2]),
    /// Bezeichnung links, Zahlenfeld rechts.
    Field(Field, &'static str),
    /// Grund einer ungültigen Eingabe, in `field_invalid`.
    Error(Meldung),
    Separator,
    Hint(&'static str),
    /// Schloss und „Gesperrt · Entsperren im Baum“ (Paket 4).
    Locked,
}

/// Beschriftung der Öffnungsknöpfe: „Wandöffnung“ passt fett nicht in einen
/// halben Knopf (Gate #86 mit Windows-Schriften), der Tooltip nennt das
/// Werkzeug mit vollem Namen.
fn view_rows(nord: Option<f64>, frei: Freischaltung) -> Vec<Row> {
    let v = |k: ViewKind| (Id::View(k), k.knopf(nord));
    vec![
        Row::Title("Ansichten"),
        Row::Button(Id::View(ViewKind::Persp), "3D"),
        Row::Button(Id::View(ViewKind::Plan), "Grundriss"),
        Row::Button(Id::View(ViewKind::Section), "Schnitt"),
        Row::Separator,
        Row::Pair([v(ViewKind::Front), v(ViewKind::Back)]),
        Row::Pair([v(ViewKind::Left), v(ViewKind::Right)]),
        Row::Separator,
        Row::Button(Id::Quantity, frei.knopf()),
    ]
}

/// `umbruch`: Text in Zeilen auf die Innenbreite des Paneels.
fn props_rows(
    p: &Props,
    edit: Option<&Edit>,
    more_open: bool,
    umbruch: impl Fn(&str) -> Vec<String>,
) -> Vec<Row> {
    let mut rows = vec![Row::Title("Eigenschaften")];
    rows.extend(p.values.iter().map(|(k, v)| match *k {
        "" => Row::Detail(v.clone()),
        k => Row::Value(k, v.clone()),
    }));
    if let Some(z) = &p.ext {
        ext_rows(&mut rows, p, z, edit);
        return rows;
    }
    if !p.fields.is_empty() || p.stack.is_some() {
        rows.push(Row::Separator);
    }
    if let Some(st) = &p.stack {
        rows.push(Row::Link(Id::PropsLink, st.partner));
    }
    let field_rows = |rows: &mut Vec<Row>, fields: &[FieldRow]| {
        for f in fields {
            rows.push(Row::Field(f.field, f.label));
            if let Some(e) = edit.filter(|e| e.field == f.field) {
                rows.extend(e.error.clone().map(Row::Error));
            }
        }
    };
    field_rows(&mut rows, &p.fields);
    if p.stack.is_some() {
        rows.push(Row::Button(Id::PropsFlush, "bündig setzen"));
    }
    if p.split {
        rows.push(Row::Button(Id::PropsSplit, "Wand teilen"));
    }
    if p.joint {
        rows.push(Row::Button(Id::PropsMerge, "an der Fuge vereinen"));
    }
    if p.ende {
        rows.push(Row::Pair([
            (Id::PropsEcke, "Ecke bilden"),
            (Id::PropsVerlaengern, "Verlängern"),
        ]));
    }
    if p.locked {
        rows.push(Row::Locked);
    }
    for sec in &p.sections {
        rows.extend([Row::Separator, Row::Label(sec.title)]);
        field_rows(&mut rows, &sec.fields);
        rows.extend(
            sec.buttons
                .iter()
                .map(|&(id, label)| Row::Button(id, label)),
        );
        // Hinweise brechen um wie die Begrenzung (Test zu #190)
        rows.extend(umbruch(sec.hint).into_iter().map(Row::Text));
    }
    if p.more {
        // Schichtliste erst auf Klick, an Ort und Stelle
        rows.push(Row::More(Id::PropsMore));
        if more_open {
            layer_rows(&mut rows, p);
        }
        notes_rows(&mut rows, p);
        return rows;
    }
    let label = if p.chip.is_some() {
        "Typ"
    } else if p.set_label.is_empty() {
        "Aufbau"
    } else {
        p.set_label
    };
    rows.extend([Row::Separator, Row::Label(label)]);
    if p.chip.is_some() {
        rows.push(Row::TypeChip(Id::PropsType));
    } else if p.set_umbruch {
        rows.extend(umbruch(&p.layer_set).into_iter().map(Row::Text));
    } else {
        rows.push(Row::Text(p.layer_set.clone()));
    }
    layer_rows(&mut rows, p);
    notes_rows(&mut rows, p);
    rows
}

/// Erweiterung (Vertrag §14): Trennlinie, Typ-Chip, je Gruppe ihre
/// Parameter; Wahl als Knöpfe (bis drei) oder Liste, `janein` als Schalter.
fn ext_rows(rows: &mut Vec<Row>, p: &Props, zeilen: &[ExtZeile], edit: Option<&Edit>) {
    if p.chip.is_some() || !zeilen.is_empty() {
        rows.push(Row::Separator);
    }
    if p.chip.is_some() {
        rows.push(Row::TypeChip(Id::PropsType));
    }
    for z in zeilen {
        match z {
            ExtZeile::Gruppe(g) => rows.push(Row::Label(g)),
            ExtZeile::Feld(f, _) => {
                rows.push(Row::Field(f.field, f.label));
                match edit.filter(|e| e.field == f.field) {
                    Some(e) => rows.extend(e.error.clone().map(Row::Error)),
                    None => rows.extend(f.ausserhalb().map(Row::Error)),
                }
            }
            ExtZeile::Wahl {
                i, name, knoepfe, ..
            } => {
                rows.push(Row::Label(name));
                let k = |n: usize| (Id::ExtWahl(*i, n as u8), knoepfe[n]);
                rows.push(match knoepfe.len() {
                    2 => Row::Pair([k(0), k(1)]),
                    _ => Row::Segments([k(0), k(1), k(2)]),
                });
            }
            ExtZeile::Liste { i, name, text, .. } => {
                rows.push(Row::Label(name));
                rows.push(Row::Button(Id::ExtListe(*i), text));
            }
            ExtZeile::Janein { i, name, .. } => rows.push(Row::Button(Id::ExtJa(*i), name)),
        }
    }
    if p.locked {
        rows.push(Row::Locked);
    }
    notes_rows(rows, p);
}

/// Satz unter einem gesperrten Bauteil (p4-5).
const LOCKED_LINE: &str = "Gesperrt · Entsperren im Baum";

/// Je Schicht Farbfeld mit Baustoff und Dicke, darunter die Menge.
fn layer_rows(rows: &mut Vec<Row>, p: &Props) {
    for (c, name, amount, link) in &p.layers {
        rows.push(Row::Layer(*c, name.clone(), *link));
        if !amount.is_empty() {
            rows.push(Row::Detail(amount.clone()));
        }
    }
}

fn notes_rows(rows: &mut Vec<Row>, p: &Props) {
    if !p.notes.is_empty() {
        rows.push(Row::Separator);
        rows.extend(p.notes.iter().map(|n| Row::Text(n.clone())));
    }
}

/// Höhe eines Paneels aus Zeilen (Pixel), mit Innenabstand oben und unten.
fn rows_px(rows: &[Row], pad: f32, scale: f32) -> f32 {
    let inner: f32 = rows
        .iter()
        .map(|r| {
            let (h, g) = row_height(r);
            h + g
        })
        .sum();
    ((inner + 2.0 * pad) * scale).round()
}

/// Höhe einer Zeile in dip und Abstand danach.
fn row_height(r: &Row) -> (f32, f32) {
    match r {
        Row::Title(_) => (22.0, 12.0),
        Row::Button(Id::Quantity, _) => (46.0, 8.0),
        Row::Button(..) | Row::Segments(_) | Row::Pair(_) => (34.0, 8.0),
        Row::Label(_) => (18.0, 6.0),
        Row::Layer(..) => (18.0, 4.0),
        Row::TypeChip(_) => (46.0, 8.0),
        Row::Link(..) => (26.0, 6.0),
        Row::Value(..) => (18.0, 4.0),
        Row::Field(..) => (26.0, 6.0),
        Row::Error(_) => (15.0, 6.0),
        Row::Detail(_) => (17.0, 6.0),
        Row::Text(_) => (17.0, 6.0),
        // Die zweite Zeile beginnt unter der Nordpfeil-Kachel (Knopf 34 + 8,
        // Zeile 17 + 5 = 64) und hat die volle Breite (Bedienbarkeit 30.1)
        Row::Caption(_, letzte) => (17.0, if *letzte { 8.0 } else { 5.0 }),
        Row::More(..) => (17.0, 6.0),
        Row::Separator => (1.0, 10.0),
        Row::Hint(_) => (17.0, 0.0),
        Row::Locked => (17.0, 6.0),
    }
}

impl Ui {
    pub fn new(scale: f32, theme: &Theme) -> Ui {
        Ui {
            size: theme.size,
            theme_rev: theme.rev,
            scale,
            dpi: scale,
            fonts: Fonts::system(),
            hover: None,
            pressed: None,
            view: ViewKind::Persp,
            quantity_open: false,
            frei: Freischaltung::ALLES,
            building: false,
            interior: false,
            ref_side: RefSide::Left,
            sprung: Sprung::R90,
            wall_layers: Vec::new(),
            tool_chip: None,
            ext_tool: None,
            ext_knopf: false,
            gelaende: None,
            opening: None,
            upper_active: false,
            foundation_active: false,
            projekt_zeilen: vec![PROJEKT_LEER.into()],
            nord_aktiv: false,
            sonne_an: false,
            nord: None,
            dialog: false,
            dialog_fields: Vec::new(),
            props: None,
            more_open: false,
            edit: None,
            images: [None, None, None, None, None],
            levels: Levels::default(),
            level_drag: None,
            fenster: false,
            fenster_w: 0.0,
            line_vis: [true; 2],
            levels_open: false,
            gesperrt: false,
            win_h: 1e6,
            top: 32,
            tree_slot: 0.0,
            props_room: 0.0,
            props_scroll: 0.0,
        }
    }

    /// Bildschirmskalierung (dpi / 96), ohne den Fensterfaktor der Paneele.
    pub fn dpi(&self) -> f32 {
        self.dpi
    }

    /// Passt die Paneelgröße an Fenster (Pixel) und Bildschirmskalierung an: in
    /// kleineren Fenstern schrumpfen Paneele und Knöpfe mit. `true`, wenn sich die
    /// Größe geändert hat.
    pub fn fit(&mut self, dpi: f32, win_w: u32, win_h: u32) -> bool {
        let (w, h) = (win_w as f32 / dpi, win_h as f32 / dpi);
        let f = (w / FULL_W).min(h / FULL_H).clamp(MIN_FIT, 1.0);
        // In Schritten von 1/40, damit nicht jedes Pixel beim Ziehen neu zeichnet
        let f = (f * 40.0).round() / 40.0;
        let scale = dpi * f;
        let changed = scale != self.scale || win_h as f32 != self.win_h;
        (self.dpi, self.scale, self.win_h) = (dpi, scale, win_h as f32);
        changed
    }

    fn rows(&self, p: Panel) -> Vec<Row> {
        let rows = match p {
            Panel::Tools => tool_rows(
                &self.projekt_zeilen,
                self.nord_aktiv,
                self.interior,
                self.tool_chip.is_some(),
                &self.wall_layers,
                self.upper_active,
                self.foundation_active,
                self.ext_tool.as_ref().map(|x| (x, self.edit.as_ref())),
                self.ext_knopf,
                self.opening.map(|k| (k, self.view == ViewKind::Plan)),
                self.gelaende.as_ref().map(|l| self.gelaende_rows(l)),
            ),
            Panel::Views => view_rows(self.nord, self.frei),
            Panel::Props => self.props.as_ref().map_or(Vec::new(), |p| {
                // In dip: die Zeilen sind in jedem Maßstab dieselben
                let breite = self.width(Panel::Props) - 2.0 * self.size.panel_pad;
                let umbruch = |t: &str| {
                    let f = self.fonts.regular.as_ref();
                    widgets::wrap(f, t, self.size.font_small, breite)
                };
                props_rows(p, self.edit.as_ref(), self.more_open, umbruch)
            }),
            Panel::Levels if self.levels_diagramm() => self.levels_kopf(),
            Panel::Levels => vec![Row::Button(Id::LevelsOpen, fenster::KNOPF)],
            Panel::Dialog => Vec::new(),
        };
        self.frei.zeilen(rows)
    }

    pub fn props(&self) -> Option<&Props> {
        self.props.as_ref()
    }

    /// „Mehr …“ auf- bzw. zuklappen.
    pub fn toggle_more(&mut self) {
        self.more_open = !self.more_open;
    }

    /// Voller Text einer gekürzten Schicht- oder Mengenzeile im Paneel
    /// „Eigenschaften“ unter der Maus (Hinweis).
    pub fn props_tip(&self, x: f64, y: f64, win_w: u32, top: u32) -> Option<String> {
        if self.dialog {
            return None;
        }
        self.props.as_ref()?;
        let r = self.rect(Panel::Props, win_w, top);
        let (lx, ly) = ((x - r.x as f64) as f32, (y - r.y as f64) as f32);
        let s = self.scale;
        let size = &self.size;
        let pad = size.panel_pad * s;
        let inner_w = (self.width(Panel::Props) - 2.0 * size.panel_pad) * s;
        if lx < pad || lx > pad + inner_w {
            return None;
        }
        let f = self.fonts.regular.as_ref();
        let mut ry = pad;
        for row in self.rows(Panel::Props) {
            let (h, g) = row_height(&row);
            let (h, g) = (h * s, g * s);
            if ly >= ry && ly < ry + h {
                let (t, tx, px) = match row {
                    Row::Layer(_, t, _) => (t, pad + 20.0 * s, size.font_small * s),
                    Row::Detail(t) => (t, pad + 20.0 * s, size.font_detail * s),
                    _ => return None,
                };
                let shown = widgets::ellipsize(f, &t, px, pad + inner_w - tx);
                return (shown != t).then_some(t);
            }
            ry += h + g;
        }
        None
    }

    /// Neuer Inhalt des Paneels „Eigenschaften“. Eine Eingabe in einem Feld,
    /// das es nicht mehr gibt, verfällt.
    pub fn set_props(&mut self, p: Option<Props>) {
        let keep = self.edit.as_ref().is_some_and(|e| {
            p.as_ref()
                .is_some_and(|p| p.fields.iter().any(|f| f.field == e.field))
        });
        if !keep {
            self.edit = None;
        }
        if !p.as_ref().is_some_and(|p| p.more) {
            self.more_open = false;
        }
        // Die Menge je Schicht und die Raumbuchmengen gehören zu Mengen
        let mengen = self.frei.frei(Modul::Mengen);
        self.props = p.map(|mut p| {
            if !mengen {
                for l in &mut p.layers {
                    l.2.clear();
                }
                let n = p.values.len();
                p.values.drain(p.mengen.start.min(n)..p.mengen.end.min(n));
                p.mengen = 0..0;
            }
            p
        });
    }

    fn field_row(&self, f: Field) -> Option<&FieldRow> {
        if let Field::Draft(_) = f {
            return self.dialog_fields.iter().find(|r| r.field == f);
        }
        if let Field::ExtTool(_) = f {
            return self.ext_tool.as_ref()?.felder.iter().find(|r| r.field == f);
        }
        if let Field::Gelaende(g) = f {
            if !g.im_paneel() {
                return self.gelaende.as_ref()?.felder.iter().find(|r| r.field == f);
            }
        }
        if let Field::ExtProp(_) = f {
            return self.ext_zeilen().find_map(|z| match z {
                ExtZeile::Feld(r, _) if r.field == f => Some(r),
                _ => None,
            });
        }
        if f.is_level() {
            return self.levels.fields.iter().find(|r| r.field == f);
        }
        let p = self.props.as_ref()?;
        p.fields
            .iter()
            .chain(p.sections.iter().flat_map(|s| &s.fields))
            .find(|r| r.field == f)
    }

    fn field_panel(f: Field) -> Panel {
        if let Field::Draft(_) = f {
            Panel::Dialog
        } else if matches!(f, Field::ExtTool(_))
            || matches!(f, Field::Gelaende(g) if !g.im_paneel())
        {
            Panel::Tools
        } else if f.is_level() {
            Panel::Levels
        } else {
            Panel::Props
        }
    }

    /// Feld in Paneelkoordinaten.
    fn field_rect(&self, f: Field) -> Option<Rect> {
        self.buttons(Ui::field_panel(f))
            .into_iter()
            .find(|b| b.0 == Id::Field(f))
            .map(|b| b.1)
    }

    /// Neue Werte der Dialogfelder; `true`, wenn sie sich geändert haben.
    pub fn set_dialog_fields(&mut self, rows: Vec<FieldRow>) -> bool {
        if rows == self.dialog_fields {
            return false;
        }
        self.dialog_fields = rows;
        true
    }

    /// Setzt die Eingabe in ein Feld (alles markiert), etwa beim Öffnen des
    /// Dialogs; eine laufende Eingabe wird vorher beendet.
    pub fn focus_field(&mut self, f: Field) -> UiOut {
        let mut out = UiOut::default();
        self.finish_edit(&mut out);
        self.begin_edit(f, &mut out);
        out
    }

    /// Eine Eingabe im Dialog ist ungültig: „Zeichnen beginnen“ gesperrt.
    fn dialog_invalid(&self) -> bool {
        self.edit
            .as_ref()
            .is_some_and(|e| matches!(e.field, Field::Draft(_)) && e.error.is_some())
    }

    /// Neuer Inhalt des Paneels „Geschosse“; `true`, wenn er sich geändert
    /// hat. Eine Eingabe in einer Zahl, die es nicht mehr gibt, verfällt.
    pub fn set_levels(&mut self, l: Levels) -> bool {
        if l == self.levels {
            return false;
        }
        if self
            .edit
            .as_ref()
            .is_some_and(|e| e.field.is_level() && !l.fields.iter().any(|f| f.field == e.field))
        {
            self.edit = None;
        }
        self.levels = l;
        true
    }

    /// Eine Ebene wird gerade gezogen.
    pub fn level_dragging(&self) -> Option<Grip> {
        self.level_drag.map(|d| d.grip)
    }

    /// Höhe (mm) der gerade gezogenen Ebene.
    pub fn level_drag_z(&self) -> Option<f64> {
        self.grip_z(self.level_drag?.grip)
    }

    /// Bricht das Ziehen einer Ebene ab (Esc); `true`, wenn eines lief.
    pub fn cancel_level_drag(&mut self) -> bool {
        self.level_drag.take().is_some()
    }

    /// Höhe (mm) einer Ebene.
    fn grip_z(&self, g: Grip) -> Option<f64> {
        let b = &self.levels.bands;
        match g {
            Grip::FoundationBottom => b.iter().find(|b| b.foundation).map(|b| b.bottom),
            Grip::Top(id) => b.iter().find(|b| b.id == id).map(|b| b.top),
            Grip::Terrain => self.levels.terrain,
            Grip::Sill(_) | Grip::Lintel(_) | Grip::Door(_) | Grip::PlanCut => self.sub_grip_z(g),
        }
    }

    /// Beginnt die Eingabe in einem Feld (alles markiert).
    fn begin_edit(&mut self, f: Field, out: &mut UiOut) {
        if let Some(row) = self.field_row(f) {
            self.edit = Some(Edit::new(row));
            out.changed.push(Id::Field(f));
            // Das Feld ist breiter als die Maßzahl
            out.relayout |= f.is_level();
        }
    }

    /// Beendet die Eingabe: gültig wird übernommen, sonst bleibt der alte Wert.
    fn finish_edit(&mut self, out: &mut UiOut) {
        if let Some(e) = self.edit.take() {
            if let Some(Ok(mm)) = self.field_row(e.field).map(|r| r.parse(&e.text)) {
                out.submit = Some((e.field, mm));
            }
            out.changed.push(Id::Field(e.field));
            out.relayout |= e.error.is_some() || e.field.is_level();
        }
    }

    /// Tastendruck. `None`, wenn kein Feld in Eingabe ist; dann gehört die
    /// Taste der übrigen App. Sonst nimmt das Feld jede Taste.
    pub fn key(&mut self, key: Key, down: bool, mods: Modifiers) -> Option<UiOut> {
        let mut out = UiOut {
            consumed: true,
            ..UiOut::default()
        };
        let e = self.edit.as_mut()?;
        if !down {
            return Some(out);
        }
        let field = e.field;
        let had_error = e.error.is_some();
        let end = e.text.len();
        let draft = match field {
            Field::Draft(d) => Some(d),
            _ => None,
        };
        let (before, orig) = (e.text.clone(), e.orig);
        match key {
            Key::Enter | Key::Tab => {
                let row = self.field_row(field).cloned();
                let e = self.edit.as_mut()?;
                match row.map(|r| r.parse(&e.text)) {
                    Some(Ok(mm)) => {
                        self.edit = None;
                        out.submit = Some((field, mm));
                        out.relayout = had_error || field.is_level();
                        // Im Dialog weiter ins nächste Feld, nach dem letzten
                        // ohne Eingabe (Enter beginnt dann)
                        if let Some(d) = draft {
                            if let Some(n) = d.next() {
                                self.begin_edit(Field::Draft(n), &mut out);
                                out.changed.push(Id::Field(Field::Draft(n)));
                            }
                            out.relayout = true;
                        }
                    }
                    Some(Err(why)) => {
                        out.relayout = e.error.as_ref() != Some(&why);
                        e.error = Some(why);
                    }
                    None => self.edit = None,
                }
            }
            Key::Escape => {
                self.edit = None;
                out.relayout = had_error || field.is_level();
                // Im Dialog galt schon jede gültige Taste: alten Wert zurück
                if draft.is_some() {
                    out.submit = Some((field, orig));
                    out.relayout = true;
                }
            }
            Key::Backspace | Key::Delete => {
                let (a, z) = e.selection();
                if a == z {
                    let r = if key == Key::Backspace {
                        a.saturating_sub(1)..a
                    } else {
                        a..(a + 1).min(end)
                    };
                    e.anchor = r.start;
                    e.caret = r.end;
                }
                e.replace_selection("");
            }
            Key::Left | Key::Right if !mods.shift && e.caret != e.anchor => {
                let (a, z) = e.selection();
                e.move_to(if key == Key::Left { a } else { z }, false);
            }
            Key::Left => e.move_to(e.caret.saturating_sub(1), mods.shift),
            Key::Right => e.move_to(e.caret + 1, mods.shift),
            Key::Home => e.move_to(0, mods.shift),
            Key::End => e.move_to(end, mods.shift),
            Key::Char('A') if mods.ctrl => {
                e.anchor = 0;
                e.caret = end;
            }
            Key::Char(c @ ('0'..='9' | ',' | '.' | '-'))
                if !mods.ctrl
                    && !mods.alt
                    && e.text.len() - (e.selection().1 - e.selection().0) < 12 =>
            {
                e.replace_selection(&c.to_string());
            }
            _ => {}
        }
        // Im Dialog gilt jede gültige Eingabe sofort (Paneel „Geschosse“)
        if let (Some(_), Some(row)) = (draft, self.field_row(field).cloned()) {
            if let Some(e) = self
                .edit
                .as_mut()
                .filter(|e| e.field == field && e.text != before)
            {
                let error = match row.parse(&e.text) {
                    Ok(mm) => {
                        out.submit = Some((field, mm));
                        None
                    }
                    Err(why) => Some(why),
                };
                out.relayout |= error != e.error;
                e.error = error;
            }
        }
        out.changed.push(Id::Field(field));
        Some(out)
    }

    /// Schreibmarke an der Stelle `x` (Paneelkoordinaten) im Feld.
    fn caret_at(&self, f: Field, x: f64) -> Option<usize> {
        let (e, r) = (self.edit.as_ref()?, self.field_rect(f)?);
        let font = self.fonts.regular.as_ref()?;
        let s = self.scale;
        let px = self.size.font_small * s;
        let unit = self.field_row(f).map_or(f.unit(), |r| r.unit());
        let unit_x = r.x + r.w - self.size.field_pad * s - font.width(unit, px);
        let num_x = unit_x - 4.0 * s - font.width(&e.text, px);
        let x = x as f32 - num_x;
        (0..=e.text.len()).min_by(|&a, &b| {
            let da = (font.width(&e.text[..a], px) - x).abs();
            let db = (font.width(&e.text[..b], px) - x).abs();
            da.total_cmp(&db)
        })
    }

    fn panel_height(&self, p: Panel) -> f32 {
        let h = self.natural_height(p);
        if p == Panel::Props && self.props_room > 0.0 {
            h.min(self.props_room.round())
        } else {
            h
        }
    }

    /// Höhe der „Eigenschaften“ mit ganzem Inhalt (Pixel).
    pub fn props_natural_height(&self) -> f32 {
        self.natural_height(Panel::Props)
    }

    /// Wie weit der Inhalt eines Paneels nach oben gerollt ist (Pixel; nur
    /// „Eigenschaften“).
    fn scroll(&self, p: Panel) -> f32 {
        if p != Panel::Props {
            return 0.0;
        }
        let max = (self.natural_height(p) - self.panel_height(p)).max(0.0);
        self.props_scroll.clamp(0.0, max)
    }

    /// Mausrad über den „Eigenschaften“ (`delta` in Rasten, + nach oben);
    /// `true`, wenn der Inhalt gerollt ist.
    pub fn scroll_props(&mut self, delta: f32) -> bool {
        let old = self.scroll(Panel::Props);
        let step = 3.0 * 20.0 * self.scale;
        self.props_scroll = old - delta * step;
        let new = self.scroll(Panel::Props);
        self.props_scroll = new;
        new != old
    }

    /// Neue Auswahl: Eigenschaften wieder von oben.
    pub fn reset_props_scroll(&mut self) {
        self.props_scroll = 0.0;
    }

    /// Rollt die „Eigenschaften“, bis der Knopf `id` ganz zu sehen ist, so
    /// weit es geht oben im Paneel (der Typ-Chip steht unter Mengen und
    /// Feldern und ist sonst meist hinausgerollt); `true`, wenn gerollt.
    pub fn reveal_props(&mut self, id: Id) -> bool {
        if !self.has_props() || self.buttons(Panel::Props).iter().any(|b| b.0 == id) {
            return false;
        }
        let old = self.scroll(Panel::Props);
        let Some((_, r, _)) = self
            .laid_out_buttons(Panel::Props)
            .into_iter()
            .find(|b| b.0 == id)
        else {
            return false;
        };
        self.props_scroll = r.y + old - self.size.panel_pad * self.scale;
        self.props_scroll = self.scroll(Panel::Props);
        self.props_scroll != old
    }

    /// Liegt `(x, y)` (Fenster) über den „Eigenschaften“?
    pub fn over_props(&self, x: f64, y: f64, win_w: u32, top: u32) -> bool {
        self.has_props() && self.rect(Panel::Props, win_w, top).contains(x, y)
    }

    fn natural_height(&self, p: Panel) -> f32 {
        match p {
            Panel::Levels if self.levels_diagramm() => return self.levels_layout().height,
            Panel::Dialog => return (self.dialog_h() * self.scale).round(),
            _ => {}
        }
        rows_px(&self.rows(p), self.size.panel_pad, self.scale)
    }

    /// Breite eines Paneels (dip): die rechte Spalte hat ihre eigene
    /// (Paket 4), links gilt `panel_width`.
    fn width(&self, p: Panel) -> f32 {
        match p {
            Panel::Views | Panel::Props => self.size.right_width,
            Panel::Levels if self.fenster => self.fenster_w,
            _ => self.size.panel_width,
        }
    }

    /// Lage eines Paneels im Fenster (ohne Schatten).
    pub fn rect(&self, p: Panel, win_w: u32, top: u32) -> Rect {
        if self.fenster {
            return self.fenster_rect(top);
        }
        let s = self.scale;
        let w = (self.width(p) * s).round();
        let m = (self.size.panel_margin * s).round();
        if p == Panel::Dialog {
            // Rechts neben „Werkzeuge“, oben bündig mit den Paneelen
            let dw = (self.size.dialog_w * s).round();
            return Rect::new(m + w + m, top as f32 + m, dw, self.panel_height(p));
        }
        let x = match p {
            Panel::Tools | Panel::Levels | Panel::Dialog => m,
            Panel::Views | Panel::Props => win_w as f32 - m - w,
        };
        let y = match p {
            // Unter „Ansichten“ und dem Baum bzw. unter „Werkzeuge“
            Panel::Props => top as f32 + 2.0 * m + self.panel_height(Panel::Views) + self.tree_slot,
            // Der Knopf „Geschossmanager“ unten links, auch neben dem Dialog
            // (Jörn 11. und 12.10.); „Werkzeuge“ wächst bis an ihn heran
            Panel::Levels => {
                let unter = top as f32 + 2.0 * m + self.panel_height(Panel::Tools);
                unter.max(self.win_h - m - self.panel_height(p))
            }
            _ => top as f32 + m,
        };
        Rect::new(x, y, w, self.panel_height(p))
    }

    /// Zeilen der gewählten Erweiterung (E7).
    fn ext_zeilen(&self) -> impl Iterator<Item = &ExtZeile> {
        self.props.iter().flat_map(|p| p.ext.iter().flatten())
    }

    /// `hilfe` des Parameters unter `id` (Tooltip, E7).
    pub fn ext_hilfe(&self, id: Id) -> Option<String> {
        self.ext_zeilen()
            .find_map(|z| z.hilfe(id))
            .map(str::to_string)
    }

    fn chip(&self, id: Id) -> Option<&Chip> {
        match id {
            Id::ToolType => self.tool_chip.as_ref(),
            Id::ExtTyp => self.ext_tool.as_ref()?.chip.as_ref(),
            Id::PropsType => self.props.as_ref()?.chip.as_ref(),
            _ => None,
        }
    }

    /// Lage eines Knopfs im Fenster (Pixel), etwa um die Typ-Liste unter
    /// dem Chip zu öffnen.
    pub fn button_rect(&self, id: Id, win_w: u32, top: u32) -> Option<Rect> {
        self.panels().into_iter().find_map(|p| {
            let r = self.rect(p, win_w, top);
            self.buttons(p)
                .into_iter()
                .find(|b| b.0 == id)
                .map(|(_, b, _)| Rect::new(r.x + b.x, r.y + b.y, b.w, b.h))
        })
    }

    /// Öffnet bzw. schließt die Typ-Liste am Chip; `true`, wenn sich das
    /// Aussehen ändert.
    pub fn set_chip_open(&mut self, id: Id, open: bool) -> bool {
        let chip = match id {
            Id::ToolType => self.tool_chip.as_mut(),
            Id::PropsType => self.props.as_mut().and_then(|p| p.chip.as_mut()),
            _ => None,
        };
        match chip {
            Some(c) if c.open != open => {
                c.open = open;
                true
            }
            _ => false,
        }
    }

    /// Ist das Paneel „Eigenschaften“ da (ein Bauteil gewählt)?
    pub fn has_props(&self) -> bool {
        self.props.is_some()
    }

    /// Sichtbare Paneele; bei offenem Dialog nimmt nur er die Maus.
    /// Lagen der gezeigten Paneele im Fenster (Pixel), etwa damit eine
    /// Angabe im Plan nicht darunter verschwindet.
    pub fn panel_rects(&self, win_w: u32, top: u32) -> Vec<Rect> {
        self.panels()
            .into_iter()
            .map(|p| self.rect(p, win_w, top))
            .collect()
    }

    fn panels(&self) -> Vec<Panel> {
        if self.fenster {
            return vec![Panel::Levels];
        }
        if self.dialog {
            return vec![Panel::Dialog];
        }
        let mut v = vec![Panel::Tools, Panel::Levels, Panel::Views];
        if self.props.is_some() {
            v.push(Panel::Props);
        }
        v
    }

    /// Knöpfe eines Paneels in Paneelkoordinaten.
    fn buttons(&self, p: Panel) -> Vec<(Id, Rect, &'static str)> {
        let mut out = self.laid_out_buttons(p);
        // Gerollt: nur ganz sichtbare Knöpfe zeichnen und treffen
        if matches!(p, Panel::Levels | Panel::Dialog) {
            return out;
        }
        if self.natural_height(p) > self.panel_height(p) {
            let pad = self.size.panel_pad * self.scale;
            let (a, b) = (pad * 0.5, self.panel_height(p) - pad * 0.5);
            out.retain(|(_, r, _)| r.y >= a && r.y + r.h <= b);
        }
        out
    }

    /// Alle Knöpfe eines Paneels am Rollstand, auch die hinausgerollten.
    fn laid_out_buttons(&self, p: Panel) -> Vec<(Id, Rect, &'static str)> {
        match p {
            Panel::Levels if self.levels_diagramm() => return self.level_buttons(),
            Panel::Dialog => return self.dialog_buttons(),
            _ => {}
        }
        let s = self.scale;
        let pad = self.size.panel_pad;
        let inner_w = (self.width(p) - 2.0 * pad) * s;
        let mut y = pad * s - self.scroll(p);
        let x = pad * s;
        let mut out = Vec::new();
        for r in self.rows(p) {
            let (h, g) = row_height(&r);
            let (h, g) = (h * s, g * s);
            match r {
                // Rechts neben Knopf und Beschriftung, oben bündig: keine
                // eigene Zeile, das kleine Fenster behält seine Grenze (A47)
                Row::Button(Id::Projektdaten, label) => {
                    let k = TILE * s;
                    let bw = inner_w - k - TILE_GAP * s;
                    out.push((Id::Projektdaten, Rect::new(x, y, bw, h), label));
                    out.push((Id::Nord, Rect::new(x + inner_w - k, y, k, k), ""));
                }
                Row::Button(id, label) => out.push((id, Rect::new(x, y, inner_w, h), label)),
                Row::Segments(items) => {
                    let gap = 4.0 * s;
                    let bw = (inner_w - 2.0 * gap) / 3.0;
                    for (i, (id, label)) in items.into_iter().enumerate() {
                        out.push((id, Rect::new(x + i as f32 * (bw + gap), y, bw, h), label));
                    }
                }
                Row::Pair(items) => {
                    let gap = 6.0 * s;
                    let bw = (inner_w - gap) / 2.0;
                    for (i, (id, label)) in items.into_iter().enumerate() {
                        out.push((id, Rect::new(x + i as f32 * (bw + gap), y, bw, h), label));
                    }
                }
                Row::Field(f, _) => {
                    let fw = match f {
                        Field::ExtTool(_) | Field::ExtProp(_) | Field::Gelaende(_) => EXT_FIELD_W,
                        _ => FIELD_W,
                    } * s;
                    out.push((Id::Field(f), Rect::new(x + inner_w - fw, y, fw, h), ""));
                }
                Row::TypeChip(id) => out.push((id, Rect::new(x, y, inner_w, h), "")),
                Row::Link(id, ..) => {
                    let cw = LINK_W * s;
                    out.push((id, Rect::new(x + inner_w - cw, y, cw, h), ""));
                }
                Row::More(id) => out.push((id, Rect::new(x, y, MORE_W * s, h), "")),
                Row::Layer(_, t, Some((g, at))) => {
                    // Nur der Name ist Verweis (Zeichnen wie unten)
                    let f = self.fonts.regular.as_ref();
                    let px = self.size.font_small * s;
                    let width = |x: &str| f.map_or(0.0, |f| f.width(x, px));
                    let tx = x + (LAYER_SWATCH + LAYER_GAP) * s;
                    let nx = tx + width(&t[..at]);
                    let nw = width(&t[at..]).min(x + inner_w - nx).max(0.0);
                    out.push((Id::PropsMaterial(g), Rect::new(nx, y, nw, h), ""));
                }
                _ => {}
            }
            y += h + g;
        }
        out
    }

    fn is_on(&self, id: Id) -> bool {
        match id {
            Id::Building => self.building && !self.interior,
            Id::Interior => self.building && self.interior,
            Id::Ref(r) => self.ref_side == r,
            Id::Sprung(s) => self.sprung == s,
            Id::View(v) => self.view == v,
            Id::Quantity => self.quantity_open,
            Id::LevelsOpen => self.levels_open,
            Id::Nord => self.nord_aktiv || self.sonne_an,
            Id::Ext => self.ext_tool.is_some(),
            Id::Opening(k) => self.opening == Some(k),
            Id::Gelaende(k) => self.gelaende_an(k),
            Id::ExtWahl(..) | Id::ExtJa(_) => self.ext_zeilen().any(|z| match (z, id) {
                (ExtZeile::Wahl { i, an, .. }, Id::ExtWahl(j, k)) => *i == j && *an == Some(k),
                (ExtZeile::Janein { i, an, .. }, Id::ExtJa(j)) => *i == j && *an,
                _ => false,
            }),
            // Standardknopf des Dialogs
            Id::DialogStart => true,
            Id::FlatRoof => self.levels.flat_roof == Some(true),
            Id::LineVis(k) => self.line_vis[k as usize],
            Id::Projektdaten
            | Id::Field(_)
            | Id::Grip(_)
            | Id::Storey(_)
            | Id::DialogClose
            | Id::DialogMinus
            | Id::DialogPlus
            | Id::DialogCancel
            | Id::ToolType
            | Id::ExtTyp
            | Id::PropsType
            | Id::PropsLink
            | Id::PropsFlush
            | Id::PropsSplit
            | Id::PropsMerge
            | Id::PropsEcke
            | Id::PropsVerlaengern
            | Id::PropsDrains
            | Id::PropsDrainAdd
            | Id::PropsDrainDelete
            | Id::PropsAction(_)
            | Id::PropsMore
            | Id::PropsMaterial(_)
            | Id::ExtListe(_)
            | Id::Raumbuch(_) => false,
        }
    }

    /// Gesperrte Knöpfe: „Gebäude“ im OG, beide Wandknöpfe im Fundament,
    /// der Zähler im Dialog (fest 2) und die Knöpfe gesperrter Module
    /// (Bedienbarkeit 41.5).
    pub(crate) fn is_disabled(&self, id: Id) -> bool {
        match id {
            id if self.frei.gesperrt(id) => true,
            Id::Building => self.upper_active || self.foundation_active,
            Id::Interior | Id::Opening(_) => self.foundation_active,
            Id::DialogMinus | Id::DialogPlus => true,
            Id::DialogStart => self.dialog_invalid(),
            Id::PropsFlush => {
                self.props_locked()
                    || self
                        .props
                        .as_ref()
                        .and_then(|p| p.stack.as_ref())
                        .is_none_or(|st| !st.offset)
            }
            // Gesperrt (Paket 4): Felder, Typ und Kette blass
            Id::Field(f) if Ui::field_panel(f) == Panel::Props => self.props_locked(),
            Id::PropsType
            | Id::PropsLink
            | Id::PropsDrains
            | Id::PropsDrainAdd
            | Id::PropsDrainDelete
            | Id::PropsAction(_)
            | Id::Raumbuch(_) => self.props_locked(),
            Id::PropsSplit | Id::PropsMerge | Id::PropsEcke | Id::PropsVerlaengern => {
                self.props_locked()
            }
            _ => false,
        }
    }

    /// Ist das gewählte Bauteil gesperrt?
    fn props_locked(&self) -> bool {
        self.props.as_ref().is_some_and(|p| p.locked)
    }

    /// Paneel und Knopf unter der Maus (Fensterkoordinaten).
    /// Liegt `(x, y)` (Fenster) über einem Paneel?
    pub fn over(&self, x: f64, y: f64, win_w: u32, top: u32) -> bool {
        self.hit(x, y, win_w, top).is_some()
    }

    fn hit(&self, x: f64, y: f64, win_w: u32, top: u32) -> Option<(Panel, Option<Id>)> {
        if self.dialog {
            // Modal: alles unterhalb der Titelleiste gehört dem Dialog
            let r = self.rect(Panel::Dialog, win_w, top);
            let (lx, ly) = (x - r.x as f64, y - r.y as f64);
            let id = self
                .buttons(Panel::Dialog)
                .into_iter()
                .find(|(_, b, _)| b.contains(lx, ly))
                .map(|b| b.0);
            return (y >= top as f64).then_some((Panel::Dialog, id));
        }
        for p in self.panels() {
            let r = self.rect(p, win_w, top);
            if r.contains(x, y) {
                let (lx, ly) = (x - r.x as f64, y - r.y as f64);
                let grip = || self.fenster.then(|| self.grip_at(lx, ly)).flatten();
                let id = self
                    .buttons(p)
                    .into_iter()
                    .find(|(_, b, _)| b.contains(lx, ly))
                    .map(|b| b.0)
                    .or_else(|| grip().map(Id::Grip));
                return Some((p, id));
            }
        }
        None
    }

    /// Verarbeitet Mausereignisse in Fensterkoordinaten.
    pub fn handle(&mut self, e: &Event, win_w: u32, top: u32) -> UiOut {
        let mut out = UiOut::default();
        if let Some(d) = self.level_drag {
            return self.handle_level_drag(e, d);
        }
        match *e {
            Event::MouseMove { x, y, .. } => {
                let hit = self.hit(x, y, win_w, top);
                let hover = hit.and_then(|h| h.1);
                if hover != self.hover {
                    out.changed.extend(self.hover.into_iter().chain(hover));
                }
                self.hover = hover;
                out.consumed = hit.is_some();
            }
            Event::MouseLeave => {
                out.changed.extend(self.hover.take());
            }
            Event::MouseDown { button, x, y, .. } => {
                let hit = self.hit(x, y, win_w, top);
                // Ungültige Eingabe sperrt „Zeichnen beginnen“ auch beim Klick,
                // der die Eingabe beendet
                let start_blocked = self.dialog_invalid();
                let field = match hit {
                    Some((_, Some(Id::Field(f))))
                        if button == MouseButton::Left && !self.is_disabled(Id::Field(f)) =>
                    {
                        Some(f)
                    }
                    _ => None,
                };
                // Klick neben das Feld in Eingabe beendet sie
                if self.edit.as_ref().is_some_and(|e| Some(e.field) != field) {
                    self.finish_edit(&mut out);
                }
                if let Some(f) = field {
                    out.consumed = true;
                    if self.edit.is_none() {
                        self.begin_edit(f, &mut out);
                    } else {
                        let r = self.rect(Ui::field_panel(f), win_w, top);
                        let lx = x - r.x as f64;
                        if let Some(i) = self.caret_at(f, lx) {
                            if let Some(e) = self.edit.as_mut() {
                                e.move_to(i, false);
                            }
                        }
                        out.changed.push(Id::Field(f));
                    }
                } else if let (Some((_, Some(Id::Grip(g)))), MouseButton::Left) = (hit, button) {
                    out.consumed = true;
                    if let Some(z0) = self.grip_z(g) {
                        self.level_drag = Some(LevelDrag {
                            grip: g,
                            y0: y,
                            z0,
                            layout: self.levels_layout(),
                        });
                        out.level = Some(LevelEvent::Begin(g));
                        out.changed.push(Id::Grip(g));
                    }
                } else if let Some((_, id)) = hit {
                    out.consumed = true;
                    let id = id.filter(|id| {
                        !self.is_disabled(*id) && !(start_blocked && *id == Id::DialogStart)
                    });
                    if button == MouseButton::Left {
                        self.pressed = id;
                        out.changed.extend(id);
                    }
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Some(p) = self.pressed.take() {
                    out.changed.push(p);
                    out.consumed = true;
                    if self.hit(x, y, win_w, top).and_then(|h| h.1) == Some(p) {
                        if let Id::LineVis(k) = p {
                            self.line_vis[k as usize] ^= true;
                        }
                        out.clicked = Some(p);
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// Lage (links oben) des Paneelbildes samt Schatten im Fenster, wie bei
    /// [`Ui::paint`].
    pub fn origin(&self, p: Panel, win_w: u32, top: u32) -> (i32, i32) {
        let r = self.rect(p, win_w, top);
        let m = (self.size.panel_shadow * self.scale).round();
        ((r.x - m) as i32, (r.y - m) as i32)
    }

    /// Zeichnet ein Paneel. Liefert das Bild und seine Lage (links oben) im Fenster.
    pub fn paint(&mut self, t: &Theme, p: Panel, win_w: u32, top: u32) -> (&Canvas, i32, i32) {
        self.use_theme(t);
        let (spare, mut cur) = match self.images[panel_index(p)].take() {
            Some(old) => (Some(old.base), old.cur),
            None => (None, Canvas::new(0, 0)),
        };
        let base = self.paint_base(t, p, win_w, top, spare);
        cur.copy_from(&base);
        for (id, b, label) in self.buttons(p) {
            self.paint_button(t, &mut cur, id, b, label);
        }
        let (x, y) = self.origin(p, win_w, top);
        let img = self.images[panel_index(p)].insert(PanelImage {
            scale: self.scale,
            base,
            cur,
        });
        (&img.cur, x, y)
    }

    /// Zeichnet einen Knopf auf dem zuletzt gezeichneten Paneelbild neu,
    /// pixelgleich zum vollen Neuzeichnen. `None`, wenn es kein passendes Bild
    /// gibt; dann muss das ganze Paneel neu gezeichnet werden.
    pub fn repaint_button(&mut self, t: &Theme, id: Id) -> Option<Patch> {
        if t.rev != self.theme_rev {
            return None;
        }
        if matches!(id, Id::Grip(_)) || matches!(id, Id::Field(f) if f.is_level()) {
            return self.repaint_levels(t);
        }
        // Der Hinweis zum Zähler steht im Grundbild des Dialogs
        if matches!(id, Id::DialogMinus | Id::DialogPlus) {
            return None;
        }
        let (panel, b, label) = self.panels().into_iter().find_map(|p| {
            self.buttons(p)
                .into_iter()
                .find(|b| b.0 == id)
                .map(|(_, b, label)| (p, b, label))
        })?;
        let slot = &mut self.images[panel_index(panel)];
        if slot.as_ref()?.scale != self.scale {
            return None;
        }
        let mut img = slot.take()?;
        // Ausschnitt samt geglätteter Kanten; Knöpfe liegen mindestens
        // 2,4 Pixel auseinander, der Rand von 1 Pixel trifft keinen Nachbarn.
        let m = (self.size.panel_shadow * self.scale).round();
        let x0 = ((b.x + m).floor() as usize).saturating_sub(1);
        let y0 = ((b.y + m).floor() as usize).saturating_sub(1);
        let x1 = (b.x + m + b.w).ceil() as usize + 1;
        let y1 = (b.y + m + b.h).ceil() as usize + 1;
        img.cur.copy_region(&img.base, x0, y0, x1 - x0, y1 - y0);
        self.paint_button(t, &mut img.cur, id, b, label);
        let (x, y, w, h, px) = img.cur.region_premul_rgba8(x0, y0, x1 - x0, y1 - y0);
        self.images[panel_index(panel)] = Some(img);
        Some(Patch {
            panel,
            x,
            y,
            w,
            h,
            px,
        })
    }

    /// Übernimmt Maße und Stand des Farbschemas; ein neuer Stand verwirft die
    /// Paneelbilder.
    /// Vergisst den Stand des Farbschemas: das nächste Zeichnen baut alle
    /// Paneelbilder neu (auch wenn ein Abbrechen den alten Stand herstellt).
    pub fn forget_theme(&mut self) {
        self.theme_rev = u64::MAX;
    }

    pub fn use_theme(&mut self, t: &Theme) {
        if t.rev != self.theme_rev {
            self.theme_rev = t.rev;
            self.size = t.size;
            self.images = [None, None, None, None, None];
        }
    }

    fn paint_button(&self, t: &Theme, c: &mut Canvas, id: Id, b: Rect, label: &str) {
        let s = self.scale;
        let m = (self.size.panel_shadow * s).round();
        // Griffe und Namen stehen im Grundbild des Paneels „Geschosse“
        if let Id::Grip(_) | Id::Storey(_) = id {
            return;
        }
        if let Id::Field(f) = id {
            let b = Rect::new(b.x + m, b.y + m, b.w, b.h);
            let edit = self.edit.as_ref().filter(|e| e.field == f);
            if f.is_level() && edit.is_none() {
                self.paint_dim_text(t, c, f, b);
                return;
            }
            let row = self.field_row(f);
            let value = row.map_or(String::new(), |r| r.display(r.value));
            let st = FieldState {
                text: edit.map_or(&value, |e| &e.text),
                unit: row.map_or(f.unit(), |r| r.unit()),
                hover: self.hover == Some(id),
                focus: edit.is_some(),
                invalid: edit.map_or(row.is_some_and(|r| r.ausserhalb().is_some()), |e| {
                    e.error.is_some()
                }),
                caret: edit.map(|e| e.caret),
                select: edit.map(|e| e.selection()),
                disabled: self.is_disabled(id),
            };
            widgets::field(c, &self.fonts, b, &st, s, t);
            return;
        }
        let disabled = self.is_disabled(id);
        let st = ButtonState {
            hover: self.hover == Some(id) && !disabled,
            pressed: self.pressed == Some(id) && self.hover == Some(id),
            active: self.is_on(id),
            disabled,
        };
        let b = Rect::new(b.x + m, b.y + m, b.w, b.h);
        if id == Id::DialogClose {
            self.paint_close(t, c, b, st.hover);
            return;
        }
        if let Some(word) = check_word(id) {
            let k = 14.0 * s;
            let kr = Rect::new(b.x, b.y + ((b.h - k) * 0.5).round(), k, k);
            widgets::checkbox(c, kr, st.active, st.hover, s, t);
            let px = self.size.font_small * s;
            let col = if st.hover { t.ui.text } else { t.ui.text_dim };
            let base = kr.y + k - 3.0 * s;
            let font = self.fonts.regular.as_ref();
            widgets::text(c, font, word, px, kr.x + k + 6.0 * s, base, col);
            return;
        }
        if let Some(chip) = self.chip(id) {
            paint_chip(c, &self.fonts, b, chip, st, s, t);
            return;
        }
        if let Id::PropsMaterial(g) = id {
            let name = self.props.as_ref().and_then(|p| {
                p.layers
                    .iter()
                    .find_map(|l| l.3.filter(|x| x.0 == g).map(|(_, at)| &l.1[at..]))
            });
            let regular = self.fonts.regular.as_ref();
            let px = self.size.font_small * s;
            let col = if st.hover {
                t.ui.accent_hover
            } else {
                t.ui.accent
            };
            let name = widgets::ellipsize(regular, name.unwrap_or(""), px, b.w);
            let base = b.y + 13.5 * s;
            widgets::text(c, regular, &name, px, b.x, base, col);
            if st.hover {
                let w = regular.map_or(b.w, |f| f.width(&name, px));
                c.fill_rect(b.x, (base + 2.0 * s).round(), w, s.round().max(1.0), col);
            }
            return;
        }
        if id == Id::PropsMore {
            let open = self.more_open;
            let px = self.size.font_small * s;
            let col = if st.hover { t.ui.text } else { t.ui.accent };
            let text = if open { "Weniger" } else { "Mehr …" };
            widgets::text(
                c,
                self.fonts.regular.as_ref(),
                text,
                px,
                b.x,
                b.y + 13.0 * s,
                col,
            );
            return;
        }
        if id == Id::PropsLink {
            let linked = self
                .props
                .as_ref()
                .and_then(|p| p.stack.as_ref())
                .is_some_and(|st| st.linked);
            let size = (crate::link_view::CHIP as f32 * s).round();
            let img = crate::link_view::paint(t, linked, st.hover, size, s);
            let y = b.y + ((b.h - size) * 0.5).round();
            if st.disabled {
                // Gesperrt: blass
                c.blit_scaled(&img, b.x.round(), y.trunc(), 1.0, 0.4);
            } else {
                c.blit(&img, b.x.round() as i32, y as i32);
            }
            return;
        }
        if id == Id::Nord {
            widgets::button(c, &self.fonts, b, "", st, s, t);
            // Ohne Nordrichtung nur umrissen und gedämpft
            let ink = if st.active {
                t.ui.on_accent
            } else if st.disabled || self.nord.is_none() {
                t.ui.text_dim
            } else {
                t.ui.text
            };
            nord_symbol(c, b, self.nord, ink, s);
            return;
        }
        if id == Id::Quantity {
            // Zweizeilig ab KA-4 (Einstellungen §3 KA-4 Punkt 8): „Mengen“ /
            // „Kosten · AVA“, 12 fett, Zeilenabstand 16
            widgets::button(c, &self.fonts, b, "", st, s, t);
            // Nur Mengen frei: einzeilig in der Mitte, in der Schrift der
            // übrigen Knöpfe (Bedienbarkeit, Ist-Bild zu #202)
            let (oben, unten) = self.frei.knopf_zeilen();
            if let Some(f) = self.fonts.bold.as_ref().or(self.fonts.regular.as_ref()) {
                let px = if unten.is_empty() { t.size.font } else { 12.0 } * s;
                let col = if st.disabled {
                    t.ui.text_disabled
                } else if st.active {
                    t.ui.on_accent
                } else {
                    t.ui.text
                };
                let mitte = b.y + b.h * 0.5 + f.cap_height(px) * 0.5;
                let zeilen = if unten.is_empty() {
                    vec![(oben, mitte)]
                } else {
                    vec![(oben, mitte - 8.0 * s), (unten, mitte + 8.0 * s)]
                };
                for (text, y) in zeilen {
                    let x = b.x + (b.w - f.width(text, px)) * 0.5;
                    f.draw(c, text, px, x.round(), y.round(), col);
                }
            }
            return;
        }
        widgets::button(c, &self.fonts, b, label, st, s, t);
        if id == Id::Gelaende(GelaendeKnopf::Schloss) {
            self.schloss_symbol(t, c, b, st);
        }
    }

    /// Paneel ohne Knöpfe.
    fn paint_base(
        &self,
        t: &Theme,
        p: Panel,
        win_w: u32,
        top: u32,
        spare: Option<Canvas>,
    ) -> Canvas {
        let (col, size) = (&t.ui, &t.size);
        let s = self.scale;
        let r = self.rect(p, win_w, top);
        let m = (size.panel_shadow * s).round();
        // Leinwand des letzten Bildes wiederverwenden: keine frischen Seiten
        // vom System (Review 1x)
        let mut c = spare.unwrap_or_else(|| Canvas::new(0, 0));
        c.reuse((r.w + 2.0 * m) as usize, (r.h + 2.0 * m) as usize);
        if self.fenster {
            // Im eigenen Fenster ohne Rahmen und Schatten
            c.clear(col.bg);
        } else {
            widgets::panel(&mut c, Rect::new(m, m, r.w, r.h), s, t);
        }
        if p == Panel::Levels && self.levels_diagramm() {
            self.paint_levels(t, &mut c, r.h);
        }
        if p == Panel::Dialog {
            self.paint_dialog(t, &mut c);
        }

        let (regular, bold) = (self.fonts.regular.as_ref(), self.fonts.bold.as_ref());
        let x = m + size.panel_pad * s;
        let inner_w = (self.width(p) - 2.0 * size.panel_pad) * s;
        let scroll = self.scroll(p);
        let rolls = self.natural_height(p) > r.h;
        let frame = rolls.then(|| c.clone());
        let mut y = m + size.panel_pad * s - scroll;
        // Unterkante der Nordpfeil-Kachel neben den Projektdaten
        let mut kachel_unten = f32::INFINITY;
        for row in self.rows(p) {
            let (h, g) = row_height(&row);
            let (h, g) = (h * s, g * s);
            if matches!(row, Row::Button(Id::Projektdaten, _)) {
                kachel_unten = y + TILE * s;
            }
            match row {
                Row::Title(t) => widgets::text(
                    &mut c,
                    bold.or(regular),
                    t,
                    size.font_title * s,
                    x,
                    y + 16.0 * s,
                    col.text,
                ),
                Row::Label(t) => {
                    widgets::text(&mut c, regular, t, size.font * s, x, y + 14.0 * s, col.text)
                }
                Row::Layer(color, t, link) => {
                    let sw = LAYER_SWATCH * s;
                    c.fill_rect(x, y + 3.0 * s, sw, sw, color);
                    let tx = x + sw + LAYER_GAP * s;
                    // Der Name als Verweis kommt mit den Knöpfen
                    let t = link.map_or(t.as_str(), |(_, at)| &t[..at]);
                    // Zu lang: gekürzt, der volle Name als Hinweis
                    let px = size.font_small * s;
                    let t = widgets::ellipsize(regular, t, px, x + inner_w - tx);
                    widgets::text(&mut c, regular, &t, px, tx, y + 13.5 * s, col.text_dim);
                }
                Row::Field(_, k) => {
                    let px = size.font_small * s;
                    widgets::text(&mut c, regular, k, px, x, y + 17.5 * s, col.text_dim);
                }
                Row::Link(_, partner) => {
                    let px = size.font_small * s;
                    let base = y + 17.5 * s;
                    widgets::text(&mut c, regular, "Kopplung", px, x, base, col.text_dim);
                    let tx = x + inner_w - (LINK_W - LINK_CHIP_GAP) * s;
                    widgets::text(&mut c, regular, partner, px, tx, base, col.text);
                }
                Row::Error(t) => widgets::text(
                    &mut c,
                    regular,
                    &t,
                    size.font_detail * s,
                    x,
                    y + 12.0 * s,
                    col.field_invalid,
                ),
                Row::Value(k, v) => {
                    let px = size.font_small * s;
                    let base = y + 13.5 * s;
                    widgets::text(&mut c, regular, k, px, x, base, col.text_dim);
                    let vw = regular.map_or(0.0, |f| f.width(&v, px));
                    widgets::text(&mut c, regular, &v, px, x + inner_w - vw, base, col.text);
                }
                Row::Detail(t) => {
                    let tx = x + 20.0 * s;
                    let px = size.font_detail * s;
                    let t = widgets::ellipsize(regular, &t, px, x + inner_w - tx);
                    widgets::text(&mut c, regular, &t, px, tx, y + 13.0 * s, col.text_dim)
                }
                Row::Text(t) => widgets::text(
                    &mut c,
                    regular,
                    &t,
                    size.font_small * s,
                    x,
                    y + 13.0 * s,
                    col.text_dim,
                ),
                Row::Caption(t, ..) => {
                    let px = size.font_small * s;
                    // Rechts steht die Kachel des Nordpfeils; unter ihr die
                    // volle Breite (Bedienbarkeit 30.1)
                    let frei = if y + 0.5 * s >= kachel_unten {
                        inner_w
                    } else {
                        inner_w - (TILE + TILE_GAP) * s
                    };
                    let t = widgets::ellipsize(regular, &t, px, frei);
                    widgets::text(&mut c, regular, &t, px, x, y + 13.0 * s, col.text_dim)
                }
                Row::Separator => widgets::separator(&mut c, x, y, inner_w, s, t),
                Row::Locked => {
                    let px = size.font_detail * s;
                    let ic = size.tree_icon * s;
                    widgets::lock_icon(
                        &mut c,
                        x + ic * 0.5,
                        y + 8.5 * s,
                        widgets::Fill::Full,
                        col.text_dim,
                        s,
                    );
                    widgets::text(
                        &mut c,
                        regular,
                        LOCKED_LINE,
                        px,
                        x + ic + size.tree_icon_gap * s,
                        y + 13.0 * s,
                        col.text_dim,
                    )
                }
                Row::Hint(t) => {
                    // Feste Sätze (Vertrag §14) werden nie gekürzt; was
                    // nicht passt, steht etwas kleiner
                    let px = size.font_small * s;
                    let tw = regular.map_or(0.0, |f| f.width(t, px));
                    let px = if tw > inner_w { px * inner_w / tw } else { px };
                    widgets::text(&mut c, regular, t, px, x, y + 13.0 * s, col.text_dim)
                }
                _ => {}
            }
            y += h + g;
        }
        // Gerollt: Rand oben und unten wieder frei, rechts ein schmaler
        // Balken für Lage und Anteil
        if let Some(frame) = frame {
            let edge = size.panel_pad * s * 0.5;
            let (a, b) = (m + edge, m + r.h - edge);
            c.copy_rows(&frame, 0, a.floor() as usize);
            c.copy_rows(&frame, b.ceil() as usize, c.height);
            let natural = self.natural_height(p);
            let track = b - a;
            let thumb = (track * r.h / natural).max(16.0 * s);
            let max = (natural - r.h).max(1.0);
            let ty = a + (track - thumb) * scroll / max;
            let bw = 3.0 * s;
            c.fill_rect(m + r.w - bw - 3.0 * s, ty, bw, thumb, col.text_disabled);
        }
        c
    }
}

/// Nordpfeil in der Kachel `b`, dasselbe Zeichen wie in der Szene ohne „N“
/// (Jörns Skizze, §8 08:42): schlanker Pfeil mit eingezogenem Fuß, links
/// umrissen, rechts gefüllt, um die Nordrichtung im Uhrzeigersinn gedreht
/// (0: nach oben, wie +y im Grundriss); ohne Nordrichtung nach oben,
/// gedämpft über `ink`.
fn nord_symbol(c: &mut Canvas, b: Rect, nord: Option<f64>, ink: Rgba, s: f32) {
    let (sin, cos) = (nord.unwrap_or(0.0) as f32).to_radians().sin_cos();
    // Richtung im Bild (y nach unten) und quer dazu, Mitte der Kachel in
    // halber Länge
    let (dx, dy) = (sin, -cos);
    let (cx, cy) = (b.x + b.w * 0.5, b.y + b.h * 0.5);
    // Drei Viertel der Kachel lang (bei 24 dip 18 dip); etwas breiter als
    // in der Szene, damit beide Hälften auch klein zu sehen sind
    let u = b.w / 24.0;
    let at = |l: f32, q: f32| (cx + (dx * l - dy * q) * u, cy + (dy * l + dx * q) * u);
    let (spitze, links, kerbe, rechts) =
        (at(9.0, 0.0), at(-9.0, -4.2), at(-6.3, 0.0), at(-9.0, 4.2));
    let mut p = Path::new();
    p.move_to(spitze.0, spitze.1)
        .line_to(kerbe.0, kerbe.1)
        .line_to(rechts.0, rechts.1)
        .close();
    c.fill(&p, ink);
    // Umriss mit der Kachel etwas kräftiger
    let k = (1.1 * s * (u / s).sqrt()).max(1.0);
    // Jeder Strich für sich: überlappende Teilpfade mit verschiedener
    // Laufrichtung höben sich sonst auf
    for (a, z) in [
        (spitze, links),
        (links, kerbe),
        (kerbe, rechts),
        (rechts, spitze),
        (spitze, kerbe),
    ] {
        let mut p = Path::new();
        p.segment(a, z, k);
        c.fill(&p, ink);
        let h = k * 0.5;
        let mut p = Path::new();
        p.rounded_rect(z.0 - h, z.1 - h, k, k, h);
        c.fill(&p, ink);
    }
}

/// Dialog: linke Kante des Zählers und obere Kante seiner Zeile (dip).
const DIALOG_COUNTER_X: f32 = 104.0;
const DIALOG_ROW_Y: f32 = 46.0;
/// Breite der Zahlenfelder im Dialog (dip).
const DIALOG_FIELD_W: f32 = 84.0;

/// Linie bei ±0,00 (fest): an ihr hängt das Diagramm.
fn anchor_index(lines: &[LevelLine]) -> usize {
    lines.iter().position(|l| l.field.is_none()).unwrap_or(0)
}

/// Typ-Chip: Feld mit Kachel, fettem Namen, blassem „Kürzel · Dicke“ und
/// Pfeil; offen mit Rand in Akzent.
fn paint_chip(
    c: &mut Canvas,
    fonts: &Fonts,
    r: Rect,
    chip: &Chip,
    st: ButtonState,
    s: f32,
    t: &Theme,
) {
    let u = &t.ui;
    let rad = 6.0 * s;
    let b = s.round().max(1.0);
    let border = if chip.open {
        u.accent
    } else if st.hover {
        u.field_hover
    } else {
        u.field_border
    };
    let fill = if st.pressed { u.pressed } else { u.field };
    let mut p = Path::new();
    p.rounded_rect(r.x, r.y, r.w, r.h, rad);
    c.fill(&p, border);
    let mut p = Path::new();
    p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, rad - b);
    c.fill(&p, fill);
    let (tw, th) = (
        (t.size.catalog_thumb_w * 0.8 * s).round(),
        (t.size.catalog_thumb_h * s).round(),
    );
    let tx = (r.x + 6.0 * s).round();
    let ty = (r.y + (r.h - th) * 0.5).round();
    let x = match &chip.look {
        Some(look) => {
            crate::type_look::paint_thumb(c, Rect::new(tx, ty, tw, th), look, s);
            tx + tw + 8.0 * s
        }
        None => tx + 4.0 * s,
    };
    let caret_w = 14.0 * s;
    let max_w = r.x + r.w - caret_w - 4.0 * s - x;
    let bold = fonts.bold.as_ref().or(fonts.regular.as_ref());
    let px = t.size.font * s;
    let name = widgets::ellipsize(bold, &chip.name, px, max_w);
    // Gesperrt: Name und Angabe blass
    let (main, dim) = if st.disabled {
        (u.text_disabled, u.text_disabled)
    } else {
        (u.text, u.text_dim)
    };
    widgets::text(c, bold, &name, px, x, (r.y + 20.0 * s).round(), main);
    let pd = t.size.font_detail * s;
    let detail = widgets::ellipsize(fonts.regular.as_ref(), &chip.detail, pd, max_w);
    widgets::text(
        c,
        fonts.regular.as_ref(),
        &detail,
        pd,
        x,
        (r.y + 36.0 * s).round(),
        dim,
    );
    // Pfeil: offen nach oben
    let (cx, cy) = (r.x + r.w - 12.0 * s, r.y + r.h * 0.5);
    let a = 3.5 * s;
    let mut p = Path::new();
    if chip.open {
        p.move_to(cx - a, cy + a * 0.5)
            .line_to(cx + a, cy + a * 0.5)
            .line_to(cx, cy - a * 0.6)
            .close();
    } else {
        p.move_to(cx - a, cy - a * 0.5)
            .line_to(cx + a, cy - a * 0.5)
            .line_to(cx, cy + a * 0.6)
            .close();
    }
    c.fill(&p, dim);
    if chip.marked {
        let d = 3.0 * s;
        let mut p = Path::new();
        p.rounded_rect(r.x + r.w - 7.0 * s - d, r.y + 5.0 * s, 2.0 * d, 2.0 * d, d);
        c.fill(&p, u.accent);
    }
}

#[cfg(test)]
mod tests;

/// E14: Paneel „Geschosse“.
#[cfg(test)]
mod levels_tests;

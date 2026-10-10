//! Auswahl eines Bauteils per Klick: Treffer, Hervorhebung und Inhalt des
//! Paneels „Eigenschaften“. Die Auswahl gehört der App, nicht dem Modell, und
//! steht nicht im Rückgängig-Verlauf.

use crate::camera::Camera;
use crate::scene::Scene;
use crate::ui::{Chip, Field, FieldRow, Props, ViewKind};
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, ElementKind, FootingShape, FoundationError, MaterialId, Model};
use sk_paint::Rgba;
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Bis zu so vielen Pixeln Bewegung zwischen Drücken und Loslassen gilt als Klick.
const CLICK_PX: f64 = 4.0;

#[derive(Default)]
pub struct Selection {
    pub id: Option<ElementId>,
    /// Wo die linke Taste in der Ansicht gedrückt wurde.
    press: Option<(f64, f64)>,
}

impl Selection {
    pub fn press(&mut self, x: f64, y: f64) {
        self.press = Some((x, y));
    }

    /// Loslassen: `true`, wenn es ein Klick war (kaum bewegt seit dem Drücken).
    pub fn release(&mut self, x: f64, y: f64, scale: f64) -> bool {
        self.press
            .take()
            .is_some_and(|(px, py)| (x - px).hypot(y - py) <= CLICK_PX * scale)
    }

    /// Wählt `id` (oder nichts). `true`, wenn sich die Auswahl geändert hat.
    pub fn set(&mut self, id: Option<ElementId>) -> bool {
        let changed = id != self.id;
        self.id = id;
        changed
    }

    /// Hebt die Auswahl auf, wenn es das Bauteil nicht mehr gibt (etwa nach
    /// Rückgängig). `true`, wenn sie aufgehoben wurde.
    #[cfg(test)]
    pub fn validate(&mut self, scene: &Scene) -> bool {
        match self.id {
            Some(id) if scene.model().element(id).is_none() => {
                self.id = None;
                true
            }
            _ => false,
        }
    }
}

/// Was ein Klick beim Loslassen mit der gemeinsamen Auswahl macht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickChange {
    /// Kein Klick (gezogen): Auswahl bleibt.
    Keep,
    /// Genau dieses Bauteil wählen (`None`: nichts).
    Replace(Option<ElementId>),
    /// Strg+Klick: zur Auswahl dazunehmen bzw. herausnehmen.
    Add(ElementId),
    Remove(ElementId),
}

/// Auswahl beim Loslassen, ohne Fenster: `band` ist das Bauteil, dessen
/// Band angeklickt wurde, `hit` der Treffer eines Klicks in die Ansicht
/// (`None`: gezogen, kein Klick; `Some(None)`: ins Leere). Das Band geht
/// vor. Strg nimmt dazu oder heraus und behält die Auswahl beim Klick ins
/// Leere, außer das Werkzeug zeichnet gerade.
pub fn release_pick(
    band: Option<ElementId>,
    hit: Option<Option<ElementId>>,
    ctrl: bool,
    tool: bool,
    selected: &[ElementId],
) -> PickChange {
    let target = match band {
        Some(b) => Some(b),
        None => match hit {
            None => return PickChange::Keep,
            Some(h) => h,
        },
    };
    match target {
        Some(id) if ctrl && !tool => {
            if selected.contains(&id) {
                PickChange::Remove(id)
            } else {
                PickChange::Add(id)
            }
        }
        // Strg+Klick daneben: wer sammelt, verliert nichts
        None if ctrl && !tool => PickChange::Keep,
        t => PickChange::Replace(t),
    }
}

/// Bauteil unter dem Bildpunkt `(x, y)` der Ansicht.
#[allow(clippy::too_many_arguments)]
pub fn pick_at(
    scene: &mut Scene,
    cam: &Camera,
    view: ViewKind,
    section: Option<(Vec3, Vec3)>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Option<ElementId> {
    let (o, d) = cam.ray(x, y, w, h);
    scene.pick(view, section, o, d)
}

/// Zahl mit `dec` Nachkommastellen im deutschen Format, z. B. 1.608,25.
/// Länge in m aus mm: zwei Nachkommastellen, eine dritte nur, wenn es
/// Millimeter gibt (2,855 m, 2,98 m).
pub fn de_m(mm: f64) -> String {
    let cm = mm / 10.0;
    let dec = if (cm - cm.round()).abs() < 1e-6 { 2 } else { 3 };
    de(mm / 1e3, dec)
}

pub fn de(v: f64, dec: usize) -> String {
    let s = format!("{:.*}", dec, v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let mut out = String::new();
    if v < 0.0 && s.bytes().any(|b| (b'1'..=b'9').contains(&b)) {
        out.push('-');
    }
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    if !frac.is_empty() {
        out.push(',');
        out.push_str(frac);
    }
    out
}

/// Zentimeter ohne überflüssige Nachkommastelle: „14“, „17,5“.
pub fn cm(mm: f64) -> String {
    let c = mm / 10.0;
    if (c - c.round()).abs() < 1e-9 {
        de(c, 0)
    } else {
        de(c, 1)
    }
}

/// Farbfeld, Name und Menge einer massiven Schicht aus einem Baustoff.
fn solid_layer(
    m: &Model,
    mat: MaterialId,
    t: f64,
    volume: Option<f64>,
) -> Option<crate::ui::LayerRow> {
    let x = m.material(mat)?;
    let rgb = m.attr().surface(x.surface)?.cut_color;
    let amount = volume.map_or(String::new(), |v| {
        format!("{} m³ · {} kg", de(v / 1e9, 3), de(v / 1e9 * x.density, 0))
    });
    let thick = format!("{} cm ", cm(t));
    Some((
        Rgba::from_rgb8(rgb),
        format!("{thick}{}", x.name),
        amount,
        Some((x.guid, thick.len())),
    ))
}

/// Feld „Sockelrücksprung“, wenn das Bauteil zu einem Zug mit Sohlplatte
/// gehört: 0 (bündig) oder 2 bis 50 cm.
fn recess_field(m: &Model, id: ElementId) -> Option<FieldRow> {
    let (slab, _) = m.foundation_of(m.run_of(id)?)?;
    let ElementKind::GroundSlab(s) = m.element(slab)?.kind else {
        return None;
    };
    Some(FieldRow {
        field: Field::Recess,
        label: "Sockelrücksprung",
        value: s.recess,
        min: sk_model::MIN_RECESS,
        max: 500.0,
        zero: true,
        einheit: None,
    })
}

/// Perimeterdämmung unter der Sohlplatte (Gelände Thema 4): 0 = keine.
fn insulation_field(t: f64) -> FieldRow {
    FieldRow {
        field: Field::Insulation,
        label: "Perimeterdämmung",
        value: t,
        min: sk_model::MIN_PERIMETER,
        max: sk_model::MAX_PERIMETER,
        zero: true,
        einheit: None,
    }
}

/// Paneel für die Perimeterdämmung: Hauptmenge Fläche, Dicke als Feld
/// (sie steht an der Sohlplatte).
fn perimeter_props(
    scene: &Scene,
    id: ElementId,
    slab: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let t = m.slab_insulation(slab);
    let q = m
        .run_of(slab)
        .and_then(|r| scene.foundation(r))
        .and_then(sk_model::perimeter_qto_of);
    if let Some(q) = &q {
        values.extend([
            ("Fläche", format!("{} m²", de(q.area / 1e6, 2))),
            ("Volumen", format!("{} m³", de(q.volume / 1e9, 3))),
        ]);
    }
    values.push((
        "Sohlplatte",
        m.element(slab).map_or("–".into(), |f| f.number.clone()),
    ));
    values.push(("Bauabschnitt", e.seq.to_string()));
    let mat = m.perimeter_material();
    Some(Props {
        values,
        layer_set: mat
            .and_then(|x| m.material(x))
            .map_or(String::new(), |x| x.name.clone()),
        layers: mat
            .and_then(|x| solid_layer(m, x, t, q.as_ref().map(|q| q.volume)))
            .into_iter()
            .collect(),
        set_label: "Baustoff",
        fields: vec![insulation_field(t)],
        notes: m.warnings(id),
        ..Default::default()
    })
}

/// Zahlenfeld ohne Sonderwert 0; Bereich in mm.
fn field(field: Field, label: &'static str, value: f64, min: f64, max: f64) -> FieldRow {
    FieldRow {
        field,
        label,
        value,
        min,
        max,
        zero: false,
        einheit: None,
    }
}

/// Paneel für die Erdgeschossdecke: Hauptmenge Fläche zuerst.
fn floor_props(
    scene: &Scene,
    id: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let ElementKind::Floor(f) = e.kind else {
        return None;
    };
    let q = scene.floor_qto(f.run);
    if let Some(q) = q {
        values.extend([
            ("Fläche", format!("{} m²", de(q.area / 1e6, 2))),
            ("Volumen", format!("{} m³", de(q.volume / 1e9, 3))),
            ("Umfang", format!("{} m", de(q.perimeter / 1e3, 2))),
        ]);
    }
    let top = m.level_z(f.top).unwrap_or(0.0);
    values.push(("Oberkante", format!("+{} m", de(top / 1e3, 3))));
    values.push(("Bauabschnitt", e.seq.to_string()));
    let mut notes = m.warnings(id);
    if let Some(Err(_)) = m.floor(f.run) {
        notes.push("Kein Körper: Lage oder Umriss ungültig".into());
    }
    Some(Props {
        values,
        layer_set: m
            .material(f.material)
            .map_or(String::new(), |x| x.name.clone()),
        layers: solid_layer(m, f.material, f.thickness, q.map(|q| q.volume))
            .into_iter()
            .collect(),
        set_label: "Baustoff",
        fields: vec![field(
            Field::FloorThickness,
            "Dicke",
            f.thickness,
            100.0,
            600.0,
        )],
        sections: soffit_section(m, id).into_iter().collect(),
        notes,
        chip: None,
        ..Default::default()
    })
}

/// Paneel für einen Randdämmstreifen (K5): Hauptmenge Länge, keine Felder
/// und keine Griffe, denn er folgt Wand und Decke.
fn strip_props(
    scene: &Scene,
    id: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let ElementKind::EdgeStrip { wall, .. } = e.kind else {
        return None;
    };
    let q = scene.edge_strip_qto(id);
    if let Some(q) = &q {
        values.extend([
            ("Länge (Achse)", format!("{} m", de(q.length / 1e3, 3))),
            (
                "Querschnitt",
                format!("{} × {} cm", cm(q.width), cm(q.height)),
            ),
            ("Volumen", format!("{} m³", de(q.volume / 1e9, 3))),
        ]);
    }
    let w = m.element(wall);
    values.push(("Gehört zu", w.map_or("–".into(), |w| w.number.clone())));
    values.push(("Bauabschnitt", e.seq.to_string()));
    let mat = w
        .and_then(|w| w.layer_set)
        .and_then(|t| m.layer_set(t))
        .and_then(|t| t.strip_material());
    let mut notes = Vec::new();
    if q.is_none() {
        notes.push("Kein Körper: Decke oder Wand ungültig".into());
    }
    Some(Props {
        values,
        layer_set: mat
            .and_then(|x| m.material(x))
            .map_or(String::new(), |x| x.name.clone()),
        layers: mat
            .and_then(|x| {
                let width = q.as_ref().map_or(0.0, |q| q.width);
                solid_layer(m, x, width, q.as_ref().map(|q| q.volume))
            })
            .into_iter()
            .collect(),
        set_label: "Baustoff",
        fields: Vec::new(),
        notes,
        chip: None,
        ..Default::default()
    })
}

/// Paneel für Sohlplatte und Frostschürze: Hauptmenge zuerst.
fn foundation_props(
    scene: &Scene,
    id: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let run = m.run_of(id)?;
    let q = scene.foundation_qto(run);
    let mut fields = Vec::new();
    let (mat, t, volume) = match e.kind {
        ElementKind::GroundSlab(s) => {
            if let Some((sq, _)) = q {
                values.extend([
                    ("Fläche", format!("{} m²", de(sq.area / 1e6, 2))),
                    ("Volumen", format!("{} m³", de(sq.volume / 1e9, 3))),
                    ("Umfang", format!("{} m", de(sq.perimeter / 1e3, 2))),
                ]);
            }
            fields.push(field(
                Field::SlabThickness,
                "Dicke",
                s.thickness,
                100.0,
                1000.0,
            ));
            fields.extend(recess_field(m, id));
            fields.push(insulation_field(s.insulation));
            (s.material, s.thickness, q.map(|q| q.0.volume))
        }
        ElementKind::PerimeterInsulation { slab } => {
            return perimeter_props(scene, id, slab, values);
        }
        ElementKind::StripFooting(f) => {
            if let Some((_, fq)) = q {
                values.extend([
                    ("Länge (Achse)", format!("{} m", de(fq.length / 1e3, 2))),
                    ("Volumen", format!("{} m³", de(fq.volume / 1e9, 3))),
                ]);
            }
            fields.extend([
                field(
                    Field::FootingWidth,
                    "Breite",
                    f.width,
                    sk_model::FOOTING_WIDTH.0,
                    sk_model::FOOTING_WIDTH.1,
                ),
                field(
                    Field::FootingDepth,
                    "Tiefe",
                    m.footing_depth(id).unwrap_or(0.0),
                    sk_model::MIN_FOOTING,
                    3000.0,
                ),
            ]);
            (f.material, f.width, q.map(|q| q.1.volume))
        }
        ElementKind::Wall(_)
        | ElementKind::Floor(_)
        | ElementKind::EdgeStrip { .. }
        | ElementKind::SoffitInsulation { .. }
        | ElementKind::RoofTerrace { .. }
        | ElementKind::Coping { .. }
        | ElementKind::Roof { .. }
        | ElementKind::Ext(_) => return None,
    };
    values.push(("Bauabschnitt", e.seq.to_string()));
    let mut notes = m.warnings(id);
    if let Some(Err(err)) = m.foundation(run) {
        notes.push(match err {
            FoundationError::RecessTooLarge => "Kein Körper: Rücksprung zu groß".into(),
            _ => "Kein Körper: Umriss ungültig".into(),
        });
    }
    Some(Props {
        values,
        layer_set: m.material(mat).map_or(String::new(), |x| x.name.clone()),
        layers: solid_layer(m, mat, t, volume).into_iter().collect(),
        set_label: "Baustoff",
        fields,
        notes,
        chip: None,
        ..Default::default()
    })
}

/// Inhalt des Paneels „Eigenschaften“ für ein Bauteil.
pub fn props(scene: &Scene, id: ElementId) -> Option<Props> {
    let mut p = props_of(scene, id)?;
    // Gesperrt (Paket 4), auch über die Quelle (Zahlenfelder der Decke)
    p.locked = scene.model().is_locked(id);
    Some(p)
}

fn props_of(scene: &Scene, id: ElementId) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let q = scene.wall_qto(id);
    let mut values = vec![
        ("Nummer", e.number.clone()),
        ("Kategorie", e.category.name().to_string()),
        (
            "Geschoss",
            m.storey(e.storey).map_or("–".into(), |s| s.short.clone()),
        ),
        (
            "Gebäude",
            m.building_of(e.storey)
                .and_then(|b| m.building(b))
                .map_or("–".into(), |b| b.number.clone()),
        ),
    ];
    match e.kind {
        ElementKind::Wall(_) => {}
        ElementKind::Floor(_) => return floor_props(scene, id, values),
        ElementKind::EdgeStrip { .. } => return strip_props(scene, id, values),
        ElementKind::SoffitInsulation { floor } => return soffit_props(scene, id, floor, values),
        ElementKind::RoofTerrace { floor } | ElementKind::Coping { floor } => {
            return terrace_props(scene, id, floor, values)
        }
        ElementKind::Roof { floor, .. } => return roof_props(scene, id, floor, values),
        _ => return foundation_props(scene, id, values),
    }
    if let Some(q) = q {
        values.extend([
            ("Länge", format!("{} m", de(q.length / 1e3, 2))),
            ("Dicke", format!("{} cm", cm(q.width))),
            ("Höhe", format!("{} m", de_m(q.height))),
            ("Fläche außen", format!("{} m²", de(q.side_outer / 1e6, 2))),
            ("Fläche innen", format!("{} m²", de(q.side_inner / 1e6, 2))),
            ("Volumen", format!("{} m³", de(q.volume / 1e9, 3))),
        ]);
    }
    let set = e.layer_set.and_then(|s| m.layer_set(s));
    let layers = set.map_or(Vec::new(), |s| {
        s.layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let mat = m.material(l.material)?;
                let rgb = m.attr().surface(mat.surface)?.cut_color;
                // Luftschicht ohne Körper: keine Menge (K4)
                let body = l.function != sk_model::LayerFunction::AirGap;
                let amount =
                    q.filter(|_| body)
                        .and_then(|q| q.layers.get(i))
                        .map_or(String::new(), |lq| {
                            // in der Aufkantung weggelassen (Innenputz)
                            if lq.thickness == 0.0 {
                                return "entfällt".into();
                            }
                            format!("{} m³ · {} kg", de(lq.volume / 1e9, 3), de(lq.mass, 0))
                        });
                let thick = format!("{} cm ", cm(l.thickness));
                Some((
                    Rgba::from_rgb8(rgb),
                    format!("{thick}{}", mat.name),
                    amount,
                    Some((mat.guid, thick.len())),
                ))
            })
            .collect()
    });
    let stack = stack_row(scene, id);
    let chip = set.map(|s| {
        let mut c = type_chip(m, scene.theme(), s);
        // Eigene Merkmale überschreiben die des Typs
        c.marked = e.props.keys().any(|k| s.props.contains_key(k));
        c
    });
    Some(Props {
        values,
        layer_set: set.map_or(String::new(), |s| s.name.clone()),
        layers,
        fields: recess_field(m, id)
            .into_iter()
            .chain(stack.as_ref().map(|(w, _)| offset_field(m, *w)))
            .collect(),
        notes: m.warnings(id),
        chip,
        stack: stack.map(|(_, st)| st),
        ..Default::default()
    })
}

/// Kopplung der Wand `id` (OG Phase 2): die gestapelte Wand selbst oder am
/// EG-Segment ihr Partner oben, mit der Zeile „Kopplung“.
fn stack_row(scene: &Scene, id: ElementId) -> Option<(ElementId, crate::ui::Stack)> {
    let m = scene.model();
    let upper = scene.stack_wall(id)?;
    let (offset, linked) = m.stack_offset(upper)?;
    // Am oberen Segment der Partner darunter (EG oder ein tieferes OG)
    let below_eg = m
        .wall_below(upper)
        .and_then(|w| m.element(w))
        .and_then(|b| m.storey(b.storey))
        .is_some_and(|st| st.short == "EG");
    let partner = match (upper == id, below_eg) {
        (true, true) => "mit EG",
        _ => "mit OG",
    };
    Some((
        upper,
        crate::ui::Stack {
            linked,
            partner,
            offset: offset != 0.0,
        },
    ))
}

/// Feld „Versatz“ (m, + außen) der gestapelten Wand `wall`.
fn offset_field(m: &Model, wall: ElementId) -> FieldRow {
    FieldRow {
        field: Field::Offset,
        label: "Versatz",
        value: m.stack_offset(wall).map_or(0.0, |o| o.0),
        min: -5000.0,
        max: 5000.0,
        zero: false,
        einheit: None,
    }
}

/// Paneel für eine Untersichtdämmung (OG-17): Fläche zuerst; ihre Dicke
/// steht bei der Decke und lässt sich auch hier ändern.
fn soffit_props(
    scene: &Scene,
    id: ElementId,
    floor: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let q = scene.soffit_qto(id);
    let (t, vol) = (
        q.as_ref().map_or(0.0, |q| q.thickness),
        q.as_ref().map(|q| q.volume),
    );
    if let Some(q) = q {
        values.extend([
            ("Fläche", format!("{} m²", de(q.area / 1e6, 2))),
            ("Volumen", format!("{} m³", de(q.volume / 1e9, 3))),
        ]);
    }
    values.push((
        "Decke",
        m.element(floor).map_or("–".into(), |f| f.number.clone()),
    ));
    values.push(("Bauabschnitt", e.seq.to_string()));
    let mat = m.soffit_material_of(floor);
    Some(Props {
        values,
        layer_set: mat
            .and_then(|x| m.material(x))
            .map_or(String::new(), |x| x.name.clone()),
        layers: mat
            .and_then(|x| solid_layer(m, x, t, vol))
            .into_iter()
            .collect(),
        set_label: "Baustoff",
        sections: soffit_section(m, floor).into_iter().collect(),
        notes: m.warnings(id),
        ..Default::default()
    })
}

/// Paneel für Dachterrasse und Attikablech (D1–D3, soll-dt-3): Hauptmenge
/// zuerst, darunter der Abschnitt „Aufbau“ mit Dämmung, Belag und Attika.
fn terrace_props(
    scene: &Scene,
    id: ElementId,
    floor: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let mut layers = Vec::new();
    let mut label = String::new();
    if let ElementKind::Coping { .. } = e.kind {
        if let Some(q) = scene.coping_qto(id) {
            let cut = sk_model::terrace::coping_cut_width(q.girth);
            values.extend([
                ("Länge", format!("{} m", de(q.length / 1e3, 2))),
                ("Abwicklung", format!("{} mm", q.girth.round())),
                ("Zuschnitt", format!("{} mm", cut.round())),
            ]);
        }
        let mat = m.coping_material(floor);
        label = mat
            .and_then(|x| m.material(x))
            .map_or(String::new(), |x| x.name.clone());
    } else {
        let q = scene.terrace_qto(id);
        if let Some(q) = &q {
            values.extend([
                ("Fläche", format!("{} m²", de(q.area / 1e6, 2))),
                ("Volumen", format!("{} m³", de(q.volume / 1e9, 3))),
            ]);
            for &(mat, t, v) in &q.layers {
                layers.extend(solid_layer(m, mat, t, Some(v)));
            }
        }
        if let Some(t) = m.terrace_type_of(floor).and_then(|t| m.layer_set(t)) {
            label = t.name.clone();
        }
    }
    values.push((
        "Decke",
        m.element(floor).map_or("–".into(), |f| f.number.clone()),
    ));
    Some(Props {
        values,
        layer_set: label,
        layers,
        set_label: "Aufbau",
        sections: terrace_section(m, floor).into_iter().collect(),
        // Dachterrasse: drei Felder, die Schichten erst nach „Mehr …“
        more: !matches!(e.kind, ElementKind::Coping { .. }),
        notes: m.warnings(id),
        ..Default::default()
    })
}

/// Gefälle des Flachdachs im Paneel (%): ab 2 % nach Flachdachrichtlinie,
/// 1 % als Sonderkonstruktion; 0 = ohne.
const ROOF_SLOPE: (f64, f64) = (1.0, 10.0);

/// Eigenschaften des Dachaufbaus eines Flachdachs (D3): Mengen, Schichten
/// und die Dämmdicke seines Typs.
fn roof_props(
    scene: &Scene,
    id: ElementId,
    floor: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let mut layers = Vec::new();
    if let Some(q) = scene.flat_roof_qto(id) {
        values.extend([
            ("Fläche", format!("{} m²", de(q.area / 1e6, 2))),
            ("Volumen", format!("{} m³", de(q.volume / 1e9, 3))),
        ]);
        for &(mat, t, v) in &q.layers {
            layers.extend(solid_layer(m, mat, t, Some(v)));
        }
    }
    if let Some(r) = scene.flat_roof_over(floor) {
        values.extend([
            ("Anschluss", format!("{} m", de(r.edge_length() / 1e3, 2))),
            ("Anschlusshöhe", format!("{} cm", cm(r.upstand()))),
        ]);
        if let Some(g) = &r.slope {
            values.extend([
                ("Abläufe", format!("{} Stück", g.drains.len())),
                (
                    "Keil mittel",
                    format!("{} cm", de(g.wedge_mean() / 10.0, 1)),
                ),
                ("Keil max.", format!("{} cm", de(g.wedge_max() / 10.0, 1))),
            ]);
        }
    }
    values.push((
        "Decke",
        m.element(floor).map_or("–".into(), |f| f.number.clone()),
    ));
    let t = m.flat_roof_type(id).and_then(|t| m.layer_set(t));
    let section = t.map(|t| {
        let ins = t
            .layers
            .iter()
            .find(|l| l.function == sk_model::LayerFunction::Insulation)
            .map_or(0.0, |l| l.thickness);
        let (lo, hi) = sk_model::ROOF_INSULATION;
        crate::ui::Section {
            title: "Aufbau",
            fields: vec![field(Field::RoofInsulation, "Dämmung", ins, lo, hi)],
            hint: "Höhe der Aufkantung in der Geschossverwaltung",
            button: None,
        }
    });
    let slope = m.drainage_of_roof(id).map(|d| d.slope).unwrap_or(0.0);
    let (lo, hi) = ROOF_SLOPE;
    let gefaelle = crate::ui::Section {
        title: "Gefälle",
        fields: vec![FieldRow {
            zero: true,
            ..field(Field::RoofSlope, "Gefälle", slope, lo, hi)
        }],
        hint: "0 = ohne Gefälle; Abläufe schlägt Skizzeo vor",
        button: (slope > 0.0).then_some((crate::ui::Id::PropsDrains, "Abläufe vorschlagen")),
    };
    Some(Props {
        values,
        layer_set: t.map_or(String::new(), |t| t.name.clone()),
        layers,
        set_label: "Aufbau",
        sections: section.into_iter().chain([gefaelle]).collect(),
        more: true,
        notes: m.warnings(id),
        ..Default::default()
    })
}

/// Abschnitt „Aufbau“ der Dachterrasse: Dämmung und Belag ändern ihren Typ,
/// die Attika steht an der Decke.
fn terrace_section(m: &Model, floor: ElementId) -> Option<crate::ui::Section> {
    m.terrace_of(floor)?;
    let ElementKind::Floor(f) = &m.element(floor)?.kind else {
        return None;
    };
    let t = m.terrace_type_of(floor).and_then(|t| m.layer_set(t))?;
    let thick = |func: sk_model::LayerFunction| {
        t.layers
            .iter()
            .find(|l| l.function == func)
            .map_or(0.0, |l| l.thickness)
    };
    let (ins, fin) = (sk_model::TERRACE_INSULATION, sk_model::TERRACE_FINISH);
    Some(crate::ui::Section {
        title: "Aufbau",
        fields: vec![
            field(
                Field::TerraceInsulation,
                "Dämmung",
                thick(sk_model::LayerFunction::Insulation),
                ins.0,
                ins.1,
            ),
            field(
                Field::TerraceFinish,
                "Belag",
                thick(sk_model::LayerFunction::Finish),
                fin.0,
                fin.1,
            ),
            FieldRow {
                zero: true,
                einheit: None,
                ..field(
                    Field::Upstand,
                    "Attika über Belag",
                    f.terrace.upstand,
                    0.0,
                    sk_model::MAX_UPSTAND,
                )
            },
        ],
        hint: "folgt dem Rücksprung des OG",
        button: None,
    })
}

/// Abschnitt „Untersicht“ an einer Decke, die über einem Vorsprung auskragt.
fn soffit_section(m: &Model, floor: ElementId) -> Option<crate::ui::Section> {
    m.soffit_of(floor)?;
    let ElementKind::Floor(f) = &m.element(floor)?.kind else {
        return None;
    };
    use sk_model::CladdingValue as C;
    let c = f.soffit;
    let mut fields = vec![
        field(
            Field::Soffit,
            "Dämmung",
            c.thickness,
            sk_model::MIN_SOFFIT,
            sk_model::MAX_SOFFIT,
        ),
        FieldRow {
            zero: true,
            ..field(
                Field::Cladding(C::Thickness),
                "Bekleidung",
                c.cladding,
                sk_model::MIN_CLADDING,
                sk_model::MAX_CLADDING,
            )
        },
    ];
    // Überstand und Lattung nur mit Bekleidung (W3)
    if c.cladding > 0.0 {
        fields.extend([
            field(
                Field::Cladding(C::Drip),
                "Überstand Außenschale",
                c.drip,
                sk_model::MIN_DRIP,
                sk_model::MAX_DRIP,
            ),
            field(
                Field::Cladding(C::Batten),
                "Grundlattung a",
                c.batten,
                sk_model::MIN_BATTEN,
                sk_model::MAX_BATTEN,
            ),
            FieldRow {
                zero: true,
                ..field(
                    Field::Cladding(C::Counter),
                    "Traglattung a",
                    c.counter,
                    sk_model::MIN_BATTEN,
                    sk_model::MAX_BATTEN,
                )
            },
        ]);
    }
    Some(crate::ui::Section {
        title: "Untersicht",
        fields,
        hint: "nur unter Vorsprüngen; Lattung nur als Menge",
        button: None,
    })
}

/// Chip eines Typs (K3): Name, „Kürzel · Dicke“, Schnittbild.
pub fn type_chip(m: &Model, theme: &Theme, s: &sk_model::LayerSet) -> Chip {
    Chip {
        name: s.name.clone(),
        detail: format!("{} · {}", s.code, crate::type_look::cm_text(s.thickness())),
        look: Some(crate::type_look::type_look(m, theme, s)),
        open: false,
        marked: false,
    }
}

/// Teil der Strecke hinter der Ebene `(p0, n)` (Seite gegen `n`).
fn behind(a: Vec3, b: Vec3, (p0, n): (Vec3, Vec3)) -> Option<(Vec3, Vec3)> {
    let (da, db) = ((a - p0).dot(n), (b - p0).dot(n));
    if da > 0.0 && db > 0.0 {
        return None;
    }
    let cut = || a + (b - a) * (da / (da - db));
    Some((
        if da > 0.0 { cut() } else { a },
        if db > 0.0 { cut() } else { b },
    ))
}

/// Umriss des gewählten Bauteils in Akzentfarbe. Im Grundriss die Schnittfläche,
/// im Schnitt nur der Teil hinter der Ebene.
pub fn helpers(
    scene: &Scene,
    id: ElementId,
    view: ViewKind,
    section: Option<(Vec3, Vec3)>,
    scale: f32,
    theme: &Theme,
) -> Vec<Helper> {
    outline(
        scene,
        id,
        view,
        section,
        scale,
        theme,
        theme.interact.select,
    )
}

/// Umriss des Bauteils unter der Maus (auch vom Mengenfenster aus, F2) in
/// der Rolle `interact.hover_element`, wie [`helpers`] gezeichnet.
pub fn hover_helpers(
    scene: &Scene,
    hover: Option<ElementId>,
    view: ViewKind,
    section: Option<(Vec3, Vec3)>,
    scale: f32,
    theme: &Theme,
) -> Vec<Helper> {
    hover.map_or_else(Vec::new, |id| {
        outline(
            scene,
            id,
            view,
            section,
            scale,
            theme,
            theme.interact.hover_element,
        )
    })
}

/// Weicher Schein hinter dem Umriss eines Bauteils unter der Maus (aus der
/// Mengenliste, B7): 8 dip breit, 43 % der Hover-Farbe.
pub fn hover_glow(
    scene: &Scene,
    id: ElementId,
    view: ViewKind,
    section: Option<(Vec3, Vec3)>,
    scale: f32,
    theme: &Theme,
) -> Vec<Helper> {
    let [r, g, b, a] = theme.interact.hover_element;
    let glow = [r, g, b, a * crate::scene::GROW_GLOW];
    let mut v = outline(scene, id, view, section, scale, theme, glow);
    for h in &mut v {
        h.width = theme.size.glow_w * scale;
        h.round = true;
    }
    v
}

/// Nachleuchten nach einem Typwechsel (K3b): Schein und Umriss wie in der
/// Rückfrage des Bauteilkatalogs, beide mit Stärke `k` (Schein: 0,43 = voll).
pub fn fading_glow(
    scene: &Scene,
    id: ElementId,
    view: ViewKind,
    section: Option<(Vec3, Vec3)>,
    scale: f32,
    theme: &Theme,
    k: f32,
) -> Vec<Helper> {
    let [r, g, b, a] = theme.interact.hover_element;
    let mut v = outline(scene, id, view, section, scale, theme, [r, g, b, a * k]);
    for h in &mut v {
        h.width = theme.size.glow_w * scale;
        h.round = true;
    }
    let full = k / crate::scene::GROW_GLOW;
    v.extend(outline(
        scene,
        id,
        view,
        section,
        scale,
        theme,
        [r, g, b, a * full],
    ));
    v
}

fn outline(
    scene: &Scene,
    id: ElementId,
    view: ViewKind,
    section: Option<(Vec3, Vec3)>,
    scale: f32,
    theme: &Theme,
    color: [f32; 4],
) -> Vec<Helper> {
    // Ausgeblendetes hat keinen Umriss (§3.5)
    if !scene.visible(id) {
        return Vec::new();
    }
    let at = |p: Vec3, z: f64| vec3(p.x, p.y, z);
    let mut lines = Vec::new();
    // Umriss `f` von z0 bis z1 als Kanten (im Grundriss nur oben)
    let mut prism = |f: &[Vec3], z0: f64, z1: f64| {
        let n = f.len();
        for i in 0..n {
            let (a, b) = (f[i], f[(i + 1) % n]);
            if view == ViewKind::Plan {
                // Von oben fallen Fuß, Kopf und Kanten zusammen
                lines.push((at(a, z1), at(b, z1)));
            } else {
                lines.push((at(a, z0), at(b, z0)));
                lines.push((at(a, z1), at(b, z1)));
                lines.push((at(a, z0), at(a, z1)));
            }
        }
    };
    // Nur der Grundriss endet an seiner Schnitthöhe
    let clip = |t: f64| {
        if view == ViewKind::Plan {
            t.min(scene.plan_cut())
        } else {
            t
        }
    };
    let m = scene.model();
    match (m.segment_of(id), m.element(id).map(|e| &e.kind)) {
        (Some((run, seg)), _) => {
            let Some(f) = scene.chain(run).and_then(|c| c.segment_footprint(seg)) else {
                return Vec::new();
            };
            let (base, height) = scene.chain(run).map_or((0.0, 0.0), |c| (c.base, c.height));
            if view == ViewKind::Plan {
                prism(&f, 0.0, scene.plan_cut().min(height));
            } else {
                // Gestapelte Wand: vom eigenen Fuß (OK Trenndecke) an
                prism(&f, base, base + height);
            }
        }
        (None, Some(ElementKind::Floor(f))) => {
            // Über der Schnittebene des Grundrisses (EG): dort nicht hervorgehoben
            let Some(slab) = scene.floor(f.run) else {
                return Vec::new();
            };
            let (b, t) = slab.band();
            if view == ViewKind::Plan && b >= scene.plan_cut() {
                return Vec::new();
            }
            prism(&slab.outline, b, clip(t));
        }
        (None, Some(ElementKind::EdgeStrip { wall, .. })) => {
            // Umriss des Streifens, obwohl seine Kanten sonst fehlen (K5)
            let Some((run, seg)) = m.segment_of(*wall) else {
                return Vec::new();
            };
            let Some((slab, q)) = scene
                .floor(run)
                .and_then(|f| f.strips.get(seg).map(|q| (f, *q)))
            else {
                return Vec::new();
            };
            let (b, t) = slab.band();
            if view == ViewKind::Plan && b >= scene.plan_cut() {
                return Vec::new();
            }
            prism(&q, b, clip(t));
        }
        (None, Some(ElementKind::SoffitInsulation { floor })) => {
            // Untersichtdämmung: je auskragendem Segment ein Streifen
            let Some(slab) = m.run_of(*floor).and_then(|r| scene.floor(r)) else {
                return Vec::new();
            };
            let Some((b, t)) = slab.soffit_band() else {
                return Vec::new();
            };
            if view == ViewKind::Plan && b >= scene.plan_cut() {
                return Vec::new();
            }
            for (_, q) in &slab.soffits {
                prism(q, b, clip(t));
            }
            // Bekleidung darunter (W3)
            if let Some((b, t)) = slab.cladding_band() {
                for (_, q) in &slab.claddings {
                    prism(q, b, clip(t));
                }
            }
        }
        (None, Some(ElementKind::RoofTerrace { floor })) => {
            // Dachterrasse: ihr Umriss von OK Rohdecke bis OK Belag
            let Some(slab) = m.run_of(*floor).and_then(|r| scene.floor(r)) else {
                return Vec::new();
            };
            let Some((b, t)) = slab.terrace_band() else {
                return Vec::new();
            };
            if view == ViewKind::Plan && b >= scene.plan_cut() {
                return Vec::new();
            }
            for o in &slab.terraces.outlines {
                for q in &o.parts {
                    prism(q, b, clip(t));
                }
            }
        }
        (None, Some(ElementKind::Roof { floor, .. })) => {
            // Flachdach: Innenfläche der Aufkantung von OK Rohdecke bis OK
            // Dachhaut
            let Some(r) = scene.flat_roof_over(*floor) else {
                return Vec::new();
            };
            let (b, t) = r.band();
            if view == ViewKind::Plan && b >= scene.plan_cut() {
                return Vec::new();
            }
            prism(&r.outline, b, clip(t));
        }
        (None, Some(ElementKind::Coping { floor })) => {
            // Attikablech: die Kanten seines Profils über der Attikakrone
            // bzw. der Krone der Aufkantung
            let body = if m.flat_roof_coping(id, *floor) {
                let Some(r) = scene.flat_roof_over(*floor) else {
                    return Vec::new();
                };
                r.coping_solid()
            } else {
                let Some(slab) = m.run_of(*floor).and_then(|r| scene.floor(r)) else {
                    return Vec::new();
                };
                slab.coping_solid()
            };
            let low = body
                .edges
                .iter()
                .map(|e| e.a.z.min(e.b.z))
                .fold(f64::MAX, f64::min);
            if view == ViewKind::Plan && low >= scene.plan_cut() {
                return Vec::new();
            }
            lines.extend(body.edges.iter().map(|e| (e.a, e.b)));
        }
        (None, Some(kind)) => {
            let Some(found) = m.run_of(id).and_then(|r| scene.foundation(r)) else {
                return Vec::new();
            };
            let p = found.params;
            let t = p.slab_thickness;
            match kind {
                ElementKind::GroundSlab(_) => prism(&found.outline, -t, 0.0),
                _ => {
                    let bottom = -t - p.footing_depth;
                    prism(&found.outline, bottom, -t);
                    if let FootingShape::Ring(inset) = &found.footing {
                        prism(&inset.pts, bottom, -t);
                    }
                }
            }
        }
        _ => return Vec::new(),
    }
    if view == ViewKind::Section {
        let Some(pl) = section else {
            return Vec::new();
        };
        lines = lines
            .into_iter()
            .filter_map(|(a, b)| behind(a, b, pl))
            .collect();
    }
    lines
        .into_iter()
        .map(|(a, b)| Helper {
            a: a.to_f32(),
            b: b.to_f32(),
            color,
            width: theme.size.outline * scale,
            dash: 0.0,
            pattern: sk_render::SOLID,
            occlude: view == ViewKind::Persp,
            round: true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::{Model, RefSide, WallChain};

    /// Ungültiges Deckenauflager aus einer Datei (Regel 21): Die Wand nennt
    /// in den Eigenschaften den Grund, warum die Randdämmstreifen fehlen.
    #[test]
    fn eigenschaften_nennen_ungueltiges_auflager() {
        let mut m = Model::with_seed(21);
        m.allow_unstepped();
        let b = m.add_building(2);
        let pts = [
            sk_math::vec3(0.0, 0.0, 0.0),
            sk_math::vec3(0.0, 8000.0, 0.0),
            sk_math::vec3(10000.0, 8000.0, 0.0),
            sk_math::vec3(10000.0, 0.0, 0.0),
        ];
        let aw = m.build_from_polygon(b, &pts).unwrap();
        let mono = m.type_by_guid(sk_model::MONO_TYPE_GUID).unwrap();
        assert!(m.set_run_type(aw, mono));
        let wand = |m: &Model| {
            m.elements()
                .iter()
                .find(|(_, e)| e.category == sk_model::Category::ExteriorWall)
                .map(|(id, _)| id)
                .unwrap()
        };
        let ok = Scene::with_model(m.clone());
        assert!(props(&ok, wand(&m)).unwrap().notes.is_empty());
        let text = sk_model::szo::write(&m).replacen("bearing=240", "bearing=400", 1);
        let l = sk_model::szo::read(&text, sk_model::GuidGen::with_seed(1)).unwrap();
        let w = wand(&l.model);
        let s = Scene::with_model(l.model);
        assert_eq!(
            props(&s, w).unwrap().notes,
            ["Deckenauflager des Typs ungültig"]
        );
    }

    #[test]
    fn deutsches_zahlenformat() {
        assert_eq!(de(1608.4, 0), "1.608");
        assert_eq!(de(8.3887, 3), "8,389");
        assert_eq!(de(27.5, 2), "27,50");
        assert_eq!(de(1234567.891, 2), "1.234.567,89");
        assert_eq!(de(-0.0001, 2), "0,00");
        assert_eq!(de(-12.5, 1), "-12,5");
        assert_eq!(
            (de_m(2855.0), de_m(2980.0), de_m(3500.0)),
            ("2,855".into(), "2,98".into(), "3,50".into())
        );
        assert_eq!(
            (cm(140.0), cm(175.0), cm(315.0)),
            ("14".into(), "17,5".into(), "31,5".into())
        );
    }

    /// Rechteck 10 × 8 m im Uhrzeigersinn, Außenkante auf der Bezugslinie.
    fn rechteck() -> WallChain {
        WallChain {
            base: 0.0,
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            layers: Vec::new(),
            height: 2750.0,
            joints: Default::default(),
        }
    }

    /// Das Rechteck mit OK EG +2,75: EG-Wände 2,75 hoch (B12: Wände reichen
    /// von UK bis OK ihres Geschosses).
    fn haus(s: &mut Scene) -> sk_model::RunId {
        let run = s.add_wall(&rechteck()).unwrap();
        assert!(s.edit_model("OK EG", |m| {
            let eg = m.defaults().storey;
            m.set_storey_top(eg, 2750.0)
        }));
        run
    }

    fn value(p: &Props, k: &str) -> String {
        p.values.iter().find(|v| v.0 == k).unwrap().1.clone()
    }

    #[test]
    fn klick_waehlt_wand_und_rueckgaengig_hebt_auf() {
        let mut s = Scene::with_model(Model::with_seed(5));
        let run = haus(&mut s);
        let mut sel = Selection::default();
        // Segment 2 läuft bei x = 10 m von y = 8 m nach 0; Strahl von außen (+x)
        let o = vec3(20000.0, 4000.0, 1000.0);
        let hit = s.pick(ViewKind::Persp, None, o, vec3(-1.0, 0.0, 0.0));
        assert_eq!(hit, s.model().wall_at(run, 2));
        assert!(sel.set(hit));
        let p = props(&s, sel.id.unwrap()).unwrap();
        assert_eq!(
            value(&p, "Nummer"),
            s.model().element(hit.unwrap()).unwrap().number
        );
        assert_eq!(value(&p, "Kategorie"), "Außenwand");
        assert_eq!(value(&p, "Geschoss"), "EG");
        assert_eq!(value(&p, "Gebäude"), "GB-01");
        assert_eq!(value(&p, "Länge"), "8,00 m");
        assert_eq!(value(&p, "Dicke"), "31,5 cm");
        assert_eq!(value(&p, "Höhe"), "2,75 m");
        assert_eq!(p.layer_set, "AW 31,5 Porenbeton + WDVS");
        assert_eq!(p.layers.len(), 2);
        assert_eq!(p.layers[0].1, "14 cm Dämmung (WDVS)");
        assert_eq!(p.layers[1].1, "17,5 cm Porenbeton");

        // Grundriss: von oben auf die Schnittfläche
        let down = vec3(0.0, 0.0, -1.0);
        let top = vec3(9900.0, 4000.0, 9000.0);
        assert_eq!(s.pick(ViewKind::Plan, None, top, down), hit);
        // Schnitt bei y = 4 m, Blick nach +y: Die Wand liegt in der Ebene und bleibt treffbar
        let pl = Some((vec3(0.0, 4000.0, 0.0), vec3(0.0, -1.0, 0.0)));
        let o = vec3(9900.0, -5000.0, 1000.0);
        assert_eq!(s.pick(ViewKind::Section, pl, o, vec3(0.0, 1.0, 0.0)), hit);
        assert!(!helpers(&s, hit.unwrap(), ViewKind::Section, pl, 1.0, &Theme::dark()).is_empty());
        assert_eq!(
            helpers(&s, hit.unwrap(), ViewKind::Persp, None, 1.0, &Theme::dark()).len(),
            12
        );

        // Daneben: nichts
        assert_eq!(s.pick(ViewKind::Persp, None, o, vec3(0.0, 0.0, 1.0)), None);

        // Rückgängig des Anlegens: Die Auswahl gilt nicht mehr
        assert!(!sel.validate(&s));
        assert!(s.undo(), "OK EG");
        assert!(s.undo(), "Anlegen");
        assert!(sel.validate(&s));
        assert_eq!(sel.id, None);
    }

    #[test]
    fn mengen_im_paneel() {
        let mut s = Scene::with_model(Model::with_seed(6));
        let run = haus(&mut s);
        // Obere Wand: 10 m außen, Gehrung an beiden Enden
        let p = props(&s, s.model().wall_at(run, 1).unwrap()).unwrap();
        assert_eq!(value(&p, "Länge"), "10,00 m");
        assert_eq!(value(&p, "Fläche außen"), "27,50 m²");
        assert_eq!(value(&p, "Fläche innen"), "25,77 m²");
        let q = s.wall_qto(s.model().wall_at(run, 1).unwrap()).unwrap();
        assert_eq!(
            value(&p, "Volumen"),
            format!("{} m³", de(q.volume / 1e9, 3))
        );
        assert!(p.layers[1].2.ends_with(" kg"), "{}", p.layers[1].2);
    }

    #[test]
    fn mengen_erst_nach_dem_loslassen() {
        let mut s = Scene::with_model(Model::with_seed(7));
        let run = s.add_wall(&rechteck()).unwrap();
        // Linke Wand: wird länger, wenn die obere nach außen rückt
        let wall = s.model().wall_at(run, 0).unwrap();
        let vol = s.wall_qto(wall).unwrap().volume;
        s.begin("Wand verschieben");
        // Live-Ziehen der oberen Wand: keine Mengen
        let moved = s
            .chain(run)
            .unwrap()
            .with_segment_moved(1, -1000.0)
            .unwrap();
        s.set_run_points(run, &moved.points);
        assert!(s.wall_qto(wall).is_none());
        // Loslassen: Mengen für den neuen Stand
        s.commit();
        let q = s.wall_qto(wall).unwrap();
        assert!(q.volume > vol);
        assert_eq!(Some(q), sk_model::wall_qto(s.model(), wall).as_ref());
    }

    #[test]
    fn klick_erkennt_kleine_bewegung() {
        let mut sel = Selection::default();
        sel.press(100.0, 100.0);
        assert!(sel.release(102.0, 101.0, 1.0));
        sel.press(100.0, 100.0);
        assert!(!sel.release(120.0, 100.0, 1.0));
        assert!(!sel.release(100.0, 100.0, 1.0));
    }

    #[test]
    fn umriss_in_der_auswahlfarbe_des_schemas() {
        let mut s = Scene::with_model(Model::with_seed(5));
        let run = s.add_wall(&rechteck()).unwrap();
        let id = s.model().wall_at(run, 0).unwrap();
        let mut th = Theme::dark();
        let colors = |th: &Theme| -> Vec<[f32; 4]> {
            helpers(&s, id, ViewKind::Persp, None, 1.0, th)
                .iter()
                .map(|h| h.color)
                .collect()
        };
        assert!(colors(&th).iter().all(|&c| c == th.ui.accent.to_f32()));
        th.set_accent(Rgba::rgb(40, 120, 220));
        assert!(colors(&th)
            .iter()
            .all(|&c| c == [40.0 / 255.0, 120.0 / 255.0, 220.0 / 255.0, 1.0]));
    }

    #[test]
    fn umriss_einer_og_wand_beginnt_an_ihrem_fuss() {
        let mut s = Scene::with_model(Model::with_seed(5));
        s.open_building_dialog();
        let eg = s.add_wall(&rechteck()).unwrap();
        let m = s.model();
        let og = m
            .runs()
            .ids()
            .find(|&r| r != eg && m.wall_at(r, 0).is_some_and(|w| m.wall_below(w).is_some()))
            .expect("OG-Zug");
        let id = m.wall_at(og, 0).unwrap();
        let base = s.chain(og).unwrap().base;
        assert!(base > 2000.0, "OG steht auf der Trenndecke");
        let z: Vec<f32> = helpers(&s, id, ViewKind::Persp, None, 1.0, &Theme::dark())
            .iter()
            .flat_map(|h| [h.a[2], h.b[2]])
            .collect();
        let low = z.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            (low - base as f32).abs() < 5.0,
            "Umriss ab {low}, Fuß {base}"
        );
    }

    /// Der Umriss einer Decke in 3D reicht über ihre ganze Dicke; die
    /// Schnitthöhe des Grundrisses (aktives EG) kürzt ihn nur im Grundriss.
    #[test]
    fn deckenumriss_in_3d_ungekuerzt() {
        let mut m = Model::with_seed(22);
        let b = m.add_building(2);
        let pts = [
            sk_math::vec3(0.0, 0.0, 0.0),
            sk_math::vec3(0.0, 8000.0, 0.0),
            sk_math::vec3(10000.0, 8000.0, 0.0),
            sk_math::vec3(10000.0, 0.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let s = Scene::with_model(m);
        let (de, _) = s
            .model()
            .elements()
            .iter()
            .filter(|(_, e)| e.category == sk_model::Category::Floor)
            .min_by(|a, b| a.1.number.cmp(&b.1.number))
            .unwrap();
        let z: Vec<f32> = helpers(&s, de, ViewKind::Persp, None, 1.0, &Theme::dark())
            .iter()
            .flat_map(|h| [h.a[2], h.b[2]])
            .collect();
        let (lo, hi) = z
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        assert!(
            (lo - 2635.0).abs() < 1.0 && (hi - 2855.0).abs() < 1.0,
            "{lo}–{hi}"
        );
    }
}

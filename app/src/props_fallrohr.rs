//! Paneel „Eigenschaften“ des Regenfallrohrs (Jörn 11.10.,
//! `entwaesserung/konzept-regenfallrohr.md`): Mengen und Höhen, der
//! Schellenabstand als Feld, Notentwässerung und Kontrollablauf als Knopf.

use super::{de, field};
use crate::scene::Scene;
use crate::ui::{Field, Id, Props, Section};
use sk_model::fallrohr::{Overflow, BOX_D, BOX_W, SPACING_MAX, SPACING_MIN};
use sk_model::{ElementId, ElementKind};

/// Knöpfe unter den Abschnitten der Eigenschaften.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Notfallrohr und Wasserspeier tauschen.
    Overflow,
    /// Kontrollablauf der Dampfsperrebene ein- oder ausbauen.
    Control,
}

/// Inhalt des Paneels für das Regenfallrohr `id`.
pub fn downpipe_props(
    scene: &Scene,
    id: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let ElementKind::Downpipe { roof, drain, spec } = m.element(id)?.kind else {
        return None;
    };
    let m_ = |mm: f64| format!("{} m", de(mm / 1000.0, 2));
    let floor = m.floor_of_roof(roof);
    let mat = floor
        .and_then(|f| m.coping_material(f))
        .and_then(|x| m.material(x))
        .map_or(String::new(), |x| x.name.clone());
    if let Some(d) = m.downpipe(id) {
        values.extend([
            ("Nennweite", "DN 100".to_string()),
            ("Fallrohr", m_(d.pipe_length())),
            ("Rohrschellen", format!("{} Stück", d.clamp_count())),
            (
                "Wasserfangkasten",
                format!(
                    "{:.0} × {:.0} × {:.0} mm",
                    BOX_W,
                    BOX_D,
                    d.box_top - d.box_bot
                ),
            ),
            (
                "Notentwässerung",
                match spec.overflow {
                    Overflow::Pipe => format!("Notfallrohr {}", m_(d.overflow_length())),
                    Overflow::Spout => "Wasserspeier".into(),
                },
            ),
            (
                "Kontrollablauf",
                if d.control.is_some() {
                    "DN 50 auf der Dampfsperre".into()
                } else {
                    "ohne".into()
                },
            ),
        ]);
    }
    let n = m.drainage_of_roof(roof).map_or(0, |d| d.drains.len());
    values.extend([
        ("Ablauf", format!("{} von {n}", drain as usize + 1)),
        (
            "Flachdach",
            m.element(roof).map_or("–".into(), |e| e.number.clone()),
        ),
    ]);
    let sections = vec![
        Section {
            title: "Rohrschellen",
            fields: vec![field(
                Field::DownpipeSpacing,
                "Abstand höchstens",
                spec.spacing,
                SPACING_MIN,
                SPACING_MAX,
            )],
            hint: "Hersteller: bis DN 100 höchstens 3 m",
            buttons: Vec::new(),
        },
        Section {
            title: "Notentwässerung",
            fields: Vec::new(),
            hint: "DIN 1986-100: frei aufs Grundstück, nicht in den Kanal",
            buttons: vec![(
                Id::PropsAction(Action::Overflow),
                match spec.overflow {
                    Overflow::Pipe => "Wasserspeier statt Notfallrohr",
                    Overflow::Spout => "Notfallrohr statt Wasserspeier",
                },
            )],
        },
        Section {
            title: "Kontrollablauf",
            fields: Vec::new(),
            hint: "Wasser von der Dampfsperre in den Wasserfangkasten",
            buttons: vec![(
                Id::PropsAction(Action::Control),
                if spec.control {
                    "Kontrollablauf entfernen"
                } else {
                    "Kontrollablauf einbauen"
                },
            )],
        },
    ];
    Some(Props {
        values,
        layer_set: mat,
        set_label: "Baustoff",
        sections,
        notes: m.warnings(id),
        ..Default::default()
    })
}

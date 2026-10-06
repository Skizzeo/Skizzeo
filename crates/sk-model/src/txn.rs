//! Änderungsprotokoll für Rückgängig und Wiederholen.
//!
//! Ein Schritt ([`Txn`]) hält je geänderten Datensatz den Stand vorher und
//! nachher, nicht das ganze Modell. Ändert ein Schritt denselben Datensatz
//! mehrfach (Ziehen mit vielen Mausbewegungen), bleibt ein Eintrag mit dem
//! ersten alten und dem letzten neuen Stand. Geöffnet und geschlossen wird ein
//! Schritt mit [`crate::Model::begin`] und [`crate::Model::commit`].

use crate::attr::{Display, Fill, FillId, LineType, LineTypeId, Pen, PenId, Surface, SurfaceId};
use crate::element::{Building, BuildingId, Element, ElementId, RunId, Storey, StoreyId, WallRun};
use crate::library::{LayerSet, LayerSetId, Material, MaterialId};

/// Ein geänderter Datensatz: `None` heißt „gab es nicht“ (angelegt bzw. gelöscht).
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Run {
        id: RunId,
        old: Option<WallRun>,
        new: Option<WallRun>,
    },
    Element {
        id: ElementId,
        old: Option<Element>,
        new: Option<Element>,
    },
    LayerSet {
        id: LayerSetId,
        old: Option<LayerSet>,
        new: Option<LayerSet>,
    },
    Material {
        id: MaterialId,
        old: Option<Material>,
        new: Option<Material>,
    },
    Storey {
        id: StoreyId,
        old: Option<Storey>,
        new: Option<Storey>,
    },
    Building {
        id: BuildingId,
        old: Option<Building>,
        new: Option<Building>,
    },
    Pen {
        id: PenId,
        old: Option<Pen>,
        new: Option<Pen>,
    },
    LineType {
        id: LineTypeId,
        old: Option<LineType>,
        new: Option<LineType>,
    },
    Fill {
        id: FillId,
        old: Option<Fill>,
        new: Option<Fill>,
    },
    Surface {
        id: SurfaceId,
        old: Option<Surface>,
        new: Option<Surface>,
    },
    Display {
        old: Display,
        new: Display,
    },
}

impl Change {
    /// Ändert der Eintrag nichts (Stand vorher gleich nachher)?
    pub fn is_noop(&self) -> bool {
        match self {
            Change::Run { old, new, .. } => old == new,
            Change::Element { old, new, .. } => old == new,
            Change::LayerSet { old, new, .. } => old == new,
            Change::Material { old, new, .. } => old == new,
            Change::Storey { old, new, .. } => old == new,
            Change::Building { old, new, .. } => old == new,
            Change::Pen { old, new, .. } => old == new,
            Change::LineType { old, new, .. } => old == new,
            Change::Fill { old, new, .. } => old == new,
            Change::Surface { old, new, .. } => old == new,
            Change::Display { old, new } => old == new,
        }
    }
}

/// Ein Schritt für Rückgängig/Wiederholen.
#[derive(Clone, Debug, PartialEq)]
pub struct Txn {
    pub label: &'static str,
    pub changes: Vec<Change>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Undo,
    Redo,
}

/// Was ein angewandter Schritt berührt hat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Touched {
    /// Wandzüge, deren Geometrie oder Wände sich geändert haben.
    pub runs: Vec<RunId>,
    /// Aufbau oder Baustoff geändert: alle Züge, die ihn nutzen, sind betroffen.
    pub library: bool,
    /// Stifte, Schraffuren, Oberflächen oder Darstellung geändert.
    pub attr: bool,
}

impl Touched {
    pub(crate) fn run(&mut self, id: RunId) {
        if !self.runs.contains(&id) {
            self.runs.push(id);
        }
    }
}

/// Datensatz-Schlüssel zum Zusammenfassen innerhalb eines Schritts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Key {
    Run(RunId),
    Element(ElementId),
    LayerSet(LayerSetId),
    Material(MaterialId),
    Storey(StoreyId),
    Building(BuildingId),
    Pen(PenId),
    LineType(LineTypeId),
    Fill(FillId),
    Surface(SurfaceId),
    Display,
}

/// Offener Schritt.
#[derive(Clone, Debug)]
pub(crate) struct Open {
    pub label: &'static str,
    pub changes: Vec<Change>,
    pub noted: std::collections::HashSet<Key>,
}

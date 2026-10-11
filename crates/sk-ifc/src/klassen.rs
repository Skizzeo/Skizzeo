//! IFC4-Klassen, die Skizzeo schreibt: Name im Schema, Zahl der Attribute,
//! erlaubte vordefinierte Typen, Typklasse und allgemeiner Eigenschaftssatz.
//! Eine Klasse aus `kinds.rs` oder aus einer Erweiterung, die hier fehlt,
//! wird ein `IfcBuildingElementProxy` mit der Klasse als `ObjectType`; ein
//! Typwert, den IFC4 nicht kennt, wird `USERDEFINED` (z. B. `COPING`, das erst
//! IFC4.3 kennt).

/// Angaben zu einer Bauteilklasse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Klasse {
    /// Name im Schema, z. B. „IfcWall“.
    pub name: &'static str,
    /// Attribute einer Instanz; 9 bei den meisten Bauteilen (bis `Tag` und
    /// `PredefinedType`), 13 bei Tür und Fenster, 11 beim Raum.
    pub attrs: u8,
    /// Vordefinierte Typen ohne `USERDEFINED` und `NOTDEFINED`.
    pub types: &'static [&'static str],
    /// Typklasse, z. B. „IfcWallType“.
    pub type_class: &'static str,
    /// Allgemeiner Eigenschaftssatz und die Eigenschaften daraus, die
    /// Skizzeo kennt.
    pub pset: Option<(&'static str, &'static [Prop])>,
}

/// Eigenschaften aus den allgemeinen Sätzen (Pset_…Common), die Skizzeo
/// füllen kann.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    Reference,
    IsExternal,
    LoadBearing,
    ThermalTransmittance,
}

use Prop::*;

const ALL4: &[Prop] = &[Reference, IsExternal, LoadBearing, ThermalTransmittance];

/// Die Tabelle (IFC4 ADD2 TC1).
pub const KLASSEN: &[Klasse] = &[
    Klasse {
        name: "IfcWall",
        attrs: 9,
        types: &[
            "MOVABLE",
            "PARAPET",
            "PARTITIONING",
            "PLUMBINGWALL",
            "SHEAR",
            "SOLIDWALL",
            "STANDARD",
            "POLYGONAL",
            "ELEMENTEDWALL",
        ],
        type_class: "IfcWallType",
        pset: Some(("Pset_WallCommon", ALL4)),
    },
    Klasse {
        name: "IfcSlab",
        attrs: 9,
        types: &["FLOOR", "ROOF", "LANDING", "BASESLAB"],
        type_class: "IfcSlabType",
        pset: Some(("Pset_SlabCommon", ALL4)),
    },
    Klasse {
        name: "IfcFooting",
        attrs: 9,
        types: &[
            "CAISSON_FOUNDATION",
            "FOOTING_BEAM",
            "PAD_FOOTING",
            "PILE_CAP",
            "STRIP_FOOTING",
        ],
        type_class: "IfcFootingType",
        pset: Some(("Pset_FootingCommon", &[Reference, LoadBearing])),
    },
    Klasse {
        name: "IfcCovering",
        attrs: 9,
        types: &[
            "CEILING",
            "FLOORING",
            "CLADDING",
            "ROOFING",
            "MOLDING",
            "SKIRTINGBOARD",
            "INSULATION",
            "MEMBRANE",
            "SLEEVING",
            "WRAPPING",
        ],
        type_class: "IfcCoveringType",
        pset: Some((
            "Pset_CoveringCommon",
            &[Reference, IsExternal, ThermalTransmittance],
        )),
    },
    Klasse {
        name: "IfcRoof",
        attrs: 9,
        types: &[
            "FLAT_ROOF",
            "SHED_ROOF",
            "GABLE_ROOF",
            "HIP_ROOF",
            "HIPPED_GABLE_ROOF",
            "GAMBREL_ROOF",
            "MANSARD_ROOF",
            "BARREL_ROOF",
            "RAINBOW_ROOF",
            "BUTTERFLY_ROOF",
            "PAVILION_ROOF",
            "DOME_ROOF",
            "FREEFORM",
        ],
        type_class: "IfcRoofType",
        pset: Some((
            "Pset_RoofCommon",
            &[Reference, IsExternal, ThermalTransmittance],
        )),
    },
    Klasse {
        name: "IfcBuildingElementPart",
        attrs: 9,
        types: &["INSULATION", "PRECASTPANEL"],
        type_class: "IfcBuildingElementPartType",
        pset: None,
    },
    Klasse {
        name: "IfcBuildingElementProxy",
        attrs: 9,
        types: &[
            "COMPLEX",
            "ELEMENT",
            "PARTIAL",
            "PROVISIONFORVOID",
            "PROVISIONFORSPACE",
        ],
        type_class: "IfcBuildingElementProxyType",
        pset: Some(("Pset_BuildingElementProxyCommon", ALL4)),
    },
    Klasse {
        name: "IfcPipeSegment",
        attrs: 9,
        types: &[
            "CULVERT",
            "FLEXIBLESEGMENT",
            "RIGIDSEGMENT",
            "GUTTER",
            "SPOOL",
        ],
        type_class: "IfcPipeSegmentType",
        pset: None,
    },
    Klasse {
        name: "IfcColumn",
        attrs: 9,
        types: &["COLUMN", "PILASTER"],
        type_class: "IfcColumnType",
        pset: Some(("Pset_ColumnCommon", &[Reference, IsExternal, LoadBearing])),
    },
    Klasse {
        name: "IfcBeam",
        attrs: 9,
        types: &[
            "BEAM",
            "JOIST",
            "HOLLOWCORE",
            "LINTEL",
            "SPANDREL",
            "T_BEAM",
        ],
        type_class: "IfcBeamType",
        pset: Some(("Pset_BeamCommon", &[Reference, IsExternal, LoadBearing])),
    },
    Klasse {
        name: "IfcMember",
        attrs: 9,
        types: &[
            "BRACE", "CHORD", "COLLAR", "MEMBER", "MULLION", "PLATE", "POST", "PURLIN", "RAFTER",
            "STRINGER", "STRUT", "STUD",
        ],
        type_class: "IfcMemberType",
        pset: Some(("Pset_MemberCommon", &[Reference, IsExternal, LoadBearing])),
    },
    Klasse {
        name: "IfcPlate",
        attrs: 9,
        types: &["CURTAIN_PANEL", "SHEET"],
        type_class: "IfcPlateType",
        pset: Some(("Pset_PlateCommon", &[Reference, IsExternal, LoadBearing])),
    },
    Klasse {
        name: "IfcRailing",
        attrs: 9,
        types: &["HANDRAIL", "GUARDRAIL", "BALUSTRADE"],
        type_class: "IfcRailingType",
        pset: Some(("Pset_RailingCommon", &[Reference, IsExternal])),
    },
    Klasse {
        name: "IfcStair",
        attrs: 9,
        types: &[
            "STRAIGHT_RUN_STAIR",
            "TWO_STRAIGHT_RUN_STAIR",
            "QUARTER_WINDING_STAIR",
            "QUARTER_TURN_STAIR",
            "HALF_WINDING_STAIR",
            "HALF_TURN_STAIR",
            "TWO_QUARTER_WINDING_STAIR",
            "TWO_QUARTER_TURN_STAIR",
            "THREE_QUARTER_WINDING_STAIR",
            "THREE_QUARTER_TURN_STAIR",
            "SPIRAL_STAIR",
            "DOUBLE_RETURN_STAIR",
            "CURVED_RUN_STAIR",
            "TWO_CURVED_RUN_STAIR",
        ],
        type_class: "IfcStairType",
        pset: Some(("Pset_StairCommon", &[Reference, IsExternal])),
    },
    Klasse {
        name: "IfcChimney",
        attrs: 9,
        types: &[],
        type_class: "IfcChimneyType",
        pset: Some((
            "Pset_ChimneyCommon",
            &[Reference, IsExternal, LoadBearing, ThermalTransmittance],
        )),
    },
    Klasse {
        name: "IfcShadingDevice",
        attrs: 9,
        types: &["JALOUSIE", "SHUTTER", "AWNING"],
        type_class: "IfcShadingDeviceType",
        pset: Some(("Pset_ShadingDeviceCommon", &[Reference, IsExternal])),
    },
    Klasse {
        name: "IfcFurniture",
        attrs: 9,
        types: &[
            "CHAIR",
            "TABLE",
            "DESK",
            "BED",
            "FILECABINET",
            "SHELF",
            "SOFA",
        ],
        type_class: "IfcFurnitureType",
        pset: None,
    },
    Klasse {
        name: "IfcDoor",
        attrs: 13,
        types: &["DOOR", "GATE", "TRAPDOOR"],
        type_class: "IfcDoorType",
        pset: Some((
            "Pset_DoorCommon",
            &[Reference, IsExternal, ThermalTransmittance],
        )),
    },
    Klasse {
        name: "IfcWindow",
        attrs: 13,
        types: &["WINDOW", "SKYLIGHT", "LIGHTDOME"],
        type_class: "IfcWindowType",
        pset: Some((
            "Pset_WindowCommon",
            &[Reference, IsExternal, ThermalTransmittance],
        )),
    },
    Klasse {
        name: "IfcSpace",
        attrs: 11,
        types: &["SPACE", "PARKING", "GFA", "INTERNAL", "EXTERNAL"],
        type_class: "IfcSpaceType",
        pset: Some(("Pset_SpaceCommon", &[Reference, IsExternal])),
    },
    Klasse {
        name: "IfcOpeningElement",
        attrs: 9,
        types: &["OPENING", "RECESS"],
        type_class: "",
        pset: None,
    },
];

/// Stellvertreter für alles, was die Tabelle nicht kennt.
pub fn proxy() -> &'static Klasse {
    lookup("IfcBuildingElementProxy").expect("Stellvertreter in der Tabelle")
}

/// Klasse nach Namen (Groß- und Kleinschreibung egal).
pub fn lookup(name: &str) -> Option<&'static Klasse> {
    KLASSEN.iter().find(|k| k.name.eq_ignore_ascii_case(name))
}

/// Wie ein Bauteil geschrieben wird: Klasse, vordefinierter Typ und
/// `ObjectType` (nur bei `USERDEFINED`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zuordnung {
    pub klasse: &'static Klasse,
    pub predefined: &'static str,
    pub object_type: Option<String>,
}

/// Deutet eine Angabe wie „IfcSlab.BASESLAB“, „IfcWall“ oder
/// „IfcCovering.COPING“.
pub fn zuordnen(ifc: &str) -> Zuordnung {
    let (name, typ) = match ifc.split_once('.') {
        Some((n, t)) => (n.trim(), Some(t.trim().to_ascii_uppercase())),
        None => (ifc.trim(), None),
    };
    let Some(klasse) = lookup(name) else {
        return Zuordnung {
            klasse: proxy(),
            predefined: "USERDEFINED",
            object_type: Some(ifc.trim().to_string()),
        };
    };
    match typ {
        None => Zuordnung {
            klasse,
            predefined: "NOTDEFINED",
            object_type: None,
        },
        Some(t) => match klasse.types.iter().find(|x| **x == t) {
            Some(x) => Zuordnung {
                klasse,
                predefined: x,
                object_type: None,
            },
            None if t == "NOTDEFINED" => Zuordnung {
                klasse,
                predefined: "NOTDEFINED",
                object_type: None,
            },
            None => Zuordnung {
                klasse,
                predefined: "USERDEFINED",
                object_type: Some(t),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zuordnung() {
        let z = zuordnen("IfcSlab.BASESLAB");
        assert_eq!((z.klasse.name, z.predefined), ("IfcSlab", "BASESLAB"));
        let z = zuordnen("IfcCovering.COPING");
        assert_eq!(z.predefined, "USERDEFINED");
        assert_eq!(z.object_type.as_deref(), Some("COPING"));
        let z = zuordnen("IfcWall");
        assert_eq!((z.klasse.name, z.predefined), ("IfcWall", "NOTDEFINED"));
        let z = zuordnen("IfcKaminofen");
        assert_eq!(z.klasse.name, "IfcBuildingElementProxy");
        assert_eq!(z.object_type.as_deref(), Some("IfcKaminofen"));
        assert_eq!(zuordnen("IFCCOLUMN.column").predefined, "COLUMN");
    }

    /// Jede Bauteilart des Lieferumfangs hat eine Klasse der Tabelle, damit
    /// nichts still zum Stellvertreter wird.
    #[test]
    fn kinds_bekannt() {
        for c in sk_model::Category::ALL {
            let z = zuordnen(c.ifc_class());
            assert!(
                lookup(c.ifc_class().split('.').next().unwrap()).is_some(),
                "{c:?}: {}",
                c.ifc_class()
            );
            assert!(matches!(z.klasse.attrs, 9 | 11 | 13));
        }
    }
}

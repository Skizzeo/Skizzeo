//! Teilzustände von [`crate::App`], nach Themen gegliedert (Paket T4):
//! Was zu einem Thema gehört, liegt beisammen statt als lose Felder im
//! Programmzustand.

/// Eingelesene Erweiterungen (E6) und ihre offene Liste; der Ordner und
/// das Fenster „Erweiterungen“ (E5).
pub(crate) struct Erweiterungen {
    pub(crate) bib: crate::ext_werkzeug::Bibliothek,
    pub(crate) ablage: crate::ext_ablage::Ablage,
    pub(crate) fenster: Option<crate::ext_verwaltung::Fenster>,
    pub(crate) liste: Option<crate::auswahl::Auswahl>,
}

/// Baumpanel (Paket 4) und was die App über ihn weiß.
pub(crate) struct Baum {
    pub(crate) panel: crate::tree_panel::TreePanel,
    /// Der Hover kommt aus dem Baum.
    pub(crate) hover: bool,
    /// Die Maus steht über dem Baum.
    pub(crate) maus: bool,
    /// Die Auswahl kam aus dem Baum, dann bleiben die Äste.
    pub(crate) gewaehlt: bool,
    /// Zuletzt gesehene Auswahl.
    pub(crate) zuletzt: Option<sk_model::ElementId>,
    /// Zuletzt gesehen: Eigenschaften offen (dann rückt der Baum animiert).
    pub(crate) eigenschaften: bool,
}

impl Baum {
    pub(crate) fn new() -> Self {
        Self {
            panel: crate::tree_panel::TreePanel::new(),
            hover: false,
            maus: false,
            gewaehlt: false,
            zuletzt: None,
            eigenschaften: false,
        }
    }
}

/// Einstellungsfenster (E5), was es sich für die Sitzung merkt, und ob sein
/// Bild bzw. sein Aufklapper neu zu zeichnen ist.
#[derive(Default)]
pub(crate) struct Einstellfenster {
    pub(crate) fenster: Option<crate::prefs::Prefs>,
    pub(crate) merk: crate::prefs::Memory,
    pub(crate) bild_neu: bool,
    pub(crate) aufklapper_neu: bool,
}

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

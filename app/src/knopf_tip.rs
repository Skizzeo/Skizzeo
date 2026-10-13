//! Tooltip an den Werkzeug- und Ansichtsknöpfen (9b), aus `main.rs`
//! ausgelagert.

use crate::ui::Id;
use crate::{help, nordpfeil, sonne_view, App};

impl App {
    /// Tooltip an Werkzeug- und Ansichtsknöpfen (9b): Name, Satz aus
    /// `hilfe.txt`, „F1: mehr“; ein gesperrter Knopf nennt statt des Satzes
    /// den Grund.
    pub(crate) fn button_tip(&self) -> Option<String> {
        let id = self.ui.hover?;
        // Parameter einer Erweiterung: ihre `hilfe` (E7)
        if let Some(h) = self.ui.ext_hilfe(id) {
            return Some(tip_text(vec![h]));
        }
        let nord = nordpfeil::tip(self.ui.nord, self.ui.sonne_an);
        let name = match id {
            Id::Building => "Gebäude",
            Id::Interior => "Innenwand",
            Id::Sprung(_) => "Winkelsprung",
            Id::Ext if self.tool.ext.is_none() => "Erweiterungen",
            Id::Opening(k) => k.name(),
            Id::Quantity => self.ui.frei.knopf(),
            Id::Projektdaten => "Projektdaten",
            Id::Nord => nord.as_str(),
            Id::View(v) => v.knopf(None),
            _ => return None,
        };
        let reason = match id {
            Id::Building if self.ui.upper_active => Some("Außenwände entstehen aus dem EG."),
            Id::Building | Id::Interior | Id::Opening(_) if self.ui.foundation_active => {
                Some("Im Fundament wird nicht gezeichnet.")
            }
            _ => None,
        };
        let mut lines = help::tooltip_lines(name);
        if let (Some(r), true) = (reason, lines.len() == 3) {
            lines[1] = r.into();
        }
        // Mit Nordrichtung heißt die Ansicht nach der Himmelsrichtung, der
        // Satz („Ansicht von vorne.“) bleibt (Sonnenstand S3)
        if let Id::View(v) = id {
            lines[0] = v.titel(self.ui.nord);
        }
        // Ohne Gebäude geht die Sonne nicht an (S9)
        let ohne_haus =
            self.ui.nord.is_some() && !self.ui.sonne_an && self.scene.bounds().is_none();
        if id == Id::Nord && ohne_haus {
            lines.insert(1, sonne_view::OHNE_HAUS.into());
        }
        Some(tip_text(lines))
    }
}

/// Zeilen eines Tooltips als Text für [`sk_ui::widgets::tooltip`] (erste
/// Zeile fett), ohne Fettschrift-Marken.
pub(crate) fn tip_text(lines: Vec<String>) -> String {
    lines
        .iter()
        .map(|l| help::plain(l))
        .collect::<Vec<_>>()
        .join("\n")
}

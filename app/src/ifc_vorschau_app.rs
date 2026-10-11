//! IFC-Vorschau in der App (ifc/plan.md §6): „IFC-Vorschau …“ im Dateimenü
//! schreibt die IFC-Datei wie der Export, liest sie mit dem eigenen Leser
//! wieder ein und zeigt sie in einem freien Fenster
//! ([`crate::ifc_vorschau::fenster`]). Was in der Datei fehlt, fehlt auch
//! hier. Nochmal geöffnet zeigt sie den neuen Stand mit derselben Kamera;
//! nach Neu oder Öffnen baut sie sich für das andere Projekt neu auf.
//! Dazwischen bleibt sie beim Stand des letzten Öffnens.

use crate::ifc_vorschau::fenster::{IfcFenster, Out};
use crate::{meldung, App, NOTICE_TIME};
use sk_platform::layout::WindowId;
use sk_platform::{Event, Surface};

impl App {
    /// Die Vorschau des aktuellen Modells, wie der Export sie schreibt und
    /// der eigene Leser sie liest; `None` mit Meldung, wenn sie nicht lesbar
    /// ist.
    fn ifc_lesen(&mut self) -> Option<IfcFenster> {
        let datei = format!("{}.ifc", self.datei_stamm());
        let opt = sk_ifc::Optionen {
            zeit: 0,
            datei: datei.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
        };
        let (text, _) = sk_ifc::export(self.scene.model(), &opt);
        match sk_ifc::step::read(&text) {
            Ok(f) => {
                let mut neu = IfcFenster::new(sk_ifc::lesen::vorschau(&f), &datei);
                neu.herkunft = self.scene.model().project().guid;
                if let Some(alt) = self.ifc.as_ref() {
                    neu.uebernehmen(alt);
                }
                Some(neu)
            }
            Err(e) => {
                // Kommt nur bei einem Fehler im eigenen Schreiber vor
                let m = meldung::Meldung::mit("IFC-Vorschau: Datei nicht lesbar ({})", &[e.what]);
                self.status(m, NOTICE_TIME);
                None
            }
        }
    }

    /// Menüpunkt „IFC-Vorschau …“: neuer Stand, Fenster öffnen oder nach
    /// vorn holen.
    pub(crate) fn open_ifc_vorschau(&mut self, surface: &Surface) {
        let Some(neu) = self.ifc_lesen() else {
            return;
        };
        surface.open_side(WindowId::Ifc, &neu.title.caption);
        surface.set_side_title(WindowId::Ifc, &neu.title.caption);
        self.ifc = Some(neu);
    }

    /// Ereignis aus dem Vorschaufenster.
    pub(crate) fn handle_ifc(&mut self, e: Event, surface: &Surface) {
        let Some(f) = self.ifc.as_mut() else {
            return;
        };
        match f.handle(&e) {
            Some(Out::Close) => {
                surface.close_side(WindowId::Ifc);
                self.ifc = None;
            }
            Some(Out::Command(c)) => surface.side_command(WindowId::Ifc, c),
            None => {}
        }
    }

    /// Vorschau neu zeigen, wenn sich etwas geändert hat. Nach Neu oder
    /// Öffnen (anderes Projekt) baut sie sich im offenen Fenster neu auf,
    /// ohne es nach vorn zu holen (Review B6 und N3 zu #83); Speichern und
    /// „Speichern unter“ wechseln das Projekt nicht.
    pub(crate) fn sync_ifc(&mut self, surface: &Surface) {
        let Some(f) = self.ifc.as_mut() else {
            return;
        };
        let projekt = self.scene.model().project().guid;
        if f.herkunft != projekt {
            // Auch wenn das Lesen scheitert: nicht bei jedem Bild neu
            f.herkunft = projekt;
            if let Some(neu) = self.ifc_lesen() {
                surface.set_side_title(WindowId::Ifc, &neu.title.caption);
                self.ifc = Some(neu);
            }
        }
        let Some(f) = self.ifc.as_mut() else {
            return;
        };
        let (w, h, area) = (f.w, f.h, f.caption_area());
        if let Some((px, zeilen)) = f.frame(&self.theme, &self.ui.fonts) {
            surface.set_side_caption_area(WindowId::Ifc, area);
            surface.present_side(WindowId::Ifc, w, h, px, zeilen);
        }
    }
}

//! IFC-Vorschau in der App (ifc/plan.md §6): „IFC-Vorschau …“ im Dateimenü
//! schreibt die IFC-Datei wie der Export, liest sie mit dem eigenen Leser
//! wieder ein und zeigt sie in einem freien Fenster
//! ([`crate::ifc_vorschau::fenster`]). Was in der Datei fehlt, fehlt auch
//! hier. Nochmal geöffnet zeigt sie den neuen Stand mit derselben Kamera;
//! nach Neu oder Öffnen baut sie sich für das andere Projekt neu auf.

use crate::ifc_vorschau::fenster::{IfcFenster, Out};
use crate::{meldung, App, NOTICE_TIME};
use sk_platform::layout::WindowId;
use sk_platform::{Event, Surface};

impl App {
    /// Woher die Vorschau stammt: Datei und Projekt. Ändert sich das (Neu,
    /// Öffnen, Speichern unter), zeigt das Fenster nicht mehr das Haus der
    /// App.
    fn ifc_herkunft(&self) -> (Option<std::path::PathBuf>, sk_model::Guid) {
        (self.doc.path.clone(), self.scene.model().project().guid)
    }

    /// Menüpunkt „IFC-Vorschau …“.
    pub(crate) fn open_ifc_vorschau(&mut self, surface: &Surface) {
        let datei = format!("{}.ifc", self.datei_stamm());
        let opt = sk_ifc::Optionen {
            zeit: 0,
            datei: datei.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
        };
        let (text, _) = sk_ifc::export(self.scene.model(), &opt);
        let f = match sk_ifc::step::read(&text) {
            Ok(f) => f,
            Err(e) => {
                // Kommt nur bei einem Fehler im eigenen Schreiber vor
                let m = meldung::Meldung::mit("IFC-Vorschau: Datei nicht lesbar ({})", &[e.what]);
                self.status(m, NOTICE_TIME);
                return;
            }
        };
        let mut neu = IfcFenster::new(sk_ifc::lesen::vorschau(&f), &datei);
        neu.herkunft = self.ifc_herkunft();
        if let Some(alt) = self.ifc.take() {
            neu.uebernehmen(&alt);
        }
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

    /// Vorschau neu zeigen, wenn sich etwas geändert hat; nach einem
    /// Projektwechsel neu aufbauen (Review B6 zu #83).
    pub(crate) fn sync_ifc(&mut self, surface: &Surface) {
        let Some(f) = self.ifc.as_ref() else {
            return;
        };
        let (pfad, guid) = &f.herkunft;
        if pfad.as_deref() != self.doc.path.as_deref() || *guid != self.scene.model().project().guid
        {
            self.open_ifc_vorschau(surface);
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

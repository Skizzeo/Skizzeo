//! Freies Fenster der IFC-Vorschau (ifc/plan.md §6): eigenes Programmfenster
//! mit Taskleisteneintrag, nicht angedockt. Es minimiert und zeigt sich für
//! sich, nicht mit dem Hauptfenster. Es öffnet mittig im Arbeitsbereich des
//! Hauptfensters; eine Lage, die der Nutzer gewählt hat, gilt beim nächsten
//! Öffnen wieder, solange sie auf diesem Bildschirm liegt.

use super::*;

/// Größe beim ersten Öffnen (dip), höchstens 90 % des Arbeitsbereichs.
pub const IFC_SIZE_DIP: (f32, f32) = (1400.0, 880.0);

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Frei {
    /// Das Fenster gibt es gerade.
    pub(super) shown: bool,
    /// Zuletzt gemeldete Lage (nicht maximiert).
    rect: Option<Rect>,
}

impl Windows {
    pub fn ifc_open(&self) -> bool {
        self.frei.shown
    }

    /// Menüpunkt „IFC-Vorschau …“: anlegen oder nach vorn holen.
    pub fn open_ifc(&mut self, work: Rect, scale: f32) -> Vec<Action> {
        if self.frei.shown {
            return vec![Action::Front(WindowId::Ifc)];
        }
        self.frei.shown = true;
        let r = self
            .frei
            .rect
            .filter(|&r| inside(r, work))
            .unwrap_or_else(|| {
                let w = ((IFC_SIZE_DIP.0 * scale) as i32).min(work.2 * 9 / 10);
                let h = ((IFC_SIZE_DIP.1 * scale) as i32).min(work.3 * 9 / 10);
                (work.0 + (work.2 - w) / 2, work.1 + (work.3 - h) / 2, w, h)
            });
        vec![Action::Create(WindowId::Ifc, r)]
    }

    /// Der Nutzer hat das Fenster verschoben oder in der Größe geändert.
    pub fn ifc_moved(&mut self, r: Rect) {
        self.frei.rect = Some(r);
    }

    pub(super) fn close_ifc(&mut self) -> Vec<Action> {
        if !std::mem::take(&mut self.frei.shown) {
            return Vec::new();
        }
        vec![Action::Close(WindowId::Ifc)]
    }

    /// Vom System geschlossen.
    pub fn ifc_gone(&mut self) {
        self.frei.shown = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = (0, 0, 1920, 1040);

    /// Mittig, höchstens 90 % groß; nochmal öffnen holt nach vorn; die
    /// gewählte Lage gilt beim nächsten Mal.
    #[test]
    fn oeffnet_mittig_und_merkt_die_lage() {
        let mut w = Windows::new(520.0);
        assert_eq!(
            w.open_ifc(WORK, 1.0),
            [Action::Create(WindowId::Ifc, (260, 80, 1400, 880))]
        );
        assert!(w.ifc_open());
        assert_eq!(w.open_ifc(WORK, 1.0), [Action::Front(WindowId::Ifc)]);
        w.ifc_moved((100, 50, 900, 700));
        assert_eq!(
            w.close_requested(WindowId::Ifc, false),
            [Action::Close(WindowId::Ifc)]
        );
        assert!(w.close_requested(WindowId::Ifc, false).is_empty());
        assert_eq!(
            w.open_ifc(WORK, 1.0),
            [Action::Create(WindowId::Ifc, (100, 50, 900, 700))]
        );
        w.ifc_gone();
        // Bei 200 % höchstens 90 % des Arbeitsbereichs
        assert_eq!(
            Windows::new(520.0).open_ifc(WORK, 2.0),
            [Action::Create(WindowId::Ifc, (96, 52, 1728, 936))]
        );
    }

    /// Minimieren betrifft das freie Fenster allein und nimmt es nicht mit;
    /// das Hauptfenster schließt es mit.
    #[test]
    fn minimiert_fuer_sich_und_geht_mit_dem_hauptfenster() {
        let mut w = Windows::new(520.0);
        w.open_ifc(WORK, 1.0);
        assert!(w.minimized(WindowId::Ifc).is_empty());
        assert!(w.restored(WindowId::Ifc).is_empty());
        assert!(!w
            .minimized(WindowId::Main)
            .contains(&Action::Show(WindowId::Ifc, Show::Minimize)));
        assert!(w.restored(WindowId::Main).is_empty());
        assert_eq!(
            w.close_requested(WindowId::Main, false),
            [Action::Close(WindowId::Ifc), Action::Close(WindowId::Main)]
        );
        assert!(!w.ifc_open());
    }
}

//! Das Geländenetz im Faden (Review #234 B-234b): Verschieben oder Setzen
//! eines Höhenpunkts teilt das Netz neu, bei 100.000 Punkten eine halbe
//! Sekunde. Der Schritt lässt das alte Netz vorläufig stehen; ein Faden
//! rechnet das neue, im Leerlauf setzt es die Szene ein. Gründungen und
//! ±0,00 folgen ihm dann wie ohne Faden, angehängt an den letzten Schritt
//! im Verlauf, damit ein „Rückgängig“ beides zurücknimmt.

use super::{raum, Scene};
use sk_model::{Model, NetzAuftrag, NetzFertig};
use std::sync::mpsc;

/// Ein laufender Faden: für welches Gelände, und wo sein Netz ankommt.
pub(super) type NetzFaden = (NetzAuftrag, mpsc::Receiver<NetzFertig>);

impl Scene {
    /// Nach einem Schritt, Rückgängig oder Wiederholen: Hat das Gelände
    /// nur ein vorläufiges Netz, rechnet ein Faden das richtige. Läuft
    /// noch einer für ein älteres Gelände, kommt der nächste nach ihm.
    pub(super) fn netz_anstossen(&mut self) {
        if self.netz_faden.is_some() {
            return;
        }
        let Some(a) = self.model.netz_auftrag() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let auftrag = a.clone();
        std::thread::spawn(move || {
            let _ = tx.send(auftrag.rechnen());
        });
        self.netz_faden = Some((a, rx));
    }

    /// Rechnet gerade ein Faden ein Netz?
    pub fn netz_rechnet(&self) -> bool {
        self.netz_faden.is_some()
    }

    /// Im Leerlauf: das Netz aus dem Faden einsetzen, wenn es da ist.
    /// `true`, wenn sich das Gelände geändert hat (Bild und Mengen folgen).
    pub fn netz_abholen(&mut self) -> bool {
        if self.model.in_step() {
            return false;
        }
        let Some((_, rx)) = &self.netz_faden else {
            return false;
        };
        let fertig = match rx.try_recv() {
            Ok(f) => Some(f),
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        self.netz_faden = None;
        self.netz_einsetzen(fertig)
    }

    /// Wartet auf den Faden und setzt sein Netz ein.
    pub fn netz_abwarten(&mut self) -> bool {
        let mut neu = false;
        while let Some((_, rx)) = self.netz_faden.take() {
            neu |= self.netz_einsetzen(rx.recv().ok());
        }
        neu
    }

    /// Hat das Gelände nur ein vorläufiges Netz, auf das fertige warten
    /// (außerhalb eines Schritts). Mengen, Kosten, Speichern und IFC rufen
    /// das vorher: Sie dürfen nichts aus dem alten Netz ausgeben, sonst
    /// stimmten sie nicht mit dem Stand nach dem Öffnen überein
    /// (Integration zu #234 B-234b).
    pub fn netz_fertig(&mut self) {
        if !self.model.in_step() {
            self.netz_anstossen();
            self.netz_abwarten();
        }
    }

    /// Das Modell mit fertigem Netz ([`Scene::netz_fertig`]), zum
    /// Speichern und für IFC.
    pub fn model_fertig(&mut self) -> &Model {
        self.netz_fertig();
        &self.model
    }

    /// Das Netz einsetzen. Was Gründungen und ±0,00 dabei ändern, kommt
    /// zum letzten Schritt im Verlauf; der Verlauf zum Wiederholen bleibt.
    /// Danach der nächste Faden, falls das Gelände inzwischen ein anderes
    /// ist.
    fn netz_einsetzen(&mut self, fertig: Option<NetzFertig>) -> bool {
        let mut neu = false;
        if let Some(f) = fertig {
            self.model.begin("Gelände");
            neu = self.model.netz_einsetzen(f);
            match self.model.try_commit() {
                Ok(Some(txn)) => {
                    self.ausbau.merken(&self.model, &txn.changes);
                    match self.undo.last_mut() {
                        Some(s) => s.txn.changes.extend(txn.changes),
                        None => self.undo.push(raum::Schritt {
                            txn,
                            raeume: Vec::new(),
                        }),
                    }
                    self.mark_all();
                }
                Ok(None) => {}
                Err(sk_model::Locked(id)) => {
                    self.locked_hit = Some(id);
                    self.mark_all();
                }
            }
            self.rebuild_dirty(false);
            // Ein Faden, der ohne Netz endet, wird nicht gleich neu
            // gestartet; sonst liefe [`Scene::netz_abwarten`] im Kreis.
            self.netz_anstossen();
        }
        neu
    }
}

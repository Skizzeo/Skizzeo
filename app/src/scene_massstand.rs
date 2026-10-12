//! Stand der Bemaßung je Geschoss (Review W30 zu #116): Jede Änderung an
//! einem Wandzug gibt seinem Geschoss einen neuen Stand, so rechnet die App
//! beim Ziehen nur dessen Maßketten neu ([`crate::bemassung::Massketten`]).
//! Öffnungen und Anschlüsse markieren ihre Wandzüge mit, Geschosse und
//! Aufbauten alle. Aus `scene.rs` ausgelagert.

use super::Scene;
use sk_model::{RunId, StoreyId};
use std::sync::atomic::{AtomicU64, Ordering};

/// Über alle Szenen eindeutig: eine neu geöffnete Datei trifft keinen
/// gemerkten Stand der vorigen.
static STAND: AtomicU64 = AtomicU64::new(1);

/// Eine neue, eindeutige Nummer.
pub fn neuer_stand() -> u64 {
    STAND.fetch_add(1, Ordering::Relaxed)
}

impl Scene {
    /// Der Wandzug `run` wird neu gerechnet: sein Geschoss bekommt einen
    /// neuen Stand; gibt es den Zug nicht mehr, alle.
    pub(super) fn mass_zug(&mut self, run: RunId) {
        match self.model.run(run) {
            Some(r) => {
                self.mass_stand.insert(r.storey, neuer_stand());
            }
            None => self.mass_alle = neuer_stand(),
        }
    }

    /// Stand der Maßketten im Geschoss `st`: neu mit jeder Änderung an
    /// seinen Wandzügen, Öffnungen und Anschlüssen. Stil und Versätze
    /// stehen im Modell und zählen eigens.
    pub fn mass_stand(&self, st: StoreyId) -> (u64, u64) {
        (
            self.mass_alle,
            self.mass_stand.get(&st).copied().unwrap_or(0),
        )
    }
}

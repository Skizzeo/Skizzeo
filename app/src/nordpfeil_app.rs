//! Der Nordpfeil in der App (Sonnenstand S2, S9): Ereignisse, die
//! geführte Eingabe nach dem ersten Umriss und der Schritt beim Setzen.
//! Aus `main.rs` hierher, damit es unter seiner Grenze bleibt.

use crate::{meldung, nordpfeil, sonne_view, App, NOTICE_TIME, OVERLAY_NOTICE};
use sk_platform::Event;

impl App {
    /// Stand des Nordpfeils im Modell (Sonnenstand S2).
    pub(crate) fn nord_stand(&self, vh: f64, sc: f64) -> nordpfeil::Stand {
        let m = self.scene.model();
        nordpfeil::Stand {
            nord: m.location().north,
            fuss: nordpfeil::platz(&self.cam, vh, sc, m.north_foot(), self.scene.bounds()),
            gesetzt: m.north_foot(),
        }
    }

    /// Ereignis an den Nordpfeil (3D und Grundriss, ohne Wandeingabe und
    /// ohne gegriffene Schnittlinie); `true`, wenn er es genommen hat.
    pub(crate) fn nord_handle(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) -> bool {
        // Das Werkzeug „Fläche zeichnen“ (G3d) geht vor
        if self.flaeche_handle(ev, vw, vh, sc) {
            return true;
        }
        let en = self.tool_allowed() && !self.tool.enabled && !self.sect.is_busy();
        let st = self.nord_stand(vh, sc);
        self.nord.gebaeude = self.scene.bounds();
        let out = self.nord.handle(ev, st, &self.cam, vw, vh, sc, en);
        self.redraw |= out.redraw;
        self.nord_commit(out.commit);
        // Klick auf den Pfeil schaltet den Sonnenstand (§8 09:25)
        if out.klick {
            self.sonne_umschalten(true);
        }
        out.consumed
    }

    /// Nach dem ersten geschlossenen Umriss ohne Nordrichtung startet der
    /// Nordpfeil einmal je Projekt von selbst (S9); Esc heißt „später“.
    pub(crate) fn nord_fuehren(&mut self, umriss: bool, erster: bool) {
        let haus = self.scene.bounds().is_some();
        let nord = self.scene.model().location().north;
        let erlaubt = self.tool_allowed();
        if !self
            .nord_fuehrung
            .nach_wandzug(umriss, erster, haus, nord, erlaubt)
        {
            return;
        }
        self.tool.set_enabled(false);
        self.nord.set_aktiv(true);
        self.nord_zeile();
        self.overlay_dirty = true;
        self.redraw = true;
    }

    /// Zeile des geführten Nordpfeils in der Statuszeile; sie steht, solange
    /// das Werkzeug an ist (§8 13:50), auch nach einem anderen Hinweis.
    pub(crate) fn nord_zeile(&mut self) {
        self.status(
            meldung::Meldung::satz(nordpfeil::GEFUEHRT),
            std::time::Duration::from_secs(3600),
        );
    }

    /// Der geführte Nordpfeil ist zu Ende (gesetzt oder Esc): seine Zeile
    /// geht aus der Statuszeile.
    pub(crate) fn nord_gefuehrt_ende(&mut self) {
        if self
            .notice
            .as_ref()
            .is_some_and(|n| n.text == nordpfeil::GEFUEHRT)
        {
            self.notice = None;
            self.renderer.set_overlay(OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
            self.redraw = true;
        }
    }

    /// Ein Schritt „Nordrichtung geändert“ bzw. „Nordpfeil verschoben“.
    /// Nach dem ersten Aufziehen ist der Sonnenstand an (§8 09:25), ohne
    /// Gebäude noch nicht (S9).
    pub(crate) fn nord_commit(&mut self, c: Option<nordpfeil::Setzen>) {
        if let Some((label, n, f)) = c {
            let neu = self.scene.model().location().north.is_none();
            if self.scene.nordpfeil_setzen(label, n, f) {
                self.overlay_dirty = true;
                // An, nicht um (Befund D); geführt aus dem Grundriss nach 3D
                // wie bei der Kachel
                if neu {
                    let w = sonne_view::erstes_setzen(self.scene.bounds().is_some());
                    self.sonne_wahl(w, self.nord_fuehrung.laeuft);
                }
            }
            self.redraw = true;
        }
    }
}

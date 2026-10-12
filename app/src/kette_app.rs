//! Kette und Fuge in der App: die Kettensymbole der gestapelten Wände (OG
//! Phase 2) und der Fugen (W-11), „Wand teilen“ und „vereinen“ (W-12,
//! Jörn 11.10.). Gleicher Ablauf wie an der Kopplung zweier Geschosse:
//! Graue, offene Kette heißt getrennt, ein Klick schließt sie. Dazu „Ecke
//! bilden“ und „Verlängern“ an der gewählten Wand (Grundriss G5).
//!
//! Die Entscheidungen stehen in [`sk_model::Model::split_wall`],
//! [`sk_model::Model::merge_walls`] und [`crate::teilen_edit`]; hier nur
//! Karte, Statuszeile und Auswahl.

use crate::delete::{self, HintCard, Link};
use crate::ui::Id;
use crate::wall_edit::{breite_edit, ecke_edit, EditOutcome};
use crate::{link_view, meldung, relink_lines, teilen_edit, App, NOTICE_TIME};
use sk_model::{ElementId, FugeError};
use sk_platform::MouseButton;
use std::time::{Duration, Instant};

/// Rückfrage nach dem bündigen Zusammenschieben (W-11).
const JOINED: [&str; 2] = [
    "In einer Flucht.",
    "Die Wände stoßen jetzt bündig aneinander.",
];
/// So lange steht die Rückfrage (unter der Maus länger).
const ASK_TIME: Duration = Duration::from_secs(12);
/// Nach dem Teilen am gewählten Teilstück.
const SPLIT: [&str; 2] = [
    "Wand geteilt.",
    "Teilstück am Band rein- oder rausziehen: Rück- oder Vorsprung.",
];

impl App {
    /// Die Kettensymbole im Bild: gestapelte Wände, Fugen und im Grundriss
    /// die Breitenschlösser an den Öffnungsenden bei der Maus.
    pub(crate) fn ketten_chips(
        &self,
        vw: f64,
        vh: f64,
        top: f64,
        scale: f64,
    ) -> Vec<link_view::Chip> {
        let plan_z = self.plan_z();
        let schloesser = match plan_z {
            Some(z) if !self.edit.is_dragging() => {
                breite_edit::schloesser(&self.scene, self.edit.oeffnungs_enden(), z)
            }
            _ => Vec::new(),
        };
        link_view::chips(&link_view::Want {
            scene: &self.scene,
            cam: &self.cam,
            w: vw,
            h: vh,
            top,
            scale,
            plan_z,
            band: self.edit.active_wall(),
            hover: self.chip_hover,
            keep_linked: self.flush_keep,
            schloesser,
        })
    }

    /// Hinweis am Kettensymbol unter der Maus.
    pub(crate) fn chain_tip(&self) -> Option<String> {
        if let Some(c) = self.chips.iter().find(|c| c.schloss.is_some() && c.hover) {
            return Some(link_view::schloss_tip(c.linked).into());
        }
        let (wall, fuge) = self.chip_hover?;
        if fuge.is_some() {
            return Some(link_view::FUGE_TIP.into());
        }
        let (_, linked) = self.scene.model().stack_offset(wall)?;
        Some(link_view::tip(linked).to_string())
    }

    /// Klick auf ein Kettensymbol: löst bzw. koppelt sofort, ohne Rückfrage
    /// (OG Phase 2); an einer Fuge vereint er die beiden Wände. Wieder
    /// gekoppelt mit Versatz: Hinweis mit „Bündig setzen“. `true`, wenn der
    /// Klick dem Symbol galt.
    pub(crate) fn press_chip(&mut self, button: MouseButton, x: f64, y: f64, sc: f64) -> bool {
        if button != MouseButton::Left || self.tool.is_active() || self.edit.splitting() {
            return false;
        }
        let Some(&chip) = link_view::chip_at(&self.chips, x, y, sc) else {
            return false;
        };
        // Breitenschloss am Öffnungsende: mit allen Partnern koppeln bzw. lösen
        if let Some(k) = chip.schloss {
            let partner = breite_edit::partner(self.scene.model(), chip.wall, k);
            // Gesperrte Öffnungen bleiben, wie sie sind (Review #109 B5)
            let mut ids = vec![chip.wall];
            ids.extend(partner.iter().map(|p| p.0));
            if let Some(l) = sk_model::edit_blocked(self.scene.model(), &ids) {
                self.show_locked(l, Some((chip.wall, delete::Act::Field)));
                return true;
            }
            if self
                .scene
                .set_end_locks(chip.wall, k, &partner, !chip.linked)
            {
                self.redraw = true;
            }
            return true;
        }
        let (wall, fuge) = chip.glied();
        match fuge {
            Some(a) => self.merge(a, Some(wall)),
            None => self.toggle_link(wall),
        }
        true
    }

    /// Kette der gestapelten Wand umschalten (Symbol im Plan oder Paneel).
    pub(crate) fn toggle_link(&mut self, wall: ElementId) {
        let Some((offset, linked)) = self.scene.model().stack_offset(wall) else {
            return;
        };
        if self.scene.set_linked(wall, !linked) {
            self.upload_model();
            self.sync_props();
            // Neben dem Paar, damit die Karte keine der Wände verdeckt (E20 §6.4)
            let pair = [Some(wall), self.scene.model().wall_below(wall)];
            self.hint = (!linked && offset != 0.0).then(|| {
                let mut h = HintCard::new(
                    relink_lines(offset),
                    Some(("Bündig setzen", Link::Flush(wall))),
                    pair.into_iter().flatten().collect(),
                    Instant::now(),
                );
                h.beside = true;
                h
            });
            self.hint_dirty = true;
            self.redraw = true;
        }
    }

    /// Knöpfe der Wand im Paneel „Eigenschaften“: Kette, „bündig setzen“,
    /// „Wand teilen“, „an der Fuge vereinen“.
    pub(crate) fn kette_klick(&mut self, id: Id) {
        let Some(sel) = self.sel.id else {
            return;
        };
        let stacked = self.scene.stack_wall(sel);
        match id {
            Id::PropsLink => stacked.into_iter().for_each(|w| self.toggle_link(w)),
            Id::PropsFlush => stacked.into_iter().for_each(|w| self.flush(w)),
            Id::PropsSplit => self.split_start(sel),
            Id::PropsMerge => self.merge(sel, None),
            Id::PropsEcke => self.ecke_start(sel, ecke_edit::Art::Ecke),
            Id::PropsVerlaengern => self.ecke_start(sel, ecke_edit::Art::Verlaengern),
            _ => {}
        }
    }

    /// „Wand teilen“ beginnen: Karte neben der Wand, bis geteilt oder
    /// abgebrochen ist.
    fn split_start(&mut self, wall: ElementId) {
        let m = self.scene.model();
        let step = m.step_joint(wall).is_some();
        if let Some(b) = sk_model::edit_blocked(m, &m.stack_column(wall)) {
            self.show_locked(b, Some((wall, delete::Act::Field)));
            return;
        }
        if step {
            self.fuge_refused(FugeError::Step);
            return;
        }
        if !self.edit.start_split(&self.scene, wall) {
            self.fuge_refused(FugeError::TooShort);
            return;
        }
        let lines = teilen_edit::CARD.map(String::from).to_vec();
        let mut h = HintCard::new(lines, None, vec![wall], Instant::now());
        h.beside = true;
        h.hold();
        self.hint = Some(h);
        self.hint_dirty = true;
        self.refresh_cursor();
        self.redraw = true;
    }

    /// „Ecke bilden“ bzw. „Verlängern“ beginnen (G5): Karte neben der Wand,
    /// bis das Werkzeug fertig ist.
    fn ecke_start(&mut self, wall: ElementId, art: ecke_edit::Art) {
        let m = self.scene.model();
        if let Some(b) = sk_model::edit_blocked(m, &m.stack_column(wall)) {
            self.show_locked(b, Some((wall, delete::Act::Field)));
            return;
        }
        if let Err(e) = self.edit.start_ecke(&self.scene, wall, art) {
            self.fehler(e.message());
            return;
        }
        let mut h = HintCard::new(ecke_edit::karte(art), None, vec![wall], Instant::now());
        h.beside = true;
        h.hold();
        self.hint = Some(h);
        self.hint_dirty = true;
        self.refresh_cursor();
        self.redraw = true;
    }

    /// Folgen von „Ecke bilden“ und „Verlängern“: Schritt, neue Karte,
    /// Ablehnung, Ende.
    fn after_ecke(&mut self, o: &ecke_edit::Outcome) {
        if let Some(e) = o.refused {
            self.fehler(e.message());
        }
        if let Some((id, wall)) = o.locked {
            self.show_locked(id, Some((wall, delete::Act::Field)));
        }
        if let Some(wall) = o.schritt {
            self.upload_model();
            self.select(Some(wall));
        }
        if let Some(lines) = &o.karte {
            let mut h = HintCard::new(lines.clone(), None, Vec::new(), Instant::now());
            h.beside = true;
            h.hold();
            self.hint = Some(h);
        }
        if o.fertig {
            self.hint = None;
        }
        if o.cancelled {
            self.status(meldung::Meldung::satz(ecke_edit::CANCELLED), NOTICE_TIME);
        }
        self.hint_dirty = true;
        self.redraw = true;
    }

    /// Folgen am Band: geteilt, Teilen abgebrochen oder abgelehnt, bündig
    /// zusammengeschoben (dann die Rückfrage).
    pub(crate) fn after_edit(&mut self, out: &EditOutcome) {
        if let Some(o) = &out.ecke {
            if o != &ecke_edit::Outcome::default() {
                self.after_ecke(o);
            }
        }
        if let Some(t) = &out.teilen {
            if let Some(e) = t.refused {
                self.fuge_refused(e);
            }
            if let Some((id, wall)) = t.locked {
                self.show_locked(id, Some((wall, delete::Act::Field)));
            }
            if t.done.is_some() || t.cancelled {
                self.hint = None;
                self.hint_dirty = true;
                self.redraw = true;
            }
            if let Some(piece) = t.done {
                self.upload_model();
                self.select(Some(piece));
                let lines = SPLIT.map(String::from).to_vec();
                let mut h = HintCard::new(lines, None, vec![piece], Instant::now());
                h.beside = true;
                self.hint = Some(h);
            }
            if t.cancelled {
                self.status(meldung::Meldung::satz(teilen_edit::CANCELLED), NOTICE_TIME);
            }
        }
        if let Some(wall) = out.joined {
            let mut h = HintCard::new(
                JOINED.map(String::from).to_vec(),
                Some(("Zu einer Wand vereinen", Link::Merge(wall))),
                vec![wall],
                Instant::now(),
            );
            h.link2 = Some(("Getrennt lassen", Link::Dismiss));
            h.beside = true;
            h.stay(ASK_TIME);
            self.hint = Some(h);
            self.hint_dirty = true;
            self.redraw = true;
        }
    }

    /// Vereinen: `a` mit `b` an ihrer Fuge, ohne `b` mit allen Nachbarn in
    /// der Flucht (ein Schritt „Wände vereint“).
    pub(crate) fn merge(&mut self, a: ElementId, b: Option<ElementId>) {
        let m = self.scene.model();
        let set: Vec<ElementId> = match b {
            Some(b) => vec![a, b],
            None => m
                .segment_of(a)
                .map(|(run, _)| m.straight_joints(run))
                .unwrap_or_default()
                .into_iter()
                .filter(|j| j.0 == a || j.1 == a)
                .flat_map(|j| [j.0, j.1])
                .collect(),
        };
        // Vereint wird der ganze Stapel
        let set: Vec<ElementId> = set.iter().flat_map(|w| m.stack_column(*w)).collect();
        if let Some(id) = sk_model::edit_blocked(m, &set) {
            self.show_locked(id, Some((a, delete::Act::Field)));
            return;
        }
        let mut res = Err(FugeError::NotJoined);
        self.scene.edit_model("Wände vereint", |m| {
            res = match b {
                Some(b) => m.merge_walls(a, b).map(|_| 1),
                None => m.merge_around(a),
            };
            res.is_ok()
        });
        match res {
            Ok(_) => {
                self.hint = None;
                self.hint_dirty = true;
                self.upload_model();
                self.sync_props();
                self.status(
                    meldung::Meldung::satz("Zu einer Wand vereint."),
                    NOTICE_TIME,
                );
            }
            Err(e) => self.fuge_refused(e),
        }
    }

    /// Statuszeile mit dem Grund, als Fehler.
    fn fuge_refused(&mut self, e: FugeError) {
        self.fehler(e.message());
    }

    /// Statuszeile mit einem Satz, als Fehler.
    fn fehler(&mut self, satz: &'static str) {
        self.status(meldung::Meldung::satz(satz), NOTICE_TIME);
        if let Some(n) = self.notice.as_mut() {
            n.error = true;
        }
    }
}

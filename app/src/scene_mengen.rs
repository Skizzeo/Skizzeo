//! Mengenliste je Gebäude gemerkt (Review M2, Architektur 16.10.): Nach
//! einem Schritt rechnet die Szene nur die Gebäude neu, die er laut
//! Bau-Log ([`sk_model::Model::gebaeude_seit`]) berührt, und führt sie mit
//! den gemerkten Teilen der übrigen zusammen ([`sk_model::qto::Teil`]).
//! Unbekannt (Laden, Änderung ohne Schritt, ein Schritt für alle) rechnet
//! alles. In Tests mit Prüfungen (Debug) vergleicht sich jede so
//! entstandene Liste mit der vollen Rechnung.

use crate::scene::Scene;
use sk_model::model::BauStand;
use sk_model::qto::{self, RoomFn, Schedule, Teil, ZugFn};
use sk_model::{BuildingId, Model, RunId};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

/// Die Teile der letzten Liste mit dem Stand des Bau-Logs, aus dem sie
/// stammen, und der Liste selbst, die ihre Gebäude trägt.
#[derive(Default)]
pub(crate) struct MengenTeile {
    teile: RefCell<Option<(BauStand, Vec<Teil>)>>,
    /// Die Liste aus den Teilen ([`MengenTeile::merken`]).
    liste: RefCell<Weak<Schedule>>,
    /// Die bisherige Liste der Szene für die nächste ([`Scene::mengen_neu`]).
    alt: RefCell<Option<Rc<Schedule>>>,
    /// Wie viele Teile zuletzt neu gerechnet wurden (Tests, Messung).
    neu: Cell<usize>,
}

impl Scene {
    /// Neue Mengenliste der Szene ([`Scene::schedule`]): Die bisherige gibt
    /// ihre Gebäude ohne Kopie an die neue ab, wenn nur die Szene sie hält.
    pub(crate) fn mengen_neu(&mut self) -> Rc<Schedule> {
        let alt = self.schedule.take().map(|(_, s)| s);
        self.ausbau.mengen.alt_geben(alt);
        let s = Rc::new(self.mengenliste());
        self.ausbau.mengen.merken(&s);
        s
    }

    /// Neue Dicke der Decke über dem Zug `run`: Er, die Innenwände unter
    /// der Decke und alle Züge darüber samt Aufkantung rechnen neu. Seit
    /// #242 nimmt die Mengenliste die Wandmengen aus dem Zwischenspeicher
    /// der Szene, und die Züge darüber hängen an der Decke (Test B-242a).
    pub(crate) fn decke_markieren(&mut self, run: RunId) {
        let m = &self.model;
        let mut zuege = vec![run];
        zuege.extend(m.runs_under_floor(run));
        zuege.extend(m.stack_above(run));
        for r in zuege {
            self.mark(r);
        }
    }
}

impl MengenTeile {
    /// Die bisherige Liste für die nächste [`MengenTeile::liste`]; nur die,
    /// die aus den gemerkten Teilen stammt.
    pub(crate) fn alt_geben(&self, alt: Option<Rc<Schedule>>) {
        let eigen = |a: &Rc<Schedule>| std::ptr::eq(Rc::as_ptr(a), self.liste.borrow().as_ptr());
        *self.alt.borrow_mut() = alt.filter(eigen);
    }

    /// Merkt sich die Liste `s` aus der letzten [`MengenTeile::liste`].
    pub(crate) fn merken(&self, s: &Rc<Schedule>) {
        *self.liste.borrow_mut() = Rc::downgrade(s);
    }

    /// Mengenliste wie [`qto::schedule_mit_zuegen`], mit den gemerkten
    /// Teilen der Gebäude, die seit ihnen kein Schritt berührt hat.
    pub(crate) fn liste(&self, m: &Model, raeume: RoomFn, zuege: ZugFn) -> Schedule {
        let stand = m.bau_stand();
        let alt = self.alt.take();
        *self.liste.borrow_mut() = Weak::new();
        let gemerkt = self.teile.borrow_mut().take();
        let neu = gemerkt.as_ref().and_then(|(s, _)| m.gebaeude_seit(*s));
        let mut nachgefuehrt = None;
        if let (Some((_, mut teile)), Some(neu), Some(alt)) = (gemerkt, neu, alt) {
            teile.retain(|t| !neu.contains(&t.gebaeude()));
            // Jedes Gebäude und die Geschosse ohne genau einmal, gemerkt
            // oder neu, und die alte Liste trägt die gemerkten; sonst alles
            // neu. Vor dem Rechnen geprüft, damit ein Abruf die Räume eines
            // Geschosses nur einmal holt (Review H-193e, Grundriss 16.10.)
            if vollzaehlig(m, &teile, &neu) && qto::zusammenfuehrbar(Some(&alt), &teile) {
                let frisch = qto::teile(m, raeume, zuege, &|b| neu.contains(&b));
                self.neu.set(frisch.len());
                teile.extend(frisch);
                debug_assert_eq!(teile.len(), m.buildings().len() + 1);
                let s = match Rc::try_unwrap(alt) {
                    Ok(a) => qto::zusammenfuehren(m, Some(Cow::Owned(a)), &mut teile),
                    Err(a) => qto::zusammenfuehren(m, Some(Cow::Borrowed(&a)), &mut teile),
                };
                let s = s.expect("geprüft");
                pruefen(m, raeume, zuege, &s);
                nachgefuehrt = Some((s, teile));
            }
        }
        let (s, teile) = nachgefuehrt.unwrap_or_else(|| {
            let mut teile = qto::teile(m, raeume, zuege, &|_| true);
            self.neu.set(teile.len());
            let s = qto::zusammenfuehren(m, None, &mut teile).expect("alle Teile frisch");
            (s, teile)
        });
        *self.teile.borrow_mut() = Some((stand, teile));
        s
    }

    /// Wie viele Teile die letzte Liste neu gerechnet hat.
    #[cfg(test)]
    pub(crate) fn zuletzt_neu(&self) -> usize {
        self.neu.get()
    }
}

/// Ob die gemerkten Teile `teile` mit den neu zu rechnenden Gebäuden `neu`
/// jedes Gebäude und die Geschosse ohne genau einmal ergeben.
fn vollzaehlig(m: &Model, teile: &[Teil], neu: &[Option<BuildingId>]) -> bool {
    let da = |b: Option<BuildingId>| teile.iter().filter(|t| t.gebaeude() == b).count();
    let soll = m.buildings().ids().map(Some).chain([None]);
    teile.len()
        + neu
            .iter()
            .filter(|b| b.is_none_or(|b| m.building(b).is_some()))
            .count()
        == m.buildings().len() + 1
        && soll
            .into_iter()
            .all(|b| da(b) + usize::from(neu.contains(&b)) == 1)
}

/// In Tests mit Prüfungen (Debug): Die aus gemerkten Teilen
/// zusammengeführte Liste ist die der vollen Rechnung, samt Schichtzeilen.
/// Die Messungen laufen ohne (Release).
#[cfg(all(test, debug_assertions))]
fn pruefen(m: &Model, raeume: RoomFn, zuege: ZugFn, s: &Schedule) {
    let voll = qto::schedule_mit_zuegen(m, raeume, zuege);
    assert_eq!(*s, voll, "gemerkte Teile weichen von der vollen Liste ab");
    assert_eq!(*s.layer_rows_ref(m), *voll.layer_rows_ref(m));
}

#[cfg(not(all(test, debug_assertions)))]
fn pruefen(_: &Model, _: RoomFn, _: ZugFn, _: &Schedule) {}

#[cfg(test)]
#[path = "scene_mengen_tests.rs"]
mod tests;

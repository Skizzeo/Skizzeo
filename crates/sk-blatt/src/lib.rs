//! Die Blätter des Mengenfensters und ihre Helfer (Paket T5,
//! `architektur/t5-kostenblaetter-entwurf.md`): Kartenleiste, Auswahlleiste
//! der Kosten- und AVA-Blätter und die kleinen Malhilfen der Fenster.
//!
//! Die Crate kennt die App nicht. Was nur die App weiß (welche Blätter
//! frei sind, die Szene), kommt als Daten oder über einen Trait herein.
//! Für `sk_cost` gelten hier dieselben Regeln wie in `app/`
//! (`architektur/bausteingrenze-sk-cost.md` §8).

#![forbid(unsafe_code)]

pub mod cards;
pub mod fokus;
pub mod window_kit;

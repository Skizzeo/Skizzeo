//! Die Blätter des Mengenfensters und ihre Helfer (Paket T5,
//! `architektur/t5-kostenblaetter-entwurf.md`): Kartenleiste, Auswahlleiste
//! der Kosten- und AVA-Blätter und die kleinen Malhilfen der Fenster.
//!
//! Die Crate kennt die App nicht. Was nur die App weiß (Freischaltung,
//! Szene), kommt als Daten oder über einen Trait herein.

#![forbid(unsafe_code)]

pub mod cards;
pub mod fokus;
pub mod window_kit;

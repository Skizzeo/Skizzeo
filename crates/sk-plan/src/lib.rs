//! Plangrafik: Bemaßung nach DIN 1356-1 bzw. DIN EN ISO 7519 für Grundrisse
//! 1:100 (Konzept `bemassung/konzept.md`).
//!
//! Die Ketten werden nie gespeichert, sondern immer aus dem Modell gerechnet
//! ([`aussen`]); gespeichert sind nur Stil und Versätze im Modell. [`bild`]
//! macht daraus Linien, Begrenzungen und Schrift in Modellkoordinaten, flach
//! auf der Geschossebene, für den Grundriss wie für 3D.

#![forbid(unsafe_code)]

pub mod aussen;
pub mod bild;
pub mod schrift;
pub mod zahl;

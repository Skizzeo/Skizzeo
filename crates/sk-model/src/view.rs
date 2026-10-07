//! Sichtbarkeit (Paket 3, projektstruktur/paket-3-sichtbarkeit.md §3.4):
//! Bauteile, Bauteilarten, Gewerke und das Gelände ausblenden, einen Ast
//! isolieren. Ansichtszustand wie die Schnitte: kein Rückgängig-Schritt,
//! keine neue Revision, ändert nie Mengen; das Ausgeblendete steht in der
//! Datei (`[hide]`), das Isolieren nie (Entscheidung 11).

use crate::element::{Category, ElementId};
use crate::guid::Guid;
use crate::model::Model;
use crate::solid::NO_LAYER;
use crate::trade::TradeId;
use std::collections::BTreeSet;

/// Was ausgeblendet bzw. isoliert ist.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Visibility {
    /// Bauteile (Karte „Baum“).
    pub hidden: BTreeSet<Guid>,
    /// Bauteilarten (Karte „Bauteil“), auch für später gezeichnete Bauteile.
    pub hidden_cat: BTreeSet<Category>,
    /// Gewerke (Karte „Gewerk“), schichtgenau.
    pub hidden_trade: BTreeSet<TradeId>,
    /// Geländeebene in 3D und Geländelinie in den 2D-Ansichten.
    pub terrain_hidden: bool,
    /// Nur Sitzung, nie in der Datei.
    pub isolate: Option<Isolate>,
}

impl Visibility {
    /// Ist an den Bauteilen nichts ausgeblendet und nichts isoliert?
    pub fn is_plain(&self) -> bool {
        self.hidden.is_empty()
            && self.hidden_cat.is_empty()
            && self.hidden_trade.is_empty()
            && self.isolate.is_none()
    }
}

/// Der isolierte Ast: nur er bleibt deckend, alles andere blass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Isolate {
    Elements(BTreeSet<Guid>),
    Category(Category),
    Trade(TradeId),
}

/// Sichtbare Schichten als Bitmaske (Bit i = Schicht i).
pub type LayerMask = u64;

/// Was von einem Bauteil zu sehen ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    All,
    None,
    Layers(LayerMask),
}

/// Deckend oder blass (außerhalb der Isolierung).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Solid,
    Ghost,
}

/// Bit für Teile ohne Schicht ([`NO_LAYER`]): sie folgen Bauteil und Art,
/// das Gewerk über die erste Schicht bzw. die Bauteilart.
pub const UNLAYERED: u64 = 1 << 63;

/// Masken eines Bauteils: deckend und blass gezeigte Schichten (Bits wie
/// [`LayerMask`], dazu [`UNLAYERED`]). Was in keiner steht, ist
/// ausgeblendet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Masks {
    pub solid: u64,
    pub ghost: u64,
}

impl Masks {
    /// Alles deckend.
    pub const ALL: Masks = Masks {
        solid: u64::MAX,
        ghost: 0,
    };

    /// Bit einer Schicht am Dreieck oder an der Kante.
    pub fn bit(layer: u8) -> u64 {
        if layer == NO_LAYER || layer >= 63 {
            UNLAYERED
        } else {
            1 << layer
        }
    }
}

impl Model {
    /// Was ausgeblendet bzw. isoliert ist.
    pub fn visibility(&self) -> &Visibility {
        &self.visibility
    }

    /// Ändert die Sichtbarkeit: ohne Schritt und ohne neue Revision, wie
    /// [`Model::set_cut`].
    pub fn set_visibility(&mut self, v: Visibility) {
        self.visibility = v;
    }

    /// Gewerk je Schicht des Aufbaus ([`Model::layer_trade`] für alle).
    fn layer_trades(&self, id: ElementId) -> Vec<Option<TradeId>> {
        (0..self.element_layers(id).len())
            .map(|i| self.layer_trade(id, i))
            .collect()
    }

    /// Masken eines Bauteils unter der Sichtbarkeit `v` (§3.4):
    /// 1. Isolieren: Isoliertes deckend (vor Ausblenden), alles andere blass
    ///    mit seinen sonst sichtbaren Schichten; nach Gewerk nur dessen
    ///    Schichten deckend.
    /// 2. Sonst: Bauteil oder Art ausgeblendet → nichts; Schichten eines
    ///    ausgeblendeten Gewerks fehlen.
    pub fn masks_in(&self, v: &Visibility, id: ElementId) -> Masks {
        let none = Masks { solid: 0, ghost: 0 };
        let Some(e) = self.element(id) else {
            return none;
        };
        if v.is_plain() {
            return Masks::ALL;
        }
        let trades = self.layer_trades(id);
        let n = trades.len().min(63);
        let all = ((1u64 << n) - 1) | UNLAYERED;
        let of = |f: &dyn Fn(Option<TradeId>) -> bool| {
            let mut m = 0;
            for (i, t) in trades.iter().take(n).enumerate() {
                if f(*t) {
                    m |= 1 << i;
                }
            }
            // Teile ohne Schicht: Gewerk der ersten Schicht bzw. der Art
            let first = trades.first().copied().flatten().or_else(|| {
                let c = crate::kinds::spec(e.category).default_trade?;
                self.trade_by_code(c)
            });
            if f(first) {
                m |= UNLAYERED;
            }
            m
        };
        let base = if v.hidden.contains(&e.guid) || v.hidden_cat.contains(&e.category) {
            0
        } else if v.hidden_trade.is_empty() {
            all
        } else {
            of(&|t| !t.is_some_and(|t| v.hidden_trade.contains(&t)))
        };
        let inside = match &v.isolate {
            None => {
                return Masks {
                    solid: base,
                    ghost: 0,
                }
            }
            Some(Isolate::Elements(s)) => {
                if s.contains(&e.guid) {
                    all
                } else {
                    0
                }
            }
            Some(Isolate::Category(c)) => {
                if e.category == *c {
                    all
                } else {
                    0
                }
            }
            Some(Isolate::Trade(t)) => of(&|x| x == Some(*t)),
        };
        Masks {
            solid: inside,
            ghost: base & !inside,
        }
    }

    /// Masken eines Bauteils unter der aktuellen Sichtbarkeit.
    pub fn masks(&self, id: ElementId) -> Masks {
        self.masks_in(&self.visibility, id)
    }

    /// Was von einem Bauteil zu sehen ist und wie (§3.4).
    pub fn shown(&self, id: ElementId) -> (Shown, Look) {
        let m = self.masks(id);
        let n = self.element_layers(id).len().min(63);
        let layers = (1u64 << n) - 1;
        let to = |mask: u64| {
            if n == 0 {
                return if mask & UNLAYERED != 0 {
                    Shown::All
                } else {
                    Shown::None
                };
            }
            match mask & layers {
                l if l == layers => Shown::All,
                0 => Shown::None,
                l => Shown::Layers(l),
            }
        };
        if self.visibility.isolate.is_none() || m.solid != 0 {
            (to(m.solid), Look::Solid)
        } else {
            (to(m.ghost), Look::Ghost)
        }
    }
}

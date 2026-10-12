//! Sichtbarkeit (Paket 3, projektstruktur/paket-3-sichtbarkeit.md §3.4):
//! Bauteile, Bauteilarten, Gewerke und das Gelände ausblenden, einen Ast
//! isolieren. Ansichtszustand wie die Schnitte: kein Rückgängig-Schritt,
//! keine neue Revision, ändert nie Mengen; das Ausgeblendete steht in der
//! Datei (`[hide]`), das Isolieren nie (Entscheidung 11).
//!
//! Dazu je Ansicht eine Schnelloption am Zahnrad (Jörn 12.10.): Arten, die
//! nur diese Ansicht ausblendet, heute die Dachentwässerung. Sie wirken wie
//! im Baum ausgeblendet, ändern den Baum aber nicht (`[viewhide]`).

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
    /// Arten, die die gezeigte Ansicht dazu ausblendet (Schnelloption am
    /// Zahnrad); setzt die Szene aus [`Model::view_hidden`] der Ansicht,
    /// nur Sitzung.
    pub view_cat: BTreeSet<Category>,
}

impl Visibility {
    /// Ist an den Bauteilen nichts ausgeblendet und nichts isoliert?
    pub fn is_plain(&self) -> bool {
        self.hidden.is_empty()
            && self.hidden_cat.is_empty()
            && self.hidden_trade.is_empty()
            && self.isolate.is_none()
            && self.view_cat.is_empty()
    }

    /// Was der Baum zeigt: ohne Isolieren und ohne die Schnelloption der
    /// Ansicht (Augen, Zeile im Mengenfenster).
    pub fn of_tree(&self) -> Visibility {
        Visibility {
            isolate: None,
            view_cat: BTreeSet::new(),
            ..self.clone()
        }
    }
}

/// Plätze der Ansichten für die Schnelloption am Zahnrad, wie ihre Namen
/// in der Datei: 3D, Grundriss, Schnitt, Vorne, Hinten, Links, Rechts.
pub const SICHT_VIEWS: [&str; 7] = ["persp", "plan", "section", "front", "back", "left", "right"];

/// Dachentwässerung (Jörn 12.10.): das Regenfallrohr mit Wasserfangkasten,
/// Schellen, Standrohr und Notentwässerung (Wasserspeier bzw. Notfallrohr).
/// Die Zeichen des Gefälleplans auf dem Dach gehören zum Dach und bleiben.
pub const ENTWAESSERUNG: [Category; 1] = [Category::Downpipe];

/// Schnelloption je Ansicht: ausgeblendete Arten je Platz in
/// [`SICHT_VIEWS`], dazu `[viewhide]`-Zeilen, die nicht zählen, roh.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ViewHide {
    pub(crate) cats: [BTreeSet<Category>; 7],
    pub(crate) raw: Vec<String>,
}

static KEINE: BTreeSet<Category> = BTreeSet::new();

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

    /// Arten, die die Ansicht `i` (Platz in [`SICHT_VIEWS`]) dazu
    /// ausblendet; leer für einen unbekannten Platz.
    pub fn view_hidden(&self, i: usize) -> &BTreeSet<Category> {
        self.view_hide.cats.get(i).unwrap_or(&KEINE)
    }

    /// Blendet die Ansicht `i` die ganze Dachentwässerung aus?
    pub fn view_drain_hidden(&self, i: usize) -> bool {
        let h = self.view_hidden(i);
        ENTWAESSERUNG.iter().all(|c| h.contains(c))
    }

    /// Schnelloption der Ansicht `i` setzen: Ansichtszustand ohne Schritt
    /// wie [`Model::set_view_below`]; eine unlesbare Zeile dieser Ansicht
    /// entfällt damit.
    pub fn set_view_hidden(&mut self, i: usize, cats: BTreeSet<Category>) {
        if let Some(v) = self.view_hide.cats.get_mut(i) {
            *v = cats;
            let name = format!("view={}", SICHT_VIEWS[i]);
            self.view_hide
                .raw
                .retain(|l| !l.split_whitespace().any(|w| w == name));
        }
    }

    /// Dachentwässerung der Ansicht `i` aus- (`true`) oder einblenden.
    pub fn set_view_drain_hidden(&mut self, i: usize, hide: bool) {
        let mut cats = self.view_hidden(i).clone();
        for c in ENTWAESSERUNG {
            if hide {
                cats.insert(c);
            } else {
                cats.remove(&c);
            }
        }
        self.set_view_hidden(i, cats);
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
        let base = if v.hidden.contains(&e.guid)
            || v.hidden_cat.contains(&e.category)
            || v.view_cat.contains(&e.category)
        {
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

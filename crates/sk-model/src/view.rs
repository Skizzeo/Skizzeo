//! Sichtbarkeit (Paket 3, projektstruktur/paket-3-sichtbarkeit.md §3.4):
//! Bauteile, Bauteilarten, Gewerke und das Gelände ausblenden, einen Ast
//! isolieren. Ansichtszustand wie die Schnitte: kein Rückgängig-Schritt,
//! keine neue Revision, ändert nie Mengen; das Ausgeblendete steht in der
//! Datei (`[hide]`), das Isolieren nie (Entscheidung 11).
//!
//! Dazu der Ausbau innen der Räume (Raumbuch, Jörn 16.10.): Ausbauarten
//! (Estrich, Wandbekleidung, …) ausblenden, und Gewerke blenden auch die
//! Ausbauschichten aus, die sie haben ([`Model::finish_masks_in`]).
//!
//! Dazu je Ansicht eine Schnelloption am Zahnrad (Jörn 12.10.): Arten, die
//! nur diese Ansicht ausblendet, heute die Dachentwässerung. Sie wirken wie
//! im Baum ausgeblendet, ändern den Baum aber nicht (`[viewhide]`).

use crate::element::{Category, ElementId};
use crate::guid::Guid;
use crate::library::TypeCategory;
use crate::model::Model;
use crate::solid::NO_LAYER;
use crate::trade::TradeId;
use crate::LayerSetId;
use std::collections::BTreeSet;

/// Was ausgeblendet bzw. isoliert ist.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Visibility {
    /// Bauteile (Karte „Baum“).
    pub hidden: BTreeSet<Guid>,
    /// Bauteilarten (Karte „Bauteil“), auch für später gezeichnete Bauteile.
    pub hidden_cat: BTreeSet<Category>,
    /// Gewerke (Karte „Gewerk“), schichtgenau, auch im Ausbau der Räume.
    pub hidden_trade: BTreeSet<TradeId>,
    /// Ausbauarten der Räume (Karte „Bauteil“, [`TypeCategory::FINISHES`]).
    pub hidden_finish: BTreeSet<TypeCategory>,
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
            && self.hidden_finish.is_empty()
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
/// [`SICHT_VIEWS`], nur aus [`ENTWAESSERUNG`] (Review W28), dazu
/// `[viewhide]`-Zeilen, die nicht zählen, roh; und `[hide]`-Zeilen mit
/// einer unbekannten Ausbauart, die nicht in die Sichtbarkeit gehören.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ViewHide {
    pub(crate) cats: [BTreeSet<Category>; 7],
    pub(crate) raw: Vec<String>,
    /// `[hide]`-Zeilen mit einer unbekannten Ausbauart, roh (Regel 72).
    pub(crate) hide_raw: Vec<String>,
}

static KEINE: BTreeSet<Category> = BTreeSet::new();

/// Bauteilart zu ihrem Namen in der Datei (`cat=`).
pub(crate) fn szo_art(w: &str) -> Option<Category> {
    Category::ALL
        .into_iter()
        .chain([Category::Extension])
        .find(|c| crate::kinds::spec(*c).szo == w)
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
    /// wie [`Model::set_view_below`]; nur Arten aus [`ENTWAESSERUNG`]
    /// zählen. Unlesbare und doppelte Zeilen dieser Ansicht entfallen damit,
    /// eine Zeile mit einer anderen Art (etwa aus einer neueren Fassung)
    /// bleibt (Review H6).
    pub fn set_view_hidden(&mut self, i: usize, cats: BTreeSet<Category>) {
        if let Some(v) = self.view_hide.cats.get_mut(i) {
            *v = cats
                .into_iter()
                .filter(|c| ENTWAESSERUNG.contains(c))
                .collect();
            let name = format!("view={}", SICHT_VIEWS[i]);
            self.view_hide.raw.retain(|l| {
                let mut w = l.split_whitespace();
                let fremd = |a: &str| szo_art(a).is_some_and(|c| !ENTWAESSERUNG.contains(&c));
                !w.clone().any(|w| w == name)
                    || w.find_map(|w| w.strip_prefix("cat=")).is_some_and(fremd)
            });
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

    /// Gewerk der Schicht `j` des Typs `typ`: Schicht vor Baustoff, wie
    /// [`Model::layer_trade`] ohne Bauteilart (Ausbau der Räume).
    pub fn type_layer_trade(&self, typ: LayerSetId, j: usize) -> Option<TradeId> {
        let l = self.layer_set(typ)?.layers.get(j)?;
        l.trade.or_else(|| self.material(l.material)?.trade)
    }

    /// Masken eines Ausbaukörpers der Art `art` mit dem Gewerk `trade` im
    /// Raum `id`, im Bit [`UNLAYERED`]: Er folgt seinem Raum, dazu blenden
    /// ihn seine Ausbauart und sein Gewerk aus. Isoliert ist er mit dem
    /// Raum, mit der Art „Räume“ oder mit seinem Gewerk.
    pub fn finish_masks_in(
        &self,
        v: &Visibility,
        id: ElementId,
        art: TypeCategory,
        trade: Option<TradeId>,
    ) -> Masks {
        let Some(e) = self.element(id) else {
            return Masks { solid: 0, ghost: 0 };
        };
        if v.is_plain() {
            return Masks::ALL;
        }
        let aus = v.hidden.contains(&e.guid)
            || v.hidden_cat.contains(&e.category)
            || v.view_cat.contains(&e.category)
            || v.hidden_finish.contains(&art)
            || trade.is_some_and(|t| v.hidden_trade.contains(&t));
        let base = if aus { 0 } else { UNLAYERED };
        let inside = match &v.isolate {
            None => {
                return Masks {
                    solid: base,
                    ghost: 0,
                }
            }
            Some(Isolate::Elements(s)) => s.contains(&e.guid),
            Some(Isolate::Category(c)) => e.category == *c,
            Some(Isolate::Trade(t)) => trade == Some(*t),
        };
        let inside = if inside { UNLAYERED } else { 0 };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Space;
    use crate::{GuidGen, LevelKind};
    use sk_math::vec3;
    use TypeCategory::{Screed, Skirting, WallFinish};

    /// Ein gespeicherter Raum im Erdgeschoss.
    fn raum() -> (Model, ElementId) {
        let mut m = Model::new();
        let b = m.add_building(1);
        let st = m.levels_in(Some(b));
        let eg = st
            .into_iter()
            .find(|s| m.storey(*s).is_some_and(|x| x.kind == LevelKind::Storey))
            .unwrap();
        let r = m.add_space(eg, Space::neu(vec3(1000.0, 1000.0, 0.0), "Raum 1".into()));
        (m, r)
    }

    /// Jörn 16.10.: Ein Ausbaukörper folgt seinem Raum, seiner Ausbauart und
    /// dem Gewerk seiner Schicht; isoliert mit dem Raum, den Räumen oder
    /// seinem Gewerk. Die Bauteile merken von der Ausbauart nichts.
    #[test]
    fn ausbau_nach_art_gewerk_und_raum() {
        let (m, r) = raum();
        let (t, u) = (m.trades()[0].id(), m.trades()[1].id());
        let k = |v: &Visibility, art, tr| m.finish_masks_in(v, r, art, tr);
        let an = Masks {
            solid: UNLAYERED,
            ghost: 0,
        };
        let blass = Masks {
            solid: 0,
            ghost: UNLAYERED,
        };
        let weg = Masks { solid: 0, ghost: 0 };
        let mut v = Visibility::default();
        assert_eq!(k(&v, Screed, Some(t)), Masks::ALL);
        v.hidden_finish.insert(Screed);
        assert!(!v.is_plain());
        assert_eq!(k(&v, Screed, Some(t)), weg);
        assert_eq!(k(&v, WallFinish, Some(t)), an);
        assert_eq!(m.masks_in(&v, r).solid & UNLAYERED, UNLAYERED, "der Raum");
        let v = Visibility {
            hidden_trade: [t].into(),
            ..Visibility::default()
        };
        assert_eq!(k(&v, WallFinish, Some(t)), weg);
        assert_eq!(k(&v, WallFinish, Some(u)), an);
        assert_eq!(k(&v, WallFinish, None), an);
        let v = Visibility {
            hidden_cat: [Category::Space].into(),
            ..Visibility::default()
        };
        assert_eq!(k(&v, WallFinish, Some(u)), weg);
        let v = Visibility {
            isolate: Some(Isolate::Trade(t)),
            hidden_finish: [Screed].into(),
            ..Visibility::default()
        };
        assert_eq!(k(&v, WallFinish, Some(t)), an);
        assert_eq!(k(&v, WallFinish, Some(u)), blass);
        assert_eq!(k(&v, Screed, Some(u)), weg);
        assert_eq!(k(&v, Screed, Some(t)), an, "Isoliertes vor Ausgeblendetem");
        let iso = |i| Visibility {
            isolate: Some(i),
            ..Visibility::default()
        };
        assert_eq!(
            k(&iso(Isolate::Category(Category::Space)), Skirting, None),
            an
        );
        assert_eq!(
            k(
                &iso(Isolate::Category(Category::ExteriorWall)),
                Skirting,
                None
            ),
            blass
        );
        let g = m.element(r).unwrap().guid;
        assert_eq!(k(&iso(Isolate::Elements([g].into())), Skirting, None), an);
    }

    /// Ausgeblendete Ausbauarten stehen in der Datei (`[hide] finish=`),
    /// Rundlauf bytegleich, eine alte Datei bleibt bytegleich; eine
    /// unbekannte Art bleibt roh stehen mit Hinweis (Regel 72), auch wenn
    /// das Panel alles zeigt.
    #[test]
    fn ausbauart_in_der_datei() {
        let load = |t: &str| crate::szo::read(t, GuidGen::with_seed(9)).unwrap();
        let (mut m, _) = raum();
        let alt = crate::szo::write(&m);
        assert!(!alt.contains("finish="));
        let mut v = m.visibility().clone();
        v.hidden_finish.extend([Skirting, Screed]);
        m.set_visibility(v.clone());
        let neu = crate::szo::write(&m);
        let zeilen: Vec<&str> = neu.lines().filter(|z| z.starts_with("[hide]")).collect();
        assert_eq!(zeilen, ["[hide] finish=screed", "[hide] finish=skirting"]);
        let back = load(&neu);
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(back.model.visibility().hidden_finish, v.hidden_finish);
        assert_eq!(crate::szo::write(&back.model), neu);
        let back = load(&alt);
        assert_eq!(crate::szo::write(&back.model), alt);
        let roh = neu.replace("finish=skirting", "finish=kamin");
        let back = load(&roh);
        assert_eq!(back.hints.len(), 1, "{:?}", back.hints);
        assert!(back.hints[0].contains("„kamin“"));
        assert_eq!(back.model.visibility().hidden_finish, [Screed].into());
        assert_eq!(crate::szo::write(&back.model), roh);
        let mut m = back.model;
        m.set_visibility(Visibility::default());
        let ohne = crate::szo::write(&m);
        assert!(ohne.contains("[hide] finish=kamin") && !ohne.contains("finish=screed"));
    }
}

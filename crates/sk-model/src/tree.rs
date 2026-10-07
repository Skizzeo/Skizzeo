//! Baum-Index (Paket 4 §2.1): die drei Karten des Baumpanels als flache
//! Listen in Vorordnung (Kinder folgen ihrem Eltern mit `depth + 1`),
//! unabhängig vom Auf- und Zuklappen. Gliederung wie das Mengenfenster:
//! Geschoss aus [`crate::qto::schedule_storey`], Gruppen nach `qto_rank`.
//! Gebaut einmal je Modellrevision.

use crate::element::{Category, ElementId, StoreyId};
use crate::guid::Guid;
use crate::model::Model;
use crate::trade::TradeId;
use crate::view::{Masks, Visibility, UNLAYERED};
use std::ops::Range;

/// Karte des Baumpanels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    Tree,
    Kind,
    Trade,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Tree, Tab::Kind, Tab::Trade];

    pub fn index(self) -> usize {
        match self {
            Tab::Tree => 0,
            Tab::Kind => 1,
            Tab::Trade => 2,
        }
    }
}

/// Schlüssel einer Zeile: stabil über Neuaufbau (Ziehen, Rückgängig), nie
/// gespeichert. Offene Äste und Hover hängen daran.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NodeKey {
    Project,
    Terrain,
    Building(Guid),
    Storey(Guid),
    /// Art in einem Geschoss (Geschoss-Guid, Art).
    Group(Guid, Category),
    Kind(Category),
    /// Typzeile unter einer Art (Guid des Aufbaus).
    KindType(Category, Guid),
    Trade(TradeId),
    Element(Guid),
    /// Bauteil unter einem Gewerk.
    TradeElement(TradeId, Guid),
}

/// Eine Zeile.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub key: NodeKey,
    /// „Außenwände (4)“, „Erdgeschoss“, „AW-005“.
    pub label: String,
    /// Kleinangabe rechts: „AW-001 … AW-004“, „EG“.
    pub right: String,
    pub depth: u8,
    /// Bauteile darunter: Bereich in [`ProjectTree::members`].
    pub elements: Range<usize>,
    /// Karte „Gewerk“: Schichten des Bauteils in diesem Gewerk.
    pub layer_hint: Option<String>,
    /// Ende des Asts (ausschließlich): die Zeilen `i + 1 .. end` liegen
    /// darunter.
    pub end: usize,
}

impl Node {
    pub fn has_children(&self, i: usize) -> bool {
        self.end > i + 1
    }
}

/// Die drei Karten und die Bauteile ihrer Äste.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectTree {
    pub tabs: [Vec<Node>; 3],
    pub members: Vec<ElementId>,
}

impl ProjectTree {
    pub fn tab(&self, t: Tab) -> &[Node] {
        &self.tabs[t.index()]
    }

    /// Bauteile unter Zeile `i` der Karte `t`.
    pub fn elements(&self, t: Tab, i: usize) -> &[ElementId] {
        self.tab(t)
            .get(i)
            .map_or(&[], |n| &self.members[n.elements.clone()])
    }
}

/// Zustand eines Asts für Auge und Schloss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Alles sichtbar bzw. alles gesperrt.
    Full,
    Partial,
    /// Nichts sichtbar bzw. nichts gesperrt.
    None,
}

impl State {
    fn of(full: usize, none: usize, n: usize) -> State {
        if full == n {
            State::Full
        } else if none == n {
            State::None
        } else {
            State::Partial
        }
    }
}

/// Bauliste in Vorordnung.
struct List<'a> {
    nodes: Vec<Node>,
    members: &'a mut Vec<ElementId>,
}

impl List<'_> {
    /// Öffnet eine Zeile; `close` schließt sie nach den Kindern.
    fn open(&mut self, key: NodeKey, label: String, right: String, depth: u8) -> usize {
        let at = self.members.len();
        self.nodes.push(Node {
            key,
            label,
            right,
            depth,
            elements: at..at,
            layer_hint: None,
            end: 0,
        });
        self.nodes.len() - 1
    }

    fn close(&mut self, i: usize) {
        let end = self.members.len();
        let n = self.nodes.len();
        let node = &mut self.nodes[i];
        node.elements.end = end;
        node.end = n;
    }

    /// Bauteilzeile mit einem Bauteil.
    fn element(&mut self, key: NodeKey, label: String, right: String, depth: u8, id: ElementId) {
        let i = self.open(key, label, right, depth);
        self.members.push(id);
        self.close(i);
    }
}

/// „AW-001 … AW-004“ bzw. „AW-001“.
fn span(m: &Model, ids: &[ElementId]) -> String {
    let nums: Vec<&str> = ids
        .iter()
        .filter_map(|&id| m.element(id).map(|e| e.number.as_str()))
        .collect();
    match nums.as_slice() {
        [] => String::new(),
        [a] => a.to_string(),
        [a, .., b] => format!("{a} … {b}"),
    }
}

/// Bauteile nach Nummer.
fn by_number(m: &Model, ids: &mut [ElementId]) {
    ids.sort_by(|a, b| {
        let n = |id: &ElementId| m.element(*id).map_or("", |e| e.number.as_str());
        n(a).cmp(n(b))
    });
}

/// Arten in Mengenfenster-Reihenfolge mit ihren Bauteilen (nach Nummer).
fn by_kind(m: &Model, ids: &[ElementId]) -> Vec<(Category, Vec<ElementId>)> {
    let mut out: Vec<(Category, Vec<ElementId>)> = Vec::new();
    for &id in ids {
        let Some(e) = m.element(id) else { continue };
        match out.iter_mut().find(|(c, _)| *c == e.category) {
            Some((_, v)) => v.push(id),
            None => out.push((e.category, vec![id])),
        }
    }
    out.sort_by_key(|(c, _)| (crate::kinds::spec(*c).qto_rank, *c));
    for (_, v) in &mut out {
        by_number(m, v);
    }
    out
}

/// Zentimeter im deutschen Format: „24“, „17,5“.
fn cm(mm: f64) -> String {
    let c = mm.round() / 10.0;
    if (c - c.round()).abs() < 1e-9 {
        format!("{}", c.round() as i64)
    } else {
        format!("{c:.1}").replace('.', ",")
    }
}

/// Gewerke eines Bauteils mit dem Text seiner Schichten darin
/// („24 cm Gasbeton“). Ohne Schicht: das Gewerk der Art.
fn trades_of(m: &Model, id: ElementId) -> Vec<(TradeId, String)> {
    let mut out: Vec<(TradeId, String)> = Vec::new();
    let layers = m.element_layers(id);
    for (i, l) in layers.iter().enumerate() {
        let Some(t) = m.layer_trade(id, i) else {
            continue;
        };
        let name = m.material(l.material).map_or("", |x| x.name.as_str());
        let text = format!("{} cm {name}", cm(l.thickness));
        match out.iter_mut().find(|(x, _)| *x == t) {
            Some((_, s)) => {
                s.push_str(", ");
                s.push_str(&text);
            }
            None => out.push((t, text)),
        }
    }
    if layers.is_empty() {
        let c = m.element(id).map(|e| e.category);
        if let Some(t) = c
            .and_then(|c| crate::kinds::spec(c).default_trade)
            .and_then(|code| m.trade_by_code(code))
        {
            out.push((t, String::new()));
        }
    }
    out
}

/// Baut die drei Karten.
pub fn build(m: &Model) -> ProjectTree {
    let mut members = Vec::new();
    let tree = tree_tab(m, &mut members);
    let kind = kind_tab(m, &mut members);
    let trade = trade_tab(m, &mut members);
    ProjectTree {
        tabs: [tree, kind, trade],
        members,
    }
}

/// Karte „Baum“: Projekt → Gelände, Gebäude → Geschosse von oben →
/// Arten → Bauteile.
fn tree_tab(m: &Model, members: &mut Vec<ElementId>) -> Vec<Node> {
    let mut l = List {
        nodes: Vec::new(),
        members,
    };
    let in_storey = |sid: StoreyId| -> Vec<ElementId> {
        m.elements()
            .iter()
            .filter(|(_, e)| crate::qto::schedule_storey(m, e) == sid)
            .map(|(id, _)| id)
            .collect()
    };
    let storey = |l: &mut List, sid: StoreyId, depth: u8| {
        let Some(st) = m.storey(sid) else { return };
        let s = l.open(
            NodeKey::Storey(st.guid),
            st.name.clone(),
            String::new(),
            depth,
        );
        for (cat, ids) in by_kind(m, &in_storey(sid)) {
            let spec = crate::kinds::spec(cat);
            if let [id] = ids[..] {
                // Eine Art mit genau einem Bauteil: nur ihr Name, die
                // Nummer steht im Hinweis
                let g = m.element(id).map_or(Guid(0), |e| e.guid);
                l.element(
                    NodeKey::Element(g),
                    spec.short.into(),
                    String::new(),
                    depth + 1,
                    id,
                );
                continue;
            }
            let label = format!("{} ({})", spec.plural, ids.len());
            let g = l.open(
                NodeKey::Group(st.guid, cat),
                label,
                span(m, &ids),
                depth + 1,
            );
            for id in ids {
                let Some(e) = m.element(id) else { continue };
                let key = NodeKey::Element(e.guid);
                l.element(key, e.number.clone(), String::new(), depth + 2, id);
            }
            l.close(g);
        }
        l.close(s);
    };
    let p = l.open(
        NodeKey::Project,
        format!("Projekt „{}“", m.project().name),
        String::new(),
        0,
    );
    let t = l.open(NodeKey::Terrain, "Gelände".into(), String::new(), 1);
    l.close(t);
    let mut buildings: Vec<_> = m.buildings().iter().collect();
    buildings.sort_by(|a, b| a.1.number.cmp(&b.1.number));
    for (bid, b) in buildings {
        let i = l.open(
            NodeKey::Building(b.guid),
            format!("Gebäude {}", b.number),
            String::new(),
            1,
        );
        for sid in m.levels_in(Some(bid)).into_iter().rev() {
            storey(&mut l, sid, 2);
        }
        l.close(i);
    }
    // Geschosse ohne Gebäude (selten): direkt unter dem Projekt
    for sid in m.levels_in(None).into_iter().rev() {
        if !in_storey(sid).is_empty() {
            storey(&mut l, sid, 1);
        }
    }
    l.close(p);
    l.nodes
}

/// Karte „Bauteil“: Arten → (Typ, wenn mehr als einer) → Bauteile.
fn kind_tab(m: &Model, members: &mut Vec<ElementId>) -> Vec<Node> {
    let mut l = List {
        nodes: Vec::new(),
        members,
    };
    let all: Vec<ElementId> = m.elements().iter().map(|(id, _)| id).collect();
    let short = |id: ElementId| {
        m.element(id)
            .and_then(|e| m.storey(crate::qto::schedule_storey(m, e)))
            .map_or(String::new(), |s| s.short.clone())
    };
    for (cat, ids) in by_kind(m, &all) {
        let spec = crate::kinds::spec(cat);
        let label = format!("{} ({})", spec.plural, ids.len());
        let k = l.open(NodeKey::Kind(cat), label, span(m, &ids), 0);
        let mut types: Vec<Option<crate::LayerSetId>> = Vec::new();
        for &id in &ids {
            let t = m.element(id).and_then(|e| e.layer_set);
            if !types.contains(&t) {
                types.push(t);
            }
        }
        let typed = types.len() > 1 && types.iter().all(Option::is_some);
        if !typed {
            for id in ids {
                let Some(e) = m.element(id) else { continue };
                l.element(NodeKey::Element(e.guid), e.number.clone(), short(id), 1, id);
            }
            l.close(k);
            continue;
        }
        let name = |t: crate::LayerSetId| m.layer_set(t).map_or(String::new(), |x| x.name.clone());
        let mut types: Vec<crate::LayerSetId> = types.into_iter().flatten().collect();
        types.sort_by_key(|t| name(*t));
        for t in types {
            let of: Vec<ElementId> = ids
                .iter()
                .copied()
                .filter(|&id| m.element(id).is_some_and(|e| e.layer_set == Some(t)))
                .collect();
            let g = m.layer_set(t).map_or(Guid(0), |x| x.guid);
            let label = format!("{} · {}×", name(t), of.len());
            let r = l.open(NodeKey::KindType(cat, g), label, span(m, &of), 1);
            for id in of {
                let Some(e) = m.element(id) else { continue };
                l.element(NodeKey::Element(e.guid), e.number.clone(), short(id), 2, id);
            }
            l.close(r);
        }
        l.close(k);
    }
    l.nodes
}

/// Karte „Gewerk“: Gewerke nach Bauablauf → Bauteile mit ihrer Schicht.
fn trade_tab(m: &Model, members: &mut Vec<ElementId>) -> Vec<Node> {
    let mut l = List {
        nodes: Vec::new(),
        members,
    };
    let mut all: Vec<ElementId> = m.elements().iter().map(|(id, _)| id).collect();
    by_number(m, &mut all);
    let per: Vec<(ElementId, Vec<(TradeId, String)>)> =
        all.iter().map(|&id| (id, trades_of(m, id))).collect();
    for t in m.trades() {
        let tid = t.id();
        let rows: Vec<(ElementId, &String)> = per
            .iter()
            .filter_map(|(id, ts)| ts.iter().find(|x| x.0 == tid).map(|x| (*id, &x.1)))
            .collect();
        if rows.is_empty() {
            continue;
        }
        let i = l.open(
            NodeKey::Trade(tid),
            t.display_name().into(),
            String::new(),
            0,
        );
        for (id, hint) in rows {
            let Some(e) = m.element(id) else { continue };
            let r = l.open(
                NodeKey::TradeElement(tid, e.guid),
                e.number.clone(),
                String::new(),
                1,
            );
            l.nodes[r].layer_hint = (!hint.is_empty()).then(|| hint.clone());
            l.members.push(id);
            l.close(r);
        }
        l.close(i);
    }
    l.nodes
}

/// Sichtbar ohne Isolieren: voll, teilweise (einzelne Schichten) oder nicht.
fn element_eye(m: &Model, v: &Visibility, id: ElementId) -> State {
    let n = m.element_layers(id).len().min(63);
    let all = ((1u64 << n) - 1) | UNLAYERED;
    let Masks { solid, .. } = m.masks_in(v, id);
    if solid & all == all {
        State::Full
    } else if solid & all == 0 {
        State::None
    } else {
        State::Partial
    }
}

/// Bauteile, von denen etwas ausgeblendet ist (ohne Isolieren): für die
/// Zeile im Mengenfenster, dessen Mengen vollständig bleiben.
pub fn hidden_count(m: &Model) -> usize {
    let v = Visibility {
        isolate: None,
        ..m.visibility().clone()
    };
    if v.is_plain() {
        return 0;
    }
    m.elements()
        .iter()
        .filter(|(id, _)| element_eye(m, &v, *id) != State::Full)
        .count()
}

/// Auge der Zeile `i` (Paket 3): Isolieren zählt nicht, nur Ausgeblendetes.
/// In der Karte „Gewerk“ ist eine Gewerkzeile zu, wenn das Gewerk
/// ausgeblendet ist.
pub fn eye(m: &Model, t: &ProjectTree, tab: Tab, i: usize) -> State {
    let Some(n) = t.tab(tab).get(i) else {
        return State::Full;
    };
    let v = Visibility {
        isolate: None,
        ..m.visibility().clone()
    };
    match n.key {
        NodeKey::Terrain => {
            return if v.terrain_hidden {
                State::None
            } else {
                State::Full
            };
        }
        NodeKey::Trade(tr) if v.hidden_trade.contains(&tr) => return State::None,
        _ => {}
    }
    let ids = t.elements(tab, i);
    let (mut full, mut none) = (0, 0);
    for &id in ids {
        match element_eye(m, &v, id) {
            State::Full => full += 1,
            State::None => none += 1,
            State::Partial => {}
        }
    }
    let s = State::of(full, none, ids.len());
    // Projekt: auch das Gelände
    if n.key == NodeKey::Project && v.terrain_hidden && s != State::None {
        return State::Partial;
    }
    s
}

/// Schloss der Zeile `i` der Karte „Baum“: alles, teilweise oder nichts
/// gesperrt (über die Quelle, [`Model::is_locked`]).
pub fn lock(m: &Model, t: &ProjectTree, i: usize) -> State {
    let ids = t.elements(Tab::Tree, i);
    if ids.is_empty() {
        return State::None;
    }
    let locked = ids.iter().filter(|&&id| m.is_locked(id)).count();
    State::of(locked, ids.len() - locked, ids.len())
}

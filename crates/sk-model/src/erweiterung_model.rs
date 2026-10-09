//! Erweiterungsbauteile im Modell (E3): Definitionen aufnehmen, Exemplare
//! setzen und ändern, rechnen. Typen in [`crate::erweiterung`].

use super::*;
use crate::erweiterung::{ExtDef, ExtPart};
use crate::erweiterung_koerper::Lage;
use sk_szb::{Ergebnis, Geschoss};

/// Warum eine Definition oder ein Exemplar nicht aufgenommen wurde.
#[derive(Clone, Debug, PartialEq)]
pub enum ExtError {
    /// Keine Definition mit diesem `key` im Projekt.
    NoDef(String),
    /// Das Präfix nutzt schon eine andere Erweiterung (Vertrag §5).
    PrefixTaken {
        prefix: String,
        by: String,
    },
    NoStorey,
}

impl std::fmt::Display for ExtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtError::NoDef(k) => write!(f, "Erweiterung „{k}“ fehlt im Projekt"),
            ExtError::PrefixTaken { prefix, by } => {
                write!(f, "Präfix {prefix} nutzt schon die Erweiterung „{by}“")
            }
            ExtError::NoStorey => write!(f, "Geschoss fehlt"),
        }
    }
}

impl Model {
    /// Die Definitionen im Projekt, nach `key`.
    pub fn ext_defs(&self) -> &[ExtDef] {
        &self.ext_defs
    }

    pub fn ext_def(&self, key: &str) -> Option<&ExtDef> {
        self.ext_defs.iter().find(|d| d.key == key)
    }

    /// Definition eines Exemplars.
    pub fn ext_def_of(&self, id: ElementId) -> Option<&ExtDef> {
        match &self.element(id)?.kind {
            ElementKind::Ext(p) => self.ext_def(&p.key),
            _ => None,
        }
    }

    /// Exemplare der Definition `key`.
    pub fn ext_uses(&self, key: &str) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|(_, e)| matches!(&e.kind, ElementKind::Ext(p) if p.key == key))
            .map(|(id, _)| id)
            .collect()
    }

    /// Unlesbare `[extdef]`- und `[extpart]`-Zeilen der Datei, roh.
    pub fn ext_raw(&self) -> &[String] {
        &self.ext_raw
    }

    fn note_ext_defs(&mut self) {
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::ExtDefs) {
                    t.changes.push(Change::ExtDefs {
                        old: self.ext_defs.clone(),
                        new: Vec::new(),
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
    }

    /// Nimmt eine Definition auf oder ersetzt die mit demselben `key`
    /// (neue Version: alle Exemplare folgen ihr). Ein Präfix, das schon eine
    /// andere Erweiterung nutzt, wird abgewiesen.
    pub fn put_ext_def(&mut self, d: ExtDef) -> Result<(), ExtError> {
        if let Some(o) = self
            .ext_defs
            .iter()
            .find(|o| o.key != d.key && o.prefix() == d.prefix())
        {
            return Err(ExtError::PrefixTaken {
                prefix: d.prefix().to_string(),
                by: o.key.clone(),
            });
        }
        if self.ext_def(&d.key) == Some(&d) {
            return Ok(());
        }
        self.note_ext_defs();
        match self.ext_defs.iter_mut().find(|o| o.key == d.key) {
            Some(o) => *o = d,
            None => {
                let i = self.ext_defs.partition_point(|o| o.key < d.key);
                self.ext_defs.insert(i, d);
            }
        }
        self.touch();
        Ok(())
    }

    /// Entfernt eine Definition ohne Exemplare. `false`: es gibt noch welche
    /// oder keine Definition.
    pub fn remove_ext_def(&mut self, key: &str) -> bool {
        if self.ext_def(key).is_none() || !self.ext_uses(key).is_empty() {
            return false;
        }
        self.note_ext_defs();
        self.ext_defs.retain(|d| d.key != key);
        self.touch();
        true
    }

    /// Ein roh gebliebenes Exemplar trägt die Nummer `s`.
    fn raw_number(&self, s: &str) -> bool {
        let (a, b) = (format!("number={s}"), format!("number=\"{s}\""));
        self.ext_raw
            .iter()
            .any(|l| l.starts_with("[extpart]") && l.split(' ').any(|w| w == a || w == b))
    }

    /// Nächste Nummer zum Präfix, z. B. „ST-004“. Nie wiederverwendet, auch
    /// nicht nach Löschen oder Rückgängig.
    fn next_ext_number(&mut self, prefix: &str) -> String {
        loop {
            let n = self.ext_numbers.entry(prefix.to_string()).or_insert(0);
            *n += 1;
            let s = format!("{prefix}-{:03}", n);
            if self.element_by_number(&s).is_none() && !self.raw_number(&s) {
                return s;
            }
        }
    }

    /// Setzt ein Exemplar ins Geschoss `storey`.
    pub fn add_ext(&mut self, storey: StoreyId, part: ExtPart) -> Result<ElementId, ExtError> {
        let prefix = self
            .ext_def(&part.key)
            .ok_or_else(|| ExtError::NoDef(part.key.clone()))?
            .prefix()
            .to_string();
        if self.storey(storey).is_none() {
            return Err(ExtError::NoStorey);
        }
        let number = self.next_ext_number(&prefix);
        let guid = self.new_guid();
        let id = self.elements.insert(Element {
            guid,
            number,
            category: Category::Extension,
            storey,
            layer_set: None,
            seq: EXT_SEQ,
            kind: ElementKind::Ext(part),
            props: PropSet::new(),
            locked: false,
        });
        note!(self, Element, new id);
        self.touch();
        Ok(id)
    }

    /// Ändert Lage, Typ oder Werte eines Exemplars; die Definition bleibt.
    pub fn set_ext(&mut self, id: ElementId, part: ExtPart) -> bool {
        match self.element(id).map(|e| &e.kind) {
            Some(ElementKind::Ext(p)) if p.key == part.key => {
                if *p == part {
                    return true;
                }
            }
            _ => return false,
        }
        note!(self, Element, self.elements, id);
        if let Some(e) = self.elements.get_mut(id) {
            e.kind = ElementKind::Ext(part);
        }
        self.touch();
        true
    }

    /// Geschosshöhe und Decke für die Formeln (`GH`, `DECKE`, `LICHT`).
    pub fn ext_geschoss(&self, storey: StoreyId) -> Geschoss {
        Geschoss {
            gh: self.storey(storey).map_or(0.0, |s| s.height),
            decke: self.floor_thickness_of(storey),
        }
    }

    /// Körper, Werte und Mengen eines Exemplars, gerechnet mit seinen
    /// Werten im eigenen Geschoss.
    pub fn ext_ergebnis(&self, id: ElementId) -> Option<Ergebnis> {
        let e = self.element(id)?;
        let ElementKind::Ext(p) = &e.kind else {
            return None;
        };
        let d = self.ext_def(&p.key)?;
        let g = self.ext_geschoss(e.storey);
        Some(sk_szb::rechnen(&d.def, &d.werte(p, &g), &g))
    }

    /// Lage und Rechnung eines Exemplars für die Darstellung.
    pub fn ext_lage(&self, id: ElementId) -> Option<(Lage, Ergebnis)> {
        let e = self.element(id)?;
        let ElementKind::Ext(p) = &e.kind else {
            return None;
        };
        let erg = self.ext_ergebnis(id)?;
        let lage = Lage {
            at: p.at,
            rot: p.rot,
            z: self.storey(e.storey)?.elevation + erg.z0,
        };
        Some((lage, erg))
    }

    /// Baustoff des Projekts für den Baustoffschlüssel `key` einer
    /// Definition: ein Werksbaustoff über seinen Namen (Vertrag §9), sonst
    /// der erste Baustoff derselben Kategorie. Eigene Baustoffe legt erst
    /// das Einlesen an (E5); bis dahin zeigen sie sich so.
    pub fn ext_material(&self, d: &ExtDef, key: &str) -> Option<MaterialId> {
        let best = sk_szb::Bestand::werk();
        let werk = best.baustoff(key);
        if let Some(w) = werk {
            if let Some((id, _)) = self.materials.iter().find(|(_, m)| m.name == w.name) {
                return Some(id);
            }
        }
        let kat = werk.map(|w| w.kategorie).or_else(|| {
            d.def
                .baustoff
                .iter()
                .find(|b| b.key() == key)
                .and_then(|b| b.get("kategorie"))
        })?;
        let kat = match kat {
            "masonry" => MatCategory::Masonry,
            "concrete" => MatCategory::Concrete,
            "insulation" => MatCategory::Insulation,
            "plaster" => MatCategory::Plaster,
            "timber" => MatCategory::Timber,
            "metal" => MatCategory::Metal,
            _ => return None,
        };
        self.materials
            .iter()
            .find(|(_, m)| m.category == kat)
            .map(|(id, _)| id)
    }

    /// Höchste vorhandene Nummer je Präfix.
    fn highest_ext_numbers(&self) -> BTreeMap<String, u32> {
        let mut n: BTreeMap<String, u32> = BTreeMap::new();
        for (_, e) in self.elements.iter() {
            if e.category != Category::Extension {
                continue;
            }
            if let Some((p, k)) = e.number.rsplit_once('-') {
                if let Ok(k) = k.parse::<u32>() {
                    let c = n.entry(p.to_string()).or_insert(0);
                    *c = (*c).max(k);
                }
            }
        }
        n
    }

    /// Zähler über der höchsten Nummer (es wurde gelöscht), für `next=`.
    pub(crate) fn ext_number_gaps(&self) -> Vec<(String, u32)> {
        let h = self.highest_ext_numbers();
        self.ext_numbers
            .iter()
            .filter(|(p, n)| **n > h.get(*p).copied().unwrap_or(0))
            .map(|(p, n)| (p.clone(), *n))
            .collect()
    }

    /// Hebt den Zähler eines Präfixes einer Erweiterung im Projekt an.
    pub(crate) fn raise_ext_counter(&mut self, prefix: &str, n: u32) -> bool {
        if !self.ext_defs.iter().any(|d| d.prefix() == prefix) {
            return false;
        }
        let k = self.ext_numbers.entry(prefix.to_string()).or_insert(0);
        *k = (*k).max(n);
        true
    }

    /// Definitionen und rohe Zeilen aus der Datei; Zähler auf die höchste
    /// vorhandene Nummer.
    pub(crate) fn load_ext(&mut self, mut defs: Vec<ExtDef>, raw: Vec<String>) {
        defs.sort_by(|a, b| a.key.cmp(&b.key));
        self.ext_defs = defs;
        self.ext_raw = raw;
        self.ext_numbers = self.highest_ext_numbers();
    }
}

/// Bauabschnitt der Erweiterungen: nach den Wänden (3).
pub const EXT_SEQ: u16 = 4;

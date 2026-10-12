//! Raum als gespeichertes Bauteil (Grundriss G3, Jörn 11.10. 16:53, J10;
//! schnittstelle-raum.md §2). Gespeichert sind nur Anker, Name, Nutzung,
//! Faktor und eine verschobene Stempellage; die Form rechnet `sk-raum` aus
//! den Wänden, nie gespeichert. Methoden von `Model`, aus `model.rs`
//! ausgelagert.
//!
//! Ein Raum ruht, wenn keine Fläche seinen Anker umschließt oder ein
//! älterer Raum dieselbe Fläche hat (Räume vereint, Wand weg): Er bleibt mit
//! seinen Daten unsichtbar im Modell und lebt wieder auf, wenn seine Fläche
//! zurückkommt. Das rechnet `sk-raum`, gespeichert ist es nicht.

use super::*;

/// Gespeicherter Raum ([`ElementKind::Space`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Space {
    /// Punkt im Raum (Grundrissebene, z = 0). Ordnet den gespeicherten Raum
    /// seiner Fläche zu.
    pub anker: Vec3,
    /// Lage des Raumstempels, nur wenn der Nutzer ihn verschoben hat.
    pub stempel: Option<Vec3>,
    /// „Raum 1“, bis der Nutzer ihn benennt.
    pub name: String,
    /// Raumvorlage im Raumkatalog des Raumbuchs (`use=`).
    pub nutzung: Option<Guid>,
    /// Abweichender Faktor für die Wohnfläche (0 … 1), selten.
    pub faktor: Option<f64>,
}

impl Space {
    pub fn neu(anker: Vec3, name: String) -> Space {
        Space {
            anker: vec3(anker.x, anker.y, 0.0),
            stempel: None,
            name,
            nutzung: None,
            faktor: None,
        }
    }
}

/// Bauabschnitt der Räume (ohne Ausführungsfolge).
pub const SPACE_SEQ: u16 = 0;

impl Model {
    /// Der Raum hinter einem Bauteil.
    pub fn space(&self, id: ElementId) -> Option<&Space> {
        match &self.element(id)?.kind {
            ElementKind::Space(s) => Some(s),
            _ => None,
        }
    }

    /// Räume eines Geschosses (auch ruhende), nach Nummer (R-010 vor R-1000).
    pub fn spaces(&self, storey: StoreyId) -> Vec<ElementId> {
        let mut v: Vec<(&str, ElementId)> = self
            .elements
            .iter()
            .filter(|(_, e)| e.storey == storey && matches!(e.kind, ElementKind::Space(_)))
            .map(|(id, e)| (e.number.as_str(), id))
            .collect();
        v.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(a.0.cmp(b.0)));
        v.into_iter().map(|(_, id)| id).collect()
    }

    /// Legt einen Raum an (im offenen Schritt; Nummer R-…).
    pub fn add_space(&mut self, storey: StoreyId, s: Space) -> ElementId {
        let id = self.new_element(Category::Space, storey, SPACE_SEQ, ElementKind::Space(s));
        self.touch();
        id
    }

    /// Ändert einen Raum; gleicher Inhalt ändert nichts. `false`, wenn `id`
    /// kein Raum ist.
    pub fn set_space(&mut self, id: ElementId, f: impl FnOnce(&mut Space)) -> bool {
        let Some(mut s) = self.space(id).cloned() else {
            return false;
        };
        f(&mut s);
        s.anker = vec3(s.anker.x, s.anker.y, 0.0);
        if self.space(id) == Some(&s) {
            return true;
        }
        note!(self, Element, self.elements, id);
        if let Some(e) = self.elements.get_mut(id) {
            e.kind = ElementKind::Space(s);
        }
        self.touch();
        true
    }

    /// Geschosse, deren Wandzüge oder Wände der offene Schritt ändert
    /// (vorher und nachher): dort gleicht `sk-raum` die Räume ab. Leer ohne
    /// offenen Schritt.
    pub fn wand_geschosse_im_schritt(&self) -> Vec<StoreyId> {
        let mut v: Vec<StoreyId> = Vec::new();
        let Some(open) = &self.txn else {
            return v;
        };
        let mut neu = |g: StoreyId| {
            if !v.contains(&g) {
                v.push(g);
            }
        };
        for c in &open.changes {
            match c {
                Change::Run { id, old, .. } => old
                    .iter()
                    .chain(self.runs.get(*id))
                    .for_each(|r| neu(r.storey)),
                Change::Element { id, old, .. } => old
                    .iter()
                    .chain(self.elements.get(*id))
                    .filter(|e| matches!(e.kind, ElementKind::Wall(_)))
                    .for_each(|e| neu(e.storey)),
                _ => {}
            }
        }
        v
    }

    /// Geschosse, deren Räume der offene Schritt ändert (anlegen, Anker,
    /// Stempel, Name): Die Szene rechnet dort den Stand der Räume neu. Leer
    /// ohne offenen Schritt.
    pub fn raum_geschosse_im_schritt(&self) -> Vec<StoreyId> {
        let mut v: Vec<StoreyId> = Vec::new();
        let Some(open) = &self.txn else {
            return v;
        };
        for c in &open.changes {
            if let Change::Element { id, old, .. } = c {
                for e in old.iter().chain(self.elements.get(*id)) {
                    if matches!(e.kind, ElementKind::Space(_)) && !v.contains(&e.storey) {
                        v.push(e.storey);
                    }
                }
            }
        }
        v
    }

    /// Kleinster freie Name „Raum n“ im Projekt (auch ruhende zählen).
    pub fn naechster_raumname(&self) -> String {
        self.freie_raumnamen()
            .next()
            .unwrap_or_else(|| "Raum 1".into())
    }

    /// Die freien Namen „Raum n“ im Projekt, kleinste zuerst (einmal über
    /// alle Bauteile, Review #87 B3).
    pub fn freie_raumnamen(&self) -> FreieRaumnamen {
        let belegt = self
            .elements
            .iter()
            .filter_map(|(_, e)| match &e.kind {
                ElementKind::Space(s) => s.name.strip_prefix("Raum ")?.parse().ok(),
                _ => None,
            })
            .collect();
        FreieRaumnamen { belegt, n: 0 }
    }
}

/// Freie Namen „Raum n“ ([`Model::freie_raumnamen`]).
pub struct FreieRaumnamen {
    belegt: std::collections::HashSet<u32>,
    n: u32,
}

impl Iterator for FreieRaumnamen {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        loop {
            self.n = self.n.checked_add(1)?;
            if !self.belegt.contains(&self.n) {
                return Some(format!("Raum {}", self.n));
            }
        }
    }
}

/// Befunde zu einem Raum für [`Model::check`].
pub(super) fn pruefe(nummer: &str, s: &Space) -> Vec<String> {
    let mut out = Vec::new();
    if !(s.anker.x.is_finite() && s.anker.y.is_finite() && s.anker.z == 0.0) {
        out.push(format!("{nummer}: Anker ungültig"));
    }
    if s.stempel
        .is_some_and(|p| !(p.x.is_finite() && p.y.is_finite()))
    {
        out.push(format!("{nummer}: Stempellage ungültig"));
    }
    if s.name.trim().is_empty() {
        out.push(format!("{nummer}: ohne Namen"));
    }
    if s.faktor.is_some_and(|f| !(0.0..=1.0).contains(&f)) {
        out.push(format!("{nummer}: Faktor außerhalb 0 … 1"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raum_anlegen_aendern_benennen() {
        let mut m = Model::new();
        let st = m.defaults().storey;
        m.begin("Raum");
        assert_eq!(m.naechster_raumname(), "Raum 1");
        let a = m.add_space(
            st,
            Space::neu(vec3(1000.0, 1000.0, 5.0), m.naechster_raumname()),
        );
        assert_eq!(m.element(a).unwrap().number, "R-001");
        assert_eq!(m.element(a).unwrap().category, Category::Space);
        assert_eq!(m.space(a).unwrap().anker.z, 0.0);
        assert_eq!(m.naechster_raumname(), "Raum 2");
        let b = m.add_space(st, Space::neu(vec3(3000.0, 1000.0, 0.0), "Raum 2".into()));
        assert!(m.set_space(a, |s| s.name = "Küche".into()));
        assert_eq!(m.naechster_raumname(), "Raum 1", "frei geworden");
        assert_eq!(m.spaces(st), vec![a, b]);
        let iw = m.defaults().interior_wall;
        let pts = [vec3(0.0, 0.0, 0.0), vec3(2000.0, 0.0, 0.0)];
        let r = m
            .add_wall_run(&pts, false, RefSide::Center, st, iw, Category::InteriorWall)
            .unwrap();
        let wand = m.wall_at(r, 0).unwrap();
        assert!(!m.set_space(wand, |_| {}), "keine Wand als Raum");
        assert!(m.commit().is_some());
    }
}

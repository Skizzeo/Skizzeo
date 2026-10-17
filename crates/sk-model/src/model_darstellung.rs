//! Gebäudemodell, Teil „Darstellung: Stifte, Linienarten, Füllungen, Oberflächen“ (aus model.rs, Paket T1).

use super::*;

impl Model {
    pub fn attr(&self) -> &Attributes {
        &self.attr
    }

    pub fn add_pen(&mut self, p: Pen) -> PenId {
        self.touch();
        let id = self.attr.add_pen(p);
        note!(self, Pen, new id);
        id
    }

    pub fn add_line_type(&mut self, l: LineType) -> LineTypeId {
        self.touch();
        let id = self.attr.add_line_type(l);
        note!(self, LineType, new id);
        id
    }

    pub fn add_fill(&mut self, f: Fill) -> FillId {
        self.touch();
        let id = self.attr.add_fill(f);
        note!(self, Fill, new id);
        id
    }

    pub fn add_surface(&mut self, s: Surface) -> SurfaceId {
        self.touch();
        let id = self.attr.add_surface(s);
        note!(self, Surface, new id);
        id
    }

    pub fn set_pen(&mut self, id: PenId, p: Pen) -> bool {
        note!(self, Pen, self.attr.pens(), id);
        let ok = self.attr.set_pen(id, p);
        self.revision += ok as u64;
        ok
    }

    /// Löscht einen unbenutzten Stift ([`Model::attr_users`] leer). `false`,
    /// wenn er verwendet wird oder fehlt; dann bleibt alles, wie es ist.
    pub fn remove_pen(&mut self, id: PenId) -> bool {
        if self.attr.pen(id).is_none() || !self.attr_users(AttrRef::Pen(id)).is_empty() {
            return false;
        }
        note!(self, Pen, self.attr.pens(), id);
        self.touch();
        self.attr.remove_pen(id).is_some()
    }

    /// Nummer für einen neuen Stift: die höchste vergebene + 1. Nummern
    /// rücken nie nach und werden nicht wieder vergeben (solange der Stift mit
    /// der höchsten Nummer lebt).
    pub fn next_pen_number(&self) -> u16 {
        let max = self.attr.pens().iter().map(|(_, p)| p.number).max();
        max.map_or(1, |n| n.saturating_add(1))
    }

    /// Wer das Attribut verwendet: Stellen der Darstellung (Stift und
    /// Linientyp) und Baustoffe (Schnittmuster, Schraffur, Grund, Oberfläche).
    pub fn attr_users(&self, r: AttrRef) -> Vec<AttrUser> {
        let mut out = Vec::new();
        for (label, st) in attr::display_slots(self.attr.display()) {
            let hit = match r {
                AttrRef::Pen(p) => st.pen == p,
                AttrRef::LineType(l) => st.line_type == l,
                _ => false,
            };
            if hit {
                out.push(AttrUser::Display(label));
            }
        }
        for (id, m) in self.materials.iter() {
            let roles: [(&'static str, bool); 4] = [
                ("Schnittmuster", r == AttrRef::Fill(m.cut_fill)),
                ("Schraffur", r == AttrRef::Pen(m.cut_fg)),
                ("Grund", r == AttrRef::Pen(m.cut_bg)),
                ("Oberfläche", r == AttrRef::Surface(m.surface)),
            ];
            for (role, hit) in roles {
                if hit {
                    out.push(AttrUser::Material {
                        id,
                        name: m.name.clone(),
                        role,
                    });
                }
            }
        }
        out
    }

    /// Ändert einen Linientyp; `false` (und nichts geändert) bei toter Kennung.
    pub fn set_line_type(&mut self, id: LineTypeId, l: LineType) -> bool {
        if self.attr.line_type(id).is_none() {
            return false;
        }
        note!(self, LineType, self.attr.line_types(), id);
        let ok = self.attr.set_line_type(id, l);
        self.revision += ok as u64;
        ok
    }

    /// Löscht einen unbenutzten Linientyp (wie [`Model::remove_pen`]).
    pub fn remove_line_type(&mut self, id: LineTypeId) -> bool {
        if self.attr.line_type(id).is_none() || !self.attr_users(AttrRef::LineType(id)).is_empty() {
            return false;
        }
        note!(self, LineType, self.attr.line_types(), id);
        self.touch();
        self.attr.remove_line_type(id).is_some()
    }

    /// Löscht eine Schraffur, auf die kein Baustoff verweist.
    pub fn remove_fill(&mut self, id: FillId) -> bool {
        if self.attr.fill(id).is_none() || !self.attr_users(AttrRef::Fill(id)).is_empty() {
            return false;
        }
        note!(self, Fill, self.attr.fills(), id);
        self.touch();
        self.attr.remove_fill(id).is_some()
    }

    /// Löscht eine Oberfläche, auf die kein Baustoff verweist.
    pub fn remove_surface(&mut self, id: SurfaceId) -> bool {
        if self.attr.surface(id).is_none() || !self.attr_users(AttrRef::Surface(id)).is_empty() {
            return false;
        }
        note!(self, Surface, self.attr.surfaces(), id);
        self.touch();
        self.attr.remove_surface(id).is_some()
    }

    /// Ändert die Darstellungsverweise eines Baustoffs (Schraffur, Stift
    /// Schraffur, Stift Grund, Oberfläche). Die übrigen Felder gehören BIM und
    /// bleiben unerreichbar. `false` (nichts geändert), wenn der Baustoff oder
    /// eines der Ziele fehlt.
    pub fn set_material_display(&mut self, id: MaterialId, d: MaterialDisplay) -> bool {
        let a = &self.attr;
        let alive = a.fill(d.cut_fill).is_some()
            && a.pen(d.cut_fg).is_some()
            && a.pen(d.cut_bg).is_some()
            && a.surface(d.surface).is_some();
        if !alive || self.materials.get(id).is_none() {
            return false;
        }
        note!(self, Material, self.materials, id);
        if let Some(m) = self.materials.get_mut(id) {
            (m.cut_fill, m.cut_fg, m.cut_bg, m.surface) =
                (d.cut_fill, d.cut_fg, d.cut_bg, d.surface);
        }
        self.touch();
        // Nur die Zeichentabelle ändert sich, keine Körper
        self.attr.bump();
        true
    }

    /// Ergänzt in Dateien vor E4 die Linientypen des Startsatzes und setzt die
    /// Schnittlinie A–A auf Strichpunkt, wenn sie auf die Volllinie zeigt (so
    /// sieht sie aus wie bisher). Ohne Rückgängig-Schritt, gleich nach dem
    /// Lesen; liefert einen Hinweis, wenn etwas ergänzt wurde.
    pub(crate) fn complete_line_types(&mut self) -> Vec<String> {
        let mut added = false;
        for (name, pattern) in attr::standard_line_types() {
            if self.attr.line_types().iter().any(|(_, l)| l.name == name) {
                continue;
            }
            let guid = self.new_guid();
            let id = self.attr.add_line_type(LineType {
                guid,
                name: name.into(),
                pattern,
            });
            added = true;
            let solid = self
                .attr
                .line_type(self.attr.display().section_line.line_type);
            if name == attr::SECTION_LINE_TYPE && solid.is_some_and(|l| l.pattern.is_empty()) {
                let mut d = self.attr.display().clone();
                d.section_line.line_type = id;
                self.attr.set_display(d);
            }
        }
        if added {
            vec!["Linientypen ergänzt".to_string()]
        } else {
            Vec::new()
        }
    }

    pub fn set_fill(&mut self, id: FillId, f: Fill) -> bool {
        note!(self, Fill, self.attr.fills(), id);
        let ok = self.attr.set_fill(id, f);
        self.revision += ok as u64;
        ok
    }

    pub fn set_surface(&mut self, id: SurfaceId, s: Surface) -> bool {
        note!(self, Surface, self.attr.surfaces(), id);
        let ok = self.attr.set_surface(id, s);
        self.revision += ok as u64;
        ok
    }

    /// Muster einer Oberfläche setzen oder abwählen (Paket 6). Fremde
    /// `[pattern]`-Zeilen derselben Oberfläche aus der gelesenen Datei fallen
    /// weg, damit nach dem Speichern genau eine Zeile gilt (Regel 64).
    pub fn set_surface_pattern(
        &mut self,
        id: SurfaceId,
        pattern: Option<crate::proctex::Pattern>,
    ) -> bool {
        let Some(s) = self.attr.surface(id).cloned() else {
            return false;
        };
        let key = format!("surface={}", s.guid);
        let mine =
            |r: &String| r.starts_with("[pattern]") && r.split_whitespace().any(|w| w == key);
        if !self.set_surface(id, Surface { pattern, ..s }) {
            return false;
        }
        // erst nach gelungenem Setzen und mit Rückgängig (Review 3q/4)
        if self.foreign.records.iter().any(mine) {
            match self.txn.as_mut() {
                Some(t) => {
                    if t.noted.insert(Key::ForeignRecords) {
                        t.changes.push(Change::ForeignRecords {
                            old: self.foreign.records.clone(),
                            new: Vec::new(),
                        });
                    }
                }
                None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
            }
            self.foreign.records.retain(|r| !mine(r));
            self.touch();
        }
        true
    }

    pub fn set_display(&mut self, d: Display) {
        match self.txn.as_mut() {
            Some(t) => {
                if t.noted.insert(Key::Display) {
                    t.changes.push(Change::Display {
                        old: self.attr.display().clone(),
                        new: d.clone(),
                    });
                }
            }
            None => debug_assert!(!self.strict, "Änderung ohne Schritt"),
        }
        self.touch();
        self.attr.set_display(d);
    }
}

//! Gebäudemodell, Teil „Prüfung des Modells“ (aus model.rs, Paket T1).

use super::*;

impl Model {
    /// Prüfregel Kopplung (B12): Partner lebt, steht im Geschoss direkt
    /// darunter im selben Gebäude, und das Segment liegt parallel auf
    /// Partnerlinie + Versatz (Toleranz 0,01 mm).
    pub(super) fn check_coupling(
        &self,
        id: ElementId,
        number: &str,
        w: Wall,
        c: Coupling,
    ) -> Vec<String> {
        let mut out = Vec::new();
        let (Some(me), Some(partner)) = (self.element(id), self.element(c.below)) else {
            out.push(format!("{number}: gekoppelte Wand darunter fehlt"));
            return out;
        };
        let ElementKind::Wall(pw) = partner.kind else {
            out.push(format!(
                "{number}: gekoppelt an ein Bauteil, das keine Wand ist"
            ));
            return out;
        };
        if self.level_below(me.storey) != Some(partner.storey) {
            out.push(format!(
                "{number}: gekoppelte Wand {} steht nicht im Geschoss darunter",
                partner.number
            ));
            return out;
        }
        let offsets = self
            .base_chain(w.run)
            .zip(self.base_chain(pw.run))
            .and_then(|(a, b)| {
                let k = self.segment_of(c.below)?.1;
                let la = a.segment_frame(w.seg as usize)?;
                let lb = b.segment_frame(k)?;
                // parallel, gleiche Richtung, Abstand quer zur Wand nach außen
                let par = cross2(la.1, lb.1).abs() < 1e-9 && la.1.dot(lb.1) > 0.0;
                let n = vec3(lb.1.y, -lb.1.x, 0.0);
                par.then(|| (la.0 - lb.0).dot(n) * b.outward_sign())
            });
        match offsets {
            Some(d) if (d - c.offset).abs() <= 0.01 => {}
            Some(d) => out.push(format!(
                "{number}: liegt {d:.2} mm statt {:.2} mm neben {}",
                c.offset, partner.number
            )),
            None => out.push(format!("{number}: nicht parallel zu {}", partner.number)),
        }
        out
    }

    /// Prüft die Strukturregeln des BIM-Konzepts (Abschnitt 8) und liefert
    /// die Verstöße als Text.
    pub fn check(&self) -> Vec<String> {
        let mut out = Vec::new();
        // Wandkörper für die Öffnungen, einmal je Prüfung
        let mut koerper = oeffnung_model::Koerper::default();
        let strips_wanted = self.edge_strip_pairs();
        let soffits_wanted = self.soffit_floors();
        let terraces_wanted = self.terrace_floors();
        let mut guids = Vec::new();
        let mut numbers: Vec<&str> = Vec::new();
        for (id, e) in self.elements.iter() {
            guids.push(e.guid);
            numbers.push(&e.number);
            if e.number.is_empty() {
                out.push(format!("{id:?}: keine Nummer"));
            }
            if !self.storeys.contains(e.storey) {
                out.push(format!("{}: Geschoss fehlt", e.number));
            }
            if let Some(s) = e.layer_set {
                match self.layer_set(s) {
                    None => out.push(format!("{}: Aufbau fehlt", e.number)),
                    // Regel 16/37: der Typ passt zum Bauteil
                    Some(t) if TypeCategory::of(e.category) != Some(t.category) => {
                        out.push(format!(
                            "{}: Typ {} passt nicht zur {}",
                            e.number,
                            t.code,
                            e.category.name()
                        ))
                    }
                    // Regel 38: der Kernbaustoff ist der Baustoff am Bauteil
                    Some(t) if t.category.variable_core() => {
                        let core = t.layers.iter().find(|l| l.core).map(|l| l.material);
                        if core.is_some() && core != e.kind.material() {
                            out.push(format!(
                                "{}: Baustoff weicht vom Kern des Typs {} ab",
                                e.number, t.code
                            ));
                        }
                    }
                    Some(_) => {}
                }
            } else if crate::kinds::spec(e.category).needs_type {
                out.push(format!("{}: kein Bauteiltyp", e.number));
            }
            match e.kind {
                ElementKind::Wall(w) => {
                    if self.wall_at(w.run, w.seg as usize) != Some(id) {
                        out.push(format!("{}: nicht im Wandzug eingetragen", e.number));
                    }
                    if let Some(c) = w.coupling {
                        out.extend(self.check_coupling(id, &e.number, w, c));
                    }
                }
                ElementKind::GroundSlab(s) => {
                    if !self.needs_foundation(s.run) {
                        out.push(format!(
                            "{}: kein geschlossener Außenwandzug im EG darüber",
                            e.number
                        ));
                    }
                    if self.footings_of(id).len() != 1 {
                        out.push(format!("{}: nicht genau eine Frostschürze", e.number));
                    }
                    let want = usize::from(s.insulation > 0.0);
                    if self.perimeters_of(id).len() != want {
                        out.push(format!(
                            "{}: Perimeterdämmung passt nicht zur Dicke {}",
                            e.number, s.insulation
                        ));
                    }
                    if s.insulation != 0.0
                        && !(s.insulation >= MIN_PERIMETER && s.insulation <= MAX_PERIMETER)
                    {
                        out.push(format!(
                            "{}: Perimeterdämmung {} mm (0 oder {MIN_PERIMETER} bis {MAX_PERIMETER} mm)",
                            e.number, s.insulation
                        ));
                    }
                    if s.recess > 0.0 && s.recess < MIN_RECESS {
                        out.push(format!(
                            "{}: Sockelrücksprung {} mm (0 oder ab {MIN_RECESS} mm)",
                            e.number, s.recess
                        ));
                    }
                    if let Some(Err(err)) = self.foundation(s.run) {
                        out.push(format!("{}: keine Gründung, {}", e.number, explain(err)));
                    }
                    if !self.materials.contains(s.material) {
                        out.push(format!("{}: Baustoff fehlt", e.number));
                    }
                }
                ElementKind::PerimeterInsulation { slab } => {
                    if self.slab_insulation(slab) <= 0.0 {
                        out.push(format!("{}: Sohlplatte fehlt oder ungedämmt", e.number));
                    }
                }
                ElementKind::StripFooting(f) => {
                    // unter einer Perimeterdämmung ab deren Unterkante
                    let ins = self.slab_insulation(f.slab);
                    if self.footing_depth(id).is_none_or(|d| d - ins <= 0.0) {
                        out.push(format!("{}: Tiefe nicht größer als 0", e.number));
                    }
                    if !matches!(
                        self.element(f.slab).map(|x| &x.kind),
                        Some(ElementKind::GroundSlab(_))
                    ) {
                        out.push(format!("{}: Sohlplatte fehlt", e.number));
                    }
                    if !self.materials.contains(f.material) {
                        out.push(format!("{}: Baustoff fehlt", e.number));
                    }
                }
                ElementKind::Floor(f) => {
                    if !self.needs_floor(f.run) {
                        out.push(format!(
                            "{}: kein geschlossener Außenwandzug darunter",
                            e.number
                        ));
                    }
                    if !(f.thickness > 0.0 && f.thickness.is_finite()) {
                        out.push(format!("{}: Dicke {} ungültig", e.number, f.thickness));
                    }
                    let crown = self
                        .run(f.run)
                        .and_then(|r| self.level_z(r.top))
                        .unwrap_or(0.0);
                    if self
                        .run(f.run)
                        .is_some_and(|r| f.top != LevelRef::top(r.storey))
                    {
                        out.push(format!("{}: nicht an der OK des Geschosses", e.number));
                    }
                    match self.level_z(f.top) {
                        Some(top) if top - f.thickness > 0.0 && top <= crown + 1e-6 => {}
                        Some(top) => out.push(format!(
                            "{}: Lage OK {} / UK {} außerhalb der Wand",
                            e.number,
                            top,
                            top - f.thickness
                        )),
                        None => out.push(format!("{}: Geschoss der Oberkante fehlt", e.number)),
                    }
                    if let Some(Err(err)) = self.floor(f.run) {
                        out.push(format!("{}: keine Decke, {}", e.number, explain_floor(err)));
                    }
                    if !self.materials.contains(f.material) {
                        out.push(format!("{}: Baustoff fehlt", e.number));
                    }
                    // Regel 35: Dicke der Untersichtdämmung je Decke
                    if !(MIN_SOFFIT..=MAX_SOFFIT).contains(&f.soffit.thickness) {
                        out.push(format!(
                            "{}: Untersichtdämmung {} mm (erlaubt {MIN_SOFFIT}…{MAX_SOFFIT} mm)",
                            e.number, f.soffit.thickness
                        ));
                    }
                    // Bekleidung (W3)
                    let c = f.soffit;
                    if !(c.cladding == 0.0 || (MIN_CLADDING..=MAX_CLADDING).contains(&c.cladding))
                        || !(MIN_DRIP..=MAX_DRIP).contains(&c.drip)
                        || !(MIN_BATTEN..=MAX_BATTEN).contains(&c.batten)
                        || !(c.counter == 0.0 || (MIN_BATTEN..=MAX_BATTEN).contains(&c.counter))
                    {
                        out.push(format!(
                            "{}: Bekleidung {} mm, Überstand {} mm, Lattung {}/{} mm außerhalb der Grenzen",
                            e.number, c.cladding, c.drip, c.batten, c.counter
                        ));
                    }
                    if c.cladding_material
                        .is_some_and(|m| !self.materials.contains(m))
                    {
                        out.push(format!("{}: Baustoff der Bekleidung fehlt", e.number));
                    }
                    if f.soffit
                        .material
                        .is_some_and(|m| !self.materials.contains(m))
                    {
                        out.push(format!(
                            "{}: Baustoff der Untersichtdämmung fehlt",
                            e.number
                        ));
                    }
                    // Regel 43: Attika 0 … 30 cm, Typ der Terrasse passend
                    if !(0.0..=MAX_UPSTAND).contains(&f.terrace.upstand) {
                        out.push(format!(
                            "{}: Attika {} mm über Belag (erlaubt 0…{MAX_UPSTAND} mm)",
                            e.number, f.terrace.upstand
                        ));
                    }
                    out.extend(self.crown_problem(&e.number, &f.terrace));
                    out.extend(self.inner_problem(&e.number, &f.terrace));
                    if f.terrace.build_up.is_some_and(|t| {
                        self.layer_set(t)
                            .is_none_or(|x| x.category != TypeCategory::RoofTerrace)
                    }) {
                        out.push(format!("{}: Typ der Dachterrasse ungültig", e.number));
                    }
                    if f.terrace
                        .coping_mat
                        .is_some_and(|m| !self.materials.contains(m))
                    {
                        out.push(format!("{}: Baustoff des Attikablechs fehlt", e.number));
                    }
                }
                ElementKind::Roof { floor, .. } => out.extend(self.check_flat_roof(id, floor)),
                ElementKind::Downpipe { roof, drain, .. } => {
                    out.extend(self.check_downpipe(&e.number, roof, drain))
                }
                ElementKind::Coping { floor } if self.flat_roof_coping(id, floor) => {
                    out.extend(self.check_flat_roof(id, floor))
                }
                ElementKind::RoofTerrace { floor } | ElementKind::Coping { floor } => {
                    // Regeln 41/46: genau dann, wenn darüber ein Rücksprung ist
                    if !matches!(
                        self.element(floor).map(|f| &f.kind),
                        Some(ElementKind::Floor(_))
                    ) {
                        out.push(format!("{}: Decke fehlt", e.number));
                    } else if !terraces_wanted.contains(&floor) {
                        out.push(format!("{}: kein Rücksprung über der Decke", e.number));
                    } else if matches!(e.kind, ElementKind::RoofTerrace { .. })
                        && e.layer_set != self.terrace_type_of(floor)
                    {
                        out.push(format!("{}: Typ weicht von der Decke ab", e.number));
                    }
                }
                ElementKind::SoffitInsulation { floor } => {
                    // Regel 35: genau dann, wenn die Decke auskragt
                    if !matches!(
                        self.element(floor).map(|f| &f.kind),
                        Some(ElementKind::Floor(_))
                    ) {
                        out.push(format!("{}: Decke fehlt", e.number));
                    } else if !soffits_wanted.contains(&floor) {
                        out.push(format!("{}: Decke kragt nicht aus", e.number));
                    }
                }
                ElementKind::EdgeStrip { wall, floor } => {
                    // Regel 22: Wand und Decke gibt es, das Paar braucht ihn
                    let wall_e = self.element(wall);
                    if !matches!(wall_e.map(|w| &w.kind), Some(ElementKind::Wall(_))) {
                        out.push(format!("{}: Wand fehlt", e.number));
                    } else if wall_e.is_some_and(|w| w.storey != e.storey) {
                        out.push(format!("{}: nicht im Geschoss seiner Wand", e.number));
                    }
                    if !matches!(
                        self.element(floor).map(|f| &f.kind),
                        Some(ElementKind::Floor(_))
                    ) {
                        out.push(format!("{}: Decke fehlt", e.number));
                    }
                    if !strips_wanted.contains(&(wall, floor)) {
                        out.push(format!(
                            "{}: Wand und Decke brauchen keinen Randdämmstreifen",
                            e.number
                        ));
                    }
                }
                ElementKind::Ext(ref p) => {
                    match self.ext_def(&p.key) {
                        None => out.push(format!("{}: Erweiterung „{}“ fehlt", e.number, p.key)),
                        Some(d) if !e.number.starts_with(&format!("{}-", d.prefix())) => {
                            out.push(format!("{}: Nummer passt nicht zum Präfix", e.number))
                        }
                        Some(_) => {}
                    }
                    let zahlen = p.at.iter().chain([&p.rot]);
                    if !zahlen
                        .chain(p.werte.iter().map(|(_, v)| v))
                        .all(|v| v.is_finite())
                    {
                        out.push(format!("{}: Lage oder Werte ungültig", e.number));
                    }
                }
                ElementKind::Opening(ref o) => {
                    if let Some(f) = self.opening_fault_in(o, Some(id), &mut koerper) {
                        out.push(format!("{}: {}", e.number, f.text(self)));
                    }
                }
                ElementKind::Space(ref s) => out.extend(raum::pruefe(&e.number, s)),
            }
        }
        // Regel 35: unter jeder auskragenden Decke genau eine Untersichtdämmung
        for floor in &soffits_wanted {
            let n = self
                .elements
                .iter()
                .filter(|(_, e)| e.kind == ElementKind::SoffitInsulation { floor: *floor })
                .count();
            if n != 1 {
                let f = self.element(*floor).map_or("?", |f| f.number.as_str());
                out.push(format!("{f}: {n} Untersichtdämmungen statt einer"));
            }
        }
        // Regeln 41/46: auf jeder Decke mit Rücksprung darüber genau eine
        // Dachterrasse und ein Attikablech
        for floor in &terraces_wanted {
            for (kind, what) in [
                (
                    ElementKind::RoofTerrace { floor: *floor },
                    "Dachterrassen statt einer",
                ),
                (
                    ElementKind::Coping { floor: *floor },
                    "Attikableche statt einem",
                ),
            ] {
                let n = self.elements.iter().filter(|(_, e)| e.kind == kind).count();
                if n != 1 {
                    let f = self.element(*floor).map_or("?", |f| f.number.as_str());
                    out.push(format!("{f}: {n} {what}"));
                }
            }
        }
        // Regel 22: zu jedem Paar genau ein Streifen
        for (wall, floor) in &strips_wanted {
            let n = self
                .elements
                .iter()
                .filter(|(_, e)| {
                    e.kind
                        == ElementKind::EdgeStrip {
                            wall: *wall,
                            floor: *floor,
                        }
                })
                .count();
            if n != 1 {
                let w = self.element(*wall).map_or("?", |w| w.number.as_str());
                out.push(format!("{w}: {n} Randdämmstreifen statt einem"));
            }
        }
        for (id, _) in self.runs.iter() {
            if self.needs_foundation(id) && self.slabs_of(id).len() != 1 {
                out.push(format!("Wandzug {id:?}: nicht genau eine Sohlplatte"));
            }
            if self.needs_floor(id) && self.floors_of(id).len() != 1 {
                out.push(format!("Wandzug {id:?}: nicht genau eine Decke"));
            }
        }
        for (id, r) in self.runs.iter() {
            guids.push(r.guid);
            // Regel 26: ein Zug ist nie leer
            if r.segments.is_empty() {
                out.push(format!("Wandzug {id:?}: leer, ohne Segment"));
            }
            let count = segment_count(r.points.len(), r.closed);
            if r.segments.len() != count {
                out.push(format!(
                    "Wandzug {id:?}: {} Wände für {count} Segmente",
                    r.segments.len()
                ));
            }
            if r.base != LevelRef::bottom(r.storey) || r.top != LevelRef::top(r.storey) {
                out.push(format!(
                    "Wandzug {id:?}: Fuß/Krone nicht an UK/OK des eigenen Geschosses"
                ));
            }
            if self
                .storey(r.storey)
                .is_some_and(|s| s.building.is_none_or(|b| !self.buildings.contains(b)))
            {
                out.push(format!("Wandzug {id:?}: Geschoss ohne Gebäude"));
            }
            let n = r.points.len();
            for k in 0..count {
                let (p, q) = (r.points[k], r.points[(k + 1) % n]);
                if (q - p).length() < 1.0 {
                    out.push(format!("Wandzug {id:?}: Segment {} ohne Länge", k + 1));
                }
            }
            // Keine Gehrung kippt einen Körper: dieselbe Regel wie beim
            // Ziehen des Wandendes (Review #87 B5), hier an den Flächen nach
            // dem Zuschnitt (#98), für gezeichnete und geladene Züge
            if let Some(c) = self.chain(id) {
                for k in c.koerper_kippt() {
                    out.push(format!(
                        "Wandzug {id:?}: Gehrung an Segment {} gekippt",
                        k + 1
                    ));
                }
            }
            if let Some(lower) = self.run_below(id).and_then(|b| self.run(b)) {
                self.check_stack(id, r, lower, &mut out);
                // Regel 30: der Umriss oben schneidet sich nicht selbst
                if r.closed && !sk_math::polygon::is_simple(&r.points) {
                    out.push(format!("Wandzug {id:?}: Umriss schneidet sich selbst"));
                }
            }
            for e in &r.segments {
                if self.segment_of(*e).map(|s| s.0) != Some(id) {
                    out.push(format!(
                        "Wandzug {id:?}: Wand {e:?} fehlt oder gehört woandershin"
                    ));
                }
            }
            // Regel 16: ein Zug hat genau einen Typ
            let mut sets = r
                .segments
                .iter()
                .filter_map(|e| self.element(*e))
                .map(|e| e.layer_set);
            if let Some(first) = sets.next() {
                if sets.any(|s| s != first) {
                    out.push(format!("Wandzug {id:?}: Wände mit verschiedenen Typen"));
                }
            }
        }
        out.extend(self.check_levels());
        out.extend(self.check_delete());
        let mut codes: Vec<&str> = Vec::new();
        for (_, set) in self.layer_sets.iter() {
            out.extend(set.problems());
            out.extend(self.bearing_problem(set));
            out.extend(self.finish_problems(set));
            codes.push(&set.code);
        }
        out.extend(self.check_space_types());
        // Regel 17: Kurzzeichen eindeutig
        codes.sort_unstable();
        for w in codes
            .windows(2)
            .filter(|w| w[0] == w[1] && !w[0].is_empty())
        {
            out.push(format!("Kurzzeichen {} doppelt vergeben", w[0]));
        }
        // Regel 18: die Standardtypen leben und haben ihre Art
        for cat in TypeCategory::WALLS {
            match self.layer_set(self.default_type(cat)) {
                Some(t) if t.category == cat => {}
                Some(t) => out.push(format!(
                    "Standardtyp {}: {} ist kein {}typ",
                    cat.name(),
                    t.code,
                    cat.name()
                )),
                None => out.push(format!("Standardtyp {} fehlt", cat.name())),
            }
        }
        for (_, m) in self.materials.iter() {
            let a = &self.attr;
            if a.fill(m.cut_fill).is_none()
                || a.pen(m.cut_fg).is_none()
                || a.pen(m.cut_bg).is_none()
                || a.surface(m.surface).is_none()
            {
                out.push(format!(
                    "Baustoff {}: Verweis auf fehlendes Attribut",
                    m.name
                ));
            }
        }
        out.extend(self.attr.check());
        guids.extend(self.attr.guids());
        guids.extend(self.materials.iter().map(|(_, m)| m.guid));
        guids.extend(self.layer_sets.iter().map(|(_, s)| s.guid));
        guids.extend(self.storeys.iter().map(|(_, s)| s.guid));
        guids.extend(self.buildings.iter().map(|(_, b)| b.guid));
        guids.push(self.project.guid);
        let mut bn: Vec<&str> = self
            .buildings
            .iter()
            .map(|(_, b)| b.number.as_str())
            .collect();
        let n = bn.len();
        bn.sort();
        bn.dedup();
        if bn.len() != n {
            out.push("Gebäudenummer doppelt vergeben".into());
        }
        let n = guids.len();
        guids.sort();
        guids.dedup();
        if guids.len() != n {
            out.push("Guid doppelt vergeben".into());
        }
        let n = numbers.len();
        numbers.sort();
        numbers.dedup();
        if numbers.len() != n {
            out.push("Bauteilnummer doppelt vergeben".into());
        }
        for (_, s) in self.layer_sets.iter() {
            for l in &s.layers {
                if !self.materials.contains(l.material) {
                    out.push(format!("Aufbau {}: Baustoff fehlt", s.name));
                }
                // Regel 47 und 49: Gewerk lebt, KG in der Gruppe 300
                if l.trade.is_some_and(|t| self.trade(t).is_none()) {
                    out.push(format!("Aufbau {}: Gewerk fehlt", s.name));
                }
                if l.kg.is_some_and(|k| !trade::valid_kg(k)) {
                    out.push(format!("Aufbau {}: Kostengruppe ungültig", s.name));
                }
            }
        }
        for (_, x) in self.materials.iter() {
            if x.trade.is_some_and(|t| self.trade(t).is_none()) {
                out.push(format!("Baustoff {}: Gewerk fehlt", x.name));
            }
        }
        // Regel 48: Gewerke eindeutig
        out.extend(trade::problems(&self.trades));
        for j in self.joins.iter() {
            if self.segment_of(j.a).map(|s| s.0) != Some(j.a_run)
                || self.segment_of(j.b).map(|s| s.0) != Some(j.b_run)
            {
                out.push(format!("Anschluss {:?}: Wand fehlt", j.a));
            }
            if self.run(j.a_run).map(|r| r.storey) != self.run(j.b_run).map(|r| r.storey) {
                out.push(format!("Anschluss {:?}: über zwei Geschosse", j.a));
            }
        }
        // Regel 13: Sichtbares hängt nur an den Punkten, nicht am Verlauf
        let fresh = self.detect_all();
        if fresh.len() != self.joins.len()
            || !fresh.iter().all(|f| self.joins.iter().any(|j| j.same(f)))
        {
            out.push("Anschlüsse passen nicht zu den Punkten".into());
        }
        out
    }
}

//! Öffnungen in der Datei (I5, ifc/plan.md §5): je Paar aus Öffnung und
//! gelochter Wand ein `IfcOpeningElement` an der Wand, über
//! `IfcRelVoidsElement`. Das Wandnetz ist schon gelocht wie in der Ansicht
//! (Reference View rechnet nicht ab); die Form der Öffnung steht deshalb als
//! `Reference` und nicht als `Body`, so zieht kein Betrachter ein zweites Mal
//! ab und keiner zeigt die Öffnung als Körper. Die gestaffelten Quader
//! kommen aus [`sk_model::Model::opening_voids`].

use super::{abgeleitet, gid, Bericht, Ctx, Grund};
use crate::geometrie;
use crate::mengen;
use crate::step::{e, lens, r, refs, s, V};
use sk_math::vec3;
use sk_model::{ElementId, ElementKind};

impl Ctx<'_> {
    /// Die Öffnungen `ids` an ihren Wänden, die schon in der Datei stehen.
    pub(super) fn oeffnungen(&mut self, ids: &[ElementId], bericht: &mut Bericht) {
        let m = self.m;
        for &id in ids {
            let Some(el) = m.element(id) else {
                continue;
            };
            let ElementKind::Opening(op) = &el.kind else {
                continue;
            };
            let voids: Vec<_> = m
                .opening_voids(id)
                .into_iter()
                .filter(|v| self.lage.contains_key(&v.wall))
                .collect();
            if voids.is_empty() {
                bericht
                    .ausgelassen
                    .push((el.number.clone(), Grund::OhneWand));
                continue;
            }
            let z = self.zuordnung(id);
            for (k, v) in voids.iter().enumerate() {
                let (wand, w_pl, elev) = self.lage[&v.wall];
                let Some(w) = m.element(v.wall) else {
                    continue;
                };
                // Die erste Wand trägt die Kennung der Öffnung, jede
                // weitere eine daraus und aus der Wand abgeleitete
                let g = if k == 0 {
                    el.guid
                } else {
                    abgeleitet(el.guid, &format!("void.{}", w.guid.to_ifc()))
                };
                let mut items = Vec::new();
                for b in &v.boxes {
                    for n in geometrie::netze(b, vec3(0.0, 0.0, elev)) {
                        let coords: Vec<V> =
                            n.punkte.iter().map(|p| lens(&[p.x, p.y, p.z])).collect();
                        let pl = self.w.add("IFCCARTESIANPOINTLIST3D", vec![V::List(coords)]);
                        let idx: Vec<V> = n
                            .dreiecke
                            .iter()
                            .map(|t| V::List(t.iter().map(|&i| V::Int(i as i64 + 1)).collect()))
                            .collect();
                        items.push(self.w.add(
                            "IFCTRIANGULATEDFACESET",
                            vec![r(pl), V::Null, V::Bool(true), V::List(idx), V::Null],
                        ));
                    }
                }
                let rep = self.w.add(
                    "IFCSHAPEREPRESENTATION",
                    vec![
                        r(self.body),
                        s("Reference"),
                        s("Tessellation"),
                        refs(&items),
                    ],
                );
                let shape = self.w.add(
                    "IFCPRODUCTDEFINITIONSHAPE",
                    vec![V::Null, V::Null, refs(&[rep])],
                );
                let origin = self.placement3(vec3(0.0, 0.0, 0.0));
                let pl = self.w.add("IFCLOCALPLACEMENT", vec![r(w_pl), r(origin)]);
                let oi = self.w.add(
                    "IFCOPENINGELEMENT",
                    vec![
                        gid(g),
                        r(self.oh),
                        s(&el.number),
                        s(&format!("{} in {}", op.kind.name(), w.number)),
                        z.object_type.as_deref().map_or(V::Null, s),
                        r(pl),
                        r(shape),
                        s(&el.number),
                        e(z.predefined),
                    ],
                );
                if let Some(satz) = mengen::oeffnung(v.width, v.height, v.depth) {
                    self.mengensatz(abgeleitet(g, satz.name), &satz, oi);
                }
                self.w.add(
                    "IFCRELVOIDSELEMENT",
                    vec![
                        gid(abgeleitet(g, "voids")),
                        r(self.oh),
                        V::Null,
                        V::Null,
                        r(wand),
                        r(oi),
                    ],
                );
            }
            bericht.oeffnungen += 1;
        }
    }
}

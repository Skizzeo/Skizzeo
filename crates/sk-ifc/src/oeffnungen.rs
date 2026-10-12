//! Öffnungen in der Datei (I5, ifc/plan.md §5): je Paar aus Öffnung und
//! gelochter Wand ein `IfcOpeningElement` an der Wand, über
//! `IfcRelVoidsElement`. Das Wandnetz ist schon gelocht wie in der Ansicht
//! (Reference View rechnet nicht ab); die Form der Öffnung steht deshalb als
//! `Reference` und nicht als `Body`, so zieht kein Betrachter ein zweites Mal
//! ab und keiner zeigt die Öffnung als Körper. Die gestaffelten Quader
//! kommen aus [`sk_model::Model::opening_voids`].
//!
//! Hat eine Tür ein Blatt (Aufschlag und Türhand), füllt ein `IfcDoor` ihre
//! Öffnung in der Wand des Blatts (`IfcRelFillsElement`). Ohne Form: das
//! Modell kennt vom Element bisher nur das Loch und das Blatt im Grundriss.

use super::{abgeleitet, gid, round9, Bericht, Ctx, Grund};
use crate::geometrie;
use crate::mengen;
use crate::step::{e, lens, r, refs, s, typed, V};
use sk_math::{vec3, Vec3};
use sk_model::oeffnung::DoorLeaf;
use sk_model::oeffnung_koerper::OpeningVoid;
use sk_model::{ElementId, ElementKind, Guid, StoreyId};
use std::collections::HashMap;

/// `OperationType` eines Drehflügels aus der Lage des Blatts. IFC4
/// (`IfcDoorTypeOperationEnum`): Die Tür schlägt nach +y ihrer Lage auf,
/// links und rechts gelten in Blickrichtung +y, also von der Seite, von der
/// aus man das Blatt aufdrückt. DIN 107 sieht von der Öffnungsfläche, aus
/// dem Raum, in den sie aufschlägt: DIN links ist in IFC rechts.
fn drehfluegel(l: &DoorLeaf) -> (Vec3, Vec3, &'static str) {
    let y = l.swing_into;
    let x = vec3(y.y, -y.x, 0.0);
    let mitte = (l.line.0 + l.line.1) * 0.5;
    let art = if (l.hinge - mitte).dot(x) < 0.0 {
        "SINGLE_SWING_LEFT"
    } else {
        "SINGLE_SWING_RIGHT"
    };
    // Ursprung am linken Ende der Blattlinie, in Blickrichtung +y
    let links = if (l.line.1 - l.line.0).dot(x) < 0.0 {
        l.line.1
    } else {
        l.line.0
    };
    (links, x, art)
}

impl Ctx<'_> {
    /// Die Öffnungen `ids` an ihren Wänden, die schon in der Datei stehen.
    pub(super) fn oeffnungen(
        &mut self,
        ids: &[ElementId],
        geschosse: &HashMap<StoreyId, (u32, u32, f64)>,
        bericht: &mut Bericht,
    ) {
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
            // Das Blatt sitzt in einer Wand seines Geschosses
            let blatt = m
                .door_leaf(id)
                .and_then(|l| Some((m.wall_at(l.run, l.seg)?, l)))
                .filter(|_| geschosse.contains_key(&el.storey));
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
                if let Some((_, l)) = blatt.as_ref().filter(|(w, _)| *w == v.wall) {
                    let (si, _, _) = geschosse[&el.storey];
                    let (z0, _) = m.opening_band(op).unwrap_or((elev, elev));
                    let tuer = self.tuer(
                        id,
                        g,
                        l,
                        (pl, elev, z0.max(elev)),
                        v,
                        w.category.is_outer_wall(),
                    );
                    self.inhalt.entry(si).or_default().push(tuer);
                    self.w.add(
                        "IFCRELFILLSELEMENT",
                        vec![
                            gid(abgeleitet(g, "fills")),
                            r(self.oh),
                            V::Null,
                            V::Null,
                            r(oi),
                            r(tuer),
                        ],
                    );
                    bericht.tueren += 1;
                }
            }
            bericht.oeffnungen += 1;
        }
    }

    /// `IfcDoor` der Öffnung `id` im Leerraum `v` (Kennung der Öffnung dort
    /// `g`), relativ zur Lage der Öffnung `pl` in einem Geschoss auf Höhe
    /// `elev`, Unterkante `z0` (absolut).
    fn tuer(
        &mut self,
        id: ElementId,
        g: Guid,
        l: &DoorLeaf,
        (pl, elev, z0): (u32, f64, f64),
        v: &OpeningVoid,
        aussen: bool,
    ) -> u32 {
        let el = self
            .m
            .element(id)
            .expect("Öffnung aus der Liste des Modells");
        let (o, x, art) = drehfluegel(l);
        let p = self
            .w
            .add("IFCCARTESIANPOINT", vec![lens(&[o.x, o.y, z0 - elev])]);
        let up = self.w.add(
            "IFCDIRECTION",
            vec![V::List(vec![V::Real(0.0), V::Real(0.0), V::Real(1.0)])],
        );
        let dx = self.w.add(
            "IFCDIRECTION",
            vec![V::List(vec![
                V::Real(round9(x.x)),
                V::Real(round9(x.y)),
                V::Real(0.0),
            ])],
        );
        let a = self.w.add("IFCAXIS2PLACEMENT3D", vec![r(p), r(up), r(dx)]);
        let lage = self.w.add("IFCLOCALPLACEMENT", vec![r(pl), r(a)]);
        let gt = abgeleitet(g, "door");
        let wand = self.m.element(v.wall).map_or("", |w| w.number.as_str());
        let tuer = self.w.add(
            "IFCDOOR",
            vec![
                gid(gt),
                r(self.oh),
                s(&el.number),
                s(&format!("Tür in {wand}")),
                V::Null,
                r(lage),
                V::Null,
                s(&el.number),
                V::Len(v.height),
                V::Len(v.width),
                e("DOOR"),
                e(art),
                V::Null,
            ],
        );
        let din = self
            .m
            .opening(id)
            .and_then(|o| o.swing)
            .map(|sw| sw.din.name());
        let mut eigen = vec![self.single("Nummer", typed("IFCIDENTIFIER", s(&el.number)))];
        if let Some(din) = din {
            eigen.push(self.single("Öffnungsrichtung", typed("IFCLABEL", s(din))));
        }
        self.pset(
            abgeleitet(gt, "Skizzeo_Bauteil"),
            "Skizzeo_Bauteil",
            &eigen,
            tuer,
        );
        let ext = self.single("IsExternal", typed("IFCBOOLEAN", V::Bool(aussen)));
        self.pset(
            abgeleitet(gt, "Pset_DoorCommon"),
            "Pset_DoorCommon",
            &[ext],
            tuer,
        );
        if let Some(satz) = mengen::tuer(v.width, v.height) {
            self.mengensatz(abgeleitet(gt, satz.name), &satz, tuer);
        }
        tuer
    }
}

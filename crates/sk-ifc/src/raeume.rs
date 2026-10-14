//! Räume und Ausbau in der Datei (I6, ifc/plan.md §5,
//! grundriss/schnittstelle-raum.md §5, raumbuch/plan.md §9) aus der
//! Raumliste des Raumbuchs ([`sk_raum::raumliste::raumliste`]). Je
//! gespeichertem Raum mit Fläche und Ausbau ein `IfcSpace`: Name die
//! Raumnummer, LongName der Raumname, Form der Raumkörper über dem
//! Fertigumriss von OK Fertigfußboden bis UK Fertigdecke, Mengen in
//! `Qto_SpaceBaseQuantities`, Nutzung und Wohnfläche in `Skizzeo_Raum`,
//! Nutzungsgruppe als Klassifikation DIN 277.
//!
//! Der Ausbau (dieselben Körper wie in der Ansicht) steht je Fläche und
//! Ausbautyp als `IfcCovering`: Boden und Estrich `FLOORING`, Decke
//! `CEILING`, Wandbekleidung und Wandfliesen je Wandseite `CLADDING`,
//! Sockelleisten je Typ `SKIRTINGBOARD`; mit Schichtsatz in wirksamer Dicke
//! (Sockelleiste mit ihrem Baustoff), `IfcCoveringType` je Ausbautyp, im
//! Geschoss enthalten, über `IfcRelCoversSpaces` am Raum und über
//! `IfcRelCoversBldgElements` am bekleideten Bauteil (Wand, Decke,
//! Sohlplatte). Ausbau aus Schichten des Wandtyps bleibt in der Wand
//! (Regel 5.6: der Putz zählt beim Wandtyp, nie doppelt). Keine
//! `IfcRelSpaceBoundary` (später).

use super::{abgeleitet, atv, gid, opt, round9, Bericht, Ctx, Grund};
use crate::geometrie::{self, Netz};
use crate::mengen;
use crate::step::{e, lens, r, refs, s, typed, V};
use sk_math::vec3;
use sk_model::{
    Category, ElementId, ElementKind, Guid, MaterialLayer, Model, StoreyId, TypeCategory,
};
use sk_raum::ausbau;
use sk_raum::oeffnungen::Geschosse;
use sk_raum::raumbuch::Flaeche;
use sk_raum::raumliste::{raumliste, AusbauDaten, RaumDaten};
use sk_raum::{Begrenzung, Kante};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Name einer Fläche im Raum, Wandseiten ab 1 gezählt.
fn flaechenname(f: Flaeche) -> String {
    match f {
        Flaeche::Boden => "Boden".into(),
        Flaeche::Estrich => "Estrich".into(),
        Flaeche::Decke => "Decke".into(),
        Flaeche::Wand(k) => format!("Wand {}", k + 1),
        Flaeche::Fliesen(k) => format!("Fliesen Wand {}", k + 1),
        Flaeche::Sockel => "Sockel".into(),
        _ => format!("{f:?}"),
    }
}

/// Zweck für abgeleitete Kennungen einer Fläche.
fn flaechenzweck(f: Flaeche) -> String {
    match f {
        Flaeche::Wand(k) => format!("wand.{k}"),
        Flaeche::Fliesen(k) => format!("fliesen.{k}"),
        _ => format!("{f:?}").to_ascii_lowercase(),
    }
}

/// Zweck für die Kennung einer Bekleidung. Eine Wandseite hängt an der
/// Guid ihrer Wand, der Seite und der Folge längs der Wand (zwei Kanten an
/// derselben Seite, etwa neben einem Wandstummel), nicht an der Zählung der
/// Raumkanten: die Kennung bleibt, wenn sich deren Reihenfolge ändert.
fn bekleidungszweck(m: &Model, kanten: &[Kante], f: Flaeche) -> String {
    let (art, k) = match f {
        Flaeche::Wand(k) => ("wand", k),
        Flaeche::Fliesen(k) => ("fliesen", k),
        _ => return format!("covering.{}", flaechenzweck(f)),
    };
    let Some(Kante {
        quelle: Begrenzung::Wand {
            wand, seite, von, ..
        },
        ..
    }) = kanten.get(k)
    else {
        return format!("covering.{}", flaechenzweck(f));
    };
    let folge = kanten
        .iter()
        .filter(|x| {
            matches!(x.quelle, Begrenzung::Wand { wand: w, seite: s, von: v, .. }
                if w == *wand && s == *seite && v < *von)
        })
        .count();
    let g = m.element(*wand).map_or(Guid(0), |e| e.guid);
    format!("covering.{art}.{}.{seite:?}.{folge}", g.to_ifc())
}

/// `PredefinedType` des `IfcCovering` und seines Typs nach der Art des
/// Ausbautyps.
fn bekleidungsart(c: TypeCategory) -> &'static str {
    match c {
        TypeCategory::FloorFinish | TypeCategory::Screed => "FLOORING",
        TypeCategory::CeilingFinish => "CEILING",
        TypeCategory::Skirting => "SKIRTINGBOARD",
        _ => "CLADDING",
    }
}

/// Menge der ersten Schicht (mm bzw. mm²) aller Ausbauflächen, für die
/// `f` gilt.
fn summe(ausbau: &[AusbauDaten], f: impl Fn(&AusbauDaten) -> bool) -> f64 {
    ausbau
        .iter()
        .filter(|a| f(a))
        .flat_map(|a| a.schichten.iter().filter(|s| s.schicht == 0))
        .map(|s| s.menge)
        .sum()
}

/// Menge einer Bekleidung: die Fläche mit ihrem Typ, an der Wandseite samt
/// Laibungen.
fn bekleidungsmenge(ausbau: &[AusbauDaten], a: &AusbauDaten) -> f64 {
    summe(ausbau, |b| {
        b.typ == a.typ
            && match (a.flaeche, b.flaeche) {
                (Flaeche::Wand(k), Flaeche::Laibung(j)) => k == j,
                (Flaeche::Fliesen(k), Flaeche::FliesenLaibung(j)) => k == j,
                (x, y) => x == y,
            }
    })
}

impl Ctx<'_> {
    /// Dreiecksnetze als Formstücke; mit Farbe je Baustoff, wenn `farbig`.
    fn netzteile(&mut self, netze: &[Netz], farbig: bool) -> Vec<u32> {
        let mut items = Vec::new();
        for n in netze {
            let coords: Vec<V> = n.punkte.iter().map(|p| lens(&[p.x, p.y, p.z])).collect();
            let pl = self.w.add("IFCCARTESIANPOINTLIST3D", vec![V::List(coords)]);
            let idx: Vec<V> = n
                .dreiecke
                .iter()
                .map(|t| V::List(t.iter().map(|&i| V::Int(i as i64 + 1)).collect()))
                .collect();
            let closed = if geometrie::geschlossen(n) {
                V::Bool(true)
            } else {
                V::Null
            };
            let fs = self.w.add(
                "IFCTRIANGULATEDFACESET",
                vec![r(pl), V::Null, closed, V::List(idx), V::Null],
            );
            if farbig {
                let st = self.style(n.mat);
                self.w
                    .add("IFCSTYLEDITEM", vec![r(fs), refs(&[st]), V::Null]);
            }
            items.push(fs);
        }
        items
    }

    /// Form und Lage im Geschoss (`s_pl`) aus den Netzen.
    fn form_und_lage(&mut self, netze: &[Netz], farbig: bool, s_pl: u32) -> (u32, u32) {
        let items = self.netzteile(netze, farbig);
        let rep = self.w.add(
            "IFCSHAPEREPRESENTATION",
            vec![r(self.body), s("Body"), s("Tessellation"), refs(&items)],
        );
        let shape = self.w.add(
            "IFCPRODUCTDEFINITIONSHAPE",
            vec![V::Null, V::Null, refs(&[rep])],
        );
        let origin = self.placement3(vec3(0.0, 0.0, 0.0));
        let pl = self.w.add("IFCLOCALPLACEMENT", vec![r(s_pl), r(origin)]);
        (shape, pl)
    }

    /// Die gespeicherten Räume `ids` (in der Reihenfolge des Modells) mit
    /// ihrem Ausbau. Nach den Bauteilen, damit die Bekleidungen ihren
    /// Träger finden.
    pub(super) fn raeume_schreiben(
        &mut self,
        ids: &[ElementId],
        geschosse: &HashMap<StoreyId, (u32, u32, f64)>,
        bericht: &mut Bericht,
    ) {
        let m = self.m;
        if ids.is_empty() {
            return;
        }
        let mut liste: HashMap<ElementId, RaumDaten> =
            raumliste(m).into_iter().map(|r| (r.raum, r)).collect();
        // Kanten der Räume mit Fläche (je Geschoss einmal erkannt): für die
        // Kennungen der Wandseiten und den Bericht
        let mut kanten: HashMap<ElementId, Vec<Kante>> = HashMap::new();
        let mut erkannt: HashSet<StoreyId> = HashSet::new();
        // Lage der Geschosse einmal je Export (auf der Sohlplatte?)
        let geschosse_lage = Geschosse::aus(m);
        let bauteile: HashMap<Guid, ElementId> =
            m.elements().iter().map(|(id, e)| (e.guid, id)).collect();
        // Träger → Bekleidungen (IfcRelCoversBldgElements), nach Träger geordnet
        let mut am_traeger: BTreeMap<u32, (Guid, Vec<u32>)> = BTreeMap::new();
        for &id in ids {
            let Some(el) = m.element(id) else { continue };
            let ElementKind::Space(raum) = &el.kind else {
                continue;
            };
            let Some(&(si, s_pl, elev)) = geschosse.get(&el.storey) else {
                bericht
                    .ausgelassen
                    .push((el.number.clone(), Grund::OhneGeschoss));
                continue;
            };
            if erkannt.insert(el.storey) {
                for f in sk_raum::raeume(m, el.storey) {
                    if let Some(r) = f.raum {
                        let mut k = f.umriss;
                        k.extend(f.loecher.into_iter().flatten());
                        kanten.entry(r).or_insert(k);
                    }
                }
            }
            let Some(rd) = liste.remove(&id) else {
                let grund = if kanten.contains_key(&id) {
                    Grund::OhneForm
                } else {
                    Grund::RaumOhneFlaeche
                };
                bericht.ausgelassen.push((el.number.clone(), grund));
                continue;
            };
            let netze = geometrie::netze(&rd.koerper, vec3(0.0, 0.0, elev));
            if netze.is_empty() {
                bericht
                    .ausgelassen
                    .push((el.number.clone(), Grund::OhneForm));
                continue;
            }
            let vorlage = ausbau::vorlage(m, rd.nutzung);
            let q = rd.mengen;

            let (shape, pl) = self.form_und_lage(&netze, false, s_pl);
            let nutzung = vorlage.as_ref().map(|v| v.name.clone()).unwrap_or_default();
            let pi = self.w.add(
                "IFCSPACE",
                vec![
                    gid(el.guid),
                    r(self.oh),
                    s(&el.number),
                    opt(&nutzung),
                    V::Null,
                    r(pl),
                    r(shape),
                    opt(&raum.name),
                    e("ELEMENT"),
                    e("INTERNAL"),
                    V::Len(round9(rd.z_fertig.0 - elev)),
                ],
            );
            self.lage.insert(id, (pi, pl, elev));
            self.raeume.entry(si).or_default().push(pi);
            bericht.raeume += 1;

            // Mengen: roh und fertig aus der Raumliste, Wand und Decke nach
            // VOB/C aus dem Raumbuch
            let satz = mengen::raum(mengen::Raum {
                hoehe: q.hoehe_roh,
                lichte_hoehe: q.hoehe_fertig,
                fussboden: q.fussboden,
                brutto_umfang: q.umfang_roh,
                netto_umfang: q.umfang_fertig,
                brutto_flaeche: q.flaeche_roh,
                netto_flaeche: q.flaeche_fertig,
                wand: summe(&rd.ausbau, |a| {
                    matches!(a.flaeche, Flaeche::Wand(_) | Flaeche::Laibung(_))
                }),
                decke: summe(&rd.ausbau, |a| a.flaeche == Flaeche::Decke),
            });
            if let Some(satz) = satz {
                self.mengensatz(abgeleitet(el.guid, satz.name), &satz, pi);
            }
            let ist = self.single("IsExternal", typed("IFCBOOLEAN", V::Bool(false)));
            self.pset(
                abgeleitet(el.guid, "Pset_SpaceCommon"),
                "Pset_SpaceCommon",
                &[ist],
                pi,
            );
            let faktor = raum
                .faktor
                .or_else(|| vorlage.as_ref().map(|v| f64::from(v.woflv) / 100.0))
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            let mut eigen = vec![
                self.single("Nummer", typed("IFCIDENTIFIER", s(&el.number))),
                self.single("Raumname", typed("IFCLABEL", opt(&raum.name))),
            ];
            if let Some(v) = &vorlage {
                eigen.push(self.single("Nutzung", typed("IFCLABEL", s(&v.name))));
                let gruppe = format!("{} {}", v.group.short(), v.group.name());
                eigen.push(self.single("Nutzungsgruppe", typed("IFCLABEL", s(&gruppe))));
                self.nutzung_nutzer
                    .entry(v.group.short().to_string())
                    .or_insert_with(|| (v.group.name().to_string(), Vec::new()))
                    .1
                    .push(pi);
            }
            eigen.push(self.single(
                "Wohnflächenfaktor",
                typed("IFCNORMALISEDRATIOMEASURE", V::Real(round9(faktor))),
            ));
            eigen.push(self.single(
                "Wohnfläche",
                typed(
                    "IFCAREAMEASURE",
                    V::Real(round9(q.flaeche_fertig * faktor / 1e6)),
                ),
            ));
            self.pset(
                abgeleitet(el.guid, "Skizzeo_Raum"),
                "Skizzeo_Raum",
                &eigen,
                pi,
            );

            // Ausbau je Fläche und Typ
            let traeger = |a: &AusbauDaten| a.traeger.and_then(|g| bauteile.get(&g).copied());
            let auf_sohle = rd
                .ausbau
                .iter()
                .filter(|a| matches!(a.flaeche, Flaeche::Boden | Flaeche::Estrich))
                .find_map(|a| m.element(traeger(a)?))
                .map_or_else(
                    || geschosse_lage.auf_sohle(rd.geschoss),
                    |t| matches!(t.kind, ElementKind::GroundSlab(_)),
                );
            let mut bekleidungen = Vec::new();
            for a in &rd.ausbau {
                let netze: Vec<Netz> = a
                    .schichten
                    .iter()
                    .filter_map(|sd| sd.koerper.as_ref())
                    .flat_map(|k| geometrie::netze(k, vec3(0.0, 0.0, elev)))
                    .collect();
                if netze.is_empty() {
                    continue;
                }
                let Some(ls) = m.layer_set(a.typ) else {
                    continue;
                };
                let traeger = traeger(a);
                // Schichten in wirksamer Dicke (die Dämmung unter dem Estrich
                // gleicht den Belag aus)
                let mut layers: Vec<MaterialLayer> = ls.layers.clone();
                for sd in &a.schichten {
                    if let Some(l) = layers.get_mut(sd.schicht) {
                        l.thickness = sd.dicke;
                    }
                }
                let art = bekleidungsart(ls.category);
                let zweck = bekleidungszweck(m, kanten.get(&id).map_or(&[], |k| k), a.flaeche);
                let g = abgeleitet(el.guid, &zweck);
                let (shape, pl) = self.form_und_lage(&netze, true, s_pl);
                let name = format!("{} {}", el.number, flaechenname(a.flaeche));
                let ci = self.w.add(
                    "IFCCOVERING",
                    vec![
                        gid(g),
                        r(self.oh),
                        s(&name),
                        opt(&ls.name),
                        V::Null,
                        r(pl),
                        r(shape),
                        opt(&ls.code),
                        e(art),
                    ],
                );
                self.inhalt.entry(si).or_default().push(ci);
                bekleidungen.push(ci);
                bericht.bekleidungen += 1;
                if let Some((ti, tg)) = traeger.and_then(|t| {
                    let (ti, _, _) = *self.lage.get(&t)?;
                    Some((ti, m.element(t)?.guid))
                }) {
                    am_traeger
                        .entry(ti)
                        .or_insert_with(|| (abgeleitet(tg, "covers"), Vec::new()))
                        .1
                        .push(ci);
                }

                // Typ und Baustoff
                let key = (a.typ.index(), "IfcCoveringType");
                let t = match self.typen.get(&key) {
                    Some(&t) => t,
                    None => {
                        let tg = abgeleitet(ls.guid, "IfcCoveringType");
                        let t = self.w.add(
                            "IFCCOVERINGTYPE",
                            vec![
                                gid(tg),
                                r(self.oh),
                                s(&ls.name),
                                V::Null,
                                V::Null,
                                V::Null,
                                V::Null,
                                opt(&ls.code),
                                s(ls.category.name()),
                                e(art),
                            ],
                        );
                        if art != "SKIRTINGBOARD" {
                            let set = self.aufbau(&ls.name, &ls.code, &ls.layers);
                            self.stoff_nutzer
                                .entry(set)
                                .or_insert((abgeleitet(tg, "material"), Vec::new()))
                                .1
                                .push(t);
                        }
                        self.typen.insert(key, t);
                        t
                    }
                };
                self.typ_nutzer.entry(t).or_default().push(ci);
                let mat = if art == "SKIRTINGBOARD" {
                    layers.first().and_then(|l| self.stoff(l.material))
                } else {
                    let set = self.aufbau(&ls.name, &ls.code, &layers);
                    match a.flaeche {
                        Flaeche::Boden | Flaeche::Estrich | Flaeche::Decke => {
                            let top = geometrie::grenzen(&netze).map_or(0.0, |b| b.1.z);
                            Some(self.w.add(
                                "IFCMATERIALLAYERSETUSAGE",
                                vec![r(set), e("AXIS3"), e("NEGATIVE"), V::Len(top), V::Null],
                            ))
                        }
                        _ => Some(set),
                    }
                };
                if let Some(mat) = mat {
                    self.stoff_nutzer
                        .entry(mat)
                        .or_insert((abgeleitet(g, "material"), Vec::new()))
                        .1
                        .push(ci);
                }

                // Eigenschaften: Kostengruppe nach Fläche und Träger
                // (raumbuch/plan.md §10), sonst die an der Schicht
                let kern = layers
                    .iter()
                    .enumerate()
                    .max_by(|x, y| x.1.thickness.total_cmp(&y.1.thickness).then(y.0.cmp(&x.0)))
                    .map(|(_, l)| *l);
                let kg = kern.and_then(|l| l.kg).or(match a.flaeche {
                    Flaeche::Boden | Flaeche::Estrich | Flaeche::Sockel => {
                        Some(if auf_sohle { 324 } else { 353 })
                    }
                    Flaeche::Decke => Some(354),
                    Flaeche::Wand(_) | Flaeche::Fliesen(_) => {
                        traeger.map(|w| match m.element(w).map(|e| e.category) {
                            Some(Category::ExteriorWall | Category::Parapet) => 336,
                            _ => 345,
                        })
                    }
                    _ => None,
                });
                let trade = kern
                    .and_then(|l| l.trade.or_else(|| m.material(l.material)?.trade))
                    .and_then(|t| m.trade(t));
                let ref_ = self.single("Reference", typed("IFCIDENTIFIER", s(&ls.code)));
                let ist = self.single("IsExternal", typed("IFCBOOLEAN", V::Bool(false)));
                self.pset(
                    abgeleitet(g, "Pset_CoveringCommon"),
                    "Pset_CoveringCommon",
                    &[ref_, ist],
                    ci,
                );
                let mut eigen = vec![
                    self.single("Nummer", typed("IFCIDENTIFIER", s(&el.number))),
                    self.single("Bauteilart", typed("IFCLABEL", s(ls.category.name()))),
                    self.single("Bauteiltyp", typed("IFCLABEL", s(&ls.name))),
                ];
                if let Some(kg) = kg {
                    eigen.push(
                        self.single("Kostengruppe", typed("IFCIDENTIFIER", s(&kg.to_string()))),
                    );
                    self.kg_nutzer.entry(kg).or_default().push(ci);
                }
                if let Some(t) = trade {
                    let gewerk = format!("{} {}", t.code, t.name);
                    eigen.push(self.single("Gewerk", typed("IFCLABEL", s(&gewerk))));
                    if atv(&t.code) {
                        let name = t.name.trim();
                        let (n, els) = self
                            .atv_nutzer
                            .entry(t.code.clone())
                            .or_insert_with(|| (name.to_string(), Vec::new()));
                        if name < n.as_str() {
                            *n = name.to_string();
                        }
                        els.push(ci);
                    }
                }
                self.pset(
                    abgeleitet(g, "Skizzeo_Bauteil"),
                    "Skizzeo_Bauteil",
                    &eigen,
                    ci,
                );
                let menge = bekleidungsmenge(&rd.ausbau, a);
                let dicke: f64 = layers.iter().map(|l| l.thickness).sum();
                let volumen: f64 = netze.iter().map(geometrie::volumen).sum();
                for satz in mengen::bekleidung(a.flaeche == Flaeche::Sockel, dicke, menge, volumen)
                {
                    self.mengensatz(abgeleitet(g, satz.name), &satz, ci);
                }
            }
            if !bekleidungen.is_empty() {
                self.w.add(
                    "IFCRELCOVERSSPACES",
                    vec![
                        gid(abgeleitet(el.guid, "covers")),
                        r(self.oh),
                        V::Null,
                        V::Null,
                        r(pi),
                        refs(&bekleidungen),
                    ],
                );
            }
        }
        for (ti, (g, els)) in am_traeger {
            self.w.add(
                "IFCRELCOVERSBLDGELEMENTS",
                vec![gid(g), r(self.oh), V::Null, V::Null, r(ti), refs(&els)],
            );
        }
    }

    /// Nutzungsgruppen der Räume als Klassifikation DIN 277.
    pub(super) fn din277_schreiben(&mut self) {
        let gruppen: Vec<(String, (String, Vec<u32>))> = std::mem::take(&mut self.nutzung_nutzer)
            .into_iter()
            .collect();
        if gruppen.is_empty() {
            return;
        }
        let cls = self.w.add(
            "IFCCLASSIFICATION",
            vec![
                s("DIN Deutsches Institut für Normung e. V."),
                s("2021-08"),
                V::Null,
                s("DIN 277"),
                s("Grundflächen und Rauminhalte im Hochbau, Nutzungsgruppen"),
                V::Null,
                V::Null,
            ],
        );
        for (kurz, (name, els)) in gruppen {
            let rf = self.w.add(
                "IFCCLASSIFICATIONREFERENCE",
                vec![V::Null, s(&kurz), s(&name), r(cls), V::Null, V::Null],
            );
            self.w.add(
                "IFCRELASSOCIATESCLASSIFICATION",
                vec![
                    gid(abgeleitet(self.projekt, &format!("din277.{kurz}"))),
                    r(self.oh),
                    s(&format!("DIN 277 {kurz}")),
                    V::Null,
                    refs(&els),
                    r(rf),
                ],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::LevelKind;

    /// Die Kennung einer Wandseite hängt an Wand und Seite: Beginnt der
    /// Umriss an einer anderen Kante, behält jede Wandseite ihren Zweck.
    #[test]
    fn wandseite_unabhaengig_von_der_zaehlung() {
        let mut m = Model::with_seed(9);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let g = m
            .levels_in(Some(b))
            .into_iter()
            .find(|g| m.storey(*g).unwrap().kind == LevelKind::Storey)
            .unwrap();
        m.begin("Räume");
        assert!(sk_raum::abgleichen(&mut m, &[g]) > 0);
        m.commit();
        let kanten = sk_raum::raeume(&m, g).remove(0).umriss;
        assert_eq!(kanten.len(), 4);
        let zwecke = |k: &[Kante]| {
            let mut z: Vec<String> = (0..k.len())
                .map(|i| bekleidungszweck(&m, k, Flaeche::Wand(i)))
                .collect();
            z.sort();
            z
        };
        let vorher = zwecke(&kanten);
        let mut gedreht = kanten.clone();
        gedreht.rotate_left(1);
        assert_eq!(zwecke(&gedreht), vorher);
        // je Wand ein eigener Zweck, Fliesen davon getrennt
        assert!(vorher.windows(2).all(|w| w[0] != w[1]), "{vorher:?}");
        assert!(vorher[0].starts_with("covering.wand."), "{vorher:?}");
        assert!(bekleidungszweck(&m, &kanten, Flaeche::Fliesen(0)).starts_with("covering.fliesen."));
        assert_eq!(
            bekleidungszweck(&m, &kanten, Flaeche::Decke),
            "covering.decke"
        );
    }
}

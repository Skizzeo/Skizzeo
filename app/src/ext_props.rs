//! Paneel „Eigenschaften“ eines gesetzten Erweiterungsbauteils (E7,
//! Vertrag §14): Nummer, Kategorie, Geschoss, Gebäude, die Werte und
//! Mengen mit `anzeigen=ja`, Typ-Chip, dann die Parameter je `gruppe` in
//! der Reihenfolge der [param]-Zeilen. Jede Änderung ist ein
//! Rückgängig-Schritt.

use crate::scene::Scene;
use crate::selection::de;
use crate::ui::{Chip, Field, FieldRow, Id, Props};
use crate::App;
use sk_model::erweiterung::{anzeige, ExtFeld};
use sk_model::{ElementId, ElementKind};

/// Längster Text aus einer .szb im Paneel.
const MAX: usize = 60;

/// Was unter Wert und Mengen steht, je Parameter eine Zeile (oder zwei).
#[derive(Clone, Debug, PartialEq)]
pub enum ExtZeile {
    /// Überschrift der Gruppe („Maße“, wenn keine angegeben).
    Gruppe(&'static str),
    /// Zahl mit Einheit; `hilfe` als Tooltip.
    Feld(FieldRow, String),
    /// `wahl` mit zwei oder drei Möglichkeiten: Knöpfe nebeneinander.
    Wahl {
        i: u8,
        name: &'static str,
        knoepfe: Vec<&'static str>,
        an: Option<u8>,
        hilfe: String,
    },
    /// `wahl` ab vier: ein Knopf mit der gewählten, der die Liste öffnet.
    Liste {
        i: u8,
        name: &'static str,
        text: &'static str,
        hilfe: String,
    },
    /// `janein`: ein Knopf, an ist gelb.
    Janein {
        i: u8,
        name: &'static str,
        an: bool,
        hilfe: String,
    },
}

impl ExtZeile {
    /// Tooltip zum Knopf oder Feld `id`.
    pub fn hilfe(&self, id: Id) -> Option<&str> {
        let h = match (self, id) {
            (ExtZeile::Feld(f, h), Id::Field(x)) if f.field == x => h,
            (ExtZeile::Wahl { i, hilfe, .. }, Id::ExtWahl(j, _))
            | (ExtZeile::Liste { i, hilfe, .. }, Id::ExtListe(j))
            | (ExtZeile::Janein { i, hilfe, .. }, Id::ExtJa(j))
                if *i == j =>
            {
                hilfe
            }
            _ => return None,
        };
        (!h.is_empty()).then_some(h.as_str())
    }
}

/// Zahl wie in der Werkbank: Nachkommastellen je Einheit.
fn wert_text(v: f64, einheit: &str, menge: bool) -> String {
    let (dec, e) = match einheit {
        "mm" | "" => (0, "mm"),
        "stk" => (0, "stk"),
        "m2" => (2, "m²"),
        "m3" => (3, "m³"),
        "kg" if menge => (1, "kg"),
        "t" if menge => (3, "t"),
        "grad" => (2, "°"),
        "-" => (2, ""),
        e => (2, e),
    };
    if !v.is_finite() {
        return "–".into();
    }
    format!("{} {e}", de(v, dec)).trim_end().to_string()
}

/// Die Zeilen der Parameter, sichtbare je Gruppe.
fn zeilen(felder: &[ExtFeld], scene: &mut Scene) -> Vec<ExtZeile> {
    let mut gruppen: Vec<(String, Vec<(usize, &ExtFeld)>)> = Vec::new();
    for (i, f) in felder.iter().enumerate().filter(|(_, f)| f.sichtbar) {
        let g = if f.gruppe.is_empty() {
            "Maße".to_string()
        } else {
            anzeige(&f.gruppe, MAX)
        };
        match gruppen.iter_mut().find(|x| x.0 == g) {
            Some(x) => x.1.push((i, f)),
            None => gruppen.push((g, vec![(i, f)])),
        }
    }
    let mut out = Vec::new();
    for (g, fs) in gruppen {
        out.push(ExtZeile::Gruppe(scene.bezeichnung(g)));
        for (i, f) in fs {
            let i = i as u8;
            let name = scene.bezeichnung(anzeige(&f.name, MAX));
            let hilfe = anzeige(&f.hilfe, 200);
            if f.janein {
                out.push(ExtZeile::Janein {
                    i,
                    name,
                    an: f.wert != 0.0,
                    hilfe,
                });
            } else if (2..=3).contains(&f.wahl.len()) {
                out.push(ExtZeile::Wahl {
                    i,
                    name,
                    knoepfe: f
                        .wahl
                        .iter()
                        .map(|(_, t)| scene.bezeichnung(anzeige(t, MAX)))
                        .collect(),
                    an: f.wahl.iter().position(|w| w.0 == f.wert).map(|k| k as u8),
                    hilfe,
                });
            } else if !f.wahl.is_empty() {
                let text = f
                    .wahl
                    .iter()
                    .find(|w| w.0 == f.wert)
                    .map_or_else(|| ui_zahl(f), |w| anzeige(&w.1, MAX));
                out.push(ExtZeile::Liste {
                    i,
                    name,
                    text: scene.bezeichnung(text),
                    hilfe,
                });
            } else {
                let row = FieldRow {
                    field: Field::ExtProp(i),
                    label: name,
                    value: f.wert,
                    min: f.min.unwrap_or(f64::NEG_INFINITY),
                    max: f.max.unwrap_or(f64::INFINITY),
                    zero: false,
                    einheit: Some(scene.bezeichnung(anzeige(&f.einheit, MAX))),
                };
                out.push(ExtZeile::Feld(row, hilfe));
            }
        }
    }
    out
}

fn ui_zahl(f: &ExtFeld) -> String {
    crate::ui::zahl_text(f.wert)
}

/// Neuer Wert aus Knopf `k` einer Wahl, ohne `k` der umgeschaltete
/// Schalter.
fn wahl_wert(f: &ExtFeld, k: Option<u8>) -> Option<f64> {
    match k {
        Some(k) => f.wahl.get(k as usize).map(|w| w.0),
        None => Some(f64::from(f.wert == 0.0)),
    }
}

/// Typ `k` der Definition; eigene Werte, die er setzt, gehen weg.
fn typ_setzen(d: &sk_model::ExtDef, p: &mut sk_model::ExtPart, k: usize) {
    let Some(t) = d.def.typ.get(k) else {
        return;
    };
    let key = t.key().to_string();
    let gesetzt: Vec<String> = d.typ_werte(&key).into_iter().map(|w| w.0).collect();
    p.typ = Some(key);
    p.werte.retain(|(k, _)| !gesetzt.contains(k));
}

/// Inhalt des Paneels für das Exemplar `id`; `None` für anderes.
pub(crate) fn props(scene: &mut Scene, id: ElementId) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let ElementKind::Ext(p) = &e.kind else {
        return None;
    };
    let d = m.ext_def(&p.key)?.clone();
    let p = p.clone();
    let g = m.ext_geschoss(e.storey);
    let erg = m.ext_ergebnis(id);
    let mut values = vec![
        ("Nummer", e.number.clone()),
        ("Kategorie", anzeige(d.name(), MAX)),
        (
            "Geschoss",
            m.storey(e.storey).map_or("–".into(), |s| s.short.clone()),
        ),
        (
            "Gebäude",
            m.building_of(e.storey)
                .and_then(|b| m.building(b))
                .map_or("–".into(), |b| b.number.clone()),
        ),
    ];
    let mut zeigen = Vec::new();
    if let Some(erg) = &erg {
        for &(k, v) in &erg.werte {
            let Some(r) = d.def.wert.get(k).filter(|r| r.ja("anzeigen")) else {
                continue;
            };
            let name = anzeige(r.get("name").unwrap_or(r.key()), MAX);
            zeigen.push((name, wert_text(v, r.get("einheit").unwrap_or(""), false)));
        }
        for &(k, v) in &erg.mengen {
            let Some(r) = d.def.menge.get(k).filter(|r| r.ja("anzeigen")) else {
                continue;
            };
            let name = anzeige(r.get("name").unwrap_or(r.key()), MAX);
            let einheit = r.get("einheit").unwrap_or("");
            zeigen.push((name, v.map_or("–".into(), |v| wert_text(v, einheit, true))));
        }
    }
    let notes = erg.as_ref().map_or(Vec::new(), |r| {
        r.befunde.iter().map(|b| anzeige(&b.text, 200)).collect()
    });
    let locked = m.is_locked(id);
    let felder = d.felder(&p, &g);
    values.extend(zeigen.into_iter().map(|(k, v)| (scene.bezeichnung(k), v)));
    let chip = (!d.def.typ.is_empty()).then(|| {
        let t = p.typ.as_deref().and_then(|k| d.typ(k));
        Chip {
            name: t.map_or("eigene Werte".to_string(), |t| {
                anzeige(t.get("name").unwrap_or(t.key()), MAX)
            }),
            detail: "Typ".to_string(),
            look: None,
            open: false,
            marked: d.ueberschreibt(&p),
        }
    });
    Some(Props {
        values,
        chip,
        notes,
        locked,
        ext: Some(zeilen(&felder, scene)),
        ..Default::default()
    })
}

impl App {
    /// Eigenschaften des gewählten Bauteils, ob Erweiterung oder nicht.
    pub(crate) fn props_von(&mut self, id: ElementId) -> Option<Props> {
        props(&mut self.scene, id).or_else(|| crate::selection::props(&self.scene, id))
    }

    /// Gewähltes Exemplar mit Definition und Feldern.
    fn ext_gewaehlt(
        &self,
    ) -> Option<(ElementId, sk_model::ExtPart, sk_model::ExtDef, Vec<ExtFeld>)> {
        let id = self.sel.id?;
        let m = self.scene.model();
        let e = m.element(id)?;
        let ElementKind::Ext(p) = &e.kind else {
            return None;
        };
        let d = m.ext_def(&p.key)?;
        let g = m.ext_geschoss(e.storey);
        let f = d.felder(p, &g);
        Some((id, p.clone(), d.clone(), f))
    }

    /// Ist das gewählte Bauteil eine Erweiterung?
    pub(crate) fn ext_gewaehlt_ist(&self) -> bool {
        self.ext_gewaehlt().is_some()
    }

    /// Ändert das gewählte Exemplar in einem Schritt; ein gesperrtes nicht.
    fn ext_aendern(&mut self, was: &str, f: impl FnOnce(&mut sk_model::ExtPart)) {
        let Some((id, mut p, d, _)) = self.ext_gewaehlt() else {
            return;
        };
        if let Some(b) = sk_model::edit_blocked(self.scene.model(), &[id]) {
            self.show_locked(b, Some((id, crate::delete::Act::Field)));
            return;
        }
        let vorher = p.clone();
        f(&mut p);
        if p == vorher {
            return;
        }
        let label = self
            .scene
            .bezeichnung(format!("{} {was}", anzeige(d.name(), MAX)));
        if self.scene.edit_model(label, |m| m.set_ext(id, p)) {
            self.upload_model();
        }
    }

    /// Feld `i` der Eigenschaften: eigener Wert des Exemplars.
    pub(crate) fn ext_prop_feld(&mut self, i: u8, v: f64) {
        let Some(k) = self
            .ext_gewaehlt()
            .and_then(|x| x.3.get(i as usize).map(|f| f.key.clone()))
        else {
            return;
        };
        self.ext_aendern("ändern", |p| p.set(&k, v));
    }

    /// Knopf einer Wahl, Schalter `janein` oder Eintrag einer Liste.
    pub(crate) fn ext_prop_wert(&mut self, i: u8, k: Option<u8>) {
        let Some(f) = self
            .ext_gewaehlt()
            .and_then(|x| x.3.get(i as usize).cloned())
        else {
            return;
        };
        let Some(v) = wahl_wert(&f, k) else {
            return;
        };
        self.ext_aendern("ändern", |p| p.set(&f.key, v));
    }

    /// Typ des Exemplars; eigene Werte, die der Typ setzt, gehen weg.
    pub(crate) fn ext_prop_typ(&mut self, k: usize) {
        let Some((_, _, d, _)) = self.ext_gewaehlt() else {
            return;
        };
        if k < d.def.typ.len() {
            self.ext_aendern("Typ ändern", |p| typ_setzen(&d, p, k));
        }
    }

    /// Einträge der Liste am Knopf `knopf` im Paneel „Eigenschaften“:
    /// Titel, Zeilen und der gewählte Eintrag.
    pub(crate) fn ext_prop_liste(
        &self,
        knopf: Id,
    ) -> Option<(String, Vec<crate::auswahl::Zeile>, Option<usize>)> {
        use crate::auswahl::Zeile;
        let (_, p, d, f) = self.ext_gewaehlt()?;
        match knopf {
            Id::PropsType => {
                let z = d
                    .def
                    .typ
                    .iter()
                    .map(|t| Zeile::Eintrag {
                        name: anzeige(t.get("name").unwrap_or(t.key()), MAX),
                        detail: anzeige(&t.get("werte").unwrap_or("").replace(';', " ·"), MAX),
                    })
                    .collect();
                let cur = p
                    .typ
                    .as_deref()
                    .and_then(|k| d.def.typ.iter().position(|t| t.key() == k));
                Some(("Typ".into(), z, cur))
            }
            Id::ExtListe(i) => {
                let f = f.get(i as usize)?;
                let z = f
                    .wahl
                    .iter()
                    .map(|(v, t)| Zeile::Eintrag {
                        name: anzeige(t, MAX),
                        detail: crate::ui::zahl_text(*v),
                    })
                    .collect();
                let cur = f.wahl.iter().position(|w| w.0 == f.wert);
                Some((anzeige(&f.name, MAX), z, cur))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::erweiterung::ExtDef;
    use sk_model::{ExtPart, Model};

    const GELAENDER: &str = include_str!("../../crates/sk-szb/beispiele/werk.stabgelaender.szb");
    const TREPPE: &str = include_str!("../../crates/sk-szb/beispiele/werk.treppe.szb");

    fn szene(text: &str, l: Option<f64>) -> (Scene, ElementId) {
        let mut m = Model::with_seed(3);
        m.add_building(1);
        let mut s = Scene::with_model(m);
        let eg = s.active_storey();
        let d = ExtDef::einlesen(text).unwrap();
        let mut p = ExtPart::new(&d, [0.0, 0.0]);
        if let Some(l) = l {
            p.set("l", l);
        }
        let mut id = None;
        assert!(s.edit_model("Setzen", |m| {
            m.put_ext_def(d).unwrap();
            id = m.add_ext(eg, p).ok();
            true
        }));
        (s, id.unwrap())
    }

    fn wert<'a>(p: &'a Props, k: &str) -> &'a str {
        &p.values.iter().find(|v| v.0 == k).unwrap().1
    }

    /// Geländer wie in der Werkbank: Gruppen, Füllung als zwei Knöpfe,
    /// Fußleiste als Schalter, Fußleistenhöhe nur mit Fußleiste.
    #[test]
    fn gelaender_wie_in_der_werkbank() {
        let (mut s, id) = szene(GELAENDER, Some(3000.0));
        let p = props(&mut s, id).unwrap();
        assert_eq!(wert(&p, "Nummer"), "GL-001");
        assert_eq!(wert(&p, "Kategorie"), "Stabgeländer");
        assert_eq!(wert(&p, "Geländer"), "3,00 m");
        assert_eq!(p.chip.as_ref().unwrap().name, "Geländer 1,00 m");
        // Die Länge kommt aus der Eingabe, der Typ setzt sie nicht
        assert!(!p.chip.as_ref().unwrap().marked);
        let z = p.ext.as_ref().unwrap();
        let gruppen: Vec<&str> = z
            .iter()
            .filter_map(|z| match z {
                ExtZeile::Gruppe(g) => Some(*g),
                _ => None,
            })
            .collect();
        assert_eq!(gruppen, ["Maße", "Ausführung"]);
        assert!(z.iter().any(|z| matches!(
            z,
            ExtZeile::Wahl { name: "Füllung", knoepfe, an: Some(1), .. } if knoepfe == &["ohne", "Stäbe"]
        )));
        assert!(z.iter().any(|z| matches!(
            z,
            ExtZeile::Janein {
                name: "Fußleiste",
                an: true,
                ..
            }
        )));
        let hoehe = |z: &[ExtZeile]| {
            z.iter()
                .any(|z| matches!(z, ExtZeile::Feld(f, _) if f.label == "Fußleistenhöhe"))
        };
        assert!(hoehe(z));
        // Ohne Fußleiste verschwindet ihre Höhe
        let mut part = match &s.model().element(id).unwrap().kind {
            ElementKind::Ext(p) => p.clone(),
            _ => unreachable!(),
        };
        part.set("fl", 0.0);
        assert!(s.edit_model("aus", |m| m.set_ext(id, part)));
        let p = props(&mut s, id).unwrap();
        assert!(!hoehe(p.ext.as_ref().unwrap()));
        assert!(p.chip.as_ref().unwrap().marked, "fl setzt der Typ");
    }

    /// Typwechsel, Wahl und Schalter am Geländer wie in der Werkbank: der
    /// Typ „ohne Stäbe“ schaltet Füllung und Fußleiste aus, der Schalter
    /// wieder an (eigener Wert, Punkt am Chip).
    #[test]
    fn typ_wahl_und_schalter() {
        let (mut s, id) = szene(GELAENDER, Some(3000.0));
        let teil = |s: &Scene| match &s.model().element(id).unwrap().kind {
            ElementKind::Ext(p) => p.clone(),
            _ => unreachable!(),
        };
        let d = s.model().ext_def("werk.stabgelaender").unwrap().clone();
        let feld = |s: &Scene, k: &str| {
            let g = s.model().ext_geschoss(s.active_storey());
            d.felder(&teil(s), &g)
                .into_iter()
                .find(|f| f.key == k)
                .unwrap()
        };
        let mut p = teil(&s);
        p.set("fl", 1.0);
        typ_setzen(&d, &mut p, 2);
        assert_eq!(p.typ.as_deref(), Some("g100o"));
        assert_eq!(p.werte, [("l".to_string(), 3000.0)], "fl setzt der Typ");
        assert!(s.edit_model("Typ", |m| m.set_ext(id, p)));
        let z = props(&mut s, id).unwrap();
        assert_eq!(z.chip.as_ref().unwrap().name, "Geländer 1,00 m ohne Stäbe");
        let z = z.ext.unwrap();
        assert!(z.iter().any(|z| matches!(
            z,
            ExtZeile::Wahl {
                name: "Füllung",
                an: Some(0),
                ..
            }
        )));
        assert!(z.iter().any(|z| matches!(
            z,
            ExtZeile::Janein {
                name: "Fußleiste",
                an: false,
                ..
            }
        )));
        // Schalter an, Wahl „Stäbe“
        for (k, x) in [("fl", None), ("fu", Some(1))] {
            let v = wahl_wert(&feld(&s, k), x).unwrap();
            assert_eq!(v, 1.0, "{k}");
            let mut p = teil(&s);
            p.set(k, v);
            assert!(s.edit_model(k, |m| m.set_ext(id, p)));
        }
        let z = props(&mut s, id).unwrap();
        assert!(z.chip.as_ref().unwrap().marked);
        assert!(z
            .ext
            .unwrap()
            .iter()
            .any(|z| matches!(z, ExtZeile::Feld(f, _) if f.label == "Fußleistenhöhe")));
        // Jede Änderung ein Schritt
        s.undo();
        assert_eq!(feld(&s, "fu").wert, 0.0);
        assert_eq!(feld(&s, "fl").wert, 1.0);
    }

    /// Treppe: Werte mit anzeigen=ja wie in der Werkbank, Steigungen ganz.
    #[test]
    fn treppe_werte() {
        let (mut s, id) = szene(TREPPE, None);
        let p = props(&mut s, id).unwrap();
        assert_eq!(wert(&p, "Auftritt"), "280 mm");
        assert!(wert(&p, "Beton C25/30").ends_with(" m³"));
        let z = p.ext.unwrap();
        let felder: Vec<&str> = z
            .iter()
            .filter_map(|z| match z {
                ExtZeile::Feld(f, _) => Some(f.label),
                _ => None,
            })
            .collect();
        assert_eq!(felder, ["Lauflänge", "Laufbreite", "Steigungen", "Dicke"]);
        assert!(z
            .iter()
            .any(|z| z.hilfe(Id::Field(Field::ExtProp(0))).is_some()));
    }

    /// Ist-Bilder E7 neben der Werkbank (Reiter „Eigenschaften“), dazu ein
    /// Wert außerhalb von min/max. Nur auf Wunsch:
    /// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder_e7 -- --ignored`
    #[test]
    #[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
    fn istbilder_e7() {
        use crate::ui::{Panel, Ui};
        use sk_paint::Canvas;
        use sk_ui::theme::Theme;
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let mut fonts = sk_ui::widgets::Fonts::system();
        if fonts.regular.is_none() {
            let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
            let lade = |n: &str| {
                std::fs::read(lib.join(n))
                    .ok()
                    .and_then(sk_paint::font::Font::parse)
            };
            fonts = sk_ui::widgets::Fonts {
                regular: lade("LiberationSans-Regular.ttf"),
                bold: lade("LiberationSans-Bold.ttf"),
                italic: lade("LiberationSans-Italic.ttf"),
            };
        }
        if fonts.regular.is_none() {
            return;
        }
        std::fs::create_dir_all(&ziel).unwrap();
        let t = Theme::dark();
        let mut ui = Ui::new(1.0, &t);
        ui.fit(1.0, 1440, 900);
        ui.fonts = fonts;
        // Text, Länge aus der Eingabe, ein eigener Wert
        type Fall<'a> = (&'a str, Option<f64>, Option<(&'a str, f64)>);
        let faelle: [Fall; 5] = [
            (GELAENDER, Some(3000.0), None),
            (
                include_str!("../../crates/sk-szb/beispiele/werk.bodenplatte.szb"),
                None,
                None,
            ),
            (
                include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb"),
                None,
                None,
            ),
            (TREPPE, None, None),
            (GELAENDER, Some(3000.0), Some(("hg", 1500.0))),
        ];
        let mut bilder = Vec::new();
        for (text, l, extra) in faelle {
            let (mut s, id) = szene(text, l);
            if let Some((k, v)) = extra {
                let mut part = match &s.model().element(id).unwrap().kind {
                    ElementKind::Ext(p) => p.clone(),
                    _ => unreachable!(),
                };
                part.set(k, v);
                assert!(s.edit_model("x", |m| m.set_ext(id, part)));
            }
            ui.set_props(props(&mut s, id));
            let (c, ..) = ui.paint(&t, Panel::Props, 1440, 32);
            bilder.push(c.clone());
        }
        let breite: usize = bilder.iter().map(|c| c.width + 20).sum::<usize>() + 20;
        let hoehe = bilder.iter().map(|c| c.height).max().unwrap() + 40;
        let mut m = Canvas::new(breite, hoehe);
        m.fill_rect(0.0, 0.0, breite as f32, hoehe as f32, t.ui.bg);
        let mut x = 20;
        for c in &bilder {
            m.blit(c, x as i32, 20);
            x += c.width + 20;
        }
        std::fs::write(ziel.join("ist-e7-eigenschaften.png"), m.to_png()).unwrap();
    }
}

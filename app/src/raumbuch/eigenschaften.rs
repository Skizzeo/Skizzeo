//! Raumbuch im Paneel „Eigenschaften“ eines Raums (RB3): Nutzung, OK
//! Fertigfußboden, lichte Höhe und je Fläche ihre Menge, darunter blass der
//! Ausbautyp (Einstellungen #112 B2: lange Namen werden gekürzt, statt über
//! die Bezeichnung zu laufen). Decken die Schichten eines Typs ungleich viel
//! (Anstrich nur über den Fliesen, Putz ganz), steht darunter jede Schicht
//! mit ihrer Menge (Bedienbarkeit 40.6). Die Zeilen je Wandseite und
//! Schicht mit Rechenweg zeigt RB5 im Mengenblatt.

use super::{de, de_m};
use sk_model::Model;
use sk_raum::ausbau::{self, RaumAusbau};
use sk_raum::oeffnungen::Rohbau;
use sk_raum::raumbuch::{self, Bezug, Flaeche, Regeln, Zeile};
use sk_raum::{Ketten, RaumForm};

/// Gehört eine Zeile des Raumbuchs zu einer Fläche im Paneel?
type Gehoert = fn(Flaeche) -> bool;

/// Flächen im Paneel und welche Zeilen des Raumbuchs zu ihnen gehören.
const FLAECHEN: [(&str, Gehoert); 6] = [
    // VOB-Menge, mit Türnischen, anders als die Grundfläche (Bedienbarkeit
    // 39.4, 40.5)
    ("Boden (VOB)", |f| f == Flaeche::Boden),
    ("Estrich", |f| f == Flaeche::Estrich),
    ("Wand", |f| matches!(f, Flaeche::Wand(_))),
    ("Wandfliesen", |f| matches!(f, Flaeche::Fliesen(_))),
    ("Decke", |f| f == Flaeche::Decke),
    ("Sockelleiste", |f| f == Flaeche::Sockel),
];

/// Zeilen für das Paneel des Raums mit der Form `form`, die Wandzüge aus
/// `k` (die berechneten der Szene, Review #112 B1), und welche davon die
/// Mengen der Flächen mit ihren Ausbautypen sind: Sie gehören zu Mengen,
/// Nutzung und Höhen zu keinem Modul (Review #120 B2).
pub fn werte(
    m: &Model,
    k: &impl Ketten,
    form: &RaumForm,
) -> (Vec<(&'static str, String)>, std::ops::Range<usize>) {
    let mut v = Vec::new();
    let g = ausbau::nutzung(m, form);
    let Some(s) = ausbau::vorlage(m, g) else {
        return (v, 0..0);
    };
    let gewaehlt = form
        .raum
        .and_then(|id| m.space(id))
        .is_some_and(|s| s.nutzung.is_some());
    v.push(("Nutzung", s.name.clone()));
    if !gewaehlt {
        v.extend([("", VORGABE.into()), ("", VORGABE_WEG.into())]);
    }
    let roh = Rohbau::mit(m, k, form.clone());
    let Some(a) = RaumAusbau::fuer(m, &roh) else {
        return (v, 0..0);
    };
    let (ffb, ufd) = ausbau::hoehen(m, &roh, &a);
    let vz = if ffb < -0.5 { "−" } else { "+" };
    v.push((
        "OK Fertigfußboden",
        format!("{vz}{} m", de(ffb.abs() / 1e3, 3)),
    ));
    v.push(("Lichte Höhe", format!("{} m", de_m(ufd - ffb))));
    let zeilen = raumbuch::rechnen(m, &roh, &a, &Regeln::default());
    let ab = v.len();
    for (name, gehoert) in FLAECHEN {
        if let Some((wert, typen)) = flaeche(m, &zeilen, gehoert) {
            v.push((name, wert));
            v.extend(typen.into_iter().map(|t| ("", t)));
        }
    }
    let mengen = ab..v.len();
    // Die Typen kommen, wenn der Raum entsteht; fehlen sie danach (im
    // Katalog gelöscht), holt eine Änderung der Vorlage sie zurück
    if !ausbau::fehlende_typen(m, form).is_empty() {
        v.extend([("Ausbau", "Typen fehlen".into()), ("", FEHLT_WEG.into())]);
    }
    (v, mengen)
}

/// Unter der Nutzung, wenn keine gewählt ist (Bedienbarkeit 40.5), und
/// wie man sie wählt.
pub const VORGABE: &str = "Nicht gewählt, angenommen";
pub const VORGABE_WEG: &str = "Wahl über den Namen am Stempel";

/// Weg zu fehlenden Ausbautypen (Einstellungen #112 B1): Jede Änderung der
/// Vorlage im Reiter „Räume“ des Katalogs legt alle ihre Typen an; fehlt
/// der Estrich des Geschosses, wählt man dort einen.
pub const FEHLT_WEG: &str = "Vorlage im Katalog ändern";

/// Je Schicht (Nummer im Ausbautyp) die Menge.
type JeSchicht = Vec<(usize, f64)>;

/// Menge der Fläche als Wert und je Ausbautyp eine Zeile darunter; mit
/// einem Typ dessen Name, mit mehreren Menge und Name, damit das Kürzen nur
/// den Namen trifft. Die Menge eines Typs ist die seiner größten Schicht,
/// also der ganzen Fläche. Decken seine Schichten ungleich viel, folgt je
/// Schicht von der Rohbauseite aus eine Zeile „Gipsputz 1,5 cm: 16,55 m²“.
/// `None` ohne Zeile.
fn flaeche(m: &Model, zeilen: &[Zeile], gehoert: Gehoert) -> Option<(String, Vec<String>)> {
    // Je Typ die Einheit und je Schicht die Summe
    let mut je_typ: Vec<(sk_model::LayerSetId, Bezug, JeSchicht)> = Vec::new();
    for z in zeilen.iter().filter(|z| gehoert(z.flaeche)) {
        let Some((id, j)) = z.typ else {
            continue;
        };
        let i = match je_typ.iter().position(|x| x.0 == id && x.1 == z.bezug) {
            Some(i) => i,
            None => {
                je_typ.push((id, z.bezug, Vec::new()));
                je_typ.len() - 1
            }
        };
        let schichten = &mut je_typ[i].2;
        match schichten.iter_mut().find(|x| x.0 == j) {
            Some(x) => x.1 += z.menge(),
            None => schichten.push((j, z.menge())),
        }
    }
    let je_typ: Vec<(&sk_model::LayerSet, Bezug, JeSchicht)> = je_typ
        .into_iter()
        .filter_map(|(id, bezug, mut schichten)| {
            schichten.sort_by_key(|x| std::cmp::Reverse(x.0));
            Some((m.layer_set(id)?, bezug, schichten))
        })
        .collect();
    let ganz = |schichten: &[(usize, f64)]| schichten.iter().map(|x| x.1).fold(0.0, f64::max);
    // Summe je Einheit; eine Fläche hat in der Regel nur eine
    let mut summen: Vec<(Bezug, f64)> = Vec::new();
    for (_, bezug, schichten) in &je_typ {
        match summen.iter_mut().find(|x| x.0 == *bezug) {
            Some(x) => x.1 += ganz(schichten),
            None => summen.push((*bezug, ganz(schichten))),
        }
    }
    let wert: Vec<String> = summen.iter().map(|&(b, x)| menge(b, x)).collect();
    let mehrere = je_typ.len() > 1;
    let mut typen = Vec::new();
    for (t, bezug, schichten) in &je_typ {
        typen.push(if mehrere {
            format!("{} {}", menge(*bezug, ganz(schichten)), t.name)
        } else {
            t.name.clone()
        });
        let texte: Vec<String> = schichten.iter().map(|&(_, x)| menge(*bezug, x)).collect();
        if texte.iter().all(|x| *x == texte[0]) {
            continue;
        }
        for (&(j, _), x) in schichten.iter().zip(texte) {
            typen.push(format!("{}: {x}", schicht(m, t, j)));
        }
    }
    if typen.is_empty() {
        return None;
    }
    Some((wert.join(" + "), typen))
}

/// Schicht `j` des Typs `t`: Baustoff mit Dicke („Gipsputz 1,5 cm“), ohne
/// Dicke (Anstrich, Tapete) nur der Baustoff.
fn schicht(m: &Model, t: &sk_model::LayerSet, j: usize) -> String {
    let Some(l) = t.layers.get(j) else {
        return "–".into();
    };
    let name = m.material(l.material).map_or("–", |x| x.name.as_str());
    if l.thickness <= 0.0 {
        return name.to_string();
    }
    let cm = l.thickness / 10.0;
    let dec = usize::from((cm - cm.round()).abs() > 1e-6);
    format!("{name} {} cm", de(cm, dec))
}

/// „20,11 m²“, „12,40 m“, „1,300 m³“.
fn menge(bezug: Bezug, x: f64) -> String {
    match bezug {
        Bezug::Laenge => format!("{} m", de(x / 1e3, 2)),
        Bezug::Volumen => format!("{} m³", de(x / 1e9, 3)),
        Bezug::Flaeche => format!("{} m²", de(x / 1e6, 2)),
    }
}

#[cfg(test)]
mod tests {
    use crate::scene::Scene;
    use sk_math::vec3;
    use sk_model::model::BATH_SPACE_GUID;
    use sk_model::{ElementId, Model};

    /// Haus `w` × `h` mm im EG mit seinem Raum.
    fn haus(w: f64, h: f64) -> (Scene, ElementId) {
        let mut s = Scene::with_model(Model::with_seed(85));
        assert!(s.edit_model("Haus", |m| {
            let b = m.add_building(2);
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, h, 0.0),
                vec3(w, h, 0.0),
                vec3(w, 0.0, 0.0),
            ];
            m.build_from_polygon(b, &pts).is_some()
        }));
        let m = s.model();
        let eg = m.levels_in(m.buildings().iter().next().map(|b| b.0))[1];
        let r = m.spaces(eg)[0];
        (s, r)
    }

    fn werte(s: &Scene, r: ElementId) -> Vec<(&'static str, String)> {
        crate::selection::props(s, r).unwrap().values
    }

    fn wert(v: &[(&str, String)], k: &str) -> Option<String> {
        v.iter().find(|x| x.0 == k).map(|x| x.1.clone())
    }

    /// Die Zeile unter `k` (ohne Bezeichnung: der Ausbautyp).
    fn darunter(v: &[(&str, String)], k: &str) -> Option<String> {
        let i = v.iter().position(|x| x.0 == k)?;
        v.get(i + 1).filter(|x| x.0.is_empty()).map(|x| x.1.clone())
    }

    /// Haus 10 × 8 m im EG: Der Raum entsteht mit Vorlage und Ausbautypen
    /// (Einstellungen #112 B1), ohne gewählte Nutzung als Wohnzimmer. Je
    /// Fläche steht die Menge als Wert und der Typ darunter (B2). Fehlt ein
    /// Typ, nennt das Paneel den Weg. Als Bad Fliesen statt Sockelleiste.
    #[test]
    fn raumbuch_im_paneel_des_raums() {
        let (mut s, r) = haus(10000.0, 8000.0);
        let eg = s.model().element(r).unwrap().storey;
        let v = werte(&s, r);
        assert_eq!(wert(&v, "Nutzung").as_deref(), Some("Wohnzimmer"));
        assert_eq!(darunter(&v, "Nutzung").as_deref(), Some(super::VORGABE));
        assert_eq!(wert(&v, "Ausbau"), None, "angelegt mit dem Raum: {v:?}");
        assert_eq!(wert(&v, "OK Fertigfußboden").as_deref(), Some("+0,160 m"));
        assert!(wert(&v, "Sockelleiste").is_some_and(|x| x.ends_with(" m")));
        let boden = s
            .model()
            .space_type(sk_model::model::DEFAULT_SPACE_GUID)
            .unwrap()
            .floor;
        let boden = s.model().type_by_guid(boden.unwrap()).unwrap();
        let name = s.model().layer_set(boden).unwrap().name.clone();
        assert!(wert(&v, "Boden (VOB)").is_some_and(|x| x.ends_with(" m²") && !x.contains(&name)));
        assert_eq!(darunter(&v, "Boden (VOB)"), Some(name));
        // Typen der Vorlage lassen sich nicht löschen, der Estrich des
        // Geschosses schon: Hinweis mit dem Weg
        assert!(s.edit_model("weg", |m| m.remove_type(boden).is_err()));
        let estrich = sk_raum::ausbau::estrich_vorgabe(s.model(), eg);
        let estrich = s.model().type_by_guid(estrich).unwrap();
        assert!(s.edit_model("weg", |m| m.remove_type(estrich).is_ok()));
        let v = werte(&s, r);
        assert_eq!(wert(&v, "Ausbau").as_deref(), Some("Typen fehlen"));
        assert_eq!(darunter(&v, "Ausbau").as_deref(), Some(super::FEHLT_WEG));
        assert_eq!(wert(&v, "Estrich"), None);
        assert!(s.edit_model("Bad", |m| {
            m.set_space(r, |x| x.nutzung = Some(BATH_SPACE_GUID))
                && sk_raum::ausbau::typen_anlegen(m, r)
        }));
        let v = werte(&s, r);
        assert_eq!(wert(&v, "Nutzung").as_deref(), Some("Bad"));
        assert_eq!(darunter(&v, "Nutzung"), None, "gewählt");
        assert_eq!(wert(&v, "OK Fertigfußboden").as_deref(), Some("+0,160 m"));
        let decke = s.model().clear_height(eg);
        let lichte = wert(&v, "Lichte Höhe").unwrap();
        assert_eq!(lichte, format!("{} m", super::de_m(decke - 10.0 - 160.0)));
        for k in ["Boden (VOB)", "Estrich", "Wand", "Wandfliesen", "Decke"] {
            assert!(
                wert(&v, k).is_some_and(|x| x.ends_with(" m²")),
                "{k}: {v:?}"
            );
            assert!(darunter(&v, k).is_some_and(|x| !x.is_empty()), "{k}: {v:?}");
        }
        assert_eq!(wert(&v, "Sockelleiste"), None, "Fliesen ab Boden");
        assert_eq!(wert(&v, "Ausbau"), None);
    }

    /// Bedienbarkeit 40.6: Im Bad deckt der Anstrich die Wand nur über den
    /// Fliesen, der Putz ganz. Der Wert der Wand ist die ganze Fläche, unter
    /// dem Typ steht jede Schicht mit ihrer Menge, der Putz zuerst. Am Boden
    /// decken alle Schichten gleich viel: keine Zeilen je Schicht.
    #[test]
    fn menge_je_schicht() {
        let (mut s, r) = haus(10000.0, 8000.0);
        assert!(s.edit_model("Bad", |m| {
            m.set_space(r, |x| x.nutzung = Some(BATH_SPACE_GUID))
                && sk_raum::ausbau::typen_anlegen(m, r)
        }));
        let v = werte(&s, r);
        let zahl = |t: &str| {
            let t = t.rsplit(' ').nth(1).unwrap();
            t.replace('.', "").replace(',', ".").parse::<f64>().unwrap()
        };
        let i = v.iter().position(|x| x.0 == "Wand").unwrap();
        let wand = zahl(&format!("{} x", v[i].1.trim_end_matches(" m²")));
        let schichten: Vec<&str> = v[i + 2..]
            .iter()
            .take_while(|x| x.0.is_empty() && x.1.contains(": "))
            .map(|x| x.1.as_str())
            .collect();
        assert_eq!(schichten.len(), 2, "{v:?}");
        let (putz, anstrich) = (zahl(schichten[0]), zahl(schichten[1]));
        assert!(
            schichten[0].contains(" cm: "),
            "Putz mit Dicke: {schichten:?}"
        );
        assert!(
            !schichten[1].contains(" cm: "),
            "Anstrich ohne: {schichten:?}"
        );
        assert!(anstrich < putz, "{schichten:?}");
        assert_eq!(wand, putz, "die ganze Wand");
        let j = v.iter().position(|x| x.0 == "Boden (VOB)").unwrap();
        assert!(!v[j + 2].1.contains(": "), "{v:?}");
    }

    /// Ein Raum, der beim Teilen entsteht, bekommt seine Typen im selben
    /// Schritt; Rückgängig nimmt beides zurück.
    #[test]
    fn typen_mit_dem_raum_im_selben_schritt() {
        let (mut s, r) = haus(10000.0, 8000.0);
        let eg = s.model().element(r).unwrap().storey;
        let n = s.model().layer_sets().len();
        assert!(s
            .model()
            .space_type(sk_model::model::DEFAULT_SPACE_GUID)
            .is_some());
        assert!(s.undo());
        assert!(s.model().spaces(eg).is_empty());
        assert!(s
            .model()
            .space_type(sk_model::model::DEFAULT_SPACE_GUID)
            .is_none());
        assert!(s.model().layer_sets().len() < n);
        assert!(s.redo());
        assert_eq!(s.model().layer_sets().len(), n);
    }

    /// Bezeichnung und Wert jeder Zeile passen nebeneinander ins Paneel,
    /// auch mit langem Typnamen (Einstellungen #112 B2; mit Liberation,
    /// wenn sie da ist).
    #[test]
    fn zeilen_passen_ins_paneel() {
        let lib = "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf";
        let Some(f) = std::fs::read(lib)
            .ok()
            .and_then(sk_paint::font::Font::parse)
        else {
            return;
        };
        let t = sk_ui::theme::Theme::dark();
        let innen = t.size.right_width - 2.0 * t.size.panel_pad;
        let (mut s, r) = haus(10000.0, 8000.0);
        let mut alle = werte(&s, r);
        assert!(s.edit_model("Bad", |m| {
            m.set_space(r, |x| x.nutzung = Some(BATH_SPACE_GUID))
                && sk_raum::ausbau::typen_anlegen(m, r)
        }));
        alle.extend(werte(&s, r));
        let px = t.size.font_small;
        for (k, w) in alle.iter().filter(|x| !x.0.is_empty()) {
            let breite = f.width(k, px) + 8.0 + f.width(w, px);
            assert!(breite <= innen, "{k}: {w} ({breite} > {innen})");
        }
    }

    /// Ist-Bilder des Paneels für Wohnzimmer und Bad bei 100 % und 150 %
    /// (Einstellungen #112). Nur auf Wunsch:
    /// `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder_raumbuch -- --ignored`
    #[test]
    #[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
    fn istbilder_raumbuch() {
        use crate::ui::{Panel, Ui};
        use sk_ui::theme::Theme;
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
        let lade = |n: &str| {
            std::fs::read(lib.join(n))
                .ok()
                .and_then(sk_paint::font::Font::parse)
        };
        let fonts = || sk_ui::widgets::Fonts {
            regular: lade("LiberationSans-Regular.ttf"),
            bold: lade("LiberationSans-Bold.ttf"),
            italic: lade("LiberationSans-Italic.ttf"),
        };
        std::fs::create_dir_all(&ziel).unwrap();
        let t = Theme::dark();
        let (wohnen, a) = haus(5000.0, 4000.0);
        let (mut bad, b) = haus(2400.0, 2000.0);
        assert!(bad.edit_model("Bad", |m| {
            m.set_space(b, |x| x.nutzung = Some(BATH_SPACE_GUID))
                && sk_raum::ausbau::typen_anlegen(m, b)
        }));
        for (s, r, name) in [(&wohnen, a, "wohnzimmer"), (&bad, b, "bad")] {
            for (scale, pz) in [(1.0, 100), (1.5, 150)] {
                // Fenster so groß, dass das Paneel nicht schrumpft
                let (w, h) = ((1920.0 * scale) as u32, (1200.0 * scale) as u32);
                let mut ui = Ui::new(scale, &t);
                ui.fit(scale, w, h);
                ui.fonts = fonts();
                ui.set_props(crate::selection::props(s, r));
                let (c, ..) = ui.paint(&t, Panel::Props, w, 32);
                let datei = format!("ist-rb3-paneel-{name}-{pz}.png");
                std::fs::write(ziel.join(datei), c.to_png()).unwrap();
            }
        }
    }
}

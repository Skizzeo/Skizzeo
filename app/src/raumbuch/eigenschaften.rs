//! Raumbuch im Paneel „Eigenschaften“ eines Raums (RB3): Nutzung, OK
//! Fertigfußboden, lichte Höhe und je Fläche der Ausbautyp mit der Menge
//! seiner sichtbaren Schicht (Belag, Anstrich, Fliese, Sockelleiste). Die
//! Zeilen je Wandseite und Schicht mit Rechenweg zeigt RB5 im Mengenblatt.

use super::{de, de_m};
use sk_model::Model;
use sk_raum::ausbau::{self, RaumAusbau};
use sk_raum::oeffnungen::Rohbau;
use sk_raum::raumbuch::{self, Bezug, Flaeche, Regeln, Zeile};
use sk_raum::RaumForm;

/// Gehört eine Zeile des Raumbuchs zu einer Fläche im Paneel?
type Gehoert = fn(Flaeche) -> bool;

/// Flächen im Paneel und welche Zeilen des Raumbuchs zu ihnen gehören.
const FLAECHEN: [(&str, Gehoert); 6] = [
    ("Boden", |f| f == Flaeche::Boden),
    ("Estrich", |f| f == Flaeche::Estrich),
    ("Wand", |f| matches!(f, Flaeche::Wand(_))),
    ("Wandfliesen", |f| matches!(f, Flaeche::Fliesen(_))),
    ("Decke", |f| f == Flaeche::Decke),
    ("Sockelleiste", |f| f == Flaeche::Sockel),
];

/// Zeilen für das Paneel des Raums mit der Form `form`.
pub fn werte(m: &Model, form: &RaumForm) -> Vec<(&'static str, String)> {
    let mut v = Vec::new();
    let g = ausbau::nutzung(m, form);
    let Some(s) = ausbau::vorlage(m, g) else {
        return v;
    };
    let gewaehlt = form
        .raum
        .and_then(|id| m.space(id))
        .is_some_and(|s| s.nutzung.is_some());
    v.push((
        "Nutzung",
        if gewaehlt {
            s.name.clone()
        } else {
            format!("{} (Vorgabe)", s.name)
        },
    ));
    let roh = Rohbau::neu(m, form.clone());
    let Some(a) = RaumAusbau::fuer(m, &roh) else {
        return v;
    };
    let (ffb, ufd) = ausbau::hoehen(m, &roh, &a);
    let vz = if ffb < -0.5 { "−" } else { "+" };
    v.push((
        "OK Fertigfußboden",
        format!("{vz}{} m", de(ffb.abs() / 1e3, 3)),
    ));
    v.push(("Lichte Höhe", format!("{} m", de_m(ufd - ffb))));
    let zeilen = raumbuch::rechnen(m, &roh, &a, &Regeln::default());
    for (name, gehoert) in FLAECHEN {
        if let Some(text) = flaeche(m, &zeilen, gehoert) {
            v.push((name, text));
        }
    }
    if !ausbau::fehlende_typen(m, form).is_empty() {
        v.push(("Ausbau", "Typen der Vorlage fehlen im Projekt".into()));
    }
    v
}

/// „Parkett Eiche 20: 20,11 m²“ je Ausbautyp der Fläche, aus den Zeilen der
/// obersten Schicht; `None` ohne Zeile.
fn flaeche(m: &Model, zeilen: &[Zeile], gehoert: Gehoert) -> Option<String> {
    let mut je_typ: Vec<(sk_model::LayerSetId, Bezug, f64)> = Vec::new();
    for z in zeilen.iter().filter(|z| gehoert(z.flaeche)) {
        let Some((id, 0)) = z.typ else {
            continue;
        };
        match je_typ.iter_mut().find(|x| x.0 == id) {
            Some(x) => x.2 += z.menge(),
            None => je_typ.push((id, z.bezug, z.menge())),
        }
    }
    let teile: Vec<String> = je_typ
        .iter()
        .filter_map(|&(id, bezug, menge)| {
            let name = &m.layer_set(id)?.name;
            Some(match bezug {
                Bezug::Laenge => format!("{name}: {} m", de(menge / 1e3, 2)),
                Bezug::Volumen => format!("{name}: {} m³", de(menge / 1e9, 3)),
                Bezug::Flaeche => format!("{name}: {} m²", de(menge / 1e6, 2)),
            })
        })
        .collect();
    (!teile.is_empty()).then(|| teile.join("; "))
}

#[cfg(test)]
mod tests {
    use crate::scene::Scene;
    use sk_math::vec3;
    use sk_model::model::{BATH_SPACE_GUID, DEFAULT_SPACE_GUID};
    use sk_model::Model;

    /// Haus 10 × 8 m im EG: erst ohne gewählte Nutzung (Wohnzimmer als
    /// Vorgabe), mit angelegtem Ausbau, dann als Bad mit Fliesen statt
    /// Sockelleiste.
    #[test]
    fn raumbuch_im_paneel_des_raums() {
        let mut s = Scene::with_model(Model::with_seed(85));
        assert!(s.edit_model("Haus", |m| {
            let b = m.add_building(2);
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ];
            m.build_from_polygon(b, &pts).is_some()
        }));
        let m = s.model();
        let eg = m.levels_in(m.buildings().iter().next().map(|b| b.0))[1];
        let r = m.spaces(eg)[0];
        let werte = |s: &Scene| crate::selection::props(s, r).unwrap().values;
        let wert = |v: &[(&str, String)], k: &str| v.iter().find(|x| x.0 == k).map(|x| x.1.clone());
        let v = werte(&s);
        assert_eq!(wert(&v, "Nutzung").as_deref(), Some("Wohnzimmer (Vorgabe)"));
        let fehlt = s.model().space_type(DEFAULT_SPACE_GUID).is_none();
        assert_eq!(wert(&v, "Ausbau").is_some(), fehlt, "{v:?}");
        assert!(s.edit_model("Ausbau", |m| sk_raum::ausbau::typen_anlegen(m, r)));
        let v = werte(&s);
        assert_eq!(wert(&v, "OK Fertigfußboden").as_deref(), Some("+0,160 m"));
        assert!(wert(&v, "Sockelleiste").is_some_and(|x| x.ends_with(" m")));
        assert_eq!(wert(&v, "Ausbau"), None);
        assert!(s.edit_model("Bad", |m| {
            m.set_space(r, |x| x.nutzung = Some(BATH_SPACE_GUID))
                && sk_raum::ausbau::typen_anlegen(m, r)
        }));
        let v = werte(&s);
        assert_eq!(wert(&v, "Nutzung").as_deref(), Some("Bad"));
        assert_eq!(wert(&v, "OK Fertigfußboden").as_deref(), Some("+0,160 m"));
        let decke = s.model().clear_height(eg);
        let lichte = wert(&v, "Lichte Höhe").unwrap();
        assert_eq!(lichte, format!("{} m", super::de_m(decke - 10.0 - 160.0)));
        for k in ["Boden", "Estrich", "Wand", "Wandfliesen", "Decke"] {
            assert!(
                wert(&v, k).is_some_and(|x| x.ends_with(" m²")),
                "{k}: {v:?}"
            );
        }
        assert_eq!(wert(&v, "Sockelleiste"), None, "Fliesen ab Boden");
        assert_eq!(wert(&v, "Ausbau"), None);
    }
}

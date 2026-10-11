//! Baum der IFC-Vorschau: Projekt › Grundstück › Gebäude › Geschoss, darin
//! die Bauteile nach Klasse gruppiert, wie ihn Empfänger der Datei sehen.
//! Hier nur die Liste der Zeilen und was ein Klick ändert; gezeichnet wird
//! im Fenster.

use sk_ifc::lesen::{Knoten, Vorschau};
use std::collections::{BTreeMap, BTreeSet};

/// Wofür eine Zeile steht.
#[derive(Clone, Debug, PartialEq)]
pub enum Ziel {
    /// Räumlicher Knoten (Projekt, Grundstück, Gebäude, Geschoss).
    Knoten { klasse: String },
    /// Bauteile einer Klasse unter einem Knoten.
    Gruppe,
    /// Ein Bauteil (Index in [`Vorschau::teile`]).
    Teil(usize),
}

/// Eine Zeile der Baumliste.
#[derive(Clone, Debug, PartialEq)]
pub struct Zeile {
    /// Schlüssel für Auf- und Zuklappen.
    pub schluessel: String,
    pub tiefe: usize,
    pub text: String,
    /// Grau dahinter (Klasse, Höhe).
    pub zusatz: String,
    pub ziel: Ziel,
    /// `Some(offen)`, wenn die Zeile etwas unter sich hat.
    pub offen: Option<bool>,
    /// Alle Bauteile darunter (für Ein- und Ausblenden).
    pub teile: Vec<usize>,
}

/// Wie viele der Bauteile sichtbar sind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sicht {
    Alle,
    Teils,
    Keine,
}

pub fn sicht(teile: &[usize], sichtbar: &[bool]) -> Sicht {
    let n = teile
        .iter()
        .filter(|&&i| sichtbar.get(i).copied().unwrap_or(true))
        .count();
    match n {
        0 => Sicht::Keine,
        n if n == teile.len() => Sicht::Alle,
        _ => Sicht::Teils,
    }
}

/// Kästchen geklickt: alles darunter aus, wenn alles sichtbar ist, sonst an.
pub fn umschalten(teile: &[usize], sichtbar: &mut [bool]) {
    let an = sicht(teile, sichtbar) != Sicht::Alle;
    for &i in teile {
        if let Some(s) = sichtbar.get_mut(i) {
            *s = an;
        }
    }
}

/// Nur diese Bauteile zeigen („Geschosse einzeln“); sind sie es schon
/// allein, wieder alle.
pub fn allein(teile: &[usize], sichtbar: &mut [bool]) {
    let schon = sichtbar
        .iter()
        .enumerate()
        .all(|(i, &s)| s == teile.contains(&i));
    for (i, s) in sichtbar.iter_mut().enumerate() {
        *s = schon || teile.contains(&i);
    }
}

/// Deutsche Bezeichnung der räumlichen Klassen.
fn knoten_art(klasse: &str) -> &str {
    match klasse {
        "IfcProject" => "Projekt",
        "IfcSite" => "Grundstück",
        "IfcBuilding" => "Gebäude",
        "IfcBuildingStorey" => "Geschoss",
        "IfcSpace" => "Raum",
        k => k,
    }
}

/// Höhe in Metern mit Vorzeichen wie in Geschosslisten: „+2,750 m“.
fn hoehe_text(mm: f64) -> String {
    let m = mm / 1000.0;
    let s = if m.abs() < 0.0005 {
        "±0,000".to_string()
    } else {
        format!("{m:+.3}").replace('.', ",")
    };
    format!("{s} m")
}

/// Das Projekt als Wurzel über dem gelesenen Baum.
fn wurzel(v: &Vorschau) -> Knoten {
    Knoten {
        klasse: "IfcProject".into(),
        name: v.projekt.clone(),
        kinder: v.baum.clone(),
        ..Knoten::default()
    }
}

/// Sichtbare Zeilen. `umgeklappt` hält die Schlüssel, die vom Anfang
/// abweichen: Knoten sind anfangs offen, Klassengruppen zu.
pub fn zeilen(v: &Vorschau, umgeklappt: &BTreeSet<String>) -> Vec<Zeile> {
    let mut out = Vec::new();
    knoten(v, &wurzel(v), "0", 0, umgeklappt, &mut out);
    out
}

/// Alle Bauteile unter einem Knoten.
fn alle_teile(k: &Knoten, out: &mut Vec<usize>) {
    out.extend(&k.teile);
    for c in &k.kinder {
        alle_teile(c, out);
    }
}

fn knoten(
    v: &Vorschau,
    k: &Knoten,
    pfad: &str,
    tiefe: usize,
    umgeklappt: &BTreeSet<String>,
    out: &mut Vec<Zeile>,
) {
    let mut teile = Vec::new();
    alle_teile(k, &mut teile);
    let hat = !k.kinder.is_empty() || !k.teile.is_empty();
    let offen = hat && !umgeklappt.contains(pfad);
    let art = knoten_art(&k.klasse);
    let mut text = match (k.name.is_empty(), k.langname.is_empty()) {
        (true, true) => art.to_string(),
        (true, false) => k.langname.clone(),
        (false, true) => k.name.clone(),
        (false, false) if k.name == k.langname => k.name.clone(),
        (false, false) => format!("{} {}", k.name, k.langname),
    };
    let zusatz = if k.klasse == "IfcBuildingStorey" {
        hoehe_text(k.hoehe)
    } else if text != art {
        art.to_string()
    } else {
        String::new()
    };
    if text.is_empty() {
        text = k.klasse.clone();
    }
    out.push(Zeile {
        schluessel: pfad.to_string(),
        tiefe,
        text,
        zusatz,
        ziel: Ziel::Knoten {
            klasse: k.klasse.clone(),
        },
        offen: hat.then_some(offen),
        teile,
    });
    if !offen {
        return;
    }
    for (i, c) in k.kinder.iter().enumerate() {
        knoten(v, c, &format!("{pfad}/{i}"), tiefe + 1, umgeklappt, out);
    }
    // Bauteile nach Klasse, darin nach Name
    let mut gruppen: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for &t in &k.teile {
        if let Some(teil) = v.teile.get(t) {
            gruppen.entry(&teil.klasse).or_default().push(t);
        }
    }
    for (klasse, mut ts) in gruppen {
        ts.sort_by(|&a, &b| v.teile[a].name.cmp(&v.teile[b].name).then(a.cmp(&b)));
        let s = format!("{pfad}#{klasse}");
        let auf = umgeklappt.contains(&s);
        out.push(Zeile {
            schluessel: s.clone(),
            tiefe: tiefe + 1,
            text: klasse.to_string(),
            zusatz: ts.len().to_string(),
            ziel: Ziel::Gruppe,
            offen: Some(auf),
            teile: ts.clone(),
        });
        if !auf {
            continue;
        }
        for t in ts {
            let teil = &v.teile[t];
            out.push(Zeile {
                schluessel: format!("{s}/{t}"),
                tiefe: tiefe + 2,
                text: if teil.name.is_empty() {
                    teil.klasse.clone()
                } else {
                    teil.name.clone()
                },
                zusatz: if teil.netze.is_empty() {
                    "ohne Form".into()
                } else {
                    String::new()
                },
                ziel: Ziel::Teil(t),
                offen: None,
                teile: vec![t],
            });
        }
    }
}

/// Klappt die Gruppe mit dem Bauteil `t` auf (Wahl im Bild), damit es im
/// Baum zu sehen ist. Gibt an, ob sich etwas geändert hat.
pub fn aufdecken(v: &Vorschau, t: usize, umgeklappt: &mut BTreeSet<String>) -> bool {
    fn suche(k: &Knoten, pfad: &str, t: usize, weg: &mut Vec<String>) -> bool {
        if k.teile.contains(&t) {
            weg.push(pfad.to_string());
            return true;
        }
        for (i, c) in k.kinder.iter().enumerate() {
            if suche(c, &format!("{pfad}/{i}"), t, weg) {
                weg.push(pfad.to_string());
                return true;
            }
        }
        false
    }
    let Some(teil) = v.teile.get(t) else {
        return false;
    };
    let mut weg = Vec::new();
    suche(&wurzel(v), "0", t, &mut weg);
    let Some(innen) = weg.first().cloned() else {
        return false;
    };
    let mut neu = false;
    // Knoten: offen heißt nicht umgeklappt
    for p in &weg {
        neu |= umgeklappt.remove(p);
    }
    // Gruppe: offen heißt umgeklappt
    neu |= umgeklappt.insert(format!("{innen}#{}", teil.klasse));
    neu
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sk_ifc::lesen::Teil;

    fn teil(klasse: &str, name: &str) -> Teil {
        Teil {
            klasse: klasse.into(),
            name: name.into(),
            ..Teil::default()
        }
    }

    pub(crate) fn beispiel() -> Vorschau {
        let geschoss = |name: &str, hoehe: f64, teile: Vec<usize>| Knoten {
            klasse: "IfcBuildingStorey".into(),
            name: name.into(),
            hoehe,
            teile,
            ..Knoten::default()
        };
        Vorschau {
            schema: "IFC4".into(),
            projekt: "Haus".into(),
            baum: vec![Knoten {
                klasse: "IfcSite".into(),
                kinder: vec![Knoten {
                    klasse: "IfcBuilding".into(),
                    name: "GB-01".into(),
                    langname: "Gebäude 1".into(),
                    kinder: vec![
                        geschoss("EG", 0.0, vec![0, 1, 2]),
                        geschoss("OG", 2750.0, vec![3]),
                    ],
                    ..Knoten::default()
                }],
                ..Knoten::default()
            }],
            teile: vec![
                teil("IfcWall", "Wand b"),
                teil("IfcWall", "Wand a"),
                teil("IfcSlab", "Bodenplatte"),
                teil("IfcWall", "Wand c"),
            ],
            ohne_form: 4,
        }
    }

    /// Anfangs offen bis zum Geschoss, Klassen zu; Aufklappen zeigt die
    /// Bauteile nach Name.
    #[test]
    fn zeilen_und_klappen() {
        let v = beispiel();
        let mut um = BTreeSet::new();
        let z = zeilen(&v, &um);
        let texte: Vec<_> = z.iter().map(|z| (z.tiefe, z.text.as_str())).collect();
        assert_eq!(
            texte,
            [
                (0, "Haus"),
                (1, "Grundstück"),
                (2, "GB-01 Gebäude 1"),
                (3, "EG"),
                (4, "IfcSlab"),
                (4, "IfcWall"),
                (3, "OG"),
                (4, "IfcWall"),
            ]
        );
        assert_eq!(z[0].zusatz, "Projekt");
        assert_eq!(z[1].zusatz, "");
        assert_eq!(z[2].zusatz, "Gebäude");
        assert_eq!(z[3].zusatz, "±0,000 m");
        assert_eq!(z[6].zusatz, "+2,750 m");
        assert_eq!(z[5].zusatz, "2");
        assert_eq!(z[0].teile.len(), 4);
        assert_eq!(z[5].offen, Some(false));
        um.insert(z[5].schluessel.clone());
        let z = zeilen(&v, &um);
        assert_eq!(z[6].text, "Wand a");
        assert_eq!(z[7].text, "Wand b");
        assert_eq!(z[6].ziel, Ziel::Teil(1));
        assert_eq!(z[6].zusatz, "ohne Form");
        // Geschoss zuklappen
        um.insert(z[3].schluessel.clone());
        let z = zeilen(&v, &um);
        assert_eq!(z[3].offen, Some(false));
        assert_eq!(z[4].text, "OG");
    }

    #[test]
    fn sichtbarkeit() {
        let mut s = vec![true; 4];
        assert_eq!(sicht(&[0, 1, 2], &s), Sicht::Alle);
        umschalten(&[0, 1, 2], &mut s);
        assert_eq!(s, [false, false, false, true]);
        assert_eq!(sicht(&[0, 1, 2, 3], &s), Sicht::Teils);
        umschalten(&[0, 1, 2, 3], &mut s);
        assert_eq!(s, [true; 4]);
        allein(&[3], &mut s);
        assert_eq!(s, [false, false, false, true]);
        allein(&[3], &mut s);
        assert_eq!(s, [true; 4], "zweimal: wieder alle");
    }

    /// Wahl im Bild klappt den Weg bis zum Bauteil auf.
    #[test]
    fn aufdecken_zeigt_das_teil() {
        let v = beispiel();
        let mut um = BTreeSet::new();
        um.insert("0/0/0/1".to_string());
        assert!(aufdecken(&v, 3, &mut um));
        let z = zeilen(&v, &um);
        assert!(z.iter().any(|z| z.ziel == Ziel::Teil(3)));
        assert!(!aufdecken(&v, 3, &mut um), "schon zu sehen");
    }
}

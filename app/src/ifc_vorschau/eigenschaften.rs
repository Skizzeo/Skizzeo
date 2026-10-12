//! Rechte Spalte der IFC-Vorschau: was die Datei über das gewählte Bauteil
//! sagt (Klasse, Typ, Schichten, Baustoffe, Klassifikation, Eigenschafts-
//! und Mengensätze), ohne Wahl eine Übersicht der Datei.

use sk_ifc::lesen::{Teil, Vorschau};

/// Eine Zeile der Spalte.
#[derive(Clone, Debug, PartialEq)]
pub enum Eintrag {
    /// Überschrift eines Abschnitts.
    Kopf(String),
    /// Bezeichnung und Wert.
    Wert(String, String),
}

fn wert(k: &str, v: impl Into<String>) -> Eintrag {
    Eintrag::Wert(k.into(), v.into())
}

/// Dicke in mm wie in den Bauteilpaneelen: ganze Zahl, sonst eine Stelle.
fn mm(d: f64) -> String {
    if (d - d.round()).abs() < 0.05 {
        format!("{} mm", d.round())
    } else {
        format!("{d:.1} mm").replace('.', ",")
    }
}

/// Übersicht, wenn nichts gewählt ist.
pub fn uebersicht(v: &Vorschau) -> Vec<Eintrag> {
    let mut out = vec![
        Eintrag::Kopf("Datei".into()),
        wert("Schema", v.schema.clone()),
        wert("Projekt", v.projekt.clone()),
        wert("Bauteile", v.teile.len().to_string()),
    ];
    if v.ohne_form > 0 {
        out.push(wert("davon ohne Form", v.ohne_form.to_string()));
    }
    let mut klassen = std::collections::BTreeMap::<&str, usize>::new();
    for t in &v.teile {
        *klassen.entry(&t.klasse).or_default() += 1;
    }
    if !klassen.is_empty() {
        out.push(Eintrag::Kopf("Klassen".into()));
        out.extend(klassen.into_iter().map(|(k, n)| wert(k, n.to_string())));
    }
    out
}

/// Alles über ein Bauteil.
pub fn bauteil(t: &Teil) -> Vec<Eintrag> {
    let mut out = vec![Eintrag::Kopf("Bauteil".into())];
    out.push(wert("Klasse", t.klasse.clone()));
    for (k, v) in [
        ("Art", &t.art),
        ("Name", &t.name),
        ("Beschreibung", &t.beschreibung),
        ("Typ", &t.typ),
    ] {
        if !v.is_empty() {
            out.push(wert(k, v.clone()));
        }
    }
    out.push(wert("GUID", t.guid.clone()));
    if t.netze.is_empty() {
        out.push(wert("Form", "keine in der Datei"));
    }
    if !t.schichten.is_empty() {
        out.push(Eintrag::Kopf("Schichten".into()));
        let mut summe = 0.0;
        for (stoff, d) in &t.schichten {
            out.push(wert(stoff, mm(*d)));
            summe += d;
        }
        if t.schichten.len() > 1 {
            out.push(wert("Summe", mm(summe)));
        }
    } else if !t.baustoffe.is_empty() {
        out.push(Eintrag::Kopf("Baustoffe".into()));
        out.extend(t.baustoffe.iter().map(|b| wert(b, "")));
    }
    if !t.klassen.is_empty() {
        out.push(Eintrag::Kopf("Klassifikation".into()));
        out.extend(t.klassen.iter().map(|k| wert(k, "")));
    }
    for (satz, werte) in &t.saetze {
        out.push(Eintrag::Kopf(satz.clone()));
        out.extend(werte.iter().map(|(k, v)| wert(k, v.clone())));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_ifc::lesen::Netz;

    #[test]
    fn bauteil_mit_schichten_und_saetzen() {
        let t = Teil {
            guid: "0abc".into(),
            klasse: "IfcWall".into(),
            name: "Außenwand".into(),
            art: "SOLID".into(),
            netze: vec![Netz::default()],
            schichten: vec![
                ("Putz".into(), 15.0),
                ("KS".into(), 175.0),
                ("WDVS".into(), 160.5),
            ],
            klassen: vec!["DIN 276: 331".into()],
            saetze: vec![(
                "Pset_WallCommon".into(),
                vec![("IsExternal".into(), "wahr".into())],
            )],
            ..Teil::default()
        };
        let e = bauteil(&t);
        assert_eq!(e[0], Eintrag::Kopf("Bauteil".into()));
        assert!(e.contains(&wert("Art", "SOLID")));
        assert!(!e
            .iter()
            .any(|e| matches!(e, Eintrag::Wert(k, _) if k == "Typ")));
        assert!(e.contains(&wert("WDVS", "160,5 mm")));
        assert!(e.contains(&wert("Summe", "350,5 mm")));
        assert!(e.contains(&Eintrag::Kopf("Klassifikation".into())));
        assert_eq!(e.last(), Some(&wert("IsExternal", "wahr")));
        assert!(!e.contains(&wert("Form", "keine in der Datei")));
    }

    #[test]
    fn uebersicht_zaehlt_klassen() {
        let v = Vorschau {
            schema: "IFC4".into(),
            teile: vec![
                Teil {
                    klasse: "IfcWall".into(),
                    ..Teil::default()
                },
                Teil {
                    klasse: "IfcWall".into(),
                    ..Teil::default()
                },
            ],
            ohne_form: 2,
            ..Vorschau::default()
        };
        let e = uebersicht(&v);
        assert!(e.contains(&wert("Bauteile", "2")));
        assert!(e.contains(&wert("davon ohne Form", "2")));
        assert_eq!(e.last(), Some(&wert("IfcWall", "2")));
    }
}

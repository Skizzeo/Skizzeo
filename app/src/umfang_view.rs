//! Umfang über den Blättern (KA-1, architektur/paket-ka1.md): Geschoss-Chips
//! und ab zwei Gebäuden das Gebäudefeld. Hier wird nichts gerechnet: das
//! Blatt liest seine Liste mit [`crate::scene::Scene::schedule_in`] im
//! [`Umfang`], den diese Leiste liefert. KA-2 und KA-4 zeigen dieselbe
//! Leiste über ihren Blättern.
//!
//! Der Umfang gilt für die Sitzung. Er steht nicht in der Datei, nicht in
//! `einstellungen.txt` und ist kein Rückgängig-Schritt.

use crate::scene::FOUNDATION_NAME;
use sk_model::qto::Umfang;
use sk_model::{BuildingId, LevelKind, Model, StoreyId};

/// Ein Geschoss-Chip: Name und die Geschosse dahinter (bei „Projekt“ alle
/// gleichnamigen Geschosse der Gebäude und die losen).
#[derive(Clone, Debug, PartialEq)]
pub struct Chip {
    pub name: String,
    pub geschosse: Vec<StoreyId>,
}

/// Kurzname eines Geschosses wie im Bogen: „Fundament“, „EG“, „OG“.
fn kurzname(m: &Model, id: StoreyId) -> String {
    match m.storey(id) {
        Some(s) if s.kind == LevelKind::Foundation => FOUNDATION_NAME.into(),
        Some(s) => s.short.clone(),
        None => String::new(),
    }
}

/// Gebäude des Modells in ihrer Reihenfolge.
pub fn gebaeude(m: &Model) -> Vec<BuildingId> {
    m.buildings().iter().map(|(id, _)| id).collect()
}

/// Chips in der Reihenfolge des Geschossbogens, von unten nach oben: die
/// Geschosse des Gebäudes `g`, bei `None` (Projekt) ein Chip je Name über
/// alle Gebäude und die losen Geschosse.
pub fn umfang_chips(m: &Model, g: Option<BuildingId>) -> Vec<Chip> {
    let mut st: Vec<(StoreyId, f64)> = m
        .storeys()
        .iter()
        .filter(|(_, s)| g.is_none() || s.building == g)
        .map(|(id, s)| (id, s.elevation))
        .collect();
    st.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut out: Vec<Chip> = Vec::new();
    for (id, _) in st {
        let name = kurzname(m, id);
        match out.iter_mut().find(|c| c.name == name) {
            Some(c) => c.geschosse.push(id),
            None => out.push(Chip {
                name,
                geschosse: vec![id],
            }),
        }
    }
    out
}

/// Einträge des Gebäudefelds ab zwei Gebäuden: „Projekt“, dann die Gebäude
/// mit ihren Geschossen (leise daneben). Leer bei höchstens einem Gebäude.
pub fn feld(m: &Model) -> Vec<(Option<BuildingId>, String, String)> {
    let gb = gebaeude(m);
    if gb.len() < 2 {
        return Vec::new();
    }
    let mut out = vec![(None, "Projekt".to_string(), String::new())];
    for g in gb {
        let name = m.building(g).map_or(String::new(), |b| b.name.clone());
        let geschosse = umfang_chips(m, Some(g))
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        out.push((Some(g), name, geschosse));
    }
    out
}

/// Bringt den Umfang auf den Modellstand (paket-ka1.md §3): gelöschte
/// Geschosse fallen aus `ohne`, neue sind gewählt, außer ihr Chip ist
/// abgewählt; ein gelöschtes Gebäude gilt als Projekt; bei höchstens einem
/// Gebäude gibt es kein Gebäudefeld und es gilt Projekt. Ist kein Chip mehr
/// gewählt, gelten alle. `true`, wenn sich etwas ändert.
pub fn bereinigen(m: &Model, u: &mut Umfang) -> bool {
    let vorher = u.clone();
    u.ohne.retain(|s| m.storey(*s).is_some());
    let gb = gebaeude(m);
    if gb.len() < 2 || u.gebaeude.is_some_and(|g| !gb.contains(&g)) {
        u.gebaeude = None;
    }
    let chips = umfang_chips(m, u.gebaeude);
    if !chips.is_empty() && chips.iter().all(|c| !an(u, c)) {
        u.ohne.clear();
    }
    // Ein abgewählter Chip lässt alle seine Geschosse weg, auch ein neues
    // Geschoss gleichen Namens im Projekt (Liste wie Chip)
    let aus: Vec<StoreyId> = chips
        .iter()
        .filter(|c| !an(u, c))
        .flat_map(|c| c.geschosse.iter().copied())
        .filter(|s| !u.ohne.contains(s))
        .collect();
    u.ohne.extend(aus);
    *u != vorher
}

/// Ist der Chip gewählt? Ein Chip über mehrere Geschosse ist gewählt, wenn
/// keines davon abgewählt ist.
pub fn an(u: &Umfang, c: &Chip) -> bool {
    c.geschosse.iter().all(|s| !u.ohne.contains(s))
}

/// Sind alle Chips gewählt?
pub fn alle_an(u: &Umfang, chips: &[Chip]) -> bool {
    chips.iter().all(|c| an(u, c))
}

/// „Alle“: jeden Chip wählen.
pub fn alle(u: &mut Umfang) {
    u.ohne.clear();
}

/// Klick auf Chip `i` (Bedienbarkeit 3.1): Sind alle gewählt, zeigt er nur
/// diesen; danach nimmt jeder Klick einen dazu oder weg. Strg+Klick wählt
/// nur diesen. `false`: abgelehnt, weil es der letzte gewählte Chip ist
/// (er wackelt), oder `i` gibt es nicht.
pub fn klick(u: &mut Umfang, chips: &[Chip], i: usize, strg: bool) -> bool {
    let Some(c) = chips.get(i) else {
        return false;
    };
    let nur = |u: &mut Umfang| {
        u.ohne = chips
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .flat_map(|(_, c)| c.geschosse.iter().copied())
            .collect();
    };
    if strg || alle_an(u, chips) {
        nur(u);
        return true;
    }
    if an(u, c) {
        if chips.iter().filter(|c| an(u, c)).count() <= 1 {
            return false;
        }
        u.ohne.extend(c.geschosse.iter().copied());
    } else {
        u.ohne.retain(|s| !c.geschosse.contains(s));
    }
    true
}

/// Tooltip am Chip (paket-ka1.md §1), zweite Zeile mit Strg+Klick.
pub fn tooltip(u: &Umfang, chips: &[Chip], i: usize) -> String {
    let Some(c) = chips.get(i) else {
        return String::new();
    };
    let erste = if alle_an(u, chips) {
        format!("Nur {} zeigen", c.name)
    } else if an(u, c) {
        format!("{} weglassen", c.name)
    } else {
        format!("{} dazunehmen", c.name)
    };
    format!("{erste}\nStrg+Klick: nur dieses Geschoss")
}

/// Name des Umfangs im Gebäudefeld und in der Kopfzeile: das Gebäude, bei
/// einem einzigen Gebäude dieses, sonst „Projekt“.
pub fn umfang_name(m: &Model, u: &Umfang) -> String {
    let gb = gebaeude(m);
    let g = u.gebaeude.or(match gb.as_slice() {
        [g] => Some(*g),
        _ => None,
    });
    match g.and_then(|g| m.building(g)) {
        Some(b) => b.name.clone(),
        None => "Projekt".into(),
    }
}

/// Name im Gebäudefeld: der gewählte Eintrag, sonst „Projekt“.
pub fn umfang_name_von(feld: &[(Option<BuildingId>, String, String)], u: &Umfang) -> String {
    feld.iter()
        .find(|(g, _, _)| *g == u.gebaeude)
        .map_or("Projekt".into(), |(_, n, _)| n.clone())
}

/// Kopfzeile unter dem Titel und erste Zeile der Tabelle (paket-ka1.md §3):
/// „Gebäude 1 · alle Geschosse · Stand 08.10.2026, 11:34“ bzw. mit den
/// gewählten Geschossen „EG + OG“. `uhr`: Jahr, Monat, Tag, Stunde, Minute.
pub fn umfang_text(m: &Model, u: &Umfang, uhr: (u16, u8, u8, u8, u8)) -> String {
    let chips = umfang_chips(m, u.gebaeude);
    let geschosse = if alle_an(u, &chips) {
        "alle Geschosse".to_string()
    } else {
        chips
            .iter()
            .filter(|c| an(u, c))
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let (y, mo, d, h, mi) = uhr;
    format!(
        "{} · {geschosse} · Stand {d:02}.{mo:02}.{y}, {h:02}:{mi:02}",
        umfang_name(m, u)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const UHR: (u16, u8, u8, u8, u8) = (2026, 10, 8, 11, 34);

    /// Standardhaus RH-1 (Fundament, EG, OG, ein Gebäude).
    fn haus() -> Model {
        sk_model::szo::read_with(
            include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
            sk_model::GuidGen::with_seed(1),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .expect("lädt")
        .model
    }

    fn namen(c: &[Chip]) -> Vec<&str> {
        c.iter().map(|c| c.name.as_str()).collect()
    }

    /// Abnahme 1–3 (paket-ka1.md §5): Chips von unten nach oben, Klickfolge,
    /// letzter Chip bleibt, Kopfzeile.
    #[test]
    fn chips_und_klickfolge() {
        let m = haus();
        let mut u = Umfang::projekt();
        bereinigen(&m, &mut u);
        let c = umfang_chips(&m, u.gebaeude);
        assert_eq!(namen(&c), ["Fundament", "EG", "OG"]);
        assert!(alle_an(&u, &c));
        assert!(umfang_text(&m, &u, UHR).ends_with(" · alle Geschosse · Stand 08.10.2026, 11:34"));
        assert_eq!(
            tooltip(&u, &c, 1),
            "Nur EG zeigen\nStrg+Klick: nur dieses Geschoss"
        );
        // EG: nur EG
        assert!(klick(&mut u, &c, 1, false));
        assert_eq!(c.iter().filter(|c| an(&u, c)).count(), 1);
        assert!(an(&u, &c[1]));
        assert_eq!(
            tooltip(&u, &c, 2),
            "OG dazunehmen\nStrg+Klick: nur dieses Geschoss"
        );
        assert!(umfang_text(&m, &u, UHR).contains(" · EG · Stand"));
        // letzter gewählter Chip bleibt
        assert!(!klick(&mut u, &c, 1, false));
        assert!(an(&u, &c[1]));
        // OG dazu: EG + OG
        assert!(klick(&mut u, &c, 2, false));
        assert!(umfang_text(&m, &u, UHR).contains(" · EG + OG · Stand"));
        assert_eq!(
            tooltip(&u, &c, 2),
            "OG weglassen\nStrg+Klick: nur dieses Geschoss"
        );
        // EG weg: nur OG
        assert!(klick(&mut u, &c, 1, false));
        assert!(!an(&u, &c[1]) && an(&u, &c[2]) && !an(&u, &c[0]));
        // Strg+Klick auf Fundament: nur Fundament
        assert!(klick(&mut u, &c, 0, true));
        assert!(an(&u, &c[0]) && !an(&u, &c[1]) && !an(&u, &c[2]));
        // Alle
        alle(&mut u);
        assert!(alle_an(&u, &c));
    }

    /// Abnahme 3 (Teil): Ist kein Chip mehr gewählt, gelten alle.
    #[test]
    fn bereinigen_ohne_gewaehlte() {
        let m = haus();
        let mut u = Umfang::projekt();
        assert!(!bereinigen(&m, &mut u));
        // alle abgewählt geht nicht
        let c = umfang_chips(&m, None);
        u.ohne = c.iter().flat_map(|c| c.geschosse.clone()).collect();
        assert!(bereinigen(&m, &mut u));
        assert!(alle_an(&u, &c));
    }

    /// Abnahme 4 (Summenprobe, Regel 96): die Geschosse einzeln ergeben
    /// zusammen die Mengen und Summen aller Geschosse, je Baustoff, Gewerk
    /// und Kostengruppe auf die mm-Einheit.
    #[test]
    fn summenprobe_je_geschoss() {
        use std::collections::HashMap;
        let m = haus();
        let ganz = sk_model::qto::schedule(&m);
        let c = umfang_chips(&m, None);
        type Summen = HashMap<String, f64>;
        fn summen(s: &sk_model::qto::Schedule, out: &mut Summen) {
            let mut add = |k: String, v: f64| *out.entry(k).or_default() += v;
            for b in &s.buildings {
                for x in &b.by_material {
                    add(format!("B{:?}v", x.material), x.volume);
                    add(format!("B{:?}a", x.material), x.area.unwrap_or(0.0));
                    add(format!("B{:?}l", x.material), x.length.unwrap_or(0.0));
                }
                for x in &b.by_trade {
                    add(format!("G{:?}v", x.trade), x.volume);
                    add(format!("G{:?}a", x.trade), x.area.unwrap_or(0.0));
                    add(format!("G{:?}l", x.trade), x.length.unwrap_or(0.0));
                    add(format!("G{:?}n", x.trade), x.rows.len() as f64);
                }
                for x in &b.by_kg {
                    add(format!("K{}v", x.kg), x.volume);
                    add(format!("K{}n", x.kg), x.rows.len() as f64);
                }
            }
        }
        let mut soll = Summen::new();
        summen(&ganz, &mut soll);
        let mut ist = Summen::new();
        for i in 0..c.len() {
            let mut u = Umfang::projekt();
            assert!(klick(&mut u, &c, i, true));
            let t = ganz.restrict(&m, &u);
            assert!(t
                .buildings
                .iter()
                .flat_map(|b| &b.storeys)
                .all(|s| c[i].geschosse.contains(&s.id)));
            summen(&t, &mut ist);
        }
        assert!(soll.len() > 10, "{soll:?}");
        assert_eq!(soll.len(), ist.len());
        for (k, v) in &soll {
            assert!((ist[k] - v).abs() < 1.0, "{k}: {} statt {v}", ist[k]);
        }
    }

    /// Abnahme 5 und 6: Zwei Gebäude geben das Gebäudefeld; bei „Projekt“
    /// ein Chip je Name für beide; ein neues Gebäude kommt gewählt dazu, ein
    /// gelöschtes nimmt seine Geschosse und die Wahl im Feld mit.
    #[test]
    fn zwei_gebaeude() {
        let mut m = haus();
        let mut u = Umfang::projekt();
        assert!(feld(&m).is_empty());
        // nur EG, dann kommt ein Nebengebäude mit Gründung und EG dazu
        let c = umfang_chips(&m, None);
        assert!(klick(&mut u, &c, 1, false));
        m.begin("Gebäude");
        let b2 = m.add_building(1);
        m.commit();
        assert!(
            bereinigen(&m, &mut u),
            "Gründung des Nebengebäudes bleibt weg"
        );
        let f = feld(&m);
        let namen_feld: Vec<&str> = f.iter().map(|x| x.1.as_str()).collect();
        assert_eq!(namen_feld.len(), 3);
        assert_eq!(namen_feld[0], "Projekt");
        assert_eq!(
            f[2],
            (
                Some(b2),
                namen_feld[2].to_string(),
                "Fundament · EG".to_string()
            )
        );
        let c = umfang_chips(&m, None);
        assert_eq!(namen(&c), ["Fundament", "EG", "OG"]);
        assert_eq!(c[1].geschosse.len(), 2, "ein EG-Chip für beide");
        assert!(an(&u, &c[1]) && !an(&u, &c[0]) && !an(&u, &c[2]));
        let t = sk_model::qto::schedule(&m).restrict(&m, &u);
        assert_eq!(t.buildings.len(), 2);
        // das leere Nebengebäude hat keine Mengenzeilen
        assert!(t
            .buildings
            .iter()
            .flat_map(|b| &b.storeys)
            .all(|s| c[1].geschosse.contains(&s.id)));
        assert_eq!(t.buildings[0].storeys.len(), 1);
        assert!(umfang_text(&m, &u, UHR).starts_with("Projekt · EG · Stand"));
        // Abwahl von EG nimmt beide heraus (OG dazu, dann EG weg)
        assert!(klick(&mut u, &c, 2, false));
        assert!(klick(&mut u, &c, 1, false));
        let t = sk_model::qto::schedule(&m).restrict(&m, &u);
        assert!(t
            .buildings
            .iter()
            .all(|b| b.storeys.iter().all(|s| c[2].geschosse.contains(&s.id))));
        // Nebengebäude gewählt: seine Chips; gelöscht: wieder Projekt
        let mut u = Umfang::gebaeude(b2);
        assert!(!bereinigen(&m, &mut u));
        assert_eq!(namen(&umfang_chips(&m, u.gebaeude)), ["Fundament", "EG"]);
        assert_eq!(umfang_name_von(&f, &u), f[2].1);
        let c2 = umfang_chips(&m, u.gebaeude);
        assert!(klick(&mut u, &c2, 1, false));
        m.begin("Löschen");
        assert!(m.remove_building(b2));
        m.commit();
        assert!(bereinigen(&m, &mut u));
        assert_eq!(u, Umfang::projekt());
        assert!(feld(&m).is_empty());
    }
}

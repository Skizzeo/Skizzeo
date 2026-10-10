//! Gewerke nach VOB/C (Paket 1a, bim/paket-1a-gewerke.md): eine feste
//! Tabelle mit Startbestand in der Reihenfolge des Bauablaufs. Baustoffe
//! schlagen ein Gewerk vor, eine Schicht kann abweichen
//! ([`crate::Model::layer_trade`]).
//!
//! Gewerke werden über ihre Guid angesprochen ([`TradeId`]); die
//! Startgewerke haben feste Guids, deshalb passen Projekt und Firmenkatalog
//! ohne Umschlüsseln zusammen.

use crate::guid::Guid;
use crate::library::MatCategory;

/// Verweis auf ein Gewerk (seine Guid).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TradeId(pub Guid);

/// Gewerk (Leistungsbereich nach VOB/C, ATV DIN 18299 ff.).
#[derive(Clone, Debug, PartialEq)]
pub struct Trade {
    pub guid: Guid,
    /// ATV-Nummer, z. B. „18330“; Text, weil Firmen eigene Nummern führen.
    pub code: String,
    pub name: String,
    /// Bauablauf: Sortierung in Mengen und Baum.
    pub order: u16,
    /// Kurzname für Baum, Karten und Hinweise (Regel 67, BIM
    /// gewerke-kurznamen.md); `Some("")`: ausdrücklich entfernt, dann gilt
    /// der Langname. Mengen, CSV und IFC nutzen ATV-Nummer und Langnamen.
    pub short: Option<String>,
}

impl Trade {
    pub fn id(&self) -> TradeId {
        TradeId(self.guid)
    }

    /// Name im Baum: Kurzname, sonst Langname.
    pub fn display_name(&self) -> &str {
        match self.short.as_deref() {
            Some(k) if !k.is_empty() => k,
            _ => &self.name,
        }
    }

    /// Kurzname des Startbestands, wenn dies ein Startgewerk ist.
    pub fn start_short(&self) -> Option<&'static str> {
        SHORT
            .iter()
            .find(|s| s.0 == self.code && start_guid(s.0) == self.guid)
            .map(|s| s.1)
    }
}

/// Höchstlänge eines Kurznamens in Zeichen (Regel 67).
pub const SHORT_MAX: usize = 14;

/// Taugt `k` als Kurzname (Regel 67): höchstens 14 Zeichen, keine Ziffer,
/// kein Zeilenumbruch? Leer heißt „entfernt“.
pub fn short_ok(k: &str) -> bool {
    k.chars().count() <= SHORT_MAX && !k.chars().any(|c| c.is_ascii_digit() || c == '\n')
}

/// Kurznamen eindeutig ohne Groß-/Kleinschrift (Regel 67): ein doppelter
/// fällt beim Laden weg, mit Hinweis; das Gewerk bekommt seinen Startnamen
/// zurück, falls der frei ist, sonst keinen.
pub fn dedup_shorts(trades: &mut [Trade]) -> Vec<String> {
    let mut hints = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for t in trades.iter_mut() {
        let Some(k) = t.short.clone().filter(|k| !k.is_empty()) else {
            continue;
        };
        let low = k.to_lowercase();
        if !seen.contains(&low) {
            seen.push(low);
            continue;
        }
        hints.push(format!(
            "Gewerk {}: Kurzname „{k}“ doppelt, entfernt",
            t.code
        ));
        let back = t
            .start_short()
            .filter(|s| !seen.contains(&s.to_lowercase()));
        t.short = Some(back.unwrap_or("").to_string());
        if let Some(b) = back {
            seen.push(b.to_lowercase());
        }
    }
    hints
}

/// Startbestand: (Reihe, ATV, Name).
pub const START: [(u16, &str, &str); 35] = [
    (1, "18459", "Abbruch- und Rückbauarbeiten"),
    (2, "18451", "Gerüstarbeiten"),
    (3, "18300", "Erdarbeiten"),
    (4, "18308", "Drän- und Versickerarbeiten"),
    (5, "18331", "Betonarbeiten"),
    (6, "18330", "Mauerarbeiten"),
    (7, "18336", "Abdichtungsarbeiten"),
    (8, "18334", "Zimmer- und Holzbauarbeiten"),
    (9, "18335", "Stahlbauarbeiten"),
    (10, "18338", "Dachdeckungs- und Dachabdichtungsarbeiten"),
    (11, "18339", "Klempnerarbeiten"),
    (12, "18355", "Tischlerarbeiten"),
    (13, "18360", "Metallbauarbeiten"),
    (14, "18361", "Verglasungsarbeiten"),
    (15, "18358", "Rollladenarbeiten"),
    (16, "18345", "Wärmedämm-Verbundsysteme"),
    (17, "18351", "Vorgehängte hinterlüftete Fassaden"),
    (18, "18350", "Putz- und Stuckarbeiten"),
    (19, "18340", "Trockenbauarbeiten"),
    (20, "18353", "Estricharbeiten"),
    (21, "18352", "Fliesen- und Plattenarbeiten"),
    (22, "18332", "Naturwerksteinarbeiten"),
    (23, "18333", "Betonwerksteinarbeiten"),
    (24, "18356", "Parkett- und Holzpflasterarbeiten"),
    (25, "18365", "Bodenbelagarbeiten"),
    (26, "18357", "Beschlagarbeiten"),
    (27, "18363", "Maler- und Lackierarbeiten"),
    (28, "18366", "Tapezierarbeiten"),
    (29, "18379", "Raumlufttechnische Anlagen"),
    (
        30,
        "18380",
        "Heizanlagen und zentrale Wassererwärmungsanlagen",
    ),
    (
        31,
        "18381",
        "Gas-, Wasser- und Entwässerungsanlagen innerhalb von Gebäuden",
    ),
    (32, "18382", "Nieder- und Mittelspannungsanlagen"),
    (33, "18384", "Blitzschutzanlagen"),
    (
        34,
        "18385",
        "Förderanlagen, Aufzugsanlagen, Fahrtreppen und Fahrsteige",
    ),
    (35, "18386", "Gebäudeautomation"),
];

/// Kurznamen der Startgewerke: (ATV, Kurzname), BIM Regel 67.
pub const SHORT: [(&str, &str); 35] = [
    ("18459", "Abbruch"),
    ("18451", "Gerüstbau"),
    ("18300", "Erdbau"),
    ("18308", "Drainage"),
    ("18331", "Beton"),
    ("18330", "Maurer"),
    ("18336", "Abdichtung"),
    ("18334", "Zimmerer"),
    ("18335", "Stahlbau"),
    ("18338", "Dachdecker"),
    ("18339", "Klempner"),
    ("18355", "Tischler"),
    ("18360", "Metallbau"),
    ("18361", "Glaser"),
    ("18358", "Rollladen"),
    ("18345", "WDVS"),
    ("18351", "VHF"),
    ("18350", "Putzer"),
    ("18340", "Trockenbau"),
    ("18353", "Estrich"),
    ("18352", "Fliesen"),
    ("18332", "Naturstein"),
    ("18333", "Betonwerkstein"),
    ("18356", "Parkett"),
    ("18365", "Bodenbelag"),
    ("18357", "Beschläge"),
    ("18363", "Maler"),
    ("18366", "Tapezierer"),
    ("18379", "Lüftung"),
    ("18380", "Heizung"),
    ("18381", "Sanitär"),
    ("18382", "Elektro"),
    ("18384", "Blitzschutz"),
    ("18385", "Aufzug"),
    ("18386", "MSR-Technik"),
];

/// Feste Guid eines Startgewerks aus seiner ATV-Nummer.
fn start_guid(code: &str) -> Guid {
    let n: u128 = code.parse().unwrap_or(0);
    Guid(0x5c1e_0a7e_0000_4000_8000_0000_0000_0000 | n)
}

/// Id des Startgewerks mit der ATV-Nummer `code`.
pub fn start_id(code: &str) -> Option<TradeId> {
    START
        .iter()
        .any(|s| s.1 == code)
        .then(|| TradeId(start_guid(code)))
}

/// Die 35 Startgewerke in der Reihenfolge des Bauablaufs.
pub fn start_trades() -> Vec<Trade> {
    START
        .iter()
        .map(|&(order, code, name)| Trade {
            guid: start_guid(code),
            code: code.into(),
            name: name.into(),
            order,
            short: SHORT.iter().find(|s| s.0 == code).map(|s| s.1.to_string()),
        })
        .collect()
}

/// Gewerk eines neu angelegten Baustoffs nach seiner Art (Luft: keins).
pub fn for_category(c: MatCategory) -> Option<TradeId> {
    start_id(match c {
        MatCategory::Masonry => "18330",
        MatCategory::Concrete => "18331",
        MatCategory::Timber => "18334",
        MatCategory::Insulation => "18345",
        MatCategory::Plaster => "18350",
        MatCategory::Metal => "18339",
        MatCategory::Membrane => "18338",
        MatCategory::Air => return None,
    })
}

/// Gewerk eines Startbaustoffs (paket-1a §3), sonst nach Baustoffart.
/// Kerndämmung und Randdämmung setzt der Maurer.
pub fn for_material(name: &str, c: MatCategory) -> Option<TradeId> {
    match name {
        "Kerndämmung (Mineralwolle)" | "Randdämmung" => start_id("18330"),
        _ => for_category(c),
    }
}

/// Untersichtdämmung: Dämmplatten mit Putz als Teil des Fassadensystems,
/// auch wenn ihr Baustoff etwas anderes vorschlägt.
pub fn soffit() -> Option<TradeId> {
    start_id("18345")
}

/// Dachterrasse und Attikablech: Dachdecker (Jörn 08:37, bim/paket-
/// dachterrasse.md §2), auch beim Blech, das nach VOB/C der Klempner wäre.
pub fn roofing() -> Option<TradeId> {
    start_id("18338")
}

/// Startbestand und Gewerke aus einer Datei zusammenführen: gleiche Guid
/// übernimmt Nummer, Name und Reihe der Datei (Firmenanpassung), neue Guids
/// kommen dazu; ohne `short=` gilt der Kurzname des Startbestands. Ergebnis
/// nach Reihe sortiert.
pub fn merge(read: Vec<Trade>) -> Vec<Trade> {
    let mut out = start_trades();
    for t in read {
        match out.iter_mut().find(|x| x.guid == t.guid) {
            Some(x) => {
                let short = t.short.clone().or_else(|| x.short.clone());
                *x = t;
                x.short = short;
            }
            None => out.push(t),
        }
    }
    out.sort_by_key(|t| (t.order, t.guid));
    out
}

/// Regel 48: Nummern eindeutig, Reihen ohne Doppel.
pub fn problems(trades: &[Trade]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, a) in trades.iter().enumerate() {
        for b in &trades[i + 1..] {
            if a.code == b.code {
                out.push(format!("Gewerk {} doppelt vergeben", a.code));
            }
            if a.order == b.order {
                out.push(format!(
                    "Gewerke {} und {}: gleiche Reihe {}",
                    a.code, b.code, a.order
                ));
            }
        }
    }
    out
}

/// Regel 49: eine KG an der Schicht liegt in der Gruppe 300 (311–399).
pub fn valid_kg(kg: u16) -> bool {
    (311..=399).contains(&kg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startbestand() {
        let t = start_trades();
        assert_eq!(t.len(), 35);
        assert!(problems(&t).is_empty());
        assert!(t.windows(2).all(|w| w[0].order < w[1].order));
        assert_eq!(start_id("18330"), Some(t[5].id()));
        assert_eq!(start_id("99999"), None);
    }

    #[test]
    fn merge_behaelt_firmennamen() {
        let mut maurer = start_trades()[5].clone();
        maurer.name = "Maurer- und Betonarbeiten".into();
        let eigen = Trade {
            guid: Guid(7),
            code: "F1".into(),
            name: "Eigenes Gewerk".into(),
            order: 99,
            short: None,
        };
        let m = merge(vec![maurer.clone(), eigen.clone()]);
        assert_eq!(m.len(), 36);
        assert_eq!(m[5], maurer);
        assert_eq!(m.last(), Some(&eigen));
    }
}

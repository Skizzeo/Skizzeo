//! Namen der Kostengruppen nach DIN 276:2018 für Gliederung, Kacheln und
//! CSV (ka-2-fach §2.1): 2. Ebene kurz („320 Gründung“), 3. Ebene mit dem
//! Wortlaut der Norm.

/// Kostengruppe der 2. Ebene zu einer der 3. Ebene (331 → 330).
pub fn ebene2(kg: u16) -> u16 {
    kg / 10 * 10
}

/// Name der Kostengruppe; `None`, wenn sie nicht in der Liste steht.
pub fn name(kg: u16) -> Option<&'static str> {
    Some(match kg {
        310 => "Baugrube/Erdbau",
        320 => "Gründung",
        330 => "Außenwände",
        340 => "Innenwände",
        350 => "Decken",
        360 => "Dächer",
        370 => "Infrastrukturanlagen",
        380 => "Baukonstruktive Einbauten",
        390 => "Sonstige Maßnahmen für Baukonstruktionen",
        311 => "Herstellung",
        312 => "Umschließung",
        313 => "Wasserhaltung",
        314 => "Vortrieb",
        321 => "Baugrundverbesserung",
        322 => "Flachgründungen und Bodenplatten",
        323 => "Tiefgründungen",
        324 => "Gründungsbeläge",
        325 => "Abdichtungen und Bekleidungen",
        326 => "Dränagen",
        331 => "Tragende Außenwände",
        332 => "Nichttragende Außenwände",
        333 => "Außenstützen",
        334 => "Außenwandöffnungen",
        335 => "Außenwandbekleidungen, außen",
        336 => "Außenwandbekleidungen, innen",
        337 => "Elementierte Außenwandkonstruktionen",
        338 => "Lichtschutz zur KG 330",
        341 => "Tragende Innenwände",
        342 => "Nichttragende Innenwände",
        343 => "Innenstützen",
        344 => "Innenwandöffnungen",
        345 => "Innenwandbekleidungen",
        346 => "Elementierte Innenwandkonstruktionen",
        347 => "Lichtschutz zur KG 340",
        351 => "Deckenkonstruktionen",
        352 => "Deckenöffnungen",
        353 => "Deckenbeläge",
        354 => "Deckenbekleidungen",
        355 => "Elementierte Deckenkonstruktionen",
        361 => "Dachkonstruktionen",
        362 => "Dachöffnungen",
        363 => "Dachbeläge",
        364 => "Dachbekleidungen",
        365 => "Elementierte Dachkonstruktionen",
        366 => "Lichtschutz zur KG 360",
        391 => "Baustelleneinrichtung",
        392 => "Gerüste",
        393 => "Sicherungsmaßnahmen",
        394 => "Abbruchmaßnahmen",
        395 => "Instandsetzungen",
        396 => "Materialentsorgung",
        397 => "Zusätzliche Maßnahmen",
        398 => "Provisorische Baukonstruktionen",
        k if k % 10 == 9 && (319..=399).contains(&k) => return Some(sonstiges(k)),
        _ => return None,
    })
}

fn sonstiges(k: u16) -> &'static str {
    match ebene2(k) {
        310 => "Sonstiges zur KG 310",
        320 => "Sonstiges zur KG 320",
        330 => "Sonstiges zur KG 330",
        340 => "Sonstiges zur KG 340",
        350 => "Sonstiges zur KG 350",
        360 => "Sonstiges zur KG 360",
        _ => "Sonstiges",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namen_der_kostengruppen() {
        assert_eq!(name(322), Some("Flachgründungen und Bodenplatten"));
        assert_eq!(name(ebene2(335)), Some("Außenwände"));
        assert_eq!(name(349), Some("Sonstiges zur KG 340"));
        assert_eq!(name(300), None);
        assert_eq!(name(391), Some("Baustelleneinrichtung"));
        assert_eq!(name(392), Some("Gerüste"));
        assert_eq!(name(399), Some("Sonstiges"));
    }
}

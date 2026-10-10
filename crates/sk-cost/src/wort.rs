//! Namen für Menschen aus einer Quelle (Bausteingrenze §5 „Sätze für
//! Menschen“, BIM-Datenmodell §3.19 „Wörter im Fenster“): Was im Fenster
//! steht, heißt nie nach einem Dateischlüssel, einer Guid oder einem
//! Operationsnamen. Die Tabellen hier sind die Wortspalte zur Feldtabelle in
//! [`crate::satz`]; ein Test hält beide vollständig gleich.

/// Abschnitte (§3.19 „Abschnitte“).
const ABSCHNITTE: [(&str, &str); 17] = [
    ("catalog", "Firmenkatalog"),
    ("article", "Artikel"),
    ("service", "Bauleistung"),
    ("svcpart", "Stoffanteil"),
    ("svcfollow", "Folgeposition"),
    ("rate", "Firmenwert"),
    ("lot", "Los"),
    ("origin", "Herkunft"),
    ("log", "Protokoll"),
    ("costproject", "Kalkulation"),
    ("project", "Projektangaben"),
    ("flow", "Ablauf"),
    ("flowstep", "Ablaufschritt"),
    ("proposal", "Vorschlag"),
    ("layer", "Schicht"),
    ("material", "Baustoff"),
    ("trade", "Gewerk"),
];

/// Felder (§3.19 „Felder“); gleiches Wort in jedem Abschnitt außer den
/// Ausnahmen in [`AUSNAHMEN`].
const FELDER: [(&str, &str); 77] = [
    ("guid", "Eintragsnummer"),
    ("key", "Eintragsnummer"),
    ("name", "Name"),
    ("stand", "Stand"),
    ("date", "Datum"),
    ("status", "Status"),
    ("pw", "Kennwort"),
    ("mat", "Baustoff"),
    ("t", "Dicke"),
    ("grade", "Güte"),
    ("format", "Format"),
    ("unit", "Einheit"),
    ("price", "Preis"),
    ("source", "Quelle"),
    ("supplier", "Lieferant"),
    ("conv", "Umrechnung"),
    ("std", "Standardartikel"),
    ("retired", "im Papierkorb"),
    ("short", "Kurztext"),
    ("trade", "Gewerk"),
    ("title", "Titel"),
    ("pos", "Ordnungszahl"),
    ("basis", "Mengenbezug"),
    ("hours", "Lohnstunden"),
    ("equip", "Gerätekosten"),
    ("other", "Sonstige Kosten"),
    ("nu", "Nachunternehmerpreis"),
    ("kind", "Art"),
    ("kg", "Kostengruppe"),
    ("cats", "Bauteilarten"),
    ("tmin", "Dicke von"),
    ("tmax", "Dicke bis"),
    ("fn", "Schichtfunktion"),
    ("auto", "Automatikmenge"),
    ("service", "Bauleistung"),
    ("nr", "Nummer"),
    ("art", "Artikel"),
    ("layer", "Stoff aus der Schicht"),
    ("qty", "Menge je Einheit"),
    ("follow", "Folgeposition"),
    ("factor", "Faktor"),
    ("num", "Wert"),
    ("parent", "Los"),
    ("pre", "Vorbemerkungen"),
    ("rec", "Art des Eintrags"),
    ("url", "Adresse"),
    ("region", "Preisregion"),
    ("proj", "im Projekt geändert"),
    ("conf", "Sicherheit"),
    ("time", "Zeit"),
    ("role", "Rolle"),
    ("op", "Vorgang"),
    ("of", "Eintrag"),
    ("old", "vorher"),
    ("new", "nachher"),
    ("site", "Bauvorhaben"),
    ("client", "Bauherr"),
    ("author", "Aufsteller"),
    ("catalog", "Firmenkatalog"),
    ("lvstorey", "Geschosse als Untertitel"),
    ("keep", "so gelassen bis Stand"),
    ("ask", "Einleitung"),
    ("flow", "Ablauf"),
    ("step", "Schrittart"),
    ("text", "Text"),
    ("type", "Feldart"),
    ("min", "kleinster Wert"),
    ("max", "größter Wert"),
    ("optional", "freiwillig"),
    ("args", "Angaben"),
    ("rule", "Prüfung"),
    ("preset", "Vorbelegung"),
    ("hint", "Hinweistext"),
    ("per", "Preiseinheiten"),
    ("project", "Projekt"),
    ("field", "Feld"),
    ("svc", "gewählte Bauleistung"),
];

/// `Abschnitt.feld` mit eigenem Wort.
const AUSNAHMEN: [(&str, &str, &str); 8] = [
    ("article", "date", "Preisstand"),
    ("catalog", "status", "Freigabe"),
    ("service", "kind", "Positionsart"),
    ("origin", "kind", "Herkunft"),
    ("flow", "kind", "Zugang"),
    ("svcpart", "nr", "Reihenfolge"),
    ("svcfollow", "nr", "Reihenfolge"),
    ("flowstep", "nr", "Reihenfolge"),
];

/// Statt eines Namens, der sich nicht auflösen lässt.
pub const EIN_EINTRAG: &str = "ein Eintrag";

/// Wort eines Abschnitts: „Artikel“; ein Los mit `parent` heißt „Titel“.
/// Unbekannt: „Eintrag“.
pub fn abschnitt(schluessel: &str) -> &'static str {
    ABSCHNITTE
        .iter()
        .find(|(k, _)| *k == schluessel)
        .map_or("Eintrag", |(_, w)| w)
}

/// Wort eines Felds im Abschnitt `abschnitt` (`None`: ohne Ausnahme):
/// „Preis“, `article.date` „Preisstand“. Unbekannt: „Angabe“.
pub fn feld(abschnitt: Option<&str>, schluessel: &str) -> &'static str {
    if let Some(a) = abschnitt {
        if let Some((_, _, w)) = AUSNAHMEN
            .iter()
            .find(|(x, f, _)| *x == a && *f == schluessel)
        {
            return w;
        }
    }
    FELDER
        .iter()
        .find(|(k, _)| *k == schluessel)
        .map_or("Angabe", |(_, w)| w)
}

/// Alle Schlüssel der Feldtabelle und der Abschnitte (für die Sperre im
/// Test `nutzersaetze_sauber`).
pub fn schluessel() -> impl Iterator<Item = &'static str> {
    FELDER
        .iter()
        .map(|(k, _)| *k)
        .chain(ABSCHNITTE.iter().map(|(k, _)| *k))
        .chain(
            crate::satz::ABSCHNITTE
                .iter()
                .flat_map(|a| std::iter::once(a.name).chain(a.felder.iter().map(|f| f.name))),
        )
}

/// Name eines Firmenwerts (`[rate] key`): „Verrechnungslohn“,
/// „Bewehrungsgrad Sohlplatte“; unbekannt „ein Eintrag“.
pub fn firmenwert(schluessel: &str) -> String {
    match schluessel {
        "wage" => "Verrechnungslohn".into(),
        "surcharge" => "Zuschlag".into(),
        "vat" => "Mehrwertsteuer".into(),
        s if s.starts_with("steel.") => bewehrungsgrad(s),
        _ => EIN_EINTRAG.into(),
    }
}

/// Name eines Bewehrungsgrads aus seinem Schlüssel `steel.{art}` nach
/// `sk_model::kinds`: „Bewehrungsgrad Sohlplatte“. Ohne bekannte
/// Bauteilart nur „Bewehrungsgrad“.
pub fn bewehrungsgrad(schluessel: &str) -> String {
    let art = schluessel.strip_prefix("steel.").unwrap_or(schluessel);
    sk_model::element::Category::ALL
        .into_iter()
        .map(sk_model::kinds::spec)
        .find(|s| s.szo == art)
        .map_or_else(
            || "Bewehrungsgrad".to_string(),
            |s| format!("Bewehrungsgrad {}", s.name),
        )
}

/// Mengenbezug als Wort: `area` „Fläche“.
pub fn bezug(b: crate::katalog::Bezug) -> &'static str {
    use crate::katalog::Bezug;
    match b {
        Bezug::Flaeche => "Fläche",
        Bezug::Volumen => "Volumen",
        Bezug::Laenge => "Länge",
        Bezug::Umfang => "Umfang",
        Bezug::Schalung => "Schalfläche",
        Bezug::Stahl => "Stahlgewicht",
        Bezug::Auto => "Automatikmenge",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bewehrungsgrad_nach_bauteilart() {
        assert_eq!(
            bewehrungsgrad("steel.groundslab"),
            "Bewehrungsgrad Sohlplatte"
        );
        assert_eq!(
            bewehrungsgrad("steel.floor"),
            "Bewehrungsgrad Geschossdecke"
        );
        assert_eq!(bewehrungsgrad("steel.xyz"), "Bewehrungsgrad");
        assert_eq!(firmenwert("wage"), "Verrechnungslohn");
    }

    /// Jedes Feld und jeder Abschnitt der Feldtabelle hat ein Wort.
    #[test]
    fn wortspalte_vollstaendig() {
        for a in crate::satz::ABSCHNITTE {
            assert_ne!(abschnitt(a.name), "Eintrag", "{}", a.name);
            for f in a.felder {
                assert_ne!(
                    feld(Some(a.name), f.name),
                    "Angabe",
                    "{}.{}",
                    a.name,
                    f.name
                );
            }
        }
        assert_eq!(feld(Some("article"), "date"), "Preisstand");
        assert_eq!(feld(Some("lot"), "date"), "Datum");
    }
}

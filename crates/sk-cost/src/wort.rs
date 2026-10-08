//! Namen für Menschen aus einer Quelle (Bausteingrenze §5 „Sätze für
//! Menschen“): Was im Fenster steht, heißt nie nach einem Dateischlüssel.

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

/// Statt eines Namens, der sich nicht auflösen lässt.
pub const EIN_EINTRAG: &str = "ein Eintrag";

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
    }
}

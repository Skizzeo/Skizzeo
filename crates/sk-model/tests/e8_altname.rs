//! E8-10 (BIM-Integration): Werks-Baustoffschlüssel einer Erweiterung in
//! einer Altdatei. `porenbeton` muss den Baustoff treffen, den die Datei
//! unter dem Altnamen „Gasbeton“ führt (szo::ALTNAMEN, R73-W), nicht den
//! ersten Mauerwerksbaustoff (dort: Verblender).
use sk_model::erweiterung::ExtDef;
use sk_model::{szo, GuidGen};

#[test]
fn porenbeton_trifft_gasbeton_der_altdatei() {
    for (name, t) in [
        (
            "abnahme_p5",
            include_str!("../../../app/src/abnahme_p5.szo"),
        ),
        (
            "abnahme_p6",
            include_str!("../../../app/src/abnahme_p6.szo"),
        ),
    ] {
        let m = szo::read(t, GuidGen::with_seed(1)).unwrap().model;
        let d = ExtDef::lesen(include_str!("../../sk-szb/beispiele/werk.stuetze.szb")).unwrap();
        let id = m.ext_material(&d, "porenbeton").expect("Baustoff");
        assert_eq!(m.material(id).unwrap().name, "Gasbeton", "{name}");
        let id = m.ext_material(&d, "verblender").expect("Baustoff");
        assert_eq!(
            m.material(id).unwrap().name,
            "Verblender (Vormauerziegel)",
            "{name}"
        );
    }
}
